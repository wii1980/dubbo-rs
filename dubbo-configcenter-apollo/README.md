# dubbo-rs-configcenter-apollo

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-configcenter-apollo)](https://crates.io/crates/dubbo-rs-configcenter-apollo)
[![docs.rs](https://docs.rs/dubbo-rs-configcenter-apollo/badge.svg)](https://docs.rs/dubbo-rs-configcenter-apollo)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Apollo-backed configuration center for Apache Dubbo Rust.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-configcenter-apollo = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-configcenter-apollo
```

## Overview

Implements the `ConfigCenter` trait from `dubbo-rs-configcenter`, communicating with Apollo Config Service via its REST API. Supports `app_id/cluster/namespace` isolation and background long-polling for config change detection.

## Key Types

| Type | Description |
|------|-------------|
| `ApolloConfigCenter` | Apollo-backed `ConfigCenter` implementation |
| `ApolloConfigCenterBuilder` | Builder with `meta_server_url`, `app_id`, `cluster`, `namespace`, `token` |

### `ApolloConfigCenter` Public Fields

| Field | Description |
|-------|-------------|
| `meta_server_url` | Apollo meta server address in `http://{host}:{port}` format |
| `app_id` | Apollo App ID |
| `cluster` | Apollo cluster name (defaults to `"default"`) |
| `namespace` | Apollo namespace (defaults to `"application"`) |
| `token` | Optional token for Apollo authentication |

## Usage

```rust
use dubbo_rs_configcenter_apollo::ApolloConfigCenterBuilder;

let cc = ApolloConfigCenterBuilder::new()
    .meta_server_url("http://127.0.0.1:8080")
    .app_id("my-app")
    .cluster("default")
    .namespace("application")
    .token("secret-token")
    .build()?;

// Get a config value
let value = cc.get_config("app.timeout").await?;

// Set a config value
cc.set_config("app.timeout", "30s").await?;

// Watch for changes
cc.watch("app.timeout".into(), "dubbo".into(), listener).await?;

// Remove a config key
cc.remove_config("app.timeout").await?;
```

## License

Apache-2.0
