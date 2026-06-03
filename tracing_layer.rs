/// Inject W3C Trace Context headers (traceparent / tracestate) and
/// a proprietary X-Request-ID into outgoing upstream requests.
///
/// This implements a subset of the W3C Trace Context spec:
/// https://www.w3.org/TR/trace-context/
///
/// Format: traceparent: 00-<trace-id>-<span-id>-<flags>
use hyper::body::Incoming;
use hyper::Request;

/// Inject or propagate distributed tracing headers.
/// If an inbound `traceparent` already exists we preserve it (pass-through mode).
/// Otherwise we generate a new trace context rooted at this proxy.
pub fn inject_trace_headers(req: &mut Request<Incoming>, request_id: &str) {
    let headers = req.headers_mut();

    // Always set X-Request-ID so every hop in the chain is identifiable
    if !headers.contains_key("x-request-id") {
        headers.insert(
            "x-request-id",
            request_id.parse().expect("valid header value"),
        );
    }

    // Pass-through existing W3C traceparent — don't overwrite a parent trace
    if headers.contains_key("traceparent") {
        return;
    }

    // Generate a new traceparent:  version=00, trace-id=128-bit, span-id=64-bit, flags=01 (sampled)
    let trace_id = new_trace_id();
    let span_id = new_span_id();
    let traceparent = format!("00-{trace_id}-{span_id}-01");

    headers.insert("traceparent", traceparent.parse().unwrap());
    headers.insert(
        "tracestate",
        format!("ferrum={span_id}").parse().unwrap(),
    );
}

/// Generate a random 128-bit hex trace ID (W3C spec: 32 hex chars)
fn new_trace_id() -> String {
    let id = uuid::Uuid::new_v4().as_u128();
    format!("{id:032x}")
}

/// Generate a random 64-bit hex span ID (W3C spec: 16 hex chars)
fn new_span_id() -> String {
    let id = uuid::Uuid::new_v4().as_u64_pair().0;
    format!("{id:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::body::Incoming;

    fn make_req() -> Request<Incoming> {
        Request::builder()
            .uri("http://upstream/test")
            .body(http_body_util::Empty::new().map_err(|e| match e {}).boxed())
            .unwrap()
    }

    #[test]
    fn test_trace_id_length() {
        assert_eq!(new_trace_id().len(), 32);
    }

    #[test]
    fn test_span_id_length() {
        assert_eq!(new_span_id().len(), 16);
    }

    #[test]
    fn test_traceparent_format() {
        let trace_id = new_trace_id();
        let span_id = new_span_id();
        let tp = format!("00-{trace_id}-{span_id}-01");
        let parts: Vec<&str> = tp.split('-').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "00");
        assert_eq!(parts[1].len(), 32);
        assert_eq!(parts[2].len(), 16);
        assert_eq!(parts[3], "01");
    }
}
