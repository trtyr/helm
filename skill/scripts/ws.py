#!/usr/bin/env python3
"""极简 RFC 6455 WebSocket 客户端（纯标准库，供 streams.py 使用）。

支持：文本/二进制帧收发（客户端帧自动 mask）、分片消息聚合、ping/pong、close。
不支持：permessage-deflate 压缩。
"""

import base64
import json
import os
import socket
import ssl
import struct
import urllib.parse


class WS:
    def __init__(self, sock):
        self.sock = sock
        self._buf = b""

    @classmethod
    def connect(cls, url, timeout=15):
        """连接 ws:// 或 wss:// URL（query 参数直接写在 url 里）。"""
        p = urllib.parse.urlparse(url)
        if p.scheme == "wss":
            host = p.hostname
            port = p.port or 443
            raw = socket.create_connection((host, port), timeout=timeout)
            ctx = ssl.create_default_context()
            if os.environ.get("HELM_TLS_INSECURE"):
                ctx.check_hostname = False
                ctx.verify_mode = ssl.CERT_NONE
            sock = ctx.wrap_socket(raw, server_hostname=host)
        else:
            host = p.hostname
            port = p.port or 80
            sock = socket.create_connection((host, port), timeout=timeout)
            sock.settimeout(timeout)

        key = base64.b64encode(os.urandom(16)).decode()
        path = p.path or "/"
        if p.query:
            path += "?" + p.query
        handshake = (
            f"GET {path} HTTP/1.1\r\n"
            f"Host: {host}:{port}\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n\r\n"
        )
        sock.sendall(handshake.encode())

        # 读握手响应头（直到 \r\n\r\n）
        resp = b""
        while b"\r\n\r\n" not in resp:
            chunk = sock.recv(4096)
            if not chunk:
                raise ConnectionError("websocket handshake: connection closed")
            resp += chunk
        head, _, rest = resp.partition(b"\r\n\r\n")
        status_line = head.split(b"\r\n")[0].decode(errors="replace")
        if "101" not in status_line:
            raise ConnectionError(f"websocket handshake failed: {status_line}")
        ws = cls(sock)
        ws._buf = rest
        return ws

    # ---- 帧收发 ----

    def _recv_exact(self, n):
        while len(self._buf) < n:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise ConnectionError("connection closed")
            self._buf += chunk
        out, self._buf = self._buf[:n], self._buf[n:]
        return out

    def _read_frame(self):
        h = self._recv_exact(2)
        fin = h[0] & 0x80
        opcode = h[0] & 0x0F
        masked = h[1] & 0x80
        length = h[1] & 0x7F
        if length == 126:
            length = struct.unpack(">H", self._recv_exact(2))[0]
        elif length == 127:
            length = struct.unpack(">Q", self._recv_exact(8))[0]
        mask = self._recv_exact(4) if masked else None
        data = self._recv_exact(length)
        if mask:
            data = bytes(b ^ mask[i % 4] for i, b in enumerate(data))
        return fin, opcode, data

    def recv(self):
        """收一条完整消息（聚合分片）。返回 (opcode, bytes)；opcode 1=text 2=binary。
        收到 close 抛 ConnectionError；ping 自动 pong。"""
        while True:
            fin, opcode, data = self._read_frame()
            if opcode == 8:  # close
                self._send_frame(8, data[:2])
                raise ConnectionError("closed by server")
            if opcode == 9:  # ping -> pong
                self._send_frame(10, data)
                continue
            if opcode in (1, 2):
                while not fin:  # 聚合分片
                    fin, op, more = self._read_frame()
                    data += more
                return opcode, data
            # 其他控制帧忽略

    def recv_text(self):
        return self.recv()[1].decode("utf-8", errors="replace")

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

    def send_binary(self, data):
        self._send_frame(2, data)

    def send_json(self, obj):
        self._send_frame(1, json.dumps(obj).encode())

    def close(self):
        try:
            self._send_frame(8, b"")
            self.sock.close()
        except OSError:
            pass
