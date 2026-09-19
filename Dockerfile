# helm-server 多阶段构建：rust 镜像编译 → debian-slim 运行。
# 注意：本镜像仅跑 Server；Agent 部署在目标机上，不进容器。

# ---- 构建阶段 ----
FROM rust:1.97-slim AS builder
WORKDIR /build

# 编译依赖（protobuf 编译器 + C 工具链；ring 需要 cc）
RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler pkg-config ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 先拷清单做依赖层缓存（源码变动不重拉依赖）
COPY Cargo.toml Cargo.lock ./
COPY proto/Cargo.toml proto/Cargo.toml
COPY server/Cargo.toml server/Cargo.toml
COPY agent/Cargo.toml agent/Cargo.toml
RUN mkdir -p proto/src server/src agent/src \
    && echo "" > proto/src/lib.rs \
    && echo "fn main() {}" > server/src/main.rs \
    && echo "" > server/src/lib.rs \
    && echo "fn main() {}" > agent/src/main.rs \
    && cargo build --release -p helm-server -p helm-proto 2>/dev/null || true

# 拷源码正式构建
COPY proto proto
COPY server server
COPY agent agent
RUN touch proto/src/lib.rs server/src/main.rs server/src/lib.rs agent/src/main.rs \
    && cargo build --release -p helm-server

# agent-gen 现场编译需要 agent 源码工作区与 cargo：镜像内保留源码 + 构建产物缓存
# （运行容器内 cargo build -p helm-agent，Linux 目标本机编译，无需交叉工具链）

# ---- 运行阶段 ----
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates protobuf-compiler pkg-config build-essential cargo \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/helm-server /usr/local/bin/helm-server

# agent-gen 工作区：源码 + 依赖构建缓存（容器内本机编译 Linux agent 用）
WORKDIR /workspace
COPY --from=builder /build/Cargo.toml /build/Cargo.lock ./
COPY --from=builder /build/proto proto
COPY --from=builder /build/agent agent
COPY --from=builder /build/server server
COPY --from=builder /build/target/release/deps target/release/deps
COPY --from=builder /build/target/release/build target/release/build

ENV HELM_AGENT_SOURCE_DIR=/workspace
EXPOSE 8080 50051

ENTRYPOINT ["/usr/local/bin/helm-server"]
