// JSON-RPC 2.0 protocol serialization and error mapping tests.

use dubbo_rs_protocol_jsonrpc::{JsonRpcRequest, JsonRpcResponse, JsonRpcErrorObj, error_code};

/// J-001: `JsonRpcRequest` serializes to expected JSON.
#[test]
fn j_001_request_serialize() {
    let req = JsonRpcRequest::new(
        "com.example.Service.sayHello".into(),
        vec![serde_json::json!("world")],
    );
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains(r#""jsonrpc":"2.0""#), "J-001: version");
    assert!(json.contains(r#""method":"com.example.Service.sayHello""#), "J-001: method");
    assert!(json.contains(r#""params":["world"]"#), "J-001: params");
    assert!(json.contains(r#""id":"#), "J-001: has id");
}

/// J-002: `JsonRpcRequest` with empty params.
#[test]
fn j_002_request_empty_params() {
    let req = JsonRpcRequest::new("ping".into(), vec![]);
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains(r#""params":[]"#), "J-002: empty params");
}

/// J-003: `JsonRpcResponse` deserialization with result.
#[test]
fn j_003_response_with_result() {
    let json = r#"{"jsonrpc":"2.0","result":"Hello, world!","id":1}"#;
    let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.jsonrpc, "2.0");
    assert_eq!(resp.result, Some(serde_json::json!("Hello, world!")));
    assert!(resp.error.is_none());
    assert_eq!(resp.id, 1);
}

/// J-004: `JsonRpcResponse` deserialization with error.
#[test]
fn j_004_response_with_error() {
    let json = r#"{"jsonrpc":"2.0","error":{"code":-32601,"message":"Method not found"},"id":1}"#;
    let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
    assert!(resp.result.is_none());
    let err = resp.error.unwrap();
    assert_eq!(err.code, -32601);
    assert_eq!(err.message, "Method not found");
}

/// J-005: `JsonRpcErrorObj` `to_rpc_error` maps `METHOD_NOT_FOUND` correctly.
#[test]
fn j_005_error_mapping_method_not_found() {
    let err = JsonRpcErrorObj::new(error_code::METHOD_NOT_FOUND, "not found");
    let rpc_err = err.to_rpc_error();
    assert_eq!(rpc_err.status_code(), 60, "J-005: METHOD_NOT_FOUND -> ServiceNotFound");
}

/// J-006: `JsonRpcErrorObj` `to_rpc_error` maps `INTERNAL_ERROR` to `ServiceError`.
#[test]
fn j_006_error_mapping_internal() {
    let err = JsonRpcErrorObj::new(error_code::INTERNAL_ERROR, "oops");
    let rpc_err = err.to_rpc_error();
    assert_eq!(rpc_err.status_code(), 70, "J-006: INTERNAL_ERROR -> ServiceError");
}

/// J-007: `JsonRpcErrorObj` `to_rpc_error` maps `INVALID_PARAMS`.
#[test]
fn j_007_error_mapping_invalid_params() {
    let err = JsonRpcErrorObj::new(error_code::INVALID_PARAMS, "bad input");
    let rpc_err = err.to_rpc_error();
    assert_eq!(rpc_err.status_code(), 40, "J-007: INVALID_PARAMS -> BadRequest");
}

/// J-008: `JsonRpcRequest` id auto-increments.
#[test]
fn j_008_request_id_auto_increment() {
    let req1 = JsonRpcRequest::new("m1".into(), vec![]);
    let req2 = JsonRpcRequest::new("m2".into(), vec![]);
    assert!(req2.id > req1.id, "J-008: ids should auto-increment");
}

/// J-009: Full roundtrip serialize then deserialize.
#[test]
fn j_009_request_roundtrip() {
    let req = JsonRpcRequest::new("test.method".into(), vec![serde_json::json!(42)]);
    let json = serde_json::to_string(&req).unwrap();
    let de: JsonRpcRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(de.method, "test.method");
    assert_eq!(de.params[0], serde_json::json!(42));
}
