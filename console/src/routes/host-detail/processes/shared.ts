// 进程页的纯逻辑与列定义（G13 拆分，2026-09-21）——自 `Processes.tsx` 拆出：
// 排序/间隔选项、列定义、应急异常父子检测规则、进程树构建。
// 本文件不依赖 React，逻辑可独立单测（`Processes.test.tsx` 直接测这里的导出）。
import type { components } from "../../../api/schema";
import type { TableColumn } from "../../../lib/useTableControls";

type ProcessInfo = components["schemas"]["ProcessInfo"];

export type SortKey = "pid" | "name" | "cpu" | "mem" | "user" | "started";
export type IntervalOpt = 0 | 3_000 | 5_000 | 10_000 | 30_000;

export const INTERVALS: { value: IntervalOpt; label: string }[] = [
  { value: 0, label: "关闭" },
  { value: 3_000, label: "3s" },
  { value: 5_000, label: "5s" },
  { value: 10_000, label: "10s" },
  { value: 30_000, label: "30s" },
];

/**
 * 列宽自适应策略：短列 `w-px whitespace-nowrap`（收缩到「内容 + padding」，消灭多余留白），
 * 名称列 `w-full max-w-0`（吸收全部剩余空间，过长截断有 title）。
 */
export const COLUMNS: {
  key: SortKey | "virt" | "status" | "actions";
  label: string;
  align: string;
  sortable: boolean;
  width: string;
}[] = [
  { key: "pid", label: "PID", align: "", sortable: true, width: "w-px" },
  { key: "name", label: "名称", align: "", sortable: true, width: "w-full" },
  { key: "cpu", label: "CPU%", align: "text-right", sortable: true, width: "w-px" },
  { key: "mem", label: "内存", align: "text-right", sortable: true, width: "w-px" },
  { key: "virt", label: "虚拟内存", align: "text-right", sortable: false, width: "w-px" },
  { key: "status", label: "状态", align: "", sortable: false, width: "w-px" },
  { key: "user", label: "用户", align: "", sortable: true, width: "w-px" },
  { key: "started", label: "已运行", align: "text-right", sortable: true, width: "w-px" },
  { key: "actions", label: "", align: "text-right", sortable: false, width: "w-px" },
];

/** 列表基座列定义（P001-T1）：useTableControls 的取值/筛选声明（status 枚举在组件内派生追加）。 */
export const PROC_TABLE_COLUMNS_BASE: TableColumn<ProcessInfo>[] = [
  { key: "pid", value: (p) => p.pid ?? 0 },
  { key: "name", value: (p) => p.name ?? "" },
  { key: "cpu", value: (p) => p.cpu_percent ?? 0 },
  { key: "mem", value: (p) => p.mem_bytes ?? 0 },
  { key: "user", value: (p) => p.user ?? "" },
  { key: "started", value: (p) => p.start_time_unix ?? 0 },
];

// ---------------------------------------------------------------------------
// 应急：异常父子关系检测（对齐 EDR 的常见检测点，保持高信号、低误报）
// ---------------------------------------------------------------------------

const RX_OFFICE = /^(winword|excel|powerpnt|outlook|onenote)\.exe$/i;
const RX_BROWSER = /^(chrome|msedge|firefox|brave|opera)\.exe$/i;
const RX_PDF = /^(acrobat|acrord32|foxit|sumatrapdf)\.exe$/i;
const RX_INTERP =
  /^(cmd|powershell|pwsh|wscript|cscript|mshta|rundll32|regsvr32|msbuild|installutil|certutil|bitsadmin|curl)\.exe$/i;

/** 返回异常规则名（无异常返回 null）。 */
export function suspiciousPair(parent: string, child: string): string | null {
  const p = (parent || "").toLowerCase();
  const c = (child || "").toLowerCase();
  if (!p || !c) return null;
  if (p === "lsass.exe") return "LSASS 派生进程（凭据窃取典型行为）";
  if (p === "smss.exe" && !/^(csrss|wininit|smss)\.exe$/.test(c)) return "SMSS 异常子进程";
  if (p === "services.exe" && RX_INTERP.test(c)) return "服务管理器派生解释器";
  if ((RX_OFFICE.test(p) || RX_BROWSER.test(p) || RX_PDF.test(p)) && RX_INTERP.test(c))
    return "办公/浏览器派生解释器（宏或网页挂马典型链）";
  return null;
}

/** 树形节点：进程 + 深度。 */
export interface TreeRow {
  p: ProcessInfo;
  depth: number;
  hasChildren: boolean;
}

/**
 * 由平铺列表构建进程树行序（深度优先、兄弟按名称排序）。
 * 父进程不在快照中的进程视为根；cycled/孤儿防环。
 */
export function buildTreeRows(
  procs: ProcessInfo[],
  collapsed: Set<number>,
  keepAncestorsOf?: Set<number>,
): TreeRow[] {
  const byPid = new Map<number, ProcessInfo>();
  for (const p of procs) if (p.pid != null) byPid.set(p.pid, p);
  const children = new Map<number, ProcessInfo[]>();
  const roots: ProcessInfo[] = [];
  for (const p of procs) {
    const parent = p.parent_pid ?? 0;
    if (parent !== p.pid && byPid.has(parent)) {
      const list = children.get(parent) ?? [];
      list.push(p);
      children.set(parent, list);
    } else {
      roots.push(p);
    }
  }
  const byName = (a: ProcessInfo, b: ProcessInfo) => (a.name ?? "").localeCompare(b.name ?? "");
  for (const list of children.values()) list.sort(byName);
  roots.sort(byName);

  const rows: TreeRow[] = [];
  const visited = new Set<number>();
  const pushTree = (p: ProcessInfo, depth: number) => {
    if (visited.has(p.pid!)) return; // 防环
    visited.add(p.pid!);
    const kids = children.get(p.pid!) ?? [];
    const keep = !keepAncestorsOf || keepAncestorsOf.has(p.pid!) || kids.some((k) => keepAncestorsOf.has(k.pid!));
    if (!keep) return;
    rows.push({ p, depth, hasChildren: (children.get(p.pid!)?.length ?? 0) > 0 });
    const isCollapsed = collapsed.has(p.pid!);
    if (!isCollapsed) {
      for (const k of [...(children.get(p.pid!) ?? [])].sort(byName)) {
        pushTree(k, depth + 1);
      }
    } else {
      // 折叠的子树全部标记已访问，避免被孤儿兜底重新收录
      // （不能用 visited.has 早退——起始节点自身已被 pushTree 标记，会整棵漏标）
      const markSubtree = (pid: number, path: Set<number>) => {
        if (path.has(pid)) return;
        path.add(pid);
        visited.add(pid);
        for (const k of children.get(pid) ?? []) if (k.pid !== undefined) markSubtree(k.pid, path);
      };
      markSubtree(p.pid!, new Set());
    }
  };
  for (const r of [...roots].sort(byName)) pushTree(r, 0);
  // 环引用兜底：DFS 不可达的进程平铺追加，保证全部可见
  const orphans = procs.filter((p) => !visited.has(p.pid!)).sort(byName);
  for (const p of orphans) pushTree(p, 0);
  return rows;
}
