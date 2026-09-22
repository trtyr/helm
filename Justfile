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

# 前端门禁（tsc + oxlint + vitest + build）
console-check:
    pnpm --dir console install --frozen-lockfile
    pnpm --dir console exec tsc -b
    pnpm --dir console run lint
    pnpm --dir console run test
    pnpm --dir console run build

# Windows-only 代码的编译校验（T6 新增）：agent/src/ir/* 与 win_native 在 macOS/Linux 上
# 不参与编译，这是本地能拿到的最强验证。缺 mingw-w64 时跳过。
# P006 P0-7：此前写成 `cmd && cargo check || echo 跳过`——`||` 会把**编译失败**也当"缺工具"
# 吞掉（正是这个配方唯一要抓的问题），门禁因此假绿。if/else 把两者分开。
windows-check:
    @if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then \
        cargo check -p helm-agent --target x86_64-pc-windows-gnu; \
    else \
        echo "⊘ 跳过 windows-check（缺 mingw-w64：brew install mingw-w64）"; \
    fi

# 全部质量门禁
check: fmt-check lint test windows-check console-check
    @echo "✓ all checks passed"

# buf 契约 lint
buf-lint:
    buf lint

# buf 契约 breaking 检查（对比上一 commit 的 proto）
buf-breaking:
    buf breaking --against '.git#ref=HEAD~1'

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
