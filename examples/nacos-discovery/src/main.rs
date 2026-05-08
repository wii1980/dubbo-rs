//! Nacos Service Discovery Demo.
//!
//! Demonstrates the full service registration and discovery lifecycle:
//! - Server: register service URL with Nacos, start gRPC server
//! - Client: subscribe to Nacos for service discovery, invoke via gRPC
//!
//! Usage:
//!   cargo run -p nacos-discovery -- server
//!   cargo run -p nacos-discovery -- client
//!   cargo run -p nacos-discovery -- both
//!
//! Environment variables:
//!   `NACOS_ADDR`      - Nacos address (default: 127.0.0.1:8848)
//!   `NACOS_NAMESPACE` - Nacos namespace (default: public)
//!   `NACOS_GROUP`     - Nacos group (default: `DEFAULT_GROUP`)
//!   `NACOS_USERNAME`  - Nacos auth username (optional)
//!   `NACOS_PASSWORD`  - Nacos auth password (optional)
//!   `SERVER_PORT`     - Server listen port (default: 50051)

use anyhow::Context;
use dubbo_rs::common::url::URL;
use dubbo_rs::nacos::NacosRegistry;
use dubbo_rs::registry::Registry;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

// Include the generated Dubbo integration code
include!(concat!(env!("OUT_DIR"), "/greeter_dubbo.rs"));

use proto::greeter_server::{Greeter, GreeterServer};
use proto::{HelloReply, HelloRequest};

#[derive(Debug, Default)]
pub struct MyGreeter;

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        let req = request.into_inner();
        Ok(Response::new(HelloReply {
            message: format!("Hello, {}! (from nacos-discovered service)", req.name),
        }))
    }
}

fn nacos_addr() -> String {
    std::env::var("NACOS_ADDR").unwrap_or_else(|_| "127.0.0.1:8848".to_string())
}

fn server_port() -> u16 {
    std::env::var("SERVER_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(50051)
}

async fn run_server(port: u16) -> Result<(), anyhow::Error> {
    tracing::info!("Starting Nacos service discovery demo (port: {port})");

    let mut service_url = URL::new("tri", "/com.example.Greeter");
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

    tracing::info!("Connected to Nacos at {addr}, registering service: {service_url:?}");
    registry
        .register(service_url.clone())
        .await
        .context("Failed to register service with Nacos")?;
    tracing::info!("Service registered successfully!");

    let addr_str = format!("127.0.0.1:{port}");
    let addr = addr_str.parse()?;
    tracing::info!("Starting gRPC server on {addr_str}...");

    Server::builder()
        .add_service(GreeterServer::new(MyGreeter))
        .serve(addr)
        .await?;

    Ok(())
}

async fn run_client() -> Result<(), anyhow::Error> {
    use dubbo_rs::registry::{NotifyListener, ServiceEvent};
    use std::sync::{Arc, Mutex};

    struct DiscoveryListener {
        listen_url: URL,
        discovered: Arc<Mutex<Vec<URL>>>,
    }
    #[async_trait::async_trait]
    impl NotifyListener for DiscoveryListener {
        async fn notify(&self, event: ServiceEvent) {
            match event {
                ServiceEvent::Add(urls) => {
                    tracing::info!("Discovered {} provider(s):", urls.len());
                    for url in &urls {
                        tracing::info!("  - {}", url.get_address());
                    }
                    let mut discovered = self.discovered.lock().unwrap();
                    discovered.extend(urls);
                }
                ServiceEvent::Remove(urls) => {
                    tracing::info!("Provider(s) removed: {:?}", urls);
                    let mut discovered = self.discovered.lock().unwrap();
                    let remove_addrs: std::collections::HashSet<String> =
                        urls.iter().map(URL::get_address).collect();
                    discovered.retain(|u| !remove_addrs.contains(&u.get_address()));
                }
                ServiceEvent::Update(urls) => {
                    tracing::info!("Provider(s) updated: {:?}", urls);
                    let mut discovered = self.discovered.lock().unwrap();
                    *discovered = urls;
                }
            }
        }
        fn listen_url(&self) -> URL {
            self.listen_url.clone()
        }
    }

    tracing::info!("Starting Nacos service discovery client");

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

    let service_url = URL::new("tri", "/com.example.Greeter");

    tracing::info!("Subscribing to service notifications for /com.example.Greeter");

    let discovered = Arc::new(Mutex::new(Vec::new()));
    let listener = Arc::new(DiscoveryListener {
        listen_url: service_url.clone(),
        discovered: discovered.clone(),
    });
    registry
        .subscribe(service_url.clone(), listener)
        .await
        .context("Failed to subscribe to Nacos")?;
    tracing::info!("Subscribed. Waiting for service discovery events...");

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
                    "No provider discovered from Nacos for /com.example.Greeter. Ensure the server is registered and Nacos is reachable at {addr}"
                );
            }
        }
    };
    tracing::info!("Connecting to gRPC server at {server_addr}...");
    let mut client = proto::greeter_client::GreeterClient::connect(server_addr).await?;

    for i in 0..3 {
        let request = tonic::Request::new(HelloRequest {
            name: format!("dubbo-rs-{i}"),
        });
        let response = client.say_hello(request).await?;
        tracing::info!("Call {i} → {}", response.into_inner().message);
    }

    tracing::info!("Demo complete!");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "both".to_string());
    let port = server_port();

    println!("=== dubbo-rs Nacos Service Discovery (Phase 2) ===\n");
    tracing::info!("Mode: {mode}, Port: {port}");

    match mode.as_str() {
        "server" => run_server(port).await?,
        "client" => run_client().await?,
        _ => {
            let server = tokio::spawn(run_server(port));
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            run_client().await?;
            server.abort();
        }
    }

    Ok(())
}
