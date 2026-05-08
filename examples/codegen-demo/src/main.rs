use anyhow::Result;
use tonic::{Request, Response, Status};

// Include the generated Dubbo integration code
include!(concat!(env!("OUT_DIR"), "/greeter_dubbo.rs"));

use proto::greeter_server::Greeter;
use proto::{HelloReply, HelloRequest};

#[derive(Debug, Default)]
struct MyGreeter;

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        let name = request.into_inner().name;
        Ok(Response::new(HelloReply {
            message: format!("Hello, {name}! (from dubbo-rs-codegen)"),
        }))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== dubbo-rs Codegen Demo ===\n");

    let host = "127.0.0.1";
    let port: u16 = 50051;

    // Step 1: Build and start the server using the generated registration function
    println!("[Server] Starting on {host}:{port}...");
    let server = dubbo_rs::server::Server::new()
        .with_application("codegen-demo")
        .with_protocol_config(dubbo_rs::config::ProtocolConfig::new("tri", host, port));
    let server = register_greeter_service(server, MyGreeter);

    tokio::spawn(async move {
        server.serve().await.expect("server failed");
    });

    // Wait for server to start
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    println!("[Server] Started successfully\n");

    // Step 2: Connect using the generated channel client
    println!("[Client] Connecting to server...");
    let mut client = GreeterChannelClient::connect(format!("http://{host}:{port}")).await?;
    println!("[Client] Connected\n");

    // Step 3: Make an RPC call
    let response = client
        .say_hello(HelloRequest {
            name: "dubbo-rs-codegen".into(),
        })
        .await?;
    println!("[Client] Response: {}\n", response.into_inner().message);

    // Step 4: Demonstrate invoker client API (just show the type is available)
    println!("[InvokerClient] GreeterInvokerClient is available for invoker-based calls.");
    println!("[InvokerClient] Construct with: GreeterInvokerClient::new(Box::new(your_invoker))");

    println!("\n=== Codegen Demo completed! ===");
    Ok(())
}
