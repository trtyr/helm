# 004 — 数据层：TanStack Query + 统一 WS hooks

Date: 2026-09-03

## Context

数据面：44 HTTP 端点 + 5 条 WS 流。HTTP 以查询为主（列表/详情/变更），
WS 是推送（metrics/job 输出/服务日志/终端/通知）。变更后列表需失效重拉。

## Decision

- **TanStack Query v5** 管全部 HTTP：query key 按域分层（`['hosts', filter]`）；
  变更（mutation）成功后精确 invalidate（如 mark-read 后 invalidate `['notifications']`）。
- 轮询用 Query 的 `refetchInterval`（页面不可见自动暂停，规格各页 30s/5s 节奏）。
- **API 类型从 openapi.yaml 生成**（`openapi-typescript` + `openapi-typescript-fetch`），
  与后端契约同源，杜绝手写类型漂移；生成物进 `src/api/schema.d.ts`。
- **WS 层自建 hooks**（不用 socket.io 库——后端是原生 WebSocket）：
  - `useWsStream(key, onMessage)`：单例连接管理（按 endpoint 复用），
    断线指数退避重连（1s/2s/4s 上限 15s），状态暴露给 StatusBar（F06）。
  - 场景 hook 叠加：`useMetricsStream(hostId)`（过滤+缓冲 60 点）、
    `useJobStream(jobId)`、`useNotificationsStream()`（角标 + toast 分发）。
  - 终端 WS（binary 双向）独立于 useWsStream（会话生命周期不同），终端 hook
    直连 + 手动管理。
- JWT 注入：fetch 层统一带 Authorization；WS 走 query token 参数（后端语义）。

## Consequences

### 启用

- 缓存/失效/重试/轮询零手写；类型与后端 44 端点契约同步（openapi 改了重新生成）。
- WS 单例复用：多组件订阅同一条流（如 metrics 同时供 sparkline 与图表）不打多个连接。

### 约束/代价

- WS 无订阅协议（后端按 URL 订阅，host 过滤靠前端丢帧）——metrics 全局流在
  大主机量下有冗余流量，记入 open-questions。

**相关：** [001 底座](001-vite-react-ts-foundation.md)、[006 图表](006-charts-library.md)
