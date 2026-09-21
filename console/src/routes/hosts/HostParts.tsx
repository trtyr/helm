// 主机列表页的视图部件（G13 拆分，2026-09-21）——自 `Hosts.tsx` 拆出：
// 单行主机、批量执行对话框、分页底栏。
import { ChevronLeft, ChevronRight, Pencil, Trash2 } from "lucide-react";
import { StatusDot } from "../../components/ui";
import { relativeTime } from "../../lib/format";
import { OS_LABEL, type HostView } from "./shared";

/** 单行主机：勾选 / 状态 / 主机名+Agent / 心跳 / IP / 系统 / 连接模式 / 标签 / 操作。 */
export function HostRow({
  h,
  now,
  selected,
  onToggleSel,
  onOpen,
  onEdit,
  onDelete,
}: {
  h: HostView;
  now: number;
  selected: boolean;
  onToggleSel: () => void;
  onOpen: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const localIp = h.local_ips?.find((ip) => !ip.includes(":")) ?? h.local_ips?.[0];
  const extra = (h.local_ips?.length ?? 0) - 1;
  return (
    <tr
      onClick={onOpen}
      className="group cursor-pointer border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
    >
      <td className="px-4 py-3">
        <input
          type="checkbox"
          aria-label={`选择 ${h.hostname}`}
          checked={selected}
          disabled={!h.agent_id}
          onClick={(e) => e.stopPropagation()}
          onChange={onToggleSel}
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
        className="px-4 py-3 font-mono text-label-13 text-gray-900 whitespace-nowrap"
        title={h.last_seen ?? undefined}
      >
        {relativeTime(h.last_seen, now)}
      </td>
      <td className="px-4 py-3 font-mono text-label-13 text-gray-1000 whitespace-nowrap">
        {h.public_ip || "—"}
      </td>
      <td className="px-4 py-3 font-mono text-label-13 text-gray-1000 whitespace-nowrap">
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
              onEdit();
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
              onDelete();
            }}
            className="flex h-7 w-7 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-red-1000"
          >
            <Trash2 size={14} strokeWidth={1.5} />
          </button>
        </span>
      </td>
    </tr>
  );
}

/** 批量执行对话框（对选中主机统一下发命令）。 */
export function BatchExecDialog({
  count,
  cmd,
  args,
  result,
  pending,
  onCmd,
  onArgs,
  onClose,
  onSubmit,
}: {
  count: number;
  cmd: string;
  args: string;
  result: { agent_id: string; job_id?: string; error?: string }[];
  pending: boolean;
  onCmd: (v: string) => void;
  onArgs: (v: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <button type="button" aria-label="关闭" onClick={onClose} className="absolute inset-0 bg-black/40" />
      <div className="relative z-10 flex w-[520px] flex-col gap-4 rounded-xl border border-gray-400 bg-background-100 p-6">
        <h2 className="text-heading-16">批量执行命令（{count} 台）</h2>
        <div className="flex items-center gap-2">
          <span className="w-16 text-label-13 text-gray-900">命令</span>
          <input
            value={cmd}
            onChange={(e) => onCmd(e.target.value)}
            className="h-8 flex-1 rounded-md border border-gray-400 bg-gray-100 px-2 font-mono text-label-13 outline-none hover:border-gray-500"
          />
        </div>
        <div className="flex items-center gap-2">
          <span className="w-16 text-label-13 text-gray-900">参数</span>
          <input
            value={args}
            onChange={(e) => onArgs(e.target.value)}
            className="h-8 flex-1 rounded-md border border-gray-400 bg-gray-100 px-2 font-mono text-label-13 outline-none hover:border-gray-500"
          />
        </div>
        {result.length > 0 && (
          <div className="max-h-40 overflow-y-auto rounded-md border border-gray-400 bg-gray-100 p-2 font-mono text-label-12">
            {result.map((r) => (
              <div key={r.agent_id} className={r.error ? "text-red-1000" : "text-green-1000"}>
                {r.agent_id}: {r.job_id ? `job ${r.job_id.slice(0, 8)}…` : r.error}
              </div>
            ))}
          </div>
        )}
        <div className="flex justify-end gap-3">
          <button
            type="button"
            onClick={onClose}
            className="h-8 rounded-md border border-gray-500 px-4 text-label-14 hover:bg-gray-200"
          >
            关闭
          </button>
          <button
            type="button"
            disabled={pending}
            onClick={onSubmit}
            className="h-8 rounded-md bg-gray-700 px-4 text-label-14 text-white hover:bg-gray-800 disabled:opacity-50"
          >
            {pending ? "下发中…" : "全部下发"}
          </button>
        </div>
      </div>
    </div>
  );
}

/** 底栏分页（非搜索态）。 */
export function HostListFooter({
  page,
  rowCount,
  hasNext,
  onPrev,
  onNext,
}: {
  page: number;
  rowCount: number;
  hasNext: boolean;
  onPrev: () => void;
  onNext: () => void;
}) {
  return (
    <div className="flex h-12 items-center justify-between border-t border-gray-400 px-4 text-label-13 text-gray-900">
      <span>
        {rowCount > 0 ? `第 ${page} 页 · 本页 ${rowCount} 台` : "无主机"}
      </span>
      <span className="flex items-center gap-1">
        <button
          type="button"
          disabled={page <= 1}
          onClick={onPrev}
          aria-label="上一页"
          className="flex h-7 w-7 items-center justify-center rounded-md transition-colors duration-150 hover:bg-gray-200 disabled:opacity-30"
        >
          <ChevronLeft size={14} strokeWidth={1.5} />
        </button>
        <span className="font-mono">{page}</span>
        <button
          type="button"
          disabled={!hasNext}
          onClick={onNext}
          aria-label="下一页"
          className="flex h-7 w-7 items-center justify-center rounded-md transition-colors duration-150 hover:bg-gray-200 disabled:opacity-30"
        >
          <ChevronRight size={14} strokeWidth={1.5} />
        </button>
      </span>
    </div>
  );
}
