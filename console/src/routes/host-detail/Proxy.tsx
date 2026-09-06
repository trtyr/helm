import { useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, X } from "lucide-react";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { toast } from "../../lib/toast";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];
type ProxyView = components["schemas"]["ProxyView"];

interface Ctx {
  host: HostView;
}

const DEFAULT_LISTEN = "127.0.0.1:1080";

/** /hosts/:id/proxy：为该主机开启 SOCKS5 跳板——Server 本地监听，
 * 流量经目标机出站，可访问仅目标机可达的网络。 */
export default function Proxy() {
  const { host } = useOutletContext<Ctx>();
  const queryClient = useQueryClient();
  const [listenAddr, setListenAddr] = useState(DEFAULT_LISTEN);

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = (agentsQuery.data?.agents ?? []).find((a) => a.host_id === host.id);

  const listQuery = useQuery({
    queryKey: ["proxies"],
    queryFn: () => api<{ proxies: ProxyView[] }>("/api/v1/proxies"),
    refetchInterval: 10_000,
  });
  // 只展示本主机的代理实例
  const mine = (listQuery.data?.proxies ?? []).filter((p) => p.agent_id === agent?.id);

  const createMutation = useMutation({
    mutationFn: () =>
      api<ProxyView>("/api/v1/proxies", {
        method: "POST",
        body: { agent_id: agent?.id, listen_addr: listenAddr },
      }),
    onSuccess: () => {
      toast(`SOCKS5 已开启：${listenAddr}`);
      queryClient.invalidateQueries({ queryKey: ["proxies"] });
    },
    onError: (e) => toast((e as Error).message, "error"),
  });

  const stopMutation = useMutation({
    mutationFn: (id: string) => api(`/api/v1/proxies/${id}`, { method: "DELETE" }),
    onSuccess: () => {
      toast("代理已停止");
      queryClient.invalidateQueries({ queryKey: ["proxies"] });
    },
    onError: (e) => toast((e as Error).message, "error"),
  });

  const offline = !host.online;

  return (
    <div className="flex flex-col gap-4">
      {/* 开启表单 */}
      <div className="flex flex-wrap items-end gap-3 rounded-lg border border-gray-400 bg-background-100 p-4">
        <div>
          <label className="block text-label-13 text-gray-900" htmlFor="proxy-addr">
            SOCKS5 监听地址（Server 侧）
          </label>
          <input
            id="proxy-addr"
            value={listenAddr}
            onChange={(e) => setListenAddr(e.target.value)}
            placeholder="127.0.0.1:1080"
            className="mt-1.5 h-8 w-48 rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
        </div>
        <button
          type="button"
          disabled={offline || !agent || createMutation.isPending}
          onClick={() => createMutation.mutate()}
          className="flex h-8 items-center gap-1.5 rounded-md bg-gray-700 px-3 text-label-13 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-40"
        >
          <Plus size={14} strokeWidth={1.5} />
          开启代理
        </button>
      </div>

      {/* 列表 */}
      <div className="overflow-hidden rounded-lg border border-gray-400 bg-background-100">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <th className="w-14 whitespace-nowrap px-3 py-2.5 font-normal">状态</th>
              <th className="w-full max-w-0 px-4 py-2.5 font-normal">SOCKS5 监听地址</th>
              <th className="w-12 px-3 py-2.5 text-right font-normal">操作</th>
            </tr>
          </thead>
          <tbody>
            {listQuery.isPending ? (
              <tr>
                <td colSpan={3} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  加载中…
                </td>
              </tr>
            ) : mine.length === 0 ? (
              <tr>
                <td colSpan={3} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  暂无活跃代理——填好监听地址后点「开启代理」
                </td>
              </tr>
            ) : (
              mine.map((p) => (
                <tr
                  key={p.id}
                  className="border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
                >
                  <td className="px-3 py-2.5">
                    <span className="inline-block h-2 w-2 rounded-full bg-green-1000" />
                  </td>
                  <td className="max-w-0 px-4 py-2.5">
                    <span className="block truncate font-mono text-label-13 text-blue-1000">
                      socks5://{p.listen_addr}
                    </span>
                  </td>
                  <td className="px-3 py-2.5 text-right">
                    <button
                      type="button"
                      aria-label={`停止 ${p.listen_addr}`}
                      title="停止"
                      onClick={() => stopMutation.mutate(p.id!)}
                      disabled={stopMutation.isPending}
                      className="flex h-7 w-7 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-red-1000 disabled:opacity-30"
                    >
                      <X size={14} strokeWidth={1.5} />
                    </button>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
          共 {mine.length} 个活跃代理
        </div>
      </div>

      <p className="text-label-12 text-gray-900">
        使用：curl --socks5-hostname 监听地址 http://目标 —— 流量经本机出站，
        可访问仅本机可达的网络（代理为内存态，Server 重启后需重新开启）。
      </p>
    </div>
  );
}
