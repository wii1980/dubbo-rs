#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
mod bench_proto {
    include!(concat!(env!("OUT_DIR"), "/bench.rs"));
}

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use dubbo_rs::serialization_hessian2::codec::encoder::Encoder;
use dubbo_rs::serialization_hessian2::decoder::Decoder;
use prost::Message;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchData {
    name: String,
    id: i32,
    score: f64,
    active: bool,
    tags: Vec<String>,
    metadata: std::collections::HashMap<String, String>,
}

impl BenchData {
    fn small() -> Self {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("version".into(), "1.0".into());
        Self {
            name: "bench-small".into(),
            id: 42,
            score: 3.15,
            active: true,
            tags: vec!["test".into()],
            metadata,
        }
    }

    fn medium() -> Self {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("version".into(), "2.0.1".into());
        metadata.insert("region".into(), "us-west-2".into());
        metadata.insert("env".into(), "production".into());
        Self {
            name: "bench-medium-payload-for-serialization-comparison".into(),
            id: 100_000,
            score: std::f64::consts::PI,
            active: false,
            tags: vec![
                "api".into(), "v2".into(), "stable".into(), "production".into(),
                "us-west".into(), "high-priority".into(), "bench".into(), "perf".into(),
            ],
            metadata,
        }
    }

    fn large() -> Self {
        let mut metadata = std::collections::HashMap::new();
        for i in 0..50 {
            metadata.insert(format!("key_{i}"), format!("value_{i}_with_some_content"));
        }
        Self {
            name: "x".repeat(1024),
            id: i32::MAX,
            score: std::f64::consts::E,
            active: true,
            tags: (0..100).map(|i| format!("tag_{i}")).collect(),
            metadata,
        }
    }

    fn label(&self) -> &'static str {
        if self.tags.len() <= 1 && self.metadata.len() <= 1 {
            "small"
        } else if self.tags.len() <= 10 {
            "medium"
        } else {
            "large"
        }
    }
}

fn hessian2_encode(data: &BenchData) -> Vec<u8> {
    let mut enc = Encoder::new();
    enc.write_string(&data.name);
    enc.write_int(data.id);
    enc.write_double(data.score);
    enc.write_bool(data.active);
    enc.write_list_begin(data.tags.len());
    for tag in &data.tags {
        enc.write_string(tag);
    }
    enc.write_map_begin();
    for (k, v) in &data.metadata {
        enc.write_string(k);
        enc.write_string(v);
    }
    enc.write_map_end();
    enc.into_bytes()
}

fn hessian2_decode(bytes: &[u8]) {
    let mut dec = Decoder::new(bytes);
    let name = dec.read_string().unwrap();
    let _id = dec.read_int().unwrap();
    let _score = dec.read_double().unwrap();
    let _active = dec.read_bool().unwrap();
    let list_len = dec.read_list_begin().unwrap();
    for _ in 0..list_len {
        let _ = dec.read_string().unwrap();
    }
    let _typed = dec.read_map_begin().unwrap();
    while !dec.peek_is_list_end() {
        let _k = dec.read_string().unwrap();
        let _v = dec.read_string().unwrap();
    }
    dec.read_list_end().unwrap();
    let _ = name;
}

fn hessian2_decode_str(bytes: &[u8]) {
    let mut dec = Decoder::new(bytes);
    let name = dec.read_str().unwrap();
    let _id = dec.read_int().unwrap();
    let _score = dec.read_double().unwrap();
    let _active = dec.read_bool().unwrap();
    let list_len = dec.read_list_begin().unwrap();
    for _ in 0..list_len {
        let _ = dec.read_str().unwrap();
    }
    let _typed = dec.read_map_begin().unwrap();
    while !dec.peek_is_list_end() {
        let _k = dec.read_str().unwrap();
        let _v = dec.read_str().unwrap();
    }
    dec.read_list_end().unwrap();
    let _ = name;
}

fn protobuf_encode(data: &BenchData) -> Vec<u8> {
    let msg = bench_proto::BenchMessage {
        name: data.name.clone(),
        id: data.id,
        score: data.score,
        active: data.active,
        tags: data.tags.clone(),
        metadata: data.metadata.clone(),
    };
    msg.encode_to_vec()
}

fn protobuf_decode(bytes: &[u8]) -> bench_proto::BenchMessage {
    bench_proto::BenchMessage::decode(bytes).unwrap()
}

fn bench_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison/encode");
    let datasets = [BenchData::small(), BenchData::medium(), BenchData::large()];

    for data in &datasets {
        let label = data.label();

        group.bench_with_input(BenchmarkId::new("hessian2", label), data, |b, data| {
            b.iter(|| hessian2_encode(data));
        });

        group.bench_with_input(BenchmarkId::new("json", label), data, |b, data| {
            b.iter(|| serde_json::to_vec(criterion::black_box(data)).unwrap());
        });

        group.bench_with_input(BenchmarkId::new("protobuf", label), data, |b, data| {
            b.iter(|| protobuf_encode(data));
        });
    }
    group.finish();
}

fn bench_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison/decode");
    let datasets = [BenchData::small(), BenchData::medium(), BenchData::large()];

    for data in &datasets {
        let label = data.label();
        let hessian2_bytes = hessian2_encode(data);
        let json_bytes = serde_json::to_vec(data).unwrap();
        let protobuf_bytes = protobuf_encode(data);

        group.bench_with_input(BenchmarkId::new("hessian2_read_string", label), &hessian2_bytes, |b, bytes| {
            b.iter(|| hessian2_decode(bytes));
        });

        group.bench_with_input(BenchmarkId::new("hessian2_read_str", label), &hessian2_bytes, |b, bytes| {
            b.iter(|| hessian2_decode_str(bytes));
        });

        group.bench_with_input(BenchmarkId::new("json", label), &json_bytes, |b, bytes| {
            b.iter(|| serde_json::from_slice::<BenchData>(criterion::black_box(bytes)).unwrap());
        });

        group.bench_with_input(BenchmarkId::new("protobuf", label), &protobuf_bytes, |b, bytes| {
            b.iter(|| protobuf_decode(bytes));
        });
    }
    group.finish();
}

fn bench_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison/roundtrip");
    let datasets = [BenchData::small(), BenchData::medium(), BenchData::large()];

    for data in &datasets {
        let label = data.label();

        group.bench_with_input(BenchmarkId::new("hessian2_read_string", label), data, |b, data| {
            b.iter(|| { let bytes = hessian2_encode(data); hessian2_decode(&bytes); bytes.len() });
        });

        group.bench_with_input(BenchmarkId::new("hessian2_read_str", label), data, |b, data| {
            b.iter(|| { let bytes = hessian2_encode(data); hessian2_decode_str(&bytes); bytes.len() });
        });

        group.bench_with_input(BenchmarkId::new("json", label), data, |b, data| {
            b.iter(|| {
                let bytes = serde_json::to_vec(data).unwrap();
                let _: BenchData = serde_json::from_slice(&bytes).unwrap();
                bytes.len()
            });
        });

        group.bench_with_input(BenchmarkId::new("protobuf", label), data, |b, data| {
            b.iter(|| {
                let bytes = protobuf_encode(data);
                let _ = protobuf_decode(&bytes);
                bytes.len()
            });
        });
    }
    group.finish();
}

fn bench_payload_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison/payload_size");
    let datasets = [BenchData::small(), BenchData::medium(), BenchData::large()];

    for data in &datasets {
        let label = data.label();
        let h = hessian2_encode(data).len();
        let j = serde_json::to_vec(data).unwrap().len();
        let p = protobuf_encode(data).len();
        group.bench_function(BenchmarkId::new("hessian2", label), |b| b.iter(|| std::hint::black_box(h)));
        group.bench_function(BenchmarkId::new("json", label), |b| b.iter(|| std::hint::black_box(j)));
        group.bench_function(BenchmarkId::new("protobuf", label), |b| b.iter(|| std::hint::black_box(p)));
    }
    group.finish();

    let datasets = [BenchData::small(), BenchData::medium(), BenchData::large()];
    for data in &datasets {
        let label = data.label();
        let h = hessian2_encode(data).len();
        let j = serde_json::to_vec(data).unwrap().len();
        let p = protobuf_encode(data).len();
        println!("{label:>8} | hessian2: {h:>6}B | json: {j:>6}B | protobuf: {p:>6}B");
    }
}

criterion_group!(
    benches,
    bench_encode,
    bench_decode,
    bench_roundtrip,
    bench_payload_size,
);
criterion_main!(benches);
