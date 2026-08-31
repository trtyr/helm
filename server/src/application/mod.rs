//! 应用层：用例编排，业务唯一入口。HTTP 与 gRPC 适配器都调用本层。

pub mod auth_service;
pub mod exec_service;
pub mod file_service;
pub mod forward_service;
pub mod scheduler;
