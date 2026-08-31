default:
    @just --list

# 格式化代码
fmt:
    cargo fmt

# 检查格式化
fmt-check:
    cargo fmt -- --check

# 静态检查（零警告）
lint:
    cargo clippy --all-targets -- -D warnings

# 运行测试
test:
    cargo test

# 全部质量门禁
check: fmt-check lint test
    @echo "✓ all checks passed"

# buf 契约 lint
buf-lint:
    buf lint

# 启动 Postgres
db-up:
    docker compose up -d postgres

# 停止 Postgres
db-down:
    docker compose down

# 运行 server
run-server:
    cargo run -p helm-server

# 运行 agent
run-agent:
    cargo run -p helm-agent
