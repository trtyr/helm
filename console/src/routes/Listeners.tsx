import { useState } from "react";
import { copyText } from "../lib/clipboard";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Play, Square, Trash2 } from "lucide-react";
import type { components } from "../api/schema";
import { api } from "../api/client";
import { toast } from "../lib/toast";
import { relativeTime } from "../lib/format";
import { SkeletonRows, StatusDot } from "../components/ui";

type Listener = components["schemas"]["Listener"];

/** /listeners 监听器管理（规格 listeners.md F64–F68：列表形式 + 启停 + CRUD）。 */
export default function Listeners() {
  const queryClient = useQueryClient();
  const [formNonce, setFormNonce] = useState(0);
  const [editTarget, setEditTarget] = useState<Listener | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Listener | null>(null);

  const listQuery = useQuery({
    queryKey: ["listeners"],
    queryFn: async () => {
      const r = await api<{ listeners: Listener[] }>("/api/v1/listeners");
      return { at: Date.now(), listeners: r.listeners ?? [] };
    },
    refetchInterval: 30_000,
  });
  const listeners = listQuery.data?.listeners ?? [];

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ["listeners"] });

  const opMutation = useMutation({
    mutationFn: ({ id, op }: { id: string; op: "start" | "stop" }) =>
      api(`/api/v1/listeners/${id}/${op}`, { method: "POST" }),
    onSuccess: () => invalidate(),
    onError: (e) => toast((e as Error).message, "error"),
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api(`/api/v1/listeners/${id}`, { method: "DELETE" }),
    onSuccess: () => {
      setDeleteTarget(null);
      toast("已删除");
      invalidate();
    },
    onError: (e) => toast((e as Error).message, "error"),
  });

  function openCreate() {
    setEditTarget(null);
    setFormNonce((n) => n + 1);
  }
  function openEdit(l: Listener) {
    setEditTarget(l);
    setFormNonce((n) => n + 1);
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="text-heading-24">监听器</h1>
          <p className="mt-1 text-copy-13 text-gray-900">Agent 接入的 gRPC 监听端点</p>
        </div>
        <button
          type="button"
          onClick={openCreate}
          className="h-8 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200"
        >
          + 创建监听器
        </button>
      </div>

      <div className="overflow-hidden rounded-lg border border-gray-400 bg-background-100">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <th className="w-14 whitespace-nowrap px-3 py-2.5 font-normal">状态</th>
              <th className="w-px whitespace-nowrap px-3 py-2.5 font-normal">名称</th>
              <th className="w-full max-w-0 px-4 py-2.5 font-normal">地址</th>
              <th className="w-px whitespace-nowrap px-3 py-2.5 font-normal">协议</th>
              <th className="w-px whitespace-nowrap px-3 py-2.5 font-normal">创建时间</th>
              <th className="w-px whitespace-nowrap px-3 py-2.5 text-right font-normal">操作</th>
            </tr>
          </thead>
          <tbody>
            {listQuery.isPending ? (
              <SkeletonRows rows={3} cols={6} />
            ) : listQuery.isError ? (
              <tr>
                <td colSpan={6} className="px-4 py-10 text-center">
                  <span className="text-label-13 text-red-1000">
                    查询失败：{(listQuery.error as Error).message}
                  </span>
                  <button
                    type="button"
                    onClick={() => listQuery.refetch()}
                    className="ml-3 text-label-13 text-blue-1000 hover:underline"
                  >
                    重试
                  </button>
                </td>
              </tr>
            ) : listeners.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-4 py-12 text-center text-label-13 text-gray-900">
                  没有监听器——Server 启动时默认 seed 一个，或手动创建
                </td>
              </tr>
            ) : (
              listeners.map((l) => {
                const running = l.status === "running";
                return (
                  <tr
                    key={l.id}
                    className="group border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
                  >
                    <td className="px-3 py-2.5">
                      <StatusDot online={running} />
                    </td>
                    <td className="whitespace-nowrap px-3 py-2.5 text-label-14">
                      {l.name}
                      {running && (
                        <span className="ml-2 text-label-12 text-green-1000">运行中</span>
                      )}
                    </td>
                    <td className="max-w-0 px-4 py-2.5">
                      <button
                        type="button"
                        onClick={() => {
                          copyText(l.addr ?? "").then((ok) => toast(ok ? "已复制地址" : "复制失败", ok ? undefined : "warn"));
                        }}
                        title={`${l.addr}（点击复制）`}
                        className="block w-full truncate text-left font-mono text-label-13 text-blue-1000 hover:underline"
                      >
                        grpc://{l.addr}
                      </button>
                    </td>
                    <td className="whitespace-nowrap px-3 py-2.5 font-mono text-label-13 text-gray-900">
                      {l.proto}
                    </td>
                    <td className="whitespace-nowrap px-3 py-2.5 font-mono text-label-13 text-gray-900">
                      {relativeTime(l.created_at, listQuery.data?.at)}
                    </td>
                    <td className="whitespace-nowrap px-3 py-2.5 text-right">
                      <span className="inline-flex items-center gap-1">
                        {running ? (
                          <OpBtn
                            label={`停止 ${l.name}`}
                            disabled={opMutation.isPending}
                            onClick={() => opMutation.mutate({ id: l.id!, op: "stop" })}
                          >
                            <Square size={13} strokeWidth={1.5} />
                          </OpBtn>
                        ) : (
                          <OpBtn
                            label={`启动 ${l.name}`}
                            disabled={opMutation.isPending}
                            onClick={() => opMutation.mutate({ id: l.id!, op: "start" })}
                          >
                            <Play size={13} strokeWidth={1.5} />
                          </OpBtn>
                        )}
                        <OpBtn label={`编辑 ${l.name}`} onClick={() => openEdit(l)}>
                          <Pencil size={13} strokeWidth={1.5} />
                        </OpBtn>
                        <OpBtn label={`删除 ${l.name}`} onClick={() => setDeleteTarget(l)}>
                          <Trash2 size={13} strokeWidth={1.5} />
                        </OpBtn>
                      </span>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center justify-between border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
          <span>
            {listeners.length > 0
              ? `共 ${listeners.length} 个 · ${listeners.filter((l) => l.status === "running").length} 运行中`
              : "无监听器"}
          </span>
          <span className="text-label-12">专用 token 出于安全不回显；留空则使用全局 token</span>
        </div>
      </div>

      {formNonce > 0 && (
        <ListenerFormModal
          key={formNonce}
          target={editTarget}
          onClose={() => setFormNonce(0)}
          onSaved={() => {
            setFormNonce(0);
            invalidate();
          }}
        />
      )}

      {deleteTarget && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <button
            type="button"
            aria-label="关闭"
            onClick={() => setDeleteTarget(null)}
            className="absolute inset-0 bg-black/40"
          />
          <div className="relative z-10 w-[400px] rounded-xl border border-gray-400 bg-background-100 p-6">
            <h2 className="text-heading-16">删除监听器</h2>
            <p className="mt-4 text-label-14">
              确认删除 <span className="font-mono">{deleteTarget.name}</span>？
            </p>
            {deleteTarget.status === "running" && (
              <p className="mt-3 text-label-13 text-red-1000">
                停止后删除将断开其上所有 Agent 连接。
              </p>
            )}
            <div className="mt-6 flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setDeleteTarget(null)}
                disabled={deleteMutation.isPending}
                className="h-8 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => deleteMutation.mutate(deleteTarget.id!)}
                disabled={deleteMutation.isPending}
                className="h-8 rounded-md border border-red-1000 px-4 text-label-14 text-red-1000 transition-colors duration-150 hover:bg-red-1000 hover:text-background-100 disabled:opacity-50"
              >
                {deleteMutation.isPending ? "删除中…" : "删除"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function OpBtn({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="flex h-7 w-7 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000 disabled:opacity-30"
    >
      {children}
    </button>
  );
}

/** 创建/编辑模态（360px：名称/地址/协议禁用/专用 token 可选）。 */
function ListenerFormModal({
  target,
  onClose,
  onSaved,
}: {
  target: Listener | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const editing = !!target;
  const [name, setName] = useState(target?.name ?? "");
  const [addr, setAddr] = useState(target?.addr ?? "");
  const [auth, setAuth] = useState("");

  const mutation = useMutation({
    mutationFn: () => {
      if (editing) {
        return api(`/api/v1/listeners/${target!.id}`, {
          method: "PUT",
          body: { name: name.trim(), addr: addr.trim(), proto: "grpc" },
        });
      }
      return api("/api/v1/listeners", {
        method: "POST",
        body: {
          name: name.trim(),
          addr: addr.trim(),
          proto: "grpc",
          ...(auth.trim() ? { auth: auth.trim() } : {}),
        },
      });
    },
    onSuccess: onSaved,
    onError: (e) => toast((e as Error).message, "error"),
  });

  const valid = name.trim().length > 0 && /^\S+:\d+$/.test(addr.trim());

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <button type="button" aria-label="关闭" onClick={onClose} className="absolute inset-0 bg-black/40" />
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (valid) mutation.mutate();
        }}
        className="relative z-10 w-[360px] rounded-xl border border-gray-400 bg-background-100 p-6"
      >
        <h2 className="text-heading-16">{editing ? "编辑监听器" : "创建监听器"}</h2>

        <label htmlFor="ln-name" className="mt-4 block text-label-14">
          名称
        </label>
        <input
          id="ln-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="内网备用"
          autoFocus
          className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
        />

        <label htmlFor="ln-addr" className="mt-4 block text-label-14">
          地址（ip:port）
        </label>
        <input
          id="ln-addr"
          value={addr}
          onChange={(e) => setAddr(e.target.value)}
          placeholder="0.0.0.0:50054"
          className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
        />

        <label htmlFor="ln-proto" className="mt-4 block text-label-14">
          协议
        </label>
        <input
          id="ln-proto"
          value="grpc"
          disabled
          className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 text-gray-900 opacity-60"
        />

        {!editing && (
          <>
            <label htmlFor="ln-auth" className="mt-4 block text-label-14">
              专用 token（可选）
            </label>
            <input
              id="ln-auth"
              type="password"
              value={auth}
              onChange={(e) => setAuth(e.target.value)}
              placeholder="留空则使用全局 HELM_SERVER_TOKEN"
              className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
          </>
        )}

        {mutation.isError && (
          <p className="mt-3 text-label-13 text-red-1000" role="alert">
            ⚠ {(mutation.error as Error).message}
          </p>
        )}

        <div className="mt-6 flex justify-end gap-3">
          <button
            type="button"
            onClick={onClose}
            disabled={mutation.isPending}
            className="h-8 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200"
          >
            取消
          </button>
          <button
            type="submit"
            disabled={!valid || mutation.isPending}
            className="h-8 rounded-md bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-50"
          >
            {mutation.isPending ? "保存中…" : editing ? "保存" : "创建"}
          </button>
        </div>
      </form>
    </div>
  );
}
