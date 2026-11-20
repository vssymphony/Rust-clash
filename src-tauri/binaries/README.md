# 内核二进制存放目录（sidecar）

Tauri 通过 `bundle.externalBin` 把内核作为 sidecar 随包分发，配置项为：

```json
"externalBin": ["binaries/mihomo"]
```

Tauri 会按「**文件名 + `-` + 目标三元组（含扩展名）**」的规则查找文件，因此本目录下的内核必须按下表命名，缺少后缀或后缀不匹配都会导致打包报错 `failed to bundle external binary`。

## 命名规则

| 目标平台 | 必须的文件名 |
| --- | --- |
| Windows x64 (MSVC) | `mihomo-x86_64-pc-windows-msvc.exe` |
| Windows x64 (GNU) | `mihomo-x86_64-pc-windows-gnu.exe` |
| Windows arm64 | `mihomo-aarch64-pc-windows-msvc.exe` |
| Linux x64 (glibc) | `mihomo-x86_64-unknown-linux-gnu` |
| Linux arm64 (glibc) | `mihomo-aarch64-unknown-linux-gnu` |

> 目标三元组可用 `rustc -vV | grep host`（Windows 用 `rustc -vV | Select-String host`）查到，务必与本机实际编译目标一致。

## 获取方式

1. 打开 <https://github.com/MetaCubeX/mihomo/releases>，下载对应平台的稳定版压缩包：
   - Windows：`mihomo-windows-amd64-vX.Y.Z.zip`，解压得到 `mihomo.exe`
   - Linux：`mihomo-linux-amd64-vX.Y.Z.gz`，解压得到 `mihomo`
2. 按上表重命名后放入本目录。
3. Linux 下还需补充可执行权限：`chmod +x src-tauri/binaries/mihomo-*`。

也可以直接用脚本自动下载并重命名：

```bash
# 跨平台统一入口（推荐）：Windows 内部调 ps1，Linux/macOS 内部调 sh
npm run prepare:sidecar
npm run prepare:sidecar -- --force v1.19.0    # 强制重下 + 指定版本

# 也可直接调用平台脚本
pwsh ./scripts/prepare-sidecar.ps1 -Version v1.19.0
./scripts/prepare-sidecar.sh v1.19.0
```

> `npm run tauri:dev` 与 `npm run tauri:build` 已内置该步骤，内核文件存在时会直接跳过。

## 说明

- 本目录中的内核二进制体积较大，已在 `.gitignore` 中忽略，请勿提交到仓库。
- 运行时也可以不使用随包内核：在「设置 → 内核路径」里指定本地 `mihomo` 可执行文件的绝对路径，内核守护会优先使用该路径，找不到时才回退到 sidecar。
- 内核仅以子进程方式运行，应用本身不实现任何代理协议栈。
