# dubbo-rs-registry-etcd

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-registry-etcd)](https://crates.io/crates/dubbo-rs-registry-etcd)
[![docs.rs](https://docs.rs/dubbo-rs-registry-etcd/badge.svg)](https://docs.rs/dubbo-rs-registry-etcd)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Etcd-based service registry for dubbo-rs using the etcd v3 HTTP API.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-registry-etcd = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-registry-etcd
```

## Overview

Implements the `Registry` trait via etcd v3's HTTP REST API using `reqwest`.
Supports service registration with lease TTL, deregistration, and subscription
with listener notification. Keys are base64-encoded for binary safety.

## Key Types

| Type | Description |
|------|-------------|
| `EtcdRegistry` | Implements `Registry` trait via etcd v3 HTTP API |

## API

```rust
impl EtcdRegistry {
    pub fn new(url: URL) -> Self;
    pub fn with_endpoints(self, endpoints: impl Into<String>) -> Self;
    pub fn with_root_path(self, path: impl Into<String>) -> Self;
}

// Registry trait methods
impl Registry for EtcdRegistry {
    async fn register(&self, url: URL) -> Result<(), RPCError>;
    async fn unregister(&self, url: URL) -> Result<(), RPCError>;
    async fn subscribe(&self, url: URL, listener: Arc<dyn NotifyListener>) -> Result<(), RPCError>;
    async fn unsubscribe(&self, url: URL, listener: Arc<dyn NotifyListener>) -> Result<(), RPCError>;
}
```

## Usage

```rust
use dubbo_rs_registry_etcd::EtcdRegistry;
use dubbo_rs_common::url::URL;
use dubbo_rs_registry::Registry;

let url = URL::new("etcd", "/com.example.DemoService");
let registry = EtcdRegistry::new(url)
    .with_endpoints("http://etcd1:2379,http://etcd2:2379")
    .with_root_path("/dubbo");

// Register a service provider (PUT with lease TTL)
let provider_url = URL::new("dubbo", "192.168.1.1:20880/com.example.DemoService");
registry.register(provider_url).await?;

// Unregister when shutting down
registry.unregister(provider_url).await?;

// Subscribe to service changes
let listener = Arc::new(MyNotifyListener);
registry.subscribe(service_url, listener).await?;
```

## Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `endpoints` | `http://127.0.0.1:2379` | etcd cluster endpoints |
| `root` | `/dubbo` | Root path for Dubbo service entries |
| Lease TTL | 30 seconds | Auto-renewed on registration |

## Storage Layout

```
/dubbo/{service_key}/providers/{url_encoded_service_url}
```

Keys and values are base64-encoded in etcd v3 API requests.

## Re-exports

- `dubbo_rs_common as common`
- `dubbo_rs_registry as registry`

## License

Apache-2.0
