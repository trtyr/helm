# 010 — API key：机器对机器认证（helm_ 前缀密钥，仅存哈希）

Date: 2026-09-06

## Context

平台认证此前只有两条路径：控制台用户的 JWT（浏览器交互）与 Agent 的注册 token（gRPC 信令）。
程序化调用方（CI 脚本、自动化工具、第三方集成、本仓库新增的运维 skill 脚本）没有合适的凭据：
JWT 24h 过期需要保管密码并反复登录，Agent token 只用于 gRPC 注册而非 HTTP API。

同时平台需要一个「健全系统」的 API key 管理机制：创建、列举、查看、吊销、过期、使用追踪，
而不是往配置文件里塞静态密钥。

## Decision

- **形态**：明文 key = `helm_` + 40 位 hex（20 随机字节，`rand::rng` CSPRNG）。
  `helm_` 前缀是认证分流依据：Bearer 值以它开头走 API key 路径，否则按 JWT 解码。
- **存储**：`api_keys` 表仅存 sha256(key) hex + 展示前缀（前 12 字符）；**明文只在创建响应出现一次**，
  列表/详情只回前缀。与 JWT 不同，key 无状态泄漏后无法伪造其余 key（哈希不可逆）。
- **生命周期**：`expires_at`（空 = 永不过期，SQL 与 Service 双重校验）、`revoked_at`（吊销幂等）、
  `last_used_at`（认证命中时尽力刷新）。已吊销 key 随保留清理循环 30 天后删除。
- **权限边界**：api-keys 管理端点（list/create/get/revoke）**仅接受 JWT**；
  API key 命中后合成 Claims `{sub: "api-key:<name>", role: "api-key"}`，
  role 标记让管理端点可识别并拒绝 key 自管（403 forbidden）——防止 key 自我续期/扩散。
- **审计**：API key 的调用以 `api-key:<name>` 记入 audit_logs actor，与 JWT（username）同轨。
- **复用**：`verify_bearer_token` 成为 HTTP 中间件与 WS query-param 端点（terminal/streams）的
  统一入口，两种凭据在所有受保护端点与 WS 流上等效。

## Consequences

### 启用

- 程序化调用方有了长效、可吊销、可审计的凭据（CI / 自动化 skill 脚本不再依赖明文密码）。
- key 泄漏的应急路径明确：吊销即失效，无需换 JWT secret（不影响其他用户/机器）。
- 管理面自洽：key 的增删查只能由持 JWT 的控制台用户执行。

### 约束/代价

- Bearer 前缀成为协议的一部分：未来若引入其他凭据形态，需在 `verify_bearer_token` 的分流处扩展。
- sha256 直接哈希（非 bcrypt）：key 是 160-bit 高熵随机值，无字典/爆破面，与密码场景不同；
  换取的是认证路径无 bcrypt 开销（每次请求都过认证）。
- 无 per-key 作用域/细粒度权限（沿用 RBAC 未强制的现状，决策 007）——key 即全权，
  交付最小权限前应按用途拆 key 并及时吊销。

**相关：** [007 安全姿态](007-security-posture.md)、
[topics/security](../topics/security.md)
