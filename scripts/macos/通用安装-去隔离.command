#!/bin/bash
# ============================================================
#  通用 macOS 应用安装脚本（自动去隔离 / Gatekeeper 解封）
#  适用：.app 目录、.dmg、.zip（内含 .app）、.pkg
#  用法：
#    1) 双击运行：脚本会自动在同目录找安装包
#    2) 把 .dmg/.zip/.app 拖到本脚本图标上运行
#    3) 终端里跑： ./通用安装-去隔离.command /路径/某某.app
#  可选参数：
#    -c, --clear-only [路径]  只清除隔离属性，不复制安装
#    -t, --target DIR         指定安装目录（默认 /Applications，无权限则 ~/Applications）
#    -n, --no-open            装完不自动打开
# ============================================================
set -euo pipefail

SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PAYLOAD_ARG=""
CLEAR_ONLY=0
NO_OPEN=0
TARGET_DIR_OVERRIDE=""

log()  { printf '%s\n' "$*"; }
warn() { printf '\033[33m⚠️  %s\033[0m\n' "$*" >&2; }
die()  { printf '\033[31m❌ %s\033[0m\n' "$*" >&2; exit 1; }

# ---------- 参数解析 ----------
while [ $# -gt 0 ]; do
  case "$1" in
    -c|--clear-only) CLEAR_ONLY=1; shift ;;
    -t|--target)     TARGET_DIR_OVERRIDE="${2:-}"; shift 2 ;;
    -n|--no-open)    NO_OPEN=1; shift ;;
    -h|--help)       awk 'NR>1 && /^#/ {sub(/^# ?/,""); print; next} NR>1 && !/^#/ {exit}' "${BASH_SOURCE[0]}"; exit 0 ;;
    *)               PAYLOAD_ARG="$1"; shift ;;
  esac
done

# ---------- 临时目录 / 卸载清理 ----------
WORK_DIR=""
MOUNT_POINT=""
# 挂载 dmg：新版 macOS 优先 diskutil，老系统回退 hdiutil
attach_dmg() {
  local dmg="$1" mp="$2"
  if diskutil image attach "$dmg" --mountOptions nobrowse --readOnly --mountPoint "$mp" >/dev/null 2>&1; then
    return 0
  fi
  diskutil unmount "$mp" >/dev/null 2>&1 || true
  hdiutil detach "$mp" -quiet -force >/dev/null 2>&1 || true
  hdiutil attach "$dmg" -nobrowse -noautoopen -readonly -mountpoint "$mp" >/dev/null 2>&1
}

detach_dmg() {
  local mp="$1"
  [ -d "$mp" ] || return 0
  if diskutil unmount "$mp" >/dev/null 2>&1; then
    rmdir "$mp" >/dev/null 2>&1 || true
    return 0
  fi
  hdiutil detach "$mp" -quiet -force >/dev/null 2>&1 || true
  rmdir "$mp" >/dev/null 2>&1 || true
}

cleanup() {
  if [ -n "$MOUNT_POINT" ] && [ -d "$MOUNT_POINT" ]; then
    detach_dmg "$MOUNT_POINT"
  fi
  if [ -n "$WORK_DIR" ] && [ -d "$WORK_DIR" ]; then
    rm -rf "$WORK_DIR" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/app-installer.XXXXXX")"

# ---------- 1. 定位安装载荷 ----------
PAYLOAD=""
if [ -n "$PAYLOAD_ARG" ]; then
  [ -e "$PAYLOAD_ARG" ] || die "路径不存在：$PAYLOAD_ARG"
  PAYLOAD="$(cd "$(dirname "$PAYLOAD_ARG")" && pwd)/$(basename "$PAYLOAD_ARG")"
else
  # 优先 .app，其次 .dmg / .zip / .pkg
  for pat in '*.app' '*.dmg' '*.zip' '*.pkg'; do
    for f in "$SELF_DIR"/$pat; do
      [ -e "$f" ] || continue
      case "$(basename "$f")" in
        __MACOSX*|.*) continue ;;
      esac
      PAYLOAD="$f"
      break
    done
    [ -n "$PAYLOAD" ] && break
  done
fi

if [ -z "$PAYLOAD" ]; then
  log "没在脚本目录找到安装包。请把 .app / .dmg / .zip / .pkg 的路径拖进来后回车："
  read -r -p "> " DROPPED
  DROPPED="${DROPPED%\"}"; DROPPED="${DROPPED#\"}"
  DROPPED="${DROPPED%\'}"; DROPPED="${DROPPED#\'}"
  [ -e "$DROPPED" ] || die "路径不存在：$DROPPED"
  PAYLOAD="$(cd "$(dirname "$DROPPED")" && pwd)/$(basename "$DROPPED")"
fi

log "📦 安装载荷：$(basename "$PAYLOAD")"

# ---------- 2. 仅清除隔离模式 ----------
clear_quarantine() {
  local t="$1"
  [ -e "$t" ] || return 0
  xattr -dr com.apple.quarantine "$t" 2>/dev/null || true
  xattr -cr "$t" 2>/dev/null || true
  xattr -dr com.apple.provenance "$t" 2>/dev/null || true
}

if [ "$CLEAR_ONLY" -eq 1 ]; then
  [ -e "$PAYLOAD" ] || die "找不到要解封的应用：$PAYLOAD"
  log "🧹 就地清除隔离属性：$PAYLOAD"
  clear_quarantine "$PAYLOAD"
  log "✅ 完成。现在可以直接双击打开。"
  exit 0
fi

# ---------- 3. 展开 dmg / zip / pkg ----------
SEARCH_ROOT=""
case "$PAYLOAD" in
  *.app)
    SEARCH_ROOT="$(dirname "$PAYLOAD")"
    FOUND_APP="$PAYLOAD"
    ;;
  *.dmg)
    MOUNT_POINT="$WORK_DIR/mnt"
    mkdir -p "$MOUNT_POINT"
    log "💿 挂载 dmg ..."
    attach_dmg "$PAYLOAD" "$MOUNT_POINT" || die "dmg 挂载失败"
    SEARCH_ROOT="$MOUNT_POINT"
    ;;
  *.zip)
    log "🗜  解压 zip ..."
    mkdir -p "$WORK_DIR/unzip"
    ditto -x -k "$PAYLOAD" "$WORK_DIR/unzip" || die "zip 解压失败"
    xattr -dr com.apple.quarantine "$WORK_DIR/unzip" 2>/dev/null || true
    SEARCH_ROOT="$WORK_DIR/unzip"
    ;;
  *.pkg)
    log "📦 pkg 安装器，需要管理员密码："
    xattr -dr com.apple.quarantine "$PAYLOAD" 2>/dev/null || true
    sudo installer -pkg "$PAYLOAD" -target / || die "pkg 安装失败"
    # 装完把 /Applications 里带隔离属性的包清一遍
    find /Applications -maxdepth 2 -name '*.app' -newermt '-10 minutes' -print0 2>/dev/null \
      | while IFS= read -r -d '' a; do clear_quarantine "$a"; done
    log "✅ pkg 安装完成，隔离属性已清除。"
    exit 0
    ;;
  *) die "不认识的格式：$(basename "$PAYLOAD")（支持 .app / .dmg / .zip / .pkg）" ;;
esac

# ---------- 4. 在载荷里找 .app ----------
APPS=()
if [ -z "${FOUND_APP:-}" ]; then
  while IFS= read -r -d '' a; do
    APPS+=("$a")
  done < <(find "$SEARCH_ROOT" -maxdepth 3 -name '*.app' -type d -print0 2>/dev/null)
  [ "${#APPS[@]}" -gt 0 ] || die "载荷里没找到 .app"
  if [ "${#APPS[@]}" -eq 1 ]; then
    FOUND_APP="${APPS[0]}"
  else
    log "找到多个应用，请选择："
    i=1
    for a in "${APPS[@]}"; do
      log "  $i) $(basename "$a")"
      i=$((i+1))
    done
    read -r -p "输入序号 [1]: " PICK
    PICK="${PICK:-1}"
    [[ "$PICK" =~ ^[0-9]+$ ]] || die "序号不合法"
    [ "$PICK" -ge 1 ] && [ "$PICK" -le "${#APPS[@]}" ] || die "序号越界"
    FOUND_APP="${APPS[$((PICK-1))]}"
  fi
fi

APP_NAME="$(basename "$FOUND_APP")"
INFO_PLIST="$FOUND_APP/Contents/Info.plist"

# ---------- 5. 读 bundle 信息 ----------
BUNDLE_ID=""
EXEC_NAME=""
if [ -f "$INFO_PLIST" ]; then
  BUNDLE_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$INFO_PLIST" 2>/dev/null || true)"
  EXEC_NAME="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$INFO_PLIST" 2>/dev/null || true)"
fi
[ -n "$EXEC_NAME" ] || EXEC_NAME="${APP_NAME%.app}"

# ---------- 6. 目标目录 ----------
if [ -n "$TARGET_DIR_OVERRIDE" ]; then
  TARGET_DIR="$TARGET_DIR_OVERRIDE"
  mkdir -p "$TARGET_DIR" || die "无法创建目标目录：$TARGET_DIR"
elif [ -w "/Applications" ]; then
  TARGET_DIR="/Applications"
else
  TARGET_DIR="$HOME/Applications"
  mkdir -p "$TARGET_DIR"
  warn "/Applications 无写入权限，改装到 $TARGET_DIR"
fi
TARGET_APP="$TARGET_DIR/$APP_NAME"

# ---------- 7. 退出正在运行的旧版本 ----------
if [ -n "$BUNDLE_ID" ]; then
  osascript -e "tell application id \"$BUNDLE_ID\" to quit" >/dev/null 2>&1 || true
fi
if [ -x "$TARGET_APP/Contents/MacOS/$EXEC_NAME" ]; then
  pkill -f "$TARGET_APP/Contents/MacOS/$EXEC_NAME" >/dev/null 2>&1 || true
fi
sleep 0.5

# ---------- 8. 复制安装 ----------
log "📥 安装到 $TARGET_DIR ..."
if [ -e "$TARGET_APP" ]; then
  rm -rf "$TARGET_APP" || die "旧版本删除失败（可能正在运行或权限不足）"
fi
ditto "$FOUND_APP" "$TARGET_APP" || die "复制失败"

# ---------- 9. 清隔离属性（关键一步） ----------
log "🧹 清除隔离属性 ..."
clear_quarantine "$TARGET_APP"
clear_quarantine "$FOUND_APP"

# ---------- 10. 签名自检 / 补 ad-hoc 签名 ----------
if ! codesign --verify --deep --strict "$TARGET_APP" >/dev/null 2>&1; then
  warn "签名校验不通过，尝试 ad-hoc 重签 ..."
  codesign --force --deep --sign - "$TARGET_APP" >/dev/null 2>&1 \
    && log "   已重签（ad-hoc）" \
    || warn "重签失败，部分系统上可能需要手动右键→打开"
fi

# ---------- 11. 结果校验 ----------
if xattr -p com.apple.quarantine "$TARGET_APP" >/dev/null 2>&1; then
  warn "隔离属性似乎仍在，建议手动执行： sudo xattr -cr \"$TARGET_APP\""
else
  log "✅ 隔离属性已清除"
fi

log "🎉 安装完成：$TARGET_APP"
[ "$NO_OPEN" -eq 1 ] || open "$TARGET_APP" || warn "自动打开失败，请手动双击"
