use crate::config::ProxyConfig;
use crate::metrics::METRICS;
use crate::tracing_layer::inject_trace_headers;
use anyhow::Result;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, warn, Instrument};
use uuid::Uuid;

/// State shared across all request handlers
#[derive(Clone)]
struct AppState {
    upstream_addr: Arc<String>,
}

pub async fn run(config: ProxyConfig) -> Result<()> {
    let listener = TcpListener::bind(config.listen_addr).await?;
    info!("listening on {}", config.listen_addr);

    let state = AppState {
        upstream_addr: Arc::new(config.upstream_addr),
    };

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let state = state.clone();

        tokio::spawn(async move {
            handle_connection(stream, peer_addr, state).await;
        });
    }
}

async fn handle_connection(stream: TcpStream, peer_addr: SocketAddr, state: AppState) {
    let io = TokioIo::new(stream);

    let service = service_fn(move |req| {
        let state = state.clone();
        let span = tracing::info_span!("request", peer = %peer_addr);
        handle_request(req, state).instrument(span)
    });

    if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
        warn!("connection error from {peer_addr}: {e}");
    }
}

async fn handle_request(
    mut req: Request<Incoming>,
    state: AppState,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let request_id = Uuid::new_v4().to_string();
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let start = Instant::now();

    info!(
        request_id = %request_id,
        method = %method,
        path = %path,
        "incoming request"
    );

    // Health check shortcut — handled locally, never forwarded
    if path == "/health" || path == "/_proxy/health" {
        return Ok(crate::health::health_response());
    }

    // Inject distributed trace headers before forwarding
    inject_trace_headers(&mut req, &request_id);

    METRICS.requests_total.inc();

    let result = forward_request(req, &state.upstream_addr, &request_id).await;

    let elapsed_ms = start.elapsed().as_millis();

    match result {
        Ok(response) => {
            let status = response.status().as_u16();

            info!(
                request_id = %request_id,
                method = %method,
                path = %path,
                status = status,
                latency_ms = elapsed_ms,
                "request completed"
            );

            METRICS
                .request_duration_ms
                .observe(start.elapsed().as_secs_f64() * 1000.0);

            if status >= 500 {
                METRICS.upstream_errors.inc();
            }

            Ok(response)
        }
        Err(e) => {
            error!(
                request_id = %request_id,
                method = %method,
                path = %path,
                error = %e,
                latency_ms = elapsed_ms,
                "upstream error"
            );

            METRICS.upstream_errors.inc();

            Ok(error_response(StatusCode::BAD_GATEWAY, "upstream unavailable"))
        }
    }
}

async fn forward_request(
    req: Request<Incoming>,
    upstream: &str,
    request_id: &str,
) -> Result<Response<Full<Bytes>>> {
    // Establish TCP connection to upstream
    let stream = TcpStream::connect(upstream)
        .await
        .map_err(|e| anyhow::anyhow!("connect to upstream failed: {e}"))?;

    let io = TokioIo::new(stream);

    // HTTP/1.1 handshake with upstream
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

    // Drive the connection in the background
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::debug!("upstream conn error: {e}");
        }
    });

    // Rebuild request with forwarding headers
    let (mut parts, body) = req.into_parts();

    // Add X-Forwarded-For and X-Request-ID
    parts.headers.insert(
        "x-request-id",
        request_id.parse().unwrap(),
    );

    let collected = body.collect().await?.to_bytes();
    let new_req = Request::from_parts(parts, Full::new(collected));

    let upstream_resp = sender.send_request(new_req).await?;
    let (resp_parts, resp_body) = upstream_resp.into_parts();

    let resp_bytes = resp_body.collect().await?.to_bytes();
    let response = Response::from_parts(resp_parts, Full::new(resp_bytes));

    Ok(response)
}

fn error_response(status: StatusCode, msg: &str) -> Response<Full<Bytes>> {
    let body = serde_json::json!({ "error": msg, "status": status.as_u16() }).to_string();
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}
