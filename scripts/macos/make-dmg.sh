#!/bin/bash
# 把 tauri build 生成的 .app 与安装脚本、安装说明一起打成 DMG。
# 用法：scripts/macos/make-dmg.sh [bundle 目录]
#   默认 bundle 目录：src-tauri/target/universal-apple-darwin/release/bundle/macos
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUNDLE_DIR="${1:-$ROOT/src-tauri/target/universal-apple-darwin/release/bundle/macos}"
CONF="$ROOT/src-tauri/tauri.conf.json"
NAME="$(node -p "require('$CONF').productName")"
VERSION="$(node -p "require('$CONF').version")"
APP="$BUNDLE_DIR/$NAME.app"
[ -d "$APP" ] || { echo "找不到 $APP" >&2; exit 1; }

OUT="$ROOT/dist-mac"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

ditto "$APP" "$STAGE/$NAME.app"
ln -s /Applications "$STAGE/Applications"
cp "$ROOT/scripts/macos/通用安装-去隔离.command" "$STAGE/"
cp "$ROOT/scripts/macos/安装说明.txt" "$STAGE/"
chmod +x "$STAGE/通用安装-去隔离.command"

mkdir -p "$OUT"
DMG="$OUT/${NAME}_${VERSION}_universal.dmg"
rm -f "$DMG"
hdiutil create -volname "$NAME" -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG"
echo "已生成：$DMG"
