# dubbo-rs-serialization-protobuf

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-serialization-protobuf)](https://crates.io/crates/dubbo-rs-serialization-protobuf)
[![docs.rs](https://docs.rs/dubbo-rs-serialization-protobuf/badge.svg)](https://docs.rs/dubbo-rs-serialization-protobuf)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Protobuf serialization for Apache Dubbo Rust — implements the `Serialization` trait as a byte-level pass-through, since actual protobuf encoding is handled by `tonic` (Triple protocol) or `prost::Message` directly (Dubbo TCP).

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-serialization-protobuf = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-serialization-protobuf
```

## Key Public Types

### `ProtobufSerialization`

```rust
impl Serialization for ProtobufSerialization {
    fn content_type(&self) -> &'static str { "application/grpc+proto" }
    fn serialize(&self, data: &[u8]) -> Result<Vec<u8>>;
    fn deserialize(&self, data: &[u8]) -> Result<Vec<u8>>;
}
```

Constructors: `ProtobufSerialization::new()`, `Default::default()`.

## Serialization Strategy

| Context | Encoding handled by |
|---------|-------------------|
| Triple (gRPC) | `tonic` transport layer with code-generated stubs |
| Dubbo TCP (SerializationId=12) | `prost::Message::encode` / `Message::decode` |
| Standalone `Serialization` trait | Byte pass-through (this crate) |

## Re-exports

- `pub use dubbo_rs_common as common`
- `pub use dubbo_rs_serialization as serialization`

## Example

```rust
use dubbo_rs_serialization_protobuf::ProtobufSerialization;
use dubbo_rs_serialization::Serialization;

let ser = ProtobufSerialization::new();
assert_eq!(ser.content_type(), "application/grpc+proto");

let data = b"encoded protobuf bytes";
let encoded = ser.serialize(data)?;
let decoded = ser.deserialize(&encoded)?;
assert_eq!(decoded, data);
```

## License

Apache-2.0
