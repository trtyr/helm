import { useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Download, Loader2 } from "lucide-react";
import type { components } from "../api/schema";
import { api, apiBlob } from "../api/client";

type Listener = components["schemas"]["Listener"];
type AgentGenJob = components["schemas"]["AgentGenJob"];

const TARGETS = [
  { os: "windows", arch: "x86_64", label: "Windows x64 (.exe)" },
  { os: "windows", arch: "aarch64", label: "Windows ARM64 (.exe)" },
  { os: "linux", arch: "x86_64", label: "Linux x64 (ELF)" },
  { os: "linux", arch: "aarch64", label: "Linux ARM64 (ELF)" },
  { os: "macos", arch: "aarch64", label: "macOS Apple (M 系列)" },
] as const;

const STATUS_META: Record<string, { label: string; cls: string }> = {
  compiling: { label: "编译中", cls: "text-blue-1000" },
  ready: { label: "就绪", cls: "text-green-1000" },
  failed: { label: "失败", cls: "text-red-1000" },
};

/** 「生成 Agent」右抽屉：选监听器 + 目标平台 → 服务端现场编译 → 下载。
 * 编译完成后拷到目标机直接运行即可上线（agent_id 按主机名自动生成）。 */
export function AgentGenerateDrawer({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const [target, setTarget] = useState<string>("windows-x86_64");
  const [connMode, setConnMode] = useState<"reverse" | "forward">("reverse");
  const [listenerId, setListenerId] = useState<string>("");
  const [addrOverride, setAddrOverride] = useState<string>("");
  const [listenAddr, setListenAddr] = useState<string>("0.0.0.0:50052");
  const [jobId, setJobId] = useState<string | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);

  const listenersQuery = useQuery({
    queryKey: ["listeners"],
    queryFn: () => api<{ listeners: Listener[] }>("/api/v1/listeners"),
    enabled: open,
  });

  const listeners = listenersQuery.data?.listeners ?? [];
  const selectedListener = useMemo(
    () => listeners.find((l) => l.id === listenerId),
    [listeners, listenerId],
  );

  // 连入地址预填：监听器 bind 地址的通配主机用当前访问主机名替换
  const resolvedAddr = useMemo(() => {
    if (!selectedListener) return "";
    const [host, port] = (selectedListener.addr ?? "").split(":");
    const effective = !host || host === "0.0.0.0" ? window.location.hostname : host;
    return `http://${effective}:${port ?? 50051}`;
  }, [selectedListener]);

  // 编译任务轮询（compiling 期间每 2s）
  const jobQuery = useQuery({
    queryKey: ["agent-gen", jobId],
    queryFn: () => api<AgentGenJob>(`/api/v1/agent-gen/${jobId}`),
    enabled: !!jobId,
    refetchInterval: (q) =>
      (q.state.data?.status ?? "compiling") === "compiling" ? 2_000 : false,
  });
  const job = jobQuery.data;

  const startMutation = useMutation({
    mutationFn: () => {
      const [os, arch] = target.split("-");
      const body = {
        os,
        arch,
        listener_id: listenerId,
        conn_mode: connMode,
        listen_addr: connMode === "forward" ? listenAddr.trim() : "",
        server_addr: addrOverride.trim() !== resolvedAddr ? addrOverride.trim() : "",
      };
      return api<AgentGenJob>("/api/v1/agent-gen", { method: "POST", body });
    },
    onSuccess: (j) => {
      setJobId(j.id ?? null);
      setDownloadError(null);
    },
  });

  const download = async () => {
    if (!jobId) return;
    setDownloadError(null);
    try {
      const { blob, filename } = await apiBlob(
        `/api/v1/agent-gen/${jobId}/download`,
        `helm-agent-${target}`,
      );
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = filename;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      setDownloadError(e instanceof Error ? e.message : "下载失败");
    }
  };

  if (!open) return null;

  const canStart = !startMutation.isPending && (connMode === "forward" || !!listenerId);
  const statusMeta = job ? STATUS_META[job.status ?? "compiling"] : null;

  return (
    <div className="fixed inset-0 z-40 flex justify-end">
      <button
        type="button"
        aria-label="关闭"
        onClick={onClose}
        className="absolute inset-0 bg-black/40"
      />
      <div className="relative z-10 flex h-full w-[480px] flex-col gap-6 overflow-y-auto border-l border-gray-400 bg-background-100 p-6">
        <div>
          <h2 className="text-heading-20">生成 Agent</h2>
          <p className="mt-1 text-copy-13 text-gray-900">
            现场编译并下载，拷到目标机运行即自动上线
          </p>
        </div>

        {/* 第一步：配置 */}
        <div className="flex flex-col gap-4">
          <div>
            <span className="block text-label-14">连接模式</span>
            <div className="mt-2 grid grid-cols-2 gap-2">
              {(
                [
                  ["reverse", "反向（双击即上线）"],
                  ["forward", "正向（Server 拨号）"],
                ] as const
              ).map(([m, label]) => (
                <button
                  key={m}
                  type="button"
                  onClick={() => setConnMode(m)}
                  className={`h-8 rounded-md border text-label-12 transition-colors duration-150 ${
                    connMode === m
                      ? "border-blue-1000 bg-blue-1000/10 text-blue-1000"
                      : "border-gray-400 text-gray-900 hover:border-gray-500"
                  }`}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>

          {connMode === "reverse" && (
            <div>
              <label className="block text-label-14" htmlFor="gen-listener">
                监听器
              </label>
              <select
                id="gen-listener"
                value={listenerId}
                onChange={(e) => {
                  setListenerId(e.target.value);
                  setAddrOverride("");
                }}
                className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-2 text-label-13 outline-none transition-colors duration-150 hover:border-gray-500"
              >
                <option value="">选择监听器…</option>
                {listeners.map((l) => (
                  <option key={l.id} value={l.id}>
                    {l.name}（{l.addr}{l.status === "running" ? " · 运行中" : ""}）
                  </option>
                ))}
              </select>
            </div>
          )}

          <div>
            <span className="block text-label-14">目标平台</span>
            <div className="mt-2 grid grid-cols-2 gap-2">
              {TARGETS.map((t) => (
                <button
                  key={t.label}
                  type="button"
                  onClick={() => setTarget(`${t.os}-${t.arch}`)}
                  className={`h-8 rounded-md border px-2 text-label-12 transition-colors duration-150 ${
                    target === `${t.os}-${t.arch}`
                      ? "border-blue-1000 bg-blue-1000/10 text-blue-1000"
                      : "border-gray-400 text-gray-900 hover:border-gray-500"
                  }`}
                >
                  {t.label}
                </button>
              ))}
            </div>
          </div>

          <div>
            <label className="block text-label-13 text-gray-900" htmlFor="gen-addr">
              {connMode === "forward" ? "监听地址（Agent 对目标机监听）" : "连入地址（Agent 连 Server 的 gRPC 地址）"}
            </label>
            <input
              id="gen-addr"
              value={
                connMode === "forward"
                  ? (addrOverride || listenAddr)
                  : (addrOverride || resolvedAddr)
              }
              onChange={(e) =>
                connMode === "forward" ? setListenAddr(e.target.value) : setAddrOverride(e.target.value)
              }
              placeholder={connMode === "forward" ? "0.0.0.0:50052" : "http://192.168.1.10:50051"}
              className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
            <p className="mt-1 text-label-12 text-gray-900">
              会与注册 token 一起烙进二进制；目标机经公网接入时改成公网地址
            </p>
          </div>

          <button
            type="button"
            disabled={!canStart}
            onClick={() => startMutation.mutate()}
            className="flex h-9 items-center justify-center gap-2 rounded-md bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-40"
          >
            {startMutation.isPending && (
              <Loader2 size={14} className="animate-spin" strokeWidth={1.5} />
            )}
            开始编译
          </button>
          {startMutation.isError && (
            <p className="text-label-13 text-red-1000" role="alert">
              ⚠ {(startMutation.error as Error).message}
            </p>
          )}
        </div>

        {/* 第二步：编译任务 */}
        {job && (
          <div className="flex flex-col gap-3 border-t border-gray-400 pt-4">
            <div className="flex items-center justify-between">
              <span className="text-label-14">编译任务</span>
              <span className={`flex items-center gap-1.5 text-label-13 ${statusMeta?.cls}`}>
                {job.status === "compiling" && (
                  <Loader2 size={12} className="animate-spin" strokeWidth={1.5} />
                )}
                {statusMeta?.label}
              </span>
            </div>
            <div className="flex items-center justify-between font-mono text-label-12 text-gray-900">
              <span>{job.triple}</span>
              {job.file_size != null && <span>{(job.file_size / 1024 / 1024).toFixed(1)} MB</span>}
            </div>
            {job.status === "ready" && (
              <button
                type="button"
                onClick={download}
                className="flex h-9 items-center justify-center gap-2 rounded-md bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800"
              >
                <Download size={14} strokeWidth={1.5} />
                下载 Agent
              </button>
            )}
            {downloadError && (
              <p className="text-label-13 text-red-1000" role="alert">
                ⚠ {downloadError}
              </p>
            )}
            {job.error && (
              <p className="text-label-13 text-red-1000" role="alert">
                ⚠ {job.error}
              </p>
            )}
            {job.log_tail && job.log_tail.length > 0 && (
              <pre className="max-h-56 overflow-y-auto rounded-md border border-gray-400 bg-gray-100 p-3 font-mono text-label-12 text-gray-900">
                {job.log_tail.join("\n")}
              </pre>
            )}
            {job.status === "ready" && (
              <p className="text-label-12 text-gray-900">
                下载后拷到目标机直接运行（Linux/macOS 需 chmod +x），无需任何参数即可上线
              </p>
            )}
          </div>
        )}

        <button
          type="button"
          onClick={onClose}
          className="mt-auto h-8 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200"
        >
          关闭
        </button>
      </div>
    </div>
  );
}
