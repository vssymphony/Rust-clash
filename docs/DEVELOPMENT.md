# 开发指南

面向本地开发的完整步骤：环境准备 → 依赖 → 运行 → 测试 → 调试 → 常见错误。
发版相关见 [RELEASE.md](RELEASE.md)；贡献流程与提交规范见 [CONTRIBUTING.md](../CONTRIBUTING.md)。

## 1. 技术栈与目录速览

| 层     | 技术                                                    |
| ------ | ------------------------------------------------------- |
| 桌面壳 | Tauri 2（Rust）                                         |
| 前端   | React 19 + TypeScript 5.7 + Vite 6 + Tailwind CSS 3     |
| 内核   | mihomo（作为 sidecar 子进程运行，不由 Rust 重写协议栈） |
| 状态   | Zustand + TanStack Query                                |

```
clash-gui/
├─ src/                     前端（页面、组件、api 调用封装、store）
├─ src-tauri/               Rust 侧（commands / core / mihomo_guard / tun / proxy …）
│  ├─ binaries/             内核 sidecar 与（可选）wintun.dll，均不入库
│  ├─ capabilities/         Tauri 权限配置
│  ├─ tauri.conf.json       主配置（version 与 package.json 同步）
│  └─ tauri.windows.conf.json  Windows 专属覆盖（wintun.dll 资源映射）
├─ scripts/                 prepare-sidecar / build / verify-wintun-sign / sync-version
├─ docs/                    DEVELOPMENT.md、RELEASE.md
└─ .github/workflows/       ci.yml（验证）、release.yml（打包发布）
```

## 2. 环境要求

- Node.js ≥ 20，npm ≥ 10
- Rust stable，MSRV **1.90**（见 `src-tauri/Cargo.toml` 的 `rust-version`）
- Windows：WebView2 Runtime（Win10 1803+ 通常已内置）、MSVC 构建工具（VS Build Tools，勾选 C++ 桌面开发）
- Linux：
  ```bash
  sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
    libayatana-appindicator3-dev librsvg2-dev patchelf build-essential curl file libssl-dev
  ```

> Tauri 2 使用 `webkit2gtk-4.1`；若发行版只有 4.0，需升级系统或换用对应版本依赖。

## 3. 首次拉取

```bash
git clone <repo> && cd clash-gui
npm ci                     # 按 package-lock.json 精确安装
npm run prepare:sidecar    # 下载 mihomo 内核到 src-tauri/binaries/（缺失时才下载）
npm run tauri:dev          # 启动桌面应用
```

内核准备细节：

- 文件名必须与 `rustc -vV` 的 `host:` 完全一致（如 `mihomo-x86_64-pc-windows-msvc.exe`），
  否则 `tauri build` 报 `failed to bundle external binary`；
- 内网无法访问 GitHub Releases 时，手动下载后放到 `src-tauri/binaries/` 并按上述规则重命名，
  或用 `npm run prepare:sidecar -- --source <本地内核路径>`；
- Windows 上若暂时没有 `wintun.dll` 且不想被脚本检查拦截，设置 `CLASH_GUI_SKIP_WINTUN=1`；
  但**没有该 DLL 时 TUN 不可用**（会自动降级为系统代理）。

## 4. 常用命令

| 命令                                                                | 作用                                                       |
| ------------------------------------------------------------------- | ---------------------------------------------------------- |
| `npm run dev`                                                       | 仅跑前端（浏览器内调试，无 Tauri API）                     |
| `npm run tauri:dev`                                                 | 桌面应用 + Vite 热重载（会先准备 sidecar）                 |
| `npm run build`                                                     | `tsc --noEmit` + `vite build`（前端产物）                  |
| `npm run typecheck`                                                 | TypeScript 类型检查                                        |
| `npm run lint` / `lint:fix`                                         | ESLint（`--max-warnings 0`）                               |
| `npm run format` / `format:check`                                   | Prettier 写 / 查                                           |
| `npm test` / `test:watch` / `test:coverage`                         | Vitest                                                     |
| `npm run verify`                                                    | typecheck + lint + test（提交前必跑）                      |
| `npm run rust:fmt` / `rust:fmt:write`                               | `cargo fmt --check` / 直接格式化                           |
| `npm run rust:clippy`                                               | `cargo clippy --all-targets --all-features -- -D warnings` |
| `npm run rust:test`                                                 | `cargo test --all-features`                                |
| `npm run verify:rust`                                               | Rust 三件套                                                |
| `npm run version:check` / `version:print` / `version:set -- <版本>` | 三处版本号校验 / 打印 / 统一                               |
| `npm run tauri:build`                                               | 打安装包（会先准备 sidecar）                               |

`cargo` 命令统一带 `--manifest-path src-tauri/Cargo.toml`，因此可在仓库根目录执行；
也可 `cd src-tauri` 后用裸命令。

## 5. 测试

| 层              | 位置                                 | 命令                |
| --------------- | ------------------------------------ | ------------------- |
| Rust 单元测试   | `src-tauri/src/**` 内 `#[cfg(test)]` | `npm run rust:test` |
| Rust 集成测试   | `src-tauri/tests/`                   | 同上                |
| 前端组件 / 逻辑 | `src/**/*.test.ts(x)`                | `npm test`          |

原则：缺陷修复必须带回归用例；涉及配置生成、脱敏、路径收敛、TUN 状态机等逻辑优先补单测。
CI 上七个 job 与上表一一对应（见 `.github/workflows/ci.yml`），本地绿了 CI 基本不会红。

## 6. 调试

- **前端**：`npm run dev` 打开 <http://localhost:1420>，可配合 React DevTools；调用 Tauri API 需回落桩，
  真实 IPC 请用 `npm run tauri:dev`。
- **Rust**：`RUST_LOG=debug npm run tauri:dev`（Windows PowerShell：`$env:RUST_LOG="debug"; npm run tauri:dev`）。
- **内核日志**：应用日志页可直接查看 sidecar stdout/stderr；内核自身的 `config.yaml` 与 `data/`
  位于配置目录（见 README「数据与配置位置」）。
- **配置目录快捷打开**：Windows `%APPDATA%\com.clashgui.desktop\`，Linux `~/.config/com.clashgui.desktop/`。
- **端口冲突**：默认混合代理 `7890`、控制面 `9090`，被占用时在设置页改端口，注意同步系统代理。
- **TUN 权限**：Windows 需管理员启动；Linux 用 `sudo setcap cap_net_admin,cap_net_raw+eip <二进制>`。

## 7. 受限环境（无 Rust 工具链）

若机器被 Smart App Control / 企业策略禁止执行 `cargo`（本项目开发机即属此类）：

- 允许：`npm run verify`、`format:check`、`version:check`、`npm run tauri:build` 的前端部分；
- 禁止：`cargo build/check/test/clippy/fmt`，Rust 侧只能做静态核对（符号级比对 registry 源码）；
- 要求：PR 中显式注明「Rust 侧未本地验证，依赖 CI」，并确保 `ci.yml` 全绿后才合并。
- 具体静态核对方法与已核项清单见 `TESTING.md`。

## 8. 常见错误

| 现象                                      | 原因                                            | 处理                                                              |
| ----------------------------------------- | ----------------------------------------------- | ----------------------------------------------------------------- |
| `failed to bundle external binary`        | sidecar 文件名与 host 三元组不匹配 / 文件缺失   | 重新执行 `npm run prepare:sidecar -- --force`，核对文件名         |
| 启动即退出，日志提示内核不存在            | 未放 `mihomo` 或路径不可执行                    | 放好 sidecar，或在设置页指定内核绝对路径                          |
| Linux 构建报 `webkit2gtk` 找不到          | 依赖缺失或版本为 4.0                            | 安装 `libwebkit2gtk-4.1-dev`                                      |
| `npm ci` 失败 `EBADENGINE`                | Node 版本过低                                   | 升级到 Node 20+                                                   |
| `npm run lint` 报大量 `prettier/prettier` | 未格式化                                        | `npm run format` 后重跑                                           |
| 版本校验失败（`version:check`）           | 三处版本不一致                                  | `npm run version:set -- <版本>`                                   |
| TUN 开关点了没反应                        | 无管理员 / 无 `CAP_NET_ADMIN` / 缺 `wintun.dll` | 见 README「TUN 模式说明」对应平台前置条件                         |
| sidecar 下载超时                          | 内网 / 代理限制                                 | 手动放置内核或 `--source` 指定本地文件                            |
| CI `audit` job 红                         | 依赖有 RustSec 告警或许可证不在白名单           | 升级依赖；确需豁免时在 `.cargo/audit.toml` / `deny.toml` 写清原因 |

## 9. 规范约定

- 代码风格：Rust 由 `rustfmt.toml`（`max_width = 100`）与 `clippy.toml` 约束，
  `Cargo.toml` 的 `[lints]` 开启 `all`（warn，CI 再用 `-D warnings` 提升为错误），
  `pedantic` 整体置为 `allow`（命令层与测试断言误报过多，理由见 `Cargo.toml` 注释）；前端由 ESLint 9 + Prettier 3 约束。
- 版本号：`package.json` 为唯一权威来源，另外两处由 `npm run version:set` 同步，**不要手改**。
- 变更日志：用户可见变更写入 `CHANGELOG.md` 的 `[Unreleased]`，发版时归档到版本段。
- 依赖新增：需通过 `cargo deny check`（许可证白名单见 `deny.toml`），并在 PR 中说明理由。
