import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Plus } from "lucide-react";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { relativeTime } from "../../lib/format";
import { SkeletonRows, StatusDot, ErrorCard } from "../../components/ui";
import { HostFormDrawer, type HostFormValues } from "../../components/HostFormDrawer";
import { DeleteHostDialog } from "../../components/DeleteHostDialog";
import { AgentsTab } from "./AgentsTab";

type HostView = components["schemas"]["HostView"];

const LIMIT = 20;

/** /hosts（规格 routes/hosts.md）：主机/Agents 双视图 + 标签过滤 + 搜索 + CRUD。 */
export default function Hosts() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [view, setView] = useState<"hosts" | "agents">("hosts");
  const [page, setPage] = useState(1);
  const [tag, setTag] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [drawer, setDrawer] = useState<{
    open: boolean;
    initial?: HostView;
    nonce: number;
  }>({ open: false, nonce: 0 });
  const [deleting, setDeleting] = useState<HostView | null>(null);

  const hostsQuery = useQuery({
    queryKey: ["hosts", page, tag],
    queryFn: () =>
      api<{ hosts: HostView[] }>(
        `/api/v1/hosts?page=${page}&limit=${LIMIT}${tag ? `&tag=${encodeURIComponent(tag)}` : ""}`,
      ),
    refetchInterval: 30_000,
  });

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ["hosts"] });

  const openDrawer = (initial?: HostView) =>
    setDrawer((d) => ({ open: true, initial, nonce: d.nonce + 1 }));
  const closeDrawer = () => setDrawer((d) => ({ ...d, open: false }));

  const saveMutation = useMutation({
    mutationFn: async (values: HostFormValues) => {
      const body = {
        hostname: values.hostname,
        conn_mode: values.conn_mode,
        addr: values.addr,
        tags: values.tags,
      };
      if (drawer.initial) {
        return api(`/api/v1/hosts/${drawer.initial.id}`, { method: "PUT", body });
      }
      return api("/api/v1/hosts", { method: "POST", body });
    },
    onSuccess: () => {
      closeDrawer();
      invalidate();
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (host: HostView) => api(`/api/v1/hosts/${host.id}`, { method: "DELETE" }),
    onSuccess: () => {
      setDeleting(null);
      invalidate();
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

  const total = hostsQuery.data?.hosts?.length ?? 0;
  const hasNext = total === LIMIT && !search;

  return (
    <div className="flex flex-col gap-6">
      {/* 页头 */}
      <div className="flex items-end justify-between">
        <div>
          <h1 className="text-heading-24">主机</h1>
          <p className="mt-1 text-copy-13 text-gray-900">管理被控主机及其 Agent</p>
        </div>
        <button
          type="button"
          onClick={() => openDrawer()}
          className="flex h-8 items-center gap-1.5 rounded-md bg-gray-700 px-3 text-label-14 transition-colors duration-150 hover:bg-gray-800"
        >
          <Plus size={14} strokeWidth={1.5} />
          创建主机
        </button>
      </div>

      {/* 容器卡 */}
      <div className="overflow-hidden rounded-lg border border-gray-400 bg-background-100">
        {/* 页签 + 工具行 */}
        <div className="flex items-center border-b border-gray-400 pl-2">
          <button
            type="button"
            data-testid="tab-hosts"
            onClick={() => setView("hosts")}
            className={`h-10 border-b-2 px-3 text-label-14 transition-colors duration-150 ${
              view === "hosts"
                ? "border-blue-1000 text-gray-1000"
                : "border-transparent text-gray-900 hover:text-gray-1000"
            }`}
          >
            主机
          </button>
          <button
            type="button"
            data-testid="tab-agents"
            onClick={() => setView("agents")}
            className={`h-10 border-b-2 px-3 text-label-14 transition-colors duration-150 ${
              view === "agents"
                ? "border-blue-1000 text-gray-1000"
                : "border-transparent text-gray-900 hover:text-gray-1000"
            }`}
          >
            Agents
          </button>

          {view === "hosts" && (
            <div className="ml-auto flex items-center gap-2 p-2">
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
                className="h-8 w-44 rounded-md border border-gray-400 bg-gray-100 px-3 text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
              />
            </div>
          )}
        </div>

        {view === "agents" ? (
          <AgentsTab />
        ) : hostsQuery.isPending ? (
          <table className="w-full">
            <tbody>
              <SkeletonRows rows={8} cols={6} />
            </tbody>
          </table>
        ) : hostsQuery.isError ? (
          <ErrorCard detail={hostsQuery.error.message} onRetry={() => hostsQuery.refetch()} />
        ) : rows.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16">
            <p className="text-copy-13 text-gray-900">
              {tag || search ? "没有匹配的主机" : "还没有主机"}
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
                <th className="px-4 py-2 font-normal">状态</th>
                <th className="px-4 py-2 font-normal">主机名</th>
                <th className="px-4 py-2 font-normal">标签</th>
                <th className="px-4 py-2 font-normal">系统</th>
                <th className="px-4 py-2 font-normal">模式</th>
                <th className="px-4 py-2 font-normal">最后心跳</th>
                <th className="px-4 py-2" />
              </tr>
            </thead>
            <tbody>
              {rows.map((h) => (
                <tr
                  key={h.id}
                  onClick={() => navigate(`/hosts/${h.id}/overview`)}
                  className="group cursor-pointer border-b border-gray-400/60 transition-colors duration-150 hover:bg-gray-100"
                >
                  <td className="px-4 py-2.5">
                    <StatusDot online={!!h.online} stale={h.stale} />
                  </td>
                  <td className="px-4 py-2.5 text-label-14">{h.hostname}</td>
                  <td className="px-4 py-2.5">
                    <span className="flex flex-wrap gap-1">
                      {h.tags?.slice(0, 3).map((t) => (
                        <span
                          key={t}
                          className="rounded border border-gray-400 bg-gray-200 px-1.5 py-0.5 text-label-12"
                        >
                          {t}
                        </span>
                      ))}
                      {(h.tags?.length ?? 0) > 3 && (
                        <span className="text-label-12 text-gray-900">
                          +{h.tags!.length - 3}
                        </span>
                      )}
                    </span>
                  </td>
                  <td className="px-4 py-2.5 text-label-13 text-gray-900">
                    {h.os || "—"}
                    {h.arch ? ` · ${h.arch}` : ""}
                  </td>
                  <td className="px-4 py-2.5 text-label-13 text-gray-900">
                    {h.conn_mode === "forward" ? "正向" : "反向"}
                  </td>
                  <td className="px-4 py-2.5 text-label-13 text-gray-900">
                    {relativeTime(h.last_seen)}
                  </td>
                  <td className="px-4 py-2.5 text-right">
                    <button
                      type="button"
                      aria-label={`编辑 ${h.hostname}`}
                      onClick={(e) => {
                        e.stopPropagation();
                        openDrawer(h);
                      }}
                      className="mr-2 text-label-13 text-gray-900 opacity-0 transition-opacity duration-150 hover:text-blue-1000 group-hover:opacity-100"
                    >
                      编辑
                    </button>
                    <button
                      type="button"
                      aria-label={`删除 ${h.hostname}`}
                      onClick={(e) => {
                        e.stopPropagation();
                        setDeleting(h);
                      }}
                      className="text-label-13 text-gray-900 opacity-0 transition-opacity duration-150 hover:text-red-1000 group-hover:opacity-100"
                    >
                      删除
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}

        {/* 底栏分页（view=hosts 且非搜索态） */}
        {view === "hosts" && !hostsQuery.isPending && !hostsQuery.isError && (
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
        key={drawer.nonce}
        open={drawer.open}
        initial={drawer.initial}
        onClose={closeDrawer}
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
    </div>
  );
}
