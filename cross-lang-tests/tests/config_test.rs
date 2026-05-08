// Configuration loading tests for dubbo-rs-config.
//
// Tests RootConfig construction, builder pattern, and YAML deserialization.

use dubbo_rs_config::{ProtocolConfig, RegistryConfig, RootConfig};

/// C-001: `RootConfig` default values.
#[test]
fn c_001_config_default() {
    let config = RootConfig::default();
    assert_eq!(config.application, "");
    assert!(config.protocols.is_empty());
    assert!(config.registries.is_empty());
}

/// C-002: `RootConfig` builder with application and version.
#[test]
fn c_002_config_builder_basic() {
    let config = RootConfig::default()
        .with_application("my-app")
        .with_version("2.0.0");
    assert_eq!(config.application, "my-app");
    assert_eq!(config.version, "2.0.0");
}

/// C-003: `RootConfig` builder with protocol.
#[test]
fn c_003_config_builder_with_protocol() {
    let config = RootConfig::default()
        .with_application("test")
        .with_protocol(ProtocolConfig::new("tri", "0.0.0.0", 50051));
    assert_eq!(config.protocols.len(), 1);
    assert_eq!(config.protocols[0].name, "tri");
    assert_eq!(config.protocols[0].host, "0.0.0.0");
    assert_eq!(config.protocols[0].port, 50051);
}

/// C-004: `RootConfig` builder with registry.
#[test]
fn c_004_config_builder_with_registry() {
    let config = RootConfig::default()
        .with_application("test")
        .with_registry(RegistryConfig {
            protocol: "zookeeper".into(),
            address: "127.0.0.1:2181".into(),
        });
    assert_eq!(config.registries.len(), 1);
    assert_eq!(config.registries[0].protocol, "zookeeper");
    assert_eq!(config.registries[0].address, "127.0.0.1:2181");
}

/// C-005: `RootConfig` YAML deserialization.
#[test]
fn c_005_config_yaml_deserialize() {
    let yaml = r#"
application: "my-provider"
version: "1.0.0"
protocols:
  - name: "tri"
    host: "0.0.0.0"
    port: 50051
registries:
  - protocol: "zookeeper"
    address: "127.0.0.1:2181"
"#;
    let config: RootConfig = serde_yaml::from_str(yaml).expect("C-005: valid YAML");
    assert_eq!(config.application, "my-provider");
    assert_eq!(config.protocols.len(), 1);
    assert_eq!(config.protocols[0].port, 50051);
    assert_eq!(config.registries[0].address, "127.0.0.1:2181");
}

/// C-006: `RootConfig` YAML with multiple protocols.
#[test]
fn c_006_config_yaml_multi_protocol() {
    let yaml = r#"
application: "multi"
protocols:
  - name: "tri"
    host: "0.0.0.0"
    port: 50051
  - name: "dubbo"
    host: "0.0.0.0"
    port: 20880
"#;
    let config: RootConfig = serde_yaml::from_str(yaml).expect("C-006: multi-protocol YAML");
    assert_eq!(config.protocols.len(), 2);
    assert_eq!(config.protocols[1].name, "dubbo");
    assert_eq!(config.protocols[1].port, 20880);
}

/// C-007: `RootConfig` YAML with TLS config.
#[test]
fn c_007_config_yaml_with_tls() {
    let yaml = r#"
application: "secure-app"
protocols:
  - name: "tri"
    host: "0.0.0.0"
    port: 50051
tls:
  server_cert_chain: "/certs/server.pem"
  server_private_key: "/certs/server.key"
"#;
    let config: RootConfig = serde_yaml::from_str(yaml).expect("C-007: TLS YAML");
    let tls = config.tls.expect("C-007: should have tls config");
    assert_eq!(tls.server_cert_chain, Some("/certs/server.pem".into()));
}

/// C-008: `ProtocolConfig` default values.
#[test]
fn c_008_protocol_config_default() {
    let config = ProtocolConfig::default();
    assert_eq!(config.name, "tri");
    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.port, 50051);
}

/// C-009: `RootConfig` chained builder (full example).
#[test]
fn c_009_config_full_builder() {
    let config = RootConfig::default()
        .with_application("full-app")
        .with_version("3.0.0")
        .with_protocol(ProtocolConfig::new("tri", "0.0.0.0", 50051))
        .with_protocol(ProtocolConfig::new("dubbo", "0.0.0.0", 20880))
        .with_registry(RegistryConfig {
            protocol: "nacos".into(),
            address: "127.0.0.1:8848".into(),
        });
    assert_eq!(config.application, "full-app");
    assert_eq!(config.protocols.len(), 2);
    assert_eq!(config.registries.len(), 1);
    assert_eq!(config.registries[0].protocol, "nacos");
}
