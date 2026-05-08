use dubbo_rs_metadata::{
    DefaultMetadataService, InMemoryMetadataStorage, MetadataInfo, MetadataService,
    MetadataStorage, MethodDefinition, ServiceDefinition, StreamType,
};
use std::sync::Arc;

/// M-001: `MethodDefinition` with builder pattern.
#[test]
fn m_001_method_definition() {
    let method = MethodDefinition::new("sayHello", "Ljava/lang/String;")
        .with_param("Ljava/lang/String;")
        .with_stream_type(StreamType::Unary);
    assert_eq!(method.name, "sayHello");
    assert_eq!(method.return_type, "Ljava/lang/String;");
    assert_eq!(method.parameter_types.len(), 1);
    assert!(!method.is_streaming());
}

/// M-002: `MethodDefinition` with streaming type.
#[test]
fn m_002_method_streaming() {
    let method =
        MethodDefinition::new("streamData", "V").with_stream_type(StreamType::ServerStreaming);
    assert!(method.is_streaming());
    assert_eq!(method.stream_type, StreamType::ServerStreaming);
}

/// M-003: `ServiceDefinition` builder.
#[test]
fn m_003_service_definition() {
    let svc = ServiceDefinition::new("com.example.Greeter")
        .with_version("2.0.0")
        .with_group("test")
        .with_method(
            MethodDefinition::new("sayHello", "Ljava/lang/String;")
                .with_param("Ljava/lang/String;"),
        );
    assert_eq!(svc.interface, "com.example.Greeter");
    assert_eq!(svc.version, "2.0.0");
    assert_eq!(svc.methods.len(), 1);
}

/// M-004: `MetadataInfo` construction.
#[test]
fn m_004_metadata_info() {
    let info = MetadataInfo::new("demo-provider")
        .with_revision(1)
        .with_service(
            ServiceDefinition::new("com.example.Greeter")
                .with_method(MethodDefinition::new("sayHello", "Ljava/lang/String;")),
        );
    assert_eq!(info.application, "demo-provider");
    assert_eq!(info.revision, 1);
    assert_eq!(info.services.len(), 1);
}

/// M-005: `MetadataInfo` JSON serialization.
#[test]
fn m_005_metadata_json() {
    let info = MetadataInfo::new("json-app")
        .with_revision(2)
        .with_service(
            ServiceDefinition::new("com.example.JsonService")
                .with_method(MethodDefinition::new("echo", "Ljava/lang/String;")),
        )
        .with_attr("region", "bj");
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("json-app"), "M-005: should contain app name");
    assert!(
        json.contains("com.example.JsonService"),
        "M-005: should contain service"
    );
    assert!(
        json.contains("region"),
        "M-005: should contain attribute key"
    );
}

/// M-006: InMemoryMetadataStorage store and get.
#[tokio::test]
async fn m_006_storage_store_and_get() {
    let storage = Arc::new(InMemoryMetadataStorage::new());
    let info = MetadataInfo::new("store-app")
        .with_revision(1)
        .with_service(
            ServiceDefinition::new("com.example.StoreService")
                .with_method(MethodDefinition::new("get", "Ljava/lang/String;")),
        );
    storage.store(info.clone());

    let retrieved = DefaultMetadataService::new(storage)
        .get_metadata_info("store-app".into())
        .await;
    assert!(
        retrieved.is_some(),
        "M-006: should retrieve stored metadata"
    );
    let info = retrieved.unwrap();
    assert_eq!(info.application, "store-app");
    assert_eq!(info.services.len(), 1);
}

/// M-007: InMemoryMetadataStorage remove.
#[tokio::test]
async fn m_007_storage_remove() {
    let storage = Arc::new(InMemoryMetadataStorage::new());
    let info = MetadataInfo::new("temp-app").with_revision(1);
    storage.store(info);

    let meta = DefaultMetadataService::new(storage.clone())
        .get_metadata_info("temp-app".into())
        .await;
    assert!(meta.is_some(), "M-007: should exist before remove");

    storage.remove("temp-app");

    let meta = DefaultMetadataService::new(storage)
        .get_metadata_info("temp-app".into())
        .await;
    assert!(meta.is_none(), "M-007: should be gone after remove");
}

/// M-008: `StreamType` defaults to Unary.
#[test]
fn m_008_stream_type_default() {
    let t = StreamType::default();
    assert_eq!(t, StreamType::Unary);
}

/// M-009: `ServiceDefinition` with params.
#[test]
fn m_009_service_with_params() {
    let svc = ServiceDefinition::new("com.example.ParamService")
        .with_param("timeout", "5000")
        .with_param("retries", "3");
    assert_eq!(svc.params.len(), 2);
    assert!(svc.params.contains(&("timeout".into(), "5000".into())));
}
