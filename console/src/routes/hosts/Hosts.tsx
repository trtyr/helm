import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Pencil, RotateCw, Trash2, Wand2 } from "lucide-react";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { toast } from "../../lib/toast";
import { relativeTime } from "../../lib/format";
import { SkeletonRows, StatusDot, ErrorCard } from "../../components/ui";
import { HostFormDrawer, type HostFormValues } from "../../components/HostFormDrawer";
import { DeleteHostDialog } from "../../components/DeleteHostDialog";
import { AgentGenerateDrawer } from "../../components/AgentGenerateDrawer";

type HostView = components["schemas"]["HostView"];

const LIMIT = 20;

const OS_LABEL: Record<string, string> = {
  windows: "Windows",
  linux: "Linux",
  macos: "macOS",
  darwin: "macOS",
};

/** /hosts：主机单列表（主机与其 Agent 同体展示）+ 标签过滤 + 搜索 + 编辑。
 * 主机不手工创建——在目标机运行 helm-agent 后自动上线。 */
export default function Hosts() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [page, setPage] = useState(1);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [batchOpen, setBatchOpen] = useState(false);
  const [batchCmd, setBatchCmd] = useState("cmd");
  const [batchArgs, setBatchArgs] = useState("/c tasklist");
  const [batchResult, setBatchResult] = useState<{ agent_id: string; job_id?: string; error?: string }[]>([]);

  const batchMutation = useMutation({
    mutationFn: () =>
      api<{ total: number; jobs: { agent_id: string; job_id?: string; error?: string }[] }>(
        "/api/v1/exec/batch",
        {
          method: "POST",
          body: {
            agent_ids: [...selected],
            command: batchCmd.trim(),
            args: batchArgs.split(/\s+/).filter(Boolean),
          },
        },
      ),
    onSuccess: (r) => {
      setBatchResult(r.jobs ?? []);
      toast(`已下发 ${r.total} 台`, "success");
    },
    onError: (e) => toast(e.message, "error"),
  });

  const toggleSel = (id: string) => {
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  };
  const [tag, setTag] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [editing, setEditing] = useState<HostView | null>(null);
  const [deleting, setDeleting] = useState<HostView | null>(null);
  const [generating, setGenerating] = useState(false);

  const hostsQuery = useQuery({
    queryKey: ["hosts", page, tag],
    queryFn: async () => {
      const res = await api<{ hosts: HostView[] }>(
        `/api/v1/hosts?page=${page}&limit=${LIMIT}${tag ? `&tag=${encodeURIComponent(tag)}` : ""}`,
      );
      return { ...res, fetchedAt: Date.now() }; // now 在异步侧产生（render 纯度）
    },
    refetchInterval: 30_000,
  });
  const now = hostsQuery.data?.fetchedAt ?? 0;

  const saveMutation = useMutation({
    mutationFn: (values: HostFormValues) => {
      const body = {
        hostname: values.hostname,
        conn_mode: values.conn_mode,
        addr: values.addr,
        tags: values.tags,
      };
      return api(`/api/v1/hosts/${editing?.id ?? ""}`, { method: "PUT", body });
    },
    onSuccess: () => {
      setEditing(null);
      queryClient.invalidateQueries({ queryKey: ["hosts"] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (host: HostView) => api(`/api/v1/hosts/${host.id}`, { method: "DELETE" }),
    onSuccess: () => {
      setDeleting(null);
      queryClient.invalidateQueries({ queryKey: ["hosts"] });
    },
  });

  // 前端本地搜索（叠加在服务器分页/过滤之上）
  const rows = useMemo(() => {
    const list = hostsQuery.data?.hosts ?? [];
    if (!search) return list;
    return list.filter((h) => h.hostname?.toLowerCase().includes(search.toLowerCase()));
  }, [hostsQuery.data, search]);

  const allTags = useMemo(() => {
    const set = new Set<string>();
    (hostsQuery.data?.hosts ?? []).forEach((h) => h.tags?.forEach((t) => set.add(t)));
    return Array.from(set).sort();
  }, [hostsQuery.data]);

  const hasNext = (hostsQuery.data?.hosts?.length ?? 0) === LIMIT && !search;

  return (
    <div className="flex flex-col gap-5">
      {/* 页头 */}
      <div>
        <h1 className="text-heading-24">主机</h1>
        <p className="mt-1 text-copy-13 text-gray-900">
          主机与 Agent 一体展示——在目标机运行 helm-agent 后自动上线
        </p>
      </div>

      {/* 列表卡 */}
      <div className="overflow-hidden rounded-lg border border-gray-400 bg-background-100">
        {/* 工具行 */}
        <div className="flex items-center gap-2 border-b border-gray-400 px-4 py-2">
          <button
          type="button"
          disabled={selected.size === 0}
          onClick={() => {
            setBatchResult([]);
            setBatchOpen(true);
          }}
          className="h-8 rounded-md border border-gray-500 px-3 text-label-13 text-gray-900 hover:bg-gray-200 disabled:opacity-40"
        >
          批量执行（已选 {selected.size}）
        </button>
        {allTags.length > 0 && (
            <select
              value={tag ?? ""}
              onChange={(e) => {
                setTag(e.target.value || null);
                setPage(1);
              }}
              aria-label="标签过滤"
              className="h-8 rounded-md border border-gray-400 bg-gray-100 px-2 text-label-13 outline-none transition-colors duration-150 hover:border-gray-500"
            >
              <option value="">全部标签</option>
              {allTags.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          )}
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索主机名"
            className="ml-auto h-8 w-44 rounded-md border border-gray-400 bg-gray-100 px-3 text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
          <button
            type="button"
            aria-label="刷新列表"
            title="刷新列表"
            onClick={() => queryClient.invalidateQueries({ queryKey: ["hosts"] })}
            className="flex h-8 w-8 items-center justify-center rounded-md border border-gray-400 text-gray-900 transition-colors duration-150 hover:border-gray-500 hover:text-gray-1000"
          >
            <RotateCw size={14} strokeWidth={1.5} />
          </button>
          <button
            type="button"
            onClick={() => setGenerating(true)}
            className="flex h-8 items-center gap-1.5 rounded-md bg-gray-700 px-3 text-label-13 transition-colors duration-150 hover:bg-gray-800"
          >
            <Wand2 size={14} strokeWidth={1.5} />
            生成 Agent
          </button>
        </div>

        {hostsQuery.isPending ? (
          <table className="w-full">
            <tbody>
              <SkeletonRows rows={8} cols={8} />
            </tbody>
          </table>
        ) : hostsQuery.isError ? (
          <ErrorCard detail={hostsQuery.error.message} onRetry={() => hostsQuery.refetch()} />
        ) : rows.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16">
            <p className="text-copy-13 text-gray-900">
              {tag || search
                ? "没有匹配的主机"
                : "还没有主机上线——在目标机运行 helm-agent 后自动出现在这里"}
            </p>
            {(tag || search) && (
              <button
                type="button"
                onClick={() => {
                  setTag(null);
                  setSearch("");
                }}
                className="mt-3 text-label-13 text-blue-1000 hover:underline"
              >
                清除过滤
              </button>
            )}
          </div>
        ) : (
          <table className="w-full text-left">
            <thead>
              <tr className="border-b border-gray-400 text-label-13 text-gray-900">
                <th className="w-10 px-3 py-2.5">
                  <input
                    type="checkbox"
                    aria-label="全选"
                    checked={rows.length > 0 && rows.every((h) => selected.has(h.agent_id ?? ""))}
                    onChange={(e) => {
                      if (e.target.checked) {
                        setSelected(new Set(rows.map((h) => h.agent_id ?? "").filter(Boolean)));
                      } else {
                        setSelected(new Set());
                      }
                    }}
                    className="h-3.5 w-3.5 accent-blue-1000"
                  />
                </th>
                <th className="w-14 px-4 py-2.5 font-normal">状态</th>
                <th className="px-4 py-2.5 font-normal">主机</th>
                <th className="px-4 py-2.5 font-normal">心跳</th>
                <th className="px-4 py-2.5 font-normal">外网 IP</th>
                <th className="px-4 py-2.5 font-normal">内网 IP</th>
                <th className="px-4 py-2.5 font-normal">操作系统</th>
                <th className="px-4 py-2.5 font-normal">连接</th>
                <th className="px-4 py-2.5 font-normal">标签</th>
                <th className="px-4 py-2.5 text-right font-normal">操作</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((h) => {
                const localIp = h.local_ips?.find((ip) => !ip.includes(":")) ?? h.local_ips?.[0];
                const extra = (h.local_ips?.length ?? 0) - 1;
                return (
                  <tr
                    key={h.id}
                    onClick={() => navigate(`/hosts/${h.id}/overview`)}
                    className="group cursor-pointer border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
                  >
                    <td className="px-4 py-3">
                      <input
                        type="checkbox"
                        aria-label={`选择 ${h.hostname}`}
                        checked={!!h.agent_id && selected.has(h.agent_id)}
                        disabled={!h.agent_id}
                        onChange={() => h.agent_id && toggleSel(h.agent_id)}
                        className="h-3.5 w-3.5 accent-blue-1000"
                      />
                    </td>
                    <td className="px-4 py-3">
                      <StatusDot online={!!h.online} stale={h.stale} />
                    </td>
                    <td className="px-4 py-3">
                      <div className="text-label-14">{h.hostname}</div>
                      <div className="mt-0.5 flex items-center gap-1.5 font-mono text-label-12 text-gray-900">
                        <span>
                          {h.agent_id ? `${h.agent_id} · v${h.agent_version ?? "?"}` : "未注册 Agent"}
                        </span>
                        {h.agent_elevated != null && (
                          <span
                            className={`whitespace-nowrap rounded px-1 py-px text-label-12 ${
                              h.agent_elevated ? "bg-green-1000/10 text-green-1000" : "bg-gray-200 text-gray-900"
                            }`}
                          >
                            {h.agent_elevated ? "管理员" : "普通"}
                          </span>
                        )}
                      </div>
                    </td>
                    <td
                      className="px-4 py-3 font-mono text-label-13 text-gray-900"
                      title={h.last_seen ?? undefined}
                    >
                      {relativeTime(h.last_seen, now)}
                    </td>
                    <td className="px-4 py-3 font-mono text-label-13 text-gray-1000">
                      {h.public_ip || "—"}
                    </td>
                    <td className="px-4 py-3 font-mono text-label-13 text-gray-1000">
                      {localIp ? (
                        <span title={h.local_ips?.join("\n")}>
                          {localIp}
                          {extra > 0 && (
                            <span className="ml-1 text-label-12 text-gray-900">+{extra}</span>
                          )}
                        </span>
                      ) : (
                        "—"
                      )}
                    </td>
                    <td
                      className="px-4 py-3 text-label-13 text-gray-900"
                      title={[h.os_version, h.kernel].filter(Boolean).join(" · ") || h.platform}
                    >
                      {h.os ? OS_LABEL[h.os] ?? h.os : "—"}
                      {h.arch ? ` · ${h.arch}` : ""}
                    </td>
                    <td className="px-4 py-3">
                      <span
                        className={`rounded border px-1.5 py-0.5 text-label-12 ${
                          h.conn_mode === "forward"
                            ? "border-blue-1000/40 bg-blue-1000/10 text-blue-1000"
                            : "border-gray-400 bg-gray-200 text-gray-900"
                        }`}
                        title={h.conn_mode === "forward" ? `拨号地址 ${h.addr || "—"}` : "Agent 主动连接 Server"}
                      >
                        {h.conn_mode === "forward" ? "正向" : "反向"}
                      </span>
                    </td>
                    <td className="max-w-40 px-4 py-3">
                      <span className="flex flex-wrap gap-1">
                        {h.tags?.slice(0, 2).map((t) => (
                          <span
                            key={t}
                            className="rounded border border-gray-400 bg-gray-200 px-1.5 py-0.5 text-label-12"
                          >
                            {t}
                          </span>
                        ))}
                        {(h.tags?.length ?? 0) > 2 && (
                          <span
                            className="text-label-12 text-gray-900"
                            title={h.tags?.join("、")}
                          >
                            +{h.tags!.length - 2}
                          </span>
                        )}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-right">
                      <span className="inline-flex items-center gap-1">
                        <button
                          type="button"
                          aria-label={`编辑 ${h.hostname}`}
                          title="编辑"
                          onClick={(e) => {
                            e.stopPropagation();
                            setEditing(h);
                          }}
                          className="flex h-7 w-7 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-blue-1000"
                        >
                          <Pencil size={14} strokeWidth={1.5} />
                        </button>
                        <button
                          type="button"
                          aria-label={`删除 ${h.hostname}`}
                          title="删除"
                          onClick={(e) => {
                            e.stopPropagation();
                            setDeleting(h);
                          }}
                          className="flex h-7 w-7 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-red-1000"
                        >
                          <Trash2 size={14} strokeWidth={1.5} />
                        </button>
                      </span>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}

        {/* 底栏分页（非搜索态） */}
        {!hostsQuery.isPending && !hostsQuery.isError && (
          <div className="flex h-12 items-center justify-between border-t border-gray-400 px-4 text-label-13 text-gray-900">
            <span>
              {rows.length > 0 ? `第 ${page} 页 · 本页 ${rows.length} 台` : "无主机"}
            </span>
            <span className="flex items-center gap-1">
              <button
                type="button"
                disabled={page <= 1}
                onClick={() => setPage((p) => p - 1)}
                aria-label="上一页"
                className="flex h-7 w-7 items-center justify-center rounded-md transition-colors duration-150 hover:bg-gray-200 disabled:opacity-30"
              >
                <ChevronLeft size={14} strokeWidth={1.5} />
              </button>
              <span className="font-mono">{page}</span>
              <button
                type="button"
                disabled={!hasNext}
                onClick={() => setPage((p) => p + 1)}
                aria-label="下一页"
                className="flex h-7 w-7 items-center justify-center rounded-md transition-colors duration-150 hover:bg-gray-200 disabled:opacity-30"
              >
                <ChevronRight size={14} strokeWidth={1.5} />
              </button>
            </span>
          </div>
        )}
      </div>

      <HostFormDrawer
        key={editing?.id ?? "none"}
        open={!!editing}
        initial={editing ?? undefined}
        onClose={() => setEditing(null)}
        onSubmit={(v) => saveMutation.mutate(v)}
        submitting={saveMutation.isPending}
        error={saveMutation.isError ? saveMutation.error.message : null}
      />
      <DeleteHostDialog
        hostname={deleting?.hostname ?? null}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting && deleteMutation.mutate(deleting)}
        submitting={deleteMutation.isPending}
      />
      <AgentGenerateDrawer open={generating} onClose={() => setGenerating(false)} />
      {/* 批量执行对话框 */}
      {batchOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <button
            type="button"
            aria-label="关闭"
            onClick={() => setBatchOpen(false)}
            className="absolute inset-0 bg-black/40"
          />
          <div className="relative z-10 flex w-[520px] flex-col gap-4 rounded-xl border border-gray-400 bg-background-100 p-6">
            <h2 className="text-heading-16">批量执行命令（{selected.size} 台）</h2>
            <div className="flex items-center gap-2">
              <span className="w-16 text-label-13 text-gray-900">命令</span>
              <input
                value={batchCmd}
                onChange={(e) => setBatchCmd(e.target.value)}
                className="h-8 flex-1 rounded-md border border-gray-400 bg-gray-100 px-2 font-mono text-label-13 outline-none hover:border-gray-500"
              />
            </div>
            <div className="flex items-center gap-2">
              <span className="w-16 text-label-13 text-gray-900">参数</span>
              <input
                value={batchArgs}
                onChange={(e) => setBatchArgs(e.target.value)}
                className="h-8 flex-1 rounded-md border border-gray-400 bg-gray-100 px-2 font-mono text-label-13 outline-none hover:border-gray-500"
              />
            </div>
            {batchResult.length > 0 && (
              <div className="max-h-40 overflow-y-auto rounded-md border border-gray-400 bg-gray-100 p-2 font-mono text-label-12">
                {batchResult.map((r) => (
                  <div key={r.agent_id} className={r.error ? "text-red-1000" : "text-green-1000"}>
                    {r.agent_id}: {r.job_id ? `job ${r.job_id.slice(0, 8)}…` : r.error}
                  </div>
                ))}
              </div>
            )}
            <div className="flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setBatchOpen(false)}
                className="h-8 rounded-md border border-gray-500 px-4 text-label-14 hover:bg-gray-200"
              >
                关闭
              </button>
              <button
                type="button"
                disabled={batchMutation.isPending}
                onClick={() => batchMutation.mutate()}
                className="h-8 rounded-md bg-gray-700 px-4 text-label-14 text-white hover:bg-gray-800 disabled:opacity-50"
              >
                {batchMutation.isPending ? "下发中…" : "全部下发"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>

  );
}
