# helm-server 多阶段构建：rust 镜像编译 → debian-slim 运行。
# 注意：本镜像仅跑 Server；Agent 部署在目标机上，不进容器。

# ---- 构建阶段 ----
# ⚠ 构建阶段必须与运行阶段同一 Debian 代号：`rust:1.97-slim` 基于 trixie(glibc 2.41)，
#   而运行镜像是 bookworm(glibc 2.36)，混用会让二进制报 `GLIBC_2.39 not found` 直接
#   跑不起来（2026-09-21 冒烟测试实测，容器表现为无限重启）。钉在 bookworm 变体还有
#   第二个好处：容器内 agent-gen 产出的 Agent 二进制 glibc 要求更低，目标机兼容面更大。
FROM rust:1.97-slim-bookworm AS builder
WORKDIR /build

# 编译依赖（protobuf 编译器 + C 工具链；ring 需要 cc）
RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler pkg-config ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 先拷清单做依赖层缓存（源码变动不重拉依赖）
# P006 P0-7：`cargo fetch --locked` 硬校验清单/锁文件（错即中止）；桩构建失败是**预期**的
# （空 lib.rs / 假 main 编不过，这层只为拉依赖），但**不再把 stderr 丢掉**——否则清单、proto
# 代码生成之类的真错误会被一起吞掉。
COPY Cargo.toml Cargo.lock ./
COPY proto/Cargo.toml proto/Cargo.toml
COPY server/Cargo.toml server/Cargo.toml
COPY agent/Cargo.toml agent/Cargo.toml
RUN mkdir -p proto/src server/src agent/src \
    && echo "" > proto/src/lib.rs \
    && echo "fn main() {}" > server/src/main.rs \
    && echo "" > server/src/lib.rs \
    && echo "fn main() {}" > agent/src/main.rs \
    && cargo fetch --locked \
    && cargo build --release --locked -p helm-server -p helm-proto || true

# 拷源码正式构建
COPY proto proto
COPY server server
COPY agent agent
RUN touch proto/src/lib.rs server/src/main.rs server/src/lib.rs agent/src/main.rs \
    && cargo build --release --locked -p helm-server

# agent-gen 现场编译需要 agent 源码工作区与 cargo：镜像内保留源码 + 构建产物缓存
# （运行容器内 cargo build -p helm-agent，Linux 目标本机编译，无需交叉工具链）

# ---- 运行阶段 ----
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates protobuf-compiler pkg-config build-essential curl \
    && rm -rf /var/lib/apt/lists/*

# agent-gen 现场编译需要现代 cargo：apt 仓库自带的 cargo 版本低于本 workspace
# 的 edition 2024 要求，故直接复用构建阶段的 rustup 工具链（离线可用；
# 代价是运行镜像体积增加约 1GB）。不需要容器内出包可删这三行。
COPY --from=builder /usr/local/rustup /usr/local/rustup
COPY --from=builder /usr/local/cargo /usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

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
