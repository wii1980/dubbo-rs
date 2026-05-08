// Phase 3: Triple/gRPC protocol verification tests.
//
// Generated code from proto/triple_test.proto is included in a nested module
// to avoid name conflicts with dubbo-rs-protocol-triple's triple wrapper types.

#![allow(clippy::semicolon_if_nothing_returned)]

#[allow(
    clippy::similar_names,
    clippy::default_trait_access,
    clippy::too_many_lines
)]
mod gen_triple {
    include!(concat!(env!("OUT_DIR"), "/triple.test.rs"));
}

use dubbo_rs_protocol_triple::triple as triple_wrapper;
use dubbo_rs_protocol_triple::TripleInvoker;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::Invoker;
use gen_triple::triple_service_server;
use prost::Message;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

fn port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

// ── Protobuf message tests ──────────────────────────────────────────────

#[test]
fn c_001_request_wrapper() {
    let req = triple_wrapper::TripleRequestWrapper {
        serialize_type: "protobuf".into(),
        args: vec![b"hello".to_vec()],
        arg_types: vec!["Ljava/lang/String;".into()],
    };
    let mut buf = Vec::new();
    Message::encode(&req, &mut buf).unwrap();
    let got = triple_wrapper::TripleRequestWrapper::decode(buf.as_slice()).unwrap();
    assert_eq!(got.serialize_type, "protobuf");
    assert_eq!(got.args[0], b"hello");
}

#[test]
fn c_002_response_wrapper() {
    let resp = triple_wrapper::TripleResponseWrapper {
        serialize_type: "json".into(),
        data: b"result".to_vec(),
        r#type: "reply".into(),
    };
    let mut buf = Vec::new();
    Message::encode(&resp, &mut buf).unwrap();
    let got = triple_wrapper::TripleResponseWrapper::decode(buf.as_slice()).unwrap();
    assert_eq!(got.data, b"result");
}

#[test]
fn c_003_empty_request() {
    let req = triple_wrapper::TripleRequestWrapper::default();
    let mut buf = Vec::new();
    Message::encode(&req, &mut buf).unwrap();
    let got = triple_wrapper::TripleRequestWrapper::decode(buf.as_slice()).unwrap();
    assert!(got.args.is_empty());
}

#[test]
fn c_004_triple_invoker_construction() {
    let mut url = URL::new("tri", "/triple.test.TripleService");
    url.ip = "127.0.0.1".into();
    url.port = "50051".into();
    let invoker = TripleInvoker::from_url(url).with_serialize_type("protobuf");
    assert!(invoker.is_available());
    assert_eq!(invoker.get_url().path, "/triple.test.TripleService");
}

// ── Network roundtrip using generated TripleService ──────────────────────

struct MockTriple;

#[tonic::async_trait]
impl triple_service_server::TripleService for MockTriple {
    async fn echo(
        &self,
        request: tonic::Request<gen_triple::TripleRequestWrapper>,
    ) -> Result<tonic::Response<gen_triple::TripleResponseWrapper>, tonic::Status> {
        let req = request.into_inner();
        eprintln!(
            "[TripleServer] echo: type={}, args={}",
            req.serialize_type,
            req.args.len()
        );

        if req.args.first().is_some_and(|a| a == b"trigger-metadata") {
            let echoed = format!(
                "metadata_echo:args={},types={}",
                req.args.len(),
                req.arg_types.join(",")
            );
            return Ok(tonic::Response::new(gen_triple::TripleResponseWrapper {
                serialize_type: req.serialize_type,
                data: echoed.into_bytes(),
                r#type: String::new(),
            }));
        }

        if req.args.first().is_some_and(|a| a == b"trigger-not-found") {
            return Err(tonic::Status::not_found("service not found"));
        }

        if req
            .args
            .first()
            .is_some_and(|a| a == b"trigger-internal-details")
        {
            return Err(tonic::Status::with_details(
                tonic::Code::Internal,
                "internal error with details",
                b"detailed-error-info".to_vec().into(),
            ));
        }

        if req.args.first().is_some_and(|a| a == b"trigger-internal") {
            return Err(tonic::Status::internal("internal server error"));
        }

        if req.args.first().is_some_and(|a| a == b"trigger-deadline") {
            return Err(tonic::Status::deadline_exceeded("deadline exceeded"));
        }
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
        let reply = format!("Hello from Triple! args={}", req.args.len());
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
        request: tonic::Request<gen_triple::TripleRequestWrapper>,
    ) -> Result<tonic::Response<Self::ServerStreamEchoStream>, tonic::Status> {
        let req = request.into_inner();
        eprintln!(
            "[TripleServer] server_stream_echo: type={}, args={:?}",
            req.serialize_type, req.args
        );
        let replies = vec![
            Ok(gen_triple::TripleResponseWrapper {
                serialize_type: req.serialize_type.clone(),
                data: b"chunk1".to_vec(),
                r#type: String::new(),
            }),
            Ok(gen_triple::TripleResponseWrapper {
                serialize_type: req.serialize_type.clone(),
                data: b"chunk2".to_vec(),
                r#type: String::new(),
            }),
            Ok(gen_triple::TripleResponseWrapper {
                serialize_type: req.serialize_type,
                data: b"chunk3".to_vec(),
                r#type: String::new(),
            }),
        ];
        Ok(tonic::Response::new(tokio_stream::iter(replies)))
    }

    async fn client_stream_echo(
        &self,
        request: tonic::Request<tonic::codec::Streaming<gen_triple::TripleRequestWrapper>>,
    ) -> Result<tonic::Response<gen_triple::TripleResponseWrapper>, tonic::Status> {
        let mut count = 0usize;
        let mut stream = request.into_inner();
        while let Ok(Some(_msg)) = stream.message().await {
            count += 1;
            eprintln!("[TripleServer] client_stream_echo: received msg #{count}");
        }
        eprintln!("[TripleServer] client_stream_echo: total {count} messages");
        Ok(tonic::Response::new(gen_triple::TripleResponseWrapper {
            serialize_type: "protobuf".into(),
            data: count.to_string().into_bytes(),
            r#type: String::new(),
        }))
    }

    type BidiStreamEchoStream = tokio_stream::Iter<
        std::vec::IntoIter<Result<gen_triple::TripleResponseWrapper, tonic::Status>>,
    >;

    async fn bidi_stream_echo(
        &self,
        request: tonic::Request<tonic::codec::Streaming<gen_triple::TripleRequestWrapper>>,
    ) -> Result<tonic::Response<Self::BidiStreamEchoStream>, tonic::Status> {
        let mut count = 0usize;
        let mut stream = request.into_inner();
        while let Ok(Some(_msg)) = stream.message().await {
            count += 1;
            eprintln!("[TripleServer] bidi_stream_echo: received msg #{count}");
        }
        let replies = vec![Ok(gen_triple::TripleResponseWrapper {
            serialize_type: "protobuf".into(),
            data: format!("bidi_received_{count}").into_bytes(),
            r#type: String::new(),
        })];
        Ok(tonic::Response::new(tokio_stream::iter(replies)))
    }
}

async fn start(port: u16) -> (tokio::task::JoinHandle<()>, Arc<Notify>) {
    let shutdown = Arc::new(Notify::new());
    let s = shutdown.clone();
    let h = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(triple_service_server::TripleServiceServer::new(MockTriple))
            .serve_with_shutdown(format!("127.0.0.1:{port}").parse().unwrap(), async {
                s.notified().await;
            })
            .await
            .ok();
    });
    // Yield to let the spawned server task begin binding the port, then poll.
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        cross_lang_tests::PortAllocator::wait_for_port(
            port,
            std::time::Duration::from_secs(5),
        ),
        "Server did not start on port {port} in time"
    );
    (h, shutdown)
}

#[tokio::test]
async fn c_005_connect_and_channel() {
    let p = port();
    let (_h, shutdown) = start(p).await;

    let mut url = URL::new("tri", "/triple.test.TripleService");
    url.ip = "127.0.0.1".into();
    url.port = p.to_string();

    let invoker = TripleInvoker::from_url(url).with_serialize_type("protobuf");
    invoker.connect().await.expect("connect");
    assert!(invoker.channel().read().await.is_some());
    shutdown.notify_one();
}

/// C-006: TripleInvoker → TripleServiceServer Echo method
#[tokio::test]
async fn c_006_triple_echo() {
    let p = port();
    let (_h, shutdown) = start(p).await;

    let mut url = URL::new("tri", "/triple.test.TripleService");
    url.ip = "127.0.0.1".into();
    url.port = p.to_string();

    let invoker = TripleInvoker::from_url(url.clone()).with_serialize_type("protobuf");
    invoker.connect().await.expect("connect");

    let mut ctx = dubbo_rs_protocol::InvocationContext::new("Echo", url);
    ctx.arguments = vec![b"test".to_vec()];

    let result = invoker.invoke(&mut ctx).await.expect("invoke");
    assert!(!result.is_error());
    if let Some(v) = result.value {
        let reply = String::from_utf8_lossy(&v);
        assert!(reply.contains("pong"), "got: {reply}");
        eprintln!("[C-006] Triple echo: {reply}");
    }
    shutdown.notify_one();
}

/// C-007: TripleInvoker → TripleServiceServer SayHello method
#[tokio::test]
async fn c_007_triple_sayhello() {
    let p = port();
    let (_h, shutdown) = start(p).await;

    let mut url = URL::new("tri", "/triple.test.TripleService");
    url.ip = "127.0.0.1".into();
    url.port = p.to_string();

    let invoker = TripleInvoker::from_url(url.clone()).with_serialize_type("protobuf");
    invoker.connect().await.expect("connect");

    let mut ctx = dubbo_rs_protocol::InvocationContext::new("SayHello", url);
    ctx.arguments = vec![b"world".to_vec()];

    let result = invoker.invoke(&mut ctx).await.expect("invoke");
    assert!(!result.is_error());
    if let Some(v) = result.value {
        let reply = String::from_utf8_lossy(&v);
        assert!(reply.contains("Hello"), "got: {reply}");
        eprintln!("[C-007] Triple sayHello: {reply}");
    }
    shutdown.notify_one();
}

// ── Streaming Tests (C-S-* series) ──────────────────────────────────────

/// C-S-001: Server streaming — receive 3 response chunks
#[tokio::test]
async fn cs_001_server_streaming() {
    let p = port();
    let (_h, shutdown) = start(p).await;

    let mut url = URL::new("tri", "/triple.test.TripleService");
    url.ip = "127.0.0.1".into();
    url.port = p.to_string();

    let invoker = TripleInvoker::from_url(url.clone()).with_serialize_type("protobuf");
    invoker.connect().await.expect("connect");

    let ctx = dubbo_rs_protocol::InvocationContext::new("ServerStreamEcho", url);
    let mut stream = invoker.server_streaming(&ctx).await.expect("server stream");

    let mut chunks = Vec::new();
    while let Some(result) = stream.next().await {
        if let Some(data) = result.value {
            chunks.push(String::from_utf8_lossy(&data).to_string());
            eprintln!("[C-S-001] received: {}", String::from_utf8_lossy(&data));
        }
    }
    assert_eq!(chunks.len(), 3, "expected 3 chunks");
    assert_eq!(chunks[0], "chunk1");
    assert_eq!(chunks[1], "chunk2");
    assert_eq!(chunks[2], "chunk3");
    shutdown.notify_one();
}

/// C-S-002: Client streaming — send 3 requests, receive count
#[tokio::test]
async fn cs_002_client_streaming() {
    let p = port();
    let (_h, shutdown) = start(p).await;

    let mut url = URL::new("tri", "/triple.test.TripleService");
    url.ip = "127.0.0.1".into();
    url.port = p.to_string();

    let invoker = TripleInvoker::from_url(url.clone()).with_serialize_type("protobuf");
    invoker.connect().await.expect("connect");

    let contexts = (0..3)
        .map(|i| {
            let mut u = URL::new("tri", "/triple.test.TripleService");
            u.ip = "127.0.0.1".into();
            u.port = p.to_string();
            dubbo_rs_protocol::InvocationContext::new("ClientStreamEcho", u)
                .with_arguments(vec![format!("msg{i}").into_bytes()])
        })
        .collect();

    let result = invoker
        .client_streaming(contexts)
        .await
        .expect("client stream");
    assert!(!result.is_error());
    if let Some(data) = result.value {
        let reply = String::from_utf8_lossy(&data);
        assert_eq!(reply, "3", "expected count=3, got {reply}");
        eprintln!("[C-S-002] server received {reply} messages");
    }
    shutdown.notify_one();
}

/// C-S-003: Bidi streaming — send 2 requests, receive response
#[tokio::test]
async fn cs_003_bidi_streaming() {
    let p = port();
    let (_h, shutdown) = start(p).await;

    let mut url = URL::new("tri", "/triple.test.TripleService");
    url.ip = "127.0.0.1".into();
    url.port = p.to_string();

    let invoker = TripleInvoker::from_url(url.clone()).with_serialize_type("protobuf");
    invoker.connect().await.expect("connect");

    let contexts = (0..2)
        .map(|i| {
            let mut u = URL::new("tri", "/triple.test.TripleService");
            u.ip = "127.0.0.1".into();
            u.port = p.to_string();
            dubbo_rs_protocol::InvocationContext::new("BidiStreamEcho", u)
                .with_arguments(vec![format!("msg{i}").into_bytes()])
        })
        .collect();

    let mut bidi = invoker.bidi_streaming(contexts).await.expect("bidi stream");
    let mut replies = Vec::new();
    while let Some(result) = bidi.recv().await {
        if let Some(data) = result.value {
            replies.push(String::from_utf8_lossy(&data).to_string());
            eprintln!("[C-S-003] received: {}", String::from_utf8_lossy(&data));
        }
    }
    assert_eq!(replies.len(), 1, "expected 1 bidi reply");
    assert!(replies[0].contains('2'), "reply should contain count 2");
    shutdown.notify_one();
}

// ── Error & Metadata Tests (C-008 ~ C-010) ──────────────────────────────

/// C-008: Attachment/metadata passthrough through gRPC request wrapper.
///
/// Since TripleInvoker does not yet propagate attachments as gRPC metadata
/// headers, we test at the wrapper level: encode metadata info in args and
/// verify the server receives and echoes it back in the response data.
#[tokio::test]
async fn c_008_metadata_passthrough() {
    let p = port();
    let (_h, shutdown) = start(p).await;

    let mut url = URL::new("tri", "/triple.test.TripleService");
    url.ip = "127.0.0.1".into();
    url.port = p.to_string();

    let invoker = TripleInvoker::from_url(url.clone()).with_serialize_type("protobuf");
    invoker.connect().await.expect("connect");

    let mut ctx = dubbo_rs_protocol::InvocationContext::new("Echo", url);
    ctx.arguments = vec![b"trigger-metadata".to_vec(), b"value1".to_vec()];
    ctx.parameter_types = vec!["Ljava/lang/String;".into(), "Ljava/lang/Integer;".into()];

    let result = invoker.invoke(&mut ctx).await.expect("invoke");
    assert!(!result.is_error());
    if let Some(v) = result.value {
        let reply = String::from_utf8_lossy(&v);
        assert!(reply.contains("metadata_echo"), "got: {reply}");
        assert!(reply.contains("args=2"), "expected 2 args, got: {reply}");
        assert!(
            reply.contains("Ljava/lang/String;"),
            "expected arg type in echo, got: {reply}"
        );
        eprintln!("[C-008] Metadata echo: {reply}");
    }
    shutdown.notify_one();
}

/// C-009: gRPC status code error mapping.
///
/// Verify that tonic::Status errors from the server propagate to the client
/// as anyhow errors containing the status message.
#[tokio::test]
async fn c_009_grpc_error_mapping() {
    let p = port();
    let (_h, shutdown) = start(p).await;

    let mut url = URL::new("tri", "/triple.test.TripleService");
    url.ip = "127.0.0.1".into();
    url.port = p.to_string();

    let invoker = TripleInvoker::from_url(url.clone()).with_serialize_type("protobuf");
    invoker.connect().await.expect("connect");

    // NOT_FOUND
    let mut ctx = dubbo_rs_protocol::InvocationContext::new("Echo", url.clone());
    ctx.arguments = vec![b"trigger-not-found".to_vec()];
    let err = invoker
        .invoke(&mut ctx)
        .await
        .expect_err("should return error for not-found");
    let msg = err.to_string();
    assert!(
        msg.contains("service not found"),
        "expected 'service not found' in error, got: {msg}"
    );
    eprintln!("[C-009] NOT_FOUND error: {msg}");

    // INTERNAL
    let mut ctx = dubbo_rs_protocol::InvocationContext::new("Echo", url.clone());
    ctx.arguments = vec![b"trigger-internal".to_vec()];
    let err = invoker
        .invoke(&mut ctx)
        .await
        .expect_err("should return error for internal");
    let msg = err.to_string();
    assert!(
        msg.contains("internal server error"),
        "expected 'internal server error' in error, got: {msg}"
    );
    eprintln!("[C-009] INTERNAL error: {msg}");

    // DEADLINE_EXCEEDED
    let mut ctx = dubbo_rs_protocol::InvocationContext::new("Echo", url);
    ctx.arguments = vec![b"trigger-deadline".to_vec()];
    let err = invoker
        .invoke(&mut ctx)
        .await
        .expect_err("should return error for deadline");
    let msg = err.to_string();
    assert!(
        msg.contains("deadline exceeded"),
        "expected 'deadline exceeded' in error, got: {msg}"
    );
    eprintln!("[C-009] DEADLINE_EXCEEDED error: {msg}");

    shutdown.notify_one();
}

/// C-010: Error details with grpc-status-details-bin.
///
/// Verify that a tonic::Status with details (binary payload) propagates
/// to the client and the error message is accessible.
#[tokio::test]
async fn c_010_error_details() {
    let p = port();
    let (_h, shutdown) = start(p).await;

    let mut url = URL::new("tri", "/triple.test.TripleService");
    url.ip = "127.0.0.1".into();
    url.port = p.to_string();

    let invoker = TripleInvoker::from_url(url.clone()).with_serialize_type("protobuf");
    invoker.connect().await.expect("connect");

    let mut ctx = dubbo_rs_protocol::InvocationContext::new("Echo", url);
    ctx.arguments = vec![b"trigger-internal-details".to_vec()];

    let err = invoker
        .invoke(&mut ctx)
        .await
        .expect_err("should return error with details");
    let msg = err.to_string();
    assert!(
        msg.contains("internal error with details"),
        "expected 'internal error with details' in error, got: {msg}"
    );
    eprintln!("[C-010] Error with details: {msg}");

    shutdown.notify_one();
}

// ── Cross-language Triple Tests (Rust → Java) ──────────────────────────

struct JavaTripleProc {
    child: Option<Child>,
    port: u16,
}

impl JavaTripleProc {
    fn start(port: u16) -> Self {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let java_dir = project_root.join("../tests/java-fixtures");

        let status = Command::new("mvn")
            .args(["-q", "compile"])
            .current_dir(&java_dir)
            .env("JAVA_HOME", "/usr/lib/jvm/java-17-openjdk")
            .status()
            .expect("mvn compile failed");
        assert!(status.success(), "mvn compile should succeed");

        let cp_path = format!("/tmp/dubbo-rs-cp-{port}.txt");
        let cp_arg = format!("-Dmdep.outputFile={cp_path}");
        let cp_status = Command::new("mvn")
            .args(["dependency:build-classpath", "-q", &cp_arg])
            .current_dir(&java_dir)
            .env("JAVA_HOME", "/usr/lib/jvm/java-17-openjdk")
            .status()
            .expect("mvn dependency:build-classpath failed");
        assert!(cp_status.success(), "mvn build-classpath failed");

        let maven_classpath = std::fs::read_to_string(&cp_path)
            .expect("read classpath file")
            .trim()
            .to_string();
        let _ = std::fs::remove_file(&cp_path);

        let classpath = format!(
            "{}:{}",
            java_dir.join("target/classes").display(),
            maven_classpath
        );

        let java_bin = "/usr/lib/jvm/java-17-openjdk/bin/java";
        eprintln!("[JavaTripleProc] Starting TripleTestServer on port {port}...");
        let child = Command::new(java_bin)
            .args([
                "-cp",
                &classpath,
                "com.dubborst.fixtures.TripleTestServer",
                &port.to_string(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("failed to start TripleTestServer");

        let deadline = Instant::now() + Duration::from_secs(25);
        let mut ready = false;
        while Instant::now() < deadline {
            if TcpStream::connect_timeout(
                &format!("127.0.0.1:{port}").parse().unwrap(),
                Duration::from_secs(1),
            )
            .is_ok()
            {
                ready = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        assert!(
            ready,
            "Java TripleTestServer did not start on port {port} in time"
        );

        eprintln!("[JavaTripleProc] TripleTestServer ready on port {port}");
        JavaTripleProc {
            child: Some(child),
            port,
        }
    }

    #[allow(dead_code)]
    fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for JavaTripleProc {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            eprintln!("[JavaTripleProc] TripleTestServer stopped");
        }
    }
}

/// CT-001: Rust TripleInvoker → Java TripleTestServer sayHello.
#[tokio::test]
async fn ct_001_rs_to_java_triple_sayhello() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let _java = JavaTripleProc::start(port);

    let mut url = URL::new("tri", "/triple.test.TripleService");
    url.ip = "127.0.0.1".into();
    url.port = port.to_string();

    let invoker = TripleInvoker::from_url(url.clone()).with_serialize_type("protobuf");
    invoker
        .connect()
        .await
        .expect("connect to Java Triple server");

    let mut ctx = dubbo_rs_protocol::InvocationContext::new("SayHello", url);
    ctx.arguments = vec![b"world".to_vec()];

    let result = invoker.invoke(&mut ctx).await.expect("invoke");
    assert!(!result.is_error(), "CT-001: response should not be error");
    if let Some(v) = result.value {
        let reply = String::from_utf8_lossy(&v);
        assert!(
            reply.contains("Hello"),
            "CT-001: reply should contain Hello, got: {reply}"
        );
        assert!(
            reply.contains("Java"),
            "CT-001: reply should mention Java, got: {reply}"
        );
        eprintln!("[CT-001] Java Triple replied: {reply}");
    }
}

/// CT-002: Rust TripleInvoker → Java TripleTestServer echo.
#[tokio::test]
async fn ct_002_rs_to_java_triple_echo() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let _java = JavaTripleProc::start(port);

    let mut url = URL::new("tri", "/triple.test.TripleService");
    url.ip = "127.0.0.1".into();
    url.port = port.to_string();

    let invoker = TripleInvoker::from_url(url.clone()).with_serialize_type("protobuf");
    invoker.connect().await.expect("connect");

    let mut ctx = dubbo_rs_protocol::InvocationContext::new("Echo", url);
    ctx.arguments = vec![b"ping".to_vec()];

    let result = invoker.invoke(&mut ctx).await.expect("invoke");
    assert!(!result.is_error(), "CT-002: response should not be error");
    if let Some(v) = result.value {
        let reply = String::from_utf8_lossy(&v);
        assert_eq!(
            reply, "pong",
            "CT-002: echo should return pong, got: {reply}"
        );
        eprintln!("[CT-002] Java Triple echoed: {reply}");
    }
}
