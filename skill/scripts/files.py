#!/usr/bin/env python3
"""文件域：下发/取回文件、列目录。

注意：`local_path` 是 **Server 所在机器**上的路径（文件经由 Server 中转），
不是运行本脚本的机器——脚本与 Server 不同机时，先把文件放到 Server 侧再调用。

用法：
  python files.py upload <agent_id> --local <server路径> --remote <目标机路径>
  python files.py download <agent_id> --remote <目标机路径> --local <server路径>
  python files.py ls <agent_id> [path]
"""

import argparse

import common


def cmd_upload(a):
    data = common.call("POST", "/files/upload", {
        "agent_id": a.agent_id, "local_path": a.local, "remote_path": a.remote,
    })
    ok = data.get("checksum_ok")
    if ok is False:
        common.die(f"传输完成但 sha256 校验失败：{data}")
    common.output(data)


def cmd_download(a):
    data = common.call("POST", "/files/download", {
        "agent_id": a.agent_id, "remote_path": a.remote, "local_path": a.local,
    })
    ok = data.get("checksum_ok")
    if ok is False:
        common.die(f"传输完成但 sha256 校验失败：{data}")
    common.output(data)


def cmd_ls(a):
    path = a.path or "/"
    data = common.call("POST", "/files/list", {"agent_id": a.agent_id, "path": path})
    if a.raw:
        for e in data.get("entries", []):
            kind = "d" if e["is_dir"] else "-"
            print(f"{kind} {e['size']:>12} {e['name']}")
    else:
        common.output(data)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    up = sub.add_parser("upload", help="下发文件到目标机（Server → Agent）")
    up.add_argument("agent_id")
    up.add_argument("--local", required=True, help="Server 侧源路径")
    up.add_argument("--remote", required=True, help="目标机路径")
    common.add_common_args(up)

    dn = sub.add_parser("download", help="取回文件（Agent → Server）")
    dn.add_argument("agent_id")
    dn.add_argument("--remote", required=True, help="目标机路径")
    dn.add_argument("--local", required=True, help="Server 侧保存路径")
    common.add_common_args(dn)

    ls = sub.add_parser("ls", help="列目录")
    ls.add_argument("agent_id")
    ls.add_argument("path", nargs="?", default="/")
    ls.add_argument("--raw", action="store_true", help="简洁输出（每行一个条目）")
    common.add_common_args(ls)

    a = p.parse_args()
    common.apply_common_args(a)
    {"upload": cmd_upload, "download": cmd_download, "ls": cmd_ls}[a.cmd](a)


if __name__ == "__main__":
    main()
