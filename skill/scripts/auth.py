#!/usr/bin/env python3
"""认证域：登录验证、凭据自检。

用法：
  python auth.py login [--username u --password p]   # 登录拿 JWT 并验证
  python auth.py status                              # healthz + 当前凭据可用性

注：API key 的创建/吊销是 JWT 专属端点（key 不可自管），不在本 skill 范围；
请在控制台或用 JWT 直接调用 POST/DELETE /api/v1/api-keys 管理。
"""

import argparse
import json
import os

import common


def cmd_login(a):
    if a.username:
        os.environ["HELM_USERNAME"] = a.username
    if a.password:
        os.environ["HELM_PASSWORD"] = a.password
    token = common.get_token()
    status, data = common.request("GET", "/hosts?page=1&limit=1", token)
    kind = "api-key" if token.startswith("helm_") else "JWT"
    if status == 200:
        print(f"ok: {kind} 有效（{common.base_url()}）")
    else:
        common.die(f"{kind} 无效（{status}）：{data}")
    if a.show_token:
        print(token)


def cmd_status(a):
    import urllib.request

    with urllib.request.urlopen(common.base_url() + "/healthz", timeout=10) as r:
        print("server:", r.read().decode())
    print("base_url:", common.base_url())
    env_token = os.environ.get("HELM_TOKEN")
    print("HELM_TOKEN:", "set (%s)" % ("api-key" if env_token.startswith("helm_") else "JWT")
          if env_token else "unset（将用用户名密码登录）")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    login = sub.add_parser("login", help="登录并验证凭据")
    login.add_argument("--username")
    login.add_argument("--password")
    login.add_argument("--show-token", action="store_true", help="打印 token 本身")
    common.add_common_args(login)

    st = sub.add_parser("status", help="Server 健康与凭据来源")
    common.add_common_args(st)

    a = p.parse_args()
    common.apply_common_args(a)
    {"login": cmd_login, "status": cmd_status}[a.cmd](a)


if __name__ == "__main__":
    main()
