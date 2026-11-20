# 本地一键打包脚本（Windows）
#
# 用法：
#   pwsh ./scripts/build.ps1
#   pwsh ./scripts/build.ps1 -KernelVersion v1.19.0
#   pwsh ./scripts/build.ps1 -SkipSidecar      # 已手动放好内核时使用
#
# 流程：校验环境 → 准备 mihomo sidecar → 安装前端依赖 → tauri build → 打印产物路径

[CmdletBinding()]
param(
    [string]$KernelVersion = "v1.19.0",
    [switch]$SkipSidecar,
    [switch]$ForceInstall
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

Write-Host "==> 仓库根目录：$repoRoot"

# 1. 环境校验
foreach ($tool in @("node", "npm", "rustc", "cargo")) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "缺少必需工具：$tool。请安装 Node.js ≥ 18 与 Rust ≥ 1.77.2。"
    }
}
Write-Host ("==> node {0} / npm {1}" -f (& node -v), (& npm -v))
$hostTriple = ((& rustc -vV | Where-Object { $_ -like "host:*" } | Select-Object -First 1) -replace "^host:\s*", "")
Write-Host ("==> rustc host: {0}" -f $hostTriple)
# 本工程统一以 MSVC 为目标（CI 与发布包命名均按 x86_64-pc-windows-msvc）。
# GNU 仅作可选兼容：host 为 GNU 时仍能构建，但产物命名与 CI 不一致，这里只告警不中断。
if ($hostTriple -notlike "*-msvc") {
    Write-Warning ("当前 rustc host 为 {0}，与项目默认目标 x86_64-pc-windows-msvc 不一致：" -f $hostTriple)
    Write-Warning "  sidecar 将按该 host 命名（mihomo-$hostTriple.exe），CI/发布产物使用 -msvc 命名。"
    Write-Warning "  如需对齐，请执行：rustup default stable-x86_64-pc-windows-msvc"
}

# 2. 准备内核 sidecar
if (-not $SkipSidecar) {
    $triple = $hostTriple
    $sidecar = Join-Path $repoRoot ("src-tauri\binaries\mihomo-" + $triple + ".exe")
    if (Test-Path $sidecar) {
        Write-Host "==> 内核已就绪：$sidecar"
    }
    else {
        Write-Host "==> 未找到内核，开始下载 $KernelVersion ..."
        & (Join-Path $PSScriptRoot "prepare-sidecar.ps1") -Version $KernelVersion
    }
}
else {
    Write-Host "==> 已跳过内核准备（-SkipSidecar）"
}

# 3. 前端依赖
if ($ForceInstall -or -not (Test-Path (Join-Path $repoRoot "node_modules"))) {
    if (Test-Path (Join-Path $repoRoot "package-lock.json")) {
        Write-Host "==> npm ci"
        & npm ci
    }
    else {
        Write-Host "==> npm install"
        & npm install
    }
    if ($LASTEXITCODE -ne 0) { throw "前端依赖安装失败（exit $LASTEXITCODE）" }
}
else {
    Write-Host "==> 前端依赖已存在，跳过安装（-ForceInstall 可强制重装）"
}

# 4. 构建
Write-Host "==> tauri build"
& npm run tauri:build
if ($LASTEXITCODE -ne 0) { throw "构建失败（exit $LASTEXITCODE）" }

# 5. 产物
$bundle = Join-Path $repoRoot "src-tauri\target\release\bundle"
Write-Host ""
Write-Host "==> 构建完成，产物目录：$bundle"
if (Test-Path $bundle) {
    Get-ChildItem -Path $bundle -Recurse -Include *.msi, *.exe, *.deb, *.AppImage, *.rpm |
        ForEach-Object { Write-Host ("    " + $_.FullName) }
}
