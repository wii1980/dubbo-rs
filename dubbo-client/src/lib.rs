pub use dubbo_rs_common;
pub use dubbo_rs_proxy;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dubbo_rs_cluster::{Cluster, StaticDirectory};
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_config::ProtocolConfig;
use dubbo_rs_filter::{Filter, FilterChain};
use dubbo_rs_loadbalance::LoadBalance;
use dubbo_rs_protocol::{InvocationContext, Invoker, RPCResult};
use dubbo_rs_registry::Registry;
use tonic::transport::{Channel, Endpoint};

pub struct Client {
    protocol_config: Option<ProtocolConfig>,
    url: Option<String>,
    channel: Option<Channel>,
    invoker: Option<Box<dyn Invoker>>,
    filters: Vec<Box<dyn Filter>>,
    cluster: Option<Box<dyn Cluster>>,
    loadbalance: Option<Box<dyn LoadBalance>>,
    registry: Option<Box<dyn Registry>>,
}

impl Client {
    #[must_use]
    pub fn new() -> Self {
        Self {
            protocol_config: None,
            url: None,
            channel: None,
            invoker: None,
            filters: Vec::new(),
            cluster: None,
            loadbalance: None,
            registry: None,
        }
    }

    #[must_use]
    pub fn with_protocol_config(mut self, config: ProtocolConfig) -> Self {
        self.protocol_config = Some(config);
        self
    }

    #[must_use]
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Add a single filter to the client's filter chain.
    ///
    /// Filters execute in insertion order, outermost first.
    #[must_use]
    pub fn with_filter(mut self, filter: Box<dyn Filter>) -> Self {
        self.filters.push(filter);
        self
    }

    /// Add multiple filters to the client's filter chain.
    ///
    /// Filters execute in the order given (index 0 is outermost).
    #[must_use]
    pub fn with_filters(mut self, filters: Vec<Box<dyn Filter>>) -> Self {
        self.filters = filters;
        self
    }

    /// Set a cluster fault-tolerance strategy for this client.
    #[must_use]
    pub fn with_cluster(mut self, cluster: Box<dyn Cluster>) -> Self {
        self.cluster = Some(cluster);
        self
    }

    /// Set a load-balance strategy for this client.
    #[must_use]
    pub fn with_loadbalance(mut self, loadbalance: Box<dyn LoadBalance>) -> Self {
        self.loadbalance = Some(loadbalance);
        self
    }

    /// Set a registry for service discovery.
    ///
    /// When configured, the client will subscribe to the registry to
    /// discover provider addresses dynamically instead of using the
    /// single URL provided via [`with_url`](Self::with_url).
    #[must_use]
    pub fn with_registry(mut self, registry: Box<dyn Registry>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Establish a gRPC connection to the remote server.
    ///
    /// Parses the URL to extract host and port, then creates a tonic
    /// `Channel`.  If filters are configured, wraps the invoker in a
    /// [`FilterChain`]. Call this before making RPC requests.
    ///
    /// # Errors
    ///
    /// Returns an error if no URL is set, the URL is malformed, or the
    /// connection cannot be established.
    pub async fn dial(&mut self) -> Result<()> {
        let url_str = self
            .url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No URL set — call with_url() before dial()"))?;

        let (host, port) = parse_triple_url(url_str)?;
        let addr = format!("http://{host}:{port}");

        let channel = Endpoint::from_shared(addr)?.connect().await?;
        self.channel = Some(channel.clone());

        let service_path = extract_service_path(url_str);
        let mut url = URL::new("tri", &service_path);
        url.ip = host.to_string();
        url.port = port.to_string();

        let base_invoker: Box<dyn Invoker> = Box::new(TonicInvoker {
            channel,
            url: url.clone(),
        });

        // If a cluster strategy is configured, wrap the invoker in a
        // StaticDirectory and join with the cluster.
        if let Some(cluster) = self.cluster.take() {
            let dir = StaticDirectory::new(url.clone());
            let arc_invoker: Arc<dyn Invoker> = Arc::from(base_invoker);
            dir.add_invoker(arc_invoker);
            let cluster_invoker = cluster
                .join(Box::new(dir))
                .await
                .map_err(|e| anyhow::anyhow!("cluster join failed: {e}"))?;
            self.invoker = Some(cluster_invoker);
        } else if self.filters.is_empty() {
            self.invoker = Some(base_invoker);
        } else {
            let filters: Vec<Box<dyn Filter>> = std::mem::take(&mut self.filters);
            let chain = FilterChain::new(filters, base_invoker);
            self.invoker = Some(chain.build());
        }

        Ok(())
    }

    /// Return a reference to the underlying tonic `Channel`, if connected.
    #[must_use]
    pub fn channel(&self) -> Option<&Channel> {
        self.channel.as_ref()
    }

    /// Return a reference to the Dubbo `Invoker`, if connected.
    ///
    /// The invoker is wrapped with the configured filter chain.
    #[must_use]
    pub fn invoker(&self) -> Option<&dyn Invoker> {
        self.invoker.as_deref()
    }

    #[must_use]
    pub fn protocol_config(&self) -> Option<&ProtocolConfig> {
        self.protocol_config.as_ref()
    }

    #[must_use]
    pub fn url(&self) -> &str {
        self.url.as_deref().unwrap_or("")
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

/// A Dubbo [`Invoker`] backed by a tonic gRPC [`Channel`].
#[allow(dead_code)]
struct TonicInvoker {
    channel: Channel,
    url: URL,
}

impl Node for TonicInvoker {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for TonicInvoker {
    async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        Err(anyhow::anyhow!(
            "TonicInvoker does not support direct invoke. \
             Use the tonic Channel directly via Client::channel() \
             for gRPC calls, or wrap this invoker in a protocol-specific invoker."
        ))
    }
}

/// Parse a triple URL like `<tri://127.0.0.1:50051/com.example.Service>` into (host, port).
fn parse_triple_url(url_str: &str) -> Result<(&str, &str)> {
    let stripped = url_str
        .strip_prefix("tri://")
        .ok_or_else(|| anyhow::anyhow!("URL must start with 'tri://': {url_str}"))?;

    let addr_end = stripped.find('/').unwrap_or(stripped.len());
    let addr = &stripped[..addr_end];

    let (host, port) = addr
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("URL must contain host:port: {url_str}"))?;

    Ok((host, port))
}

#[must_use]
fn extract_service_path(url_str: &str) -> String {
    let stripped = url_str.strip_prefix("tri://").unwrap_or(url_str);

    if let Some(slash_pos) = stripped.find('/') {
        stripped[slash_pos..].to_string()
    } else {
        "/".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_dial_missing_url() {
        let mut client = Client::new();
        let result = client.dial().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_client_dial_invalid_url() {
        let mut client = Client::new().with_url("not-a-url");
        let result = client.dial().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_client_dial_bad_prefix() {
        let mut client = Client::new().with_url("http://127.0.0.1:50051/test");
        let result = client.dial().await;
        assert!(result.is_err());
    }

    #[test]
    fn test_client_channel_before_dial() {
        let client = Client::new().with_url("tri://127.0.0.1:50051/test");
        assert!(client.channel().is_none());
    }

    #[test]
    fn test_client_builder_default() {
        let client = Client::new();
        assert!(client.protocol_config().is_none());
    }

    #[test]
    fn test_client_builder_with_config() {
        let config = ProtocolConfig::new("tri", "127.0.0.1", 50051);
        let client = Client::new().with_protocol_config(config);
        assert_eq!(client.protocol_config().unwrap().port, 50051);
        assert_eq!(client.protocol_config().unwrap().host, "127.0.0.1");
    }

    #[test]
    fn test_client_builder_with_url() {
        let client = Client::new().with_url("tri://127.0.0.1:50051/com.example.GreetService");
        assert_eq!(
            client.url(),
            "tri://127.0.0.1:50051/com.example.GreetService"
        );
    }

    #[test]
    fn test_parse_triple_url() {
        let (host, port) =
            parse_triple_url("tri://192.168.1.1:20880/com.example.DemoService").unwrap();
        assert_eq!(host, "192.168.1.1");
        assert_eq!(port, "20880");
    }

    #[test]
    fn test_parse_triple_url_no_port() {
        let result = parse_triple_url("tri://127.0.0.1/service");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_triple_url_empty_host() {
        let (host, port) = parse_triple_url("tri://:50051/service").unwrap();
        assert_eq!(host, "");
        assert_eq!(port, "50051");
    }

    #[test]
    fn test_parse_triple_url_no_path() {
        let (host, port) = parse_triple_url("tri://127.0.0.1:50051").unwrap();
        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, "50051");
    }

    #[test]
    fn test_client_default_url() {
        let client = Client::new();
        assert_eq!(client.url(), "");
    }

    #[test]
    fn test_client_default_protocol_config() {
        let client = Client::new();
        assert!(client.protocol_config().is_none());
    }

    #[test]
    fn test_parse_triple_url_long_path() {
        let (host, port) = parse_triple_url("tri://host:8080/com/example/Service").unwrap();
        assert_eq!(host, "host");
        assert_eq!(port, "8080");
    }

    #[test]
    fn test_invoker_before_dial() {
        let client = Client::new().with_url("tri://127.0.0.1:50051/test");
        assert!(client.invoker().is_none());
    }

    #[test]
    fn test_extract_service_path() {
        assert_eq!(
            extract_service_path("tri://127.0.0.1:50051/com.example.Service"),
            "/com.example.Service"
        );
        assert_eq!(extract_service_path("tri://127.0.0.1:50051"), "/");
        assert_eq!(extract_service_path("tri://127.0.0.1:50051/"), "/");
    }

    #[test]
    fn test_with_filter_chain_builder() {
        use dubbo_rs_filter::EchoFilter;

        let client = Client::new()
            .with_url("tri://127.0.0.1:50051/test")
            .with_filter(Box::new(EchoFilter));

        assert!(client.channel().is_none());
        assert!(client.invoker().is_none());
    }

    #[test]
    fn test_client_builder_with_filters() {
        use dubbo_rs_filter::EchoFilter;

        let filters: Vec<Box<dyn Filter>> = vec![Box::new(EchoFilter)];

        let client = Client::new()
            .with_url("tri://127.0.0.1:50051/test")
            .with_filters(filters);

        assert!(client.invoker().is_none());
    }

    #[test]
    fn test_client_builder_with_cluster() {
        use dubbo_rs_cluster::FailoverCluster;

        let client = Client::new()
            .with_url("tri://127.0.0.1:50051/test")
            .with_cluster(Box::new(FailoverCluster::new().with_retries(5)));
        assert!(client.invoker().is_none());
    }

    #[test]
    fn test_client_builder_with_loadbalance() {
        use dubbo_rs_loadbalance::RandomLoadBalance;

        let client = Client::new()
            .with_url("tri://127.0.0.1:50051/test")
            .with_loadbalance(Box::new(RandomLoadBalance));
        assert!(client.invoker().is_none());
    }

    #[test]
    fn test_client_builder_with_registry() {
        let registry = TestRegistry;

        let client = Client::new()
            .with_url("tri://127.0.0.1:50051/test")
            .with_registry(Box::new(registry));
        assert!(client.invoker().is_none());
    }

    #[test]
    fn test_client_full_builder_chain() {
        use dubbo_rs_cluster::FailoverCluster;
        use dubbo_rs_filter::EchoFilter;
        use dubbo_rs_loadbalance::RandomLoadBalance;

        let client = Client::new()
            .with_url("tri://127.0.0.1:50051/com.example.Service")
            .with_protocol_config(ProtocolConfig::new("tri", "127.0.0.1", 50051))
            .with_filter(Box::new(EchoFilter))
            .with_cluster(Box::new(FailoverCluster::new()))
            .with_loadbalance(Box::new(RandomLoadBalance))
            .with_registry(Box::new(TestRegistry));

        assert_eq!(client.url(), "tri://127.0.0.1:50051/com.example.Service");
        assert_eq!(client.protocol_config().unwrap().port, 50051);
        assert!(client.invoker().is_none());
    }

    #[test]
    fn test_extract_service_path_edge_cases() {
        assert_eq!(
            extract_service_path("tri://192.168.1.1:20880/path/to/service"),
            "/path/to/service"
        );
        assert_eq!(extract_service_path("tri://host:8080"), "/");
        assert_eq!(extract_service_path("tri://host:8080/"), "/");
        assert_eq!(extract_service_path(""), "/");
    }

    // ── Test helpers ──────────────────────────────────────────────────

    use async_trait::async_trait;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_registry::Registry;

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
            _listener: std::sync::Arc<dyn dubbo_rs_registry::NotifyListener>,
        ) -> std::result::Result<(), dubbo_rs_common::error::RPCError> {
            Ok(())
        }
        async fn unsubscribe(
            &self,
            _url: dubbo_rs_common::url::URL,
            _listener: std::sync::Arc<dyn dubbo_rs_registry::NotifyListener>,
        ) -> std::result::Result<(), dubbo_rs_common::error::RPCError> {
            Ok(())
        }
    }
}
