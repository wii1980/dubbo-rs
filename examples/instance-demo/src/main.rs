use std::sync::Arc;

use anyhow::Result;
use tonic::{Request, Response, Status};

// All imports through the single `dubbo` crate
use dubbo_rs::client::Client;
use dubbo_rs::config::ProtocolConfig;
use dubbo_rs::filter::GracefulShutdownFilter;
use dubbo_rs::server::Server;
use dubbo_rs::Instance;

// Include the generated Dubbo integration code (package greet → greet_dubbo.rs)
include!(concat!(env!("OUT_DIR"), "/greet_dubbo.rs"));

use proto::greeter_server::Greeter;
use proto::{HelloReply, HelloRequest};

#[derive(Default)]
struct MyGreeter;

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        let name = request.into_inner().name;
        println!("[Server] Received request from: {name}");
        Ok(Response::new(HelloReply {
            message: format!("Hello, {name}! (from dubbo-rs Instance API)"),
        }))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== dubbo-rs Instance API Demo ===\n");

    let host = "127.0.0.1";
    let port: u16 = 50051;

    // ============================================================
    // Step 1: RootConfig — Builder pattern
    // ============================================================
    let config = dubbo_rs::config::RootConfig::default()
        .with_application("instance-demo")
        .with_version("1.0.0")
        .with_protocol(ProtocolConfig::new("tri", host, port));

    println!("[Config] Application: {}", config.application);
    println!(
        "[Config] Protocol: {}://{}:{}\n",
        config.protocols[0].name, config.protocols[0].host, config.protocols[0].port,
    );

    // ============================================================
    // Step 2: Server — Builder + generated registration function
    // ============================================================
    let server = Server::new()
        .with_application("instance-demo")
        .with_protocol_config(ProtocolConfig::new("tri", host, port));
    let server = register_greeter_service(server, MyGreeter);

    // ============================================================
    // Step 3: Client — Builder with URL + protocol config
    // ============================================================
    let client = Client::new()
        .with_url(format!("tri://{host}:{port}/greet.Greeter"))
        .with_protocol_config(ProtocolConfig::new("tri", host, port));

    // ============================================================
    // Step 4: Instance — unified entry: config + server + client + shutdown filter
    // ============================================================
    let shutdown_filter = Arc::new(GracefulShutdownFilter::new());

    let mut instance = Instance::new(config);
    instance.set_provider_service(server);
    instance.set_client(client);
    instance.set_shutdown_filter(shutdown_filter.clone());

    // Spawn the server in background
    instance.start()?;
    println!("[Instance] Server started in background\n");

    // Wait for server to bind
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // ============================================================
    // Step 5: RPC calls — connect via tonic channel and invoke
    // ============================================================
    let channel = tonic::transport::Channel::from_shared(format!("http://{host}:{port}"))?
        .connect()
        .await?;
    let mut grpc_client = proto::greeter_client::GreeterClient::new(channel);

    for name in &["dubbo-rs", "Instance API", "World"] {
        let request = Request::new(HelloRequest {
            name: (*name).to_string(),
        });
        let response = grpc_client.say_hello(request).await?;
        println!("[Client] Response: {}", response.into_inner().message);
    }
    println!();

    // ============================================================
    // Step 6: Graceful shutdown — trigger filter + instance.shutdown()
    // ============================================================
    println!("[Instance] Triggering graceful shutdown...");
    shutdown_filter.shutdown();
    println!(
        "[Instance] Shutdown flag set: is_shutdown={}",
        shutdown_filter.is_shutdown(),
    );

    // Instance::shutdown() cleans up registries and waits for in-flight requests
    instance.shutdown().await;
    println!("[Instance] Shutdown complete.");

    println!("\n=== Instance API Demo completed! ===");
    Ok(())
}
