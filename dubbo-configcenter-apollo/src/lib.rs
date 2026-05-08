pub use dubbo_rs_common;
pub use dubbo_rs_configcenter;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_configcenter::{ConfigCenter, ConfigChangeEvent, ConfigChangeType, ConfigListener};
use reqwest::Client;

const DEFAULT_CLUSTER: &str = "default";
const DEFAULT_NAMESPACE: &str = "application";
const POLL_INTERVAL_SECS: u64 = 30;

type ListenerMap = Arc<RwLock<HashMap<String, Vec<Arc<dyn ConfigListener>>>>>;

/// Apollo configuration center implementation using HTTP API.
///
/// Communicates with Apollo Config Service via its REST API.
/// Supports `app_id/cluster/namespace` isolation and background
/// long-polling for config change detection.
pub struct ApolloConfigCenter {
    url: URL,
    /// Apollo meta server address in `http://{host}:{port}` format.
    pub meta_server_url: String,
    /// Apollo App ID.
    pub app_id: String,
    /// Apollo cluster name.
    pub cluster: String,
    /// Apollo namespace (typically `application`).
    pub namespace: String,
    /// Optional token for Apollo authentication.
    pub token: Option<String>,
    http_client: Client,
    listeners: ListenerMap,
    poll_handles: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
}

/// Builder for [`ApolloConfigCenter`].
///
/// # Examples
///
/// ```
/// use dubbo_rs_configcenter_apollo::ApolloConfigCenterBuilder;
///
/// let cc = ApolloConfigCenterBuilder::new()
///     .meta_server_url("http://127.0.0.1:8080")
///     .app_id("my-app")
///     .build()
///     .expect("required fields should be set");
/// ```
pub struct ApolloConfigCenterBuilder {
    meta_server_url: Option<String>,
    app_id: Option<String>,
    cluster: Option<String>,
    namespace: Option<String>,
    token: Option<String>,
}

impl ApolloConfigCenterBuilder {
    /// Create a new builder with default (empty) settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            meta_server_url: None,
            app_id: None,
            cluster: None,
            namespace: None,
            token: None,
        }
    }

    /// Set the Apollo meta server URL (required).
    #[must_use]
    pub fn meta_server_url(mut self, url: impl Into<String>) -> Self {
        self.meta_server_url = Some(url.into());
        self
    }

    /// Set the Apollo App ID (required).
    #[must_use]
    pub fn app_id(mut self, id: impl Into<String>) -> Self {
        self.app_id = Some(id.into());
        self
    }

    /// Set the Apollo cluster name (defaults to `"default"`).
    #[must_use]
    pub fn cluster(mut self, cluster: impl Into<String>) -> Self {
        self.cluster = Some(cluster.into());
        self
    }

    /// Set the Apollo namespace (defaults to `"application"`).
    #[must_use]
    pub fn namespace(mut self, ns: impl Into<String>) -> Self {
        self.namespace = Some(ns.into());
        self
    }

    /// Set the Apollo access token for authentication.
    #[must_use]
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Build the [`ApolloConfigCenter`].
    ///
    /// # Errors
    ///
    /// Returns an error if `meta_server_url` or `app_id` is not set.
    pub fn build(self) -> Result<ApolloConfigCenter, anyhow::Error> {
        let meta_server_url = self
            .meta_server_url
            .ok_or_else(|| anyhow::anyhow!("meta_server_url is required"))?;
        let app_id = self
            .app_id
            .ok_or_else(|| anyhow::anyhow!("app_id is required"))?;

        let mut url = URL::new("apollo", "");
        url.ip.clone_from(&meta_server_url);
        url.port = String::new();

        Ok(ApolloConfigCenter {
            url,
            meta_server_url,
            app_id,
            cluster: self.cluster.unwrap_or_else(|| DEFAULT_CLUSTER.to_string()),
            namespace: self
                .namespace
                .unwrap_or_else(|| DEFAULT_NAMESPACE.to_string()),
            token: self.token,
            http_client: Client::new(),
            listeners: Arc::new(RwLock::new(HashMap::new())),
            poll_handles: Mutex::new(HashMap::new()),
        })
    }
}

impl Default for ApolloConfigCenterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ApolloConfigCenter {
    /// Build the config URL for a given key.
    ///
    /// Format: `{meta_server_url}/configs/{app_id}/{cluster}/{namespace}/{key}`
    #[must_use]
    pub fn build_config_url(&self, key: &str) -> String {
        format!(
            "{}/configs/{}/{}/{}/{}",
            self.meta_server_url, self.app_id, self.cluster, self.namespace, key
        )
    }

    /// Build the notification long-poll URL.
    ///
    /// Format: `{meta_server_url}/notifications/v2`
    #[must_use]
    pub fn build_notification_url(&self) -> String {
        format!("{}/notifications/v2", self.meta_server_url)
    }

    /// Fetch the current config value for a key.
    ///
    /// Sends `GET {meta_server_url}/configs/{app_id}/{cluster}/{namespace}/{key}`.
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if the HTTP request fails or the server returns
    /// a non-success status (config not found returns `Ok(None)`).
    pub async fn get_config(&self, key: &str) -> Result<Option<String>, RPCError> {
        let url = self.build_config_url(key);
        let mut builder = self.http_client.get(&url);

        if let Some(ref token) = self.token {
            builder = builder.header("Authorization", token.as_str());
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| RPCError::ServerError(format!("Apollo get_config failed: {e}")))?;

        let status = resp.status();
        if status.as_u16() == 404 {
            return Ok(None);
        }

        let text = resp
            .text()
            .await
            .map_err(|e| RPCError::ServerError(format!("Apollo get_config read failed: {e}")))?;

        if !status.is_success() {
            return Err(RPCError::ServerError(format!(
                "Apollo get_config HTTP {status}: {text}",
            )));
        }

        if text.is_empty() {
            return Ok(None);
        }

        // Apollo returns JSON with an `appId` field for success,
        // or an empty body when the config does not exist.
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(val) => {
                // Apollo config API returns the raw value in the response body.
                // If the value is a string, extract it; otherwise return the whole body.
                if let Some(v) = val.get("value").and_then(|v| v.as_str()) {
                    Ok(Some(v.to_string()))
                } else {
                    Ok(Some(text))
                }
            }
            Err(_) => {
                // Plain text value
                Ok(Some(text))
            }
        }
    }

    /// Publish a config value.
    ///
    /// Sends `POST {meta_server_url}/configs` with JSON body containing
    /// `appId`, `clusterName`, `namespaceName`, `key`, and `value`.
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if the HTTP request fails or Apollo rejects the request.
    pub async fn set_config(&self, key: &str, value: &str) -> Result<(), RPCError> {
        let url = format!("{}/configs", self.meta_server_url);

        let body = serde_json::json!({
            "appId": self.app_id,
            "clusterName": self.cluster,
            "namespaceName": self.namespace,
            "key": key,
            "value": value,
        });

        let mut builder = self.http_client.post(&url).json(&body);

        if let Some(ref token) = self.token {
            builder = builder.header("Authorization", token.as_str());
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| RPCError::ServerError(format!("Apollo set_config failed: {e}")))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| RPCError::ServerError(format!("Apollo set_config read failed: {e}")))?;

        if !status.is_success() {
            return Err(RPCError::ServerError(format!(
                "Apollo set_config HTTP {status}: {text}",
            )));
        }

        Ok(())
    }

    /// Delete a config key.
    ///
    /// Sends `DELETE {meta_server_url}/configs/{app_id}/{cluster}/{namespace}/{key}`.
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if the HTTP request fails or Apollo rejects the request.
    pub async fn remove_config(&self, key: &str) -> Result<(), RPCError> {
        let url = self.build_config_url(key);

        let mut builder = self.http_client.delete(&url);

        if let Some(ref token) = self.token {
            builder = builder.header("Authorization", token.as_str());
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| RPCError::ServerError(format!("Apollo remove_config failed: {e}")))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| RPCError::ServerError(format!("Apollo remove_config read failed: {e}")))?;

        if !status.is_success() {
            return Err(RPCError::ServerError(format!(
                "Apollo remove_config HTTP {status}: {text}",
            )));
        }

        Ok(())
    }

    /// Build the listener key used for internal tracking.
    fn listener_key(key: &str) -> String {
        key.to_string()
    }

    /// Start a background polling task that periodically checks for config changes.
    ///
    /// The task fetches the config value and compares it against the previous value.
    /// On change, all registered listeners are notified with the appropriate
    /// [`ConfigChangeType`].
    fn start_poll_task(&self, key: String) {
        let lk = Self::listener_key(&key);

        // Don't start a second poll for the same key.
        if self.poll_handles.lock().unwrap().contains_key(&lk) {
            return;
        }

        let client = self.http_client.clone();
        let meta_server_url = self.meta_server_url.clone();
        let app_id = self.app_id.clone();
        let cluster = self.cluster.clone();
        let namespace = self.namespace.clone();
        let token = self.token.clone();
        let listeners = self.listeners.clone();
        let poll_key = key;
        let lk_clone = lk.clone();

        let handle = tokio::spawn(async move {
            let mut previous_value: Option<String> = None;

            loop {
                let config_url =
                    format!("{meta_server_url}/configs/{app_id}/{cluster}/{namespace}/{poll_key}");

                let mut builder = client.get(&config_url);
                if let Some(ref t) = token {
                    builder = builder.header("Authorization", t.as_str());
                }

                let current_value = match builder.send().await {
                    Ok(resp) if resp.status().as_u16() == 404 => None,
                    Ok(resp) if resp.status().is_success() => match resp.text().await {
                        Ok(text) if text.is_empty() => None,
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

    /// Parse an Apollo config API response body into a config value.
    ///
    /// Returns `None` if the response is empty.
    /// Useful for testing response parsing without network calls.
    #[must_use]
    pub fn parse_config_response(body: &str) -> Option<String> {
        if body.is_empty() {
            return None;
        }
        match serde_json::from_str::<serde_json::Value>(body) {
            Ok(val) => {
                if let Some(v) = val.get("value").and_then(|v| v.as_str()) {
                    Some(v.to_string())
                } else {
                    Some(body.to_string())
                }
            }
            Err(_) => Some(body.to_string()),
        }
    }

    /// Extract meta server URL from a URL.
    ///
    /// Returns the meta server URL parsed from the URL's `ip` field.
    #[must_use]
    pub fn extract_meta_server_url(url: &URL) -> String {
        url.ip.clone()
    }

    /// Build auth header string for request construction.
    ///
    /// Returns an empty string if no auth is configured.
    #[must_use]
    pub fn build_auth_header(&self) -> String {
        match &self.token {
            Some(t) => t.clone(),
            None => String::new(),
        }
    }
}

impl Node for ApolloConfigCenter {
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
impl ConfigCenter for ApolloConfigCenter {
    async fn register(&self, _key: String, _group: String) -> Result<(), RPCError> {
        // Apollo config center doesn't require explicit registration.
        Ok(())
    }

    async fn unregister(&self, _key: String, _group: String) -> Result<(), RPCError> {
        // Apollo config center doesn't require explicit unregistration.
        Ok(())
    }

    async fn watch(
        &self,
        key: String,
        _group: String,
        listener: Arc<dyn ConfigListener>,
    ) -> Result<(), RPCError> {
        let lk = Self::listener_key(&key);

        self.listeners
            .write()
            .unwrap()
            .entry(lk.clone())
            .or_default()
            .push(listener);

        self.start_poll_task(key);

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

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
    fn test_apollo_config_center_builder_required_fields() {
        let cc = ApolloConfigCenterBuilder::new()
            .meta_server_url("http://127.0.0.1:8080")
            .app_id("test-app")
            .build()
            .expect("build should succeed with required fields");

        assert_eq!(cc.meta_server_url, "http://127.0.0.1:8080");
        assert_eq!(cc.app_id, "test-app");
    }

    #[test]
    fn test_apollo_config_center_builder_with_all_options() {
        let cc = ApolloConfigCenterBuilder::new()
            .meta_server_url("http://apollo.example.com:8080")
            .app_id("my-service")
            .cluster("prod-cluster")
            .namespace("application")
            .token("secret-token")
            .build()
            .expect("build should succeed with all fields");

        assert_eq!(cc.meta_server_url, "http://apollo.example.com:8080");
        assert_eq!(cc.app_id, "my-service");
        assert_eq!(cc.cluster, "prod-cluster");
        assert_eq!(cc.namespace, "application");
        assert_eq!(cc.token, Some("secret-token".to_string()));
    }

    #[test]
    fn test_apollo_config_center_default_cluster() {
        let cc = ApolloConfigCenterBuilder::new()
            .meta_server_url("http://localhost:8080")
            .app_id("app")
            .build()
            .expect("build should succeed");

        assert_eq!(cc.cluster, DEFAULT_CLUSTER);
    }

    #[test]
    fn test_apollo_config_center_builder_missing_meta_server() {
        let result = ApolloConfigCenterBuilder::new().app_id("test-app").build();

        assert!(result.is_err());
        let err_msg = match result {
            Err(e) => e.to_string(),
            Ok(_) => unreachable!("expected error"),
        };
        assert!(
            err_msg.contains("meta_server_url"),
            "error should mention meta_server_url: {err_msg}"
        );
    }

    #[test]
    fn test_apollo_config_center_builder_missing_app_id() {
        let result = ApolloConfigCenterBuilder::new()
            .meta_server_url("http://localhost:8080")
            .build();

        assert!(result.is_err());
        let err_msg = match result {
            Err(e) => e.to_string(),
            Ok(_) => unreachable!("expected error"),
        };
        assert!(
            err_msg.contains("app_id"),
            "error should mention app_id: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_apollo_config_center_listener_registration() {
        let cc = ApolloConfigCenterBuilder::new()
            .meta_server_url("http://127.0.0.1:8080")
            .app_id("test-app")
            .build()
            .expect("build should succeed");

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
        let lk = ApolloConfigCenter::listener_key("app.timeout");
        let stored = cc.listeners.read().unwrap().get(&lk).cloned();
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().len(), 1);

        // Verify poll task was started.
        assert!(cc.poll_handles.lock().unwrap().contains_key(&lk));

        cc.destroy();
    }

    #[test]
    fn test_apollo_config_center_url_construction() {
        let cc = ApolloConfigCenterBuilder::new()
            .meta_server_url("http://apollo.example.com:8080")
            .app_id("my-app")
            .cluster("prod")
            .namespace("application")
            .build()
            .expect("build should succeed");

        // Verify config URL format
        let config_url = cc.build_config_url("timeout");
        assert_eq!(
            config_url,
            "http://apollo.example.com:8080/configs/my-app/prod/application/timeout"
        );

        // Verify notification URL format
        let notification_url = cc.build_notification_url();
        assert_eq!(
            notification_url,
            "http://apollo.example.com:8080/notifications/v2"
        );
    }

    #[test]
    fn test_apollo_config_center_default_values() {
        let cc = ApolloConfigCenterBuilder::new()
            .meta_server_url("http://localhost:8080")
            .app_id("app")
            .build()
            .expect("build should succeed");

        assert_eq!(cc.cluster, "default");
        assert_eq!(cc.namespace, "application");
        assert!(cc.token.is_none());
        assert!(cc.is_available());
    }

    #[test]
    fn test_apollo_config_center_parse_config_response() {
        // JSON with value field
        assert_eq!(
            ApolloConfigCenter::parse_config_response(r#"{"value":"30s","key":"timeout"}"#),
            Some("30s".to_string())
        );

        // Empty body
        assert_eq!(ApolloConfigCenter::parse_config_response(""), None);

        // Plain text value
        assert_eq!(
            ApolloConfigCenter::parse_config_response("timeout=30s"),
            Some("timeout=30s".to_string())
        );

        // JSON without value field
        assert_eq!(
            ApolloConfigCenter::parse_config_response(r#"{"appId":"my-app"}"#),
            Some(r#"{"appId":"my-app"}"#.to_string())
        );
    }

    #[test]
    fn test_apollo_config_center_extract_meta_server_url() {
        let mut url = URL::new("apollo", "");
        url.ip = "http://apollo.example.com:8080".to_string();

        let extracted = ApolloConfigCenter::extract_meta_server_url(&url);
        assert_eq!(extracted, "http://apollo.example.com:8080");
    }

    #[test]
    fn test_apollo_config_center_auth_header() {
        // Without token
        let cc_no_auth = ApolloConfigCenterBuilder::new()
            .meta_server_url("http://localhost:8080")
            .app_id("app")
            .build()
            .expect("build should succeed");
        assert!(cc_no_auth.build_auth_header().is_empty());

        // With token
        let cc_auth = ApolloConfigCenterBuilder::new()
            .meta_server_url("http://localhost:8080")
            .app_id("app")
            .token("my-secret-token")
            .build()
            .expect("build should succeed");
        assert_eq!(cc_auth.build_auth_header(), "my-secret-token");
    }

    #[test]
    fn test_apollo_config_center_destroy() {
        let cc = ApolloConfigCenterBuilder::new()
            .meta_server_url("http://localhost:8080")
            .app_id("app")
            .build()
            .expect("build should succeed");

        let listener = Arc::new(TestConfigListener::new());
        let lk = ApolloConfigCenter::listener_key("test.key");
        cc.listeners
            .write()
            .unwrap()
            .insert(lk.clone(), vec![listener]);

        assert!(!cc.listeners.read().unwrap().is_empty());

        cc.destroy();

        assert!(cc.listeners.read().unwrap().is_empty());
        assert!(cc.poll_handles.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_apollo_config_center_register_ok() {
        let cc = ApolloConfigCenterBuilder::new()
            .meta_server_url("http://localhost:8080")
            .app_id("app")
            .build()
            .expect("build should succeed");

        let result = cc.register("key1".to_string(), "group1".to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_apollo_config_center_unregister_ok() {
        let cc = ApolloConfigCenterBuilder::new()
            .meta_server_url("http://localhost:8080")
            .app_id("app")
            .build()
            .expect("build should succeed");

        let result = cc
            .unregister("key1".to_string(), "group1".to_string())
            .await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_apollo_config_center_multiple_watchers() {
        let cc = ApolloConfigCenterBuilder::new()
            .meta_server_url("http://localhost:8080")
            .app_id("app")
            .build()
            .expect("build should succeed");

        let l1 = Arc::new(TestConfigListener::new());
        let l2 = Arc::new(TestConfigListener::new());

        let lk = ApolloConfigCenter::listener_key("app.timeout");

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
    fn test_apollo_config_center_default_constants() {
        assert_eq!(DEFAULT_CLUSTER, "default");
        assert_eq!(DEFAULT_NAMESPACE, "application");
        assert_eq!(POLL_INTERVAL_SECS, 30);
    }
}
