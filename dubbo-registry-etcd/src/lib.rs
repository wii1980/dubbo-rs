pub use dubbo_rs_common;
pub use dubbo_rs_registry;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_registry::{NotifyListener, Registry, ServiceEvent};
use etcd_client::{
    Client as EtcdClient, ConnectOptions, DeleteOptions, EventType, GetOptions, PutOptions,
    WatchOptions,
};

const DEFAULT_TTL: i64 = 30;
const DEFAULT_DUBBO_ROOT: &str = "/dubbo";

/// Etcd-based service registry using gRPC (etcd-client).
///
/// Connects to etcd via gRPC, matching the approach used by dubbo-java (jetcd).
/// Supports lease-based ephemeral registration, keep-alive, and watch-based
/// service discovery.
pub struct EtcdRegistry {
    url: URL,
    endpoints: Vec<String>,
    root_path: String,
    client: tokio::sync::OnceCell<EtcdClient>,
    lease_id: tokio::sync::Mutex<Option<i64>>,
    subscribed: DashMap<String, Vec<Arc<dyn NotifyListener>>>,
    shutdown: Arc<AtomicBool>,
}

impl EtcdRegistry {
    #[must_use]
    pub fn new(url: URL) -> Self {
        let endpoints = url.get_param("endpoints").map_or_else(
            || vec![format!("{}:{}", url.ip, url.port)],
            |e| e.split(',').map(|s| s.trim().to_string()).collect(),
        );

        let root_path = url
            .get_param("root")
            .map_or_else(|| DEFAULT_DUBBO_ROOT.to_string(), Clone::clone);

        Self {
            url,
            endpoints,
            root_path,
            client: tokio::sync::OnceCell::new(),
            lease_id: tokio::sync::Mutex::new(None),
            subscribed: DashMap::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    #[must_use]
    pub fn with_endpoints(mut self, endpoints: impl Into<String>) -> Self {
        let e: String = endpoints.into();
        self.endpoints = e.split(',').map(|s| s.trim().to_string()).collect();
        self
    }

    #[must_use]
    pub fn with_root_path(mut self, path: impl Into<String>) -> Self {
        self.root_path = path.into();
        self
    }

    fn provider_path(&self, service_key: &str) -> String {
        format!("{}/{service_key}/providers", self.root_path)
    }

    fn provider_key(&self, service_key: &str, url_str: &str) -> String {
        let dir = self.provider_path(service_key);
        format!("{dir}/{url_str}")
    }

    async fn connect(&self) -> Result<&EtcdClient, RPCError> {
        self.client
            .get_or_try_init(|| async {
                let endpoints: Vec<String> = if self.endpoints.is_empty() {
                    vec![format!("{}:{}", self.url.ip, self.url.port)]
                } else {
                    self.endpoints.clone()
                };
                EtcdClient::connect(endpoints, Some(ConnectOptions::default()))
                    .await
                    .map_err(|e| RPCError::ServerError(format!("etcd connect failed: {e}")))
            })
            .await
    }

    async fn ensure_lease(&self) -> Result<i64, RPCError> {
        let mut guard = self.lease_id.lock().await;
        if let Some(id) = *guard {
            return Ok(id);
        }

        let client = self.connect().await?;
        let mut lease_client = client.lease_client();
        let resp = lease_client
            .grant(DEFAULT_TTL, None)
            .await
            .map_err(|e| RPCError::ServerError(format!("etcd lease grant failed: {e}")))?;
        let lease_id = resp.id();

        // Start background keep-alive for the lease
        let (mut _keeper, mut stream) = lease_client
            .keep_alive(lease_id)
            .await
            .map_err(|e| RPCError::ServerError(format!("etcd keepalive failed: {e}")))?;

        let shutdown = self.shutdown.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = stream.message() => {
                        if result.is_err() || !matches!(result, Ok(Some(_))) {
                            break;
                        }
                    }
                    () = async {
                        while !shutdown.load(Ordering::SeqCst) {
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        }
                    } => {
                        break;
                    }
                }
            }
        });

        *guard = Some(lease_id);
        Ok(lease_id)
    }

    async fn put_with_lease(&self, key: &str, value: &str) -> Result<(), RPCError> {
        let lease_id = self.ensure_lease().await?;
        let client = self.connect().await?;
        let mut kv = client.kv_client();
        kv.put(key, value, Some(PutOptions::new().with_lease(lease_id)))
            .await
            .map_err(|e| RPCError::ServerError(format!("etcd put failed: {e}")))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), RPCError> {
        let client = self.connect().await?;
        let mut kv = client.kv_client();
        kv.delete(key, Some(DeleteOptions::new()))
            .await
            .map_err(|e| RPCError::ServerError(format!("etcd delete failed: {e}")))?;
        Ok(())
    }

    async fn get_prefix(&self, prefix: &str) -> Result<Vec<String>, RPCError> {
        let client = self.connect().await?;
        let mut kv = client.kv_client();
        let resp = kv
            .get(prefix, Some(GetOptions::new().with_prefix()))
            .await
            .map_err(|e| RPCError::ServerError(format!("etcd range failed: {e}")))?;

        let values = resp
            .kvs()
            .iter()
            .map(|kv| String::from_utf8(kv.value().to_vec()).unwrap_or_default())
            .filter(|v| !v.is_empty())
            .collect();

        Ok(values)
    }

    async fn start_watch(&self, service_key: &str) -> Result<(), RPCError> {
        let dir = self.provider_path(service_key);
        let client = self.connect().await?;
        let mut watch = client.watch_client();

        let mut stream = watch
            .watch(dir.clone(), Some(WatchOptions::new().with_prefix()))
            .await
            .map_err(|e| RPCError::ServerError(format!("etcd watch failed: {e}")))?;

        let subscribed = self.subscribed.clone();
        let watch_service_key = service_key.to_string();
        let shutdown = self.shutdown.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = stream.message() => {
                        match result {
                            Ok(Some(resp)) => {
                                for event in resp.events() {
                                    if event.event_type() != EventType::Put {
                                        continue;
                                    }
                                    if let Some(kv) = event.kv() {
                                        let url_str =
                                            String::from_utf8(kv.value().to_vec()).unwrap_or_default();
                                        if url_str.is_empty() {
                                            continue;
                                        }
                                        if let Some(parsed) = parse_provider_url(&url_str) {
                                            let ev = ServiceEvent::Add(vec![parsed]);
                                            if let Some(listeners) = subscribed.get(&watch_service_key) {
                                                for l in listeners.value() {
                                                    l.notify(ev.clone()).await;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => break,
                        }
                    }
                    () = async {
                        while !shutdown.load(Ordering::SeqCst) {
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        }
                    } => {
                        break;
                    }
                }
            }
        });

        Ok(())
    }
}

impl Drop for EtcdRegistry {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

impl Node for EtcdRegistry {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl Registry for EtcdRegistry {
    async fn register(&self, url: URL) -> Result<(), RPCError> {
        let service_key = url.get_service_key();
        let full = url.to_full_string();
        let key = self.provider_key(&service_key, &full);
        self.put_with_lease(&key, &full).await
    }

    async fn unregister(&self, url: URL) -> Result<(), RPCError> {
        let service_key = url.get_service_key();
        let full = url.to_full_string();
        let key = self.provider_key(&service_key, &full);
        self.delete(&key).await
    }

    async fn subscribe(&self, url: URL, listener: Arc<dyn NotifyListener>) -> Result<(), RPCError> {
        let service_key = url.get_service_key();
        let is_first = {
            let mut entries = self.subscribed.entry(service_key.clone()).or_default();
            let is_first = entries.is_empty();
            entries.push(listener);
            is_first
        };

        if is_first {
            let dir = self.provider_path(&service_key);
            let values = self.get_prefix(&dir).await?;

            let provider_urls: Vec<URL> = values
                .iter()
                .filter_map(|v| parse_provider_url(v))
                .collect();

            if !provider_urls.is_empty() {
                let event = ServiceEvent::Add(provider_urls);
                if let Some(listeners) = self.subscribed.get(&service_key) {
                    for l in listeners.value() {
                        l.notify(event.clone()).await;
                    }
                }
            }

            self.start_watch(&service_key).await?;
        }

        Ok(())
    }

    async fn unsubscribe(
        &self,
        url: URL,
        _listener: Arc<dyn NotifyListener>,
    ) -> Result<(), RPCError> {
        let service_key = url.get_service_key();
        self.subscribed.remove(&service_key);
        Ok(())
    }
}

/// Parse a provider URL string produced by `URL::to_full_string()`.
///
/// Format: `protocol://ip:port/path/version?key=value&...`
/// Example: `tri://127.0.0.1:50051//com.example.Service/1.0.0?side=provider`
fn parse_provider_url(s: &str) -> Option<URL> {
    let (protocol, rest) = s.split_once("://")?;
    let (ip_port, path_and_more) = rest.split_once('/')?;
    let (ip, port) = ip_port.split_once(':')?;
    let (full_path, params_str) = path_and_more.split_once('?').unwrap_or((path_and_more, ""));
    let last_slash = full_path.rfind('/')?;
    let path = &full_path[..last_slash];
    let version = &full_path[last_slash + 1..];

    let mut url = URL::new(protocol, path);
    url.ip = ip.to_string();
    url.port = port.to_string();
    url.set_param("version", version);

    if !params_str.is_empty() {
        for pair in params_str.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                url.set_param(k, v);
            }
        }
    }

    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_etcd_registry_creation() {
        let mut url = URL::new("etcd", "/com.example.Service");
        url.ip = "127.0.0.1".into();
        url.port = "2379".into();
        let registry = EtcdRegistry::new(url);
        assert!(registry.is_available());
        assert_eq!(registry.root_path, "/dubbo");
    }

    #[test]
    fn test_etcd_with_custom_root() {
        let mut url = URL::new("etcd", "/com.example.Service");
        url.ip = "127.0.0.1".into();
        url.port = "2379".into();
        let registry = EtcdRegistry::new(url).with_root_path("/custom");
        assert_eq!(registry.root_path, "/custom");
    }

    #[test]
    fn test_etcd_with_endpoints() {
        let mut url = URL::new("etcd", "/com.example.Service");
        url.ip = "127.0.0.1".into();
        url.port = "2379".into();
        let registry = EtcdRegistry::new(url).with_endpoints("http://etcd1:2379,http://etcd2:2379");
        assert_eq!(
            registry.endpoints,
            vec![
                "http://etcd1:2379".to_string(),
                "http://etcd2:2379".to_string(),
            ]
        );
    }

    #[test]
    fn test_provider_path_generation() {
        let mut url = URL::new("etcd", "/com.example.Service");
        url.ip = "127.0.0.1".into();
        url.port = "2379".into();
        let registry = EtcdRegistry::new(url);
        let path = registry.provider_path("com.example.Service");
        assert_eq!(path, "/dubbo/com.example.Service/providers");
    }
}
