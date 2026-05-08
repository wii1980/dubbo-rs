# dubbo-rs-serialization-json

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-serialization-json)](https://crates.io/crates/dubbo-rs-serialization-json)
[![docs.rs](https://docs.rs/dubbo-rs-serialization-json/badge.svg)](https://docs.rs/dubbo-rs-serialization-json)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

JSON serialization for Dubbo RPC via `serde_json`.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-serialization-json = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-serialization-json
```

## Overview

Implements the `Serialization` trait with JSON validation and normalization.
Both `serialize` and `deserialize` parse the input as `serde_json::Value` and
re-serialize it, ensuring well-formed JSON output regardless of input formatting.

## Key Types

| Type | Description |
|------|-------------|
| `JsonSerialization` | Implements `dubbo_serialization::Serialization` |
| content type | `"application/json"` |

## API

```rust
impl Serialization for JsonSerialization {
    fn content_type(&self) -> &'static str;       // "application/json"
    fn serialize(&self, data: &[u8]) -> Result<Vec<u8>>;
    fn deserialize(&self, data: &[u8]) -> Result<Vec<u8>>;
}
```

## Usage

```rust
use dubbo_rs_serialization_json::JsonSerialization;
use dubbo_rs_serialization::Serialization;

let ser = JsonSerialization::new();

// Serialize: validates and normalizes JSON
let input = br#"{"name":"dubbo","version":3}"#;
let encoded = ser.serialize(input)?;

// Deserialize: pass-through with validation
let decoded = ser.deserialize(&encoded)?;

assert_eq!(ser.content_type(), "application/json");
```

## Behavior

- `serialize()` — parses input as JSON, re-serializes for normalization
- `deserialize()` — identical validation + re-serialize pass-through
- Invalid JSON input returns an error on both methods

## Re-exports

- `dubbo_rs_common as common`
- `dubbo_rs_serialization as serialization`

## License

Apache-2.0
