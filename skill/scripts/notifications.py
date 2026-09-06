#!/usr/bin/env python3
"""通知域：系统内通知（上线/下线/预警）列表、未读数、已读。

用法：
  python notifications.py list [--unread] [--page 1 --limit 50]
  python notifications.py unread
  python notifications.py read <notification_uuid>
  python notifications.py read-all
"""

import argparse

import common


def cmd_list(a):
    query = {"page": a.page, "limit": a.limit}
    if a.unread:
        query["unread"] = "true"
    common.output(common.call("GET", "/notifications", query=query))


def cmd_unread(a):
    common.output(common.call("GET", "/notifications/unread-count"))


def cmd_read(a):
    common.output(common.call("POST", f"/notifications/{a.id}/read"))


def cmd_read_all(a):
    common.output(common.call("POST", "/notifications/read-all"))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    ls = sub.add_parser("list", help="分页列出通知")
    ls.add_argument("--unread", action="store_true", help="只看未读")
    ls.add_argument("--page", type=int, default=1)
    ls.add_argument("--limit", type=int, default=50)
    common.add_common_args(ls)

    sub.add_parser("unread", help="未读数")
    rd = sub.add_parser("read", help="标记单条已读")
    rd.add_argument("id")
    sub.add_parser("read-all", help="全部已读")
    for s in (sub.choices["unread"], sub.choices["read"], sub.choices["read-all"]):
        common.add_common_args(s)

    a = p.parse_args()
    common.apply_common_args(a)
    {"list": cmd_list, "unread": cmd_unread, "read": cmd_read, "read-all": cmd_read_all}[a.cmd](a)


if __name__ == "__main__":
    main()
