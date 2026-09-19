import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { clearToken, getToken } from "../../api/token";
import { jwtExp, jwtSub } from "../../lib/jwt";

/** /settings/sessions：会话——当前身份、Token 过期、退出登录（含确认弹窗）。 */
export default function SessionsSettings() {
  const navigate = useNavigate();
  const token = getToken() ?? "";
  const exp = jwtExp(token);
  const jwtUser = jwtSub(token) ?? "—";
  const [nowSec, setNowSec] = useState<number | null>(null);
  const [confirmLogout, setConfirmLogout] = useState(false);

  // 剩余时间每分钟重算（now 异步侧）
  useEffect(() => {
    const t = setInterval(() => setNowSec(Math.floor(Date.now() / 1000)), 60_000);
    return () => clearInterval(t);
  }, []);

  const remainHours = exp && nowSec ? Math.max(0, (exp - nowSec) / 3600) : null;

  function logout() {
    clearToken();
    navigate("/login");
  }

  return (
    <>
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
    </>
  );
}
