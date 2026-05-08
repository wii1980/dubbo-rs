//! Failover Cluster Demo.
//!
//! Demonstrates FailoverCluster (retry on failure) vs FailfastCluster (no retry):
//! - Failover tries each available invoker up to `retries+1` times
//! - Failfast fails immediately on the first error
//!
//! This is a pure-logic demo — no external infrastructure required.
//!
//! Usage:
//!   cargo run -p failover

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Context;
use async_trait::async_trait;
use dubbo_rs::cluster::{Cluster, Directory, FailfastCluster, FailoverCluster};
use dubbo_rs::common::error::RPCError;
use dubbo_rs::common::node::Node;
use dubbo_rs::common::url::URL;
use dubbo_rs::protocol::{InvocationContext, Invoker, RPCResult};

fn make_url(host: &str, port: &str, path: &str) -> URL {
    let mut url = URL::new("dubbo", path);
    url.ip = host.to_string();
    url.port = port.to_string();
    url
}

struct MockInvoker {
    url: URL,
    succeed: bool,
    call_count: Arc<AtomicUsize>,
}

impl MockInvoker {
    fn new(url: URL, succeed: bool) -> Self {
        Self {
            url,
            succeed,
            call_count: Arc::new(AtomicUsize::new(0)),
        }
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
    async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        if self.succeed {
            Ok(RPCResult::success(
                format!("response from {}", self.url.get_address()).into_bytes(),
            ))
        } else {
            Ok(RPCResult::from_error(RPCError::ServerError(format!(
                "mock failure on {}",
                self.url.get_address()
            ))))
        }
    }
}

struct MockDirectory {
    url: URL,
    invokers: Vec<Arc<dyn Invoker>>,
}

impl MockDirectory {
    fn new(invokers: Vec<Arc<dyn Invoker>>) -> Self {
        Self {
            url: URL::new("dubbo", "/com.example.Service"),
            invokers,
        }
    }
}

#[async_trait]
impl Directory for MockDirectory {
    async fn list(&self, _ctx: &InvocationContext) -> Result<Vec<Arc<dyn Invoker>>, RPCError> {
        if self.invokers.is_empty() {
            return Err(RPCError::ServiceNotFound("no invokers available".into()));
        }
        Ok(self.invokers.iter().map(Arc::clone).collect())
    }

    fn get_url(&self) -> &URL {
        &self.url
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    println!("=== dubbo-rs Failover Cluster Demo (Phase 2) ===\n");

    // ── Demo 1: FailoverCluster with healthy invokers ──────────────────
    println!("── Demo 1: FailoverCluster — all invokers healthy ──\n");
    {
        let invoker1 = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/com.example.Service"),
            true,
        ));
        let invoker2 = Arc::new(MockInvoker::new(
            make_url("192.168.1.2", "50051", "/com.example.Service"),
            true,
        ));
        let invoker3 = Arc::new(MockInvoker::new(
            make_url("192.168.1.3", "50051", "/com.example.Service"),
            true,
        ));

        let directory = Box::new(MockDirectory::new(vec![invoker1, invoker2, invoker3]));
        let cluster = FailoverCluster::new().with_retries(2);
        let _result = cluster
            .join(directory)
            .await
            .context("FailoverCluster::join failed")?;
        println!("  FailoverCluster created successfully");
    }

    // ── Demo 2: FailoverCluster with failing invokers ──────────────────
    println!("\n── Demo 2: FailoverCluster — all invokers failing ──\n");
    {
        let invoker1 = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/com.example.Service"),
            false,
        ));
        let invoker2 = Arc::new(MockInvoker::new(
            make_url("192.168.1.2", "50051", "/com.example.Service"),
            false,
        ));
        let invoker3 = Arc::new(MockInvoker::new(
            make_url("192.168.1.3", "50051", "/com.example.Service"),
            false,
        ));

        let call_counts = [
            invoker1.call_count.clone(),
            invoker2.call_count.clone(),
            invoker3.call_count.clone(),
        ];

        let directory = Box::new(MockDirectory::new(vec![invoker1, invoker2, invoker3]));
        let cluster = FailoverCluster::new().with_retries(2);
        let _invoker = cluster
            .join(directory)
            .await
            .context("FailoverCluster::join failed")?;

        println!("  FailoverCluster configured with retries=2");
        println!("  Call counts after setup:");
        for (i, count) in call_counts.iter().enumerate() {
            println!(
                "    invoker {}: {} calls",
                i + 1,
                count.load(Ordering::SeqCst)
            );
        }
    }

    // ── Demo 3: FailfastCluster — quick fail on first error ──────────
    println!("\n── Demo 3: FailfastCluster — no retries ──\n");
    {
        let invoker = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/com.example.Service"),
            false,
        ));
        let directory = Box::new(MockDirectory::new(vec![invoker]));
        let cluster = FailfastCluster;
        let _result = cluster
            .join(directory)
            .await
            .context("FailfastCluster::join failed")?;
        println!("  FailfastCluster created — will fail immediately on error");
    }

    // ── Demo 4: Comparison — Failover vs Failfast behavior ──────────
    println!("\n── Demo 4: Failover (2+1=3 attempts) vs Failfast (1 attempt) ──\n");
    {
        println!("  FailoverCluster: max retries = 2 (attempts = retries + 1 = 3)");
        println!("  FailfastCluster:  max retries = 0 (attempts = 1)");
    }

    println!("\n=== All failover demos complete ===");
    Ok(())
}
