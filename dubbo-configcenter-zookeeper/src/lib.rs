pub use dubbo_rs_common;
pub use dubbo_rs_configcenter;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_configcenter::{ConfigCenter, ConfigChangeEvent, ConfigChangeType, ConfigListener};
use zookeeper::{Acl, CreateMode, WatchedEvent, Watcher, ZooKeeper};

const DEFAULT_ROOT_PATH: &str = "/dubbo";
const DEFAULT_SESSION_TIMEOUT_MS: u64 = 60_000;

type ListenerMap = Arc<RwLock<HashMap<String, Vec<Arc<dyn ConfigListener>>>>>;

/// `ZooKeeper`-backed configuration center.
///
/// Stores configuration values as persistent znodes under
/// `/dubbo/config/{group}/{key}` and supports watching keys for
/// changes.
pub struct ZookeeperConfigCenter {
    url: URL,
    zk: RwLock<Option<Arc<ZooKeeper>>>,
    root_path: String,
    session_timeout: Duration,
    listeners: ListenerMap,
}

impl ZookeeperConfigCenter {
    /// Create a new [`ZookeeperConfigCenterBuilder`].
    #[must_use]
    pub fn builder() -> ZookeeperConfigCenterBuilder {
        ZookeeperConfigCenterBuilder::new()
    }

    /// Generate the `ZooKeeper` path for a configuration key.
    ///
    /// Format: `{root_path}/config/{group}/{key}`
    fn config_path(&self, group: &str, key: &str) -> String {
        format!("{}/config/{}/{}", self.root_path, group, key)
    }

    /// Lazily establish a `ZooKeeper` connection.
    ///
    /// Uses the URL's `ip:port` as the ZK connection string.
    fn ensure_connection(&self) -> Result<(), RPCError> {
        if self.zk.read().unwrap().is_some() {
            return Ok(());
        }

        let addr = format!("{}:{}", self.url.ip, self.url.port);
        let zk = ZooKeeper::connect(&addr, self.session_timeout, NoopWatcher)
            .map_err(|e| RPCError::ServerError(format!("ZK connect failed: {e}")))?;

        *self.zk.write().unwrap() = Some(Arc::new(zk));
        Ok(())
    }
}

/// Builder for [`ZookeeperConfigCenter`].
///
/// # Examples
///
/// ```rust
/// use dubbo_rs_common::url::URL;
/// use dubbo_rs_configcenter_zookeeper::ZookeeperConfigCenter;
///
/// let mut url = URL::new("zookeeper", "/dubbo/config");
/// url.ip = "127.0.0.1".into();
/// url.port = "2181".into();
///
/// let cc = ZookeeperConfigCenter::builder()
///     .with_url(url)
///     .build();
/// ```
pub struct ZookeeperConfigCenterBuilder {
    url: Option<URL>,
    root_path: Option<String>,
    session_timeout: Option<Duration>,
}

impl ZookeeperConfigCenterBuilder {
    /// Create a new builder with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            url: None,
            root_path: None,
            session_timeout: None,
        }
    }

    /// Set the `ZooKeeper` connection URL.
    #[must_use]
    pub fn with_url(mut self, url: URL) -> Self {
        self.url = Some(url);
        self
    }

    /// Override the root path prefix (default: `"/dubbo"`).
    #[must_use]
    pub fn with_root_path(mut self, path: impl Into<String>) -> Self {
        self.root_path = Some(path.into());
        self
    }

    /// Override the `ZooKeeper` session timeout (default: 60 seconds).
    #[must_use]
    pub fn with_session_timeout(mut self, timeout: Duration) -> Self {
        self.session_timeout = Some(timeout);
        self
    }

    /// Build the [`ZookeeperConfigCenter`].
    ///
    /// # Panics
    ///
    /// Never — missing fields fall back to defaults.
    #[must_use]
    pub fn build(self) -> ZookeeperConfigCenter {
        ZookeeperConfigCenter {
            url: self.url.unwrap_or_default(),
            zk: RwLock::new(None),
            root_path: self
                .root_path
                .unwrap_or_else(|| DEFAULT_ROOT_PATH.to_string()),
            session_timeout: self
                .session_timeout
                .unwrap_or(Duration::from_millis(DEFAULT_SESSION_TIMEOUT_MS)),
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for ZookeeperConfigCenterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Node for ZookeeperConfigCenter {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        self.zk.read().unwrap().is_some()
    }

    fn destroy(&self) {
        self.listeners.write().unwrap().clear();
        *self.zk.write().unwrap() = None;
    }
}

#[async_trait]
impl ConfigCenter for ZookeeperConfigCenter {
    async fn register(&self, key: String, group: String) -> Result<(), RPCError> {
        self.ensure_connection()?;
        let zk_guard = self.zk.read().unwrap();
        let zk = zk_guard
            .as_ref()
            .ok_or_else(|| RPCError::ServerError("ZK not connected".to_string()))?;

        let path = self.config_path(&group, &key);
        ensure_parent_paths(zk, &path);

        let acls = Acl::open_unsafe().to_owned();
        zk.create(&path, vec![], acls, CreateMode::Persistent)
            .map_err(|e| RPCError::ServerError(format!("ZK create failed: {e}")))?;

        Ok(())
    }

    async fn unregister(&self, key: String, group: String) -> Result<(), RPCError> {
        self.ensure_connection()?;
        let zk_guard = self.zk.read().unwrap();
        let zk = zk_guard
            .as_ref()
            .ok_or_else(|| RPCError::ServerError("ZK not connected".to_string()))?;

        let path = self.config_path(&group, &key);
        zk.delete(&path, None)
            .map_err(|e| RPCError::ServerError(format!("ZK delete failed: {e}")))?;
        self.listeners.write().unwrap().remove(&path);

        Ok(())
    }

    async fn watch(
        &self,
        key: String,
        group: String,
        listener: Arc<dyn ConfigListener>,
    ) -> Result<(), RPCError> {
        self.ensure_connection()?;
        let zk_guard = self.zk.read().unwrap();
        let zk = zk_guard
            .as_ref()
            .ok_or_else(|| RPCError::ServerError("ZK not connected".to_string()))?;

        let path = self.config_path(&group, &key);

        self.listeners
            .write()
            .unwrap()
            .entry(path.clone())
            .or_default()
            .push(listener);

        let watcher = ConfigWatcher {
            path: path.clone(),
            key: key.clone(),
            zk: Arc::clone(zk),
            listeners: Arc::clone(&self.listeners),
        };

        if let Ok((data, _stat)) = zk.get_data_w(&path, watcher) {
            let value = String::from_utf8_lossy(&data).to_string();
            notify_listeners(
                &self.listeners,
                &path,
                &key,
                None,
                Some(value),
                ConfigChangeType::Created,
            );
        }

        Ok(())
    }
}

fn notify_listeners(
    listeners: &RwLock<HashMap<String, Vec<Arc<dyn ConfigListener>>>>,
    path: &str,
    key: &str,
    old_value: Option<String>,
    new_value: Option<String>,
    change_type: ConfigChangeType,
) {
    let snapshot: Vec<Arc<dyn ConfigListener>> = listeners
        .read()
        .unwrap()
        .get(path)
        .cloned()
        .unwrap_or_default();

    if snapshot.is_empty() {
        return;
    }

    let event = ConfigChangeEvent {
        key: key.to_string(),
        old_value,
        new_value,
        change_type,
    };

    for listener in &snapshot {
        let event = event.clone();
        let listener = Arc::clone(listener);
        tokio::spawn(async move {
            listener.on_change(event).await;
        });
    }
}

/// Ensure all ancestor paths of `path` exist in `ZooKeeper`.
///
/// Creates missing path segments with `CreateMode::Persistent` and
/// open ACLs.  Already-existing segments are silently skipped.
fn ensure_parent_paths(zk: &ZooKeeper, path: &str) {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let parents = if segments.len() > 1 {
        &segments[..segments.len() - 1]
    } else {
        return;
    };

    let acls = Acl::open_unsafe().to_owned();
    let mut current = String::new();
    for segment in parents {
        current.push('/');
        current.push_str(segment);
        let _ = zk.create(&current, vec![], acls.clone(), CreateMode::Persistent);
    }
}

/// Default no-op watcher used for connection establishment.
struct NoopWatcher;

impl Watcher for NoopWatcher {
    fn handle(&self, _event: WatchedEvent) {}
}

/// `ZooKeeper` watcher that dispatches config change events to registered
/// [`ConfigListener`]s.
///
/// Each watched key gets its own `ConfigWatcher` instance.  On
/// receiving a ZK event the watcher re-reads the node data (or
/// detects deletion) and fans out the change to every listener.
struct ConfigWatcher {
    path: String,
    key: String,
    zk: Arc<ZooKeeper>,
    listeners: ListenerMap,
}

impl Watcher for ConfigWatcher {
    fn handle(&self, event: WatchedEvent) {
        let path = self.path.clone();
        let key = self.key.clone();
        let zk = Arc::clone(&self.zk);
        let listeners = Arc::clone(&self.listeners);

        tokio::spawn(async move {
            match event.event_type {
                zookeeper::WatchedEventType::NodeDeleted => {
                    notify_listeners(
                        &listeners,
                        &path,
                        &key,
                        None,
                        None,
                        ConfigChangeType::Deleted,
                    );
                    listeners.write().unwrap().remove(&path);
                }
                zookeeper::WatchedEventType::NodeDataChanged => {
                    if let Ok((data, _stat)) = zk.get_data(&path, false) {
                        let new_value = String::from_utf8_lossy(&data).to_string();
                        notify_listeners(
                            &listeners,
                            &path,
                            &key,
                            None,
                            Some(new_value),
                            ConfigChangeType::Modified,
                        );
                    } else {
                        notify_listeners(
                            &listeners,
                            &path,
                            &key,
                            None,
                            None,
                            ConfigChangeType::Deleted,
                        );
                        listeners.write().unwrap().remove(&path);
                    }
                }
                zookeeper::WatchedEventType::NodeCreated => {
                    if let Ok((data, _stat)) = zk.get_data(&path, false) {
                        let new_value = String::from_utf8_lossy(&data).to_string();
                        notify_listeners(
                            &listeners,
                            &path,
                            &key,
                            None,
                            Some(new_value),
                            ConfigChangeType::Created,
                        );
                    }
                }
                _ => {}
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_zk_url() -> URL {
        let mut url = URL::new("zookeeper", "/dubbo/config");
        url.ip = "127.0.0.1".into();
        url.port = "2181".into();
        url
    }

    #[test]
    fn test_builder_defaults() {
        let cc = ZookeeperConfigCenter::builder().build();

        assert_eq!(cc.root_path, DEFAULT_ROOT_PATH);
        assert_eq!(
            cc.session_timeout,
            Duration::from_millis(DEFAULT_SESSION_TIMEOUT_MS)
        );
        assert!(
            !cc.is_available(),
            "should not be connected without explicit connect"
        );
    }

    #[test]
    fn test_builder_with_url() {
        let url = make_zk_url();
        let cc = ZookeeperConfigCenter::builder()
            .with_url(url.clone())
            .build();

        assert_eq!(cc.get_url().ip, "127.0.0.1");
        assert_eq!(cc.get_url().port, "2181");
        assert_eq!(cc.get_url().protocol, "zookeeper");
    }

    #[test]
    fn test_builder_with_custom_root_path() {
        let cc = ZookeeperConfigCenter::builder()
            .with_url(make_zk_url())
            .with_root_path("/custom")
            .build();

        assert_eq!(cc.root_path, "/custom");
    }

    #[test]
    fn test_builder_with_session_timeout() {
        let timeout = Duration::from_secs(30);
        let cc = ZookeeperConfigCenter::builder()
            .with_url(make_zk_url())
            .with_session_timeout(timeout)
            .build();

        assert_eq!(cc.session_timeout, timeout);
    }

    #[test]
    fn test_builder_chained() {
        let timeout = Duration::from_millis(30_000);
        let cc = ZookeeperConfigCenter::builder()
            .with_url(make_zk_url())
            .with_root_path("/mydubbo")
            .with_session_timeout(timeout)
            .build();

        assert_eq!(cc.root_path, "/mydubbo");
        assert_eq!(cc.session_timeout, timeout);
        assert_eq!(cc.get_url().ip, "127.0.0.1");
    }

    #[test]
    fn test_builder_default_impl() {
        let builder = ZookeeperConfigCenterBuilder::default();
        let cc = builder.build();
        assert_eq!(cc.root_path, DEFAULT_ROOT_PATH);
    }

    #[test]
    fn test_config_path_default_root() {
        let cc = ZookeeperConfigCenter::builder()
            .with_url(make_zk_url())
            .build();

        assert_eq!(
            cc.config_path("mygroup", "mykey"),
            "/dubbo/config/mygroup/mykey"
        );
    }

    #[test]
    fn test_config_path_custom_root() {
        let cc = ZookeeperConfigCenter::builder()
            .with_url(make_zk_url())
            .with_root_path("/app")
            .build();

        assert_eq!(cc.config_path("grp", "k"), "/app/config/grp/k");
    }

    #[test]
    fn test_config_path_no_special_chars() {
        let cc = ZookeeperConfigCenter::builder()
            .with_url(make_zk_url())
            .build();

        let path = cc.config_path("com.example", "timeout.ms");
        assert_eq!(path, "/dubbo/config/com.example/timeout.ms");
    }

    #[test]
    fn test_config_path_empty_group() {
        let cc = ZookeeperConfigCenter::builder()
            .with_url(make_zk_url())
            .build();

        assert_eq!(cc.config_path("", "key"), "/dubbo/config//key");
    }

    #[test]
    fn test_connection_addr_format() {
        let mut url = make_zk_url();
        url.ip = "10.0.0.1".into();
        url.port = "2189".into();

        let cc = ZookeeperConfigCenter::builder().with_url(url).build();

        assert_eq!(cc.get_url().ip, "10.0.0.1");
        assert_eq!(cc.get_url().port, "2189");
    }

    #[test]
    fn test_is_available_before_connect() {
        let cc = ZookeeperConfigCenter::builder()
            .with_url(make_zk_url())
            .build();

        assert!(!cc.is_available());
    }

    #[test]
    fn test_destroy_clears_state() {
        let cc = ZookeeperConfigCenter::builder()
            .with_url(make_zk_url())
            .build();

        {
            let mut listeners = cc.listeners.write().unwrap();
            listeners.insert("/dubbo/config/g/k".into(), Vec::new());
        }

        cc.destroy();

        assert!(!cc.is_available());
        assert!(cc.listeners.read().unwrap().is_empty());
    }

    #[test]
    fn test_get_url_returns_reference() {
        let url = make_zk_url();
        let cc = ZookeeperConfigCenter::builder()
            .with_url(url.clone())
            .build();

        assert_eq!(cc.get_url(), &url);
    }
}
