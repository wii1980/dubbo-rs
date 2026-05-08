# dubbo-rs-filter

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-filter)](https://crates.io/crates/dubbo-rs-filter)
[![docs.rs](https://docs.rs/dubbo-rs-filter/badge.svg)](https://docs.rs/dubbo-rs-filter)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Filter chain framework and built-in service governance filters for dubbo-rs.

Provides the `Filter` trait for intercepting RPC invocations in a chain-of-responsibility pattern, `FilterChain` for composing filters, and a collection of production-ready filters for health checks, auth, logging, rate limiting, circuit breaking, and generic invocation.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-filter = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-filter
```

## Key Types

### `Filter` trait

Intercepts invocations with pre/post processing:

| Method | Description |
|--------|-------------|
| `invoke(ctx, next)` | Pre-process; call `next.invoke(ctx)` to proceed |
| `on_response(ctx, result, invoker)` | Post-process the result (default: passthrough) |

### `FilterChain`

Composes a `Vec<Box<dyn Filter>>` around a base invoker. Execute outermost-first on invoke, innermost-first on response.

### Built-in Filters

| Filter | Description |
|--------|-------------|
| `EchoFilter` | Health check — returns echo payload on `$echo` method |
| `TokenFilter` | Bearer token validation from invocation attachments |
| `AccessLogFilter` | Logs request/response via `tracing` (INFO/WARN) |
| `TPSLimitFilter` | Rate limiting with pluggable `TPSLimiter` trait |
| `GracefulShutdownFilter` | Rejects new calls after shutdown signal; shared `AtomicBool` flag |
| `ActiveLimitFilter` | Rejects when concurrent active calls exceed max |
| `ExecuteLimitFilter` | Semaphore-based concurrency limiter |
| `CircuitBreakerFilter` | Wraps `CircuitBreaker` for Sentinel-style fault tolerance |

### `CircuitBreaker`

Sliding-window breaker with `Closed` → `Open` → `HalfOpen` states. Key methods: `is_call_permitted()`, `record_success()`, `record_failure()`. Builder: `with_failure_threshold(10)`, `with_recovery_timeout(60s)`, `with_max_half_open_probes(3)`.

### `GenericService` trait + `GenericInvoker`

Dynamic invocation without compiled POJOs via method name, type descriptors, and JSON arguments.

## Usage

### Building a filter chain

```rust
use dubbo_rs_filter::*;
use dubbo_rs_protocol::Invoker;
fn build_chain(base: Box<dyn Invoker>) -> Box<dyn Invoker> {
    FilterChain::new(vec![
        Box::new(EchoFilter),
        Box::new(TokenFilter::new("secret")),
        Box::new(AccessLogFilter),
        Box::new(ActiveLimitFilter::new(100)),
    ], base).build()
}
```

### Circuit breaker

```rust
use std::sync::Arc;
use std::time::Duration;
use dubbo_rs_filter::{CircuitBreaker, CircuitBreakerFilter, CircuitBreakerState};
let breaker = Arc::new(CircuitBreaker::new().with_failure_threshold(5)
    .with_recovery_timeout(Duration::from_secs(30)));
let filter = CircuitBreakerFilter::new(breaker.clone());
```

### Generic invocation

```rust
use dubbo_rs_filter::{GenericService, GenericInvoker};
let svc = GenericInvoker::new(base_invoker, url);
let res = svc.invoke("sayHello".into(),
    vec!["Ljava/lang/String;".into()], vec!["\"world\"".into()]).await.unwrap();
```

## Re-exports

- `pub use dubbo_rs_common as common`
- `pub use dubbo_rs_protocol as protocol`

## License

Apache-2.0
