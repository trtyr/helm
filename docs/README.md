# docs/ — helm 项目归档索引

这是 `helm`（集中式运维平台）的完整书面记录，由 project-init 只读流程从**当前实现**生成。
一句话：Server 控制端 + 跨平台 Agent 被控端，gRPC 双向流为信令底座，axum 出 HTTP API，Postgres 持久化。

## 文档清单

| 文档 | 覆盖内容 | 何时读 |
|------|---------|--------|
| [overview.md](overview.md) | 项目是什么、核心能力、连接模式、30 秒全貌 | 第一次接触，先读这个 |
| [architecture.md](architecture.md) | 目录树、模块边界、依赖方向、运行时流程 | 想找某段代码 / 理解分层时 |
| [tech-stack.md](tech-stack.md) | 语言、框架、依赖版本、工具链、可观测性 | 想知道用了什么库、什么版本 |
| [api.md](api.md) | gRPC 契约 + HTTP API + 配置环境变量 | 要写/改接口、对接前端或 Agent |
| [data-model.md](data-model.md) | Postgres schema、实体关系、状态机、迁移 | 要改表结构、查数据流 |
| [run-and-deploy.md](run-and-deploy.md) | 本地运行、开发命令、e2e smoke、发布门禁 | 要跑起来 / 部署 / 提 PR 前 |
| [conventions.md](conventions.md) | 分层、错误处理、命名、测试、契约演进、git 约定 | 要写代码、保持一致时 |
| [current-state.md](current-state.md) | 已验证的构建/测试结果、git 状态、开放项/已知问题 | 接手时看现状与坑 |

## 规划树（另行维护）

- [plantree/](plantree/README.md) — 项目早期的规划与决策树（baseline / plans / decisions）。
  **注意**：其中部分内容已过时（仍称项目为"空壳"、SQLite 起步等），以本归档为准；
  但决策链 001–004 仍有效，可交叉参考。详见 [current-state.md](current-state.md) 开放项 #2。

## 未收录

- **frontend-backend.md** — 跳过：本仓库**没有前端**。控制台是独立工程，不在本树内；
  其要消费的接口即 [api.md](api.md) 的 HTTP API 章节。
