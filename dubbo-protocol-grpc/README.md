# dubbo-rs-protocol-grpc

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-protocol-grpc)](https://crates.io/crates/dubbo-rs-protocol-grpc)
[![docs.rs](https://docs.rs/dubbo-rs-protocol-grpc/badge.svg)](https://docs.rs/dubbo-rs-protocol-grpc)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Native gRPC protocol support for dubbo-rs.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-protocol-grpc = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-protocol-grpc
```

Implements the `Protocol` trait from `dubbo-rs-protocol` using tonic for HTTP/2-based gRPC calls. Supports standard gRPC wire format with length-prefixed framing.

## Key Types

| Type | Description |
|------|-------------|
| `GrpcProtocol` | Implements `Protocol` — `export` and `refer` |
| `GrpcInvoker` | gRPC client invoker — connect via tonic `Channel`, invoke with gRPC framing |
| `GrpcExporter` | Wraps an `Invoker` for server-side export |

## Wire Format

- **Encoding**: 5-byte gRPC length-prefixed frame (1 byte compressed flag + 4 byte big-endian length + payload).
- **Path**: `/{service}/{method}` with `content-type: application/grpc` and `te: trailers` headers.
- **Compression**: Not supported — returns error for compressed frames.

## Usage

```rust
use dubbo_rs_protocol_grpc::{GrpcProtocol, GrpcInvoker};
use dubbo_rs_protocol::Protocol;
use dubbo_rs_common::url::URL;

// Create a protocol instance
let protocol = GrpcProtocol::new();

// Refer a remote service
let mut url = URL::new("grpc", "/com.example.GreetService");
url.ip = "127.0.0.1".into();
url.port = "50051".into();

let invoker = protocol.refer(&url).await?;
invoker.connect().await?;

// Invoke an RPC
let mut ctx = InvocationContext::new("sayHello", url);
ctx.arguments.push(b"world".to_vec());
let result = invoker.invoke(&mut ctx).await?;

// Export a local service
let exporter = protocol.export(local_invoker).await?;
```

## License

Apache-2.0
