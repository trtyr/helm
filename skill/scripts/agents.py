#!/usr/bin/env python3
"""Agent 域：Agent 列表、详情、注销、卸载、标签。

用法：
  python agents.py list
  python agents.py get <agent_id>
  python agents.py tags <agent_id> --tags a,b       # 更新关联主机标签
  python agents.py deregister <agent_id>            # 注销（删 agent 行 + 孤儿主机软删）
  python agents.py uninstall <agent_id> [--keep-binary]  # 下发卸载（SelfDestruct）
"""

import argparse

import common


def cmd_list(a):
    common.output(common.call("GET", "/agents"))


def cmd_get(a):
    common.output(common.call("GET", f"/agents/{a.id}"))


def cmd_tags(a):
    common.output(common.call("PUT", f"/agents/{a.id}/tags", {"tags": common.parse_list(a.tags)}))


def cmd_deregister(a):
    common.output(common.call("DELETE", f"/agents/{a.id}"))


def cmd_uninstall(a):
    common.output(common.call("POST", f"/agents/{a.id}/uninstall",
                              {"remove_binary": not a.keep_binary}))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    ls = sub.add_parser("list", help="列出已注册 Agent")
    common.add_common_args(ls)

    get = sub.add_parser("get", help="Agent 详情（含在线状态）")
    get.add_argument("id")
    common.add_common_args(get)

    tg = sub.add_parser("tags", help="更新 Agent 关联主机标签")
    tg.add_argument("id")
    tg.add_argument("--tags", required=True)
    common.add_common_args(tg)

    de = sub.add_parser("deregister", help="注销 Agent")
    de.add_argument("id")
    common.add_common_args(de)

    un = sub.add_parser("uninstall", help="下发卸载指令（危险：目标机自删）")
    un.add_argument("id")
    un.add_argument("--keep-binary", action="store_true", help="保留二进制（仅停服务清自启）")
    common.add_common_args(un)

    a = p.parse_args()
    common.apply_common_args(a)
    {"list": cmd_list, "get": cmd_get, "tags": cmd_tags,
     "deregister": cmd_deregister, "uninstall": cmd_uninstall}[a.cmd](a)


if __name__ == "__main__":
    main()
