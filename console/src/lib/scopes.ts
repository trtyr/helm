/** API key scope 清单（单一事实源：server application::scopes）。
 *  ApiKeysPanel（签发勾选）与 MCP 页（scope 表）共用。 */
export type ApiKeyScope = string;

export const SCOPES: { id: ApiKeyScope; label: string }[] = [
  { id: "hosts", label: "主机 / Agent 档案" },
  { id: "exec", label: "命令执行 / 终端" },
  { id: "files", label: "文件传输" },
  { id: "services", label: "服务管理" },
  { id: "processes", label: "进程 / 网络" },
  { id: "metrics", label: "指标 / 告警" },
  { id: "notifications", label: "通知" },
  { id: "listeners", label: "监听器" },
  { id: "forward", label: "正向连接" },
  { id: "proxy", label: "SOCKS 代理" },
  { id: "ir", label: "应急响应 IR（Windows）" },
  { id: "agent-gen", label: "Agent 生成" },
  { id: "audit", label: "审计" },
  { id: "skill", label: "技能包" },
];

export function scopeLabel(id: string): string {
  return SCOPES.find((s) => s.id === id)?.label ?? id;
}
