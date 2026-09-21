// 服务页的视图部件（G13 拆分，2026-09-21）——自 `Services.tsx` 拆出：
// 状态徽章、操作按钮、常驻服务区块（含状态实时流合并）。
import { useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { api } from "../../../api/client";
import { useWsStream } from "../../../api/ws";
import { managedStatusClass, type ManagedService } from "./shared";

export function SysStatusBadge({ status }: { status: string }) {
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

export function OpBtn({
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

/**
 * 常驻服务区块（D4）：Server 下发的自建服务列表 + 状态实时流。
 *
 * - 列表：GET /api/v1/services?host_id=（仅本机）；
 * - 实时：GET /api/v1/services/stream（WS）——连接即推全库快照（按 host_id 过滤），
 *   此后 agent 上报的状态变更为增量 patch（同状态重复通知幂等，以最新为准）。
 */
export function ManagedServices({ hostId }: { hostId: string }) {
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
