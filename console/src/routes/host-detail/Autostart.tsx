import { useEffect, useMemo, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "../../lib/toast";
import type { components } from "../../api/schema";
import { api, pickAgent } from "../../api/client";

type Agent = components["schemas"]["Agent"];
type IrFinding = components["schemas"]["IrFinding"];

interface Ctx {
  host: components["schemas"]["HostView"];
}

interface DiffState {
  added: IrFinding[];
  removed: IrFinding[];
  baseLabel: string;
}

const SEV_CLS: Record<string, string> = {
  critical: "bg-red-1000/10 text-red-1000",
  warn: "bg-amber-1000/10 text-amber-1000",
  info: "bg-gray-200 text-gray-900",
};
const SEV_LABEL: Record<string, string> = { critical: "严重", warn: "可疑", info: "信息" };
const SEV_ORDER = (s: string) => (s === "critical" ? 0 : s === "warn" ? 1 : 2);

/** Autoruns 分类标签（顺序即展示顺序）。 */
const CATEGORIES = [
  "登录",
  "服务",
  "驱动",
  "计划任务",
  "WMI 订阅",
  "浏览器",
  "外壳",
  "映像劫持",
  "认证",
  "引导执行",
  "已知 DLL",
  "Winsock",
  "Office",
  "编解码器",
] as const;

const SIGN_CLS: Record<string, string> = {
  verified: "bg-green-1000/10 text-green-1000",
  unsigned: "bg-amber-1000/10 text-amber-1000",
  invalid: "bg-red-1000/10 text-red-1000",
  unknown: "bg-gray-200 text-gray-900",
};
const SIGN_LABEL: Record<string, string> = {
  verified: "已验证",
  unsigned: "未签名",
  invalid: "无效",
  unknown: "未知",
};

function fmtTime(unix?: string | null): string {
  if (!unix) return "—";
  const n = Number(unix);
  if (!Number.isFinite(n) || n <= 0) return "—";
  const d = new Date(n * 1000);
  const pad = (x: number) => String(x).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** 自启动项持久化全景——对标 Sysinternals Autoruns：
 *  分类标签 + 文件厂商/签名校验 + 禁用/启用/删除（AutorunsDisabled 机制）+
 *  基线快照对比。 */
export default function Autostart() {
  const { host } = useOutletContext<Ctx>();
  const queryClient = useQueryClient();
  const [cat, setCat] = useState<string>("全部");
  const [keyword, setKeyword] = useState("");
  const [hideMicrosoft, setHideMicrosoft] = useState(false);
  const [diff, setDiff] = useState<DiffState | null>(null);

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = pickAgent(agentsQuery.data?.agents ?? [], host.id);

  // 页面缓存：秒开最后一次扫描结果；重新扫描按钮才真正触发 agent 重扫
  const cacheQuery = useQuery({
    queryKey: ["ir-autostart-cache", agent?.id],
    queryFn: () =>
      api<{
        findings: IrFinding[] | null;
        entryCount?: number;
        createdAt?: string;
      }>(`/api/v1/ir/cache?agent_id=${agent?.id}&types=autostart,registry`),
    enabled: !!agent,
    staleTime: 30_000,
  });

  const scanMutation = useMutation({
    mutationFn: () =>
      api<{ findings: IrFinding[]; error?: string | null }>("/api/v1/ir/scan", {
        method: "POST",
        body: { agent_id: agent?.id, types: ["autostart", "registry"] },
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["ir-autostart-cache", agent?.id] }),
    onError: (e: Error) => toast(e.message, "error"),
  });

  // 无缓存时自动扫一次（首次使用）
  useEffect(() => {
    if (agent && host.online && cacheQuery.data && cacheQuery.data.findings === null && !scanMutation.isPending) {
      scanMutation.mutate();
    }
  }, [agent, host.online, cacheQuery.data]);

  // 基线快照列表
  const snapshotsQuery = useQuery({
    queryKey: ["ir-snapshots", agent?.id],
    queryFn: () =>
      api<{
        snapshots: {
          id: string;
          label: string;
          entry_count: number;
          created_at: string;
        }[];
      }>(`/api/v1/ir/snapshots?agent_id=${agent?.id}`),
    enabled: !!agent,
  });
  const [baseId, setBaseId] = useState("");

  // ---- 操作（禁用/启用/删除） ----
  const actionMutation = useMutation({
    mutationFn: (v: { action: string; key: string }) =>
      api<{ ok: boolean; error?: string | null }>("/api/v1/ir/autorun-action", {
        method: "POST",
        body: { agent_id: agent?.id, action: v.action, key: v.key },
      }),
    onSuccess: (r, v) => {
      if (r.ok) {
        toast(`${v.action === "disable" ? "已禁用" : v.action === "enable" ? "已启用" : "已删除"}，正在重新扫描`, "success");
        scanMutation.mutate();
      } else {
        toast(r.error ?? "操作失败", "error");
      }
    },
    onError: (e: Error) => toast(e.message, "error"),
  });

  const runAction = (action: "disable" | "enable" | "delete", f: IrFinding) => {
    const name = f.name ?? "";
    const confirmText =
      action === "delete"
        ? `确认删除自启动项「${name}」？此操作不可恢复（删除后只能重新添加）。`
        : action === "disable"
          ? `确认禁用自启动项「${name}」？（条目将被移入 AutorunsDisabled 禁用区，可随时恢复）`
          : `确认启用自启动项「${name}」？`;
    if (!window.confirm(confirmText)) return;
    actionMutation.mutate({ action, key: f.opKey ?? "" });
  };

  // ---- 基线对比 ----
  const saveBaseline = useMutation({
    mutationFn: () =>
      api<{ id: string; entry_count: number }>("/api/v1/ir/snapshots", {
        method: "POST",
        body: { agent_id: agent?.id, label: new Date().toLocaleString() },
      }),
    onSuccess: (r) => {
      toast(`基线已保存（${r.entry_count} 条）`, "success");
      queryClient.invalidateQueries({ queryKey: ["ir-snapshots", agent?.id] });
    },
    onError: (e: Error) => toast(e.message, "error"),
  });

  const compare = useMutation({
    mutationFn: (id: string) =>
      api<{ added: IrFinding[]; removed: IrFinding[] }>("/api/v1/ir/snapshots/compare", {
        method: "POST",
        body: { base_id: id, agent_id: agent?.id },
      }),
    onSuccess: (r) => {
      setDiff({ added: r.added ?? [], removed: r.removed ?? [], baseLabel: "基线" });
      toast(`对比完成：新增 ${r.added?.length ?? 0} · 移除 ${r.removed?.length ?? 0}`, "success");
    },
    onError: (e: Error) => toast(e.message, "error"),
  });


  // ---- 过滤与排序 ----
  const findings = useMemo(
    () => scanMutation.data?.findings ?? cacheQuery.data?.findings ?? [],
    [scanMutation.data, cacheQuery.data],
  );
  const scanning = scanMutation.isPending;
  const cacheTime = scanMutation.data ? null : cacheQuery.data?.createdAt ?? null;

  const catCounts = useMemo(() => {
    const m = new Map<string, number>();
    for (const f of findings) m.set(f.category ?? "", (m.get(f.category ?? "") ?? 0) + 1);
    return m;
  }, [findings]);

  const filtered = useMemo(() => {
    let list = diff ? [...diff.added, ...diff.removed] : findings;
    if (!diff) {
      if (cat !== "全部") list = list.filter((f) => f.category === cat);
      if (hideMicrosoft) {
        list = list.filter(
          (f) =>
            !(f.signState === "verified" && (f.publisher ?? "").toLowerCase().includes("microsoft")),
        );
      }
    }
    const kw = keyword.trim().toLowerCase();
    if (kw) {
      list = list.filter((f) =>
        [f.name, f.detail, f.path, f.publisher, f.desc].some((s) =>
          (s ?? "").toLowerCase().includes(kw),
        ),
      );
    }
    return [...list].sort((a, b) => {
      const sev = SEV_ORDER(a.severity ?? "info") - SEV_ORDER(b.severity ?? "info");
      if (sev !== 0) return sev;
      return (
        (a.category ?? "").localeCompare(b.category ?? "") ||
        (a.name ?? "").localeCompare(b.name ?? "")
      );
    });
  }, [findings, cat, keyword, hideMicrosoft, diff]);

  const criticalCount = findings.filter((f) => f.severity === "critical").length;
  const isDiffRow = (f: IrFinding) =>
    diff ? ((diff.added.includes(f) ? "added" : "removed") as "added" | "removed") : undefined;

  return (
    <div className="flex flex-col gap-3">
      {/* 分类标签栏（Autoruns tabs） */}
      {!diff && (
        <div className="flex flex-wrap items-center gap-1.5">
          <Pill label="全部" count={findings.length} active={cat === "全部"} onClick={() => setCat("全部")} />
          {CATEGORIES.filter((c) => (catCounts.get(c) ?? 0) > 0).map((c) => (
            <Pill key={c} label={c} count={catCounts.get(c) ?? 0} active={cat === c} onClick={() => setCat(c)} />
          ))}
        </div>
      )}

      {/* 工具行 */}
      <div className="flex flex-wrap items-center gap-2">
        <input
          value={keyword}
          onChange={(e) => setKeyword(e.target.value)}
          placeholder="搜索条目 / 路径 / 厂商…"
          className="h-8 w-60 rounded-md border border-gray-500 bg-background-100 px-2.5 text-label-13 outline-none placeholder:text-gray-900/40 focus:border-gray-900"
        />
        {!diff && (
          <label className="flex cursor-pointer select-none items-center gap-1.5 text-label-13 text-gray-900">
            <input
              type="checkbox"
              checked={hideMicrosoft}
              onChange={(e) => setHideMicrosoft(e.target.checked)}
              className="h-3.5 w-3.5 accent-blue-1000"
            />
            隐藏微软签名条目
          </label>
        )}
        {criticalCount > 0 && !diff && (
          <span className="whitespace-nowrap rounded bg-red-1000/10 px-1.5 py-0.5 text-label-12 text-red-1000">
            {criticalCount} 条严重发现
          </span>
        )}
        {cacheTime && !scanning && (
          <span className="text-label-12 text-gray-900/50" title="上次扫描时间（缓存）">
            缓存于 {new Date(cacheTime).toLocaleString()}
          </span>
        )}
        <button
          type="button"
          onClick={() => scanMutation.mutate()}
          disabled={!host.online || scanMutation.isPending}
          className="ml-auto h-8 rounded-md border border-gray-500 px-3 text-label-13 hover:bg-gray-200 disabled:opacity-40"
        >
          {scanMutation.isPending ? "扫描中…" : "重新扫描"}
        </button>
      </div>

      {/* 基线对比工具行 */}
      <div className="flex flex-wrap items-center gap-2 rounded-lg border border-gray-400 bg-background-100 px-3 py-2">
        <span className="text-label-13 font-medium text-gray-900">基线对比</span>
        <select
          value={baseId}
          onChange={(e) => setBaseId(e.target.value)}
          className="h-8 max-w-72 rounded-md border border-gray-500 bg-background-100 px-2 text-label-13 outline-none focus:border-gray-900"
        >
          <option value="">选择基线快照…</option>
          {(snapshotsQuery.data?.snapshots ?? []).map((s) => (
            <option key={s.id} value={s.id}>
              {s.label}（{s.entry_count} 条）
            </option>
          ))}
        </select>
        <button
          type="button"
          disabled={!baseId || !host.online || compare.isPending}
          onClick={() => compare.mutate(baseId)}
          className="h-8 rounded-md border border-gray-500 px-3 text-label-13 hover:bg-gray-200 disabled:opacity-40"
        >
          {compare.isPending ? "对比中…" : diff ? "重新对比" : "对比当前"}
        </button>
        <button
          type="button"
          disabled={!host.online || saveBaseline.isPending}
          onClick={() => {
            if (window.confirm("把当前扫描结果保存为新的基线快照？")) saveBaseline.mutate();
          }}
          className="h-8 rounded-md border border-gray-500 px-3 text-label-13 hover:bg-gray-200 disabled:opacity-40"
        >
          {saveBaseline.isPending ? "保存中…" : "保存当前为基线"}
        </button>
        {diff && (
          <>
            <span className="rounded bg-green-1000/10 px-1.5 py-0.5 text-label-12 text-green-1000">
              新增 {diff.added.length}
            </span>
            <span className="rounded bg-red-1000/10 px-1.5 py-0.5 text-label-12 text-red-1000">
              移除 {diff.removed.length}
            </span>
            <button
              type="button"
              onClick={() => setDiff(null)}
              className="ml-auto text-label-13 text-blue-1000 hover:underline"
            >
              退出对比
            </button>
          </>
        )}
      </div>

      <div className="overflow-hidden rounded-lg border border-gray-400 bg-background-100">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <th className="w-16 whitespace-nowrap px-3 py-2.5 font-normal">级别</th>
              <th className="w-52 whitespace-nowrap px-3 py-2.5 font-normal">条目</th>
              <th className="w-40 whitespace-nowrap px-3 py-2.5 font-normal">发布者</th>
              <th className="w-20 whitespace-nowrap px-3 py-2.5 font-normal">签名</th>
              <th className="w-28 whitespace-nowrap px-3 py-2.5 font-normal">文件时间</th>
              <th className="w-full max-w-0 px-4 py-2.5 font-normal">路径</th>
              <th className="w-24 whitespace-nowrap px-3 py-2.5 font-normal">操作</th>
            </tr>
          </thead>
          <tbody>
            {scanning && findings.length === 0 && !diff ? (
              <tr><td colSpan={7} className="px-4 py-10 text-center text-label-13 text-gray-900">扫描中…（全景扫描含逐文件签名校验，首次可能需要十几秒）</td></tr>
            ) : scanMutation.isError && findings.length === 0 && !diff ? (
              <tr><td colSpan={7} className="px-4 py-10 text-center">
                <span className="text-label-13 text-red-1000">{(scanMutation.error as Error).message}</span>
                <button onClick={() => scanMutation.mutate()} className="ml-3 text-label-13 text-blue-1000 hover:underline">重试</button>
              </td></tr>
            ) : filtered.length === 0 ? (
              <tr><td colSpan={7} className="px-4 py-10 text-center text-label-13 text-gray-900">无条目</td></tr>
            ) : (
              filtered.map((f, i) => {
                const ds = isDiffRow(f);
                return (
                  <tr
                    key={`${f.category}-${f.name}-${i}`}
                    title={f.detail}
                    className={`border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100 ${
                      ds === "added"
                        ? "bg-green-1000/5"
                        : ds === "removed"
                          ? "bg-red-1000/5"
                          : f.disabled
                            ? "opacity-50"
                            : ""
                    }`}
                  >
                    <td className="px-3 py-2">
                      <span className={`whitespace-nowrap rounded px-1.5 py-0.5 text-label-12 ${SEV_CLS[f.severity ?? "info"] ?? SEV_CLS.info}`}>
                        {SEV_LABEL[f.severity ?? "info"] ?? "信息"}
                      </span>
                    </td>
                    <td className="px-3 py-2">
                      <div className="truncate text-label-13 text-gray-900" title={f.name}>
                        {ds === "added" && <span className="mr-1 text-green-1000">▲</span>}
                        {ds === "removed" && <span className="mr-1 text-red-1000">▼</span>}
                        {f.name}
                      </div>
                      {(f.desc || f.disabled) && (
                        <div className="truncate text-label-12 text-gray-900/60">
                          {f.disabled && <span className="mr-1 text-amber-1000">[已禁用]</span>}
                          <span title={f.desc ?? ""}>{f.desc}</span>
                        </div>
                      )}
                    </td>
                    <td className="px-3 py-2">
                      <span className="block truncate text-label-12 text-gray-900" title={f.publisher ?? ""}>
                        {f.publisher ?? "—"}
                      </span>
                    </td>
                    <td className="px-3 py-2">
                      {f.signState ? (
                        <span className={`whitespace-nowrap rounded px-1.5 py-0.5 text-label-12 ${SIGN_CLS[f.signState] ?? SIGN_CLS.unknown}`}>
                          {SIGN_LABEL[f.signState] ?? f.signState}
                        </span>
                      ) : (
                        <span className="text-label-12 text-gray-900/40">—</span>
                      )}
                    </td>
                    <td className="whitespace-nowrap px-3 py-2 text-label-12 text-gray-900">
                      {fmtTime(f.mtime)}
                    </td>
                    <td className="max-w-0 px-4 py-2">
                      <span className="block truncate font-mono text-label-12 text-gray-900" title={f.path ?? ""}>
                        {f.path ?? "—"}
                      </span>
                    </td>
                    <td className="whitespace-nowrap px-3 py-2">
                      <div className="flex items-center gap-1.5">
                        {f.opKey && (
                          <>
                            <button
                              type="button"
                              title={f.disabled ? "启用（移出 AutorunsDisabled）" : "禁用（移入 AutorunsDisabled）"}
                              disabled={actionMutation.isPending}
                              onClick={() => runAction(f.disabled ? "enable" : "disable", f)}
                              className="rounded border border-gray-500 px-1.5 py-0.5 text-label-12 text-gray-900 hover:bg-gray-200 disabled:opacity-40"
                            >
                              {f.disabled ? "启用" : "禁用"}
                            </button>
                            <button
                              type="button"
                              title="删除该自启动项"
                              disabled={actionMutation.isPending}
                              onClick={() => runAction("delete", f)}
                              className="rounded border border-red-1000/40 px-1.5 py-0.5 text-label-12 text-red-1000 hover:bg-red-1000/10 disabled:opacity-40"
                            >
                              删除
                            </button>
                          </>
                        )}
                      </div>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
          {diff
            ? `对比模式：新增 ${diff.added.length} · 移除 ${diff.removed.length}（对照全部 ${findings.length} 条）`
            : `显示 ${filtered.length} / 共 ${findings.length} 条 · critical ${criticalCount} · warn ${findings.filter((f) => f.severity === "warn").length}`}
        </div>
      </div>
    </div>
  );
}

function Pill({ label, count, active, onClick }: { label: string; count: number; active: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex h-7 items-center gap-1 rounded-full border px-2.5 text-label-13 transition-colors ${
        active
          ? "border-gray-900 bg-gray-900 text-white"
          : "border-gray-500 text-gray-900 hover:bg-gray-200"
      }`}
    >
      {label}
      <span className={`font-mono text-label-12 ${active ? "text-white/70" : "text-gray-900/50"}`}>{count}</span>
    </button>
  );
}
