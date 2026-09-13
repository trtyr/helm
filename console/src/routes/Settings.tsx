import { useEffect, useState } from "react";
import { copyText } from "../lib/clipboard";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { ChevronRight, Copy } from "lucide-react";
import { api, ApiError } from "../api/client";
import { getToken, clearToken } from "../api/token";
import { useTheme } from "../hooks/useTheme";
import { ApiKeysPanel } from "../components/ApiKeysPanel";
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

/** /settings 设置：账号管理（单用户）+ 外观 + 会话 + 系统。
 *  账号卡：/auth/me 权威身份、改用户名 / 改密码（校验当前密码，成功后强制重登）。 */

interface Account {
  username: string;
  role: string;
  created_at: string;
}

export default function Settings() {
  const { theme, toggle } = useTheme();
  const navigate = useNavigate();
  const token = getToken() ?? "";
  const exp = jwtExp(token);
  const [nowSec, setNowSec] = useState<number | null>(null);
  const [confirmLogout, setConfirmLogout] = useState(false);
  const [installOpen, setInstallOpen] = useState(false);
  const [connMode, setConnMode] = useState<"reverse" | "forward">("reverse");
  const [agentToken, setAgentToken] = useState("");

  // 账号管理表单状态
  const [newUsername, setNewUsername] = useState("");
  const [usernamePassword, setUsernamePassword] = useState("");
  const [usernameBusy, setUsernameBusy] = useState(false);
  const [usernameError, setUsernameError] = useState<string | null>(null);
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [passwordBusy, setPasswordBusy] = useState(false);
  const [passwordError, setPasswordError] = useState<string | null>(null);

  const meQuery = useQuery({
    queryKey: ["me"],
    queryFn: () => api<{ account: Account }>("/api/v1/auth/me", { redirectOn401: true }),
    retry: false,
  });
  const account = meQuery.data?.account;
  const jwtUser = jwtSub(token) ?? "—";

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

  /** 凭证变更成功 → 清 token 回登录页（带提示；toast 在登录页不可见故用 notice 参数）。 */
  function reloginWithNotice(notice: string) {
    clearToken();
    navigate(`/login?notice=${encodeURIComponent(notice)}`);
  }

  function errorMessage(err: unknown, unauthorized: string, fallback: string): string {
    if (err instanceof TypeError) return "无法连接服务器";
    if (err instanceof ApiError) {
      if (err.status === 401) return unauthorized;
      if (err.status === 400) return fallback;
    }
    if (err instanceof Error && err.message) return err.message;
    return fallback;
  }

  async function submitUsername(e: React.FormEvent) {
    e.preventDefault();
    setUsernameError(null);
    const name = newUsername.trim();
    if (!name) {
      setUsernameError("新用户名不能为空");
      return;
    }
    setUsernameBusy(true);
    try {
      await api<{ ok: boolean }>("/api/v1/auth/change-username", {
        method: "POST",
        body: { current_password: usernamePassword, new_username: name },
        // 401（当前密码错）走行内提示，不触发全局跳登录
        redirectOn401: false,
      });
      reloginWithNotice(`用户名已改为「${name}」，请重新登录`);
    } catch (err) {
      setUsernameError(errorMessage(err, "当前密码错误", "修改失败（用户名可能已被占用）"));
    } finally {
      setUsernameBusy(false);
    }
  }

  async function submitPassword(e: React.FormEvent) {
    e.preventDefault();
    setPasswordError(null);
    if (newPassword.length < 6) {
      setPasswordError("新密码至少 6 个字符");
      return;
    }
    if (newPassword !== confirmPassword) {
      setPasswordError("两次输入的新密码不一致");
      return;
    }
    setPasswordBusy(true);
    try {
      await api<{ ok: boolean }>("/api/v1/auth/change-password", {
        method: "POST",
        body: { current_password: currentPassword, new_password: newPassword },
        // 401（当前密码错）走行内提示，不触发全局跳登录
        redirectOn401: false,
      });
      reloginWithNotice("密码已更新，请重新登录");
    } catch (err) {
      setPasswordError(errorMessage(err, "当前密码错误", "修改失败（新密码需至少 6 个字符）"));
    } finally {
      setPasswordBusy(false);
    }
  }

  const installCmd =
    connMode === "reverse"
      ? `helm-agent --agent-id <id> --server-addr <server-grpc-addr> --token ${agentToken || "<token>"}`
      : `helm-agent --conn-mode forward --server-addr <server-grpc-addr> --cert-dir <dir> --token ${agentToken || "<token>"}`;

  return (
    <div className="mx-auto flex w-full max-w-[640px] flex-col gap-6">
      <h1 className="text-heading-24">设置</h1>

      {/* 账号（单用户） */}
      <section className="rounded-lg border border-gray-400 p-6">
        <div className="flex items-baseline justify-between">
          <h2 className="text-heading-16">账号</h2>
          <span className="text-label-12 text-gray-900">单用户模式</span>
        </div>
        <dl className="mt-4 flex flex-col">
          <div className="flex h-10 items-center">
            <dt className="w-32 text-label-13 text-gray-900">用户名</dt>
            <dd className="font-mono text-label-14">
              {account?.username ?? jwtUser}
              {meQuery.isError && <span className="ml-2 text-amber-1000">（来自本地凭证）</span>}
            </dd>
          </div>
          <div className="flex h-10 items-center">
            <dt className="w-32 text-label-13 text-gray-900">角色</dt>
            <dd className="font-mono text-label-14">
              {account ? (account.role === "admin" ? "admin（管理员）" : account.role) : "—"}
            </dd>
          </div>
          <div className="flex h-10 items-center">
            <dt className="w-32 text-label-13 text-gray-900">创建时间</dt>
            <dd className="font-mono text-label-14">
              {account ? new Date(account.created_at).toLocaleString() : "—"}
            </dd>
          </div>
        </dl>

        {/* 修改用户名 */}
        <form onSubmit={submitUsername} className="mt-4 rounded-lg border border-gray-400 p-4">
          <p className="text-label-13 text-gray-1000">修改用户名</p>
          <div className="mt-3 flex flex-col gap-2 sm:flex-row">
            <input
              value={newUsername}
              onChange={(e) => setNewUsername(e.target.value)}
              placeholder="新用户名"
              autoComplete="username"
              disabled={usernameBusy}
              className="h-8 flex-1 rounded-md border border-gray-400 bg-gray-100 px-3 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
            <input
              type="password"
              value={usernamePassword}
              onChange={(e) => setUsernamePassword(e.target.value)}
              placeholder="当前密码"
              autoComplete="current-password"
              disabled={usernameBusy}
              className="h-8 flex-1 rounded-md border border-gray-400 bg-gray-100 px-3 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
            <button
              type="submit"
              disabled={usernameBusy || !newUsername.trim() || !usernamePassword}
              className="h-8 rounded-md bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-50"
            >
              {usernameBusy ? "保存中…" : "保存"}
            </button>
          </div>
          {usernameError && (
            <p className="mt-2 text-label-13 text-red-1000" role="alert">
              ⚠ {usernameError}
            </p>
          )}
          <p className="mt-2 text-label-12 text-gray-900">修改后当前登录立即失效，需用新用户名重新登录</p>
        </form>

        {/* 修改密码 */}
        <form onSubmit={submitPassword} className="mt-3 rounded-lg border border-gray-400 p-4">
          <p className="text-label-13 text-gray-1000">修改密码</p>
          <div className="mt-3 flex flex-col gap-2 sm:flex-row">
            <input
              type="password"
              value={currentPassword}
              onChange={(e) => setCurrentPassword(e.target.value)}
              placeholder="当前密码"
              autoComplete="current-password"
              disabled={passwordBusy}
              className="h-8 flex-1 rounded-md border border-gray-400 bg-gray-100 px-3 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
            <input
              type="password"
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
              placeholder="新密码（≥ 6 字符）"
              autoComplete="new-password"
              disabled={passwordBusy}
              className="h-8 flex-1 rounded-md border border-gray-400 bg-gray-100 px-3 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
            <input
              type="password"
              value={confirmPassword}
              onChange={(e) => setConfirmPassword(e.target.value)}
              placeholder="确认新密码"
              autoComplete="new-password"
              disabled={passwordBusy}
              className="h-8 flex-1 rounded-md border border-gray-400 bg-gray-100 px-3 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
            <button
              type="submit"
              disabled={passwordBusy || !currentPassword || !newPassword || !confirmPassword}
              className="h-8 rounded-md bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-50"
            >
              {passwordBusy ? "更新中…" : "更新"}
            </button>
          </div>
          {passwordError && (
            <p className="mt-2 text-label-13 text-red-1000" role="alert">
              ⚠ {passwordError}
            </p>
          )}
          <p className="mt-2 text-label-12 text-gray-900">修改后需用新密码重新登录</p>
        </form>
      </section>

      {/* API 凭证（scope 化，供 AI / 脚本） */}
      <ApiKeysPanel />

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
            <dd className="font-mono text-label-14">{jwtUser}</dd>
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
