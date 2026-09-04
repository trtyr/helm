// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api, ApiError } from "./client";
import { clearToken, getToken, setToken } from "./token";

describe("token 存取", () => {
  beforeEach(() => localStorage.clear());

  it("set/get/clear 往返", () => {
    expect(getToken()).toBeNull();
    setToken("jwt-abc");
    expect(getToken()).toBe("jwt-abc");
    clearToken();
    expect(getToken()).toBeNull();
  });
});

describe("api client", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.unstubAllGlobals();
  });
  afterEach(() => vi.unstubAllGlobals());

  it("携带 JWT 并解析响应 JSON", async () => {
    setToken("jwt-xyz");
    const fetchMock = vi.fn(
      async (_input: RequestInfo | URL, init?: RequestInit) =>
        new Response(
          JSON.stringify({ auth: init?.headers ? new Headers(init.headers).get("Authorization") : null }),
          {
            status: 200,
            headers: { "Content-Type": "application/json" },
          },
        ),
    );
    vi.stubGlobal("fetch", fetchMock);

    const res = await api<{ auth: string | null }>("/api/v1/hosts");
    expect(res.auth).toBe("Bearer jwt-xyz");
  });

  it("非 2xx 抛 ApiError（透传后端 error.code/message）", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({ error: { code: "not_found", message: "resource not found" } }),
            { status: 404, headers: { "Content-Type": "application/json" } },
          ),
      ),
    );

    const err = (await api("/api/v1/hosts/x").catch((e: unknown) => e)) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(404);
    expect(err.code).toBe("not_found");
    expect(err.message).toBe("resource not found");
  });

  it("401 拦截：清 token 并跳转 /login?redirect=（F03）", async () => {
    setToken("expired-jwt");
    const assign = vi.fn();
    const originalDescriptor = Object.getOwnPropertyDescriptor(window, "location");
    Object.defineProperty(window, "location", {
      value: { pathname: "/", search: "", assign },
      configurable: true,
      writable: true,
    });
    vi.stubGlobal("fetch", vi.fn(async () => new Response("{}", { status: 401 })));

    try {
      await expect(api("/api/v1/jobs")).rejects.toThrow("登录已过期");
      expect(getToken()).toBeNull();
      expect(assign).toHaveBeenCalledWith(expect.stringMatching(/^\/login\?redirect=%2F/));
    } finally {
      if (originalDescriptor) {
        Object.defineProperty(window, "location", originalDescriptor);
      }
    }
  });

  it("redirectOn401=false 时不跳转（登录请求自身）", async () => {
    const assign = vi.fn();
    const originalDescriptor = Object.getOwnPropertyDescriptor(window, "location");
    Object.defineProperty(window, "location", {
      value: { pathname: "/", search: "", assign },
      configurable: true,
      writable: true,
    });
    vi.stubGlobal("fetch", vi.fn(async () => new Response("{}", { status: 401 })));

    try {
      await expect(
        api("/api/v1/auth/login", { method: "POST", redirectOn401: false }),
      ).rejects.toBeInstanceOf(ApiError);
      expect(assign).not.toHaveBeenCalled();
    } finally {
      if (originalDescriptor) {
        Object.defineProperty(window, "location", originalDescriptor);
      }
    }
  });
});
