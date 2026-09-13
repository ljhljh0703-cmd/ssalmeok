#!/bin/bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

# Keep cargo and the post-build strip/sign steps on the same target directory.
# Relative overrides are resolved before Tauri changes its working directory.
PACKAGE_TARGET_DIR="${CARGO_TARGET_DIR:-$PROJECT_ROOT/src-tauri/target}"
if [[ "$PACKAGE_TARGET_DIR" != /* ]]; then
  PACKAGE_TARGET_DIR="$PROJECT_ROOT/$PACKAGE_TARGET_DIR"
fi
export CARGO_TARGET_DIR="$PACKAGE_TARGET_DIR"

pnpm tauri build --bundles app

APP_PATH="$PACKAGE_TARGET_DIR/release/bundle/macos/쌀먹.app"
for EXECUTABLE in ssalmeok codexbar ssalmeok-menubar; do
  test -x "$APP_PATH/Contents/MacOS/$EXECUTABLE"
done
test -s "$APP_PATH/Contents/Resources/icons/tray-codex.png"
test -s "$APP_PATH/Contents/Resources/icons/tray-claude.png"
strip -x "$APP_PATH/Contents/MacOS/codexbar"
strip -x "$APP_PATH/Contents/MacOS/ssalmeok-menubar"
codesign --force --deep --sign - "$APP_PATH"
codesign --verify --deep --strict --verbose=2 "$APP_PATH"
echo "패키징 완료: $APP_PATH"
