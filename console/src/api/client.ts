/**
 * 统一 fetch 客户端：JWT 注入 + 错误规范化 + 401 全局拦截（F03）。
 *
 * - base：dev 走 Vite proxy（同源，空串）；生产由 VITE_API_BASE 注入（如 https://api.example.com）。
 * - 401：清 token 并跳 /login?redirect=<当前路径>（登录后回跳原页面）。
 * - 非 2xx：抛 ApiError（携带后端 { error: { code, message } } 或状态文本）。
 */

import { clearToken, getToken } from "./token";

const BASE = import.meta.env.VITE_API_BASE ?? "";

export class ApiError extends Error {
  status: number;
  code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
  }
}

export interface ApiOptions {
  method?: "GET" | "POST" | "PUT" | "DELETE";
  body?: unknown;
  /** 401 时是否跳转登录（默认 true；登录请求自身传 false 避免循环）。 */
  redirectOn401?: boolean;
}

export async function api<T>(path: string, options: ApiOptions = {}): Promise<T> {
  const { method = "GET", body, redirectOn401 = true } = options;

  const res = await fetch(`${BASE}${path}`, {
    method,
    headers: {
      ...(body !== undefined ? { "Content-Type": "application/json" } : {}),
      ...(getToken() ? { Authorization: `Bearer ${getToken()}` } : {}),
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });

  if (res.status === 401 && redirectOn401) {
    clearToken();
    const redirect = encodeURIComponent(
      window.location.pathname + window.location.search,
    );
    window.location.assign(`/login?redirect=${redirect}`);
    throw new ApiError(401, "unauthorized", "登录已过期");
  }

  if (!res.ok) {
    let code = `http_${res.status}`;
    let message = res.statusText || "请求失败";
    try {
      const data = (await res.json()) as {
        error?: { code?: string; message?: string };
      };
      if (data.error) {
        code = data.error.code ?? code;
        message = data.error.message ?? message;
      }
    } catch {
      // 响应体不是 JSON（如网关错误页）——保留状态文本
    }
    throw new ApiError(res.status, code, message);
  }

  if (res.status === 204) {
    return undefined as T;
  }
  return (await res.json()) as T;
}
