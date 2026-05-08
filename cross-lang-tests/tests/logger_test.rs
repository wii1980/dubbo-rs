use dubbo_rs_logger::{LogLevel, LoggerBuilder, LoggerConfig, OutputFormat};
use dubbo_rs_serialization::Serialization;
use dubbo_rs_serialization_protobuf::ProtobufSerialization;

/// L-001: `LogLevel` `as_str` returns correct values.
#[test]
fn l_001_log_level_as_str() {
    assert_eq!(LogLevel::Trace.as_str(), "trace");
    assert_eq!(LogLevel::Debug.as_str(), "debug");
    assert_eq!(LogLevel::Info.as_str(), "info");
    assert_eq!(LogLevel::Warn.as_str(), "warn");
    assert_eq!(LogLevel::Error.as_str(), "error");
}

/// L-002: `LogLevel` from `LogLevel` to `tracing::Level`.
#[test]
fn l_002_log_level_to_tracing() {
    let level: tracing::Level = LogLevel::Warn.into();
    assert_eq!(level, tracing::Level::WARN);
}

/// L-003: `LoggerConfig` default values.
#[test]
fn l_003_logger_config_default() {
    let config = LoggerConfig::default();
    assert_eq!(config.log_level, LogLevel::Info);
}

/// L-004: `LoggerBuilder` constructs with custom log level.
#[test]
fn l_004_logger_builder_custom_level() {
    let builder = LoggerBuilder::new()
        .with_log_level(LogLevel::Debug)
        .with_output_format(OutputFormat::Json);
    assert_eq!(builder.config().log_level, LogLevel::Debug);
}

/// L-005: `ProtobufSerialization` content type.
#[test]
fn l_005_protobuf_content_type() {
    let proto = ProtobufSerialization::new();
    assert_eq!(proto.content_type(), "application/grpc+proto");
}

/// L-006: `ProtobufSerialization` serialize roundtrip (pass-through at byte level).
#[test]
fn l_006_protobuf_roundtrip() {
    let proto = ProtobufSerialization::new();
    let data = b"raw bytes";
    let serialized = proto.serialize(data).unwrap();
    let deserialized = proto.deserialize(&serialized).unwrap();
    assert_eq!(deserialized, data);
}
