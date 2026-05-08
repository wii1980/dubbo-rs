# dubbo-rs-protocol

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-protocol)](https://crates.io/crates/dubbo-rs-protocol)
[![docs.rs](https://docs.rs/dubbo-rs-protocol/badge.svg)](https://docs.rs/dubbo-rs-protocol)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Protocol abstraction layer for Apache Dubbo Rust — defines the core `Protocol`, `Invoker`, `Exporter` traits along with `InvocationContext`, `RPCResult`, and streaming interfaces.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-protocol = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-protocol
```

## Key Public Types

### `Protocol` trait

Top-level protocol abstraction for exporting and referring services.

```rust
#[async_trait]
pub trait Protocol: Send + Sync {
    async fn export(&self, invoker: Box<dyn Invoker>) -> Result<Box<dyn Exporter>>;
    async fn refer(&self, url: &URL) -> Result<Box<dyn Invoker>>;
    fn destroy(&self);
}
```

### `Invoker` trait

Unified invocation interface. Extends `Node` for lifecycle management.

```rust
#[async_trait]
pub trait Invoker: Node {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult>;
}
```

### `Exporter` trait

Handle for an exported service.

```rust
pub trait Exporter: Send + Sync {
    fn get_invoker(&self) -> &dyn Invoker;
    fn un_export(&self);
}
```

### `InvocationContext`

Call context with builder pattern, carrying method name, parameter types, arguments, attachments, and URL.

```rust
let ctx = InvocationContext::new("sayHello", url)
    .with_parameter_types(vec!["Ljava/lang/String;".to_string()])
    .with_arguments(vec![payload])
    .with_attachment("trace_id", "abc123");
```

### `RPCResult`

Call result holding optional value, optional error, and attachments.

Constructors: `success(value)`, `from_error(error)`. Method: `is_error()`.

### Streaming Traits

| Trait | Purpose |
|-------|---------|
| `ServerStream` | Server-streaming: async iterator over `RPCResult` chunks |
| `ClientStream` | Client-streaming: `send()` + `close_and_recv()` |
| `BidiStream` | Bidirectional: `send()` + `recv()` + `close_send()` |

## Example

```rust
use dubbo_rs_protocol::{Protocol, Invoker, Exporter, InvocationContext, RPCResult};

// Implement a custom invoker
struct MyInvoker { url: URL }

impl Node for MyInvoker {
    fn get_url(&self) -> &URL { &self.url }
    fn is_available(&self) -> bool { true }
    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for MyInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult> {
        Ok(RPCResult::success(b"response".to_vec()))
    }
}
```

## License

Apache-2.0
