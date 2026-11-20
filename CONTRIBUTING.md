# 贡献指南

感谢参与 Clash GUI。为保证可维护性与发布可追溯，所有改动都走「分支 → PR → 评审 → 合并」流程，
`main` 分支受保护，不直接推送。

## 1. 环境准备

本地开发所需工具链、依赖与调试方法见 [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)。
最小要求：

- Node.js ≥ 20，npm ≥ 10
- Rust stable（MSRV 1.90，见 `src-tauri/Cargo.toml` 的 `rust-version`）
- 平台依赖：Windows 需 WebView2 与 MSVC 构建工具；Linux 需 `libwebkit2gtk-4.1-dev`、
  `libgtk-3-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、`patchelf`
- mihomo 内核与 wintun.dll **不入库**，按 `npm run prepare:sidecar` 与
  [docs/RELEASE.md](docs/RELEASE.md)「wintun.dll 分发策略」自行准备

## 2. 分支规范

从最新的 `main` 拉出，命名 `<类型>/<短描述>`，短描述用小写字母与连字符，不超过 5 个单词：

| 类型       | 用途                 | 示例                        |
| ---------- | -------------------- | --------------------------- |
| `feat`     | 新功能               | `feat/tun-auto-recover`     |
| `fix`      | 缺陷修复             | `fix/proxy-toggle-crash`    |
| `refactor` | 不改变行为的重构     | `refactor/kernel-lifecycle` |
| `perf`     | 性能优化             | `perf/subscription-parse`   |
| `docs`     | 仅文档               | `docs/release-rollback`     |
| `test`     | 仅测试               | `test/rule-switch`          |
| `chore`    | 构建 / 依赖 / 工具链 | `chore/bump-tauri`          |
| `ci`       | CI 与工作流          | `ci/cache-npm`              |

禁止：直接推 `main`、在他人分支上强推、一个分支混装多个不相关改动。

## 3. 提交信息：Conventional Commits

格式：

```
<type>(<scope>): <subject>

<body 可选>

<footer 可选>
```

- `type` 取值同分支类型（`feat` / `fix` / `refactor` / `perf` / `docs` / `test` / `chore` / `ci` / `build` / `revert`）；
- `scope` 用模块名，常用：`core`（内核进程）、`proxy`、`sub`（订阅）、`nodes`、`tun`、`settings`、
  `ui`、`security`、`deps`、`ci`；
- `subject` 用中文或英文均可，动词开头，不加句号，不超过 50 字；
- 破坏性变更在 `type` 后加 `!`，并在 footer 写 `BREAKING CHANGE: 说明`。

示例：

```
feat(tun): 支持 wintun 缺失时降级为系统代理
fix(proxy): 修复切换全局模式后托盘状态不同步
chore(deps): 升级 tauri 至 2.2
docs(release): 补充产物 SHA256 校验步骤
```

`CHANGELOG.md` 的条目由这些 type 归类生成，提交信息不规范会导致变更日志失真。

## 4. 提交前自检

在有完整工具链的机器上，提交前至少执行：

```bash
npm ci
npm run verify            # typecheck + eslint + vitest
npm run format:check      # Prettier
npm run version:check     # 三处版本号一致
npm run rust:fmt          # cargo fmt --check
npm run rust:clippy       # clippy -D warnings
npm run rust:test         # 单元 + 集成测试
```

受限环境（无 Rust 工具链）请至少完成 `npm run verify` / `npm run format:check` /
`npm run version:check`，并在 PR 描述中显式注明「Rust 侧未本地验证，依赖 CI」，详见 `TESTING.md`。

## 5. Pull Request

- 标题同样遵循 Conventional Commits；
- 使用 `.github/pull_request_template.md`，逐项填写，不留空章节；
- 单个 PR 聚焦一件事，超过 400 行改动请拆分；
- 提交后 CI 必须全绿（`ci.yml` 七个 job）；
- 至少 1 名维护者 Approve；涉及内核层 / TUN / 安全写 / 权限配置的改动需 2 名。

### 评审检查项

评审人按下表逐项确认，缺项不得合并：

| 类别     | 检查内容                                                                                                                                                           |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 功能     | 与 issue 描述一致；边界情况（空订阅、内核崩溃、网络断开、TUN 不可用）有处理                                                                                        |
| 测试     | 新增逻辑有对应单测；修复缺陷有回归用例；`npm run test` / `cargo test` 通过                                                                                         |
| **安全** | 无硬编码密钥 / 订阅链接 / token；日志与错误信息已脱敏；新增写文件走安全写；新增 `#[tauri::command]` 有参数校验；`capabilities` 权限**最小化**，不得为省事放开 `**` |
| **依赖** | 新增依赖需说明理由与许可证（须在 `deny.toml` 白名单内）；不引入 git 源依赖；`cargo audit` 无新增告警                                                               |
| 兼容     | Windows / Linux 双平台行为差异有说明；不破坏已有用户配置与 `tunEnabled` 等持久化字段语义                                                                           |
| 文档     | 行为变化同步更新 `README.md` / `docs/*`；用户可见变更写入 `CHANGELOG.md` 的 `[Unreleased]`                                                                         |
| 版本     | 不做无授权的版本号改动；发版由维护者用 `npm run version:set -- <版本>` 统一                                                                                        |

## 6. 安全红线

以下内容**禁止**进入仓库，发现即回滚 PR 并轮换相关凭据：

- 订阅链接、`secret`、UUID、密码、token、私钥（`.env`、`.ssh/`、`*.pem`、`*.p12`）；
- 用户真实配置与日志（`%APPDATA%\clash-gui\`、`~/.config/clash-gui/` 下的文件）；
- 内核二进制 `mihomo*` 与 `wintun.dll`（体积大且需签名校验，走 `scripts/prepare-sidecar.*` 与手动放置）；
- 打包产物（`src-tauri/target/`、`dist/`、`*.msi`、`*.deb`、`*.AppImage`）。

发现安全漏洞请勿提交公开 issue，按 `SECURITY.md`（如存在）或邮件联系维护者。

## 7. 许可证

提交即表示同意以本项目 MIT 许可证发布你的贡献。新增第三方代码须兼容 MIT 且通过
`cargo deny check` 的许可证白名单。
