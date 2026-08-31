//! 共享 protobuf 契约（gRPC 底座）。
//!
//! Server 与 Agent 都依赖此 crate，保证契约是单一事实来源。
//! 契约演进只做向后兼容变更（`buf breaking` 门禁）。

pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/helm.agent.v1.rs"));
}
