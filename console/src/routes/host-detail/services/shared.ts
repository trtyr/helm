// 服务页的共享类型与纯工具（G13 拆分，2026-09-21）——自 `Services.tsx` 拆出。
import type { components } from "../../../api/schema";

export type HostView = components["schemas"]["HostView"];
export type Agent = components["schemas"]["Agent"];
export type SysService = components["schemas"]["SysService"];

/** 常驻服务（Server 下发的自建服务；字段为后端 ServiceRow 的 snake_case 序列化）。 */
export interface ManagedService {
  id: string;
  host_id: string;
  name: string;
  command: string;
  status: string;
  pid: number | null;
  exit_code: number | null;
}

export interface Ctx {
  host: HostView;
}

export type StatusFilter = "all" | "running" | "stopped" | "failed";

export const STATUS_LABEL: Record<StatusFilter, string> = {
  all: "全部",
  running: "运行中",
  stopped: "已停止",
  failed: "失败",
};

/** 秒数 → 人类可读时长（分钟内精确到分，天内到小时，再往上到天）。 */
export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "—";
  const m = Math.floor(seconds / 60);
  if (m < 1) return "<1 分钟";
  if (m < 60) return `${m} 分钟`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} 小时`;
  const d = Math.floor(h / 24);
  return `${d} 天`;
}

/** 常驻服务状态徽章色（与系统服务页同语义）。 */
export function managedStatusClass(status: string): string {
  if (status === "running") return "text-green-1000";
  if (status === "failed") return "text-red-1000";
  return "text-gray-800";
}
