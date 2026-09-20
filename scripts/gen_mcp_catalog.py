#!/usr/bin/env python3
"""从 server/src/application/mcp_registry.rs 的 OPS 注册表生成 console/src/lib/mcpCatalog.ts。

P002 两域极简版：域分组来自 // -- <name> 域 注释（容忍括号说明）。
内置断言（防域分组回归）：域数=2（host/platform）、op 数=41（host 28 / platform 13）、已删 6 op 不出现。
"""
import re

REGISTRY = 'server/src/application/mcp_registry.rs'
TARGET = 'console/src/lib/mcpCatalog.ts'
EXPECT_DOMAINS = {'host': 28, 'platform': 13}
FORBIDDEN = {'metrics.list', 'alerts.list', 'notifications.list',
             'notifications.read', 'notifications.read_all', 'audit.list'}

src = open(REGISTRY, encoding='utf-8').read()
lines = src.split('\n')
start = next(i for i, l in enumerate(lines) if 'pub static OPS' in l)
end = next(i for i, l in enumerate(lines) if l.strip() == '];' and i > start)

domain_re = re.compile(r'^\s*//\s*--\s*([\w-]+)\s+域')
blocks = {}
cur_domain, cur_block = None, None
for l in lines[start + 1:end]:
    m = domain_re.match(l)
    if m:
        cur_domain = m.group(1)
        blocks.setdefault(cur_domain, [])
        continue
    if l.strip().startswith('op!('):
        cur_block = [l]
    elif cur_block is not None:
        cur_block.append(l)
        if l.strip() == '),':
            blocks[cur_domain].append('\n'.join(cur_block))
            cur_block = None


def parse_block(block):
    scalars = re.findall(r'^\s+"((?:[^"\\]|\\.)*)",\s*$', block, re.MULTILINE)
    os_m = re.search(r'Os::(\w+)', block)
    assert os_m, f"块缺 Os:: 标注:\n{block}"
    assert len(scalars) == 5, f"块应含 5 个标量字符串（name/scope/method/path/summary），实际 {len(scalars)}:\n{block}"
    name, scope, method, path, summary = scalars
    params = re.findall(r'\(\s*"((?:[^"\\]|\\.)*)"\s*,\s*"((?:[^"\\]|\\.)*)"\s*\)', block)
    return {'name': name, 'scope': scope, 'os': os_m.group(1),
            'method': method, 'path': path, 'summary': summary, 'params': params}


groups = [(domain, [parse_block(b) for b in blist]) for domain, blist in blocks.items()]

# ── 断言 ──
assert set(d for d, _ in groups) == set(EXPECT_DOMAINS), \
    f"域应为 {sorted(EXPECT_DOMAINS)}，实际 {sorted(d for d, _ in groups)}"
all_names = [o['name'] for _, ops in groups for o in ops]
assert len(all_names) == 41, f"op 总数应为 41，实际 {len(all_names)}"
assert len(set(all_names)) == len(all_names), "op 名重复"
assert not (set(all_names) & FORBIDDEN), f"已删 op 出现: {set(all_names) & FORBIDDEN}"
for domain, expect_n in EXPECT_DOMAINS.items():
    actual = next(len(ops) for d, ops in groups if d == domain)
    assert actual == expect_n, f"{domain} 域应为 {expect_n} op，实际 {actual}"


def ts_str(s):
    return '"' + s.replace('\\', '\\\\').replace('"', '\\"') + '"'


out = []
out.append('/** MCP 工具编目（前端静态镜像，与 server/src/application/mcp_registry.rs OPS 注册表同步；SCOPES 同模式）。由 scripts/gen_mcp_catalog.py 生成——勿手改。 */')
out.append('export interface McpOp {')
for f in ['name', 'scope', 'os', 'method', 'path', 'summary']:
    out.append(f'  {f}: string;')
out.append('  params: [string, string][];')
out.append('}')
out.append('export interface McpDomainGroup {')
out.append('  domain: string;')
out.append('  ops: McpOp[];')
out.append('}')
out.append('export const MCP_CATALOG: McpDomainGroup[] = [')
for domain, ops in groups:
    out.append('  {')
    out.append(f'    domain: {ts_str(domain)},')
    out.append('    ops: [')
    for o in ops:
        out.append('      {')
        for f in ['name', 'scope', 'os', 'method', 'path', 'summary']:
            out.append(f'        {f}: {ts_str(o[f])},')
        pairs = ', '.join(f'[{ts_str(k)}, {ts_str(v)}]' for k, v in o['params'])
        out.append(f'        params: [{pairs}] as [string, string][],')
        out.append('      },')
    out.append('    ],')
    out.append('  },')
out.append('];')
out.append('')
out.append('/** op 总数（与 check_docs 的 MCP op 对账口径一致）。 */')
out.append('export const MCP_OP_COUNT = MCP_CATALOG.reduce((n, d) => n + d.ops.length, 0); // 41')
out.append('')

open(TARGET, 'w', encoding='utf-8').write('\n'.join(out))
print(f"✓ 生成 {TARGET}：" + '，'.join(f'{d} {len(ops)} op' for d, ops in groups) + f"，共 {len(all_names)} op")
