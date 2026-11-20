# 发布指南

覆盖：发版前检查 → 版本号与日志 → 打标签 → 自动构建 → 产物校验 → 发布说明 → 回滚。
发布工作流：`.github/workflows/release.yml`；验证工作流：`.github/workflows/ci.yml`。

## 1. 产物矩阵

| 平台        | 目标三元组                 | 安装包                        | 说明                                                                                          |
| ----------- | -------------------------- | ----------------------------- | --------------------------------------------------------------------------------------------- |
| Windows x64 | `x86_64-pc-windows-msvc`   | `.msi`（WiX）、`.exe`（NSIS） | NSIS 默认 `perMachine` 安装，语言 SimpChinese / English；WebView2 采用 `downloadBootstrapper` |
| Linux x64   | `x86_64-unknown-linux-gnu` | `.deb`、`.rpm`、`.AppImage`   | deb 依赖 `libwebkit2gtk-4.1-0`、`libgtk-3-0`                                                  |

每个平台额外产出：

| 文件                      | 内容                                                              |
| ------------------------- | ----------------------------------------------------------------- |
| `SHA256SUMS-<三元组>.txt` | 该平台全部安装包的 SHA256，发布时合并为 Release 上的 `SHA256SUMS` |
| `SIDECAR-<三元组>.sha256` | 随包 mihomo 内核的 SHA256 + 版本号注释行                          |
| `wintun-status.txt`       | 仅 Windows：本次产物**是否包含** wintun.dll 及原因 / 哈希         |

## 2. 发版前检查清单

1. `main` 分支 CI 全绿（`ci.yml` 七个 job）；
2. 工作区干净，无未提交改动（`git status`）；
3. 三处版本号一致：`npm run version:check`；
4. `CHANGELOG.md` 的 `[Unreleased]` 已整理，按 `Added / Changed / Fixed / Security` 归类，
   并在发版时改名为 `## [x.y.z] - YYYY-MM-DD`，同时新增空的 `[Unreleased]` 段；
5. 依赖安全：`cargo audit` 无非预期告警；`.cargo/audit.toml` 的忽略项均有原因与计划修复版本；
6. 许可证：`cargo deny check` 通过（白名单见根目录 `deny.toml`）；
7. Windows 产物的 wintun 策略已确定（随包 / 不随包），见第 6 节。

## 3. 版本号与标签

版本号唯一权威来源是 `package.json`，另外两处由脚本同步：

```bash
npm run version:set -- 0.2.0     # 同步 package.json / src-tauri/Cargo.toml / src-tauri/tauri.conf.json
npm run version:check            # 复核
```

提交并打标签（标签必须与版本号严格一致，`release.yml` 会校验，不一致直接失败）：

```bash
git add -A
git commit -m "chore(release): 0.2.0"
git tag -a v0.2.0 -m "Clash GUI v0.2.0"
git push origin main --follow-tags
```

## 4. 自动构建

- 触发：推送 `v*` 标签；或 Actions 页面手动 `workflow_dispatch`（可选 `mihomo_version`、
  `draft`，手动触发**只产 artifact、不发版**，用于发布前验证）。
- 流程：checkout → Node/Rust 环境 → 缓存 → Linux 依赖 → 版本与 tag 校验 → `npm ci` →
  `prepare-sidecar`（按 `MIHOMO_VERSION`，默认 `v1.19.0`）→ 记录 sidecar 哈希 →
  wintun 决策 → `npm run tauri:build` → 收集产物 + 生成 `SHA256SUMS` → 上传 artifact →
  （tag 触发）合并校验文件、生成发布说明、创建 GitHub Release。

任一步失败时，对应 job 会上传 `logs-release-<三元组>` artifact（含 npm / sidecar / tauri 构建日志）。

## 5. 产物校验

发布后必须做一次独立校验，确认 Release 上的文件与构建产物一致：

```bash
# Linux / macOS
sha256sum -c SHA256SUMS
shasum -a 256 -c SHA256SUMS

# Windows（PowerShell）
Get-FileHash .\clash-gui_0.2.0_x64-setup.exe -Algorithm SHA256
certutil -hashfile .\clash-gui_0.2.0_x64_en-US.msi SHA256
```

逐项确认：

- [ ] 四个安装包齐全（msi / nsis exe / deb + rpm + AppImage 中的 Linux 三项）；
- [ ] `SHA256SUMS` 行数与实际文件数一致，且无 `*` 之类字面量残留；
- [ ] 每个安装包能被系统正常识别（Windows 双平台安装一遍；Linux `dpkg -i` / `rpm -i` / 直接运行 AppImage）；
- [ ] 安装后启动无「内核缺失」错误，设置页显示 sidecar 版本与路径；
- [ ] 安装包内 `mihomo` 内核哈希与 `SIDECAR-<三元组>.sha256` 一致；
- [ ] Windows 产物中 `wintun.dll` 的存在性与 `wintun-status.txt` 描述一致；
- [ ] 卸载后配置目录与注册表残留符合预期（`%APPDATA%\com.clashgui.desktop\` 保留用户数据属预期行为）。

## 6. 发布说明必含：wintun 校验状态

GitHub Release 正文由工作流自动生成，其中 **「wintun.dll 校验状态（必读）」** 一节不可删除，
必须明确写出下列之一：

| 情形                | 必须写明的内容                                                                                                                                         |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 随包包含 wintun.dll | 「本产物包含 wintun.dll（已通过 Authenticode 签名校验，签发者 WireGuard LLC）」+ 该 DLL 的 SHA256 值                                                   |
| 不随包              | 「本产物不含 wintun.dll」+ 手动放置步骤（来源 <https://www.wintun.net/>、目标目录、重启后校验方式）+ 明确说明「未放置时 TUN 不可用，系统代理不受影响」 |

同时如实说明：**应用内自动下载 wintun / 内核的功能当前未实现**，需手动放置。

## 7. sidecar 与 wintun 分发策略

| 资源              | 策略                 | 关键点                                                                                                                                                                                      |
| ----------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| mihomo（sidecar） | **随包**，构建期下载 | 由 `scripts/prepare-sidecar.*` 按 `MIHOMO_VERSION` 下载并重命名为 `mihomo-<三元组>[.exe]`；哈希写入 `SIDECAR-<三元组>.sha256`；升级内核只改 `MIHOMO_VERSION`/标签版本，不改代码             |
| wintun.dll        | **默认不随包**       | 仓库与 Release 均不长期分发该文件（分发受 WireGuard 许可约束）；构建机自备时**必须先**通过 `scripts/verify-wintun-sign.ps1 -DllPath src-tauri/binaries/wintun.dll` 校验签名，通过才允许随包 |
| 自动下载          | **未实现**           | 不在应用内下载内核 / 驱动。首次启动检测缺失 → 设置页给出警告与下载指引，TUN 请求被拒绝并降级为系统代理，`tunEnabled` 不被改写                                                               |

> 若未来实现自动下载，必须补齐：固定来源域名、哈希校验、失败回退、用户显式同意四件事，
> 并同步更新本节与 README。

## 8. 回滚方案

按影响面从轻到重选择，**任何回滚都要在 `CHANGELOG.md` 追加 `### Reverted` 段说明原因与受影响版本**。

### 8.1 产物有问题但版本可救（最常见）

1. 在 GitHub Release 页面把该 Release 标记为 **pre-release**（或编辑说明加醒目警告），
   阻止用户继续下载；
2. 修复后走正常发版流程发 `x.y.z+1`；
3. 不删除旧 tag，保留可追溯性。

### 8.2 版本必须撤回

```bash
# 1) 撤销 Release 与 tag（远端）
gh release delete v0.2.0 --yes
git push origin :refs/tags/v0.2.0
git tag -d v0.2.0

# 2) 回退版本号并重打 tag（如改用 0.2.1）
npm run version:set -- 0.2.1
git commit -am "chore(release): 0.2.1（回滚 0.2.0）"
git tag -a v0.2.1 -m "Clash GUI v0.2.1"
git push origin main --follow-tags
```

注意：已下载的用户不会被召回，因此发布说明里要写清「请升级到 x.y.z」。

### 8.3 已合并的代码引入回归

```bash
git revert --no-edit <commit>      # 生成反向提交，保留历史
git push origin main               # CI 通过后按 8.1 发补丁版
```

禁止 `git reset --hard` 强推 `main`（分支受保护且会破坏他人工作副本）。

### 8.4 用户侧应急指引

发布说明的排障段落应给出：删除/重命名配置目录以恢复默认（
`%APPDATA%\com.clashgui.desktop\` 或 `~/.config/com.clashgui.desktop/`）、
回退到上一版本的下载链接、以及如何用 `system_proxy_backup.json` 手工恢复系统代理。

## 9. 发布后

1. 在 Release 中补齐 `### 已知问题` 与 `### 升级方式`；
2. 把 `CHANGELOG.md` 的版本段链接补到文件底部的比较链接（如有）；
3. 检查 Dependabot 是否已因新版本开 PR；
4. 若本次改动了构建流程（bundle 目标、依赖、sidecar 版本），同步更新 README 与本文档。
