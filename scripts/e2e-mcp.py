#!/usr/bin/env python3
"""E2E：MCP 端点 + 细分 API 凭证（scope）。

覆盖：签发 scope 化 key → MCP initialize（协议握手）→ tools/list 单工具 +
scope 裁剪（受限 key 看不到 ir）→ catalog 编目 → tools/call loopback 透传
（exec.run 对离线 agent 报 409、越权 op 报 scope 缺失、os 不匹配报仅支持）→
batch 数组请求 → JWT 全量目录 → 吊销后 401。

依赖已构建的 ./target/debug/helm-server（本脚本负责起 Postgres 与 Server）。
"""

import json
import os
import sys
import urllib.error
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from e2e_helpers import (
    HTTP_ADDR,
    login,
    http_json,
    start_background,
    start_postgres,
    stop_processes,
    wait_server,
)

MCP_URL = f"http://{HTTP_ADDR}/mcp"


def mcp_raw(token: str, payload) -> tuple[int, dict | list | None]:
    """POST /mcp，返回 (status, 解析后的 JSON)。非 2xx 也返回解析错误体。"""
    req = urllib.request.Request(MCP_URL, data=json.dumps(payload).encode(), method="POST")
    req.add_header("Content-Type", "application/json")
    req.add_header("Accept", "application/json, text/event-stream")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
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


def call_tool(token: str, arguments: dict, id: int = 10) -> tuple[bool, dict | str]:
    """tools/call 便捷封装。返回 (isError, 解析后的业务 JSON 或错误文本)。"""
    status, resp = mcp_raw(token, rpc("tools/call", {"name": "helm", "arguments": arguments}, id))
    assert status == 200, f"tools/call HTTP {status}: {resp}"
    assert "error" not in resp, f"tools/call 协议错误: {resp['error']}"
    result = resp["result"]
    assert result["isError"] is False or result["isError"] is True
    text = result["content"][0]["text"]
    return result["isError"], (json.loads(text) if not result["isError"] else text)


def main() -> None:
    start_postgres()
    os.environ["HELM_HTTP_ADDR"] = HTTP_ADDR
    server = start_background(["./target/debug/helm-server"], "e2e-mcp-server.log")
    try:
        wait_server()
        run_tests()
    finally:
        stop_processes([server])


def run_tests() -> None:
    jwt = login()

    # ---- 签发受限 key（exec + metrics）----
    created = http_json(
        "POST",
        "/api-keys",
        {"name": "e2e-mcp", "scopes": ["exec", "metrics"]},
        token=jwt,
    )
    key = created["key"]
    assert key.startswith("helm_"), key
    assert created["api_key"]["scopes"] == ["exec", "metrics"], created["api_key"]
    print("[1] scope 化 key 签发 OK")

    # ---- MCP 握手 ----
    status, resp = mcp_raw(key, rpc("initialize", {"protocolVersion": "2025-06-18"}))
    assert status == 200 and resp["result"]["protocolVersion"] == "2025-06-18", resp
    assert resp["result"]["serverInfo"]["name"] == "helm"
    # 通知：202 无响应体
    status, body = mcp_raw(key, {"jsonrpc": "2.0", "method": "notifications/initialized"})
    assert status == 202 and body is None, (status, body)
    print("[2] initialize / notifications OK")

    # ---- tools/list：单工具 + scope 裁剪 ----
    status, resp = mcp_raw(key, rpc("tools/list", {}, 2))
    assert status == 200
    tools = resp["result"]["tools"]
    assert len(tools) == 1 and tools[0]["name"] == "helm", tools
    desc = tools[0]["description"]
    assert "exec.run" in desc, "受限 key 应能看到 exec 域"
    assert "ir.scan" not in desc, "受限 key 不应看到 ir 域"
    print("[3] tools/list 单工具 + scope 裁剪 OK")

    # ---- catalog：编目按 scope 裁剪（P002 三级形状：域 → 能力组 → 工具）----
    is_err, catalog = call_tool(key, {"op": "catalog"}, 3)
    assert not is_err
    group_names = [d["group"] for d in catalog["domains"]]
    assert group_names == ["host"], group_names
    # exec+metrics key 可见 6 op（scope 均为 exec，全落「执行」组）；metrics 域 op 已在 P002 删除
    ops = sorted(o["op"] for s in catalog["domains"][0]["subgroups"] for o in s["ops"])
    assert ops == ["exec.batch", "exec.run", "jobs.get", "jobs.list", "tasks.schedule", "tasks.script"], ops
    print("[4] catalog 编目裁剪 OK")

    # ---- tools/call 透传：exec.run 对不存在 agent → 平台 409 映射为 isError ----
    is_err, msg = call_tool(key, {"op": "exec.run", "args": {"agent_id": "no-such-agent", "command": "echo"}}, 4)
    assert is_err and ("404" in msg and "not found" in msg), msg
    print("[5] tools/call loopback 透传（含平台错误映射）OK")

    # ---- 越权 op：scope 缺失 → 明确提示 ----
    is_err, msg = call_tool(key, {"op": "ir.scan", "args": {"agent_id": "x"}}, 5)
    assert is_err and "ir" in msg and "scope" in msg, msg
    # 未编目（管理面）op 一律不可见也不可调（协议级 -32602 或工具级 isError 均可）
    status, resp = mcp_raw(key, rpc("tools/call", {"name": "helm", "arguments": {"op": "nonexistent.op"}}, 6))
    assert status == 200
    if "error" in resp:
        assert resp["error"]["code"] == -32602 and "unknown op" in resp["error"]["message"], resp
    else:
        assert resp["result"]["isError"] and "unknown op" in resp["result"]["content"][0]["text"], resp
    print("[6] 越权 / 未知 op 拒绝 OK")

    # ---- os 检查：ir（Windows 专属）传 linux → 拒绝 ----
    ir_jwt_created = http_json("POST", "/api-keys", {"name": "e2e-mcp-ir", "scopes": ["ir"]}, token=jwt)
    ir_key = ir_jwt_created["key"]
    is_err, msg = call_tool(ir_key, {"op": "ir.scan", "os": "linux", "args": {"agent_id": "x"}}, 7)
    assert is_err and "windows" in msg, msg
    print("[7] os 不匹配拒绝 OK")

    # ---- batch 数组请求 ----
    status, resp = mcp_raw(key, [rpc("initialize", {}, 1), rpc("tools/list", {}, 2)])
    assert status == 200 and isinstance(resp, list) and len(resp) == 2, resp
    print("[8] batch 数组请求 OK")

    # ---- JWT 走 MCP：全量目录 ----
    status, resp = mcp_raw(jwt, rpc("tools/list", {}, 3))
    assert status == 200 and "ir.scan" in resp["result"]["tools"][0]["description"]
    print("[9] JWT 全量目录 OK")

    # ---- 吊销后 401 ----
    key_id = created["api_key"]["id"]
    http_json("DELETE", f"/api-keys/{key_id}", token=jwt)
    status, resp = mcp_raw(key, rpc("tools/list", {}, 4))
    assert status == 401, (status, resp)
    print("[10] 吊销后 401 OK")

    # 清理第二把 key
    http_json("DELETE", f"/api-keys/{ir_jwt_created['api_key']['id']}", token=jwt)

    print("\n全部通过 ✓")


if __name__ == "__main__":
    main()
