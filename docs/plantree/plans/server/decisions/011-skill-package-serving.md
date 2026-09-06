# 011 — Skill 包由 Server 内嵌分发（API key 即取即用）

Date: 2026-09-06

## Context

平台已具备完整 HTTP API（OpenAPI 契约机器校验），程序化调用方（CI、自动化 Agent、
运维脚本）的认证由 API key（决策 010）解决。但调用方仍缺一份**随平台走的操作知识包**：
脚本怎么调每个端点、域之间怎么编排、错误怎么排查——这些知识如果只躺在仓库文档里，
每个使用者都要重新踩一遍。

要求：平台自己提供 skill 安装包，用户持凭据即可下载，下载后放哪、怎么装由用户自便；
包内容覆盖平台操作的方方面面（SKILL.md + Python 分域脚本 + references），
且必须通过 API key 鉴权才能获取。

## Decision

- **包源在仓库 `skill/` 目录**，编译期经 `include_dir` 内嵌进 Server 二进制——
  分发不需要文件系统上存在该目录，Server 部署即自带。
- **两个端点**（挂受保护路由，JWT / API key 均可）：
  - `GET /api/v1/skill` → zip（deflate，脚本带可执行位，`Content-Disposition: attachment`）；
  - `GET /api/v1/skill/manifest` → `{name, version, file_count, files:[{path,size,sha256}]}`，
    用于升级对比与完整性校验。清单确定性生成（同输入同输出）。
- **鉴权取 key 而非 JWT**：API key 本身就是 skill 的取用凭据——若 skill 下载仅收 JWT，
  远端机器（无密码、只有 key）将永远拿不到操作知识，形成「先有 key 还是先有 skill」死锁。
- **包版本随 Server 版本**（`env!("CARGO_PKG_VERSION")`），Server 升级即包升级；
  清单 sha256 让脚本可自检是否需要重新下载。

## Consequences

### 启用

- 新机器接入零文档成本：拿到 URL + key → 下载 zip → 解压 → `python auth.py status` 即可用。
- skill 内容与 Server 版本强一致：文档描述的端点集合不可能落后于部署的二进制。
- 单元测试覆盖：zip 可被标准库解压、逐文件内容一致、manifest sha256 正确、构建确定性。

### 约束/代价

- skill 内容变更需要重新编译 Server（内嵌的代价；换来的是分发零依赖）。
- `skill/` 是包的**单一事实来源**：仓库外的已安装副本只是部署产物，修改必须回落仓库，
  否则下次下载即被覆盖。
- zip 打包引入 `zip` + `include_dir` 两个依赖（仅 server crate 使用）。

**相关：** [010 API key](010-api-keys.md)
