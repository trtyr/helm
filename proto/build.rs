fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 优先用环境 protoc，没有则落到 vendored 二进制（本机未装 protobuf 时仍可构建）
    if std::env::var_os("PROTOC").is_none() {
        // build script 此刻单线程，set_var 安全（edition 2024 要求显式 unsafe）
        unsafe {
            std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
        }
    }

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &["helm/agent/v1/agent.proto", "helm/agent/v1/types.proto"],
            &["."],
        )?;
    Ok(())
}
