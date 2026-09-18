# docs/ — helm 文档指针索引

> **2026-09-15 起，helm 的全部书面记录归档于 engram（projects 域，project `helm`）。**
> 本地 `docs/` 只保留本索引与机器契约资产。正文不再放本地——查文档一律走 engram。

## 怎么查

- engram MCP：`projects` 工具，`project_name = "helm"`
  - `doc_search`：跨文档按行检索（grep 式）
  - `doc_get`：按 doc_id 精读（映射表里有每个文档的 doc_id）
- 分类结构：总览 / 架构与设计 / 接口契约 / 数据模型 / 运行与部署 / 约定 / 现状与门禁 / 应急响应 / 规划

## 分类 → 内容对照

| engram 分类 | 内容（原本地文件） |
|---|---|
| 总览 | overview（项目是什么）、README（原归档索引）、本地归档 → engram 映射表 |
| 架构与设计 | architecture（目录树/模块边界/运行时流程）、tech-stack（依赖版本）、实证地图（2026-09-15 codefind 实测） |
| 接口契约 | api（gRPC 契约 + HTTP API + 环境变量）、mcp（MCP 接入：`POST /mcp`，50 op） |
| 数据模型 | data-model（17 表 / 16 迁移 / 状态机） |
| 运行与部署 | run-and-deploy（运行/开发命令/e2e/部署模板）、real-machine-test-report（真机 15/15） |
| 约定 | conventions（分层/错误处理/命名/测试/契约演进/git 约定） |
| 现状与门禁 | current-state（实测门禁基线）、文档 vs 代码差异清单（2026-09-15） |
| 应急响应 | ir-capabilities（IR 全能力：自启动/内存扫描/时间线/证据包） |
| 规划 | 原 plantree 全树 75 篇（README / baseline 6 / roadmap / decisions 001–011 / topics / frontend 全套），folder 保留原目录结构 |

## 本地保留资产

- `docs/openapi.yaml` — HTTP API 契约（OpenAPI 3.0.3，75 端点）；`scripts/check_openapi.py` 消费，勿迁。
- 本文件 — 指针索引。

## 历史备注

- 原 docs 归档 12 篇 + plantree 75 篇于 2026-09-15 逐字迁入 engram（全量 doc_get 与原文 diff 一致后删除本地）。
- 旧索引中「mcp.md 未入册」「决策 001–008」「migrations 14 个版本」等过期项已在 engram 内修正，
  明细见 engram「现状与门禁 / 文档 vs 代码差异清单（2026-09-15）」。
