#!/usr/bin/env python3
"""指标与告警域：主机指标时序、阈值告警列表。

用法：
  python metrics.py get <host_uuid> [--limit 100]
  python metrics.py alerts [--page 1 --limit 50]
  python metrics.py watch   # 连 WS 指标实时流（Ctrl+C 退出），见 streams.py
"""

import argparse

import common


def cmd_get(a):
    common.output(common.call("GET", "/metrics", query={"host_id": a.host_id, "limit": a.limit}))


def cmd_alerts(a):
    common.output(common.call("GET", "/alerts", query={"page": a.page, "limit": a.limit}))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    g = sub.add_parser("get", help="查询主机指标（最近 N 条）")
    g.add_argument("host_id", help="主机 UUID（hosts.py list 获取）")
    g.add_argument("--limit", type=int, default=100)
    common.add_common_args(g)

    al = sub.add_parser("alerts", help="分页列出告警")
    al.add_argument("--page", type=int, default=1)
    al.add_argument("--limit", type=int, default=50)
    common.add_common_args(al)

    a = p.parse_args()
    common.apply_common_args(a)
    {"get": cmd_get, "alerts": cmd_alerts}[a.cmd](a)


if __name__ == "__main__":
    main()
