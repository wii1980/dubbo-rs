# dubbo-rs

[![crates.io](https://img.shields.io/crates/v/dubbo-rs)](https://crates.io/crates/dubbo-rs)
[![docs.rs](https://docs.rs/dubbo-rs/badge.svg)](https://docs.rs/dubbo-rs)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Top-level entry point that unifies Server and Client under a single `Instance`.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs
```

## Overview

The `Instance` struct is the main façade for the dubbo-rs framework. It holds
a `RootConfig`, an optional `Server`, and an optional `Client`. Calling `start()`
spawns the server via `tokio::spawn` and leaves the client available for RPC calls.

## Key Types

| Type       | Description                                            |
|------------|--------------------------------------------------------|
| `Instance` | Unified entry point holding config, server, and client |

## API

```rust
impl Instance {
    pub fn new(config: RootConfig) -> Self;
    pub fn set_provider_service(&mut self, server: Server) -> &mut Self;
    pub fn set_client(&mut self, client: Client) -> &mut Self;
    pub fn start(&mut self) -> Result<()>;
    pub fn config(&self) -> &RootConfig;
    pub fn server(&self) -> Option<&Server>;
    pub fn client(&self) -> Option<&Client>;
}
```

## Usage

```rust
use dubbo_rs::{Instance, client::Client, server::Server, config::{RootConfig, ProtocolConfig}};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = RootConfig::default()
        .with_application("demo-app")
        .with_version("1.0.0")
        .with_protocol(ProtocolConfig::new("tri", "0.0.0.0", 50051));

    let server = Server::new()
        .with_application("demo-app")
        .with_protocol_config(ProtocolConfig::new("tri", "0.0.0.0", 50051))
        .register_service(|builder| {
            builder.add_service(my_service::new())
        });

    let client = Client::new()
        .with_url("tri://127.0.0.1:50051/com.example.GreetService");

    let mut instance = Instance::new(config);
    instance.set_provider_service(server);
    instance.set_client(client);

    // Spawns the server in a background tokio task
    instance.start()?;

    // Use instance.client() to make RPC calls...

    Ok(())
}
```

## Re-exports

- `dubbo_rs_client as client`
- `dubbo_rs_common as common`
- `dubbo_rs_config as config`
- `dubbo_rs_server as server`

## License

Apache-2.0
