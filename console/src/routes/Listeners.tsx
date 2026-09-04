import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { MoreHorizontal, Play, Square } from "lucide-react";
import type { components } from "../api/schema";
import { api } from "../api/client";
import { toast } from "../lib/toast";
import { relativeTime } from "../lib/format";

type Listener = components["schemas"]["Listener"];

/** /listeners 监听器管理（规格 listeners.md F64–F68：卡片网格 + 启停 + CRUD）。 */
export default function Listeners() {
  const queryClient = useQueryClient();
  const [menuFor, setMenuFor] = useState<string | null>(null);
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
    <div className="flex flex-col gap-6">
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

      {listQuery.isPending ? (
        <div className="grid grid-cols-1 gap-6 xl:grid-cols-2">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="h-40 animate-pulse rounded-lg border border-gray-400 bg-gray-100" />
          ))}
        </div>
      ) : listQuery.isError ? (
        <div className="rounded-lg border border-gray-400 p-8 text-center">
          <p className="text-label-13 text-red-1000">查询失败：{(listQuery.error as Error).message}</p>
          <button
            type="button"
            onClick={() => listQuery.refetch()}
            className="mt-3 h-8 rounded-md border border-gray-500 px-4 text-label-13 transition-colors duration-150 hover:bg-gray-200"
          >
            重试
          </button>
        </div>
      ) : listeners.length === 0 ? (
        <div className="rounded-lg border border-dashed border-gray-500 p-12 text-center">
          <p className="text-label-13 text-gray-900">没有监听器——Server 启动时默认 seed 一个，或手动创建</p>
          <button
            type="button"
            onClick={openCreate}
            className="mt-4 h-8 rounded-md border border-gray-500 px-4 text-label-13 transition-colors duration-150 hover:bg-gray-200"
          >
            创建监听器
          </button>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-6 xl:grid-cols-2">
          {listeners.map((l) => {
            const running = l.status === "running";
            return (
              <div key={l.id} className="relative rounded-lg border border-gray-400 p-6">
                <div className="flex items-center gap-2">
                  <span
                    className={`h-2 w-2 rounded-full ${running ? "bg-green-1000 animate-breath" : "border border-gray-600"}`}
                  />
                  <span className={`text-label-13 ${running ? "text-green-1000" : "text-gray-900"}`}>
                    {running ? "running" : "stopped"}
                  </span>
                  <span className="ml-auto font-mono text-label-12 text-gray-900">{l.proto}</span>
                  <button
                    type="button"
                    aria-label={`更多 ${l.name}`}
                    onClick={() => setMenuFor(menuFor === l.id ? null : l.id!)}
                    className="flex h-7 w-7 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000"
                  >
                    <MoreHorizontal size={14} strokeWidth={1.5} />
                  </button>
                  {menuFor === l.id && (
                    <>
                      <button
                        type="button"
                        aria-label="关闭菜单"
                        onClick={() => setMenuFor(null)}
                        className="fixed inset-0 z-30 cursor-default"
                      />
                      <div className="absolute right-6 top-12 z-40 w-28 rounded-xl border border-gray-400 bg-background-100 py-1 shadow-lg">
                        <button
                          type="button"
                          onClick={() => {
                            setMenuFor(null);
                            openEdit(l);
                          }}
                          className="block w-full px-3 py-2 text-left text-label-13 transition-colors duration-150 hover:bg-gray-200"
                        >
                          编辑
                        </button>
                        <button
                          type="button"
                          onClick={() => {
                            setMenuFor(null);
                            setDeleteTarget(l);
                          }}
                          className="block w-full px-3 py-2 text-left text-label-13 text-red-1000 transition-colors duration-150 hover:bg-gray-200"
                        >
                          删除
                        </button>
                      </div>
                    </>
                  )}
                </div>
                <h3 className="mt-3 text-heading-16">{l.name}</h3>
                <button
                  type="button"
                  onClick={() => {
                    navigator.clipboard.writeText(l.addr ?? "");
                    toast("已复制地址");
                  }}
                  title={`${(l.addr ?? "").split(":")[0]}\n端口 ${(l.addr ?? "").split(":")[1] ?? ""}（点击复制）`}
                  className="mt-1 block font-mono text-label-13 text-blue-1000 hover:underline"
                >
                  grpc://{l.addr}
                </button>
                <p className="mt-3 text-label-12 text-gray-900">
                  专用 token：留空则使用全局 token（出于安全不回显）
                </p>
                <p className="mt-2 font-mono text-label-12 text-gray-900">
                  创建 {relativeTime(l.created_at, listQuery.data?.at)}
                  {running && ` · 运行中`}
                </p>
                <div className="mt-4">
                  {running ? (
                    <button
                      type="button"
                      onClick={() => opMutation.mutate({ id: l.id!, op: "stop" })}
                      disabled={opMutation.isPending}
                      className="flex h-8 w-24 items-center justify-center gap-1.5 rounded-md border border-gray-500 text-label-13 transition-colors duration-150 hover:bg-gray-200 disabled:opacity-40"
                    >
                      {opMutation.isPending ? (
                        <span className="h-3.5 w-3.5 animate-spin rounded-full border border-gray-900 border-t-gray-1000" />
                      ) : (
                        <Square size={13} strokeWidth={1.5} />
                      )}
                      停止
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => opMutation.mutate({ id: l.id!, op: "start" })}
                      disabled={opMutation.isPending}
                      className="flex h-8 w-24 items-center justify-center gap-1.5 rounded-md border border-gray-500 text-label-13 transition-colors duration-150 hover:bg-gray-200 disabled:opacity-40"
                    >
                      {opMutation.isPending ? (
                        <span className="h-3.5 w-3.5 animate-spin rounded-full border border-gray-900 border-t-gray-1000" />
                      ) : (
                        <Play size={13} strokeWidth={1.5} />
                      )}
                      启动
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}

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
