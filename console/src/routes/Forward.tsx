import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Play } from "lucide-react";
import type { components } from "../api/schema";
import { api } from "../api/client";
import { toast } from "../lib/toast";

type Host = components["schemas"]["Host"];

interface ExecResult {
  output: string;
  exit_code: number;
}

/** /forward 正向快捷执行（规格 forward.md F70：720px 工具页，同步等待，不落 job 历史）。 */
export default function Forward() {
  const [mode, setMode] = useState<"hostname" | "addr">("hostname");
  const [hostname, setHostname] = useState("");
  const [addr, setAddr] = useState("");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [result, setResult] = useState<{ output: string; exit_code: number; ms: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

  // 历史下拉（本会话内最近 5 条，纯前端）
  const [histOpen, setHistOpen] = useState(false);
  const histRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!histOpen) return;
    const onClick = (e: MouseEvent) => {
      if (!histRef.current?.contains(e.target as Node)) setHistOpen(false);
    };
    document.addEventListener("mousedown", onClick);
    return () => document.removeEventListener("mousedown", onClick);
  }, [histOpen]);

  const hostsQuery = useQuery({
    queryKey: ["hosts", "all"],
    queryFn: () => api<{ hosts: Host[] }>("/api/v1/hosts?page=1&limit=200"),
    refetchInterval: 30_000,
  });
  const forwardHosts = (hostsQuery.data?.hosts ?? []).filter(
    (h) => h.conn_mode === "forward" && h.hostname,
  );

  const execMutation = useMutation({
    mutationFn: async () => {
      setError(null);
      setResult(null);
      const body =
        mode === "hostname"
          ? { hostname, command: command.trim(), args: args.trim() ? args.split(/\s+/) : [] }
          : { agent_addr: addr.trim(), command: command.trim(), args: args.trim() ? args.split(/\s+/) : [] };
      const started = performance.now();
      const r = await api<ExecResult>("/api/v1/forward/exec", { method: "POST", body });
      return { ...r, ms: Math.round(performance.now() - started) };
    },
    onSuccess: (r) => {
      setResult(r);
      // 会话历史（去重置顶，最近 5 条）
      setHistory((prev) => [command.trim(), ...prev.filter((c) => c !== command.trim())].slice(0, 5));
    },
    onError: (e) => {
      const msg = (e as Error).message;
      setError(msg.includes("not forward mode") ? "该主机不是 forward 模式，拒绝执行" : msg);
      toast(msg, "error");
    },
  });

  const validTarget =
    mode === "hostname" ? hostname.length > 0 : /^\S+:\d+$/.test(addr.trim());
  const canRun = validTarget && command.trim().length > 0 && !execMutation.isPending;

  function run() {
    if (canRun) execMutation.mutate();
  }

  return (
    <div className="mx-auto flex w-full max-w-[720px] flex-col gap-6">
      <div>
        <h1 className="text-heading-24">正向执行</h1>
        <p className="mt-1 text-copy-13 text-gray-900">
          对 forward 主机按需拨号执行单条命令（临时连接，结果同步返回）
        </p>
      </div>

      {/* 目标 */}
      <section className="rounded-lg border border-gray-400 p-6">
        <h2 className="text-heading-16">目标</h2>
        <div className="mt-3 flex rounded-md border border-gray-500 p-0.5">
          {(["hostname", "addr"] as const).map((m) => (
            <button
              key={m}
              type="button"
              onClick={() => setMode(m)}
              className={`h-7 flex-1 rounded text-label-13 transition-colors duration-150 ${
                mode === m ? "bg-gray-200 text-gray-1000" : "text-gray-900 hover:text-gray-1000"
              }`}
            >
              {m === "hostname" ? "● 按主机名" : "○ 按地址"}
            </button>
          ))}
        </div>
        <div className="mt-4">
          {mode === "hostname" ? (
            forwardHosts.length === 0 ? (
              <p className="text-label-13 text-gray-900">
                没有 forward 模式主机——先在主机页创建
              </p>
            ) : (
              <select
                value={hostname}
                onChange={(e) => setHostname(e.target.value)}
                aria-label="选择 forward 主机"
                className="h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500"
              >
                <option value="">选择主机…</option>
                {forwardHosts.map((h) => (
                  <option key={h.id} value={h.hostname ?? ""}>
                    {h.hostname}
                    {h.addr ? "" : "（未配置地址）"}
                  </option>
                ))}
              </select>
            )
          ) : (
            <input
              value={addr}
              onChange={(e) => setAddr(e.target.value)}
              placeholder="0.0.0.0:50052"
              aria-label="目标地址"
              className="h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
          )}
        </div>
        {mode === "hostname" && hostname && !forwardHosts.find((h) => h.hostname === hostname)?.addr && (
          <p className="mt-2 text-label-12 text-amber-1000">该主机未配置地址</p>
        )}
      </section>

      {/* 命令 */}
      <section className="rounded-lg border border-gray-400 p-6">
        <h2 className="text-heading-16">命令</h2>
        <div className="mt-3 flex gap-2">
          <div className="relative flex-1" ref={histRef}>
            <span className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 select-none text-label-14 text-gray-900">
              $
            </span>
            <input
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") run();
                if (e.key === "ArrowUp" && history.length > 0 && !histOpen) {
                  e.preventDefault();
                  setHistOpen(true);
                }
              }}
              onFocus={() => history.length > 0 && setHistOpen(true)}
              placeholder="uname -a"
              aria-label="命令"
              className="h-8 w-full rounded-md border border-gray-400 bg-gray-100 pl-7 pr-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
            {histOpen && history.length > 0 && (
              <div className="absolute left-0 right-0 top-9 z-30 rounded-lg border border-gray-400 bg-background-100 py-1 shadow-lg">
                {history.map((c) => (
                  <button
                    key={c}
                    type="button"
                    onClick={() => {
                      setCommand(c);
                      setHistOpen(false);
                    }}
                    className="block w-full truncate px-3 py-1.5 text-left font-mono text-label-13 transition-colors duration-150 hover:bg-gray-200"
                  >
                    {c}
                  </button>
                ))}
              </div>
            )}
          </div>
          <button
            type="button"
            onClick={run}
            disabled={!canRun}
            className="flex h-8 items-center gap-1.5 rounded-md bg-gray-700 px-4 text-label-14 text-gray-1000 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-50"
          >
            <Play size={13} strokeWidth={1.5} />
            {execMutation.isPending ? "拨号执行中…" : "执行"}
          </button>
        </div>
        <input
          value={args}
          onChange={(e) => setArgs(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && run()}
          placeholder="参数（可选，空格分隔）"
          aria-label="参数"
          className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
        />
        <p className="mt-3 text-label-12 text-gray-900">
          此入口不产生任务记录；需历史请用主机详情的执行
        </p>
      </section>

      {/* 结果 */}
      {error && (
        <div className="rounded-lg border border-gray-400 p-6">
          <p className="text-label-13 text-red-1000">无法连接目标（地址不可达 / 非 forward 模式）</p>
          <p className="mt-1 font-mono text-label-12 text-gray-900">{error}</p>
          <button
            type="button"
            onClick={run}
            className="mt-3 h-7 rounded-md border border-gray-500 px-3 text-label-12 transition-colors duration-150 hover:bg-gray-200"
          >
            重试
          </button>
        </div>
      )}

      {execMutation.isPending && (
        <div className="rounded-lg border border-gray-400 p-6">
          <p className="font-mono text-label-13 text-gray-900">拨号中…（同步等待远端执行完成）</p>
        </div>
      )}

      {result && !execMutation.isPending && (
        <section className="overflow-hidden rounded-lg border border-gray-400">
          <div className="flex h-10 items-center gap-3 border-b border-gray-400 px-4">
            <span className={`font-mono text-label-14 ${result.exit_code === 0 ? "text-green-1000" : "text-red-1000"}`}>
              exit_code: {result.exit_code}
            </span>
            <span className="font-mono text-label-12 text-gray-900">耗时 {(result.ms / 1000).toFixed(1)}s</span>
          </div>
          <pre className="min-h-24 max-h-96 overflow-auto bg-[#0d0d0d] px-4 py-3 font-mono text-[13px] leading-5 whitespace-pre-wrap text-gray-100">
            {result.output.length > 0 ? result.output : "（无输出）"}
          </pre>
        </section>
      )}
    </div>
  );
}
