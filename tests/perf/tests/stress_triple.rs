#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss, clippy::mut_from_ref, clippy::unnecessary_mut_passed, clippy::float_cmp, clippy::approx_constant)]
use std::sync::Arc;
use std::time::Duration;
use tests_perf::stats::StressCollector;

#[tokio::test]
async fn stress_hessian2_concurrent_roundtrip() {
    let concurrency = 100;
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
                enc.write_string(&format!("task-{task}-req-{i}"));
                enc.write_int(i as i32);
                enc.write_double(3.14159);
                let encoded = enc.into_bytes();

                let mut dec = Decoder::new(&encoded);
                let _: String = dec.read_string().unwrap();
                let _: i32 = dec.read_int().unwrap();
                let _: f64 = dec.read_double().unwrap();

                latencies.push(t0.elapsed().as_micros() as u64);
                success += 1;
            }
            (success, latencies)
        }));
    }

    for handle in handles {
        let (success, latencies) = handle.await.unwrap();
        for _ in 0..success { collector.record_success(Duration::from_micros(0)); }
        collector.latencies_us.extend(latencies);
    }

    let elapsed = start.elapsed();
    let report = collector.finalize(elapsed);
    report.print_report("Hessian2 Concurrent Roundtrip");
}

#[tokio::test]
async fn stress_concurrent_invoke() {
    use async_trait::async_trait;
    use dubbo_rs::common::node::Node;
    use dubbo_rs::common::url::URL;
    use dubbo_rs::protocol::{InvocationContext, Invoker, RPCResult};

    struct EchoInvoker;
    impl Node for EchoInvoker {
        fn get_url(&self) -> &URL {
            static URL_VAL: std::sync::OnceLock<URL> = std::sync::OnceLock::new();
            URL_VAL.get_or_init(|| { let mut u = URL::new("tri", "/com.example.EchoService"); u.ip = "127.0.0.1".into(); u.port = "50051".into(); u })
        }
        fn is_available(&self) -> bool { true }
        fn destroy(&self) {}
    }
    #[async_trait]
    impl Invoker for EchoInvoker {
        async fn invoke(&self, ctx: &mut InvocationContext) -> anyhow::Result<RPCResult> {
            Ok(RPCResult::success(ctx.arguments.first().cloned().unwrap_or_default()))
        }
    }

    let invoker: Arc<Box<dyn Invoker>> = Arc::new(Box::new(EchoInvoker));
    let concurrency = 100;
    let requests_per_task = 1_000;
    let total = concurrency * requests_per_task;

    let start = std::time::Instant::now();
    let mut collector = StressCollector::new(total);

    let mut handles = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let invoker = Arc::clone(&invoker);
        handles.push(tokio::spawn(async move {
            let mut latencies = Vec::with_capacity(requests_per_task);
            let mut success = 0u64;
            let mut failed = 0u64;

            for i in 0..requests_per_task {
                let mut url = URL::new("tri", "/com.example.EchoService");
                url.ip = "127.0.0.1".into();
                let mut ctx = InvocationContext::new("echo", url)
                    .with_arguments(vec![format!("payload-{i}").into_bytes()]);

                let t0 = std::time::Instant::now();
                match invoker.invoke(&mut ctx).await {
                    Ok(_) => { latencies.push(t0.elapsed().as_micros() as u64); success += 1; }
                    Err(_) => { failed += 1; }
                }
            }
            (success, failed, latencies)
        }));
    }

    for handle in handles {
        let (success, failed, latencies) = handle.await.unwrap();
        for _ in 0..success { collector.record_success(Duration::from_micros(0)); }
        collector.latencies_us.extend(latencies);
        for _ in 0..failed { collector.record_failure(); }
    }

    let elapsed = start.elapsed();
    let report = collector.finalize(elapsed);
    report.print_report("Concurrent Invoke (mock, no network)");
}

#[tokio::test]
async fn stress_loadbalance_concurrent_select() {
    use std::sync::Arc;
    use dubbo_rs::common::url::URL;
    use dubbo_rs::loadbalance::{LoadBalance, RandomLoadBalance};
    use dubbo_rs::protocol::InvocationContext;
    use tests_perf::make_invokers;

    let invokers = Arc::new(make_invokers(256));
    let concurrency = 50;
    let selects_per_task = 10_000;
    let total = concurrency * selects_per_task;

    let start = std::time::Instant::now();
    let mut collector = StressCollector::new(total);

    let mut handles = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let invokers = Arc::clone(&invokers);
        handles.push(tokio::spawn(async move {
            let mut latencies = Vec::with_capacity(selects_per_task);
            let mut success = 0u64;
            let lb = RandomLoadBalance;
            let url = URL::new("tri", "/com.example.BenchmarkService");
            let mut ctx = InvocationContext::new("sayHello", url.clone());

            for _ in 0..selects_per_task {
                let t0 = std::time::Instant::now();
                let _ = lb.select(&invokers, &url, &mut ctx);
                latencies.push(t0.elapsed().as_micros() as u64);
                success += 1;
            }
            (success, latencies)
        }));
    }

    for handle in handles {
        let (success, latencies) = handle.await.unwrap();
        for _ in 0..success { collector.record_success(Duration::from_micros(0)); }
        collector.latencies_us.extend(latencies);
    }

    let elapsed = start.elapsed();
    let report = collector.finalize(elapsed);
    report.print_report("LoadBalance Random (concurrent, 256 invokers)");
}
