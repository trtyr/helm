import { useOutletContext } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { toast } from "../../lib/toast";
import { ifaceKind } from "../../lib/net";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];
type NetInterface = components["schemas"]["NetInterface"];

interface Ctx {
  host: HostView;
}

/** 网络信息（规格 network.md F49：接口卡片网格 + 地址 chip 复制 + 主机名行）。 */
export default function Network() {
  const { host } = useOutletContext<Ctx>();

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = (agentsQuery.data?.agents ?? []).find((a) => a.host_id === host.id);

  const netQuery = useQuery({
    queryKey: ["net-info", agent?.id],
    queryFn: async () => {
      const r = await api<{ hostname?: string; interfaces?: NetInterface[] }>("/api/v1/net/info", {
        method: "POST",
        body: { agent_id: agent!.id },
      });
      return { at: Date.now(), hostname: r.hostname ?? "", interfaces: r.interfaces ?? [] };
    },
    enabled: !!agent && host.online,
  });

  async function copy(text: string) {
    await navigator.clipboard.writeText(text);
    toast(`已复制 ${text}`);
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h2 className="text-label-13 text-gray-900">网络接口</h2>
        <button
          type="button"
          aria-label="刷新"
          onClick={() => netQuery.refetch()}
          disabled={!host.online || netQuery.isFetching}
          className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000 disabled:opacity-30"
        >
          <RefreshCw size={14} strokeWidth={1.5} className={netQuery.isFetching ? "animate-spin" : ""} />
        </button>
      </div>

      {netQuery.isPending ? (
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="h-28 animate-pulse rounded-lg border border-gray-400 bg-gray-100" />
          ))}
        </div>
      ) : netQuery.isError ? (
        <div className="rounded-lg border border-gray-400 p-8 text-center">
          <p className="text-label-13 text-red-1000">网络信息获取失败：{(netQuery.error as Error).message}</p>
          <button
            type="button"
            onClick={() => netQuery.refetch()}
            className="mt-3 h-8 rounded-md border border-gray-500 px-4 text-label-13 transition-colors duration-150 hover:bg-gray-200"
          >
            重试
          </button>
        </div>
      ) : netQuery.data!.interfaces.length === 0 ? (
        <div className="rounded-lg border border-dashed border-gray-500 p-8 text-center text-label-13 text-gray-900">
          暂无网络信息
        </div>
      ) : (
        <>
          {!host.online && (
            <p className="font-mono text-label-12 text-amber-1000">
              快照 · {new Date(netQuery.data!.at).toLocaleTimeString()}
            </p>
          )}
          <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
            {netQuery.data!.interfaces.map((iface) => (
              <div key={iface.name} className="rounded-lg border border-gray-400 p-4">
                <div className="flex items-baseline gap-2">
                  <h3 className="font-mono text-label-16">{iface.name}</h3>
                  {ifaceKind(iface.name ?? "") && (
                    <span className="text-label-12 text-gray-900">{ifaceKind(iface.name ?? "")}</span>
                  )}
                </div>
                <div className="mt-3 flex flex-wrap gap-2">
                  {(iface.addrs ?? []).map((addr) => (
                    <button
                      key={addr}
                      type="button"
                      onClick={() => copy(addr)}
                      title="点击复制"
                      className="flex items-center gap-1.5 rounded border border-gray-400 bg-gray-100 px-2 py-1 font-mono text-label-12 transition-colors duration-150 hover:border-gray-500 hover:bg-gray-200"
                    >
                      {addr}
                      {addr.includes(":") && (
                        <span className="rounded-sm border border-gray-500 px-0.5 text-[10px] leading-tight text-gray-900">
                          v6
                        </span>
                      )}
                    </button>
                  ))}
                </div>
              </div>
            ))}
          </div>
          {netQuery.data!.hostname && (
            <p className="mt-2">
              主机名{" "}
              <button
                type="button"
                onClick={() => copy(netQuery.data!.hostname)}
                title="点击复制"
                className="font-mono text-label-14 text-blue-1000 hover:underline"
              >
                {netQuery.data!.hostname}
              </button>
            </p>
          )}
        </>
      )}
    </div>
  );
}
