#!/usr/bin/env python3
"""从 mcp_registry 的**域文件**（server/src/application/mcp_registry/ops_{host,platform}.rs）生成 console/src/lib/mcpCatalog.ts。

注：2026-09-20 G7 拆分后，op 表由单文件 mcp_registry.rs 拆成两个域文件
（`pub static HOST_OPS` / `pub static PLATFORM_OPS`），本脚本的解析面随之改为
「按 REGISTRY_PARTS 顺序逐文件解析」——域顺序仍由列表顺序决定（host 在前）。
生成物中的指针注释保持原样（指向模块根 mcp_registry.rs），以确保输出逐字不变。

P002 两域极简版：域分组来自 // -- <name> 域 注释（容忍括号说明）。
每个 op 附 group 字段 = 能力组（复验反馈：目录三级化——域 → 能力组 → 工具，与 registry top_subgroup 同映射）。
内置断言（防域分组回归）：域数=2（host/platform）、op 数=41、能力组 8 组计数吻合、已删 6 op 不出现。
"""
import re

REGISTRY_PARTS = ['server/src/application/mcp_registry/ops_host.rs',
                  'server/src/application/mcp_registry/ops_platform.rs']
TARGET = 'console/src/lib/mcpCatalog.ts'
EXPECT_DOMAINS = {'host': 28, 'platform': 13}
FORBIDDEN = {'metrics.list', 'alerts.list', 'notifications.list',
             'notifications.read', 'notifications.read_all', 'audit.list'}

# 能力组映射（与 server mcp_registry.rs top_subgroup 保持一致）
SUBGROUPS = {
    'exec': '执行', 'jobs': '执行', 'tasks': '执行',
    'files': '文件',
    'services': '系统状态', 'sys_services': '系统状态', 'processes': '系统状态', 'net': '系统状态',
    'ir': '取证 IR',
    'forward': '反向执行',
    'hosts': '主机管理', 'agent': '主机管理',
    'listeners': '网络服务', 'proxies': '网络服务',
    'agent_gen': 'Agent 生成',
}
EXPECT_SUBGROUPS = {'执行': 6, '文件': 3, '系统状态': 10, '取证 IR': 8, '反向执行': 1,
                    '主机管理': 4, '网络服务': 7, 'Agent 生成': 2}

src = ''
lines = []
domain_re = re.compile(r'^\s*//\s*--\s*([\w-]+)\s+域')


def parse_registry_file(path):
    """解析一个域文件，返回 [(domain, block_text), ...]（按文件内出现顺序）。"""
    flines = open(path, encoding='utf-8').read().split('\n')
    start = next(i for i, l in enumerate(flines)
                 if l.startswith('pub static ') and l.rstrip().endswith('= &['))
    end = next(i for i, l in enumerate(flines) if l.strip() == '];' and i > start)
    out, cur_domain, cur_block = [], None, None
    for l in flines[start + 1:end]:
        m = domain_re.match(l)
        if m:
            cur_domain = m.group(1)
            continue
        if l.strip().startswith('op!('):
            cur_block = [l]
        elif cur_block is not None:
            cur_block.append(l)
            if l.strip() == '),':
                assert cur_domain, f"{path}: op 块出现在 `// -- <域> 域` 注释之前"
                out.append((cur_domain, '\n'.join(cur_block)))
                cur_block = None
    return out


blocks = {}
for path in REGISTRY_PARTS:
    for domain, block in parse_registry_file(path):
        blocks.setdefault(domain, []).append(block)


def parse_block(block):
    scalars = re.findall(r'^\s+"((?:[^"\\]|\\.)*)",\s*$', block, re.MULTILINE)
    os_m = re.search(r'Os::(\w+)', block)
    assert os_m, f"块缺 Os:: 标注:\n{block}"
    assert len(scalars) == 5, f"块应含 5 个标量字符串（name/scope/method/path/summary），实际 {len(scalars)}:\n{block}"
    name, scope, method, path, summary = scalars
    params = re.findall(r'\(\s*"((?:[^"\\]|\\.)*)"\s*,\s*"((?:[^"\\]|\\.)*)"\s*\)', block)
    prefix = name.split('.')[0]
    group = SUBGROUPS.get(prefix, '其他')
    return {'name': name, 'scope': scope, 'os': os_m.group(1), 'group': group,
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

# 能力组计数断言
sub_count: dict = {}
for _, ops in groups:
    for o in ops:
        sub_count[o['group']] = sub_count.get(o['group'], 0) + 1
assert sub_count == EXPECT_SUBGROUPS, f"能力组计数不符：{sub_count} vs {EXPECT_SUBGROUPS}"


def ts_str(s):
    return '"' + s.replace('\\', '\\\\').replace('"', '\\"') + '"'


out = []
out.append('/** MCP 工具编目（前端静态镜像，与 server/src/application/mcp_registry.rs OPS 注册表同步；SCOPES 同模式）。由 scripts/gen_mcp_catalog.py 生成——勿手改。 */')
out.append('export interface McpOp {')
for f in ['name', 'scope', 'os', 'group', 'method', 'path', 'summary']:
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
        for f in ['name', 'scope', 'os', 'group', 'method', 'path', 'summary']:
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
print(f"✓ 生成 {TARGET}：" + '，'.join(f'{d} {len(ops)} op' for d, ops in groups)
      + f"，共 {len(all_names)} op；能力组：" + '，'.join(f'{k} {v}' for k, v in sub_count.items()))
