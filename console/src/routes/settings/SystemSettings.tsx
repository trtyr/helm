import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronRight, Copy } from "lucide-react";
import { api } from "../../api/client";
import { copyText } from "../../lib/clipboard";
import { toast } from "../../lib/toast";

/** /settings/system：系统——控制台版本、后端地址、心跳参数与 Agent 安装命令。 */
export default function SystemSettings() {
  const [installOpen, setInstallOpen] = useState(false);
  const [connMode, setConnMode] = useState<"reverse" | "forward">("reverse");
  const [agentToken, setAgentToken] = useState("");

  const healthQuery = useQuery({
    queryKey: ["healthz"],
    queryFn: () => api<{ status?: string }>("/healthz"),
    retry: false,
  });

  const apiBase = import.meta.env.VITE_API_BASE || location.origin;

  const installCmd =
    connMode === "reverse"
      ? `helm-agent --agent-id <id> --server-addr <server-grpc-addr> --token ${agentToken || "<token>"}`
      : `helm-agent --conn-mode forward --server-addr <server-grpc-addr> --cert-dir <dir> --token ${agentToken || "<token>"}`;

  return (
    <section className="rounded-lg border border-gray-400 p-6">
      <h2 className="text-heading-16">系统</h2>
      <dl className="mt-4 flex flex-col">
        <div className="flex h-10 items-center">
          <dt className="w-32 text-label-13 text-gray-900">控制台版本</dt>
          <dd className="font-mono text-label-14">helm-console v0.1.0</dd>
        </div>
        <div className="flex h-10 items-center">
          <dt className="w-32 text-label-13 text-gray-900">后端地址</dt>
          <dd className="flex items-center gap-2 font-mono text-label-14">
            {apiBase}
            <button
              type="button"
              aria-label="复制后端地址"
              onClick={() => {
                copyText(apiBase).then((ok) => toast(ok ? "已复制" : "复制失败", ok ? undefined : "warn"));
              }}
              className="text-gray-900 transition-colors duration-150 hover:text-gray-1000"
            >
              <Copy size={12} strokeWidth={1.5} />
            </button>
          </dd>
        </div>
        <div className="flex h-10 items-center">
          <dt className="w-32 text-label-13 text-gray-900">心跳超时</dt>
          <dd className="font-mono text-label-14">
            {healthQuery.isError ? (
              <span className="text-amber-1000">后端不可达</span>
            ) : (
              "30s · 会话空闲超时 300s"
            )}
          </dd>
        </div>
      </dl>

      {/* Agent 安装命令折叠块 */}
      <button
        type="button"
        onClick={() => setInstallOpen((v) => !v)}
        aria-expanded={installOpen}
        className="mt-3 flex items-center gap-1 text-label-13 text-gray-900 transition-colors duration-150 hover:text-gray-1000"
      >
        <ChevronRight
          size={13}
          strokeWidth={1.5}
          className={`transition-transform duration-150 ${installOpen ? "rotate-90" : ""}`}
        />
        Agent 安装命令
      </button>
      {installOpen && (
        <div className="mt-3 rounded-lg border border-gray-400 p-4">
          <div className="flex rounded-md border border-gray-500 p-0.5">
            {(["reverse", "forward"] as const).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => setConnMode(m)}
                className={`h-7 flex-1 rounded text-label-13 transition-colors duration-150 ${
                  connMode === m ? "bg-gray-200 text-gray-1000" : "text-gray-900 hover:text-gray-1000"
                }`}
              >
                {m === "reverse" ? "反向" : "正向（forward）"}
              </button>
            ))}
          </div>
          <input
            value={agentToken}
            onChange={(e) => setAgentToken(e.target.value)}
            placeholder="粘贴 token（控制台不持有全局 token——安全边界）"
            aria-label="Agent token"
            className="mt-3 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
          <div className="relative mt-3">
            <pre className="overflow-x-auto rounded-md bg-[#0d0d0d] px-3 py-2.5 font-mono text-label-12 text-gray-100">
              {installCmd.replace(agentToken ? agentToken : /<token>/, agentToken ? "••••••••" : "<token>")}
            </pre>
            <button
              type="button"
              aria-label="复制安装命令"
              onClick={() => {
                copyText(installCmd).then((ok) => toast(ok ? "已复制完整命令" : "复制失败", ok ? undefined : "warn"));
              }}
              className="absolute right-2 top-2 text-gray-900 transition-colors duration-150 hover:text-gray-1000"
              title="复制真实命令（含 token）"
            >
              <Copy size={12} strokeWidth={1.5} />
            </button>
          </div>
          {connMode === "forward" && (
            <p className="mt-2 text-label-12 text-gray-900">
              正向模式需预置 mTLS 证书（离线签发），详见后端仓 docs/run-and-deploy.md
            </p>
          )}
        </div>
      )}
    </section>
  );
}
