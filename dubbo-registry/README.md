# dubbo-rs-registry

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-registry)](https://crates.io/crates/dubbo-rs-registry)
[![docs.rs](https://docs.rs/dubbo-rs-registry/badge.svg)](https://docs.rs/dubbo-rs-registry)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Service registration and discovery abstraction for the dubbo-rs framework.

Provides the core `Registry` and `NotifyListener` traits that all registry backends (ZooKeeper, Nacos, Etcd) implement, along with the `ServiceEvent` enum for provider change notifications.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-registry = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-registry
```

## Key Types

### `Registry` trait

Primary abstraction for service registration and discovery. Implementors must also implement `Node` from `dubbo-rs-common`.

| Method | Description |
|--------|-------------|
| `register(url)` | Register a service provider URL |
| `unregister(url)` | Remove a previously registered provider |
| `subscribe(url, listener)` | Watch for provider changes on a service |
| `unsubscribe(url, listener)` | Stop watching a service |

All methods are `async` and return `Result<(), RPCError>`.

### `NotifyListener` trait

| Method | Description |
|--------|-------------|
| `notify(event)` | Called when providers change (async) |
| `listen_url()` | Returns the service URL this listener monitors |

### `ServiceEvent` enum

```rust
pub enum ServiceEvent {
    Add(Vec<URL>),      // New providers registered
    Remove(Vec<URL>),   // Providers unregistered
    Update(Vec<URL>),   // Provider metadata updated
}
```

## Usage

```rust
use std::sync::Arc;
use async_trait::async_trait;
use dubbo_rs_common::url::URL;
use dubbo_rs_registry::{Registry, NotifyListener, ServiceEvent};

struct MyListener { service_url: URL }

#[async_trait]
impl NotifyListener for MyListener {
    async fn notify(&self, event: ServiceEvent) {
        match event {
            ServiceEvent::Add(urls) => println!("added: {:?}", urls),
            ServiceEvent::Remove(urls) => println!("removed: {:?}", urls),
            ServiceEvent::Update(urls) => println!("updated: {:?}", urls),
        }
    }
    fn listen_url(&self) -> URL { self.service_url.clone() }
}

async fn example(registry: &dyn Registry) {
    let service_url = URL::new("tri", "/com.example.GreetService");

    // Register a provider
    registry.register(service_url.clone()).await.unwrap();

    // Subscribe to provider changes
    let listener = Arc::new(MyListener { service_url: service_url.clone() });
    registry.subscribe(service_url.clone(), listener).await.unwrap();

    // Unregister when shutting down
    registry.unregister(service_url).await.unwrap();
}
```

## Re-exports

- `pub use dubbo_rs_common as common` — access `URL`, `RPCError`, `Node`

## License

Apache-2.0
