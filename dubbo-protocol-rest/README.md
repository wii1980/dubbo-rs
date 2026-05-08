# dubbo-rs-protocol-rest

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-protocol-rest)](https://crates.io/crates/dubbo-rs-protocol-rest)
[![docs.rs](https://docs.rs/dubbo-rs-protocol-rest/badge.svg)](https://docs.rs/dubbo-rs-protocol-rest)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

RESTful HTTP protocol for dubbo-rs.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-protocol-rest = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-protocol-rest
```

Implements the `Protocol` trait with automatic HTTP method selection based on method name prefixes — read operations use GET, others use POST.

## Key Types

| Type | Description |
|------|-------------|
| `RestProtocol` | Implements `Protocol` — `export` and `refer` |
| `RestInvoker` | HTTP invoker — GET for reads, POST for writes, via reqwest |
| `RestExporter` | Wraps an `Invoker` for server-side export |

## Method Mapping

Methods starting with the following prefixes (case-insensitive) are mapped to **GET**:

`get`, `find`, `list`, `query`, `read`, `fetch`, `search`, `count`, `exists`, `check`, `has`, `is`, `can`

All other methods are mapped to **POST**.

## URL Format

```
{protocol}://{ip}:{port}{path}/{method_name}
```

GET requests pass arguments as query parameters (`args0`, `args1`, ...). POST requests send arguments as JSON body.

## Usage

```rust
use dubbo_rs_protocol_rest::{RestProtocol, RestInvoker};
use dubbo_rs_protocol::Protocol;
use dubbo_rs_common::url::URL;

let protocol = RestProtocol::new();

let mut url = URL::new("http", "/com.example.UserService");
url.ip = "127.0.0.1".into();
url.port = "8080".into();

let invoker = protocol.refer(&url).await?;

// GET request (method starts with "get")
let mut ctx = InvocationContext::new("getUser", url.clone());
ctx.arguments.push(b"42".to_vec());
let result = invoker.invoke(&mut ctx).await?;

// POST request (method starts with "create")
let mut ctx = InvocationContext::new("createUser", url);
ctx.arguments.push(br#"{"name":"Alice"}"#.to_vec());
let result = invoker.invoke(&mut ctx).await?;
```

## License

Apache-2.0
