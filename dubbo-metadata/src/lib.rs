use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Streaming type for RPC methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StreamType {
    /// Unary call: single request → single response.
    #[default]
    Unary,
    /// Client streaming: multiple requests → single response.
    ClientStreaming,
    /// Server streaming: single request → multiple responses.
    ServerStreaming,
    /// Bidirectional streaming: multiple requests ↔ multiple responses.
    BidiStreaming,
}

/// Per-method metadata definition.
///
/// Describes a single RPC method: its name, parameter types (Java-style
/// descriptors), return type, streaming type, and whether it is a one-way call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodDefinition {
    pub name: String,
    pub parameter_types: Vec<String>,
    pub return_type: String,
    pub stream_type: StreamType,
    pub oneway: bool,
}

impl MethodDefinition {
    #[must_use]
    pub fn new(name: impl Into<String>, return_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parameter_types: Vec::new(),
            return_type: return_type.into(),
            stream_type: StreamType::default(),
            oneway: false,
        }
    }

    #[must_use]
    pub fn with_param(mut self, param_type: impl Into<String>) -> Self {
        self.parameter_types.push(param_type.into());
        self
    }

    #[must_use]
    pub fn with_stream_type(mut self, stream_type: StreamType) -> Self {
        self.stream_type = stream_type;
        self
    }

    #[must_use]
    pub fn with_oneway(mut self, oneway: bool) -> Self {
        self.oneway = oneway;
        self
    }

    #[must_use]
    pub fn is_streaming(&self) -> bool {
        self.stream_type != StreamType::Unary
    }
}

/// Per-service metadata definition.
///
/// Describes a single Dubbo service: its interface name, version, group,
/// and the methods it exposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceDefinition {
    /// Fully-qualified interface name (e.g., "com.example.Greeter").
    pub interface: String,
    /// Service version (e.g., "1.0.0").
    pub version: String,
    /// Service group.
    pub group: String,
    /// Methods exposed by this service.
    pub methods: Vec<MethodDefinition>,
    /// Service-level parameters (key-value).
    pub params: Vec<(String, String)>,
}

impl ServiceDefinition {
    #[must_use]
    pub fn new(interface: impl Into<String>) -> Self {
        Self {
            interface: interface.into(),
            version: "1.0.0".into(),
            group: String::new(),
            methods: Vec::new(),
            params: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    #[must_use]
    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = group.into();
        self
    }

    #[must_use]
    pub fn with_method(mut self, method: MethodDefinition) -> Self {
        self.methods.push(method);
        self
    }

    #[must_use]
    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.push((key.into(), value.into()));
        self
    }

    /// Build a unique service key: `{group}/{interface}:{version}`.
    #[must_use]
    pub fn service_key(&self) -> String {
        if self.group.is_empty() {
            format!("{}:{}", self.interface, self.version)
        } else {
            format!("{}/{}:{}", self.group, self.interface, self.version)
        }
    }
}

/// Application-level metadata information.
///
/// Contains all service definitions for a single Dubbo application,
/// along with the application name and a revision number for change
/// detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataInfo {
    /// Application name.
    pub application: String,
    /// Revision number — incremented on metadata changes.
    pub revision: u64,
    /// All service definitions exported by this application.
    pub services: Vec<ServiceDefinition>,
    /// Arbitrary key-value attributes.
    pub attributes: Vec<(String, String)>,
}

impl MetadataInfo {
    #[must_use]
    pub fn new(application: impl Into<String>) -> Self {
        Self {
            application: application.into(),
            revision: 0,
            services: Vec::new(),
            attributes: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }

    #[must_use]
    pub fn with_service(mut self, service: ServiceDefinition) -> Self {
        self.services.push(service);
        self
    }

    #[must_use]
    pub fn with_attr(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.push((key.into(), value.into()));
        self
    }

    /// Find a service definition by interface name.
    #[must_use]
    pub fn find_service(&self, interface: &str) -> Option<&ServiceDefinition> {
        self.services.iter().find(|s| s.interface == interface)
    }

    /// Increment revision and return a new `MetadataInfo` with updated revision.
    #[must_use]
    pub fn bump_revision(mut self) -> Self {
        self.revision += 1;
        self
    }
}

/// Metadata storage abstraction.
///
/// Implementations may use in-memory storage, a remote metadata center,
/// or a composite of both.
pub trait MetadataStorage: Send + Sync {
    /// Store metadata for a given application.
    fn store(&self, metadata: MetadataInfo);

    /// Retrieve metadata for a given application.
    fn get(&self, application: &str) -> Option<MetadataInfo>;

    /// Remove metadata for a given application.
    fn remove(&self, application: &str) -> Option<MetadataInfo>;

    /// List all known application names.
    fn applications(&self) -> Vec<String>;
}

/// In-memory metadata storage backed by a concurrent hash map.
///
/// Suitable for local development and testing. Production deployments
/// should use a remote metadata center for cross-node visibility.
pub struct InMemoryMetadataStorage {
    store: DashMap<String, MetadataInfo>,
}

impl InMemoryMetadataStorage {
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: DashMap::new(),
        }
    }
}

impl Default for InMemoryMetadataStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataStorage for InMemoryMetadataStorage {
    fn store(&self, metadata: MetadataInfo) {
        self.store.insert(metadata.application.clone(), metadata);
    }

    fn get(&self, application: &str) -> Option<MetadataInfo> {
        self.store.get(application).map(|entry| entry.clone())
    }

    fn remove(&self, application: &str) -> Option<MetadataInfo> {
        self.store.remove(application).map(|(_, v)| v)
    }

    fn applications(&self) -> Vec<String> {
        self.store.iter().map(|entry| entry.key().clone()).collect()
    }
}

/// A no-op watcher for `ZooKeeper` connections.
///
/// Silently ignores all watched events. Used as the default watcher
/// when establishing a ZK connection for metadata storage.
struct NoopWatcher;

impl zookeeper::Watcher for NoopWatcher {
    fn handle(&self, _event: zookeeper::WatchedEvent) {}
}

/// ZooKeeper-backed metadata storage.
///
/// Stores `MetadataInfo` as JSON documents in `ZooKeeper`, organized under
/// a configurable root path (default: `/dubbo/metadata`). Each application's
/// metadata is stored at `{root_path}/{application}`.
///
/// Connections are established lazily on first access and reused across
/// subsequent operations.
pub struct ZkMetadataStorage {
    zk_addr: String,
    root_path: String,
    zk: RwLock<Option<zookeeper::ZooKeeper>>,
}

impl ZkMetadataStorage {
    /// Create a new `ZkMetadataStorage` pointing at the given `ZooKeeper` address.
    ///
    /// Uses `/dubbo/metadata` as the default root path.
    #[must_use]
    pub fn new(zk_addr: &str) -> Self {
        Self {
            zk_addr: zk_addr.to_string(),
            root_path: "/dubbo/metadata".to_string(),
            zk: RwLock::new(None),
        }
    }

    /// Set a custom root path (builder pattern).
    #[must_use]
    pub fn with_root_path(mut self, path: &str) -> Self {
        self.root_path = path.to_string();
        self
    }

    /// Ensure a ZK connection exists, creating one lazily if needed.
    fn ensure_connection(&self) -> Result<(), String> {
        {
            let guard = self.zk.read().map_err(|e| e.to_string())?;
            if guard.is_some() {
                return Ok(());
            }
        }

        let mut guard = self.zk.write().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Ok(());
        }

        let zk = zookeeper::ZooKeeper::connect(&self.zk_addr, Duration::from_secs(5), NoopWatcher)
            .map_err(|e| format!("ZK connect error: {e}"))?;

        *guard = Some(zk);
        Ok(())
    }

    /// Build the full ZK path for a given application name.
    fn app_path(&self, app: &str) -> String {
        format!("{}/{}", self.root_path, app)
    }

    /// Ensure all ancestor znodes exist along `path`, creating persistent
    /// nodes where needed.
    fn ensure_path(&self, path: &str) -> Result<(), String> {
        let guard = self.zk.read().map_err(|e| e.to_string())?;
        let zk = guard.as_ref().ok_or("ZK not connected")?;

        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = String::new();

        for part in &parts {
            current.push('/');
            current.push_str(part);

            if zk
                .exists(&current, false)
                .map_err(|e| format!("ZK exists error for {current}: {e}"))?
                .is_none()
            {
                zk.create(
                    &current,
                    Vec::new(),
                    zookeeper::Acl::open_unsafe().clone(),
                    zookeeper::CreateMode::Persistent,
                )
                .map_err(|e| format!("ZK create path error for {current}: {e}"))?;
            }
        }

        Ok(())
    }

    /// Get a reference to the inner ZK client (must be called after `ensure_connection`).
    fn with_zk<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(&zookeeper::ZooKeeper) -> Result<T, String>,
    {
        let guard = self.zk.read().map_err(|e| e.to_string())?;
        let zk = guard.as_ref().ok_or("ZK not connected")?;
        f(zk)
    }
}

impl MetadataStorage for ZkMetadataStorage {
    fn store(&self, metadata: MetadataInfo) {
        if self.ensure_connection().is_err() {
            return;
        }

        let path = self.app_path(&metadata.application);
        if self.ensure_path(&path).is_err() {
            return;
        }

        let Ok(json) = serde_json::to_vec(&metadata) else {
            return;
        };

        let _ = self.with_zk(|zk| {
            match zk.exists(&path, false) {
                Ok(Some(_)) => {
                    zk.set_data(&path, json, None)
                        .map_err(|e| format!("ZK set_data error: {e}"))?;
                }
                Ok(None) => {
                    zk.create(
                        &path,
                        json,
                        zookeeper::Acl::open_unsafe().clone(),
                        zookeeper::CreateMode::Persistent,
                    )
                    .map_err(|e| format!("ZK create error: {e}"))?;
                }
                Err(e) => return Err(format!("ZK exists error: {e}")),
            }
            Ok(())
        });
    }

    fn get(&self, application: &str) -> Option<MetadataInfo> {
        if self.ensure_connection().is_err() {
            return None;
        }

        let path = self.app_path(application);

        self.with_zk(|zk| {
            let (data, _) = zk
                .get_data(&path, false)
                .map_err(|e| format!("ZK get_data error: {e}"))?;
            let metadata: MetadataInfo = serde_json::from_slice(&data)
                .map_err(|e| format!("JSON deserialize error: {e}"))?;
            Ok(metadata)
        })
        .ok()
    }

    fn remove(&self, application: &str) -> Option<MetadataInfo> {
        let removed = self.get(application)?;
        let path = self.app_path(application);

        let _ = self.with_zk(|zk| {
            zk.delete(&path, None)
                .map_err(|e| format!("ZK delete error: {e}"))
        });

        Some(removed)
    }

    fn applications(&self) -> Vec<String> {
        if self.ensure_connection().is_err() {
            return Vec::new();
        }

        self.with_zk(|zk| {
            zk.get_children(&self.root_path, false)
                .map_err(|e| format!("ZK get_children error: {e}"))
        })
        .unwrap_or_default()
    }
}

/// Nacos-backed metadata storage.
///
/// Stores `MetadataInfo` as JSON documents in Nacos Config service,
/// using the Nacos Open API (`/nacos/v1/cs/configs`).
///
/// Each application's metadata is stored as a config entry with:
/// - `dataId`: `{data_id_prefix}{application}` (default: `dubbo.metadata.{app}`)
/// - `group`: configurable (default: `dubbo`)
/// - `namespaceId`: configurable (default: `public`)
///
/// Because Nacos lacks a direct config-list API, a local `DashMap` cache
/// tracks known application names: `store()` adds to it, `remove()` removes
/// from it, and `applications()` returns cached keys.
pub struct NacosMetadataStorage {
    server_addr: String,
    namespace: String,
    group: String,
    data_id_prefix: String,
    client: reqwest::blocking::Client,
    known_apps: DashMap<String, ()>,
}

impl NacosMetadataStorage {
    /// Create a new `NacosMetadataStorage` pointing at the given Nacos server address.
    ///
    /// Uses default values: `namespace=public`, `group=dubbo`,
    /// `data_id_prefix=dubbo.metadata.`.
    #[must_use]
    pub fn new(server_addr: &str) -> Self {
        Self {
            server_addr: server_addr.to_string(),
            namespace: "public".to_string(),
            group: "dubbo".to_string(),
            data_id_prefix: "dubbo.metadata.".to_string(),
            client: reqwest::blocking::Client::new(),
            known_apps: DashMap::new(),
        }
    }

    /// Set a custom Nacos namespace (builder pattern).
    #[must_use]
    pub fn with_namespace(mut self, ns: &str) -> Self {
        self.namespace = ns.to_string();
        self
    }

    /// Set a custom Nacos group (builder pattern).
    #[must_use]
    pub fn with_group(mut self, group: &str) -> Self {
        self.group = group.to_string();
        self
    }

    /// Set a custom data ID prefix (builder pattern).
    ///
    /// Default is `"dubbo.metadata."`. The full data ID for an application
    /// is `{prefix}{application_name}`.
    #[must_use]
    pub fn with_data_id_prefix(mut self, prefix: &str) -> Self {
        self.data_id_prefix = prefix.to_string();
        self
    }

    /// Build the Nacos data ID for a given application name.
    ///
    /// Returns `{data_id_prefix}{application}`.
    #[must_use]
    pub fn data_id_for(&self, app: &str) -> String {
        format!("{}{}", self.data_id_prefix, app)
    }
}

impl MetadataStorage for NacosMetadataStorage {
    fn store(&self, metadata: MetadataInfo) {
        let app = metadata.application.clone();
        let data_id = self.data_id_for(&app);

        let json = match serde_json::to_string(&metadata) {
            Ok(data) => data,
            Err(e) => {
                tracing::warn!("Nacos store: JSON serialization failed for app '{app}': {e}");
                return;
            }
        };

        let url = format!("{}/nacos/v1/cs/configs", self.server_addr);
        let result = self
            .client
            .post(&url)
            .form(&[
                ("dataId", data_id.as_str()),
                ("group", self.group.as_str()),
                ("content", json.as_str()),
                ("namespaceId", self.namespace.as_str()),
                ("type", "json"),
            ])
            .send();

        match result {
            Ok(resp) if resp.status().is_success() => {
                self.known_apps.insert(app, ());
            }
            Ok(resp) => {
                tracing::warn!(
                    "Nacos store: server returned status {} for app '{app}'",
                    resp.status()
                );
            }
            Err(e) => {
                tracing::warn!("Nacos store: HTTP request failed for app '{app}': {e}");
            }
        }
    }

    fn get(&self, application: &str) -> Option<MetadataInfo> {
        let data_id = self.data_id_for(application);
        let url = format!("{}/nacos/v1/cs/configs", self.server_addr);

        let resp = self
            .client
            .get(&url)
            .query(&[
                ("dataId", data_id.as_str()),
                ("group", self.group.as_str()),
                ("namespaceId", self.namespace.as_str()),
            ])
            .send();

        match resp {
            Ok(resp) if resp.status().is_success() => match resp.text() {
                Ok(text) if !text.is_empty() => serde_json::from_str::<MetadataInfo>(&text)
                    .map_err(|e| {
                        tracing::warn!(
                            "Nacos get: JSON deserialization failed for app '{application}': {e}"
                        );
                        e
                    })
                    .ok(),
                _ => None,
            },
            Ok(resp) => {
                tracing::warn!(
                    "Nacos get: server returned status {} for app '{application}'",
                    resp.status()
                );
                None
            }
            Err(e) => {
                tracing::warn!("Nacos get: HTTP request failed for app '{application}': {e}");
                None
            }
        }
    }

    fn remove(&self, application: &str) -> Option<MetadataInfo> {
        let existing = self.get(application)?;

        let data_id = self.data_id_for(application);
        let url = format!("{}/nacos/v1/cs/configs", self.server_addr);

        let result = self
            .client
            .delete(&url)
            .query(&[
                ("dataId", data_id.as_str()),
                ("group", self.group.as_str()),
                ("namespaceId", self.namespace.as_str()),
            ])
            .send();

        match result {
            Ok(resp) if resp.status().is_success() => {
                self.known_apps.remove(application);
            }
            Ok(resp) => {
                tracing::warn!(
                    "Nacos remove: server returned status {} for app '{application}'",
                    resp.status()
                );
            }
            Err(e) => {
                tracing::warn!("Nacos remove: HTTP request failed for app '{application}': {e}");
            }
        }

        Some(existing)
    }

    fn applications(&self) -> Vec<String> {
        self.known_apps.iter().map(|e| e.key().clone()).collect()
    }
}

/// The standard Dubbo MetadataService interface.
///
/// Every Dubbo provider that supports application-level service
/// discovery must export a MetadataService. Consumers query this
/// service to obtain the provider's service definitions.
#[async_trait::async_trait]
pub trait MetadataService: Send + Sync {
    /// Get the full metadata info for a given application.
    async fn get_metadata_info(&self, application: String) -> Option<MetadataInfo>;

    /// Get the JSON-encoded service definition for a given service path.
    async fn get_service_definition(&self, path: String) -> Option<String>;

    /// Get all exported service URLs (Dubbo URL format) for an application.
    async fn get_exported_service_urls(&self, application: String) -> Vec<String>;

    /// Health-check: always returns "true".
    async fn echo(&self, msg: String) -> String;
}

/// A concrete implementation of `MetadataService` backed by a `MetadataStorage`.
pub struct DefaultMetadataService {
    storage: Arc<dyn MetadataStorage>,
}

impl DefaultMetadataService {
    #[must_use]
    pub fn new(storage: Arc<dyn MetadataStorage>) -> Self {
        Self { storage }
    }
}

#[async_trait::async_trait]
impl MetadataService for DefaultMetadataService {
    async fn get_metadata_info(&self, application: String) -> Option<MetadataInfo> {
        self.storage.get(&application)
    }

    async fn get_service_definition(&self, path: String) -> Option<String> {
        for app in self.storage.applications() {
            if let Some(meta) = self.storage.get(&app) {
                for svc in &meta.services {
                    if svc.interface == path {
                        return serde_json::to_string(svc).ok();
                    }
                }
            }
        }
        None
    }

    async fn get_exported_service_urls(&self, application: String) -> Vec<String> {
        self.storage
            .get(&application)
            .map(|meta| {
                meta.services
                    .iter()
                    .map(|svc| format!("{}:{}", svc.interface, svc.version))
                    .collect()
            })
            .unwrap_or_default()
    }

    async fn echo(&self, msg: String) -> String {
        msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_method_definition_builder() {
        let method = MethodDefinition::new("sayHello", "Ljava/lang/String;")
            .with_param("Ljava/lang/String;")
            .with_oneway(false);

        assert_eq!(method.name, "sayHello");
        assert_eq!(method.return_type, "Ljava/lang/String;");
        assert_eq!(method.parameter_types, vec!["Ljava/lang/String;"]);
        assert!(!method.oneway);
    }

    #[test]
    fn test_method_definition_oneway() {
        let method = MethodDefinition::new("notify", "V").with_oneway(true);

        assert!(method.oneway);
        assert_eq!(method.return_type, "V");
    }

    #[test]
    fn test_service_definition_builder() {
        let svc = ServiceDefinition::new("com.example.Greeter")
            .with_version("1.0.0")
            .with_group("default")
            .with_method(
                MethodDefinition::new("sayHello", "Ljava/lang/String;")
                    .with_param("Ljava/lang/String;"),
            )
            .with_method(MethodDefinition::new("echo", "Ljava/lang/String;"))
            .with_param("timeout", "3000");

        assert_eq!(svc.interface, "com.example.Greeter");
        assert_eq!(svc.version, "1.0.0");
        assert_eq!(svc.group, "default");
        assert_eq!(svc.methods.len(), 2);
        assert_eq!(svc.params.len(), 1);
    }

    #[test]
    fn test_service_key_format() {
        let svc = ServiceDefinition::new("com.example.Greeter").with_version("1.0.0");
        assert_eq!(svc.service_key(), "com.example.Greeter:1.0.0");

        let svc_grouped = ServiceDefinition::new("com.example.Greeter")
            .with_version("1.0.0")
            .with_group("dev");
        assert_eq!(svc_grouped.service_key(), "dev/com.example.Greeter:1.0.0");
    }

    #[test]
    fn test_metadata_info_builder() {
        let meta = MetadataInfo::new("demo-provider")
            .with_revision(3)
            .with_service(
                ServiceDefinition::new("com.example.Greeter")
                    .with_method(MethodDefinition::new("sayHello", "V")),
            )
            .with_attr("owner", "team-a");

        assert_eq!(meta.application, "demo-provider");
        assert_eq!(meta.revision, 3);
        assert_eq!(meta.services.len(), 1);
        assert_eq!(meta.attributes.len(), 1);
    }

    #[test]
    fn test_metadata_info_find_service() {
        let meta = MetadataInfo::new("demo-provider")
            .with_service(ServiceDefinition::new("com.example.Greeter"))
            .with_service(ServiceDefinition::new("com.example.UserService"));

        assert!(meta.find_service("com.example.Greeter").is_some());
        assert!(meta.find_service("com.example.UserService").is_some());
        assert!(meta.find_service("com.example.Unknown").is_none());
    }

    #[test]
    fn test_metadata_info_bump_revision() {
        let meta = MetadataInfo::new("demo-provider")
            .with_revision(0)
            .bump_revision();

        assert_eq!(meta.revision, 1);
    }

    #[test]
    fn test_in_memory_storage_store_and_get() {
        let storage = InMemoryMetadataStorage::new();

        let meta = MetadataInfo::new("demo-provider")
            .with_service(ServiceDefinition::new("com.example.Greeter"));

        storage.store(meta.clone());

        let retrieved = storage.get("demo-provider");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().application, "demo-provider");
    }

    #[test]
    fn test_in_memory_storage_get_nonexistent() {
        let storage = InMemoryMetadataStorage::new();
        assert!(storage.get("unknown-app").is_none());
    }

    #[test]
    fn test_in_memory_storage_remove() {
        let storage = InMemoryMetadataStorage::new();
        let meta = MetadataInfo::new("demo-provider");
        storage.store(meta);

        let removed = storage.remove("demo-provider");
        assert!(removed.is_some());

        assert!(storage.get("demo-provider").is_none());
    }

    #[test]
    fn test_in_memory_storage_applications() {
        let storage = InMemoryMetadataStorage::new();

        storage.store(MetadataInfo::new("app-a"));
        storage.store(MetadataInfo::new("app-b"));
        storage.store(MetadataInfo::new("app-c"));

        let mut apps = storage.applications();
        apps.sort();
        assert_eq!(apps, vec!["app-a", "app-b", "app-c"]);
    }

    #[test]
    fn test_in_memory_storage_overwrite() {
        let storage = InMemoryMetadataStorage::new();

        let meta_v1 = MetadataInfo::new("demo-provider").with_revision(1);
        storage.store(meta_v1);

        let meta_v2 = MetadataInfo::new("demo-provider").with_revision(2);
        storage.store(meta_v2);

        let retrieved = storage.get("demo-provider").unwrap();
        assert_eq!(retrieved.revision, 2);
    }

    #[test]
    fn test_metadata_service_get_metadata_info() {
        let storage = Arc::new(InMemoryMetadataStorage::new());
        storage.store(
            MetadataInfo::new("demo-provider")
                .with_service(ServiceDefinition::new("com.example.Greeter")),
        );

        let service = DefaultMetadataService::new(storage);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(service.get_metadata_info("demo-provider".into()));
        assert!(result.is_some());
        assert_eq!(result.unwrap().services.len(), 1);
    }

    #[test]
    fn test_metadata_service_echo() {
        let storage = Arc::new(InMemoryMetadataStorage::new());
        let service = DefaultMetadataService::new(storage);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(service.echo("ping".into()));
        assert_eq!(result, "ping");
    }

    #[test]
    fn test_metadata_service_get_missing_metadata() {
        let storage = Arc::new(InMemoryMetadataStorage::new());
        let service = DefaultMetadataService::new(storage);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(service.get_metadata_info("unknown".into()));
        assert!(result.is_none());
    }

    #[test]
    fn test_metadata_service_get_exported_urls() {
        let storage = Arc::new(InMemoryMetadataStorage::new());
        storage.store(
            MetadataInfo::new("demo-provider")
                .with_service(ServiceDefinition::new("com.example.Greeter").with_version("1.0.0"))
                .with_service(
                    ServiceDefinition::new("com.example.UserService").with_version("2.0.0"),
                ),
        );

        let service = DefaultMetadataService::new(storage);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let urls = rt.block_on(service.get_exported_service_urls("demo-provider".into()));
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"com.example.Greeter:1.0.0".to_string()));
        assert!(urls.contains(&"com.example.UserService:2.0.0".to_string()));
    }

    #[test]
    fn test_metadata_service_get_service_definition() {
        let storage = Arc::new(InMemoryMetadataStorage::new());

        let svc = ServiceDefinition::new("com.example.Greeter")
            .with_version("1.0.0")
            .with_method(MethodDefinition::new("sayHello", "Ljava/lang/String;"));
        let iface = svc.interface.clone();

        storage.store(MetadataInfo::new("demo-provider").with_service(svc));

        let service = DefaultMetadataService::new(storage);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(service.get_service_definition(iface));
        assert!(result.is_some());
        let def_str = result.unwrap();
        assert!(def_str.contains("com.example.Greeter"));
        assert!(def_str.contains("1.0.0"));
    }

    #[test]
    fn test_serde_roundtrip_metadata_info() {
        let meta = MetadataInfo::new("demo-provider")
            .with_revision(5)
            .with_service(
                ServiceDefinition::new("com.example.Greeter")
                    .with_method(MethodDefinition::new("sayHello", "V")),
            );

        let json = serde_json::to_string(&meta).unwrap();
        let parsed: MetadataInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.application, meta.application);
        assert_eq!(parsed.revision, meta.revision);
        assert_eq!(parsed.services.len(), meta.services.len());
    }

    #[test]
    fn test_service_definition_empty_group() {
        let svc = ServiceDefinition::new("com.example.Foo").with_version("2.0");
        assert_eq!(svc.group, "");
        assert_eq!(svc.service_key(), "com.example.Foo:2.0");
    }

    #[test]
    fn test_method_definition_empty_params() {
        let method = MethodDefinition::new("ping", "V");
        assert!(method.parameter_types.is_empty());
    }

    #[test]
    fn test_stream_type_default_is_unary() {
        let method = MethodDefinition::new("sayHello", "V");
        assert_eq!(method.stream_type, StreamType::Unary);
        assert!(!method.is_streaming());
    }

    #[test]
    fn test_method_definition_streaming() {
        let method =
            MethodDefinition::new("upload", "V").with_stream_type(StreamType::ClientStreaming);
        assert_eq!(method.stream_type, StreamType::ClientStreaming);
        assert!(method.is_streaming());
    }

    #[test]
    fn test_stream_type_all_variants() {
        let variants = [
            (StreamType::Unary, false),
            (StreamType::ClientStreaming, true),
            (StreamType::ServerStreaming, true),
            (StreamType::BidiStreaming, true),
        ];

        for (st, expected) in variants {
            let method = MethodDefinition::new("m", "V").with_stream_type(st);
            assert_eq!(method.is_streaming(), expected);
        }
    }

    #[test]
    fn test_zk_metadata_storage_new() {
        let storage = ZkMetadataStorage::new("127.0.0.1:2181");
        assert_eq!(storage.zk_addr, "127.0.0.1:2181");
        assert_eq!(storage.root_path, "/dubbo/metadata");
        assert!(storage.zk.read().unwrap().is_none());
    }

    #[test]
    fn test_zk_metadata_storage_with_root_path() {
        let storage = ZkMetadataStorage::new("127.0.0.1:2181").with_root_path("/custom/metadata");
        assert_eq!(storage.root_path, "/custom/metadata");
    }

    #[test]
    fn test_zk_metadata_storage_app_path() {
        let storage = ZkMetadataStorage::new("127.0.0.1:2181");
        assert_eq!(
            storage.app_path("demo-provider"),
            "/dubbo/metadata/demo-provider"
        );

        let storage_custom = ZkMetadataStorage::new("127.0.0.1:2181").with_root_path("/myapp/meta");
        assert_eq!(storage_custom.app_path("my-app"), "/myapp/meta/my-app");
    }

    #[test]
    fn test_zk_metadata_store_serialization() {
        let meta = MetadataInfo::new("demo-provider")
            .with_revision(3)
            .with_service(
                ServiceDefinition::new("com.example.Greeter")
                    .with_version("1.0.0")
                    .with_method(
                        MethodDefinition::new("sayHello", "Ljava/lang/String;")
                            .with_param("Ljava/lang/String;"),
                    ),
            )
            .with_attr("owner", "team-a");

        let json = serde_json::to_vec(&meta).unwrap();
        let parsed: MetadataInfo = serde_json::from_slice(&json).unwrap();

        assert_eq!(parsed.application, "demo-provider");
        assert_eq!(parsed.revision, 3);
        assert_eq!(parsed.services.len(), 1);
        assert_eq!(parsed.services[0].interface, "com.example.Greeter");
        assert_eq!(parsed.attributes.len(), 1);
    }

    #[test]
    fn test_zk_metadata_get_without_connection_returns_none() {
        let storage = ZkMetadataStorage::new("256.256.256.256:99999");
        let result = storage.get("any-app");
        assert!(result.is_none());
    }

    #[test]
    fn test_zk_metadata_remove_without_connection_returns_none() {
        let storage = ZkMetadataStorage::new("256.256.256.256:99999");
        let result = storage.remove("any-app");
        assert!(result.is_none());
    }

    #[test]
    fn test_in_memory_store_still_works() {
        let storage = InMemoryMetadataStorage::new();
        let meta = MetadataInfo::new("verify-app")
            .with_revision(42)
            .with_service(ServiceDefinition::new("com.example.VerifyService"));

        storage.store(meta);
        let retrieved = storage.get("verify-app").unwrap();
        assert_eq!(retrieved.application, "verify-app");
        assert_eq!(retrieved.revision, 42);

        let apps = storage.applications();
        assert_eq!(apps, vec!["verify-app"]);

        let removed = storage.remove("verify-app").unwrap();
        assert_eq!(removed.application, "verify-app");
        assert!(storage.get("verify-app").is_none());
    }

    #[test]
    fn test_metadata_info_json_roundtrip() {
        let meta = MetadataInfo::new("roundtrip-app")
            .with_revision(7)
            .with_service(
                ServiceDefinition::new("com.example.Svc")
                    .with_version("2.0.0")
                    .with_group("prod")
                    .with_method(
                        MethodDefinition::new("process", "V")
                            .with_param("Ljava/lang/String;")
                            .with_stream_type(StreamType::BidiStreaming),
                    )
                    .with_param("timeout", "5000"),
            )
            .with_attr("env", "production");

        let json_bytes = serde_json::to_vec(&meta).unwrap();
        let deserialized: MetadataInfo = serde_json::from_slice(&json_bytes).unwrap();

        assert_eq!(deserialized, meta);
    }

    fn start_mock_http_server(status: u16, body: impl Into<String>) -> String {
        use std::io::{Read, Write};
        let body = body.into();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        std::thread::spawn(move || {
            for _ in 0..10 {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buf = vec![0u8; 65536];
                        let _ = stream.read(&mut buf);
                        let resp = format!(
                            "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.flush();
                    }
                    Err(_) => break,
                }
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(50));
        format!("http://{addr}")
    }

    fn start_multi_mock_http_server(responses: Vec<(u16, String)>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        std::thread::spawn(move || {
            for (status, body) in responses {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buf = vec![0u8; 65536];
                        let _ = stream.read(&mut buf);
                        let resp = format!(
                            "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.flush();
                    }
                    Err(_) => break,
                }
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(50));
        format!("http://{addr}")
    }

    #[test]
    fn test_nacos_metadata_storage_new() {
        let storage = NacosMetadataStorage::new("http://127.0.0.1:8848");
        assert_eq!(storage.server_addr, "http://127.0.0.1:8848");
        assert_eq!(storage.namespace, "public");
        assert_eq!(storage.group, "dubbo");
        assert_eq!(storage.data_id_prefix, "dubbo.metadata.");
        assert!(storage.known_apps.is_empty());
    }

    #[test]
    fn test_nacos_metadata_storage_builder_chaining() {
        let storage = NacosMetadataStorage::new("http://nacos:8848")
            .with_namespace("dev")
            .with_group("my-group")
            .with_data_id_prefix("custom.");
        assert_eq!(storage.namespace, "dev");
        assert_eq!(storage.group, "my-group");
        assert_eq!(storage.data_id_prefix, "custom.");
    }

    #[test]
    fn test_nacos_metadata_storage_default_values() {
        let storage = NacosMetadataStorage::new("http://127.0.0.1:8848");
        assert_eq!(storage.namespace, "public");
        assert_eq!(storage.group, "dubbo");
        assert_eq!(storage.data_id_prefix, "dubbo.metadata.");
    }

    #[test]
    fn test_nacos_metadata_storage_data_id_format() {
        let storage = NacosMetadataStorage::new("http://127.0.0.1:8848");
        assert_eq!(
            storage.data_id_for("demo-provider"),
            "dubbo.metadata.demo-provider"
        );

        let custom =
            NacosMetadataStorage::new("http://127.0.0.1:8848").with_data_id_prefix("meta.");
        assert_eq!(custom.data_id_for("my-app"), "meta.my-app");
    }

    #[test]
    fn test_nacos_metadata_store_serialization_roundtrip() {
        let meta = MetadataInfo::new("demo-provider")
            .with_revision(3)
            .with_service(
                ServiceDefinition::new("com.example.Greeter")
                    .with_version("1.0.0")
                    .with_method(MethodDefinition::new("sayHello", "Ljava/lang/String;")),
            )
            .with_attr("owner", "team-a");

        let json = serde_json::to_string(&meta).unwrap();
        let parsed: MetadataInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, meta);
    }

    #[test]
    fn test_nacos_metadata_get_without_server_returns_none() {
        let storage = NacosMetadataStorage::new("http://127.0.0.1:1");
        let result = storage.get("any-app");
        assert!(result.is_none());
    }

    #[test]
    fn test_nacos_metadata_remove_without_server_returns_none() {
        let storage = NacosMetadataStorage::new("http://127.0.0.1:1");
        let result = storage.remove("any-app");
        assert!(result.is_none());
    }

    #[test]
    fn test_nacos_metadata_applications_cache() {
        let storage = NacosMetadataStorage::new("http://127.0.0.1:1");

        storage.known_apps.insert("app-a".to_string(), ());
        storage.known_apps.insert("app-b".to_string(), ());
        storage.known_apps.insert("app-c".to_string(), ());

        let mut apps = storage.applications();
        apps.sort();
        assert_eq!(apps, vec!["app-a", "app-b", "app-c"]);

        storage.known_apps.remove("app-b");
        let mut apps = storage.applications();
        apps.sort();
        assert_eq!(apps, vec!["app-a", "app-c"]);
    }

    #[test]
    fn test_nacos_metadata_store_caches_app_name() {
        let url = start_mock_http_server(200, "true");
        let storage = NacosMetadataStorage::new(&url);

        let meta = MetadataInfo::new("demo-provider")
            .with_revision(1)
            .with_service(ServiceDefinition::new("com.example.Greeter"));

        storage.store(meta);

        let apps = storage.applications();
        assert_eq!(apps, vec!["demo-provider"]);
    }

    #[test]
    fn test_nacos_metadata_remove_clears_cache() {
        let meta = MetadataInfo::new("demo-provider")
            .with_revision(1)
            .with_service(ServiceDefinition::new("com.example.Greeter"));
        let json = serde_json::to_string(&meta).unwrap();

        let url = start_multi_mock_http_server(vec![(200, json), (200, "true".to_string())]);

        let storage = NacosMetadataStorage::new(&url);
        storage.known_apps.insert("demo-provider".to_string(), ());

        let removed = storage.remove("demo-provider");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().application, "demo-provider");

        let apps = storage.applications();
        assert!(apps.is_empty());
    }

    #[test]
    fn test_nacos_metadata_storage_with_namespace() {
        let storage = NacosMetadataStorage::new("http://nacos:8848").with_namespace("production");
        assert_eq!(storage.namespace, "production");
    }

    #[test]
    fn test_nacos_metadata_storage_with_group() {
        let storage = NacosMetadataStorage::new("http://nacos:8848").with_group("custom-group");
        assert_eq!(storage.group, "custom-group");
    }
}
