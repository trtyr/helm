//! 烙入配置变更时触发 agent crate 重编译（option_env! 本身不被 cargo 追踪）。

fn main() {
    for key in [
        "HELM_BAKE_SERVER_ADDR",
        "HELM_BAKE_AGENT_TOKEN",
        "HELM_BAKE_AGENT_ID",
        "HELM_BAKE_CONN_MODE",
        "HELM_BAKE_LISTEN_ADDR",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }
}
