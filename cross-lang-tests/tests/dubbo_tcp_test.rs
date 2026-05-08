// Phase 2: Dubbo TCP protocol verification tests.
//
// Test ID conventions:
//   B-H-*  = Header: magic, flags, request ID, body length encoding
//   B-B-*  = Body: dubbo version, service path, method, args, attachments
//   B-R-*  = Roundtrip: encode header+body → decode → verify

use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::InvocationContext;
use dubbo_rs_protocol_dubbo::body::{
    decode_request_body, decode_response_body, encode_request_body, encode_response_body,
};
use dubbo_rs_protocol_dubbo::codec::{DubboCodec, SerializationId, HEADER_LENGTH};
use dubbo_rs_remoting::{Codec, Request, Response};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn test_codec() -> DubboCodec {
    DubboCodec::new(SerializationId::Hessian2)
}

// ── B-H: Header Tests ─────────────────────────────────────────────────────

mod b_h_header {
    use super::*;

    /// B-H-001: Magic number 0xdabb in `BigEndian`
    #[test]
    fn bh_001_magic_number() {
        let codec = test_codec();
        let req = Request {
            id: 1,
            is_twoway: false,
            is_event: false,
            data: vec![],
        };
        let encoded = codec.encode_request(&req).expect("encode");
        // First 2 bytes = magic in BigEndian
        assert_eq!(encoded[0], 0xda, "B-H-001: magic byte 0");
        assert_eq!(encoded[1], 0xbb, "B-H-001: magic byte 1");

        // Verify BigEndian: 0xdabb as BE bytes
        let be = 0xdabbu16.to_be_bytes();
        assert_eq!(encoded[0..2], be, "B-H-001: magic as BE");
    }

    /// B-H-002: Request flag set in header (byte 2)
    #[test]
    fn bh_002_request_flag() {
        let codec = test_codec();
        let req = Request {
            id: 1,
            is_twoway: false,
            is_event: false,
            data: vec![],
        };
        let encoded = codec.encode_request(&req).expect("encode");
        // Flag byte (offset 2): FLAG_REQUEST (0x80) | serialization_id (2 for Hessian2)
        assert_eq!(
            encoded[2] & 0x80,
            0x80,
            "B-H-002: FLAG_REQUEST should be set"
        );
        assert_eq!(
            encoded[2] & 0x1f,
            2,
            "B-H-002: serialization ID should be 2 (Hessian2)"
        );
    }

    /// B-H-002b: Response flag (`FLAG_REQUEST` NOT set)
    #[test]
    fn bh_002b_response_flag() {
        let codec = test_codec();
        let resp = Response {
            id: 1,
            status: 20,
            data: vec![],
        };
        let encoded = codec.encode_response(&resp).expect("encode");
        // Response: FLAG_REQUEST (0x80) NOT set, only serialization ID
        assert_eq!(
            encoded[2] & 0x80,
            0,
            "B-H-002b: FLAG_REQUEST should NOT be set"
        );
        assert_eq!(
            encoded[2] & 0x1f,
            2,
            "B-H-002b: serialization ID should be 2"
        );
    }

    /// B-H-002c: `TwoWay` flag
    #[test]
    fn bh_002c_twoway_flag() {
        let codec = test_codec();
        let req = Request {
            id: 1,
            is_twoway: true,
            is_event: false,
            data: vec![],
        };
        let encoded = codec.encode_request(&req).expect("encode");
        assert_ne!(encoded[2] & 0x40, 0, "B-H-002c: FLAG_TWOWAY should be set");

        // Without twoway
        let req2 = Request {
            id: 1,
            is_twoway: false,
            is_event: false,
            data: vec![],
        };
        let encoded2 = codec.encode_request(&req2).expect("encode");
        assert_eq!(
            encoded2[2] & 0x40,
            0,
            "B-H-002c: FLAG_TWOWAY should NOT be set"
        );
    }

    /// B-H-002d: Event flag
    #[test]
    fn bh_002d_event_flag() {
        let codec = test_codec();
        let req = Request {
            id: 1,
            is_twoway: false,
            is_event: true,
            data: vec![],
        };
        let encoded = codec.encode_request(&req).expect("encode");
        assert_ne!(encoded[2] & 0x20, 0, "B-H-002d: FLAG_EVENT should be set");
    }

    /// B-H-003: Serialization ID = 2 for Hessian2
    #[test]
    fn bh_003_serialization_id() {
        let codec = test_codec();
        let req = Request {
            id: 1,
            is_twoway: false,
            is_event: false,
            data: vec![],
        };
        let encoded = codec.encode_request(&req).expect("encode");
        assert_eq!(encoded[2] & 0x1f, 2, "B-H-003: Hessian2 serial ID = 2");
    }

    /// B-H-004: Request ID in `BigEndian`
    #[test]
    fn bh_004_request_id_be() {
        let codec = test_codec();
        let req = Request {
            id: 42,
            is_twoway: false,
            is_event: false,
            data: vec![],
        };
        let encoded = codec.encode_request(&req).expect("encode");
        // Request ID at offset 4, 8 bytes, BigEndian
        let id_bytes = &encoded[4..12];
        let id = u64::from_be_bytes(id_bytes.try_into().unwrap());
        assert_eq!(id, 42, "B-H-004: request ID should be 42");
    }

    /// B-H-005: Body length field
    #[test]
    fn bh_005_body_length() {
        let codec = test_codec();
        let body_data = b"hello dubbo body";
        let req = Request {
            id: 1,
            is_twoway: false,
            is_event: false,
            data: body_data.to_vec(),
        };
        let encoded = codec.encode_request(&req).expect("encode");
        // Body length at offset 12, 4 bytes, BigEndian
        let len_bytes = &encoded[12..16];
        let body_len = u32::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
        assert_eq!(body_len, body_data.len(), "B-H-005: body length matches");
        assert_eq!(
            encoded.len(),
            HEADER_LENGTH + body_data.len(),
            "B-H-005: total frame size"
        );
    }

    /// B-H-005b: Empty body
    #[test]
    fn bh_005b_body_length_zero() {
        let codec = test_codec();
        let req = Request {
            id: 1,
            is_twoway: false,
            is_event: false,
            data: vec![],
        };
        let encoded = codec.encode_request(&req).expect("encode");
        let len_bytes = &encoded[12..16];
        let body_len = u32::from_be_bytes(len_bytes.try_into().unwrap());
        assert_eq!(body_len, 0, "B-H-005b: empty body length = 0");
        assert_eq!(encoded.len(), HEADER_LENGTH, "B-H-005b: header only");
    }

    /// B-H-005c: Response body length
    #[test]
    fn bh_005c_response_body_length() {
        let codec = test_codec();
        let resp = Response {
            id: 1,
            status: 20,
            data: b"response data".to_vec(),
        };
        let encoded = codec.encode_response(&resp).expect("encode");
        let len_bytes = &encoded[12..16];
        let body_len = u32::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
        assert_eq!(
            body_len,
            b"response data".len(),
            "B-H-005c: response body length"
        );
    }

    /// B-H-005d: Large body length (> 64KB)
    #[test]
    fn bh_005d_large_body() {
        let codec = test_codec();
        let large = vec![0xABu8; 70000];
        let req = Request {
            id: 1,
            is_twoway: false,
            is_event: false,
            data: large.clone(),
        };
        let encoded = codec.encode_request(&req).expect("encode");
        let len_bytes = &encoded[12..16];
        let body_len = u32::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
        assert_eq!(body_len, large.len(), "B-H-005d: large body length");
    }

    /// Status byte: 0 for requests, configurable for responses
    #[test]
    fn bh_006_status_byte() {
        let codec = test_codec();
        // Request: status byte at offset 3 should be 0
        let req = Request {
            id: 1,
            is_twoway: false,
            is_event: false,
            data: vec![],
        };
        let encoded = codec.encode_request(&req).expect("encode");
        assert_eq!(encoded[3], 0, "B-H-006: request status = 0");

        // Response: status should be preserved
        let resp = Response {
            id: 1,
            status: 30,
            data: vec![],
        };
        let encoded = codec.encode_response(&resp).expect("encode");
        assert_eq!(encoded[3], 30, "B-H-006: response status preserved");
    }

    /// Decode roundtrip: encode → decode → verify all header fields
    #[test]
    fn bh_007_encode_decode_header_roundtrip() {
        let codec = test_codec();
        let req = Request {
            id: 12345,
            is_twoway: true,
            is_event: false,
            data: b"test payload".to_vec(),
        };
        let encoded = codec.encode_request(&req).expect("encode");
        let decoded = codec.decode_request(&encoded).expect("decode");

        assert_eq!(decoded.id, 12345);
        assert!(decoded.is_twoway);
        assert!(!decoded.is_event);
        assert_eq!(decoded.data, b"test payload");
    }

    #[test]
    fn bh_008_encode_decode_response_header_roundtrip() {
        let codec = test_codec();
        let resp = Response {
            id: 999,
            status: 20,
            data: b"response".to_vec(),
        };
        let encoded = codec.encode_response(&resp).expect("encode");
        let decoded = codec.decode_response(&encoded).expect("decode");

        assert_eq!(decoded.id, 999);
        assert_eq!(decoded.status, 20);
        assert_eq!(decoded.data, b"response");
    }
}

// ── B-B: Body Tests ───────────────────────────────────────────────────────

mod b_b_body {
    use super::*;

    /// B-B-001: Dubbo version field in request body
    #[test]
    fn bb_001_dubbo_version() {
        let mut url = URL::new("dubbo", "/com.example.Greeter");
        url.set_param("version", "1.0.0");
        let ctx = InvocationContext::new("sayHello", url)
            .with_parameter_types(vec!["Ljava/lang/String;".to_string()])
            .with_arguments(vec![b"world".to_vec()]);
        let bytes = encode_request_body(&ctx).expect("encode body");

        let mut dec = dubbo_rs_serialization_hessian2::decoder::Decoder::new(&bytes);
        let version = dec.read_string().expect("read dubbo_version");
        assert_eq!(version, "2.0.2", "B-B-001: dubbo version should be 2.0.2");
    }

    /// B-B-002: Service path
    #[test]
    fn bb_002_service_path() {
        let mut url = URL::new("dubbo", "/com.example.Greeter");
        url.set_param("version", "1.0.0");
        let ctx = InvocationContext::new("sayHello", url)
            .with_parameter_types(vec!["Ljava/lang/String;".to_string()])
            .with_arguments(vec![b"world".to_vec()]);
        let bytes = encode_request_body(&ctx).expect("encode body");

        let mut dec = dubbo_rs_serialization_hessian2::decoder::Decoder::new(&bytes);
        let _version = dec.read_string().expect("version");
        let path = dec.read_string().expect("service path");
        assert_eq!(
            path, "com.example.Greeter",
            "B-B-002: service path without leading slash"
        );
    }

    /// B-B-003: Service version
    #[test]
    fn bb_003_service_version() {
        let mut url = URL::new("dubbo", "/com.example.Greeter");
        url.set_param("version", "2.0.0");
        let ctx = InvocationContext::new("sayHello", url)
            .with_parameter_types(vec!["Ljava/lang/String;".to_string()])
            .with_arguments(vec![b"world".to_vec()]);
        let bytes = encode_request_body(&ctx).expect("encode body");

        let mut dec = dubbo_rs_serialization_hessian2::decoder::Decoder::new(&bytes);
        let _version = dec.read_string().expect("version");
        let _path = dec.read_string().expect("path");
        let svc_ver = dec.read_string().expect("service version");
        assert_eq!(svc_ver, "2.0.0", "B-B-003: service version");
    }

    /// B-B-004: Method name
    #[test]
    fn bb_004_method_name() {
        let url = URL::new("dubbo", "/com.example.Greeter");
        let ctx = InvocationContext::new("sayHello", url)
            .with_parameter_types(vec!["Ljava/lang/String;".to_string()])
            .with_arguments(vec![b"world".to_vec()]);
        let bytes = encode_request_body(&ctx).expect("encode body");

        let mut dec = dubbo_rs_serialization_hessian2::decoder::Decoder::new(&bytes);
        let _version = dec.read_string().expect("version");
        let _path = dec.read_string().expect("path");
        let _svc_ver = dec.read_string().expect("svc ver");
        let method = dec.read_string().expect("method");
        assert_eq!(method, "sayHello", "B-B-004: method name");
    }

    /// B-B-005: Parameter types descriptor
    #[test]
    fn bb_005_param_types() {
        let url = URL::new("dubbo", "/com.example.Greeter");
        let ctx = InvocationContext::new("sayHello", url)
            .with_parameter_types(vec!["Ljava/lang/String;".to_string(), "I".to_string()])
            .with_arguments(vec![b"hello".to_vec(), vec![0x00, 0x00, 0x00, 0x2a]]);
        let bytes = encode_request_body(&ctx).expect("encode body");

        let mut dec = dubbo_rs_serialization_hessian2::decoder::Decoder::new(&bytes);
        let _version = dec.read_string().expect("version");
        let _path = dec.read_string().expect("path");
        let _svc_ver = dec.read_string().expect("svc ver");
        let _method = dec.read_string().expect("method");
        let descriptor = dec.read_string().expect("param descriptor");
        assert_eq!(
            descriptor, "Ljava/lang/String;I",
            "B-B-005: param descriptor"
        );
    }

    /// B-B-006: Arguments list
    #[test]
    fn bb_006_arguments() {
        let url = URL::new("dubbo", "/com.example.Greeter");
        let ctx = InvocationContext::new("add", url)
            .with_parameter_types(vec!["I".to_string(), "I".to_string()])
            .with_arguments(vec![
                b"\x00\x00\x00\x01".to_vec(), // int 1 as raw binary
                b"\x00\x00\x00\x02".to_vec(), // int 2 as raw binary
            ]);
        let bytes = encode_request_body(&ctx).expect("encode body");

        let mut dec = dubbo_rs_serialization_hessian2::decoder::Decoder::new(&bytes);
        let _version = dec.read_string().expect("version");
        let _path = dec.read_string().expect("path");
        let _svc_ver = dec.read_string().expect("svc ver");
        let _method = dec.read_string().expect("method");
        let _desc = dec.read_string().expect("desc");

        let arg_count = dec.read_list_begin().expect("arg count");
        assert_eq!(arg_count, 2, "B-B-006: arg count");
        let arg1 = dec.read_binary().expect("arg1");
        assert_eq!(arg1, b"\x00\x00\x00\x01");
        let arg2 = dec.read_binary().expect("arg2");
        assert_eq!(arg2, b"\x00\x00\x00\x02");
    }

    /// B-B-007: Attachments map
    #[test]
    fn bb_007_attachments() {
        let url = URL::new("dubbo", "/com.example.Greeter");
        let mut ctx = InvocationContext::new("sayHello", url)
            .with_parameter_types(vec![])
            .with_arguments(vec![]);
        ctx.attachments
            .insert("path".into(), "com.example.Greeter".into());
        ctx.attachments.insert("version".into(), "1.0.0".into());
        let bytes = encode_request_body(&ctx).expect("encode body");

        let mut dec = dubbo_rs_serialization_hessian2::decoder::Decoder::new(&bytes);
        let _version = dec.read_string().expect("version");
        let _path = dec.read_string().expect("path");
        let _svc_ver = dec.read_string().expect("svc ver");
        let _method = dec.read_string().expect("method");
        let _desc = dec.read_string().expect("desc");
        let _argc = dec.read_list_begin().expect("arg count");

        let is_typed = dec.read_map_begin().expect("map");
        assert!(!is_typed, "B-B-007: attachments should be untyped map");
        // Read key-value pairs until 'Z'
        let mut found_path = false;
        let mut found_ver = false;
        while !dec.peek_is_list_end() {
            let k = dec.read_string().expect("key");
            let v = dec.read_string().expect("value");
            match k.as_str() {
                "path" => {
                    assert_eq!(v, "com.example.Greeter");
                    found_path = true;
                }
                "version" => {
                    assert_eq!(v, "1.0.0");
                    found_ver = true;
                }
                _ => {}
            }
        }
        assert!(found_path, "B-B-007: path attachment");
        assert!(found_ver, "B-B-007: version attachment");
    }

    /// Encode → decode full request body roundtrip
    #[test]
    fn bb_008_body_roundtrip() {
        let mut url = URL::new("dubbo", "/com.example.Calc");
        url.set_param("version", "1.0.0");
        let ctx = InvocationContext::new("multiply", url)
            .with_parameter_types(vec!["I".to_string(), "I".to_string()])
            .with_arguments(vec![
                b"\x00\x00\x00\x03".to_vec(),
                b"\x00\x00\x00\x04".to_vec(),
            ]);
        let bytes = encode_request_body(&ctx).expect("encode");

        let base_url = URL::new("dubbo", "/com.example.Calc");
        let decoded = decode_request_body(&bytes, &base_url).expect("decode");

        assert_eq!(decoded.method_name, "multiply");
        assert_eq!(decoded.arguments.len(), 2);
        assert_eq!(decoded.arguments[0], b"\x00\x00\x00\x03");
        assert_eq!(decoded.arguments[1], b"\x00\x00\x00\x04");
    }

    /// Response body encode → decode roundtrip
    #[test]
    fn bb_009_response_roundtrip() {
        use dubbo_rs_protocol::RPCResult;

        let result = RPCResult::success(b"Greeter reply".to_vec());
        let bytes = encode_response_body(&result).expect("encode");
        let decoded = decode_response_body(&bytes).expect("decode");

        assert!(!decoded.is_error());
        assert_eq!(decoded.value, Some(b"Greeter reply".to_vec()));
    }

    /// Response body with exception
    #[test]
    fn bb_010_response_exception() {
        use dubbo_rs_common::error::RPCError;
        use dubbo_rs_protocol::RPCResult;

        let err = RPCError::ServiceError("internal failure".into());
        let result = RPCResult::from_error(err);
        let bytes = encode_response_body(&result).expect("encode");
        let decoded = decode_response_body(&bytes).expect("decode");

        assert!(decoded.is_error(), "B-B-010: should be error");
    }

    /// Verify the header + body are decoded correctly as a full Dubbo frame
    #[test]
    fn bb_011_header_plus_body_roundtrip() {
        let codec = test_codec();
        let mut url = URL::new("dubbo", "/com.example.Echo");
        url.set_param("version", "1.0.0");
        let ctx = InvocationContext::new("echo", url)
            .with_parameter_types(vec!["Ljava/lang/String;".to_string()])
            .with_arguments(vec![b"ping".to_vec()]);
        let body = encode_request_body(&ctx).expect("encode body");

        let req = Request {
            id: 7,
            is_twoway: true,
            is_event: false,
            data: body,
        };
        let frame = codec.encode_request(&req).expect("encode frame");
        let decoded_req = codec.decode_request(&frame).expect("decode frame");

        assert_eq!(decoded_req.id, 7);
        assert!(decoded_req.is_twoway);

        // Decode the body from the frame
        let base_url = URL::new("dubbo", "/com.example.Echo");
        let decoded_ctx =
            decode_request_body(&decoded_req.data, &base_url).expect("decode body from frame");
        assert_eq!(decoded_ctx.method_name, "echo");
        assert_eq!(decoded_ctx.arguments[0], b"ping");
    }
}

// ── B-R: End-to-End Roundtrip Tests ──────────────────────────────────────
//
// These tests start a real Java DubboTcpTestServer process and communicate
// with it via TCP using the Dubbo binary protocol.

mod b_r_roundtrip {

    use dubbo_rs_remoting::{ExchangeClient, ExchangeServer};

    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    /// Wrapper that kills the Java process on drop.
    #[allow(dead_code)]
    struct JavaProc {
        child: Option<Child>,
        port: u16,
    }

    impl JavaProc {
        fn start(port: u16) -> Self {
            let project_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let java_dir = project_root.join("../tests/java-fixtures");

            // Ensure Java is compiled
            let status = Command::new("mvn")
                .args(["-q", "compile"])
                .current_dir(&java_dir)
                .env("JAVA_HOME", "/usr/lib/jvm/java-17-openjdk")
                .status()
                .expect("mvn compile failed");
            assert!(status.success(), "mvn compile should succeed");

            // Get classpath from Maven via temp file
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

            let java_bin = "/usr/lib/jvm/java-17-openjdk/bin/java".to_string();
            eprintln!("[JavaProc] Starting DubboTcpTestServer on port {port}...");
            let child = Command::new(&java_bin)
                .args([
                    "-cp",
                    &classpath,
                    "com.dubborst.fixtures.DubboTcpTestServer",
                    &port.to_string(),
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("failed to start DubboTcpTestServer");

            // Wait for the port to be ready
            let deadline = Instant::now() + Duration::from_secs(25);
            let mut ready = false;
            while Instant::now() < deadline {
                if std::net::TcpStream::connect_timeout(
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
                "Java DubboTcpTestServer did not start on port {port} in time"
            );

            eprintln!("[JavaProc] DubboTcpTestServer ready on port {port}");
            JavaProc {
                child: Some(child),
                port,
            }
        }

        #[allow(dead_code)]
        fn port(&self) -> u16 {
            self.port
        }
    }

    impl Drop for JavaProc {
        fn drop(&mut self) {
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
                eprintln!("[JavaProc] DubboTcpTestServer stopped");
            }
        }
    }

    fn get_available_port() -> u16 {
        cross_lang_tests::PortAllocator::allocate()
    }

    /// B-R-001: RS DubboClient → Java DubboTcpTestServer (sayHello)
    ///
    /// This is the most critical end-to-end cross-language test:
    ///   1. Start Java DubboTcpTestServer
    ///   2. Rust DubboClient connects via TCP
    ///   3. Sends a sayHello(String) request
    ///   4. Receives and verifies the response
    #[tokio::test]
    async fn br_001_rs_client_to_java_server_sayhello() {
        let port = get_available_port();
        let _java = JavaProc::start(port);

        let mut url = dubbo_rs_common::url::URL::new("dubbo", "/com.example.Greeter");
        url.ip = "127.0.0.1".into();
        url.port = port.to_string();

        let mut client = dubbo_rs_protocol_dubbo::transport::DubboClient::new(
            dubbo_rs_protocol_dubbo::codec::SerializationId::Hessian2,
        );
        client.connect(&url).await.expect("connect to Java server");

        // Build a sayHello request with Hessian2-encoded argument
        let mut ctx = dubbo_rs_protocol::InvocationContext::new("sayHello", url)
            .with_parameter_types(vec!["Ljava/lang/String;".to_string()]);
        ctx.arguments = vec![b"World".to_vec()];

        let body = dubbo_rs_protocol_dubbo::body::encode_request_body(&ctx)
            .expect("encode request body");
        let req = dubbo_rs_remoting::Request {
            id: 1001,
            is_twoway: true,
            is_event: false,
            data: body,
        };

        let resp = client.request(req).await.expect("request to Java server");
        assert_eq!(resp.id, 1001, "B-R-001: response ID matches");
        assert_eq!(resp.status, 20, "B-R-001: response status OK");

        // Decode response body
        let result = dubbo_rs_protocol_dubbo::body::decode_response_body(&resp.data)
            .expect("decode response body");
        assert!(!result.is_error(), "B-R-001: response should not be error");

        if let Some(value) = result.value {
            let reply = String::from_utf8_lossy(&value);
            assert!(
                reply.contains("Hello"),
                "B-R-001: reply should contain Hello"
            );
            assert!(
                reply.contains("World"),
                "B-R-001: reply should contain World"
            );
            eprintln!("[B-R-001] Java replied: {reply}");
        }
    }

    /// B-R-002: RS DubboClient → Java DubboTcpTestServer (echo)
    #[tokio::test]
    async fn br_002_rs_client_to_java_server_echo() {
        let port = get_available_port();
        let _java = JavaProc::start(port);

        let mut url = dubbo_rs_common::url::URL::new("dubbo", "/com.example.EchoService");
        url.ip = "127.0.0.1".into();
        url.port = port.to_string();

        let mut client = dubbo_rs_protocol_dubbo::transport::DubboClient::new(
            dubbo_rs_protocol_dubbo::codec::SerializationId::Hessian2,
        );
        client.connect(&url).await.expect("connect");

        let mut ctx = dubbo_rs_protocol::InvocationContext::new("echo", url)
            .with_parameter_types(vec!["Ljava/lang/String;".to_string()]);
        ctx.arguments = vec![b"ping".to_vec()];

        let body = dubbo_rs_protocol_dubbo::body::encode_request_body(&ctx).expect("encode");
        let req = dubbo_rs_remoting::Request {
            id: 2002,
            is_twoway: true,
            is_event: false,
            data: body,
        };

        let resp = client.request(req).await.expect("request");
        let result =
            dubbo_rs_protocol_dubbo::body::decode_response_body(&resp.data).expect("decode");
        assert!(!result.is_error(), "B-R-002: response should not be error");

        if let Some(value) = result.value {
            let reply = String::from_utf8_lossy(&value);
            assert!(
                reply.contains("ping"),
                "B-R-002: echo should contain ping, got '{reply}'"
            );
            eprintln!("[B-R-002] Java echoed: {reply}");
        }
    }

    /// B-R-003: RS DubboClient → Java DubboTcpTestServer (ping, no args)
    #[tokio::test]
    async fn br_003_rs_client_to_java_server_ping() {
        let port = get_available_port();
        let _java = JavaProc::start(port);

        let mut url = dubbo_rs_common::url::URL::new("dubbo", "/com.example.HealthService");
        url.ip = "127.0.0.1".into();
        url.port = port.to_string();

        let mut client = dubbo_rs_protocol_dubbo::transport::DubboClient::new(
            dubbo_rs_protocol_dubbo::codec::SerializationId::Hessian2,
        );
        client.connect(&url).await.expect("connect");

        let ctx = dubbo_rs_protocol::InvocationContext::new("ping", url);
        let body = dubbo_rs_protocol_dubbo::body::encode_request_body(&ctx).expect("encode");
        let req = dubbo_rs_remoting::Request {
            id: 3003,
            is_twoway: true,
            is_event: false,
            data: body,
        };

        let resp = client.request(req).await.expect("request");
        let result =
            dubbo_rs_protocol_dubbo::body::decode_response_body(&resp.data).expect("decode");

        if let Some(value) = result.value {
            let reply = String::from_utf8_lossy(&value);
            assert_eq!(reply, "pong", "B-R-003: ping should return pong");
        }
    }

    // ── Mock Invoker for RS self-loop tests ──────────────────────────────

    struct MockInvoker {
        url: dubbo_rs_common::url::URL,
    }

    impl MockInvoker {
        fn new(url: dubbo_rs_common::url::URL) -> Self {
            Self { url }
        }
    }

    impl dubbo_rs_common::node::Node for MockInvoker {
        fn get_url(&self) -> &dubbo_rs_common::url::URL {
            &self.url
        }

        fn is_available(&self) -> bool {
            true
        }

        fn destroy(&self) {}
    }

    #[tonic::async_trait]
    impl dubbo_rs_protocol::Invoker for MockInvoker {
        async fn invoke(
            &self,
            ctx: &mut dubbo_rs_protocol::InvocationContext,
        ) -> anyhow::Result<dubbo_rs_protocol::RPCResult> {
            let reply = match ctx.method_name.as_str() {
                "sayHello" => {
                    // Return Hessian2-encoded string
                    let mut enc = dubbo_rs_serialization_hessian2::codec::encoder::Encoder::new();
                    enc.write_string("Hello from dubbo-rs!");
                    enc.into_bytes()
                }
                "echo" => {
                    // Echo back the first argument
                    if let Some(arg) = ctx.arguments.first() {
                        arg.clone()
                    } else {
                        vec![]
                    }
                }
                "ping" => {
                    let mut enc = dubbo_rs_serialization_hessian2::codec::encoder::Encoder::new();
                    enc.write_string("pong");
                    enc.into_bytes()
                }
                _ => {
                    let mut enc = dubbo_rs_serialization_hessian2::codec::encoder::Encoder::new();
                    enc.write_string("unknown method");
                    enc.into_bytes()
                }
            };
            Ok(dubbo_rs_protocol::RPCResult::success(reply))
        }
    }

    /// B-R-004: RS DubboClient → RS DubboServer (self-loop via DubboServer + DubboClient)
    ///
    /// Starts an RS DubboServer with a mock Invoker, connects a DubboClient
    /// to it, sends a sayHello request, and verifies the response.
    #[tokio::test]
    async fn br_004_rs_client_to_rs_server_sayhello() {
        let (held_port, port) = cross_lang_tests::PortAllocator::allocate_held();

        let mut server_url = dubbo_rs_common::url::URL::new("dubbo", "/com.example.Greeter");
        server_url.ip = "127.0.0.1".into();
        server_url.port = port.to_string();

        let invoker = std::sync::Arc::new(MockInvoker::new(server_url.clone()));
        let server = dubbo_rs_protocol_dubbo::transport::DubboServer::new(
            dubbo_rs_protocol_dubbo::codec::SerializationId::Hessian2,
        )
        .with_invoker(invoker);
        drop(held_port);
        server.bind(&server_url).await.expect("server bind");
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Connect client
        let mut client_url = dubbo_rs_common::url::URL::new("dubbo", "/com.example.Greeter");
        client_url.ip = "127.0.0.1".into();
        client_url.port = port.to_string();

        let mut client = dubbo_rs_protocol_dubbo::transport::DubboClient::new(
            dubbo_rs_protocol_dubbo::codec::SerializationId::Hessian2,
        );
        client
            .connect(&client_url)
            .await
            .expect("connect to RS server");

        // Build sayHello request
        let mut ctx = dubbo_rs_protocol::InvocationContext::new("sayHello", client_url)
            .with_parameter_types(vec!["Ljava/lang/String;".to_string()]);
        let mut enc = dubbo_rs_serialization_hessian2::codec::encoder::Encoder::new();
        enc.write_string("World");
        ctx.arguments = vec![enc.into_bytes()];

        let body = dubbo_rs_protocol_dubbo::body::encode_request_body(&ctx)
            .expect("encode request body");
        let req = dubbo_rs_remoting::Request {
            id: 4004,
            is_twoway: true,
            is_event: false,
            data: body,
        };

        let resp = client.request(req).await.expect("request to RS server");
        assert_eq!(resp.id, 4004, "B-R-004: response ID matches");
        assert_eq!(resp.status, 20, "B-R-004: response status OK");

        let result = dubbo_rs_protocol_dubbo::body::decode_response_body(&resp.data)
            .expect("decode response body");
        assert!(!result.is_error(), "B-R-004: response should not be error");

        if let Some(value) = result.value {
            let reply = String::from_utf8_lossy(&value);
            assert!(
                reply.contains("Hello from dubbo-rs!"),
                "B-R-004: reply should contain 'Hello from dubbo-rs!', got '{reply}'"
            );
            eprintln!("[B-R-004] RS server replied: {reply}");
        }

        server.close().await;
    }

    /// B-R-005: Multiple consecutive calls to RS DubboServer verifying stateless behavior
    ///
    /// Sends 5 consecutive requests to the RS DubboServer and verifies all succeed.
    #[tokio::test]
    async fn br_005_rs_client_to_rs_server_multiple_calls() {
        let (held_port, port) = cross_lang_tests::PortAllocator::allocate_held();

        let mut server_url = dubbo_rs_common::url::URL::new("dubbo", "/com.example.Greeter");
        server_url.ip = "127.0.0.1".into();
        server_url.port = port.to_string();

        let invoker = std::sync::Arc::new(MockInvoker::new(server_url.clone()));
        let server = dubbo_rs_protocol_dubbo::transport::DubboServer::new(
            dubbo_rs_protocol_dubbo::codec::SerializationId::Hessian2,
        )
        .with_invoker(invoker);
        drop(held_port);
        server.bind(&server_url).await.expect("server bind");
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Connect client
        let mut client_url = dubbo_rs_common::url::URL::new("dubbo", "/com.example.Greeter");
        client_url.ip = "127.0.0.1".into();
        client_url.port = port.to_string();

        let mut client = dubbo_rs_protocol_dubbo::transport::DubboClient::new(
            dubbo_rs_protocol_dubbo::codec::SerializationId::Hessian2,
        );
        client
            .connect(&client_url)
            .await
            .expect("connect to RS server");

        // Send 5 consecutive requests
        for i in 0..5u64 {
            let mut ctx = dubbo_rs_protocol::InvocationContext::new("sayHello", client_url.clone())
                .with_parameter_types(vec!["Ljava/lang/String;".to_string()]);
            let mut enc = dubbo_rs_serialization_hessian2::codec::encoder::Encoder::new();
            enc.write_string(&format!("call_{i}"));
            ctx.arguments = vec![enc.into_bytes()];

            let body = dubbo_rs_protocol_dubbo::body::encode_request_body(&ctx)
                .expect("encode request body");
            let req = dubbo_rs_remoting::Request {
                id: 5000 + i,
                is_twoway: true,
                is_event: false,
                data: body,
            };

            let resp = client.request(req).await.expect("request to RS server");
            assert_eq!(
                resp.id,
                5000 + i,
                "B-R-005: response ID matches for call {i}"
            );
            assert_eq!(resp.status, 20, "B-R-005: response status OK for call {i}");

            let result = dubbo_rs_protocol_dubbo::body::decode_response_body(&resp.data)
                .expect("decode response body");
            assert!(
                !result.is_error(),
                "B-R-005: response should not be error for call {i}"
            );

            if let Some(ref value) = result.value {
                let reply = String::from_utf8_lossy(value);
                assert!(
                    reply.contains("Hello from dubbo-rs!"),
                    "B-R-005: call {i} reply should contain 'Hello from dubbo-rs!', got '{reply}'"
                );
            }
            eprintln!("[B-R-005] call {i}: OK");
        }

        server.close().await;
    }

    /// B-R-006: RS DubboClient → Java DubboTcpTestServer (add, multi-param)
    #[tokio::test]
    async fn br_006_rs_client_to_java_server_add() {
        let port = get_available_port();
        let _java = JavaProc::start(port);

        let mut url = dubbo_rs_common::url::URL::new("dubbo", "/com.example.CalcService");
        url.ip = "127.0.0.1".into();
        url.port = port.to_string();

        let mut client = dubbo_rs_protocol_dubbo::transport::DubboClient::new(
            dubbo_rs_protocol_dubbo::codec::SerializationId::Hessian2,
        );
        client.connect(&url).await.expect("connect");

        let mut ctx =
            dubbo_rs_protocol::InvocationContext::new("add", url).with_parameter_types(vec![
                "Ljava/lang/String;".to_string(),
                "Ljava/lang/String;".to_string(),
            ]);
        ctx.arguments = vec![b"10".to_vec(), b"32".to_vec()];

        let body = dubbo_rs_protocol_dubbo::body::encode_request_body(&ctx).expect("encode");
        let req = dubbo_rs_remoting::Request {
            id: 6006,
            is_twoway: true,
            is_event: false,
            data: body,
        };

        let resp = client.request(req).await.expect("request");
        let result =
            dubbo_rs_protocol_dubbo::body::decode_response_body(&resp.data).expect("decode");
        assert!(!result.is_error(), "B-R-006: response should not be error");

        if let Some(value) = result.value {
            let reply = String::from_utf8_lossy(&value);
            assert!(
                reply.contains("42"),
                "B-R-006: add(10,32) should return 42, got '{reply}'"
            );
            eprintln!("[B-R-006] Java replied: {reply}");
        }
    }

    /// B-R-007: RS DubboClient → Java DubboTcpTestServer (echoAttachments)
    #[tokio::test]
    async fn br_007_rs_client_to_java_server_echo_attachments() {
        let port = get_available_port();
        let _java = JavaProc::start(port);

        let mut url = dubbo_rs_common::url::URL::new("dubbo", "/com.example.AttachService");
        url.ip = "127.0.0.1".into();
        url.port = port.to_string();

        let mut client = dubbo_rs_protocol_dubbo::transport::DubboClient::new(
            dubbo_rs_protocol_dubbo::codec::SerializationId::Hessian2,
        );
        client.connect(&url).await.expect("connect");

        let mut ctx = dubbo_rs_protocol::InvocationContext::new("echoAttachments", url);
        ctx.attachments.insert("trace_id".into(), "abc123".into());
        ctx.attachments.insert("source".into(), "dubbo-rs".into());

        let body = dubbo_rs_protocol_dubbo::body::encode_request_body(&ctx).expect("encode");
        let req = dubbo_rs_remoting::Request {
            id: 7007,
            is_twoway: true,
            is_event: false,
            data: body,
        };

        let resp = client.request(req).await.expect("request");
        let result =
            dubbo_rs_protocol_dubbo::body::decode_response_body(&resp.data).expect("decode");
        assert!(!result.is_error(), "B-R-007: response should not be error");

        if let Some(value) = result.value {
            let reply = String::from_utf8_lossy(&value);
            assert!(
                reply.contains("trace_id=abc123"),
                "B-R-007: should contain trace_id=abc123, got '{reply}'"
            );
            assert!(
                reply.contains("source=dubbo-rs"),
                "B-R-007: should contain source=dubbo-rs, got '{reply}'"
            );
            eprintln!("[B-R-007] Java replied: {reply}");
        }
    }
}
