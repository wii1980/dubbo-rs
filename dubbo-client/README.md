# dubbo-rs-client

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-client)](https://crates.io/crates/dubbo-rs-client)
[![docs.rs](https://docs.rs/dubbo-rs-client/badge.svg)](https://docs.rs/dubbo-rs-client)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

High-level client API for connecting to Dubbo RPC services.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-client = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-client
```

## Overview

Provides a builder-pattern `Client` that wraps tonic's gRPC channel management.
Configure protocol settings, set a service URL, and dial to establish a connection.

## Key Types

| Type | Description |
|------|-------------|
| `Client` | Builder-pattern client with `new()`, `with_url()`, `dial()` |

## API

```rust
impl Client {
    pub fn new() -> Self;
    pub fn with_protocol_config(self, config: ProtocolConfig) -> Self;
    pub fn with_url(self, url: impl Into<String>) -> Self;
    pub async fn dial(&mut self) -> Result<()>;
    pub fn channel(&self) -> Option<&Channel>;
    pub fn protocol_config(&self) -> Option<&ProtocolConfig>;
    pub fn url(&self) -> &str;
}
```

## Usage

```rust
use dubbo_rs_client::Client;
use dubbo_rs_config::ProtocolConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut client = Client::new()
        .with_protocol_config(ProtocolConfig::new("tri", "127.0.0.1", 50051))
        .with_url("tri://127.0.0.1:50051/com.example.GreetService");

    // Establish the gRPC connection
    client.dial().await?;

    // Access the underlying tonic Channel for making RPC calls
    if let Some(channel) = client.channel() {
        // Use channel with generated gRPC client stubs
    }

    Ok(())
}
```

## URL Format

Client expects URLs in the `tri://host:port/service_path` format:

```
tri://127.0.0.1:50051/com.example.GreetService
```

## Re-exports

- `dubbo_rs_common as common`
- `dubbo_rs_proxy as proxy`

## License

Apache-2.0
