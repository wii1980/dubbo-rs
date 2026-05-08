fn main() -> anyhow::Result<()> {
    let out_dir = std::env::var("OUT_DIR")?;

    let config = dubbo_rs_codegen::GeneratorConfigBuilder::new()
        .proto_path("proto/exchange.proto")
        .output_dir(&out_dir)
        .enable_client(true)
        .enable_server(true)
        .client_mode(dubbo_rs_codegen::ClientMode::Both)
        .build()?;

    let generator = dubbo_rs_codegen::CodeGenerator::new(config);
    let generated = generator.generate()?;

    generated.write_to_dir(std::path::Path::new(&out_dir))?;

    Ok(())
}
