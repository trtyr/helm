import { useMemo, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { ChevronDown, ChevronRight, RefreshCw, Search, X } from "lucide-react";
import type { components } from "../../api/schema";
import { api, pickAgent } from "../../api/client";
import { formatDateTime, formatUptime } from "../../lib/format";
import { humanSize } from "../../lib/paths";
import { toast } from "../../lib/toast";
import { useTableControls, type TableColumn } from "../../lib/useTableControls";
import { SortableTh } from "../../components/tableControls";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];
type ProcessInfo = components["schemas"]["ProcessInfo"];

interface Ctx {
  host: HostView;
}

type SortKey = "pid" | "name" | "cpu" | "mem" | "user" | "started";
type IntervalOpt = 0 | 3_000 | 5_000 | 10_000 | 30_000;

const INTERVALS: { value: IntervalOpt; label: string }[] = [
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
const COLUMNS: {
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
const PROC_TABLE_COLUMNS_BASE: TableColumn<ProcessInfo>[] = [
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
const RX_INTERP = /^(cmd|powershell|pwsh|wscript|cscript|mshta|rundll32|regsvr32|msbuild|installutil|certutil|bitsadmin|curl)\.exe$/i;

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
interface TreeRow {
  p: ProcessInfo;
  depth: number;
  hasChildren: boolean;
}

/**
 * 由平铺列表构建进程树行序（深度优先、兄弟按名称排序）。
 * 父进程不在快照中的进程视为根；cycled/孤儿防环。
 */
export function buildTreeRows(procs: ProcessInfo[], collapsed: Set<number>, keepAncestorsOf?: Set<number>): TreeRow[] {
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

/**
 * 进程监控（Process Hacker 风格：平铺排序视图 + 应急进程树视图，
 * 全维搜索/自动刷新/负载着色/单进程详情抽屉/异常父子高亮）。
 */
export default function Processes() {
  const { host } = useOutletContext<Ctx>();
  const [interval, setIntervalOpt] = useState<IntervalOpt>(5_000);
  const [killTarget, setKillTarget] = useState<ProcessInfo | null>(null);
  const [detail, setDetail] = useState<ProcessInfo | null>(null);
  const [view, setView] = useState<"flat" | "tree">("tree");
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = pickAgent(agentsQuery.data?.agents ?? [], host.id);

  const listQuery = useQuery({
    queryKey: ["processes", agent?.id],
    queryFn: async () => {
      const r = await api<{ processes: ProcessInfo[] }>("/api/v1/processes/list", {
        method: "POST",
        body: { agent_id: agent!.id },
      });
      // 时间戳在异步侧产生（render 纯度）
      return { at: Date.now(), processes: r.processes ?? [] };
    },
    enabled: !!agent && host.online,
    refetchInterval: interval || false,
  });

  const killMutation = useMutation({
    mutationFn: (pid: number) =>
      api<{ ok?: boolean }>("/api/v1/processes/kill", {
        method: "POST",
        body: { agent_id: agent!.id, pid },
      }),
    onSuccess: (res) => {
      setKillTarget(null);
      setDetail(null);
      if (res.ok) {
        toast(`进程 ${killTarget?.pid} 已结束`);
      } else {
        toast("进程不存在（可能已退出）", "warn");
      }
      listQuery.refetch();
    },
    onError: (e) => {
      toast((e as Error).message, "error");
    },
  });

  const all = useMemo(() => listQuery.data?.processes ?? [], [listQuery.data]);

  // 列表基座（P001-T1）：排序/搜索/枚举筛选统一控制
  const statusOptions = useMemo(() => {
    const set = new Set<string>();
    for (const p of all) if (p.status) set.add(p.status);
    return [...set].sort().map((s) => ({ value: s, label: s }));
  }, [all]);
  const procColumns: TableColumn<ProcessInfo>[] = useMemo(
    () => [
      ...PROC_TABLE_COLUMNS_BASE,
      {
        key: "status",
        value: (p) => p.status ?? "",
        enumOptions: () => statusOptions,
        matchesEnum: (p, v) => (p.status ?? "") === v,
      },
    ],
    [statusOptions],
  );
  const tc = useTableControls(all, {
    columns: procColumns,
    searchText: (p) => `${p.name ?? ""} ${p.pid ?? ""} ${p.user ?? ""}`,
    defaultSort: { key: "cpu", desc: true },
  });
  const search = tc.search;
  const setSearch = tc.setSearch;

  const procByName = useMemo(() => {
    const m = new Map<number, string>();
    for (const p of all) if (p.pid != null) m.set(p.pid, p.name ?? "");
    return m;
  }, [all]);

  // 搜索命中集合（树模式下保留祖先链）
  const matchSet = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return null;
    const matched = new Set<number>();
    for (const p of all) {
      if (
        (p.name ?? "").toLowerCase().includes(q) ||
        String(p.pid ?? "").startsWith(q) ||
        (p.user ?? "").toLowerCase().includes(q)
      ) {
        matched.add(p.pid!);
      }
    }
    // 补齐祖先链
    let grew = true;
    while (grew) {
      grew = false;
      for (const p of all) {
        if (matched.has(p.pid!) || !p.parent_pid) continue;
        if (matched.has(p.parent_pid)) {
          matched.add(p.pid!);
          grew = true;
        }
      }
    }
    return matched;
  }, [all, search]);

  // 平铺视图：基座筛选/排序（hook）+ 树搜索命中过滤
  const filtered = useMemo(
    () => (matchSet ? tc.visible.filter((p) => matchSet.has(p.pid!)) : tc.visible),
    [tc.visible, matchSet],
  );

  // 树视图行
  const treeRows = useMemo(() => {
    const procs = matchSet ? all.filter((p) => matchSet.has(p.pid!)) : all;
    return buildTreeRows(procs, collapsed, matchSet ?? undefined);
  }, [all, collapsed, matchSet]);

  const suspiciousCount = useMemo(() => {
    let n = 0;
    for (const p of all) {
      const parent = p.parent_pid ? (procByName.get(p.parent_pid) ?? "") : "";
      if (suspiciousPair(parent, p.name ?? "")) n += 1;
    }
    return n;
  }, [all, procByName]);

  const metricsQuery = useQuery({
    queryKey: ["metrics", host.id],
    queryFn: () => api<{ metrics: components["schemas"]["Metric"][] }>(
      `/api/v1/metrics?host_id=${host.id}&limit=50`,
    ),
    refetchInterval: 30_000,
  });
  const latestMetric = (name: string): number | undefined => {
    for (const m of metricsQuery.data?.metrics ?? []) {
      if (m.name === name) return m.value ?? undefined;
    }
    return undefined;
  };
  const sysCpu = latestMetric("cpu.usage");
  const memUsed = latestMetric("mem.used");
  const memTotalSys = latestMetric("mem.total");
  const memPercent = latestMetric("mem.percent") ?? (memUsed && memTotalSys ? (memUsed / memTotalSys) * 100 : undefined);
  const cpuTotal = all.reduce((sum, p) => sum + (p.cpu_percent ?? 0), 0);
  const memRssSum = all.reduce((sum, p) => sum + (p.mem_bytes ?? 0), 0);
  const offline = !host.online;
  const isTree = view === "tree";
  const displayRows: TreeRow[] | null = isTree ? treeRows : null;

  const toggleCollapse = (pid: number) => {
    setCollapsed((s) => {
      const n = new Set(s);
      if (n.has(pid)) n.delete(pid);
      else n.add(pid);
      return n;
    });
  };

  return (
    <div className="flex flex-col gap-4">
      {/* 总负载条（系统口径，同 Process Hacker 资源摘要） */}
      <div className="grid grid-cols-2 gap-4">
        <LoadCard
          label="CPU（系统）"
          value={sysCpu !== undefined ? `${sysCpu.toFixed(1)}%` : `${cpuTotal.toFixed(1)}%`}
          percent={Math.min(100, sysCpu ?? cpuTotal)}
          over={(sysCpu ?? cpuTotal) > 80}
          sub={`进程 CPU 合计 ${cpuTotal.toFixed(1)}%（单核口径）`}
        />
        <LoadCard
          label="内存（系统）"
          value={memUsed !== undefined ? `${humanSize(memUsed)} / ${humanSize(memTotalSys ?? 0)}` : "—"}
          percent={Math.min(100, memPercent ?? 0)}
          over={(memPercent ?? 0) > 80}
          sub={`进程 RSS 合计 ${humanSize(memRssSum)}（含共享页重复计数，仅供参考）`}
        />
      </div>

      {/* 工具行 */}
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative">
          <Search size={13} strokeWidth={1.5} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-gray-900" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索名称 / PID / 用户"
            aria-label="搜索进程"
            className="h-8 w-56 rounded-md border border-gray-400 bg-gray-100 pl-8 pr-7 text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
          {search && (
            <button
              type="button"
              aria-label="清除搜索"
              onClick={() => setSearch("")}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-900 hover:text-gray-1000"
            >
              <X size={12} strokeWidth={1.5} />
            </button>
          )}
        </div>
        <select
          aria-label="按状态筛选"
          value={tc.enumFilters.status ?? ""}
          onChange={(e) => tc.setEnumFilter("status", e.target.value)}
          className="h-8 rounded-md border border-gray-400 bg-gray-100 px-2 text-label-13 outline-none transition-colors duration-150 hover:border-gray-500"
        >
          <option value="">状态：全部</option>
          {statusOptions.map((o) => (
            <option key={o.value} value={o.value}>{o.value}</option>
          ))}
        </select>
        {/* 树形/平铺切换 */}
        <div className="flex h-8 items-center overflow-hidden rounded-md border border-gray-400">
          {(["tree", "flat"] as const).map((v) => (
            <button
              key={v}
              type="button"
              onClick={() => setView(v)}
              className={`h-full px-3 text-label-13 transition-colors duration-150 ${
                view === v ? "bg-gray-700 text-white" : "bg-gray-100 text-gray-900 hover:bg-gray-200"
              }`}
            >
              {v === "tree" ? "进程树" : "平铺"}
            </button>
          ))}
        </div>
        {isTree && (
          <>
            {suspiciousCount > 0 && (
              <span
                className="whitespace-nowrap rounded bg-red-1000/10 px-1.5 py-0.5 text-label-12 text-red-1000"
                title="父子关系命中应急检测规则（办公/浏览器派生解释器、LSASS 派生等）"
              >
                ⚠ 异常父子 {suspiciousCount}
              </span>
            )}
            <button
              type="button"
              onClick={() => setCollapsed(new Set())}
              className="h-8 rounded-md border border-gray-400 px-2 text-label-12 text-gray-900 hover:bg-gray-200"
            >
              全部展开
            </button>
            <button
              type="button"
              onClick={() => {
                const allPids = new Set<number>();
                for (const p of all) if (p.pid) allPids.add(p.pid);
                setCollapsed(allPids);
              }}
              className="h-8 rounded-md border border-gray-400 px-2 text-label-12 text-gray-900 hover:bg-gray-200"
            >
              全部折叠
            </button>
          </>
        )}
        <span className="flex-1" />
        <span className="font-mono text-label-12 text-gray-900">
          {listQuery.data ? `快照 ${new Date(listQuery.data.at).toLocaleTimeString()}` : ""}
        </span>
        <button
          type="button"
          aria-label="刷新"
          disabled={offline}
          onClick={() => listQuery.refetch()}
          className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000 disabled:opacity-30"
        >
          <RefreshCw size={14} strokeWidth={1.5} className={listQuery.isFetching ? "animate-spin" : ""} />
        </button>
        <select
          aria-label="自动刷新间隔"
          value={interval}
          onChange={(e) => setIntervalOpt(Number(e.target.value) as IntervalOpt)}
          className="h-8 rounded-md border border-gray-400 bg-gray-100 px-2 font-mono text-label-13 outline-none transition-colors duration-150 hover:border-gray-500"
        >
          {INTERVALS.map((i) => (
            <option key={i.value} value={i.value}>
              ⏱ {i.label}
            </option>
          ))}
        </select>
      </div>

      <div className="overflow-visible rounded-lg border border-gray-400 bg-background-100">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              {COLUMNS.map(({ key, label, align, sortable, width }) =>
                sortable && !isTree ? (
                  <SortableTh
                    key={key}
                    label={label}
                    sortKey={key}
                    sort={tc.sort}
                    onSort={tc.toggleSort}
                    align={align}
                    className={width === "w-full" ? "max-w-0" : ""}
                  />
                ) : (
                  <th
                    key={key}
                    className={`${width} whitespace-nowrap px-3 py-2.5 font-normal ${align} ${key === "name" ? "max-w-0" : ""}`}
                  >
                    {label}
                  </th>
                ),
              )}
            </tr>
          </thead>
          <tbody>
            {listQuery.isPending ? (
              <tr>
                <td colSpan={9} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  加载进程列表…
                </td>
              </tr>
            ) : (isTree ? treeRows : filtered).length === 0 ? (
              <tr>
                <td colSpan={9} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  {search ? (
                    <>
                      没有匹配 「{search}」 的进程
                      <button type="button" onClick={() => setSearch("")} className="ml-2 text-blue-1000 hover:underline">
                        清除
                      </button>
                    </>
                  ) : (
                    "无法获取进程列表"
                  )}
                </td>
              </tr>
            ) : isTree ? (
              displayRows!.map(({ p, depth, hasChildren }) => {
                const parent = p.parent_pid ? (procByName.get(p.parent_pid) ?? "") : "";
                const rule = suspiciousPair(parent, p.name ?? "");
                const isCollapsed = collapsed.has(p.pid!);
                return (
                  <ProcessRow
                    key={`t-${p.pid}-${p.name}`}
                    p={p}
                    offline={offline}
                    suspicious={rule}
                    indent={depth}
                    expandable={hasChildren}
                    expanded={!isCollapsed}
                    onToggle={() => toggleCollapse(p.pid!)}
                    onClick={() => setDetail(p)}
                    onKill={() => setKillTarget(p)}
                  />
                );
              })
            ) : (
              filtered.slice(0, 200).map((p) => (
                <ProcessRow
                  key={`f-${p.pid}-${p.name}`}
                  p={p}
                  offline={offline}
                  onClick={() => setDetail(p)}
                  onKill={() => setKillTarget(p)}
                />
              ))
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center justify-between border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
          <span>
            共 {all.length} 个进程{search && !isTree && filtered.length !== all.length ? ` · 匹配 ${filtered.length}` : ""}
            {isTree ? ` · 树显示 ${treeRows.length}` : ""}
            {filtered.length > 200 && !isTree ? " · 显示前 200" : ""}
          </span>
          <span>CPU 合计 {cpuTotal.toFixed(1)}%（单核口径）· RSS 合计 {humanSize(memRssSum)}</span>
        </div>
      </div>

      {/* 进程详情抽屉（点击行展开，Process Hacker properties 风格） */}
      {detail && (
        <ProcessDetail
          p={detail}
          parentName={detail.parent_pid ? (procByName.get(detail.parent_pid) ?? "") : ""}
          killing={killMutation.isPending}
          onClose={() => setDetail(null)}
          onKill={() => detail && killMutation.mutate(detail.pid!)}
        />
      )}

      {/* kill 确认模态 */}
      {killTarget && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <button
            type="button"
            aria-label="关闭"
            onClick={() => setKillTarget(null)}
            className="absolute inset-0 bg-black/40"
          />
          <div className="relative z-10 w-[400px] rounded-xl border border-gray-400 bg-background-100 p-6">
            <h2 className="text-heading-16">结束进程</h2>
            <p className="mt-4 font-mono text-label-14">
              PID {killTarget.pid} · {killTarget.name}
            </p>
            <p className="mt-4 text-label-13 text-red-1000">该操作不可恢复。</p>
            {killMutation.isError && (
              <p className="mt-3 text-label-13 text-red-1000" role="alert">
                ⚠ {(killMutation.error as Error).message}
              </p>
            )}
            <div className="mt-6 flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setKillTarget(null)}
                disabled={killMutation.isPending}
                className="h-8 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => killMutation.mutate(killTarget.pid!)}
                disabled={killMutation.isPending}
                className="h-8 rounded-md border border-red-1000 px-4 text-label-14 text-red-1000 transition-colors duration-150 hover:bg-red-1000 hover:text-background-100 disabled:opacity-50"
              >
                {killMutation.isPending ? "结束中…" : "结束进程"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function LoadCard({
  label,
  value,
  percent,
  over,
  sub,
}: {
  label: string;
  value: string;
  percent: number;
  over: boolean;
  sub?: string;
}) {
  return (
    <div className="rounded-lg border border-gray-400 bg-background-100 p-4">
      <div className="flex items-baseline justify-between">
        <span className="text-label-13 text-gray-900">{label}</span>
        <span className={`font-mono text-label-16 tabular-nums ${over ? "text-amber-1000" : ""}`}>{value}</span>
      </div>
      {percent > 0 && (
        <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-gray-200">
          <div
            className={`h-full rounded-full transition-all duration-300 ${over ? "bg-amber-1000" : "bg-blue-1000"}`}
            style={{ width: `${percent}%` }}
          />
        </div>
      )}
      {sub && <p className="mt-1.5 text-label-12 text-gray-900">{sub}</p>}
    </div>
  );
}

/** 单行（平铺/树共用）：树模式带缩进、折叠箭头与异常父子标记。 */
function ProcessRow({
  p,
  offline,
  suspicious,
  indent = 0,
  expandable = false,
  expanded = true,
  onToggle,
  onClick,
  onKill,
}: {
  p: ProcessInfo;
  offline: boolean;
  suspicious?: string | null;
  indent?: number;
  expandable?: boolean;
  expanded?: boolean;
  onToggle?: () => void;
  onClick?: () => void;
  onKill?: () => void;
}) {
  const cpu = p.cpu_percent ?? 0;
  const mem = p.mem_bytes ?? 0;
  return (
    <tr
      onClick={onClick}
      className={`group cursor-pointer border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100 ${
        cpu > 50 ? "bg-amber-1000/5" : ""
      }`}
      title={suspicious ?? undefined}
    >
      <td className="whitespace-nowrap px-3 py-2 font-mono text-label-13 text-gray-900 tabular-nums">{p.pid}</td>
      <td className="max-w-0 px-3 py-2">
        <div className="flex items-center gap-1" style={{ paddingLeft: indent * 18 }}>
          {expandable ? (
            <button
              type="button"
              aria-label={expanded ? "折叠" : "展开"}
              onClick={(e) => {
                e.stopPropagation();
                onToggle?.();
              }}
              className="shrink-0 rounded p-0.5 text-gray-900 hover:bg-gray-200"
            >
              {expanded ? <ChevronDown size={13} strokeWidth={1.5} /> : <ChevronRight size={13} strokeWidth={1.5} />}
            </button>
          ) : (
            <span className="w-[19px] shrink-0" />
          )}
          {suspicious && (
            <span className="shrink-0 rounded bg-red-1000/10 px-1 py-px text-label-12 font-medium text-red-1000" title={suspicious}>
              ⚠
            </span>
          )}
          <span className="block truncate text-label-14" title={p.cmd || p.name}>
            {p.name}
          </span>
        </div>
      </td>
      <td className="whitespace-nowrap px-3 py-2 text-right font-mono text-label-13 tabular-nums">
        <span className={cpu > 50 ? "font-semibold text-amber-1000" : cpu > 5 ? "text-gray-1000" : "text-gray-900"}>
          {cpu.toFixed(1)}
        </span>
      </td>
      <td className="whitespace-nowrap px-3 py-2 text-right font-mono text-label-13 tabular-nums">
        <span className={mem > 500 * 1024 * 1024 ? "text-amber-1000" : "text-gray-1000"}>
          {humanSize(mem)}
        </span>
      </td>
      <td className="whitespace-nowrap px-3 py-2 text-right font-mono text-label-13 text-gray-900 tabular-nums">
        {p.virt_mem_bytes ? humanSize(p.virt_mem_bytes) : "—"}
      </td>
      <td className="whitespace-nowrap px-3 py-2 text-label-12 text-gray-900">{p.status || "—"}</td>
      <td className="max-w-32 truncate whitespace-nowrap px-3 py-2 text-label-13 text-gray-900" title={p.user}>
        {p.user || "—"}
      </td>
      <td className="whitespace-nowrap px-3 py-2 text-right font-mono text-label-13 text-gray-900 tabular-nums">
        {p.start_time_unix ? formatUptime(p.start_time_unix, Date.now() / 1000) : "—"}
      </td>
      <td className="whitespace-nowrap px-3 py-2 text-right">
        <button
          type="button"
          aria-label={`结束 ${p.name ?? p.pid}`}
          disabled={offline}
          onClick={(e) => {
            e.stopPropagation();
            onKill?.();
          }}
          className="rounded border border-gray-400 px-1.5 py-0.5 text-label-12 text-gray-900 opacity-0 transition-all duration-150 hover:border-red-1000 hover:text-red-1000 group-hover:opacity-100 disabled:opacity-30"
        >
          结束
        </button>
      </td>
    </tr>
  );
}

/** 单进程详情（只读属性表 + 终止操作 + 父进程名称）。 */
function ProcessDetail({
  p,
  parentName,
  killing,
  onClose,
  onKill,
}: {
  p: ProcessInfo;
  parentName: string;
  killing: boolean;
  onClose: () => void;
  onKill: () => void;
}) {
  const parentLabel = parentName ? `${p.parent_pid ?? "—"}（${parentName}）` : String(p.parent_pid ?? "—");
  const rule = suspiciousPair(parentName, p.name ?? "");
  const rows: [string, string][] = [
    ["PID", String(p.pid ?? "—")],
    ["名称", p.name ?? "—"],
    ["状态", p.status || "—"],
    ["用户", p.user || "—"],
    ["父进程", parentLabel],
    ["CPU", `${(p.cpu_percent ?? 0).toFixed(1)}%`],
    ["内存", humanSize(p.mem_bytes ?? 0)],
    ["虚拟内存", p.virt_mem_bytes ? humanSize(p.virt_mem_bytes) : "—"],
    ["已运行", p.start_time_unix ? formatUptime(p.start_time_unix) : "—"],
    ["启动时间", p.start_time_unix ? formatDateTime(new Date(p.start_time_unix * 1000).toISOString()) : "—"],
  ];
  return (
    <div className="fixed inset-0 z-40 flex justify-end">
      <button type="button" aria-label="关闭" onClick={onClose} className="absolute inset-0 bg-black/40" />
      <div className="relative z-10 flex h-full w-[440px] flex-col gap-5 overflow-y-auto border-l border-gray-400 bg-background-100 p-6">
        <div className="flex items-center justify-between">
          <h2 className="text-heading-20">进程详情</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="关闭"
            className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200"
          >
            <X size={14} strokeWidth={1.5} />
          </button>
        </div>
        {rule && (
          <p className="rounded-md bg-red-1000/10 px-3 py-2 text-label-13 text-red-1000">⚠ 异常父子关系：{rule}</p>
        )}
        <dl className="flex flex-col">
          {rows.map(([k, v]) => (
            <div
              key={k}
              className="flex items-start justify-between gap-4 border-b border-gray-400/60 py-2 last:border-0"
            >
              <dt className="shrink-0 text-label-13 text-gray-900">{k}</dt>
              <dd className="text-right font-mono text-label-13">{v}</dd>
            </div>
          ))}
        </dl>
        {(p.exe_path || p.cmd) && (
          <div>
            <p className="text-label-13 text-gray-900">可执行文件 / 命令行</p>
            {p.exe_path && (
              <p className="mt-1 break-all rounded-md border border-gray-400 bg-gray-100 p-2 font-mono text-label-12">
                {p.exe_path}
              </p>
            )}
            {p.cmd && (
              <p className="mt-2 break-all rounded-md border border-gray-400 bg-gray-100 p-2 font-mono text-label-12">
                {p.cmd}
              </p>
            )}
          </div>
        )}
        <button
          type="button"
          onClick={onKill}
          disabled={killing}
          className="mt-auto h-9 rounded-md border border-red-1000 text-label-14 text-red-1000 transition-colors duration-150 hover:bg-red-1000 hover:text-background-100 disabled:opacity-50"
        >
          {killing ? "结束中…" : "结束此进程"}
        </button>
      </div>
    </div>
  );
}
