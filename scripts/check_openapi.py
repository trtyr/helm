#!/usr/bin/env python3
"""校验 docs/openapi.yaml 与 server/src/http/mod.rs 的端点一致性（机器可查）。

优先用 yq（Rust）解析 YAML；本机无 yq 时回退 PyYAML。纯标准库对比。
"""

import os
import re
import subprocess
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")


def _parse_paths_yq() -> set[str]:
    out = subprocess.run(
        ["yq", ".paths | keys | .[]", "docs/openapi.yaml"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        sys.exit(f"openapi.yaml 解析失败: {out.stderr}")
    return set(out.stdout.split())


def _parse_version_yq() -> str:
    out = subprocess.run(
        ["yq", ".openapi", "docs/openapi.yaml"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        sys.exit(f"openapi.yaml 解析失败: {out.stderr}")
    return out.stdout.strip()


def _parse_paths_pyyaml() -> set[str]:
    import yaml

    with open(os.path.join(ROOT, "docs/openapi.yaml"), encoding="utf-8") as f:
        doc = yaml.safe_load(f)
    return set(doc.get("paths", {}).keys())


def _parse_version_pyyaml() -> str:
    import yaml

    with open(os.path.join(ROOT, "docs/openapi.yaml"), encoding="utf-8") as f:
        doc = yaml.safe_load(f)
    return str(doc.get("openapi", "")).strip()


def yq_available() -> bool:
    try:
        return (
            subprocess.run(
                ["yq", "--version"], capture_output=True, text=True
            ).returncode
            == 0
        )
    except FileNotFoundError:
        return False


def parse_paths() -> set[str]:
    return _parse_paths_yq() if yq_available() else _parse_paths_pyyaml()


def parse_version() -> str:
    return _parse_version_yq() if yq_available() else _parse_version_pyyaml()


def server_routes() -> set[str]:
    mod = open(os.path.join(ROOT, "server/src/http/mod.rs")).read()
    raw = re.findall(r'\.route\(\s*"([^"]+)"', mod)
    full: set[str] = set()
    for r in raw:
        if r == "/healthz":
            full.add("/healthz")
        elif r.startswith("/api/v1"):
            full.add(r)  # 顶层路由（terminal / cert / 各 WS 流）
        else:
            full.add("/api/v1" + r)  # protected 路由（nest 到 /api/v1）
    return full


def main() -> None:
    version = parse_version()
    assert version == "3.0.3", f"openapi 版本应为 3.0.3，实际 {version}"

    paths = parse_paths()
    routes = server_routes()

    missing = routes - paths
    extra = paths - routes
    assert not missing, f"server 有但 openapi 缺失的端点: {sorted(missing)}"
    assert not extra, f"openapi 有但 server 无的端点: {sorted(extra)}"

    print(f"✓ openapi.yaml 与 server 路由一致（{len(paths)} 端点，版本 {version}）")


if __name__ == "__main__":
    main()
