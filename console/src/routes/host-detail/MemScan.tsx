import { useRef, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useMutation } from "@tanstack/react-query";
import { useQuery } from "@tanstack/react-query";
import { Play } from "lucide-react";
import { toast } from "../../lib/toast";
import type { components } from "../../api/schema";
import { api, pickAgent } from "../../api/client";
import { useWsStream } from "../../api/ws";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];

interface Ctx {
  host: HostView;
}

interface MemBatch {
  finished: boolean;
  matches?: string[];
  scannedBytes?: number;
  pidsTotal?: number;
  pidsScanned?: number;
  truncated?: boolean;
  timedOut?: boolean;
  error?: string | null;
}

interface ScanState {
  matches: string[];
  scannedBytes: number;
  pidsTotal: number;
  pidsScanned: number;
  truncated: boolean;
  timedOut: boolean;
  finished: boolean;
  error?: string | null;
}

const MAX_MATCHES = 5000;
const RENDER_CAP = 300;

function newScanId(): string {
  return crypto.randomUUID();
}

/** 内存字符串扫描：读取目标进程内存，提取可打印字符串（Volatility strings 简化版）。
 *  PID 留空 = 遍历全部进程；结果流式增量推送，边扫边看。 */
export default function MemScan() {
  const { host } = useOutletContext<Ctx>();
  const [pid, setPid] = useState("");
  const [keyword, setKeyword] = useState("");
  const [scanId, setScanId] = useState<string | null>(null);
  const [scan, setScan] = useState<ScanState | null>(null);

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = pickAgent(agentsQuery.data?.agents ?? [], host.id);

  const startMutation = useMutation({
    mutationFn: async () => {
      // 先订阅 WS（scanId 客户端生成），稍候再启动扫描，避免早期批次丢失
      const id = newScanId();
      setScan(null);
      setScanId(id);
      await new Promise((r) => setTimeout(r, 300));
      return api<{ scanId: string }>("/api/v1/ir/memscan/stream", {
        method: "POST",
        body: { agent_id: agent?.id, scan_id: id, pid: Number(pid) || 0, min_len: 6, keyword },
      });
    },
    onError: (e: Error) => {
      setScanId(null);
      toast(e.message, "error");
    },
  });

  const handleBatch = (raw: string) => {
    let b: MemBatch;
    try {
      b = JSON.parse(raw);
    } catch {
      return;
    }
    setScan((prev) => {
      const matches = [...(prev?.matches ?? []), ...(b.matches ?? [])];
      return {
        matches: matches.length > MAX_MATCHES ? matches.slice(0, MAX_MATCHES) : matches,
        scannedBytes: b.scannedBytes ?? prev?.scannedBytes ?? 0,
        pidsTotal: b.pidsTotal ?? prev?.pidsTotal ?? 0,
        pidsScanned: b.pidsScanned ?? prev?.pidsScanned ?? 0,
        truncated: b.truncated ?? prev?.truncated ?? false,
        timedOut: b.timedOut ?? prev?.timedOut ?? false,
        finished: b.finished,
        error: b.error ?? prev?.error,
      };
    });
    if (b.finished) {
      setScanId(null);
    }
  };

  // scanIdRef 供 WS 回调读取当前扫描态（hook 持 ref，回调不需稳定）
  const scanRef = useRef<ScanState | null>(null);
  scanRef.current = scan;

  useWsStream(
    scanId ? `/api/v1/ir/memscan/${scanId}/stream` : "",
    handleBatch,
    { enabled: !!scanId },
  );

  const offline = !host.online;
  const scanning = scanId !== null;
  const result = scan;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end gap-3 rounded-lg border border-gray-400 bg-background-100 p-4">
        <div>
          <label className="block text-label-13 text-gray-900" htmlFor="ms-pid">进程 PID</label>
          <input id="ms-pid" value={pid} onChange={(e) => setPid(e.target.value)} placeholder="留空 = 全部进程"
            className="mt-1 h-8 w-36 rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-13 outline-none hover:border-gray-500" />
        </div>
        <div>
          <label className="block text-label-13 text-gray-900" htmlFor="ms-kw">关键词</label>
          <input id="ms-kw" value={keyword} onChange={(e) => setKeyword(e.target.value)} placeholder="password / http / ..."
            className="mt-1 h-8 w-52 rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-13 outline-none hover:border-gray-500" />
        </div>
        <button type="button" disabled={offline || scanning || startMutation.isPending}
          onClick={() => startMutation.mutate()}
          className="flex h-8 items-center gap-1.5 rounded-md bg-gray-700 px-3 text-label-13 hover:bg-gray-800 disabled:opacity-40">
          <Play size={14} strokeWidth={1.5} />
          {scanning ? "扫描中…" : startMutation.isPending ? "启动中…" : "扫描"}
        </button>
        {scanning && (
          <span className="text-label-12 text-gray-900/60">
            {result ? `已扫 ${result.pidsScanned}/${result.pidsTotal} 进程` : "等待首批数据…"} · 边扫边显示
          </span>
        )}
      </div>
      {result && (
        <div className="flex flex-col gap-2">
          <p className="font-mono text-label-12 text-gray-900">
            {result.error
              ? `错误: ${result.error}`
              : `${result.finished ? "完成" : "扫描中"} · 命中 ${result.matches.length} 条 · 扫描 ${((result.scannedBytes ?? 0) / 1024 / 1024).toFixed(0)}MB · 进程 ${result.pidsScanned}/${result.pidsTotal}${result.truncated ? " · 已截断" : ""}${result.timedOut ? " · 超时截止" : ""}`}
          </p>
          <pre className="max-h-96 overflow-y-auto rounded-md border border-gray-400 bg-gray-100 p-3 font-mono text-label-12 text-gray-900">
            {result.matches.slice(0, RENDER_CAP).join("\n") || "（等待命中…）"}
            {result.matches.length > RENDER_CAP && `\n…（仅显示前 ${RENDER_CAP} 条，共 ${result.matches.length} 条）`}
          </pre>
        </div>
      )}
      <p className="text-label-12 text-gray-900">
        内存字符串扫描需要 SeDebugPrivilege；agent 以管理员运行时可读系统进程。PID 留空遍历全部进程，无截止时间，扫完为止（进度实时可见）。
      </p>
    </div>
  );
}
