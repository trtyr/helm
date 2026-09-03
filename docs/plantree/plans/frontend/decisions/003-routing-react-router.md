# 003 — 路由：React Router v7（声明式）

Date: 2026-09-03

## Context

20 个路由（1 公开 + 19 受保护，主机详情 8 tabs 为嵌套布局）。
需要：布局嵌套（Topbar/Sidebar 包裹层 + 详情 tabs 层）、守卫重定向、
URL query 同步（过滤/分页/分享）。

## Decision

- **React Router v7**（library 模式，非 framework 模式——不引 SSR/RSC 复杂度）。
- 声明式 `<Route>` 树：`/login` 公开；`/` 布局路由（Outlet：Topbar + Sidebar +
  StatusBar）嵌套全部受保护路由；`/hosts/:id` 二级布局路由嵌套 8 个 tab 子路由。
- 守卫：布局路由 loader 检查 token → `<Navigate to="/login?redirect=>`。
- URL query 状态（?status=&kind=&page=）：自定义 `useQueryState` hook
  （读写同步到 router location）。

## Consequences

### 启用

- 嵌套布局与 tab 子路由天然匹配详情页结构；文档与生态最成熟。
- query state 可分享/回退（规格中多处要求）。

### 约束/代价

- TanStack Router 类型安全更强，但社区份额与心智成本不如 React Router；
  声明式树 + 严格 TS 路由参数类型足够。

**相关：** [001 底座](001-vite-react-ts-foundation.md)、[routes/README](../routes/README.md)
