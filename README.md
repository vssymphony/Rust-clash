# Clash GUI

基于 **Tauri 2 + React 19 + TypeScript** 的跨平台 Clash / mihomo 桌面客户端，支持 **Windows** 与 **Linux**。

- Rust 侧只做「进程守护 + 控制面 + 系统集成」，代理能力完全由 **mihomo 内核（sidecar 子进程）** 提供，不在 Rust 中重写协议栈。
- 支持 **系统代理** 与 **TUN（虚拟网卡）** 两种接管方式，二者可独立开关；TUN 的启停、权限探测与降级逻辑见下文「TUN 模式说明」。

---

## 功能一览

| 模块         | 说明                                                                                                                                                                                                                            |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 内核守护     | 启动 / 停止 / 重启 mihomo，健康检查（轮询 `/version`）、崩溃自动重启（最多 5 次，指数退避）、退出时优雅终止并清理残留                                                                                                           |
| 控制面       | Clash REST API 客户端（reqwest，强制绕过系统代理避免回环）、WebSocket 订阅 `/traffic`、`/connections`、`/logs`，断线 2 秒自动重连                                                                                               |
| 配置生成     | 订阅 YAML 解析（serde_yaml）、节点数量统计、运行配置生成（端口 / 模式 / 日志等级 / 密钥 / 外部控制 / 局域网），原子写盘                                                                                                         |
| 订阅管理     | 本地文件导入与远程 URL 导入、手动更新、定时更新（间隔可配，默认 360 分钟）、切换生效、删除                                                                                                                                      |
| 节点与策略组 | 代理组展示、节点切换、单节点延迟测试、整组延迟测试（并发）                                                                                                                                                                      |
| 连接与规则   | 连接明细（含进程、规则、流量）、单条关闭、全部关闭、规则列表检索                                                                                                                                                                |
| 系统代理     | Windows：注册表 `Internet Settings` + `InternetSetOption` 刷新；Linux：GNOME `gsettings` 优先、KDE `kioslaverc` 兜底，均不可用时返回明确降级提示                                                                                |
| TUN 模式     | 生成 `tun` 配置段（stack / device / auto-route / auto-detect-interface / strict-route / dns-hijack）；Windows 检测管理员权限与 `wintun.dll`，Linux 检测 root 或 `CAP_NET_ADMIN`；权限不足时本次运行自动降级为系统代理并给出指引 |
| 其他         | 单实例锁、系统托盘（显示窗口 / 切换系统代理 / 重启内核 / 退出）、开机自启、日志面板、流量曲线                                                                                                                                   |

---

## 目录结构

```
clash-gui/
├─ src/                        # React 前端（Vite + TS + Tailwind + shadcn 风格组件）
│  ├─ components/
│  │  ├─ charts/TrafficChart.tsx
│  │  ├─ layout/{Sidebar,LogPanel}.tsx
│  │  └─ ui/                  # button / card / input / switch / tabs / dialog / badge / toast
│  ├─ hooks/useClashData.ts   # TanStack Query 数据层
│  ├─ lib/{api,types,format,utils,queryClient}.ts
│  ├─ pages/                  # Overview / Proxies / Connections / Rules / Subscriptions / Settings
│  ├─ store/appStore.ts       # Zustand（UI 状态、事件订阅、日志缓冲）
│  ├─ App.tsx  main.tsx  index.css
├─ src-tauri/
│  ├─ src/
│  │  ├─ lib.rs               # 应用装配：插件、单实例、托盘、命令注册、setup、退出清理
│  │  ├─ main.rs              # 入口（Windows 下隐藏控制台）
│  │  ├─ commands.rs          # 全部 #[tauri::command]
│  │  ├─ state.rs             # AppState（设置 / 订阅 / 内核状态 / 流量 / 日志）
│  │  ├─ models.rs            # 数据模型与默认值
│  │  ├─ error.rs             # AppError（thiserror）+ AppResult
│  │  ├─ storage.rs           # 目录解析、原子读写
│  │  ├─ mihomo_guard.rs      # 内核守护（启动 / 健康检查 / 崩溃重启 / 优雅退出）
│  │  ├─ clash_api.rs         # REST 客户端 + WebSocket 订阅
│  │  ├─ subscription.rs      # 订阅解析与运行配置生成
│  │  ├─ subscription_service.rs # 订阅导入 / 更新 / 切换 / 定时任务
│  │  ├─ system_proxy.rs      # 系统代理（Windows / Linux 双实现）
│  │  ├─ tun.rs               # TUN 权限探测（Windows 管理员 + wintun.dll / Linux root + CAP_NET_ADMIN）与状态组装
│  │  ├─ tray.rs              # 系统托盘
│  │  └─ time.rs
│  ├─ capabilities/default.json
│  ├─ binaries/               # 内核 sidecar（命名规则见该目录 README）
│  ├─ icons/
│  ├─ Cargo.toml  build.rs  tauri.conf.json
├─ scripts/                   # 本地打包 / 内核准备脚本（PowerShell + Bash）
├─ .github/workflows/build.yml # 双平台打包 CI
├─ package.json  vite.config.ts  tsconfig.json  tailwind.config.js  postcss.config.js
```

---

## 环境要求

| 项目    | 要求                                                                                                                                                                                                                     |
| ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Node.js | ≥ 18（推荐 20 / 22）                                                                                                                                                                                                     |
| Rust    | **≥ 1.90**（受 tauri 2.12 / tauri-build 2.7 / muda 0.20 等依赖的 MSRV 约束，低于此版本会在解析或编译依赖时直接报错）。Windows 可用 `x86_64-pc-windows-msvc`（推荐）或 `x86_64-pc-windows-gnu`（需自备 mingw-w64 链接器） |
| Windows | Windows 10/11 + WebView2 Runtime（Win11 自带；Win10 若缺失安装器会自动下载 bootstrapper）+ MSVC Build Tools                                                                                                              |
| Linux   | `libwebkit2gtk-4.1-dev`、`libgtk-3-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、`build-essential`、`pkg-config`（Debian/Ubuntu 示例，见下）                                                                     |

```bash
# Ubuntu / Debian
sudo apt update
sudo apt install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf build-essential curl file libssl-dev
```

> 注意：Tauri 2 使用 `webkit2gtk-4.1`；若系统只有 4.0，请升级发行版或改用对应版本依赖。

---

## 内核二进制放置（必读）

应用运行时通过 sidecar 启动内核，仓库**不包含**内核二进制，需自行下载并按目标三元组命名放入 `src-tauri/binaries/`：

| 平台                | 文件名                              |
| ------------------- | ----------------------------------- |
| Windows x64（MSVC） | `mihomo-x86_64-pc-windows-msvc.exe` |
| Windows x64（GNU）  | `mihomo-x86_64-pc-windows-gnu.exe`  |
| Linux x64           | `mihomo-x86_64-unknown-linux-gnu`   |
| Linux arm64         | `mihomo-aarch64-unknown-linux-gnu`  |

> 文件名后缀必须与 `rustc -vV` 输出的 `host:` 完全一致，否则 `tauri build` 会报 `failed to bundle external binary`。

获取地址：<https://github.com/MetaCubeX/mihomo/releases>

一键准备（自动下载 + 解压 + 重命名）。`npm run tauri:dev` 与 `npm run tauri:build` 已内置该步骤，内核已存在时会瞬间跳过：

```bash
npm run prepare:sidecar                      # 跨平台统一入口（Windows 调 ps1，Linux/macOS 调 sh）
npm run prepare:sidecar -- --force v1.19.0   # 强制重新下载并指定内核版本
```

```powershell
# 也可直接调用平台脚本
pwsh ./scripts/prepare-sidecar.ps1 -Version v1.19.0
```

```bash
# Linux
chmod +x ./scripts/prepare-sidecar.sh
./scripts/prepare-sidecar.sh v1.19.0
```

详细规则见 [`src-tauri/binaries/README.md`](src-tauri/binaries/README.md)。

运行时也可以不使用随包内核：在「设置 → 内核路径」填入本地 `mihomo` 可执行文件的绝对路径即可，守护逻辑会优先使用该路径。

---

## 开发

```bash
npm install          # 安装前端依赖
npm run tauri:dev    # 启动桌面应用（Vite + Rust 热重载）
npm run dev          # 仅启动前端（浏览器内调试，无 Tauri API）
npm run typecheck    # TypeScript 类型检查
```

首次 `npm run tauri:dev` 会先准备内核 sidecar（缺失时自动下载到 `src-tauri/binaries/`），随后编译 Rust 依赖，耗时较长属于正常现象。若内网限制导致下载失败，可手动放置内核文件后重试。

## 构建

```bash
npm run tauri:build            # 产出安装包 / 可执行文件
# 或使用本地打包脚本（会先准备内核）
pwsh ./scripts/build.ps1       # Windows
./scripts/build.sh             # Linux
```

产物位置：

| 平台    | 路径                                                                |
| ------- | ------------------------------------------------------------------- |
| Windows | `src-tauri/target/release/bundle/msi/`、`.../nsis/`                 |
| Linux   | `src-tauri/target/release/bundle/deb/`、`.../appimage/`、`.../rpm/` |

## CI 与发布

工程使用两个职责分离的工作流（原 `build.yml` 已拆分合并进这两个文件，仓库中不再单独存在）：

| 工作流                          | 触发                                          | 职责                                                                                                                                                                                                                                                                                                                                           |
| ------------------------------- | --------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `.github/workflows/ci.yml`      | PR、`main` 推送、手动                         | **只验证、不产包**。9 个 job：`secret-scan`（gitleaks）/ `rust-fmt` / `rust-clippy`（`-D warnings`）/ `rust-test` / `version-consistency` / `frontend-typecheck` / `frontend-lint` / `frontend-test` / `audit`（`cargo-audit` + `cargo-deny`）。Windows + Ubuntu 双平台矩阵，启用 `Swatinem/rust-cache` 与 npm 缓存，失败时上传日志 artifact。 |
| `.github/workflows/release.yml` | 打 `v*` 标签（手动触发只产 artifact、不发版） | **打包并发布**。Windows x64（nsis / msi）+ Linux x64（deb / rpm / AppImage）矩阵构建，自动准备 mihomo sidecar，生成 `SHA256SUMS` 与 `SIDECAR-<三元组>.sha256`，创建 GitHub Release 并挂载全部产物。tag 与三处版本号不一致时构建直接失败。                                                                                                      |

发版步骤、产物校验与回滚方案见 [docs/RELEASE.md](docs/RELEASE.md)；本地开发与调试见 [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)。

---

## 分发策略：mihomo sidecar 与 wintun.dll

| 资源        | 是否随安装包       | 具体做法                                                                                                                                                                                                                                                                         |
| ----------- | ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| mihomo 内核 | 随包（构建时下载） | `npm run tauri:build` 与 `release.yml` 都会先跑 `scripts/prepare-sidecar.*`，按 `MIHOMO_VERSION`（默认 `v1.19.0`）下载对应三元组的内核并重命名；产物中 `SIDECAR-<三元组>.sha256` 记录其 SHA256 与版本号，便于事后核验来源。                                                      |
| wintun.dll  | **默认不随包**     | 仓库不存放该文件。仅当构建机自备 `src-tauri/binaries/wintun.dll` 且通过 `scripts/verify-wintun-sign.ps1` 的 Authenticode 签名校验（预期签发者 `WireGuard LLC`）时才随包，并在 Release 说明中写入其 SHA256；未提供时构建脚本会移除该资源映射，产物中附 `wintun-status.txt` 说明。 |

### 未随包时的首次启动引导

Windows 产物若不含 `wintun.dll`，应用首次启动时按以下行为处理（不会阻断主流程）：

1. 启动时按「`clash-gui.exe` 同目录 → 应用配置目录 → 应用数据目录」顺序探测 `wintun.dll`；
2. 未找到时，设置页「运行状态」区块以警告样式展示缺失原因与下载指引（<https://www.wintun.net/>）；
3. 此时开启 TUN 会被拒绝并提示先放置驱动；系统代理功能不受影响，可正常使用；
4. `settings.json` 中的 `tunEnabled` **不会被改写**，放好驱动后重启应用即可自动恢复 TUN。

### 手动放置步骤（当前唯一方式）

**应用内自动下载当前未实现**，需手动完成：

1. 打开 <https://www.wintun.net/>，下载与系统架构匹配的 `wintun-<版本>.zip`（x64 机器取 `bin/amd64/wintun.dll`，ARM64 取 `bin/arm64/wintun.dll`）；
2. 解压后把 `wintun.dll` 复制到 `clash-gui.exe` 同目录（或应用配置目录 / 数据目录，三者任选其一）；
3. 重启应用，设置页「运行状态」应显示探测到的 `wintun.dll` 绝对路径；
4. 建议记录该 DLL 的 SHA256（`certutil -hashfile wintun.dll SHA256`）以便比对官方发布值。

> 应用不会校验用户手动放入的 DLL 签名（校验发生在**构建期**，见上表）；请只使用官方来源，避免放入来路不明的驱动。

---

## 数据与配置位置

| 内容         | 路径                                                                                                             |
| ------------ | ---------------------------------------------------------------------------------------------------------------- |
| 应用设置     | Windows：`%APPDATA%\com.clashgui.desktop\settings.json`<br>Linux：`~/.config/com.clashgui.desktop/settings.json` |
| 订阅清单     | 同目录 `subscriptions.json`                                                                                      |
| 订阅原始文件 | 同目录 `profiles/<订阅ID>.yaml`                                                                                  |
| 运行时配置   | 同目录 `config.yaml`（由应用生成，交给 mihomo 使用）                                                             |
| 运行数据     | 同目录 `data/`                                                                                                   |

默认端口：混合代理 `7890`、控制面 `9090`（控制密钥默认为空，建议在设置中填写；该密钥同时用于 REST 与 WebSocket 鉴权）。

---

## 系统代理说明

- **Windows**：写入 `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings` 的 `ProxyEnable` / `ProxyServer` / `ProxyOverride`，并调用 `InternetSetOptionW`（`SETTINGS_CHANGED` + `REFRESH`）让 WinINet 立即生效。开启前会备份原值到 `system_proxy_backup.json`，关闭或退出时按备份恢复；若上次异常退出，启动时会自动自愈。
- **Linux**：
  - GNOME：`gsettings set org.gnome.system.proxy mode 'manual'` + `http` / `https` / `socks` 的 host、port 与 `ignore-hosts`；
  - KDE：改写 `~/.config/kioslaverc` 的 `[Proxy Settings]`（`ProxyType=1` 及各 `*Proxy=` 项）；
  - 两者都不可用时返回 `backend = unsupported` 与明确提示，前端在设置页以警告样式展示，指引手动把代理指向 `127.0.0.1:7890`。
- 退出与崩溃场景：正常退出走 `RunEvent::Exit` 清理；异常退出在下次启动的 `recover_on_startup` 中按用户期望状态恢复；仅当本次运行确实开启过系统代理时才会在退出时改写系统设置，不会误关用户原有代理。

---

## TUN 模式说明

TUN 通过虚拟网卡在系统层接管流量，可代理不遵循系统代理设置的进程（如部分游戏、命令行工具、容器）。本应用只负责生成 `tun` 配置段、探测权限并按结果降级，虚拟网卡的创建仍由 mihomo 内核完成。

### 生成的配置段

设置页开启 TUN 后写入运行配置（`config.yaml`），字段与 mihomo 一致：

| 字段                    | 取值                                        | 说明                                                                       |
| ----------------------- | ------------------------------------------- | -------------------------------------------------------------------------- |
| `enable`                | `true`                                      | 仅开启时生成整个 `tun` 段，关闭时配置中不出现 `tun` 字段                   |
| `stack`                 | `gvisor` / `system` / `mixed`               | 协议栈实现；默认 `gvisor`（兼容性最好），`system` 性能更高但对系统要求更严 |
| `device`                | 默认 `Mihomo`（Windows）/ `mihomo`（Linux） | 虚拟网卡名，可在设置页自定义，留空回落平台默认值                           |
| `auto-route`            | 默认 `true`                                 | 自动接管系统路由表                                                         |
| `auto-detect-interface` | 默认 `true`                                 | 自动探测并绑定出口网卡                                                     |
| `strict-route`          | 默认 `false`                                | 严格路由，阻断绕开 TUN 的流量泄漏；开启后可能影响同机虚拟机 / 局域网访问   |
| `dns-hijack`            | 默认 `["any:53"]`                           | 把 DNS 查询劫持给内核内置 DNS                                              |

> TUN 开启时应用会自动把 `dns.enable` 置为 `true`，并在缺失时补齐 `enhanced-mode: fake-ip` 与 `fake-ip-range`。否则 DNS 查询被劫持后没有处理方，会出现「能连上但打不开网页」的现象。

### Windows 前置条件

1. **管理员权限**：创建虚拟网卡需要提权。请在启动程序时选择「以管理员身份运行」（会弹 UAC）。应用通过 PowerShell 读取当前进程令牌判断是否已提升，未提升时设置页显示红色徽章与指引，开启 TUN 会被拒绝。
2. **`wintun.dll`**：mihomo 在 Windows 上使用 TUN 依赖 Wintun 驱动库，仓库不分发该文件。获取方式：
   - 打开 <https://www.wintun.net/>，下载最新版 `wintun-<版本>.zip`；
   - 解压后把 `bin/amd64/wintun.dll`（ARM64 机器取 `bin/arm64/`）放到 **`clash-gui.exe` 同目录**，或放到应用配置目录 / 数据目录（三者任选其一，应用按此顺序查找）；
   - 设置页的「运行状态」区块会显示探测到的 `wintun.dll` 绝对路径，未找到时给出下载指引。
3. 建议在路由器 / 防火墙层面确认没有第三方安全软件拦截虚拟网卡驱动加载。

### Linux 前置条件

TUN 需要 `CAP_NET_ADMIN`（创建网卡与改路由）+ `CAP_NET_RAW`（部分场景）。应用读取 `/proc/self/status` 的 `Uid` 与 `CapEff` 判断，`euid == 0` 或 `CapEff` 的 bit 12（`CAP_NET_ADMIN`）置位即视为满足。三种授权方式任选其一：

```bash
# ① 临时验证：直接用 root 启动
sudo ./clash-gui

# ② 长期方案（推荐）：给可执行文件授予能力，普通用户启动即可
sudo setcap cap_net_admin,cap_net_raw+eip "$(which clash-gui)"

# ③ systemd 方案：在 service 单元中授予
# [Service]
# AmbientCapabilities=CAP_NET_ADMIN CAP_NET_RAW
# CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_RAW
```

> 使用 `setcap` 后不要再用 `sudo` 启动，否则运行用户与能力来源不一致。升级 / 重装二进制后能力会丢失，需重新执行 `setcap`。

### 权限不足时的行为（降级而非失败）

配置生成（`mihomo_guard::write_config`）在写出 `config.yaml` 前会先探测权限：

- 满足 → 正常写入含 `tun` 段的配置；
- 不满足（无管理员 / 无 `CAP_NET_ADMIN` / 缺 `wintun.dll`）→ **本次运行**把配置降级为不含 `tun` 段的内容，内核仍可正常启动并继续提供系统代理能力，同时写入一条 `warning` 日志说明原因；
- **用户设置不被修改**：`settings.json` 里的 `tunEnabled` 保持 `true`，下次以正确权限启动时会自动恢复 TUN。

### 相关命令

| 命令                  | 前端调用                  | 说明                                                                                                                                                         |
| --------------------- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `tun_status`          | `api.tunStatus()`         | 返回 `TunState`：`supported` / `enabled` / `active` / `stack` / `deviceName` / `privilege` / `message`；内核运行中时 `active` 以 `/configs` 返回的实际值为准 |
| `tun_check_privilege` | `api.tunCheckPrivilege()` | 仅探测并返回 `TunPrivilege`（`elevated` / `mechanism` / `wintunPresent` / `detail` / `hint`），不修改任何状态                                                |
| `tun_toggle`          | `api.tunToggle(enabled)`  | 持久化 `tunEnabled` → 重写配置 → 内核运行中则调用 `PUT /configs` 热重载；权限不满足时返回带指引的错误                                                        |

---

## 常见问题

**Q：启动后提示"内核未就绪"？**
检查 `src-tauri/binaries/` 下是否正确命名了内核文件、Linux 是否加了可执行权限，或在设置中指定内核路径。

**Q：Windows 打包报 `failed to bundle external binary`？**
内核文件名缺少目标三元组后缀，参考上文命名表。

**Q：Linux 设置页提示系统代理不可用？**
当前桌面环境既没有 `gsettings`（GNOME 系）也没有 `~/.config/kioslaverc`（KDE），属于明确降级场景，请手动配置系统代理或在应用内使用端口 `7890`。

**Q：开启 TUN 后提示权限不足 / 内核以非 TUN 模式启动？**
Windows：以管理员身份重新启动程序（会弹 UAC），并确认 `wintun.dll` 已放到 `clash-gui.exe` 同目录（或配置目录 / 数据目录）。Linux：用 `sudo` 启动，或按上文执行 `setcap cap_net_admin,cap_net_raw+eip`。日志面板会出现一条 `warning`，内容即具体缺失项；本次运行会降级为系统代理，设置里的 TUN 开关保持开启，下次以正确权限启动即恢复。

**Q：TUN 显示已开启但部分应用仍不走代理？**
先看设置页「运行状态」中的 `active` 是否为真、`stack` 与 `device` 是否符合预期。若使用的是 `system` 栈，部分系统（尤其旧内核）可能不兼容，可切到 `gvisor`；若存在绕过 TUN 的流量（如绑定物理网卡的进程），可开启「严格路由」；同时确认没有同时启用系统代理导致两套接管叠加冲突。

**Q：Linux 上 `setcap` 后仍提示权限不足？**
`setcap` 记录在文件 inode 上，重新安装 / 覆盖二进制后能力会丢失，需重新执行；另外不要用 `sudo` 与 `setcap` 混用，并确认挂载 `/tmp`、家目录时没有 `nosuid`/能力被清除的限制。

---

## 技术栈

Tauri 2 · React 19 · TypeScript 5 · Vite 6 · Tailwind CSS 3 · Zustand 5 · TanStack Query 5 · lucide-react · serde / serde_yaml / reqwest / tokio / tokio-tungstenite / thiserror / winreg / windows-sys

## 许可

MIT
