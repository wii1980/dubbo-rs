# dubbo-rs-tracing

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-tracing)](https://crates.io/crates/dubbo-rs-tracing)
[![docs.rs](https://docs.rs/dubbo-rs-tracing/badge.svg)](https://docs.rs/dubbo-rs-tracing)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

OpenTelemetry distributed tracing for dubbo-rs with W3C Trace Context propagation.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-tracing = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-tracing
```

## Overview

Implements a `TracingFilter` that integrates with the dubbo-rs filter chain to
propagate trace context across RPC calls. Supports W3C `traceparent` header format,
configurable sampling rates, and optional OTLP exporter for span export.

## Key Types

| Type | Description |
|------|-------------|
| `TracingFilter` | `Filter` implementation for automatic trace propagation |
| `TracingConfig` | Configuration: endpoint, sample_rate, enable_trace_id_log |
| `TraceContextPropagator` | Trait for custom trace header extraction/injection |
| `W3CPropagator` | Default W3C `traceparent` header propagator |

## API

```rust
impl TracingConfig {
    pub fn new() -> Self;
    pub fn with_endpoint(self, endpoint: impl Into<String>) -> Self;
    pub fn with_sample_rate(self, rate: f64) -> Self;  // 0.0..1.0, clamped
    pub fn with_trace_id_log(self, enable: bool) -> Self;
}

impl TracingFilter {
    pub fn new(config: TracingConfig) -> Self;
    pub fn new_with_propagator(config: TracingConfig, propagator: impl TraceContextPropagator) -> Self;
}
```

## Usage

### Basic Tracing

```rust
use dubbo_rs_tracing::{TracingFilter, TracingConfig};

let config = TracingConfig::new()
    .with_sample_rate(1.0)
    .with_trace_id_log(true);

let filter = TracingFilter::new(config);
// Add to filter chain — each RPC call gets a traceparent header
```

### With OTLP Exporter

```rust
let config = TracingConfig::new()
    .with_endpoint("http://localhost:4317")
    .with_sample_rate(0.5);

let filter = TracingFilter::new(config);
// 50% of requests will be sampled and exported to OTLP collector
```

### Custom Propagator

```rust
use dubbo_rs_tracing::{TraceContextPropagator, TracingFilter, TracingConfig};

struct MyPropagator;
impl TraceContextPropagator for MyPropagator {
    fn extract(&self, ctx: &InvocationContext) -> Option<String> {
        ctx.attachments.get("x-trace-id").cloned()
    }
    fn inject(&self, ctx: &mut InvocationContext, traceparent: &str) {
        ctx.attachments.insert("x-trace-id".into(), traceparent.into());
    }
}

let filter = TracingFilter::new_with_propagator(TracingConfig::new(), MyPropagator);
```

## W3C traceparent Format

```
00-{trace_id(32hex)}-{span_id(16hex)}-{flags(2hex)}
```

Trace ID is preserved across hops; a new span ID is generated per invocation.

## Re-exports

- `dubbo_rs_common as common`
- `dubbo_rs_protocol as protocol`
- `dubbo_rs_filter as filter`

## License

Apache-2.0
