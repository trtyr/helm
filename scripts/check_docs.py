#!/usr/bin/env python3
"""校验 docs/ 归档 + plantree 现状陈述与代码一致（机器可查）。

检查项：
1. 必需文档齐全。
2. 无残留旧痕迹（已删脚本 / 旧测试数 / 旧迁移数 / 「未实现」现状陈述）。
3. data-model.md 迁移版本数与 migrations/ 目录一致。
4. tech-stack.md 关键依赖与 Cargo.toml 一致。
5. api.md 关键端点覆盖。

扫描范围：docs/ 归档 + 根 README + plantree 的 baseline/roadmap/topics/README
（排除 decisions/——那是历史决策上下文，记录「决策时」的现状，属合理保留）。
"""

import os
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
DOCS = os.path.join(ROOT, "docs")

REQUIRED = [
    "docs/README.md",
    "docs/overview.md",
    "docs/architecture.md",
    "docs/tech-stack.md",
    "docs/api.md",
    "docs/data-model.md",
    "docs/run-and-deploy.md",
    "docs/conventions.md",
    "docs/current-state.md",
    "docs/openapi.yaml",
    "README.md",
]

# 过时痕迹（现状陈述，不含 decisions/ 历史决策上下文）。
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
    with open(os.path.join(ROOT, rel)) as f:
        return f.read()


def plantree_md_files() -> list[str]:
    """plantree 下除 decisions/ 外的所有 .md（现状陈述），用相对路径返回。"""
    out = []
    for dirpath, dirnames, filenames in os.walk(os.path.join(DOCS, "plantree")):
        dirnames[:] = [d for d in dirnames if d != "decisions"]
        for fn in filenames:
            if fn.endswith(".md"):
                out.append(os.path.relpath(os.path.join(dirpath, fn), ROOT))
    return out


def main() -> None:
    # 1. 文件齐全
    for d in REQUIRED:
        assert os.path.exists(os.path.join(ROOT, d)), f"缺失文档: {d}"

    # 2. 旧痕迹：docs/ 归档 + README + plantree（现状陈述，排除 decisions）
    scanned = list(REQUIRED) + plantree_md_files()
    for rel in scanned:
        text = read(rel)
        for pat in STALE:
            assert pat not in text, f"{rel} 残留旧痕迹 '{pat}'"

    # 3. 迁移版本数：data-model.md 与 migrations/ 目录一致
    migrations = sorted(
        f for f in os.listdir(os.path.join(ROOT, "server/migrations")) if f.endswith(".up.sql")
    )
    assert len(migrations) == 9, f"迁移版本应为 9，实际 {len(migrations)}: {migrations}"
    dm = read("docs/data-model.md")
    assert "0007_add_alerts" in dm, "data-model.md 缺 0007_add_alerts 迁移"
    assert "0008_add_notifications" in dm, "data-model.md 缺 0008_add_notifications 迁移"
    assert "0009_add_api_keys" in dm, "data-model.md 缺 0009_add_api_keys 迁移"

    # 4. 关键依赖：tech-stack.md 与 Cargo.toml 双向一致
    cargo = read("Cargo.toml")
    ts = read("docs/tech-stack.md")
    for dep in ["rcgen", "tracing-appender", "portable-pty", "reqwest", "tls-ring", "time"]:
        assert dep in cargo, f"Cargo.toml 缺依赖 '{dep}'"
        assert dep in ts, f"tech-stack.md 缺依赖 '{dep}'"

    # 5. api.md 关键端点 spot-check
    api = read("docs/api.md")
    for ep in [
        "/api/v1/listeners",
        "/api/v1/agents/cert",
        "/api/v1/services",
        "/api/v1/audit",
        "/api/v1/alerts",
        "/api/v1/processes/list",
        "/api/v1/net/info",
        "metrics/stream",
        "logs/stream",
        "terminal",
        "/api/v1/forward/exec",
    ]:
        assert ep in api, f"api.md 缺端点 '{ep}'"

    print(
        f"✓ docs/ + plantree 现状与代码一致（{len(migrations)} 迁移、关键依赖/端点全覆盖、无旧痕迹，"
        f"扫描 {len(scanned)} 个文档）"
    )


if __name__ == "__main__":
    main()
