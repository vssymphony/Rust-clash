#!/usr/bin/env bash
# 下载并准备 mihomo 内核 sidecar（Linux / macOS）
#
# 用法：
#   ./scripts/prepare-sidecar.sh                          # 默认 v1.19.0
#   ./scripts/prepare-sidecar.sh v1.19.0
#   ./scripts/prepare-sidecar.sh --triple x86_64-unknown-linux-gnu --force
#   ./scripts/prepare-sidecar.sh --source ./downloads/mihomo
#   ./scripts/prepare-sidecar.sh --url https://mirror.example.com/mihomo.gz --sha256 <hash>
#
# 作用：解析目标三元组（--triple 优先，其次 rustc host）→ 取得内核
#       → 重命名为 mihomo-<三元组> 放入 src-tauri/binaries → 可选 SHA256 校验。
#
# 说明：sidecar 文件名必须与编译 host 三元组完全一致（Windows 为
#       mihomo-<triple>.exe，Linux 为 mihomo-<triple>），否则 Tauri 打包失败。

set -euo pipefail

DEFAULT_VERSION="v1.19.0"
VERSION="$DEFAULT_VERSION"
SOURCE=""
TRIPLE=""
URL=""
SHA256_EXPECTED=""
FORCE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      SOURCE="${2:-}"
      shift 2
      ;;
    --triple)
      TRIPLE="${2:-}"
      shift 2
      ;;
    --url)
      URL="${2:-}"
      shift 2
      ;;
    --sha256)
      SHA256_EXPECTED="${2:-}"
      shift 2
      ;;
    -f|--force)
      FORCE=1
      shift
      ;;
    -h|--help)
      grep '^#' "$0" | sed 's/^# \{0,1\}//' | head -n 22
      exit 0
      ;;
    *)
      VERSION="$1"
      shift
      ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="$REPO_ROOT/src-tauri/binaries"
mkdir -p "$BIN_DIR"

HOST_TRIPLE=""
if command -v rustc >/dev/null 2>&1; then
  HOST_TRIPLE="$(rustc -vV | awk '/^host: /{print $2}')"
fi

if [[ -n "$TRIPLE" ]]; then
  TARGET_TRIPLE="$TRIPLE"
  if [[ -n "$HOST_TRIPLE" && "$HOST_TRIPLE" != "$TRIPLE" ]]; then
    echo "警告：--triple（$TRIPLE）与 rustc host（$HOST_TRIPLE）不一致。" >&2
    echo "      sidecar 文件名必须与编译 host 一致，否则 tauri-build 会报 externalBin 缺失。" >&2
  fi
elif [[ -n "$HOST_TRIPLE" ]]; then
  TARGET_TRIPLE="$HOST_TRIPLE"
else
  echo "警告：未找到 rustc，无法读取 host 三元组；请用 --triple 显式指定（如 x86_64-unknown-linux-gnu）。" >&2
  exit 1
fi
echo "目标三元组：$TARGET_TRIPLE"

EXT=""
case "$TARGET_TRIPLE" in
  *windows*) EXT=".exe" ;;
esac
TARGET="$BIN_DIR/mihomo-${TARGET_TRIPLE}${EXT}"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

assert_sha256() {
  local path="$1"
  local expected="${2:-}"
  if [[ -z "$expected" ]]; then
    return 0
  fi
  local actual
  actual="$(sha256_of "$path" | tr 'A-F' 'a-f')"
  local normalized
  normalized="$(echo "$expected" | tr -d '[:space:]' | tr 'A-F' 'a-f')"
  if [[ "$actual" != "$normalized" ]]; then
    echo "SHA256 校验失败：期望 $normalized，实际 $actual。内核可能被篡改或版本不匹配，已中止。" >&2
    exit 1
  fi
  echo "SHA256 校验通过：$actual"
}

if [[ -f "$TARGET" && $FORCE -eq 0 ]]; then
  echo "已存在：$TARGET（如需覆盖请加 --force）"
  exit 0
fi

EXPECTED_HASH="${SHA256_EXPECTED:-${MIHOMO_SHA256:-}}"

if [[ -n "$SOURCE" ]]; then
  if [[ ! -f "$SOURCE" ]]; then
    echo "指定的内核文件不存在：$SOURCE" >&2
    exit 1
  fi
  cp -f "$SOURCE" "$TARGET"
  chmod +x "$TARGET" || true
  assert_sha256 "$TARGET" "$EXPECTED_HASH"
  echo "已复制本地内核 → $TARGET"
  exit 0
fi

if [[ -z "$EXPECTED_HASH" ]]; then
  echo "提示：未提供 --sha256 或 MIHOMO_SHA256，本次跳过哈希校验。公开 CI 建议用 secret 固定内核字节。" >&2
fi

case "$TARGET_TRIPLE" in
  x86_64-unknown-linux-gnu)   ASSET_OS="linux"; ASSET_ARCH="amd64"; PACK="gz" ;;
  aarch64-unknown-linux-gnu)  ASSET_OS="linux"; ASSET_ARCH="arm64"; PACK="gz" ;;
  x86_64-apple-darwin)        ASSET_OS="darwin"; ASSET_ARCH="amd64"; PACK="gz" ;;
  aarch64-apple-darwin)       ASSET_OS="darwin"; ASSET_ARCH="arm64"; PACK="gz" ;;
  *)
    echo "暂不支持的目标三元组：$TARGET_TRIPLE，请用 --source 手动指定内核文件" >&2
    exit 1
    ;;
esac

ASSET="mihomo-${ASSET_OS}-${ASSET_ARCH}-${VERSION}.${PACK}"
if [[ -z "$URL" ]]; then
  if [[ -n "${MIHOMO_DOWNLOAD_BASE:-}" ]]; then
    URL="${MIHOMO_DOWNLOAD_BASE%/}/${ASSET}"
  else
    URL="https://github.com/MetaCubeX/mihomo/releases/download/${VERSION}/${ASSET}"
  fi
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "下载：$URL"
if ! { command -v curl >/dev/null 2>&1 && curl -fL --retry 3 -o "$WORK/$ASSET" "$URL"; } \
   && ! { command -v wget >/dev/null 2>&1 && wget -O "$WORK/$ASSET" "$URL"; }; then
  echo "下载失败：$URL" >&2
  echo "可选处理：1) 用 --url 指向可访问镜像或设置 MIHOMO_DOWNLOAD_BASE；2) 手动下载后 --source 指定；3) 按 src-tauri/binaries/README.md 放置。" >&2
  exit 1
fi

assert_sha256 "$WORK/$ASSET" "$EXPECTED_HASH"

if [[ "$PACK" == "gz" ]]; then
  gunzip -c "$WORK/$ASSET" > "$TARGET"
else
  cp "$WORK/$ASSET" "$TARGET"
fi

chmod +x "$TARGET"
echo "已写入：$TARGET"
echo "内核 SHA256：$(sha256_of "$TARGET")"
