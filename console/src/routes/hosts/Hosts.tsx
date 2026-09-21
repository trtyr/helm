import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { RotateCw, Wand2 } from "lucide-react";
import { api } from "../../api/client";
import { toast } from "../../lib/toast";
import { SkeletonRows, ErrorCard } from "../../components/ui";
import { HostFormDrawer, type HostFormValues } from "../../components/HostFormDrawer";
import { DeleteHostDialog } from "../../components/DeleteHostDialog";
import { AgentGenerateDrawer } from "../../components/AgentGenerateDrawer";
import { LIMIT, type HostView } from "./shared";
import { BatchExecDialog, HostListFooter, HostRow } from "./HostParts";

/**
 * /hosts：主机单列表（主机与其 Agent 同体展示）+ 标签过滤 + 搜索 + 编辑。
 * 主机不手工创建——在目标机运行 helm-agent 后自动上线。
 *
 * G13 拆分（2026-09-21）：原为 492 行单文件。现拆为——`hosts/shared.ts`（类型与常量）/
 * `hosts/HostParts.tsx`（单行主机、批量执行对话框、分页底栏）；本文件保留状态、数据与编排。
 */
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
          <div className="overflow-x-auto">
          <table className="w-full text-left">
            <thead>
              <tr className="border-b border-gray-400 text-label-13 text-gray-900">
                <th className="w-10 px-4 py-2.5">
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
                <th className="w-16 px-4 py-2.5 font-normal whitespace-nowrap">状态</th>
                <th className="px-4 py-2.5 font-normal whitespace-nowrap">主机</th>
                <th className="px-4 py-2.5 font-normal whitespace-nowrap">心跳</th>
                <th className="px-4 py-2.5 font-normal whitespace-nowrap">外网 IP</th>
                <th className="px-4 py-2.5 font-normal whitespace-nowrap">内网 IP</th>
                <th className="px-4 py-2.5 font-normal whitespace-nowrap">操作系统</th>
                <th className="px-4 py-2.5 font-normal whitespace-nowrap">连接</th>
                <th className="px-4 py-2.5 font-normal whitespace-nowrap">标签</th>
                <th className="px-4 py-2.5 text-right font-normal whitespace-nowrap">操作</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((h) => (
                <HostRow
                  key={h.id}
                  h={h}
                  now={now}
                  selected={!!h.agent_id && selected.has(h.agent_id)}
                  onToggleSel={() => h.agent_id && toggleSel(h.agent_id)}
                  onOpen={() => navigate(`/hosts/${h.id}/overview`)}
                  onEdit={() => setEditing(h)}
                  onDelete={() => setDeleting(h)}
                />
              ))}
            </tbody>
          </table>
          </div>
        )}

        {/* 底栏分页（非搜索态） */}
        {!hostsQuery.isPending && !hostsQuery.isError && (
          <HostListFooter
            page={page}
            rowCount={rows.length}
            hasNext={hasNext}
            onPrev={() => setPage((p) => p - 1)}
            onNext={() => setPage((p) => p + 1)}
          />
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
        <BatchExecDialog
          count={selected.size}
          cmd={batchCmd}
          args={batchArgs}
          result={batchResult}
          pending={batchMutation.isPending}
          onCmd={setBatchCmd}
          onArgs={setBatchArgs}
          onClose={() => setBatchOpen(false)}
          onSubmit={() => batchMutation.mutate()}
        />
      )}
    </div>
  );
}
