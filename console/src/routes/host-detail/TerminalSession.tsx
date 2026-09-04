import { useCallback, useEffect, useRef, useState } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";
import { getToken } from "../../api/token";
import { resolveTermTheme, type TermThemeName } from "../../lib/termSettings";

/** 浏览器 → Server 二进制帧协议（与 server/src/http/terminal.rs 对齐）。 */
const FRAME_INPUT = 0x01;
const FRAME_RESIZE = 0x02;

/** 会话空闲提示窗口（与后端 HELM_SESSION_IDLE_TIMEOUT=300s 同步的心智模型）。 */
const IDLE_SECS = 300;

export type ConnState = "connecting" | "open" | "closed";

/**
 * 单个终端会话组件：xterm + WS 生命周期 + 底部状态条。
 * 卸载即关闭会话（用户关 tab / 离开页面）。
 */
export function TerminalSession({
  agentId,
  fontSize,
  theme,
  active,
  onExited,
}: {
  agentId: string;
  fontSize: number;
  theme: TermThemeName;
  active: boolean;
  onExited?: () => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [conn, setConn] = useState<ConnState>("connecting");
  const [closeReason, setCloseReason] = useState<string | null>(null);
  const [dims, setDims] = useState<[number, number]>([80, 24]);
  const [idleLeft, setIdleLeft] = useState(IDLE_SECS);
  const [connectedFor, setConnectedFor] = useState(0);

  const lastActivity = useRef(0);
  const openedAt = useRef<number | null>(null);

  const touch = useCallback(() => {
    lastActivity.current = Date.now();
  }, []);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    lastActivity.current = Date.now();

    const termTheme = resolveTermTheme(theme, window.matchMedia("(prefers-color-scheme: dark)").matches);
    const term = new XTerm({
      fontSize,
      fontFamily: '"Geist Mono", ui-monospace, "SF Mono", monospace',
      lineHeight: 1.2, // 规格 terminal.md：行高 1.2 固定
      cursorBlink: true,
      scrollback: 5000,
      theme: termTheme,
    });
    container.style.background = termTheme.background;
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new WebLinksAddon());
    term.open(container);

    // 初始 fit 后用实际行列建 WS（SessionOpen 使用该尺寸）
    try {
      fit.fit();
    } catch {
      /* 布局未完成用默认 */
    }
    setDims([term.cols, term.rows]);

    const proto = location.protocol === "https:" ? "wss" : "ws";
    const url =
      `${proto}://${location.host}/api/v1/agents/${encodeURIComponent(agentId)}` +
      `/terminal?token=${encodeURIComponent(getToken() ?? "")}&cols=${term.cols}&rows=${term.rows}`;
    const ws = new WebSocket(url);
    ws.binaryType = "arraybuffer";

    ws.onopen = () => {
      setConn("open");
      openedAt.current = Date.now();
      touch();
      term.focus();
    };
    ws.onmessage = (ev) => {
      touch();
      if (ev.data instanceof ArrayBuffer) {
        term.write(new Uint8Array(ev.data));
      } else if (typeof ev.data === "string") {
        term.write(ev.data);
      }
    };
    ws.onclose = (ev) => {
      const reason =
        ev.reason === "idle_timeout"
          ? "已超时关闭（空闲 5 分钟）"
          : openedAt.current !== null
            ? "连接已断开"
            : "连接失败（主机可能离线）";
      term.writeln(`\x1b[31m--- ${reason} ---\x1b[0m`);
      setConn("closed");
      setCloseReason(reason);
    };

    // 输入：0x01 前缀帧
    term.onData((data) => {
      touch();
      if (ws.readyState !== WebSocket.OPEN) return;
      const payload = new TextEncoder().encode(data);
      const frame = new Uint8Array(payload.length + 1);
      frame[0] = FRAME_INPUT;
      frame.set(payload, 1);
      ws.send(frame);
    });

    // resize：容器变化（150ms 防抖）→ fit + 0x02 帧
    let timer: ReturnType<typeof setTimeout> | undefined;
    const observer = new ResizeObserver(() => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        try {
          fit.fit();
          setDims([term.cols, term.rows]);
          if (ws.readyState === WebSocket.OPEN) {
            const payload = new TextEncoder().encode(JSON.stringify([term.cols, term.rows]));
            const frame = new Uint8Array(payload.length + 1);
            frame[0] = FRAME_RESIZE;
            frame.set(payload, 1);
            ws.send(frame);
          }
        } catch {
          /* 布局中忽略 */
        }
      }, 150);
    });
    observer.observe(container);

    // 状态条时钟：空闲倒计时 + 连接时长
    const clock = setInterval(() => {
      setIdleLeft(Math.max(0, IDLE_SECS - Math.floor((Date.now() - lastActivity.current) / 1000)));
      if (openedAt.current !== null) {
        setConnectedFor(Math.floor((Date.now() - openedAt.current) / 1000));
      }
    }, 1000);

    return () => {
      clearInterval(clock);
      clearTimeout(timer);
      observer.disconnect();
      ws.close();
      term.dispose();
    };
  }, [agentId, fontSize, theme, touch]);

  // 激活时聚焦（切 tab 回来）
  useEffect(() => {
    if (active && conn === "open") {
      containerRef.current?.querySelector("textarea")?.focus();
    }
  }, [active, conn]);

  const idleWarn = idleLeft <= 60 && conn === "open";

  return (
    <div className="flex min-h-[420px] flex-1 flex-col overflow-hidden rounded-lg border border-gray-400">
      <div
        ref={containerRef}
        className="flex-1 p-3"
        style={{ display: active ? "block" : "none" }}
      />
      {/* 状态条 */}
      <div className="flex h-7 shrink-0 items-center gap-4 border-t border-gray-400 px-3 font-mono text-label-12 text-gray-900">
        <span className="flex items-center gap-1.5">
          <span
            className={`h-1.5 w-1.5 rounded-full ${
              conn === "open"
                ? "bg-green-1000"
                : conn === "connecting"
                  ? "bg-amber-1000"
                  : "bg-red-1000"
            }`}
          />
          sh · {dims[0]}×{dims[1]}
          {conn === "open" && ` · 已连接 ${Math.floor(connectedFor / 60)}m${connectedFor % 60}s`}
        </span>
        {idleWarn && (
          <span className="text-amber-1000">⚠ 空闲 {Math.floor(idleLeft / 60)}:{String(idleLeft % 60).padStart(2, "0")} 后自动关闭</span>
        )}
        {conn === "closed" && (
          <span className="flex items-center gap-2">
            <span className="text-red-1000">{closeReason}</span>
            <button
              type="button"
              onClick={onExited}
              className="rounded border border-gray-500 px-2 py-0.5 text-blue-1000 transition-colors duration-150 hover:bg-gray-200"
            >
              重开新会话
            </button>
          </span>
        )}
      </div>
    </div>
  );
}
