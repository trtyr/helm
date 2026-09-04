# helm-console — 前端控制台（主仓 console/ 子目录）

helm 集中式运维平台的前端控制台（Vite + React 19 + TS + Tailwind v4，Vercel/Geist 设计语言），
与 Rust 后端（Cargo workspace：proto/server/agent）同仓。

- 设计规格：`../../docs/plantree/plans/frontend/`（20 路由 × 四要素 + 70 功能清单）
- 后端契约：`../../docs/openapi.yaml`（44 HTTP + 5 WS）
- 当前进度：**M4 全局视图里程碑**（20 路由全部就绪：仪表盘/通知中心+铃铛/告警/审计/监听器/设置/forward）

> 工作流：进入本目录后 `pnpm` 独立运行（非根 workspace）。设计源头在
> `../../docs/plantree/plans/frontend/`（各规格已注记落地里程碑）。

## 开发

```bash
pnpm install
pnpm dev        # :5180（dev proxy → 127.0.0.1:18081 后端）
pnpm api:gen    # 从后端 openapi.yaml 重新生成 src/api/schema.d.ts
pnpm test       # vitest
pnpm lint       # oxlint
pnpm build      # tsc -b && vite build
```

联调前置：后端仓 `HELM_HTTP_ADDR=127.0.0.1:18081 cargo run -p helm-server`
（Postgres `docker compose up -d postgres`）。默认登录 `admin / admin123`（仅开发）。

## 联调验证脚本

```bash
node scripts/verify-m1.mjs        # 登录流（守卫/错误提示/跳转）
node scripts/verify-m1-hosts.mjs  # 主机 CRUD 全流程
node scripts/verify-m2.mjs        # M2：概览/终端（回显+resize）/文件（上传下载校验）/列表导航/离线守卫
node scripts/verify-m3.mjs        # M3：服务（启停+日志实时 tail）/进程 kill/网络/指标（历史+实时 WS）/任务域/终端设置
node scripts/verify-m4.mjs        # M4：仪表盘/通知流+已读/告警/审计/监听器/设置 JWT/forward exec 真执行/404
```
