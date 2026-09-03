# 路由：/hosts/:id/terminal — 交互终端

Role: route-spec
Status: active
Related: [主机详情框架](host-detail.md)、[功能 F32–F34](../topics/feature-inventory.md)、WS /agents/{id}/terminal

## 功能清单

- F32 Web 终端（xterm.js + WS PTY 双向流）
- F33 终端多开会话（页签）
- F34 终端设置（字号/主题/resize）

## 布局与留白

tab 内容区**满高**（终端是全站唯一突破「内容区 32px 底距」的页面：flex 撑满剩余视口，
仅留 StatusBar 上缘）。这是 IDE 级页面，不是文档级页面。

```text
┌ Tab 内容区（满高 flex column）────────────────────────────────────┐
│ ┌ 会话 1 ─┬┬ 会话 2 ─┬┐                    [+ 新会话] [⚙] [⤢]  │ ← 会话条 h-40
│ ├──────────────────────────────────────────────────────────────┤   底 1px 分割
│ │                                                              │
│ │                                                              │
│ │              xterm.js 渲染区（flex:1，黑底 #0d0d0d）          │
│ │              内边距 12px（四边，terminal padding）            │
│ │              ▊ 光标；输出按 PTY 原色（ANSI 16 色）            │
│ │                                                              │
│ │                                                              │
│ ├──────────────────────────────────────────────────────────────┤
│ │ sh · 80×24 · 已连接 4m        ⚠ 空闲 4:32 / 5:00 自动关闭    │ ← 状态条 h-28
│ └──────────────────────────────────────────────────────────────┘ label-12-mono
└──────────────────────────────────────────────────────────────────┘
```

## 组件与细节

**xterm.js 集成要点**：

- 依赖：`@xterm/xterm` + `@xterm/addon-fit`（自适应尺寸）+ `@xterm/addon-web-links`
  （URL 可点击）。不引 weblinks 之外的装饰 addon。
- WS：`ws(s)://<api>/api/v1/agents/{id}/terminal?token=<JWT>`；**binary 模式**——
  浏览端 ArrayBuffer 直传（输入下行 / 输出上行均二进制帧，无 JSON 包装）。
- 打开会话：WS onopen 后由**后端下发 SessionOpened**，前端置「已连接」；
  前端在 URL hash 或本地设置中记 cols/rows 初始值（默认 80×24）。
- resize：`fit()` 计算 → 若 cols/rows 变化 → 发送 SessionResize（容器 ResizeObserver
  驱动，150ms 防抖）。
- 输入：xterm `onData` → WS send（UTF-8 encode）；粘贴走 xterm 默认（`Ctrl+V`
  之外 `⌘V` 显式 paste 事件写入）。
- 关闭：会话 tab 关闭 → 发 SessionClose → WS close；双向关闭语义
  （后端 SessionClosed 到达时前端标记远端已关，禁输入并提示）。

**多开会话（F33）**：

- 会话条：每会话一个 tab（名称 `sh 1` `sh 2`，label-13）；关闭 `×` hover 显示；
  非激活会话 WS 保持连接（切换即时），超过 4 个会话提示「会话较多，注意主机负载」。
- 每会话独立 xterm 实例 + 独立 SessionRegistry 映射（session_id ↔ term）。

**设置浮层（F34）**：

- ⚙ 弹出 240px 浮层：字号（12/14/16/18，默认 14）、行高 1.2 固定、
  主题（跟随系统 / 终端黑 / 终端亮——仅影响 xterm 配色，不动全局主题）；
  设置持久化 localStorage（per-user）。
- ⤢ 全屏：终端区进入浏览器 Fullscreen API；Esc 退出。

**空闲超时（状态条）**：

- 后端空闲超时 300s；前端本地倒计时（无输入输出即计），最后 60s 显示
  ⚠ amber 倒计时；到点后端关流 → 前端会话标记「已超时关闭」，可一键重开。

## 状态矩阵

| 状态 | 表现 |
|------|------|
| 连接中 | 终端区显示「正在连接 session…」mono 灰字，输入禁用 |
| 已连接 | 光标闪烁，状态条绿色「已连接」 |
| 主机离线 | 会话条下方 amber 通栏；重试按钮（主机上线后自动重连一次） |
| WS 断开（非正常） | 终端尾部插入红字 `--- 连接已断开 ---`；[重连] 按钮（新 session_id） |
| 远端关闭（exit） | 尾部 `--- 会话已结束 ---`；tab 标记 ⏹；重开 = 新会话 |
| 空闲超时 | 状态条 ⚠ 倒计时 → 关闭提示（同上） |
| 多 tab 切换 | 非激活会话保持滚动位置与缓冲区（xterm 实例常驻，仅 display:none） |

## 交互细节

- 终端获得焦点时 tab 页签指示条变 blue-1000（提示当前输入焦点在哪会话）。
- 会话缓冲上限 5000 行（xterm scrollback），超出丢弃最旧。
- `Ctrl+L` 等控制字符直传（由远端 shell 处理，前端不拦截）；`Ctrl+C` 复制行为：
  有选区时复制、无选区时透传中断（xterm 默认）。
