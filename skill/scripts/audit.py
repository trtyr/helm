#!/usr/bin/env python3
"""审计域：审计日志分页查询。

用法：
  python audit.py list [--page 1 --limit 50]
"""

import argparse

import common


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ls = p.add_subparsers(dest="cmd", required=True).add_parser("list", help="分页列出审计日志")
    ls.add_argument("--page", type=int, default=1)
    ls.add_argument("--limit", type=int, default=50)
    common.add_common_args(ls)

    a = p.parse_args()
    common.apply_common_args(a)
    common.output(common.call("GET", "/audit", query={"page": a.page, "limit": a.limit}))


if __name__ == "__main__":
    main()
