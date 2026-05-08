// Phase 6: Full-stack Instance integration test.
//
// Exercises the complete dubbo-rs stack:
//   Config → Instance → Server (tonic) → Client (gRPC Channel) → RPC call

#![allow(clippy::semicolon_if_nothing_returned, clippy::too_many_lines)]

#[allow(clippy::similar_names, clippy::default_trait_access)]
mod gen_triple {
    include!(concat!(env!("OUT_DIR"), "/triple.test.rs"));
}

use dubbo_rs_config::{ProtocolConfig, RootConfig};
use dubbo_rs_server::Server;
use dubbo_rs::Instance;
use gen_triple::triple_service_client::TripleServiceClient;
use gen_triple::triple_service_server::{TripleService, TripleServiceServer};
use std::net::TcpListener;

fn available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct MockTriple;

#[tonic::async_trait]
impl TripleService for MockTriple {
    async fn echo(
        &self,
        request: tonic::Request<gen_triple::TripleRequestWrapper>,
    ) -> Result<tonic::Response<gen_triple::TripleResponseWrapper>, tonic::Status> {
        let req = request.into_inner();
        Ok(tonic::Response::new(gen_triple::TripleResponseWrapper {
            serialize_type: req.serialize_type,
            data: b"pong".to_vec(),
            r#type: String::new(),
        }))
    }

    async fn say_hello(
        &self,
        request: tonic::Request<gen_triple::TripleRequestWrapper>,
    ) -> Result<tonic::Response<gen_triple::TripleResponseWrapper>, tonic::Status> {
        let req = request.into_inner();
        let reply = format!("Hello from Instance! args={}", req.args.len());
        Ok(tonic::Response::new(gen_triple::TripleResponseWrapper {
            serialize_type: req.serialize_type,
            data: reply.into_bytes(),
            r#type: String::new(),
        }))
    }

    type ServerStreamEchoStream = tokio_stream::Iter<
        std::vec::IntoIter<Result<gen_triple::TripleResponseWrapper, tonic::Status>>,
    >;

    async fn server_stream_echo(
        &self,
        _: tonic::Request<gen_triple::TripleRequestWrapper>,
    ) -> Result<tonic::Response<Self::ServerStreamEchoStream>, tonic::Status> {
        Ok(tonic::Response::new(tokio_stream::iter(vec![])))
    }

    async fn client_stream_echo(
        &self,
        _: tonic::Request<tonic::codec::Streaming<gen_triple::TripleRequestWrapper>>,
    ) -> Result<tonic::Response<gen_triple::TripleResponseWrapper>, tonic::Status> {
        Ok(tonic::Response::new(
            gen_triple::TripleResponseWrapper::default(),
        ))
    }

    type BidiStreamEchoStream = tokio_stream::Iter<
        std::vec::IntoIter<Result<gen_triple::TripleResponseWrapper, tonic::Status>>,
    >;

    async fn bidi_stream_echo(
        &self,
        _: tonic::Request<tonic::codec::Streaming<gen_triple::TripleRequestWrapper>>,
    ) -> Result<tonic::Response<Self::BidiStreamEchoStream>, tonic::Status> {
        Ok(tonic::Response::new(tokio_stream::iter(vec![])))
    }
}

/// IF-001: Full-stack smoke test — Instance with Server + Client, make a gRPC call.
#[tokio::test]
async fn if_001_instance_full_stack() {
    let port = available_port();

    let server = Server::new()
        .with_application("instance-test")
        .with_version("1.0.0")
        .with_protocol_config(ProtocolConfig::new("tri", "0.0.0.0", port))
        .register_service(|mut builder| builder.add_service(TripleServiceServer::new(MockTriple)));

    let config = RootConfig::default()
        .with_application("instance-test")
        .with_version("1.0.0")
        .with_protocol(ProtocolConfig::new("tri", "0.0.0.0", port));

    let mut instance = Instance::new(config);
    instance.set_provider_service(server);

    instance.start().expect("Instance should start");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let channel = tonic::transport::Channel::from_shared(format!("http://127.0.0.1:{port}"))
        .unwrap()
        .connect()
        .await
        .expect("should connect to server");

    let mut client = TripleServiceClient::new(channel);

    let req = tonic::Request::new(gen_triple::TripleRequestWrapper {
        serialize_type: "protobuf".into(),
        args: vec![b"world".to_vec()],
        arg_types: vec!["Ljava/lang/String;".into()],
    });

    let response = client
        .say_hello(req)
        .await
        .expect("sayHello should succeed");
    let inner = response.into_inner();
    let reply = String::from_utf8_lossy(&inner.data);
    assert!(
        reply.contains("Hello"),
        "IF-001: should contain Hello, got: {reply}"
    );
    assert!(
        reply.contains("Instance"),
        "IF-001: should mention Instance, got: {reply}"
    );
}

/// IF-002: Instance Echo method via full stack.
#[tokio::test]
async fn if_002_instance_echo() {
    let port = available_port();

    let server = Server::new()
        .with_application("instance-test")
        .with_version("1.0.0")
        .with_protocol_config(ProtocolConfig::new("tri", "0.0.0.0", port))
        .register_service(|mut builder| builder.add_service(TripleServiceServer::new(MockTriple)));

    let config = RootConfig::default()
        .with_application("instance-test")
        .with_version("1.0.0")
        .with_protocol(ProtocolConfig::new("tri", "0.0.0.0", port));

    let mut instance = Instance::new(config);
    instance.set_provider_service(server);
    instance.start().expect("Instance start");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let channel = tonic::transport::Channel::from_shared(format!("http://127.0.0.1:{port}"))
        .unwrap()
        .connect()
        .await
        .expect("connect");

    let mut client = TripleServiceClient::new(channel);
    let req = tonic::Request::new(gen_triple::TripleRequestWrapper {
        serialize_type: "protobuf".into(),
        args: vec![b"ping".to_vec()],
        arg_types: vec![],
    });

    let response = client.echo(req).await.expect("echo should succeed");
    let inner = response.into_inner();
    let reply = String::from_utf8_lossy(&inner.data);
    assert_eq!(
        reply, "pong",
        "IF-002: echo should return pong, got: {reply}"
    );
}
