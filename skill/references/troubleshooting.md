# 故障排查

## 连接与凭据

| 现象 | 原因 / 处理 |
|------|------------|
| `error: 连接失败 ... Server 未启动？` | HELM_URL 不对或 Server 没起。`python scripts/auth.py status` 看健康。 |
| 401 `unauthorized`（HTTP） | token 无效/过期/已吊销。API key 被吊销需换新（JWT 专属端点重建）；缓存 JWT 坏了可删临时目录 `helm-skill-tokens.json`。 |
| 403 `forbidden`（且当前凭据是 helm_ key） | 用 key 调了 api-keys 管理端点——**设计如此**，key 不可自管；管理 key 需控制台或 JWT。 |
| 409 `not_connected` | 目标 Agent 不在线。`hosts.py list` 看 online/stale；reverse 模式检查 Agent 进程与 gRPC 地址，forward 模式检查监听器/拨号地址。 |
| 400 `invalid_argument` | 请求字段问题，看脚本参数（如 forward/exec 缺 hostname 或 addr）。 |

## 命令执行

- **本机是 Git Bash / MSYS 时**，`/` 开头的参数会被路径改写（`cmd /c ver` 的 `/c` 变成 `C:/`，
  命令跑成交互式 cmd）。规避：用 `tasks.py script --ps/--sh` 封装，或 `MSYS_NO_PATHCONV=1`，
  或在 PowerShell/CMD 里跑脚本。
- **脚本选项必须写在 `--` 之前**：`--` 之后的参数原样传给目标机，本地脚本不再解析
  （`exec.py run --raw -- foo` 正确；`exec.py run agent -- foo --raw` 会把 `--raw` 传给远端）。
- **输出为空但 succeeded**：命令本身无输出；用 `--raw` 或看 `output` 字段。
- **Windows 乱码**：Server 已按 OEM 代码页解码；若仍异常，目标机可用
  `tasks.py script --ps "chcp 65001; ..."` 或改用 PTY 终端（ConPTY 输出 UTF-8）。
- **命令挂住**：exec.py run 的 `--timeout` 只是本地等待；真正的命令超时是 `--timeout-secs`（传给 Agent）。
- **job 一直 running**：Agent 掉线时 job 可能停在 running；用 `exec.py wait --timeout N` 兜底。

## 文件传输

- `local_path` 是 **Server 机器**上的路径——脚本与 Server 不同机时无法传本地文件，
  需先把文件放到 Server 侧（或直接在 Server 上跑脚本）。
- `checksum_ok=false`：传输完成但 sha256 不符，文件不可信，重传。
- Windows 路径在目标机侧（remote_path）用 Windows 形式（`C:\Users\...`）。

## 服务 / 监听器

- 删除服务报错：running 状态需先 `stop`。
- 监听器 `start` 失败：地址被占用或无权限绑定；看 Server 日志。
- forward 主机 `stale=true` 恒定（无心跳），在线与否以持久连接注册为准（`agents.py get` 看 online）。

## 定时任务

- 无列出/删除端点（平台缺口）。误建的定时任务只能直接删库：
  `DELETE FROM tasks WHERE id = '...'`（Server 运行中还需重启以清内存调度）。

## 终端

- `streams.py terminal` 需要 Agent 在线；空闲 300s（`HELM_SESSION_IDLE_TIMEOUT`）服务端主动断，
  WS close reason 为 `idle_timeout`。
- Windows 下终端交互用 msvcrt，功能键部分支持有限；复杂交互建议用控制台 Web 终端。
