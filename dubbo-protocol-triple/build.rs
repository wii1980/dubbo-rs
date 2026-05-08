fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&["proto/triple_wrapper.proto"], &["proto"])?;

    println!("cargo::rerun-if-changed=proto/triple_wrapper.proto");
    Ok(())
}
