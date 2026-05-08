# dubbo-rs-configcenter

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-configcenter)](https://crates.io/crates/dubbo-rs-configcenter)
[![docs.rs](https://docs.rs/dubbo-rs-configcenter/badge.svg)](https://docs.rs/dubbo-rs-configcenter)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Dynamic configuration center abstraction for dubbo-rs.

Defines the core traits and types for runtime configuration management, along with an in-memory `DynamicConfiguration` implementation suitable for testing and single-node deployments.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-configcenter = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-configcenter
```

## Key Types

| Type | Description |
|------|-------------|
| `ConfigCenter` | Async trait for config backends — `register`, `unregister`, `watch` |
| `ConfigChangeEvent` | Carries `key`, `old_value`, `new_value`, `change_type` |
| `ConfigChangeType` | Enum: `Created`, `Modified`, `Deleted` |
| `ConfigListener` | Async trait — receives `on_change(event)` callbacks |
| `DynamicConfiguration` | In-memory store with get/set/remove and listener notification |
| `DynamicConfigurationBuilder` | Builder pattern for `DynamicConfiguration` |
| `ConfigCenterUrlExt` | URL extension trait — `get_config_center_group`, `get_config_center_namespace`, `get_config_center_timeout`, `get_config_center_data_id` |

## Usage

```rust
use std::sync::Arc;
use dubbo_rs_configcenter::{
    ConfigCenter, ConfigListener, ConfigChangeEvent, ConfigChangeType,
    DynamicConfiguration,
};
use async_trait::async_trait;

// Implement a listener
struct LogListener;

#[async_trait]
impl ConfigListener for LogListener {
    async fn on_change(&self, event: ConfigChangeEvent) {
        println!("config changed: {} {:?}", event.key, event.change_type);
    }
}

#[tokio::main]
async fn main() {
    let config = DynamicConfiguration::builder().build();

    // Watch a key
    config
        .watch("app.timeout".into(), "default".into(), Arc::new(LogListener))
        .await
        .unwrap();

    // Set / modify / remove — listeners are notified automatically
    config.set("app.timeout", "30s").await;
    config.set("app.timeout", "60s").await;
    config.remove("app.timeout").await;

    // Read by group prefix
    let group = config.get_configs_by_group("default");
}
```

## License

Apache-2.0
