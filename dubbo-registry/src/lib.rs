pub use dubbo_rs_common;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

use async_trait::async_trait;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;

/// Service discovery event — notifies listeners of provider changes.
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceEvent {
    /// New provider nodes registered.
    Add(Vec<URL>),
    /// Provider nodes unregistered.
    Remove(Vec<URL>),
    /// Provider nodes updated (metadata change).
    Update(Vec<URL>),
}

/// Listener that receives service discovery events from a registry.
///
/// Implementors should be cheaply clonable (e.g. wrapped in `Arc`)
/// since registries may hold multiple references.
#[async_trait]
pub trait NotifyListener: Send + Sync {
    /// Handle a service change event.
    ///
    /// Called by the registry when provider nodes are added, removed,
    /// or updated for a subscribed service.
    async fn notify(&self, event: ServiceEvent);

    /// Returns the service URL this listener monitors.
    fn listen_url(&self) -> URL;
}

/// Registry abstraction for service registration and discovery.
///
/// Registries are responsible for:
/// - **Registration**: Publish provider URLs so consumers can discover them.
/// - **Discovery**: Watch for provider changes and notify subscribers.
/// - **Lifecycle**: Manage connection and ephemeral node cleanup.
///
/// Supported backends: ZooKeeper, Nacos, Etcd, direct-connect.
#[async_trait]
pub trait Registry: Node + Send + Sync {
    /// Register a service URL with the registry.
    ///
    /// For ZooKeeper, this creates an ephemeral node under the
    /// `/dubbo/{service}/providers/` path.
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if registration fails (network error,
    /// connection lost, etc.).
    async fn register(&self, url: URL) -> Result<(), RPCError>;

    /// Unregister a previously registered service URL.
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if the unregistration fails.
    async fn unregister(&self, url: URL) -> Result<(), RPCError>;

    /// Subscribe to provider changes for a service URL.
    ///
    /// The `listener` will be called whenever providers for the
    /// specified service are added, removed, or updated.
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if subscription fails.
    async fn subscribe(&self, url: URL, listener: Arc<dyn NotifyListener>) -> Result<(), RPCError>;

    /// Unsubscribe a previously registered listener.
    ///
    /// # Errors
    ///
    /// Returns `RPCError` if unsubscription fails.
    async fn unsubscribe(
        &self,
        url: URL,
        listener: Arc<dyn NotifyListener>,
    ) -> Result<(), RPCError>;
}

// ---------------------------------------------------------------------------
// ServiceInstance — application-level service discovery model
// ---------------------------------------------------------------------------

/// Represents a single service instance in application-level service discovery.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceInstance {
    pub service_name: String,
    pub host: String,
    pub port: u16,
    pub metadata: HashMap<String, String>,
}

impl ServiceInstance {
    /// Create a new `ServiceInstance` with no metadata.
    pub fn new(service_name: impl Into<String>, host: impl Into<String>, port: u16) -> Self {
        Self {
            service_name: service_name.into(),
            host: host.into(),
            port,
            metadata: HashMap::new(),
        }
    }

    /// Builder-style method to attach a metadata key-value pair.
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Convert this instance into a `URL`.
    ///
    /// The URL uses `"dubbo"` as the protocol, the host/port for addressing,
    /// and the service name as the path (prefixed with `/` if needed).
    #[must_use]
    pub fn to_url(&self) -> URL {
        let mut url = URL::new("dubbo", &self.service_name);
        url.ip.clone_from(&self.host);
        url.port = self.port.to_string();
        for (k, v) in &self.metadata {
            url.set_param(k, v);
        }
        url
    }

    /// Reconstruct a `ServiceInstance` from a `URL`.
    ///
    /// Extracts the path as `service_name`, ip as `host`, parses `port` from
    /// the URL's port string, and copies all params into `metadata`.
    #[must_use]
    pub fn from_url(url: &URL) -> Self {
        let port: u16 = url.port.parse().unwrap_or(0);
        Self {
            service_name: url.path.clone(),
            host: url.ip.clone(),
            port,
            metadata: url.params.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// ServiceDiscovery trait — application-level service discovery
// ---------------------------------------------------------------------------

/// Trait for application-level service discovery.
///
/// Unlike the interface-level `Registry`, `ServiceDiscovery` operates on
/// application-level `ServiceInstance`s (host:port + metadata) rather than
/// raw URLs.
#[async_trait]
pub trait ServiceDiscovery: Node + Send + Sync {
    /// Register a service instance.
    async fn register_instance(&self, instance: ServiceInstance) -> Result<(), RPCError>;

    /// Unregister a previously registered service instance.
    async fn unregister_instance(&self, instance: ServiceInstance) -> Result<(), RPCError>;

    /// Retrieve all currently registered instances for a service.
    async fn get_instances(&self, service_name: &str) -> Result<Vec<ServiceInstance>, RPCError>;

    /// Subscribe to changes for a service.
    async fn subscribe_service(
        &self,
        service_name: &str,
        listener: Arc<dyn NotifyListener>,
    ) -> Result<(), RPCError>;

    /// Unsubscribe from changes for a service.
    async fn unsubscribe_service(
        &self,
        service_name: &str,
        listener: Arc<dyn NotifyListener>,
    ) -> Result<(), RPCError>;
}

// ---------------------------------------------------------------------------
// InMemoryServiceDiscovery — simple in-memory implementation
// ---------------------------------------------------------------------------

/// In-memory `ServiceDiscovery` implementation for testing and local use.
pub struct InMemoryServiceDiscovery {
    url: URL,
    instances: RwLock<HashMap<String, Vec<ServiceInstance>>>,
    listeners: RwLock<HashMap<String, Vec<Arc<dyn NotifyListener>>>>,
}

impl InMemoryServiceDiscovery {
    /// Create a new empty discovery with a default URL.
    #[must_use]
    pub fn new() -> Self {
        Self {
            url: URL::new("memory", "/service-discovery"),
            instances: RwLock::new(HashMap::new()),
            listeners: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryServiceDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl Node for InMemoryServiceDiscovery {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl ServiceDiscovery for InMemoryServiceDiscovery {
    async fn register_instance(&self, instance: ServiceInstance) -> Result<(), RPCError> {
        let key = instance.service_name.clone();
        let url = instance.to_url();

        {
            let mut map = self.instances.write().unwrap();
            map.entry(key.clone()).or_default().push(instance);
        }

        let listeners: Vec<Arc<dyn NotifyListener>> = {
            let guard = self.listeners.read().unwrap();
            guard.get(&key).cloned().unwrap_or_default()
        };
        for listener in &listeners {
            listener.notify(ServiceEvent::Add(vec![url.clone()])).await;
        }
        Ok(())
    }

    async fn unregister_instance(&self, instance: ServiceInstance) -> Result<(), RPCError> {
        let key = instance.service_name.clone();
        let url = instance.to_url();

        {
            let mut map = self.instances.write().unwrap();
            if let Some(list) = map.get_mut(&key) {
                list.retain(|i| i != &instance);
            }
        }

        let listeners: Vec<Arc<dyn NotifyListener>> = {
            let guard = self.listeners.read().unwrap();
            guard.get(&key).cloned().unwrap_or_default()
        };
        for listener in &listeners {
            listener
                .notify(ServiceEvent::Remove(vec![url.clone()]))
                .await;
        }
        Ok(())
    }

    async fn get_instances(&self, service_name: &str) -> Result<Vec<ServiceInstance>, RPCError> {
        let map = self.instances.read().unwrap();
        Ok(map.get(service_name).cloned().unwrap_or_default())
    }

    async fn subscribe_service(
        &self,
        service_name: &str,
        listener: Arc<dyn NotifyListener>,
    ) -> Result<(), RPCError> {
        let mut map = self.listeners.write().unwrap();
        map.entry(service_name.to_string())
            .or_default()
            .push(listener);
        Ok(())
    }

    async fn unsubscribe_service(
        &self,
        service_name: &str,
        listener: Arc<dyn NotifyListener>,
    ) -> Result<(), RPCError> {
        let mut map = self.listeners.write().unwrap();
        if let Some(lst) = map.get_mut(service_name) {
            lst.retain(|l| !Arc::ptr_eq(l, &listener));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MultipleRegistry — delegates to multiple Registry backends
// ---------------------------------------------------------------------------

/// A registry composite that delegates every operation to all inner registries.
///
/// Operations return the first error encountered (or `Ok(())` if all succeed).
pub struct MultipleRegistry {
    url: URL,
    registries: Vec<Arc<dyn Registry>>,
}

impl MultipleRegistry {
    /// Create a new `MultipleRegistry` from a URL and a list of registry backends.
    #[must_use]
    pub fn new(url: URL, registries: Vec<Arc<dyn Registry>>) -> Self {
        Self { url, registries }
    }

    /// Return a slice of the underlying registries.
    #[must_use]
    pub fn registries(&self) -> &[Arc<dyn Registry>] {
        &self.registries
    }
}

impl Node for MultipleRegistry {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        self.registries.iter().any(|r| r.is_available())
    }

    fn destroy(&self) {
        for r in &self.registries {
            r.destroy();
        }
    }
}

#[async_trait]
impl Registry for MultipleRegistry {
    async fn register(&self, url: URL) -> Result<(), RPCError> {
        for r in &self.registries {
            r.register(url.clone()).await?;
        }
        Ok(())
    }

    async fn unregister(&self, url: URL) -> Result<(), RPCError> {
        for r in &self.registries {
            r.unregister(url.clone()).await?;
        }
        Ok(())
    }

    async fn subscribe(&self, url: URL, listener: Arc<dyn NotifyListener>) -> Result<(), RPCError> {
        for r in &self.registries {
            r.subscribe(url.clone(), listener.clone()).await?;
        }
        Ok(())
    }

    async fn unsubscribe(
        &self,
        url: URL,
        listener: Arc<dyn NotifyListener>,
    ) -> Result<(), RPCError> {
        for r in &self.registries {
            r.unsubscribe(url.clone(), listener.clone()).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_event_add() {
        let url = URL::new("dubbo", "/com.example.Service");
        let event = ServiceEvent::Add(vec![url.clone()]);
        match event {
            ServiceEvent::Add(urls) => {
                assert_eq!(urls.len(), 1);
                assert_eq!(urls[0].path, "/com.example.Service");
            }
            _ => panic!("expected Add variant"),
        }
    }

    #[test]
    fn test_service_event_remove() {
        let url = URL::new("dubbo", "/com.example.Service");
        let event = ServiceEvent::Remove(vec![url]);
        assert!(matches!(event, ServiceEvent::Remove(_)));
    }

    #[test]
    fn test_service_event_clone_eq() {
        let url = URL::new("tri", "/test");
        let a = ServiceEvent::Update(vec![url.clone()]);
        let b = a.clone();
        assert_eq!(a, b);
    }

    struct DummyRegistry {
        url: URL,
    }

    impl Node for DummyRegistry {
        fn get_url(&self) -> &URL {
            &self.url
        }

        fn is_available(&self) -> bool {
            true
        }

        fn destroy(&self) {}
    }

    #[async_trait]
    impl Registry for DummyRegistry {
        async fn register(&self, _url: URL) -> Result<(), RPCError> {
            Ok(())
        }

        async fn unregister(&self, _url: URL) -> Result<(), RPCError> {
            Ok(())
        }

        async fn subscribe(
            &self,
            _url: URL,
            _listener: Arc<dyn NotifyListener>,
        ) -> Result<(), RPCError> {
            Ok(())
        }

        async fn unsubscribe(
            &self,
            _url: URL,
            _listener: Arc<dyn NotifyListener>,
        ) -> Result<(), RPCError> {
            Ok(())
        }
    }

    #[test]
    fn test_dummy_registry_is_available() {
        let registry = DummyRegistry {
            url: URL::new("test", "/path"),
        };
        assert!(registry.is_available());
        assert_eq!(registry.get_url().path, "/path");
    }

    struct TestListener {
        url: URL,
        received_events: std::sync::Mutex<Vec<ServiceEvent>>,
    }

    #[async_trait]
    impl NotifyListener for TestListener {
        async fn notify(&self, event: ServiceEvent) {
            self.received_events.lock().unwrap().push(event);
        }

        fn listen_url(&self) -> URL {
            self.url.clone()
        }
    }

    #[test]
    fn test_notify_listener_listen_url() {
        let listener = TestListener {
            url: URL::new("tri", "/com.example.Foo"),
            received_events: std::sync::Mutex::new(Vec::new()),
        };
        assert_eq!(listener.listen_url().path, "/com.example.Foo");
    }

    #[tokio::test]
    async fn test_registry_subscribe_unsubscribe() {
        let registry = DummyRegistry {
            url: URL::new("zookeeper", "127.0.0.1:2181"),
        };
        let listener = Arc::new(TestListener {
            url: URL::new("tri", "/com.example.Foo"),
            received_events: std::sync::Mutex::new(Vec::new()),
        });

        let result = registry
            .subscribe(URL::new("tri", "/com.example.Foo"), listener.clone())
            .await;
        assert!(result.is_ok());

        let result = registry
            .unsubscribe(URL::new("tri", "/com.example.Foo"), listener)
            .await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_service_event_add_empty() {
        let event = ServiceEvent::Add(vec![]);
        match event {
            ServiceEvent::Add(urls) => {
                assert!(urls.is_empty());
            }
            _ => panic!("expected Add variant"),
        }
    }

    #[test]
    fn test_service_event_update() {
        let url_a = URL::new("dubbo", "/svc.A");
        let url_b = URL::new("dubbo", "/svc.B");
        let event = ServiceEvent::Update(vec![url_a.clone(), url_b.clone()]);
        match event {
            ServiceEvent::Update(urls) => {
                assert_eq!(urls.len(), 2);
                assert_eq!(urls[0].path, "/svc.A");
                assert_eq!(urls[1].path, "/svc.B");
            }
            _ => panic!("expected Update variant"),
        }
    }

    #[test]
    fn test_service_event_update_empty() {
        let event = ServiceEvent::Update(vec![]);
        assert!(matches!(event, ServiceEvent::Update(ref urls) if urls.is_empty()));
    }

    #[test]
    fn test_service_event_add_multiple() {
        let url_a = URL::new("tri", "/com.example.Foo");
        let url_b = URL::new("tri", "/com.example.Bar");
        let url_c = URL::new("tri", "/com.example.Baz");
        let event = ServiceEvent::Add(vec![url_a, url_b, url_c]);
        match event {
            ServiceEvent::Add(urls) => {
                assert_eq!(urls.len(), 3);
                assert_eq!(urls[0].path, "/com.example.Foo");
                assert_eq!(urls[1].path, "/com.example.Bar");
                assert_eq!(urls[2].path, "/com.example.Baz");
            }
            _ => panic!("expected Add variant"),
        }
    }

    #[test]
    fn test_service_event_remove_multiple() {
        let url_a = URL::new("dubbo", "/svc.X");
        let url_b = URL::new("dubbo", "/svc.Y");
        let event = ServiceEvent::Remove(vec![url_a.clone(), url_b.clone()]);
        match event {
            ServiceEvent::Remove(urls) => {
                assert_eq!(urls.len(), 2);
                assert_eq!(urls[0].path, "/svc.X");
                assert_eq!(urls[1].path, "/svc.Y");
            }
            _ => panic!("expected Remove variant"),
        }
    }

    #[tokio::test]
    async fn test_dummy_registry_register() {
        let registry = DummyRegistry {
            url: URL::new("test", "/reg"),
        };
        let url = URL::new("tri", "/com.example.Service");
        let result = registry.register(url).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_dummy_registry_unregister() {
        let registry = DummyRegistry {
            url: URL::new("test", "/unreg"),
        };
        let url = URL::new("tri", "/com.example.Service");
        let result = registry.unregister(url).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_dummy_registry_get_url() {
        let registry = DummyRegistry {
            url: URL::new("zookeeper", "/services"),
        };
        let url = registry.get_url();
        assert_eq!(url.protocol, "zookeeper");
        assert_eq!(url.path, "/services");
    }

    #[test]
    fn test_registry_trait_object() {
        let registry = DummyRegistry {
            url: URL::new("nacos", "localhost:8848"),
        };
        let _boxed: Box<dyn Registry> = Box::new(registry);
    }

    #[tokio::test]
    async fn test_notify_listener_notify_called() {
        let listener = TestListener {
            url: URL::new("tri", "/com.example.Foo"),
            received_events: std::sync::Mutex::new(Vec::new()),
        };

        let add_evt = ServiceEvent::Add(vec![URL::new("dubbo", "/svc")]);
        let remove_evt = ServiceEvent::Remove(vec![]);
        listener.notify(add_evt.clone()).await;
        listener.notify(remove_evt.clone()).await;

        let received = listener.received_events.lock().unwrap();
        assert_eq!(received.len(), 2);
        assert_eq!(received[0], add_evt);
        assert_eq!(received[1], remove_evt);
    }

    // =======================================================================
    // ServiceInstance tests
    // =======================================================================

    #[test]
    fn test_service_instance_new() {
        let inst = ServiceInstance::new("my-service", "127.0.0.1", 8080);
        assert_eq!(inst.service_name, "my-service");
        assert_eq!(inst.host, "127.0.0.1");
        assert_eq!(inst.port, 8080);
        assert!(inst.metadata.is_empty());
    }

    #[test]
    fn test_service_instance_with_metadata() {
        let inst = ServiceInstance::new("svc", "10.0.0.1", 9090)
            .with_metadata("zone", "us-east")
            .with_metadata("weight", "100");
        assert_eq!(inst.metadata.get("zone"), Some(&"us-east".to_string()));
        assert_eq!(inst.metadata.get("weight"), Some(&"100".to_string()));
    }

    #[test]
    fn test_service_instance_to_url() {
        let inst = ServiceInstance::new("com.example.Greet", "192.168.1.1", 20880)
            .with_metadata("timeout", "3000");
        let url = inst.to_url();
        assert_eq!(url.protocol, "dubbo");
        assert_eq!(url.ip, "192.168.1.1");
        assert_eq!(url.port, "20880");
        assert_eq!(url.path, "com.example.Greet");
        assert_eq!(url.get_param("timeout"), Some(&"3000".to_string()));
    }

    #[test]
    fn test_service_instance_from_url() {
        let mut url = URL::new("dubbo", "com.example.Foo");
        url.ip = "10.0.0.5".to_string();
        url.port = "50051".to_string();
        url.set_param("version", "2.0.0");
        let inst = ServiceInstance::from_url(&url);
        assert_eq!(inst.service_name, "com.example.Foo");
        assert_eq!(inst.host, "10.0.0.5");
        assert_eq!(inst.port, 50051);
        assert_eq!(inst.metadata.get("version"), Some(&"2.0.0".to_string()));
    }

    // =======================================================================
    // InMemoryServiceDiscovery tests
    // =======================================================================

    #[tokio::test]
    async fn test_in_memory_discovery_register() {
        let disc = InMemoryServiceDiscovery::new();
        let inst = ServiceInstance::new("svc-a", "127.0.0.1", 8080);
        disc.register_instance(inst.clone()).await.unwrap();
        let instances = disc.get_instances("svc-a").await.unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0], inst);
    }

    #[tokio::test]
    async fn test_in_memory_discovery_unregister() {
        let disc = InMemoryServiceDiscovery::new();
        let inst = ServiceInstance::new("svc-b", "127.0.0.1", 9090);
        disc.register_instance(inst.clone()).await.unwrap();
        disc.unregister_instance(inst.clone()).await.unwrap();
        let instances = disc.get_instances("svc-b").await.unwrap();
        assert!(instances.is_empty());
    }

    #[tokio::test]
    async fn test_in_memory_discovery_get_instances() {
        let disc = InMemoryServiceDiscovery::new();
        let instances = disc.get_instances("nonexistent").await.unwrap();
        assert!(instances.is_empty());

        let a = ServiceInstance::new("svc", "10.0.0.1", 1000);
        let b = ServiceInstance::new("svc", "10.0.0.2", 1001);
        disc.register_instance(a.clone()).await.unwrap();
        disc.register_instance(b.clone()).await.unwrap();
        let instances = disc.get_instances("svc").await.unwrap();
        assert_eq!(instances.len(), 2);
    }

    #[tokio::test]
    async fn test_in_memory_discovery_subscribe() {
        let disc = InMemoryServiceDiscovery::new();
        let listener = Arc::new(TestListener {
            url: URL::new("dubbo", "svc-c"),
            received_events: std::sync::Mutex::new(Vec::new()),
        });
        disc.subscribe_service("svc-c", listener.clone())
            .await
            .unwrap();

        let inst = ServiceInstance::new("svc-c", "127.0.0.1", 3000);
        disc.register_instance(inst).await.unwrap();

        {
            let events = listener.received_events.lock().unwrap();
            assert_eq!(events.len(), 1);
            assert!(matches!(&events[0], ServiceEvent::Add(urls) if urls[0].ip == "127.0.0.1"));
        }

        disc.unsubscribe_service("svc-c", listener).await.unwrap();
    }

    // =======================================================================
    // MultipleRegistry tests
    // =======================================================================

    #[tokio::test]
    async fn test_multiple_registry_delegates_register() {
        let r1 = Arc::new(DummyRegistry {
            url: URL::new("zk", "/r1"),
        });
        let r2 = Arc::new(DummyRegistry {
            url: URL::new("nacos", "/r2"),
        });
        let multi = MultipleRegistry::new(URL::new("multi", "/registry"), vec![r1, r2]);
        let url = URL::new("tri", "/com.example.Svc");
        let result = multi.register(url).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_registry_delegates_unregister() {
        let r1 = Arc::new(DummyRegistry {
            url: URL::new("zk", "/r1"),
        });
        let r2 = Arc::new(DummyRegistry {
            url: URL::new("nacos", "/r2"),
        });
        let multi = MultipleRegistry::new(URL::new("multi", "/registry"), vec![r1, r2]);
        let url = URL::new("tri", "/com.example.Svc");
        let result = multi.unregister(url).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_registry_delegates_subscribe() {
        let r1 = Arc::new(DummyRegistry {
            url: URL::new("zk", "/r1"),
        });
        let listener = Arc::new(TestListener {
            url: URL::new("tri", "/com.example.Svc"),
            received_events: std::sync::Mutex::new(Vec::new()),
        });
        let multi = MultipleRegistry::new(URL::new("multi", "/registry"), vec![r1]);
        let url = URL::new("tri", "/com.example.Svc");
        let result = multi.subscribe(url.clone(), listener.clone()).await;
        assert!(result.is_ok());
        let result = multi.unsubscribe(url, listener).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_registry_empty_registries() {
        let multi = MultipleRegistry::new(URL::new("multi", "/empty"), vec![]);
        assert!(multi.registries().is_empty());
        let url = URL::new("tri", "/test");
        assert!(multi.register(url.clone()).await.is_ok());
        assert!(multi.unregister(url.clone()).await.is_ok());
        let listener = Arc::new(TestListener {
            url: url.clone(),
            received_events: std::sync::Mutex::new(Vec::new()),
        });
        assert!(multi.subscribe(url.clone(), listener.clone()).await.is_ok());
        assert!(multi.unsubscribe(url, listener).await.is_ok());
    }
}
