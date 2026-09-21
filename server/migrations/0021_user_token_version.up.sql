-- JWT 吊销（A5）：token 版本号——改密/改名后自增，使已签发的旧 JWT 立即失效
-- 用 BIGINT（INT8）与 Rust 侧 i64 对齐；JWT 的 tv 是 JSON number，统一按 i64 处理
ALTER TABLE users ADD COLUMN token_version BIGINT NOT NULL DEFAULT 1;
