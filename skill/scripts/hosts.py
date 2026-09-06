#!/usr/bin/env python3
"""主机域：主机 CRUD 与标签。

用法：
  python hosts.py list [--tag prod] [--page 1 --limit 20] [--online] [--stale]
  python hosts.py create --hostname web1 [--conn-mode reverse|forward] [--addr host:port] [--tags a,b]
  python hosts.py update <host_id> --hostname web1 [--addr ...] [--tags ...]
  python hosts.py delete <host_id>
  python hosts.py set-tags <host_id> --tags a,b
"""

import argparse
import json

import common


def cmd_list(a):
    query = {"page": a.page, "limit": a.limit}
    if a.tag:
        query["tag"] = a.tag
    data = common.call("GET", "/hosts", query=query)
    hosts = data.get("hosts", [])
    if a.online:
        hosts = [h for h in hosts if h.get("online")]
    if a.stale:
        hosts = [h for h in hosts if h.get("stale")]
    common.output({"hosts": hosts, "count": len(hosts)})


def cmd_create(a):
    body = {"hostname": a.hostname, "conn_mode": a.conn_mode, "addr": a.addr or ""}
    if a.tags:
        body["tags"] = common.parse_list(a.tags)
    common.output(common.call("POST", "/hosts", body))


def cmd_update(a):
    body = {"hostname": a.hostname}
    if a.addr is not None:
        body["addr"] = a.addr
    if a.conn_mode:
        body["conn_mode"] = a.conn_mode
    if a.tags:
        body["tags"] = common.parse_list(a.tags)
    common.output(common.call("PUT", f"/hosts/{a.id}", body))


def cmd_delete(a):
    common.output(common.call("DELETE", f"/hosts/{a.id}"))


def cmd_set_tags(a):
    common.output(common.call("POST", f"/hosts/{a.id}/tags", {"tags": common.parse_list(a.tags)}))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    ls = sub.add_parser("list", help="列出主机（附在线状态）")
    ls.add_argument("--tag")
    ls.add_argument("--page", type=int, default=1)
    ls.add_argument("--limit", type=int, default=50)
    ls.add_argument("--online", action="store_true", help="只看在线")
    ls.add_argument("--stale", action="store_true", help="只看心跳超时")
    common.add_common_args(ls)

    cr = sub.add_parser("create", help="创建主机")
    cr.add_argument("--hostname", required=True)
    cr.add_argument("--conn-mode", choices=["reverse", "forward"], default="reverse")
    cr.add_argument("--addr", help="forward 模式拨号地址 host:port")
    cr.add_argument("--tags", help="逗号分隔标签")
    common.add_common_args(cr)

    up = sub.add_parser("update", help="更新主机")
    up.add_argument("id")
    up.add_argument("--hostname", required=True)
    up.add_argument("--conn-mode", choices=["reverse", "forward"])
    up.add_argument("--addr")
    up.add_argument("--tags")
    common.add_common_args(up)

    de = sub.add_parser("delete", help="删除主机（软删）")
    de.add_argument("id")
    common.add_common_args(de)

    tg = sub.add_parser("set-tags", help="设置主机标签")
    tg.add_argument("id")
    tg.add_argument("--tags", required=True, help="逗号分隔标签（全量覆盖）")
    common.add_common_args(tg)

    a = p.parse_args()
    common.apply_common_args(a)
    {"list": cmd_list, "create": cmd_create, "update": cmd_update,
     "delete": cmd_delete, "set-tags": cmd_set_tags}[a.cmd](a)


if __name__ == "__main__":
    main()
