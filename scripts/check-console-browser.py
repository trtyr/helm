#!/usr/bin/env python3
"""控制台真浏览器冒烟验证：CDP 直驱本地 Chromium（headless），零第三方依赖。

真跑一遍用户路径：注入登录态 → 主机列表 → 主机概览（方案 B 信息卡）→
内存扫描页（点击扫描按钮，验证 newScanId/WS 链路无运行时错误）→ 设置页
（API 凭证块）。收集 window error / unhandledrejection / console.error，
存在 'not defined' 类运行时错误即退出码 1。

前置：helm-server 运行中（HELM_URL）、vite dev 运行中（CONSOLE_URL）、
本地 Chromium（PLAYWRIGHT_CHROME 或 ms-playwright 默认路径）。
"""

import json
import os
import socket
import ssl
import struct
import subprocess
import sys
import tempfile
import time
import urllib.request
import urllib.parse

HELM_URL = os.environ.get("HELM_URL", "http://127.0.0.1:18081")
CONSOLE = os.environ.get("CONSOLE_URL", "http://127.0.0.1:5180")
DEBUG_PORT = 9233
CHROME = os.environ.get(
    "PLAYWRIGHT_CHROME",
    os.path.expanduser("~/AppData/Local/ms-playwright/chromium-1234/chrome-win64/chrome.exe"),
)


# --- 最小 WebSocket 客户端（客户端帧 + ping/pong，足够 CDP 会话） ---
class WS:
    def __init__(self, sock):
        self.sock = sock
        self._buf = b""

    @classmethod
    def connect(cls, url, timeout=15):
        p = urllib.parse.urlparse(url)
        raw = socket.create_connection((p.hostname, p.port or 80), timeout=timeout)
        key = __import__("base64").b64encode(os.urandom(16)).decode()
        path = f"{p.path}?{p.query}" if p.query else p.path
        req = (
            f"GET {path} HTTP/1.1\r\nHost: {p.hostname}:{p.port}\r\n"
            "Upgrade: websocket\r\nConnection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
        raw.sendall(req.encode())
        resp = b""
        while b"\r\n\r\n" not in resp:
            chunk = raw.recv(4096)
            if not chunk:
                raise ConnectionError("handshake closed")
            resp += chunk
        status = resp.split(b"\r\n", 1)[0]
        if b"101" not in status:
            raise ConnectionError(f"handshake failed: {status}")
        return cls(raw)

    def _read_frame(self):
        head = self._recv_exact(2)
        fin, opcode = head[0] & 0x80, head[0] & 0x0F
        n, mask = head[1] & 0x7F, head[1] & 0x80
        if n == 126:
            n = struct.unpack(">H", self._recv_exact(2))[0]
        elif n == 127:
            n = struct.unpack(">Q", self._recv_exact(8))[0]
        mask_key = self._recv_exact(4) if mask else None
        data = self._recv_exact(n)
        if mask_key:
            data = bytes(b ^ mask_key[i % 4] for i, b in enumerate(data))
        return fin, opcode, data

    def _recv_exact(self, n):
        while len(self._buf) < n:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise ConnectionError("closed")
            self._buf += chunk
        out, self._buf = self._buf[:n], self._buf[n:]
        return out

    def recv(self):
        while True:
            fin, opcode, data = self._read_frame()
            if opcode == 8:
                raise ConnectionError("closed by server")
            if opcode == 9:
                self._send_frame(10, data)
                continue
            if opcode in (1, 2):
                while not fin:
                    fin, op, more = self._read_frame()
                    data += more
                return opcode, data

    def recv_text(self, timeout=None):
        if timeout is not None:
            self.sock.settimeout(timeout)
        try:
            return self.recv()[1].decode("utf-8", errors="replace")
        except (socket.timeout, TimeoutError):
            return None

    def _send_frame(self, opcode, payload):
        mask = os.urandom(4)
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        h = bytearray([0x80 | opcode])
        n = len(payload)
        if n < 126:
            h.append(0x80 | n)
        elif n < 65536:
            h.append(0x80 | 126)
            h += struct.pack(">H", n)
        else:
            h.append(0x80 | 127)
            h += struct.pack(">Q", n)
        self.sock.sendall(bytes(h) + mask + masked)

    def send_json(self, obj):
        self._send_frame(1, json.dumps(obj).encode())

    def close(self):
        try:
            self._send_frame(8, b"\x00\x00")
            self.sock.close()
        except Exception:
            pass


def api_json(method: str, path: str, body=None, token=None) -> dict:
    req = urllib.request.Request(f"{HELM_URL}/api/v1{path}", method=method)
    req.add_header("Content-Type", "application/json")
    if body is not None:
        req.data = json.dumps(body).encode()
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read().decode())


class Cdp:
    def __init__(self, ws):
        self.ws = ws
        self.seq = 0
        self.console_errors: list[str] = []

    def cmd(self, method, params=None, timeout=15):
        self.seq += 1
        mid = self.seq
        self.ws.send_json({"id": mid, "method": method, "params": params or {}})
        result = None
        while True:
            msg = self.ws.recv_text(timeout=timeout)
            if msg is None:
                raise TimeoutError(f"CDP {method} 等待超时")
            m = json.loads(msg)
            if m.get("id") == mid:
                result = m.get("result", {})
                break
            self._collect_event(m)
        return result

    def _collect_event(self, m):
        method = m.get("method", "")
        if method == "Runtime.exceptionThrown":
            d = m["params"]["exceptionDetails"]
            text = d.get("exception", {}).get("description") or d.get("text", "")
            self.console_errors.append(text)
        elif method == "Log.entryAdded" and m["params"]["entry"].get("level") == "error":
            self.console_errors.append(m["params"]["entry"].get("text", ""))

    def eval_js(self, expr, await_promise=False):
        r = self.cmd(
            "Runtime.evaluate",
            {"expression": expr, "returnByValue": True, "awaitPromise": await_promise},
        )
        if r.get("exceptionDetails"):
            raise AssertionError(f"页面执行出错: {r['exceptionDetails'].get('text')}")
        return r.get("result", {}).get("value")

    def nav(self, url, settle=2.5):
        self.cmd("Page.navigate", {"url": url})
        time.sleep(settle)

    def wait_text(self, marker, tries=12):
        for _ in range(tries):
            try:
                if marker in (self.eval_js("document.body.innerText") or ""):
                    return
            except (TimeoutError, ConnectionError):
                pass
            time.sleep(1)
        raise AssertionError(f"页面未见「{marker}」")


def main() -> None:
    if not os.path.exists(CHROME):
        sys.exit(f"Chromium 不存在: {CHROME}")
    jwt = api_json("POST", "/auth/login", {"username": "admin", "password": "admin123"})["token"]
    hosts = api_json("GET", "/hosts?limit=50", token=jwt)["hosts"]
    target = next((h for h in hosts if h["online"]), None)
    assert target, "没有在线主机可供验证"
    print(f"[0] 目标主机 {target['hostname']} ({target['id']})")

    profile = tempfile.mkdtemp(prefix="cdp-console-")
    chrome = subprocess.Popen(
        [
            CHROME,
            f"--remote-debugging-port={DEBUG_PORT}",
            f"--user-data-dir={profile}",
            "--headless=new",
            "--no-first-run",
            "--window-size=1440,900",
            "about:blank",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    ws = None
    try:
        ws_url = None
        for _ in range(20):
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{DEBUG_PORT}/json/list", timeout=2) as r:
                    tabs = json.loads(r.read().decode())
                pages = [t for t in tabs if t.get("type") == "page"]
                if pages:
                    ws_url = pages[0]["webSocketDebuggerUrl"]
                    break
            except Exception:
                pass
            time.sleep(0.5)
        assert ws_url, "CDP 端口未就绪"
        ws = WS.connect(ws_url)
        cdp = Cdp(ws)
        cdp.cmd("Runtime.enable")
        cdp.cmd("Page.enable")
        cdp.cmd("Log.enable")
        # 运行时错误捕获：window error + unhandledrejection
        cdp.eval_js(
            "window.__errs=[];window.addEventListener('error',e=>window.__errs.push(String(e.message||e)));"
            "window.addEventListener('unhandledrejection',e=>window.__errs.push('unhandled: '+String(e.reason)));1"
        )

        # [1] 注入登录态 → 主机列表
        cdp.nav(f"{CONSOLE}/login")
        cdp.eval_js(f"localStorage.setItem('helm-console.token', '{jwt}');1")
        cdp.nav(f"{CONSOLE}/hosts", settle=3.5)
        cdp.wait_text(target["hostname"])
        print("[1] 主机列表渲染 OK")

        # [2] 概览页（方案 B 信息卡）
        cdp.nav(f"{CONSOLE}/hosts/{target['id']}/overview")
        cdp.wait_text("系统版本")
        cdp.wait_text("内核")
        print("[2] 概览主机信息卡 OK")

        # [3] 内存扫描页：渲染 + 点击扫描按钮（原 randomUUID 崩点）
        cdp.nav(f"{CONSOLE}/hosts/{target['id']}/memscan")
        cdp.wait_text("内存字符串扫描")
        cdp.eval_js(
            "const b=[...document.querySelectorAll('button')].find(x=>x.textContent.trim()==='扫描');"
            "if(!b) throw new Error('扫描按钮未找到'); b.click(); 1",
            await_promise=False,
        )
        time.sleep(6)
        errs = ""
        try:
            errs = cdp.eval_js("(window.__errs||[]).join(';')") or ""
        except AssertionError:
            pass  # 页面上下文异常时以 CDP 事件收集为准
        all_errs = errs + "\n" + "\n".join(cdp.console_errors)
        assert "randomUUID" not in all_errs and "not defined" not in all_errs, f"内存扫描运行时错误: {all_errs[:800]}"
        print("[3] 内存扫描页点击扫描 OK（无运行时错误）")

        # [4] 设置页：API 凭证块 + 签发对话框
        cdp.nav(f"{CONSOLE}/settings")
        cdp.wait_text("API 凭证")
        cdp.eval_js(
            "[...document.querySelectorAll('button')].find(x=>x.textContent.includes('签发凭证')).click();1"
        )
        cdp.wait_text("Scopes（全不选 = 全功能）")
        print("[4] 设置页 API 凭证块 OK")

        browser_errors = [e for e in cdp.console_errors if e.strip()]
        fatal = [e for e in browser_errors if "not defined" in e or "is not a function" in e]
        assert not fatal, f"浏览器运行时错误: {fatal}"
        if browser_errors:
            print(f"（{len(browser_errors)} 条非致命 console error: {browser_errors[:5]}）")
        print("\n真浏览器验证全部通过 ✓")
    finally:
        if ws:
            ws.close()
        chrome.terminate()


if __name__ == "__main__":
    main()
