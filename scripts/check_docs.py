#!/usr/bin/env python3
"""本地文档资产与现状陈述对账（机器可查）。

2026-09-15 起，helm 全部书面记录归档于 engram（projects 域，project=helm），
本地 docs/ 只保留 README.md 指针索引与 openapi.yaml 机器契约。
本脚本只校验**本地**能对账的事实（不依赖 engram 服务）：

1. 必需本地资产齐全（根 README / docs 指针索引 / openapi 契约 / 关键脚本）。
2. 本地 md 无残留旧痕迹（已删脚本 / 旧测试数等历史表述）。
3. migrations/ 对账：up/down 成对、版本号连续无缺号，且与根 README 声明一致。
4. 根 README 现状陈述 vs 代码/契约：ir 模块数、迁移版本数、HTTP 端点数、MCP op 数。
5. openapi.yaml 关键端点 spot-check（复用 check_openapi.parse_paths，口径一致）。

归档正文（原 docs/*.md 与 plantree 全树）与代码的一致性由 engram 侧流程承担
（doc_search/doc_get + 差异清单），不在本脚本范围。
所有文件遍历一律跳过 `._*`（exFAT AppleDouble 垃圾，曾致本脚本编码崩溃）。
"""

import os
import re
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

REQUIRED = [
    "README.md",
    "docs/README.md",
    "docs/openapi.yaml",
    "scripts/check_openapi.py",
    "scripts/check_docs.py",
    "Cargo.toml",
    "server/migrations",
    "server/src/config/mod.rs",
    "agent/src/ir/mod.rs",
    "server/src/application/mcp_registry.rs",
]

# 过时痕迹（现状陈述）；归档正文已迁 engram，本地仅剩根 README 与 docs 指针索引。
STALE = [
    "e2e-smoke.sh",
    "e2e-scheduler.sh",
    "16 passed",
    "3 个迁移",
    "0001..0003",
    "12 个端点",
    "规划中，未实现",
    "无交互终端",
    "无 systemd unit",
    "无超时判离线",
]


def read(rel: str) -> str:
    with open(os.path.join(ROOT, rel), encoding="utf-8") as f:
        return f.read()


def is_junk(name: str) -> bool:
    """exFAT AppleDouble 垃圾（._*）——遍历与读取一律跳过。"""
    return name.startswith("._")


def local_md_files() -> list[str]:
    """本地全部 .md（根目录 + docs/，跳过 ._* 与构建目录）。"""
    out = []
    for base in ("", "docs"):
        d = os.path.join(ROOT, base) if base else ROOT
        for dirpath, dirnames, filenames in os.walk(d):
            dirnames[:] = [x for x in dirnames if x not in (".git", "target", "node_modules")]
            for fn in filenames:
                if fn.endswith(".md") and not is_junk(fn):
                    out.append(os.path.relpath(os.path.join(dirpath, fn), ROOT))
    return sorted(set(out))


def migrations_audit() -> list[str]:
    """up/down 成对 + 版本号连续无缺号，返回 up 文件名列表。"""
    mig_dir = os.path.join(ROOT, "server/migrations")
    files = [f for f in os.listdir(mig_dir) if not is_junk(f)]
    ups = sorted(f for f in files if f.endswith(".up.sql"))
    up_stems = {u[:-7] for u in ups}  # 去掉 ".up.sql"
    down_stems = {f[:-9] for f in files if f.endswith(".down.sql")}  # 去掉 ".down.sql"
    assert up_stems == down_stems, (
        f"up/down 不成对: 缺 down 的 up={sorted(up_stems - down_stems)}, "
        f"缺 up 的 down={sorted(down_stems - up_stems)}"
    )
    nums = sorted(int(u.split("_", 1)[0]) for u in ups)
    assert nums == list(range(1, len(nums) + 1)), f"迁移版本号不连续: {nums}"
    return ups


def first_int(text: str, pattern: str, what: str) -> int:
    m = re.search(pattern, text)
    assert m, f"README 未找到 {what} 声明（模式 {pattern}）"
    return int(m.group(1))


def main() -> None:
    # 1. 本地资产齐全
    for rel in REQUIRED:
        assert os.path.exists(os.path.join(ROOT, rel)), f"缺失本地资产: {rel}"

    # 2. 旧痕迹扫描（本地 md，跳过 ._*）
    scanned = local_md_files()
    for rel in scanned:
        for pat in STALE:
            assert pat not in read(rel), f"{rel} 残留旧痕迹 '{pat}'"

    # 3. migrations 对账（成对 + 连续，无硬编码总数）
    ups = migrations_audit()
    n_mig = len(ups)

    # 4. 根 README 现状陈述 vs 代码/契约
    readme = read("README.md")

    n_readme_mig = first_int(readme, r"(\d+) 个版本", "迁移版本数")
    assert n_readme_mig == n_mig, f"README 声称迁移 {n_readme_mig} 个版本，migrations/ 实际 {n_mig}"

    ir_mod_rs = read("agent/src/ir/mod.rs")
    n_ir = len(re.findall(r"^mod \w+;", ir_mod_rs, flags=re.M))
    n_readme_ir = first_int(readme, r"(\d+) 个模块", "ir 模块数")
    assert n_readme_ir == n_ir, f"README 声称 ir {n_readme_ir} 个模块，agent/src/ir/mod.rs 实际声明 {n_ir}"

    import check_openapi  # 同目录：复用路径解析（yq 优先，PyYAML 兜底），口径与契约门禁一致
    n_ep = len(check_openapi.parse_paths())
    n_readme_ep = first_int(readme, r"(\d+) 端点", "HTTP 端点数")
    assert n_readme_ep == n_ep, f"README 声称 {n_readme_ep} 端点，openapi.yaml 实际 {n_ep}"

    registry = read("server/src/application/mcp_registry.rs")
    n_op = len(re.findall(r"^    op!\(", registry, flags=re.M))
    assert n_op > 0, "未能从 mcp_registry.rs 统计 op! 条目"
    assert f"{n_op} op" in readme, f"README 未声明 MCP {n_op} op（现状陈述漂移）"

    # 5. openapi 关键端点 spot-check
    paths = check_openapi.parse_paths()
    for ep in [
        "/api/v1/listeners",
        "/api/v1/agents/cert",
        "/mcp",
        "/api/v1/audit",
        "/api/v1/forward/exec",
    ]:
        assert ep in paths, f"openapi.yaml 缺端点 '{ep}'"

    # 6. docs/README.md 指针索引自检（engram 归档声明的结构性检查）
    index_doc = read("docs/README.md")
    for cat in ["总览", "架构与设计", "接口契约", "数据模型", "运行与部署", "约定", "现状与门禁", "应急响应", "规划"]:
        assert cat in index_doc, f"docs/README.md 指针索引缺分类 '{cat}'"

    print(
        f"✓ 本地文档资产与现状陈述一致（{n_mig} 迁移 up/down 成对且连续、"
        f"ir {n_ir} 模块、{n_ep} 端点、MCP {n_op} op 与 README 对账、无旧痕迹，扫描 {len(scanned)} 个本地 md）"
    )


if __name__ == "__main__":
    main()
