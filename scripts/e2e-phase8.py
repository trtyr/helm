#!/usr/bin/env python3
"""Phase 8 e2e：CRUD 补全 + 实时流（WebSocket 三个端点）。"""

import asyncio
import os
import subprocess
import sys
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

AGENT_ID = "phase8-agent"


def agent_online(token: str) -> bool:
    agents = http_json("GET", "/agents", token=token)["agents"]
    return any(a["id"] == AGENT_ID and a.get("online") for a in agents)


def test_crud(token: str) -> None:
    # hosts：创建 → PUT 改 → 分页 → DELETE
    host = http_json(
        "POST", "/hosts",
        {"hostname": "e2e-crud-host", "conn_mode": "reverse", "addr": "", "tags": ["crud"]},
        token,
    )["host"]
    hid = host["id"]
    upd = http_json(
        "PUT", f"/hosts/{hid}",
        {
            "hostname": "e2e-crud-renamed",
            "conn_mode": "forward",
            "addr": "1.2.3.4:50052",
            "tags": ["crud", "prod"],
            "os": "linux",
            "arch": "x86_64",
            "platform": "linux-x86_64",
        },
        token,
    )
    assert upd["host"]["hostname"] == "e2e-crud-renamed", f"hosts PUT 失败: {upd}"
    paged = http_json("GET", "/hosts?page=1&limit=5", token=token)
    assert "hosts" in paged, "hosts 分页失败"
    http_json("DELETE", f"/hosts/{hid}", token=token)
    hosts = http_json("GET", "/hosts", token=token)["hosts"]
    assert all(h["id"] != hid for h in hosts), "hosts DELETE 后仍存在"

    # services：创建 → PUT 改 → 分页 → DELETE
    svc = http_json(
        "POST", "/services",
        {"agent_id": AGENT_ID, "name": "e2e-crud-svc", "command": "sh", "args": [], "restart_policy": "no"},
        token,
    )["service"]
    sid = svc["id"]
    http_json(
        "PUT", f"/services/{sid}",
        {"name": "e2e-crud-svc2", "command": "sh", "args": ["-c", "true"], "restart_policy": "no"},
        token,
    )
    svc_page = http_json("GET", "/services?page=1&limit=10", token=token)
    assert "services" in svc_page, "services 分页失败"
    http_json("DELETE", f"/services/{sid}", token=token)

    # listeners：创建 → PUT 改 → DELETE
    ln = http_json("POST", "/listeners", {"name": "e2e-crud-ln", "addr": "127.0.0.1:15920"}, token)["listener"]
    lid = ln["id"]
    http_json(
        "PUT", f"/listeners/{lid}",
        {"name": "e2e-crud-ln2", "addr": "127.0.0.1:15920", "proto": "grpc", "auth": ""},
        token,
    )
    http_json("DELETE", f"/listeners/{lid}", token=token)

    # agents 详情
    agents = http_json("GET", "/agents", token=token)["agents"]
    assert agents, "无 agent"
    aid = agents[0]["id"]
    detail = http_json("GET", f"/agents/{aid}", token=token)
    assert detail["agent"]["id"] == aid, f"agents 详情失败: {detail}"

    # audit / alerts / jobs 分页
    audit = http_json("GET", "/audit?page=1&limit=10", token=token)
    assert "audit" in audit, "audit 分页失败"
    alerts = http_json("GET", "/alerts?page=1&limit=10", token=token)
    assert "alerts" in alerts, "alerts 分页失败"
    jobs = http_json("GET", "/jobs?page=1&limit=10", token=token)
    assert "jobs" in jobs, "jobs 分页失败"
    print("✓ CRUD 补全通过（hosts/services/listeners 增改删 + agents 详情 + 5 处列表分页）")


async def recv_until(ws, needle: bytes, timeout: float) -> bytes:
    out = b""
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            m = await asyncio.wait_for(ws.recv(), timeout=1.0)
        except asyncio.TimeoutError:
            continue
        out += m if isinstance(m, bytes) else m.encode()
        if needle in out:
            return out
    return out


async def test_job_stream(token: str) -> None:
    import websockets

    job = http_json(
        "POST", "/exec",
        {"agent_id": AGENT_ID, "command": "sh", "args": ["-c", "sleep 2; echo JOB_STREAM_MARKER_8821"]},
        token,
    )
    jid = job["job_id"]
    uri = f"ws://{HTTP_ADDR}/api/v1/jobs/{jid}/stream?token={token}"
    async with websockets.connect(uri) as ws:
        out = await recv_until(ws, b"JOB_STREAM_MARKER_8821", timeout=8)
    assert b"JOB_STREAM_MARKER_8821" in out, f"job 流未收到输出: {out[:200]!r}"
    print("✓ job 输出流通过")


async def test_service_log_stream(token: str) -> None:
    import websockets

    svc = http_json(
        "POST", "/services",
        {
            "agent_id": AGENT_ID,
            "name": "e2e-stream-svc",
            "command": "sh",
            "args": ["-c", "while true; do echo SVC_LOG_MARKER_7733; sleep 1; done"],
            "restart_policy": "no",
        },
        token,
    )["service"]
    sid = svc["id"]
    http_json("POST", f"/services/{sid}/start", token=token)
    uri = f"ws://{HTTP_ADDR}/api/v1/services/{sid}/logs/stream?token={token}"
    async with websockets.connect(uri) as ws:
        out = await recv_until(ws, b"SVC_LOG_MARKER_7733", timeout=10)
    http_json("POST", f"/services/{sid}/stop", token=token)
    assert b"SVC_LOG_MARKER_7733" in out, f"服务日志流未收到: {out[:200]!r}"
    print("✓ 服务日志 tail-f 流通过")


async def test_metrics_stream(token: str) -> None:
    import websockets

    uri = f"ws://{HTTP_ADDR}/api/v1/metrics/stream?token={token}"
    async with websockets.connect(uri) as ws:
        out = await recv_until(ws, b"cpu.usage", timeout=40)
    assert b"cpu.usage" in out, f"指标流未收到: {out[:200]!r}"
    print("✓ 指标流通过")


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
                ["./target/debug/helm-server", "--http-addr", HTTP_ADDR, "--grpc-addr", GRPC_ADDR],
                "/tmp/helm-phase8-server.log",
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
                "/tmp/helm-phase8-agent.log",
            )
        )
        token = login()
        assert wait_for(lambda: agent_online(token), timeout=15), "agent 未上线"

        print("==> CRUD 补全")
        test_crud(token)

        print("==> 实时流")
        asyncio.run(test_job_stream(token))
        asyncio.run(test_service_log_stream(token))
        asyncio.run(test_metrics_stream(token))
    finally:
        stop_processes(procs)


if __name__ == "__main__":
    main()
