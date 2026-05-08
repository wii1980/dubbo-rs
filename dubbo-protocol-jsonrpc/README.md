# dubbo-rs-protocol-jsonrpc

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-protocol-jsonrpc)](https://crates.io/crates/dubbo-rs-protocol-jsonrpc)
[![docs.rs](https://docs.rs/dubbo-rs-protocol-jsonrpc/badge.svg)](https://docs.rs/dubbo-rs-protocol-jsonrpc)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

JSON-RPC 2.0 over HTTP protocol for dubbo-rs.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-protocol-jsonrpc = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-protocol-jsonrpc
```

Implements the `Protocol` trait using reqwest for HTTP transport, conforming to the [JSON-RPC 2.0 specification](https://www.jsonrpc.org/specification).

## Key Types

| Type | Description |
|------|-------------|
| `JsonRpcProtocol` | Implements `Protocol` — `export` and `refer` |
| `JsonRpcInvoker` | HTTP client invoker — POSTs JSON-RPC request objects via reqwest |
| `JsonRpcExporter` | Wraps an `Invoker` for server-side export |
| `JsonRpcRequest` | Request: `jsonrpc`, `method`, `params`, `id` (auto-incremented) |
| `JsonRpcResponse` | Response: `jsonrpc`, `result`, `error`, `id` |
| `JsonRpcErrorObj` | Error object: `code`, `message`, `data` — maps to `RPCError` |
| `error_code` | Standard codes: `PARSE_ERROR` (-32700), `INVALID_REQUEST` (-32600), `METHOD_NOT_FOUND` (-32601), `INVALID_PARAMS` (-32602), `INTERNAL_ERROR` (-32603) |

## Request Format

```json
{
  "jsonrpc": "2.0",
  "method": "com.example.GreetService.sayHello",
  "params": ["world"],
  "id": 1
}
```

## Usage

```rust
use dubbo_rs_protocol_jsonrpc::JsonRpcProtocol;
use dubbo_rs_protocol::Protocol;
use dubbo_rs_common::url::URL;

let protocol = JsonRpcProtocol::new();

let mut url = URL::new("jsonrpc", "/com.example.GreetService");
url.ip = "127.0.0.1".into();
url.port = "8080".into();

let invoker = protocol.refer(&url).await?;

// Invoke — method name is built from interface attachment or URL path
let mut ctx = InvocationContext::new("sayHello", url)
    .with_attachment("interface", "com.example.GreetService")
    .with_arguments(vec![b"\"world\"".to_vec()]);
let result = invoker.invoke(&mut ctx).await?;
```

## License

Apache-2.0
