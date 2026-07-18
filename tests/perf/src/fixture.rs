use async_trait::async_trait;
use dubbo_rs::common::node::Node;
use dubbo_rs::common::url::URL;
use dubbo_rs::protocol::{InvocationContext, Invoker, RPCResult};

pub struct MockInvoker {
    url: URL,
}

impl MockInvoker {
    #[must_use] pub fn new(host: &str, port: &str) -> Self {
        let mut url = URL::new("tri", "/com.example.BenchmarkService");
        url.ip = host.to_string();
        url.port = port.to_string();
        Self { url }
    }

    #[must_use] pub fn with_weight(host: &str, port: &str, weight: u64) -> Self {
        let mut inv = Self::new(host, port);
        inv.url.set_param("weight", weight.to_string());
        inv
    }
}

impl Node for MockInvoker {
    fn get_url(&self) -> &URL {
        &self.url
    }
    fn is_available(&self) -> bool {
        true
    }
    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for MockInvoker {
    async fn invoke(&self, _ctx: &mut InvocationContext) -> anyhow::Result<RPCResult> {
        Ok(RPCResult::success(b"benchmark_response".to_vec()))
    }
}

#[must_use] pub fn make_invokers(count: usize) -> Vec<Box<dyn Invoker>> {
    (0..count)
        .map(|i| {
            let host = format!("192.168.1.{i}");
            let port = format!("{}", 20000 + i);
            Box::new(MockInvoker::new(&host, &port)) as Box<dyn Invoker>
        })
        .collect()
}

#[must_use] pub fn dummy_ctx() -> InvocationContext {
    let mut url = URL::new("tri", "/com.example.BenchmarkService");
    url.ip = "127.0.0.1".into();
    url.port = "50051".into();
    InvocationContext::new("sayHello", url)
}

#[must_use] pub fn ctx_with_args(key: &[u8]) -> InvocationContext {
    let mut ctx = dummy_ctx();
    ctx.arguments = vec![key.to_vec()];
    ctx
}
