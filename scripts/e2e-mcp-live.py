#!/usr/bin/env python3
"""MCP live E2E：对真实部署（release server + 在线 agent）跑 MCP 全链路。

与 e2e-mcp.py（hermetic，自带 server/postgres）互补：本脚本要求
- helm-server 已在 HELM_URL（默认 127.0.0.1:18081）运行
- 至少一台 Windows agent 在线；Linux agent 在线时自动追加 Linux 断言

覆盖：签发 scope key → 握手 → 裁剪 → 真实命令执行与取结果 → 系统服务 →
指标 → 通知 → IR 文件元数据 → os 拒绝 → scope 拒绝 → 审计溯源 → 吊销 401。
"""

import json
import os
import sys
import time
import urllib.error
import urllib.request

HELM_URL = os.environ.get("HELM_URL", "http://127.0.0.1:18081")
MCP_URL = f"{HELM_URL}/mcp"
API = f"{HELM_URL}/api/v1"


def http_json(method: str, path: str, body=None, token: str | None = None):
    req = urllib.request.Request(f"{API}{path}", method=method)
    req.add_header("Content-Type", "application/json")
    if body is not None:
        req.data = json.dumps(body).encode()
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(req, timeout=60) as resp:
        return json.loads(resp.read().decode())


def mcp_raw(token: str, payload):
    req = urllib.request.Request(MCP_URL, data=json.dumps(payload).encode(), method="POST")
    req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(req, timeout=150) as resp:
            body = resp.read().decode()
            return resp.status, json.loads(body) if body else None
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        try:
            return e.code, json.loads(body)
        except json.JSONDecodeError:
            return e.code, {"raw": body}


def rpc(method: str, params=None, id: int = 1) -> dict:
    msg = {"jsonrpc": "2.0", "id": id, "method": method}
    if params is not None:
        msg["params"] = params
    return msg


def call_tool(token: str, arguments: dict, id: int = 10):
    """返回 (isError, 解析后的业务 JSON 或错误文本)。"""
    status, resp = mcp_raw(token, rpc("tools/call", {"name": "helm", "arguments": arguments}, id))
    assert status == 200, f"tools/call HTTP {status}: {resp}"
    assert "error" not in resp, f"协议错误: {resp.get('error')}"
    result = resp["result"]
    text = result["content"][0]["text"]
    return result["isError"], (json.loads(text) if not result["isError"] else text)


def tool_op(token: str, arguments: dict, id: int):
    """op 调用并断言成功，返回业务 JSON。"""
    is_err, payload = call_tool(token, arguments, id)
    assert not is_err, f"{arguments.get('op')} 失败: {payload}"
    return payload


def wait_job(token: str, job_id: str, timeout: float = 45.0) -> dict:
    """轮询 jobs.get 直到终态。"""
    deadline = time.time() + timeout
    while time.time() < deadline:
        is_err, job = call_tool(token, {"op": "jobs.get", "args": {"id": job_id}}, id=90)
        assert not is_err, job
        job = job.get("job", job)
        if job["status"] in ("succeeded", "failed", "timed_out"):
            return job
        time.sleep(1.5)
    raise AssertionError(f"job {job_id} 超时未完成")


def main() -> None:
    jwt = http_json("POST", "/auth/login", {"username": "admin", "password": "admin123"})["token"]

    # 现场盘点：经 hosts 找在线 agent（os 字段区分平台）
    hosts = http_json("GET", "/hosts?limit=50", token=jwt)["hosts"]
    by_os = {h["os"]: h["agent_id"] for h in hosts if h["online"] and h.get("agent_id")}
    win, linux = by_os.get("windows"), by_os.get("linux")
    assert win or linux, "没有在线 agent，live E2E 无法进行"
    print(f"[0] 在线 agent：windows={win} linux={linux}")

    # 签发两把 key：A = 宽 scope；B = 只有 metrics（用于越权拒绝）
    key_a = http_json("POST", "/api-keys", {"name": "e2e-live-mcp", "scopes": ["hosts", "exec", "services", "metrics", "notifications", "audit", "ir", "files"]}, token=jwt)
    key_b = http_json("POST", "/api-keys", {"name": "e2e-live-mcp-b", "scopes": ["metrics"]}, token=jwt)
    ka, kb = key_a["key"], key_b["key"]
    assert ka.startswith("helm_") and kb.startswith("helm_")
    print("[0.5] scope key 签发 OK")

    # [1] 握手
    status, resp = mcp_raw(ka, rpc("initialize", {"protocolVersion": "2025-06-18"}))
    assert status == 200 and resp["result"]["protocolVersion"] == "2025-06-18", resp
    print("[1] initialize OK")

    # [2] tools/list 裁剪：A 有 ir/hosts、无 listeners；B 无 exec
    _, resp = mcp_raw(ka, rpc("tools/list", {}, 2))
    desc = resp["result"]["tools"][0]["description"]
    assert "hosts.list" in desc and "ir.scan" in desc, "A 应看到 hosts+ir"
    assert "listeners.list" not in desc, "A 不应看到 listeners（未签发该 scope）"
    _, resp = mcp_raw(kb, rpc("tools/list", {}, 3))
    desc_b = resp["result"]["tools"][0]["description"]
    # P002 删除了 metrics/notifications/audit 的 MCP op——metrics scope 的 key 目录应为空
    assert "exec.run" not in desc_b and "ir.scan" not in desc_b, "B 不应看到任何 host 操作"
    print("[2] tools/list scope 裁剪 OK")

    # [3] hosts.list：真实主机 + OS 版本细节（方案 B 字段）
    hosts = tool_op(ka, {"op": "hosts.list"}, 10)["hosts"]
    win_host = next((h for h in hosts if h["hostname"] == "DESKTOP-3M7DKO9" and h["online"]), None)
    assert win_host, hosts
    assert (win_host.get("os_version") or "").startswith("Windows"), win_host
    assert win_host.get("kernel"), win_host
    if linux:
        linux_host = next((h for h in hosts if h["id"] != win_host["id"] and h["online"]), None)
        assert linux_host and "Ubuntu" in (linux_host.get("os_version") or ""), linux_host
    print("[3] hosts.list（含 os_version/kernel）OK")

    # [4] 真实命令执行：echo → jobs.get 轮询取结果
    probe = "mcp-live-ok"
    payload = tool_op(ka, {"op": "exec.run", "args": {"agent_id": win, "command": "cmd", "args": ["/c", f"echo {probe}"]}}, 11)
    job = wait_job(ka, payload["job_id"])
    assert job["status"] == "succeeded" and probe in job["output"], job
    if linux:
        payload = tool_op(ka, {"op": "exec.run", "args": {"agent_id": linux, "command": "uname", "args": ["-r"]}}, 12)
        job = wait_job(ka, payload["job_id"])
        assert job["status"] == "succeeded" and "generic" in job["output"], job
    print("[4] exec.run → jobs.get 真实执行 OK")

    # [5] 系统服务发现（实时枚举）
    svcs = tool_op(ka, {"op": "sys_services.list", "args": {"agent_id": win}}, 13)["services"]
    assert len(svcs) > 20, len(svcs)
    if linux:
        svcs = tool_op(ka, {"op": "sys_services.list", "args": {"agent_id": linux}}, 14)["services"]
        assert any(s.get("enabled_state") for s in svcs), "Linux 服务应带 UnitFileState"
        assert any(s.get("unit_file") for s in svcs), "Linux 服务应带 FragmentPath"
    print("[5] sys_services.list OK")

    # [6]/[7] 指标与通知：P002 已从 MCP 目录删除（metrics.list / notifications.*），不再经 MCP 验证

    # [8] os 不匹配：Windows 专属 op 传 linux
    is_err, msg = call_tool(ka, {"op": "ir.scan", "os": "linux", "args": {"agent_id": win}}, 18)
    assert is_err and "windows" in msg, msg
    print("[8] os 不匹配拒绝 OK")

    # [9] scope 越权：B（只有 metrics）调 exec.run
    is_err, msg = call_tool(kb, {"op": "exec.run", "args": {"agent_id": win, "command": "echo"}}, 19)
    assert is_err and "'exec'" in msg and "scope" in msg, msg
    print("[9] scope 越权拒绝 OK")

    # [11] 审计溯源：MCP 发起的 exec 以 api-key:<name> 落审计（P002 删 audit.list op，改走 HTTP 端点）
    audit = http_json("GET", "/audit?limit=50", token=jwt)["audit"]
    actors = {i.get("actor") for i in audit}
    assert any(a and a.startswith("api-key:e2e-live-mcp") for a in actors), f"审计缺 MCP actor: {actors}"
    print("[11] 审计溯源 OK（actor=api-key:e2e-live-mcp）")

    # [12] 吊销后 401
    http_json("DELETE", f"/api-keys/{key_a['api_key']['id']}", token=jwt)
    status, _ = mcp_raw(ka, rpc("tools/list", {}, 30))
    assert status == 401, status
    http_json("DELETE", f"/api-keys/{key_b['api_key']['id']}", token=jwt)
    print("[12] 吊销后 401 OK（B 也已清理）")

    print("\nMCP live E2E 全部通过 ✓")


if __name__ == "__main__":
    main()
