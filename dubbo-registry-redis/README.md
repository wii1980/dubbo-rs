# dubbo-rs-registry-redis

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-registry-redis)](https://crates.io/crates/dubbo-rs-registry-redis)
[![docs.rs](https://docs.rs/dubbo-rs-registry-redis/badge.svg)](https://docs.rs/dubbo-rs-registry-redis)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Redis-based service registry for dubbo-rs — fully compatible with dubbo-java's `RedisRegistry`.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-registry-redis = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-registry-redis
```

## Overview

Implements the `Registry` trait from `dubbo-rs-registry` using Redis Hashes for storage and Redis Pub/Sub for real-time notifications. The data model and key structure match dubbo-java's `RedisRegistry` exactly, enabling cross-language service discovery between Rust and Java Dubbo services.

## Key Structure (compatible with Java)

```
Root:           /dubbo/               (configurable via `group` param)
Service path:   /dubbo/{interface}
Category key:   /dubbo/{interface}/{category}
```

Each category key is a Redis Hash:
- **Field**: provider URL in Java-compatible format (`{protocol}://{host}:{port}/{path}?{params}`)
- **Value**: expiration timestamp (epoch milliseconds)

## Key Types

| Type | Description |
|------|-------------|
| `RedisRegistry` | Implements `Registry` trait via Redis |

## Configuration

| URL Parameter | Default | Description |
|---------------|---------|-------------|
| `group` / `root` | `dubbo` | Root path prefix |
| `session` / `expire_period` | `60000` | Session timeout in milliseconds |
| `password` | — | Redis password |
| `db` | `0` | Redis database index |

## Usage

```rust
use std::sync::Arc;
use dubbo_rs_common::url::URL;
use dubbo_rs_registry::Registry;
use dubbo_rs_registry_redis::RedisRegistry;

#[tokio::main]
async fn main() {
    // Create registry pointing to Redis
    let redis_url = {
        let mut u = URL::new("redis", "");
        u.ip = "127.0.0.1".to_string();
        u.port = "6379".to_string();
        u
    };

    let registry = RedisRegistry::new(redis_url);

    // Register a provider — HSET + PUBLISH register
    let mut provider_url = URL::new("tri", "/com.example.GreetService");
    provider_url.ip = "10.0.0.1".to_string();
    provider_url.port = "50051".to_string();
    provider_url.set_param("version", "1.0.0");
    registry.register(provider_url).await.unwrap();

    // Subscribe to provider changes — PSUBSCRIBE + initial HGETALL
    // let listener = Arc::new(MyListener { ... });
    // registry.subscribe(service_url, listener).await.unwrap();

    // Cleanup
    registry.destroy();
}
```

## Storage Layout in Redis

```
Hash: /dubbo/com.example.GreetService/providers
─────────────────────────────────────────────────────────────
Field (provider URL)                          │ Expiry (ms)
tri://10.0.0.1:50051/com.example.GreetServi.. │ 1709123456789
─────────────────────────────────────────────────────────────

Pub/Sub channels:
  /dubbo/com.example.GreetService/providers  ← register/unregister
```

## Notifier Architecture

Each subscribed service spawns a background notifier task that:
1. **PSUBSCRIBEs** to `{servicePath}/*` for real-time change notifications
2. On `REGISTER`/`UNREGISTER` message: **HGETALL** the category key → filter expired entries → notify listeners
3. Automatically reconnects on connection failure

## Heartbeat

A background task runs every `expire_period / 2` milliseconds, extending the expiration timestamp of all locally registered services. This matches Java's `deferExpired()` scheduled executor behaviour.

## Re-exports

- `dubbo_rs_common as common`
- `dubbo_rs_registry as registry`

## License

Apache-2.0
