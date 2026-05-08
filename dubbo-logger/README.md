# dubbo-rs-logger

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-logger)](https://crates.io/crates/dubbo-rs-logger)
[![docs.rs](https://docs.rs/dubbo-rs-logger/badge.svg)](https://docs.rs/dubbo-rs-logger)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Logging abstraction for dubbo-rs based on `tracing-subscriber`.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-logger = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-logger
```

## Overview

Provides a `LoggerBuilder` for configuring log levels, output formats, and optional
file output. Supports environment variable fallback via `RUST_LOG` and integrates
with `dubbo_config::RootConfig`.

## Key Types

| Type | Description |
|------|-------------|
| `LoggerBuilder` | Builder for constructing and initializing the subscriber |
| `LoggerConfig` | Log level, output format, optional log file path |
| `LogLevel` | Enum: `Trace`, `Debug`, `Info`, `Warn`, `Error` |
| `OutputFormat` | Enum: `Json`, `Pretty`, `Compact` |

## API

```rust
impl LoggerBuilder {
    pub fn new() -> Self;
    pub fn from_config(config: LoggerConfig) -> Self;
    pub fn from_root_config(root: &RootConfig) -> Self;
    pub fn with_log_level(self, level: LogLevel) -> Self;
    pub fn with_output_format(self, format: OutputFormat) -> Self;
    pub fn with_log_file(self, path: impl Into<PathBuf>) -> Self;
    pub fn init(&self);           // panics on failure
    pub fn try_init(&self) -> Result<...>;  // returns error
}
```

## Usage

### Basic Setup

```rust
use dubbo_rs_logger::{LoggerBuilder, LogLevel, OutputFormat};

LoggerBuilder::new()
    .with_log_level(LogLevel::Info)
    .with_output_format(OutputFormat::Pretty)
    .init();
```

### JSON Logging to File

```rust
use dubbo_rs_logger::{LoggerBuilder, LogLevel, OutputFormat};

LoggerBuilder::new()
    .with_log_level(LogLevel::Debug)
    .with_output_format(OutputFormat::Json)
    .with_log_file("/var/log/dubbo.log")
    .init();
```

### Quick Start with Environment Variable

```rust
// Reads RUST_LOG env var, uses pretty-print subscriber
dubbo_logger::set_rust_log();
```

### From YAML Config

```yaml
log_level: "debug"
output_format: "json"
log_file: "/var/log/dubbo.log"
```

```rust
let config: LoggerConfig = serde_yaml::from_str(&yaml)?;
LoggerBuilder::from_config(config).init();
```

## Behavior

- `RUST_LOG` environment variable takes precedence over configured `log_level`
- Defaults: `Info` level, `Pretty` format, stdout output

## License

Apache-2.0
