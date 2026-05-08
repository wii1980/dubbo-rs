# dubbo-rs-server

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-server)](https://crates.io/crates/dubbo-rs-server)
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

Provides a builder-pattern `Server` that wraps tonic's gRPC server. Configure
application metadata, protocol settings, register tonic services, and serve.

## Key Types

| Type | Description |
|------|-------------|
| `Server` | Builder-pattern server with service registration and `serve()` |

## API

```rust
impl Server {
    pub fn new() -> Self;
    pub fn with_application(self, name: impl Into<String>) -> Self;
    pub fn with_version(self, version: impl Into<String>) -> Self;
    pub fn with_protocol_config(self, config: ProtocolConfig) -> Self;
    pub fn register_service<F>(self, f: F) -> Self;
    pub async fn serve(self) -> Result<()>;
    pub fn protocol_config(&self) -> Option<&ProtocolConfig>;
    pub fn application(&self) -> &str;
    pub fn version(&self) -> &str;
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
            // Register tonic service with the router
            builder.add_service(my_tonic_serviceImplementation::new())
        });

    // Bind and start serving
    server.serve().await?;

    Ok(())
}
```

## Re-exports

- `dubbo_rs_common as common`
- `dubbo_rs_proxy as proxy`

## License

Apache-2.0
