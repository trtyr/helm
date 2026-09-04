import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Copy, Maximize2 } from "lucide-react";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { useBinaryStream, type WsStatus } from "../../api/ws";
import { toast } from "../../lib/toast";
import { jobStatusMeta } from "../../lib/job";

type Job = components["schemas"]["Job"];
type Host = components["schemas"]["Host"];

/** /jobs/:id 任务详情（规格 job-detail.md F27/F28：元信息 + 输出回放 + WS 实时）。 */
export default function JobDetail() {
  const { id = "" } = useParams();
  const [wsChunks, setWsChunks] = useState<string[]>([]);
  const [autoScroll, setAutoScroll] = useState(true);
  const outputRef = useRef<HTMLDivElement>(null);
  const wsStatusRef = useRef<WsStatus>("closed");

  const jobQuery = useQuery({
    queryKey: ["job", id],
    queryFn: () => api<{ job: Job }>(`/api/v1/jobs/${id}`),
    // WS 重连超限（closed）且任务仍运行 → 5s 轮询降级保底（规格 job-detail.md）
    refetchInterval: (query) => {
      const st = query.state.data?.job?.status;
      const live = st === "running" || st === "queued";
      return live && wsStatusRef.current === "closed" ? 5_000 : false;
    },
  });
  const job = jobQuery.data?.job;

  const hostsQuery = useQuery({
    queryKey: ["hosts", "all"],
    queryFn: () => api<{ hosts: Host[] }>("/api/v1/hosts?page=1&limit=200"),
  });
  const host = (hostsQuery.data?.hosts ?? []).find((h) => h.id === job?.host_id);

  // 运行中：WS 实时追加；重连上限 3 次，超限由上方 refetchInterval 5s 轮询降级（规格 job-detail.md）
  const isLive = job?.status === "running" || job?.status === "queued";
  const status = useBinaryStream(
    `/api/v1/jobs/${id}/stream`,
    (chunk) => {
      setWsChunks((prev) => {
        const next = [...prev, chunk];
        return next.length > 2000 ? next.slice(next.length - 2000) : next;
      });
    },
    { enabled: isLive, maxRetries: 3 },
  );
  useEffect(() => {
    wsStatusRef.current = status;
  }, [status]);

  // 运行中每秒计数器（前端本地刷新，不重查——规格 job-detail.md；now 为异步侧产生的绝对秒）
  const [nowSec, setNowSec] = useState<number | null>(null);
  useEffect(() => {
    if (!isLive) return;
    const t = setInterval(() => setNowSec(Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(t);
  }, [isLive]);

  // 终态翻转后重拉元信息（断 WS + 刷新）
  useEffect(() => {
    if (job && !isLive && wsChunks.length > 0 && status === "closed") {
      const t = setTimeout(() => jobQuery.refetch(), 500);
      return () => clearTimeout(t);
    }
  }, [isLive, wsChunks.length, status, job, jobQuery]);

  const output = useMemo(() => {
    const replay = job?.output ?? "";
    return replay + (wsChunks.length > 0 ? (replay ? "\n" : "") + wsChunks.join("") : "");
  }, [job?.output, wsChunks]);

  useEffect(() => {
    if (autoScroll && outputRef.current) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [output, autoScroll]);

  function onScroll() {
    const el = outputRef.current;
    if (!el) return;
    setAutoScroll(el.scrollTop + el.clientHeight >= el.scrollHeight - 48);
  }

  async function copyOutput() {
    await navigator.clipboard.writeText(output);
    toast(`已复制 ${output.length.toLocaleString()} 字节`);
  }

  function fullscreen() {
    outputRef.current?.requestFullscreen?.();
  }

  if (jobQuery.isPending) {
    return (
      <div className="flex flex-col gap-6">
        <div className="h-8 w-64 animate-pulse rounded bg-gray-200" />
        <div className="h-40 animate-pulse rounded-lg border border-gray-400 bg-gray-100" />
      </div>
    );
  }
  if (jobQuery.isError || !job) {
    return (
      <div className="rounded-lg border border-gray-400 p-10 text-center">
        <p className="text-label-13 text-red-1000">任务不存在</p>
        <Link to="/jobs" className="mt-3 inline-block text-label-13 text-blue-1000 hover:underline">
          返回任务列表
        </Link>
      </div>
    );
  }

  const meta = jobStatusMeta(job.status);
  const started = job.started_at ? new Date(job.started_at) : null;
  const finished = job.finished_at ? new Date(job.finished_at) : null;
  const elapsed = finished && started
    ? `${((finished.getTime() - started.getTime()) / 1000).toFixed(1)}s`
    : started && isLive && nowSec != null
      ? `已运行 ${Math.max(0, nowSec - Math.floor(started.getTime() / 1000))}s`
      : "—";

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center gap-4">
        <Link to="/jobs" className="text-label-13 text-gray-900 transition-colors duration-150 hover:text-gray-1000">
          ← 返回任务列表
        </Link>
        <span className="flex-1" />
        <span className={`text-label-14 ${meta.cls}`}>{meta.label}</span>
      </div>

      <button
        type="button"
        onClick={() => {
          navigator.clipboard.writeText(job.id ?? "");
          toast("已复制任务 ID");
        }}
        className="self-start font-mono text-label-13 text-gray-900 transition-colors duration-150 hover:text-gray-1000"
        title="点击复制全值"
      >
        任务 #{job.id}
      </button>

      {/* 元信息卡 */}
      <dl className="grid grid-cols-2 gap-x-8 gap-y-0 rounded-lg border border-gray-400 p-6 md:grid-cols-4">
        <Item label="主机">
          {host ? (
            <Link to={`/hosts/${host.id}/overview`} className="text-blue-1000 hover:underline">
              {host.hostname}
            </Link>
          ) : (
            "—"
          )}
        </Item>
        <Item label="命令">
          <span className="font-mono">{job.command}</span>
        </Item>
        <Item label="参数">
          <span className="flex flex-wrap gap-1">
            {(job.args ?? []).length === 0
              ? "—"
              : (job.args ?? []).map((a, i) => (
                  <span key={i} className="rounded border border-gray-400 px-1.5 font-mono text-label-12">
                    {a}
                  </span>
                ))}
          </span>
        </Item>
        <Item label="类型">{job.task_id ? "定时" : "快速执行"}</Item>
        <Item label="开始" mono>
          {started ? started.toLocaleTimeString() : "—"}
        </Item>
        <Item label="结束" mono>
          {finished ? finished.toLocaleTimeString() : "—"}
        </Item>
        <Item label="耗时" mono>
          {elapsed}
        </Item>
        <Item label="退出码">
          <span
            className={`font-mono ${
              job.exit_code == null ? "text-gray-900" : job.exit_code === 0 ? "text-green-1000" : "text-red-1000"
            }`}
          >
            {job.exit_code ?? "—"}
          </span>
        </Item>
      </dl>

      {/* 输出块 */}
      <div className="overflow-hidden rounded-lg border border-gray-400">
        <div className="flex h-10 items-center justify-between border-b border-gray-400 px-4">
          <span className="text-label-13 text-gray-900">输出</span>
          <span className="flex items-center gap-3">
            <button
              type="button"
              onClick={copyOutput}
              className="flex items-center gap-1 text-label-12 text-gray-900 transition-colors duration-150 hover:text-gray-1000"
            >
              <Copy size={12} strokeWidth={1.5} /> 复制
            </button>
            <button
              type="button"
              onClick={fullscreen}
              className="flex items-center gap-1 text-label-12 text-gray-900 transition-colors duration-150 hover:text-gray-1000"
            >
              <Maximize2 size={12} strokeWidth={1.5} /> 全屏
            </button>
          </span>
        </div>
        <div
          ref={outputRef}
          onScroll={onScroll}
          className="min-h-60 bg-[#0d0d0d] px-4 py-3 font-mono text-[13px] leading-5 whitespace-pre-wrap text-gray-100"
        >
          {output.length === 0 ? (
            isLive ? (
              <p className="text-gray-600">{job.status === "queued" ? "等待执行…" : "等待输出…"}</p>
            ) : (
              <p className="text-gray-600">（无输出）</p>
            )
          ) : (
            output
          )}
        </div>
        <div className="flex h-9 items-center justify-between border-t border-gray-400 px-4 font-mono text-label-12 text-gray-900">
          <span>{output.length.toLocaleString()} 字节</span>
          <span>
            {isLive
              ? status === "open"
                ? "实时已连接 ●"
                : status === "connecting"
                  ? "连接中…"
                  : "实时断开，已降级 5s 轮询"
              : "传输完成"}
            {!autoScroll && (
              <button
                type="button"
                onClick={() => {
                  setAutoScroll(true);
                  if (outputRef.current) outputRef.current.scrollTop = outputRef.current.scrollHeight;
                }}
                className="ml-3 text-blue-1000 hover:underline"
              >
                已暂停 · 回到最新
              </button>
            )}
          </span>
        </div>
      </div>
    </div>
  );
}

function Item({ label, children, mono }: { label: string; children: React.ReactNode; mono?: boolean }) {
  return (
    <div className="flex h-8 items-center gap-2">
      <dt className="w-14 shrink-0 text-label-13 text-gray-900">{label}</dt>
      <dd className={`min-w-0 flex-1 truncate text-label-14 ${mono ? "font-mono" : ""}`}>{children}</dd>
    </div>
  );
}
