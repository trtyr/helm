import { useMemo, useRef, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Play, RefreshCw, RotateCw, Search, Square, X } from "lucide-react";
import type { components } from "../../api/schema";
import { api, pickAgent } from "../../api/client";
import { useWsStream } from "../../api/ws";
import { toast } from "../../lib/toast";
import { useTableControls } from "../../lib/useTableControls";
import { SortableTh } from "../../components/tableControls";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];
type SysService = components["schemas"]["SysService"];

/** 常驻服务（Server 下发的自建服务；字段为后端 ServiceRow 的 snake_case 序列化）。 */
interface ManagedService {
  id: string;
  host_id: string;
  name: string;
  command: string;
  status: string;
  pid: number | null;
  exit_code: number | null;
}

interface Ctx {
  host: HostView;
}

type StatusFilter = "all" | "running" | "stopped" | "failed";

const STATUS_LABEL: Record<StatusFilter, string> = {
  all: "全部",
  running: "运行中",
  stopped: "已停止",
  failed: "失败",
};

/** 系统服务（实时发现：Windows Service / systemd / launchctl，无需手工创建）。
 * 启动/停止/重启直接作用于目标机原生服务。 */
export default function Services() {
  const { host } = useOutletContext<Ctx>();
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [auto, setAuto] = useState(true);
  const [pendingOp, setPendingOp] = useState<string | null>(null);

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = pickAgent(agentsQuery.data?.agents ?? [], host.id);

  const servicesQuery = useQuery({
    queryKey: ["sys-services", agent?.id],
    queryFn: async () => {
      const r = await api<{
        services: SysService[];
        error?: string | null;
      }>("/api/v1/sys-services/list", { method: "POST", body: { agent_id: agent!.id } });
      return { at: Date.now(), services: r.services ?? [], error: r.error ?? null };
    },
    enabled: !!agent && host.online,
    refetchInterval: auto ? 15_000 : false,
  });

  const actionMutation = useMutation({
    mutationFn: ({ name, action }: { name: string; action: string }) => {
      setPendingOp(`${name}:${action}`);
      return api<{ ok: boolean; error?: string | null }>("/api/v1/sys-services/action", {
        method: "POST",
        body: { agent_id: agent!.id, name, action },
      });
    },
    onSuccess: (res, vars) => {
      setPendingOp(null);
      if (res.ok) {
        toast(`${vars.name} ${vars.action === "start" ? "已启动" : vars.action === "stop" ? "已停止" : "已重启"}`);
      } else {
        toast(res.error || "操作失败（可能需要管理员权限）", "warn");
      }
      servicesQuery.refetch();
    },
    onError: (e) => {
      setPendingOp(null);
      toast((e as Error).message, "error");
    },
  });

  const services = useMemo(() => {
    const list = servicesQuery.data?.services ?? [];
    const q = search.trim().toLowerCase();
    return list.filter((s) => {
      if (statusFilter !== "all" && s.status !== statusFilter) return false;
      if (!q) return true;
      return (
        (s.name ?? "").toLowerCase().includes(q) ||
        (s.display_name ?? "").toLowerCase().includes(q) ||
        (s.description ?? "").toLowerCase().includes(q)
      );
    });
  }, [servicesQuery.data, search, statusFilter]);

  // 列表基座（P001-T1）：排序层（搜索/状态筛选沿用页面既有实现）
  const { sort: svcSort, toggleSort: svcToggleSort, visible: svcVisible } = useTableControls(
    services,
    {
      columns: [
        { key: "name", value: (s) => s.name ?? "" },
        { key: "description", value: (s) => s.display_name ?? s.description ?? "" },
        { key: "status", value: (s) => s.status ?? "" },
        { key: "pid", value: (s) => s.pid ?? 0 },
      ],
    },
  );

  const counts = useMemo(() => {
    const all = servicesQuery.data?.services ?? [];
    return {
      total: all.length,
      running: all.filter((s) => s.status === "running").length,
      stopped: all.filter((s) => s.status === "stopped").length,
      failed: all.filter((s) => s.status === "failed").length,
    };
  }, [servicesQuery.data]);

  const offline = !host.online;
  // Linux（systemd）与 Windows（SCM）列组不同：systemd 用 自启状态/运行时长/单元文件，
  // Windows 用 显示名/启动类型——字段语义不勉强互相解释。
  const isSystemd = host.os === "linux";
  const colCount = isSystemd ? 7 : 6;
  // 秒级精度足够；0 = 数据未到（时长为负走 — 兜底），避免 render 里取 Date.now()（纯度）
  const fetchedAt = (servicesQuery.data?.at ?? 0) / 1000;

  return (
    <div className="flex flex-col gap-4">
      {/* D4：常驻服务（Server 下发的自建服务）+ 状态实时流 */}
      <ManagedServices hostId={host.id ?? ""} />
      {/* 工具行 */}
      <div className="flex flex-wrap items-center gap-2">
        {(["all", "running", "stopped", "failed"] as const).map((s) => (
          <button
            key={s}
            type="button"
            onClick={() => setStatusFilter(s)}
            aria-pressed={statusFilter === s}
            className={`h-7 rounded-full border px-3 font-mono text-label-12 transition-colors duration-150 ${
              statusFilter === s
                ? "border-gray-1000 bg-gray-200 text-gray-1000"
                : "border-gray-500 text-gray-900 hover:border-gray-600"
            }`}
          >
            {STATUS_LABEL[s]}
          </button>
        ))}
        <div className="relative ml-auto">
          <Search size={13} strokeWidth={1.5} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-gray-900" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索服务名 / 描述"
            aria-label="搜索服务"
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
          aria-label={auto ? "关闭自动刷新" : "开启自动刷新"}
          title={auto ? "自动刷新：开" : "自动刷新：关"}
          onClick={() => setAuto((a) => !a)}
          className={`flex h-8 items-center gap-1.5 rounded-md border px-2 font-mono text-label-12 transition-colors duration-150 ${
            auto ? "border-blue-1000 text-blue-1000" : "border-gray-400 text-gray-900"
          }`}
        >
          ⏱ 15s
        </button>
        <button
          type="button"
          aria-label="刷新"
          disabled={offline}
          onClick={() => servicesQuery.refetch()}
          className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000 disabled:opacity-30"
        >
          <RefreshCw size={14} strokeWidth={1.5} className={servicesQuery.isFetching ? "animate-spin" : ""} />
        </button>
      </div>

      <div className="overflow-visible rounded-lg border border-gray-400 bg-background-100">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              {isSystemd ? (
                <>
                  <SortableTh className="w-44" label="单元" sortKey="name" sort={svcSort} onSort={svcToggleSort} />
                  <th className="px-4 py-2.5 font-normal">描述</th>
                  <SortableTh className="w-24" label="状态" sortKey="status" sort={svcSort} onSort={svcToggleSort} />
                  <th className="w-24 px-4 py-2.5 font-normal">自启</th>
                  <th className="w-28 px-4 py-2.5 font-normal">运行时长</th>
                  <SortableTh className="w-20" label="PID" sortKey="pid" sort={svcSort} onSort={svcToggleSort} />
                  <th className="w-32 px-4 py-2.5 text-right font-normal">操作</th>
                </>
              ) : (
                <>
                  <SortableTh className="w-44" label="服务名" sortKey="name" sort={svcSort} onSort={svcToggleSort} />
                  <th className="px-4 py-2.5 font-normal">显示名 / 描述</th>
                  <SortableTh className="w-24" label="状态" sortKey="status" sort={svcSort} onSort={svcToggleSort} />
                  <th className="w-24 px-4 py-2.5 font-normal">启动类型</th>
                  <SortableTh className="w-20" label="PID" sortKey="pid" sort={svcSort} onSort={svcToggleSort} />
                  <th className="w-32 px-4 py-2.5 text-right font-normal">操作</th>
                </>
              )}
            </tr>
          </thead>
          <tbody>
            {servicesQuery.isPending ? (
              <tr>
                <td colSpan={colCount} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  正在枚举系统服务…
                </td>
              </tr>
            ) : servicesQuery.isError ? (
              <tr>
                <td colSpan={colCount} className="px-4 py-10 text-center">
                  <span className="text-label-13 text-red-1000">
                    获取失败：{(servicesQuery.error as Error).message}
                  </span>
                  <button
                    type="button"
                    onClick={() => servicesQuery.refetch()}
                    className="ml-3 text-label-13 text-blue-1000 hover:underline"
                  >
                    重试
                  </button>
                </td>
              </tr>
            ) : services.length === 0 ? (
              <tr>
                <td colSpan={colCount} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  {search || statusFilter !== "all" ? "没有匹配的服务" : "未发现系统服务"}
                </td>
              </tr>
            ) : (
              svcVisible.map((svc) => (
                <tr
                  key={svc.name}
                  className="border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
                >
                  <td className="px-4 py-2 font-mono text-label-13">
                    <span className="block truncate" title={svc.name}>
                      {svc.name}
                    </span>
                    {isSystemd && svc.unit_file && (
                      <span className="block truncate font-mono text-label-12 text-gray-900" title={svc.unit_file}>
                        {svc.unit_file}
                      </span>
                    )}
                  </td>
                  {isSystemd ? (
                    <>
                      <td className="max-w-72 px-4 py-2">
                        <span className="block truncate text-label-13" title={svc.description}>
                          {svc.description || "—"}
                        </span>
                      </td>
                      <td className="px-4 py-2">
                        <SysStatusBadge status={svc.status ?? ""} />
                      </td>
                      <td className="px-4 py-2 text-label-12 text-gray-900" title={svc.enabled_state || undefined}>
                        {svc.enabled_state || "—"}
                      </td>
                      <td className="px-4 py-2 text-label-13 text-gray-900 tabular-nums">
                        {svc.since_unix && fetchedAt > 0 ? formatDuration(fetchedAt - svc.since_unix) : "—"}
                      </td>
                    </>
                  ) : (
                    <>
                      <td className="max-w-72 px-4 py-2">
                        <span
                          className="block truncate text-label-13"
                          title={svc.display_name ? `${svc.display_name}${svc.description ? " · " + svc.description : ""}` : svc.description}
                        >
                          {svc.display_name || svc.description || "—"}
                        </span>
                      </td>
                      <td className="px-4 py-2">
                        <SysStatusBadge status={svc.status ?? ""} />
                      </td>
                      <td className="px-4 py-2 text-label-12 text-gray-900">{svc.start_type || "—"}</td>
                    </>
                  )}
                  <td className="px-4 py-2 text-right font-mono text-label-13 text-gray-900 tabular-nums">
                    {svc.pid ? svc.pid : "—"}
                  </td>
                  <td className="px-4 py-2 text-right">
                    <span className="inline-flex items-center gap-1">
                      {svc.status !== "running" ? (
                        <OpBtn
                          label={`启动 ${svc.name}`}
                          disabled={offline || actionMutation.isPending}
                          spinning={pendingOp === `${svc.name}:start`}
                          onClick={() => actionMutation.mutate({ name: svc.name!, action: "start" })}
                        >
                          <Play size={13} strokeWidth={1.5} />
                        </OpBtn>
                      ) : (
                        <>
                          <OpBtn
                            label={`停止 ${svc.name}`}
                            disabled={offline || actionMutation.isPending}
                            spinning={pendingOp === `${svc.name}:stop`}
                            onClick={() => actionMutation.mutate({ name: svc.name!, action: "stop" })}
                          >
                            <Square size={13} strokeWidth={1.5} />
                          </OpBtn>
                          <OpBtn
                            label={`重启 ${svc.name}`}
                            disabled={offline || actionMutation.isPending}
                            spinning={pendingOp === `${svc.name}:restart`}
                            onClick={() => actionMutation.mutate({ name: svc.name!, action: "restart" })}
                          >
                            <RotateCw size={13} strokeWidth={1.5} />
                          </OpBtn>
                        </>
                      )}
                    </span>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center justify-between border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
          <span>
            共 {counts.total} 个服务 · {counts.running} 运行 · {counts.stopped} 停止 ·{" "}
            <span className={counts.failed > 0 ? "text-red-1000" : ""}>{counts.failed} 失败</span>
          </span>
          {servicesQuery.data?.error && (
            <span className="max-w-md truncate text-label-12 text-amber-1000" title={servicesQuery.data.error}>
              {servicesQuery.data.error}
            </span>
          )}
        </div>
      </div>
      <p className="text-label-12 text-gray-900">
        列表来自目标机系统服务实时枚举（Windows: Get-CimInstance Win32_Service / Linux: systemctl / macOS: launchctl）；
        启停操作需要 Agent 具备相应权限（Linux 通常需 root）。
      </p>
    </div>
  );
}

/** 秒数 → 人类可读时长（分钟内精确到分，天内到小时，再往上到天）。 */
function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "—";
  const m = Math.floor(seconds / 60);
  if (m < 1) return "<1 分钟";
  if (m < 60) return `${m} 分钟`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} 小时`;
  const d = Math.floor(h / 24);
  return `${d} 天`;
}

function SysStatusBadge({ status }: { status: string }) {
  const s = (status || "").toLowerCase();
  if (s === "running") {
    return (
      <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-label-13 text-green-1000">
        <span className="h-2 w-2 rounded-full bg-green-1000" />运行
      </span>
    );
  }
  if (s === "failed" || s === "stopped_list") {
    return (
      <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-label-13 text-red-1000">
        <span className="text-xs leading-none">✕</span>{status}
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-label-13 text-gray-900">
      <span className="h-2 w-2 rounded-full border border-gray-600" />
      {status || "stopped"}
    </span>
  );
}

function OpBtn({
  label,
  disabled,
  spinning,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  spinning?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="flex h-7 w-7 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000 disabled:opacity-30"
    >
      {spinning ? (
        <span className="h-3.5 w-3.5 animate-spin rounded-full border border-gray-900 border-t-gray-1000" />
      ) : (
        children
      )}
    </button>
  );
}


/** 常驻服务状态徽章色（与系统服务页同语义）。 */
function managedStatusClass(status: string): string {
  if (status === "running") return "text-green-1000";
  if (status === "failed") return "text-red-1000";
  return "text-gray-800";
}

/**
 * 常驻服务区块（D4）：Server 下发的自建服务列表 + 状态实时流。
 *
 * - 列表：GET /api/v1/services?host_id=（仅本机）；
 * - 实时：GET /api/v1/services/stream（WS）——连接即推全库快照（按 host_id 过滤），
 *   此后 agent 上报的状态变更为增量 patch（同状态重复通知幂等，以最新为准）。
 */
function ManagedServices({ hostId }: { hostId: string }) {
  const listQuery = useQuery({
    queryKey: ["services", hostId],
    queryFn: () =>
      api<{ services: ManagedService[] }>(`/api/v1/services?host_id=${hostId}&limit=100`),
  });

  // 流式状态覆盖层：service_id → 最新状态（快照/增量都写这里，渲染时合并）
  const [overrides, setOverrides] = useState<
    Record<string, { status?: string; pid?: number | null; exit_code?: number | null }>
  >({});
  const hostIdRef = useRef(hostId);
  hostIdRef.current = hostId;

  const wsStatus = useWsStream("/api/v1/services/stream", (raw) => {
    try {
      const msg = JSON.parse(raw) as
        | { snapshot: (ManagedService & { host_id: string })[] }
        | { service_id: string; status?: string; pid?: number | null; exit_code?: number | null };
      if ("snapshot" in msg) {
        // 全量快照：重建本机覆盖层（以快照为准）
        const next: Record<string, { status?: string; pid?: number | null; exit_code?: number | null }> = {};
        for (const s of msg.snapshot ?? []) {
          if (s.host_id === hostIdRef.current) {
            next[s.id] = { status: s.status, pid: s.pid, exit_code: s.exit_code };
          }
        }
        setOverrides(next);
      } else if ("service_id" in msg) {
        // 增量：patch 单个服务
        setOverrides((prev) => ({ ...prev, [msg.service_id]: { ...prev[msg.service_id], ...msg } }));
      }
    } catch {
      // 非 JSON 帧（网关错误页等）——忽略
    }
  });

  const services = useMemo(() => {
    const list = listQuery.data?.services ?? [];
    return list.map((s) => {
      const o = overrides[s.id];
      return o ? { ...s, status: o.status ?? s.status, pid: o.pid ?? s.pid, exit_code: o.exit_code ?? s.exit_code } : s;
    });
  }, [listQuery.data, overrides]);

  return (
    <section className="rounded-lg border border-gray-600 p-4">
      <div className="mb-2 flex items-center gap-2">
        <h2 className="text-title-14 font-medium">常驻服务</h2>
        <span className="text-label-12 text-gray-800">
          {wsStatus === "open" ? "实时" : wsStatus === "connecting" ? "连接中…" : "离线（列表为快照）"}
        </span>
        <button
          type="button"
          onClick={() => listQuery.refetch()}
          className="ml-auto inline-flex items-center gap-1 text-label-12 text-blue-1000 hover:underline"
        >
          <RefreshCw size={12} /> 刷新
        </button>
      </div>
      {services.length === 0 ? (
        <p className="text-label-12 text-gray-900">
          本机暂无常驻服务（Server 下发的自建服务会显示在这里，状态实时更新）。
        </p>
      ) : (
        <ul className="flex flex-col divide-y divide-gray-600">
          {services.map((s) => (
            <li key={s.id} className="flex items-center gap-3 py-2">
              <span className="text-label-13 font-medium">{s.name}</span>
              <span className={`text-label-12 ${managedStatusClass(s.status)}`}>{s.status}</span>
              {s.pid != null && <span className="text-label-12 text-gray-800">pid {s.pid}</span>}
              {s.exit_code != null && (
                <span className="text-label-12 text-gray-800">exit {s.exit_code}</span>
              )}
              <span className="ml-auto truncate font-mono text-label-12 text-gray-800" title={s.command}>
                {s.command}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
