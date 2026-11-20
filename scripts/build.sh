#!/usr/bin/env bash
# 本地一键打包脚本（Linux / macOS）
#
# 用法：
#   ./scripts/build.sh                  # 默认内核版本 v1.19.0
#   ./scripts/build.sh v1.19.0
#   SKIP_SIDECAR=1 ./scripts/build.sh   # 已手动放好内核时使用
#   FORCE_INSTALL=1 ./scripts/build.sh  # 强制重装前端依赖

set -euo pipefail

KERNEL_VERSION="${1:-v1.19.0}"
SKIP_SIDECAR="${SKIP_SIDECAR:-0}"
FORCE_INSTALL="${FORCE_INSTALL:-0}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
echo "==> 仓库根目录：$REPO_ROOT"

# 1. 环境校验
for tool in node npm rustc cargo; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "缺少必需工具：$tool。请安装 Node.js ≥ 18 与 Rust ≥ 1.77.2。" >&2
    exit 1
  fi
done
echo "==> node $(node -v) / npm $(npm -v)"
echo "==> $(rustc -vV | awk '/^host: /{print $0}')"

# 2. 准备内核 sidecar
if [[ "$SKIP_SIDECAR" != "1" ]]; then
  TRIPLE="$(rustc -vV | awk '/^host: /{print $2}')"
  # 项目默认目标为 MSVC（Windows）；Linux 上 host 为 x86_64-unknown-linux-gnu 属正常。
  case "$TRIPLE" in
    *-pc-windows-gnu)
      echo "警告：rustc host 为 $TRIPLE，与项目默认目标 x86_64-pc-windows-msvc 不一致；" >&2
      echo "      sidecar 将按该 host 命名，CI/发布产物使用 -msvc 命名。对齐方式：rustup default stable-x86_64-pc-windows-msvc" >&2
      ;;
  esac
  SIDECAR="$REPO_ROOT/src-tauri/binaries/mihomo-${TRIPLE}"
  if [[ -f "$SIDECAR" ]]; then
    echo "==> 内核已就绪：$SIDECAR"
  else
    echo "==> 未找到内核，开始下载 $KERNEL_VERSION ..."
    "$REPO_ROOT/scripts/prepare-sidecar.sh" "$KERNEL_VERSION"
  fi
else
  echo "==> 已跳过内核准备（SKIP_SIDECAR=1）"
fi

# 3. 前端依赖
if [[ "$FORCE_INSTALL" == "1" || ! -d "$REPO_ROOT/node_modules" ]]; then
  if [[ -f "$REPO_ROOT/package-lock.json" ]]; then
    echo "==> npm ci"
    npm ci
  else
    echo "==> npm install"
    npm install
  fi
else
  echo "==> 前端依赖已存在，跳过安装（FORCE_INSTALL=1 可强制重装）"
fi

# 4. 构建
echo "==> tauri build"
npm run tauri:build

# 5. 产物
BUNDLE="$REPO_ROOT/src-tauri/target/release/bundle"
echo ""
echo "==> 构建完成，产物目录：$BUNDLE"
if [[ -d "$BUNDLE" ]]; then
  find "$BUNDLE" -type f \( -name "*.deb" -o -name "*.AppImage" -o -name "*.rpm" -o -name "*.msi" -o -name "*.exe" \) -print | sed 's/^/    /'
fi
