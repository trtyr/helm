#!/usr/bin/env python3
"""Phase 11 e2e：Skill 包分发（决策 011）。

覆盖：key 下载 zip → 清单校验（版本/文件/sha256 与 zip 一致）→
解包后用包内脚本真操作平台 → 无凭据/错凭据 401 → 吊销后 401。

依赖已构建的 ./target/debug/helm-server（本脚本负责起 Postgres 与 Server）。
"""

import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
import zipfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from e2e_helpers import (
    HTTP_ADDR,
    login,
    http_json,
    start_background,
    start_postgres,
    stop_processes,
    wait_server,
)

BASE = f"http://{HTTP_ADDR}/api/v1"


def http_raw(path: str, token: str | None = None) -> tuple[int, bytes, object]:
    """原始 HTTP 请求，返回 (status, body, headers)（headers 大小写不敏感取值）。
    401 也返回不抛。"""
    req = urllib.request.Request(f"{BASE}{path}", method="GET")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            return resp.status, resp.read(), resp.headers
    except urllib.error.HTTPError as e:
        return e.code, e.read(), e.headers


def main() -> None:
    start_postgres()
    os.environ["HELM_HTTP_ADDR"] = HTTP_ADDR
    server = start_background(["./target/debug/helm-server"], "e2e-skill-server.log")
    try:
        wait_server()

        # 1. JWT 创建 key（key 的常规获取路径）
        jwt = login()
        key = http_json("POST", "/api-keys", {"name": "e2e-skill"}, jwt)["key"]

        # 2. key 下载 zip：魔数 + 可解压 + 关键文件齐全
        st, body, headers = http_raw("/skill", key)
        assert st == 200, (st, body[:200])
        assert body[:4] == b"PK\x03\x04", "not a zip"
        assert "attachment" in headers.get("Content-Disposition", ""), headers
        archive = zipfile.ZipFile(io.BytesIO(body))
        names = set(archive.namelist())
        for required in ("SKILL.md", "scripts/common.py", "scripts/exec.py",
                         "scripts/ws.py", "references/api.md"):
            assert required in names, f"zip 缺 {required}"
        print(f"1. zip ok（{len(body)} bytes, {len(names)} files, attachment）")

        # 3. 清单与 zip 一致（版本/文件集/sha256）
        manifest = http_json("GET", "/skill/manifest", token=key)
        assert manifest["version"], manifest
        mfiles = {f["path"]: f for f in manifest["files"]}
        assert set(mfiles) == names, "清单与 zip 文件集不一致"
        for path, info in mfiles.items():
            data = archive.read(path)
            assert info["size"] == len(data), path
            assert info["sha256"] == hashlib.sha256(data).hexdigest(), path
        print(f"2. manifest ok（v{manifest['version']}, {manifest['file_count']} files, sha256 一致）")

        # 4. 解包后用包内脚本真操作平台（零安装）
        skill_dir = tempfile.mkdtemp(prefix="helm-skill-e2e-")
        archive.extractall(skill_dir)
        env = dict(os.environ, HELM_URL=f"http://{HTTP_ADDR}", HELM_TOKEN=key)
        script = os.path.join(skill_dir, "scripts", "hosts.py")
        out = subprocess.run([sys.executable, script, "list", "--limit", "5"],
                             capture_output=True, text=True, env=env, timeout=60)
        assert out.returncode == 0, out.stdout + out.stderr
        hosts = json.loads(out.stdout)
        assert isinstance(hosts.get("hosts"), list)
        print(f"3. 解包即用 ok（包内脚本列出 {len(hosts['hosts'])} 台主机）")
        shutil.rmtree(skill_dir, ignore_errors=True)

        # 5. 无凭据 / 错凭据 → 401
        st, _, _ = http_raw("/skill")
        assert st == 401, st
        st, _, _ = http_raw("/skill", "helm_" + "0" * 40)
        assert st == 401, st
        print("4. 401 ok（无凭据 / 错凭据均拒绝）")

        # 6. 吊销后同 key → 401（生命周期闭环）
        listing = http_json("GET", "/api-keys?page=1&limit=50", token=jwt)["api_keys"]
        kid = listing[0]["id"]
        key_name = [k for k in listing if k["id"] == kid][0]["name"]
        http_json("DELETE", f"/api-keys/{kid}", token=jwt)
        st, _, _ = http_raw("/skill", key)
        assert st == 401, f"吊销后仍可下载: {st}"
        print(f"5. 吊销闭环 ok（{key_name} 吊销后下载被拒）")

        print("\nE2E SKILL PACKAGE: ALL CHECKS PASSED")
    finally:
        stop_processes([server])


if __name__ == "__main__":
    main()
