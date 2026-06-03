use bytes::Bytes;
use http_body_util::Full;
use hyper::{Response, StatusCode};
use serde_json::json;

/// Returns a lightweight health check response handled locally by the proxy,
/// never forwarded to the upstream.
pub fn health_response() -> Response<Full<Bytes>> {
    let body = json!({
        "status": "ok",
        "service": "ferrum-proxy",
        "version": env!("CARGO_PKG_VERSION"),
    })
    .to_string();

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}
