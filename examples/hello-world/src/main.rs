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
        let reply = HelloReply {
            message: format!("Hello, {}! (from dubbo-rs)", req.name),
        };
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== dubbo-rs Hello World (Phase 1) ===\n");

    // --- Server ---
    let server_addr = "[::1]:50051".parse().unwrap();
    let greeter_service = MyGreeter;

    println!("Starting gRPC server on {server_addr}");
    let _server_handle = tokio::spawn(async move {
        Server::builder()
            .add_service(GreeterServer::new(greeter_service))
            .serve(server_addr)
            .await
            .expect("server failed");
    });

    // Give the server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // --- Client ---
    println!("Connecting to gRPC server...");
    let mut client = proto::greeter_client::GreeterClient::connect("http://[::1]:50051").await?;

    let request = tonic::Request::new(HelloRequest {
        name: "dubbo-rs".into(),
    });

    let response = client.say_hello(request).await?;
    println!("Response: {}", response.into_inner().message);

    println!("\n=== Hello World completed successfully! ===");
    Ok(())
}
