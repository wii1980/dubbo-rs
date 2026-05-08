pub use dubbo_rs_common;
pub use dubbo_rs_tls;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
}

impl ProtocolConfig {
    #[must_use]
    pub fn new(name: impl Into<String>, host: impl Into<String>, port: u16) -> Self {
        Self {
            name: name.into(),
            host: host.into(),
            port,
        }
    }
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            name: "tri".into(),
            host: "0.0.0.0".into(),
            port: 50051,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    pub protocol: String,
    pub address: String,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            protocol: "zookeeper".into(),
            address: "127.0.0.1:2181".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RootConfig {
    #[serde(default)]
    pub application: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub protocols: Vec<ProtocolConfig>,
    #[serde(default)]
    pub registries: Vec<RegistryConfig>,
    #[serde(default)]
    pub tls: Option<TlsConfig>,
}

/// TLS/mTLS configuration for Dubbo services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to server certificate chain PEM file.
    #[serde(default)]
    pub server_cert_chain: Option<String>,
    /// Path to server private key PEM file.
    #[serde(default)]
    pub server_private_key: Option<String>,
    /// Path to client CA certificate PEM file (for mTLS).
    #[serde(default)]
    pub client_ca: Option<String>,
    /// Path to client certificate PEM file (for mTLS client auth).
    #[serde(default)]
    pub client_cert: Option<String>,
    /// Path to client private key PEM file (for mTLS client auth).
    #[serde(default)]
    pub client_key: Option<String>,
    /// Path to root CA certificate PEM file (for client verification).
    #[serde(default)]
    pub root_ca: Option<String>,
    /// Enable mTLS (mutual TLS). When true, both server and client authenticate each other.
    #[serde(default)]
    pub enable_mtls: bool,
}

impl TlsConfig {
    /// Convert to a `dubbo_rs_tls::ServerTlsConfig` builder.
    #[must_use]
    pub fn to_server_config(&self) -> dubbo_rs_tls::ServerTlsConfig {
        let mut config = dubbo_rs_tls::ServerTlsConfig::new();
        if let Some(ref path) = self.server_cert_chain {
            config = config.with_cert_chain(path);
        }
        if let Some(ref path) = self.server_private_key {
            config = config.with_private_key(path);
        }
        if self.enable_mtls {
            if let Some(ref path) = self.client_ca {
                config = config.with_client_ca(path);
            }
        }
        config
    }

    /// Convert to a `dubbo_rs_tls::ClientTlsConfig` builder.
    #[must_use]
    pub fn to_client_config(&self) -> dubbo_rs_tls::ClientTlsConfig {
        let mut config = dubbo_rs_tls::ClientTlsConfig::new();
        if let Some(ref path) = self.root_ca {
            config = config.with_root_ca(path);
        }
        if self.enable_mtls {
            if let Some(ref path) = self.client_cert {
                config = config.with_client_cert(path);
            }
            if let Some(ref path) = self.client_key {
                config = config.with_client_key(path);
            }
        }
        config
    }
}

fn default_version() -> String {
    "1.0.0".into()
}

impl RootConfig {
    #[must_use]
    pub fn with_application(mut self, name: impl Into<String>) -> Self {
        self.application = name.into();
        self
    }

    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    #[must_use]
    pub fn with_protocol(mut self, protocol: ProtocolConfig) -> Self {
        self.protocols.push(protocol);
        self
    }

    #[must_use]
    pub fn with_registry(mut self, registry: RegistryConfig) -> Self {
        self.registries.push(registry);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_config_default() {
        let config = ProtocolConfig::default();
        assert_eq!(config.name, "tri");
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 50051u16);
    }

    #[test]
    fn test_protocol_config_custom() {
        let config = ProtocolConfig::new("dubbo", "192.168.1.1", 20880);
        assert_eq!(config.name, "dubbo");
        assert_eq!(config.host, "192.168.1.1");
        assert_eq!(config.port, 20880u16);
    }

    #[test]
    fn test_registry_config_default() {
        let config = RegistryConfig::default();
        assert_eq!(config.protocol, "zookeeper");
        assert_eq!(config.address, "127.0.0.1:2181");
    }

    #[test]
    fn test_root_config_builder() {
        let root = RootConfig::default()
            .with_application("demo-app")
            .with_version("1.0.0");

        assert_eq!(root.application, "demo-app");
        assert_eq!(root.version, "1.0.0");
    }

    #[test]
    fn test_root_config_with_protocols() {
        let triple = ProtocolConfig::new("tri", "0.0.0.0", 50051);
        let root = RootConfig::default().with_protocol(triple);
        assert_eq!(root.protocols.len(), 1);
        assert_eq!(root.protocols[0].name, "tri");
    }

    #[test]
    fn test_yaml_parsing() {
        let yaml = r#"
application: "demo-provider"
version: "1.0.0"
protocols:
  - name: "tri"
    host: "0.0.0.0"
    port: 50051
registries:
  - protocol: "zookeeper"
    address: "127.0.0.1:2181"
"#;
        let config: RootConfig = serde_yaml::from_str(yaml).expect("parse yaml");
        assert_eq!(config.application, "demo-provider");
        assert_eq!(config.version, "1.0.0");
        assert_eq!(config.protocols.len(), 1);
        assert_eq!(config.protocols[0].name, "tri");
        assert_eq!(config.protocols[0].port, 50051);
        assert_eq!(config.registries.len(), 1);
        assert_eq!(config.registries[0].protocol, "zookeeper");
    }

    #[test]
    fn test_registry_config_custom() {
        let config = RegistryConfig {
            protocol: "nacos".into(),
            address: "127.0.0.1:8848".into(),
        };
        assert_eq!(config.protocol, "nacos");
        assert_eq!(config.address, "127.0.0.1:8848");
    }

    #[test]
    fn test_root_config_with_registry() {
        let registry = RegistryConfig {
            protocol: "zookeeper".into(),
            address: "127.0.0.1:2181".into(),
        };
        let root = RootConfig::default().with_registry(registry);
        assert_eq!(root.registries.len(), 1);
        assert_eq!(root.registries[0].protocol, "zookeeper");
        assert_eq!(root.registries[0].address, "127.0.0.1:2181");
    }

    #[test]
    fn test_root_config_with_version() {
        let root = RootConfig::default().with_version("2.0.0");
        assert_eq!(root.version, "2.0.0");
    }

    #[test]
    fn test_tls_config_default() {
        let config = TlsConfig {
            server_cert_chain: None,
            server_private_key: None,
            client_ca: None,
            client_cert: None,
            client_key: None,
            root_ca: None,
            enable_mtls: false,
        };
        assert!(config.server_cert_chain.is_none());
        assert!(config.server_private_key.is_none());
        assert!(config.client_ca.is_none());
        assert!(config.client_cert.is_none());
        assert!(config.client_key.is_none());
        assert!(config.root_ca.is_none());
        assert!(!config.enable_mtls);
    }

    #[test]
    fn test_tls_config_fields() {
        let config = TlsConfig {
            server_cert_chain: Some("/certs/server.pem".into()),
            server_private_key: Some("/certs/server.key".into()),
            client_ca: Some("/certs/ca.pem".into()),
            client_cert: None,
            client_key: None,
            root_ca: Some("/certs/root-ca.pem".into()),
            enable_mtls: true,
        };
        assert_eq!(
            config.server_cert_chain.as_deref(),
            Some("/certs/server.pem")
        );
        assert_eq!(
            config.server_private_key.as_deref(),
            Some("/certs/server.key")
        );
        assert_eq!(config.client_ca.as_deref(), Some("/certs/ca.pem"));
        assert!(config.client_cert.is_none());
        assert!(config.client_key.is_none());
        assert_eq!(config.root_ca.as_deref(), Some("/certs/root-ca.pem"));
        assert!(config.enable_mtls);
    }

    #[test]
    fn test_yaml_roundtrip() {
        let original = RootConfig::default()
            .with_application("roundtrip-app")
            .with_version("3.0.0")
            .with_protocol(ProtocolConfig::new("tri", "0.0.0.0", 50051))
            .with_registry(RegistryConfig {
                protocol: "nacos".into(),
                address: "127.0.0.1:8848".into(),
            });

        let yaml = serde_yaml::to_string(&original).expect("serialize to yaml");
        let parsed: RootConfig = serde_yaml::from_str(&yaml).expect("deserialize from yaml");

        assert_eq!(parsed.application, "roundtrip-app");
        assert_eq!(parsed.version, "3.0.0");
        assert_eq!(parsed.protocols.len(), 1);
        assert_eq!(parsed.protocols[0].name, "tri");
        assert_eq!(parsed.registries.len(), 1);
        assert_eq!(parsed.registries[0].protocol, "nacos");
        assert_eq!(parsed.registries[0].address, "127.0.0.1:8848");
    }

    #[test]
    fn test_yaml_parse_minimal() {
        let yaml = "application: test";
        let config: RootConfig = serde_yaml::from_str(yaml).expect("parse yaml");
        assert_eq!(config.application, "test");
        assert_eq!(config.version, "1.0.0");
        assert!(config.protocols.is_empty());
        assert!(config.registries.is_empty());
        assert!(config.tls.is_none());
    }

    #[test]
    fn test_yaml_parse_empty() {
        let yaml = "{}";
        let config: RootConfig = serde_yaml::from_str(yaml).expect("parse yaml");
        assert!(config.application.is_empty());
        assert_eq!(config.version, "1.0.0");
        assert!(config.protocols.is_empty());
        assert!(config.registries.is_empty());
        assert!(config.tls.is_none());
    }
}
