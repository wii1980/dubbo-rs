# dubbo-rs-registry-nacos

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-registry-nacos)](https://crates.io/crates/dubbo-rs-registry-nacos)
[![docs.rs](https://docs.rs/dubbo-rs-registry-nacos/badge.svg)](https://docs.rs/dubbo-rs-registry-nacos)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Nacos-based service registry implementing the `Registry` trait from `dubbo-rs-registry`.

Communicates with Nacos via its HTTP API (`reqwest`). Supports service registration with heartbeat, discovery via polling, namespace/group isolation, and username/password authentication.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-registry-nacos = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-registry-nacos
```

## Key Types

### `NacosRegistry`

Implements `Registry` + `Node`.

| Method | Description |
|--------|-------------|
| `new(url)` | Create with Nacos server URL (host:port) |
| `with_namespace(ns)` | Builder: set namespace (default: `"public"`) |
| `with_group(group)` | Builder: set group (default: `"DEFAULT_GROUP"`) |
| `with_auth(user, pass)` | Builder: enable accessKey/secretKey auth |
| `destroy()` | Abort heartbeat and polling tasks |

### Configuration Constants

| Constant | Default | Description |
|----------|---------|-------------|
| `DEFAULT_NACOS_GROUP` | `"DEFAULT_GROUP"` | Service group |
| `DEFAULT_NAMESPACE` | `"public"` | Nacos namespace |
| `HEARTBEAT_INTERVAL_SECS` | `5` | Heartbeat frequency |
| `POLL_INTERVAL_SECS` | `10` | Discovery polling frequency |

### Features

- **Automatic heartbeat** — spawns a background task sending heartbeats every 5s for ephemeral instances
- **Polling discovery** — subscribers poll Nacos every 10s and emit `ServiceEvent::Update` on provider changes
- **Thread-safe listeners** — uses `DashMap` for concurrent listener management
- **Auth support** — appends `accessKey`/`secretKey` query params when configured

## Usage

```rust
use std::sync::Arc;
use dubbo_rs_common::url::URL;
use dubbo_rs_registry::Registry;
use dubbo_rs_registry_nacos::NacosRegistry;

#[tokio::main]
async fn main() {
    // Create registry pointing to Nacos server
    let nacos_url = {
        let mut u = URL::new("nacos", "");
        u.ip = "127.0.0.1".to_string();
        u.port = "8848".to_string();
        u
    };

    let registry = NacosRegistry::new(nacos_url)
        .with_namespace("dev")
        .with_group("MY_GROUP");
    // Or with auth:
    // .with_auth("admin", "secret123");

    // Register a provider — sends POST to /nacos/v1/ns/instance
    let mut provider_url = URL::new("tri", "/com.example.GreetService");
    provider_url.ip = "10.0.0.1".to_string();
    provider_url.port = "20880".to_string();
    registry.register(provider_url).await.unwrap();

    // Subscribe — starts background polling task
    // let listener = Arc::new(MyListener { ... });
    // registry.subscribe(service_url, listener).await.unwrap();

    // Unregister — sends DELETE to /nacos/v1/ns/instance
    // registry.unregister(provider_url).await.unwrap();

    // Cleanup — aborts background tasks
    registry.destroy();
}
```

## Re-exports

- `pub use dubbo_rs_common as common`
- `pub use dubbo_rs_registry as registry`

## License

Apache-2.0
