use anyhow::Result;
use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use prometheus::{Counter, Histogram, HistogramOpts, Registry};
use std::net::SocketAddr;
use std::sync::OnceLock;
use tokio::net::TcpListener;

/// Global metrics registry — initialized once at startup.
pub static METRICS: OnceLock<Metrics> = OnceLock::new();

pub struct Metrics {
    pub requests_total: Counter,
    pub upstream_errors: Counter,
    pub request_duration_ms: Histogram,
    registry: Registry,
}

impl Metrics {
    fn new() -> Self {
        let registry = Registry::new();

        let requests_total = Counter::new("proxy_requests_total", "Total requests proxied").unwrap();
        let upstream_errors =
            Counter::new("proxy_upstream_errors_total", "Total upstream errors").unwrap();

        let buckets = vec![1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0];
        let request_duration_ms = Histogram::with_opts(
            HistogramOpts::new(
                "proxy_request_duration_ms",
                "Request duration in milliseconds",
            )
            .buckets(buckets),
        )
        .unwrap();

        registry.register(Box::new(requests_total.clone())).unwrap();
        registry
            .register(Box::new(upstream_errors.clone()))
            .unwrap();
        registry
            .register(Box::new(request_duration_ms.clone()))
            .unwrap();

        Metrics {
            requests_total,
            upstream_errors,
            request_duration_ms,
            registry,
        }
    }

    pub fn gather_text(&self) -> String {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let mut buf = Vec::new();
        encoder.encode(&self.registry.gather(), &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }
}

// Auto-initialize on first use
impl std::ops::Deref for OnceLock<Metrics> {
    // Not needed; we use a helper instead
}

// Re-export a ready-to-use global accessor
pub mod global {
    use super::*;
    static INIT: OnceLock<Metrics> = OnceLock::new();

    pub fn get() -> &'static Metrics {
        INIT.get_or_init(Metrics::new)
    }
}

// Convenience accessor used by other modules via `crate::metrics::METRICS`
// We store in a module-level static initialized via global::get()
pub struct MetricsHandle;

impl std::ops::Deref for MetricsHandle {
    type Target = Metrics;
    fn deref(&self) -> &'static Metrics {
        global::get()
    }
}

pub const METRICS: MetricsHandle = MetricsHandle;

/// Serve Prometheus /metrics endpoint on a dedicated port
pub async fn serve_metrics(addr: SocketAddr) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("metrics server listening on {addr}");

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        tokio::spawn(async move {
            let svc = service_fn(|req: Request<hyper::body::Incoming>| async move {
                let body = if req.uri().path() == "/metrics" {
                    global::get().gather_text()
                } else {
                    "404 not found\n".to_string()
                };
                Ok::<_, hyper::Error>(
                    Response::builder()
                        .header("content-type", "text/plain; version=0.0.4")
                        .body(Full::new(Bytes::from(body)))
                        .unwrap(),
                )
            });

            let _ = http1::Builder::new().serve_connection(io, svc).await;
        });
    }
}
