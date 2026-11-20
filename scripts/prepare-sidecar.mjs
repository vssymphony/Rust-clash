#!/usr/bin/env node
/**
 * 跨平台 sidecar 准备统一入口。
 *
 * 用法：
 *   node scripts/prepare-sidecar.mjs                 # 已存在则跳过，否则按平台下载
 *   node scripts/prepare-sidecar.mjs --force         # 强制重新下载
 *   node scripts/prepare-sidecar.mjs v1.19.0         # 指定内核版本
 *   node scripts/prepare-sidecar.mjs --force v1.19.0
 *   node scripts/prepare-sidecar.mjs --triple x86_64-pc-windows-msvc --force
 *   node scripts/prepare-sidecar.mjs --url <镜像地址> --sha256 <hash>
 *
 * 说明：
 *   - Tauri externalBin 要求 src-tauri/binaries/mihomo-<host-triple>[.exe] 存在，
 *     否则 `cargo build` 在 tauri-build 阶段直接失败；因此 `tauri:dev` / `tauri:build`
 *     均先执行本脚本。
 *   - 本脚本只负责「是否需要准备」与「平台分派」，真正的下载逻辑在
 *     scripts/prepare-sidecar.ps1（Windows）与 scripts/prepare-sidecar.sh（Linux/macOS）。
 */
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptsDir = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(scriptsDir, "..");
const binDir = path.join(projectRoot, "src-tauri", "binaries");

const rawArgs = process.argv.slice(2);
const force = rawArgs.includes("--force") || rawArgs.includes("-f");
const valueOptions = ["--triple", "--url", "--sha256"];
// 支持 `--key value` 与 `--key=value` 两种写法；其余参数（内核版本号）作为位置参数透传。
const optionValues = {};
const positional = [];
for (let i = 0; i < rawArgs.length; i += 1) {
  const arg = rawArgs[i];
  if (arg === "--force" || arg === "-f") continue;
  const eq = arg.indexOf("=");
  const key = eq > 0 ? arg.slice(0, eq) : arg;
  if (valueOptions.includes(key)) {
    optionValues[key] = eq > 0 ? arg.slice(eq + 1) : (rawArgs[i + 1] ?? "");
    if (eq < 0) i += 1;
    continue;
  }
  positional.push(arg);
}

function log(message) {
  console.log(`[prepare-sidecar] ${message}`);
}

function fail(message) {
  console.error(`[prepare-sidecar] ${message}`);
  process.exit(1);
}

// 判断 tauri.windows.conf.json 是否仍把 wintun.dll 声明为打包资源。
// 文件缺失（CI 未自备 DLL 时会被删除）或未引用该文件时返回 false。
function wintunResourceMapped() {
  const confPath = path.join(projectRoot, "src-tauri", "tauri.windows.conf.json");
  if (!existsSync(confPath)) {
    return false;
  }
  try {
    const conf = JSON.parse(readFileSync(confPath, "utf8"));
    const resources = conf?.bundle?.resources;
    if (!resources) {
      return false;
    }
    const values = Array.isArray(resources) ? resources : Object.values(resources);
    return values.some((value) => typeof value === "string" && /wintun\.dll$/i.test(value));
  } catch (error) {
    // 配置解析失败交由 tauri-build 报告，这里不作为 wintun 缺失的判据
    log(`警告：解析 tauri.windows.conf.json 失败（${error.message}），按未映射 wintun.dll 处理。`);
    return false;
  }
}

// Windows 前置检查：wintun.dll 由 tauri.windows.conf.json 的 bundle.resources
// 映射到安装根目录（即 exe 同级，对应 wintun.rs 的候选路径之一）。
// 该文件无法自动下载（需人工从官网取对应架构版本），因此缺失时在这里明确报错，
// 而不是留给 tauri-build 在 bundle 阶段抛出「resource not found」。
function checkWintun() {
  if (process.platform !== "win32") {
    return;
  }
  const wintun = path.join(binDir, "wintun.dll");
  if (!existsSync(wintun)) {
    // 资源映射不存在时无需要求 wintun.dll：CI 的「wintun 分发策略」步骤会在未自备
    // DLL 时删除 tauri.windows.conf.json，此处必须与之一致，否则 `tauri:build`
    // 会在 prepare:sidecar 阶段误报缺文件而中断流水线。
    if (!wintunResourceMapped()) {
      log(
        "未配置 wintun.dll 资源映射（src-tauri/tauri.windows.conf.json 不存在或未引用该文件），" +
          "本次构建不随包 TUN 驱动，跳过 wintun 检查。",
      );
      return;
    }
    if (process.env.CLASH_GUI_SKIP_WINTUN === "1") {
      log("警告：未找到 src-tauri/binaries/wintun.dll，已按 CLASH_GUI_SKIP_WINTUN=1 跳过检查。");
      log("      此时必须临时移除 src-tauri/tauri.windows.conf.json 中 wintun.dll 的资源映射，");
      log("      否则 bundle 阶段仍会因资源缺失失败；且打包产物无法使用 TUN 模式。");
      return;
    }
    fail(
      `缺少 ${wintun}。Windows 打包需要它：TUN 模式依赖 wintun 驱动，` +
        `且 tauri.windows.conf.json 已把该文件映射到安装根目录。` +
        `请从 https://www.wintun.net/ 下载与架构一致（通常为 amd64）的 wintun.dll 放入 ` +
        `src-tauri/binaries/，并执行 scripts/verify-wintun-sign.ps1 校验签名；` +
        `若本次构建明确不需要 TUN，可设置 CLASH_GUI_SKIP_WINTUN=1 并移除上述资源映射。`,
    );
    return;
  }
  log(
    `wintun.dll 就绪（${statSync(wintun).size} 字节）。发布前请执行 scripts/verify-wintun-sign.ps1 校验发行者签名。`,
  );
}

// 1. 已有 sidecar 且未要求强制刷新 → 直接跳过，避免每次构建重复下载。
if (!force && existsSync(binDir)) {
  const wantedTriple = optionValues["--triple"];
  const existing = readdirSync(binDir).filter((name) => {
    if (!name.startsWith("mihomo-") || name.endsWith(".md")) return false;
    // 显式指定三元组时只认对应命名的 sidecar，避免 GNU 产物被误判为已就绪
    return wantedTriple ? name.startsWith(`mihomo-${wantedTriple}`) : true;
  });
  if (existing.length > 0) {
    log(`已存在 sidecar：${existing.join(", ")}，跳过（如需重新下载请加 --force）`);
    checkWintun();
    process.exit(0);
  }
}

// 2. 平台分派到既有脚本，保持下载逻辑单一来源。
const isWindows = process.platform === "win32";
const shell = isWindows ? "powershell" : "bash";
const scriptPath = path.join(scriptsDir, isWindows ? "prepare-sidecar.ps1" : "prepare-sidecar.sh");
if (!existsSync(scriptPath)) {
  fail(`未找到脚本：${scriptPath}`);
}

const tripleArgs = optionValues["--triple"] ? [optionValues["--triple"]] : [];
const urlArgs = optionValues["--url"] ? [optionValues["--url"]] : [];
const shaArgs = optionValues["--sha256"] ? [optionValues["--sha256"]] : [];

const args = isWindows
  ? [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      scriptPath,
      ...(force ? ["-Force"] : []),
      ...(tripleArgs.length ? ["-Triple", ...tripleArgs] : []),
      ...(urlArgs.length ? ["-Url", ...urlArgs] : []),
      ...(shaArgs.length ? ["-Sha256", ...shaArgs] : []),
      ...positional,
    ]
  : [
      scriptPath,
      ...(tripleArgs.length ? ["--triple", ...tripleArgs] : []),
      ...(urlArgs.length ? ["--url", ...urlArgs] : []),
      ...(shaArgs.length ? ["--sha256", ...shaArgs] : []),
      ...positional,
      ...(force ? ["--force"] : []),
    ];

log(`执行：${shell} ${args.join(" ")}`);
const result = spawnSync(shell, args, { stdio: "inherit", cwd: projectRoot });

if (result.error) {
  fail(
    `无法启动 ${shell}：${result.error.message}。` +
      `Windows 需 PowerShell 5.1+，Linux/macOS 需 bash；也可手动下载内核后按 src-tauri/binaries/README.md 命名。`,
  );
}
if (result.status !== 0) {
  fail(
    `sidecar 准备失败（退出码 ${result.status}）。` +
      `可手动下载 mihomo 内核，按 src-tauri/binaries/README.md 中的三元组命名规则放入 src-tauri/binaries 后重试。`,
  );
}

// 3. 产物兜底校验：下载脚本退出码为 0 也必须确认文件真的落盘。
//    否则 tauri-build 会在更晚的阶段以「找不到 externalBin」的形式失败，定位成本更高。
const produced = existsSync(binDir)
  ? readdirSync(binDir).filter((name) => name.startsWith("mihomo-") && !name.endsWith(".md"))
  : [];
if (produced.length === 0) {
  fail(
    `脚本执行完毕但 ${binDir} 下仍没有 mihomo-<target-triple>[.exe] 文件。` +
      `请检查上面的下载输出（网络受限 / 代理拦截 / 版本号不存在都会导致空产物），` +
      `或手动下载内核并按 src-tauri/binaries/README.md 的三元组命名规则放置后重试。`,
  );
}

log(`已就绪的 sidecar：${produced.join(", ")}`);

// 4. Windows 额外前置检查（见文件头部 checkWintun 的说明）。
checkWintun();

log("完成。");
