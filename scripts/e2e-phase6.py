#!/usr/bin/env python3
"""Phase 6 e2e：交互会话终端（WebSocket + PTY）。"""

import asyncio
import os
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from e2e_helpers import (
    GRPC_ADDR,
    HTTP_ADDR,
    http_json,
    login,
    start_background,
    start_postgres,
    stop_processes,
    wait_for,
    wait_server,
)

AGENT_ID = "phase6-agent"
MARKER = "PHASE6_MARKER_7711"


def agent_online(token: str) -> bool:
    agents = http_json("GET", "/agents", token=token)["agents"]
    return any(a["id"] == AGENT_ID and a.get("online") for a in agents)


def service_status(token: str, sid: str) -> str:
    services = http_json("GET", "/services", token=token)["services"]
    svc = next((s for s in services if s["id"] == sid), None)
    return svc["status"] if svc else "?"


async def run_terminal(token: str) -> bytes:
    import websockets

    uri = f"ws://{HTTP_ADDR}/api/v1/agents/{AGENT_ID}/terminal?token={token}"
    async with websockets.connect(uri) as ws:
        await asyncio.sleep(1.5)  # 等 shell prompt
        await ws.send(f"echo {MARKER}\r")
        out = b""
        deadline = time.time() + 8
        while time.time() < deadline:
            try:
                m = await asyncio.wait_for(ws.recv(), timeout=1.0)
            except asyncio.TimeoutError:
                continue
            out += m if isinstance(m, bytes) else m.encode()
            if MARKER.encode() in out:
                break
        assert MARKER.encode() in out, f"未收到命令输出: {out[:200]!r}"
        return out


async def run_multi_terminal(token: str) -> None:
    import websockets

    uri = f"ws://{HTTP_ADDR}/api/v1/agents/{AGENT_ID}/terminal?token={token}"

    async def session(marker: str) -> bool:
        async with websockets.connect(uri) as ws:
            await asyncio.sleep(1.0)
            await ws.send(f"echo {marker}\r")
            out = b""
            deadline = time.time() + 8
            while time.time() < deadline:
                try:
                    m = await asyncio.wait_for(ws.recv(), timeout=1.0)
                except asyncio.TimeoutError:
                    continue
                out += m if isinstance(m, bytes) else m.encode()
                if marker.encode() in out:
                    return True
            return marker.encode() in out

    ok = await asyncio.gather(session("MULTI_A_9921"), session("MULTI_B_3344"))
    assert all(ok), f"多会话失败: {ok}"
    print("✓ 多会话通过（两个并发 WS 各自独立回显）")


async def run_idle_timeout(token: str) -> None:
    import websockets

    uri = f"ws://{HTTP_ADDR}/api/v1/agents/{AGENT_ID}/terminal?token={token}"
    t0 = time.time()
    try:
        async with websockets.connect(uri) as ws:
            while True:
                try:
                    await asyncio.wait_for(ws.recv(), timeout=10)
                except asyncio.TimeoutError:
                    break
    except Exception:
        pass  # server 主动 Close 会抛 ConnectionClosed，属预期
    elapsed = time.time() - t0
    assert elapsed < 9, f"空闲超时未触发（耗时 {elapsed:.1f}s）"
    print(f"✓ 会话空闲超时通过（{elapsed:.1f}s 后自动关闭）")


def test_service(token: str) -> None:
    svc = http_json(
        "POST", "/services",
        {
            "agent_id": AGENT_ID,
            "name": "svc-e2e",
            "command": "sh",
            "args": ["-c", "while true; do echo svc-tick; sleep 1; done"],
            "restart_policy": "no",
        },
        token,
    )["service"]
    sid = svc["id"]

    http_json("POST", f"/services/{sid}/start", token=token)
    assert wait_for(lambda: service_status(token, sid) == "running", timeout=10), "服务未 running"

    time.sleep(2)
    log = http_json("GET", f"/services/{sid}/logs", token=token)["log"]
    assert "svc-tick" in log, f"日志缺少 svc-tick: {log[:200]!r}"

    http_json("POST", f"/services/{sid}/stop", token=token)
    assert wait_for(lambda: service_status(token, sid) == "stopped", timeout=10), "服务未 stopped"

    http_json("POST", f"/services/{sid}/restart", token=token)
    assert wait_for(lambda: service_status(token, sid) == "running", timeout=10), "服务未 restart"
    print("✓ 常驻服务管理通过（start→running→日志→stop→restart）")


def test_files(token: str) -> None:
    entries = http_json("POST", "/files/list", {"agent_id": AGENT_ID, "path": "/tmp"}, token)["entries"]
    assert len(entries) > 0, "列目录为空"
    print(f"✓ 列目录通过（/tmp 下 {len(entries)} 项）")

    tmpdir = tempfile.mkdtemp()
    for i in range(3):
        local = os.path.join(tmpdir, f"up-{i}.txt")
        with open(local, "w") as f:
            f.write(f"file-content-{i}\n")
        up = http_json("POST", "/files/upload", {
            "agent_id": AGENT_ID,
            "local_path": local,
            "remote_path": f"/tmp/helm-up-{i}.txt",
        }, token)
        assert up["checksum_ok"], f"上传校验失败 {i}"

    for i in range(3):
        local = os.path.join(tmpdir, f"down-{i}.txt")
        down = http_json("POST", "/files/download", {
            "agent_id": AGENT_ID,
            "remote_path": f"/tmp/helm-up-{i}.txt",
            "local_path": local,
        }, token)
        assert down["checksum_ok"], f"下载校验失败 {i}"
        with open(local) as f:
            assert f.read() == f"file-content-{i}\n", f"下载内容不符 {i}"
    print("✓ 批量上传下载通过（3 文件往返 + checksum）")


def test_process_net(token: str) -> None:
    procs = http_json("POST", "/processes/list", {"agent_id": AGENT_ID}, token)["processes"]
    assert len(procs) > 0, "进程列表为空"
    print(f"✓ 进程列表通过（{len(procs)} 个进程）")

    net = http_json("POST", "/net/info", {"agent_id": AGENT_ID}, token)
    assert net["hostname"], "hostname 为空"
    print(f"✓ 网络信息通过（hostname={net['hostname']}，{len(net['interfaces'])} 个接口）")

    # 起一个前台 sleep，从进程列表定位它再 kill
    http_json("POST", "/exec", {"agent_id": AGENT_ID, "command": "sleep", "args": ["300"]}, token)
    time.sleep(1.5)
    procs = http_json("POST", "/processes/list", {"agent_id": AGENT_ID}, token)["processes"]
    sleep_procs = [p for p in procs if "sleep" in p["name"]]
    assert sleep_procs, "未找到 sleep 进程"
    pid = sleep_procs[-1]["pid"]
    kill = http_json("POST", "/processes/kill", {"agent_id": AGENT_ID, "pid": pid}, token)
    assert kill["ok"], f"kill 失败: {kill}"

    kill2 = http_json("POST", "/processes/kill", {"agent_id": AGENT_ID, "pid": 999999}, token)
    assert not kill2["ok"], "kill 不存在 pid 应返回 false"
    print("✓ 进程 kill 通过（真实 pid + 不存在 pid）")


def test_group_tags(token: str) -> None:
    hosts = http_json("GET", "/hosts", token=token)["hosts"]
    assert hosts, "无主机"
    hid = hosts[0]["id"]

    http_json("POST", f"/hosts/{hid}/tags", {"tags": ["e2e-group", "prod"]}, token)
    filtered = http_json("GET", "/hosts?tag=e2e-group", token=token)["hosts"]
    assert any(h["id"] == hid for h in filtered), "标签过滤未命中"

    empty = http_json("GET", "/hosts?tag=nonexistent-tag", token=token)["hosts"]
    assert empty == [], "不存在的标签应返回空"
    print("✓ 分组/标签通过（设置标签 + 按标签过滤）")


def main() -> None:
    os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
    procs = []
    try:
        print("==> 启动 Postgres")
        start_postgres()
        print("==> 构建")
        subprocess.run(["cargo", "build", "-p", "helm-server", "-p", "helm-agent"], check=True)

        print("==> 启动 Server")
        procs.append(
            start_background(
                ["./target/debug/helm-server", "--http-addr", HTTP_ADDR, "--grpc-addr", GRPC_ADDR, "--session-idle-timeout-secs", "6"],
                "/tmp/helm-phase6-server.log",
            )
        )
        wait_server()

        print("==> 启动 Agent")
        procs.append(
            start_background(
                [
                    "./target/debug/helm-agent",
                    "--agent-id", AGENT_ID,
                    "--server-addr", f"http://{GRPC_ADDR}",
                    "--token", "dev-token-change-me",
                ],
                "/tmp/helm-phase6-agent.log",
            )
        )
        token = login()
        assert wait_for(lambda: agent_online(token), timeout=15), "agent 未上线"

        print("==> WebSocket 会话终端")
        out = asyncio.run(run_terminal(token))
        print(f"✓ 会话终端通过（命令输出回传）: {out[:120]!r}")

        print("==> 会话多开 + 空闲超时")
        asyncio.run(run_multi_terminal(token))
        asyncio.run(run_idle_timeout(token))

        print("==> 常驻服务管理")
        test_service(token)

        print("==> 文件管理")
        test_files(token)

        print("==> 进程管理 + 网络信息")
        test_process_net(token)

        print("==> 分组/标签管理")
        test_group_tags(token)
    finally:
        stop_processes(procs)


if __name__ == "__main__":
    main()
