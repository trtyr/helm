#!/usr/bin/env python3
"""Phase 9 e2e：通知中心（决策 009 系统内小卡片）。

覆盖：上线通知 / 下线通知 / 冷却窗口合并 / 已读未读全流程 / WS 实时推送。
预警联动（alert 类型通知）由 server/tests/notification_test.rs 的集成测试覆盖
（InboundCtx.handle 即 gRPC 入站后的生产处理路径）——本脚本内嵌跑该测试作为 gate。
"""

import asyncio
import json
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
    psql,
    psql_value,
    start_background,
    start_postgres,
    stop_processes,
    wait_for,
    wait_server,
)

AGENT_ID = "phase9-agent"


def start_agent():
    return start_background(
        [
            "./target/debug/helm-agent",
            "--agent-id", AGENT_ID,
            "--server-addr", f"http://{GRPC_ADDR}",
            "--token", "dev-token-change-me",
        ],
        "/tmp/helm-phase9-agent.log",
    )


def agent_online(token: str) -> bool:
    agents = http_json("GET", "/agents", token=token)["agents"]
    return any(a["id"] == AGENT_ID and a.get("online") for a in agents)


def notif_count(token: str, host_id: str, kind: str | None = None) -> int:
    rows = http_json("GET", "/notifications?limit=1000", token=token)["notifications"]
    rows = [r for r in rows if r["host_id"] == host_id]
    if kind:
        rows = [r for r in rows if r["kind"] == kind]
    return len(rows)


def stop_agent(proc) -> None:
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()


async def test_ws_offline_push(token: str, agent_proc) -> None:
    """连 WS 通知流 → 杀 agent → 收到 offline 推送帧。"""
    import websockets

    uri = f"ws://{HTTP_ADDR}/api/v1/notifications/stream?token={token}"
    async with websockets.connect(uri) as ws:
        await asyncio.sleep(1)  # 订阅生效
        stop_agent(agent_proc)
        deadline = time.time() + 10
        frames = []
        while time.time() < deadline:
            try:
                m = await asyncio.wait_for(ws.recv(), timeout=1.0)
            except asyncio.TimeoutError:
                if notif_count_by_ws(frames, "offline"):
                    break
                continue
            if isinstance(m, bytes):
                frames.append(json.loads(m.decode()))
                if frames[-1].get("kind") == "offline":
                    break
        assert any(f.get("kind") == "offline" for f in frames), \
            f"WS 未收到 offline 推送: {frames!r}"
        assert any("已下线" in f.get("message", "") for f in frames), \
            f"推送消息不含下线文案: {frames!r}"
    print("✓ WS 通知实时推送通过（杀 agent → offline 帧）")


def notif_count_by_ws(frames: list, kind: str) -> bool:
    return any(f.get("kind") == kind for f in frames)


def main() -> None:
    os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
    procs = []
    agent = None
    try:
        print("==> 启动 Postgres")
        start_postgres()
        print("==> 构建")
        subprocess.run(["cargo", "build", "-p", "helm-server", "-p", "helm-agent"], check=True)

        # 干净基线：清掉历史通知，避免残留干扰计数
        psql("DELETE FROM notifications")

        print("==> 启动 Server")
        procs.append(
            start_background(
                ["./target/debug/helm-server", "--http-addr", HTTP_ADDR, "--grpc-addr", GRPC_ADDR],
                "/tmp/helm-phase9-server.log",
            )
        )
        wait_server()

        token = login()

        print("==> 1/5 上线通知")
        agent = start_agent()
        procs.append(agent)
        assert wait_for(lambda: agent_online(token), timeout=15), "agent 未上线"
        host_id = psql_value(f"SELECT host_id FROM agents WHERE id = '{AGENT_ID}'")
        assert host_id, "agents 表无 host_id"
        assert wait_for(lambda: notif_count(token, host_id, "online") == 1, timeout=10), \
            "上线通知未产生"
        note = [
            r for r in http_json("GET", "/notifications", token=token)["notifications"]
            if r["host_id"] == host_id and r["kind"] == "online"
        ][0]
        assert note["message"].endswith("已上线"), f"上线文案异常: {note['message']}"
        assert note["read"] is False, "新通知应为未读"
        print("✓ 上线通知通过")

        print("==> 2/5 冷却窗口合并（杀→重启，同 kind 窗口内一条）")
        stop_agent(agent)
        assert wait_for(lambda: notif_count(token, host_id, "offline") == 1, timeout=10), \
            "下线通知未产生"
        agent = start_agent()
        procs.append(agent)
        assert wait_for(lambda: agent_online(token), timeout=15), "agent 重启后未上线"
        # online 在 5 分钟冷却窗口内被 refresh → 仍只有一条 online
        assert notif_count(token, host_id, "online") == 1, \
            "冷却窗口内 online 通知应合并为一条"
        print("✓ 冷却窗口合并通过（online 单条 + offline 单条）")

        print("==> 3/5 已读未读全流程")
        n0 = http_json("GET", "/notifications/unread-count", token=token)["count"]
        assert n0 >= 2, f"未读数异常: {n0}"
        first = [
            r for r in http_json("GET", f"/notifications?unread=true&limit=1000", token=token)["notifications"]
            if r["host_id"] == host_id
        ][0]
        http_json("POST", f"/notifications/{first['id']}/read", token=token)
        n1 = http_json("GET", "/notifications/unread-count", token=token)["count"]
        assert n1 == n0 - 1, f"单条已读后未读数未减: {n0} -> {n1}"
        updated = http_json("POST", "/notifications/read-all", token=token)["updated"]
        assert updated >= 1, f"read-all 无更新: {updated}"
        n2 = http_json("GET", "/notifications/unread-count", token=token)["count"]
        assert n2 == 0, f"read-all 后未读数非 0: {n2}"
        # unread 过滤参数
        assert http_json("GET", "/notifications?unread=true", token=token)["notifications"] == [], \
            "unread=true 应返回空"
        print("✓ 已读未读全流程通过（unread-count / 单条已读 / read-all / unread 过滤）")

        print("==> 4/5 WS 通知实时推送")
        asyncio.run(test_ws_offline_push(token, agent))

        print("==> 5/5 预警联动 gate（集成测试：超阈值指标 → alert 通知）")
        r = subprocess.run(
            ["cargo", "test", "-p", "helm-server", "--test", "notification_test"],
            capture_output=True, text=True,
        )
        assert r.returncode == 0, f"预警联动集成测试失败:\n{r.stdout[-2000:]}\n{r.stderr[-2000:]}"
        print("✓ 预警联动通过（InboundCtx 超阈值指标 → alert 通知，集成测试全绿）")

        print("\nphase9 OK: 通知中心全链路（上线/下线/冷却合并/已读未读/WS 推送/预警联动）")
    finally:
        stop_processes(procs)
        # 清理测试残留
        psql(f"DELETE FROM hosts WHERE hostname LIKE '%{AGENT_ID}%' OR hostname LIKE 'demotestde%'")


if __name__ == "__main__":
    main()
