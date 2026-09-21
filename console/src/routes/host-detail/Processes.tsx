import { useMemo, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import type { components } from "../../api/schema";
import { api, pickAgent } from "../../api/client";
import { humanSize } from "../../lib/paths";
import { toast } from "../../lib/toast";
import { useTableControls, type TableColumn } from "../../lib/useTableControls";
import { SortableTh } from "../../components/tableControls";
import {
  COLUMNS,
  PROC_TABLE_COLUMNS_BASE,
  buildTreeRows,
  suspiciousPair,
  type IntervalOpt,
  type TreeRow,
} from "./processes/shared";
import { KillProcessDialog, LoadCard, ProcessDetail, ProcessRow } from "./processes/ProcessParts";
import { ProcessToolbar } from "./processes/ProcessToolbar";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];
type ProcessInfo = components["schemas"]["ProcessInfo"];

interface Ctx {
  host: HostView;
}

/**
 * 进程监控（Process Hacker 风格：平铺排序视图 + 应急进程树视图，
 * 全维搜索/自动刷新/负载着色/单进程详情抽屉/异常父子高亮）。
 *
 * G13 拆分（2026-09-21）：原为 804 行单文件。现拆为——
 * `processes/shared.ts`（纯逻辑与列定义）/ `processes/ProcessParts.tsx`（视图部件）/
 * `processes/ProcessToolbar.tsx`（工具行）；本文件只保留状态、数据获取与编排。
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

      <ProcessToolbar
        search={search}
        onSearch={setSearch}
        statusOptions={statusOptions}
        statusFilter={tc.enumFilters.status ?? ""}
        onStatusFilter={(v) => tc.setEnumFilter("status", v)}
        view={view}
        onView={setView}
        isTree={isTree}
        suspiciousCount={suspiciousCount}
        onExpandAll={() => setCollapsed(new Set())}
        onCollapseAll={() => {
          const allPids = new Set<number>();
          for (const p of all) if (p.pid) allPids.add(p.pid);
          setCollapsed(allPids);
        }}
        snapshotAt={listQuery.data?.at}
        isFetching={listQuery.isFetching}
        onRefresh={() => listQuery.refetch()}
        offline={offline}
        interval={interval}
        onInterval={setIntervalOpt}
      />

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
        <KillProcessDialog
          p={killTarget}
          pending={killMutation.isPending}
          error={killMutation.isError ? (killMutation.error as Error).message : undefined}
          onCancel={() => setKillTarget(null)}
          onConfirm={() => killMutation.mutate(killTarget.pid!)}
        />
      )}
    </div>
  );
}
