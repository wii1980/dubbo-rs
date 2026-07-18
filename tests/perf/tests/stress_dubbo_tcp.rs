#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::approx_constant
)]
use std::time::Duration;
use tests_perf::stats::StressCollector;

#[tokio::test]
async fn stress_dubbo_body_encode_concurrent() {
    let concurrency = 100;
    let ops_per_task = 1_000;
    let total = concurrency * ops_per_task;

    let start = std::time::Instant::now();
    let mut collector = StressCollector::new(total);

    let mut handles = Vec::with_capacity(concurrency);
    for task in 0..concurrency {
        handles.push(tokio::spawn(async move {
            use dubbo_rs::common::url::URL;
            use dubbo_rs::protocol::InvocationContext;
            use dubbo_rs::serialization_hessian2::codec::encoder::Encoder;

            let mut latencies = Vec::with_capacity(ops_per_task);
            let mut success = 0u64;

            for i in 0..ops_per_task {
                let t0 = std::time::Instant::now();
                let mut url = URL::new("dubbo", "/com.example.UserService");
                url.ip = "127.0.0.1".into();
                url.port = "20880".into();
                let ctx = InvocationContext::new("getUser", url)
                    .with_parameter_types(vec!["Ljava/lang/String;".to_string()])
                    .with_arguments(vec![format!("user-{task}-{i}").into_bytes()])
                    .with_attachment("dubbo_version", "2.0.2")
                    .with_attachment("timeout", "3000");

                let mut enc = Encoder::new();
                enc.write_string(ctx.attachments.get("dubbo_version").map_or("2.0.2", |v| v));
                enc.write_string(&ctx.url.path);
                enc.write_string(ctx.attachments.get("version").map_or("0.0.0", |v| v));
                enc.write_string(&ctx.method_name);
                enc.write_string(
                    &ctx.parameter_types
                        .iter()
                        .map(std::string::String::as_str)
                        .collect::<Vec<_>>()
                        .join(""),
                );
                enc.write_list_begin(ctx.arguments.len());
                for arg in &ctx.arguments {
                    enc.write_binary(arg);
                }
                enc.write_map_begin();
                for (k, v) in &ctx.attachments {
                    enc.write_string(k);
                    enc.write_string(v);
                }
                enc.write_map_end();

                let _body = enc.into_bytes();
                latencies.push(t0.elapsed().as_micros() as u64);
                success += 1;
            }
            (success, latencies)
        }));
    }

    for handle in handles {
        let (success, latencies) = handle.await.unwrap();
        for _ in 0..success {
            collector.record_success(Duration::from_micros(0));
        }
        collector.latencies_us.extend(latencies);
    }

    let elapsed = start.elapsed();
    let report = collector.finalize(elapsed);
    report.print_report("Dubbo TCP Body Encode (concurrent)");
}

#[tokio::test]
async fn stress_dubbo_header_codec() {
    let concurrency = 50;
    let ops_per_task = 10_000;
    let total = concurrency * ops_per_task;

    let start = std::time::Instant::now();
    let mut collector = StressCollector::new(total);

    let mut handles = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        handles.push(tokio::spawn(async move {
            use dubbo_rs::dubbo2::codec::{DubboCodec, SerializationId};
            use dubbo_rs::remoting::Codec;

            let codec = DubboCodec::new(SerializationId::Hessian2);
            let mut latencies = Vec::with_capacity(ops_per_task);
            let mut success = 0u64;

            for i in 0..ops_per_task {
                let t0 = std::time::Instant::now();
                let req = dubbo_rs::remoting::Request {
                    id: i as u64 + 1,
                    is_twoway: true,
                    is_event: false,
                    data: vec![0u8; 64],
                };
                let encoded = codec.encode_request(&req).unwrap();
                let decoded = codec.decode_request(&encoded).unwrap();
                latencies.push(t0.elapsed().as_micros() as u64);
                success += 1;
                let _ = decoded;
            }
            (success, latencies)
        }));
    }

    for handle in handles {
        let (success, latencies) = handle.await.unwrap();
        for _ in 0..success {
            collector.record_success(Duration::from_micros(0));
        }
        collector.latencies_us.extend(latencies);
    }

    let elapsed = start.elapsed();
    let report = collector.finalize(elapsed);
    report.print_report("Dubbo TCP Header+Body Codec (concurrent)");
}

#[tokio::test]
async fn stress_dubbo_body_roundtrip_concurrent() {
    let concurrency = 50;
    let ops_per_task = 1_000;
    let total = concurrency * ops_per_task;

    let start = std::time::Instant::now();
    let mut collector = StressCollector::new(total);

    let mut handles = Vec::with_capacity(concurrency);
    for task in 0..concurrency {
        handles.push(tokio::spawn(async move {
            use dubbo_rs::serialization_hessian2::codec::encoder::Encoder;
            use dubbo_rs::serialization_hessian2::decoder::Decoder;

            let mut latencies = Vec::with_capacity(ops_per_task);
            let mut success = 0u64;

            for i in 0..ops_per_task {
                let t0 = std::time::Instant::now();
                let mut enc = Encoder::new();
                enc.write_string("2.0.2");
                enc.write_string("/com.example.UserService");
                enc.write_string("1.0.0");
                enc.write_string("getUser");
                enc.write_string("Ljava/lang/String;");
                enc.write_list_begin(1);
                enc.write_string(&format!("user-{task}-{i}"));
                enc.write_map_begin();
                enc.write_string("trace_id");
                enc.write_string(&format!("trace-{task}-{i}"));
                enc.write_string("source");
                enc.write_string("stress-test");
                enc.write_map_end();

                let encoded = enc.into_bytes();
                let mut dec = Decoder::new(&encoded);
                let _: String = dec.read_string().unwrap();
                let _: String = dec.read_string().unwrap();
                let _: String = dec.read_string().unwrap();
                let _: String = dec.read_string().unwrap();
                let _: String = dec.read_string().unwrap();

                latencies.push(t0.elapsed().as_micros() as u64);
                success += 1;
            }
            (success, latencies)
        }));
    }

    for handle in handles {
        let (success, latencies) = handle.await.unwrap();
        for _ in 0..success {
            collector.record_success(Duration::from_micros(0));
        }
        collector.latencies_us.extend(latencies);
    }

    let elapsed = start.elapsed();
    let report = collector.finalize(elapsed);
    report.print_report("Dubbo TCP Full Body Roundtrip (concurrent)");
}
