fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &["helm/agent/v1/agent.proto", "helm/agent/v1/types.proto"],
            &["."],
        )?;
    Ok(())
}
