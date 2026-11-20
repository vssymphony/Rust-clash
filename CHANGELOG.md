# 变更日志

本项目的所有重要变更都记录在此文件。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。
变更类型固定为 `Added` / `Changed` / `Deprecated` / `Removed` / `Fixed` / `Security` 六类。

## [Unreleased]

### Added

- CI 与发布流程（第二批工程化）：
  - `.github/workflows/ci.yml`：PR 与 main 推送触发，`rust-fmt` / `rust-clippy`（`-D warnings`）/
    `rust-test` / `frontend-typecheck` / `frontend-lint` / `frontend-test` / `audit` 七个 job，
    Windows + Ubuntu 双平台矩阵，启用 `Swatinem/rust-cache` 与 npm 缓存，失败时上传日志。
  - `.github/workflows/release.yml`：`v*` 标签触发，Windows x64（nsis / msi）与
    Linux x64（deb / rpm / AppImage）矩阵出包，含 sidecar 准备、wintun 分发决策、
    `SHA256SUMS` 生成与 GitHub Release 创建。
  - `.github/dependabot.yml`：cargo / npm / github-actions 三生态周更，并按 tauri、react、
    tooling 等关联组分组。
  - `deny.toml`：cargo-deny 的许可证白名单、依赖来源限制与重复依赖策略。
  - `.cargo/audit.toml`：cargo-audit 忽略项白名单（当前为空，附填写规范）。
  - `.github/pull_request_template.md`：含安全自查项的评审清单模板。
  - `CONTRIBUTING.md`：分支命名、Conventional Commits、评审要求与本地验证命令。
  - `scripts/sync-version.mjs` 与 `npm run version:check / version:print / version:set`：
    校验并统一 `package.json` / `src-tauri/Cargo.toml` / `src-tauri/tauri.conf.json` 三处版本号。
  - `docs/DEVELOPMENT.md`（本地开发、依赖准备、测试、调试、常见错误）与
    `docs/RELEASE.md`（发版步骤、产物校验、回滚方案、wintun 分发策略）。
- 测试体系：Rust 侧 56 个单元测试 + 10 个集成测试，前端 16 个 Vitest 用例，
  并新增 `TESTING.md` 记录编译机必跑命令与受限环境说明。

### Changed

- 工程规范（第一批工程化）：新增 `rustfmt.toml`、`clippy.toml`，`Cargo.toml` 启用
  `[lints]`（`all` + `pedantic` 及 5 条降级），前端接入 ESLint 9 与 Prettier 3 并加 `lint` /
  `format:check` 脚本。
- 构建流程收敛：原 `.github/workflows/build.yml`（打包 + 上传 artifact）拆分为
  `ci.yml`（只验证、不产包）与 `release.yml`（只打包发布），二者不再并存重复。
- wintun.dll 路径统一收敛到 `src-tauri/binaries/wintun.dll`，并在启动时做存在性校验与
  路径回显；`src-tauri/tauri.windows.conf.json` 将其映射为安装包资源。
- Tauri 配置一致性：`tauri.conf.json` 的 `identifier` / `productName` / 打包目标与
  实际代码、Linux 依赖（`libwebkit2gtk-4.1-0`、`libgtk-3-0`）对齐。

### Fixed

- 全量审计发现的缺陷修复：前端命令名与 Rust `#[tauri::command]` 注册列表、
  `capabilities` 权限标识符三方对齐。
- 修复 TUN 模式接入后遗留的启用状态与实际内核运行状态不同步问题（TUN 不可用时降级并提示，
  不覆写用户已保存的 `tunEnabled` 设置）。
- 配置与日志脱敏：订阅链接、`secret`、UUID 等敏感字段在落盘与日志输出前统一打码。
- 文件写入改为安全写（临时文件 + 原子替换），避免进程中断导致配置截断。

### Security

- 敏感信息脱敏覆盖日志、错误上报与前端展示三处。
- 依赖安全：CI 增加 `cargo-audit`（RustSec 漏洞库）与 `cargo-deny`（许可证 / 来源 / 重复依赖）
  双重检查，Dependabot 每周巡检三生态依赖。
- wintun.dll 不随包分发策略：仓库不存放 DLL，仅在使用者自备时支持随包，
  且必须先通过 Authenticode 签名校验（`scripts/verify-wintun-sign.ps1`），否则构建中断。

## [0.1.0] - 2026-11-19

首个可运行版本：基于 mihomo 内核的 Tauri 2 + React 19 跨平台桌面客户端。

### Added

- 内核进程管理：sidecar 启动 / 停止 / 重启，启动参数与配置目录隔离，异常退出自动重启与退避。
- 订阅管理：订阅新增、更新、切换、失效检测与流量信息展示。
- 代理控制：系统代理一键开关（Windows 注册表 / Linux 环境变量），规则与全局模式切换。
- 节点与策略组：节点延迟测速、策略组选择、连接列表查看与单条 / 全部断开。
- TUN 模式接入：基于 wintun 的虚拟网卡接管，含权限检测、驱动缺失提示与不可用降级。
- 设置页：开机自启、内核路径与版本、运行状态、日志查看与导出。
- 安全写与脱敏：配置原子写入、敏感字段打码、日志分级落盘。
