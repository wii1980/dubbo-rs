# dubbo-rs-serialization

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-serialization)](https://crates.io/crates/dubbo-rs-serialization)
[![docs.rs](https://docs.rs/dubbo-rs-serialization/badge.svg)](https://docs.rs/dubbo-rs-serialization)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Serialization abstraction for Apache Dubbo Rust — defines a `Serialization` trait that serves as a thin abstraction over wire-format encoding.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-serialization = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-serialization
```

## Key Public Types

### `Serialization` trait

```rust
pub trait Serialization: Send + Sync {
    fn content_type(&self) -> &'static str;
    fn serialize(&self, data: &[u8]) -> Result<Vec<u8>>;
    fn deserialize(&self, data: &[u8]) -> Result<Vec<u8>>;
}
```

| Method | Description |
|--------|-------------|
| `content_type()` | Returns the MIME type for this serialization format |
| `serialize(&[u8])` | Encode raw bytes into wire-format payload |
| `deserialize(&[u8])` | Decode wire-format payload back to raw bytes |

## Re-exports

- `pub use dubbo_rs_common as common` — access to URL, Node, constants, etc.

## Implementations

This trait is implemented by:

- **dubbo-serialization-protobuf** — `"application/grpc+proto"` (byte pass-through)
- **dubbo-serialization-hessian2** — Hessian2 binary encoding
- **dubbo-serialization-json** — JSON encoding via `serde_json`

## Example

```rust
use dubbo_rs_serialization::Serialization;

// Implement a custom serialization
struct JsonSerialization;

impl Serialization for JsonSerialization {
    fn content_type(&self) -> &'static str { "application/json" }
    fn serialize(&self, data: &[u8]) -> anyhow::Result<Vec<u8>> { Ok(data.to_vec()) }
    fn deserialize(&self, data: &[u8]) -> anyhow::Result<Vec<u8>> { Ok(data.to_vec()) }
}
```

## License

Apache-2.0
