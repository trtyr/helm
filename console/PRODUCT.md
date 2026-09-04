# PRODUCT.md — helm-console

## What is this?

helm 的前端控制台。helm 是一个集中式运维平台后端（Rust：Server 控制端 + 跨平台
Agent 被控端，gRPC 双向流 + Postgres）。本控制台是运维者管理主机舰队、执行命令、
传输文件、看监控、收通知的唯一界面。

## Who is it for?

单一运维者（个人使用，非多租户 SaaS）。用户是资深工程师，审美取向 Vercel：
黑白灰、锐利、克制、信息密度优先。日常在暗色环境工作。

## Core jobs

1. 看清主机舰队状态（在线/离线/stale、标签分组）
2. 对单台主机：终端、文件、服务、进程、网络、指标、任务七个面
3. 感知事件：主机上下线与指标预警的系统内通知（铃铛小卡片，不外发）
4. 管理接入面：监听器、审计、告警历史

## Truth sources

- 后端契约：helm 仓 `docs/openapi.yaml`（44 HTTP + 5 WS）
- 设计规格：helm 仓 `docs/plantree/plans/frontend/`（20 路由 × 四要素 + 70 功能清单）
- 本文件由该规格浓缩；冲突时以 plantree 规格为准。

## Non-goals

多租户 / 权限体系（单用户 JWT）、通知外发（webhook 等，决策 009 明确不做）、
移动端优化（桌面优先 ≥1024）。
