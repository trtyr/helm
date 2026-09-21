import { useMemo, useState } from "react";
import { SortableTh } from "../../components/tableControls";
import { useTableControls } from "../../lib/useTableControls";
import { useOutletContext } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Download, Search, X } from "lucide-react";
import { api, pickAgent } from "../../api/client";
import { crumbLabel, humanSize, joinPath } from "../../lib/paths";
import { DRIVE_TYPE_LABEL, formatMtime, type Agent, type Ctx, type FileEntry, type Transfer } from "./files/shared";
import { PathPairModal, TransferQueue } from "./files/FileParts";
import { FileToolbar } from "./files/FileToolbar";

/**
 * /hosts/:id/files：全盘文件浏览器。
 * - 空路径 =「此电脑」根视图（Windows 枚举全部驱动器；POSIX 为 /）
 * - 面包屑 + 路径跳转输入框（回车直达任意路径）+ 双击进入目录
 * - 上传/下载均为「路径对路径」（Server 中转），与后端 push/pull 语义一致
 *
 * G13 拆分（2026-09-21）：原为 585 行单文件。现拆为——
 * `files/shared.ts`（类型与纯工具）/ `files/FileParts.tsx`（模态与传输队列）/
 * `files/FileToolbar.tsx`（工具行）；本文件保留状态、数据获取与表格编排。
 */
export default function Files() {
  const { host } = useOutletContext<Ctx>();
  // "" = 此电脑根视图（Windows 驱动器列表 / POSIX 根目录）
  const [path, setPath] = useState("");
  const [jumpDraft, setJumpDraft] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [transfers, setTransfers] = useState<Transfer[]>([]);
  const [uploadModal, setUploadModal] = useState(false);
  const [downloadTarget, setDownloadTarget] = useState<FileEntry | null>(null);

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = pickAgent(agentsQuery.data?.agents ?? [], host.id);

  const listQuery = useQuery({
    queryKey: ["files", agent?.id, path],
    queryFn: () =>
      api<{ path: string; entries: FileEntry[] }>("/api/v1/files/list", {
        method: "POST",
        body: { agent_id: agent?.id, path },
      }),
    enabled: !!agent && host.online,
  });

  const isDriveRootView = path === "" && (listQuery.data?.entries ?? []).some((e) => /^[A-Za-z]:\\?$/.test(e.name ?? ""));

  // 列表基座（P001-T1）：排序/枚举走 hook；目录恒优先为领域 preSort（取消列排序时仅按目录优先）
  const filteredEntries = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const list = [...(listQuery.data?.entries ?? [])];
    return q ? list.filter((e) => (e.name ?? "").toLowerCase().includes(q)) : list;
  }, [listQuery.data, filter]);

  const fileTc = useTableControls<FileEntry>(filteredEntries, {
    columns: [
      { key: "name", value: (e) => e.name ?? "" },
      { key: "size", value: (e) => e.size ?? 0 },
      { key: "modified", value: (e) => e.modified_unix_ms ?? 0 },
      {
        key: "type",
        enumOptions: () => [
          { value: "dir", label: "目录" },
          { value: "file", label: "文件" },
        ],
        matchesEnum: (e, v) => (v === "dir" ? !!e.is_dir : !e.is_dir),
      },
    ],
    preSort: (a, b) => (!!a.is_dir !== !!b.is_dir ? (a.is_dir ? -1 : 1) : 0),
  });
  const sorted = fileTc.visible;
  const toggleSort = fileTc.toggleSort;
  const sort = fileTc.sort;

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

  const dirCount = sorted.filter((e) => e.is_dir).length;
  const fileCount = sorted.length - dirCount;
  const atRoot = path === "";

  return (
    <div className="flex flex-col gap-4">
      {/* 工具行：面包屑 / 路径跳转 / 上级 / 刷新 / 上传 */}
      <FileToolbar
        path={path}
        onPath={setPath}
        jumpDraft={jumpDraft}
        onJumpDraft={setJumpDraft}
        isFetching={listQuery.isFetching}
        onRefresh={() => listQuery.refetch()}
        canUpload={!!agent && !!host.online}
        onUpload={() => setUploadModal(true)}
      />

      {/* 表格 */}
      <div className="overflow-hidden rounded-lg border border-gray-400 bg-background-100">
        <div className="flex items-center gap-2 border-b border-gray-400 px-4 py-2">
          <span className="text-label-13 text-gray-900">
            {atRoot ? "此电脑 · 全部驱动器" : crumbLabel(path)}
          </span>
          <select
            aria-label="按类型筛选"
            value={fileTc.enumFilters.type ?? ""}
            onChange={(e) => fileTc.setEnumFilter("type", e.target.value)}
            className="ml-2 h-7 rounded-md border border-gray-400 bg-gray-100 px-1.5 text-label-12 outline-none transition-colors duration-150 hover:border-gray-500"
          >
            <option value="">全部类型</option>
            <option value="dir">目录</option>
            <option value="file">文件</option>
          </select>
          <div className="relative ml-auto">
            <Search size={13} strokeWidth={1.5} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-gray-900" />
            <input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="过滤当前目录"
              aria-label="过滤文件"
              className="h-7 w-44 rounded-md border border-gray-400 bg-gray-100 pl-8 pr-7 text-label-12 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
            {filter && (
              <button
                type="button"
                aria-label="清除过滤"
                onClick={() => setFilter("")}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-900 hover:text-gray-1000"
              >
                <X size={11} strokeWidth={1.5} />
              </button>
            )}
          </div>
        </div>
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <SortableTh className="px-4 py-2" label="名称" sortKey="name" sort={sort} onSort={toggleSort} />
              <th className="px-4 py-2 font-normal">类型</th>
              <SortableTh className="px-4 py-2" align="text-right" label="大小" sortKey="size" sort={sort} onSort={toggleSort} />
              <th className="px-4 py-2 font-normal">{isDriveRootView ? "磁盘类型" : "权限"}</th>
              <SortableTh className="px-4 py-2" label="修改时间" sortKey="modified" sort={sort} onSort={toggleSort} />
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
                    onClick={() => setPath("")}
                    className="ml-3 text-label-13 text-blue-1000 hover:underline"
                  >
                    回到此电脑
                  </button>
                </td>
              </tr>
            ) : sorted.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  {filter ? "没有匹配的文件" : "此目录为空"}
                </td>
              </tr>
            ) : (
              sorted.map((entry) => {
                const isDrive = isDriveRootView;
                const target = joinPath(path, entry.name ?? "");
                return (
                  <tr
                    key={entry.name}
                    onDoubleClick={() => entry.is_dir && setPath(isDrive ? entry.name ?? target : target)}
                    className="group cursor-pointer border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
                  >
                    <td className="px-4 py-2.5">
                      <span className="flex items-center gap-2">
                        <span className={entry.is_dir ? "text-blue-1000" : "text-gray-900"}>
                          {isDrive ? "💽" : entry.is_dir ? "▸" : "▤"}
                        </span>
                        <span
                          className={`text-label-14 ${entry.is_dir ? "text-blue-1000" : ""}`}
                          onDoubleClick={undefined}
                        >
                          {entry.name}
                        </span>
                        {!isDrive && entry.is_dir && (
                          <button
                            type="button"
                            onClick={() => setPath(target)}
                            className="text-label-12 text-blue-1000 opacity-0 transition-opacity duration-150 group-hover:opacity-100"
                          >
                            打开
                          </button>
                        )}
                      </span>
                    </td>
                    <td className="px-4 py-2.5 text-label-13 text-gray-900">
                      {isDrive ? "驱动器" : entry.is_dir ? "目录" : "文件"}
                    </td>
                    <td className="px-4 py-2.5 text-right font-mono text-label-13 text-gray-900 tabular-nums">
                      {entry.is_dir ? "—" : humanSize(entry.size ?? 0)}
                    </td>
                    <td
                      className="px-4 py-2.5 font-mono text-label-13 text-gray-900"
                      title={entry.mode ?? ""}
                    >
                      {isDrive
                        ? DRIVE_TYPE_LABEL[entry.mode ?? ""] ?? entry.mode ?? "—"
                        : entry.mode?.replace(/^([d-])/, "") || "—"}
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
                );
              })
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
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
          toDefault={path || "C:\\"}
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
      <TransferQueue
        transfers={transfers}
        onDismiss={(id) => setTransfers((l) => l.filter((x) => x.id !== id))}
      />
    </div>
  );
}
