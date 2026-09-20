import { useQuery } from "@tanstack/react-query";

import { api } from "../api/client";

/**
 * 统一筛选栏（P003 T7）：时间范围档位 + 主机下拉，任务/审计/状态事件三子页复用。
 * 关键字与各页特有筛选（状态/动作/类型）留在页面内（与 TableToolbar 等已有 UI 协作）。
 * 状态经 URL params 持久化（range/from/to/host_id），页面侧 patchParams 驱动。
 */

export interface FilterValue {
  range: string;
  from: string;
  to: string;
  host_id: string;
}

interface FilterBarProps {
  value: FilterValue;
  /** 页面侧的 URL 参数补丁（null 删除键；page 重置由页面统一处理）。 */
  onPatch: (patch: Record<string, string | null>) => void;
  /** 隐藏主机下拉（审计日志无 host 维度）。 */
  hideHost?: boolean;
}

const MINUTES: Record<string, number> = { "1h": 60, "24h": 1440, "7d": 10080 };

export function FilterBar({ value, onPatch, hideHost = false }: FilterBarProps) {
  const hostsQ = useQuery({
    queryKey: ["hosts-for-filterbar"],
    queryFn: () => api<{ hosts: { id: string; hostname: string }[] }>("/api/v1/hosts?limit=200"),
    staleTime: 60_000,
    enabled: !hideHost,
  });

  /** 档位 change：预设档计算 from/to；custom 清空时间由用户手输；空档清除全部。 */
  const applyRange = (r: string) => {
    const mins = MINUTES[r];
    if (mins) {
      onPatch({
        range: r,
        from: new Date(Date.now() - mins * 60_000).toISOString(),
        to: new Date().toISOString(),
      });
    } else if (r === "custom") {
      onPatch({ range: r });
    } else {
      onPatch({ range: null, from: null, to: null });
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-2">
      <select
        value={value.range}
        onChange={(e) => applyRange(e.target.value)}
        aria-label="按时间范围筛选"
        className="h-7 rounded-md border border-gray-400 px-2 text-label-13"
      >
        <option value="">时间：全部</option>
        <option value="1h">近 1 小时</option>
        <option value="24h">近 24 小时</option>
        <option value="7d">近 7 天</option>
        <option value="custom">自定义…</option>
      </select>
      {value.range === "custom" && (
        <>
          <input
            value={value.from}
            onChange={(e) => onPatch({ from: e.target.value || null })}
            placeholder="起始 RFC3339"
            className="h-7 w-56 rounded-md border border-gray-400 px-2 font-mono text-label-12"
          />
          <input
            value={value.to}
            onChange={(e) => onPatch({ to: e.target.value || null })}
            placeholder="结束 RFC3339"
            className="h-7 w-56 rounded-md border border-gray-400 px-2 font-mono text-label-12"
          />
        </>
      )}
      {!hideHost && (
        <select
          value={value.host_id}
          onChange={(e) => onPatch({ host_id: e.target.value || null })}
          aria-label="按主机筛选"
          className="h-7 max-w-52 rounded-md border border-gray-400 px-2 text-label-13"
        >
          <option value="">主机：全部</option>
          {(hostsQ.data?.hosts ?? []).map((h) => (
            <option key={h.id} value={h.id}>
              {h.hostname}
            </option>
          ))}
        </select>
      )}
    </div>
  );
}
