# dubbo-rs-common

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-common)](https://crates.io/crates/dubbo-rs-common)
[![docs.rs](https://docs.rs/dubbo-rs-common/badge.svg)](https://docs.rs/dubbo-rs-common)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Core foundation library for Apache Dubbo Rust — provides URL, Node trait, constants, SPI extension registry, and RPC error types.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-common = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-common
```

## Key Public Types

### `URL`

Service address descriptor used throughout the Dubbo framework.

```rust
pub struct URL {
    pub protocol: String,
    pub location: String,
    pub ip: String,
    pub port: String,
    pub path: String,
    pub username: String,
    pub password: String,
    pub methods: Vec<String>,
    pub params: HashMap<String, String>,
}
```

Key methods: `new()`, `get_param()`, `set_param()`, `get_method_param()`, `get_service_key()`, `get_address()`, `to_full_string()`.

### `Node` trait

Lifecycle interface shared by Invoker, Protocol, and other components.

```rust
pub trait Node: Send + Sync {
    fn get_url(&self) -> &URL;
    fn is_available(&self) -> bool;
    fn destroy(&self);
}
```

### `RPCError` enum

Typed RPC error with bidirectional status code mapping.

Variants: `ClientTimeout`, `ServerTimeout`, `BadRequest`, `BadResponse`, `ServiceNotFound`, `ServiceError`, `ServerError`, `ClientError`, `ServerThreadpoolExhausted`.

Methods: `from_status_code()`, `status_code()`.

### `ExtensionRegistry<T>`

Generic SPI-style extension registry with factory-based lazy instantiation.

Methods: `register_factory()`, `get_or_create_extension()`, `set_default()`, `has_extension()`, `available_extensions()`.

### Constants (`constants` module)

Protocol names (`DUBBO_PROTOCOL`, `TRIPLE_PROTOCOL`), registry types, load balance strategies, cluster strategies, serialization IDs, status codes, and Dubbo header magic bytes.

## Modules

| Module | Description |
|--------|-------------|
| `constants` | Protocol, registry, load balance, and status code constants |
| `error` | `RPCError` enum with status code conversion |
| `extension` | `ExtensionRegistry<T>` SPI extension mechanism |
| `node` | `Node` trait for component lifecycle |
| `url` | `URL` struct for service addressing |

## Example

```rust
use dubbo_rs_common::url::URL;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::extension::ExtensionRegistry;

// Build a service URL
let mut url = URL::new("tri", "/org.example.GreetService");
url.ip = "127.0.0.1".to_string();
url.port = "50051".to_string();
url.set_param("timeout", "3000");
url.set_param("version", "1.0.0");

assert_eq!(url.get_address(), "127.0.0.1:50051");
assert_eq!(url.get_version(), "1.0.0");

// Status code round-trip
let err = RPCError::ServiceNotFound("Greeter".into());
let code = err.status_code();
let roundtrip = RPCError::from_status_code(code, "Greeter");
assert_eq!(err, roundtrip);
```

## License

Apache-2.0
