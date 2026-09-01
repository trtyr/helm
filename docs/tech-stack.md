# Tech Stack

## 语言与工具链（已实测）

| 项 | 版本 |
|----|------|
| Rust（rustc） | 1.98.0（2026-08-18） |
| Cargo | 1.98.0（2026-08-05） |
| edition | 2024 |
| buf | 1.72.0 |
| Docker | 29.6.0 |
| 任务运行器 | Just（`Justfile`） |

## Workspace 结构

`Cargo.toml` 定义 `[workspace]`，`members = ["proto", "server", "agent"]`，
共享 `[workspace.package]`（version 0.1.0，edition 2024，MIT）与 `[workspace.dependencies]`。

| crate | 类型 | 职责 |
|-------|------|------|
| `helm-proto` | lib | 共享 protobuf 契约（tonic-prost-build 编译产物） |
| `helm-server` | lib + bin | 控制端（axum + tonic + sqlx） |
| `helm-agent` | bin | 被控端（tokio，跨平台） |

## 关键依赖（workspace 声明版本）

| 依赖 | 声明 | 用途 |
|------|------|------|
| tokio | 1.53（full） | 异步运行时 |
| tokio-stream | 0.1 | 双向流包装（ReceiverStream） |
| tonic / tonic-prost / prost | 0.14 | gRPC 服务端/客户端 |
| tonic-prost-build | 0.14 | build.rs 编译 proto |
| axum | 0.8 | HTTP API |
| http | 1 | 底层 HTTP 类型 |
| sqlx | 0.9（runtime-tokio, tls-rustls, postgres, migrate, chrono, uuid, json） | 异步 DB + 迁移 |
| tracing / tracing-subscriber | 0.1 / 0.3 | 结构化日志（env-filter, json） |
| serde / serde_json | 1 / 1 | 序列化 |
| clap | 4.6（derive, env） | CLI 配置 |
| thiserror / anyhow | 2 / 1 | 错误处理 |
| uuid | 1（v4, serde） | ID |
| chrono | 0.4（serde） | 时间戳 |
| jsonwebtoken | 11（rust_crypto） | JWT 签发/校验 |
| bcrypt | 0.19 | 密码哈希 |
| hostname | 0.4 | Agent 取主机名 |
| sysinfo | 0.39 | Agent 采集指标 |
| sha2 / hex | 0.11 / 0.4 | 文件校验和 |

> 注：`sha2` 同时存在 0.11.0（直接依赖，声明 "0.11"）与 0.10.9（bcrypt/jsonwebtoken 等传递依赖）两个版本。

## crate 依赖分布

- **helm-server**：helm-proto + tokio/tonic/prost/axum/sqlx/clap/tracing/serde/serde_json/tokio-stream/uuid/chrono/bcrypt/jsonwebtoken/sha2/hex。
- **helm-agent**：helm-proto + tokio/tonic/prost/tracing/clap/serde/tokio-stream/hostname/sysinfo/sha2/hex。
- **helm-proto**：prost/tonic/tonic-prost/http + build-dep tonic-prost-build。

## 契约工具

- `buf.yaml`（v2）：`lint` 用 `STANDARD`（豁免 `RPC_REQUEST_STANDARD_NAME`、`RPC_RESPONSE_STANDARD_NAME`、`RPC_REQUEST_RESPONSE_UNIQUE`——双向流 envelope 语义）；`breaking` 用 `FILE`。

## 可观测性

- `tracing` + `tracing-subscriber`，`EnvFilter` 优先读 `RUST_LOG`，否则用配置默认级别。
- 结构化字段（agent_id / job_id / task_id / error / code / retryable）。
- 健康探针 `GET /healthz`。
