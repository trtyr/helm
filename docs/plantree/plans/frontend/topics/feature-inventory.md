# 功能清单 — 70 个功能点 + API 全映射

Role: topic-capsule
Status: active
Read when: 设计任何路由/页面时，先来这里确认该页承载哪些功能点（F 编号）；路由规格引用 F 编号
Related: [路由地图](../routes/README.md)、[API 契约](../../../../api.md)、[openapi.yaml](../../../../openapi.yaml)

## One-Screen Summary

后端 44 HTTP 端点 + 4 WS 反推 + 1Panel/Tactical RMM 功能面并集，得出 **70 个功能点**，
按 14 个域组织。每个功能含一句话描述与设计要点（Vercel/Geist 风）。
文末映射表保证后端每个端点至少被一个功能消费——可逐行核对无遗漏。

## 域与功能点

### A. 认证与全局（6）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F01 | 登录 | 居中窄卡（360px），logo + 用户名/密码 + 错误行内提示；无注册入口 | POST /auth/login |
| F02 | 登出 | 顶栏用户菜单内项；清 token 回登录页 | 前端状态 |
| F03 | 会话过期拦截 | 401 统一拦截 → toast「登录已过期」→ 跳登录（保留回跳地址） | 全局 fetch 层 |
| F04 | 主题切换 | 顶栏日/月图标切换，localStorage 持久化，默认暗色 | 前端 |
| F05 | 全局搜索 ⌘K | Topbar 命令面板：搜主机名/路由，键盘上下选择 | GET /hosts（本地过滤）|
| F06 | WS 连接状态 | StatusBar 常驻：绿点已连/红点重连中；断线时全局横幅 | 4 条 WS 任一心跳 |

### B. 仪表盘（5）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F07 | 概览统计卡 | 4 联卡：主机总数/在线/未读通知/今日告警；数字 heading-32 mono | GET /hosts + /notifications/unread-count + /alerts |
| F08 | 在线率环形图 | 单 SVG 环 + 中心百分比；灰阶底 + green 填充 | GET /hosts 聚合 |
| F09 | 最近通知 | 8 行迷你列表（kind 圆点 + message + 相对时间），点击进通知中心 | GET /notifications?limit=8 |
| F10 | 最近告警 | 5 行（metric_name + value 红字 mono + 相对时间），点击进告警页 | GET /alerts?limit=5 |
| F11 | 全局实时 CPU | WS 推动的迷你 sparkline（所有主机 cpu.usage 均值，60 点窗口） | WS /metrics/stream |

### C. 主机管理（7）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F12 | 主机列表 | 表格：状态点/主机名/SN 标签/系统/连接模式/addr/最后心跳；行高 40 | GET /hosts |
| F13 | 标签过滤 | 列表上方标签 chip 组（多选 AND），「全部」默认 | GET /hosts?tag= |
| F14 | 在线状态徽标 | 在线绿实心/离线灰空心/stale 黄半透明 + 20px 圆点 + tooltip | hosts.online/stale |
| F15 | 创建主机 | 右抽屉表单：hostname/conn_mode 分段器/addr（forward 时显示）/tags | POST /hosts |
| F16 | 编辑主机 | 同创建，预填；conn_mode 变更提示重连语义 | PUT /hosts/{id} |
| F17 | 删除主机 | 行内菜单 → 模态确认（红 danger 按钮，输入 hostname 二次确认） | DELETE /hosts/{id} |
| F18 | 设置标签 | 详情页标签编辑器：chip 增删 + 回车添加 | POST /hosts/{id}/tags |

### D. Agent 管理（5）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F20 | Agent 列表 | 表格：agent_id(mono)/版本/注册时间/最后心跳/在线/所属主机 | GET /agents |
| F21 | Agent 详情 | 右抽屉：基本信息 dl 列表 + 在线状态大徽标 + 主机链接 | GET /agents/{id} |
| F22 | Agent 注销 | danger 菜单项 → 确认模态（提示孤儿主机软删行为） | DELETE /agents/{id} |
| F23 | Agent 卸载 | 注销增强选项：勾选「同时移除二进制与自启」→ 状态流转提示 | POST /agents/{id}/uninstall |
| F24 | Agent 标签同步 | Agent 详情内改标签 = 改关联主机标签（同 F18 交互） | PUT /agents/{id}/tags |

### E. 命令执行与任务（7）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F25 | 快速执行 | 主机详情顶部：命令输入（mono）+ args + 运行按钮 → 建 job 跳转 | POST /exec |
| F26 | Job 列表 | 表格：job_id(mono 短)/主机/命令/状态色字/退出码/时间；状态过滤 chips | GET /jobs |
| F27 | Job 详情 | 上：元信息 dl；下：输出终端块（mono，stdout 原色/错误红）；复制按钮 | GET /jobs/{id} |
| F28 | Job 实时输出 | 详情页 running 时自动接 WS，增量 append + 自动滚底（手动滚离则停） | WS /jobs/{id}/stream |
| F29 | 脚本任务 | 快速执行的别名入口（表单一致）；历史入 Job 列表 | POST /tasks/script |
| F30 | 定时任务 | 表单：命令 + interval_secs + 说明「Server 重启自动恢复」 | POST /tasks/schedule |
| F31 | 定时任务视图 | 主机详情「任务」页签：tasks 表列出 kind/params/超时 + 关联 jobs | GET /jobs（task 维度分组） |

### F. 交互终端（3）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F32 | Web 终端 | xterm.js 全宽面板：默认 shell、黑底（独立于主题）、连接状态角标 | WS /agents/{id}/terminal |
| F33 | 终端多开 | 页签条 + 号（1、2、3…），每 tab 独立 session；空闲超时倒计时提示 | 同上（多 session） |
| F34 | 终端设置 | 面板内浮层：字号 12-18/主题（跟随系统 or 固定）；resize 双向同步 | 前端 + SessionResize |

### G. 文件管理（5）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F35 | 文件浏览器 | 单栏表格 + 面包屑路径（mono）；目录/文件图标区分；双击进入 | POST /files/list |
| F36 | 文件上传 | 工具栏上传按钮 → 选本地文件 + 目标路径确认 → 进度条 + sha256 结果徽标 | POST /files/upload |
| F37 | 文件下载 | 行内菜单「下载」→ 保存路径确认 → 进度 + 校验结果 | POST /files/download |
| F38 | 文件信息 | 列表列：名称/目录/大小(人类可读)/权限 mode(mono)/修改时间 | FileEntry |
| F39 | 排序与刷新 | 表头排序（名称/大小/时间）；刷新按钮 + 最后浏览时间显示 | 前端 |

### H. 服务管理（6）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F40 | 服务列表 | 表格：名称/命令(mono 截断)/状态徽标(running 绿/stopped 灰/failed 红)/PID/重启策略 | GET /services |
| F41 | 创建服务 | 抽屉表单：选主机 + 名称 + command + args 行编辑 + restart_policy 分段 | POST /services |
| F42 | 启停控制 | 行内三按钮 启动/停止/重启；running 需先停的删除守卫提示 | POST /services/{id}/start\|stop\|restart |
| F43 | 状态实时化 | 列表页订阅在线主机服务状态推送（徽标即时翻转 + 行闪一下 gray-200） | ServiceStatus 推送 |
| F44 | 日志快照 | 详情抽屉：日志区块 mono 滚动区 + 复制；「查看实时」入口 | GET /services/{id}/logs |
| F45 | 日志实时 tail | 抽屉内 WS 模式：增量 append + 自动滚底 + 暂停/继续 + 已收行数 | WS /services/{id}/logs/stream |

### I. 进程与网络（4）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F46 | 进程列表 | 表格：PID(mono)/名称/CPU%/内存（可读）；默认 CPU 降序；刷新间隔可调 | POST /processes/list |
| F47 | 进程搜索 | 顶部过滤框（名称/PID 即时过滤，前端本地） | 前端 |
| F48 | 杀进程 | 行内 danger 菜单 → 确认模态（展示 PID+名称）→ 结果 toast（ok=false 也 200 语义说明） | POST /processes/kill |
| F49 | 网络信息 | 详情页网络 tab：hostname + 接口卡片组（接口名 + addrs mono chip 列表） | POST /net/info |

### J. 监控（4）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F50 | 指标图表组 | 2×2 卡片：CPU/内存/磁盘/网络；折线 + 面积淡填充，蓝色单系列 | GET /metrics?host_id= |
| F51 | 实时追加点 | 打开页面即接 WS：新点右进左出（60 点窗），无动画直接刷新 | WS /metrics/stream |
| F52 | 时间范围 | segmented：实时/1h/6h/24h（映射 limit 参数）；mono 数字 tabular | limit 参数 |
| F53 | 指标切换 | 折叠区展开次要指标（net.rx/tx、proc.count 迷你 sparkline） | 同 F50 |

### K. 告警（3）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F54 | 告警列表 | 表格：主机/指标(mono)/阈值→实测(mono 红字)/级别/时间；分页 | GET /alerts |
| F55 | 阈值说明 | 列表头部 info 条：当前阈值硬编码（cpu/mem/disk > 90%）提示 | 前端静态 |
| F56 | 主机跳转 | 行点击 → 主机详情指标页（带高亮该指标） | 路由 |

### L. 通知中心（5）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F57 | 铃铛 + 角标 | Topbar 铃铛；unread-count 轮询 30s + WS 即时刷新；≥99 显示 99+ | GET /notifications/unread-count |
| F58 | 下拉小卡片 | 铃铛点击下拉（menu 材质 12px 圆角）：最近 10 条（kind 点 + 消息 + 相对时间）+ 「查看全部」 | GET /notifications?limit=10 |
| F59 | 通知中心页 | 全页列表：kind 筛选 chips（全部/上线/下线/预警）+ 未读开关 + 分页 | GET /notifications?unread= |
| F60 | 标记已读 | 行 hover 显「标为已读」；列表头「全部已读」按钮（confirm 后 updated 数 toast） | POST /{id}/read、/read-all |
| F61 | 实时推送 | WS 新通知 → 铃铛角标 +1 脉冲动画 + 右上 toast（含消息文案，4s 消失） | WS /notifications/stream |

### M. 审计（2）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F62 | 审计列表 | 表格：时间(mono)/操作者/动作(snake 徽标)/资源；时间倒序分页 | GET /audit |
| F63 | 详情展开 | 行展开：detail JSON 树视图（mono，可折叠键） | AuditLog.detail |

### N. 监听器（5）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F64 | 监听器列表 | 卡片网格：名称/addr(mono)/协议/状态徽标/专用 token 掩码显示 | GET /listeners |
| F65 | 创建监听器 | 模态：名称/addr（默认端口提示）/proto（仅 grpc）/auth token（留空回退全局） | POST /listeners |
| F66 | 启停 | 卡片主按钮 启动/停止（状态翻转即时） | POST /{id}/start\|stop |
| F67 | 编辑删除 | 卡片菜单；删除需确认 | PUT/DELETE /listeners/{id} |
| F68 | 状态徽标 | running 绿点脉动/stopped 灰点 | listener.status |

### O. 设置（2）

| ID | 功能 | 设计要点 | 数据来源 |
|----|------|---------|---------|
| F69 | 系统信息 | 版本/后端地址/心跳超时/会话超时/JWT 过期时间（token 解码 exp） | config + token decode |
| F70 | 正向快捷执行 | 独立小页：hostname 或 agent_addr + 命令 → 同步结果块（output + exit_code） | POST /forward/exec |

（F19 编号废除——拨号测试无对应 API；F70 顶替编号保证连续可读。）

## API → 功能映射核对表

44 HTTP 端点 + 4 WS，每个至少一个功能消费：

| # | 端点/流 | 消费功能 |
|---|---------|---------|
| 1 | POST /auth/login | F01 |
| 2 | POST /agents/cert | Agent 自用（前端无 UI；设置页 F69 文档化说明） |
| 3 | GET /hosts | F05, F07, F08, F12, F13 |
| 4 | POST /hosts | F15 |
| 5 | PUT /hosts/{id} | F16 |
| 6 | DELETE /hosts/{id} | F17 |
| 7 | POST /hosts/{id}/tags | F18 |
| 8 | GET /agents | F20 |
| 9 | GET /agents/{id} | F21 |
| 10 | DELETE /agents/{id} | F22 |
| 11 | PUT /agents/{id}/tags | F24 |
| 12 | POST /agents/{id}/uninstall | F23 |
| 13 | POST /exec | F25 |
| 14 | GET /jobs | F26, F31 |
| 15 | GET /jobs/{id} | F27 |
| 16 | GET /metrics | F50, F53 |
| 17 | GET /alerts | F07, F10, F54 |
| 18 | POST /files/upload | F36 |
| 19 | POST /files/download | F37 |
| 20 | POST /files/list | F35 |
| 21 | POST /tasks/script | F29 |
| 22 | POST /tasks/schedule | F30 |
| 23 | POST /forward/exec | F70 |
| 24 | GET /listeners | F64 |
| 25 | POST /listeners | F65 |
| 26 | PUT /listeners/{id} | F67 |
| 27 | DELETE /listeners/{id} | F67 |
| 28 | POST /listeners/{id}/start | F66 |
| 29 | POST /listeners/{id}/stop | F66 |
| 30 | GET /services | F40 |
| 31 | POST /services | F41 |
| 32 | PUT /services/{id} | F41（编辑态） |
| 33 | DELETE /services/{id} | F42（菜单项） |
| 34 | POST /services/{id}/start | F42 |
| 35 | POST /services/{id}/stop | F42 |
| 36 | POST /services/{id}/restart | F42 |
| 37 | GET /services/{id}/logs | F44 |
| 38 | POST /processes/list | F46 |
| 39 | POST /processes/kill | F48 |
| 40 | POST /net/info | F49 |
| 41 | GET /audit | F62, F63 |
| 42 | GET /notifications | F09, F58, F59 |
| 43 | GET /notifications/unread-count | F07, F57 |
| 44a | POST /notifications/{id}/read | F60 |
| 44b | POST /notifications/read-all | F60 |
| WS1 | /agents/{id}/terminal | F32, F33, F34 |
| WS2 | /services/{id}/logs/stream | F45 |
| WS3 | /jobs/{id}/stream | F28 |
| WS4 | /metrics/stream | F11, F51 |
| WS5 | /notifications/stream | F57, F61 |

> WS 共 5 条（metrics/jobs/services/terminal/notifications），44 HTTP 端点计数含
> read-all 与 read 拆分。映射表覆盖全部后端接口面，无孤儿端点。
