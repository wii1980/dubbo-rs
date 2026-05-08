# dubbo-rs-metadata

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-metadata)](https://crates.io/crates/dubbo-rs-metadata)
[![docs.rs](https://docs.rs/dubbo-rs-metadata/badge.svg)](https://docs.rs/dubbo-rs-metadata)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Metadata center for application-level service discovery in dubbo-rs.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-metadata = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-metadata
```

Provides data structures for describing Dubbo services and a storage abstraction for metadata management. Supports JSON serialization via serde.

## Key Types

| Type | Description |
|------|-------------|
| `MetadataInfo` | Application metadata — `application`, `revision`, `services`, `attributes` |
| `ServiceDefinition` | Per-service metadata — `interface`, `version`, `group`, `methods`, `params` |
| `MethodDefinition` | Per-method metadata — `name`, `parameter_types`, `return_type`, `stream_type`, `oneway` |
| `StreamType` | Enum: `Unary` (default), `ClientStreaming`, `ServerStreaming`, `BidiStreaming` |
| `MetadataStorage` | Trait: `store`, `get`, `remove`, `applications` |
| `InMemoryMetadataStorage` | Concurrent in-memory storage backed by `DashMap` |
| `MetadataService` | Async trait: `get_metadata_info`, `get_service_definition`, `get_exported_service_urls`, `echo` |
| `DefaultMetadataService` | Concrete `MetadataService` backed by `MetadataStorage` |

## Usage

```rust
use std::sync::Arc;
use dubbo_rs_metadata::*;

// Define services with builder pattern
let service = ServiceDefinition::new("com.example.Greeter")
    .with_version("1.0.0")
    .with_group("default")
    .with_method(
        MethodDefinition::new("sayHello", "Ljava/lang/String;")
            .with_param("Ljava/lang/String;")
    )
    .with_method(
        MethodDefinition::new("streamData", "V")
            .with_stream_type(StreamType::ServerStreaming)
    );

// Build application metadata
let metadata = MetadataInfo::new("demo-provider")
    .with_revision(1)
    .with_service(service);

// Store in memory
let storage = Arc::new(InMemoryMetadataStorage::new());
storage.store(metadata);

// Query via MetadataService
let service = DefaultMetadataService::new(storage);
let info = service.get_metadata_info("demo-provider".into()).await;
let urls = service.get_exported_service_urls("demo-provider".into()).await;

// JSON serialization
let json = serde_json::to_string(&info).unwrap();
```

## License

Apache-2.0
