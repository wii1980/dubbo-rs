//! High-level server API for hosting Dubbo RPC services.
//!
//! Provides a [`Server`] builder that wraps tonic's gRPC server, integrating
//! Dubbo concepts such as application metadata, protocol configuration,
//! filter chains, and registry-based service registration.
//!
//! # Quick start
//!
//! ```no_run
//! use dubbo_rs_config::ProtocolConfig;
//! use dubbo_rs_server::Server;
//!
//! # async fn run() -> anyhow::Result<()> {
//! let server = Server::new()
//!     .with_application("my-dubbo-app")
//!     .with_version("1.0.0")
//!     .with_protocol_config(ProtocolConfig::new("tri", "0.0.0.0", 50051))
//!     .register_service(|builder| {
//!         // Register tonic service implementations here via `builder.add_service(svc)`.
//!         // The closure must return a `Router`; the simplest no-op is shown below.
//!         todo!("builder.add_service(my_service::server::ServiceServer::new(impl_))")
//!     });
//!
//! server.serve().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Shutdown modes
//!
//! The server exposes three lifecycle entry points:
//!
//! - [`Server::serve`] — block until fatal error.
//! - [`Server::serve_with_graceful_shutdown`] — caller supplies a `Future`
//!   that triggers shutdown (e.g. a `ctrl_c` signal).
//! - [`Server::serve_with_internal_shutdown`] — uses an internal
//!   [`tokio::sync::watch`] channel; trigger via [`Server::graceful_shutdown`].
//!
//! For socket-activation or supervisor-driven deployments where the listener
//! is supplied externally, prefer [`Server::serve_with_listener`] or
//! [`Server::serve_with_listener_and_shutdown`].

pub use dubbo_rs_common;
pub use dubbo_rs_proxy;

use anyhow::Result;

mod conn_hook;

pub use conn_hook::{tracked_incoming, ConnectionEvent, ConnectionHook, HookedIncoming, HookedStream};
use dubbo_rs_common::url::URL;
use dubbo_rs_config::ProtocolConfig;
use dubbo_rs_filter::Filter;
use dubbo_rs_registry::Registry;
use std::future::Future;
use std::net::SocketAddr;
use tokio::sync::watch;
use tonic::transport::server::Router;
use tonic::transport::server::TcpIncoming;
use tonic::transport::Server as TonicServer;

/// Builder-pattern server hosting Dubbo RPC services on top of tonic.
///
/// A `Server` is constructed with [`Server::new`] (or [`Server::default`]),
/// progressively configured via the `with_*` builder methods, populated with
/// one or more tonic services via [`Server::register_service`], and finally
/// started with one of the `serve*` methods.
///
/// The struct owns all configuration state needed to start a tonic
/// [`Router`], including application metadata, protocol settings, the
/// filter chain, optional registry, and an internal shutdown channel.
///
/// # Examples
///
/// ```no_run
/// use dubbo_rs_config::ProtocolConfig;
/// use dubbo_rs_server::Server;
///
/// # async fn run() -> anyhow::Result<()> {
/// let server = Server::new()
///     .with_application("order-service")
///     .with_version("2.1.0")
///     .with_protocol_config(ProtocolConfig::new("tri", "0.0.0.0", 50051));
/// server.serve().await?;
/// # Ok(())
/// # }
/// ```
pub struct Server {
    application: String,
    version: String,
    protocol_config: Option<ProtocolConfig>,
    router: Option<Router>,
    filters: Vec<Box<dyn Filter>>,
    shutdown_tx: Option<watch::Sender<bool>>,
    shutdown_rx: Option<watch::Receiver<bool>>,
    registry: Option<Box<dyn Registry>>,
    service_url: Option<URL>,
    connection_hook: Option<ConnectionHook>,
}

impl Server {
    /// Create a new `Server` with no application metadata, no protocol
    /// config, no registered services, and an internal shutdown channel
    /// already paired.
    ///
    /// Equivalent to [`Server::default`].
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(false);
        Self {
            application: String::new(),
            version: String::new(),
            protocol_config: None,
            router: None,
            filters: Vec::new(),
            shutdown_tx: Some(tx),
            shutdown_rx: Some(rx),
            registry: None,
            service_url: None,
            connection_hook: None,
        }
    }

    /// Set the application name. Appears in startup logs and is intended
    /// for registry-based service registration metadata.
    #[must_use]
    pub fn with_application(mut self, name: impl Into<String>) -> Self {
        self.application = name.into();
        self
    }

    /// Set the application version. Free-form string surfaced via
    /// [`version`](Self::version) for registry metadata.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Set the protocol configuration used to derive the bind address
    /// (`host:port`) for the `serve*` entry points that perform their own
    /// bind ([`serve`](Self::serve),
    /// [`serve_with_graceful_shutdown`](Self::serve_with_graceful_shutdown),
    /// [`serve_with_internal_shutdown`](Self::serve_with_internal_shutdown)).
    ///
    /// Not consulted for binding by
    /// [`serve_with_listener`](Self::serve_with_listener) — only its absence
    /// is checked there for API consistency.
    #[must_use]
    pub fn with_protocol_config(mut self, config: ProtocolConfig) -> Self {
        self.protocol_config = Some(config);
        self
    }

    /// Add a single filter to the server's filter chain.
    #[must_use]
    pub fn with_filter(mut self, filter: Box<dyn Filter>) -> Self {
        self.filters.push(filter);
        self
    }

    /// Add multiple filters to the server's filter chain.
    #[must_use]
    pub fn with_filters(mut self, filters: Vec<Box<dyn Filter>>) -> Self {
        self.filters = filters;
        self
    }

    /// Configure a registry for automatic service registration.
    #[must_use]
    pub fn with_registry(mut self, registry: Box<dyn Registry>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Set the service URL to register with the registry.
    #[must_use]
    pub fn with_service_url(mut self, url: URL) -> Self {
        self.service_url = Some(url);
        self
    }

    /// Install a hook that is invoked when an accepted TCP connection opens
    /// and when its IO is dropped. Supervisors use this to report busy/idle
    /// without changing the transport.
    #[must_use]
    pub fn with_connection_hook<F>(mut self, hook: F) -> Self
    where
        F: Fn(ConnectionEvent) + Send + Sync + 'static,
    {
        self.connection_hook = Some(std::sync::Arc::new(hook));
        self
    }

    /// Return a reference to the configured filters.
    #[must_use]
    pub fn filters(&self) -> &[Box<dyn Filter>] {
        &self.filters
    }

    /// Register a tonic service by transforming a fresh
    /// [`tonic::transport::Server`](TonicServer) builder into a
    /// [`Router`] inside the supplied closure.
    ///
    /// The closure receives `Server::builder()` and must return a `Router`
    /// (typically via `builder.add_service(svc)`). The resulting `Router`
    /// is stored and later driven by one of the `serve*` methods.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use dubbo_rs_server::Server;
    /// let server = Server::new().register_service(|builder| {
    ///     // Add tonic services, then return the resulting `Router`.
    ///     // Example: builder.add_service(greeter_server::GreeterServer::new(impl_))
    ///     todo!("return Router built from `builder.add_service(...)`")
    /// });
    /// ```
    #[must_use]
    pub fn register_service<F>(mut self, f: F) -> Self
    where
        F: FnOnce(TonicServer) -> Router,
    {
        let builder = TonicServer::builder();
        self.router = Some(f(builder));
        self
    }

    /// # Errors
    ///
    /// Returns an error if no protocol config is set or if no services
    /// have been registered.
    pub async fn serve(self) -> Result<()> {
        let config = self
            .protocol_config
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No protocol config set"))?;

        let router = self
            .router
            .ok_or_else(|| anyhow::anyhow!("No services registered"))?;

        let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;

        println!(
            "dubbo-rs server [{}] starting on {}",
            self.application, addr
        );

        router.serve(addr).await?;
        Ok(())
    }

    /// Serve with graceful shutdown: listens for the given signal future
    /// and initiates shutdown when it resolves.
    ///
    /// # Errors
    ///
    /// Returns an error if no protocol config is set or if no services
    /// have been registered.
    pub async fn serve_with_graceful_shutdown<Fut>(self, signal: Fut) -> Result<()>
    where
        Fut: Future<Output = ()>,
    {
        let config = self
            .protocol_config
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No protocol config set"))?;

        let router = self
            .router
            .ok_or_else(|| anyhow::anyhow!("No services registered"))?;

        let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;

        println!(
            "dubbo-rs server [{}] starting on {} (with graceful shutdown)",
            self.application, addr
        );

        router.serve_with_shutdown(addr, signal).await?;
        Ok(())
    }

    /// Send the internal shutdown signal to the server.
    /// Only works if the server was started via `serve_with_internal_shutdown()`.
    pub fn graceful_shutdown(&self) {
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(true);
        }
    }

    /// Start serving with an internal shutdown channel.
    /// Call `graceful_shutdown()` to trigger shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error if no protocol config is set or if no services
    /// have been registered.
    pub async fn serve_with_internal_shutdown(self) -> Result<()> {
        let config = self
            .protocol_config
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No protocol config set"))?;

        let router = self
            .router
            .ok_or_else(|| anyhow::anyhow!("No services registered"))?;

        let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;

        let mut rx = self
            .shutdown_rx
            .ok_or_else(|| anyhow::anyhow!("No shutdown receiver available"))?;

        println!(
            "dubbo-rs server [{}] starting on {} (with internal shutdown)",
            self.application, addr
        );

        let shutdown_signal = async move {
            loop {
                if *rx.borrow_and_update() {
                    break;
                }
                if rx.changed().await.is_err() {
                    break;
                }
            }
        };

        router.serve_with_shutdown(addr, shutdown_signal).await?;
        Ok(())
    }

    /// Serve using a pre-bound `tokio::net::TcpListener`.
    ///
    /// Skips address parsing and `TcpListener::bind` entirely — the caller is
    /// responsible for binding (e.g. socket-activation, fd inheritance from a
    /// supervisor process). The `protocol_config` is not used for binding;
    /// only its absence is still checked for API consistency with `serve()`.
    ///
    /// # Errors
    ///
    /// Returns an error if no services have been registered.
    pub async fn serve_with_listener(self, listener: tokio::net::TcpListener) -> Result<()> {
        let Self {
            router,
            connection_hook,
            ..
        } = self;
        let router = router.ok_or_else(|| anyhow::anyhow!("No services registered"))?;

        let incoming = conn_hook::HookedIncoming::new(TcpIncoming::from(listener), connection_hook);
        router.serve_with_incoming(incoming).await?;
        Ok(())
    }

    /// Serve using a pre-bound `tokio::net::TcpListener`, with graceful shutdown.
    ///
    /// Same as [`serve_with_listener`](Self::serve_with_listener), but initiates
    /// graceful shutdown when `signal` resolves. Useful for supervisor-driven
    /// shutdown (SIGTERM/SIGINT) where the caller controls the listener lifecycle.
    ///
    /// # Errors
    ///
    /// Returns an error if no services have been registered.
    pub async fn serve_with_listener_and_shutdown<Fut>(
        self,
        listener: tokio::net::TcpListener,
        signal: Fut,
    ) -> Result<()>
    where
        Fut: Future<Output = ()>,
    {
        let Self {
            router,
            connection_hook,
            ..
        } = self;
        let router = router.ok_or_else(|| anyhow::anyhow!("No services registered"))?;

        let incoming = conn_hook::HookedIncoming::new(TcpIncoming::from(listener), connection_hook);
        router.serve_with_incoming_shutdown(incoming, signal).await?;
        Ok(())
    }

    /// Return a reference to the configured [`ProtocolConfig`], if any.
    #[must_use]
    pub fn protocol_config(&self) -> Option<&ProtocolConfig> {
        self.protocol_config.as_ref()
    }

    /// Return the configured application name as a string slice.
    ///
    /// Empty until [`with_application`](Self::with_application) is called.
    #[must_use]
    pub fn application(&self) -> &str {
        &self.application
    }

    /// Return the configured application version as a string slice.
    ///
    /// Empty until [`with_version`](Self::with_version) is called.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

impl Default for Server {
    /// Equivalent to [`Server::new`].
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_builder_default() {
        let server = Server::new();
        assert!(server.protocol_config().is_none());
    }

    #[test]
    fn test_server_builder_with_config() {
        let config = ProtocolConfig::new("tri", "0.0.0.0", 50051);
        let server = Server::new().with_protocol_config(config.clone());
        assert_eq!(server.protocol_config().unwrap().port, 50051);
    }

    #[test]
    fn test_server_builder_chaining() {
        let server = Server::new()
            .with_application("my-app")
            .with_version("1.0.0");
        assert_eq!(server.application(), "my-app");
        assert_eq!(server.version(), "1.0.0");
    }

    #[test]
    fn test_server_missing_config() {
        let server = Server::new().with_application("test");
        assert_eq!(server.application(), "test");
        assert!(server.protocol_config().is_none());
    }

    #[test]
    fn test_server_default_protocol_config() {
        let server = Server::new();
        assert!(server.protocol_config().is_none());
    }

    #[test]
    fn test_server_default_application() {
        let server = Server::new();
        assert_eq!(server.application(), "");
    }

    #[test]
    fn test_server_default_version() {
        let server = Server::new();
        assert_eq!(server.version(), "");
    }

    #[test]
    fn test_server_set_application_and_version() {
        let server = Server::new().with_application("test").with_version("2.0.0");
        assert_eq!(server.application(), "test");
        assert_eq!(server.version(), "2.0.0");
    }

    #[test]
    fn test_server_with_version_empty_string() {
        let server = Server::new().with_version("");
        assert_eq!(server.version(), "");
    }

    #[test]
    fn test_server_with_application_empty_string() {
        let server = Server::new().with_application("");
        assert_eq!(server.application(), "");
    }

    #[test]
    fn test_server_default_with_version() {
        let server = Server::default();
        assert_eq!(server.version(), "");
        assert_eq!(server.application(), "");
    }

    #[tokio::test]
    async fn test_server_serve_without_protocol_config() {
        let server = Server::new().with_application("test");
        let result = server.serve().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No protocol config"));
    }

    #[tokio::test]
    async fn test_server_serve_without_services() {
        let config = ProtocolConfig::new("tri", "0.0.0.0", 50051);
        let server = Server::new().with_protocol_config(config);
        let result = server.serve().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No services registered"));
    }

    #[test]
    fn test_server_filters_default() {
        let server = Server::new();
        assert!(server.filters().is_empty());
    }

    #[test]
    fn test_server_with_single_filter() {
        use dubbo_rs_filter::EchoFilter;
        let server = Server::new().with_filter(Box::new(EchoFilter));
        assert_eq!(server.filters().len(), 1);
    }

    #[test]
    fn test_server_with_multiple_filters() {
        use dubbo_rs_filter::{AccessLogFilter, EchoFilter};
        let server = Server::new()
            .with_filter(Box::new(EchoFilter))
            .with_filter(Box::new(AccessLogFilter));
        assert_eq!(server.filters().len(), 2);
    }

    #[test]
    fn test_server_with_filters_vec() {
        use dubbo_rs_filter::EchoFilter;
        let filters: Vec<Box<dyn Filter>> = vec![Box::new(EchoFilter)];
        let server = Server::new().with_filters(filters);
        assert_eq!(server.filters().len(), 1);
    }

    #[test]
    fn test_server_graceful_shutdown_sends_signal() {
        let server = Server::new();
        assert!(server.shutdown_tx.is_some());
        server.graceful_shutdown();
    }

    #[tokio::test]
    async fn test_server_serve_with_graceful_shutdown_missing_config() {
        let server = Server::new().with_application("test");
        let result = server.serve_with_graceful_shutdown(async {}).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No protocol config"));
    }

    #[tokio::test]
    async fn test_server_serve_with_graceful_shutdown_missing_services() {
        let config = ProtocolConfig::new("tri", "0.0.0.0", 50051);
        let server = Server::new().with_protocol_config(config);
        let result = server.serve_with_graceful_shutdown(async {}).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No services registered"));
    }

    #[tokio::test]
    async fn test_server_serve_with_internal_shutdown_missing_config() {
        let server = Server::new().with_application("test");
        let result = server.serve_with_internal_shutdown().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No protocol config"));
    }

    #[tokio::test]
    async fn test_server_serve_with_internal_shutdown_missing_services() {
        let config = ProtocolConfig::new("tri", "0.0.0.0", 50051);
        let server = Server::new().with_protocol_config(config);
        let result = server.serve_with_internal_shutdown().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No services registered"));
    }

    #[tokio::test]
    async fn test_server_serve_compatibility() {
        // Existing serve() still works without shutdown support (error cases)
        let server = Server::new().with_application("test");
        let result = server.serve().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No protocol config"));
    }

    #[test]
    fn test_server_with_registry_builder() {
        let registry = TestRegistry;
        let server = Server::new()
            .with_application("test-app")
            .with_registry(Box::new(registry));
        assert_eq!(server.application(), "test-app");
    }

    #[test]
    fn test_server_with_service_url_builder() {
        let url = URL::new("tri", "/com.example.Service");
        let server = Server::new()
            .with_application("test-app")
            .with_service_url(url.clone());
        assert_eq!(server.application(), "test-app");
    }

    #[test]
    fn test_server_with_registry_and_url() {
        let url = URL::new("tri", "/com.example.Service");
        let server = Server::new()
            .with_application("test-app")
            .with_registry(Box::new(TestRegistry))
            .with_service_url(url);
        assert_eq!(server.application(), "test-app");
    }

    #[test]
    fn test_server_protocol_config_full() {
        let config = ProtocolConfig::new("tri", "127.0.0.1", 50051);
        let server = Server::new().with_protocol_config(config);
        let got = server.protocol_config().expect("protocol_config should be set");
        assert_eq!(got.name, "tri");
        assert_eq!(got.host, "127.0.0.1");
        assert_eq!(got.port, 50051);
    }

    #[test]
    fn test_server_default_matches_new() {
        let new_server = Server::new();
        let default_server = Server::default();

        assert_eq!(new_server.application(), default_server.application());
        assert_eq!(new_server.version(), default_server.version());
        assert_eq!(
            new_server.protocol_config().is_none(),
            default_server.protocol_config().is_none()
        );
        assert_eq!(new_server.filters().len(), default_server.filters().len());
    }

    #[tokio::test]
    async fn test_server_serve_with_listener_missing_services() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = Server::new().with_application("test");
        let result = server.serve_with_listener(listener).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No services registered"));
    }

    #[tokio::test]
    async fn test_server_serve_with_listener_and_shutdown_missing_services() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = Server::new().with_application("test");
        let result = server
            .serve_with_listener_and_shutdown(listener, async {})
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No services registered"));
    }

    // ── Test helpers ──────────────────────────────────────────────────

    use async_trait::async_trait;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_registry::{NotifyListener, Registry};

    /// A no-op registry for testing builder methods.
    struct TestRegistry;

    impl Node for TestRegistry {
        fn get_url(&self) -> &dubbo_rs_common::url::URL {
            static DEFAULT_URL: std::sync::LazyLock<dubbo_rs_common::url::URL> =
                std::sync::LazyLock::new(|| dubbo_rs_common::url::URL::new("test", "/"));
            &DEFAULT_URL
        }
        fn is_available(&self) -> bool {
            true
        }
        fn destroy(&self) {}
    }

    #[async_trait]
    impl Registry for TestRegistry {
        async fn register(
            &self,
            _url: dubbo_rs_common::url::URL,
        ) -> std::result::Result<(), dubbo_rs_common::error::RPCError> {
            Ok(())
        }
        async fn unregister(
            &self,
            _url: dubbo_rs_common::url::URL,
        ) -> std::result::Result<(), dubbo_rs_common::error::RPCError> {
            Ok(())
        }
        async fn subscribe(
            &self,
            _url: dubbo_rs_common::url::URL,
            _listener: std::sync::Arc<dyn NotifyListener>,
        ) -> std::result::Result<(), dubbo_rs_common::error::RPCError> {
            Ok(())
        }
        async fn unsubscribe(
            &self,
            _url: dubbo_rs_common::url::URL,
            _listener: std::sync::Arc<dyn NotifyListener>,
        ) -> std::result::Result<(), dubbo_rs_common::error::RPCError> {
            Ok(())
        }
    }
}
