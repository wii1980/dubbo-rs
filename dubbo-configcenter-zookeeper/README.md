# dubbo-rs-configcenter-zookeeper

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-configcenter-zookeeper)](https://crates.io/crates/dubbo-rs-configcenter-zookeeper)
[![docs.rs](https://docs.rs/dubbo-rs-configcenter-zookeeper/badge.svg)](https://docs.rs/dubbo-rs-configcenter-zookeeper)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

ZooKeeper-backed configuration center for dubbo-rs.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-configcenter-zookeeper = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-configcenter-zookeeper
```

Implements the `ConfigCenter` trait from `dubbo-rs-configcenter`, storing configuration values as persistent znodes under `/dubbo/config/{group}/{key}` and converting ZK watcher events into async listener notifications.

## Key Types

| Type | Description |
|------|-------------|
| `ZookeeperConfigCenter` | ZK-backed `ConfigCenter` implementation |
| `ZookeeperConfigCenterBuilder` | Builder with `with_url`, `with_root_path`, `with_session_timeout` |

## How It Works

- **Path mapping**: `{root_path}/config/{group}/{key}` — default root is `/dubbo`.
- **Lazy connection**: ZK connection is established on the first `register`, `unregister`, or `watch` call.
- **Watchers**: Each watched key installs a `ConfigWatcher` that maps `NodeCreated`, `NodeDataChanged`, and `NodeDeleted` ZK events to `ConfigChangeType` variants and fans out notifications via `tokio::spawn`.
- **Parent paths**: Missing ancestor znodes are auto-created with persistent nodes and open ACLs.

## Usage

```rust
use dubbo_rs_common::url::URL;
use dubbo_rs_configcenter_zookeeper::ZookeeperConfigCenter;

let mut url = URL::new("zookeeper", "/dubbo/config");
url.ip = "127.0.0.1".into();
url.port = "2181".into();

let cc = ZookeeperConfigCenter::builder()
    .with_url(url)
    .with_root_path("/dubbo")
    .with_session_timeout(std::time::Duration::from_secs(60))
    .build();

// Register a config key (creates the znode)
cc.register("app.timeout".into(), "default".into()).await?;

// Watch for changes
cc.watch("app.timeout".into(), "default".into(), listener).await?;

// Unregister (deletes the znode)
cc.unregister("app.timeout".into(), "default".into()).await?;
```

## License

Apache-2.0
