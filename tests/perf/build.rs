fn main() {
    let proto_dir = std::path::Path::new("proto");
    prost_build::Config::new()
        .compile_protos(&[proto_dir.join("bench.proto")], &[proto_dir])
        .unwrap();
}
