pub use dubbo_rs_common;

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;

#[derive(Debug, Clone)]
pub struct InvocationContext {
    pub method_name: String,
    pub parameter_types: Vec<String>,
    pub arguments: Vec<Vec<u8>>,
    pub attachments: HashMap<String, String>,
    pub url: URL,
}

impl InvocationContext {
    #[must_use]
    pub fn new(method_name: impl Into<String>, url: URL) -> Self {
        Self {
            method_name: method_name.into(),
            parameter_types: Vec::new(),
            arguments: Vec::new(),
            attachments: HashMap::new(),
            url,
        }
    }

    #[must_use]
    pub fn with_parameter_types(mut self, types: Vec<String>) -> Self {
        self.parameter_types = types;
        self
    }

    #[must_use]
    pub fn with_arguments(mut self, args: Vec<Vec<u8>>) -> Self {
        self.arguments = args;
        self
    }

    #[must_use]
    pub fn with_attachment(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attachments.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct RPCResult {
    pub value: Option<Vec<u8>>,
    pub error: Option<RPCError>,
    pub attachments: HashMap<String, String>,
}

impl RPCResult {
    #[must_use]
    pub fn success(value: Vec<u8>) -> Self {
        Self {
            value: Some(value),
            error: None,
            attachments: HashMap::new(),
        }
    }

    #[must_use]
    pub fn from_error(error: RPCError) -> Self {
        Self {
            value: None,
            error: Some(error),
            attachments: HashMap::new(),
        }
    }

    #[must_use]
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

#[async_trait]
pub trait Protocol: Send + Sync {
    /// Export a service invoker, returning an exporter handle.
    ///
    /// # Errors
    ///
    /// Returns an error if the export fails (e.g., port conflict).
    async fn export(&self, invoker: Box<dyn Invoker>) -> Result<Box<dyn Exporter>>;

    /// Refer to a remote service, returning an invoker.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection fails.
    async fn refer(&self, url: &URL) -> Result<Box<dyn Invoker>>;

    fn destroy(&self);
}

#[async_trait]
pub trait Invoker: Node {
    /// Invoke a remote call.
    ///
    /// # Errors
    ///
    /// Returns an error if the invocation fails (network error, timeout, etc.).
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult>;
}

pub trait Exporter: Send + Sync {
    fn get_invoker(&self) -> &dyn Invoker;
    fn un_export(&self);
}

// ============================================================================
// Streaming traits
// ============================================================================

/// Server-streaming response — an async iterator over response chunks.
///
/// Allocates all response values ahead of time. For truly lazy streaming
/// (one chunk at a time over the network), protocol implementations
/// should use their own streaming types (e.g., tonic `Streaming`).
#[async_trait]
pub trait ServerStream: Send + Sync {
    /// Fetch the next chunk from the stream.
    ///
    /// Returns `None` when the stream is exhausted.
    async fn next(&mut self) -> Option<RPCResult>;
}

/// Client-streaming request sender.
///
/// Call `send()` for each request chunk, then `close_and_recv()`
/// to finalize the stream and receive the single response.
#[async_trait]
pub trait ClientStream: Send + Sync {
    /// Send one request chunk to the server.
    async fn send(&mut self, ctx: &InvocationContext) -> Result<()>;

    /// Close the send side and await the single response.
    async fn close_and_recv(&mut self) -> Result<RPCResult>;
}

/// Bidirectional streaming handle.
///
/// Supports simultaneous sending and receiving of chunks.
#[async_trait]
pub trait BidiStream: Send + Sync {
    /// Send one request chunk.
    async fn send(&mut self, ctx: &InvocationContext) -> Result<()>;

    /// Receive the next response chunk.
    async fn recv(&mut self) -> Option<RPCResult>;

    /// Close the send side.
    async fn close_send(&mut self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invocation_context_builder() {
        let url = URL::new("tri", "/com.example.GreetService");
        let ctx = InvocationContext::new("sayHello", url)
            .with_parameter_types(vec!["Ljava/lang/String;".to_string()])
            .with_arguments(vec!["world".as_bytes().to_vec()])
            .with_attachment("key1", "value1");

        assert_eq!(ctx.method_name, "sayHello");
        assert_eq!(ctx.parameter_types.len(), 1);
        assert_eq!(ctx.arguments.len(), 1);
        assert_eq!(ctx.attachments.get("key1"), Some(&"value1".to_string()));
    }

    #[test]
    fn test_rpc_result_success() {
        let result = RPCResult::success(b"hello".to_vec());
        assert!(!result.is_error());
        assert_eq!(result.value, Some(b"hello".to_vec()));
    }

    #[test]
    fn test_rpc_result_error() {
        let err = RPCError::ServiceNotFound("test".into());
        let result = RPCResult::from_error(err);
        assert!(result.is_error());
        assert!(result.value.is_none());
    }

    #[test]
    fn test_rpc_result_with_attachments() {
        let mut result = RPCResult::success(vec![]);
        result
            .attachments
            .insert("trace_id".to_string(), "abc".to_string());
        assert_eq!(result.attachments.get("trace_id"), Some(&"abc".to_string()));
    }

    #[test]
    fn test_invoker_chain() {
        let url = URL::new("tri", "/com.example.TestService");

        let invoker = TestInvoker::new(url.clone());
        assert!(invoker.is_available());
        assert_eq!(invoker.get_url().path, "/com.example.TestService");
    }

    struct TestInvoker {
        url: URL,
    }

    impl TestInvoker {
        fn new(url: URL) -> Self {
            Self { url }
        }
    }

    impl Node for TestInvoker {
        fn get_url(&self) -> &URL {
            &self.url
        }

        fn is_available(&self) -> bool {
            true
        }

        fn destroy(&self) {}
    }

    #[async_trait]
    impl Invoker for TestInvoker {
        async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult> {
            Ok(RPCResult::success(b"test_response".to_vec()))
        }
    }

    // =========================================================================
    // Test helper structs
    // =========================================================================

    struct TestExporter {
        invoker: TestInvoker,
    }

    impl TestExporter {
        fn new(url: URL) -> Self {
            Self {
                invoker: TestInvoker::new(url),
            }
        }
    }

    impl Exporter for TestExporter {
        fn get_invoker(&self) -> &dyn Invoker {
            &self.invoker
        }

        fn un_export(&self) {}
    }

    struct TestServerStream {
        items: Vec<RPCResult>,
        index: usize,
    }

    impl TestServerStream {
        fn new(items: Vec<RPCResult>) -> Self {
            Self { items, index: 0 }
        }
    }

    #[async_trait]
    impl ServerStream for TestServerStream {
        async fn next(&mut self) -> Option<RPCResult> {
            if self.index < self.items.len() {
                let result = self.items[self.index].clone();
                self.index += 1;
                Some(result)
            } else {
                None
            }
        }
    }

    struct TestClientStream {
        sent: Vec<InvocationContext>,
        response: RPCResult,
    }

    impl TestClientStream {
        fn new(response: RPCResult) -> Self {
            Self {
                sent: Vec::new(),
                response,
            }
        }
    }

    #[async_trait]
    impl ClientStream for TestClientStream {
        async fn send(&mut self, ctx: &InvocationContext) -> Result<()> {
            self.sent
                .push(InvocationContext::new(&ctx.method_name, ctx.url.clone()));
            Ok(())
        }

        async fn close_and_recv(&mut self) -> Result<RPCResult> {
            Ok(self.response.clone())
        }
    }

    struct TestBidiStream {
        sent: Vec<String>,
        received: Vec<RPCResult>,
        recv_index: usize,
        closed: bool,
    }

    impl TestBidiStream {
        fn new(received: Vec<RPCResult>) -> Self {
            Self {
                sent: Vec::new(),
                received,
                recv_index: 0,
                closed: false,
            }
        }
    }

    #[async_trait]
    impl BidiStream for TestBidiStream {
        async fn send(&mut self, ctx: &InvocationContext) -> Result<()> {
            self.sent.push(ctx.method_name.clone());
            Ok(())
        }

        async fn recv(&mut self) -> Option<RPCResult> {
            if self.recv_index < self.received.len() {
                let result = self.received[self.recv_index].clone();
                self.recv_index += 1;
                Some(result)
            } else {
                None
            }
        }

        async fn close_send(&mut self) -> Result<()> {
            self.closed = true;
            Ok(())
        }
    }

    // =========================================================================
    // New tests
    // =========================================================================

    #[test]
    fn test_invocation_context_default_state() {
        let url = URL::new("tri", "/test");
        let ctx = InvocationContext::new("test", url);

        assert_eq!(ctx.method_name, "test");
        assert!(ctx.parameter_types.is_empty());
        assert!(ctx.arguments.is_empty());
        assert!(ctx.attachments.is_empty());
    }

    #[test]
    fn test_invocation_context_builder_chain() {
        let url = URL::new("tri", "/com.example.Foo");
        let ctx = InvocationContext::new("bar", url)
            .with_parameter_types(vec!["int".to_string(), "String".to_string()])
            .with_arguments(vec![vec![1, 2, 3], vec![4, 5, 6]])
            .with_attachment("k1", "v1")
            .with_attachment("k2", "v2");

        assert_eq!(ctx.method_name, "bar");
        assert_eq!(ctx.parameter_types, vec!["int", "String"]);
        assert_eq!(ctx.arguments.len(), 2);
        assert_eq!(ctx.arguments[0], vec![1, 2, 3]);
        assert_eq!(ctx.attachments.get("k1"), Some(&"v1".to_string()));
        assert_eq!(ctx.attachments.get("k2"), Some(&"v2".to_string()));
    }

    #[test]
    fn test_rpc_result_default_attachments() {
        let success = RPCResult::success(b"ok".to_vec());
        assert!(success.attachments.is_empty());

        let err = RPCResult::from_error(RPCError::ServiceNotFound("x".into()));
        assert!(err.attachments.is_empty());
    }

    #[test]
    fn test_rpc_result_clone() {
        let original = RPCResult::success(b"data".to_vec());
        let cloned = original.clone();

        assert_eq!(original.value, cloned.value);
        assert_eq!(original.error, cloned.error);
        assert_eq!(original.attachments, cloned.attachments);
    }

    #[test]
    fn test_exporter_trait_object() {
        let url = URL::new("tri", "/com.example.ExportService");
        let exporter = TestExporter::new(url);

        let invoker = exporter.get_invoker();
        assert!(invoker.is_available());
        assert_eq!(invoker.get_url().path, "/com.example.ExportService");
    }

    #[tokio::test]
    async fn test_invoker_trait_object() {
        let url = URL::new("tri", "/com.example.InvokeService");
        let invoker: Box<dyn Invoker> = Box::new(TestInvoker::new(url));

        assert!(invoker.is_available());
        assert_eq!(invoker.get_url().path, "/com.example.InvokeService");

        let mut ctx = InvocationContext::new("echo", URL::new("tri", "/com.example.InvokeService"));
        let result = invoker.invoke(&mut ctx).await.unwrap();
        assert_eq!(result.value, Some(b"test_response".to_vec()));
    }

    #[tokio::test]
    async fn test_server_stream_trait() {
        let stream = TestServerStream::new(vec![
            RPCResult::success(b"chunk1".to_vec()),
            RPCResult::success(b"chunk2".to_vec()),
        ]);

        let mut boxed: Box<dyn ServerStream> = Box::new(stream);

        let first = boxed.next().await;
        assert!(first.is_some_and(|r| r.value == Some(b"chunk1".to_vec())));

        let second = boxed.next().await;
        assert!(second.is_some_and(|r| r.value == Some(b"chunk2".to_vec())));

        let third = boxed.next().await;
        assert!(third.is_none());
    }

    #[tokio::test]
    async fn test_client_stream_trait() {
        let response = RPCResult::success(b"final".to_vec());
        let mut stream: Box<dyn ClientStream> = Box::new(TestClientStream::new(response));

        let ctx = InvocationContext::new("push", URL::new("tri", "/svc"));
        stream.send(&ctx).await.unwrap();

        let result = stream.close_and_recv().await.unwrap();
        assert_eq!(result.value, Some(b"final".to_vec()));
    }

    #[tokio::test]
    async fn test_bidi_stream_trait() {
        let responses = vec![
            RPCResult::success(b"r1".to_vec()),
            RPCResult::success(b"r2".to_vec()),
        ];
        let mut stream: Box<dyn BidiStream> = Box::new(TestBidiStream::new(responses));

        let ctx = InvocationContext::new("msg", URL::new("tri", "/svc"));
        stream.send(&ctx).await.unwrap();

        let r1 = stream.recv().await;
        assert!(r1.is_some_and(|r| r.value == Some(b"r1".to_vec())));

        stream.close_send().await.unwrap();

        let r2 = stream.recv().await;
        assert!(r2.is_some_and(|r| r.value == Some(b"r2".to_vec())));

        let r3 = stream.recv().await;
        assert!(r3.is_none());
    }
}
