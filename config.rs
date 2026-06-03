use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub listen_addr: SocketAddr,
    pub upstream_addr: String,
    pub metrics_addr: SocketAddr,
}
