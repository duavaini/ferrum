# Ferrum

A high-performance HTTP reverse proxy built in Rust.

## Features
- Async request forwarding via Tokio
- Distributed tracing with structured JSON logs
- Prometheus metrics endpoint
- Health check endpoint
- CLI-based configuration

## Usage
cargo run -- --listen 127.0.0.1:8080 --upstream 127.0.0.1:3000
