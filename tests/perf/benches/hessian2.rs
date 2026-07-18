#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::float_cmp, clippy::approx_constant)]
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use dubbo_rs::serialization_hessian2::codec::encoder::Encoder;
use dubbo_rs::serialization_hessian2::decoder::Decoder;

fn bench_encode_int(c: &mut Criterion) {
    let mut group = c.benchmark_group("hessian2/encode/int");
    for (label, val) in [("compact_0", 0i32), ("compact_47", 47), ("medium_2047", 2047), ("large_1m", 1_000_000), ("large_min", i32::MIN)] {
        group.bench_with_input(BenchmarkId::new(label, val), &val, |b, &v| {
            b.iter(|| { let mut enc = Encoder::new(); enc.write_int(black_box(v)); enc.into_bytes() });
        });
    }
    group.finish();
}

fn bench_encode_long(c: &mut Criterion) {
    let mut group = c.benchmark_group("hessian2/encode/long");
    for (label, val) in [("compact_0", 0i64), ("medium_2047", 2047i64), ("large_1b", 1_000_000_000i64), ("full_max", i64::MAX)] {
        group.bench_with_input(BenchmarkId::new(label, val), &val, |b, &v| {
            b.iter(|| { let mut enc = Encoder::new(); enc.write_long(black_box(v)); enc.into_bytes() });
        });
    }
    group.finish();
}

fn bench_encode_string(c: &mut Criterion) {
    let mut group = c.benchmark_group("hessian2/encode/string");
    for size in [0, 16, 64, 256, 1024, 4096] {
        let s = "a".repeat(size);
        group.bench_with_input(BenchmarkId::new("len", size), &s, |b, s| {
            b.iter(|| { let mut enc = Encoder::new(); enc.write_string(black_box(s)); enc.into_bytes() });
        });
    }
    group.finish();
}

fn bench_encode_double(c: &mut Criterion) {
    let mut group = c.benchmark_group("hessian2/encode/double");
    for (label, val) in [("zero", 0.0f64), ("one", 1.0), ("byte", 42.0), ("full", std::f64::consts::PI)] {
        group.bench_with_input(BenchmarkId::new(label, val), &val, |b, &v| {
            b.iter(|| { let mut enc = Encoder::new(); enc.write_double(black_box(v)); enc.into_bytes() });
        });
    }
    group.finish();
}

fn bench_encode_binary(c: &mut Criterion) {
    let mut group = c.benchmark_group("hessian2/encode/binary");
    for size in [16, 256, 1024, 4096] {
        let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        group.bench_with_input(BenchmarkId::new("len", size), &data, |b, data| {
            b.iter(|| { let mut enc = Encoder::new(); enc.write_binary(black_box(data)); enc.into_bytes() });
        });
    }
    group.finish();
}

fn bench_decode_int(c: &mut Criterion) {
    c.bench_function("hessian2/decode/int/1m", |b| {
        let mut enc = Encoder::new(); enc.write_int(1_000_000); let encoded = enc.into_bytes();
        b.iter(|| { let mut dec = Decoder::new(black_box(&encoded)); dec.read_int().unwrap() });
    });
}

fn bench_decode_string_read_string(c: &mut Criterion) {
    let mut group = c.benchmark_group("hessian2/decode/string/read_string");
    for size in [16, 256, 1024, 4096] {
        let s = "a".repeat(size);
        let mut enc = Encoder::new(); enc.write_string(&s); let encoded = enc.into_bytes();
        group.bench_with_input(BenchmarkId::new("len", size), &encoded, |b, encoded| {
            b.iter(|| {
                let mut dec = Decoder::new(black_box(encoded));
                let _: String = dec.read_string().unwrap();
            });
        });
    }
    group.finish();
}

fn bench_decode_string_read_str(c: &mut Criterion) {
    let mut group = c.benchmark_group("hessian2/decode/string/read_str");
    for size in [16, 256, 1024, 4096] {
        let s = "a".repeat(size);
        let mut enc = Encoder::new(); enc.write_string(&s); let encoded = enc.into_bytes();
        group.bench_with_input(BenchmarkId::new("len", size), &encoded, |b, encoded| {
            b.iter(|| {
                let mut dec = Decoder::new(black_box(encoded));
                let _ = dec.read_str().unwrap();
            });
        });
    }
    group.finish();
}

fn bench_roundtrip_string(c: &mut Criterion) {
    let mut group = c.benchmark_group("hessian2/roundtrip/string");
    for size in [64, 256, 1024, 4096] {
        let s = "hello world ".repeat(size / 12).chars().take(size).collect::<String>();
        group.bench_with_input(BenchmarkId::new("len", size), &s, |b, s| {
            b.iter(|| {
                let mut enc = Encoder::new(); enc.write_string(black_box(s)); let encoded = enc.into_bytes();
                let mut dec = Decoder::new(&encoded); let decoded: String = dec.read_string().unwrap();
                assert_eq!(decoded.len(), s.len()); decoded
            });
        });
    }
    group.finish();
}

fn bench_roundtrip_dubbo_body(c: &mut Criterion) {
    c.bench_function("hessian2/roundtrip/dubbo_request_body", |b| {
        b.iter(|| {
            let mut enc = Encoder::new();
            enc.write_string("2.0.2"); enc.write_string("/com.example.UserService");
            enc.write_string("1.0.0"); enc.write_string("getUser");
            enc.write_string("Ljava/lang/String;");
            enc.write_list_begin(3);
            enc.write_string("user-12345"); enc.write_int(25); enc.write_bool(true);
            enc.write_map_begin();
            enc.write_string("trace_id"); enc.write_string("abc-def-ghi");
            enc.write_string("source"); enc.write_string("benchmark");
            enc.write_map_end();
            let encoded = enc.into_bytes();
            let mut dec = Decoder::new(&encoded);
            let _: String = dec.read_string().unwrap(); let _: String = dec.read_string().unwrap();
            let _: String = dec.read_string().unwrap(); let _: String = dec.read_string().unwrap();
            let _: String = dec.read_string().unwrap();
            let _ = encoded.len();
        });
    });
}

criterion_group!(
    benches,
    bench_encode_int,
    bench_encode_long,
    bench_encode_string,
    bench_encode_double,
    bench_encode_binary,
    bench_decode_int,
    bench_decode_string_read_string,
    bench_decode_string_read_str,
    bench_roundtrip_string,
    bench_roundtrip_dubbo_body,
);
criterion_main!(benches);
