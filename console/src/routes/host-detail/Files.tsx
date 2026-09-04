import { useEffect, useMemo, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { ArrowUp, ChevronRight, Download, RefreshCw, Upload } from "lucide-react";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { crumbLabel, crumbSegments, humanSize, joinPath, parentPath } from "../../lib/paths";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];
type FileEntry = components["schemas"]["FileEntry"];

interface Ctx {
  host: HostView;
}

type SortKey = "name" | "size" | "modified";

interface Transfer {
  id: number;
  direction: "upload" | "download";
  label: string;
  state: "running" | "done" | "failed";
  checksumOk?: boolean;
  error?: string;
  finishedAt?: number;
}

/**
 * /hosts/:id/files（规格 routes/files.md）：单栏远端文件浏览 + Server 中转传输。
 * 上传/下载均为「路径对路径」（Server 侧 ↔ Agent 侧），与后端 push/pull 语义一致。
 */
export default function Files() {
  const { host } = useOutletContext<Ctx>();
  const [path, setPath] = useState("/");
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "name", desc: false });
  const [transfers, setTransfers] = useState<Transfer[]>([]);
  const [uploadModal, setUploadModal] = useState(false);
  const [downloadTarget, setDownloadTarget] = useState<FileEntry | null>(null);

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = (agentsQuery.data?.agents ?? []).find((a) => a.host_id === host.id);

  const listQuery = useQuery({
    queryKey: ["files", agent?.id, path],
    queryFn: () =>
      api<{ path: string; entries: FileEntry[] }>("/api/v1/files/list", {
        method: "POST",
        body: { agent_id: agent?.id, path },
      }),
    enabled: !!agent && host.online,
  });

  const sorted = useMemo(() => {
    const list = [...(listQuery.data?.entries ?? [])];
    list.sort((a, b) => {
      // 目录恒定排前
      if (!!a.is_dir !== !!b.is_dir) return a.is_dir ? -1 : 1;
      const dir = sort.desc ? -1 : 1;
      switch (sort.key) {
        case "size":
          return ((a.size ?? 0) - (b.size ?? 0)) * dir;
        case "modified":
          return ((a.modified_unix_ms ?? 0) - (b.modified_unix_ms ?? 0)) * dir;
        default:
          return (a.name ?? "").localeCompare(b.name ?? "") * dir;
      }
    });
    return list;
  }, [listQuery.data, sort]);

  function pushTransfer(t: Omit<Transfer, "id">) {
    const id = Date.now() + Math.random();
    setTransfers((list) => [...list, { ...t, id }]);
    return id;
  }

  function patchTransfer(id: number, patch: Partial<Transfer>) {
    setTransfers((list) => list.map((t) => (t.id === id ? { ...t, ...patch } : t)));
  }

  const uploadMutation = useMutation({
    mutationFn: async (v: { localPath: string; remotePath: string }) => {
      const tid = pushTransfer({
        direction: "upload",
        label: `${v.localPath} → ${v.remotePath}`,
        state: "running",
      });
      try {
        const res = await api<{ checksum_ok: boolean }>("/api/v1/files/upload", {
          method: "POST",
          body: { agent_id: agent?.id, local_path: v.localPath, remote_path: v.remotePath },
        });
        patchTransfer(tid, {
          state: "done",
          checksumOk: res.checksum_ok,
          finishedAt: Date.now(),
        });
        listQuery.refetch();
      } catch (e) {
        patchTransfer(tid, { state: "failed", error: (e as Error).message, finishedAt: Date.now() });
        throw e;
      }
    },
  });

  const downloadMutation = useMutation({
    mutationFn: async (v: { remotePath: string; localPath: string }) => {
      const tid = pushTransfer({
        direction: "download",
        label: `${v.remotePath} → ${v.localPath}`,
        state: "running",
      });
      try {
        const res = await api<{ checksum_ok: boolean }>("/api/v1/files/download", {
          method: "POST",
          body: { agent_id: agent?.id, remote_path: v.remotePath, local_path: v.localPath },
        });
        patchTransfer(tid, {
          state: "done",
          checksumOk: res.checksum_ok,
          finishedAt: Date.now(),
        });
      } catch (e) {
        patchTransfer(tid, { state: "failed", error: (e as Error).message, finishedAt: Date.now() });
        throw e;
      }
    },
  });

  // 完成项 30s 后自动清理
  useEffect(() => {
    const timer = setInterval(() => {
      setTransfers((list) =>
        list.filter(
          (t) => t.state === "running" || !t.finishedAt || Date.now() - t.finishedAt < 30_000,
        ),
      );
    }, 5000);
    return () => clearInterval(timer);
  }, []);

  const dirCount = sorted.filter((e) => e.is_dir).length;
  const fileCount = sorted.length - dirCount;

  return (
    <div className="flex flex-col gap-4">
      {/* 工具行：面包屑 + 操作 */}
      <div className="flex flex-wrap items-center gap-2">
        <nav
          className="flex min-w-0 flex-1 flex-wrap items-center gap-0.5 font-mono text-label-13"
          aria-label="路径"
        >
          {crumbSegments(path).map((seg, i, arr) => (
            <span key={seg} className="flex items-center">
              {i > 0 && <ChevronRight size={12} strokeWidth={1.5} className="text-gray-900" />}
              <button
                type="button"
                onClick={() => setPath(seg)}
                className={`rounded px-1 py-0.5 transition-colors duration-150 hover:bg-gray-200 ${
                  i === arr.length - 1 ? "text-gray-1000" : "text-blue-1000"
                }`}
              >
                {crumbLabel(seg)}
              </button>
            </span>
          ))}
        </nav>
        <button
          type="button"
          aria-label="返回上级"
          disabled={parentPath(path) === null}
          onClick={() => setPath(parentPath(path) ?? "/")}
          className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000 disabled:opacity-30"
        >
          <ArrowUp size={14} strokeWidth={1.5} />
        </button>
        <button
          type="button"
          aria-label="刷新"
          onClick={() => listQuery.refetch()}
          className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000"
        >
          <RefreshCw size={14} strokeWidth={1.5} />
        </button>
        <button
          type="button"
          onClick={() => setUploadModal(true)}
          disabled={!agent || !host.online}
          className="flex h-8 items-center gap-1.5 rounded-md border border-gray-500 px-3 text-label-13 transition-colors duration-150 hover:bg-gray-200 disabled:opacity-40"
        >
          <Upload size={14} strokeWidth={1.5} />
          上传
        </button>
      </div>

      {/* 表格 */}
      <div className="overflow-hidden rounded-lg border border-gray-400">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <th
                className="cursor-pointer px-4 py-2 font-normal select-none hover:text-gray-1000"
                onClick={() => setSort((s) => ({ key: "name", desc: s.key === "name" && !s.desc }))}
              >
                名称 {sort.key === "name" && (sort.desc ? "↓" : "↑")}
              </th>
              <th className="px-4 py-2 font-normal">类型</th>
              <th
                className="cursor-pointer px-4 py-2 text-right font-normal select-none hover:text-gray-1000"
                onClick={() => setSort((s) => ({ key: "size", desc: s.key === "size" && !s.desc }))}
              >
                大小 {sort.key === "size" && (sort.desc ? "↓" : "↑")}
              </th>
              <th className="px-4 py-2 font-normal">权限</th>
              <th
                className="cursor-pointer px-4 py-2 font-normal select-none hover:text-gray-1000"
                onClick={() =>
                  setSort((s) => ({ key: "modified", desc: s.key === "modified" && !s.desc }))
                }
              >
                修改时间 {sort.key === "modified" && (sort.desc ? "↓" : "↑")}
              </th>
              <th className="px-4 py-2" />
            </tr>
          </thead>
          <tbody>
            {listQuery.isPending ? (
              <tr>
                <td colSpan={6} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  加载目录…
                </td>
              </tr>
            ) : listQuery.isError ? (
              <tr>
                <td colSpan={6} className="px-4 py-10 text-center">
                  <span className="text-label-13 text-red-1000">路径不存在或无权限</span>
                  <button
                    type="button"
                    onClick={() => setPath("/")}
                    className="ml-3 text-label-13 text-blue-1000 hover:underline"
                  >
                    回到 /
                  </button>
                </td>
              </tr>
            ) : sorted.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  此目录为空
                </td>
              </tr>
            ) : (
              sorted.map((entry) => (
                <tr
                  key={entry.name}
                  onDoubleClick={() => entry.is_dir && setPath(joinPath(path, entry.name ?? ""))}
                  className="group cursor-pointer border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
                >
                  <td className="px-4 py-2.5">
                    <span className="flex items-center gap-2">
                      <span className={entry.is_dir ? "text-blue-1000" : "text-gray-900"}>
                        {entry.is_dir ? "▸" : "▤"}
                      </span>
                      <span className={`text-label-14 ${entry.is_dir ? "text-blue-1000" : ""}`}>
                        {entry.name}
                      </span>
                    </span>
                  </td>
                  <td className="px-4 py-2.5 text-label-13 text-gray-900">
                    {entry.is_dir ? "目录" : "文件"}
                  </td>
                  <td className="px-4 py-2.5 text-right font-mono text-label-13 text-gray-900 tabular-nums">
                    {entry.is_dir ? "—" : humanSize(entry.size ?? 0)}
                  </td>
                  <td
                    className="px-4 py-2.5 font-mono text-label-13 text-gray-900"
                    title={entry.mode ?? ""}
                  >
                    {entry.mode?.replace(/^([d-])/, "") || "—"}
                  </td>
                  <td className="px-4 py-2.5 font-mono text-label-13 text-gray-900">
                    {formatMtime(entry.modified_unix_ms)}
                  </td>
                  <td className="px-4 py-2.5 text-right">
                    {!entry.is_dir && (
                      <button
                        type="button"
                        aria-label={`下载 ${entry.name}`}
                        onClick={() => setDownloadTarget(entry)}
                        className="flex h-7 w-7 items-center justify-center rounded-md text-gray-900 opacity-0 transition-opacity duration-150 hover:bg-gray-200 hover:text-blue-1000 group-hover:opacity-100"
                      >
                        <Download size={14} strokeWidth={1.5} />
                      </button>
                    )}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center border-t border-gray-400 px-4 text-label-13 text-gray-900">
          共 {sorted.length} 项 · {dirCount} 目录 · {fileCount} 文件
        </div>
      </div>

      {/* 上传模态（Server 侧路径 → 远端路径） */}
      {uploadModal && (
        <PathPairModal
          title="上传文件"
          hint="从 Server 侧路径读取，推送到目标机（Server 中转，路径为 Server 可访问的本地路径）"
          fromLabel="Server 侧源路径"
          fromPlaceholder="/tmp/deploy.tar.gz"
          toLabel="远端目标路径"
          toDefault={path === "/" ? "/" : path}
          submitLabel="开始上传"
          submitting={uploadMutation.isPending}
          error={uploadMutation.isError ? uploadMutation.error.message : null}
          onClose={() => setUploadModal(false)}
          onSubmit={(from, to) =>
            uploadMutation.mutate(
              { localPath: from, remotePath: to },
              { onSuccess: () => setUploadModal(false) },
            )
          }
        />
      )}

      {/* 下载模态（远端 → Server 侧路径） */}
      {downloadTarget && (
        <PathPairModal
          title="下载文件"
          hint={`从目标机拉取 ${downloadTarget.name}，保存到 Server 侧路径`}
          fromLabel="远端源路径"
          fromDefault={joinPath(path, downloadTarget.name ?? "")}
          fromDisabled
          toLabel="Server 侧保存路径"
          toPlaceholder="/tmp/"
          submitLabel="开始下载"
          submitting={downloadMutation.isPending}
          error={downloadMutation.isError ? downloadMutation.error.message : null}
          onClose={() => setDownloadTarget(null)}
          onSubmit={(_from, to) =>
            downloadMutation.mutate(
              {
                remotePath: joinPath(path, downloadTarget.name ?? ""),
                localPath: to,
              },
              { onSuccess: () => setDownloadTarget(null) },
            )
          }
        />
      )}

      {/* 传输队列（右下固定卡） */}
      {transfers.length > 0 && (
        <div className="fixed bottom-10 right-4 z-40 w-96 rounded-xl border border-gray-400 bg-background-100 p-3 shadow-lg">
          <p className="text-label-13 text-gray-900">传输队列</p>
          <div className="mt-2 flex flex-col gap-1.5">
            {transfers.map((t) => (
              <div key={t.id} className="flex items-center gap-2 text-label-12">
                <span className={t.direction === "upload" ? "text-blue-1000" : "text-teal-1000"}>
                  {t.direction === "upload" ? "↑" : "↓"}
                </span>
                <span className="min-w-0 flex-1 truncate font-mono text-gray-900">{t.label}</span>
                {t.state === "running" && (
                  <span className="h-3 w-3 animate-spin rounded-full border border-gray-900 border-t-gray-1000" />
                )}
                {t.state === "done" && (
                  <span className={t.checksumOk ? "text-green-1000" : "text-red-1000"}>
                    {t.checksumOk ? "✓ 校验通过" : "✗ 校验失败"}
                  </span>
                )}
                {t.state === "failed" && (
                  <span className="truncate text-red-1000" title={t.error}>
                    ✗ 失败
                  </span>
                )}
                {t.state !== "running" && (
                  <button
                    type="button"
                    aria-label="移除"
                    onClick={() => setTransfers((l) => l.filter((x) => x.id !== t.id))}
                    className="text-gray-900 hover:text-gray-1000"
                  >
                    ×
                  </button>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function formatMtime(ms?: number): string {
  if (!ms) return "—";
  const d = new Date(ms);
  const months = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
  ];
  return `${months[d.getMonth()]} ${String(d.getDate()).padStart(2, "0")} ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** 路径对模态（上传/下载共用）。 */
function PathPairModal({
  title,
  hint,
  fromLabel,
  fromPlaceholder,
  fromDefault,
  fromDisabled,
  toLabel,
  toPlaceholder,
  toDefault,
  submitLabel,
  submitting,
  error,
  onClose,
  onSubmit,
}: {
  title: string;
  hint: string;
  fromLabel: string;
  fromPlaceholder?: string;
  fromDefault?: string;
  fromDisabled?: boolean;
  toLabel: string;
  toPlaceholder?: string;
  toDefault?: string;
  submitLabel: string;
  submitting: boolean;
  error: string | null;
  onClose: () => void;
  onSubmit: (from: string, to: string) => void;
}) {
  const [from, setFrom] = useState(fromDefault ?? "");
  const [to, setTo] = useState(toDefault ?? "");

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <button type="button" aria-label="关闭" onClick={onClose} className="absolute inset-0 bg-black/40" />
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (from.trim() && to.trim()) onSubmit(from.trim(), to.trim());
        }}
        className="relative z-10 w-[420px] rounded-xl border border-gray-400 bg-background-100 p-6"
      >
        <h2 className="text-heading-16">{title}</h2>
        <p className="mt-2 text-label-12 text-gray-900">{hint}</p>
        <label className="mt-4 block text-label-14" htmlFor="pair-from">
          {fromLabel}
        </label>
        <input
          id="pair-from"
          value={from}
          onChange={(e) => setFrom(e.target.value)}
          placeholder={fromPlaceholder}
          disabled={fromDisabled || submitting}
          className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
        />
        <label className="mt-4 block text-label-14" htmlFor="pair-to">
          {toLabel}
        </label>
        <input
          id="pair-to"
          value={to}
          onChange={(e) => setTo(e.target.value)}
          placeholder={toPlaceholder}
          disabled={submitting}
          className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
        />
        {error && (
          <p className="mt-3 text-label-13 text-red-1000" role="alert">
            ⚠ {error}
          </p>
        )}
        <div className="mt-6 flex justify-end gap-3">
          <button
            type="button"
            onClick={onClose}
            disabled={submitting}
            className="h-8 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200"
          >
            取消
          </button>
          <button
            type="submit"
            disabled={submitting || !from.trim() || !to.trim()}
            className="h-8 rounded-md bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-50"
          >
            {submitting ? "传输中…" : submitLabel}
          </button>
        </div>
      </form>
    </div>
  );
}
