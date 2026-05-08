//! JSON-RPC 2.0 protocol implementation for Dubbo.
//!
//! Implements JSON-RPC 2.0 over HTTP as a Dubbo protocol,
//! conforming to the [JSON-RPC 2.0 specification](https://www.jsonrpc.org/specification).
//!
//! ## Usage
//!
//! ```rust,ignore
//! use dubbo_rs_protocol_jsonrpc::JsonRpcProtocol;
//! use dubbo_rs_protocol::Protocol;
//! use dubbo_rs_common::url::URL;
//!
//! let protocol = JsonRpcProtocol::new();
//! let url = URL::new("jsonrpc", "/com.example.GreetService");
//! url.ip = "127.0.0.1".to_string();
//! url.port = "8080".to_string();
//! let invoker = protocol.refer(&url).await?;
//! ```

pub use dubbo_rs_common;
pub use dubbo_rs_protocol;

use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use async_trait::async_trait;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::{Exporter, InvocationContext, Invoker, Protocol, RPCResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// JSON-RPC 2.0 request object.
///
/// ```json
/// {
///   "jsonrpc": "2.0",
///   "method": "com.example.GreetService.sayHello",
///   "params": ["world"],
///   "id": 1
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Vec<serde_json::Value>,
    pub id: u64,
}

impl JsonRpcRequest {
    /// Creates a new JSON-RPC request with an auto-incremented id.
    #[must_use]
    pub fn new(method: String, params: Vec<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method,
            params,
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        }
    }
}

/// JSON-RPC 2.0 response object.
///
/// A valid response has either `result` or `error`, but not both.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcErrorObj>,
    pub id: u64,
}

/// JSON-RPC 2.0 error object within a response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorObj {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 standard error codes.
pub mod error_code {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;

    /// Server error range start (implementation-defined).
    pub const SERVER_ERROR_START: i64 = -32000;
    /// Server error range end (implementation-defined).
    pub const SERVER_ERROR_END: i64 = -32099;
}

impl JsonRpcErrorObj {
    /// Creates a new JSON-RPC error object.
    #[must_use]
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Maps this JSON-RPC error to a `RPCError` for Dubbo's invocation result.
    #[must_use]
    pub fn to_rpc_error(&self) -> RPCError {
        match self.code {
            error_code::METHOD_NOT_FOUND => RPCError::ServiceNotFound(self.message.clone()),
            error_code::INVALID_PARAMS | error_code::INVALID_REQUEST | error_code::PARSE_ERROR => {
                RPCError::BadRequest(self.message.clone())
            }
            code if (error_code::SERVER_ERROR_END..=error_code::SERVER_ERROR_START)
                .contains(&code) =>
            {
                RPCError::ServerError(self.message.clone())
            }
            _ => RPCError::ServiceError(self.message.clone()),
        }
    }
}

/// JSON-RPC 2.0 protocol implementation.
///
/// `export` wraps a local invoker as an exporter (for server-side),
/// `refer` creates an HTTP-based invoker for remote calls.
#[derive(Debug, Clone)]
pub struct JsonRpcProtocol;

impl JsonRpcProtocol {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonRpcProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Protocol for JsonRpcProtocol {
    async fn export(&self, invoker: Box<dyn Invoker>) -> Result<Box<dyn Exporter>> {
        Ok(Box::new(JsonRpcExporter { invoker }))
    }

    async fn refer(&self, url: &URL) -> Result<Box<dyn Invoker>> {
        let client = Client::new();
        let http_url = build_http_url(url);
        Ok(Box::new(JsonRpcInvoker {
            client,
            url: url.clone(),
            http_url: http_url.into(),
        }))
    }

    fn destroy(&self) {}
}

/// JSON-RPC invoker that calls a remote HTTP endpoint.
#[derive(Debug, Clone)]
pub struct JsonRpcInvoker {
    client: Client,
    url: URL,
    http_url: Box<str>,
}

impl Node for JsonRpcInvoker {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for JsonRpcInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult> {
        let method = build_method_name(ctx);
        let params = build_params(ctx);
        let request = JsonRpcRequest::new(method, params);

        let response: JsonRpcResponse = self
            .client
            .post(&*self.http_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("HTTP request failed: {e}"))?
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("failed to parse JSON-RPC response: {e}"))?;

        match (response.result, response.error) {
            (Some(result), _) => {
                let bytes = serde_json::to_vec(&result)
                    .map_err(|e| anyhow::anyhow!("failed to serialize result: {e}"))?;
                Ok(RPCResult::success(bytes))
            }
            (_, Some(err)) => Ok(RPCResult::from_error(err.to_rpc_error())),
            (None, None) => Err(anyhow::anyhow!(
                "invalid JSON-RPC response: both result and error are null"
            )),
        }
    }
}

/// JSON-RPC exporter that holds a wrapped invoker.
pub struct JsonRpcExporter {
    invoker: Box<dyn Invoker>,
}

impl Exporter for JsonRpcExporter {
    fn get_invoker(&self) -> &dyn Invoker {
        self.invoker.as_ref()
    }

    fn un_export(&self) {}
}

fn build_http_url(url: &URL) -> String {
    let host = if url.ip.is_empty() {
        "127.0.0.1"
    } else {
        &url.ip
    };
    let port = if url.port.is_empty() { "80" } else { &url.port };
    format!("http://{host}:{port}{}", url.path)
}

fn build_method_name(ctx: &InvocationContext) -> String {
    let service = ctx
        .attachments
        .get("interface")
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| ctx.url.path.trim_start_matches('/').to_string());
    format!("{service}.{}", ctx.method_name)
}

fn build_params(ctx: &InvocationContext) -> Vec<serde_json::Value> {
    ctx.arguments
        .iter()
        .map(|arg| {
            serde_json::from_slice::<serde_json::Value>(arg).unwrap_or_else(|_| {
                serde_json::Value::String(String::from_utf8_lossy(arg).to_string())
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dubbo_rs_common::url::URL;

    #[test]
    fn test_jsonrpc_request_serialization() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "com.example.GreetService.sayHello".to_string(),
            params: vec![serde_json::Value::String("world".to_string())],
            id: 1,
        };

        let json = serde_json::to_string(&req).expect("serialize request");
        let expected = r#"{"jsonrpc":"2.0","method":"com.example.GreetService.sayHello","params":["world"],"id":1}"#;
        assert_eq!(json, expected);
    }

    #[test]
    fn test_jsonrpc_request_new_auto_id() {
        let first = JsonRpcRequest::new(
            "svc.foo".to_string(),
            vec![serde_json::Value::Number(serde_json::Number::from(42))],
        );
        let second = JsonRpcRequest::new("svc.bar".to_string(), vec![]);

        assert_eq!(first.jsonrpc, "2.0");
        assert_eq!(second.jsonrpc, "2.0");
        assert!(
            second.id > first.id,
            "IDs should be monotonically increasing"
        );
        assert_eq!(first.params.len(), 1);
        assert_eq!(second.params.len(), 0);
    }

    #[test]
    fn test_jsonrpc_response_success_deserialization() {
        let json = r#"{"jsonrpc":"2.0","result":"hello","id":1}"#;
        let resp: JsonRpcResponse =
            serde_json::from_str(json).expect("deserialize success response");

        assert_eq!(resp.jsonrpc, "2.0");
        assert_eq!(resp.id, 1);
        assert_eq!(
            resp.result,
            Some(serde_json::Value::String("hello".to_string()))
        );
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_jsonrpc_response_error_deserialization() {
        let json =
            r#"{"jsonrpc":"2.0","error":{"code":-32601,"message":"Method not found"},"id":1}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).expect("deserialize error response");

        assert_eq!(resp.jsonrpc, "2.0");
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_none());
        let err = resp.error.expect("should have error");
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "Method not found");
    }

    #[test]
    fn test_jsonrpc_response_serialization_skips_nulls() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(serde_json::Value::String("ok".to_string())),
            error: None,
            id: 42,
        };
        let json = serde_json::to_string(&resp).expect("serialize response");
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_jsonrpc_error_obj_to_rpc_error() {
        let not_found = JsonRpcErrorObj::new(error_code::METHOD_NOT_FOUND, "Method not found");
        assert_eq!(
            not_found.to_rpc_error(),
            RPCError::ServiceNotFound("Method not found".into())
        );

        let bad_req = JsonRpcErrorObj::new(error_code::INVALID_PARAMS, "Invalid params");
        assert_eq!(
            bad_req.to_rpc_error(),
            RPCError::BadRequest("Invalid params".into())
        );

        let server_err = JsonRpcErrorObj::new(error_code::SERVER_ERROR_START, "Server error");
        assert_eq!(
            server_err.to_rpc_error(),
            RPCError::ServerError("Server error".into())
        );

        let custom = JsonRpcErrorObj::new(-100, "Custom error");
        assert_eq!(
            custom.to_rpc_error(),
            RPCError::ServiceError("Custom error".into())
        );
    }

    #[test]
    fn test_jsonrpc_request_roundtrip() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "test.echo".to_string(),
            params: vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::Number(serde_json::Number::from(1)),
                serde_json::Value::Bool(true),
            ],
            id: 7,
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let back: JsonRpcRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.jsonrpc, req.jsonrpc);
        assert_eq!(back.method, req.method);
        assert_eq!(back.id, req.id);
        assert_eq!(back.params.len(), 3);
    }

    struct StubInvoker {
        url: URL,
    }

    impl Node for StubInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }

        fn is_available(&self) -> bool {
            true
        }

        fn destroy(&self) {}
    }

    #[async_trait]
    impl Invoker for StubInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult> {
            Ok(RPCResult::success(b"stub_response".to_vec()))
        }
    }

    #[tokio::test]
    async fn test_protocol_export() {
        let protocol = JsonRpcProtocol::new();
        let stub = StubInvoker {
            url: URL::new("jsonrpc", "/com.example.TestService"),
        };

        let exporter = protocol
            .export(Box::new(stub))
            .await
            .expect("export should succeed");

        let invoker = exporter.get_invoker();
        assert!(invoker.is_available());
        assert_eq!(invoker.get_url().path, "/com.example.TestService");
    }

    #[tokio::test]
    async fn test_protocol_refer() {
        let protocol = JsonRpcProtocol::new();
        let mut url = URL::new("jsonrpc", "/com.example.RemoteService");
        url.ip = "127.0.0.1".to_string();
        url.port = "9090".to_string();

        let invoker = protocol.refer(&url).await.expect("refer should succeed");
        assert!(invoker.is_available());
        assert_eq!(invoker.get_url().path, "/com.example.RemoteService");
        assert_eq!(invoker.get_url().ip, "127.0.0.1");
        assert_eq!(invoker.get_url().port, "9090");
    }

    #[tokio::test]
    async fn test_exporter_get_invoker() {
        let stub = StubInvoker {
            url: URL::new("jsonrpc", "/com.example.ExportTest"),
        };
        let exporter = JsonRpcExporter {
            invoker: Box::new(stub),
        };

        let invoker = exporter.get_invoker();
        assert!(invoker.is_available());
        assert_eq!(invoker.get_url().path, "/com.example.ExportTest");
    }

    #[tokio::test]
    async fn test_exporter_un_export_noop() {
        let stub = StubInvoker {
            url: URL::new("jsonrpc", "/com.example.NoopTest"),
        };
        let exporter = JsonRpcExporter {
            invoker: Box::new(stub),
        };
        exporter.un_export();
        assert!(exporter.get_invoker().is_available());
    }

    #[test]
    fn test_build_http_url_defaults() {
        let url = URL::new("jsonrpc", "/api");
        let http_url = build_http_url(&url);
        assert_eq!(http_url, "http://127.0.0.1:80/api");
    }

    #[test]
    fn test_build_http_url_full() {
        let mut url = URL::new("jsonrpc", "/com.example.GreetService");
        url.ip = "10.0.0.1".to_string();
        url.port = "8080".to_string();
        let http_url = build_http_url(&url);
        assert_eq!(http_url, "http://10.0.0.1:8080/com.example.GreetService");
    }

    #[test]
    fn test_build_method_name_from_path() {
        let url = URL::new("jsonrpc", "/com.example.GreetService");
        let ctx = InvocationContext::new("sayHello", url);
        let method = build_method_name(&ctx);
        assert_eq!(method, "com.example.GreetService.sayHello");
    }

    #[test]
    fn test_build_method_name_from_attachment() {
        let url = URL::new("jsonrpc", "/some.path");
        let ctx = InvocationContext::new("sayHello", url)
            .with_attachment("interface", "com.example.GreetService");
        let method = build_method_name(&ctx);
        assert_eq!(method, "com.example.GreetService.sayHello");
    }

    #[test]
    fn test_build_method_name_attachment_empty_fallback() {
        let url = URL::new("jsonrpc", "/com.example.FallbackService");
        let ctx = InvocationContext::new("echo", url).with_attachment("interface", "");
        let method = build_method_name(&ctx);
        assert_eq!(method, "com.example.FallbackService.echo");
    }

    #[test]
    fn test_build_params_parses_json_arguments() {
        let url = URL::new("jsonrpc", "/test");
        let ctx = InvocationContext::new("m", url).with_arguments(vec![
            serde_json::to_vec(&serde_json::Value::String("hello".to_string())).expect("serialize"),
            serde_json::to_vec(&serde_json::Value::Number(serde_json::Number::from(42)))
                .expect("serialize"),
        ]);
        let params = build_params(&ctx);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], serde_json::Value::String("hello".to_string()));
        assert_eq!(
            params[1],
            serde_json::Value::Number(serde_json::Number::from(42))
        );
    }

    #[test]
    fn test_build_params_fallback_to_string() {
        let url = URL::new("jsonrpc", "/test");
        let ctx = InvocationContext::new("m", url).with_arguments(vec![b"plain text".to_vec()]);
        let params = build_params(&ctx);
        assert_eq!(params.len(), 1);
        assert_eq!(
            params[0],
            serde_json::Value::String("plain text".to_string())
        );
    }

    #[test]
    fn test_build_params_empty() {
        let url = URL::new("jsonrpc", "/test");
        let ctx = InvocationContext::new("m", url);
        let params = build_params(&ctx);
        assert!(params.is_empty());
    }

    #[test]
    fn test_protocol_default_and_destroy() {
        let protocol = JsonRpcProtocol;
        protocol.destroy();
    }

    #[test]
    fn test_invoker_node_traits() {
        let client = Client::new();
        let url = URL::new("jsonrpc", "/com.example.NodeTest");
        let invoker = JsonRpcInvoker {
            client,
            url: url.clone(),
            http_url: "http://127.0.0.1:8080/com.example.NodeTest"
                .to_string()
                .into(),
        };

        assert_eq!(invoker.get_url().path, "/com.example.NodeTest");
        assert!(invoker.is_available());
        invoker.destroy();
    }
}
