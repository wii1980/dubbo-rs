use dubbo_rs_serialization::Serialization;
use dubbo_rs_serialization_json::JsonSerialization;

/// S-001: `JsonSerialization` serialize/deserialize roundtrip.
#[test]
fn s_001_json_roundtrip() {
    let json = JsonSerialization::new();
    let input = b"{\"name\":\"hello\"}";
    let serialized = json.serialize(input).unwrap();
    let deserialized = json.deserialize(&serialized).unwrap();
    assert_eq!(deserialized, input, "S-001: roundtrip should preserve JSON");
}

/// S-002: `JsonSerialization` handles array.
#[test]
fn s_002_json_array() {
    let json = JsonSerialization::new();
    let input = b"[1,2,3]";
    let data = json.serialize(input).unwrap();
    let output = json.deserialize(&data).unwrap();
    assert_eq!(output, input, "S-002: JSON array roundtrip");
}

/// S-003: `JsonSerialization` handles empty object.
#[test]
fn s_003_json_empty_object() {
    let json = JsonSerialization::new();
    let data = json.serialize(b"{}").unwrap();
    let output = json.deserialize(&data).unwrap();
    assert_eq!(output, b"{}", "S-003: empty object roundtrip");
}

/// S-004: `JsonSerialization` content type.
#[test]
fn s_004_json_content_type() {
    let json = JsonSerialization::new();
    assert_eq!(json.content_type(), "application/json");
}

/// S-005: `JsonSerialization` serialize string.
#[test]
fn s_005_json_string() {
    let json = JsonSerialization::new();
    let result = json.serialize(b"\"hello world\"").unwrap();
    let output = json.deserialize(&result).unwrap();
    assert_eq!(output, b"\"hello world\"");
}
