import { Fragment, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, ChevronRight as ChevronExpand } from "lucide-react";
import type { components } from "../api/schema";
import { api } from "../api/client";
import { auditResource } from "../lib/audit";
import { SkeletonRows } from "../components/ui";
import { SortableTh } from "../components/tableControls";

type Audit = components["schemas"]["Audit"];

/** /audit 审计日志（规格 audit.md F62/F63：列表 + 行展开 JSON 树）。 */
export default function Audit() {
  const [params, setParams] = useSearchParams();
  const page = Math.max(1, Number(params.get("page") ?? 1));
  const limit = 20;
  const [expanded, setExpanded] = useState<string | null>(null);

  // 列表基座（P001-T1c）：排序走服务端（sort 参数），状态存 URL params
  const sortField = params.get("sort") ?? "";
  const sortDesc = sortField.endsWith(":desc");
  const sortKey = sortField.replace(":desc", "");
  const sort: { key: string; desc: boolean } | null = sortKey ? { key: sortKey, desc: sortDesc } : null;
  const toggleSort = (key: string) => {
    const next = new URLSearchParams(params);
    if (sort?.key === key) {
      if (sort.desc) next.delete("sort");
      else next.set("sort", `${key}:desc`);
    } else next.set("sort", key);
    setParams(next, { replace: true });
    setExpanded(null); // 展开状态跨排序不保留
  };

  const auditQuery = useQuery({
    queryKey: ["audit", page, sortField],
    queryFn: async () => {
      const sp = new URLSearchParams({ page: String(page), limit: String(limit) });
      if (sortField) sp.set("sort", sortField);
      const r = await api<{ audit: Audit[] }>(`/api/v1/audit?${sp}`);
      return r.audit ?? [];
    },
  });
  const rows = auditQuery.data ?? [];

  function goPage(p: number) {
    const next = new URLSearchParams(params);
    next.set("page", String(p));
    setParams(next, { replace: true });
    setExpanded(null); // 展开状态跨分页不保留（规格）
  }

  return (
    <div className="flex flex-col gap-5">
      <p className="-mt-1 text-copy-13 text-gray-900">关键操作的历史记录（登录 / 执行 / 文件 / 主机 / 监听器）</p>

      {/* 列表卡 */}
      <div className="overflow-hidden rounded-lg border border-gray-400 bg-background-100">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <th className="w-10 px-3 py-2.5" />
              <SortableTh className="px-4 py-2.5" label="时间" sortKey="created_at" sort={sort} onSort={toggleSort} />
              <SortableTh className="px-4 py-2.5" label="操作者" sortKey="actor" sort={sort} onSort={toggleSort} />
              <SortableTh className="px-4 py-2.5" label="动作" sortKey="action" sort={sort} onSort={toggleSort} />
              <SortableTh className="px-4 py-2.5" label="资源" sortKey="resource" sort={sort} onSort={toggleSort} />
            </tr>
          </thead>
          <tbody>
            {auditQuery.isPending ? (
              <SkeletonRows rows={8} cols={5} />
            ) : auditQuery.isError ? (
              <tr>
                <td colSpan={5} className="px-4 py-12 text-center">
                  <span className="text-label-13 text-red-1000">
                    查询失败：{(auditQuery.error as Error).message}
                  </span>
                  <button
                    type="button"
                    onClick={() => auditQuery.refetch()}
                    className="ml-3 text-label-13 text-blue-1000 hover:underline"
                  >
                    重试
                  </button>
                </td>
              </tr>
            ) : rows.length === 0 ? (
              <tr>
                <td colSpan={5} className="px-4 py-12 text-center text-label-13 text-gray-900">
                  暂无审计记录
                </td>
              </tr>
            ) : (
              rows.map((row) => {
                const detail = row.detail ?? {};
                const hasDetail = Object.keys(detail).length > 0;
                const open = expanded === row.id;
                return (
                  <Fragment key={row.id}>
                    <tr
                      onClick={() => setExpanded(open ? null : row.id ?? null)}
                      className={`cursor-pointer border-b border-gray-400/60 transition-colors duration-150 hover:bg-gray-100 ${
                        open ? "bg-gray-100" : ""
                      }`}
                    >
                      <td className="px-3 py-3 text-center text-gray-900">
                        {hasDetail && (
                          <ChevronExpand
                            size={12}
                            strokeWidth={1.5}
                            className={`mx-auto transition-transform duration-150 ${open ? "rotate-90" : ""}`}
                          />
                        )}
                      </td>
                      <td
                        className="px-4 py-3 font-mono text-label-13 text-gray-900"
                        title={row.created_at ?? ""}
                      >
                        {row.created_at ? row.created_at.replace("T", " ").slice(0, 19) : "—"}
                      </td>
                      <td className="px-4 py-3 text-label-13">{row.actor}</td>
                      <td className="px-4 py-3">
                        <span className="rounded bg-gray-200 px-1.5 py-0.5 font-mono text-label-12 text-gray-1000">
                          {row.action}
                        </span>
                      </td>
                      <td className="px-4 py-3 font-mono text-label-13 text-gray-900">
                        {auditResource(detail)}
                      </td>
                    </tr>
                    {open && (
                      <tr className="border-b border-gray-400/60 bg-gray-100/50">
                        <td colSpan={5} className="px-10 py-3">
                          {hasDetail ? (
                            <JsonTree value={detail} depth={0} />
                          ) : (
                            <p className="text-label-12 text-gray-900">（无附加详情）</p>
                          )}
                        </td>
                      </tr>
                    )}
                  </Fragment>
                );
              })
            )}
          </tbody>
        </table>

        {/* 底栏分页 */}
        <div className="flex h-12 items-center justify-between border-t border-gray-400 px-4 text-label-13 text-gray-900">
          <span>{rows.length > 0 ? `第 ${page} 页 · 本页 ${rows.length} 条` : "无记录"}</span>
          <span className="flex items-center gap-1">
            <button
              type="button"
              disabled={page <= 1}
              onClick={() => goPage(page - 1)}
              aria-label="上一页"
              className="flex h-7 w-7 items-center justify-center rounded-md transition-colors duration-150 hover:bg-gray-200 disabled:opacity-30"
            >
              <ChevronLeft size={14} strokeWidth={1.5} />
            </button>
            <span className="font-mono">{page}</span>
            <button
              type="button"
              disabled={rows.length < limit}
              onClick={() => goPage(page + 1)}
              aria-label="下一页"
              className="flex h-7 w-7 items-center justify-center rounded-md transition-colors duration-150 hover:bg-gray-200 disabled:opacity-30"
            >
              <ChevronRight size={14} strokeWidth={1.5} />
            </button>
          </span>
        </div>
      </div>
    </div>
  );
}

/** detail JSON 树（规格：键 label-13 gray-900、值 mono、长值折叠）。 */
function JsonTree({ value, depth }: { value: unknown; depth: number }) {
  if (value === null || typeof value !== "object") {
    return <ValueText value={value} />;
  }
  const entries = Object.entries(value as Record<string, unknown>);
  return (
    <div className="flex flex-col gap-1" style={{ paddingLeft: depth > 0 ? 16 : 0 }}>
      {entries.map(([k, v]) => (
        <div key={k} className="flex gap-2 text-label-13">
          <span className="shrink-0 text-gray-900">{k}:</span>
          <span className="min-w-0">
            {v !== null && typeof v === "object" ? (
              <JsonTree value={v} depth={depth + 1} />
            ) : (
              <ValueText value={v} />
            )}
          </span>
        </div>
      ))}
    </div>
  );
}

function ValueText({ value }: { value: unknown }) {
  const text = value === null ? "null" : String(value);
  const [expandedValue, setExpandedValue] = useState(false);
  const long = text.length > 80;
  return (
    <span
      className={`font-mono text-label-13 text-gray-1000 ${long ? "cursor-pointer hover:underline" : ""}`}
      onClick={long ? () => setExpandedValue((v) => !v) : undefined}
      title={long ? "点击展开全值" : undefined}
    >
      {long && !expandedValue ? `${text.slice(0, 80)}…` : text}
    </span>
  );
}
