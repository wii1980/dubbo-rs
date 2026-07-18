# dubbo-rs-server

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-server)](https://crates.io/crates/v/dubbo-rs-server)
[![docs.rs](https://docs.rs/dubbo-rs-server/badge.svg)](https://docs.rs/dubbo-rs-server)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

High-level server API for hosting Dubbo RPC services.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-server = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-server
```

## Overview

Provides a builder-pattern `Server` that wraps tonic's gRPC server, integrating
Dubbo concepts such as application metadata, protocol configuration, filter
chains, and registry-based service registration.

The server exposes multiple lifecycle entry points covering plain blocking
serve, caller-driven graceful shutdown, internal shutdown channels, and
pre-bound `TcpListener` flows for socket-activation / supervisor deployments.

## Key Types

| Type | Description |
|------|-------------|
| `Server` | Builder-pattern server with service registration and the `serve*` family |

## API

```rust
impl Server {
    pub fn new() -> Self;
    pub fn with_application(self, name: impl Into<String>) -> Self;
    pub fn with_version(self, version: impl Into<String>) -> Self;
    pub fn with_protocol_config(self, config: ProtocolConfig) -> Self;
    pub fn with_filter(self, filter: Box<dyn Filter>) -> Self;
    pub fn with_filters(self, filters: Vec<Box<dyn Filter>>) -> Self;
    pub fn with_registry(self, registry: Box<dyn Registry>) -> Self;
    pub fn with_service_url(self, url: URL) -> Self;
    pub fn register_service<F: FnOnce(TonicServer) -> Router>(self, f: F) -> Self;

    pub fn filters(&self) -> &[Box<dyn Filter>];
    pub fn protocol_config(&self) -> Option<&ProtocolConfig>;
    pub fn application(&self) -> &str;
    pub fn version(&self) -> &str;

    pub async fn serve(self) -> Result<()>;
    pub async fn serve_with_graceful_shutdown<Fut: Future<Output = ()>>(
        self, signal: Fut,
    ) -> Result<()>;
    pub async fn serve_with_internal_shutdown(self) -> Result<()>;
    pub fn graceful_shutdown(&self);
    pub async fn serve_with_listener(
        self, listener: tokio::net::TcpListener,
    ) -> Result<()>;
    pub async fn serve_with_listener_and_shutdown<Fut: Future<Output = ()>>(
        self, listener: tokio::net::TcpListener, signal: Fut,
    ) -> Result<()>;
}

impl Default for Server {
    fn default() -> Self;
}
```

## Usage

```rust
use dubbo_rs_config::ProtocolConfig;
use dubbo_rs_server::Server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server = Server::new()
        .with_application("my-dubbo-app")
        .with_version("1.0.0")
        .with_protocol_config(ProtocolConfig::new("tri", "0.0.0.0", 50051))
        .register_service(|builder| {
            // Register tonic service implementations here.
            // The closure must return a Router — obtain one via
            // `builder.add_service(svc)`.
            builder.add_service(my_tonic_service::server::MyServiceServer::new(impl_))
        });

    // Bind and start serving
    server.serve().await?;

    Ok(())
}
```

## Shutdown modes

The server exposes three lifecycle entry points:

| Method | Shutdown trigger |
|--------|------------------|
| [`serve`](https://docs.rs/dubbo-rs-server/latest/dubbo_rs_server/struct.Server.html#method.serve) | none — blocks until fatal error |
| [`serve_with_graceful_shutdown`](https://docs.rs/dubbo-rs-server/latest/dubbo_rs_server/struct.Server.html#method.serve_with_graceful_shutdown) | caller-supplied `Future` (e.g. `tokio::signal::ctrl_c()`) |
| [`serve_with_internal_shutdown`](https://docs.rs/dubbo-rs-server/latest/dubbo_rs_server/struct.Server.html#method.serve_with_internal_shutdown) | internal `tokio::sync::watch` channel; trigger via [`graceful_shutdown`](https://docs.rs/dubbo-rs-server/latest/dubbo_rs_server/struct.Server.html#method.graceful_shutdown) |

### Caller-driven graceful shutdown

```rust
use dubbo_rs_config::ProtocolConfig;
use dubbo_rs_server::Server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server = Server::new()
        .with_application("my-dubbo-app")
        .with_protocol_config(ProtocolConfig::new("tri", "0.0.0.0", 50051))
        .register_service(|b| b.add_service(my_svc::server::MySvcServer::new(impl_)));

    server
        .serve_with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.expect("ctrl_c signal");
        })
        .await?;

    Ok(())
}
```

### Internal shutdown channel

`serve_with_internal_shutdown` wires an internal `tokio::sync::watch` channel
into tonic's `serve_with_shutdown`. The handle is driven via
`graceful_shutdown(&self)`, which sends `true` on the channel.

```rust,ignore
// Pattern is only meaningful when the `Server` is shared (e.g. via an outer
// Arc / registry) so that `graceful_shutdown()` can be called from a different
// task than the one driving `serve_with_internal_shutdown(self)`. The `Server`
// type itself does not implement `Clone`.
let server = Server::new()
    .with_application("my-dubbo-app")
    .with_protocol_config(ProtocolConfig::new("tri", "0.0.0.0", 50051))
    .register_service(|b| b.add_service(my_svc::server::MySvcServer::new(impl_)));

server.serve_with_internal_shutdown().await?;
```

For most supervisor-driven shutdown flows prefer `serve_with_graceful_shutdown`
or `serve_with_listener_and_shutdown` — they accept an external `Future` and
avoid the ownership dance.

## Socket-activated serving

For systemd-style socket activation, fd inheritance from a supervisor, or any
flow where the listener is bound externally, use `serve_with_listener` /
`serve_with_listener_and_shutdown`. The `protocol_config` is **not** used for
binding — only its absence is still checked for API consistency.

```rust
use dubbo_rs_config::ProtocolConfig;
use dubbo_rs_server::Server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:50051").await?;

    let server = Server::new()
        .with_application("my-dubbo-app")
        .with_protocol_config(ProtocolConfig::new("tri", "0.0.0.0", 50051))
        .register_service(|b| b.add_service(my_svc::server::MySvcServer::new(impl_)));

    server
        .serve_with_listener_and_shutdown(listener, async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;

    Ok(())
}
```

## Re-exports

- `dubbo_rs_common` — URL, Node, error types
- `dubbo_rs_proxy` — proxy factory abstractions

## License

Apache-2.0
