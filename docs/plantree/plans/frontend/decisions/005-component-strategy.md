# 005 — 组件方案：Tailwind v4 + 官方 tokens 优先，geistcn 按需

Date: 2026-09-03

## Context

Geist 官方已开源 React 组件库 `@vercel/geistcn`（Button/Modal/Toggle 等）与
图标资产 `@vercel/geistcn-assets`。备选：全自建（Tailwind）或通用组件库
（shadcn/ui、AntD）。需要权衡：还原度、维护成本、主题控制。

## Decision

- **Tailwind CSS v4** 为样式底座；Geist 的 typography class（`text-label-14` 等）
  与色彩变量（`--ds-*`）按官方 tokens 配置进 Tailwind theme。
- **`@vercel/geistcn` 可用则用**（Button/Input/Modal/Toggle/Dropdown 等基础件），
  版本可用性在脚手架期验证；若与 React 19 兼容性问题，降级方案 = 抽其视觉规范
  自建同款（tokens 已在 design-language.md，工作量可控：基础件 ≤15 个）。
- 业务组件（表格工具行、状态徽标、传输队列卡、日志抽屉、通知下拉……）
  一律自建（Tailwind + tokens），这是产品差异所在。
- 图标：`@vercel/geistcn-assets`（不可用则 lucide-react 同风格替代）。

## Consequences

### 启用

- 视觉还原 Vercel 的成本最低路径；主题 = 换 CSS 变量组，组件零改动。
- 自建范围明确：只有「Geist 没有」的业务件，不重复造基础件。

### 约束/代价

- geistcn 较新，API 稳定性需锁定版本；降级路径已备（自建基础件清单见
  design-language 补充，实现期产出）。

**相关：** [002 设计语言](002-design-language-vercel-geist.md)、[007 目录结构](007-project-structure.md)
