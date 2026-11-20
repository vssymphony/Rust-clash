# 测试与质量校验说明

> 本文件说明本项目三层测试（前端 / Rust 单元 / Rust 集成）的执行方式、覆盖范围，
> 以及**哪些步骤必须到具备 Rust 工具链的机器上执行**。
> 最后更新：2026-11-19（工程规范与安全加固批次）

## 1. 现状摘要

| 层次              | 位置                                                 | 规模             | 本机能否执行 | 本机执行结果           |
| ----------------- | ---------------------------------------------------- | ---------------- | ------------ | ---------------------- |
| 前端单元/组件测试 | `src/lib/api.test.ts`、`src/pages/Settings.test.tsx` | 2 文件 / 16 用例 | 可以         | 16/16 通过             |
| 前端类型与构建    | `tsc --noEmit`、`vite build`                         | —                | 可以         | 通过，`dist/` 正常产出 |
| 前端规范          | ESLint（`--max-warnings 0`）、Prettier               | —                | 可以         | 0 error / 0 warning    |
| Rust 单元测试     | `src-tauri/src/**` 内 `#[cfg(test)]`                 | 7 文件 / 56 用例 | **不可以**   | 未执行（见第 2 节）    |
| Rust 集成测试     | `src-tauri/tests/config_build.rs`                    | 1 文件 / 10 用例 | **不可以**   | 未执行（见第 2 节）    |
| Rust 规范         | rustfmt、clippy（`-D warnings`）                     | —                | **不可以**   | 未执行（见第 2 节）    |

## 2. 本机环境的硬约束

本机开启了 **Smart App Control** 且 Rust 工具链已清理，`cargo` 命令被系统拦截，
因此 **`cargo build / check / test / clippy / fmt` 一律无法在本机执行**。
这一批新增的 Rust 测试代码是通过「逐文件阅读 + 依赖 API 符号核对」方式保证自洽性，
**没有经过真实编译**——首次到编译机或 CI 时请把第 6 节的命令完整跑一遍。

前端不受该限制，`npm run *` 全部可用。

## 3. 前端测试

```powershell
cd output\clash-gui
npm install            # 首次或依赖变动后
npm run typecheck      # tsc --noEmit
npm run lint           # eslint . --max-warnings 0
npm run test           # vitest run
npm run build          # tsc --noEmit && vite build
npm run verify         # 上述 typecheck + lint + test 串联
```

覆盖范围：

| 文件                          | 覆盖点                                                                                     |
| ----------------------------- | ------------------------------------------------------------------------------------------ |
| `src/lib/api.test.ts`         | `invoke` 封装（命令名、参数 camelCase 映射、错误归一化）、错误分支抛出 `Error`、返回值直通 |
| `src/pages/Settings.test.tsx` | TUN 区块渲染、开关交互触发对应命令、stack 下拉切换、加载/错误态分支                        |

约定：

- 测试运行在 jsdom 下（`vitest.config.ts` → `environment: 'jsdom'`），`src/test/setup.ts` 负责注入 `@testing-library/jest-dom` 匹配器；
- 涉及 Tauri `invoke` 的用例统一 `vi.mock('@tauri-apps/api/core')`，不依赖真实 IPC；
- 组件测试使用 `@testing-library/react` 的 `screen` + `userEvent` 语义查询，禁止依赖 CSS 类名或实现细节。

## 4. Rust 单元测试（56 用例）

全部位于各模块末尾的 `#[cfg(test)] mod tests`，**只覆盖纯逻辑，不创建 `AppHandle`、不读用户目录、不联网**。

| 文件              | 用例数 | 覆盖点                                                                                      |
| ----------------- | ------ | ------------------------------------------------------------------------------------------- |
| `subscription.rs` | 11     | 配置拼装：TUN 开/关、stack 枚举、dns-hijack 空值与回退、默认策略组、规则兜底、订阅 DNS 保留 |
| `tun.rs`          | 7      | Linux `/proc/self/status` 能力解析、Windows 路径与设备判定、网卡名规范化                    |
| `wintun.rs`       | 11     | 候选路径收敛顺序、PE 头/架构解析、版本比较、体积阈值拒绝                                    |
| `storage.rs`      | 8      | 唯一临时名生成、原子替换、Unix 0600 权限、路径安全（`..` 与符号链接）                       |
| `models.rs`       | 7      | `AppSettings` 默认值、字段缺省时的向后兼容、枚举 `as_str` 与反序列化                        |
| `redact.rs`       | 7      | 订阅 URL / token / `Bearer` 头 / 键值对脱敏，非敏感文本不被误伤                             |
| `error.rs`        | 5      | `AppError` → 字符串、`From<String>/&str` 转换、序列化给前端的形状                           |

新增用例的硬性要求：

1. 被测算的纯逻辑函数不得依赖 `AppHandle`；确实需要时，把逻辑抽成 `fn foo_inner(input) -> Result<_>`，命令层只做参数搬运；
2. 断言用具体值而非 `assert!(x.is_ok())`，避免"改坏了也过"；
3. 涉及平台差异的分支用 `#[cfg(target_os = "...")]` 包住对应断言，**不要让测试在别的平台必然失败**。

## 5. Rust 集成测试（10 用例）

`src-tauri/tests/config_build.rs`，把「`AppSettings` + 订阅文本 → `build_config` → YAML」当成黑盒链路，
只通过 `clash_gui_lib::` 的公开 API 访问（因此 `Cargo.toml` 里 `[lib] name = "clash_gui_lib"` 不能改名）。

| 用例                                                                | 断言要点                                                              |
| ------------------------------------------------------------------- | --------------------------------------------------------------------- |
| `full_profile_with_tun_produces_expected_structure`                 | 端口/控制器/secret/mode、节点与策略组保留、tun 段齐全、dns 补 fake-ip |
| `tun_disabled_keeps_system_proxy_only_shape`                        | 无 `tun` 段、订阅 `nameserver` 原样保留                               |
| `subscription_without_dns_gets_default_dns`                         | 订阅缺 `dns` 时补默认 DNS + `enhanced-mode`                           |
| `without_subscription_falls_back_to_direct_rules`                   | 无订阅时 `MATCH,DIRECT` + `GEOIP,LAN` 直连兜底                        |
| `generated_yaml_is_reparsable_and_stable`                           | 同一输入两次生成完全一致、无 `!!` 标签                                |
| `baseline_ports_mode_and_log_level_follow_settings`                 | 端口/`mode`/`log-level`/`allow-lan` 跟随设置                          |
| `every_tun_stack_variant_serializes_to_its_own_name`                | Gvisor / System / Mixed 三种枚举值落盘正确                            |
| `custom_dns_hijack_drops_blank_entries_and_falls_back`              | 空白项剔除、全空白回退 `any:53`                                       |
| `nodes_without_groups_get_default_groups_using_configured_test_url` | 默认 PROXY/AUTO 组、`url` 取 `latency_test_url`、`MATCH,PROXY`        |
| `subscription_dns_is_preserved_but_forced_on_for_tun`               | 订阅 `dns.enable=false` 在 TUN 下被强制为 `true`，`nameserver` 不丢   |

约定：

- 断言对象是**解析后的 `serde_yaml::Value`**，不比对字符串全文（避免键序/缩进变化导致脆弱失败）；
- 不使用网络、不写磁盘、不读 `%APPDATA%`，因此可以在 CI 任意 runner 上跑；
- 新增配置字段时，优先补一个"缺省即兼容"的用例，保证老设置文件不炸。

## 6. 必须在编译机 / CI 执行的命令

以下命令**本机无法执行**，请到有 Rust 工具链（MSRV 1.90+）的机器或 CI 上运行：

```powershell
cd output\clash-gui\src-tauri

# 1) 格式：只检查不修改
cargo fmt --all -- --check

# 2) 静态检查：任何告警都视为失败（等价的 npm 脚本：npm run rust:clippy）
cargo clippy --all-targets --all-features -- -D warnings

# 3) 单元 + 集成测试（等价：npm run rust:test）
cargo test --all-features

# 4) 发布构建前的最后一道（会真正链接 wintun.dll 与 mihomo sidecar）
cd .. && npm run prepare:sidecar && npm run tauri:build
```

也可以直接用 npm 封装（参数已固定为 CI 口径）：

```powershell
npm run verify:rust   # rust:fmt + rust:clippy + rust:test
```

首次在 CI 上跑请重点关注三类可能失败：

1. **风格差异**：`cargo fmt --check` 报出的纯格式差异，直接 `npm run rust:fmt:write` 收敛；
2. **pedantic 告警**：`[lints.clippy]` 开启了 `all` + `pedantic`，已把 `module_name_repetitions`、`must_use_candidate` 等与 GUI 代码形态冲突的项降级；若仍有个别误报，优先改代码，确有理由再加 `#[allow(clippy::xxx, reason = "...")]`；
3. **跨平台断言**：`tun.rs` / `wintun.rs` 中的平台分支只在对应 OS 生效，Windows runner 上不会执行 Linux 用例，反之亦然——这是设计意图，不是漏测。

## 7. 代码规范配置位置

| 关注点         | 配置文件                                                   | 要点                                                                                                     |
| -------------- | ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Rust 格式      | `src-tauri/rustfmt.toml`                                   | `max_width = 100`、`edition = 2021`；`newline_style = "Auto"` 以免 Windows 检出 CRLF 后 `--check` 恒失败 |
| Rust lint 行为 | `src-tauri/clippy.toml`                                    | `msrv = "1.90"`（与 `Cargo.toml` 的 `rust-version` 一致）、参数/复杂度阈值                               |
| Rust lint 清单 | `src-tauri/Cargo.toml` → `[lints.rust]` / `[lints.clippy]` | 开启 `all` + `pedantic`；`unsafe_code = "warn"`，全项目仅 `system_proxy.rs` 一处 FFI 显式豁免            |
| JS/TS lint     | `eslint.config.js`                                         | flat config，含 `react-hooks` 规则，CI 用 `--max-warnings 0`                                             |
| JS/TS 格式     | `.prettierrc.json`                                         | 与 ESLint 不冲突（无 stylistic 规则重叠）                                                                |
| 测试运行器     | `vitest.config.ts`                                         | jsdom 环境、`src/test/setup.ts` 注入 jest-dom                                                            |

## 8. 手工验收（自动化测试覆盖不到的部分）

自动化测试无法覆盖需要真实内核、驱动与管理员权限的路径，发布前请人工确认：

1. **wintun.dll 供应链**
   - 放置位置：`src-tauri/binaries/wintun.dll`（运行时候选只有两级：**可执行文件所在目录**与**应用数据目录** `app.path().app_data_dir()`，不含 `%APPDATA%` 根、`%TEMP%`、`System32` 与当前工作目录，见 `wintun.rs::candidate_files`）；
   - 运行时校验：体积阈值 + `MZ`/`PE` 头 + 架构匹配 + 版本号下限；
   - 签名校验：运行时不做（需 WinTrust，且每次启动校验会明显拖慢启动），改由发布前脚本执行：
     ```powershell
     powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-wintun-sign.ps1
     ```
     脚本要求 Authenticode 状态为 `Valid` 且签名主体含 `WireGuard LLC`。**降级方案**：若该机器无法联网校验证书链，脚本会以非 0 退出并提示；此时必须在发布说明中记录"未做签名校验 + 已人工核对下载来源哈希"，不得默默跳过。
2. **TUN 模式**：以管理员身份运行 `npm run tauri:dev`，开启 TUN 后确认（a）虚拟网卡出现且路由表默认路由指向它；（b）`nslookup` 解析走 fake-ip；（c）关闭后路由与 DNS 恢复原状。
3. **系统代理**：开启/关闭后检查 `HKCU\...\Internet Settings` 的 `ProxyEnable`/`ProxyServer` 是否随状态变化，异常退出后重启应能按备份文件恢复。
4. **日志脱敏**：在设置里填入含 `token=` 的订阅链接，触发一次失败，检查 `logs/` 下日志文本中该 token 已被替换为掩码。

## 9. 常见问题

| 现象                                           | 原因与处理                                                                                  |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `cargo` 无输出或被系统终止                     | Smart App Control / 应用控制策略拦截，换编译机或 CI                                         |
| `integrated test` 找不到 `clash_gui_lib`       | `[lib] name` 被改名，集成测试通过该名字引用 crate                                           |
| `cargo fmt --check` 在 Windows 全红            | `rustfmt.toml` 的 `newline_style` 被改成 `Unix`，改回 `Auto`                                |
| Vitest 报 `matchMedia`/`ResizeObserver` 未定义 | 在 `src/test/setup.ts` 里补 polyfill，不要在每个用例里重复 mock                             |
| `npm run tauri:dev` 报找不到 sidecar           | 先 `npm run prepare:sidecar`；脚本现在会在产物缺失时以退出码 1 + 明确提示失败，不再静默通过 |
