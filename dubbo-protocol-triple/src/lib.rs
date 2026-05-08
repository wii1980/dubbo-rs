pub use dubbo_rs_common;
pub use dubbo_rs_protocol;
pub use dubbo_rs_remoting;

// Generated protobuf code from proto/triple_wrapper.proto
pub mod triple {
    tonic::include_proto!("dubbo.triple");
}

use anyhow::Result;
use async_trait::async_trait;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::{Exporter, InvocationContext, Invoker, Protocol, RPCResult};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tonic::transport::{Channel, Endpoint};

// ── Keepalive configuration ──────────────────────────────────────────────

/// HTTP/2 keepalive (PING frame) configuration for `TripleInvoker`.
#[derive(Debug, Clone)]
pub struct KeepaliveConfig {
    /// Time between HTTP/2 PING frames.
    pub interval: Duration,
    /// Time to wait for a PING ACK before considering the connection dead.
    pub timeout: Duration,
    /// Send PING frames even when there are no active streams.
    pub permit_without_calls: bool,
}

impl KeepaliveConfig {
    /// Create a config with sensible defaults (60s interval, 20s timeout).
    #[must_use]
    pub fn new() -> Self {
        Self {
            interval: Duration::from_secs(60),
            timeout: Duration::from_secs(20),
            permit_without_calls: false,
        }
    }

    /// Set the PING interval.
    #[must_use]
    pub fn with_interval(mut self, d: Duration) -> Self {
        self.interval = d;
        self
    }

    /// Set the PING ACK timeout.
    #[must_use]
    pub fn with_timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    /// Allow sending PING frames when no streams are active.
    #[must_use]
    pub fn with_permit_without_calls(mut self, allow: bool) -> Self {
        self.permit_without_calls = allow;
        self
    }

    /// Build a `KeepaliveConfig` from URL parameters.
    ///
    /// Reads `keepalive.interval` (ms), `keepalive.timeout` (ms),
    /// and `keepalive.permit_without_calls` (bool) from the URL params.
    #[must_use]
    pub fn from_url(url: &URL) -> Self {
        let interval_ms: u64 = url
            .get_param("keepalive.interval")
            .and_then(|v| v.parse().ok())
            .unwrap_or(60_000);
        let timeout_ms: u64 = url
            .get_param("keepalive.timeout")
            .and_then(|v| v.parse().ok())
            .unwrap_or(20_000);
        let permit = url
            .get_param("keepalive.permit_without_calls")
            .is_some_and(|v| v == "true");

        Self {
            interval: Duration::from_millis(interval_ms),
            timeout: Duration::from_millis(timeout_ms),
            permit_without_calls: permit,
        }
    }
}

impl Default for KeepaliveConfig {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TripleInvoker {
    url: URL,
    channel: Arc<RwLock<Option<Channel>>>,
    /// Serialization type sent in `TripleRequestWrapper`.
    /// Default: "hessian2". Set to "protobuf" for native Triple/gRPC interop.
    serialize_type: String,
    keepalive: Option<KeepaliveConfig>,
}

impl TripleInvoker {
    #[must_use]
    pub fn from_url(url: URL) -> Self {
        Self {
            url,
            channel: Arc::new(RwLock::new(None)),
            serialize_type: "hessian2".to_string(),
            keepalive: None,
        }
    }

    /// Set the serialization type for this invoker.
    ///
    /// Triple protocol uses a wrapper (TripleRequestWrapper/TripleResponseWrapper)
    /// with a `serialize_type` field. The default is `"hessian2"` for compatibility
    /// with existing dubbo-java services. Set to `"protobuf"` for native gRPC interop.
    #[must_use]
    pub fn with_serialize_type(mut self, serialize_type: impl Into<String>) -> Self {
        self.serialize_type = serialize_type.into();
        self
    }

    /// Configure HTTP/2 keepalive (PING frames) for this invoker.
    #[must_use]
    pub fn with_keepalive(mut self, config: KeepaliveConfig) -> Self {
        self.keepalive = Some(config);
        self
    }

    #[must_use]
    pub fn get_url(&self) -> &URL {
        &self.url
    }

    /// Establish a tonic Channel connection to the remote server.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be established.
    pub async fn connect(&self) -> Result<()> {
        let addr = format!("http://{}", self.url.get_address());
        let mut endpoint = Endpoint::from_shared(addr)?;

        if let Some(ref ka) = self.keepalive {
            endpoint = endpoint
                .http2_keep_alive_interval(ka.interval)
                .keep_alive_timeout(ka.timeout)
                .keep_alive_while_idle(ka.permit_without_calls)
                .tcp_keepalive(Some(ka.interval));
        }

        let channel = endpoint.connect().await?;
        let mut guard = self.channel.write().await;
        *guard = Some(channel);
        Ok(())
    }

    #[must_use]
    pub fn channel(&self) -> Arc<RwLock<Option<Channel>>> {
        Arc::clone(&self.channel)
    }
}

impl Node for TripleInvoker {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        !self.url.ip.is_empty()
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for TripleInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult> {
        let guard = self.channel.read().await;
        let channel = guard.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "TripleInvoker: not connected to {} — call connect() first",
                self.url.get_address()
            )
        })?;

        let service = ctx.url.path.trim_start_matches('/');
        let path: http::uri::PathAndQuery = format!("/{service}/{}", ctx.method_name)
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid gRPC path: {e}"))?;

        let req_wrapper = triple::TripleRequestWrapper {
            serialize_type: self.serialize_type.clone(),
            args: ctx.arguments.clone().into_iter().collect(),
            arg_types: ctx.parameter_types.clone().into_iter().collect(),
        };

        let request = tonic::Request::new(req_wrapper);
        let codec = tonic_prost::ProstCodec::<
            triple::TripleRequestWrapper,
            triple::TripleResponseWrapper,
        >::default();

        let mut grpc_client = tonic::client::Grpc::new(channel.clone());
        grpc_client
            .ready()
            .await
            .map_err(|e| anyhow::anyhow!("gRPC client not ready: {e}"))?;
        let response = grpc_client
            .unary(request, path, codec)
            .await
            .map_err(|e| anyhow::anyhow!("Triple invoke failed: {e}"))?;

        let resp_wrapper = response.into_inner();

        if resp_wrapper.r#type.is_empty() && resp_wrapper.data.is_empty() {
            return Ok(RPCResult::success(vec![]));
        }
        Ok(RPCResult::success(resp_wrapper.data))
    }
}

// ── Streaming support ─────────────────────────────────────────────────────

/// Helper to build a gRPC path from service and method.
fn make_path(ctx: &InvocationContext) -> Result<http::uri::PathAndQuery> {
    let service = ctx.url.path.trim_start_matches('/');
    let path: http::uri::PathAndQuery = format!("/{service}/{}", ctx.method_name)
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid gRPC path: {e}"))?;
    Ok(path)
}

/// Helper to build a request wrapper from invocation context.
fn make_request(ctx: &InvocationContext, serialize_type: &str) -> triple::TripleRequestWrapper {
    triple::TripleRequestWrapper {
        serialize_type: serialize_type.to_string(),
        args: ctx.arguments.clone().into_iter().collect(),
        arg_types: ctx.parameter_types.clone().into_iter().collect(),
    }
}

/// Wraps a tonic `Streaming<TripleResponseWrapper>` as a `ServerStream`.
struct TripleServerStream {
    inner: tonic::codec::Streaming<triple::TripleResponseWrapper>,
}

#[async_trait]
impl dubbo_rs_protocol::ServerStream for TripleServerStream {
    async fn next(&mut self) -> Option<RPCResult> {
        self.inner
            .message()
            .await
            .ok()
            .flatten()
            .map(|wrapper| RPCResult::success(wrapper.data))
    }
}

impl TripleInvoker {
    /// Perform a server-streaming RPC call.
    ///
    /// Sends a single request and receives a stream of responses.
    ///
    /// # Errors
    /// Returns an error if the channel is not connected, or the gRPC call fails.
    pub async fn server_streaming(
        &self,
        ctx: &InvocationContext,
    ) -> Result<Box<dyn dubbo_rs_protocol::ServerStream>> {
        let guard = self.channel.read().await;
        let channel = guard.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "TripleInvoker: not connected to {} — call connect() first",
                self.url.get_address()
            )
        })?;

        let path = make_path(ctx)?;
        let req_wrapper = make_request(ctx, &self.serialize_type);
        let request = tonic::Request::new(req_wrapper);
        let codec = tonic_prost::ProstCodec::<
            triple::TripleRequestWrapper,
            triple::TripleResponseWrapper,
        >::default();

        let mut grpc_client = tonic::client::Grpc::new(channel.clone());
        grpc_client
            .ready()
            .await
            .map_err(|e| anyhow::anyhow!("gRPC client not ready: {e}"))?;
        let response = grpc_client
            .server_streaming(request, path, codec)
            .await
            .map_err(|e| anyhow::anyhow!("Triple server streaming failed: {e}"))?;

        Ok(Box::new(TripleServerStream {
            inner: response.into_inner(),
        }))
    }

    /// Perform a client-streaming RPC call.
    ///
    /// Sends a stream of requests and receives a single response.
    ///
    /// # Errors
    /// Returns an error if the channel is not connected, or the gRPC call fails.
    pub async fn client_streaming(&self, ctxs: Vec<InvocationContext>) -> Result<RPCResult> {
        let guard = self.channel.read().await;
        let channel = guard.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "TripleInvoker: not connected to {} — call connect() first",
                self.url.get_address()
            )
        })?;

        let path = make_path(
            ctxs.first()
                .ok_or_else(|| anyhow::anyhow!("empty contexts"))?,
        )?;
        let codec = tonic_prost::ProstCodec::<
            triple::TripleRequestWrapper,
            triple::TripleResponseWrapper,
        >::default();

        let serialize_type = self.serialize_type.clone();
        let stream =
            tokio_stream::iter(
                ctxs.into_iter()
                    .map(move |ctx| triple::TripleRequestWrapper {
                        serialize_type: serialize_type.clone(),
                        args: ctx.arguments.clone().into_iter().collect(),
                        arg_types: ctx.parameter_types.clone().into_iter().collect(),
                    }),
            );

        let mut grpc_client = tonic::client::Grpc::new(channel.clone());
        grpc_client
            .ready()
            .await
            .map_err(|e| anyhow::anyhow!("gRPC client not ready: {e}"))?;
        let response = grpc_client
            .client_streaming(tonic::Request::new(stream), path, codec)
            .await
            .map_err(|e| anyhow::anyhow!("Triple client streaming failed: {e}"))?;

        let wrapper = response.into_inner();
        if wrapper.r#type.is_empty() && wrapper.data.is_empty() {
            return Ok(RPCResult::success(vec![]));
        }
        Ok(RPCResult::success(wrapper.data))
    }

    /// Perform a bidirectional streaming RPC call.
    ///
    /// Sends and receives a stream of messages.
    ///
    /// # Errors
    /// Returns an error if the channel is not connected, or the gRPC call fails.
    pub async fn bidi_streaming(
        &self,
        ctxs: Vec<InvocationContext>,
    ) -> Result<Box<dyn dubbo_rs_protocol::BidiStream>> {
        let guard = self.channel.read().await;
        let channel = guard.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "TripleInvoker: not connected to {} — call connect() first",
                self.url.get_address()
            )
        })?;

        let path = make_path(
            ctxs.first()
                .ok_or_else(|| anyhow::anyhow!("empty contexts"))?,
        )?;
        let codec = tonic_prost::ProstCodec::<
            triple::TripleRequestWrapper,
            triple::TripleResponseWrapper,
        >::default();

        let serialize_type = self.serialize_type.clone();
        let stream =
            tokio_stream::iter(
                ctxs.into_iter()
                    .map(move |ctx| triple::TripleRequestWrapper {
                        serialize_type: serialize_type.clone(),
                        args: ctx.arguments.clone().into_iter().collect(),
                        arg_types: ctx.parameter_types.clone().into_iter().collect(),
                    }),
            );

        let mut grpc_client = tonic::client::Grpc::new(channel.clone());
        grpc_client
            .ready()
            .await
            .map_err(|e| anyhow::anyhow!("gRPC client not ready: {e}"))?;

        let response = grpc_client
            .streaming(tonic::Request::new(stream), path, codec)
            .await
            .map_err(|e| anyhow::anyhow!("Triple bidi streaming failed: {e}"))?;

        let rx_stream = response.into_inner();
        Ok(Box::new(TripleBidiStream { inner: rx_stream }))
    }
}

struct TripleBidiStream {
    inner: tonic::codec::Streaming<triple::TripleResponseWrapper>,
}

#[async_trait]
impl dubbo_rs_protocol::BidiStream for TripleBidiStream {
    async fn send(&mut self, _ctx: &InvocationContext) -> Result<()> {
        Ok(())
    }

    async fn recv(&mut self) -> Option<RPCResult> {
        match self.inner.message().await {
            Ok(Some(wrapper)) => Some(RPCResult::success(wrapper.data)),
            _ => None,
        }
    }

    async fn close_send(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct TripleExporter {
    invoker: Box<dyn Invoker>,
}

impl Exporter for TripleExporter {
    fn get_invoker(&self) -> &dyn Invoker {
        self.invoker.as_ref()
    }

    fn un_export(&self) {}
}

pub struct TripleProtocol;

impl TripleProtocol {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn name(&self) -> &'static str {
        "triple"
    }
}

impl Default for TripleProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Protocol for TripleProtocol {
    async fn export(&self, invoker: Box<dyn Invoker>) -> Result<Box<dyn Exporter>> {
        Ok(Box::new(TripleExporter { invoker }))
    }

    async fn refer(&self, url: &URL) -> Result<Box<dyn Invoker>> {
        let has_ka = url
            .get_param("keepalive.interval")
            .or_else(|| url.get_param("keepalive.timeout"))
            .or_else(|| url.get_param("keepalive.permit_without_calls"))
            .is_some();

        let invoker = if has_ka {
            TripleInvoker::from_url(url.clone()).with_keepalive(KeepaliveConfig::from_url(url))
        } else {
            TripleInvoker::from_url(url.clone())
        };

        Ok(Box::new(invoker))
    }

    fn destroy(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triple_invoker_from_url() {
        let mut url = URL::new("tri", "/com.example.GreetService");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();
        let invoker = TripleInvoker::from_url(url.clone());

        assert_eq!(invoker.get_url().path, "/com.example.GreetService");
        assert_eq!(invoker.get_url().protocol, "tri");
        assert!(invoker.is_available());
    }

    #[test]
    fn test_triple_invoker_unavailable() {
        let url = URL::new("tri", "/com.example.MissingService");
        let invoker = TripleInvoker::from_url(url);
        assert!(!invoker.is_available());
    }

    #[test]
    fn test_triple_protocol_name() {
        let protocol = TripleProtocol::new();
        assert_eq!(protocol.name(), "triple");
    }

    #[tokio::test]
    async fn test_triple_invoker_invoke_without_channel_returns_error() {
        let mut url = URL::new("tri", "/com.example.GreetService");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();
        let invoker = TripleInvoker::from_url(url.clone());
        let mut ctx = InvocationContext::new("sayHello", url);

        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_triple_protocol_export_creates_exporter() {
        let protocol = TripleProtocol::new();
        let mut url = URL::new("tri", "/test");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();
        let invoker: Box<dyn Invoker> = Box::new(TripleInvoker::from_url(url));

        let exporter = protocol
            .export(invoker)
            .await
            .expect("export should succeed");
        assert!(exporter.get_invoker().is_available());
    }

    #[test]
    fn test_triple_exporter_get_invoker() {
        let mut url = URL::new("tri", "/com.example.TestService");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();
        let invoker: Box<dyn Invoker> = Box::new(TripleInvoker::from_url(url));
        let exporter = TripleExporter { invoker };

        assert!(exporter.get_invoker().is_available());
    }

    #[test]
    fn test_triple_exporter_un_export() {
        let mut url = URL::new("tri", "/com.example.UnexportService");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();
        let invoker: Box<dyn Invoker> = Box::new(TripleInvoker::from_url(url));
        let exporter = TripleExporter { invoker };

        exporter.un_export();
        // Should complete without panicking
    }

    #[tokio::test]
    async fn test_triple_invoker_channel_initial_none() {
        let mut url = URL::new("tri", "/com.example.ChannelService");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();
        let invoker = TripleInvoker::from_url(url);

        let channel_lock = invoker.channel();
        let channel = channel_lock.read().await;
        assert!(channel.is_none());
    }

    #[test]
    fn test_triple_protocol_destroy() {
        let protocol = TripleProtocol::new();
        protocol.destroy();
        // Should complete without panicking
    }

    #[tokio::test]
    async fn test_triple_protocol_refer_creates_invoker() {
        let protocol = TripleProtocol::new();
        let mut url = URL::new("tri", "/com.example.ReferService");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();

        let invoker = protocol.refer(&url).await.expect("refer should succeed");

        assert!(invoker.is_available());
        assert_eq!(invoker.get_url().path, "/com.example.ReferService");
    }

    #[test]
    fn test_triple_response_wrapper_construction() {
        let resp = triple::TripleResponseWrapper {
            serialize_type: "json".into(),
            data: b"hello".to_vec(),
            r#type: "reply".into(),
        };

        assert_eq!(resp.serialize_type, "json");
        assert_eq!(resp.data, b"hello".to_vec());
        assert_eq!(resp.r#type, "reply");
    }

    #[test]
    fn test_triple_request_wrapper_construction() {
        let req = triple::TripleRequestWrapper {
            serialize_type: "json".into(),
            args: vec![b"test".to_vec()],
            arg_types: vec!["Ljava/lang/String;".into()],
        };

        assert_eq!(req.serialize_type, "json");
        assert_eq!(req.args, vec![b"test".to_vec()]);
        assert_eq!(req.arg_types, vec!["Ljava/lang/String;"]);
    }

    // ── Keepalive tests ──────────────────────────────────────────────────

    #[test]
    fn test_keepalive_config_defaults() {
        let ka = KeepaliveConfig::new();
        assert_eq!(ka.interval, Duration::from_secs(60));
        assert_eq!(ka.timeout, Duration::from_secs(20));
        assert!(!ka.permit_without_calls);

        let ka_default = KeepaliveConfig::default();
        assert_eq!(ka_default.interval, ka.interval);
        assert_eq!(ka_default.timeout, ka.timeout);
        assert_eq!(ka_default.permit_without_calls, ka.permit_without_calls);
    }

    #[test]
    fn test_keepalive_config_builder() {
        let ka = KeepaliveConfig::new()
            .with_interval(Duration::from_secs(30))
            .with_timeout(Duration::from_secs(10))
            .with_permit_without_calls(true);

        assert_eq!(ka.interval, Duration::from_secs(30));
        assert_eq!(ka.timeout, Duration::from_secs(10));
        assert!(ka.permit_without_calls);
    }

    #[test]
    fn test_triple_invoker_with_keepalive() {
        let mut url = URL::new("tri", "/com.example.KeepaliveService");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();

        let ka = KeepaliveConfig::new()
            .with_interval(Duration::from_secs(15))
            .with_timeout(Duration::from_secs(5));

        let invoker = TripleInvoker::from_url(url).with_keepalive(ka);

        assert!(invoker.keepalive.is_some());
        let config = invoker.keepalive.as_ref().unwrap();
        assert_eq!(config.interval, Duration::from_secs(15));
        assert_eq!(config.timeout, Duration::from_secs(5));
        assert!(!config.permit_without_calls);
    }

    #[test]
    fn test_triple_invoker_without_keepalive() {
        let mut url = URL::new("tri", "/com.example.NoKeepaliveService");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();
        let invoker = TripleInvoker::from_url(url);
        assert!(invoker.keepalive.is_none());
    }

    #[tokio::test]
    async fn test_keepalive_from_url_params() {
        let mut url = URL::new("tri", "/com.example.UrlKeepaliveService");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();
        url.set_param("keepalive.interval", "30000");
        url.set_param("keepalive.timeout", "10000");
        url.set_param("keepalive.permit_without_calls", "true");

        let protocol = TripleProtocol::new();
        let invoker = protocol.refer(&url).await.expect("refer should succeed");

        assert!(invoker.is_available());
        assert_eq!(
            invoker.get_url().get_param("keepalive.interval"),
            Some(&"30000".to_string())
        );
    }

    #[test]
    fn test_keepalive_config_clone() {
        let ka = KeepaliveConfig::new()
            .with_interval(Duration::from_secs(45))
            .with_timeout(Duration::from_secs(15))
            .with_permit_without_calls(true);

        let ka2 = ka.clone();
        assert_eq!(ka2.interval, Duration::from_secs(45));
        assert_eq!(ka2.timeout, Duration::from_secs(15));
        assert!(ka2.permit_without_calls);
    }
}
