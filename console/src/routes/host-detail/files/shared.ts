// 文件浏览页的共享类型与纯工具（G13 拆分，2026-09-21）——自 `Files.tsx` 拆出。
import type { components } from "../../../api/schema";

export type HostView = components["schemas"]["HostView"];
export type Agent = components["schemas"]["Agent"];
export type FileEntry = components["schemas"]["FileEntry"];

export interface Ctx {
  host: HostView;
}

export interface Transfer {
  id: number;
  direction: "upload" | "download";
  label: string;
  state: "running" | "done" | "failed";
  checksumOk?: boolean;
  error?: string;
  finishedAt?: number;
}

export const DRIVE_TYPE_LABEL: Record<string, string> = {
  fixed: "本地磁盘",
  removable: "可移动磁盘",
  network: "网络磁盘",
  cdrom: "光盘",
  ramdisk: "内存盘",
};

export function formatMtime(ms?: number): string {
  if (!ms) return "—";
  const d = new Date(ms);
  const months = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
  ];
  return `${months[d.getMonth()]} ${String(d.getDate()).padStart(2, "0")} ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}
