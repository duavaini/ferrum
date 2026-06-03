mod config;
mod proxy;
mod metrics;
mod tracing_layer;
mod health;

use anyhow::Result;
use clap::Parser;
use std::net::SocketAddr;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "ferrum-proxy")]
#[command(about = "A high-performance HTTP reverse proxy with distributed tracing and metrics")]
struct Args {
    /// Address to listen on
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    listen: SocketAddr,

    /// Upstream backend address
    #[arg(short, long, default_value = "127.0.0.1:3000")]
    upstream: String,

    /// Metrics server address
    #[arg(short, long, default_value = "127.0.0.1:9090")]
    metrics_addr: SocketAddr,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize structured tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| args.log_level.parse().unwrap()),
        )
        .with_target(true)
        .with_thread_ids(true)
        .json()
        .init();

    info!(
        listen = %args.listen,
        upstream = %args.upstream,
        metrics = %args.metrics_addr,
        "ferrum-proxy starting"
    );

    let config = config::ProxyConfig {
        listen_addr: args.listen,
        upstream_addr: args.upstream,
        metrics_addr: args.metrics_addr,
    };

    // Spawn metrics server
    let metrics_config = config.clone();
    tokio::spawn(async move {
        if let Err(e) = metrics::serve_metrics(metrics_config.metrics_addr).await {
            tracing::error!("metrics server error: {e}");
        }
    });

    // Run the proxy
    proxy::run(config).await
}
