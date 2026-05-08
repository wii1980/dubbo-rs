//! dubbo-rs comprehensive feature showcase — phone dialing with server streaming.
//!
//! Demonstrates the following dubbo-rs capabilities in a single binary:
//!
//! 1. **Configuration** — YAML loading + builder pattern (`dubbo-rs-config`)
//! 2. **Filter Chain** — `EchoFilter`, `TokenFilter`, `AccessLogFilter` (`dubbo-rs-filter`)
//! 3. **Load Balance** — `Random`, `RoundRobin`, `LeastActive`, `ConsistentHash` (`dubbo-rs-loadbalance`)
//! 4. **Cluster Fault Tolerance** — `FailoverCluster`, `FailfastCluster` (`dubbo-rs-cluster`)
//! 5. **Server Streaming RPC** — phone dialing scenarios via gRPC (`dubbo-rs-server` / `dubbo-rs-client`)
//! 6. **Optional Nacos Registry** — service registration and discovery (`dubbo-rs-registry-nacos`)
//! 7. **Optional `ZooKeeper` Registry** — service registration and discovery (`dubbo-rs-registry-zookeeper`)
//! 8. **Optional Etcd Registry** — service registration and discovery (`dubbo-rs-registry-etcd`)
//!
//! # Usage
//!
//! ```text
//! cargo run -p phone-dialing                  # all demos (no external deps)
//! cargo run -p phone-dialing -- both          # same as above
//! cargo run -p phone-dialing -- server        # gRPC server + Nacos registration (needs Nacos)
//! cargo run -p phone-dialing -- client        # gRPC client + Nacos discovery (needs Nacos)
//! cargo run -p phone-dialing -- nacos         # all demos + Nacos streaming (needs Nacos)
//! cargo run -p phone-dialing -- server-zk     # gRPC server + ZooKeeper registration (needs ZK)
//! cargo run -p phone-dialing -- client-zk     # gRPC client + ZooKeeper discovery (needs ZK)
//! cargo run -p phone-dialing -- zk            # all demos + ZooKeeper streaming (needs ZK)
//! cargo run -p phone-dialing -- server-etcd   # gRPC server + Etcd registration (needs Etcd)
//! cargo run -p phone-dialing -- client-etcd   # gRPC client + Etcd discovery (needs Etcd)
//! cargo run -p phone-dialing -- etcd          # all demos + Etcd streaming (needs Etcd)
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use dubbo_rs::client::Client;
use dubbo_rs::cluster::{Cluster, FailfastCluster, FailoverCluster, StaticDirectory};
use dubbo_rs::common::error::RPCError;
use dubbo_rs::common::node::Node;
use dubbo_rs::common::url::URL;
use dubbo_rs::config::{ProtocolConfig, RegistryConfig, RootConfig};
use dubbo_rs::etcd::EtcdRegistry;
use dubbo_rs::filter::{AccessLogFilter, EchoFilter, FilterChain, TokenFilter};
use dubbo_rs::loadbalance::{
    ConsistentHashLoadBalance, LeastActiveLoadBalance, LoadBalance, RandomLoadBalance,
    RoundRobinLoadBalance,
};
use dubbo_rs::nacos::NacosRegistry;
use dubbo_rs::protocol::{InvocationContext, Invoker, RPCResult};
use dubbo_rs::registry::Registry;
use dubbo_rs::server::Server;
use dubbo_rs::zookeeper::ZookeeperRegistry;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

// ---------------------------------------------------------------------------
// Include the generated Dubbo integration code (package exchange → exchange_dubbo.rs)
// ---------------------------------------------------------------------------

include!(concat!(env!("OUT_DIR"), "/exchange_dubbo.rs"));

use proto::telephone_exchange_client::TelephoneExchangeClient;
use proto::telephone_exchange_server::TelephoneExchange;
use proto::{DialProgress, DialRequest, DialStage};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Shared test scenarios used by all registry-based client demos (nacos/zk/etcd).
const DEMO_CALLS: &[(&str, &str, i32, &str)] = &[
    (
        "021-5678",
        "010-1234",
        30,
        "Normal call — rings 3 times then answers",
    ),
    (
        "010-9999",
        "021-4321",
        30,
        "Quick answer — ends with 9, answers on ring 1",
    ),
    (
        "666-1001",
        "010-5555",
        30,
        "Line busy — number starts with 666",
    ),
    (
        "000",
        "010-5555",
        30,
        "Invalid number — callee is 000, fails at validation",
    ),
];

#[allow(clippy::cast_possible_truncation)]
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn make_progress(
    call_id: &str,
    callee: &str,
    stage: DialStage,
    percent: i32,
    msg: String,
) -> DialProgress {
    DialProgress {
        call_id: call_id.to_string(),
        stage: stage as i32,
        progress_percent: percent,
        message: msg,
        timestamp_ms: now_ms(),
        callee_number: callee.to_string(),
    }
}

fn stage_name(stage: i32) -> &'static str {
    match DialStage::try_from(stage).unwrap_or(DialStage::Unspecified) {
        DialStage::Unspecified => "UNSPECIFIED",
        DialStage::Validating => "VALIDATING",
        DialStage::Routing => "ROUTING",
        DialStage::Ringing => "RINGING",
        DialStage::Connected => "CONNECTED",
        DialStage::Failed => "FAILED",
        DialStage::Completed => "COMPLETED",
    }
}

fn make_url(host: &str, port: &str, path: &str) -> URL {
    let mut url = URL::new("tri", path);
    url.ip = host.to_string();
    url.port = port.to_string();
    url
}

fn server_port() -> u16 {
    std::env::var("SERVER_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(50051)
}

fn nacos_addr() -> String {
    std::env::var("NACOS_ADDR").unwrap_or_else(|_| "127.0.0.1:8848".to_string())
}

fn zk_addr() -> String {
    std::env::var("ZK_ADDR").unwrap_or_else(|_| "127.0.0.1:2181".to_string())
}

fn etcd_addr() -> String {
    std::env::var("ETCD_ADDR").unwrap_or_else(|_| "127.0.0.1:2379".to_string())
}

// ---------------------------------------------------------------------------
// MockInvoker — used in filter, load-balance, and cluster demos
// ---------------------------------------------------------------------------

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
    async fn invoke(&self, ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        if self.succeed {
            let msg = format!(
                "Hello from {}! method={}",
                self.url.get_address(),
                ctx.method_name
            );
            Ok(RPCResult::success(msg.into_bytes()))
        } else {
            Ok(RPCResult::from_error(RPCError::ServerError(format!(
                "mock failure on {}",
                self.url.get_address()
            ))))
        }
    }
}

// ---------------------------------------------------------------------------
// MyExchange — the TelephoneExchange streaming service (unchanged logic)
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct MyExchange;

#[tonic::async_trait]
impl TelephoneExchange for MyExchange {
    type DialStream = ReceiverStream<Result<DialProgress, Status>>;

    #[allow(clippy::too_many_lines, clippy::similar_names, clippy::cast_sign_loss)]
    async fn dial(
        &self,
        request: Request<DialRequest>,
    ) -> Result<Response<Self::DialStream>, Status> {
        let req = request.into_inner();
        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            let call_id = uuid::Uuid::new_v4().to_string();
            let callee = req.callee.clone();
            let caller = req.caller.clone();
            let timeout_secs = if req.timeout_seconds > 0 {
                req.timeout_seconds as u64
            } else {
                30
            };

            // VALIDATING
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            if tx
                .send(Ok(make_progress(
                    &call_id,
                    &callee,
                    DialStage::Validating,
                    10,
                    format!("Validating numbers: {caller} -> {callee}"),
                )))
                .await
                .is_err()
            {
                return;
            }

            if callee == "000" {
                let _ = tx
                    .send(Ok(make_progress(
                        &call_id,
                        &callee,
                        DialStage::Failed,
                        100,
                        "Invalid number".to_string(),
                    )))
                    .await;
                return;
            }

            // ROUTING
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if tx
                .send(Ok(make_progress(
                    &call_id,
                    &callee,
                    DialStage::Routing,
                    30,
                    format!("Routing call through exchange to {callee}"),
                )))
                .await
                .is_err()
            {
                return;
            }

            if callee.starts_with("111") {
                let _ = tx
                    .send(Ok(make_progress(
                        &call_id,
                        &callee,
                        DialStage::Failed,
                        100,
                        "Number not in service".to_string(),
                    )))
                    .await;
                return;
            }

            if callee.starts_with("666") {
                let _ = tx
                    .send(Ok(make_progress(
                        &call_id,
                        &callee,
                        DialStage::Failed,
                        100,
                        "Line busy".to_string(),
                    )))
                    .await;
                return;
            }

            // RINGING
            let answer_ring = if callee.starts_with("777") {
                999
            } else if callee.ends_with('9') {
                1
            } else if callee.ends_with('8') {
                2
            } else {
                3
            };

            let ring_duration = std::time::Duration::from_millis(800);
            let timeout = std::time::Duration::from_secs(timeout_secs);
            let start = std::time::Instant::now();
            let mut answered = false;

            for ring in 1..=answer_ring {
                if start.elapsed() >= timeout {
                    let _ = tx
                        .send(Ok(make_progress(
                            &call_id,
                            &callee,
                            DialStage::Failed,
                            100,
                            "No answer".to_string(),
                        )))
                        .await;
                    return;
                }

                tokio::time::sleep(ring_duration).await;
                let progress = 30 + ring * 10;

                if ring == answer_ring {
                    if tx
                        .send(Ok(make_progress(
                            &call_id,
                            &callee,
                            DialStage::Ringing,
                            progress,
                            format!("Ring {ring}: {callee} answered!"),
                        )))
                        .await
                        .is_err()
                    {
                        return;
                    }
                    answered = true;
                    break;
                } else if tx
                    .send(Ok(make_progress(
                        &call_id,
                        &callee,
                        DialStage::Ringing,
                        progress,
                        format!("Ring {ring}: ringing {callee}..."),
                    )))
                    .await
                    .is_err()
                {
                    return;
                }
            }

            if !answered {
                let _ = tx
                    .send(Ok(make_progress(
                        &call_id,
                        &callee,
                        DialStage::Failed,
                        100,
                        "No answer".to_string(),
                    )))
                    .await;
                return;
            }

            // CONNECTED
            if tx
                .send(Ok(make_progress(
                    &call_id,
                    &callee,
                    DialStage::Connected,
                    80,
                    "Call established!".to_string(),
                )))
                .await
                .is_err()
            {
                return;
            }

            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

            if callee.starts_with("888") {
                if tx
                    .send(Ok(make_progress(
                        &call_id,
                        &callee,
                        DialStage::Connected,
                        85,
                        "Remote party speaking...".to_string(),
                    )))
                    .await
                    .is_err()
                {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let _ = tx
                    .send(Ok(make_progress(
                        &call_id,
                        &callee,
                        DialStage::Failed,
                        100,
                        "Remote party hung up".to_string(),
                    )))
                    .await;
                return;
            }

            if tx
                .send(Ok(make_progress(
                    &call_id,
                    &callee,
                    DialStage::Connected,
                    90,
                    "Call in progress...".to_string(),
                )))
                .await
                .is_err()
            {
                return;
            }

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            // COMPLETED
            let _ = tx
                .send(Ok(make_progress(
                    &call_id,
                    &callee,
                    DialStage::Completed,
                    100,
                    "Call ended normally".to_string(),
                )))
                .await;
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

// ===========================================================================
// Phase 1: Configuration demo
// ===========================================================================

fn demo_config() {
    println!("=== Phase 1: Configuration (YAML + Builder) ===\n");

    // Load from YAML
    let yaml = r#"
application: "phone-dialing-demo"
version: "1.0.0"
protocols:
  - name: "tri"
    host: "0.0.0.0"
    port: 50051
registries:
  - protocol: "nacos"
    address: "127.0.0.1:8848"
"#;
    let from_yaml: RootConfig = serde_yaml::from_str(yaml).expect("failed to parse YAML config");
    println!("  Loaded from YAML:");
    println!("    application = {}", from_yaml.application);
    println!("    version     = {}", from_yaml.version);
    println!(
        "    protocol    = {}://{}:{}",
        from_yaml.protocols[0].name, from_yaml.protocols[0].host, from_yaml.protocols[0].port,
    );
    println!(
        "    registry    = {}://{}",
        from_yaml.registries[0].protocol, from_yaml.registries[0].address,
    );

    // Builder pattern
    let from_builder = RootConfig::default()
        .with_application("phone-dialing-builder")
        .with_version("2.0.0")
        .with_protocol(ProtocolConfig::new("tri", "0.0.0.0", 50052))
        .with_registry(RegistryConfig {
            protocol: "nacos".into(),
            address: "127.0.0.1:8848".into(),
        });
    println!("\n  Built via builder:");
    println!("    application = {}", from_builder.application);
    println!("    version     = {}", from_builder.version);
    println!(
        "    protocol    = {}://{}:{}",
        from_builder.protocols[0].name,
        from_builder.protocols[0].host,
        from_builder.protocols[0].port,
    );
    println!(
        "    registry    = {}://{}",
        from_builder.registries[0].protocol, from_builder.registries[0].address,
    );

    // Show defaults
    let defaults = RootConfig::default();
    println!("\n  Defaults (RootConfig::default()):");
    println!("    application = {:?}", defaults.application);
    println!("    version     = {}", defaults.version);
    println!("    protocols   = {} entries", defaults.protocols.len());
    println!("    registries  = {} entries", defaults.registries.len());
    println!("    tls         = {:?}", defaults.tls);

    println!("\n  [Phase 1 complete]\n");
}

// ===========================================================================
// Phase 2: Filter chain demo
// ===========================================================================

async fn demo_filters() {
    println!("=== Phase 2: Filter Chain (Echo + Token + AccessLog) ===\n");

    let filters: Vec<Box<dyn dubbo_rs::filter::Filter>> = vec![
        Box::new(EchoFilter),
        Box::new(TokenFilter::new("dial-secret")),
        Box::new(AccessLogFilter),
    ];
    let base_invoker = Box::new(MockInvoker::new(
        URL::new("tri", "/com.example.TelephoneExchange"),
        true,
    ));
    let chain = FilterChain::new(filters, base_invoker).build();

    // Demo 1: Normal call with correct token
    println!("  Demo 1: Normal call with correct token");
    let mut ctx = InvocationContext::new("dial", URL::new("tri", "/com.example.TelephoneExchange"));
    ctx.attachments
        .insert("token".to_string(), "dial-secret".to_string());
    match chain.invoke(&mut ctx).await {
        Ok(r) => println!(
            "    => success: {}\n",
            String::from_utf8_lossy(r.value.as_ref().unwrap())
        ),
        Err(e) => println!("    => unexpected error: {e}\n"),
    }

    // Demo 2: $echo health check (bypasses token)
    println!("  Demo 2: $echo health check (bypasses token)");
    let mut ctx =
        InvocationContext::new("$echo", URL::new("tri", "/com.example.TelephoneExchange"));
    ctx.arguments = vec![b"ping".to_vec()];
    match chain.invoke(&mut ctx).await {
        Ok(r) => println!(
            "    => echo: {}\n",
            String::from_utf8_lossy(r.value.as_ref().unwrap())
        ),
        Err(e) => println!("    => unexpected error: {e}\n"),
    }

    // Demo 3: Missing token
    println!("  Demo 3: Missing token (should fail)");
    let mut ctx = InvocationContext::new("dial", URL::new("tri", "/com.example.TelephoneExchange"));
    match chain.invoke(&mut ctx).await {
        Ok(r) => println!("    => unexpected success: {r:?}\n"),
        Err(e) => println!("    => expected error: {e}\n"),
    }

    // Demo 4: Wrong token
    println!("  Demo 4: Wrong token (should fail)");
    let mut ctx = InvocationContext::new("dial", URL::new("tri", "/com.example.TelephoneExchange"));
    ctx.attachments
        .insert("token".to_string(), "wrong-token".to_string());
    match chain.invoke(&mut ctx).await {
        Ok(r) => println!("    => unexpected success: {r:?}\n"),
        Err(e) => println!("    => expected error: {e}\n"),
    }

    println!("  [Phase 2 complete]\n");
}

// ===========================================================================
// Phase 3: Load balance demo
// ===========================================================================

fn demo_loadbalance() {
    println!("=== Phase 3: Load Balance (4 strategies) ===\n");

    // Create 3 invokers with different weights
    let invoker_weights = [100u64, 200, 300];
    let invokers: Vec<Box<dyn Invoker>> = invoker_weights
        .iter()
        .enumerate()
        .map(|(i, &w)| {
            let mut url = make_url(
                &format!("192.168.1.{}", i + 1),
                "50051",
                "/com.example.TelephoneExchange",
            );
            url.set_param("weight", w.to_string());
            Box::new(MockInvoker::new(url, true)) as Box<dyn Invoker>
        })
        .collect();

    let url = URL::new("tri", "/com.example.TelephoneExchange");
    let ctx = InvocationContext::new("dial", url.clone());

    // Random
    println!("  RandomLoadBalance (10 picks, weights: 100/200/300):");
    let lb = RandomLoadBalance;
    let mut counts = [0usize; 3];
    for _ in 0..10 {
        let idx = lb.select(&invokers, &url, &ctx).unwrap();
        counts[idx] += 1;
    }
    println!(
        "    selections: A={}, B={}, C={}",
        counts[0], counts[1], counts[2]
    );

    // RoundRobin
    println!("\n  RoundRobinLoadBalance (6 picks, same weights):");
    let lb = RoundRobinLoadBalance::new();
    let sequence: Vec<usize> = (0..6)
        .map(|_| lb.select(&invokers, &url, &ctx).unwrap())
        .collect();
    println!("    sequence: {sequence:?}");

    // LeastActive
    println!("\n  LeastActiveLoadBalance (prefer lowest 'active'):");
    let active_invokers: Vec<Box<dyn Invoker>> = [
        ("192.168.1.1", 10u64),
        ("192.168.1.2", 2u64),
        ("192.168.1.3", 5u64),
    ]
    .iter()
    .map(|&(host, active)| {
        let mut url = make_url(host, "50051", "/com.example.TelephoneExchange");
        url.set_param("active", active.to_string());
        url.set_param("weight", "100");
        Box::new(MockInvoker::new(url, true)) as Box<dyn Invoker>
    })
    .collect();
    let lb = LeastActiveLoadBalance;
    let idx = lb.select(&active_invokers, &url, &ctx).unwrap();
    println!("    selected: invoker {} (active=2, lowest)\n", idx + 1);

    // ConsistentHash
    println!("  ConsistentHashLoadBalance (same input => same output):");
    let lb = ConsistentHashLoadBalance::new().with_virtual_nodes(320);
    let mut ctx_a = InvocationContext::new("dial", url.clone());
    ctx_a.arguments = vec![b"caller-021-5678".to_vec()];
    let mut ctx_b = InvocationContext::new("dial", url.clone());
    ctx_b.arguments = vec![b"caller-021-5678".to_vec()];
    let idx_a = lb.select(&invokers, &url, &ctx_a).unwrap();
    let idx_b = lb.select(&invokers, &url, &ctx_b).unwrap();
    println!(
        "    same key 'caller-021-5678': idx_a={idx_a}, idx_b={idx_b} (identical={})",
        idx_a == idx_b
    );

    let mut ctx_c = InvocationContext::new("dial", url.clone());
    ctx_c.arguments = vec![b"caller-010-9999".to_vec()];
    let idx_c = lb.select(&invokers, &url, &ctx_c).unwrap();
    println!("    different key 'caller-010-9999': idx_c={idx_c}");

    println!("\n  [Phase 3 complete]\n");
}

// ===========================================================================
// Phase 4: Cluster fault tolerance demo
// ===========================================================================

async fn demo_cluster() -> Result<(), anyhow::Error> {
    println!("=== Phase 4: Cluster Fault Tolerance ===\n");

    // Demo 1: FailoverCluster — mixed invokers (1 failing, 1 succeeding)
    println!("  Demo 1: FailoverCluster (retries=2, mixed invokers)");
    {
        let invoker_ok = Arc::new(MockInvoker::new(
            make_url("192.168.1.1", "50051", "/com.example.TelephoneExchange"),
            true,
        ));
        let invoker_fail = Arc::new(MockInvoker::new(
            make_url("192.168.1.2", "50051", "/com.example.TelephoneExchange"),
            false,
        ));

        let count_ok = invoker_ok.call_count.clone();
        let count_fail = invoker_fail.call_count.clone();

        let dir = StaticDirectory::new(URL::new("tri", "/com.example.TelephoneExchange"));
        dir.add_invoker(invoker_ok);
        dir.add_invoker(invoker_fail);

        let cluster = FailoverCluster::new().with_retries(2);
        let cluster_invoker = cluster
            .join(Box::new(dir))
            .await
            .context("FailoverCluster::join failed")?;

        let mut ctx =
            InvocationContext::new("dial", URL::new("tri", "/com.example.TelephoneExchange"));
        match cluster_invoker.invoke(&mut ctx).await {
            Ok(r) if !r.is_error() => println!(
                "    => success: {}",
                String::from_utf8_lossy(r.value.as_ref().unwrap())
            ),
            Ok(r) => println!("    => error result: {:?}", r.error),
            Err(e) => println!("    => error: {e}"),
        }
        println!(
            "    call counts: ok={}, fail={}",
            count_ok.load(Ordering::SeqCst),
            count_fail.load(Ordering::SeqCst),
        );
    }

    // Demo 2: FailfastCluster — failing invoker → immediate error
    println!("\n  Demo 2: FailfastCluster (failing invoker, no retry)");
    {
        let invoker_fail = Arc::new(MockInvoker::new(
            make_url("192.168.1.3", "50051", "/com.example.TelephoneExchange"),
            false,
        ));
        let count_fail = invoker_fail.call_count.clone();

        let dir = StaticDirectory::new(URL::new("tri", "/com.example.TelephoneExchange"));
        dir.add_invoker(invoker_fail);

        let cluster = FailfastCluster;
        let cluster_invoker = cluster
            .join(Box::new(dir))
            .await
            .context("FailfastCluster::join failed")?;

        let mut ctx =
            InvocationContext::new("dial", URL::new("tri", "/com.example.TelephoneExchange"));
        match cluster_invoker.invoke(&mut ctx).await {
            Ok(r) => println!("    => result: error={:?}, value={:?}", r.error, r.value),
            Err(e) => println!("    => error: {e}"),
        }
        println!(
            "    call count: {} (single attempt, no retry)",
            count_fail.load(Ordering::SeqCst),
        );
    }

    println!("\n  [Phase 4 complete]\n");
    Ok(())
}

// ===========================================================================
// Phase 5: Server streaming RPC — direct mode
// ===========================================================================

async fn demo_streaming_direct(port: u16) -> Result<(), anyhow::Error> {
    println!("=== Phase 5: Server Streaming RPC (direct) ===\n");

    // Start gRPC server in background using generated registration function
    let server = Server::new()
        .with_application("phone-dialing-demo")
        .with_protocol_config(ProtocolConfig::new("tri", "127.0.0.1", port));
    let server = register_telephone_exchange_service(server, MyExchange);

    let server_handle = tokio::spawn(server.serve());
    // Give the server a moment to bind
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Connect via dubbo_rs_client::Client
    let mut client = Client::new().with_url(format!(
        "tri://127.0.0.1:{port}/com.example.TelephoneExchange"
    ));
    client
        .dial()
        .await
        .context("failed to connect to gRPC server")?;

    let channel = client.channel().context("no channel after dial")?.clone();
    let mut grpc_client = TelephoneExchangeClient::new(channel);

    let scenarios: Vec<(&str, &str, i32, &str)> = vec![
        (
            "021-5678",
            "010-1234",
            30,
            "Normal call — callee ends with 8, rings 2 times then answers",
        ),
        (
            "010-9999",
            "021-4321",
            30,
            "Quick answer — callee ends with 9, answers on ring 1",
        ),
        (
            "666-1001",
            "010-5555",
            30,
            "Line busy — callee starts with 666",
        ),
        (
            "000",
            "021-8888",
            30,
            "Invalid number — callee is 000, fails at validation",
        ),
    ];

    for (i, (callee, caller, timeout, desc)) in scenarios.iter().enumerate() {
        println!("  Call {}: {}", i + 1, desc);

        let request = tonic::Request::new(DialRequest {
            caller: (*caller).to_string(),
            callee: (*callee).to_string(),
            timeout_seconds: *timeout,
        });

        match grpc_client.dial(request).await {
            Ok(response) => {
                let mut stream = response.into_inner();
                while let Ok(Some(progress)) = stream.message().await {
                    println!(
                        "    [{:>3}%] {:<12} | {}",
                        progress.progress_percent,
                        stage_name(progress.stage),
                        progress.message,
                    );
                }
            }
            Err(e) => {
                println!("    RPC error: {e}");
            }
        }
        println!();
    }

    println!("  [Phase 5 complete]\n");

    // Clean up
    server_handle.abort();
    Ok(())
}

// ===========================================================================
// Phase 6: Server with Nacos registration
// ===========================================================================

async fn run_server_with_nacos(port: u16) -> Result<(), anyhow::Error> {
    tracing::info!("Starting phone dialing server with Nacos (port: {port})");

    let mut service_url = URL::new("tri", "/com.example.TelephoneExchange");
    service_url.ip = "127.0.0.1".into();
    service_url.port = port.to_string();
    service_url.set_param("side", "provider");

    let mut nacos_url = URL::new("nacos", "");
    let addr = nacos_addr();
    let mut parts = addr.splitn(2, ':');
    nacos_url.ip = parts.next().unwrap_or("127.0.0.1").to_string();
    nacos_url.port = parts.next().unwrap_or("8848").to_string();

    let mut registry = NacosRegistry::new(nacos_url);
    if let Ok(ns) = std::env::var("NACOS_NAMESPACE") {
        registry = registry.with_namespace(ns);
    }
    if let Ok(group) = std::env::var("NACOS_GROUP") {
        registry = registry.with_group(group);
    }
    if let (Ok(user), Ok(pass)) = (
        std::env::var("NACOS_USERNAME"),
        std::env::var("NACOS_PASSWORD"),
    ) {
        registry = registry.with_auth(user, pass);
    }

    tracing::info!("Registering with Nacos at {addr}");
    registry
        .register(service_url)
        .await
        .context("failed to register with Nacos")?;
    tracing::info!("Service registered successfully");

    let server = Server::new()
        .with_application("phone-dialing-nacos")
        .with_protocol_config(ProtocolConfig::new("tri", "0.0.0.0", port));
    let server = register_telephone_exchange_service(server, MyExchange);

    server.serve().await
}

// ===========================================================================
// Phase 7: Client with Nacos discovery
// ===========================================================================

#[allow(clippy::too_many_lines)]
async fn run_client_with_nacos() -> Result<(), anyhow::Error> {
    use dubbo_rs::registry::{NotifyListener, ServiceEvent};
    use std::sync::Mutex;

    struct DiscoveryListener {
        listen_url: URL,
        discovered: Arc<Mutex<Vec<URL>>>,
    }

    #[async_trait::async_trait]
    impl NotifyListener for DiscoveryListener {
        async fn notify(&self, event: ServiceEvent) {
            match event {
                ServiceEvent::Add(urls) => {
                    tracing::info!("Discovered {} provider(s)", urls.len());
                    let mut discovered = self.discovered.lock().unwrap();
                    discovered.extend(urls);
                }
                ServiceEvent::Remove(urls) => {
                    let remove_addrs: std::collections::HashSet<String> =
                        urls.iter().map(URL::get_address).collect();
                    let mut discovered = self.discovered.lock().unwrap();
                    discovered.retain(|u| !remove_addrs.contains(&u.get_address()));
                }
                ServiceEvent::Update(urls) => {
                    let mut discovered = self.discovered.lock().unwrap();
                    *discovered = urls;
                }
            }
        }
        fn listen_url(&self) -> URL {
            self.listen_url.clone()
        }
    }

    tracing::info!("Starting phone dialing client with Nacos discovery");

    let mut nacos_url = URL::new("nacos", "");
    let addr = nacos_addr();
    let mut parts = addr.splitn(2, ':');
    nacos_url.ip = parts.next().unwrap_or("127.0.0.1").to_string();
    nacos_url.port = parts.next().unwrap_or("8848").to_string();

    let mut registry = NacosRegistry::new(nacos_url);
    if let Ok(ns) = std::env::var("NACOS_NAMESPACE") {
        registry = registry.with_namespace(ns);
    }
    if let Ok(group) = std::env::var("NACOS_GROUP") {
        registry = registry.with_group(group);
    }

    let service_url = URL::new("tri", "/com.example.TelephoneExchange");

    let discovered = Arc::new(Mutex::new(Vec::new()));
    let listener = Arc::new(DiscoveryListener {
        listen_url: service_url.clone(),
        discovered: discovered.clone(),
    });
    registry
        .subscribe(service_url, listener)
        .await
        .context("failed to subscribe to Nacos")?;
    tracing::info!("Subscribed. Waiting for service discovery...");

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let server_addr = {
        let providers = discovered.lock().unwrap();
        match providers.first() {
            Some(url) => {
                let addr = format!("http://{}", url.get_address());
                tracing::info!("Using discovered provider: {addr}");
                addr
            }
            None => {
                anyhow::bail!(
                    "No provider discovered from Nacos. \
                     Ensure the server is registered and Nacos is reachable at {addr}"
                );
            }
        }
    };

    // Connect using dubbo_rs_client::Client via the discovered address
    let host_port = server_addr.strip_prefix("http://").unwrap_or(&server_addr);
    let mut client =
        Client::new().with_url(format!("tri://{host_port}/com.example.TelephoneExchange"));
    client.dial().await.context("failed to dial server")?;

    let channel = client.channel().context("no channel")?.clone();
    let mut grpc_client = TelephoneExchangeClient::new(channel);

    let calls = DEMO_CALLS;

    for (i, (callee, caller, timeout, desc)) in calls.iter().enumerate() {
        println!("\n--- Call {}: {} ---", i + 1, desc);

        let request = tonic::Request::new(DialRequest {
            caller: (*caller).to_string(),
            callee: (*callee).to_string(),
            timeout_seconds: *timeout,
        });

        let response = grpc_client.dial(request).await?;
        let mut stream = response.into_inner();

        while let Some(progress) = stream.message().await? {
            println!(
                "  [{:>3}%] {:<12} | {}",
                progress.progress_percent,
                stage_name(progress.stage),
                progress.message,
            );
        }

        println!("  Call {} finished.", i + 1);
    }

    tracing::info!("Nacos client demo complete!");
    Ok(())
}

// ===========================================================================
// Phase 8: Server with ZooKeeper registration
// ===========================================================================

async fn run_server_with_zookeeper(port: u16) -> Result<(), anyhow::Error> {
    tracing::info!("Starting phone dialing server with ZooKeeper (port: {port})");

    let mut service_url = URL::new("tri", "/com.example.TelephoneExchange");
    service_url.ip = "127.0.0.1".into();
    service_url.port = port.to_string();
    service_url.set_param("side", "provider");

    let mut zk_url = URL::new("zookeeper", "");
    let addr = zk_addr();
    let mut parts = addr.splitn(2, ':');
    zk_url.ip = parts.next().unwrap_or("127.0.0.1").to_string();
    zk_url.port = parts.next().unwrap_or("2181").to_string();

    let registry = ZookeeperRegistry::new(zk_url);
    tracing::info!("Registering with ZooKeeper at {addr}");
    registry
        .register(service_url)
        .await
        .context("failed to register with ZooKeeper")?;
    tracing::info!("Service registered successfully");

    let server = Server::new()
        .with_application("phone-dialing-zk")
        .with_protocol_config(ProtocolConfig::new("tri", "0.0.0.0", port));
    let server = register_telephone_exchange_service(server, MyExchange);

    server.serve().await
}

// ===========================================================================
// Phase 9: Client with ZooKeeper discovery
// ===========================================================================

async fn run_client_with_zookeeper() -> Result<(), anyhow::Error> {
    use dubbo_rs::registry::{NotifyListener, ServiceEvent};
    use std::sync::Mutex;

    struct DiscoveryListener {
        listen_url: URL,
        discovered: Arc<Mutex<Vec<URL>>>,
    }

    #[async_trait::async_trait]
    impl NotifyListener for DiscoveryListener {
        async fn notify(&self, event: ServiceEvent) {
            match event {
                ServiceEvent::Add(urls) => {
                    tracing::info!("Discovered {} provider(s)", urls.len());
                    let mut discovered = self.discovered.lock().unwrap();
                    discovered.extend(urls);
                }
                ServiceEvent::Remove(urls) => {
                    let remove_addrs: std::collections::HashSet<String> =
                        urls.iter().map(URL::get_address).collect();
                    let mut discovered = self.discovered.lock().unwrap();
                    discovered.retain(|u| !remove_addrs.contains(&u.get_address()));
                }
                ServiceEvent::Update(urls) => {
                    let mut discovered = self.discovered.lock().unwrap();
                    *discovered = urls;
                }
            }
        }
        fn listen_url(&self) -> URL {
            self.listen_url.clone()
        }
    }

    tracing::info!("Starting phone dialing client with ZooKeeper discovery");

    let mut zk_url = URL::new("zookeeper", "");
    let addr = zk_addr();
    let mut parts = addr.splitn(2, ':');
    zk_url.ip = parts.next().unwrap_or("127.0.0.1").to_string();
    zk_url.port = parts.next().unwrap_or("2181").to_string();

    let registry = ZookeeperRegistry::new(zk_url);

    let service_url = URL::new("tri", "/com.example.TelephoneExchange");

    let discovered = Arc::new(Mutex::new(Vec::new()));
    let listener = Arc::new(DiscoveryListener {
        listen_url: service_url.clone(),
        discovered: discovered.clone(),
    });
    registry
        .subscribe(service_url, listener)
        .await
        .context("failed to subscribe to ZooKeeper")?;
    tracing::info!("Subscribed. Waiting for service discovery...");

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let server_addr = {
        let providers = discovered.lock().unwrap();
        match providers.first() {
            Some(url) => {
                let addr = format!("http://{}", url.get_address());
                tracing::info!("Using discovered provider: {addr}");
                addr
            }
            None => {
                anyhow::bail!(
                    "No provider discovered from ZooKeeper. \
                     Ensure the server is registered and ZooKeeper is reachable at {addr}"
                );
            }
        }
    };

    let host_port = server_addr.strip_prefix("http://").unwrap_or(&server_addr);
    let mut client =
        Client::new().with_url(format!("tri://{host_port}/com.example.TelephoneExchange"));
    client.dial().await.context("failed to dial server")?;

    let channel = client.channel().context("no channel")?.clone();
    let mut grpc_client = TelephoneExchangeClient::new(channel);

    let calls = DEMO_CALLS;

    for (i, (callee, caller, timeout, desc)) in calls.iter().enumerate() {
        println!("\n--- Call {}: {} ---", i + 1, desc);

        let request = tonic::Request::new(DialRequest {
            caller: (*caller).to_string(),
            callee: (*callee).to_string(),
            timeout_seconds: *timeout,
        });

        let response = grpc_client.dial(request).await?;
        let mut stream = response.into_inner();

        while let Some(progress) = stream.message().await? {
            println!(
                "  [{:>3}%] {:<12} | {}",
                progress.progress_percent,
                stage_name(progress.stage),
                progress.message,
            );
        }

        println!("  Call {} finished.", i + 1);
    }

    tracing::info!("ZooKeeper client demo complete!");
    Ok(())
}

// ===========================================================================
// Phase 10: Server with Etcd registration
// ===========================================================================

async fn run_server_with_etcd(port: u16) -> Result<(), anyhow::Error> {
    tracing::info!("Starting phone dialing server with Etcd (port: {port})");

    let mut service_url = URL::new("tri", "/com.example.TelephoneExchange");
    service_url.ip = "127.0.0.1".into();
    service_url.port = port.to_string();
    service_url.set_param("side", "provider");

    let mut etcd_url = URL::new("etcd", "");
    let addr = etcd_addr();
    let mut parts = addr.splitn(2, ':');
    etcd_url.ip = parts.next().unwrap_or("127.0.0.1").to_string();
    etcd_url.port = parts.next().unwrap_or("2379").to_string();

    let registry = EtcdRegistry::new(etcd_url).with_endpoints(format!("http://{addr}"));

    tracing::info!("Registering with Etcd at {addr}");
    registry
        .register(service_url)
        .await
        .context("failed to register with Etcd")?;
    tracing::info!("Service registered successfully");

    let server = Server::new()
        .with_application("phone-dialing-etcd")
        .with_protocol_config(ProtocolConfig::new("tri", "0.0.0.0", port));
    let server = register_telephone_exchange_service(server, MyExchange);

    server.serve().await
}

// ===========================================================================
// Phase 11: Client with Etcd discovery
// ===========================================================================

async fn run_client_with_etcd() -> Result<(), anyhow::Error> {
    use dubbo_rs::registry::{NotifyListener, ServiceEvent};
    use std::sync::Mutex;

    struct DiscoveryListener {
        listen_url: URL,
        discovered: Arc<Mutex<Vec<URL>>>,
    }

    #[async_trait::async_trait]
    impl NotifyListener for DiscoveryListener {
        async fn notify(&self, event: ServiceEvent) {
            match event {
                ServiceEvent::Add(urls) => {
                    tracing::info!("Discovered {} provider(s)", urls.len());
                    let mut discovered = self.discovered.lock().unwrap();
                    discovered.extend(urls);
                }
                ServiceEvent::Remove(urls) => {
                    let remove_addrs: std::collections::HashSet<String> =
                        urls.iter().map(URL::get_address).collect();
                    let mut discovered = self.discovered.lock().unwrap();
                    discovered.retain(|u| !remove_addrs.contains(&u.get_address()));
                }
                ServiceEvent::Update(urls) => {
                    let mut discovered = self.discovered.lock().unwrap();
                    *discovered = urls;
                }
            }
        }
        fn listen_url(&self) -> URL {
            self.listen_url.clone()
        }
    }

    tracing::info!("Starting phone dialing client with Etcd discovery");

    let mut etcd_url = URL::new("etcd", "");
    let addr = etcd_addr();
    let mut parts = addr.splitn(2, ':');
    etcd_url.ip = parts.next().unwrap_or("127.0.0.1").to_string();
    etcd_url.port = parts.next().unwrap_or("2379").to_string();

    let registry = EtcdRegistry::new(etcd_url).with_endpoints(format!("http://{addr}"));

    let service_url = URL::new("tri", "/com.example.TelephoneExchange");

    let discovered = Arc::new(Mutex::new(Vec::new()));
    let listener = Arc::new(DiscoveryListener {
        listen_url: service_url.clone(),
        discovered: discovered.clone(),
    });
    registry
        .subscribe(service_url, listener)
        .await
        .context("failed to subscribe to Etcd")?;
    tracing::info!("Subscribed. Waiting for service discovery...");

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let server_addr = {
        let providers = discovered.lock().unwrap();
        match providers.first() {
            Some(url) => {
                let addr = format!("http://{}", url.get_address());
                tracing::info!("Using discovered provider: {addr}");
                addr
            }
            None => {
                anyhow::bail!(
                    "No provider discovered from Etcd. \
                     Ensure the server is registered and Etcd is reachable at {addr}"
                );
            }
        }
    };

    let host_port = server_addr.strip_prefix("http://").unwrap_or(&server_addr);
    let mut client =
        Client::new().with_url(format!("tri://{host_port}/com.example.TelephoneExchange"));
    client.dial().await.context("failed to dial server")?;

    let channel = client.channel().context("no channel")?.clone();
    let mut grpc_client = TelephoneExchangeClient::new(channel);

    let calls = DEMO_CALLS;

    for (i, (callee, caller, timeout, desc)) in calls.iter().enumerate() {
        println!("\n--- Call {}: {} ---", i + 1, desc);

        let request = tonic::Request::new(DialRequest {
            caller: (*caller).to_string(),
            callee: (*callee).to_string(),
            timeout_seconds: *timeout,
        });

        let response = grpc_client.dial(request).await?;
        let mut stream = response.into_inner();

        while let Some(progress) = stream.message().await? {
            println!(
                "  [{:>3}%] {:<12} | {}",
                progress.progress_percent,
                stage_name(progress.stage),
                progress.message,
            );
        }

        println!("  Call {} finished.", i + 1);
    }

    tracing::info!("Etcd client demo complete!");
    Ok(())
}

// ===========================================================================
// main
// ===========================================================================

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "both".to_string());
    let port = server_port();

    println!("=== dubbo-rs Phone Dialing — Comprehensive Feature Showcase ===\n");
    tracing::info!("Mode: {mode}, Port: {port}");

    match mode.as_str() {
        "server" => {
            println!("Mode: server (gRPC + Nacos registration)\n");
            run_server_with_nacos(port).await?;
        }
        "client" => {
            println!("Mode: client (Nacos discovery + streaming)\n");
            run_client_with_nacos().await?;
        }
        "nacos" => {
            println!("Mode: nacos (all demos + Nacos streaming)\n");
            demo_config();
            demo_filters().await;
            demo_loadbalance();
            demo_cluster().await?;
            println!("--- Launching Nacos server + client ---\n");
            let server = tokio::spawn(run_server_with_nacos(port));
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            run_client_with_nacos().await?;
            server.abort();
        }
        "server-zk" => {
            println!("Mode: server-zk (gRPC + ZooKeeper registration)\n");
            run_server_with_zookeeper(port).await?;
        }
        "client-zk" => {
            println!("Mode: client-zk (ZooKeeper discovery + streaming)\n");
            run_client_with_zookeeper().await?;
        }
        "zk" => {
            println!("Mode: zk (all demos + ZooKeeper streaming)\n");
            demo_config();
            demo_filters().await;
            demo_loadbalance();
            demo_cluster().await?;
            println!("--- Launching ZooKeeper server + client ---\n");
            let server = tokio::spawn(run_server_with_zookeeper(port));
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            run_client_with_zookeeper().await?;
            server.abort();
        }
        "server-etcd" => {
            println!("Mode: server-etcd (gRPC + Etcd registration)\n");
            run_server_with_etcd(port).await?;
        }
        "client-etcd" => {
            println!("Mode: client-etcd (Etcd discovery + streaming)\n");
            run_client_with_etcd().await?;
        }
        "etcd" => {
            println!("Mode: etcd (all demos + Etcd streaming)\n");
            demo_config();
            demo_filters().await;
            demo_loadbalance();
            demo_cluster().await?;
            println!("--- Launching Etcd server + client ---\n");
            let server = tokio::spawn(run_server_with_etcd(port));
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            run_client_with_etcd().await?;
            server.abort();
        }
        _ => {
            // "both" or no arg — run all demos without external dependencies
            println!("Mode: both (all demos, no external deps)\n");

            demo_config();
            demo_filters().await;
            demo_loadbalance();
            demo_cluster().await?;

            // Use a different port for streaming to avoid conflicts if run again quickly
            let stream_port = port + 100;
            demo_streaming_direct(stream_port).await?;
        }
    }

    println!("=== All demos complete! ===");
    Ok(())
}
