#!/usr/bin/env python3
"""监听器域：gRPC 监听器（DB 实体）的增删查改与启停。

用法：
  python listeners.py list
  python listeners.py create --name edge-1 --addr 0.0.0.0:50052 [--auth <token>]
  python listeners.py update <id> --name edge-1 --addr 0.0.0.0:50053
  python listeners.py start|stop <id>
  python listeners.py delete <id>
"""

import argparse

import common


def cmd_list(a):
    common.output(common.call("GET", "/listeners"))


def cmd_create(a):
    body = {"name": a.name, "addr": a.addr, "proto": "grpc"}
    if a.auth:
        body["auth"] = a.auth
    common.output(common.call("POST", "/listeners", body))


def cmd_update(a):
    body = {"name": a.name, "addr": a.addr, "proto": "grpc"}
    if a.auth is not None:
        body["auth"] = a.auth
    common.output(common.call("PUT", f"/listeners/{a.id}", body))


def cmd_simple(a):
    common.output(common.call("POST", f"/listeners/{a.id}/{a.action}"))


def cmd_delete(a):
    common.output(common.call("DELETE", f"/listeners/{a.id}"))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    sub.add_parser("list", help="列出监听器")
    common.add_common_args(sub.choices["list"])
    cr = sub.add_parser("create", help="创建监听器（默认 stopped）")
    cr.add_argument("--name", required=True)
    cr.add_argument("--addr", required=True, help="监听地址 host:port")
    cr.add_argument("--auth", help="该监听器的 Agent 注册 token")
    common.add_common_args(cr)

    up = sub.add_parser("update", help="更新监听器")
    up.add_argument("id")
    up.add_argument("--name", required=True)
    up.add_argument("--addr", required=True)
    up.add_argument("--auth")
    common.add_common_args(up)

    for action in ("start", "stop"):
        s = sub.add_parser(action, help=f"{action} 监听器")
        s.add_argument("id")
        common.add_common_args(s)

    de = sub.add_parser("delete", help="删除监听器")
    de.add_argument("id")
    common.add_common_args(de)

    a = p.parse_args()
    common.apply_common_args(a)
    dispatch = {"list": cmd_list, "create": cmd_create, "update": cmd_update,
                "start": cmd_simple, "stop": cmd_simple, "delete": cmd_delete}
    if a.cmd in ("start", "stop"):
        a.action = a.cmd
    dispatch[a.cmd](a)


if __name__ == "__main__":
    main()
