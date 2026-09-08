# Current State — 已验证基线

> 本文件记录**当前**实测的门禁结果与开放项，随落地同步更新。
> 最后更新：2026-09-08

## 验证结果

| 检查 | 结果 |
|------|------|
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 0 错误 |
| `cargo test --workspace` | ✅ 全绿 |
| `cargo check --target x86_64-unknown-linux-musl` | ✅ 交叉编译通过 |
| `python scripts/check_openapi.py` | ✅ 72 端点一致 |
| `console`: tsc + vitest | ✅ 62/62 全过 |
| Postgres（helm-postgres） | ✅ Up, healthy |
| Linux musl 交叉编译 | ✅ 通过 |

## 测试组成

- 后端单元/集成测试：job 状态机、auth JWT、file checksum、forward、scheduler、
  agent token/status、connection/transfer/session/stream registry、online_status、
  cert_service、alert_service、forward_manager、encoding、notification、
  agent_lifecycle、audit、store（host/alert/api_key/listener/notification）
- 前端：vitest 52 + Processes.test 10 = 62
- E2E：浏览器 Playwright 35+ 项全功能（进程树/自启动/基线对比/内存扫描/批量/证据包/权限/下线）

## 功能基线（全部实测通过）

### IR 应急响应

- 自启动项全景 12 分类 1307 条（签名/厂商/SYSTEM 服务详情）
- 自启动项禁用/启用/删除 × 注册表/文件/服务/计划任务 四类闭环
- 基线快照对比 新增/移除双向 diff
- 系统日志 Security 60 条（40×4624 + 20×7045）
- 全进程流式内存扫描 290/309 进程 52.7GB
- NTFS USN 文件时间线（Reason 码过滤）
- 证据包一键收集（JSON 8 数据源）
- VirusTotal 接口（待 HELM_VT_API_KEY）
- 页面缓存秒开（autostart 644ms / syslog 464ms）

### 体系化运维

- 多主机批量命令下发
- 进程树 + 异常父子高亮（EDR 检测规则）
- 管理员权限徽章（agents.elevated + SeDebugPrivilege）
- 下线（列表消失 + 进程消失 + 二进制保留）
- 离线挂起补下线（agent_pending_offline 重连补执行）

## 已知限制

| 限制 | 说明 |
|------|------|
| USN TimeStamp | 部分 Windows 版本 journal TimeStamp=0，时间过滤不可用（Reason 过滤已补） |
| 服务 start 500 | 伪服务（非服务二进制 binPath）start 返回 500 而非优雅错误 |
| VT 正向查询 | 需配置 HELM_VT_API_KEY 环境变量 |
| 非 Windows IR | agent IR 模块返回"仅支持 Windows" |
| 注册表页 | 已撤除（内容合并入自启动项页），旧路由重定向 |
| 时间线页 | 已撤除（价值依赖尚未实现的 4625 聚合/结构化解析/异常链标注） |
