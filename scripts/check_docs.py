#!/usr/bin/env python3
"""校验 docs/ 归档与代码现状一致（机器可查）。

检查项：
1. 必需文档齐全。
2. 无残留旧痕迹（已删脚本 / 旧测试数 / 旧迁移数 / 已删文件）。
3. data-model.md 迁移版本数与 migrations/ 目录一致。
4. tech-stack.md 关键依赖与 Cargo.toml 一致。
5. api.md 关键端点覆盖。
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

# 过时痕迹（docs/ 归档 + README，不含 plantree 历史规划）。
STALE = [
    ("e2e-smoke.sh", "已删的 bash e2e 脚本"),
    ("e2e-scheduler.sh", "已删的 bash e2e 脚本"),
    ("16 passed", "过时测试数（现 47）"),
    ("3 个迁移", "过时迁移版本数（现 7）"),
    ("孤儿 src/main.rs", "已删的孤儿文件"),
    ("迁移（3 个版本）", "过时迁移版本数"),
]


def read(path: str) -> str:
    with open(os.path.join(ROOT, path)) as f:
        return f.read()


def main() -> None:
    # 1. 文件齐全
    for d in REQUIRED:
        assert os.path.exists(os.path.join(ROOT, d)), f"缺失文档: {d}"

    # 2. 旧痕迹（仅 docs/*.md + README，不扫 plantree/）
    for d in REQUIRED:
        text = read(d)
        for pat, desc in STALE:
            assert pat not in text, f"{d} 残留旧痕迹 '{pat}'（{desc}）"

    # 3. 迁移版本数：data-model.md 与 migrations/ 目录一致
    migrations = sorted(
        f for f in os.listdir(os.path.join(ROOT, "server/migrations")) if f.endswith(".up.sql")
    )
    assert len(migrations) == 7, f"迁移版本应为 7，实际 {len(migrations)}: {migrations}"
    dm = read("docs/data-model.md")
    assert "0007_add_alerts" in dm, "data-model.md 缺 0007_add_alerts 迁移"

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
        f"✓ docs/ 归档与代码现状一致（{len(migrations)} 迁移、关键依赖/端点全覆盖、无旧痕迹）"
    )


if __name__ == "__main__":
    main()
