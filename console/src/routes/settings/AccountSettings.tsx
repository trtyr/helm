import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { api, ApiError } from "../../api/client";
import { getToken, clearToken } from "../../api/token";
import { jwtSub } from "../../lib/jwt";

interface Account {
  username: string;
  role: string;
  created_at: string;
}

/** /settings/account：账号管理（单用户）——权威身份、改用户名 / 改密码（校验当前密码，成功后强制重登）。 */
export default function AccountSettings() {
  const navigate = useNavigate();
  const token = getToken() ?? "";
  const jwtUser = jwtSub(token) ?? "—";

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

  /** 凭证变更成功 → 清 token 回登录页（toast 在登录页不可见故用 notice 参数）。 */
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

  return (
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
  );
}
