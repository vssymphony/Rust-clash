# 下载并准备 mihomo 内核 sidecar（Windows）
#
# 用法：
#   pwsh ./scripts/prepare-sidecar.ps1
#   pwsh ./scripts/prepare-sidecar.ps1 -Version v1.19.0
#   pwsh ./scripts/prepare-sidecar.ps1 -Triple x86_64-pc-windows-msvc -Force
#   pwsh ./scripts/prepare-sidecar.ps1 -Source .\downloads\mihomo.exe
#   pwsh ./scripts/prepare-sidecar.ps1 -Url https://mirror.example.com/mihomo.zip -Sha256 <hash>
#
# 作用：
#   1. 解析目标三元组：优先 -Triple，其次 rustc -vV 的 host；MSVC 为默认/推荐目标；
#   2. 从 GitHub Releases（或 -Url / MIHOMO_DOWNLOAD_BASE 镜像 / 本地文件）取得内核；
#   3. 重命名为 Tauri externalBin 要求的 “mihomo-<三元组>.exe” 并放入 src-tauri/binaries；
#   4. 提供 -Sha256（或环境变量 MIHOMO_SHA256）时强制校验，不匹配即失败。
#
# 为什么三元组必须与实际编译 host 一致：
#   Tauri 在 bundle 阶段按「externalBin 文件名 + - + 编译时 host 三元组」查找文件，
#   MSVC 工具链下就必须是 mihomo-x86_64-pc-windows-msvc.exe，缺后缀或后缀不符会直接
#   报 failed to bundle external binary。

[CmdletBinding()]
param(
    [string]$Version = "v1.19.0",
    [string]$Source = "",
    [string]$Triple = "",
    [string]$Url = "",
    [string]$Sha256 = "",
    [switch]$Force
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$binDir = Join-Path $repoRoot "src-tauri\binaries"

if (-not (Test-Path $binDir)) {
    New-Item -ItemType Directory -Path $binDir | Out-Null
}

function Get-RustHostTriple {
    if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
        return ""
    }
    $rustInfo = & rustc -vV
    $line = $rustInfo | Where-Object { $_ -like "host:*" } | Select-Object -First 1
    if (-not $line) { return "" }
    return ($line -replace "^host:\s*", "").Trim()
}

function Assert-Sha256 {
    param([string]$Path, [string]$Expected)
    if ([string]::IsNullOrWhiteSpace($Expected)) { return }
    $actual = (Get-FileHash -Algorithm SHA256 -Path $Path).Hash.ToLower()
    $expectedNormalized = ($Expected -replace "^\s*|\s*$", "").ToLower()
    if ($actual -ne $expectedNormalized) {
        throw ("SHA256 校验失败：期望 $expectedNormalized，实际 $actual。" +
            "内核文件可能被篡改或版本不匹配，已中止。")
    }
    Write-Host "SHA256 校验通过：$actual"
}

# ---------------------------------------------------------------- 目标三元组
$hostTriple = Get-RustHostTriple
if ($Triple -ne "") {
    $targetTriple = $Triple
    if ($hostTriple -ne "" -and $hostTriple -ne $Triple) {
        Write-Warning ("显式指定的 -Triple（$Triple）与 rustc host（$hostTriple）不一致。" +
            "sidecar 文件名必须与编译 host 完全一致，否则 tauri-build 会报 externalBin 缺失。")
    }
}
elseif ($hostTriple -ne "") {
    $targetTriple = $hostTriple
}
else {
    # 无 rustc（如精简 CI 环境）时按项目默认目标推断，避免直接抛错中断流水线
    $targetTriple = "x86_64-pc-windows-msvc"
    Write-Warning ("未找到 rustc，无法读取 host，按默认目标 $targetTriple 命名。" +
        "若实际使用 GNU 工具链请显式传 -Triple x86_64-pc-windows-gnu。")
}

Write-Host "目标三元组：$targetTriple"
if ($targetTriple -like "*-windows-gnu") {
    Write-Warning ("当前定位到 GNU 工具链。工程默认与 CI 均为 MSVC：" +
        "rustup default stable-x86_64-pc-windows-msvc，并用 -Triple 显式传入。")
}

$extension = ".exe"
$targetPath = Join-Path $binDir ("mihomo-$targetTriple" + $extension)

if ((Test-Path $targetPath) -and -not $Force) {
    Write-Host "已存在：$targetPath（如需覆盖请加 -Force）"
    exit 0
}

# ---------------------------------------------------------------- 本地文件直取
if ($Source -ne "") {
    if (-not (Test-Path $Source)) { throw "指定的内核文件不存在：$Source" }
    Copy-Item $Source $targetPath -Force
    Assert-Sha256 -Path $targetPath -Expected $(if ($Sha256 -ne "") { $Sha256 } else { $env:MIHOMO_SHA256 })
    Write-Host "已复制本地内核 → $targetPath"
    exit 0
}

# ---------------------------------------------------------------- 解析下载地址
switch -Regex ($targetTriple) {
    "^x86_64-pc-windows" { $arch = "amd64" }
    "^aarch64-pc-windows" { $arch = "arm64" }
    default {
        throw ("暂不支持的目标三元组：$targetTriple。" +
            "请用 -Source 指定本地内核文件，或按 src-tauri/binaries/README.md 手动命名放置。")
    }
}

$asset = "mihomo-windows-$arch-$Version.zip"
if ($Url -eq "") {
    if ($env:MIHOMO_DOWNLOAD_BASE) {
        $Url = ($env:MIHOMO_DOWNLOAD_BASE.TrimEnd("/")) + "/" + $asset
    }
    else {
        $Url = "https://github.com/MetaCubeX/mihomo/releases/download/$Version/$asset"
    }
}

$expectedHash = if ($Sha256 -ne "") { $Sha256 } else { $env:MIHOMO_SHA256 }
if ([string]::IsNullOrWhiteSpace($expectedHash)) {
    Write-Warning ("未提供 -Sha256 / MIHOMO_SHA256，本次跳过哈希校验。" +
        "公开 CI 建议在仓库 secret 中配置 MIHOMO_SHA256 以固定内核字节。")
}

$work = Join-Path $env:TEMP ("mihomo-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $work | Out-Null

try {
    $zip = Join-Path $work $asset
    Write-Host "下载：$Url"
    try {
        Invoke-WebRequest -Uri $Url -OutFile $zip -UseBasicParsing
    }
    catch {
        throw ("下载失败：$Url`n原因：$($_.Exception.Message)`n可选处理：`n" +
            "  1) 用 -Url 指向可访问的镜像，或设置环境变量 MIHOMO_DOWNLOAD_BASE；`n" +
            "  2) 手动下载后通过 -Source 指定本地文件；`n" +
            "  3) 按 src-tauri/binaries/README.md 的命名规则自行放置。")
    }

    Assert-Sha256 -Path $zip -Expected $expectedHash

    Expand-Archive -Path $zip -DestinationPath $work -Force

    $exe = Get-ChildItem -Path $work -Recurse -Filter "mihomo*.exe" |
        Where-Object { $_.Name -notlike "*.old*" } |
        Sort-Object Length -Descending |
        Select-Object -First 1
    if (-not $exe) { throw "压缩包中未找到 mihomo 可执行文件，请确认下载源与版本号（$Version）是否正确。" }

    Copy-Item $exe.FullName $targetPath -Force
    $actual = (Get-FileHash -Algorithm SHA256 -Path $targetPath).Hash.ToLower()
    Write-Host "已写入：$targetPath"
    Write-Host "内核 SHA256：$actual"
}
finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}
