pub use dubbo_rs_common;
pub use dubbo_rs_protocol;

use anyhow::Result;
use async_trait::async_trait;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::{Exporter, InvocationContext, Invoker, Protocol, RPCResult};

const READ_METHOD_PREFIXES: &[&str] = &[
    "get", "find", "list", "query", "read", "fetch", "search", "count", "exists", "check", "has",
    "is", "can",
];

const UPDATE_METHOD_PREFIXES: &[&str] = &["update", "put", "modify"];

const DELETE_METHOD_PREFIXES: &[&str] = &["delete", "remove"];

const PATCH_METHOD_PREFIXES: &[&str] = &["patch", "partial"];

/// Returns `true` if the method name indicates a read-only (GET) operation.
#[must_use]
fn is_read_method(method_name: &str) -> bool {
    let lower = method_name.to_lowercase();
    READ_METHOD_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// Returns `true` if the method name indicates an update (PUT) operation.
#[must_use]
fn is_update_method(method_name: &str) -> bool {
    let lower = method_name.to_lowercase();
    UPDATE_METHOD_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// Returns `true` if the method name indicates a delete (DELETE) operation.
#[must_use]
fn is_delete_method(method_name: &str) -> bool {
    let lower = method_name.to_lowercase();
    DELETE_METHOD_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// Returns `true` if the method name indicates a partial update (PATCH) operation.
#[must_use]
fn is_patch_method(method_name: &str) -> bool {
    let lower = method_name.to_lowercase();
    PATCH_METHOD_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

/// Build the HTTP URL for a given service URL and method name.
///
/// Format: `{protocol}://{ip}:{port}{path}/{method_name}`
#[must_use]
fn build_url(url: &URL, method_name: &str) -> String {
    format!(
        "{}://{}:{}{}/{}",
        url.protocol, url.ip, url.port, url.path, method_name
    )
}

fn serialize_arguments(ctx: &InvocationContext) -> serde_json::Value {
    if ctx.arguments.is_empty() {
        serde_json::Value::Null
    } else if ctx.arguments.len() == 1 {
        match serde_json::from_slice::<serde_json::Value>(&ctx.arguments[0]) {
            Ok(v) => v,
            Err(_) => {
                serde_json::Value::String(String::from_utf8_lossy(&ctx.arguments[0]).into_owned())
            }
        }
    } else {
        let values: Vec<serde_json::Value> = ctx
            .arguments
            .iter()
            .map(|arg| {
                serde_json::from_slice::<serde_json::Value>(arg).unwrap_or_else(|_| {
                    serde_json::Value::String(String::from_utf8_lossy(arg).into_owned())
                })
            })
            .collect();
        serde_json::Value::Array(values)
    }
}

async fn handle_response(resp: reqwest::Response) -> Result<RPCResult> {
    if resp.status().is_success() {
        let body = resp.bytes().await?;
        Ok(RPCResult::success(body.to_vec()))
    } else {
        let status = resp.status().as_u16();
        let err_body = resp.text().await.unwrap_or_default();
        Ok(RPCResult::from_error(
            dubbo_rs_common::error::RPCError::ServiceError(format!("HTTP {status}: {err_body}")),
        ))
    }
}

pub struct RestInvoker {
    url: URL,
    client: reqwest::Client,
}

impl RestInvoker {
    #[must_use]
    pub fn from_url(url: URL) -> Self {
        Self {
            url,
            client: reqwest::Client::new(),
        }
    }

    #[must_use]
    pub fn get_url(&self) -> &URL {
        &self.url
    }
}

impl Node for RestInvoker {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        !self.url.ip.is_empty() && !self.url.port.is_empty()
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for RestInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult> {
        let url_str = build_url(&ctx.url, &ctx.method_name);

        if is_read_method(&ctx.method_name) {
            self.invoke_get(&url_str, ctx).await
        } else if is_update_method(&ctx.method_name) {
            self.invoke_put(&url_str, ctx).await
        } else if is_delete_method(&ctx.method_name) {
            self.invoke_delete(&url_str, ctx).await
        } else if is_patch_method(&ctx.method_name) {
            self.invoke_patch(&url_str, ctx).await
        } else {
            self.invoke_post(&url_str, ctx).await
        }
    }
}

impl RestInvoker {
    async fn invoke_get(&self, url: &str, ctx: &InvocationContext) -> Result<RPCResult> {
        let mut request = self.client.get(url);

        for (i, arg) in ctx.arguments.iter().enumerate() {
            let value = String::from_utf8_lossy(arg);
            request = request.query(&[("args".to_string() + &i.to_string(), value.into_owned())]);
        }

        let resp = request.send().await?;

        handle_response(resp).await
    }

    async fn invoke_post(&self, url: &str, ctx: &InvocationContext) -> Result<RPCResult> {
        let body = serialize_arguments(ctx);

        let resp = self.client.post(url).json(&body).send().await?;

        handle_response(resp).await
    }

    async fn invoke_put(&self, url: &str, ctx: &InvocationContext) -> Result<RPCResult> {
        let body = serialize_arguments(ctx);

        let resp = self.client.put(url).json(&body).send().await?;

        handle_response(resp).await
    }

    async fn invoke_delete(&self, url: &str, ctx: &InvocationContext) -> Result<RPCResult> {
        let mut request = self.client.delete(url);

        for (i, arg) in ctx.arguments.iter().enumerate() {
            let value = String::from_utf8_lossy(arg);
            request = request.query(&[("args".to_string() + &i.to_string(), value.into_owned())]);
        }

        let resp = request.send().await?;

        handle_response(resp).await
    }

    async fn invoke_patch(&self, url: &str, ctx: &InvocationContext) -> Result<RPCResult> {
        let body = serialize_arguments(ctx);

        let resp = self.client.patch(url).json(&body).send().await?;

        handle_response(resp).await
    }
}

pub struct RestExporter {
    invoker: Box<dyn Invoker>,
}

impl Exporter for RestExporter {
    fn get_invoker(&self) -> &dyn Invoker {
        self.invoker.as_ref()
    }

    fn un_export(&self) {}
}

pub struct RestProtocol;

impl RestProtocol {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn name(&self) -> &'static str {
        "rest"
    }
}

impl Default for RestProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Protocol for RestProtocol {
    async fn export(&self, invoker: Box<dyn Invoker>) -> Result<Box<dyn Exporter>> {
        Ok(Box::new(RestExporter { invoker }))
    }

    async fn refer(&self, url: &URL) -> Result<Box<dyn Invoker>> {
        Ok(Box::new(RestInvoker::from_url(url.clone())))
    }

    fn destroy(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── URL construction tests ──

    #[test]
    fn test_build_url() {
        let mut url = URL::new("http", "/com.example.GreetService");
        url.ip = "127.0.0.1".into();
        url.port = "8080".into();

        let result = build_url(&url, "sayHello");
        assert_eq!(
            result,
            "http://127.0.0.1:8080/com.example.GreetService/sayHello"
        );
    }

    #[test]
    fn test_build_url_with_https() {
        let mut url = URL::new("https", "/api/v1/UserService");
        url.ip = "192.168.1.100".into();
        url.port = "443".into();

        let result = build_url(&url, "getUser");
        assert_eq!(
            result,
            "https://192.168.1.100:443/api/v1/UserService/getUser"
        );
    }

    #[test]
    fn test_build_url_different_method_names() {
        let mut url = URL::new("http", "/com.example.OrderService");
        url.ip = "10.0.0.1".into();
        url.port = "3000".into();

        assert_eq!(
            build_url(&url, "create"),
            "http://10.0.0.1:3000/com.example.OrderService/create"
        );
        assert_eq!(
            build_url(&url, "findById"),
            "http://10.0.0.1:3000/com.example.OrderService/findById"
        );
    }

    // ── is_read_method tests ──

    #[test]
    fn test_is_read_method_true() {
        assert!(is_read_method("getUser"));
        assert!(is_read_method("findById"));
        assert!(is_read_method("listOrders"));
        assert!(is_read_method("queryByStatus"));
        assert!(is_read_method("readConfig"));
        assert!(is_read_method("fetchData"));
        assert!(is_read_method("searchProducts"));
        assert!(is_read_method("countUsers"));
        assert!(is_read_method("existsUser"));
        assert!(is_read_method("checkHealth"));
        assert!(is_read_method("hasPermission"));
        assert!(is_read_method("isValid"));
        assert!(is_read_method("canAccess"));
    }

    #[test]
    fn test_is_read_method_false() {
        assert!(!is_read_method("createUser"));
        assert!(!is_read_method("updateOrder"));
        assert!(!is_read_method("deleteItem"));
        assert!(!is_read_method("saveConfig"));
        assert!(!is_read_method("processPayment"));
    }

    #[test]
    fn test_is_read_method_case_insensitive() {
        assert!(is_read_method("GET_USER"));
        assert!(is_read_method("FindAll"));
        assert!(is_read_method("LIST_ITEMS"));
    }

    // ── RestInvoker creation tests ──

    #[test]
    fn test_rest_invoker_from_url() {
        let mut url = URL::new("http", "/com.example.GreetService");
        url.ip = "127.0.0.1".into();
        url.port = "8080".into();
        let invoker = RestInvoker::from_url(url.clone());

        assert_eq!(invoker.get_url().path, "/com.example.GreetService");
        assert_eq!(invoker.get_url().protocol, "http");
        assert!(invoker.is_available());
    }

    #[test]
    fn test_rest_invoker_unavailable_missing_ip() {
        let mut url = URL::new("http", "/com.example.TestService");
        url.port = "8080".into();
        let invoker = RestInvoker::from_url(url);
        assert!(!invoker.is_available());
    }

    #[test]
    fn test_rest_invoker_unavailable_missing_port() {
        let mut url = URL::new("http", "/com.example.TestService");
        url.ip = "127.0.0.1".into();
        let invoker = RestInvoker::from_url(url);
        assert!(!invoker.is_available());
    }

    #[test]
    fn test_rest_invoker_unavailable_empty_all() {
        let url = URL::new("http", "/com.example.TestService");
        let invoker = RestInvoker::from_url(url);
        assert!(!invoker.is_available());
    }

    // ── RestProtocol tests ──

    #[test]
    fn test_rest_protocol_name() {
        let protocol = RestProtocol::new();
        assert_eq!(protocol.name(), "rest");
    }

    #[test]
    fn test_rest_protocol_default() {
        let protocol = RestProtocol;
        assert_eq!(protocol.name(), "rest");
    }

    #[tokio::test]
    async fn test_rest_protocol_refer_creates_invoker() {
        let protocol = RestProtocol::new();
        let mut url = URL::new("http", "/com.example.GreetService");
        url.ip = "127.0.0.1".into();
        url.port = "8080".into();

        let invoker = protocol.refer(&url).await.expect("refer should succeed");
        assert!(invoker.is_available());
        assert_eq!(invoker.get_url().path, "/com.example.GreetService");
    }

    #[tokio::test]
    async fn test_rest_protocol_export_creates_exporter() {
        let protocol = RestProtocol::new();
        let mut url = URL::new("http", "/com.example.GreetService");
        url.ip = "127.0.0.1".into();
        url.port = "8080".into();
        let invoker: Box<dyn Invoker> = Box::new(RestInvoker::from_url(url));

        let exporter = protocol
            .export(invoker)
            .await
            .expect("export should succeed");
        assert!(exporter.get_invoker().is_available());
    }

    #[test]
    fn test_rest_exporter_get_invoker() {
        let mut url = URL::new("http", "/com.example.TestService");
        url.ip = "127.0.0.1".into();
        url.port = "8080".into();
        let invoker: Box<dyn Invoker> = Box::new(RestInvoker::from_url(url));
        let exporter = RestExporter { invoker };

        assert!(exporter.get_invoker().is_available());
    }

    // ── is_update_method tests ──

    #[test]
    fn test_is_update_method_true() {
        assert!(is_update_method("updateUser"));
        assert!(is_update_method("updateOrder"));
        assert!(is_update_method("putItem"));
        assert!(is_update_method("putConfig"));
        assert!(is_update_method("modifyRecord"));
        assert!(is_update_method("modifySettings"));
    }

    #[test]
    fn test_is_update_method_false() {
        assert!(!is_update_method("createUser"));
        assert!(!is_update_method("deleteItem"));
        assert!(!is_update_method("getUser"));
        assert!(!is_update_method("saveData"));
    }

    // ── is_delete_method tests ──

    #[test]
    fn test_is_delete_method_true() {
        assert!(is_delete_method("deleteItem"));
        assert!(is_delete_method("deleteUser"));
        assert!(is_delete_method("removeRecord"));
        assert!(is_delete_method("removeCache"));
    }

    #[test]
    fn test_is_delete_method_false() {
        assert!(!is_delete_method("getUser"));
        assert!(!is_delete_method("updateOrder"));
        assert!(!is_delete_method("createUser"));
        assert!(!is_delete_method("patchItem"));
    }

    // ── is_patch_method tests ──

    #[test]
    fn test_is_patch_method_true() {
        assert!(is_patch_method("patchUser"));
        assert!(is_patch_method("patchOrder"));
        assert!(is_patch_method("partialUpdate"));
        assert!(is_patch_method("partialModify"));
    }

    #[test]
    fn test_is_patch_method_false() {
        assert!(!is_patch_method("updateUser"));
        assert!(!is_patch_method("postOrder"));
        assert!(!is_patch_method("createItem"));
        assert!(!is_patch_method("getUser"));
    }

    #[test]
    fn test_method_classification_case_insensitive() {
        assert!(is_update_method("UPDATE_USER"));
        assert!(is_update_method("PutItem"));
        assert!(is_delete_method("DELETE_RECORD"));
        assert!(is_delete_method("RemoveCache"));
        assert!(is_patch_method("PATCH_ORDER"));
        assert!(is_patch_method("PartialUpdate"));
    }

    #[test]
    fn test_method_priority_read_over_update() {
        // "updateAndGet" starts with "update" → update, not read
        assert!(!is_read_method("updateAndGet"));
        assert!(is_update_method("updateAndGet"));
        // "getAndUpdate" starts with "get" → read takes priority
        assert!(is_read_method("getAndUpdate"));
    }

    #[test]
    fn test_delete_not_read() {
        assert!(is_delete_method("deleteItem"));
        assert!(!is_read_method("deleteItem"));
    }

    #[test]
    fn test_all_method_classifications_distinct() {
        // GET
        assert!(is_read_method("getUser"));
        assert!(!is_update_method("getUser"));
        assert!(!is_delete_method("getUser"));
        assert!(!is_patch_method("getUser"));

        // PUT
        assert!(!is_read_method("updateUser"));
        assert!(is_update_method("updateUser"));
        assert!(!is_delete_method("updateUser"));
        assert!(!is_patch_method("updateUser"));

        // DELETE
        assert!(!is_read_method("deleteUser"));
        assert!(!is_update_method("deleteUser"));
        assert!(is_delete_method("deleteUser"));
        assert!(!is_patch_method("deleteUser"));

        // PATCH
        assert!(!is_read_method("patchUser"));
        assert!(!is_update_method("patchUser"));
        assert!(!is_delete_method("patchUser"));
        assert!(is_patch_method("patchUser"));

        // POST (default - none of the above)
        assert!(!is_read_method("createUser"));
        assert!(!is_update_method("createUser"));
        assert!(!is_delete_method("createUser"));
        assert!(!is_patch_method("createUser"));
    }
}
