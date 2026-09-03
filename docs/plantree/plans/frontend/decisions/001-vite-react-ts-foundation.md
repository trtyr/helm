# 001 — 底座：Vite + React + TypeScript（strict）

Date: 2026-09-03（M1 脚手架期修订：Vite 8 / oxlint，见文末变更记录）

## Context

用户定调前端技术栈为 Vite + React。helm 控制台是重交互、多路由、实时流（5 条 WS）
的应用；团队（单人 + AI）Rust 背景，前端需要最成熟的生态与类型安全。

## Decision

- **Vite 8 + React 19 + TypeScript（strict，写入 tsconfig.app/node）**。
- 包管理 pnpm；Node LTS（≥22）。
- 构建产物 SPA（history 路由见 003），部署为纯静态文件。
- **oxlint**（Vite 8 模板默认，替代 ESLint；不引入 Prettier——oxlint + tsc 已覆盖
  本项目规模，格式化由编辑器默认承担）。

## Consequences

### 启用

- 生态最全：xterm.js / TanStack / chart 库全部一等 React 支持。
- Vite 8（rolldown 构建）冷启动与 HMR 快；TS strict 落在配置层，API 契约
  （openapi 生成类型，见 004）受编译期保护。

### 约束/代价

- 与后端分仓部署；API 地址经 env（`VITE_API_BASE`）注入，dev 走 proxy（见 004）。

## 变更记录

- 原文（规划期）写 Vite 7 + ESLint/Prettier；M1 脚手架期（2026-09-03）采用
  `pnpm create vite` 当前稳定版 **Vite 8.2**（rolldown 构建，实测兼容全部依赖）
  与模板默认 **oxlint**。行为等价、版本更新，按实际落地修订。

**相关：** [002 设计语言](002-design-language-vercel-geist.md)、[003 路由](003-routing-react-router.md)
