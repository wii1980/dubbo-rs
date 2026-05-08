pub use dubbo_rs_common;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;

/// Type of configuration change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigChangeType {
    /// New config key created.
    Created,
    /// Existing config value modified.
    Modified,
    /// Config key deleted.
    Deleted,
}

/// Event carrying details of a configuration change.
///
/// Delivered to [`ConfigListener`]s when a watched key is created,
/// modified, or deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigChangeEvent {
    /// The config key that changed.
    pub key: String,
    /// Previous value, if any.  `None` for `Created`.
    pub old_value: Option<String>,
    /// New value after change, if any.  `None` for `Deleted`.
    pub new_value: Option<String>,
    /// Type of change.
    pub change_type: ConfigChangeType,
}

impl ConfigChangeEvent {
    /// Create a new `ConfigChangeEvent`.
    #[must_use]
    pub fn new(
        key: impl Into<String>,
        old_value: Option<impl Into<String>>,
        new_value: Option<impl Into<String>>,
        change_type: ConfigChangeType,
    ) -> Self {
        Self {
            key: key.into(),
            old_value: old_value.map(Into::into),
            new_value: new_value.map(Into::into),
            change_type,
        }
    }
}

/// Listener notified when a watched configuration key changes.
///
/// Implementors should be cheaply clonable (typically wrapped in
/// `Arc`) since config centers may hold multiple references.
#[async_trait]
pub trait ConfigListener: Send + Sync {
    /// Called by the config center when a watched key changes.
    async fn on_change(&self, event: ConfigChangeEvent);
}

/// Abstraction for dynamic configuration centers.
///
/// Config centers are responsible for:
/// - **Registration**: Publish interest in config keys.
/// - **Discovery**: Watch keys and notify listeners on changes.
/// - **Lifecycle**: Manage connections to backend config stores.
///
/// Supported backends: Nacos, Apollo, ZooKeeper, local files.
#[async_trait]
pub trait ConfigCenter: Node + Send + Sync {
    /// Register a configuration key under the given group.
    ///
    /// Backends may create a node or record interest in the key.
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if registration fails.
    async fn register(&self, key: String, group: String) -> Result<(), RPCError>;

    /// Unregister a previously registered configuration key.
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if unregistration fails.
    async fn unregister(&self, key: String, group: String) -> Result<(), RPCError>;

    /// Watch a configuration key for changes.
    ///
    /// The `listener` will be called whenever the specified key's value
    /// is created, modified, or deleted.  If multiple listeners are
    /// registered for the same key, all are notified (sequentially).
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if the watch request fails.
    async fn watch(
        &self,
        key: String,
        group: String,
        listener: Arc<dyn ConfigListener>,
    ) -> Result<(), RPCError>;
}

// ---------------------------------------------------------------------------
// DynamicConfiguration — in-memory implementation
// ---------------------------------------------------------------------------

/// In-memory dynamic configuration store.
///
/// Thread-safe key-value store backed by `RwLock<HashMap>` that supports
/// get/set/remove operations and listener notification.  Suitable for
/// testing and single-node deployments.
pub struct DynamicConfiguration {
    url: URL,
    store: RwLock<HashMap<String, String>>,
    listeners: RwLock<HashMap<String, Vec<Arc<dyn ConfigListener>>>>,
}

/// Builder for [`DynamicConfiguration`].
#[derive(Default)]
pub struct DynamicConfigurationBuilder {
    url: Option<URL>,
}

impl DynamicConfigurationBuilder {
    /// Create a new builder with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self { url: None }
    }

    /// Set the URL for the configuration center.
    #[must_use]
    pub fn with_url(mut self, url: URL) -> Self {
        self.url = Some(url);
        self
    }

    /// Build the [`DynamicConfiguration`].
    ///
    /// # Panics
    ///
    /// Never — missing fields fall back to defaults.
    #[must_use]
    pub fn build(self) -> DynamicConfiguration {
        DynamicConfiguration {
            url: self.url.unwrap_or_default(),
            store: RwLock::new(HashMap::new()),
            listeners: RwLock::new(HashMap::new()),
        }
    }
}

impl DynamicConfiguration {
    /// Create a new builder.
    #[must_use]
    pub fn builder() -> DynamicConfigurationBuilder {
        DynamicConfigurationBuilder::new()
    }

    /// Get a configuration value by key.
    ///
    /// Returns `None` if the key does not exist.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<String> {
        self.store.read().unwrap().get(key).cloned()
    }

    /// Set a configuration value for a key.
    ///
    /// Creates the key if it does not exist; modifies it otherwise.
    /// Notifies registered [`ConfigListener`]s after the value is stored.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    pub async fn set(&self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let new_value = value.into();

        let (old_value, change_type) = {
            let mut store = self.store.write().unwrap();
            let old = store.get(&key).cloned();
            let ct = if old.is_some() {
                ConfigChangeType::Modified
            } else {
                ConfigChangeType::Created
            };
            store.insert(key.clone(), new_value.clone());
            (old, ct)
        };

        self.notify_listeners(&key, old_value, Some(new_value), change_type)
            .await;
    }

    /// Remove a configuration key.
    ///
    /// Returns the previous value if the key existed.
    /// Notifies registered [`ConfigListener`]s with `ConfigChangeType::Deleted`.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    pub async fn remove(&self, key: &str) -> Option<String> {
        let old_value = self.store.write().unwrap().remove(key);

        if let Some(ref old) = old_value {
            self.notify_listeners(key, Some(old.clone()), None, ConfigChangeType::Deleted)
                .await;
        }

        old_value
    }

    /// Get all configuration entries belonging to a group.
    ///
    /// Keys are matched by the dotted prefix `"{group}."`.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn get_configs_by_group(&self, group: &str) -> HashMap<String, String> {
        let prefix = format!("{group}.");
        self.store
            .read()
            .unwrap()
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Return the URL associated with this configuration center.
    #[must_use]
    pub fn url(&self) -> &URL {
        &self.url
    }

    async fn notify_listeners(
        &self,
        key: &str,
        old_value: Option<String>,
        new_value: Option<String>,
        change_type: ConfigChangeType,
    ) {
        let snapshot: Vec<Arc<dyn ConfigListener>> = {
            self.listeners
                .read()
                .unwrap()
                .get(key)
                .cloned()
                .unwrap_or_default()
        };

        if snapshot.is_empty() {
            return;
        }

        let event = ConfigChangeEvent {
            key: key.to_string(),
            old_value,
            new_value,
            change_type,
        };

        for listener in snapshot {
            listener.on_change(event.clone()).await;
        }
    }

    fn add_listener(&self, key: String, listener: Arc<dyn ConfigListener>) {
        self.listeners
            .write()
            .unwrap()
            .entry(key)
            .or_default()
            .push(listener);
    }
}

impl Node for DynamicConfiguration {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {
        self.listeners.write().unwrap().clear();
        self.store.write().unwrap().clear();
    }
}

#[async_trait]
impl ConfigCenter for DynamicConfiguration {
    async fn register(&self, _key: String, _group: String) -> Result<(), RPCError> {
        Ok(())
    }

    async fn unregister(&self, _key: String, _group: String) -> Result<(), RPCError> {
        Ok(())
    }

    async fn watch(
        &self,
        key: String,
        _group: String,
        listener: Arc<dyn ConfigListener>,
    ) -> Result<(), RPCError> {
        self.add_listener(key, listener);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// URL extension trait — config center metadata
// ---------------------------------------------------------------------------
/// Extension trait for [`URL`] providing config center metadata parsing.
pub trait ConfigCenterUrlExt {
    /// Get config center group from URL params (default: `"dubbo"`).
    #[must_use]
    fn get_config_center_group(&self) -> String;

    /// Get config center namespace from URL params (default: `""`).
    #[must_use]
    fn get_config_center_namespace(&self) -> String;

    /// Get config center timeout from URL params in milliseconds (default: `3000`).
    #[must_use]
    fn get_config_center_timeout(&self) -> u64;

    /// Get the config center data ID from the URL path.
    #[must_use]
    fn get_config_center_data_id(&self) -> String;
}

impl ConfigCenterUrlExt for URL {
    fn get_config_center_group(&self) -> String {
        self.get_param_or_default("group", "dubbo")
    }

    fn get_config_center_namespace(&self) -> String {
        self.get_param_or_default("namespace", "")
    }

    fn get_config_center_timeout(&self) -> u64 {
        self.get_param_or_default("timeout", "3000")
            .parse()
            .unwrap_or(3000)
    }

    fn get_config_center_data_id(&self) -> String {
        self.path.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct TestConfigListener {
        events: Mutex<Vec<ConfigChangeEvent>>,
    }

    impl TestConfigListener {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }

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
    fn test_event_created() {
        let event = ConfigChangeEvent::new(
            "app.timeout",
            Option::<&str>::None,
            Some("30s"),
            ConfigChangeType::Created,
        );
        assert_eq!(event.key, "app.timeout");
        assert_eq!(event.old_value, None);
        assert_eq!(event.new_value, Some("30s".to_string()));
        assert_eq!(event.change_type, ConfigChangeType::Created);
    }

    #[test]
    fn test_event_modified() {
        let event = ConfigChangeEvent::new(
            "app.timeout",
            Some("30s"),
            Some("60s"),
            ConfigChangeType::Modified,
        );
        assert_eq!(event.old_value, Some("30s".to_string()));
        assert_eq!(event.new_value, Some("60s".to_string()));
        assert_eq!(event.change_type, ConfigChangeType::Modified);
    }

    #[test]
    fn test_event_deleted() {
        let event = ConfigChangeEvent::new(
            "app.timeout",
            Some("60s"),
            Option::<&str>::None,
            ConfigChangeType::Deleted,
        );
        assert_eq!(event.old_value, Some("60s".to_string()));
        assert_eq!(event.new_value, None);
        assert_eq!(event.change_type, ConfigChangeType::Deleted);
    }

    #[test]
    fn test_event_clone_eq() {
        let a = ConfigChangeEvent::new(
            "db.host",
            Some("localhost"),
            Some("10.0.0.1"),
            ConfigChangeType::Modified,
        );
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn test_get_set_remove() {
        let dc = DynamicConfiguration::builder().build();

        assert_eq!(dc.get("my.key"), None);

        dc.set("my.key", "hello").await;
        assert_eq!(dc.get("my.key"), Some("hello".to_string()));

        dc.set("my.key", "world").await;
        assert_eq!(dc.get("my.key"), Some("world".to_string()));

        let old = dc.remove("my.key").await;
        assert_eq!(old, Some("world".to_string()));
        assert_eq!(dc.get("my.key"), None);

        let none = dc.remove("no.such.key").await;
        assert_eq!(none, None);
    }

    #[test]
    fn test_get_configs_by_group() {
        let dc = DynamicConfiguration::builder().build();
        dc.store
            .write()
            .unwrap()
            .insert("default.key1".into(), "v1".into());
        dc.store
            .write()
            .unwrap()
            .insert("default.key2".into(), "v2".into());
        dc.store
            .write()
            .unwrap()
            .insert("other.key3".into(), "v3".into());

        let group = dc.get_configs_by_group("default");
        assert_eq!(group.len(), 2);
        assert_eq!(group.get("default.key1"), Some(&"v1".to_string()));
        assert_eq!(group.get("default.key2"), Some(&"v2".to_string()));
    }

    #[test]
    fn test_builder_with_url() {
        let url = URL::new("nacos", "/config");
        let dc = DynamicConfiguration::builder()
            .with_url(url.clone())
            .build();
        assert_eq!(dc.url().protocol, "nacos");
        assert_eq!(dc.url().path, "/config");
        assert!(dc.is_available());
    }

    #[test]
    fn test_builder_default_url() {
        let dc = DynamicConfiguration::builder().build();
        assert_eq!(dc.url().protocol, "");
    }

    #[tokio::test]
    async fn test_listener_notification() {
        let dc = DynamicConfiguration::builder().build();
        let listener = Arc::new(TestConfigListener::new());

        dc.watch("app.timeout".into(), "default".into(), listener.clone())
            .await
            .unwrap();

        dc.set("app.timeout", "10s").await;
        {
            let events = listener.events();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].key, "app.timeout");
            assert_eq!(events[0].change_type, ConfigChangeType::Created);
            assert_eq!(events[0].new_value, Some("10s".to_string()));
        }

        dc.set("app.timeout", "20s").await;
        {
            let events = listener.events();
            assert_eq!(events.len(), 2);
            assert_eq!(events[1].change_type, ConfigChangeType::Modified);
            assert_eq!(events[1].old_value, Some("10s".to_string()));
            assert_eq!(events[1].new_value, Some("20s".to_string()));
        }

        dc.remove("app.timeout").await;
        {
            let events = listener.events();
            assert_eq!(events.len(), 3);
            assert_eq!(events[2].change_type, ConfigChangeType::Deleted);
            assert_eq!(events[2].old_value, Some("20s".to_string()));
            assert_eq!(events[2].new_value, None);
        }
    }

    #[tokio::test]
    async fn test_multiple_listeners_same_key() {
        let dc = DynamicConfiguration::builder().build();
        let a = Arc::new(TestConfigListener::new());
        let b = Arc::new(TestConfigListener::new());

        dc.watch("shared.key".into(), "default".into(), a.clone())
            .await
            .unwrap();
        dc.watch("shared.key".into(), "default".into(), b.clone())
            .await
            .unwrap();

        dc.set("shared.key", "val").await;

        assert_eq!(a.events().len(), 1);
        assert_eq!(b.events().len(), 1);
    }

    #[test]
    fn test_url_config_center_defaults() {
        let url = URL::new("nacos", "/config");
        assert_eq!(url.get_config_center_group(), "dubbo");
        assert_eq!(url.get_config_center_namespace(), "");
        assert_eq!(url.get_config_center_timeout(), 3000);
        assert_eq!(url.get_config_center_data_id(), "/config");
    }

    #[test]
    fn test_url_config_center_params() {
        let mut url = URL::new("nacos", "/myapp");
        url.set_param("group", "mygroup");
        url.set_param("namespace", "dev");
        url.set_param("timeout", "5000");

        assert_eq!(url.get_config_center_group(), "mygroup");
        assert_eq!(url.get_config_center_namespace(), "dev");
        assert_eq!(url.get_config_center_timeout(), 5000);
    }

    #[tokio::test]
    async fn test_config_center_register_unregister_ok() {
        let dc = DynamicConfiguration::builder().build();
        assert!(dc.register("key1".into(), "group1".into()).await.is_ok());
        assert!(dc.unregister("key1".into(), "group1".into()).await.is_ok());
    }

    #[test]
    fn test_dynamic_config_destroy() {
        let dc = DynamicConfiguration::builder().build();
        dc.store.write().unwrap().insert("k".into(), "v".into());
        dc.destroy();
        assert_eq!(dc.get("k"), None);
    }
}
