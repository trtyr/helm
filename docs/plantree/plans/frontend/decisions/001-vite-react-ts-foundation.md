# 001 — 底座：Vite + React + TypeScript

Date: 2026-09-03

## Context

用户定调前端技术栈为 Vite + React。helm 控制台是重交互、多路由、实时流（5 条 WS）
的应用；团队（单人 + AI）Rust 背景，前端需要最成熟的生态与类型安全。

## Decision

- **Vite 7 + React 19 + TypeScript（strict）**。
- 包管理 pnpm；Node LTS（≥22）。
- 构建产物 SPA 单 bundle（hash 路由不用——见 003 用 history 路由），
  部署为纯静态文件（由 Server 托管或任意静态服务）。
- ESLint（typescript-eslint）+ Prettier + `tsc --noEmit` 门禁。

## Consequences

### 启用

- 生态最全：xterm.js / TanStack / chart 库全部一等 React 支持。
- Vite 冷启动 <1s、HMR 快，开发体验好；TS strict 保证 API 契约（openapi 生成类型，见 004）。

### 约束/代价

- 与后端分仓部署；API 地址经 env（`VITE_API_BASE`）注入，生产可同域反代。

**相关：** [002 设计语言](002-design-language-vercel-geist.md)、[003 路由](003-routing-react-router.md)
