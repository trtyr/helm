// 进程页的视图部件（G13 拆分，2026-09-21）——自 `Processes.tsx` 拆出：
// 负载卡片、进程行（平铺/树共用）、详情抽屉、结束确认模态。全部为无状态展示组件。
import { ChevronDown, ChevronRight, X } from "lucide-react";
import type { components } from "../../../api/schema";
import { formatDateTime, formatUptime } from "../../../lib/format";
import { humanSize } from "../../../lib/paths";
import { suspiciousPair } from "./shared";

type ProcessInfo = components["schemas"]["ProcessInfo"];

export function LoadCard({
  label,
  value,
  percent,
  over,
  sub,
}: {
  label: string;
  value: string;
  percent: number;
  over: boolean;
  sub?: string;
}) {
  return (
    <div className="rounded-lg border border-gray-400 bg-background-100 p-4">
      <div className="flex items-baseline justify-between">
        <span className="text-label-13 text-gray-900">{label}</span>
        <span className={`font-mono text-label-16 tabular-nums ${over ? "text-amber-1000" : ""}`}>{value}</span>
      </div>
      {percent > 0 && (
        <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-gray-200">
          <div
            className={`h-full rounded-full transition-all duration-300 ${over ? "bg-amber-1000" : "bg-blue-1000"}`}
            style={{ width: `${percent}%` }}
          />
        </div>
      )}
      {sub && <p className="mt-1.5 text-label-12 text-gray-900">{sub}</p>}
    </div>
  );
}

/** 单行（平铺/树共用）：树模式带缩进、折叠箭头与异常父子标记。 */
export function ProcessRow({
  p,
  offline,
  suspicious,
  indent = 0,
  expandable = false,
  expanded = true,
  onToggle,
  onClick,
  onKill,
}: {
  p: ProcessInfo;
  offline: boolean;
  suspicious?: string | null;
  indent?: number;
  expandable?: boolean;
  expanded?: boolean;
  onToggle?: () => void;
  onClick?: () => void;
  onKill?: () => void;
}) {
  const cpu = p.cpu_percent ?? 0;
  const mem = p.mem_bytes ?? 0;
  return (
    <tr
      onClick={onClick}
      className={`group cursor-pointer border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100 ${
        cpu > 50 ? "bg-amber-1000/5" : ""
      }`}
      title={suspicious ?? undefined}
    >
      <td className="whitespace-nowrap px-3 py-2 font-mono text-label-13 text-gray-900 tabular-nums">{p.pid}</td>
      <td className="max-w-0 px-3 py-2">
        <div className="flex items-center gap-1" style={{ paddingLeft: indent * 18 }}>
          {expandable ? (
            <button
              type="button"
              aria-label={expanded ? "折叠" : "展开"}
              onClick={(e) => {
                e.stopPropagation();
                onToggle?.();
              }}
              className="shrink-0 rounded p-0.5 text-gray-900 hover:bg-gray-200"
            >
              {expanded ? <ChevronDown size={13} strokeWidth={1.5} /> : <ChevronRight size={13} strokeWidth={1.5} />}
            </button>
          ) : (
            <span className="w-[19px] shrink-0" />
          )}
          {suspicious && (
            <span className="shrink-0 rounded bg-red-1000/10 px-1 py-px text-label-12 font-medium text-red-1000" title={suspicious}>
              ⚠
            </span>
          )}
          <span className="block truncate text-label-14" title={p.cmd || p.name}>
            {p.name}
          </span>
        </div>
      </td>
      <td className="whitespace-nowrap px-3 py-2 text-right font-mono text-label-13 tabular-nums">
        <span className={cpu > 50 ? "font-semibold text-amber-1000" : cpu > 5 ? "text-gray-1000" : "text-gray-900"}>
          {cpu.toFixed(1)}
        </span>
      </td>
      <td className="whitespace-nowrap px-3 py-2 text-right font-mono text-label-13 tabular-nums">
        <span className={mem > 500 * 1024 * 1024 ? "text-amber-1000" : "text-gray-1000"}>
          {humanSize(mem)}
        </span>
      </td>
      <td className="whitespace-nowrap px-3 py-2 text-right font-mono text-label-13 text-gray-900 tabular-nums">
        {p.virt_mem_bytes ? humanSize(p.virt_mem_bytes) : "—"}
      </td>
      <td className="whitespace-nowrap px-3 py-2 text-label-12 text-gray-900">{p.status || "—"}</td>
      <td className="max-w-32 truncate whitespace-nowrap px-3 py-2 text-label-13 text-gray-900" title={p.user}>
        {p.user || "—"}
      </td>
      <td className="whitespace-nowrap px-3 py-2 text-right font-mono text-label-13 text-gray-900 tabular-nums">
        {p.start_time_unix ? formatUptime(p.start_time_unix, Date.now() / 1000) : "—"}
      </td>
      <td className="whitespace-nowrap px-3 py-2 text-right">
        <button
          type="button"
          aria-label={`结束 ${p.name ?? p.pid}`}
          disabled={offline}
          onClick={(e) => {
            e.stopPropagation();
            onKill?.();
          }}
          className="rounded border border-gray-400 px-1.5 py-0.5 text-label-12 text-gray-900 opacity-0 transition-all duration-150 hover:border-red-1000 hover:text-red-1000 group-hover:opacity-100 disabled:opacity-30"
        >
          结束
        </button>
      </td>
    </tr>
  );
}

/** 单进程详情（只读属性表 + 终止操作 + 父进程名称）。 */
export function ProcessDetail({
  p,
  parentName,
  killing,
  onClose,
  onKill,
}: {
  p: ProcessInfo;
  parentName: string;
  killing: boolean;
  onClose: () => void;
  onKill: () => void;
}) {
  const parentLabel = parentName ? `${p.parent_pid ?? "—"}（${parentName}）` : String(p.parent_pid ?? "—");
  const rule = suspiciousPair(parentName, p.name ?? "");
  const rows: [string, string][] = [
    ["PID", String(p.pid ?? "—")],
    ["名称", p.name ?? "—"],
    ["状态", p.status || "—"],
    ["用户", p.user || "—"],
    ["父进程", parentLabel],
    ["CPU", `${(p.cpu_percent ?? 0).toFixed(1)}%`],
    ["内存", humanSize(p.mem_bytes ?? 0)],
    ["虚拟内存", p.virt_mem_bytes ? humanSize(p.virt_mem_bytes) : "—"],
    ["已运行", p.start_time_unix ? formatUptime(p.start_time_unix) : "—"],
    ["启动时间", p.start_time_unix ? formatDateTime(new Date(p.start_time_unix * 1000).toISOString()) : "—"],
  ];
  return (
    <div className="fixed inset-0 z-40 flex justify-end">
      <button type="button" aria-label="关闭" onClick={onClose} className="absolute inset-0 bg-black/40" />
      <div className="relative z-10 flex h-full w-[440px] flex-col gap-5 overflow-y-auto border-l border-gray-400 bg-background-100 p-6">
        <div className="flex items-center justify-between">
          <h2 className="text-heading-20">进程详情</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="关闭"
            className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200"
          >
            <X size={14} strokeWidth={1.5} />
          </button>
        </div>
        {rule && (
          <p className="rounded-md bg-red-1000/10 px-3 py-2 text-label-13 text-red-1000">⚠ 异常父子关系：{rule}</p>
        )}
        <dl className="flex flex-col">
          {rows.map(([k, v]) => (
            <div
              key={k}
              className="flex items-start justify-between gap-4 border-b border-gray-400/60 py-2 last:border-0"
            >
              <dt className="shrink-0 text-label-13 text-gray-900">{k}</dt>
              <dd className="text-right font-mono text-label-13">{v}</dd>
            </div>
          ))}
        </dl>
        {(p.exe_path || p.cmd) && (
          <div>
            <p className="text-label-13 text-gray-900">可执行文件 / 命令行</p>
            {p.exe_path && (
              <p className="mt-1 break-all rounded-md border border-gray-400 bg-gray-100 p-2 font-mono text-label-12">
                {p.exe_path}
              </p>
            )}
            {p.cmd && (
              <p className="mt-2 break-all rounded-md border border-gray-400 bg-gray-100 p-2 font-mono text-label-12">
                {p.cmd}
              </p>
            )}
          </div>
        )}
        <button
          type="button"
          onClick={onKill}
          disabled={killing}
          className="mt-auto h-9 rounded-md border border-red-1000 text-label-14 text-red-1000 transition-colors duration-150 hover:bg-red-1000 hover:text-background-100 disabled:opacity-50"
        >
          {killing ? "结束中…" : "结束此进程"}
        </button>
      </div>
    </div>
  );
}

/** 结束进程确认模态（二次确认 + 错误回显）。 */
export function KillProcessDialog({
  p,
  pending,
  error,
  onCancel,
  onConfirm,
}: {
  p: ProcessInfo;
  pending: boolean;
  error?: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <button type="button" aria-label="关闭" onClick={onCancel} className="absolute inset-0 bg-black/40" />
      <div className="relative z-10 w-[400px] rounded-xl border border-gray-400 bg-background-100 p-6">
        <h2 className="text-heading-16">结束进程</h2>
        <p className="mt-4 font-mono text-label-14">
          PID {p.pid} · {p.name}
        </p>
        <p className="mt-4 text-label-13 text-red-1000">该操作不可恢复。</p>
        {error && (
          <p className="mt-3 text-label-13 text-red-1000" role="alert">
            ⚠ {error}
          </p>
        )}
        <div className="mt-6 flex justify-end gap-3">
          <button
            type="button"
            onClick={onCancel}
            disabled={pending}
            className="h-8 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200"
          >
            取消
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={pending}
            className="h-8 rounded-md border border-red-1000 px-4 text-label-14 text-red-1000 transition-colors duration-150 hover:bg-red-1000 hover:text-background-100 disabled:opacity-50"
          >
            {pending ? "结束中…" : "结束进程"}
          </button>
        </div>
      </div>
    </div>
  );
}
