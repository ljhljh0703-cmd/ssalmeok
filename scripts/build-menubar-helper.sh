#!/bin/bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="$PROJECT_ROOT/src-tauri/native/menubar.swift"
MACHINE_ARCH="$(uname -m)"

case "$MACHINE_ARCH" in
  arm64)
    RUST_TRIPLE="aarch64-apple-darwin"
    SWIFT_TARGET="arm64-apple-macos14.0"
    ;;
  x86_64)
    RUST_TRIPLE="x86_64-apple-darwin"
    SWIFT_TARGET="x86_64-apple-macos14.0"
    ;;
  *)
    echo "지원하지 않는 Mac 구조입니다: $MACHINE_ARCH" >&2
    exit 1
    ;;
esac

OUTPUT="$PROJECT_ROOT/src-tauri/binaries/ssalmeok-menubar-$RUST_TRIPLE"
mkdir -p "$(dirname "$OUTPUT")"

swiftc \
  -parse-as-library \
  -swift-version 5 \
  -Osize \
  -whole-module-optimization \
  -target "$SWIFT_TARGET" \
  "$SOURCE" \
  -framework AppKit \
  -framework Foundation \
  -o "$OUTPUT"

chmod +x "$OUTPUT"
echo "AppKit 메뉴 막대 도우미 빌드 완료: $OUTPUT"
