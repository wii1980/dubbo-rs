# dubbo-rs-config

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-config)](https://crates.io/crates/dubbo-rs-config)
[![docs.rs](https://docs.rs/dubbo-rs-config/badge.svg)](https://docs.rs/dubbo-rs-config)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Configuration management for dubbo-rs with YAML support and builder pattern.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-config = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-config
```

## Overview

Provides `RootConfig`, `ProtocolConfig`, `RegistryConfig`, and `TlsConfig` structs
with full serde serialization. Load from YAML, build programmatically, or use
defaults.

## Key Types

| Type | Description |
|------|-------------|
| `RootConfig` | Top-level config: application, version, protocols, registries, tls |
| `ProtocolConfig` | Protocol settings: name, host, port |
| `RegistryConfig` | Registry settings: protocol, address |
| `TlsConfig` | TLS/mTLS: cert paths, key paths, mTLS toggle |

## API

```rust
// RootConfig builder
impl RootConfig {
    pub fn with_application(self, name: impl Into<String>) -> Self;
    pub fn with_version(self, version: impl Into<String>) -> Self;
    pub fn with_protocol(self, protocol: ProtocolConfig) -> Self;
    pub fn with_registry(self, registry: RegistryConfig) -> Self;
}

// ProtocolConfig
impl ProtocolConfig {
    pub fn new(name: impl Into<String>, host: impl Into<String>, port: u16) -> Self;
}
```

## Usage

### Programmatic Configuration

```rust
use dubbo_rs_config::{RootConfig, ProtocolConfig, RegistryConfig};

let config = RootConfig::default()
    .with_application("demo-app")
    .with_version("1.0.0")
    .with_protocol(ProtocolConfig::new("tri", "0.0.0.0", 50051))
    .with_registry(RegistryConfig {
        protocol: "zookeeper".into(),
        address: "127.0.0.1:2181".into(),
    });
```

### YAML Configuration

```yaml
application: "demo-provider"
version: "1.0.0"
protocols:
  - name: "tri"
    host: "0.0.0.0"
    port: 50051
registries:
  - protocol: "zookeeper"
    address: "127.0.0.1:2181"
tls:
  server_cert_chain: "/certs/server.pem"
  server_private_key: "/certs/server.key"
  enable_mtls: true
  client_ca: "/certs/ca.pem"
```

```rust
let yaml = std::fs::read_to_string("dubbo.yaml")?;
let config: RootConfig = serde_yaml::from_str(&yaml)?;
```

### Defaults

| Field | Default |
|-------|---------|
| `ProtocolConfig.name` | `"tri"` |
| `ProtocolConfig.host` | `"0.0.0.0"` |
| `ProtocolConfig.port` | `50051` |
| `RegistryConfig.protocol` | `"zookeeper"` |
| `RegistryConfig.address` | `"127.0.0.1:2181"` |
| `RootConfig.version` | `"1.0.0"` |

## Re-exports

- `dubbo_rs_common as common`
- `dubbo_tls`

## License

Apache-2.0
