/**
 * WS 流基座（决策 004）：单例连接按 endpoint 复用、断线指数退避（1s/2s/4s…封顶 15s）、
 * 页面不可见暂停重连、状态上报。终端 WS（binary 双向会话）独立于此，不走本模块。
 */
import { useEffect, useRef, useState } from "react";
import { getToken } from "./token";

export type WsStatus = "connecting" | "open" | "closed";

interface SharedConn {
  ws: WebSocket | null;
  status: WsStatus;
  listeners: Set<(data: string) => void>;
  statusListeners: Set<(s: WsStatus) => void>;
  retry: number;
  timer: ReturnType<typeof setTimeout> | null;
  refCount: number;
  connect: () => void;
}

/** endpoint → 共享连接（模块级单例，多组件订阅同一条流只开一个连接）。 */
const pool = new Map<string, SharedConn>();

function wsUrl(path: string): string {
  const proto = location.protocol === "https:" ? "wss" : "ws";
  return `${proto}://${location.host}${path}`;
}

function getConn(endpoint: string): SharedConn {
  const existing = pool.get(endpoint);
  if (existing) return existing;

  const conn: SharedConn = {
    ws: null,
    status: "closed",
    listeners: new Set(),
    statusListeners: new Set(),
    retry: 0,
    timer: null,
    refCount: 0,
    connect: () => {},
  };

  conn.connect = () => {
    const token = getToken();
    if (!token || document.hidden) return; // visibilitychange 恢复后再连
    conn.status = "connecting";
    conn.statusListeners.forEach((fn) => fn("connecting"));
    const url = wsUrl(`${endpoint}?token=${encodeURIComponent(token)}`);
    const ws = new WebSocket(url);
    conn.ws = ws;
    ws.onopen = () => {
      conn.retry = 0;
      conn.status = "open";
      conn.statusListeners.forEach((fn) => fn("open"));
    };
    ws.onmessage = async (ev) => {
      // 后端 StreamRegistry 统一 to_string().into_bytes() → 浏览器收到 Binary（Blob）；
      // 少数直发文本的场景两者都兼容
      const text =
        typeof ev.data === "string" ? ev.data : await (ev.data as Blob).text();
      conn.listeners.forEach((fn) => fn(text));
    };
    ws.onclose = () => {
      conn.status = "closed";
      conn.statusListeners.forEach((fn) => fn("closed"));
      if (conn.refCount > 0) {
        // 指数退避 1s/2s/4s…封顶 15s（含随机抖动）
        const delay = Math.min(1000 * 2 ** conn.retry, 15_000) * (0.75 + Math.random() * 0.5);
        conn.retry += 1;
        conn.timer = setTimeout(conn.connect, delay);
      }
    };
  };

  // 页面隐藏断流省资源、恢复重连（可见性暂停）
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
      if (conn.timer) clearTimeout(conn.timer);
      conn.retry = 0;
      conn.ws?.close();
    } else if (conn.refCount > 0 && (!conn.ws || conn.status === "closed")) {
      conn.connect();
    }
  });

  pool.set(endpoint, conn);
  return conn;
}

/**
 * 订阅一条文本 WS 流（jobs/metrics/notifications 等单例复用场景）。返回连接状态。
 */
export function useWsStream(
  endpoint: string,
  onMessage: (data: string) => void,
  options?: { enabled?: boolean; onStatus?: (s: WsStatus) => void },
): WsStatus {
  const { enabled = true } = options ?? {};
  const msgRef = useRef(onMessage);
  const statusRef = useRef(options?.onStatus);
  // 回调 ref 在 effect 中同步（render 保持纯度）
  useEffect(() => {
    msgRef.current = onMessage;
    statusRef.current = options?.onStatus;
  });
  const [status, setStatus] = useState<WsStatus>("closed");

  useEffect(() => {
    if (!enabled) return;
    const conn = getConn(endpoint);
    const listener = (data: string) => msgRef.current(data);
    const statusListener = (s: WsStatus) => {
      setStatus(s);
      statusRef.current?.(s);
    };
    conn.listeners.add(listener);
    conn.statusListeners.add(statusListener);
    conn.refCount += 1;

    // 初始状态同步（异步侧 setState，避免 effect 内同步级联渲染）
    queueMicrotask(() => {
      if (!conn.ws || conn.status === "closed") conn.connect();
      setStatus(conn.status);
    });

    return () => {
      conn.listeners.delete(listener);
      conn.statusListeners.delete(statusListener);
      conn.refCount -= 1;
      if (conn.refCount === 0) {
        if (conn.timer) clearTimeout(conn.timer);
        conn.retry = 0;
        conn.ws?.close();
        conn.ws = null;
      }
    };
  }, [endpoint, enabled]);

  return status;
}

/**
 * 二进制 WS 流订阅（Blob 帧 → UTF-8 文本增量；服务日志/任务输出场景）。
 * 独占连接（不进单例池：按 id 一人一连接，无共享诉求）。
 * maxRetries：重连上限（默认无限）；超限后停在 closed 不再重连——调用方自行降级（如轮询）。
 */
export function useBinaryStream(
  endpoint: string,
  onChunk: (text: string) => void,
  options?: { enabled?: boolean; maxRetries?: number },
): WsStatus {
  const { enabled = true, maxRetries = Number.POSITIVE_INFINITY } = options ?? {};
  const chunkRef = useRef(onChunk);
  useEffect(() => {
    chunkRef.current = onChunk;
  });
  const [status, setStatus] = useState<WsStatus>("closed");

  useEffect(() => {
    if (!enabled || !endpoint) return;
    let retry = 0;
    let timer: ReturnType<typeof setTimeout> | null = null;
    let ws: WebSocket | null = null;
    let alive = true;

    const connect = () => {
      const token = getToken();
      if (!token || document.hidden) return;
      setStatus("connecting");
      ws = new WebSocket(wsUrl(`${endpoint}?token=${encodeURIComponent(token)}`));
      ws.onopen = () => {
        retry = 0;
        setStatus("open");
      };
      ws.onmessage = async (ev) => {
        const text = typeof ev.data === "string" ? ev.data : await (ev.data as Blob).text();
        chunkRef.current(text);
      };
      ws.onclose = () => {
        setStatus("closed");
        if (alive && retry < maxRetries) {
          const delay = Math.min(1000 * 2 ** retry, 15_000) * (0.75 + Math.random() * 0.5);
          retry += 1;
          timer = setTimeout(connect, delay);
        }
      };
    };
    const onVis = () => {
      if (!document.hidden && alive && retry < maxRetries && (!ws || ws.readyState === WebSocket.CLOSED)) {
        retry = 0;
        connect();
      }
    };
    document.addEventListener("visibilitychange", onVis);
    connect();

    return () => {
      alive = false;
      document.removeEventListener("visibilitychange", onVis);
      if (timer) clearTimeout(timer);
      ws?.close();
    };
  }, [endpoint, enabled, maxRetries]);

  return status;
}
