import { useMemo, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { MoreHorizontal, Play, RotateCw, Square } from "lucide-react";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import ServiceFormDrawer from "./ServiceFormDrawer";
import LogDrawer from "./LogDrawer";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];
type Service = components["schemas"]["Service"];

interface Ctx {
  host: HostView;
}

/** 服务列表（规格 routes/services.md F40–F45）。 */
export default function Services() {
  const { host } = useOutletContext<Ctx>();
  const queryClient = useQueryClient();
  const [menuFor, setMenuFor] = useState<string | null>(null);
  const [formDrawer, setFormDrawer] = useState<{ nonce: number; service: Service | null } | null>(null);
  const [logFor, setLogFor] = useState<Service | null>(null);
  const [pendingOp, setPendingOp] = useState<string | null>(null);

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = (agentsQuery.data?.agents ?? []).find((a) => a.host_id === host.id);

  const servicesQuery = useQuery({
    queryKey: ["services", host.id],
    queryFn: () => api<{ services: Service[] }>("/api/v1/services?limit=100"),
    refetchInterval: 30_000,
  });
  const services = useMemo(
    () => (servicesQuery.data?.services ?? []).filter((s) => s.host_id === host.id),
    [servicesQuery.data, host.id],
  );

  const opMutation = useMutation({
    mutationFn: async ({ id, op }: { id: string; op: "start" | "stop" | "restart" }) => {
      setPendingOp(`${id}:${op}`);
      try {
        return await api(`/api/v1/services/${id}/${op}`, { method: "POST" });
      } finally {
        setPendingOp(null);
      }
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["services", host.id] }),
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api(`/api/v1/services/${id}`, { method: "DELETE" }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["services", host.id] }),
  });

  const counts = useMemo(() => {
    const running = services.filter((s) => s.status === "running").length;
    const failed = services.filter((s) => s.status === "failed").length;
    return { total: services.length, running, failed, stopped: services.length - running - failed };
  }, [services]);

  const offline = !host.online;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h2 className="text-label-13 text-gray-900">服务</h2>
        <button
          type="button"
          onClick={() => setFormDrawer({ nonce: Date.now(), service: null })}
          disabled={offline || !agent}
          className="flex h-8 items-center gap-1.5 rounded-md border border-gray-500 px-3 text-label-13 transition-colors duration-150 hover:bg-gray-200 disabled:opacity-40"
        >
          + 创建服务
        </button>
      </div>

      <div className="overflow-visible rounded-lg border border-gray-400">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <th className="px-4 py-2 font-normal">名称</th>
              <th className="px-4 py-2 font-normal">命令</th>
              <th className="px-4 py-2 font-normal">状态</th>
              <th className="px-4 py-2 font-normal">PID</th>
              <th className="px-4 py-2 font-normal">重启策略</th>
              <th className="px-4 py-2 text-right font-normal">操作</th>
            </tr>
          </thead>
          <tbody>
            {servicesQuery.isPending ? (
              <SkeletonRows />
            ) : services.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-4 py-10 text-center">
                  <p className="text-label-13 text-gray-900">还没有常驻服务</p>
                  <button
                    type="button"
                    onClick={() => setFormDrawer({ nonce: Date.now(), service: null })}
                    disabled={offline || !agent}
                    className="mt-3 h-8 rounded-md border border-gray-500 px-4 text-label-13 transition-colors duration-150 hover:bg-gray-200 disabled:opacity-40"
                  >
                    创建服务
                  </button>
                </td>
              </tr>
            ) : (
              services.map((svc) => (
                <tr
                  key={svc.id}
                  className="group border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
                >
                  <td className="px-4 py-2.5">
                    <button
                      type="button"
                      onClick={() => setLogFor(svc)}
                      className="text-label-14 text-blue-1000 hover:underline"
                    >
                      {svc.name}
                      {svc.status === "failed" && (
                        <span className="ml-1.5 text-amber-1000" title={`exit ${svc.exit_code ?? "?"}`}>
                          ⚠
                        </span>
                      )}
                    </button>
                  </td>
                  <td className="max-w-64 px-4 py-2.5">
                    <span
                      className="block truncate font-mono text-label-13 text-gray-900"
                      title={[svc.command, ...(svc.args ?? [])].join(" ")}
                    >
                      {svc.command} {(svc.args ?? []).join(" ")}
                    </span>
                  </td>
                  <td className="px-4 py-2.5">
                    <StatusBadge status={svc.status ?? "stopped"} />
                    {svc.status === "failed" && svc.exit_code != null && (
                      <span className="ml-2 font-mono text-label-12 text-red-1000">
                        exit {svc.exit_code}
                      </span>
                    )}
                  </td>
                  <td className="px-4 py-2.5 font-mono text-label-13 text-gray-900 tabular-nums">
                    {svc.status === "running" ? (svc.pid ?? "—") : "—"}
                  </td>
                  <td className="px-4 py-2.5 text-label-13 text-gray-900">
                    {svc.restart_policy === "always" ? "always" : "no"}
                  </td>
                  <td className="relative px-4 py-2.5 text-right">
                    <span className="inline-flex items-center gap-1">
                      {svc.status === "running" ? (
                        <>
                          <IconBtn
                            label={`停止 ${svc.name}`}
                            disabled={offline || opMutation.isPending}
                            spinning={pendingOp === `${svc.id}:stop`}
                            onClick={() => opMutation.mutate({ id: svc.id!, op: "stop" })}
                          >
                            <Square size={14} strokeWidth={1.5} />
                          </IconBtn>
                          <IconBtn
                            label={`重启 ${svc.name}`}
                            disabled={offline || opMutation.isPending}
                            spinning={pendingOp === `${svc.id}:restart`}
                            onClick={() => opMutation.mutate({ id: svc.id!, op: "restart" })}
                          >
                            <RotateCw size={14} strokeWidth={1.5} />
                          </IconBtn>
                        </>
                      ) : (
                        <IconBtn
                          label={`启动 ${svc.name}`}
                          disabled={offline || opMutation.isPending}
                          spinning={pendingOp === `${svc.id}:start`}
                          onClick={() => opMutation.mutate({ id: svc.id!, op: "start" })}
                        >
                          <Play size={14} strokeWidth={1.5} />
                        </IconBtn>
                      )}
                      <IconBtn
                        label={`更多 ${svc.name}`}
                        disabled={offline}
                        onClick={() => setMenuFor(menuFor === svc.id ? null : svc.id!)}
                      >
                        <MoreHorizontal size={14} strokeWidth={1.5} />
                      </IconBtn>
                    </span>
                    {menuFor === svc.id && (
                      <Menu
                        onEdit={() => {
                          setMenuFor(null);
                          setFormDrawer({ nonce: Date.now(), service: svc });
                        }}
                        canDelete={svc.status !== "running"}
                        onDelete={() => {
                          setMenuFor(null);
                          deleteMutation.mutate(svc.id!);
                        }}
                        onClose={() => setMenuFor(null)}
                      />
                    )}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
          共 {counts.total} 个服务 · {counts.running} 运行 · {counts.stopped} 停止 ·{" "}
          <span className={counts.failed > 0 ? "text-red-1000" : ""}>{counts.failed} 失败</span>
        </div>
      </div>

      {formDrawer && (
        <ServiceFormDrawer
          key={formDrawer.nonce}
          service={formDrawer.service}
          agentId={agent?.id}
          onClose={() => setFormDrawer(null)}
          onSaved={(svc) => {
            setFormDrawer(null);
            queryClient.invalidateQueries({ queryKey: ["services", host.id] });
            // 后端契约：创建只登记不启动——创建场景自动补一次 start（UX 预期）
            if (!formDrawer.service && svc?.id) {
              opMutation.mutate({ id: svc.id, op: "start" });
            }
          }}
        />
      )}

      {logFor && (
        <LogDrawer
          key={logFor.id}
          service={logFor}
          onClose={() => setLogFor(null)}
        />
      )}
    </div>
  );
}

function StatusBadge({ status }: { status: Service["status"] }) {
  if (status === "running") {
    return (
      <span className="inline-flex items-center gap-1.5 text-label-13 text-green-1000">
        <span className="h-2 w-2 rounded-full bg-green-1000" />运行
      </span>
    );
  }
  if (status === "failed") {
    return (
      <span className="inline-flex items-center gap-1.5 text-label-13 text-red-1000">
        <span className="text-xs leading-none">✕</span>失败
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1.5 text-label-13 text-gray-900">
      <span className="h-2 w-2 rounded-full border border-gray-600" />停止
    </span>
  );
}

function IconBtn({
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

function Menu({
  onEdit,
  canDelete,
  onDelete,
  onClose,
}: {
  onEdit: () => void;
  canDelete: boolean;
  onDelete: () => void;
  onClose: () => void;
}) {
  return (
    <>
      <button
        type="button"
        aria-label="关闭菜单"
        onClick={onClose}
        className="fixed inset-0 z-30 cursor-default"
      />
      <div className="absolute right-4 top-10 z-40 w-36 rounded-xl border border-gray-400 bg-background-100 py-1 shadow-lg">
        <button
          type="button"
          onClick={onEdit}
          className="block w-full px-3 py-2 text-left text-label-13 transition-colors duration-150 hover:bg-gray-200"
        >
          编辑
        </button>
        <button
          type="button"
          onClick={onDelete}
          disabled={!canDelete}
          title={canDelete ? undefined : "先停止服务再删除"}
          className="block w-full px-3 py-2 text-left text-label-13 text-red-1000 transition-colors duration-150 hover:bg-gray-200 disabled:cursor-not-allowed disabled:text-gray-900 disabled:opacity-50"
        >
          删除
        </button>
      </div>
    </>
  );
}

function SkeletonRows() {
  return (
    <>
      {Array.from({ length: 5 }).map((_, i) => (
        <tr key={i} className="border-b border-gray-400/60 last:border-0">
          <td colSpan={6} className="px-4 py-3">
            <div className={`h-4 animate-pulse rounded ${i % 2 ? "bg-gray-100" : "bg-gray-200"}`} style={{ width: `${40 + (i * 11) % 50}%` }} />
          </td>
        </tr>
      ))}
    </>
  );
}
