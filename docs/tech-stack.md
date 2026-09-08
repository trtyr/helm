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
| tonic / tonic-prost / prost | 0.14（tonic 带 `tls-ring`） | gRPC 服务端/客户端 + mTLS（rustls/ring） |
| tonic-prost-build | 0.14 | build.rs 编译 proto |
| axum | 0.8（`ws`） | HTTP API + WebSocket |
| http | 1 | 底层 HTTP 类型 |
| sqlx | 0.9（runtime-tokio, tls-rustls, postgres, migrate, chrono, uuid, json） | 异步 DB + 迁移 |
| tracing / tracing-subscriber | 0.1 / 0.3（env-filter, json） | 结构化日志 |
| tracing-appender | 0.2 | 日志按天滚动落文件（Agent 服务模式） |
| serde / serde_json | 1 / 1 | 序列化 |
| clap | 4.6（derive, env） | CLI 配置 |
| thiserror / anyhow | 2 / 1 | 错误处理 |
| uuid | 1（v4, serde） | ID |
| chrono | 0.4（serde） | 时间戳 |
| jsonwebtoken | 11（rust_crypto） | JWT 签发/校验 |
| bcrypt | 0.19 | 密码哈希 |
| hostname | 0.4 | Agent 取主机名 |
| sysinfo | 0.39 | Agent 采集指标（CPU/内存/磁盘/网络/进程） |
| sha2 / hex | 0.11 / 0.4 | 文件校验和 |
| portable-pty | 0.9 | 交互终端 PTY（Linux/macOS pty + Windows ConPTY） |
| rcgen | 0.14（crypto, pem, x509-parser） | Server 内置 CA 生成 + 签发 mTLS 证书；Agent 生成 key/CSR |
| time | 0.3 | rcgen 证书时间字段 |
| reqwest | 0.12（json, rustls-tls） | Agent 换证书 HTTP 调用（rustls，musl 交叉编译友好） |
| encoding_rs | 0.8 | Agent 控制台输出解码（Windows OEM 代码页 936/GBK 等 → UTF-8） |
| windows-sys | 0.59（`cfg(windows)`，features 见下方） | Windows 原生能力层：SCM 服务管理、IpHelper 网络/连接表、进程路径查询、Authenticode 签名校验（WinTrust + CryptCatalog）、USN 文件时间线（DeviceIoControl + Ioctl）、权限检测（CheckTokenMembership）、CREATE_NO_WINDOW 子进程包装 |
| — features | `Win32_Globalization` / `Win32_Foundation` / `Win32_Storage_FileSystem` / `Win32_System_Services` / `Win32_System_Threading` / `Win32_System_Registry` / `Win32_System_Diagnostics_Debug` / `Win32_System_Diagnostics_ToolHelp` / `Win32_System_Memory` / `Win32_NetworkManagement_IpHelper` / `Win32_NetworkManagement_Ndis` / `Win32_NetworkManagement_NetManagement` / `Win32_Networking_WinSock` / `Win32_Security` / `Win32_Security_WinTrust` / `Win32_Security_Cryptography` / `Win32_Security_Cryptography_Catalog` / `Win32_System_IO` / `Win32_System_Ioctl` | |

> 注：`sha2` 同时存在 0.11.0（直接依赖，声明 "0.11"）与 0.10.9（bcrypt/jsonwebtoken 等传递依赖）两个版本。

## crate 依赖分布

- **helm-server**：helm-proto + tokio/tonic/prost/axum/sqlx/clap/tracing/tracing-appender/serde/serde_json/tokio-stream/uuid/chrono/bcrypt/jsonwebtoken/sha2/hex/rcgen/time/reqwest。
- **helm-agent**：helm-proto + tokio/tonic/prost/tracing/tracing-appender/clap/serde/serde_json/tokio-stream/hostname/sysinfo/sha2/hex/portable-pty/rcgen/reqwest/encoding_rs（+ `cfg(windows)` 下 windows-sys 19 个 feature）。
- **helm-proto**：prost/tonic/tonic-prost/http + build-dep tonic-prost-build。

## 契约工具

- `buf.yaml`（v2）：`lint` 用 `STANDARD`（豁免 `RPC_REQUEST_STANDARD_NAME`、`RPC_RESPONSE_STANDARD_NAME`、`RPC_REQUEST_RESPONSE_UNIQUE`——双向流 envelope 语义）；`breaking` 用 `FILE`。
- `docs/openapi.yaml`（OpenAPI 3.0.3）：HTTP API 契约，经 `scripts/check_openapi.py` 机器校验与 server 路由一致。

## 可观测性

- `tracing` + `tracing-subscriber`，`EnvFilter` 优先读 `RUST_LOG`，否则用配置默认级别。
- 结构化字段（agent_id / job_id / task_id / error / code / retryable）。
- 健康探针 `GET /healthz`；审计日志 `GET /api/v1/audit`；告警 `GET /api/v1/alerts`。
