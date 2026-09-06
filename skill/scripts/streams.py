#!/usr/bin/env python3
"""实时流域：WebSocket 订阅（job 输出 / 指标 / 通知 / 服务日志）与交互终端。

用法：
  python streams.py job <job_uuid>            # job 输出实时流（增量打印）
  python streams.py metrics                   # 指标实时流（JSON 逐条打印）
  python streams.py notifications             # 通知实时流（JSON 逐条打印）
  python streams.py service-logs <service_uuid>
  python streams.py terminal <agent_id>       # 交互式 PTY 终端（Ctrl+] 退出）

终端快捷键：Ctrl+] 或 Ctrl+C 退出；进入后 Ctrl+R 重绘当前窗口大小。
"""

import argparse
import json
import os
import sys

import common
import ws as ws_mod

FRAME_INPUT = 0x01
FRAME_RESIZE = 0x02


def ws_url(path):
    from urllib.parse import urlencode
    token = common.get_token()
    return common.base_url().replace("http", "ws", 1) + "/api/v1" + path + \
        "?" + urlencode({"token": token})


def pump(sock, as_json=False):
    """收消息直到断开；二进制帧按文本打印（as_json 则解析后缩进）。"""
    try:
        while True:
            _, data = sock.recv()
            text = data.decode("utf-8", errors="replace")
            if as_json:
                try:
                    print(json.dumps(json.loads(text), ensure_ascii=False, indent=2), flush=True)
                except ValueError:
                    print(text, end="", flush=True)
            else:
                print(text, end="", flush=True)
    except (ConnectionError, KeyboardInterrupt):
        pass
    finally:
        sock.close()


def cmd_stream(a):
    paths = {
        "job": f"/jobs/{a.id}/stream",
        "service-logs": f"/services/{a.id}/logs/stream",
    }
    path = paths.get(a.cmd)
    if path is None:  # metrics / notifications 无 id
        path = f"/{a.cmd}/stream"
    sock = ws_mod.WS.connect(ws_url(path))
    pump(sock, as_json=a.cmd in ("metrics", "notifications"))


def terminal_size():
    import shutil
    size = shutil.get_terminal_size((80, 24))
    return size.columns, size.lines


def cmd_terminal(a):
    """交互终端：本机 stdin → WS（0x01 帧），WS 二进制 → 本机 stdout 直写。"""
    from urllib.parse import urlencode
    token = common.get_token()
    url = common.base_url().replace("http", "ws", 1) + \
        f"/api/v1/agents/{a.agent_id}/terminal?" + \
        urlencode({"token": token, "cols": a.cols or 80, "rows": a.rows or 24})
    sock = ws_mod.WS.connect(url)

    if os.name == "nt":
        import msvcrt
        stdin_fd = None
    else:
        import termios
        import tty
        stdin_fd = sys.stdin.fileno()
        old_attrs = termios.tcgetattr(stdin_fd)
        tty.setraw(stdin_fd)

    print(f"[connected to {a.agent_id}; Ctrl+] to exit]", file=sys.stderr)

    def resize_payload():
        cols, rows = terminal_size()
        return bytes([FRAME_RESIZE]) + json.dumps([cols, rows]).encode()

    try:
        while True:
            # 先尽力收一轮输出
            try:
                sock.sock.settimeout(0.05)
                _, data = sock.recv()
                sys.stdout.buffer.write(data)
                sys.stdout.buffer.flush()
                continue
            except (TimeoutError, OSError):
                pass
            sock.sock.settimeout(None)

            # 读本地输入
            if os.name == "nt":
                if not msvcrt.kbhit():
                    continue
                ch = msvcrt.getwch()
                if ch in ("\x1d",):  # Ctrl+]
                    break
                if ch == "\x00" or ch == "\xe0":  # 功能键前缀，吞掉第二码
                    msvcrt.getwch()
                    continue
                data = ch.encode("utf-8", errors="replace")
            else:
                data = os.read(stdin_fd, 1024)
                if b"\x1d" in data or b"\x03" in data:
                    break
            sock.send_binary(bytes([FRAME_INPUT]) + data)
    except (ConnectionError, OSError):
        pass
    finally:
        if stdin_fd is not None:
            import termios
            termios.tcsetattr(stdin_fd, termios.TCSADRAIN, old_attrs)
        sock.close()
        print("\n[terminal closed]", file=sys.stderr)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    for name, help_ in (("job", "job 输出实时流"), ("service-logs", "服务日志实时流")):
        s = sub.add_parser(name, help=help_)
        s.add_argument("id")
        common.add_common_args(s)

    for name in ("metrics", "notifications"):
        s = sub.add_parser(name, help=f"{name} 实时流")
        common.add_common_args(s)

    tm = sub.add_parser("terminal", help="交互式 PTY 终端")
    tm.add_argument("agent_id")
    tm.add_argument("--cols", type=int)
    tm.add_argument("--rows", type=int)
    common.add_common_args(tm)

    a = p.parse_args()
    common.apply_common_args(a)
    {"job": cmd_stream, "service-logs": cmd_stream,
     "metrics": cmd_stream, "notifications": cmd_stream,
     "terminal": cmd_terminal}[a.cmd](a)


if __name__ == "__main__":
    main()
