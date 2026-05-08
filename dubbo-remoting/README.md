# dubbo-rs-remoting

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-remoting)](https://crates.io/crates/dubbo-rs-remoting)
[![docs.rs](https://docs.rs/dubbo-rs-remoting/badge.svg)](https://docs.rs/dubbo-rs-remoting)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Network transport abstraction for Apache Dubbo Rust — provides `ExchangeClient`, `ExchangeServer`, `Codec`, `Request`/`Response`, and `ConnectionPool`.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-remoting = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-remoting
```

## Key Public Types

### `Request` / `Response`

Wire-level message structs.

```rust
pub struct Request {
    pub id: u64,
    pub is_twoway: bool,
    pub is_event: bool,
    pub data: Vec<u8>,
}

pub struct Response {
    pub id: u64,
    pub status: u8,
    pub data: Vec<u8>,
}
```

`Response` helpers: `success(id, data)`, `error(id, status, data)`, `is_error()`.

### `Codec` trait

Encode/decode requests and responses to/from wire format.

```rust
pub trait Codec: Send + Sync {
    fn encode_request(&self, req: &Request) -> Result<Vec<u8>>;
    fn decode_request(&self, data: &[u8]) -> Result<Request>;
    fn encode_response(&self, resp: &Response) -> Result<Vec<u8>>;
    fn decode_response(&self, data: &[u8]) -> Result<Response>;
}
```

### `ExchangeClient` / `ExchangeServer` traits

Async transport endpoints.

```rust
pub trait ExchangeClient: Send + Sync {
    async fn connect(&mut self, url: &URL) -> Result<()>;
    async fn request(&self, req: Request) -> Result<Response>;
    fn close(&self);
}

pub trait ExchangeServer: Send + Sync {
    async fn bind(&self, url: &URL) -> Result<()>;
    async fn close(&self);
}
```

### `ConnectionPool` trait & `SimpleConnectionPool`

Connection management with factory-based pooling.

```rust
pub trait ConnectionPool: Send + Sync {
    async fn get(&self, url: &URL) -> Result<Box<dyn ExchangeClient>>;
}
```

`SimpleConnectionPool<F>` — HashMap-backed pool with `tokio::Mutex`, replaces existing connections on re-entry.

## Example

```rust
use dubbo_rs_remoting::{Request, Response, ConnectionPool};
use dubbo_rs_remoting::pool::SimpleConnectionPool;

// Create a connection pool with a client factory
let pool = SimpleConnectionPool::new(|| Box::new(MyExchangeClient));

let url = URL::new("tri", "127.0.0.1:20880");
let client = pool.get(&url).await?;

let req = Request::new(1, true, b"payload".to_vec());
let resp = client.request(req).await?;
assert!(!resp.is_error());
```

## License

Apache-2.0
