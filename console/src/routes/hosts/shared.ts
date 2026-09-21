// 主机列表页的共享类型与常量（G13 拆分，2026-09-21）——自 `Hosts.tsx` 拆出。
import type { components } from "../../api/schema";

export type HostView = components["schemas"]["HostView"];

export const LIMIT = 20;

export const OS_LABEL: Record<string, string> = {
  windows: "Windows",
  linux: "Linux",
  macos: "macOS",
  darwin: "macOS",
};
