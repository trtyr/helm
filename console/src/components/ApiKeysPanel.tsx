import { useMemo, useState } from "react";
import { copyText } from "../lib/clipboard";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, Plus, Trash2 } from "lucide-react";
import type { components } from "../api/schema";
import { api } from "../api/client";
import { toast } from "../lib/toast";

type ApiKey = components["schemas"]["ApiKey"];
type ApiKeyScope = components["schemas"]["ApiKeyScope"];

/** scope 中文说明（与 server application::scopes 对应）。 */
const SCOPES: { id: ApiKeyScope; label: string }[] = [
  { id: "hosts", label: "主机 / Agent 档案" },
  { id: "exec", label: "命令执行 / 终端" },
  { id: "files", label: "文件传输" },
  { id: "services", label: "服务管理" },
  { id: "processes", label: "进程 / 网络" },
  { id: "metrics", label: "指标 / 告警" },
  { id: "notifications", label: "通知" },
  { id: "listeners", label: "监听器" },
  { id: "forward", label: "正向连接" },
  { id: "proxy", label: "SOCKS 代理" },
  { id: "ir", label: "应急响应 IR（Windows）" },
  { id: "agent-gen", label: "Agent 生成" },
  { id: "audit", label: "审计" },
  { id: "skill", label: "技能包" },
];

function scopeLabel(id: string): string {
  return SCOPES.find((s) => s.id === id)?.label ?? id;
}

/** /settings 的「API 凭证」块：scope 化 key 的创建 / 列表 / 吊销。
 *  明文 key 仅创建响应出现一次；api-keys 端点仅 JWT 可调（后端强制）。 */
export function ApiKeysPanel() {
  const queryClient = useQueryClient();
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [newExpires, setNewExpires] = useState("");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [plaintext, setPlaintext] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<ApiKey | null>(null);

  const keysQuery = useQuery({
    queryKey: ["api-keys"],
    queryFn: () => api<{ api_keys: ApiKey[] }>("/api/v1/api-keys?page=1&limit=100"),
  });

  const createMutation = useMutation({
    mutationFn: (body: { name: string; expires_at?: string; scopes: string[] }) =>
      api<{ key: string }>("/api/v1/api-keys", { method: "POST", body }),
    onSuccess: (r) => {
      setPlaintext(r.key);
      setCreating(false);
      setNewName("");
      setNewExpires("");
      setPicked(new Set());
      setError(null);
      queryClient.invalidateQueries({ queryKey: ["api-keys"] });
    },
    onError: (e) => setError((e as Error).message),
  });

  const revokeMutation = useMutation({
    mutationFn: (id: string) => api(`/api/v1/api-keys/${id}`, { method: "DELETE" }),
    onSuccess: () => {
      toast("凭证已吊销", "success");
      setDeleting(null);
      queryClient.invalidateQueries({ queryKey: ["api-keys"] });
    },
    onError: (e) => toast((e as Error).message, "error"),
  });

  const rows = useMemo(() => keysQuery.data?.api_keys ?? [], [keysQuery.data]);
  const togglePick = (id: string) => {
    setPicked((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  };

  return (
    <section className="rounded-lg border border-gray-400 p-6">
      <div className="flex items-baseline justify-between">
        <h2 className="text-heading-16">API 凭证</h2>
        <span className="text-label-12 text-gray-900">按功能域签发 · 供 AI / 脚本使用</span>
      </div>
      <p className="mt-2 text-label-12 text-gray-900">
        空勾选 = 全功能。MCP 接入方式见 docs/mcp.md；凭证泄漏请立即吊销。
      </p>

      <div className="mt-3 flex justify-end">
        <button
          type="button"
          onClick={() => setCreating(true)}
          className="flex h-8 items-center gap-1.5 rounded-md bg-gray-700 px-3 text-label-13 transition-colors duration-150 hover:bg-gray-800"
        >
          <Plus size={14} strokeWidth={1.5} />
          签发凭证
        </button>
      </div>

      <table className="mt-3 w-full text-left">
        <thead>
          <tr className="border-b border-gray-400 text-label-13 text-gray-900">
            <th className="px-2 py-2 font-normal">名称</th>
            <th className="px-2 py-2 font-normal">前缀</th>
            <th className="px-2 py-2 font-normal">Scopes</th>
            <th className="px-2 py-2 font-normal">最近使用</th>
            <th className="px-2 py-2 font-normal">状态</th>
            <th className="px-2 py-2 text-right font-normal">操作</th>
          </tr>
        </thead>
        <tbody>
          {keysQuery.isPending ? (
            <tr>
              <td colSpan={6} className="px-2 py-6 text-center text-label-13 text-gray-900">
                加载中…
              </td>
            </tr>
          ) : rows.length === 0 ? (
            <tr>
              <td colSpan={6} className="px-2 py-6 text-center text-label-13 text-gray-900">
                还没有凭证
              </td>
            </tr>
          ) : (
            rows.map((k) => (
              <tr key={k.id} className="border-b border-gray-400/60 last:border-0">
                <td className="px-2 py-2 text-label-13">{k.name}</td>
                <td className="px-2 py-2 font-mono text-label-12 text-gray-900">{k.prefix}…</td>
                <td className="max-w-56 px-2 py-2">
                  {k.scopes?.length ? (
                    <span
                      className="block truncate text-label-12 text-gray-900"
                      title={k.scopes.map(scopeLabel).join("、")}
                    >
                      {k.scopes.map(scopeLabel).join("、")}
                    </span>
                  ) : (
                    <span className="text-label-12 text-gray-900">全功能</span>
                  )}
                </td>
                <td className="px-2 py-2 text-label-12 text-gray-900">
                  {k.last_used_at ? new Date(k.last_used_at).toLocaleString() : "从未"}
                </td>
                <td className="px-2 py-2">
                  {k.revoked_at ? (
                    <span className="text-label-12 text-red-1000">已吊销</span>
                  ) : k.expires_at && new Date(k.expires_at) < new Date() ? (
                    <span className="text-label-12 text-amber-1000">已过期</span>
                  ) : (
                    <span className="text-label-12 text-green-1000">有效</span>
                  )}
                </td>
                <td className="px-2 py-2 text-right">
                  {!k.revoked_at && (
                    <button
                      type="button"
                      aria-label={`吊销 ${k.name}`}
                      title="吊销"
                      onClick={() => setDeleting(k)}
                      className="inline-flex h-7 w-7 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-red-1000"
                    >
                      <Trash2 size={14} strokeWidth={1.5} />
                    </button>
                  )}
                </td>
              </tr>
            ))
          )}
        </tbody>
      </table>

      {/* 创建对话框 */}
      {creating && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <button
            type="button"
            aria-label="关闭"
            onClick={() => setCreating(false)}
            className="absolute inset-0 bg-black/40"
          />
          <div className="relative z-10 flex max-h-[80vh] w-[480px] flex-col gap-3 overflow-y-auto rounded-xl border border-gray-400 bg-background-100 p-6">
            <h3 className="text-heading-16">签发 API 凭证</h3>
            <input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="名称（如 ai-ops / ci-bot）"
              className="h-8 rounded-md border border-gray-400 bg-gray-100 px-2 text-label-13 outline-none hover:border-gray-500"
            />
            <input
              value={newExpires}
              onChange={(e) => setNewExpires(e.target.value)}
              placeholder="过期时间 RFC 3339（可选，如 2026-12-31T23:59:59Z）"
              className="h-8 rounded-md border border-gray-400 bg-gray-100 px-2 font-mono text-label-13 outline-none hover:border-gray-500"
            />
            <div className="rounded-lg border border-gray-400 p-3">
              <p className="text-label-13 text-gray-1000">Scopes（全不选 = 全功能）</p>
              <div className="mt-2 grid grid-cols-2 gap-1.5">
                {SCOPES.map((s) => (
                  <label
                    key={s.id}
                    className="flex cursor-pointer items-center gap-2 rounded px-1 py-0.5 text-label-12 hover:bg-gray-100"
                  >
                    <input
                      type="checkbox"
                      checked={picked.has(s.id)}
                      onChange={() => togglePick(s.id)}
                      className="h-3.5 w-3.5 accent-blue-1000"
                    />
                    <span className="font-mono">{s.id}</span>
                    <span className="text-gray-900">{s.label}</span>
                  </label>
                ))}
              </div>
            </div>
            {error && <p className="text-label-12 text-red-1000">{error}</p>}
            <div className="flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setCreating(false)}
                className="h-8 rounded-md border border-gray-500 px-4 text-label-13 hover:bg-gray-200"
              >
                取消
              </button>
              <button
                type="button"
                disabled={!newName.trim() || createMutation.isPending}
                onClick={() =>
                  createMutation.mutate({
                    name: newName.trim(),
                    expires_at: newExpires.trim() || undefined,
                    scopes: [...picked],
                  })
                }
                className="h-8 rounded-md bg-gray-700 px-4 text-label-13 text-white hover:bg-gray-800 disabled:opacity-40"
              >
                签发
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 明文一次性展示 */}
      {plaintext && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="absolute inset-0 bg-black/40" />
          <div className="relative z-10 w-[520px] rounded-xl border border-gray-400 bg-background-100 p-6">
            <h3 className="text-heading-16">凭证已签发</h3>
            <p className="mt-2 text-label-13 text-red-1000">
              明文仅此一次显示，关闭后无法再查看——请立即复制保存。
            </p>
            <div className="mt-3 flex items-center gap-2 rounded-md border border-gray-400 bg-gray-100 p-3">
              <code className="flex-1 break-all font-mono text-label-13">{plaintext}</code>
              <button
                type="button"
                aria-label="复制凭证"
                title="复制"
                onClick={() => {
                  copyText(plaintext).then(
                    (ok) => toast(ok ? "已复制" : "复制失败，请手动选择复制", ok ? "success" : "warn"),
                  );
                }}
                className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-gray-400 hover:bg-gray-200"
              >
                <Copy size={14} strokeWidth={1.5} />
              </button>
            </div>
            <div className="mt-4 flex justify-end">
              <button
                type="button"
                onClick={() => setPlaintext(null)}
                className="h-8 rounded-md bg-gray-700 px-4 text-label-13 text-white hover:bg-gray-800"
              >
                我已保存
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 吊销确认 */}
      {deleting && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <button
            type="button"
            aria-label="取消"
            onClick={() => setDeleting(null)}
            className="absolute inset-0 bg-black/40"
          />
          <div className="relative z-10 w-[420px] rounded-xl border border-gray-400 bg-background-100 p-6">
            <h3 className="text-heading-16">吊销凭证「{deleting.name}」？</h3>
            <p className="mt-2 text-label-13 text-gray-900">
              使用该凭证的 AI / 脚本将立即失去访问能力。此操作幂等，吊销后不可恢复。
            </p>
            <div className="mt-4 flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setDeleting(null)}
                className="h-8 rounded-md border border-gray-500 px-4 text-label-13 hover:bg-gray-200"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => revokeMutation.mutate(deleting.id)}
                className="h-8 rounded-md bg-red-1000 px-4 text-label-13 text-white hover:opacity-90"
              >
                吊销
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
