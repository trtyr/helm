// 进程页工具行（G13 拆分，2026-09-21）——自 `Processes.tsx` 拆出：
// 搜索、状态筛选、树/平铺切换、展开/折叠、快照时间、刷新、自动刷新间隔。
import { RefreshCw, Search, X } from "lucide-react";
import { INTERVALS, type IntervalOpt } from "./shared";

export function ProcessToolbar({
  search,
  onSearch,
  statusOptions,
  statusFilter,
  onStatusFilter,
  view,
  onView,
  isTree,
  suspiciousCount,
  onExpandAll,
  onCollapseAll,
  snapshotAt,
  isFetching,
  onRefresh,
  offline,
  interval,
  onInterval,
}: {
  search: string;
  onSearch: (v: string) => void;
  statusOptions: { value: string; label: string }[];
  statusFilter: string;
  onStatusFilter: (v: string) => void;
  view: "flat" | "tree";
  onView: (v: "flat" | "tree") => void;
  isTree: boolean;
  suspiciousCount: number;
  onExpandAll: () => void;
  onCollapseAll: () => void;
  snapshotAt?: number;
  isFetching: boolean;
  onRefresh: () => void;
  offline: boolean;
  interval: IntervalOpt;
  onInterval: (v: IntervalOpt) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2">
      <div className="relative">
        <Search size={13} strokeWidth={1.5} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-gray-900" />
        <input
          value={search}
          onChange={(e) => onSearch(e.target.value)}
          placeholder="搜索名称 / PID / 用户"
          aria-label="搜索进程"
          className="h-8 w-56 rounded-md border border-gray-400 bg-gray-100 pl-8 pr-7 text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
        />
        {search && (
          <button
            type="button"
            aria-label="清除搜索"
            onClick={() => onSearch("")}
            className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-900 hover:text-gray-1000"
          >
            <X size={12} strokeWidth={1.5} />
          </button>
        )}
      </div>
      <select
        aria-label="按状态筛选"
        value={statusFilter}
        onChange={(e) => onStatusFilter(e.target.value)}
        className="h-8 rounded-md border border-gray-400 bg-gray-100 px-2 text-label-13 outline-none transition-colors duration-150 hover:border-gray-500"
      >
        <option value="">状态：全部</option>
        {statusOptions.map((o) => (
          <option key={o.value} value={o.value}>{o.value}</option>
        ))}
      </select>
      {/* 树形/平铺切换 */}
      <div className="flex h-8 items-center overflow-hidden rounded-md border border-gray-400">
        {(["tree", "flat"] as const).map((v) => (
          <button
            key={v}
            type="button"
            onClick={() => onView(v)}
            className={`h-full px-3 text-label-13 transition-colors duration-150 ${
              view === v ? "bg-gray-700 text-white" : "bg-gray-100 text-gray-900 hover:bg-gray-200"
            }`}
          >
            {v === "tree" ? "进程树" : "平铺"}
          </button>
        ))}
      </div>
      {isTree && (
        <>
          {suspiciousCount > 0 && (
            <span
              className="whitespace-nowrap rounded bg-red-1000/10 px-1.5 py-0.5 text-label-12 text-red-1000"
              title="父子关系命中应急检测规则（办公/浏览器派生解释器、LSASS 派生等）"
            >
              ⚠ 异常父子 {suspiciousCount}
            </span>
          )}
          <button
            type="button"
            onClick={onExpandAll}
            className="h-8 rounded-md border border-gray-400 px-2 text-label-12 text-gray-900 hover:bg-gray-200"
          >
            全部展开
          </button>
          <button
            type="button"
            onClick={onCollapseAll}
            className="h-8 rounded-md border border-gray-400 px-2 text-label-12 text-gray-900 hover:bg-gray-200"
          >
            全部折叠
          </button>
        </>
      )}
      <span className="flex-1" />
      <span className="font-mono text-label-12 text-gray-900">
        {snapshotAt ? `快照 ${new Date(snapshotAt).toLocaleTimeString()}` : ""}
      </span>
      <button
        type="button"
        aria-label="刷新"
        disabled={offline}
        onClick={onRefresh}
        className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000 disabled:opacity-30"
      >
        <RefreshCw size={14} strokeWidth={1.5} className={isFetching ? "animate-spin" : ""} />
      </button>
      <select
        aria-label="自动刷新间隔"
        value={interval}
        onChange={(e) => onInterval(Number(e.target.value) as IntervalOpt)}
        className="h-8 rounded-md border border-gray-400 bg-gray-100 px-2 font-mono text-label-13 outline-none transition-colors duration-150 hover:border-gray-500"
      >
        {INTERVALS.map((i) => (
          <option key={i.value} value={i.value}>
            ⏱ {i.label}
          </option>
        ))}
      </select>
    </div>
  );
}
