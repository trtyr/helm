import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { ChevronRight, Copy } from "lucide-react";
import { api } from "../api/client";
import { getToken, clearToken } from "../api/token";
import { useTheme } from "../hooks/useTheme";
import { toast } from "../lib/toast";

/** JWT exp 本地解码（atob payload；无效 token 返回 null）。 */
function jwtExp(token: string): number | null {
  try {
    const payload = JSON.parse(atob(token.split(".")[1])) as { exp?: number };
    return typeof payload.exp === "number" ? payload.exp : null;
  } catch {
    return null;
  }
}

function jwtSub(token: string): string | null {
  try {
    const payload = JSON.parse(atob(token.split(".")[1])) as { sub?: string };
    return payload.sub ?? null;
  } catch {
    return null;
  }
}

/** /settings 设置（规格 settings.md F04/F69：居中三卡，全部只读 + 退出登录）。 */
export default function Settings() {
  const { theme, toggle } = useTheme();
  const navigate = useNavigate();
  const token = getToken() ?? "";
  const exp = jwtExp(token);
  const user = jwtSub(token) ?? "—";
  const [nowSec, setNowSec] = useState<number | null>(null);
  const [confirmLogout, setConfirmLogout] = useState(false);
  const [installOpen, setInstallOpen] = useState(false);
  const [connMode, setConnMode] = useState<"reverse" | "forward">("reverse");
  const [agentToken, setAgentToken] = useState("");

  // 剩余时间每分钟重算（now 异步侧）
  useEffect(() => {
    const t = setInterval(() => setNowSec(Math.floor(Date.now() / 1000)), 60_000);
    return () => clearInterval(t);
  }, []);

  const healthQuery = useQuery({
    queryKey: ["healthz"],
    queryFn: () => api<{ status?: string }>("/healthz"),
    retry: false,
  });

  const apiBase = import.meta.env.VITE_API_BASE || location.origin;
  const remainHours = exp && nowSec ? Math.max(0, (exp - nowSec) / 3600) : null;

  function logout() {
    clearToken();
    navigate("/login");
  }

  const installCmd =
    connMode === "reverse"
      ? `helm-agent --agent-id <id> --server-addr <server-grpc-addr> --token ${agentToken || "<token>"}`
      : `helm-agent --conn-mode forward --server-addr <server-grpc-addr> --cert-dir <dir> --token ${agentToken || "<token>"}`;

  return (
    <div className="mx-auto flex w-full max-w-[640px] flex-col gap-6">
      <h1 className="text-heading-24">设置</h1>

      {/* 外观 */}
      <section className="rounded-lg border border-gray-400 p-6">
        <h2 className="text-heading-16">外观</h2>
        <div className="mt-4 flex items-center gap-4">
          <span className="text-label-14">主题</span>
          <div className="flex rounded-md border border-gray-500 p-0.5">
            <button
              type="button"
              onClick={() => theme === "light" && toggle()}
              aria-pressed={theme === "dark"}
              className={`h-7 rounded px-4 text-label-13 transition-colors duration-150 ${
                theme === "dark" ? "bg-gray-200 text-gray-1000" : "text-gray-900 hover:text-gray-1000"
              }`}
            >
              ● 暗色
            </button>
            <button
              type="button"
              onClick={() => theme === "dark" && toggle()}
              aria-pressed={theme === "light"}
              className={`h-7 rounded px-4 text-label-13 transition-colors duration-150 ${
                theme === "light" ? "bg-gray-200 text-gray-1000" : "text-gray-900 hover:text-gray-1000"
              }`}
            >
              ○ 亮色
            </button>
          </div>
        </div>
        <p className="mt-2 text-label-12 text-gray-900">默认暗色，跟随本浏览器持久化</p>
      </section>

      {/* 会话 */}
      <section className="rounded-lg border border-gray-400 p-6">
        <h2 className="text-heading-16">会话</h2>
        <dl className="mt-4 flex flex-col">
          <div className="flex h-10 items-center">
            <dt className="w-32 text-label-13 text-gray-900">当前用户</dt>
            <dd className="font-mono text-label-14">{user}</dd>
          </div>
          <div className="flex h-10 items-center">
            <dt className="w-32 text-label-13 text-gray-900">Token 过期</dt>
            <dd className="font-mono text-label-14">
              {exp ? new Date(exp * 1000).toLocaleString() : "—"}
              {remainHours != null && (
                <span className="text-gray-900"> · 剩余 {remainHours.toFixed(1)}h</span>
              )}
            </dd>
          </div>
        </dl>
        <p className="mt-2 text-label-12 text-gray-900">续期 = 重新登录（当前无 refresh 端点）</p>
        <button
          type="button"
          onClick={() => setConfirmLogout(true)}
          className="mt-4 h-8 rounded-md border border-red-1000 px-4 text-label-14 text-red-1000 transition-colors duration-150 hover:bg-red-1000 hover:text-background-100"
        >
          退出登录
        </button>
      </section>

      {/* 系统 */}
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
                  navigator.clipboard.writeText(apiBase);
                  toast("已复制");
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
                  navigator.clipboard.writeText(installCmd);
                  toast("已复制完整命令");
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

      {/* 退出确认 */}
      {confirmLogout && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <button
            type="button"
            aria-label="关闭"
            onClick={() => setConfirmLogout(false)}
            className="absolute inset-0 bg-black/40"
          />
          <div className="relative z-10 w-[360px] rounded-xl border border-gray-400 bg-background-100 p-6">
            <h2 className="text-heading-16">退出登录</h2>
            <p className="mt-3 text-label-13 text-gray-900">确认退出？</p>
            <div className="mt-6 flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setConfirmLogout(false)}
                className="h-8 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200"
              >
                取消
              </button>
              <button
                type="button"
                onClick={logout}
                className="h-8 rounded-md border border-red-1000 px-4 text-label-14 text-red-1000 transition-colors duration-150 hover:bg-red-1000 hover:text-background-100"
              >
                退出
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
