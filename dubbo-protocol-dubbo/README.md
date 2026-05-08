# dubbo-rs-protocol-dubbo

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-protocol-dubbo)](https://crates.io/crates/dubbo-rs-protocol-dubbo)
[![docs.rs](https://docs.rs/dubbo-rs-protocol-dubbo/badge.svg)](https://docs.rs/dubbo-rs-protocol-dubbo)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Dubbo TCP binary protocol (Dubbo2 compatible) with Hessian2 body serialization.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-protocol-dubbo = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-protocol-dubbo
```

## Overview

Implements the Dubbo2 binary protocol with a 16-byte header and Hessian2-encoded
body. Compatible with dubbo-java 2.x/3.x servers and clients.

### Protocol Header (16 bytes)

| Offset | Size | Field       | Description                      |
|--------|------|-------------|----------------------------------|
| 0      | 2    | Magic       | `0xdabb`                         |
| 2      | 1    | Flags       | Req/Res, TwoWay, Event, SerialId |
| 3      | 1    | Status      | Response status code             |
| 4      | 8    | Request ID  | Unique request identifier        |
| 12     | 4    | Body Length | Length of body data              |

## Key Types

| Type              | Description                                               |
|-------------------|-----------------------------------------------------------|
| `DubboProtocol`   | Implements `Protocol` trait — `export()` and `refer()`    |
| `DubboCodec`      | 16-byte header codec implementing `dubbo_rs_remoting::Codec` |
| `DubboInvoker`    | Client-side invoker with lazy `DubboClient` creation      |
| `DubboExporter`   | Server-side exporter holding invoker reference            |
| `SerializationId` | Enum: `Hessian2`(2), `Protobuf`(12), `Json`(21)           |

## Modules

| Module      | Description                                               |
|-------------|-----------------------------------------------------------|
| `codec`     | Header encoding/decoding, `DubboCodec`, `SerializationId` |
| `protocol`  | `DubboProtocol`, `DubboInvoker`, `DubboExporter`          |
| `transport` | `DubboClient` (TCP), `DubboServer` (TCP with shutdown)    |
| `body`      | Hessian2 request/response body encoding                   |

## Usage

```rust
use dubbo_rs_protocol_dubbo::{DubboProtocol, DubboCodec, codec::SerializationId};
use dubbo_rs_protocol::Protocol;

// Server-side: export a service
let protocol = DubboProtocol::new();
let exporter = protocol.export(my_invoker).await?;

// Client-side: refer to a remote service
let url = URL::new("dubbo", "192.168.1.1:20880/com.example.DemoService");
let invoker = protocol.refer(&url).await?;

// Codec for custom transport
let codec = DubboCodec::new(SerializationId::Hessian2);
```

## Request Body Format (Hessian2)

| Field            | Type   |
|------------------|--------|
| dubbo_version    | string |
| service_path     | string |
| service_version  | string |
| method_name      | string |
| param_types_desc | string |
| arguments        | list   |
| attachments      | map    |

## License

Apache-2.0
