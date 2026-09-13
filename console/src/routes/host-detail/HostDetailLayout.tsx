import { useMemo, useState } from "react";
import { copyText } from "../../lib/clipboard";
import { Link, NavLink, Outlet, useLocation, useNavigate, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, Copy } from "lucide-react";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { StatusDot } from "../../components/ui";
import { HostFormDrawer, type HostFormValues } from "../../components/HostFormDrawer";
import { DeleteHostDialog } from "../../components/DeleteHostDialog";
import { toast } from "../../lib/toast";

type HostView = components["schemas"]["HostView"];

const TABS = [
  { seg: "overview", label: "概览", ready: true, os: null },
  { seg: "terminal", label: "终端", ready: true, os: null },
  { seg: "files", label: "文件", ready: true, os: null },
  { seg: "services", label: "服务", ready: true, os: null },
  { seg: "processes", label: "进程", ready: true, os: null },
  { seg: "network", label: "网络", ready: true, os: null },
  // 以下为 Windows 专属 IR 能力（agent 其余平台返回「仅支持 Windows」）
  { seg: "autostart", label: "自启动项", ready: true, os: "windows" },
  { seg: "syslog", label: "系统日志", ready: true, os: "windows" },
  { seg: "memscan", label: "内存扫描", ready: true, os: "windows" },
  { seg: "proxy", label: "代理", ready: true, os: null },
  { seg: "tasks", label: "任务", ready: true, os: null },
] as const;

/** 受操作守卫的 tab（离线时显示通栏并禁用操作；概览/指标/任务不受限，规格 host-detail.md）。 */
const GUARDED = new Set(["terminal", "files", "services", "processes", "network", "proxy", "autostart", "syslog", "memscan"]);

/** /hosts/:id 布局框架：主机头 + 8 页签 + tab 内容（子路由 Outlet）。 */
export default function HostDetailLayout() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const location = useLocation();
  const queryClient = useQueryClient();
  const [tagsModal, setTagsModal] = useState(false);
  const [drawer, setDrawer] = useState<{ open: boolean; initial?: HostView; nonce: number }>({
    open: false,
    nonce: 0,
  });
  const [deleting, setDeleting] = useState(false);

  // 复用列表缓存（后端无 GET /hosts/{id} 单查，缺口见 plantree open-questions #9）
  const { data, isPending } = useQuery({
    queryKey: ["hosts", 1, null],
    queryFn: () => api<{ hosts: HostView[] }>("/api/v1/hosts?page=1&limit=20"),
    refetchInterval: 30_000,
  });
  const host = useMemo(() => data?.hosts.find((h) => h.id === id), [data, id]);

  const saveTags = useMutation({
    mutationFn: (tags: string[]) =>
      api(`/api/v1/hosts/${id}/tags`, { method: "POST", body: { tags } }),
    onSuccess: () => {
      setTagsModal(false);
      queryClient.invalidateQueries({ queryKey: ["hosts"] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: () => api(`/api/v1/hosts/${id}`, { method: "DELETE" }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["hosts"] });
      navigate("/hosts", { replace: true });
    },
  });

  const editMutation = useMutation({
    mutationFn: (values: HostFormValues) =>
      api(`/api/v1/hosts/${id}`, {
        method: "PUT",
        body: {
          hostname: values.hostname,
          conn_mode: values.conn_mode,
          addr: values.addr,
          tags: values.tags,
        },
      }),
    onSuccess: () => {
      setDrawer((d) => ({ ...d, open: false }));
      queryClient.invalidateQueries({ queryKey: ["hosts"] });
    },
  });

  if (isPending) {
    return (
      <div className="animate-pulse space-y-6">
        <div className="h-8 w-56 rounded bg-gray-200" />
        <div className="h-10 rounded bg-gray-200" />
        <div className="h-64 rounded bg-gray-100" />
      </div>
    );
  }
  if (!host) {
    return (
      <div className="flex flex-col items-center justify-center rounded-lg border border-gray-400 py-24">
        <p className="text-label-14 text-red-1000">主机不存在或已删除</p>
        <Link to="/hosts" className="mt-4 text-label-13 text-blue-1000 hover:underline">
          返回主机列表 →
        </Link>
      </div>
    );
  }

  const offline = !host.online;
  // 离线守卫通栏：仅守卫 tab 显示（概览/指标/任务不受限，规格 host-detail.md）
  const activeSeg = location.pathname.split("/")[3] ?? "overview";
  const guardedOffline = offline && GUARDED.has(activeSeg);

  return (
    <div className="flex flex-col gap-6">
      {/* 主机头 */}
      <div>
        <Link
          to="/hosts"
          className="flex items-center gap-1.5 text-label-13 text-gray-900 transition-colors duration-150 hover:text-gray-1000"
        >
          <ArrowLeft size={14} strokeWidth={1.5} />
          返回主机列表
        </Link>
        <div className="mt-2 flex flex-wrap items-center gap-3">
          <StatusDot online={!!host.online} stale={host.stale} />
          <h1 className="text-heading-24">{host.hostname}</h1>
          {host.tags?.map((t) => (
            <span
              key={t}
              className="rounded border border-gray-400 bg-gray-200 px-1.5 py-0.5 text-label-12"
            >
              {t}
            </span>
          ))}
          <span className="text-label-13 text-gray-900">
            {host.online ? "在线" : host.stale ? "心跳超时" : "离线"} · {host.os || "—"}
            {host.arch ? ` · ${host.arch}` : ""} · {host.conn_mode === "forward" ? "正向" : "反向"}
          </span>
          <AgentPrivilegeBadge hostId={host.id ?? ""} />
          <span className="ml-auto flex items-center gap-2">
            <button
              type="button"
              onClick={() => setTagsModal(true)}
              className="h-8 rounded-md border border-gray-500 px-3 text-label-13 transition-colors duration-150 hover:bg-gray-200"
            >
              编辑标签
            </button>
            <button
              type="button"
              onClick={() =>
                setDrawer((d) => ({ open: true, initial: host, nonce: d.nonce + 1 }))
              }
              className="h-8 rounded-md border border-gray-500 px-3 text-label-13 transition-colors duration-150 hover:bg-gray-200"
            >
              编辑
            </button>
            <button
              type="button"
              onClick={() => copyText(host.id ?? "").then((ok) => toast(ok ? "已复制 host_id" : "复制失败", ok ? undefined : "warn"))}
              aria-label="复制 host_id"
              className="flex h-8 w-8 items-center justify-center rounded-md border border-gray-500 text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000"
            >
              <Copy size={14} strokeWidth={1.5} />
            </button>
            <button
              type="button"
              onClick={() => setDeleting(true)}
              className="h-8 rounded-md border border-red-700 px-3 text-label-13 text-red-1000 transition-colors duration-150 hover:bg-red-700/10"
            >
              删除
            </button>
          </span>
        </div>
      </div>

      {/* 页签 */}
      <nav className="flex items-center gap-6 border-b border-gray-400">
        {TABS.filter(({ os }) => !os || os === host?.os).map(({ seg, label, ready }) => (
          <NavLink
            key={seg}
            to={seg}
            replace
            className={({ isActive }) =>
              `relative flex h-10 items-center gap-1.5 text-label-14 transition-colors duration-150 ${
                isActive ? "text-gray-1000" : "text-gray-900 hover:text-gray-1000"
              } ${ready ? "" : "opacity-50"}`
            }
          >
            {({ isActive }) => (
              <>
                {isActive && (
                  <span className="absolute inset-x-0 bottom-0 h-0.5 rounded-full bg-blue-1000" />
                )}
                {label}
                {!ready && <span className="text-label-12 text-gray-900">M3</span>}
              </>
            )}
          </NavLink>
        ))}
      </nav>

      {/* 离线守卫通栏：仅守卫 tab 且主机离线时显示（ Outlet 上方） */}
      {guardedOffline && <OfflineBanner />}

      {/* tab 内容：守卫 tab 离线时仍渲染但禁操作（子路由自行消费 offline 上下文） */}
      <div className="pb-8" data-host-online={host.online ? "1" : "0"} data-host-id={host.id}>
        <Outlet context={{ host }} />
      </div>

      <TagsModal
        open={tagsModal}
        initial={host.tags ?? []}
        submitting={saveTags.isPending}
        error={saveTags.isError ? saveTags.error.message : null}
        onClose={() => setTagsModal(false)}
        onSave={(tags) => saveTags.mutate(tags)}
      />
      <HostFormDrawer
        key={drawer.nonce}
        open={drawer.open}
        initial={drawer.initial}
        onClose={() => setDrawer((d) => ({ ...d, open: false }))}
        onSubmit={(v) => editMutation.mutate(v)}
        submitting={editMutation.isPending}
        error={editMutation.isError ? editMutation.error.message : null}
      />
      {deleting && (
        <DeleteHostDialog
          hostname={host.hostname ?? ""}
          onClose={() => setDeleting(false)}
          onConfirm={() => deleteMutation.mutate()}
          submitting={deleteMutation.isPending}
        />
      )}
    </div>
  );
}

function OfflineBanner() {
  return (
    <div
      role="status"
      className="flex items-center gap-2 rounded-md border border-amber-700 bg-amber-700/10 px-4 py-2 text-label-13 text-amber-1000"
    >
      主机离线——操作将在其上线后可用
    </div>
  );
}

/** 标签编辑模态（规格 host-detail.md：chip 增删）。 */
function TagsModal({
  open,
  initial,
  submitting,
  error,
  onClose,
  onSave,
}: {
  open: boolean;
  initial: string[];
  submitting: boolean;
  error: string | null;
  onClose: () => void;
  onSave: (tags: string[]) => void;
}) {
  const [tags, setTags] = useState(initial);
  const [draft, setDraft] = useState("");
  if (!open) return null;

  const add = () => {
    const v = draft.trim();
    if (v && !tags.includes(v)) setTags([...tags, v]);
    setDraft("");
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <button type="button" aria-label="关闭" onClick={onClose} className="absolute inset-0 bg-black/40" />
      <div
        role="dialog"
        aria-label="编辑标签"
        className="relative z-10 w-[360px] rounded-xl border border-gray-400 bg-background-100 p-6"
      >
        <h2 className="text-heading-16">编辑标签</h2>
        <div className="mt-4 flex flex-wrap gap-1.5">
          {tags.map((t) => (
            <span
              key={t}
              className="flex items-center gap-1 rounded border border-gray-400 bg-gray-200 px-1.5 py-0.5 text-label-12"
            >
              {t}
              <button
                type="button"
                aria-label={`移除 ${t}`}
                onClick={() => setTags(tags.filter((x) => x !== t))}
                className="text-gray-900 transition-colors duration-150 hover:text-red-1000"
              >
                ×
              </button>
            </span>
          ))}
        </div>
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), add())}
          onBlur={add}
          placeholder="输入后回车添加"
          className="mt-3 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
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
            type="button"
            onClick={() => onSave(tags)}
            disabled={submitting}
            className="h-8 rounded-md bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-50"
          >
            {submitting ? "保存中…" : "保存"}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Agent 权限徽章：在线的优先；无 agent 不显示。 */
function AgentPrivilegeBadge({ hostId }: { hostId: string }) {
  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: components["schemas"]["Agent"][] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const same = (agentsQuery.data?.agents ?? []).filter((a) => a.host_id === hostId);
  const agent = same.find((a) => a.online) ?? same[0];
  if (!agent || agent.elevated === undefined || agent.elevated === null) return null;
  return (
    <span
      className={`whitespace-nowrap rounded px-1.5 py-0.5 text-label-12 ${
        agent.elevated ? "bg-green-1000/10 text-green-1000" : "bg-gray-200 text-gray-900"
      }`}
      title={agent.elevated ? "Agent 以管理员权限运行：可读系统进程、安全日志" : "Agent 以普通权限运行：无法读系统进程/安全日志"}
    >
      {agent.elevated ? "管理员权限" : "普通权限"}
    </span>
  );
}
