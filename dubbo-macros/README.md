# dubbo-rs-macros

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-macros)](https://crates.io/crates/dubbo-rs-macros)
[![docs.rs](https://docs.rs/dubbo-rs-macros/badge.svg)](https://docs.rs/dubbo-rs-macros)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Procedural macro crate for Apache Dubbo Rust — generates service metadata and client proxies from trait definitions.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-macros = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-macros
```

## Macros

### `#[dubbo_rs_service]`

Attribute macro for service implementation blocks. Generates a `__service_methods()` function that returns method names for runtime introspection.

```rust
#[dubbo_rs_service]
impl GreeterService for MyGreeter {
    async fn say_hello(&self, name: String) -> Result<String, Error> { ... }
    async fn goodbye(&self) -> Result<(), Error> { ... }
}

// Auto-generated:
impl MyGreeter {
    fn __service_methods() -> Vec<&'static str> {
        vec!["say_hello", "goodbye"]
    }
}
```

### `#[dubbo_rs_client]`

Attribute macro for service traits. Generates a `{TraitName}Client` struct with `new(invoker)` constructor that implements the original trait. Each method serializes arguments via `serde_json`, calls `Invoker::invoke()`, and deserializes the response.

```rust
#[dubbo_rs_client]
pub trait Greeter {
    async fn say_hello(&self, name: String) -> Result<String, anyhow::Error>;
}

// Auto-generated:
pub struct GreeterClient {
    invoker: Box<dyn dubbo_protocol::Invoker>,
}

impl GreeterClient {
    pub fn new(invoker: Box<dyn dubbo_protocol::Invoker>) -> Self { ... }
}

impl Greeter for GreeterClient { ... }
```

## Example

```rust
use dubbo_rs_macros::{service, client}; // or re-exported via `dubbo`

// Server side — annotate your impl block
#[dubbo_rs_service]
impl UserService for MyUserService {
    async fn get_user(&self, id: u64) -> Result<User, Error> { ... }
}

// Client side — annotate your trait
#[dubbo_rs_client]
pub trait UserService {
    async fn get_user(&self, id: u64) -> Result<User, anyhow::Error>;
}

// Use the generated client
let invoker: Box<dyn dubbo_protocol::Invoker> = /* ... */;
let client = UserServiceClient::new(invoker);
let user = client.get_user(42).await?;
```

## License

Apache-2.0
