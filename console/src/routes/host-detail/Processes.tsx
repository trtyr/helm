import { useMemo, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { MoreHorizontal, RefreshCw, Search, X } from "lucide-react";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { humanSize } from "../../lib/paths";
import { toast } from "../../lib/toast";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];
type ProcessInfo = components["schemas"]["ProcessInfo"];

interface Ctx {
  host: HostView;
}

type SortKey = "pid" | "cpu" | "mem";
type IntervalOpt = 0 | 5_000 | 10_000 | 30_000;

const INTERVALS: { value: IntervalOpt; label: string }[] = [
  { value: 0, label: "关闭" },
  { value: 5_000, label: "5s" },
  { value: 10_000, label: "10s" },
  { value: 30_000, label: "30s" },
];

/** 进程管理（规格 processes.md F46–F48：排序/搜索/kill 确认/自动刷新）。 */
export default function Processes() {
  const { host } = useOutletContext<Ctx>();
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "cpu", desc: true });
  const [interval, setIntervalOpt] = useState<IntervalOpt>(5_000);
  const [menuFor, setMenuFor] = useState<number | null>(null);
  const [killTarget, setKillTarget] = useState<ProcessInfo | null>(null);

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

  // 搜索 + 排序（CPU 降序默认）
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    let list = listQuery.data?.processes ?? [];
    if (q) {
      list = list.filter(
        (p) => (p.name ?? "").toLowerCase().includes(q) || String(p.pid ?? "").startsWith(q),
      );
    }
    const dir = sort.desc ? -1 : 1;
    return [...list].sort((a, b) => {
      switch (sort.key) {
        case "cpu":
          return ((a.cpu_percent ?? 0) - (b.cpu_percent ?? 0)) * dir;
        case "mem":
          return ((a.mem_bytes ?? 0) - (b.mem_bytes ?? 0)) * dir;
        default:
          return ((a.pid ?? 0) - (b.pid ?? 0)) * dir;
      }
    });
  }, [listQuery.data, search, sort]);

  const all = listQuery.data?.processes ?? [];
  const cpuTotal = all.reduce((sum, p) => sum + (p.cpu_percent ?? 0), 0);
  const shown = filtered.slice(0, 100);
  const offline = !host.online;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-label-13 text-gray-900">进程</h2>
        <span className="flex-1" />
        <div className="relative">
          <Search size={13} strokeWidth={1.5} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-gray-900" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索名称 / PID"
            aria-label="搜索进程"
            className="h-8 w-52 rounded-md border border-gray-400 bg-gray-100 pl-8 pr-7 text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
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
        <button
          type="button"
          aria-label="刷新"
          disabled={offline}
          onClick={() => listQuery.refetch()}
          className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000 disabled:opacity-30"
        >
          <RefreshCw size={14} strokeWidth={1.5} />
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

      {offline && listQuery.data && (
        <p className="font-mono text-label-12 text-amber-1000">
          快照 · {new Date(listQuery.data.at).toLocaleTimeString()}
        </p>
      )}

      <div className="overflow-visible rounded-lg border border-gray-400">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              {(
                [
                  ["pid", "PID", ""],
                  ["name", "名称", ""],
                  ["cpu", "CPU%", "text-right"],
                  ["mem", "内存", "text-right"],
                ] as const
              ).map(([key, label, align]) => {
                const sortable = key !== "name";
                const active = sort.key === key;
                return (
                  <th
                    key={key}
                    className={`px-4 py-2 font-normal ${align} ${
                      sortable ? "cursor-pointer select-none hover:text-gray-1000" : ""
                    }`}
                    onClick={sortable ? () => setSort((s) => ({ key, desc: s.key === key && !s.desc })) : undefined}
                  >
                    {label} {active && (sort.desc ? "↓" : "↑")}
                  </th>
                );
              })}
              <th className="px-4 py-2" />
            </tr>
          </thead>
          <tbody>
            {listQuery.isPending ? (
              <tr>
                <td colSpan={5} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  加载进程列表…
                </td>
              </tr>
            ) : filtered.length === 0 ? (
              <tr>
                <td colSpan={5} className="px-4 py-10 text-center text-label-13 text-gray-900">
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
              shown.map((p) => (
                <tr
                  key={`${p.pid}-${p.name}`}
                  className="group relative border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
                >
                  <td className="px-4 py-2.5 font-mono text-label-13 text-gray-900 tabular-nums">{p.pid}</td>
                  <td className="px-4 py-2.5 text-label-14">{p.name}</td>
                  <td className="px-4 py-2.5 text-right font-mono text-label-13 tabular-nums">
                    <span className={(p.cpu_percent ?? 0) > 80 ? "text-amber-1000" : "text-gray-900"}>
                      {(p.cpu_percent ?? 0).toFixed(1)}
                    </span>
                  </td>
                  <td className="px-4 py-2.5 text-right font-mono text-label-13 tabular-nums">
                    <span className={(p.mem_bytes ?? 0) > 500 * 1024 * 1024 ? "text-amber-1000" : "text-gray-900"}>
                      {humanSize(p.mem_bytes ?? 0)}
                    </span>
                  </td>
                  <td className="relative px-4 py-2.5 text-right">
                    <button
                      type="button"
                      aria-label={`操作 ${p.name ?? p.pid}`}
                      disabled={offline}
                      onClick={() => setMenuFor(menuFor === p.pid ? null : p.pid!)}
                      className="flex h-7 w-7 items-center justify-center rounded-md text-gray-900 opacity-0 transition-opacity duration-150 hover:bg-gray-200 hover:text-gray-1000 group-hover:opacity-100 disabled:opacity-30"
                    >
                      <MoreHorizontal size={14} strokeWidth={1.5} />
                    </button>
                    {menuFor === p.pid && (
                      <>
                        <button
                          type="button"
                          aria-label="关闭菜单"
                          onClick={() => setMenuFor(null)}
                          className="fixed inset-0 z-30 cursor-default"
                        />
                        <div className="absolute right-4 top-10 z-40 w-32 rounded-xl border border-gray-400 bg-background-100 py-1 shadow-lg">
                          <button
                            type="button"
                            onClick={() => {
                              setMenuFor(null);
                              setKillTarget(p);
                            }}
                            className="block w-full px-3 py-2 text-left text-label-13 text-red-1000 transition-colors duration-150 hover:bg-gray-200"
                          >
                            结束进程
                          </button>
                        </div>
                      </>
                    )}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
          共 {all.length} 个进程
          {filtered.length > 100 && ` · 显示前 100`} · CPU 合计 {cpuTotal.toFixed(1)}%
        </div>
      </div>

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
