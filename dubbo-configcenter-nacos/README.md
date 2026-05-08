# dubbo-rs-configcenter-nacos

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-configcenter-nacos)](https://crates.io/crates/dubbo-rs-configcenter-nacos)
[![docs.rs](https://docs.rs/dubbo-rs-configcenter-nacos/badge.svg)](https://docs.rs/dubbo-rs-configcenter-nacos)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Nacos-backed configuration center for Apache Dubbo Rust.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-configcenter-nacos = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-configcenter-nacos
```

## Overview

Implements the `ConfigCenter` trait from `dubbo-rs-configcenter`, communicating with Nacos server via its Open API (`/nacos/v1/cs/configs`). Supports namespace/group isolation, username/password authentication, and background polling for config change detection.

## Key Types

| Type | Description |
|------|-------------|
| `NacosConfigCenter` | Nacos-backed `ConfigCenter` implementation with builder pattern |
| `check_nacos_response` | Helper function to validate Nacos API response bodies |

### `NacosConfigCenter` Public Fields

| Field | Description |
|-------|-------------|
| `server_addr` | Nacos server address in `http://{host}:{port}` format |
| `namespace` | Nacos namespace (tenant) for config isolation |
| `group` | Nacos group for config grouping |
| `username` | Optional accessKey for Nacos auth |
| `password` | Optional secretKey for Nacos auth |

## Usage

```rust
use dubbo_rs_common::url::URL;
use dubbo_rs_configcenter_nacos::NacosConfigCenter;

let mut url = URL::new("nacos", "");
url.ip = "127.0.0.1".into();
url.port = "8848".into();

let cc = NacosConfigCenter::new(url)
    .with_namespace("dev-ns")
    .with_group("MY_GROUP")
    .with_auth("admin", "secret123");

// Get a config value
let value = cc.get_config("app.timeout", "dubbo").await?;

// Set a config value
cc.set_config("app.timeout", "dubbo", "30s").await?;

// Watch for changes
cc.watch("app.timeout".into(), "dubbo".into(), listener).await?;

// Remove a config key
cc.remove_config("app.timeout", "dubbo").await?;
```

## License

Apache-2.0
