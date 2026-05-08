use std::sync::Arc;

use anyhow::Result;
use dubbo_rs_config::RootConfig;
use dubbo_rs_filter::GracefulShutdownFilter;
use dubbo_rs_registry::Registry;

#[cfg(feature = "client")]
use dubbo_rs_client::Client;
#[cfg(feature = "server")]
use dubbo_rs_server::Server;

pub struct Instance {
    config: RootConfig,
    #[cfg(feature = "server")]
    server: Option<Arc<Server>>,
    #[cfg(feature = "client")]
    client: Option<Arc<Client>>,
    shutdown_filter: Option<Arc<GracefulShutdownFilter>>,
    registries: Vec<Arc<dyn Registry>>,
    registered_urls: Vec<dubbo_rs_common::url::URL>,
}

impl Instance {
    #[must_use]
    pub fn new(config: RootConfig) -> Self {
        Self {
            config,
            #[cfg(feature = "server")]
            server: None,
            #[cfg(feature = "client")]
            client: None,
            shutdown_filter: None,
            registries: Vec::new(),
            registered_urls: Vec::new(),
        }
    }

    #[cfg(feature = "server")]
    pub fn set_provider_service(&mut self, server: Server) -> &mut Self {
        self.server = Some(Arc::new(server));
        self
    }

    #[cfg(feature = "client")]
    pub fn set_client(&mut self, client: Client) -> &mut Self {
        self.client = Some(Arc::new(client));
        self
    }

    pub fn set_shutdown_filter(&mut self, filter: Arc<GracefulShutdownFilter>) -> &mut Self {
        self.shutdown_filter = Some(filter);
        self
    }

    pub fn add_registry(&mut self, registry: Arc<dyn Registry>) -> &mut Self {
        self.registries.push(registry);
        self
    }

    pub fn add_registered_url(&mut self, url: dubbo_rs_common::url::URL) -> &mut Self {
        self.registered_urls.push(url);
        self
    }

    /// Start the server in a background task.
    ///
    /// # Errors
    /// Returns an error if the server has multiple references (Arc refcount > 1).
    pub fn start(&mut self) -> Result<()> {
        #[cfg(feature = "server")]
        if let Some(server_arc) = self.server.take() {
            let server = Arc::try_unwrap(server_arc)
                .map_err(|_| anyhow::anyhow!("server has multiple references"))?;
            tokio::spawn(async move {
                if let Err(e) = server.serve().await {
                    eprintln!("dubbo server error: {e}");
                }
            });
        }
        Ok(())
    }

    pub async fn shutdown(&self) {
        tracing::info!("Instance shutdown: starting graceful shutdown");

        if let Some(filter) = &self.shutdown_filter {
            tracing::info!("Instance shutdown: setting shutdown flag, rejecting new requests");
            filter.shutdown();
        }

        for registry in &self.registries {
            for url in &self.registered_urls {
                tracing::info!("Instance shutdown: unregistering {}", url.path);
                if let Err(e) = registry.unregister(url.clone()).await {
                    tracing::warn!("Instance shutdown: failed to unregister {}: {e}", url.path);
                }
            }
        }

        if let Some(filter) = &self.shutdown_filter {
            let timeout = std::time::Duration::from_secs(30);
            tracing::info!(
                "Instance shutdown: waiting for {} in-flight requests (timeout {:?})",
                filter.active_count(),
                timeout,
            );
            let completed = filter.wait_for_shutdown(timeout).await;
            if completed {
                tracing::info!("Instance shutdown: all in-flight requests completed");
            } else {
                tracing::warn!("Instance shutdown: timed out waiting for in-flight requests");
            }
        }

        #[cfg(feature = "server")]
        if let Some(server) = &self.server {
            tracing::info!("Instance shutdown: sending shutdown signal to server");
            server.graceful_shutdown();
        }

        tracing::info!("Instance shutdown: complete");
    }

    /// Wait for a shutdown signal (SIGTERM, SIGINT, or Ctrl+C on Unix).
    ///
    /// # Errors
    /// Returns an error if the signal handler cannot be installed.
    pub async fn wait_for_signal(self) -> Result<()> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};

            let mut sigterm = signal(SignalKind::terminate())?;
            let mut sigint = signal(SignalKind::interrupt())?;

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Received SIGINT (ctrl-c)");
                }
                _ = sigint.recv() => {
                    tracing::info!("Received SIGINT");
                }
                _ = sigterm.recv() => {
                    tracing::info!("Received SIGTERM");
                }
            }
        }

        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await?;
            tracing::info!("Received ctrl-c signal");
        }

        self.shutdown().await;
        Ok(())
    }

    #[must_use]
    pub fn config(&self) -> &RootConfig {
        &self.config
    }

    #[cfg(feature = "server")]
    #[must_use]
    pub fn server(&self) -> Option<&Server> {
        self.server.as_deref()
    }

    #[cfg(feature = "client")]
    #[must_use]
    pub fn client(&self) -> Option<&Client> {
        self.client.as_deref()
    }
}

impl Default for Instance {
    fn default() -> Self {
        Self::new(RootConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use dubbo_rs_common::url::URL;

    #[cfg(any(feature = "server", feature = "client"))]
    use dubbo_rs_config::ProtocolConfig;

    #[test]
    fn test_instance_default() {
        let instance = Instance::default();
        #[cfg(feature = "server")]
        assert!(instance.server().is_none());
        #[cfg(feature = "client")]
        assert!(instance.client().is_none());
    }

    #[test]
    fn test_instance_with_config() {
        let config = RootConfig {
            application: "test-app".into(),
            version: "1.0.0".into(),
            ..RootConfig::default()
        };
        let instance = Instance::new(config);
        assert_eq!(instance.config().application, "test-app");
        assert_eq!(instance.config().version, "1.0.0");
    }

    #[cfg(feature = "server")]
    #[test]
    fn test_instance_set_server() {
        let config = RootConfig::default();
        let mut instance = Instance::new(config);
        let server = Server::new().with_application("test");
        instance.set_provider_service(server);
        assert!(instance.server().is_some());
        assert_eq!(instance.server().unwrap().application(), "test");
    }

    #[cfg(feature = "client")]
    #[test]
    fn test_instance_set_client() {
        let config = RootConfig::default();
        let mut instance = Instance::new(config);
        let client = Client::new()
            .with_url("tri://127.0.0.1:50051/com.example.Service")
            .with_protocol_config(ProtocolConfig::new("tri", "127.0.0.1", 50051));
        instance.set_client(client);
        assert!(instance.client().is_some());
        assert_eq!(
            instance.client().unwrap().url(),
            "tri://127.0.0.1:50051/com.example.Service"
        );
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_instance_start_without_server() {
        let mut instance = Instance::default();
        let result = instance.start();
        assert!(result.is_ok());
        assert!(instance.server().is_none());
    }

    #[test]
    fn test_instance_default_config() {
        let instance = Instance::default();
        let default_config = RootConfig::default();
        assert_eq!(instance.config().application, default_config.application);
        assert_eq!(instance.config().version, default_config.version);
    }

    #[test]
    fn test_instance_config_method() {
        let config = RootConfig {
            application: "my-app".into(),
            version: "2.0.0".into(),
            ..RootConfig::default()
        };
        let instance = Instance::new(config);
        assert_eq!(instance.config().application, "my-app");
        assert_eq!(instance.config().version, "2.0.0");
    }

    #[cfg(all(feature = "server", feature = "client"))]
    #[test]
    fn test_instance_builder_pattern() {
        let config = RootConfig::default();
        let server = Server::new().with_application("builder-test");
        let client = Client::new()
            .with_url("tri://127.0.0.1:50052/com.example.BuilderService")
            .with_protocol_config(ProtocolConfig::new("tri", "127.0.0.1", 50052));
        let mut instance = Instance::new(config);
        instance.set_provider_service(server);
        instance.set_client(client);
        assert!(instance.server().is_some());
        assert!(instance.client().is_some());
        assert_eq!(instance.server().unwrap().application(), "builder-test");
        assert_eq!(
            instance.client().unwrap().url(),
            "tri://127.0.0.1:50052/com.example.BuilderService"
        );
    }

    #[test]
    fn test_instance_default_values() {
        let instance = Instance::default();
        let default_config = RootConfig::default();
        assert_eq!(instance.config().application, default_config.application);
        assert_eq!(instance.config().version, default_config.version);
        #[cfg(feature = "server")]
        assert!(instance.server().is_none());
        #[cfg(feature = "client")]
        assert!(instance.client().is_none());
    }

    #[test]
    fn test_instance_set_shutdown_filter() {
        let mut instance = Instance::default();
        let filter = Arc::new(GracefulShutdownFilter::new());
        instance.set_shutdown_filter(filter);
    }

    #[tokio::test]
    async fn test_instance_shutdown_without_filter_or_server() {
        let instance = Instance::default();
        instance.shutdown().await;
    }

    #[tokio::test]
    async fn test_instance_shutdown_flow() {
        struct MockRegistry {
            unregistered: Arc<std::sync::Mutex<Vec<String>>>,
        }

        #[async_trait]
        impl dubbo_rs_registry::Registry for MockRegistry {
            async fn register(&self, _url: URL) -> Result<(), dubbo_rs_common::error::RPCError> {
                Ok(())
            }
            async fn unregister(&self, url: URL) -> Result<(), dubbo_rs_common::error::RPCError> {
                self.unregistered.lock().unwrap().push(url.path.clone());
                Ok(())
            }
            async fn subscribe(
                &self,
                _url: URL,
                _listener: Arc<dyn dubbo_rs_registry::NotifyListener>,
            ) -> Result<(), dubbo_rs_common::error::RPCError> {
                Ok(())
            }
            async fn unsubscribe(
                &self,
                _url: URL,
                _listener: Arc<dyn dubbo_rs_registry::NotifyListener>,
            ) -> Result<(), dubbo_rs_common::error::RPCError> {
                Ok(())
            }
        }

        impl dubbo_rs_common::node::Node for MockRegistry {
            fn get_url(&self) -> &URL {
                static URL: std::sync::OnceLock<URL> = std::sync::OnceLock::new();
                URL.get_or_init(|| URL::new("mock", "/registry"))
            }
            fn is_available(&self) -> bool {
                true
            }
            fn destroy(&self) {}
        }

        let unregistered: Arc<std::sync::Mutex<Vec<String>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let registry = Arc::new(MockRegistry {
            unregistered: unregistered.clone(),
        });

        let filter = Arc::new(GracefulShutdownFilter::new());

        let mut instance = Instance::default();
        instance.set_shutdown_filter(filter.clone());
        instance.add_registry(registry);
        instance.add_registered_url(URL::new("tri", "/com.example.Service"));

        assert!(!filter.is_shutdown());

        instance.shutdown().await;

        assert!(filter.is_shutdown());
        let urls = unregistered.lock().unwrap();
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "/com.example.Service");
    }

    #[cfg(feature = "server")]
    #[tokio::test]
    async fn test_instance_shutdown_with_server() {
        let filter = Arc::new(GracefulShutdownFilter::new());
        let server = Arc::new(
            Server::new()
                .with_application("shutdown-test")
                .with_protocol_config(ProtocolConfig::new("tri", "0.0.0.0", 0)),
        );

        let mut instance = Instance::default();
        instance.set_shutdown_filter(filter.clone());

        instance.server = Some(server.clone());

        instance.shutdown().await;
        assert!(filter.is_shutdown());
    }

    #[test]
    fn test_instance_add_registry() {
        struct MinimalRegistry;

        #[async_trait]
        impl dubbo_rs_registry::Registry for MinimalRegistry {
            async fn register(&self, _url: URL) -> Result<(), dubbo_rs_common::error::RPCError> {
                Ok(())
            }
            async fn unregister(&self, _url: URL) -> Result<(), dubbo_rs_common::error::RPCError> {
                Ok(())
            }
            async fn subscribe(
                &self,
                _url: URL,
                _listener: Arc<dyn dubbo_rs_registry::NotifyListener>,
            ) -> Result<(), dubbo_rs_common::error::RPCError> {
                Ok(())
            }
            async fn unsubscribe(
                &self,
                _url: URL,
                _listener: Arc<dyn dubbo_rs_registry::NotifyListener>,
            ) -> Result<(), dubbo_rs_common::error::RPCError> {
                Ok(())
            }
        }

        impl dubbo_rs_common::node::Node for MinimalRegistry {
            fn get_url(&self) -> &URL {
                static URL: std::sync::OnceLock<URL> = std::sync::OnceLock::new();
                URL.get_or_init(|| URL::new("mock", "/registry"))
            }
            fn is_available(&self) -> bool {
                true
            }
            fn destroy(&self) {}
        }

        let mut instance = Instance::default();
        assert!(instance.registries.is_empty());

        instance.add_registry(Arc::new(MinimalRegistry));
        assert_eq!(instance.registries.len(), 1);

        instance.add_registry(Arc::new(MinimalRegistry));
        assert_eq!(instance.registries.len(), 2);
    }

    #[test]
    fn test_instance_add_registered_url() {
        let mut instance = Instance::default();
        assert!(instance.registered_urls.is_empty());

        instance.add_registered_url(URL::new("tri", "/com.example.Foo"));
        assert_eq!(instance.registered_urls.len(), 1);

        instance.add_registered_url(URL::new("tri", "/com.example.Bar"));
        assert_eq!(instance.registered_urls.len(), 2);
    }
}
