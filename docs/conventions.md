# Conventions — 代码风格与工作流

## 架构约定

- **分层**：`domain`（纯领域，无 IO/框架）→ `application`（用例编排，唯一业务入口）→ 适配层 `grpc` / `http` / `store`。
- **依赖方向**：外层适配器 → 内层领域，**禁止反向 import**；适配层之间禁止互相 import。
- HTTP 与 gRPC 适配器**都**调用 `application`，不直接互相调用，也不绕过 application 直接访问领域。
- **深模块**：`domain` 是深模块（实体 + 状态机 + 不变量，无外部依赖）；`ConnectionRegistry` / `TransferRegistry` / `SessionRegistry` / `FileListRegistry` / `QueryRegistry` / `StreamRegistry` 把活跃连接 / 传输等待 / 会话输出 / 请求-应答 / 实时广播收敛到单点。

## 错误处理约定（`domain/error.rs`）

- 统一类型化错误 `Error`，每个变体带稳定 `code()`、`retryable()` 分类、`safe_message()`。
- **单点错误边界**：HTTP 层 `IntoResponse for Error` 一处日志 + 一处安全响应；内部细节只进 tracing 日志，`message` 用 `safe_message()` 不外泄 DB/IO 串。
- `retryable()` 仅 `NotConnected` / `Storage` 为 true（瞬时/基础设施类）。

## 命名与风格

- 模块用 snake_case，crate 名 `helm-*`（`helm-server` / `helm-agent` / `helm-proto`）。
- 仓储按实体拆分：`*_repo.rs`（`HostRepo` / `AgentRepo` / `JobRepo` / `MetricRepo` / `FileTransferRepo` / `TaskRepo` / `UserRepo` / `ListenerRepo` / `ServiceRepo` / `AuditRepo` / `AlertRepo`）。
- 纯函数抽到模块顶层并配 `#[cfg(test)]` 单测（如 `checksum`、`job_status`、`token_matches`、`parse_schedule_params`、`is_stale`、`map_service_status`、`threshold_for`、`diff_hosts`、`decode_with_codepage`）。
- 文件头用 `//!` 模块级文档说明职责。

## 注册表 / 桥接模式（gRPC ↔ HTTP）

Server 侧在 gRPC 入站流与 HTTP 处理器之间，用 `Arc<Mutex<HashMap<...>>>` 注册表桥接：

- **请求-应答**：Server 发起查询前注册 `oneshot`，Agent 回 `*Result` 时 complete。实例：`TransferRegistry`（file chunk）、`FileListRegistry`（列目录）、`QueryRegistry`（进程/网络）。
- **会话输出**：`SessionRegistry`（`session_id → mpsc::Sender<Vec<u8>>`）把 Agent `SessionOutput` 转发给 WebSocket 端点。
- **实时广播**：`StreamRegistry`（key → 订阅者列表，`service:{id}` / `job:{id}` / `metrics`），增量推送，失效 sender 惰性清理。

新功能如需 Server 主动发起请求-应答，新增一个 registry 模块沿用此模式。

## proto 序列化约定

- `proto/build.rs` 只用 `tonic_prost_build` 生成类型，**不加 serde**——proto 类型不实现 `Serialize`。
- HTTP 处理器要返回 proto 类型为 JSON 时，写一个手写 `Serialize` view struct + `impl From<ProtoType>`（如 `FileEntryView` / `ProcessView` / `NetInterfaceView` / `HostView` / `AgentDetail` / `ListenerView`）。

## 测试约定

- 单元测试：`#[cfg(test)] mod tests` 内联在源码模块中，覆盖纯函数与边界。
- 集成测试：`server/tests/` 下连**真实 Postgres**（读 `HELM_DATABASE_URL`，默认 docker compose 5433），需先起容器。
- **自清理**：集成测试触及会影响 Server 启动行为的表（如 `listeners` 的 running 行）时，测试结束须删除自建行。
- 测试真实：断言真实行为（返回值/状态码/校验和），不 mock 糊弄。
- e2e：`scripts/*.py`（Python 纯标准库 + `urllib`/`subprocess`/`json`），共享 `e2e_helpers.py`；
  唯一例外是需 WebSocket 的脚本（phase6/phase8）用第三方 `websockets`。

## 契约演进（protobuf + OpenAPI）

- `proto/` 是 Server/Agent 契约单一事实来源，两 crate 都依赖它，不复制契约。
- 只做向后兼容变更（新增字段/服务，不删不改编号）；破坏性变更走 `v2`。
- `buf breaking --against '.git#ref=HEAD~1'` 作为兼容性门禁。
- `docs/openapi.yaml` 是 HTTP 契约，`scripts/check_openapi.py` 机器校验与 server 路由一致。

## Git 工作流

- 当前仓库：分支 `master`，remote `origin` → github.com/trtyr/helm（私有）。
- 提交信息用 conventional commits（`feat:` / `fix:` / `docs:` / `test:` / `refactor:` / `chore:`）。
- 无 CI 配置文件；门禁为本地 `just check` + `buf`。

## 配置约定

- 配置用 `clap` derive，字段支持 `#[arg(long, env = "...")]`——CLI 参数与环境变量同名等价。
- 默认值里的 `dev-*-change-me` 仅用于开发，生产必须覆盖。
- 构造器参数过多（>7）时加 `#[allow(clippy::too_many_arguments)]`（先例：`ListenerService::new`、`http::serve`、`AgentServiceImpl::new`、`listener_registry::start`）。
