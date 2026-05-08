pub use dubbo_rs_common;
pub use dubbo_rs_configcenter;

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::{Arc, RwLock};
use std::time::Duration;

type ListenerMap = Arc<RwLock<HashMap<String, Vec<Arc<dyn ConfigListener>>>>>;

use async_trait::async_trait;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_configcenter::{ConfigCenter, ConfigChangeEvent, ConfigChangeType, ConfigListener};
use reqwest::Client;

const DEFAULT_NACOS_GROUP: &str = "DEFAULT_GROUP";
const DEFAULT_NAMESPACE: &str = "public";
const POLL_INTERVAL_SECS: u64 = 30;

/// Nacos config center implementation using HTTP API.
///
/// Communicates with Nacos server via its Open API (`/nacos/v1/cs/configs`).
/// Supports namespace/group isolation, username/password authentication,
/// and background polling for config change detection.
pub struct NacosConfigCenter {
    url: URL,
    /// Nacos server address in `http://{host}:{port}` format.
    pub server_addr: String,
    /// Nacos namespace (tenant) for config isolation.
    pub namespace: String,
    /// Nacos group for config grouping.
    pub group: String,
    /// Optional accessKey for Nacos auth.
    pub username: Option<String>,
    /// Optional secretKey for Nacos auth.
    pub password: Option<String>,
    http_client: Client,
    listeners: ListenerMap,
    poll_handles: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
}

impl NacosConfigCenter {
    /// Create a new `NacosConfigCenter` from a URL.
    ///
    /// The URL's `ip` and `port` fields are used to construct the server address.
    /// Defaults to namespace `"public"` and group `"DEFAULT_GROUP"`.
    #[must_use]
    pub fn new(url: URL) -> Self {
        let server_addr = format!("http://{}:{}", url.ip, url.port);
        Self {
            url,
            server_addr,
            namespace: DEFAULT_NAMESPACE.to_string(),
            group: DEFAULT_NACOS_GROUP.to_string(),
            username: None,
            password: None,
            http_client: Client::new(),
            listeners: Arc::new(RwLock::new(HashMap::new())),
            poll_handles: Mutex::new(HashMap::new()),
        }
    }

    /// Builder: set the Nacos namespace (tenant).
    #[must_use]
    pub fn with_namespace(mut self, ns: &str) -> Self {
        self.namespace = ns.to_string();
        self
    }

    /// Builder: set the Nacos group.
    #[must_use]
    pub fn with_group(mut self, group: &str) -> Self {
        self.group = group.to_string();
        self
    }

    /// Builder: set accessKey/secretKey for Nacos authentication.
    #[must_use]
    pub fn with_auth(mut self, user: &str, pass: &str) -> Self {
        self.username = Some(user.to_string());
        self.password = Some(pass.to_string());
        self
    }

    /// Fetch the current config value for a key.
    ///
    /// Sends `GET /nacos/v1/cs/configs?dataId={key}&group={group}&tenant={namespace}`.
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if the HTTP request fails or the server returns
    /// a non-success status (config not found returns `Ok(None)`).
    pub async fn get_config(&self, key: &str, group: &str) -> Result<Option<String>, RPCError> {
        let url = format!("{}/nacos/v1/cs/configs", self.server_addr);
        let mut builder = self.http_client.get(&url).query(&[
            ("dataId", key),
            ("group", group),
            ("tenant", self.namespace.as_str()),
        ]);

        builder = self.append_auth(builder);

        let resp = builder
            .send()
            .await
            .map_err(|e| RPCError::ServerError(format!("Nacos get_config failed: {e}")))?;

        let status = resp.status();
        if status.as_u16() == 404 {
            return Ok(None);
        }

        let text = resp
            .text()
            .await
            .map_err(|e| RPCError::ServerError(format!("Nacos get_config read failed: {e}")))?;

        if !status.is_success() {
            return Err(RPCError::ServerError(format!(
                "Nacos get_config HTTP {status}: {text}",
            )));
        }

        if text.is_empty() || text == "config data not exist" {
            return Ok(None);
        }

        Ok(Some(text))
    }

    /// Publish a config value.
    ///
    /// Sends `POST /nacos/v1/cs/configs` with form body.
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if the HTTP request fails or Nacos rejects the request.
    pub async fn set_config(&self, key: &str, group: &str, value: &str) -> Result<(), RPCError> {
        let url = format!("{}/nacos/v1/cs/configs", self.server_addr);
        let mut params = vec![
            ("dataId", key.to_string()),
            ("group", group.to_string()),
            ("content", value.to_string()),
            ("tenant", self.namespace.clone()),
        ];

        if let (Some(u), Some(p)) = (&self.username, &self.password) {
            params.push(("accessKey", u.clone()));
            params.push(("secretKey", p.clone()));
        }

        let resp = self
            .http_client
            .post(&url)
            .form(&params)
            .send()
            .await
            .map_err(|e| RPCError::ServerError(format!("Nacos set_config failed: {e}")))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| RPCError::ServerError(format!("Nacos set_config read failed: {e}")))?;

        if !status.is_success() {
            return Err(RPCError::ServerError(format!(
                "Nacos set_config HTTP {status}: {text}",
            )));
        }

        check_nacos_response(&text).map_err(|msg| {
            RPCError::ServerError(format!("Nacos set_config rejected: {msg} (body: {text})"))
        })?;

        Ok(())
    }

    /// Delete a config key.
    ///
    /// Sends `DELETE /nacos/v1/cs/configs?dataId={key}&group={group}&tenant={namespace}`.
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if the HTTP request fails or Nacos rejects the request.
    pub async fn remove_config(&self, key: &str, group: &str) -> Result<(), RPCError> {
        let url = format!("{}/nacos/v1/cs/configs", self.server_addr);
        let mut builder = self.http_client.delete(&url).query(&[
            ("dataId", key),
            ("group", group),
            ("tenant", self.namespace.as_str()),
        ]);

        builder = self.append_auth(builder);

        let resp = builder
            .send()
            .await
            .map_err(|e| RPCError::ServerError(format!("Nacos remove_config failed: {e}")))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| RPCError::ServerError(format!("Nacos remove_config read failed: {e}")))?;

        if !status.is_success() {
            return Err(RPCError::ServerError(format!(
                "Nacos remove_config HTTP {status}: {text}",
            )));
        }

        check_nacos_response(&text).map_err(|msg| {
            RPCError::ServerError(format!(
                "Nacos remove_config rejected: {msg} (body: {text})"
            ))
        })?;

        Ok(())
    }

    /// Append auth params to a request builder.
    fn append_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let (Some(u), Some(p)) = (&self.username, &self.password) {
            builder.query(&[("accessKey", u.as_str()), ("secretKey", p.as_str())])
        } else {
            builder
        }
    }

    /// Build the listener key used for internal tracking.
    fn listener_key(key: &str, group: &str) -> String {
        format!("{group}:{key}")
    }

    /// Start a background polling task that periodically checks for config changes.
    ///
    /// The task fetches the config value and compares it against the previous value.
    /// On change, all registered listeners are notified with the appropriate
    /// `ConfigChangeType`.
    fn start_poll_task(&self, key: &str, group: &str) {
        let lk = Self::listener_key(key, group);

        // Don't start a second poll for the same key+group.
        if self.poll_handles.lock().unwrap().contains_key(&lk) {
            return;
        }

        let client = self.http_client.clone();
        let server_addr = self.server_addr.clone();
        let namespace = self.namespace.clone();
        let username = self.username.clone();
        let password = self.password.clone();
        let listeners = self.listeners.clone();
        let poll_key = key.to_string();
        let poll_group = group.to_string();
        let lk_clone = lk.clone();

        let handle = tokio::spawn(async move {
            let mut previous_value: Option<String> = None;

            loop {
                let url = format!("{server_addr}/nacos/v1/cs/configs");
                let mut builder = client.get(&url).query(&[
                    ("dataId", poll_key.as_str()),
                    ("group", poll_group.as_str()),
                    ("tenant", namespace.as_str()),
                ]);

                if let (Some(u), Some(p)) = (&username, &password) {
                    builder =
                        builder.query(&[("accessKey", u.as_str()), ("secretKey", p.as_str())]);
                }

                let current_value = match builder.send().await {
                    Ok(resp) if resp.status().as_u16() == 404 => None,
                    Ok(resp) if resp.status().is_success() => match resp.text().await {
                        Ok(text) if text.is_empty() || text == "config data not exist" => None,
                        Ok(text) => Some(text),
                        Err(_) => {
                            tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
                            continue;
                        }
                    },
                    Ok(_) | Err(_) => {
                        tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
                        continue;
                    }
                };

                if current_value != previous_value {
                    let change_type = match (&previous_value, &current_value) {
                        (None, Some(_)) => ConfigChangeType::Created,
                        (Some(_), None) => ConfigChangeType::Deleted,
                        (Some(_), Some(_)) => ConfigChangeType::Modified,
                        (None, None) => {
                            previous_value = current_value;
                            tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
                            continue;
                        }
                    };

                    let event = ConfigChangeEvent::new(
                        &poll_key,
                        previous_value.clone(),
                        current_value.clone(),
                        change_type,
                    );

                    let snapshot: Vec<Arc<dyn ConfigListener>> = listeners
                        .read()
                        .unwrap()
                        .get(&lk_clone)
                        .cloned()
                        .unwrap_or_default();

                    for listener in snapshot {
                        listener.on_change(event.clone()).await;
                    }

                    previous_value = current_value;
                }

                tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
            }
        });

        self.poll_handles.lock().unwrap().insert(lk, handle);
    }

    /// Parse a Nacos config API response body into a config value.
    ///
    /// Returns `None` if the response indicates the config does not exist.
    /// Useful for testing response parsing without network calls.
    #[must_use]
    pub fn parse_config_response(body: &str) -> Option<String> {
        if body.is_empty() || body == "config data not exist" {
            return None;
        }
        Some(body.to_string())
    }

    /// Extract server address components from a URL.
    ///
    /// Returns `(host, port)` parsed from the URL fields.
    #[must_use]
    pub fn extract_server_addr(url: &URL) -> (String, String) {
        (url.ip.clone(), url.port.clone())
    }

    /// Build auth query params string for URL construction.
    ///
    /// Returns an empty string if no auth is configured.
    #[must_use]
    pub fn build_auth_params(&self) -> String {
        match (&self.username, &self.password) {
            (Some(u), Some(p)) => format!("&accessKey={u}&secretKey={p}"),
            _ => String::new(),
        }
    }
}

/// Check a Nacos API response body for success/failure.
///
/// Nacos 1.x returns plain `"ok"` for success.
/// JSON responses may contain a `code` field.
///
/// # Errors
///
/// Returns a string description on failure.
#[allow(clippy::missing_errors_doc)]
pub fn check_nacos_response(text: &str) -> Result<(), String> {
    if text == "ok" {
        return Ok(());
    }
    let body = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|e| format!("parse failed: {e}"))?;
    match body.get("code").and_then(serde_json::Value::as_i64) {
        Some(code) if code == 200 || code == 0 => Ok(()),
        Some(code) => {
            let msg = body
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown");
            Err(format!("code={code}, message={msg}"))
        }
        None => Ok(()),
    }
}

impl Node for NacosConfigCenter {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {
        let mut handles = self.poll_handles.lock().unwrap();
        for (_, handle) in handles.drain() {
            handle.abort();
        }
        self.listeners.write().unwrap().clear();
    }
}

#[async_trait]
impl ConfigCenter for NacosConfigCenter {
    async fn register(&self, _key: String, _group: String) -> Result<(), RPCError> {
        // Nacos config center doesn't require explicit registration.
        Ok(())
    }

    async fn unregister(&self, _key: String, _group: String) -> Result<(), RPCError> {
        // Nacos config center doesn't require explicit unregistration.
        Ok(())
    }

    async fn watch(
        &self,
        key: String,
        group: String,
        listener: Arc<dyn ConfigListener>,
    ) -> Result<(), RPCError> {
        let lk = Self::listener_key(&key, &group);

        self.listeners
            .write()
            .unwrap()
            .entry(lk.clone())
            .or_default()
            .push(listener);

        self.start_poll_task(&key, &group);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Helper to create a Nacos URL.
    fn make_nacos_url() -> URL {
        let mut url = URL::new("nacos", "");
        url.ip = "127.0.0.1".to_string();
        url.port = "8848".to_string();
        url
    }

    // Test listener that records events.
    struct TestConfigListener {
        events: Mutex<Vec<ConfigChangeEvent>>,
    }

    impl TestConfigListener {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }

        #[allow(dead_code)]
        fn events(&self) -> Vec<ConfigChangeEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl ConfigListener for TestConfigListener {
        async fn on_change(&self, event: ConfigChangeEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    #[test]
    fn test_nacos_config_center_new() {
        let url = make_nacos_url();
        let cc = NacosConfigCenter::new(url);

        assert!(cc.is_available());
        assert_eq!(cc.server_addr, "http://127.0.0.1:8848");
        assert_eq!(cc.namespace, DEFAULT_NAMESPACE);
        assert_eq!(cc.group, DEFAULT_NACOS_GROUP);
        assert!(cc.username.is_none());
        assert!(cc.password.is_none());
    }

    #[test]
    fn test_nacos_config_center_builder() {
        let cc = NacosConfigCenter::new(make_nacos_url())
            .with_namespace("dev-ns")
            .with_group("MY_GROUP")
            .with_auth("admin", "secret123");

        assert_eq!(cc.namespace, "dev-ns");
        assert_eq!(cc.group, "MY_GROUP");
        assert_eq!(cc.username, Some("admin".to_string()));
        assert_eq!(cc.password, Some("secret123".to_string()));
    }

    #[test]
    fn test_nacos_config_center_is_available() {
        let cc = NacosConfigCenter::new(make_nacos_url());
        assert!(cc.is_available());
    }

    #[tokio::test]
    async fn test_nacos_config_center_register_ok() {
        let cc = NacosConfigCenter::new(make_nacos_url());
        let result = cc
            .register("app.timeout".to_string(), "dubbo".to_string())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_nacos_config_center_unregister_ok() {
        let cc = NacosConfigCenter::new(make_nacos_url());
        let result = cc
            .unregister("app.timeout".to_string(), "dubbo".to_string())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_nacos_config_center_watch_adds_listener() {
        let cc = NacosConfigCenter::new(make_nacos_url());
        let listener = Arc::new(TestConfigListener::new());

        let result = cc
            .watch(
                "app.timeout".to_string(),
                "dubbo".to_string(),
                listener.clone(),
            )
            .await;
        assert!(result.is_ok());

        // Verify listener was stored.
        let lk = NacosConfigCenter::listener_key("app.timeout", "dubbo");
        let stored = cc.listeners.read().unwrap().get(&lk).cloned();
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().len(), 1);

        // Verify poll task was started.
        assert!(cc.poll_handles.lock().unwrap().contains_key(&lk));

        cc.destroy();
    }

    #[test]
    fn test_nacos_config_center_destroy() {
        let cc = NacosConfigCenter::new(make_nacos_url());
        let listener = Arc::new(TestConfigListener::new());

        // Manually insert a listener to verify it's cleared.
        let lk = NacosConfigCenter::listener_key("test.key", "dubbo");
        cc.listeners
            .write()
            .unwrap()
            .insert(lk.clone(), vec![listener]);

        assert!(!cc.listeners.read().unwrap().is_empty());

        cc.destroy();

        assert!(cc.listeners.read().unwrap().is_empty());
        assert!(cc.poll_handles.lock().unwrap().is_empty());
    }

    #[test]
    fn test_nacos_config_center_parse_config_response() {
        // Normal value
        assert_eq!(
            NacosConfigCenter::parse_config_response("timeout=30s"),
            Some("timeout=30s".to_string())
        );

        // Empty body
        assert_eq!(NacosConfigCenter::parse_config_response(""), None);

        // Not exist message
        assert_eq!(
            NacosConfigCenter::parse_config_response("config data not exist"),
            None
        );

        // JSON value
        assert_eq!(
            NacosConfigCenter::parse_config_response("{\"key\":\"value\"}"),
            Some("{\"key\":\"value\"}".to_string())
        );
    }

    #[test]
    fn test_nacos_config_center_url_extraction() {
        let mut url = URL::new("nacos", "/myapp");
        url.ip = "nacos.example.com".to_string();
        url.port = "8848".to_string();

        let (host, port) = NacosConfigCenter::extract_server_addr(&url);
        assert_eq!(host, "nacos.example.com");
        assert_eq!(port, "8848");

        // Verify NacosConfigCenter uses the URL correctly.
        let cc = NacosConfigCenter::new(url);
        assert_eq!(cc.server_addr, "http://nacos.example.com:8848");
    }

    #[test]
    fn test_nacos_config_center_auth_params() {
        // Without auth
        let cc_no_auth = NacosConfigCenter::new(make_nacos_url());
        assert!(cc_no_auth.build_auth_params().is_empty());

        // With auth
        let cc_auth = NacosConfigCenter::new(make_nacos_url()).with_auth("myUser", "myPass");
        let params = cc_auth.build_auth_params();
        assert!(params.contains("accessKey=myUser"));
        assert!(params.contains("secretKey=myPass"));
    }

    #[test]
    fn test_check_nacos_response_ok() {
        assert!(check_nacos_response("ok").is_ok());
        assert!(check_nacos_response("{\"code\":200}").is_ok());
        assert!(check_nacos_response("{\"code\":0}").is_ok());
        assert!(check_nacos_response("{\"data\":\"something\"}").is_ok());
    }

    #[test]
    fn test_check_nacos_response_error() {
        assert!(check_nacos_response("{\"code\":500,\"message\":\"internal error\"}").is_err());
        assert!(check_nacos_response("not json at all").is_err());
    }

    #[test]
    fn test_listener_key_format() {
        assert_eq!(
            NacosConfigCenter::listener_key("app.timeout", "dubbo"),
            "dubbo:app.timeout"
        );
    }

    #[test]
    fn test_nacos_config_center_multiple_watchers() {
        let cc = NacosConfigCenter::new(make_nacos_url());
        let l1 = Arc::new(TestConfigListener::new());
        let l2 = Arc::new(TestConfigListener::new());

        let lk = NacosConfigCenter::listener_key("app.timeout", "dubbo");

        // Manually add listeners to test multi-listener storage.
        cc.listeners
            .write()
            .unwrap()
            .entry(lk.clone())
            .or_default()
            .push(l1);
        cc.listeners
            .write()
            .unwrap()
            .entry(lk.clone())
            .or_default()
            .push(l2);

        let stored = cc.listeners.read().unwrap().get(&lk).cloned().unwrap();
        assert_eq!(stored.len(), 2);
    }

    #[test]
    fn test_nacos_config_center_default_constants() {
        assert_eq!(DEFAULT_NACOS_GROUP, "DEFAULT_GROUP");
        assert_eq!(DEFAULT_NAMESPACE, "public");
        assert_eq!(POLL_INTERVAL_SECS, 30);
    }
}
