import { useMemo, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { RefreshCw, Search, X } from "lucide-react";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { formatDateTime, formatUptime } from "../../lib/format";
import { humanSize } from "../../lib/paths";
import { toast } from "../../lib/toast";

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

/** 进程监控（Process Hacker 风格：全维排序/搜索/自动刷新/负载着色/单进程详情抽屉）。 */
export default function Processes() {
  const { host } = useOutletContext<Ctx>();
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "cpu", desc: true });
  const [interval, setIntervalOpt] = useState<IntervalOpt>(5_000);
  const [killTarget, setKillTarget] = useState<ProcessInfo | null>(null);
  const [detail, setDetail] = useState<ProcessInfo | null>(null);

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = (agentsQuery.data?.agents ?? []).find((a) => a.host_id === host.id);

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

  // 搜索 + 全维排序（CPU 降序默认）
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    let list = listQuery.data?.processes ?? [];
    if (q) {
      list = list.filter(
        (p) =>
          (p.name ?? "").toLowerCase().includes(q) ||
          String(p.pid ?? "").startsWith(q) ||
          (p.user ?? "").toLowerCase().includes(q),
      );
    }
    const dir = sort.desc ? -1 : 1;
    return [...list].sort((a, b) => {
      switch (sort.key) {
        case "cpu":
          return ((a.cpu_percent ?? 0) - (b.cpu_percent ?? 0)) * dir;
        case "mem":
          return ((a.mem_bytes ?? 0) - (b.mem_bytes ?? 0)) * dir;
        case "user":
          return (a.user ?? "").localeCompare(b.user ?? "") * dir;
        case "started":
          return ((b.start_time_unix ?? 0) - (a.start_time_unix ?? 0)) * dir;
        case "name":
          return (a.name ?? "").localeCompare(b.name ?? "") * dir;
        default:
          return ((a.pid ?? 0) - (b.pid ?? 0)) * dir;
      }
    });
  }, [listQuery.data, search, sort]);

  const all = listQuery.data?.processes ?? [];
  // 系统级负载（telemetry）：进程 RSS 合计会把共享页重复计数（Linux 上远超物理内存），
  // 顶部资源条以系统口径为准；进程口径仅作参考副行。
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
  const now = listQuery.data?.at ?? 0;
  const offline = !host.online;

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
              {COLUMNS.map(({ key, label, align, sortable, width }) => (
                <th
                  key={key}
                  className={`${width} whitespace-nowrap px-3 py-2.5 font-normal ${align} ${
                    sortable ? "cursor-pointer select-none hover:text-gray-1000" : ""
                  } ${key === "name" ? "max-w-0" : ""}`}
                  onClick={
                    sortable
                      ? () => setSort((s) => ({ key: key as SortKey, desc: s.key === key && !s.desc }))
                      : undefined
                  }
                >
                  {label} {sort.key === key && (sort.desc ? "↓" : "↑")}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {listQuery.isPending ? (
              <tr>
                <td colSpan={9} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  加载进程列表…
                </td>
              </tr>
            ) : filtered.length === 0 ? (
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
            ) : (
              filtered.slice(0, 200).map((p) => {
                const cpu = p.cpu_percent ?? 0;
                const mem = p.mem_bytes ?? 0;
                return (
                  <tr
                    key={`${p.pid}-${p.name}`}
                    onClick={() => setDetail(p)}
                    className={`group cursor-pointer border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100 ${
                      cpu > 50 ? "bg-amber-1000/5" : ""
                    }`}
                  >
                    <td className="whitespace-nowrap px-3 py-2 font-mono text-label-13 text-gray-900 tabular-nums">{p.pid}</td>
                    <td className="max-w-0 px-3 py-2">
                      <span className="block truncate text-label-14" title={p.cmd || p.name}>
                        {p.name}
                      </span>
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
                      {p.start_time_unix ? formatUptime(p.start_time_unix, now) : "—"}
                    </td>
                    <td className="whitespace-nowrap px-3 py-2 text-right">
                      <button
                        type="button"
                        aria-label={`结束 ${p.name ?? p.pid}`}
                        disabled={offline}
                        onClick={(e) => {
                          e.stopPropagation();
                          setKillTarget(p);
                        }}
                        className="rounded border border-gray-400 px-1.5 py-0.5 text-label-12 text-gray-900 opacity-0 transition-all duration-150 hover:border-red-1000 hover:text-red-1000 group-hover:opacity-100 disabled:opacity-30"
                      >
                        结束
                      </button>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center justify-between border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
          <span>
            共 {all.length} 个进程{search && filtered.length !== all.length ? ` · 匹配 ${filtered.length}` : ""}
            {filtered.length > 200 ? " · 显示前 200" : ""}
          </span>
          <span>CPU 合计 {cpuTotal.toFixed(1)}%（单核口径）· RSS 合计 {humanSize(memRssSum)}</span>
        </div>
      </div>

      {/* 进程详情抽屉（点击行展开，Process Hacker properties 风格） */}
      {detail && (
        <ProcessDetail
          p={detail}
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

/** 单进程详情（只读属性表 + 终止操作）。 */
function ProcessDetail({
  p,
  killing,
  onClose,
  onKill,
}: {
  p: ProcessInfo;
  killing: boolean;
  onClose: () => void;
  onKill: () => void;
}) {
  const rows: [string, string][] = [
    ["PID", String(p.pid ?? "—")],
    ["名称", p.name ?? "—"],
    ["状态", p.status || "—"],
    ["用户", p.user || "—"],
    ["父进程 PID", p.parent_pid ? String(p.parent_pid) : "—"],
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
