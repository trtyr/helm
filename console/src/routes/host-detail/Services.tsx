import { useMemo, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Play, RefreshCw, RotateCw, Search, Square, X } from "lucide-react";
import type { components } from "../../api/schema";
import { api, pickAgent } from "../../api/client";
import { toast } from "../../lib/toast";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];
type SysService = components["schemas"]["SysService"];

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

  return (
    <div className="flex flex-col gap-4">
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
              <th className="w-44 px-4 py-2.5 font-normal">服务名</th>
              <th className="px-4 py-2.5 font-normal">显示名 / 描述</th>
              <th className="w-24 px-4 py-2.5 font-normal">状态</th>
              <th className="w-24 px-4 py-2.5 font-normal">启动类型</th>
              <th className="w-20 px-4 py-2.5 font-normal">PID</th>
              <th className="w-32 px-4 py-2.5 text-right font-normal">操作</th>
            </tr>
          </thead>
          <tbody>
            {servicesQuery.isPending ? (
              <tr>
                <td colSpan={6} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  正在枚举系统服务…
                </td>
              </tr>
            ) : servicesQuery.isError ? (
              <tr>
                <td colSpan={6} className="px-4 py-10 text-center">
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
                <td colSpan={6} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  {search || statusFilter !== "all" ? "没有匹配的服务" : "未发现系统服务"}
                </td>
              </tr>
            ) : (
              services.map((svc) => (
                <tr
                  key={svc.name}
                  className="border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
                >
                  <td className="px-4 py-2 font-mono text-label-13">
                    <span className="block truncate" title={svc.name}>
                      {svc.name}
                    </span>
                  </td>
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
