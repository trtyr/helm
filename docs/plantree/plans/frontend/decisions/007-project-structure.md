# 007 — 目录结构：按路由域分组 + 契约生成层

Date: 2026-09-03

## Context

20 路由 / 70 功能 / 5 WS / 生成类型。结构要服务两个导航习惯：
按页面找代码（路由域）与按抽象找代码（api/components/hooks）。

## Decision

```text
helm-console/
├── src/
│   ├── api/               # 契约与请求层
│   │   ├── schema.d.ts    # openapi-typescript 生成（勿手改）
│   │   ├── client.ts      # fetch 封装（JWT 注入、401 拦截 F03）
│   │   └── ws.ts          # useWsStream 及场景 hooks
│   ├── components/        # 跨页业务组件（状态徽标/传输队列/日志抽屉/空态…）
│   ├── ui/                # 基础件（geistcn 或自建降级，按 005）
│   ├── layouts/
│   │   ├── AppLayout.tsx  # Topbar + Sidebar + StatusBar（/ 布局路由）
│   │   └── HostDetailLayout.tsx
│   ├── routes/            # 路由域：每路由一目录，页内私有组件同目录
│   │   ├── login/
│   │   ├── dashboard/
│   │   ├── hosts/
│   │   ├── host-detail/   # 8 个 tab 子目录（overview/ terminal/ …）
│   │   ├── jobs/          # 列表 + detail/
│   │   ├── notifications/ alerts/ audit/ listeners/ settings/ forward/
│   ├── hooks/             # 通用 hooks（useQueryState、useRelativeTime…）
│   ├── styles/            # tokens.css（--ds-* 两组主题）、tailwind 配置
│   └── main.tsx / router.tsx
├── e2e/                   # Playwright（后话，结构预留）
└── vite.config.ts
```

- 路由域目录与 [routes/ 规格](../routes/README.md) 一一对应——「规格 ↔ 实现」同构导航。
- 页内私有组件不下沉 components/（只有跨页复用才上移）。
- ws 场景 hook 与消费它的路由域若强绑定，可放路由域内（api/ws.ts 只留通用层）。

## Consequences

### 启用

- 找代码两条路都短：页面入口 `routes/<name>/`，契约 `api/schema.d.ts`；
  AI/新人按规格文档直达实现位置。

### 约束/代价

- components/ 与 ui/ 边界须守（ui = 无业务语义的基础件）。

**相关：** [001 底座](001-vite-react-ts-foundation.md)、[007 之前的目录讨论见 routes/README](../routes/README.md)
