# docs/ — helm 项目归档索引

这是 `helm`（集中式运维平台）的完整书面记录，随落地持续维护，以**当前实现**为准。
一句话：Server 控制端 + 跨平台 Agent 被控端，gRPC 双向流（可选 mTLS）为信令底座，axum 出 HTTP API，Postgres 持久化。

## 文档清单

| 文档 | 覆盖内容 | 何时读 |
|------|---------|--------|
| [overview.md](overview.md) | 项目是什么、核心能力、连接模式、30 秒全貌 | 第一次接触，先读这个 |
| [architecture.md](architecture.md) | 目录树、模块边界、依赖方向、运行时流程 | 想找某段代码 / 理解分层时 |
| [tech-stack.md](tech-stack.md) | 语言、框架、依赖版本、工具链、可观测性 | 想知道用了什么库、什么版本 |
| [api.md](api.md) | gRPC 契约 + HTTP API + 配置环境变量 | 要写/改接口、对接前端或 Agent |
| [data-model.md](data-model.md) | Postgres schema、实体关系、状态机、迁移 | 要改表结构、查数据流 |
| [run-and-deploy.md](run-and-deploy.md) | 本地运行、开发命令、e2e、部署模板、发布门禁 | 要跑起来 / 部署 / 提 PR 前 |
| [conventions.md](conventions.md) | 分层、错误处理、命名、测试、契约演进、git 约定 | 要写代码、保持一致时 |
| [current-state.md](current-state.md) | 已验证的构建/测试结果、git 状态、开放项/已知问题 | 接手时看现状与坑 |
| [real-machine-test-report.md](real-machine-test-report.md) | Linux 真机测试报告：forward 持久连接 + mTLS 15/15、Windows GBK 修复验证 | 要看真机实证 / 评估 forward 模式时 |
| [openapi.yaml](openapi.yaml) | HTTP API 契约（OpenAPI 3.0.3，44 端点） | 前端对接 / 接口校验时 |

## 规划树（另行维护）

- [plantree/](plantree/README.md) — 项目的规划与决策树（baseline / plans / decisions / topics）。
  决策链 001–008 保留历史规划性质；roadmap Phase 0–8 已回写 Done。
  baseline 部分由早期 project-init 生成，可能残留历史表述（如「SQLite 起步」已被决策 004 取代），
  以本归档（`docs/*.md`）为准。

## 未收录

- **frontend-backend.md** — 跳过：前端在同仓 `console/`（M1–M4 全落地、20 路由就绪）；
  其要消费的接口即 [api.md](api.md) 的 HTTP API 章节。前端对接对照请用
  [plantree 前端规划](plantree/plans/frontend/README.md)（feature-inventory 70 功能标注落地轮次）。
