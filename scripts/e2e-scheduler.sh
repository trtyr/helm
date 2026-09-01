#!/usr/bin/env bash
# 定时任务持久化恢复 e2e：创建 schedule → 重启 Server → 验证恢复执行。
set -euo pipefail

cd "$(dirname "$0")/.."

HTTP_ADDR="127.0.0.1:18080"
GRPC_ADDR="127.0.0.1:50051"
AGENT_ID="sched-e2e-agent"

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

echo "==> 清理旧任务与 job"
docker compose exec -T postgres psql -U helm -d helm \
    -c "DELETE FROM tasks; DELETE FROM jobs;" >/dev/null

echo "==> 构建"
cargo build -p helm-server -p helm-agent

start_server() {
    ./target/debug/helm-server --http-addr "$HTTP_ADDR" --grpc-addr "$GRPC_ADDR" \
        >/tmp/helm-sched-server.log 2>&1 &
    sleep 3
}

echo "==> 启动 Server + Agent"
start_server
./target/debug/helm-agent --agent-id "$AGENT_ID" \
    --server-addr "http://$GRPC_ADDR" --token dev-token-change-me \
    >/tmp/helm-sched-agent.log 2>&1 &
sleep 3

echo "==> 登录 + 创建定时任务"
TOKEN=$(curl -sf -X POST "http://$HTTP_ADDR/api/v1/auth/login" \
    -H 'Content-Type: application/json' \
    -d '{"username":"admin","password":"admin123"}' \
    | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')
AUTH="Authorization: Bearer $TOKEN"
curl -sf -X POST "http://$HTTP_ADDR/api/v1/tasks/schedule" -H "$AUTH" \
    -H 'Content-Type: application/json' \
    -d "{\"agent_id\":\"$AGENT_ID\",\"command\":\"echo\",\"args\":[\"sched-e2e\"],\"interval_secs\":3}" \
    >/dev/null

sleep 7
BEFORE=$(docker compose exec -T postgres psql -U helm -d helm -tAc \
    "SELECT COUNT(*) FROM jobs WHERE command='echo';")
echo "重启前 job 数: $BEFORE"

echo "==> 重启 Server"
pkill -f 'target/debug/helm-server' || true
sleep 2
start_server

sleep 8
AFTER=$(docker compose exec -T postgres psql -U helm -d helm -tAc \
    "SELECT COUNT(*) FROM jobs WHERE command='echo';")
echo "重启后 job 数: $AFTER"

RESUMED=$(grep -c 'resumed scheduled task' /tmp/helm-sched-server.log || true)
echo "resumed 日志条数: $RESUMED"

python3 -c "
before = int('$BEFORE'); after = int('$AFTER'); resumed = int('$RESUMED')
assert before > 0, '任务重启前未执行'
assert after > before, f'任务重启后未恢复执行 (before={before}, after={after})'
assert resumed >= 1, '未发现 resumed scheduled task 日志'
print(f'✓ 定时任务持久化恢复通过: {before} -> {after} jobs, resumed={resumed}')
"
