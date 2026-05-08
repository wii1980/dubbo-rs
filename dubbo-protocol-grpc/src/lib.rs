pub use dubbo_rs_common;
pub use dubbo_rs_protocol;

use anyhow::Result;
use async_trait::async_trait;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::{Exporter, InvocationContext, Invoker, Protocol, RPCResult};
use http_body_util::BodyExt;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::transport::{Channel, Endpoint};
use tower::Service as _;

pub struct GrpcInvoker {
    url: URL,
    channel: Arc<RwLock<Option<Channel>>>,
}

impl GrpcInvoker {
    #[must_use]
    pub fn from_url(url: URL) -> Self {
        Self {
            url,
            channel: Arc::new(RwLock::new(None)),
        }
    }

    #[must_use]
    pub fn get_url(&self) -> &URL {
        &self.url
    }

    /// # Errors
    ///
    /// Returns an error if the endpoint cannot be constructed or
    /// the TCP/TLS handshake fails.
    pub async fn connect(&self) -> Result<()> {
        let addr = format!("http://{}", self.url.get_address());
        let channel = Endpoint::from_shared(addr)?.connect().await?;

        let mut guard = self.channel.write().await;
        *guard = Some(channel);
        Ok(())
    }

    #[must_use]
    pub fn channel(&self) -> Arc<RwLock<Option<Channel>>> {
        Arc::clone(&self.channel)
    }
}

impl Node for GrpcInvoker {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        !self.url.ip.is_empty()
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for GrpcInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult> {
        let guard = self.channel.read().await;
        let channel = guard.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "GrpcInvoker: not connected to {} — call connect() first",
                self.url.get_address()
            )
        })?;

        let service = ctx.url.path.trim_start_matches('/');
        let path: http::uri::PathAndQuery = format!("/{service}/{}", ctx.method_name)
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid gRPC path: {e}"))?;

        let body = if ctx.arguments.is_empty() {
            vec![]
        } else {
            ctx.arguments.concat()
        };

        let grpc_frame = encode_grpc_frame(&body);

        let http_request = http::Request::builder()
            .method(http::Method::POST)
            .uri(path.to_string())
            .header("content-type", "application/grpc")
            .header("te", "trailers")
            .body(tonic::body::Body::new(http_body_util::Full::new(
                bytes::Bytes::from(grpc_frame),
            )))
            .map_err(|e| anyhow::anyhow!("failed to build request: {e}"))?;

        let mut channel_clone = channel.clone();
        let response = tower::ServiceExt::ready(&mut channel_clone)
            .await
            .map_err(|e| anyhow::anyhow!("gRPC channel not ready: {e}"))?
            .call(http_request)
            .await
            .map_err(|e| anyhow::anyhow!("gRPC call to {path} failed: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            return Err(anyhow::anyhow!("gRPC HTTP error: {status}"));
        }

        let body_bytes = collect_body(response.into_body()).await?;
        let payload = decode_grpc_frame(&body_bytes)?;
        Ok(RPCResult::success(payload))
    }
}

fn encode_grpc_frame(payload: &[u8]) -> Vec<u8> {
    let len = u32::try_from(payload.len()).unwrap_or(u32::MAX);
    let mut frame = Vec::with_capacity(5 + payload.len());
    frame.push(0);
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

fn decode_grpc_frame(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 5 {
        return Ok(vec![]);
    }
    let compressed = data[0];
    if compressed != 0 {
        return Err(anyhow::anyhow!("compressed gRPC frames not supported"));
    }
    let len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;
    if data.len() < 5 + len {
        return Ok(vec![]);
    }
    Ok(data[5..5 + len].to_vec())
}

async fn collect_body(body: tonic::body::Body) -> Result<Vec<u8>> {
    let collected = body
        .collect()
        .await
        .map_err(|e| anyhow::anyhow!("failed to read response body: {e}"))?;
    Ok(collected.to_bytes().to_vec())
}

pub struct GrpcExporter {
    invoker: Box<dyn Invoker>,
}

impl Exporter for GrpcExporter {
    fn get_invoker(&self) -> &dyn Invoker {
        self.invoker.as_ref()
    }

    fn un_export(&self) {}
}

pub struct GrpcProtocol;

impl GrpcProtocol {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn name(&self) -> &'static str {
        "grpc"
    }
}

impl Default for GrpcProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Protocol for GrpcProtocol {
    async fn export(&self, invoker: Box<dyn Invoker>) -> Result<Box<dyn Exporter>> {
        Ok(Box::new(GrpcExporter { invoker }))
    }

    async fn refer(&self, url: &URL) -> Result<Box<dyn Invoker>> {
        Ok(Box::new(GrpcInvoker::from_url(url.clone())))
    }

    fn destroy(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grpc_invoker_from_url() {
        let mut url = URL::new("grpc", "/com.example.GreetService");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();
        let invoker = GrpcInvoker::from_url(url.clone());

        assert_eq!(invoker.get_url().path, "/com.example.GreetService");
        assert_eq!(invoker.get_url().protocol, "grpc");
        assert_eq!(invoker.get_url().get_address(), "127.0.0.1:50051");
        assert!(invoker.is_available());
    }

    #[test]
    fn test_grpc_invoker_unavailable() {
        let url = URL::new("grpc", "/com.example.MissingService");
        let invoker = GrpcInvoker::from_url(url);
        assert!(!invoker.is_available());
    }

    #[test]
    fn test_grpc_invoker_address_parsing() {
        let mut url = URL::new("grpc", "/com.example.TestService");
        url.ip = "10.0.0.1".into();
        url.port = "8080".into();
        let invoker = GrpcInvoker::from_url(url);

        assert_eq!(invoker.get_url().get_address(), "10.0.0.1:8080");
    }

    #[tokio::test]
    async fn test_grpc_invoker_invoke_without_channel_returns_error() {
        let mut url = URL::new("grpc", "/com.example.GreetService");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();
        let invoker = GrpcInvoker::from_url(url.clone());
        let mut ctx = InvocationContext::new("sayHello", url);

        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not connected to 127.0.0.1:50051"));
    }

    #[test]
    fn test_grpc_protocol_creation() {
        let protocol = GrpcProtocol::new();
        assert_eq!(protocol.name(), "grpc");
    }

    #[test]
    fn test_grpc_protocol_default() {
        let protocol = GrpcProtocol;
        assert_eq!(protocol.name(), "grpc");
    }

    #[tokio::test]
    async fn test_grpc_protocol_refer_creates_invoker() {
        let protocol = GrpcProtocol::new();
        let mut url = URL::new("grpc", "/com.example.HelloService");
        url.ip = "192.168.1.1".into();
        url.port = "50051".into();

        let invoker = protocol.refer(&url).await.expect("refer should succeed");
        assert!(invoker.is_available());
        assert_eq!(invoker.get_url().path, "/com.example.HelloService");
    }

    #[tokio::test]
    async fn test_grpc_protocol_export_creates_exporter() {
        let protocol = GrpcProtocol::new();
        let mut url = URL::new("grpc", "/test");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();
        let invoker: Box<dyn Invoker> = Box::new(GrpcInvoker::from_url(url));

        let exporter = protocol
            .export(invoker)
            .await
            .expect("export should succeed");
        assert!(exporter.get_invoker().is_available());
    }

    #[test]
    fn test_grpc_exporter_get_invoker() {
        let mut url = URL::new("grpc", "/com.example.TestService");
        url.ip = "127.0.0.1".into();
        url.port = "50051".into();
        let invoker: Box<dyn Invoker> = Box::new(GrpcInvoker::from_url(url));
        let exporter = GrpcExporter { invoker };

        assert!(exporter.get_invoker().is_available());
    }

    #[test]
    fn test_encode_grpc_frame_empty() {
        let frame = encode_grpc_frame(&[]);
        assert_eq!(frame, vec![0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_encode_grpc_frame_with_payload() {
        let payload = b"hello";
        let frame = encode_grpc_frame(payload);
        assert_eq!(&frame[..5], &[0, 0, 0, 0, 5]);
        assert_eq!(&frame[5..], b"hello");
    }

    #[test]
    fn test_decode_grpc_frame_roundtrip() {
        let payload = b"test data";
        let encoded = encode_grpc_frame(payload);
        let decoded = decode_grpc_frame(&encoded).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_decode_grpc_frame_too_short() {
        let result = decode_grpc_frame(&[0, 0, 0]);
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_decode_grpc_frame_compressed_returns_error() {
        let mut frame = encode_grpc_frame(b"data");
        frame[0] = 1;
        assert!(decode_grpc_frame(&frame).is_err());
    }
}
