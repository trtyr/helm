import { Navigate, Outlet, useLocation } from "react-router-dom";
import { getToken } from "../api/token";

/** JWT 守卫：未认证访问受保护路由 → /login?redirect=<原地址>（登录后回跳）。 */
export function RequireAuth() {
  const location = useLocation();
  if (!getToken()) {
    const redirect = encodeURIComponent(location.pathname + location.search);
    return <Navigate to={`/login?redirect=${redirect}`} replace />;
  }
  return <Outlet />;
}
