# dubbo-rs-serialization-hessian2

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-serialization-hessian2)](https://crates.io/crates/dubbo-rs-serialization-hessian2)
[![docs.rs](https://docs.rs/dubbo-rs-serialization-hessian2/badge.svg)](https://docs.rs/dubbo-rs-serialization-hessian2)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Full Hessian 2.0 serialization codec for Apache Dubbo cross-language RPC.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-serialization-hessian2 = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-serialization-hessian2
```

## Overview

Implements the Hessian 2.0 binary serialization protocol as used by dubbo-java.
Supports all standard types, compact encoding variants, object serialization via
class definitions, reference tracking, and Dubbo-specific Java types.

## Modules

| Module | Description |
|--------|-------------|
| `codec` | Encoder/Decoder with tag constants and compact encoding |
| `decoder` | `Decoder` — reads Hessian2 values from a byte buffer |
| `class_def` | `ClassDef` — Hessian2 class definition (`C` tag) |
| `type_descriptor` | `JavaType` — Java JVM type descriptor parser |
| `type_registry` | `TypeRegistry` — register custom classes for POJO serialization |
| `dubbo_types` | `JavaDate`, `BigDecimal`, `StackTraceElement`, `Duration`, `JavaException` |
| `types` | `Hessian2Error` and `Hessian2Result<T>` |

## Supported Types

| Tag | Type | Notes |
|-----|------|-------|
| `N` | null | |
| `T`/`F` | boolean | |
| `I` | int (i32) | Compact: `0x80..0xbf` maps to `-16..47` |
| `L` | long (i64) | Compact: `0xd8..0xef` maps to `-8..15` |
| `D` | double (f64) | Compact: `0x5b`=0.0, `0x5c`=1.0, byte/short/float variants |
| `S`/`R` | string | UTF-8 with chunking and back-references |
| `B` | binary | With chunking support |
| `V` | list | Typed or untyped variable-length list |
| `M`/`H` | map | Typed map and untyped map |
| `R` | ref | Shared reference with cycle detection |
| `C`/`O` | class-def / object | POJO serialization |

## Usage

```rust
use dubbo_rs_serialization_hessian2::codec::encoder::Encoder;
use dubbo_rs_serialization_hessian2::decoder::Decoder;

// Encode
let mut enc = Encoder::new();
enc.write_string("hello");
enc.write_int(42);
enc.write_bool(true);
let encoded = enc.finish();

// Decode
let mut dec = Decoder::new(&encoded);
let s: String = dec.read_string()?;
let i: i32 = dec.read_int()?;
let b: bool = dec.read_bool()?;

// Custom type registration
use dubbo_rs_serialization_hessian2::type_registry::TypeRegistry;
let mut registry = TypeRegistry::new();
registry.register("com.example.MyDto", vec![
    "field1".into(), "field2".into(),
]);
```

### Dubbo-specific Types

```rust
use dubbo_rs_serialization_hessian2::dubbo_types::{JavaDate, JavaException, StackTraceElement};

// Date → epoch millis
let date = JavaDate::from_millis(1700000000000i64);

// Exception with detailMessage, stackTrace, cause
let exc = JavaException::new("Something went wrong");
```

## Limitations

- Cross-language verification with Java is not yet validated
- 56 roundtrip unit tests cover all core types

## License

Apache-2.0
