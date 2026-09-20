import { useState } from "react";
import { Link } from "react-router-dom";
import { ChevronRight, Copy, KeyRound, PlugZap } from "lucide-react";
import { copyText } from "../lib/clipboard";
import { SCOPES } from "../lib/scopes";
import { MCP_CATALOG, MCP_OP_COUNT, type McpDomainGroup, type McpOp } from "../lib/mcpCatalog";
import { toast } from "../lib/toast";

/** MCP 端点页：接入地址、工具目录（领域 → 工具 → 参数/JSON 示例）、连接测试、scope 说明。 */

const apiBase = import.meta.env.VITE_API_BASE || location.origin;
const ENDPOINT = `${apiBase}/mcp`;

interface TestResult {
  ok: boolean;
  protocolVersion?: string;
  opCount?: number;
  ops?: string[];
  error?: string;
}

/** 连接测试：用给定 key 走 initialize + tools/list，展示该 key 可见的 op 目录。 */
async function probe(key: string): Promise<TestResult> {
  // 直连真实 MCP 端点 /mcp（与端点段宣传的接入路径一致）。
  const call = async (method: string, params?: object, id = 1) => {
    const resp = await fetch(`${apiBase}/mcp`, {
      method: "POST",
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${key}` },
      body: JSON.stringify({ jsonrpc: "2.0", id, method, params }),
    });
    if (resp.status === 401) return { ok: false, error: "凭证无效或已吊销（401）" };
    if (!resp.ok) return { ok: false, error: `HTTP ${resp.status}` };
    const body = await resp.json();
    if (body.error) return { ok: false, error: body.error.message ?? JSON.stringify(body.error) };
    return body.result;
  };
  const init = await call("initialize", { protocolVersion: "2025-06-18" });
  if (init.error) return init;
  const list = await call("tools/list", {}, 2);
  if (list.error) return list;
  const tool = list.tools?.[0];
  if (!tool) return { ok: false, error: "服务端未返回工具" };
  const ops = [...tool.description.matchAll(/^(\S+) — /gm)].map((m) => m[1]);
  return { ok: true, protocolVersion: init.protocolVersion, opCount: ops.length, ops };
}

/** 单个工具的可展开行：点击展开完整描述、参数表与调用 JSON 示例。 */
function OpRow({ op }: { op: McpOp }) {
  const [open, setOpen] = useState(false);
  const argsExample: Record<string, string> = {};
  for (const [k] of op.params) {
    argsExample[k] = k === "page" ? "1" : k === "limit" ? "20" : `<${k}>`;
  }
  const example = JSON.stringify(
    { op: op.name, args: Object.keys(argsExample).length > 0 ? argsExample : {} },
    null,
    2,
  );
  return (
    <div className="border-b border-gray-400/60 last:border-0">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-label-13 transition-colors duration-150 hover:bg-gray-200"
      >
        <ChevronRight
          size={12}
          strokeWidth={1.5}
          className={`shrink-0 transition-transform duration-150 ${open ? "rotate-90" : ""}`}
        />
        <span className="shrink-0 font-mono">{op.name}</span>
        {op.os === "Windows" && (
          <span className="shrink-0 rounded border border-gray-400 px-1 text-label-12 text-gray-900">Windows</span>
        )}
        <span className="ml-auto min-w-0 truncate text-gray-900">{op.summary}</span>
      </button>
      {open && (
        <div className="flex flex-col gap-2 px-6 pb-3">
          <div className="flex flex-wrap items-center gap-2 text-label-12 text-gray-900">
            <span className="rounded border border-gray-400 px-1 font-mono">{op.method}</span>
            <span className="font-mono">{op.path}</span>
            <span className="ml-auto font-mono">scope: {op.scope}</span>
          </div>
          {op.params.length > 0 && (
            <div className="flex flex-col gap-0.5">
              <p className="text-label-12 text-gray-900">参数</p>
              {op.params.map(([k, v]) => (
                <div key={k} className="flex gap-2 text-label-12">
                  <code className="w-32 shrink-0 font-mono text-gray-1000">{k}</code>
                  <span className="text-gray-900">{v}</span>
                </div>
              ))}
            </div>
          )}
          <div className="relative">
            <pre className="overflow-x-auto rounded-md bg-[#0d0d0d] px-3 py-2 font-mono text-label-12 text-gray-100">
              {example}
            </pre>
            <button
              type="button"
              aria-label={`复制 ${op.name} 示例`}
              onClick={() => copyText(example).then((ok) => toast(ok ? "已复制" : "复制失败", ok ? undefined : "warn"))}
              className="absolute right-2 top-2 text-gray-900 transition-colors duration-150 hover:text-gray-1000"
            >
              <Copy size={12} strokeWidth={1.5} />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/** 领域分组可展开块：域标题 → 该域工具列表。 */
function DomainGroup({ group }: { group: McpDomainGroup }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="rounded-lg border border-gray-400">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="flex w-full items-center gap-2 px-4 py-3 text-left transition-colors duration-150 hover:bg-gray-200/60"
      >
        <ChevronRight
          size={14}
          strokeWidth={1.5}
          className={`shrink-0 transition-transform duration-150 ${open ? "rotate-90" : ""}`}
        />
        <span className="text-label-14">{group.domain}</span>
        <span className="ml-auto font-mono text-label-12 text-gray-900">{group.ops.length} op</span>
      </button>
      {open && (
        <div className="border-t border-gray-400 px-2 py-1">
          {group.ops.map((op) => (
            <OpRow key={op.name} op={op} />
          ))}
        </div>
      )}
    </div>
  );
}

export default function Mcp() {
  const [testKey, setTestKey] = useState("");
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<TestResult | null>(null);
  const runTest = async () => {
    setTesting(true);
    setResult(null);
    try {
      setResult(await probe(testKey.trim()));
    } catch (e) {
      setResult({ ok: false, error: (e as Error).message });
    } finally {
      setTesting(false);
    }
  };

  const copy = (text: string, okMsg: string) =>
    copyText(text).then((ok) => toast(ok ? okMsg : "复制失败", ok ? undefined : "warn"));

  return (
    <div className="flex flex-col gap-6">
      <div>
        <h1 className="text-heading-24">MCP</h1>
        <p className="mt-1 text-copy-13 text-gray-900">
          把平台交给 AI——任意 MCP 客户端经此端点操作平台；可见能力由凭证 scope 决定
        </p>
      </div>

      {/* 端点 */}
      <section className="rounded-lg border border-gray-400 p-6">
        <div className="flex items-baseline justify-between">
          <h2 className="text-heading-16">端点</h2>
          <span className="text-label-12 text-gray-900">Streamable HTTP · JSON-RPC 2.0</span>
        </div>
        <div className="mt-3 flex items-center gap-2 rounded-md border border-gray-400 bg-gray-100 p-3">
          <code className="flex-1 break-all font-mono text-label-13">{ENDPOINT}</code>
          <button
            type="button"
            aria-label="复制端点"
            title="复制"
            onClick={() => copy(ENDPOINT, "已复制端点")}
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-gray-400 hover:bg-gray-200"
          >
            <Copy size={14} strokeWidth={1.5} />
          </button>
        </div>
        <p className="mt-2 text-label-12 text-gray-900">
          鉴权为 Bearer API key（即「设置 → API 凭证」签发的凭证）；
          key 的 scopes 决定 AI 可见的操作目录——最小授权按需签发。
        </p>
      </section>

      {/* 工具目录（roadmap T9：领域 → 工具 → 参数描述/JSON 示例，可展开） */}
      <section className="rounded-lg border border-gray-400 p-6">
        <div className="flex items-baseline justify-between">
          <h2 className="text-heading-16">工具目录</h2>
          <span className="text-label-12 text-gray-900">
            {MCP_CATALOG.length} 域 · {MCP_OP_COUNT} op
          </span>
        </div>
        <p className="mt-2 text-label-12 text-gray-600">
          暴露分层 HELM_MCP_TIER：1=入口（清单不展开）/ 2=host 域（对主机做的一切，默认）/
          3=全量（含平台管理）。当前生效 tier 可调 catalog 操作查看（响应含 tier 字段）。
        </p>
        <div className="mt-3 flex flex-col gap-2">
          {MCP_CATALOG.map((g) => (
            <DomainGroup key={g.domain} group={g} />
          ))}
        </div>
        <p className="mt-3 text-label-12 text-gray-900">
          实际可见目录由凭证 scope 决定——用下方「连接测试」查看某凭证的真实可见集。
        </p>
      </section>

      {/* 连接测试 */}
      <section className="rounded-lg border border-gray-400 p-6">
        <div className="flex items-baseline justify-between">
          <h2 className="text-heading-16">连接测试</h2>
          <span className="text-label-12 text-gray-900">initialize + tools/list 实测</span>
        </div>
        <div className="mt-3 flex gap-2">
          <input
            value={testKey}
            onChange={(e) => setTestKey(e.target.value)}
            placeholder="粘贴 API 凭证明文（helm_…）"
            className="h-8 flex-1 rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-13 outline-none hover:border-gray-500"
          />
          <button
            type="button"
            disabled={!testKey.trim() || testing}
            onClick={runTest}
            className="flex h-8 items-center gap-1.5 rounded-md bg-gray-700 px-3 text-label-13 text-white hover:bg-gray-800 disabled:opacity-40"
          >
            <PlugZap size={14} strokeWidth={1.5} />
            {testing ? "测试中…" : "测试连接"}
          </button>
        </div>
        {result && (
          <div
            className={`mt-3 rounded-md border p-3 font-mono text-label-12 ${
              result.ok ? "border-gray-400 bg-gray-100" : "border-red-700 bg-red-700/10 text-red-1000"
            }`}
          >
            {result.ok ? (
              <>
                <p>✓ 握手成功 · 协议 {result.protocolVersion} · 可见操作 {result.opCount} 个</p>
                {result.ops && (
                  <p className="mt-2 break-all text-gray-900">{result.ops.join(" · ")}</p>
                )}
              </>
            ) : (
              <p>{result.error}</p>
            )}
          </div>
        )}
        <p className="mt-2 text-label-12 text-gray-900">
          测试仅在本页发起，不会保存凭证。
        </p>
      </section>

      {/* scope 说明 */}
      <section className="rounded-lg border border-gray-400 p-6">
        <div className="flex items-baseline justify-between">
          <h2 className="text-heading-16">Scope 与可见操作</h2>
          <Link
            to="/settings/api-keys"
            className="flex items-center gap-1 text-label-13 text-blue-1000 hover:underline"
          >
            <KeyRound size={13} strokeWidth={1.5} />
            去设置页签发凭证
          </Link>
        </div>
        <table className="mt-3 w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <th className="w-32 px-2 py-2 font-normal">Scope</th>
              <th className="px-2 py-2 font-normal">能力域</th>
            </tr>
          </thead>
          <tbody>
            {SCOPES.map((s) => (
              <tr key={s.id} className="border-b border-gray-400/60 last:border-0">
                <td className="px-2 py-1.5 font-mono text-label-13">{s.id}</td>
                <td className="px-2 py-1.5 text-label-13 text-gray-900">{s.label}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="mt-3 text-label-12 text-gray-900">
          空 scopes = 全功能。api-keys 管理与账号端点不在 MCP 目录内（永远仅控制台 JWT 可操作）。
        </p>
      </section>
    </div>
  );
}
