// Phase 7: TLS/mTLS integration tests.
//
// Tests the dubbo-rs-tls crate: cert loading, config building, TLS connection.

#![allow(clippy::semicolon_if_nothing_returned)]

use dubbo_rs_tls::{ClientTlsConfig, ServerTlsConfig};
use std::path::PathBuf;
use std::sync::OnceLock;

fn init_tls() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        rustls::crypto::ring::default_provider()
            .install_default()
            .expect("ring crypto provider install");
    });
}

fn fixture_dir() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.join("../tests/fixtures/tls")
}

/// T-001: `ServerTlsConfig` builds successfully with valid cert chain and key.
#[test]
fn t_001_server_tls_config_build() {
    init_tls();
    let config = ServerTlsConfig::new()
        .with_cert_chain(
            fixture_dir()
                .join("server.pem")
                .to_string_lossy()
                .to_string(),
        )
        .with_private_key(
            fixture_dir()
                .join("server.key")
                .to_string_lossy()
                .to_string(),
        )
        .build()
        .expect("T-001: should build ServerTlsConfig");
    assert!(
        config.alpn_protocols.is_empty(),
        "T-001: should have no ALPN configured"
    );
}

/// T-002: `ServerTlsConfig` with mTLS (client CA) builds successfully.
#[test]
fn t_002_server_mtls_config_build() {
    init_tls();
    let config = ServerTlsConfig::new()
        .with_cert_chain(
            fixture_dir()
                .join("server.pem")
                .to_string_lossy()
                .to_string(),
        )
        .with_private_key(
            fixture_dir()
                .join("server.key")
                .to_string_lossy()
                .to_string(),
        )
        .with_client_ca(fixture_dir().join("ca.pem").to_string_lossy().to_string())
        .build()
        .expect("T-002: should build mTLS ServerTlsConfig");
    let _ = config;
}

/// T-003: `ServerTlsConfig` fails with missing cert file.
#[test]
fn t_003_server_tls_config_missing_cert() {
    init_tls();
    let result = ServerTlsConfig::new()
        .with_cert_chain("/nonexistent/cert.pem")
        .with_private_key(
            fixture_dir()
                .join("server.key")
                .to_string_lossy()
                .to_string(),
        )
        .build();
    assert!(result.is_err(), "T-003: should fail with missing cert");
}

/// T-004: `ClientTlsConfig` builds successfully with root CA.
#[test]
fn t_004_client_tls_config_build() {
    init_tls();
    let config = ClientTlsConfig::new()
        .with_root_ca(fixture_dir().join("ca.pem").to_string_lossy().to_string())
        .build()
        .expect("T-004: should build ClientTlsConfig");
    let _ = config;
}

/// T-005: `ClientTlsConfig` with mTLS client cert builds successfully.
#[test]
fn t_005_client_mtls_config_build() {
    init_tls();
    let config = ClientTlsConfig::new()
        .with_root_ca(fixture_dir().join("ca.pem").to_string_lossy().to_string())
        .with_client_cert(
            fixture_dir()
                .join("client.pem")
                .to_string_lossy()
                .to_string(),
        )
        .with_client_key(
            fixture_dir()
                .join("client.key")
                .to_string_lossy()
                .to_string(),
        )
        .build()
        .expect("T-005: should build mTLS ClientTlsConfig");
    let _ = config;
}

/// T-006: `ClientTlsConfig` builds without root CA (uses system store).
#[test]
fn t_006_client_tls_config_no_ca() {
    init_tls();
    let config = ClientTlsConfig::new()
        .build()
        .expect("T-006: should build without root CA");
    let _ = config;
}

/// T-007: `ClientTlsConfig` with custom server name.
#[test]
fn t_007_client_tls_config_with_server_name() {
    init_tls();
    let config = ClientTlsConfig::new()
        .with_root_ca(fixture_dir().join("ca.pem").to_string_lossy().to_string())
        .with_server_name("localhost")
        .build()
        .expect("T-007: should build with server name");
    let _ = config;
}
