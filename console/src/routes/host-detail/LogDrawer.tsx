import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Copy, Radio } from "lucide-react";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { useBinaryStream } from "../../api/ws";

type Service = components["schemas"]["Service"];

const NO_LINES: string[] = [];

/** 服务日志抽屉（规格 services.md F44/F45：快照 + WS tail + 智能滚底 + 级别着色）。 */
export default function LogDrawer({ service, onClose }: { service: Service; onClose: () => void }) {
  const [live, setLive] = useState(false);
  const [wsLines, setWsLines] = useState<string[]>([]);
  const [autoScroll, setAutoScroll] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);

  // 快照（打开即拉）；显示 = 快照 + WS 增量（派生，不镜像 state）
  const snapshot = useQuery({
    queryKey: ["service-logs", service.id],
    queryFn: async () => {
      const res = await api<{ log?: string }>(`/api/v1/services/${service.id}/logs`);
      return (res.log ?? "").split("\n").filter((l) => l.length > 0);
    },
  });
  const snapshotLines = snapshot.data ?? NO_LINES;

  // 实时 tail（二进制帧 → 文本增量）
  const status = useBinaryStream(
    `/api/v1/services/${service.id}/logs/stream`,
    (chunk) => {
      setWsLines((prev) => {
        const next = [...prev, ...chunk.split("\n").filter((l) => l.length > 0)];
        return next.length > 5000 ? next.slice(next.length - 5000) : next; // 上限 5000 行
      });
    },
    { enabled: live },
  );

  // 断开标记：实时模式断连时尾部插一行（派生计算，不入 state）
  const dropped = live && status === "closed" && (snapshotLines.length + wsLines.length) > 0;
  const lines = useMemo(
    () => [...snapshotLines, ...wsLines, ...(dropped ? ["--- 实时断开 ---"] : [])],
    [snapshotLines, wsLines, dropped],
  );

  // 智能滚底：贴底时跟随，上滚暂停
  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [lines, autoScroll]);

  function onScroll() {
    const el = scrollRef.current;
    if (!el) return;
    setAutoScroll(el.scrollTop + el.clientHeight >= el.scrollHeight - 48);
  }

  async function copyAll() {
    await navigator.clipboard.writeText(lines.join("\n"));
  }

  return (
    <div className="fixed inset-0 z-50">
      <button type="button" aria-label="关闭" onClick={onClose} className="absolute inset-0 bg-black/40" />
      <div className="absolute right-0 top-0 flex h-full w-[560px] flex-col border-l border-gray-400 bg-background-100 xl:w-[640px]">
        {/* 头 */}
        <div className="flex items-center gap-3 border-b border-gray-400 px-5 py-4">
          <h2 className="text-heading-16">{service.name}</h2>
          <span className="text-label-13 text-gray-900">
            {service.status === "running" ? "● 运行" : service.status === "failed" ? "✕ 失败" : "○ 停止"}
            {service.exit_code != null && ` · exit ${service.exit_code}`}
          </span>
          <span className="flex-1" />
          <button
            type="button"
            onClick={() => setLive((v) => !v)}
            aria-pressed={live}
            className={`flex h-7 items-center gap-1.5 rounded-md border px-2.5 text-label-13 transition-colors duration-150 ${
              live
                ? "border-teal-1000 text-teal-1000"
                : "border-gray-500 text-gray-900 hover:bg-gray-200"
            }`}
          >
            <Radio size={13} strokeWidth={1.5} />
            {live ? (status === "open" ? "实时中" : status === "connecting" ? "连接中" : "重连中") : "实时"}
          </button>
          <button
            type="button"
            aria-label="关闭抽屉"
            onClick={onClose}
            className="flex h-7 w-7 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000"
          >
            ×
          </button>
        </div>

        {/* 日志区（黑底 mono） */}
        <div
          ref={scrollRef}
          onScroll={onScroll}
          className="flex-1 overflow-y-auto bg-[#0d0d0d] px-4 py-3 font-mono text-[12px] leading-5 text-gray-100"
        >
          {snapshot.isPending ? (
            <p className="text-gray-600">加载日志…</p>
          ) : snapshot.isError ? (
            <div className="text-red-1000">
              <p>日志加载失败：{(snapshot.error as Error).message}</p>
              <button type="button" onClick={() => snapshot.refetch()} className="mt-2 text-blue-1000 hover:underline">
                重试
              </button>
            </div>
          ) : lines.length === 0 ? (
            <p className="text-gray-600">暂无日志输出</p>
          ) : (
            lines.map((line, i) => <LogLine key={i} line={line} />)
          )}
        </div>

        {/* 底栏 */}
        <div className="flex h-9 items-center gap-3 border-t border-gray-400 px-4 font-mono text-label-12 text-gray-900">
          <span>共 {lines.length.toLocaleString()} 行</span>
          {!autoScroll && (
            <button
              type="button"
              onClick={() => {
                setAutoScroll(true);
                if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
              }}
              className="text-blue-1000 hover:underline"
            >
              已暂停 · 回到最新
            </button>
          )}
          <span className="flex-1" />
          <button
            type="button"
            onClick={copyAll}
            className="flex items-center gap-1 transition-colors duration-150 hover:text-gray-1000"
          >
            <Copy size={12} strokeWidth={1.5} />
            复制
          </button>
        </div>
      </div>
    </div>
  );
}

function LogLine({ line }: { line: string }) {
  if (line === "--- 实时断开 ---") {
    return <p className="my-1 text-gray-600">{line}</p>;
  }
  if (/\bERROR\b|\bFATAL\b/i.test(line)) {
    return <p className="whitespace-pre-wrap text-red-400">{line}</p>;
  }
  if (/\bWARN(ING)?\b/i.test(line)) {
    return <p className="whitespace-pre-wrap text-amber-300">{line}</p>;
  }
  return <p className="whitespace-pre-wrap">{line}</p>;
}
