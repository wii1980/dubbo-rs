#![allow(unused_variables)]
#![allow(clippy::cast_possible_truncation, clippy::too_many_lines, deprecated)]

pub use dubbo_rs_common;
pub use dubbo_rs_registry;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use dashmap::DashMap;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_registry::{NotifyListener, Registry, ServiceEvent};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use tokio::sync::mpsc;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

// =========================================================================
// Constants matching dubbo-java's RedisRegistry
// =========================================================================

const DEFAULT_ROOT: &str = "dubbo";

const DEFAULT_EXPIRE_PERIOD_MS: u64 = 60_000;

const DEFAULT_RECONNECT_PERIOD_MS: u64 = 3000;

const REGISTER: &str = "register";
const UNREGISTER: &str = "unregister";

const DEFAULT_CATEGORY: &str = "providers";

// =========================================================================
// RedisRegistry
// =========================================================================

pub struct RedisRegistry {
    url: URL,
    root: String,
    expire_period_ms: u64,
    reconnect_period_ms: u64,
    redis_url: String,
    client: tokio::sync::OnceCell<redis::Client>,
    cm: tokio::sync::OnceCell<ConnectionManager>,
    registered: DashMap<String, (String, String)>,
    subscribers: DashMap<String, Vec<Arc<dyn NotifyListener>>>,
    notifier_handles: TokioMutex<Vec<JoinHandle<()>>>,
    heartbeat_handle: TokioMutex<Option<JoinHandle<()>>>,
    shutdown: Arc<AtomicBool>,
}

impl RedisRegistry {
    /// Create a new `RedisRegistry` from a Dubbo URL.
    ///
    /// URL parameters:
    /// - `group` or `root` — root path (default: `dubbo`)
    /// - `session` or `expire_period` — session timeout in milliseconds (default: 60000)
    /// - `password` — Redis password
    /// - `db` — Redis database index (default: `0`)
    #[must_use]
    pub fn new(url: URL) -> Self {
        let host = if url.ip.is_empty() {
            "127.0.0.1".to_string()
        } else {
            url.ip.clone()
        };
        let port = if url.port.is_empty() {
            "6379".to_string()
        } else {
            url.port.clone()
        };
        let db = url.get_param_or_default("db", "0");
        let password = url.get_param("password").cloned();

        let redis_url = if let Some(pwd) = &password {
            format!("redis://:{pwd}@{host}:{port}/{db}")
        } else {
            format!("redis://{host}:{port}/{db}")
        };

        // Root path: matches Java's group handling
        let raw_root = url
            .get_param("group")
            .or_else(|| url.get_param("root"))
            .cloned()
            .unwrap_or_else(|| DEFAULT_ROOT.to_string());
        let root = format!("/{}/", raw_root.trim_matches('/'));

        // Expire period in milliseconds: Java's SESSION_TIMEOUT_KEY (default 60000)
        let expire_period_ms = url
            .get_param("session")
            .or_else(|| url.get_param("expire_period"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_EXPIRE_PERIOD_MS);

        Self {
            url,
            root,
            expire_period_ms,
            reconnect_period_ms: DEFAULT_RECONNECT_PERIOD_MS,
            redis_url,
            client: tokio::sync::OnceCell::new(),
            cm: tokio::sync::OnceCell::new(),
            registered: DashMap::new(),
            subscribers: DashMap::new(),
            notifier_handles: TokioMutex::new(Vec::new()),
            heartbeat_handle: TokioMutex::new(None),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Override the root path.
    #[must_use]
    pub fn with_root_path(mut self, path: impl Into<String>) -> Self {
        let p = path.into();
        self.root = format!("/{}/", p.trim_matches('/'));
        self
    }

    /// Override the expire period in milliseconds (default: 60000).
    #[must_use]
    pub fn with_expire_period(mut self, ms: u64) -> Self {
        self.expire_period_ms = ms;
        self
    }
}

// ---------------------------------------------------------------------------
// Key helpers — matching Java's toServicePath, toCategoryPath etc.
// ---------------------------------------------------------------------------

impl RedisRegistry {
    /// `toServicePath(url)` = `{root}{serviceInterface}`
    fn to_service_path(&self, service_interface: &str) -> String {
        let iface = service_interface.trim_start_matches('/');
        format!("{}{}", self.root, iface)
    }

    /// `toCategoryPath(url)` = `{servicePath}/{category}`
    fn to_category_path(&self, url: &URL) -> String {
        let service_iface = url.path.trim_start_matches('/');
        let category = url.get_param_or_default("category", DEFAULT_CATEGORY);
        format!("{}{}/{}", self.root, service_iface, category)
    }

    /// `toServicePath(categoryKey)` — extract service path from a category key.
    fn to_service_path_from_key(&self, key: &str) -> String {
        let after_root = &key[self.root.len()..];
        if let Some(idx) = after_root.find('/') {
            format!("{}/{}", self.root.trim_end_matches('/'), &after_root[..idx])
        } else {
            key.to_string()
        }
    }

    /// Build the psubscribe pattern: `{servicePath}/*`
    fn service_pattern(service_path: &str) -> String {
        format!("{service_path}/*")
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn expire_value(&self) -> u64 {
        Self::now_ms() + self.expire_period_ms
    }
}

// ---------------------------------------------------------------------------
// Redis connection helpers
// ---------------------------------------------------------------------------

impl RedisRegistry {
    async fn get_client(&self) -> Result<&redis::Client, RPCError> {
        self.client
            .get_or_try_init(|| async {
                redis::Client::open(self.redis_url.as_str())
                    .map_err(|e| RPCError::ServerError(format!("redis open failed: {e}")))
            })
            .await
    }

    async fn get_conn(&self) -> Result<ConnectionManager, RPCError> {
        self.cm
            .get_or_try_init(|| async {
                let client = self.get_client().await?.clone();
                ConnectionManager::new(client)
                    .await
                    .map_err(|e| RPCError::ServerError(format!("redis connect failed: {e}")))
            })
            .await
            .cloned()
    }
}

// ---------------------------------------------------------------------------
// doNotify — Java's doNotify(String key)
// ---------------------------------------------------------------------------

impl RedisRegistry {
    /// React to a REGISTER/UNREGISTER event on a category key.
    async fn do_notify(&self, key: &str, conn: &mut ConnectionManager) {
        let now = Self::now_ms();
        let values: HashMap<String, String> = conn.hgetall(key).await.unwrap_or_default();

        let urls: Vec<URL> = values
            .iter()
            .filter_map(|(url_str, expire_str)| {
                let expire: u64 = expire_str.parse().ok()?;
                if expire >= now {
                    parse_redis_url(url_str)
                } else {
                    None
                }
            })
            .collect();

        let service_path = self.to_service_path_from_key(key);

        if let Some(listeners) = self.subscribers.get(&service_path) {
            let event = if urls.is_empty() {
                ServiceEvent::Remove(vec![])
            } else {
                ServiceEvent::Add(urls)
            };
            for l in listeners.value() {
                l.notify(event.clone()).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pub/Sub notifier — matches Java's Notifier + NotifySub
// ---------------------------------------------------------------------------

impl RedisRegistry {
    /// Start a background notifier for a service path.
    async fn start_notifier(&self, service_path: String) {
        let client = match self.get_client().await {
            Ok(c) => c.clone(),
            Err(e) => {
                tracing::error!("RedisRegistry: failed to get client for notifier: {e}");
                return;
            }
        };

        let pattern = Self::service_pattern(&service_path);
        let shutdown = self.shutdown.clone();
        let subscribers = self.subscribers.clone();
        let reconnect_ms = self.reconnect_period_ms;

        // Channel to forward pubsub messages to a processor that has
        // its own connection manager for HGETALL.
        let redis_url = self.redis_url.clone();
        let (tx, mut rx) = mpsc::unbounded_channel::<(String, String)>();

        // --- Processor task: receives (key, payload) via channel ---
        let proc_shutdown = shutdown.clone();
        let proc_subscribers = subscribers.clone();
        let proc = tokio::spawn(async move {
            let cm = loop {
                if proc_shutdown.load(Ordering::SeqCst) {
                    return;
                }
                if let Ok(c) = redis::Client::open(redis_url.as_str()) {
                    if let Ok(cm) = ConnectionManager::new(c).await {
                        break cm;
                    }
                }
                tokio::time::sleep(Duration::from_millis(1000)).await;
            };

            loop {
                tokio::select! {
                    Some((key, _payload)) = rx.recv() => {
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        let values: HashMap<String, String> = {
                            let mut c = cm.clone();
                            c.hgetall(&key).await.unwrap_or_default()
                        };
                        let urls: Vec<URL> = values
                            .iter()
                            .filter_map(|(url_str, expire_str)| {
                                let expire: u64 = expire_str.parse().ok()?;
                                if expire >= now { parse_redis_url(url_str) } else { None }
                            })
                            .collect();
                        // Extract service_path from key
                        let stripped = key.strip_suffix('/').unwrap_or(&key);
                        let service_path = if let Some(idx) = stripped.rfind('/') {
                            stripped[..idx].to_string()
                        } else {
                            key.clone()
                        };
                        let event = if urls.is_empty() {
                            ServiceEvent::Remove(vec![])
                        } else {
                            ServiceEvent::Add(urls)
                        };
                        if let Some(listeners) = proc_subscribers.get(&service_path) {
                            for l in listeners.value() {
                                l.notify(event.clone()).await;
                            }
                        }
                    }
                    () = async {
                        while !proc_shutdown.load(Ordering::SeqCst) {
                            tokio::time::sleep(Duration::from_millis(200)).await;
                        }
                    } => return,
                }
            }
        });

        // --- Pub/sub listener: PSUBSCRIBE + forward to processor ---
        let handle = tokio::spawn(async move {
            loop {
                if shutdown.load(Ordering::SeqCst) {
                    return;
                }

                let conn = match client.get_async_connection().await {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("RedisRegistry: notifier connection failed: {e}");
                        tokio::time::sleep(Duration::from_millis(reconnect_ms)).await;
                        continue;
                    }
                };

                let mut pubsub = conn.into_pubsub();
                if let Err(e) = pubsub.psubscribe(pattern.as_str()).await {
                    tracing::warn!("RedisRegistry: psubscribe failed: {e}");
                    tokio::time::sleep(Duration::from_millis(reconnect_ms)).await;
                    continue;
                }

                tracing::info!("RedisRegistry: subscribed to '{}'", pattern);

                let mut stream = pubsub.on_message();

                loop {
                    tokio::select! {
                        msg = stream.next() => {
                            if let Some(msg) = msg {
                                let channel = msg.get_channel().unwrap_or_default();
                                let payload = msg.get_payload().unwrap_or_default();
                                if payload == REGISTER || payload == UNREGISTER {
                                    let _ = tx.send((channel, payload));
                                }
                            } else {
                                tracing::warn!("RedisRegistry: notifier stream ended");
                                break;
                            }
                        }
                        () = async {
                            while !shutdown.load(Ordering::SeqCst) {
                                tokio::time::sleep(Duration::from_millis(200)).await;
                            }
                        } => return,
                    }
                }
            }
        });

        let mut handles = self.notifier_handles.lock().await;
        handles.push(handle);
        handles.push(proc);
    }
}

// ---------------------------------------------------------------------------
// Heartbeat — matches Java's deferExpired()
// ---------------------------------------------------------------------------

impl RedisRegistry {
    async fn start_heartbeat(&self) {
        let registered = self.registered.clone();
        let expire_period_ms = self.expire_period_ms;
        let cm = match self.get_conn().await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("RedisRegistry: heartbeat connection failed: {e}");
                return;
            }
        };
        let shutdown = self.shutdown.clone();

        let interval_ms = expire_period_ms / 2;

        let handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;

                if shutdown.load(Ordering::SeqCst) {
                    return;
                }

                let expire_ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64
                    + expire_period_ms;

                let mut conn = cm.clone();
                for entry in &registered {
                    let (key, (field, _)) = entry.pair();
                    // If hset returns 0 (updated existing), no publish.
                    // This matches Java's heartbeat behaviour.
                    let _ = conn
                        .hset::<_, _, _, ()>(key.as_str(), field.as_str(), expire_ts.to_string())
                        .await;
                }
            }
        });

        *self.heartbeat_handle.lock().await = Some(handle);
    }
}

impl Drop for RedisRegistry {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

impl Node for RedisRegistry {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        self.cm.get().is_some()
    }

    fn destroy(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl Registry for RedisRegistry {
    /// Register a service URL.
    ///
    /// Java: `doRegister(URL url)`
    ///   key = toCategoryPath(url)
    ///   value = url.toFullString()
    ///   expire = String.valueOf(System.currentTimeMillis() + expirePeriod)
    ///   redisClient.hset(key, value, expire)
    ///   redisClient.publish(key, REGISTER)
    async fn register(&self, url: URL) -> Result<(), RPCError> {
        let key = self.to_category_path(&url);
        let field = url_to_redis_string(&url);
        let expire = self.expire_value().to_string();

        let mut conn = self.get_conn().await?;

        let _: () = conn
            .hset(key.as_str(), field.as_str(), expire.as_str())
            .await
            .map_err(|e| RPCError::ServerError(format!("redis hset failed: {e}")))?;

        let _: () = conn
            .publish(key.as_str(), REGISTER)
            .await
            .map_err(|e| RPCError::ServerError(format!("redis publish failed: {e}")))?;

        self.registered.insert(key.clone(), (field, key.clone()));

        // Start heartbeat on first registration (matches Java's scheduled executor)
        if self.heartbeat_handle.lock().await.is_none() {
            self.start_heartbeat().await;
        }

        Ok(())
    }

    /// Unregister a service URL.
    ///
    /// Java: `doUnregister(URL url)`
    ///   key = toCategoryPath(url)
    ///   value = url.toFullString()
    ///   redisClient.hdel(key, value)
    ///   redisClient.publish(key, UNREGISTER)
    async fn unregister(&self, url: URL) -> Result<(), RPCError> {
        let key = self.to_category_path(&url);
        let field = url_to_redis_string(&url);

        let mut conn = self.get_conn().await?;

        let _: () = conn
            .hdel(key.as_str(), field.as_str())
            .await
            .map_err(|e| RPCError::ServerError(format!("redis hdel failed: {e}")))?;

        let _: () = conn
            .publish(key.as_str(), UNREGISTER)
            .await
            .map_err(|e| RPCError::ServerError(format!("redis publish failed: {e}")))?;

        self.registered.remove(&key);

        Ok(())
    }

    /// Subscribe to provider changes.
    ///
    /// Java: `doSubscribe(URL url, NotifyListener listener)`
    ///   1. Create Notifier (one per service_path) with PSUBSCRIBE
    ///   2. SCAN {servicePath}/* → HGETALL each key → notify listener
    async fn subscribe(&self, url: URL, listener: Arc<dyn NotifyListener>) -> Result<(), RPCError> {
        let service_iface = url.path.trim_start_matches('/');
        let service_path = self.to_service_path(service_iface);

        // Each service_path gets one notifier (Java's ConcurrentMap<String, Notifier>)
        let is_first = {
            let mut entries = self.subscribers.entry(service_path.clone()).or_default();
            let is_first = entries.is_empty();
            entries.push(listener);
            is_first
        };

        if is_first {
            // Initial snapshot: fetch all category keys for this service
            let pattern = Self::service_pattern(&service_path);
            let mut conn = self.get_conn().await?;

            let keys: Vec<String> = conn
                .keys(pattern.as_str())
                .await
                .map_err(|e| RPCError::ServerError(format!("redis keys failed: {e}")))?;

            for key in &keys {
                self.do_notify(key, &mut conn).await;
            }

            // Start background notifier (PSUBSCRIBE)
            self.start_notifier(service_path).await;
        }

        Ok(())
    }

    async fn unsubscribe(
        &self,
        url: URL,
        _listener: Arc<dyn NotifyListener>,
    ) -> Result<(), RPCError> {
        let service_iface = url.path.trim_start_matches('/');
        let service_path = self.to_service_path(service_iface);
        self.subscribers.remove(&service_path);
        Ok(())
    }
}

// =========================================================================
// URL format helpers — Java-compatible format
// =========================================================================

/// Serialize a URL to a Java-compatible full string.
///
/// Java's `URL.toFullString()` format:
///   `{protocol}://{host}:{port}/{path}?{key1}={value1}&...`
fn url_to_redis_string(url: &URL) -> String {
    let mut params = String::new();
    for (i, (k, v)) in url.params.iter().enumerate() {
        if i > 0 {
            params.push('&');
        }
        params.push_str(k);
        params.push('=');
        params.push_str(v);
    }

    let path = url.path.trim_start_matches('/');
    format!(
        "{}://{}:{}/{}?{}",
        url.protocol, url.ip, url.port, path, params
    )
}

/// Parse a Java-compatible URL string back into a URL.
///
/// Input: `{protocol}://{host}:{port}/{path}?{params}`
fn parse_redis_url(s: &str) -> Option<URL> {
    let (protocol, rest) = s.split_once("://")?;
    if protocol.is_empty() {
        return None;
    }
    let (ip_port, path_and_params) = rest.split_once('/')?;
    let (ip, port) = ip_port.split_once(':')?;

    let (path, params_str) = path_and_params
        .split_once('?')
        .unwrap_or((path_and_params, ""));

    let mut url = URL::new(protocol, format!("/{path}"));
    url.ip = ip.to_string();
    url.port = port.to_string();

    if !params_str.is_empty() {
        for pair in params_str.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                url.set_param(k, v);
            }
        }
    }

    Some(url)
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_redis_url() -> URL {
        let mut url = URL::new("redis", "");
        url.ip = "127.0.0.1".to_string();
        url.port = "6379".to_string();
        url
    }

    // -------------------------------------------------------------------
    // RedisRegistry construction
    // -------------------------------------------------------------------

    #[test]
    fn test_redis_registry_creation() {
        let registry = RedisRegistry::new(make_redis_url());
        assert_eq!(registry.root, "/dubbo/");
        assert_eq!(registry.expire_period_ms, 60000);
    }

    #[test]
    fn test_redis_with_custom_root() {
        let registry = RedisRegistry::new(make_redis_url()).with_root_path("myapp");
        assert_eq!(registry.root, "/myapp/");
    }

    #[test]
    fn test_redis_with_expire_period() {
        let registry = RedisRegistry::new(make_redis_url()).with_expire_period(120_000);
        assert_eq!(registry.expire_period_ms, 120_000);
    }

    #[test]
    fn test_root_from_url_group_param() {
        let mut url = URL::new("redis", "");
        url.ip = "127.0.0.1".to_string();
        url.port = "6379".to_string();
        url.set_param("group", "mygroup");
        let registry = RedisRegistry::new(url);
        assert_eq!(registry.root, "/mygroup/");
    }

    #[test]
    fn test_root_from_url_root_param() {
        let mut url = URL::new("redis", "");
        url.ip = "127.0.0.1".to_string();
        url.port = "6379".to_string();
        url.set_param("root", "/custom/");
        let registry = RedisRegistry::new(url);
        assert_eq!(registry.root, "/custom/");
    }

    // -------------------------------------------------------------------
    // Key helpers
    // -------------------------------------------------------------------

    #[test]
    fn test_to_service_path() {
        let registry = RedisRegistry::new(make_redis_url());
        let path = registry.to_service_path("com.example.UserService");
        assert_eq!(path, "/dubbo/com.example.UserService");
    }

    #[test]
    fn test_to_service_path_with_leading_slash() {
        let registry = RedisRegistry::new(make_redis_url());
        let path = registry.to_service_path("/com.example.UserService");
        assert_eq!(path, "/dubbo/com.example.UserService");
    }

    #[test]
    fn test_to_category_path_default() {
        let registry = RedisRegistry::new(make_redis_url());
        let mut url = URL::new("tri", "/com.example.UserService");
        url.ip = "127.0.0.1".to_string();
        url.port = "50051".to_string();
        let path = registry.to_category_path(&url);
        assert_eq!(path, "/dubbo/com.example.UserService/providers");
    }

    #[test]
    fn test_to_category_path_custom_category() {
        let registry = RedisRegistry::new(make_redis_url());
        let mut url = URL::new("tri", "/com.example.UserService");
        url.ip = "127.0.0.1".to_string();
        url.port = "50051".to_string();
        url.set_param("category", "consumers");
        let path = registry.to_category_path(&url);
        assert_eq!(path, "/dubbo/com.example.UserService/consumers");
    }

    #[test]
    fn test_service_path_from_key() {
        let registry = RedisRegistry::new(make_redis_url());
        let key = "/dubbo/com.example.UserService/providers";
        let service_path = registry.to_service_path_from_key(key);
        assert_eq!(service_path, "/dubbo/com.example.UserService");
    }

    #[test]
    fn test_service_pattern() {
        let registry = RedisRegistry::new(make_redis_url());
        let pattern = RedisRegistry::service_pattern("/dubbo/com.example.UserService");
        assert_eq!(pattern, "/dubbo/com.example.UserService/*");
    }

    // -------------------------------------------------------------------
    // url_to_redis_string / parse_redis_url
    // -------------------------------------------------------------------

    #[test]
    fn test_url_to_redis_string_basic() {
        let mut url = URL::new("dubbo", "/com.example.UserService");
        url.ip = "192.168.1.100".to_string();
        url.port = "20880".to_string();
        url.set_param("version", "1.0.0");

        let s = url_to_redis_string(&url);
        assert_eq!(
            s,
            "dubbo://192.168.1.100:20880/com.example.UserService?version=1.0.0"
        );
    }

    #[test]
    fn test_url_to_redis_string_with_params() {
        let mut url = URL::new("tri", "/com.example.GreetService");
        url.ip = "10.0.0.1".to_string();
        url.port = "50051".to_string();
        url.set_param("version", "1.0.0");
        url.set_param("application", "demo");
        url.set_param("side", "provider");

        let s = url_to_redis_string(&url);
        assert!(s.starts_with("tri://10.0.0.1:50051/com.example.GreetService?"));
        assert!(s.contains("version=1.0.0"));
        assert!(s.contains("application=demo"));
        assert!(s.contains("side=provider"));
    }

    #[test]
    fn test_parse_redis_url_basic() {
        let s = "dubbo://192.168.1.100:20880/com.example.UserService?version=1.0.0";
        let url = parse_redis_url(s);
        assert!(url.is_some());
        let url = url.unwrap();
        assert_eq!(url.protocol, "dubbo");
        assert_eq!(url.ip, "192.168.1.100");
        assert_eq!(url.port, "20880");
        assert_eq!(url.path, "/com.example.UserService");
        assert_eq!(url.get_param("version"), Some(&"1.0.0".to_string()));
    }

    #[test]
    fn test_parse_redis_url_with_multi_params() {
        let s = "tri://10.0.0.1:50051/com.example.GreetService?version=1.0.0&application=demo&side=provider";
        let url = parse_redis_url(s);
        assert!(url.is_some());
        let url = url.unwrap();
        assert_eq!(url.protocol, "tri");
        assert_eq!(url.ip, "10.0.0.1");
        assert_eq!(url.port, "50051");
        assert_eq!(url.path, "/com.example.GreetService");
        assert_eq!(url.get_param("version"), Some(&"1.0.0".to_string()));
        assert_eq!(url.get_param("application"), Some(&"demo".to_string()));
        assert_eq!(url.get_param("side"), Some(&"provider".to_string()));
    }

    #[test]
    fn test_parse_redis_url_no_params() {
        let s = "dubbo://10.0.0.1:20880/com.example.Service";
        let url = parse_redis_url(s);
        assert!(url.is_some());
        let url = url.unwrap();
        assert_eq!(url.protocol, "dubbo");
        assert_eq!(url.ip, "10.0.0.1");
        assert_eq!(url.port, "20880");
        assert_eq!(url.path, "/com.example.Service");
    }

    #[test]
    fn test_parse_redis_url_invalid() {
        assert!(parse_redis_url("not-a-valid-url").is_none());
        assert!(parse_redis_url("").is_none());
        assert!(parse_redis_url("://host:port/path").is_none());
        assert!(parse_redis_url("proto://noport").is_none());
    }

    #[test]
    fn test_parse_redis_url_roundtrip() {
        let mut original = URL::new("dubbo", "/com.example.UserService");
        original.ip = "192.168.1.100".to_string();
        original.port = "20880".to_string();
        original.set_param("version", "1.0.0");
        original.set_param("application", "demo");
        original.set_param("side", "provider");
        original.set_param("methods", "sayHello,sayGoodbye");

        let serialized = url_to_redis_string(&original);
        let parsed = parse_redis_url(&serialized).unwrap();

        assert_eq!(parsed.protocol, original.protocol);
        assert_eq!(parsed.ip, original.ip);
        assert_eq!(parsed.port, original.port);
        assert_eq!(parsed.path, original.path);
        assert_eq!(parsed.params, original.params);
    }

    // -------------------------------------------------------------------
    // Java-compatibility tests
    // -------------------------------------------------------------------

    #[test]
    fn test_java_key_compatibility() {
        let registry = RedisRegistry::new(make_redis_url());
        let mut url = URL::new("dubbo", "/com.example.UserService");
        url.ip = "192.168.1.100".to_string();
        url.port = "20880".to_string();

        let service_path = registry.to_service_path("com.example.UserService");
        assert_eq!(service_path, "/dubbo/com.example.UserService");

        let category_path = registry.to_category_path(&url);
        assert_eq!(category_path, "/dubbo/com.example.UserService/providers");

        let pattern = RedisRegistry::service_pattern(&service_path);
        assert_eq!(pattern, "/dubbo/com.example.UserService/*");
    }

    #[test]
    fn test_java_url_format_compatibility() {
        let mut url = URL::new("dubbo", "/com.example.UserService");
        url.ip = "192.168.1.100".to_string();
        url.port = "20880".to_string();
        url.set_param("version", "1.0.0");
        url.set_param("methods", "sayHello,sayGoodbye");

        let redis_str = url_to_redis_string(&url);
        assert!(!redis_str.contains("//com")); // no double slash
        assert!(redis_str.starts_with("dubbo://192.168.1.100:20880/com.example.UserService?"));
        assert!(redis_str.contains("version=1.0.0"));
        assert!(redis_str.contains("methods=sayHello,sayGoodbye"));
    }

    #[test]
    fn test_expire_period_from_url() {
        let mut url = URL::new("redis", "");
        url.ip = "127.0.0.1".to_string();
        url.port = "6379".to_string();
        url.set_param("session", "120000");

        let registry = RedisRegistry::new(url);
        assert_eq!(registry.expire_period_ms, 120_000);
    }

    // -------------------------------------------------------------------
    // Redis connection URL building tests
    // -------------------------------------------------------------------

    #[test]
    fn test_redis_url_default() {
        let registry = RedisRegistry::new(make_redis_url());
        assert_eq!(registry.redis_url, "redis://127.0.0.1:6379/0");
    }

    #[test]
    fn test_redis_url_with_password() {
        let mut url = URL::new("redis", "");
        url.ip = "127.0.0.1".to_string();
        url.port = "6379".to_string();
        url.set_param("password", "pass123");
        let registry = RedisRegistry::new(url);
        assert_eq!(registry.redis_url, "redis://:pass123@127.0.0.1:6379/0");
    }

    #[test]
    fn test_redis_url_with_db() {
        let mut url = URL::new("redis", "");
        url.ip = "127.0.0.1".to_string();
        url.port = "6379".to_string();
        url.set_param("db", "3");
        let registry = RedisRegistry::new(url);
        assert_eq!(registry.redis_url, "redis://127.0.0.1:6379/3");
    }

    // -------------------------------------------------------------------
    // Lifecycle tests
    // -------------------------------------------------------------------

    #[test]
    #[allow(unused_variables)]
    fn test_destroy_sets_shutdown() {
        let registry = RedisRegistry::new(make_redis_url());
        assert!(!registry.shutdown.load(Ordering::SeqCst));
        registry.destroy();
        assert!(registry.shutdown.load(Ordering::SeqCst));
    }
}
