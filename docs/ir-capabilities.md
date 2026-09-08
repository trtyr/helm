# IR 应急响应能力 — 完整技术文档

> 本文档覆盖 helm 平台的 **IR（Incident Response）应急响应** 与 **P2 体系化运维** 全部能力，
> 包括每个功能的技术实现、API 端点、前端交互与实测验证结果。
> 代码位于 `agent/src/ir/`（agent 端采集/操作）、`server/src/http/ir.rs + ir_ops.rs + p2.rs`（服务端）。

---

## 目录

1. [自启动项全景扫描](#1-自启动项全景扫描)
2. [自启动项操作（禁用/启用/删除）](#2-自启动项操作)
3. [基线快照对比](#3-基线快照对比)
4. [进程列表与进程树](#4-进程列表与进程树)
5. [进程杀（SeDebugPrivilege）](#5-进程杀)
6. [系统日志（安全事件）](#6-系统日志)
7. [流式内存扫描](#7-流式内存扫描)
8. [NTFS USN 文件时间线](#8-ntfs-usn-文件时间线)
9. [证据包一键收集](#9-证据包一键收集)
10. [批量命令下发](#10-批量命令下发)
11. [权限检测与徽章](#11-权限检测与徽章)
12. [下线与离线挂起补执行](#12-下线与离线挂起补执行)
13. [页面缓存](#13-页面缓存)
14. [VirusTotal 接口](#14-virustotal-接口)
15. [API 端点总表](#15-api-端点总表)
16. [实测验证结果](#16-实测验证结果)
17. [已知限制](#17-已知限制)

---

## 1. 自启动项全景扫描

对标 Sysinternals **Autoruns**，对目标 Windows 主机做持久化机制全量枚举。

### 覆盖范围（12 分类）

| 分类 | 扫描位置 | 典型条目数 | 说明 |
|------|---------|-----------|------|
| 登录 | HKLM/HKCU Run·RunOnce·RunOnceEx、策略 Run、HKU 多用户、Active Setup StubPath、GPExtensions、启动文件夹（含 AutorunsDisabled 禁用区） | ~107 | 传统"启动项" |
| 服务 | `HKLM\SYSTEM\CCS\Services` 全量（Type 过滤内核驱动），svchost 组服务自动解析 `Parameters\ServiceDll` | ~311 | 含描述和 DisplayName |
| 驱动 | 同上，`Type & 0x3` 过滤内核/文件系统驱动 | ~413 | 数量最大 |
| 计划任务 | `schtasks /query /fo csv /v`（兼容中英文表头） | ~163~229 | 含禁用状态检测 |
| 外壳 | ShellIconOverlayIdentifiers、ShellServiceObjects、ShellExecuteHooks、8 对象根 × 5 处理器类型的 ContextMenu/DragDrop/PropertySheet/Column/CopyHook Handlers | ~122 | 右键菜单、图标覆盖等 |
| 认证 | LSA Authentication/Notification/Security Packages、MSV1_0、SecurityProviders、凭据提供程序、打印监视器 | ~29 | 登录验证链 |
| 浏览器 | Chrome/Edge Extensions（HKLM/HKCU）、BHO、Firefox profiles | ~2 | |
| 映像劫持 | IFEO Debugger、SilentProcessExit、AppInit_DLLs（64/32）、Winlogon Shell/Userinit/GinaDLL 异常 | 0~4 | 正常系统应零发现 |
| WMI 订阅 | `root\subscription` FilterToConsumerBinding（SCM Event Log Filter 白名单排除） | 0~1 | 正常系统几乎为空 |
| 引导执行 | Session Manager BootExecute/SetupExecute/Execute | ~1 | smss 阶段原生镜像 |
| 已知 DLL | `Session Manager\KnownDLLs`（引导期预加载映射进所有进程） | ~32 | 劫持 = 全局注入 |
| Winsock | Protocol_Catalog9 LSP（含 64 位目录）、NameSpace_Catalog5、网络提供程序 | ~14 | LSP 劫持 = 流量拦截 |
| 编解码器 | Drivers32（vidc/msacm/wave/midi） | ~42 | 恶意编解码 DLL |
| Office | COM 加载项（Word/Excel/PPT/Outlook/Access）、Word 启动文件夹 WLL、Excel XLL OPEN | 0~N | 需安装 MS Office |

### 每条目字段

```json
{
  "category": "登录",
  "name": "SecurityHealth",
  "detail": "[HKLM\\SOFTWARE\\...] C:\\Windows\\system32\\SecurityHealthSystray.exe",
  "severity": "info | warn | critical",
  "path": "C:\\Windows\\system32\\SecurityHealthSystray.exe",
  "publisher": "Microsoft Corporation",
  "signState": "verified | unsigned | invalid | unknown",
  "desc": "服务 DisplayName 或 CLSID 名称",
  "opKey": "reg\x1fHKLM\x1fSOFTWARE\\...\\Run\x1fSecurityHealth",
  "disabled": false,
  "mtime": "1788839711"
}
```

### 签名校验

每个条目关联的可执行文件自动进行 **Authenticode 双链路校验**：

1. **嵌入式签名**：`WinVerifyTrust(WINTRUST_ACTION_GENERIC_VERIFY_V2)` 验证 PE 内嵌签名
2. **目录签名回退**：嵌入式失败时走 `CryptCATAdminAcquireContext` → `CryptCATAdminEnumCatalogFromHash` → 逐目录验证（DRIVER_ACTION_VERIFY + GENERIC_VERIFY_V2 双策略）
3. 部分系统目录签名条目需 **DRIVER_ACTION_VERIFY** 策略（实测 alg.exe/msdtc.exe 等）
4. 文件不可读返回 `CRYPT_E_FILE_ERROR(0x80092003)` → 归为 `unknown` 而非 `invalid`

**Publisher 来源**：优先取证书主体（`CryptQueryObject` → `CertGetNameStringW(CERT_NAME_SIMPLE_DISPLAY_TYPE)`），回退版本资源 CompanyName。

### 路径归一化

注册表中存储的可执行路径格式多样，统一归一化（`resolve_pe_path`）：
- `%SystemRoot%\System32\` → `C:\Windows\System32\`
- `\??\` 前缀剥离
- 裸 DLL 名按 System32 → drivers → Windows 顺序探测
- 相对路径 `System32\drivers\x.sys` → `C:\Windows\System32\drivers\x.sys`

### 严重级别启发式

| 条件 | 级别 |
|------|------|
| 可疑目录（Temp/Public/PerfLogs/ProgramData 非 Microsoft） | critical |
| 签名无效 | critical |
| 文件缺失 | warn |
| 目录外未签名 | warn |
| 其余 | info |

---

## 2. 自启动项操作

对标 Autoruns 的 **AutorunsDisabled 机制**——禁用不是删除，是"移到 Windows 不会读的位置"。

### 操作地址（opKey）格式

以 `\x1f`（Unit Separator）分隔的四段式地址，由扫描自动生成：

| 类型 | 格式 | 禁用方式 |
|------|------|---------|
| 注册表值 | `reg\x1f{hive}\x1f{subkey}\x1f{value}` | 值移入同级 `AutorunsDisabled` 子键 |
| 文件 | `file\x1f{绝对路径}` | 移入同级 `AutorunsDisabled` 子目录 |
| 服务/驱动 | `svc\x1f{服务名}` | 原始 Start 存入 `AutorunsDisabled` 值 + Start 置 4 |
| 计划任务 | `task\x1f{任务名}` | `schtasks /change /disable` |

hive 取值：`HKLM` | `HKCU` | `HKU:{SID}`（其他用户）

### API

```
POST /api/v1/ir/autorun-action
{ "agent_id": "...", "action": "disable | enable | delete", "key": "<opKey>" }
```

### 验证结果

| 操作 | 系统状态变化 |
|------|-------------|
| 禁用注册表值 | `Run\HelmTest` 消失 → `Run\AutorunsDisabled\HelmTest` 出现 |
| 启用注册表值 | 原位还原，禁用区清空 |
| 删除注册表值 | 彻底消失 |
| 禁用服务 | `Start=0x4` + 原值 `0x3` 存入 `AutorunsDisabled` |
| 启用服务 | Start 还原 `0x3`，标记清除 |
| 删除服务 | SCM `DeleteService` → 1060（不存在） |
| 禁用计划任务 | 状态变为"已禁用" |
| 启用计划任务 | 状态还原为"就绪" |
| 删除计划任务 | schtasks 确认不存在 |

---

## 3. 基线快照对比

保存当前自启动项扫描为基线快照，后续扫描与基线 diff 出**新增持久化**（新增恶意自启动）和**移除**（被删的正常条目）。

### 存储

```sql
CREATE TABLE ir_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    findings JSONB NOT NULL DEFAULT '[]',
    entry_count INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### 条目身份键

- 优先：`opKey`（操作地址唯一）
- 回退：`category + name + path` 拼接

### API

| 端点 | 方法 | 说明 |
|------|------|------|
| `/ir/snapshots` | POST | 现场扫描并保存为基线 |
| `/ir/snapshots?agent_id=` | GET | 快照列表 |
| `/ir/snapshots/{id}` | GET | 快照详情（含 findings 全文） |
| `/ir/snapshots/{id}` | DELETE | 删除快照 |
| `/ir/snapshots/compare` | POST | 对比（`base_id` + `target_id` 或 `agent_id` 现场重扫） |

### 实测

- 保存基线（1247 条）→ 新建 Run 值 → 对比 → `added=[HelmIrNew]` ✓
- 删除 Run 值 → 对比 → `removed=[HelmIrNew]` ✓
- 两份快照互比 → 0 差异 ✓

---

## 4. 进程列表与进程树

### 进程列表

对标 Process Hacker：每个进程含 PID、名称、CPU%、物理内存、虚拟内存、状态、属主、父进程 PID、已运行时间、启动时间、可执行路径、完整命令行。

- 数据源：sysinfo 常驻快照 + 周期刷新（CPU 为两次刷新差值的瞬时值）
- 距上次刷新 >3s 自动重建 300ms 短测量窗口，避免闲置后首次显示长平均值
- 提权（SeDebugPrivilege）后可读 SYSTEM 进程的路径/命令行/属主

### 进程树

按 `parent_pid` 构建父子树，支持折叠/展开/全部展开/全部折叠。

**异常父子检测规则**（行内 ⚠ 标记 + 详情抽屉红条）：

| 规则 | 条件 | 含义 |
|------|------|------|
| 办公/浏览器派生解释器 | 父 = Word/Excel/PPT/Outlook/Chrome/Edge/Firefox/PDF，子 = cmd/powershell/mshta/rundll32 等 | 宏攻击或网页挂马典型链 |
| LSASS 派生进程 | 父 = lsass.exe | 凭据窃取典型行为 |
| SMSS 异常子进程 | 父 = smss.exe，子 ∉ {csrss, wininit, smss} | |
| 服务管理器派生解释器 | 父 = services.exe，子 = cmd/powershell 等 | |

### 实测

- 313 行渲染、13 根节点、最大 9 层缩进
- 折叠/展开/全部展开/全部折叠交互正常
- 树模式搜索保留命中节点的祖先链

---

## 5. 进程杀（SeDebugPrivilege）

Windows 管理员令牌默认持有但**不激活** SeDebugPrivilege，不显式启用则无法终止 SYSTEM 进程。

实现：扫描前调用 `AdjustTokenPrivileges` 显式启用（`privilege.rs`）。

```
POST /api/v1/processes/kill
{ "agent_id": "...", "pid": 1234 }
```

实测：可杀 SYSTEM 进程（svchost 子进程等），非提权时返回权限不足。

---

## 6. 系统日志（安全事件）

提权后 Windows Security 日志可读，提取高价值事件：

| 事件 ID | 含义 | 级别 |
|---------|------|------|
| 4624 | 登录成功 | info |
| 4625 | 登录失败 | info |
| 4720 | 创建用户账户 | warn |
| 1102 | 审计日志清除 | **critical** |
| 7045 | 安装新服务 | warn |
| 4104 | PowerShell 脚本块 | info |

实测：提权后 60 条事件（40 条 4624 + 20 条 7045）。

---

## 7. 流式内存扫描

对标 Volatility strings：读取目标进程内存区域，提取可打印字符串（ASCII + UTF-16LE）。

### 架构

- **全进程模式**（pid=0）：枚举全部进程，逐一扫描，WebSocket 逐进程推送增量批次
- **单进程模式**：定向扫描单个 PID
- 提权下 SeDebugPrivilege 启用后可读 SYSTEM 进程（lsass.exe 等）

### 关键参数

| 参数 | 默认值 | 说明 |
|------|--------|------|
| min_len | 6 | 字符串最小长度 |
| keyword | 空 | 过滤关键词（空 = 不过滤） |
| MAX_ENTRIES | 5000 | 全局命中上限（配额满后停止收集但继续统计覆盖率） |
| PER_PROCESS_CAP | 100 | 单进程最多贡献条数 |
| 无截止时间 | — | 扫完为止，前端实时可见进度 |

### 实测（提权 + 全进程）

- **290/309 进程**成功扫描（94% 覆盖率，剩余为受保护进程）
- **52.7 GB** 内存扫描
- 5000 条命中（达上限截断）
- 每条带 `[PID 进程名]` 标注

### API

```
POST /api/v1/ir/memscan/stream   ← 启动（返回 scanId）
WS   /api/v1/ir/memscan/{scanId}/stream?token=  ← 订阅增量
POST /api/v1/ir/memscan           ← 非流式（一次性返回，兼容保留）
```

---

## 8. NTFS USN 文件时间线

读取 NTFS 卷的 USN 变更日志（`FSCTL_READ_USN_JOURNAL`），获取文件创建/删除/重命名/数据修改活动。每条含文件名、时间戳（FILETIME → unix 秒）和原因码。

### 已知限制

- 部分 Windows 版本的 journal 记录 TimeStamp 字段填 0（系统行为）——时间过滤不可用
- 解决方案（已实施）：按 **Reason 码过滤**高价值操作（file_create/file_delete/rename/data_overwrite），丢弃噪声 close 事件——即使无时间戳，操作类型本身就是信号
- 需要**管理员权限**打开卷设备（`\\.\C:`）
- journal 大小有限（默认 32MB），可能已旋转覆盖早期记录

---

## 9. 证据包一键收集

一键收集目标主机的完整应急取证数据，输出 JSON 附件下载。

### 包含内容

| 数据 | 来源 |
|------|------|
| 进程列表（全量含 SYSTEM） | `svc.list()` |
| 网络接口 + TCP/UDP 连接 | `svc.net()` |
| 系统服务全量 | `svc.sys_services()` |
| 自启动项全景 + 映像劫持 | ir_scan(types=["autostart","registry"]) |
| 安全事件 | ir_scan(types=["events"]) |
| 可疑落地文件 | ir_scan(types=["suspicious_files"]) |
| 账户审计 | ir_scan(types=["accounts"]) |
| Agent 元数据 | agents 表 |

### API

```
POST /api/v1/ir/evidence
{ "agent_id": "..." }
→ Content-Disposition: attachment; filename="evidence-{hostname}-{ts}.json"
```

---

## 10. 批量命令下发

对多台主机同时下发同一条命令，逐 agent 建立 job（复用现有 job 基础设施）。

```
POST /api/v1/exec/batch
{ "agent_ids": ["agent-a", "agent-b"], "command": "cmd", "args": ["/c", "tasklist"] }
→ { "total": 2, "jobs": [{ "agent_id": "agent-a", "job_id": "..." }, ...] }
```

离线 agent 不报错，返回 `error` 字段标注。任务进度统一在任务页跟踪。

---

## 11. 权限检测与徽章

Agent 注册时自动检测并上报运行权限。

### Windows

`CheckTokenMembership(Administrators SID S-1-5-32-544)` —— 检查当前令牌是否属于管理员组。

### Unix

读取 `/proc/self/status` 的 Uid 行，有效 UID = 0 即 root。

### 存储

`agents.elevated BOOLEAN` 列，注册时写入，API 列表和主机详情均返回。

### 前端

- 主机列表：agent 行旁绿色"管理员" / 灰色"普通"徽章
- 主机详情头部：同款徽章（悬停显示权限说明）

---

## 12. 下线与离线挂起补执行

### 在线卸载（即时下线）

```
POST /api/v1/agents/{id}/uninstall { "remove_binary": false }
→ SelfDestruct 指令送达 → agent exit(0) → 进程消失 → 档案删除
```

`remove_binary=false` 保留二进制文件（可重新上线）。

### 离线卸载（挂起补执行）

Agent 已断连时无法送达 SelfDestruct → 进程残留。解决方案：

```sql
CREATE TABLE agent_pending_offline (
    agent_id TEXT PRIMARY KEY,
    action TEXT NOT NULL,  -- "uninstall" | "deregister"
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Agent 重连注册瞬间，server 检查挂起表 → 立即补送 SelfDestruct → 进程退出 → 档案删除。**实测通过**。

---

## 13. 页面缓存

自启动项和系统日志页面每次打开需要 10~25 秒（含签名校验）。通过服务端缓存（`ir_page_cache` 表）实现秒开：

- 扫描完成时自动写入缓存（agent_id + types 组合为 key）
- 页面打开时先读缓存渲染（标注"缓存于 X 时间"）
- 用户点"重新扫描"才真正触发 agent 重扫
- 无缓存时自动扫一次

---

## 14. VirusTotal 接口

按文件 SHA256 查询 VT 检出率，结果缓存 7 天（`ir_vt_cache` 表）。

```
POST /api/v1/ir/vt
{ "sha256": "da5807bb..." }
→ { "positives": 3, "total": 70, "permalink": "https://www.virustotal.com/gui/file/...", "cached": false }
```

- 需配置 `HELM_VT_API_KEY` 环境变量
- 未配置时返回明确提示
- VT 未收录的文件 positives = -1
- 公共 API 限速 4 次/分钟，超限返回提示

---

## 15. API 端点总表

IR 相关端点共 16 个（全部在 `/api/v1` 下，需 JWT 认证）：

| 端点 | 方法 | 说明 |
|------|------|------|
| `/ir/scan` | POST | 应急扫描（types: autostart/registry/events/suspicious_files/accounts） |
| `/ir/autorun-action` | POST | 自启动项操作（disable/enable/delete） |
| `/ir/memscan` | POST | 内存扫描（非流式） |
| `/ir/memscan/stream` | POST | 启动流式内存扫描 |
| `/ir/memscan/{id}/stream` | GET | 内存扫描 WS 流 |
| `/ir/file-meta` | POST | 文件元数据（SHA256/大小/mtime） |
| `/ir/vt` | POST | VirusTotal 查询 |
| `/ir/evidence` | POST | 证据包下载 |
| `/ir/snapshots` | POST/GET | 快照保存/列表 |
| `/ir/snapshots/compare` | POST | 基线对比 |
| `/ir/snapshots/{id}` | GET/DELETE | 快照详情/删除 |
| `/ir/fs-timeline` | POST | USN 文件时间线 |
| `/ir/cache` | GET | 页面缓存查询 |

完整契约见 `docs/openapi.yaml`（72 端点）。

---

## 16. 实测验证结果

在 Windows 10 提权环境实测（DESKTOP-3M7DKO9，2026-09-07~08）：

### 功能测试（35+ 项全 PASS）

| 类别 | 测试项 | 结果 |
|------|--------|------|
| 主机 | 在线状态/管理员徽章/IP 上报 | ✓ |
| 进程 | 316 个、SYSTEM 内存可见、CPU 瞬时值、属主覆盖 301/316 | ✓ |
| 进程树 | 313 行 13 根 9 层缩进、折叠/展开/搜索/祖先保留 | ✓ |
| 进程杀 | 启动 notepad → 列表可见 → 杀 → 消失 | ✓ |
| 自启动 | 1307 条 12 分类、SYSTEM 服务厂商、操作闭环×3类 | ✓ |
| 禁用/启用/删除 | 注册表值、文件、服务、任务（全部注册表/文件系统取证） | ✓ |
| 基线对比 | 新增/移除双向 diff、快照互比 | ✓ |
| 缓存 | 秒开（autostart 644ms / syslog 464ms） | ✓ |
| 系统日志 | 60 条（40×4624 + 20×7045） | ✓ |
| 流式扫描 | 全进程 290/309、52.7GB、5000 命中 | ✓ |
| 批量命令 | 3 台含离线 agent 错误处理 | ✓ |
| 证据包 | 8 数据源 JSON 下载 | ✓ |
| USN 时间线 | V2 记录、Reason 码、关键词过滤 | ✓ |
| SHA256 | 与 Python hashlib 精确对拍 | ✓ |
| VT | 无 key 优雅报错 | ✓ |
| 权限 | 管理员徽章 + SeDebug 启用确认 | ✓ |
| 下线 | 列表消失 + 进程消失 + 二进制保留（两次复验） | ✓ |
| 离线挂起 | 标记 → 重连补执行 → 档案注销 | ✓ |

### 门禁

| 检查 | 结果 |
|------|------|
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 0 错误 |
| `cargo test --workspace` | ✅ 全绿（偶发 DB 竞争不影响） |
| `cargo check --target x86_64-unknown-linux-musl` | ✅ 交叉编译通过 |
| `vitest` | ✅ 52/52 |
| `tsc --noEmit` | ✅ 0 错误 |
| `check_openapi.py` | ✅ 72 端点一致 |

---

## 17. 已知限制

| 限制 | 说明 | 状态 |
|------|------|------|
| USN TimeStamp | 部分 Windows 版本 journal 记录 TimeStamp=0，时间过滤不可用 | Reason 过滤已补，时间精确过滤需后续方案 |
| 服务 start 500 | 伪服务（notepad.exe binPath）start 会 500 而非优雅返回 | 低优先级，正常服务操作正常 |
| VT 正向查询 | 需配置 HELM_VT_API_KEY | 功能就绪，等用户配置 |
| 离线 agent USN | 非提权 agent 打开卷设备失败 | 提权 agent 正常 |
| Linux IR | agent 端返回"仅支持 Windows" | Linux 应急需后续开发 |

---

## 前端页面索引

| 标签页 | 路由 | 功能 |
|--------|------|------|
| 概览 | /hosts/:id/overview | 快捷入口、系统负载、Agent 信息、导出证据包 |
| 终端 | /hosts/:id/terminal | 交互式终端 |
| 文件 | /hosts/:id/files | 文件浏览器（列/上传/下载/删除） |
| 服务 | /hosts/:id/services | 常驻服务管理 |
| 进程 | /hosts/:id/processes | 进程树/平铺 + 异常父子 + 杀进程 |
| 网络 | /hosts/:id/network | 网卡 + TCP/UDP 连接 |
| 自启动项 | /hosts/:id/autostart | Autoruns 全景 + 禁用/启用/删除 + 基线对比 + 缓存 |
| 系统日志 | /hosts/:id/syslog | 安全事件 + 缓存 |
| 内存扫描 | /hosts/:id/memscan | 流式全进程/单 PID 内存字符串扫描 |
| 代理 | /hosts/:id/proxy | SOCKS5 代理管理 |
| 任务 | /hosts/:id/tasks | 任务跟踪 |
