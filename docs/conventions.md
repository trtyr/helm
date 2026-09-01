# Conventions — 代码风格与工作流

## 架构约定

- **分层**：`domain`（纯领域，无 IO/框架）→ `application`（用例编排，唯一业务入口）→ 适配层 `grpc` / `http` / `store`。
- **依赖方向**：外层适配器 → 内层领域，**禁止反向 import**；适配层之间禁止互相 import。
- HTTP 与 gRPC 适配器**都**调用 `application`，不直接互相调用，也不绕过 application 直接访问领域。
- **深模块**：`domain` 是深模块（实体 + 状态机 + 不变量，无外部依赖）；`ConnectionRegistry` / `TransferRegistry` 把活跃连接/传输等待表收敛到单点。

## 错误处理约定（`domain/error.rs`）

- 统一类型化错误 `Error`，每个变体带稳定 `code()`、`retryable()` 分类、`safe_message()`。
- **单点错误边界**：HTTP 层 `IntoResponse for Error` 一处日志 + 一处安全响应；内部细节只进 tracing 日志，`message` 用 `safe_message()` 不外泄 DB/IO 串。
- `retryable()` 仅 `NotConnected` / `Storage` 为 true（瞬时/基础设施类）。

## 命名与风格

- 模块用 snake_case，crate 名 `helm-*`（`helm-server` / `helm-agent` / `helm-proto`）。
- 仓储按实体拆分：`*_repo.rs`（`HostRepo` / `AgentRepo` / `JobRepo` / `MetricRepo` / `FileTransferRepo` / `TaskRepo` / `UserRepo`）。
- 纯函数抽到模块顶层并配 `#[cfg(test)]` 单测（如 `checksum`、`job_status`、`token_matches`、`parse_schedule_params`）。
- 文件头用 `//!` 模块级文档说明职责。

## 测试约定

- 单元测试：`#[cfg(test)] mod tests` 内联在源码模块中，覆盖纯函数与边界。
- 集成测试：`server/tests/` 下连**真实 Postgres**（读 `HELM_DATABASE_URL`，默认 docker compose 5433），需先起容器。
- 测试真实：断言真实行为（返回值/状态码/校验和），不 mock 糊弄。

## 契约演进（protobuf）

- `proto/` 是 Server/Agent 契约单一事实来源，两 crate 都依赖它，不复制契约。
- 只做向后兼容变更（新增字段/服务，不删不改编号）；破坏性变更走 `v2`。
- `buf breaking --against '.git#ref=HEAD~1'` 作为兼容性门禁。

## Git 工作流

- 当前仓库：分支 `master`，**无 remote 配置**（本地仓库）。
- 提交信息用 conventional commits（`feat:` / `fix:`，见 `git log`：`feat: 集中式运维平台后端…`、`fix: 审计整改…`）。
- 无 CI 配置文件；门禁为本地 `just check` + `buf`。

## 配置约定

- 配置用 `clap` derive，字段支持 `#[arg(long, env = "...")]`——CLI 参数与环境变量同名等价。
- 默认值里的 `dev-*-change-me` 仅用于开发，生产必须覆盖。
