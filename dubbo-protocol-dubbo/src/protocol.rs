use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::{Exporter, InvocationContext, Invoker, Protocol, RPCResult};
use dubbo_rs_remoting::{ConnectionPool, ExchangeClient, ExchangeServer};
use tokio::sync::Mutex;

use crate::body;
use crate::codec::SerializationId;
use crate::transport::{DubboClient, DubboServer};

static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct DubboProtocol {
    pool: Option<Arc<dyn ConnectionPool>>,
}

impl DubboProtocol {
    #[must_use]
    pub fn new() -> Self {
        Self { pool: None }
    }

    #[must_use]
    pub fn with_pool(pool: Arc<dyn ConnectionPool>) -> Self {
        Self { pool: Some(pool) }
    }
}

impl Default for DubboProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Protocol for DubboProtocol {
    async fn export(&self, invoker: Box<dyn Invoker>) -> Result<Box<dyn Exporter>> {
        let invoker: Arc<dyn Invoker> = invoker.into();
        let url = invoker.get_url().clone();

        let server =
            Arc::new(DubboServer::new(SerializationId::Hessian2).with_invoker(invoker.clone()));
        server
            .bind(&url)
            .await
            .with_context(|| format!("failed to bind Dubbo server on {}", url.get_address()))?;

        Ok(Box::new(DubboExporter {
            invoker,
            server: Some(server),
        }))
    }

    async fn refer(&self, url: &URL) -> Result<Box<dyn Invoker>> {
        Ok(Box::new(DubboInvoker::with_pool(
            url.clone(),
            self.pool.clone(),
        )))
    }

    fn destroy(&self) {}
}

pub struct DubboInvoker {
    url: URL,
    client: Mutex<Option<DubboClient>>,
    pool: Option<Arc<dyn ConnectionPool>>,
}

impl DubboInvoker {
    #[must_use]
    pub fn new(url: URL) -> Self {
        Self {
            url,
            client: Mutex::new(None),
            pool: None,
        }
    }

    #[must_use]
    pub fn with_pool(url: URL, pool: Option<Arc<dyn ConnectionPool>>) -> Self {
        Self {
            url,
            client: Mutex::new(None),
            pool,
        }
    }
}

impl Node for DubboInvoker {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for DubboInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult> {
        let body_data = body::encode_request_body(ctx)
            .map_err(|e| anyhow::anyhow!("failed to encode request body: {e}"))?;

        let req_id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let req = dubbo_rs_remoting::Request {
            id: req_id,
            is_twoway: true,
            is_event: false,
            data: body_data,
        };

        let resp =
            if let Some(ref pool) = self.pool {
                let conn = pool.get(&self.url).await.with_context(|| {
                    format!("failed to get connection for {}", self.url.get_address())
                })?;
                conn.request(req).await.with_context(|| {
                    format!("Dubbo request failed to {}", self.url.get_address())
                })?
            } else {
                let mut guard = self.client.lock().await;
                if guard.is_none() {
                    let mut client = DubboClient::new(SerializationId::Hessian2);
                    client.connect(&self.url).await.with_context(|| {
                        format!("failed to connect to {}", self.url.get_address())
                    })?;
                    *guard = Some(client);
                }
                let client = guard.as_ref().unwrap();
                let resp = client.request(req).await.with_context(|| {
                    format!("Dubbo request failed to {}", self.url.get_address())
                })?;
                drop(guard);
                resp
            };

        if resp.is_error() {
            let err_msg = String::from_utf8_lossy(&resp.data).to_string();
            return Ok(RPCResult::from_error(
                dubbo_rs_common::error::RPCError::from_status_code(resp.status, err_msg),
            ));
        }

        body::decode_response_body(&resp.data)
            .map_err(|e| anyhow::anyhow!("failed to decode response body: {e}"))
    }
}

pub struct DubboExporter {
    invoker: Arc<dyn Invoker>,
    server: Option<Arc<DubboServer>>,
}

impl Exporter for DubboExporter {
    fn get_invoker(&self) -> &dyn Invoker {
        self.invoker.as_ref()
    }

    fn un_export(&self) {
        if let Some(ref server) = self.server {
            drop(server.close());
        }
    }
}

impl DubboExporter {
    pub async fn handle_request(&self, body_data: &[u8], base_url: &URL) -> Result<RPCResult> {
        let mut ctx = body::decode_request_body(body_data, base_url)?;
        self.invoker.invoke(&mut ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dubbo_rs_common::url::URL;

    struct TestInvoker {
        url: URL,
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
            Ok(RPCResult::success(b"stub".to_vec()))
        }
    }

    #[tokio::test]
    #[ignore = "requires network — binds to a real port"]
    async fn test_dubbo_protocol_export() {
        let protocol = DubboProtocol::new();
        let mut url = URL::new("dubbo", "/com.example.TestService");
        url.ip = "127.0.0.1".to_string();
        url.port = "0".to_string();
        let test_invoker = TestInvoker { url };

        let exporter = protocol
            .export(Box::new(test_invoker))
            .await
            .expect("export should succeed");

        let invoker = exporter.get_invoker();
        assert!(invoker.is_available());
        assert_eq!(invoker.get_url().path, "/com.example.TestService");
    }

    #[tokio::test]
    async fn test_dubbo_protocol_refer() {
        let protocol = DubboProtocol::new();
        let url = URL::new("dubbo", "/com.example.RemoteService");

        let invoker = protocol.refer(&url).await.expect("refer should succeed");

        assert!(invoker.is_available());
        assert_eq!(invoker.get_url().path, "/com.example.RemoteService");
    }

    #[test]
    fn test_dubbo_invoker_node() {
        let url = URL::new("dubbo", "/com.example.NodeTest");
        let invoker = DubboInvoker::new(url.clone());

        assert_eq!(invoker.get_url().path, "/com.example.NodeTest");
        assert!(invoker.is_available());
        invoker.destroy();
    }

    #[tokio::test]
    async fn test_dubbo_invoker_invoke_fails_without_server() {
        let url = URL::new("dubbo", "/com.example.InvokeTest");
        let invoker = DubboInvoker::new(url);

        let mut ctx = InvocationContext::new("sayHello", URL::new("dubbo", "/test"));
        ctx.arguments = vec![b"world".to_vec()];
        ctx.parameter_types = vec!["Ljava/lang/String;".to_string()];

        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "expected connection error, got success");
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("failed to connect") || err_msg.contains("connection"),
            "expected connection error, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_dubbo_invoker_invoke_requires_server() {
        let mut url = URL::new("dubbo", "/com.example.Greeter");
        url.set_param("version", "1.0.0");
        let invoker = DubboInvoker::new(url);

        let mut ctx = InvocationContext::new("sayHello", URL::new("dubbo", "/com.example.Greeter"))
            .with_parameter_types(vec!["Ljava/lang/String;".to_string()])
            .with_arguments(vec![b"dubbo-rs".to_vec()]);

        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "expected connection error, got success");
    }

    #[tokio::test]
    async fn test_exporter_handle_request_decodes_and_invokes() {
        let url = URL::new("dubbo", "/com.example.Greeter");
        let test_invoker = Arc::new(TestInvoker { url: url.clone() });
        let exporter = DubboExporter {
            invoker: test_invoker,
            server: None,
        };

        let mut ctx = InvocationContext::new("greet", url.clone())
            .with_parameter_types(vec!["Ljava/lang/String;".to_string()])
            .with_arguments(vec![b"hello".to_vec()]);
        ctx.attachments
            .insert("interface".to_string(), "com.example.Greeter".to_string());

        let body_data = body::encode_request_body(&ctx).expect("encode should succeed");
        let result = exporter
            .handle_request(&body_data, &url)
            .await
            .expect("handle_request should succeed");
        assert!(!result.is_error());
        assert_eq!(result.value, Some(b"stub".to_vec()));
    }

    #[test]
    fn test_dubbo_exporter_get_invoker() {
        let test_invoker = Arc::new(TestInvoker {
            url: URL::new("dubbo", "/com.example.ExportTest"),
        });

        let exporter = DubboExporter {
            invoker: test_invoker,
            server: None,
        };

        let invoker = exporter.get_invoker();
        assert!(invoker.is_available());
        assert_eq!(invoker.get_url().path, "/com.example.ExportTest");
    }

    #[test]
    fn test_dubbo_exporter_un_export_noop() {
        let test_invoker = Arc::new(TestInvoker {
            url: URL::new("dubbo", "/com.example.NoopTest"),
        });

        let exporter = DubboExporter {
            invoker: test_invoker,
            server: None,
        };

        exporter.un_export();
        assert!(exporter.get_invoker().is_available());
    }

    #[test]
    fn test_dubbo_protocol_default() {
        let protocol = DubboProtocol::default();
        protocol.destroy();
    }

    #[test]
    fn test_dubbo_protocol_new_and_default_equivalent() {
        let _p1 = DubboProtocol::new();
        let _p2 = DubboProtocol::default();
    }

    #[test]
    fn test_dubbo_protocol_with_pool() {
        use dubbo_rs_remoting::pool::SimpleConnectionPool;
        use dubbo_rs_remoting::{ConnectionPool, Request, Response};

        struct MockClient;

        #[async_trait]
        impl dubbo_rs_remoting::ExchangeClient for MockClient {
            async fn connect(&mut self, _url: &URL) -> Result<()> {
                Ok(())
            }
            async fn request(&self, _req: Request) -> Result<Response> {
                Ok(Response::success(1, vec![]))
            }
            fn close(&self) {}
        }

        let pool: Arc<dyn ConnectionPool> =
            Arc::new(SimpleConnectionPool::new(|| Box::new(MockClient)));
        let _protocol = DubboProtocol::with_pool(pool);
    }

    #[test]
    fn test_dubbo_invoker_with_pool_field() {
        let url = URL::new("dubbo", "/com.example.PoolTest");
        let invoker = DubboInvoker::with_pool(url.clone(), None);
        assert_eq!(invoker.get_url().path, "/com.example.PoolTest");
        assert!(invoker.is_available());
    }

    #[tokio::test]
    async fn test_dubbo_invoker_with_pool_connect_failure() {
        use dubbo_rs_remoting::pool::{PoolConfig, PooledConnectionPool};
        use dubbo_rs_remoting::{ConnectionPool, Request, Response};

        struct FailingClient;

        #[async_trait]
        impl dubbo_rs_remoting::ExchangeClient for FailingClient {
            async fn connect(&mut self, _url: &URL) -> Result<()> {
                anyhow::bail!("connection refused")
            }
            async fn request(&self, _req: Request) -> Result<Response> {
                unreachable!()
            }
            fn close(&self) {}
        }

        let pool: Arc<dyn ConnectionPool> =
            Arc::new(PooledConnectionPool::new(PoolConfig::default(), || {
                Box::new(FailingClient)
            }));
        let url = URL::new("dubbo", "127.0.0.1:9999");
        let invoker = DubboInvoker::with_pool(url, Some(pool));

        let mut ctx = InvocationContext::new("sayHello", URL::new("dubbo", "/test"));
        ctx.arguments = vec![b"world".to_vec()];
        ctx.parameter_types = vec!["Ljava/lang/String;".to_string()];

        let result = invoker.invoke(&mut ctx).await;
        assert!(result.is_err(), "should fail with connection error");
    }
}
