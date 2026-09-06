#!/usr/bin/env python3
"""常驻服务域：目标机上的常驻后台任务（启动/停止/重启/日志）。

用法：
  python services.py list [--page 1 --limit 20]
  python services.py create <agent_id> --name my-svc --command python --args "-m,http.server" [--restart-policy always]
  python services.py update <service_uuid> --name my-svc --command python [--args ...] [--restart-policy ...]
  python services.py start|stop|restart <service_uuid>
  python services.py logs <service_uuid>            # 快照
  python services.py tail <service_uuid>            # WS 实时流（Ctrl+C 退出）
  python services.py delete <service_uuid>
"""

import argparse

import common


def cmd_list(a):
    common.output(common.call("GET", "/services", query={"page": a.page, "limit": a.limit}))


def cmd_create(a):
    common.output(common.call("POST", "/services", {
        "agent_id": a.agent_id, "name": a.name, "command": a.command,
        "args": common.parse_list(a.args), "restart_policy": a.restart_policy or "",
    }))


def cmd_update(a):
    common.output(common.call("PUT", f"/services/{a.id}", {
        "name": a.name, "command": a.command, "args": common.parse_list(a.args),
        "restart_policy": a.restart_policy or "",
    }))


def cmd_simple(a):
    common.output(common.call("POST", f"/services/{a.id}/{a.action}"))


def cmd_logs(a):
    common.output(common.call("GET", f"/services/{a.id}/logs"))


def cmd_tail(a):
    import ws as ws_mod
    from urllib.parse import urlencode, quote

    token = common.get_token()
    url = common.base_url().replace("http", "ws", 1) + \
        f"/api/v1/services/{a.id}/logs/stream?" + urlencode({"token": token})
    sock = ws_mod.WS.connect(url)
    try:
        while True:
            _, data = sock.recv()
            print(data.decode("utf-8", errors="replace"), end="", flush=True)
    except (ConnectionError, KeyboardInterrupt):
        pass
    finally:
        sock.close()


def cmd_delete(a):
    common.output(common.call("DELETE", f"/services/{a.id}"))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    ls = sub.add_parser("list", help="分页列出服务")
    ls.add_argument("--page", type=int, default=1)
    ls.add_argument("--limit", type=int, default=20)
    common.add_common_args(ls)

    cr = sub.add_parser("create", help="创建常驻服务")
    cr.add_argument("agent_id")
    cr.add_argument("--name", required=True)
    cr.add_argument("--command", required=True)
    cr.add_argument("--args", help="逗号分隔参数")
    cr.add_argument("--restart-policy", choices=["no", "always"], default="no")
    common.add_common_args(cr)

    up = sub.add_parser("update", help="更新服务")
    up.add_argument("id")
    up.add_argument("--name", required=True)
    up.add_argument("--command", required=True)
    up.add_argument("--args")
    up.add_argument("--restart-policy", choices=["no", "always"])
    common.add_common_args(up)

    for action in ("start", "stop", "restart"):
        s = sub.add_parser(action, help=f"{action} 服务")
        s.add_argument("id")
        common.add_common_args(s)

    lg = sub.add_parser("logs", help="服务日志快照")
    lg.add_argument("id")
    common.add_common_args(lg)

    tl = sub.add_parser("tail", help="服务日志实时流")
    tl.add_argument("id")
    common.add_common_args(tl)

    de = sub.add_parser("delete", help="删除服务（running 需先 stop）")
    de.add_argument("id")
    common.add_common_args(de)

    a = p.parse_args()
    common.apply_common_args(a)
    dispatch = {"list": cmd_list, "create": cmd_create, "update": cmd_update,
                "start": cmd_simple, "stop": cmd_simple, "restart": cmd_simple,
                "logs": cmd_logs, "tail": cmd_tail, "delete": cmd_delete}
    if a.cmd in ("start", "stop", "restart"):
        a.action = a.cmd
    dispatch[a.cmd](a)


if __name__ == "__main__":
    main()
