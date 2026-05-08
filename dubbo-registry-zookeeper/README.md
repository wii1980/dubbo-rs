# dubbo-rs-registry-zookeeper

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-registry-zookeeper)](https://crates.io/crates/dubbo-rs-registry-zookeeper)
[![docs.rs](https://docs.rs/dubbo-rs-registry-zookeeper/badge.svg)](https://docs.rs/dubbo-rs-registry-zookeeper)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

ZooKeeper-based service registry implementing the `Registry` trait from `dubbo-rs-registry`.

Uses ephemeral znodes under `/dubbo/{service}/providers/` for automatic cleanup when a provider disconnects. Supports both interface-level (Dubbo2) and application-level (Dubbo3) discovery patterns.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-registry-zookeeper = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-registry-zookeeper
```

## Key Types

### `ZookeeperRegistry`

Implements `Registry` + `Node`. Connects lazily to ZooKeeper on first operation.

| Method / Field | Description |
|----------------|-------------|
| `new(url)` | Create with a ZooKeeper URL (host:port) |
| `with_root_path(path)` | Builder: override default `/dubbo` root |
| `destroy()` | Cleans up all registered ephemeral nodes |

### Features

- **Lazy connection** — ZK session established on first `register`/`subscribe` call via `ensure_connection()`
- **Ephemeral znodes** — provider nodes auto-removed on disconnect
- **URL encoding** — provider URLs are percent-encoded for safe ZK path usage
- **Listener notification** — subscribers receive `ServiceEvent::Add` with current children on subscribe

## Usage

```rust
use std::sync::Arc;
use dubbo_rs_common::url::URL;
use dubbo_rs_registry::Registry;
use dubbo_rs_registry_zookeeper::ZookeeperRegistry;

#[tokio::main]
async fn main() {
    // Create registry pointing to ZK cluster
    let zk_url = {
        let mut u = URL::new("zookeeper", "");
        u.ip = "127.0.0.1".to_string();
        u.port = "2181".to_string();
        u
    };
    let registry = ZookeeperRegistry::new(zk_url);
    // Optional: customize root path
    // let registry = ZookeeperRegistry::new(zk_url).with_root_path("/myapp");

    // Register a provider — creates ephemeral node at
    // /dubbo/com.example.GreetService/providers/{encoded_url}
    let mut provider_url = URL::new("tri", "/com.example.GreetService");
    provider_url.ip = "192.168.1.100".to_string();
    provider_url.port = "50051".to_string();
    registry.register(provider_url).await.unwrap();

    // Subscribe to provider changes
    // let listener = Arc::new(MyListener { ... });
    // registry.subscribe(service_url, listener).await.unwrap();

    // Cleanup on shutdown — removes ephemeral nodes
    registry.destroy();
}
```

## Re-exports

- `pub use dubbo_rs_common as common`
- `pub use dubbo_rs_registry as registry`

## License

Apache-2.0
