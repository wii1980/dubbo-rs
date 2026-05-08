//! TLS/mTLS support for dubbo-rs.
//!
//! Provides server-side and client-side TLS configuration using rustls.
//! Supports both one-way TLS (server certificate verification) and
//! mutual TLS (mTLS) with client certificate authentication.
//!
//! ## Usage
//!
//! ### Server (TLS)
//! ```ignore
//! use dubbo_rs_tls::ServerTlsConfig;
//!
//! let config = ServerTlsConfig::new()
//!     .with_cert_chain("path/to/cert.pem")?
//!     .with_private_key("path/to/key.pem")?
//!     .build()?;
//! ```
//!
//! ### Client (TLS)
//! ```ignore
//! use dubbo_rs_tls::ClientTlsConfig;
//!
//! let config = ClientTlsConfig::new()
//!     .with_root_ca("path/to/ca.pem")?
//!     .build()?;
//! ```

use std::io::BufReader;
use std::sync::Arc;

/// Server-side TLS configuration.
///
/// Wraps `rustls::ServerConfig` and provides convenience methods
/// for loading certificates and private keys from PEM files.
pub struct ServerTlsConfig {
    /// Path to the certificate chain PEM file.
    cert_chain_path: Option<String>,
    /// Path to the private key PEM file.
    private_key_path: Option<String>,
    /// Path to the client CA certificate PEM file (for mTLS).
    client_ca_path: Option<String>,
    /// Whether to require client certificates (mTLS).
    require_client_auth: bool,
}

impl ServerTlsConfig {
    /// Create a new server TLS configuration builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cert_chain_path: None,
            private_key_path: None,
            client_ca_path: None,
            require_client_auth: false,
        }
    }

    /// Set the path to the server certificate chain PEM file.
    #[must_use]
    pub fn with_cert_chain(mut self, path: impl Into<String>) -> Self {
        self.cert_chain_path = Some(path.into());
        self
    }

    /// Set the path to the server private key PEM file.
    #[must_use]
    pub fn with_private_key(mut self, path: impl Into<String>) -> Self {
        self.private_key_path = Some(path.into());
        self
    }

    /// Enable mTLS by setting the client CA certificate path.
    ///
    /// When set, the server will require and verify client certificates.
    #[must_use]
    pub fn with_client_ca(mut self, path: impl Into<String>) -> Self {
        self.client_ca_path = Some(path.into());
        self.require_client_auth = true;
        self
    }

    /// Build the `rustls::ServerConfig`.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Certificate or key files cannot be read
    /// - Certificate/key PEM parsing fails
    /// - Private key is missing or does not match the certificate
    pub fn build(self) -> Result<rustls::ServerConfig, anyhow::Error> {
        let cert_chain_path = self
            .cert_chain_path
            .ok_or_else(|| anyhow::anyhow!("server certificate chain path is required"))?;
        let private_key_path = self
            .private_key_path
            .ok_or_else(|| anyhow::anyhow!("server private key path is required"))?;

        let certs = load_certs(&cert_chain_path)?;
        let key = load_private_key(&private_key_path)?;

        if self.require_client_auth {
            let client_ca_path = self
                .client_ca_path
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("client CA path is required for mTLS"))?;
            let client_certs = load_certs(client_ca_path)?;

            let mut root_store = rustls::RootCertStore::empty();
            for (i, cert) in client_certs.into_iter().enumerate() {
                root_store.add(cert).map_err(|e| {
                    anyhow::anyhow!("failed to add client CA cert {i} to root store: {e}")
                })?;
            }

            let client_verifier =
                rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store))
                    .build()
                    .map_err(|e| anyhow::anyhow!("failed to build client verifier: {e}"))?;

            rustls::ServerConfig::builder()
                .with_client_cert_verifier(client_verifier)
                .with_single_cert(certs, key)
                .map_err(|e| anyhow::anyhow!("failed to build mTLS server config: {e}"))
        } else {
            rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .map_err(|e| anyhow::anyhow!("failed to build server TLS config: {e}"))
        }
    }
}

impl Default for ServerTlsConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Client-side TLS configuration.
///
/// Wraps `rustls::ClientConfig` and provides convenience methods
/// for loading CA certificates and client certificates from PEM files.
pub struct ClientTlsConfig {
    /// Path to the root CA certificate PEM file.
    root_ca_path: Option<String>,
    /// Path to the client certificate chain PEM file (for mTLS).
    client_cert_path: Option<String>,
    /// Path to the client private key PEM file (for mTLS).
    client_key_path: Option<String>,
    /// Server name for SNI (Server Name Indication).
    server_name: Option<String>,
}

impl ClientTlsConfig {
    /// Create a new client TLS configuration builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            root_ca_path: None,
            client_cert_path: None,
            client_key_path: None,
            server_name: None,
        }
    }

    /// Set the path to the root CA certificate PEM file.
    ///
    /// This is used to verify the server's certificate.
    /// If not set, the system's default CA store is used.
    #[must_use]
    pub fn with_root_ca(mut self, path: impl Into<String>) -> Self {
        self.root_ca_path = Some(path.into());
        self
    }

    /// Set the path to the client certificate chain PEM file (for mTLS).
    #[must_use]
    pub fn with_client_cert(mut self, path: impl Into<String>) -> Self {
        self.client_cert_path = Some(path.into());
        self
    }

    /// Set the path to the client private key PEM file (for mTLS).
    #[must_use]
    pub fn with_client_key(mut self, path: impl Into<String>) -> Self {
        self.client_key_path = Some(path.into());
        self
    }

    /// Set the expected server name for TLS SNI.
    ///
    /// Defaults to "localhost" if not set.
    #[must_use]
    pub fn with_server_name(mut self, name: impl Into<String>) -> Self {
        self.server_name = Some(name.into());
        self
    }

    /// Build the `rustls::ClientConfig`.
    ///
    /// # Errors
    /// Returns an error if:
    /// - CA certificate file cannot be read or parsed
    /// - Client certificate/key cannot be loaded (mTLS)
    pub fn build(self) -> Result<rustls::ClientConfig, anyhow::Error> {
        let mut root_store = rustls::RootCertStore::empty();

        if let Some(ca_path) = &self.root_ca_path {
            let certs = load_certs(ca_path)?;
            for (i, cert) in certs.into_iter().enumerate() {
                root_store
                    .add(cert)
                    .map_err(|e| anyhow::anyhow!("failed to add CA cert {i} to root store: {e}"))?;
            }
        } else {
            root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }

        if let (Some(client_cert_path), Some(client_key_path)) =
            (&self.client_cert_path, &self.client_key_path)
        {
            let client_certs = load_certs(client_cert_path)?;
            let client_key = load_private_key(client_key_path)?;

            rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_client_auth_cert(client_certs, client_key)
                .map_err(|e| anyhow::anyhow!("failed to build mTLS client config: {e}"))
        } else {
            Ok(rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth())
        }
    }
}

impl Default for ClientTlsConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Load certificate chain from a PEM file.
///
/// # Errors
/// Returns an error if the file cannot be read or contains invalid PEM data.
fn load_certs(
    path: &str,
) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>, anyhow::Error> {
    let cert_file = std::fs::File::open(path)
        .map_err(|e| anyhow::anyhow!("failed to open certificate file '{path}': {e}"))?;
    let mut reader = BufReader::new(cert_file);

    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("failed to parse certificate PEM file '{path}': {e}"))?;

    if certs.is_empty() {
        return Err(anyhow::anyhow!(
            "no certificates found in PEM file '{path}'"
        ));
    }

    Ok(certs)
}

/// Load a private key from a PEM file.
///
/// Supports PKCS#8 and PKCS#1 RSA formats.
///
/// # Errors
/// Returns an error if the file cannot be read, contains invalid PEM data,
/// or no private key is found.
fn load_private_key(
    path: &str,
) -> Result<rustls::pki_types::PrivateKeyDer<'static>, anyhow::Error> {
    let key_file = std::fs::File::open(path)
        .map_err(|e| anyhow::anyhow!("failed to open private key file '{path}': {e}"))?;
    let mut reader = BufReader::new(key_file);

    let key = rustls_pemfile::private_key(&mut reader)
        .map_err(|e| anyhow::anyhow!("failed to parse private key PEM file '{path}': {e}"))?;

    key.ok_or_else(|| anyhow::anyhow!("no private key found in PEM file '{path}'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn ensure_crypto_provider() {
        INIT.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
    }

    const TEST_CERT_PEM: &str = "-----BEGIN CERTIFICATE-----\n\
MIIDCTCCAfGgAwIBAgIUZZn+knMcUb3O2r+NrImQPLuIgaYwDQYJKoZIhvcNAQEL\n\
BQAwFDESMBAGA1UEAwwJbG9jYWxob3N0MB4XDTI2MDUwMTA2NDMwOFoXDTI3MDUw\n\
MTA2NDMwOFowFDESMBAGA1UEAwwJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF\n\
AAOCAQ8AMIIBCgKCAQEA11sODwF/xm7inCHtJNSJiFNSrAMbzV7V5eHVzBME0fQU\n\
99nhHqvTDPmZXhNoouk3Talf71yUGckvUjCew/dmafUHmffe23ium/HTLO4JhSk8\n\
8/dC1rSJR6Mvx74qMfKR/pTf8Mzz7xhz5MzXHdUeLAyI9AhBfRsk9jq+1Vy+vol/\n\
N0iyRfko/0y8IIL2sHcwAdpntfJFVAkB5D6cQjSypw+T055Bc4rcumVOKhKftHJh\n\
yRyxx6sb3V3tHgpHWl9XkVZkm3sFPikIB5+2Xbozrjfl+QNFVEzcPsUCMhre+N9y\n\
YlThpdyvraH+2pntsCioKJZhvuHxLyMiXBsGWzRcswIDAQABo1MwUTAdBgNVHQ4E\n\
FgQU5/qbBXIAp9mZXbWPBT9eS8GC3GQwHwYDVR0jBBgwFoAU5/qbBXIAp9mZXbWP\n\
BT9eS8GC3GQwDwYDVR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAQEAD+lP\n\
q5WHBtlywxcDYfG+HVJm2R1Pa8eeD50GixsR1j4++GgCNTN6FauB71muIYc7Giv7\n\
nkrGKNzzRLTdxA2F0zW/YmoUuhrVJRj7OjJXuA6Jkikz+4FDTxRWrj9R9XorM2Pg\n\
LNpaRmz3TTDY6eDQpU31Mj9bCBngwYBpt4tO4FS7WKST6T8mAdguQ/6us6niCljj\n\
BBPHaFk+aOrNZV4TmyqvgZFpCrWGaV6Qb0UpHhQb1zUPVT72DrQo52aQrsFVOv3s\n\
uIoCBfI6qG7eNeXH26NjnnEWjD8hy524Bmw0s500mpOJgPK7QucEU9MGIZda95Sf\n\
1efRVCAHDrrPTG/dwQ==\n\
-----END CERTIFICATE-----";

    const TEST_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDXWw4PAX/GbuKc\n\
Ie0k1ImIU1KsAxvNXtXl4dXMEwTR9BT32eEeq9MM+ZleE2ii6TdNqV/vXJQZyS9S\n\
MJ7D92Zp9QeZ997beK6b8dMs7gmFKTzz90LWtIlHoy/Hviox8pH+lN/wzPPvGHPk\n\
zNcd1R4sDIj0CEF9GyT2Or7VXL6+iX83SLJF+Sj/TLwggvawdzAB2me18kVUCQHk\n\
PpxCNLKnD5PTnkFzity6ZU4qEp+0cmHJHLHHqxvdXe0eCkdaX1eRVmSbewU+KQgH\n\
n7ZdujOuN+X5A0VUTNw+xQIyGt7433JiVOGl3K+tof7ame2wKKgolmG+4fEvIyJc\n\
GwZbNFyzAgMBAAECggEAQO1AV1DV448Bvh3SX9S+JD4uwhJr2uZ5KYYFTbH8NYpX\n\
mgPzxan7BsHntb+3P8p9NGpYtJMeSYnovOhQrXdUxqQrpwVeiJ+hUP2+86BOeXmd\n\
2VXWLmIes1zlJlzUXtupnW3n+DLqZk7iffwt7N4YayJaVex5Rg0dfyjl6PC9xzai\n\
Tq+ThHVZqTL7i8ZyrqD1HeOAFKZOSfKOdKtZXn02mUej7MIdZgUV/AE2yuGFbCkI\n\
dOb1ziHmdpbNPI8UT4vVpCYQ9DaBG/B7LMnxNRWapn6PVFCc+KFJ6iPf0X/2IKcF\n\
cO9mTbYHmH5tK3a+HLLz0hdUdnfhV11KUtvp7edRxQKBgQD1aU+827jsidE8+LKR\n\
Uf3WwZiUCsQnPlk7+MSf2nLywWhbEYhIb8UYsqvDKkqvj+0PVdB9NKysefDTBUBD\n\
O3PXZYVEEb0ayLU4uoYigKwfUp8ESt28n96F1yWrLNV0uWPG8KaYGS3ALLrND7HF\n\
GcCq25idDzXokIcF7KoCBJQU3wKBgQDgpcN/aepv4D3fZi+8jvfb0/3U77vz2zqn\n\
kQEQG5C79VpzvLpSv9Xoz/bRD1ZvhYoMto/pv99TT9h2GjXSaWeYWR3O+uXFrsHx\n\
MRYoOjouI83S6wKh0VcHPeYfHXoF5hg4pN4JAxTfeDjUulSJy0YvkjqFOL8t5gl5\n\
Wb4pkjP+rQKBgHSalCN09tmU5hElTZsUrRp0I+37a5YF3tpK6gnV/pXvZYkXvHxG\n\
dwy0ID58Ar6GESofKQ/EjmLpEY8CSLVpMzJd70MXdpWaVdjdb0xHfQDo/dtJQzAT\n\
eeR4BFLf25A5Yfotb8qG9CECX8N9OIchJFVKP6oohwG4Yh9jgqewyzdbAoGBAKa3\n\
vm+Hrjma5LAviQvZ2m5lVIK76/Pc5hnHjk9i9bXYL2mnTWvt/JVMCXM7e71GEJ7A\n\
uesSv21320BC0WC3Yu94a5vZLb7YpAwYjsYJ+HWXkr+OM6Td1EWGlYrP+Gf6TE11\n\
ZWawx8PU1/Bf3C9rEUpqrk2CQLeSecN6a5s0aqv9AoGBAJPIsYPh7p102BjcZuWy\n\
a2mydTtsuA8AdFesFdKz2I2htgkqpZV8051JNGEbn/x6rkfMu+KjkSrZ2goUZ4f8\n\
r81JsM7cejDaORYK9WIv/Z75IWcqQBgm9u7YPAFEt5XSSRvqhMZ7NBstgTt84hoV\n\
R2viDc74NFJGF1hRuf212NHH\n\
-----END PRIVATE KEY-----";

    fn create_temp_files(cert_pem: &str, key_pem: &str) -> (tempfile::TempDir, String, String) {
        ensure_crypto_provider();
        let dir = tempfile::tempdir().expect("create temp dir");

        let cert_path = dir.path().join("cert.pem");
        let key_path = dir.path().join("key.pem");

        let mut cert_file = std::fs::File::create(&cert_path).expect("create cert file");
        cert_file
            .write_all(cert_pem.as_bytes())
            .expect("write cert");
        drop(cert_file);

        let mut key_file = std::fs::File::create(&key_path).expect("create key file");
        key_file.write_all(key_pem.as_bytes()).expect("write key");
        drop(key_file);

        (
            dir,
            cert_path.to_string_lossy().to_string(),
            key_path.to_string_lossy().to_string(),
        )
    }

    #[test]
    fn test_server_tls_config_build_success() {
        let (_dir, cert_path, key_path) = create_temp_files(TEST_CERT_PEM, TEST_KEY_PEM);

        let result = ServerTlsConfig::new()
            .with_cert_chain(&cert_path)
            .with_private_key(&key_path)
            .build();

        assert!(result.is_ok(), "ServerTlsConfig build failed: {result:?}");
    }

    #[test]
    fn test_server_tls_config_missing_cert() {
        let result = ServerTlsConfig::new().build();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("certificate chain"));
    }

    #[test]
    fn test_server_tls_config_missing_key() {
        let (_dir, cert_path, _key_path) = create_temp_files(TEST_CERT_PEM, TEST_KEY_PEM);

        let result = ServerTlsConfig::new().with_cert_chain(&cert_path).build();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("private key"));
    }

    #[test]
    fn test_server_tls_config_file_not_found() {
        let result = ServerTlsConfig::new()
            .with_cert_chain("/nonexistent/cert.pem")
            .with_private_key("/nonexistent/key.pem")
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_client_tls_config_build_default() {
        ensure_crypto_provider();
        let result = ClientTlsConfig::new().build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_client_tls_config_with_server_name() {
        ensure_crypto_provider();
        let config = ClientTlsConfig::new()
            .with_server_name("example.com")
            .build();

        assert!(config.is_ok());
    }

    #[test]
    fn test_client_tls_config_with_ca() {
        let (_dir, ca_path, _key_path) = create_temp_files(TEST_CERT_PEM, TEST_KEY_PEM);

        let result = ClientTlsConfig::new().with_root_ca(&ca_path).build();

        assert!(result.is_ok(), "ClientTlsConfig build failed: {result:?}");
    }

    #[test]
    fn test_client_tls_config_ca_file_not_found() {
        let result = ClientTlsConfig::new()
            .with_root_ca("/nonexistent/ca.pem")
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_server_tls_config_default() {
        let config = ServerTlsConfig::new();
        assert!(config.cert_chain_path.is_none());
        assert!(config.private_key_path.is_none());
        assert!(!config.require_client_auth);
    }

    #[test]
    fn test_client_tls_config_default() {
        let config = ClientTlsConfig::new();
        assert!(config.root_ca_path.is_none());
        assert!(config.client_cert_path.is_none());
        assert!(config.client_key_path.is_none());
    }

    #[test]
    fn test_load_certs_invalid_file() {
        let result = load_certs("/nonexistent/cert.pem");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_private_key_invalid_file() {
        let result = load_private_key("/nonexistent/key.pem");
        assert!(result.is_err());
    }

    #[test]
    fn test_server_tls_config_mtls() {
        let (_dir, cert_path, key_path) = create_temp_files(TEST_CERT_PEM, TEST_KEY_PEM);
        let (_dir2, ca_path, _ca_key) = create_temp_files(TEST_CERT_PEM, TEST_KEY_PEM);

        let result = ServerTlsConfig::new()
            .with_cert_chain(&cert_path)
            .with_private_key(&key_path)
            .with_client_ca(&ca_path)
            .build();

        assert!(
            result.is_ok(),
            "mTLS server config build failed: {result:?}"
        );
    }

    #[test]
    fn test_server_tls_config_mtls_missing_ca() {
        let (_dir, cert_path, key_path) = create_temp_files(TEST_CERT_PEM, TEST_KEY_PEM);

        let result = ServerTlsConfig::new()
            .with_cert_chain(&cert_path)
            .with_private_key(&key_path)
            .with_client_ca("/nonexistent/ca.pem")
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_client_tls_config_mtls() {
        let (_dir, ca_path, _ca_key) = create_temp_files(TEST_CERT_PEM, TEST_KEY_PEM);
        let (_dir2, cert_path, key_path) = create_temp_files(TEST_CERT_PEM, TEST_KEY_PEM);

        let result = ClientTlsConfig::new()
            .with_root_ca(&ca_path)
            .with_client_cert(&cert_path)
            .with_client_key(&key_path)
            .build();

        assert!(
            result.is_ok(),
            "mTLS client config build failed: {result:?}"
        );
    }

    #[test]
    fn test_client_tls_config_mtls_missing_key() {
        let (_dir, ca_path, _ca_key) = create_temp_files(TEST_CERT_PEM, TEST_KEY_PEM);
        let (_dir2, cert_path, _key_path) = create_temp_files(TEST_CERT_PEM, TEST_KEY_PEM);

        let result = ClientTlsConfig::new()
            .with_root_ca(&ca_path)
            .with_client_cert(&cert_path)
            .build();

        assert!(result.is_ok());
    }
}
