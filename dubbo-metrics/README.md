# dubbo-rs-metrics

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-metrics)](https://crates.io/crates/dubbo-rs-metrics)
[![docs.rs](https://docs.rs/dubbo-rs-metrics/badge.svg)](https://docs.rs/dubbo-rs-metrics)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Prometheus-based RPC metrics collection and export for dubbo-rs.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-metrics = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-metrics
```

## Overview

Provides `MetricsCollector` for recording RPC request counts, latency histograms,
and error counters with service/method/status labels. `MetricsExporter` produces
Prometheus text exposition format for scraping.

## Key Types

| Type | Description |
|------|-------------|
| `MetricsCollector` | Central metrics registry with recording methods |
| `MetricsCollectorBuilder` | Builder with optional namespace prefix |
| `MetricsExporter` | Exports metrics in Prometheus text format |
| `Counter` / `Gauge` / `Histogram` | Typed wrappers around `prometheus` types |

## Metric Names

| Constant | Metric | Labels |
|----------|--------|--------|
| `METRIC_RPC_REQUESTS_TOTAL` | `rpc_requests_total` | service, method, status |
| `METRIC_RPC_REQUEST_DURATION_SECONDS` | `rpc_request_duration_seconds` | service, method |
| `METRIC_RPC_ERRORS_TOTAL` | `rpc_errors_total` | service, method, error_type |

## Usage

```rust
use dubbo_rs_metrics::{MetricsCollector, MetricsCollectorBuilder, MetricsExporter};

// Create collector (default namespace)
let collector = MetricsCollector::new()?;

// Record metrics
collector.record_request("com.example.GreetService", "sayHello", "success");
collector.record_duration("com.example.GreetService", "sayHello", 0.042);
collector.record_error("com.example.GreetService", "sayHello", "timeout");

// Export for Prometheus scraping
let exporter = MetricsExporter::new(&collector);
let text = exporter.export_metrics();
assert!(text.contains("rpc_requests_total"));
```

### With Namespace Prefix

```rust
let collector = MetricsCollectorBuilder::new()
    .namespace("dubbo")
    .build()?;
// Produces: dubbo_rpc_requests_total, dubbo_rpc_request_duration_seconds, etc.
```

### Convenience: Record Full Invocation

```rust
// Automatically derives service, method, status, and error_type
collector.record_invocation(&ctx, &result, duration_secs);
```

## Re-exports

- `dubbo_rs_common as common`
- `dubbo_rs_protocol as protocol`

## License

Apache-2.0
