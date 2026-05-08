use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use async_trait::async_trait;
use dubbo_rs::common::node::Node;
use dubbo_rs::common::url::URL;
use dubbo_rs::filter::{AccessLogFilter, EchoFilter, FilterChain, TokenFilter};
use dubbo_rs::protocol::{InvocationContext, Invoker, RPCResult};

struct HelloInvoker {
    url: URL,
    call_count: Arc<AtomicUsize>,
}

impl HelloInvoker {
    fn new() -> Self {
        Self {
            url: URL::new("tri", "/com.example.Greeter"),
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Node for HelloInvoker {
    fn get_url(&self) -> &URL {
        &self.url
    }
    fn is_available(&self) -> bool {
        true
    }
    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for HelloInvoker {
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let msg = format!("Hello, {}!", ctx.method_name);
        Ok(RPCResult::success(msg.as_bytes().to_vec()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let filters: Vec<Box<dyn dubbo_rs::filter::Filter>> = vec![
        Box::new(EchoFilter),
        Box::new(TokenFilter::new("secret-token")),
        Box::new(AccessLogFilter),
    ];

    let invoker = Box::new(HelloInvoker::new());
    let chain = FilterChain::new(filters, invoker);
    let chain_invoker = chain.build();

    println!("=== Demo 1: Normal call with token ===\n");
    let mut ctx = InvocationContext::new("sayHello", URL::new("tri", "/com.example.Greeter"));
    ctx.attachments
        .insert("token".to_string(), "secret-token".to_string());
    let result = chain_invoker.invoke(&mut ctx).await?;
    println!(
        "Result: {}\n",
        String::from_utf8_lossy(result.value.as_ref().unwrap())
    );

    println!("=== Demo 2: Echo health check ===\n");
    let mut ctx = InvocationContext::new("$echo", URL::new("tri", "/com.example.Greeter"));
    ctx.arguments = vec![b"ping".to_vec()];
    let result = chain_invoker.invoke(&mut ctx).await?;
    println!(
        "Result: {}\n",
        String::from_utf8_lossy(result.value.as_ref().unwrap())
    );

    println!("=== Demo 3: Missing token (should fail) ===\n");
    let mut ctx = InvocationContext::new("sayHi", URL::new("tri", "/com.example.Greeter"));
    match chain_invoker.invoke(&mut ctx).await {
        Ok(r) => println!("Unexpected success: {r:?}"),
        Err(e) => println!("Expected error: {e}\n"),
    }

    println!("=== Demo 4: Wrong token (should fail) ===\n");
    let mut ctx = InvocationContext::new("sayHi", URL::new("tri", "/com.example.Greeter"));
    ctx.attachments
        .insert("token".to_string(), "wrong-token".to_string());
    match chain_invoker.invoke(&mut ctx).await {
        Ok(r) => println!("Unexpected success: {r:?}"),
        Err(e) => println!("Expected error: {e}\n"),
    }

    println!("=== All demos complete ===");
    Ok(())
}
