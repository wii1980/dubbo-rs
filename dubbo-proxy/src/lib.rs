pub use dubbo_rs_cluster;
pub use dubbo_rs_common;
pub use dubbo_rs_protocol;

use std::sync::Arc;

use anyhow::Result;
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::Invoker;

type InvokerFactoryFn = dyn Fn(&URL) -> Result<Box<dyn Invoker>> + Send + Sync;

pub trait ProxyFactory: Send + Sync {
    /// Create a client proxy for the given URL.
    ///
    /// # Errors
    ///
    /// Returns an error if the invoker cannot be created (e.g. connection failure).
    fn get_proxy(&self, url: &URL) -> Result<Box<dyn Invoker>>;

    fn get_invoker(&self, invoker: Box<dyn Invoker>) -> Box<dyn Invoker>;
}

pub struct DefaultProxyFactory {
    invoker_factory: Arc<InvokerFactoryFn>,
}

impl DefaultProxyFactory {
    pub fn new<F>(factory: F) -> Self
    where
        F: Fn(&URL) -> Result<Box<dyn Invoker>> + Send + Sync + 'static,
    {
        Self {
            invoker_factory: Arc::new(factory),
        }
    }
}

impl ProxyFactory for DefaultProxyFactory {
    fn get_proxy(&self, url: &URL) -> Result<Box<dyn Invoker>> {
        (self.invoker_factory)(url)
    }

    fn get_invoker(&self, invoker: Box<dyn Invoker>) -> Box<dyn Invoker> {
        invoker
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use dubbo_rs_common::node::Node;
    use dubbo_rs_protocol::{InvocationContext, RPCResult};

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
            Ok(RPCResult::success(b"ok".to_vec()))
        }
    }

    #[test]
    fn test_default_proxy_factory_get_proxy() {
        let expected_url = URL::new("tri", "/com.example.Service");
        let url_clone = expected_url.clone();
        let factory = DefaultProxyFactory::new(move |url| {
            assert_eq!(url.path, url_clone.path);
            Ok(Box::new(TestInvoker::new(url.clone())) as Box<dyn Invoker>)
        });

        let proxy = factory
            .get_proxy(&expected_url)
            .expect("get_proxy should succeed");
        assert!(proxy.is_available());
        assert_eq!(proxy.get_url().path, "/com.example.Service");
    }

    #[test]
    fn test_default_proxy_factory_get_invoker() {
        let factory = DefaultProxyFactory::new(|_| unreachable!());
        let invoker = Box::new(TestInvoker::new(URL::new("tri", "/test")));
        let result = factory.get_invoker(invoker);
        assert!(result.is_available());
    }

    #[test]
    fn test_proxy_factory_object_safety() {
        let factory: Box<dyn ProxyFactory> = Box::new(DefaultProxyFactory::new(|url| {
            Ok(Box::new(TestInvoker::new(url.clone())) as Box<dyn Invoker>)
        }));
        let url = URL::new("tri", "/com.example.Service");
        let proxy = factory.get_proxy(&url).expect("get_proxy should succeed");
        assert!(proxy.is_available());
    }
}
