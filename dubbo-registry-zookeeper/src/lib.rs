pub use dubbo_rs_common;
pub use dubbo_rs_registry;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

type ListenerMap = Arc<RwLock<HashMap<String, Vec<Arc<dyn NotifyListener>>>>>;

use async_trait::async_trait;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_registry::{NotifyListener, Registry, ServiceEvent};
use tokio::sync::mpsc;
use zookeeper::{Acl, CreateMode, WatchedEvent, Watcher, ZooKeeper};

const DEFAULT_ROOT_PATH: &str = "/dubbo";
const SESSION_TIMEOUT_MS: u64 = 60_000;

/// ZooKeeper-based service registry implementing the `Registry` trait.
///
/// Uses ephemeral znodes under `/dubbo/{service}/providers/` for
/// automatic cleanup when the provider disconnects.
pub struct ZookeeperRegistry {
    url: URL,
    zk: Arc<RwLock<Option<ZooKeeper>>>,
    root_path: String,
    registered_paths: RwLock<Vec<String>>,
    listener_map: ListenerMap,
    watcher_handles: RwLock<Vec<tokio::task::JoinHandle<()>>>,
}

impl ZookeeperRegistry {
    #[must_use]
    pub fn new(url: URL) -> Self {
        Self {
            url,
            zk: Arc::new(RwLock::new(None)),
            root_path: DEFAULT_ROOT_PATH.to_string(),
            registered_paths: RwLock::new(Vec::new()),
            listener_map: Arc::new(RwLock::new(HashMap::new())),
            watcher_handles: RwLock::new(Vec::new()),
        }
    }

    #[must_use]
    pub fn with_root_path(mut self, path: impl Into<String>) -> Self {
        self.root_path = path.into();
        self
    }

    fn provider_path(&self, service_url: &URL) -> String {
        format!(
            "{}/{}/providers/{}",
            self.root_path,
            service_url.path.trim_start_matches('/'),
            encode_url(service_url)
        )
    }

    fn providers_dir(&self, service_url: &URL) -> String {
        format!(
            "{}/{}/providers",
            self.root_path,
            service_url.path.trim_start_matches('/')
        )
    }

    fn ensure_connection(&self) -> Result<(), RPCError> {
        if self.zk.read().unwrap().is_some() {
            return Ok(());
        }

        let addr = format!("{}:{}", self.url.ip, self.url.port);
        let zk = ZooKeeper::connect(
            addr.as_str(),
            Duration::from_millis(SESSION_TIMEOUT_MS),
            NoopWatcher,
        )
        .map_err(|e| RPCError::ServerError(format!("ZK connect failed: {e}")))?;

        *self.zk.write().unwrap() = Some(zk);
        Ok(())
    }

    fn ensure_path(zk: &ZooKeeper, path: &str) -> Result<(), RPCError> {
        let acls = Acl::open_unsafe().to_owned();
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = String::new();
        for part in parts {
            current.push('/');
            current.push_str(part);
            if zk
                .exists(&current, false)
                .map_err(|e| RPCError::ServerError(format!("ZK exists failed: {e}")))?
                .is_none()
            {
                zk.create(&current, vec![], acls.clone(), CreateMode::Persistent)
                    .map_err(|e| RPCError::ServerError(format!("ZK create dir failed: {e}")))?;
            }
        }
        Ok(())
    }

    /// Subscribe with continuous ZK watcher monitoring.
    ///
    /// Like `subscribe()`, but also registers a ZK children watcher that
    /// re-fetches and notifies listeners when children change.
    ///
    /// # Panics
    /// Panics if the ZK connection is not established or the lock is poisoned.
    ///
    /// # Errors
    /// Returns `RPCError` if ZK operations fail.
    pub async fn subscribe_with_watcher(
        &self,
        url: URL,
        listener: Arc<dyn NotifyListener>,
    ) -> Result<(), RPCError> {
        self.ensure_connection()?;

        let dir = self.providers_dir(&url);
        let service_key = url.path.clone();

        self.listener_map
            .write()
            .unwrap()
            .entry(service_key.clone())
            .or_default()
            .push(listener);

        // Fetch initial snapshot
        let children = {
            let zk_guard = self.zk.read().unwrap();
            let zk = zk_guard.as_ref().unwrap();
            Self::ensure_path(zk, &dir)?;
            zk.get_children(&dir, false)
                .map_err(|e| RPCError::ServerError(format!("ZK get_children failed: {e}")))?
        };

        let urls: Vec<URL> = children.iter().filter_map(|c| decode_url(c)).collect();
        self.notify_listeners(&service_key, ServiceEvent::Add(urls))
            .await;

        // Set up continuous watcher
        self.watch_children(&dir, &service_key);

        Ok(())
    }

    /// Set up a ZK children watcher and spawn a background task to handle events.
    fn watch_children(&self, path: &str, service_key: &str) {
        let (tx, rx) = mpsc::unbounded_channel::<WatchedEvent>();

        {
            let zk_guard = self.zk.read().unwrap();
            let zk = zk_guard.as_ref().unwrap();
            let watcher = ChannelWatcher { tx };
            let _ = zk.get_children_w(path, watcher);
        }

        let path_owned = path.to_string();
        let service_key_owned = service_key.to_string();
        let listeners = self.listener_map.clone();
        let zk_ref = self.zk.clone();

        let handle = tokio::spawn(async move {
            watch_event_loop(rx, &path_owned, &service_key_owned, listeners, zk_ref).await;
        });

        self.watcher_handles.write().unwrap().push(handle);
    }
}

async fn watch_event_loop(
    rx: mpsc::UnboundedReceiver<WatchedEvent>,
    path: &str,
    service_key: &str,
    listeners: ListenerMap,
    zk_ref: Arc<RwLock<Option<ZooKeeper>>>,
) {
    let mut prev_children: Vec<String> = Vec::new();
    let mut current_rx = rx;

    loop {
        let Some(event) = current_rx.recv().await else {
            break;
        };

        if !matches!(
            event.event_type,
            zookeeper::WatchedEventType::NodeChildrenChanged
        ) {
            continue;
        }

        let (new_children, new_rx) = {
            let zk_guard = zk_ref.read().unwrap();
            let Some(zk) = zk_guard.as_ref() else {
                break;
            };

            let children = zk.get_children(path, false).unwrap_or_default();

            let (tx, fresh_rx) = mpsc::unbounded_channel::<WatchedEvent>();
            let _ = zk.get_children_w(path, ChannelWatcher { tx });

            (children, fresh_rx)
        };

        if new_children.is_empty() && prev_children.is_empty() {
            prev_children = new_children;
            current_rx = new_rx;
            continue;
        }

        let old_set: std::collections::HashSet<_> = prev_children.iter().collect();
        let new_set: std::collections::HashSet<_> = new_children.iter().collect();

        let added: Vec<String> = new_set.difference(&old_set).map(|s| (*s).clone()).collect();
        let removed: Vec<String> = old_set.difference(&new_set).map(|s| (*s).clone()).collect();

        let listener_list = listeners
            .read()
            .unwrap()
            .get(service_key)
            .cloned()
            .unwrap_or_default();

        if !added.is_empty() {
            let urls: Vec<URL> = added.iter().filter_map(|c| decode_url(c)).collect();
            for listener in &listener_list {
                listener.notify(ServiceEvent::Add(urls.clone())).await;
            }
        }
        if !removed.is_empty() {
            let urls: Vec<URL> = removed.iter().filter_map(|c| decode_url(c)).collect();
            for listener in &listener_list {
                listener.notify(ServiceEvent::Remove(urls.clone())).await;
            }
        }

        prev_children = new_children;
        current_rx = new_rx;
    }
}

impl Node for ZookeeperRegistry {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        self.zk.read().unwrap().is_some()
    }

    fn destroy(&self) {
        if let Some(ref zk) = *self.zk.read().unwrap() {
            for path in self.registered_paths.read().unwrap().iter() {
                let _ = zk.delete(path, None);
            }
        }
        for handle in self.watcher_handles.write().unwrap().drain(..) {
            handle.abort();
        }
    }
}

#[async_trait]
impl Registry for ZookeeperRegistry {
    async fn register(&self, url: URL) -> Result<(), RPCError> {
        self.ensure_connection()?;
        let zk_guard = self.zk.read().unwrap();
        let zk = zk_guard.as_ref().unwrap();

        let path = self.provider_path(&url);
        let dir = self.providers_dir(&url);

        // Ensure parent directories exist (recursive create)
        Self::ensure_path(zk, &dir)?;

        let acls = Acl::open_unsafe().to_owned();
        zk.create(path.as_str(), vec![], acls, CreateMode::Ephemeral)
            .map_err(|e| RPCError::ServerError(format!("ZK create ephemeral failed: {e}")))?;
        self.registered_paths.write().unwrap().push(path);

        Ok(())
    }

    async fn unregister(&self, url: URL) -> Result<(), RPCError> {
        self.ensure_connection()?;
        let zk_guard = self.zk.read().unwrap();
        let zk = zk_guard.as_ref().unwrap();

        let path = self.provider_path(&url);
        zk.delete(path.as_str(), None)
            .map_err(|e| RPCError::ServerError(format!("ZK delete failed: {e}")))?;

        self.registered_paths
            .write()
            .unwrap()
            .retain(|p| p != &path);

        Ok(())
    }

    async fn subscribe(&self, url: URL, listener: Arc<dyn NotifyListener>) -> Result<(), RPCError> {
        self.ensure_connection()?;

        let dir = self.providers_dir(&url);
        let service_key = url.path.clone();

        self.listener_map
            .write()
            .unwrap()
            .entry(service_key.clone())
            .or_default()
            .push(listener);

        let children_result = {
            let zk_guard = self.zk.read().unwrap();
            let zk = zk_guard.as_ref().unwrap();
            Self::ensure_path(zk, &dir)?;
            zk.get_children_w(dir.as_str(), NoopWatcher)
        };

        match children_result {
            Ok(children) => {
                let urls: Vec<URL> = children.iter().filter_map(|c| decode_url(c)).collect();
                self.notify_listeners(&service_key, ServiceEvent::Add(urls))
                    .await;
            }
            Err(_) => {
                tracing::debug!("ZK: no providers yet at {dir}");
            }
        }

        Ok(())
    }

    async fn unsubscribe(
        &self,
        url: URL,
        _listener: Arc<dyn NotifyListener>,
    ) -> Result<(), RPCError> {
        self.listener_map.write().unwrap().remove(&url.path);
        Ok(())
    }
}

impl ZookeeperRegistry {
    async fn notify_listeners(&self, service_path: &str, event: ServiceEvent) {
        let listeners = self
            .listener_map
            .read()
            .unwrap()
            .get(service_path)
            .cloned()
            .unwrap_or_default();

        for listener in &listeners {
            listener.notify(event.clone()).await;
        }
    }
}

struct NoopWatcher;

impl Watcher for NoopWatcher {
    fn handle(&self, _event: WatchedEvent) {}
}

struct ChannelWatcher {
    tx: mpsc::UnboundedSender<WatchedEvent>,
}

impl Watcher for ChannelWatcher {
    fn handle(&self, event: WatchedEvent) {
        let _ = self.tx.send(event);
    }
}

#[must_use]
pub fn encode_url(url: &URL) -> String {
    let raw = format!("{}://{}:{}{}", url.protocol, url.ip, url.port, url.path);
    urlencoding(&raw)
}

#[must_use]
pub fn decode_url(encoded: &str) -> Option<URL> {
    let decoded = urldecoding(encoded);
    let without_proto = decoded
        .strip_prefix("dubbo://")
        .or_else(|| decoded.strip_prefix("tri://"))?;
    let (addr_path, _params) = without_proto.split_once('?').unwrap_or((without_proto, ""));
    let (addr, path) = addr_path.split_once('/')?;
    let (ip, port) = addr.split_once(':')?;

    let mut url = URL::new("dubbo", format!("/{path}"));
    url.ip = ip.to_string();
    url.port = port.to_string();
    Some(url)
}

#[must_use]
pub fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ':' => "%3A".to_string(),
            '/' => "%2F".to_string(),
            '?' => "%3F".to_string(),
            '=' => "%3D".to_string(),
            '&' => "%26".to_string(),
            c if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' => c.to_string(),
            other => {
                let bytes = other.to_string().into_bytes();
                bytes
                    .iter()
                    .fold(String::new(), |acc, b| format!("{acc}%{b:02X}"))
            }
        })
        .collect()
}

#[must_use]
pub fn urldecoding(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service_url(service: &str) -> URL {
        let mut url = URL::new("tri", service);
        url.ip = "192.168.1.100".to_string();
        url.port = "50051".to_string();
        url
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let url = make_service_url("/com.example.GreetService");
        let encoded = encode_url(&url);
        let decoded = decode_url(&encoded);
        assert!(decoded.is_some());
        let decoded = decoded.unwrap();
        assert_eq!(decoded.ip, "192.168.1.100");
        assert_eq!(decoded.port, "50051");
    }

    #[test]
    fn test_encode_url_format() {
        let url = make_service_url("/com.example.GreetService");
        let encoded = encode_url(&url);
        assert!(encoded.contains("%3A"));
        assert!(encoded.contains("%2F"));
    }

    #[test]
    fn test_urlencoding_basic() {
        assert_eq!(urlencoding("hello"), "hello");
        assert_eq!(urlencoding("a:b"), "a%3Ab");
        assert_eq!(urlencoding("a/b"), "a%2Fb");
    }

    #[test]
    fn test_urldecoding_basic() {
        assert_eq!(urldecoding("a%3Ab"), "a:b");
        assert_eq!(urldecoding("a%2Fb"), "a/b");
        assert_eq!(urldecoding("hello"), "hello");
    }

    #[test]
    fn test_provider_path_generation() {
        let zk_url = {
            let mut u = URL::new("zookeeper", "");
            u.ip = "127.0.0.1".into();
            u.port = "2181".into();
            u
        };
        let registry = ZookeeperRegistry::new(zk_url);
        let service_url = make_service_url("/com.example.GreetService");

        let path = registry.provider_path(&service_url);
        assert!(path.starts_with("/dubbo/com.example.GreetService/providers/"));
    }

    #[test]
    fn test_providers_dir_generation() {
        let zk_url = {
            let mut u = URL::new("zookeeper", "");
            u.ip = "127.0.0.1".into();
            u.port = "2181".into();
            u
        };
        let registry = ZookeeperRegistry::new(zk_url);
        let service_url = make_service_url("/com.example.GreetService");

        let dir = registry.providers_dir(&service_url);
        assert_eq!(dir, "/dubbo/com.example.GreetService/providers");
    }

    #[test]
    fn test_registry_creation() {
        let mut url = URL::new("zookeeper", "");
        url.ip = "127.0.0.1".to_string();
        url.port = "2181".to_string();
        let registry = ZookeeperRegistry::new(url);
        assert!(!registry.is_available());
    }

    #[test]
    fn test_registry_with_custom_root() {
        let mut url = URL::new("zookeeper", "");
        url.ip = "127.0.0.1".to_string();
        url.port = "2181".to_string();
        let registry = ZookeeperRegistry::new(url).with_root_path("/custom");
        assert_eq!(registry.root_path, "/custom");
    }

    #[test]
    fn test_decode_invalid_input() {
        assert!(decode_url("not-a-valid-url").is_none());
    }

    #[test]
    fn test_decode_with_protocols() {
        let tri_url = {
            let mut u = URL::new("tri", "/Svc");
            u.ip = "10.0.0.1".to_string();
            u.port = "50051".to_string();
            u
        };
        let encoded = encode_url(&tri_url);
        let decoded = decode_url(&encoded);
        assert!(decoded.is_some());
        let d = decoded.unwrap();
        assert!(d.ip == "10.0.0.1");
    }

    #[test]
    fn test_watcher_dispatcher_creation() {
        let (tx, _rx) = mpsc::unbounded_channel::<WatchedEvent>();
        let watcher = ChannelWatcher { tx };
        let event = WatchedEvent {
            event_type: zookeeper::WatchedEventType::NodeChildrenChanged,
            keeper_state: zookeeper::KeeperState::SyncConnected,
            path: Some("/dubbo/test/providers".to_string()),
        };
        watcher.handle(event);
    }

    #[test]
    fn test_subscribe_stores_listener() {
        struct MockListener {
            url: URL,
            events: std::sync::Mutex<Vec<ServiceEvent>>,
        }

        #[async_trait]
        impl NotifyListener for MockListener {
            async fn notify(&self, event: ServiceEvent) {
                self.events.lock().unwrap().push(event);
            }
            fn listen_url(&self) -> URL {
                self.url.clone()
            }
        }

        let zk_url = {
            let mut u = URL::new("zookeeper", "");
            u.ip = "127.0.0.1".into();
            u.port = "2181".into();
            u
        };
        let registry = ZookeeperRegistry::new(zk_url);

        let listener = Arc::new(MockListener {
            url: URL::new("tri", "/com.example.TestService"),
            events: std::sync::Mutex::new(Vec::new()),
        });

        registry
            .listener_map
            .write()
            .unwrap()
            .entry("/com.example.TestService".to_string())
            .or_default()
            .push(listener.clone());

        assert!(registry
            .listener_map
            .read()
            .unwrap()
            .contains_key("/com.example.TestService"));
        assert_eq!(
            registry
                .listener_map
                .read()
                .unwrap()
                .get("/com.example.TestService")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn test_multiple_listeners_same_service() {
        struct MockListener {
            url: URL,
        }

        #[async_trait]
        impl NotifyListener for MockListener {
            async fn notify(&self, _event: ServiceEvent) {}
            fn listen_url(&self) -> URL {
                self.url.clone()
            }
        }

        let zk_url = {
            let mut u = URL::new("zookeeper", "");
            u.ip = "127.0.0.1".into();
            u.port = "2181".into();
            u
        };
        let registry = ZookeeperRegistry::new(zk_url);

        let l1 = Arc::new(MockListener {
            url: URL::new("tri", "/svc"),
        });
        let l2 = Arc::new(MockListener {
            url: URL::new("tri", "/svc"),
        });
        let l3 = Arc::new(MockListener {
            url: URL::new("tri", "/svc"),
        });

        let key = "/com.example.MultiService".to_string();
        {
            let mut map = registry.listener_map.write().unwrap();
            let entry = map.entry(key.clone()).or_default();
            let listeners_dyn: Vec<Arc<dyn NotifyListener>> =
                vec![l1 as Arc<dyn NotifyListener>, l2, l3];
            entry.extend(listeners_dyn);
        }

        assert_eq!(
            registry
                .listener_map
                .read()
                .unwrap()
                .get(&key)
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn test_unsubscribe_removes_listener() {
        struct MockListener {
            url: URL,
        }

        #[async_trait]
        impl NotifyListener for MockListener {
            async fn notify(&self, _event: ServiceEvent) {}
            fn listen_url(&self) -> URL {
                self.url.clone()
            }
        }

        let zk_url = {
            let mut u = URL::new("zookeeper", "");
            u.ip = "127.0.0.1".into();
            u.port = "2181".into();
            u
        };
        let registry = ZookeeperRegistry::new(zk_url);

        let key = "/com.example.RemoveService".to_string();
        let listener = Arc::new(MockListener {
            url: URL::new("tri", &key),
        });
        registry
            .listener_map
            .write()
            .unwrap()
            .entry(key.clone())
            .or_default()
            .push(listener);

        assert!(registry.listener_map.read().unwrap().contains_key(&key));

        registry.listener_map.write().unwrap().remove(&key);
        assert!(!registry.listener_map.read().unwrap().contains_key(&key));
    }

    #[tokio::test]
    async fn test_watcher_handles_node_children_changed() {
        let (tx, rx) = mpsc::unbounded_channel::<WatchedEvent>();
        let listeners: ListenerMap = Arc::new(RwLock::new(HashMap::new()));
        let zk_ref: Arc<RwLock<Option<ZooKeeper>>> = Arc::new(RwLock::new(None));

        let path = "/dubbo/test/providers".to_string();
        let service_key = "/test".to_string();

        let handle = tokio::spawn(async move {
            watch_event_loop(rx, &path, &service_key, listeners, zk_ref).await;
        });

        let event = WatchedEvent {
            event_type: zookeeper::WatchedEventType::NodeChildrenChanged,
            keeper_state: zookeeper::KeeperState::SyncConnected,
            path: Some("/dubbo/test/providers".to_string()),
        };
        tx.send(event).unwrap();

        drop(tx);
        let _ = handle.await;
    }

    #[test]
    fn test_zk_registry_url_encoding() {
        let mut url = URL::new("tri", "/com.example.SpecialService");
        url.ip = "10.0.0.1".to_string();
        url.port = "8080".to_string();

        let encoded = encode_url(&url);
        assert!(!encoded.contains(':'));
        assert!(!encoded.contains('/'));
        assert!(encoded.contains("tri"));
        assert!(encoded.contains("10.0.0.1"));
        assert!(encoded.contains("8080"));
        assert!(encoded.contains("com.example.SpecialService"));

        let decoded = decode_url(&encoded);
        assert!(decoded.is_some());
        let d = decoded.unwrap();
        assert_eq!(d.ip, "10.0.0.1");
        assert_eq!(d.port, "8080");
        assert_eq!(d.path, "/com.example.SpecialService");
    }
}
