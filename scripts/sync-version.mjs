#!/usr/bin/env node
/**
 * 版本号单一来源校验 / 同步脚本
 *
 * 背景：本工程的版本号同时出现在三个文件里，任何一处不一致都会导致
 * 安装包版本、窗口标题、`tauri build` 产物名、Release tag 对不上：
 *
 *   - package.json          → 前端与 npm 生态（npm version 的权威来源）
 *   - src-tauri/Cargo.toml  → Rust crate（[package] version）
 *   - src-tauri/tauri.conf.json → Tauri 应用版本（打进安装包元数据）
 *
 * 以 package.json 为唯一权威来源，其余两处必须与它一致。
 *
 * 用法：
 *   node scripts/sync-version.mjs              # 校验（默认，CI 使用）；不一致 → 退出码 1
 *   node scripts/sync-version.mjs --check      # 同上，显式写法
 *   node scripts/sync-version.mjs --print      # 打印 package.json 的版本号（供 shell 取用）
 *   node scripts/sync-version.mjs --set 0.2.0  # 把三处统一写成 0.2.0
 *
 * 接入 npm 脚本：npm run version:check / npm run version:print / npm run version:set -- 0.2.0
 */

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(SCRIPT_DIR, "..");

const PKG_PATH = join(ROOT, "package.json");
const CARGO_PATH = join(ROOT, "src-tauri", "Cargo.toml");
const TAURI_CONF_PATH = join(ROOT, "src-tauri", "tauri.conf.json");

/** 语义化版本（允许 1.2.3 / 1.2.3-beta.1 / 1.2.3+build） */
const SEMVER_RE = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

/** 从 package.json 读取权威版本 */
function readPackageVersion() {
  const raw = readFileSync(PKG_PATH, "utf8");
  const pkg = JSON.parse(raw);
  if (typeof pkg.version !== "string" || !SEMVER_RE.test(pkg.version)) {
    fail([`package.json 的 version 非法：${JSON.stringify(pkg.version)}`]);
  }
  return pkg.version;
}

/**
 * 从 Cargo.toml 的 [package] 段读取 version。
 * 只取 [package] 与下一个段头之间的第一处 version，避免误读 [dependencies] 里的版本。
 */
function readCargoVersion() {
  const text = readFileSync(CARGO_PATH, "utf8");
  const pkgStart = text.indexOf("[package]");
  if (pkgStart < 0) {
    fail(["src-tauri/Cargo.toml 中找不到 [package] 段"]);
  }
  const rest = text.slice(pkgStart + "[package]".length);
  const nextSection = rest.search(/^\[/m);
  const section = nextSection >= 0 ? rest.slice(0, nextSection) : rest;
  const m = section.match(/^\s*version\s*=\s*"([^"]+)"/m);
  if (!m) {
    fail(["src-tauri/Cargo.toml 的 [package] 段中找不到 version 字段"]);
  }
  return m[1];
}

/** 从 tauri.conf.json 读取 version（顶层字段） */
function readTauriVersion() {
  const conf = JSON.parse(readFileSync(TAURI_CONF_PATH, "utf8"));
  if (typeof conf.version !== "string" || !SEMVER_RE.test(conf.version)) {
    fail([`src-tauri/tauri.conf.json 的 version 非法：${JSON.stringify(conf.version)}`]);
  }
  return conf.version;
}

function fail(lines) {
  for (const line of lines) console.error(`[版本校验] 失败：${line}`);
  process.exit(1);
}

function collect() {
  return [
    { file: "package.json", version: readPackageVersion(), authoritative: true },
    { file: "src-tauri/Cargo.toml", version: readCargoVersion(), authoritative: false },
    { file: "src-tauri/tauri.conf.json", version: readTauriVersion(), authoritative: false },
  ];
}

function check() {
  const entries = collect();
  const expected = entries[0].version;
  const mismatched = entries.filter((e) => e.version !== expected);

  if (mismatched.length > 0) {
    console.error(
      "[版本校验] 三处版本号不一致，请执行 `npm run version:set -- <版本>` 统一后才可提交：",
    );
    for (const e of entries) {
      const flag = e.version === expected ? "OK  " : "差异";
      console.error(`  ${flag} ${e.file.padEnd(28)} ${e.version}`);
    }
    console.error(`\n基准版本（package.json）：${expected}`);
    process.exit(1);
  }

  console.log("[版本校验] 通过，三处版本号一致：");
  for (const e of entries) {
    console.log(`  OK  ${e.file.padEnd(28)} ${e.version}`);
  }
  return expected;
}

function setVersion(next) {
  if (!SEMVER_RE.test(next)) {
    fail([`目标版本非法：${next}`]);
  }

  // 1) package.json：保留原有缩进（2 空格）与结尾换行
  const pkgRaw = readFileSync(PKG_PATH, "utf8");
  const pkg = JSON.parse(pkgRaw);
  pkg.version = next;
  writeFileSync(PKG_PATH, `${JSON.stringify(pkg, null, 2)}\n`, "utf8");

  // 2) Cargo.toml：只替换 [package] 段内的第一处 version
  const cargoRaw = readFileSync(CARGO_PATH, "utf8");
  const pkgStart = cargoRaw.indexOf("[package]");
  if (pkgStart < 0) fail(["src-tauri/Cargo.toml 中找不到 [package] 段"]);
  const head = cargoRaw.slice(0, pkgStart + "[package]".length);
  const tail = cargoRaw.slice(pkgStart + "[package]".length);
  const nextSection = tail.search(/^\[/m);
  const section = nextSection >= 0 ? tail.slice(0, nextSection) : tail;
  const remainder = nextSection >= 0 ? tail.slice(nextSection) : "";
  if (!/^\s*version\s*=\s*"[^"]+"/m.test(section)) {
    fail(["src-tauri/Cargo.toml 的 [package] 段中找不到 version 字段"]);
  }
  const patchedSection = section.replace(/^(\s*version\s*=\s*)"[^"]+"/m, `$1"${next}"`);
  writeFileSync(CARGO_PATH, `${head}${patchedSection}${remainder}`, "utf8");

  // 3) tauri.conf.json：只替换顶层 version 文本，不重新序列化
  //    （reserialize 会把内联数组展开成多行，产生无意义的格式 diff）
  const confRaw = readFileSync(TAURI_CONF_PATH, "utf8");
  const confRe = /^(\s{2}"version"\s*:\s*)"([^"]*)"/m;
  if (!confRe.test(confRaw)) {
    fail(["src-tauri/tauri.conf.json 中找不到顶层 version 字段"]);
  }
  const confNext = confRaw.replace(confRe, `$1"${next}"`);
  writeFileSync(TAURI_CONF_PATH, confNext, "utf8");

  console.log(`[版本校验] 已把三处版本号统一为 ${next}：`);
  console.log("  - package.json");
  console.log("  - src-tauri/Cargo.toml");
  console.log("  - src-tauri/tauri.conf.json");
  console.log("\n下一步：同步更新 CHANGELOG.md 的版本段，再提交并打 tag（见 docs/RELEASE.md）。");
}

const args = process.argv.slice(2);
const [first, second] = args;

if (first === undefined || first === "--check") {
  check();
} else if (first === "--print") {
  console.log(readPackageVersion());
} else if (first === "--set") {
  if (!second) fail(["用法：node scripts/sync-version.mjs --set <版本>，例如 --set 0.2.0"]);
  setVersion(second);
} else if (first === "--help" || first === "-h") {
  console.log("用法：node scripts/sync-version.mjs [--check | --print | --set <版本>]");
} else {
  fail([`未知参数：${args.join(" ")}`, "可用参数：--check / --print / --set <版本> / --help"]);
}
