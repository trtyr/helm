#!/usr/bin/env bash
# 一键端到端 smoke：Postgres + Server + Agent → 登录 → 下发命令 → 验证结果。
set -euo pipefail

cd "$(dirname "$0")/.."

HTTP_ADDR="127.0.0.1:18080"
GRPC_ADDR="127.0.0.1:50051"
AGENT_ID="smoke-agent"

cleanup() {
    pkill -f 'target/debug/helm-' 2>/dev/null || true
}
trap cleanup EXIT

echo "==> 启动 Postgres"
docker compose up -d postgres
for i in $(seq 1 30); do
    s=$(docker inspect -f '{{.State.Health.Status}}' helm-postgres 2>/dev/null)
    [ "$s" = "healthy" ] && break
    sleep 1
done

echo "==> 构建"
cargo build -p helm-server -p helm-agent

echo "==> 启动 Server"
./target/debug/helm-server --http-addr "$HTTP_ADDR" --grpc-addr "$GRPC_ADDR" \
    >/tmp/helm-smoke-server.log 2>&1 &
sleep 3

echo "==> 启动 Agent"
./target/debug/helm-agent --agent-id "$AGENT_ID" \
    --server-addr "http://$GRPC_ADDR" --token dev-token-change-me \
    >/tmp/helm-smoke-agent.log 2>&1 &
sleep 3

echo "==> 登录"
TOKEN=$(curl -sf -X POST "http://$HTTP_ADDR/api/v1/auth/login" \
    -H 'Content-Type: application/json' \
    -d '{"username":"admin","password":"admin123"}' \
    | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')
AUTH="Authorization: Bearer $TOKEN"

echo "==> 下发命令"
JOB_ID=$(curl -sf -X POST "http://$HTTP_ADDR/api/v1/exec" -H "$AUTH" \
    -H 'Content-Type: application/json' \
    -d "{\"agent_id\":\"$AGENT_ID\",\"command\":\"echo\",\"args\":[\"smoke-ok\"]}" \
    | python3 -c 'import sys,json;print(json.load(sys.stdin)["job_id"])')
sleep 2

echo "==> 验证结果"
curl -sf "http://$HTTP_ADDR/api/v1/jobs/$JOB_ID" -H "$AUTH" | python3 -c '
import sys, json
job = json.load(sys.stdin)["job"]
assert job["status"] == "succeeded", "unexpected status: " + job["status"]
assert "smoke-ok" in (job["output"] or ""), "output missing smoke-ok"
print("smoke OK: status=" + job["status"] + " output=" + repr(job["output"]))
'
