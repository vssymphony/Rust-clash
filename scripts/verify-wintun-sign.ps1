<#
.SYNOPSIS
    校验 wintun.dll 是否为官方签名版本（发布前手工 / CI 执行）。

.DESCRIPTION
    `crate::wintun` 的运行时校验只能做到「体积 + PE 头 + 架构 + 版本号」四级静态检查，
    这些检查无法证明 DLL 真的来自官方发布（攻击者可以构造出体积、架构、版本号都合规的
    恶意 DLL）。Authenticode 签名校验依赖 Windows 的证书链与受信任根，必须在
    PowerShell / WinTrust API 上执行，因此拆成发布前的独立校验步骤：

      1. 运行时（wintun.rs）：体积 / MZ+PE 头 / 架构 / 版本 —— 拦截下载错误、压缩包
         误放、架构错配、版本过旧这几类最常见的事故；
      2. 发布前（本脚本）：Authenticode 签名 + 签名主体 —— 确认 DLL 由 WireGuard LLC
         签发，拦截被替换 / 被植入的 DLL。

    两者的组合覆盖了 wintun.dll 供应链上的主要风险；运行时不调用 WinTrust 的原因是
    Tauri 侧没有对应依赖，且签名校验在每次启动时执行会带来明显的启动延迟。

.PARAMETER DllPath
    wintun.dll 的路径，默认为脚本同级的 ..\src-tauri\binaries\wintun.dll。

.PARAMETER ExpectedSubject
    期望的签名主体关键字，默认 "WireGuard LLC"（wintun 官方发行者）。

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-wintun-sign.ps1
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-wintun-sign.ps1 -DllPath D:\tools\wintun.dll
#>
param(
    [string]$DllPath = (Join-Path $PSScriptRoot "..\src-tauri\binaries\wintun.dll"),
    [string]$ExpectedSubject = "WireGuard LLC"
)

$ErrorActionPreference = "Stop"

function Fail([string]$Message) {
    Write-Host "[verify-wintun-sign] 失败：$Message" -ForegroundColor Red
    exit 1
}

if (-not (Test-Path -LiteralPath $DllPath)) {
    Fail "未找到 $DllPath。请从 https://www.wintun.net/ 下载对应架构的 wintun.dll 后重试；若要跳过签名校验，请在发布说明中明确记录该降级。"
}

$file = Get-Item -LiteralPath $DllPath
Write-Host "[verify-wintun-sign] 校验文件：$($file.FullName)（$($file.Length) 字节）"

$signature = Get-AuthenticodeSignature -LiteralPath $file.FullName

if ($signature.Status -ne "Valid") {
    $reason = $signature.StatusMessage
    Fail "Authenticode 签名不可信（状态 $($signature.Status)：$reason）。可能是被替换 / 修改过的 DLL，或本机缺少联网校验所需的根证书。请重新从官方站点下载。"
}

$subject = $signature.SignerCertificate.Subject
Write-Host "[verify-wintun-sign] 签名主体：$subject"
Write-Host "[verify-wintun-sign] 签发者：$($signature.SignerCertificate.Issuer)"

if ($subject -notlike "*$ExpectedSubject*") {
    Fail "签名主体不是期望的 '$ExpectedSubject'，实际为 '$subject'。请确认使用的是官方发行版本。"
}

Write-Host "[verify-wintun-sign] 通过：wintun.dll 由官方主体签名且证书链可验证。" -ForegroundColor Green
exit 0
