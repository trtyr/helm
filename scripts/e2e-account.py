#!/usr/bin/env python3
"""Phase 12 e2e：单用户账号管理。

覆盖：/auth/me（JWT 身份 / API key 403）/ 改密（当前密码错 401、过短 400、
生效后旧密 401）/ 改名（生效、旧 token sub 失效 401、新名可登录）/
审计埋点 / 结束后凭据还原为 admin/admin123。

依赖已构建的 ./target/debug/helm-server（本脚本负责起 Postgres 与 Server）。
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from e2e_helpers import (
    HTTP_ADDR,
    http_json,
    login,
    start_background,
    start_postgres,
    stop_processes,
    wait_server,
)
import urllib.error
import urllib.request

BASE = f"http://{HTTP_ADDR}/api/v1"


def login_status(username: str, password: str) -> tuple[int, dict]:
    """登录并返回 (status, body)，401 不抛。"""
    req = urllib.request.Request(
        f"{BASE}/auth/login",
        data=json.dumps({"username": username, "password": password}).encode(),
        method="POST",
    )
    req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            return resp.status, json.loads(resp.read())
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read())


def main() -> None:
    start_postgres()
    os.environ["HELM_HTTP_ADDR"] = HTTP_ADDR
    server = start_background(["./target/debug/helm-server"], "e2e-account-server.log")
    try:
        wait_server()

        # 1. /auth/me：JWT 身份来自库（admin / admin）
        jwt = login()
        me = http_json("GET", "/auth/me", token=jwt)["account"]
        assert me["username"] == "admin" and me["role"] == "admin", me
        assert me.get("created_at"), me
        print("1. /auth/me ok（admin / admin / created_at）")

        # 2. API key 访问账号端点 → 403（key 无账号概念）
        key = http_json("POST", "/api-keys", {"name": "e2e-acct"}, jwt)["key"]
        try:
            http_json("GET", "/auth/me", token=key)
            raise AssertionError("api key 调 /auth/me 应 403")
        except urllib.error.HTTPError as e:
            assert e.code == 403, e.code
        print("2. api key 403 ok")

        # 3. 改密：当前密码错 → 401；新密码过短 → 400
        try:
            http_json("POST", "/auth/change-password",
                      {"current_password": "wrong-pass", "new_password": "new-pass-9"}, token=jwt)
            raise AssertionError("错误当前密码应 401")
        except urllib.error.HTTPError as e:
            assert e.code == 401, e.code
        try:
            http_json("POST", "/auth/change-password",
                      {"current_password": "admin123", "new_password": "12345"}, token=jwt)
            raise AssertionError("过短新密码应 400")
        except urllib.error.HTTPError as e:
            assert e.code == 400, e.code
        print("3. 改密拒绝路径 ok（401 / 400）")

        # 4. 改密生效：新密可登录、旧密 401；随后还原
        http_json("POST", "/auth/change-password",
                  {"current_password": "admin123", "new_password": "rotated-pass-7"}, token=jwt)
        st, _ = login_status("admin", "rotated-pass-7")
        assert st == 200, st
        st, _ = login_status("admin", "admin123")
        assert st == 401, st
        http_json("POST", "/auth/change-password",
                  {"current_password": "rotated-pass-7", "new_password": "admin123"},
                  token=login("admin", "rotated-pass-7"))
        st, _ = login_status("admin", "admin123")
        assert st == 200, "还原失败"
        print("4. 改密全链路 + 还原 ok（新密生效 / 旧密 401）")

        # 5. 改名：生效、旧 JWT sub 失效、新名可登录
        new_name = f"acct-e2e-{os.getpid()}"
        http_json("POST", "/auth/change-username",
                  {"current_password": "admin123", "new_username": new_name}, token=jwt)
        st, _ = login_status(new_name, "admin123")
        assert st == 200, st
        me2 = http_json("GET", "/auth/me", token=login(new_name, "admin123"))["account"]
        assert me2["username"] == new_name, me2
        try:
            http_json("GET", "/auth/me", token=jwt)  # 旧 token（sub=admin）
            raise AssertionError("旧 token /auth/me 应 401")
        except urllib.error.HTTPError as e:
            assert e.code == 401, e.code
        print("5. 改名全链路 ok（旧 sub 401 / 新名可登录）")

        # 6. 审计埋点 + 还原用户名
        fresh = login(new_name, "admin123")
        http_json("POST", "/auth/change-username",
                  {"current_password": "admin123", "new_username": "admin"}, token=fresh)
        audit = http_json("GET", "/audit?page=1&limit=20", token=login("admin", "admin123"))
        actions = [a["action"] for a in audit["audit"]]
        assert "password_change" in actions and "username_change" in actions, actions
        me3 = http_json("GET", "/auth/me", token=login("admin", "admin123"))["account"]
        assert me3["username"] == "admin", me3
        print("6. 审计 + 还原 ok（admin/admin123）")

        print("\nE2E ACCOUNT: ALL CHECKS PASSED")
    finally:
        stop_processes([server])


if __name__ == "__main__":
    main()
