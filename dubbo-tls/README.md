# dubbo-rs-tls

[![crates.io](https://img.shields.io/crates/v/dubbo-rs-tls)](https://crates.io/crates/dubbo-rs-tls)
[![docs.rs](https://docs.rs/dubbo-rs-tls/badge.svg)](https://docs.rs/dubbo-rs-tls)
[![Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

TLS/mTLS support for dubbo-rs via rustls.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
dubbo-rs-tls = "0.1"
```

Or use `cargo add`:

```bash
cargo add dubbo-rs-tls
```

Provides server-side and client-side TLS configuration builders that produce `rustls::ServerConfig` and `rustls::ClientConfig`. Supports one-way TLS and mutual TLS (mTLS) with client certificate verification.

## Key Types

| Type | Description |
|------|-------------|
| `ServerTlsConfig` | Server TLS builder — cert chain + private key, optional client CA for mTLS |
| `ClientTlsConfig` | Client TLS builder — root CA, optional client cert/key for mTLS, SNI server name |

## Server TLS

```rust
use dubbo_rs_tls::ServerTlsConfig;

// One-way TLS
let server_config = ServerTlsConfig::new()
    .with_cert_chain("certs/server-cert.pem")
    .with_private_key("certs/server-key.pem")
    .build()?;

// Mutual TLS (mTLS) — verify client certificates
let server_config = ServerTlsConfig::new()
    .with_cert_chain("certs/server-cert.pem")
    .with_private_key("certs/server-key.pem")
    .with_client_ca("certs/client-ca.pem")
    .build()?;
```

## Client TLS

```rust
use dubbo_rs_tls::ClientTlsConfig;

// Connect with custom CA
let client_config = ClientTlsConfig::new()
    .with_root_ca("certs/ca.pem")
    .build()?;

// mTLS — present client certificate
let client_config = ClientTlsConfig::new()
    .with_root_ca("certs/ca.pem")
    .with_client_cert("certs/client-cert.pem")
    .with_client_key("certs/client-key.pem")
    .build()?;

// Without explicit CA — uses system root store (webpki roots)
let client_config = ClientTlsConfig::new().build()?;
```

## PEM Support

Supports PKCS#8 and PKCS#1 RSA private key formats via `rustls_pemfile`.

## License

Apache-2.0
