#![allow(
    clippy::needless_pass_by_value,
    clippy::borrowed_box,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::approx_constant
)]
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use dubbo_rs::common::url::URL;
use dubbo_rs::loadbalance::{
    ConsistentHashLoadBalance, LeastActiveLoadBalance, LoadBalance, P2CLoadBalance,
    RandomLoadBalance, RoundRobinLoadBalance, ShortestResponseLoadBalance,
};
use tests_perf::{ctx_with_args, dummy_ctx, make_invokers};

fn invoker_key(invoker: &Box<dyn dubbo_rs::protocol::Invoker>) -> String {
    let url = invoker.get_url();
    format!("{}:{}", url.ip, url.port)
}

fn bench_lb<L: LoadBalance>(c: &mut Criterion, name: &str, lb: L, counts: &[usize]) {
    let mut group = c.benchmark_group(format!("loadbalance/{name}"));
    for &count in counts {
        let invokers = make_invokers(count);
        let url = URL::new("tri", "/com.example.BenchmarkService");
        let ctx = dummy_ctx();
        group.bench_with_input(BenchmarkId::new("select", count), &count, |b, _| {
            b.iter(|| {
                let _ = lb.select(
                    criterion::black_box(&invokers),
                    criterion::black_box(&url),
                    criterion::black_box(&ctx),
                );
            });
        });
    }
    group.finish();
}

fn bench_random(c: &mut Criterion) {
    bench_lb(c, "random", RandomLoadBalance, &[10, 50, 100, 256]);
}
fn bench_round_robin(c: &mut Criterion) {
    bench_lb(
        c,
        "round_robin",
        RoundRobinLoadBalance::new(),
        &[10, 50, 100, 256],
    );
}
fn bench_least_active(c: &mut Criterion) {
    bench_lb(
        c,
        "least_active",
        LeastActiveLoadBalance,
        &[10, 50, 100, 256],
    );
}
fn bench_shortest_response(c: &mut Criterion) {
    bench_lb(
        c,
        "shortest_response",
        ShortestResponseLoadBalance,
        &[10, 50, 100, 256],
    );
}

fn bench_consistent_hash(c: &mut Criterion) {
    let mut group = c.benchmark_group("loadbalance/consistent_hash");
    for count in [10, 50, 100, 256] {
        let invokers = make_invokers(count);
        let url = URL::new("tri", "/com.example.BenchmarkService");
        let ctx = ctx_with_args(b"hash-key-benchmark");
        let lb = ConsistentHashLoadBalance::new();
        group.bench_with_input(BenchmarkId::new("select", count), &count, |b, _| {
            b.iter(|| {
                let _ = lb.select(
                    criterion::black_box(&invokers),
                    criterion::black_box(&url),
                    criterion::black_box(&ctx),
                );
            });
        });
    }
    group.finish();
}

fn bench_p2c(c: &mut Criterion) {
    let mut group = c.benchmark_group("loadbalance/p2c");
    for count in [10, 50, 100, 256] {
        let invokers = make_invokers(count);
        let url = URL::new("tri", "/com.example.BenchmarkService");
        let ctx = dummy_ctx();
        let lb = P2CLoadBalance::new();
        group.bench_with_input(BenchmarkId::new("select", count), &count, |b, _| {
            b.iter(|| {
                let idx = lb
                    .select(
                        criterion::black_box(&invokers),
                        criterion::black_box(&url),
                        criterion::black_box(&ctx),
                    )
                    .unwrap();
                let key = invoker_key(&invokers[idx]);
                lb.record_result(&key, 10, true);
            });
        });
    }
    group.finish();
}

fn bench_all_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("loadbalance/comparison/256_invokers");
    let invokers = make_invokers(256);
    let url = URL::new("tri", "/com.example.BenchmarkService");
    let ctx = dummy_ctx();
    let ctx_hash = ctx_with_args(b"hash-key");

    group.bench_function("random", |b| {
        let lb = RandomLoadBalance;
        b.iter(|| {
            let _ = lb.select(
                criterion::black_box(&invokers),
                criterion::black_box(&url),
                criterion::black_box(&ctx),
            );
        });
    });
    group.bench_function("round_robin", |b| {
        let lb = RoundRobinLoadBalance::new();
        b.iter(|| {
            let _ = lb.select(
                criterion::black_box(&invokers),
                criterion::black_box(&url),
                criterion::black_box(&ctx),
            );
        });
    });
    group.bench_function("least_active", |b| {
        let lb = LeastActiveLoadBalance;
        b.iter(|| {
            let _ = lb.select(
                criterion::black_box(&invokers),
                criterion::black_box(&url),
                criterion::black_box(&ctx),
            );
        });
    });
    group.bench_function("consistent_hash", |b| {
        let lb = ConsistentHashLoadBalance::new();
        b.iter(|| {
            let _ = lb.select(
                criterion::black_box(&invokers),
                criterion::black_box(&url),
                criterion::black_box(&ctx_hash),
            );
        });
    });
    group.bench_function("p2c", |b| {
        let lb = P2CLoadBalance::new();
        b.iter(|| {
            let idx = lb
                .select(
                    criterion::black_box(&invokers),
                    criterion::black_box(&url),
                    criterion::black_box(&ctx),
                )
                .unwrap();
            let key = invoker_key(&invokers[idx]);
            lb.record_result(&key, 10, true);
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_random,
    bench_round_robin,
    bench_least_active,
    bench_shortest_response,
    bench_consistent_hash,
    bench_p2c,
    bench_all_comparison
);
criterion_main!(benches);
