import { useState } from "react";
import { Navigate, useLocation, useNavigate } from "react-router-dom";
import { Eye, EyeOff } from "lucide-react";
import { api } from "../../api/client";
import { getToken, setToken } from "../../api/token";

interface LoginResponse {
  token: string;
}

/** /login（规格 routes/login.md）：居中窄卡 360px、行内错误、spinner、已登录重定向。
 *  M1 适配：默认跳转目标为 `/`（→ /hosts）——dashboard 路由在 M4 提供。 */
export default function Login() {
  const navigate = useNavigate();
  const location = useLocation();
  const redirectParam = new URLSearchParams(location.search).get("redirect");
  const redirect = redirectParam ? decodeURIComponent(redirectParam) : "/";

  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [passwordVisible, setPasswordVisible] = useState(false);
  const [error, setError] = useState<{ message: string; retryable: boolean } | null>(null);
  const [submitting, setSubmitting] = useState(false);

  // 已登录访问 /login → 直接进入目标（规格状态矩阵）
  if (getToken()) {
    return <Navigate to={redirect} replace />;
  }

  async function submit() {
    setError(null);
    setSubmitting(true);
    try {
      const { token } = await api<LoginResponse>("/api/v1/auth/login", {
        method: "POST",
        body: { username, password },
        redirectOn401: false,
      });
      setToken(token);
      navigate(redirect, { replace: true });
    } catch (err) {
      const network = err instanceof TypeError; // fetch 网络失败
      setError(
        network
          ? { message: "无法连接服务器", retryable: true }
          : { message: "用户名或密码错误", retryable: false },
      );
      if (!network) setPassword("");
    } finally {
      setSubmitting(false);
    }
  }

  function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    void submit();
  }

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-background-100 px-4">
      <form
        onSubmit={onSubmit}
        className="w-[360px] rounded-xl border border-gray-400 bg-background-100 p-8"
      >
        <div className="flex items-center gap-2">
          <span className="flex h-7 w-7 items-center justify-center rounded-md bg-gray-1000 font-mono text-label-14 font-semibold text-background-100">
            ▲
          </span>
          <h1 className="text-heading-24">helm</h1>
        </div>
        <p className="mt-1 text-copy-13 text-gray-900">集中式运维平台</p>

        <label className="mt-8 block text-label-14" htmlFor="username">
          用户名
        </label>
        <input
          id="username"
          autoFocus
          autoComplete="username"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          disabled={submitting}
          className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
        />

        <label className="mt-4 block text-label-14" htmlFor="password">
          密码
        </label>
        <div className="relative mt-2">
          <input
            id="password"
            type={passwordVisible ? "text" : "password"}
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            disabled={submitting}
            className={`h-8 w-full rounded-md border bg-gray-100 pl-3 pr-8 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600 ${
              error && !error.retryable ? "border-red-1000" : "border-gray-400"
            }`}
          />
          <button
            type="button"
            aria-label={passwordVisible ? "隐藏密码" : "显示密码"}
            onClick={() => setPasswordVisible((v) => !v)}
            className="absolute right-1.5 top-1/2 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded text-gray-900 transition-colors duration-150 hover:text-gray-1000"
          >
            {passwordVisible ? <EyeOff size={14} strokeWidth={1.5} /> : <Eye size={14} strokeWidth={1.5} />}
          </button>
        </div>

        {error && (
          <p className="mt-3 flex items-center gap-1.5 text-label-13 text-red-1000" role="alert">
            ⚠ {error.message}
            {error.retryable && (
              <button
                type="button"
                onClick={() => void submit()}
                className="text-blue-1000 hover:underline"
              >
                重试
              </button>
            )}
          </p>
        )}

        <button
          type="submit"
          disabled={submitting || !username || !password}
          className="mt-6 flex h-8 w-full items-center justify-center gap-2 rounded-md bg-gray-700 text-label-14 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-50"
        >
          {submitting && (
            <span className="h-3.5 w-3.5 animate-spin rounded-full border border-gray-900 border-t-gray-1000" />
          )}
          {submitting ? "验证中…" : "登录"}
        </button>
      </form>
      <p className="mt-6 font-mono text-label-12 text-gray-900">helm-console v0.1.0</p>
    </div>
  );
}
