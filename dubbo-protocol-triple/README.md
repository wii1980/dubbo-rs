# dubbo-rs-protocol-triple

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-protocol-triple)](https://crates.io/crates/dubbo-rs-protocol-triple)
[![docs.rs](https://docs.rs/dubbo-rs-protocol-triple/badge.svg)](https://docs.rs/dubbo-rs-protocol-triple)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Triple protocol implementation for Apache Dubbo Rust — HTTP/2 gRPC-compatible protocol based on `tonic`. Supports unary RPC via `TripleRequestWrapper` / `TripleResponseWrapper`.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-protocol-triple = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-protocol-triple
```

## Key Public Types

### `TripleProtocol`

Implements `Protocol` trait. `export()` creates a `TripleExporter`, `refer()` creates a `TripleInvoker` with lazy connection.

### `TripleInvoker`

Wraps a tonic `Channel` behind `Arc<RwLock<Option<Channel>>>` for lazy connection. On `invoke()`, encodes a `TripleRequestWrapper` with prost, sends a unary gRPC call, and decodes the `TripleResponseWrapper`.

```rust
impl Node for TripleInvoker {
    fn get_url(&self) -> &URL;
    fn is_available(&self) -> bool; // true when url.ip is non-empty
    fn destroy(&self);
}
```

Methods: `from_url()`, `connect()`, `channel()`.

### `TripleExporter`

Holds a `Box<dyn Invoker>`. Implements `Exporter` with `get_invoker()` and `un_export()`.

### Protobuf Wrappers (`triple` module)

Generated from `proto/triple_wrapper.proto`:

- `triple::TripleRequestWrapper` — `serialize_type`, `args`, `arg_types`
- `triple::TripleResponseWrapper` — `serialize_type`, `data`, `r#type`

## Re-exports

- `pub use dubbo_rs_common as common`
- `pub use dubbo_rs_protocol as protocol`
- `pub use dubbo_rs_remoting as remoting`

## Example

```rust
use dubbo_rs_protocol_triple::{TripleProtocol, TripleInvoker};
use dubbo_rs_protocol::{Protocol, Invoker, InvocationContext};

// Server side — export a service
let protocol = TripleProtocol::new();
let invoker: Box<dyn Invoker> = /* your service invoker */;
let exporter = protocol.export(invoker).await?;

// Client side — refer and invoke
let mut url = URL::new("tri", "/org.example.GreetService");
url.ip = "127.0.0.1".to_string();
url.port = "50051".to_string();

let invoker = TripleInvoker::from_url(url.clone());
invoker.connect().await?;

let mut ctx = InvocationContext::new("sayHello", url);
ctx.arguments = vec![b"world".to_vec()];
let result = invoker.invoke(&mut ctx).await?;
```

## License

Apache-2.0
