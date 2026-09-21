import { useMemo, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Play, RefreshCw, RotateCw, Square } from "lucide-react";
import { api, pickAgent } from "../../api/client";
import { toast } from "../../lib/toast";
import { useTableControls } from "../../lib/useTableControls";
import { SortableTh, TableToolbar } from "../../components/tableControls";
import { STATUS_LABEL, formatDuration, type Agent, type Ctx, type SysService } from "./services/shared";
import { ManagedServices, OpBtn, SysStatusBadge } from "./services/ServiceParts";

/**
 * 系统服务（实时发现：Windows Service / systemd / launchctl，无需手工创建）。
 * 启动/停止/重启直接作用于目标机原生服务。
 *
 * G13 拆分（2026-09-21）：原为 510 行单文件。现拆为——`services/shared.ts`（类型与纯工具）/
 * `services/ServiceParts.tsx`（状态徽章、操作按钮、常驻服务区块）；本文件保留状态、数据与表格编排。
 */
export default function Services() {
  const { host } = useOutletContext<Ctx>();
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

  // 列表基座（P001-T1）：三层统一走 hook（搜索 / 状态枚举 / 排序）
  const serviceList = useMemo(() => servicesQuery.data?.services ?? [], [servicesQuery.data]);
  const {
    search: svcSearch,
    setSearch: svcSetSearch,
    sort: svcSort,
    toggleSort: svcToggleSort,
    enumFilters: svcEnumFilters,
    setEnumFilter: svcSetEnumFilter,
    visible: svcVisible,
  } = useTableControls(serviceList, {
    columns: [
      { key: "name", value: (s) => s.name ?? "" },
      { key: "description", value: (s) => s.display_name ?? s.description ?? "" },
      {
        key: "status",
        value: (s) => s.status ?? "",
        enumOptions: () => [
          { value: "running", label: STATUS_LABEL.running },
          { value: "stopped", label: STATUS_LABEL.stopped },
          { value: "failed", label: STATUS_LABEL.failed },
        ],
        matchesEnum: (s, v) => s.status === v,
      },
      { key: "pid", value: (s) => s.pid ?? 0 },
    ],
    searchText: (s) => `${s.name ?? ""} ${s.display_name ?? ""} ${s.description ?? ""}`,
  });

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
      {/* 工具行（列表基座：搜索 + 状态枚举下拉统一走基座工具栏） */}
      <TableToolbar
        search={svcSearch}
        onSearch={svcSetSearch}
        filters={[
          {
            key: "status",
            label: "状态",
            value: svcEnumFilters.status ?? "",
            options: [
              { value: "running", label: STATUS_LABEL.running },
              { value: "stopped", label: STATUS_LABEL.stopped },
              { value: "failed", label: STATUS_LABEL.failed },
            ],
            onChange: (v) => svcSetEnumFilter("status", v),
          },
        ]}
      >
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
      </TableToolbar>

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
            ) : serviceList.length === 0 ? (
              <tr>
                <td colSpan={colCount} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  {svcSearch || svcEnumFilters.status ? "没有匹配的服务" : "未发现系统服务"}
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
