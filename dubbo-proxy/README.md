# dubbo-rs-proxy

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-proxy)](https://crates.io/crates/dubbo-rs-proxy)
[![docs.rs](https://docs.rs/dubbo-rs-proxy/badge.svg)](https://docs.rs/dubbo-rs-proxy)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Proxy factory abstraction for creating RPC invoker proxies.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-proxy = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-proxy
```

## Overview

Provides the `ProxyFactory` trait and a default `DefaultProxyFactory` implementation
that wraps a closure-based invoker factory. The proxy layer sits between the
high-level client/server API and the low-level protocol invokers.

## Key Types

| Type | Description |
|------|-------------|
| `ProxyFactory` | Trait for creating client proxies and server-side invokers |
| `DefaultProxyFactory` | Default implementation using a closure-based factory |
| `InvokerFactoryFn` | Function type: `Fn(&URL) -> Result<Box<dyn Invoker>>` |

## API

```rust
// ProxyFactory trait
trait ProxyFactory: Send + Sync {
    fn get_proxy(&self, url: &URL) -> Result<Box<dyn Invoker>>;
    fn get_invoker(&self, invoker: Box<dyn Invoker>) -> Box<dyn Invoker>;
}

// DefaultProxyFactory wraps a custom invoker creation closure
impl DefaultProxyFactory {
    pub fn new<F>(factory: F) -> Self
    where
        F: Fn(&URL) -> Result<Box<dyn Invoker>> + Send + Sync + 'static;
}
```

## Usage

```rust
use dubbo_rs_proxy::{DefaultProxyFactory, ProxyFactory};
use dubbo_rs_common::url::URL;

// Create a factory with a custom invoker creation function
let factory = DefaultProxyFactory::new(|url| {
    // Build and return a Box<dyn Invoker> for the given URL
    // e.g., connect to the remote server and create an invoker
    todo!("create invoker for {}", url.path)
});

// Create a client proxy
let url = URL::new("tri", "/com.example.GreetService");
let proxy = factory.get_proxy(&url)?;

// get_invoker is an identity pass-through for server-side use
let invoker = factory.get_invoker(proxy);
```

## Re-exports

- `dubbo_rs_cluster as cluster`
- `dubbo_rs_common as common`
- `dubbo_rs_protocol as protocol`

## License

Apache-2.0
