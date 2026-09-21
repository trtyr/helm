// 自启动页的共享类型与常量（G13 拆分，2026-09-21）——自 `Autostart.tsx` 拆出。
import type { components } from "../../../api/schema";

export type Agent = components["schemas"]["Agent"];
export type IrFinding = components["schemas"]["IrFinding"];

export interface Ctx {
  host: components["schemas"]["HostView"];
}

export interface DiffState {
  added: IrFinding[];
  removed: IrFinding[];
  baseLabel: string;
}

export const SEV_CLS: Record<string, string> = {
  critical: "bg-red-1000/10 text-red-1000",
  warn: "bg-amber-1000/10 text-amber-1000",
  info: "bg-gray-200 text-gray-900",
};
export const SEV_LABEL: Record<string, string> = { critical: "严重", warn: "可疑", info: "信息" };
export const SEV_ORDER = (s: string) => (s === "critical" ? 0 : s === "warn" ? 1 : 2);

/** Autoruns 分类标签（顺序即展示顺序）。 */
export const CATEGORIES = [
  "登录",
  "服务",
  "驱动",
  "计划任务",
  "WMI 订阅",
  "浏览器",
  "外壳",
  "映像劫持",
  "认证",
  "引导执行",
  "已知 DLL",
  "Winsock",
  "Office",
  "编解码器",
] as const;

export const SIGN_CLS: Record<string, string> = {
  verified: "bg-green-1000/10 text-green-1000",
  unsigned: "bg-amber-1000/10 text-amber-1000",
  invalid: "bg-red-1000/10 text-red-1000",
  unknown: "bg-gray-200 text-gray-900",
};
export const SIGN_LABEL: Record<string, string> = {
  verified: "已验证",
  unsigned: "未签名",
  invalid: "无效",
  unknown: "未知",
};

export function fmtTime(unix?: string | null): string {
  if (!unix) return "—";
  const n = Number(unix);
  if (!Number.isFinite(n) || n <= 0) return "—";
  const d = new Date(n * 1000);
  const pad = (x: number) => String(x).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
