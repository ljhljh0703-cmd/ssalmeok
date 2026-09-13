#!/bin/bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$PROJECT_ROOT/src-tauri/Cargo.toml"
APP_SOURCE="$PROJECT_ROOT/src-tauri/src/lib.rs"
NATIVE_SOURCE="$PROJECT_ROOT/src-tauri/src/native_tray.rs"

fail() {
  echo "메뉴 막대 구조 검사 실패: $1" >&2
  exit 1
}

test -f "$NATIVE_SOURCE" || fail "native_tray.rs가 없습니다."
grep -Fq 'mod native_tray;' "$APP_SOURCE" || fail "앱이 네이티브 메뉴 막대 모듈을 연결하지 않았습니다."
grep -Fq 'NSStatusBar::systemStatusBar()' "$NATIVE_SOURCE" || fail "AppKit NSStatusBar 생성 경로가 없습니다."

if grep -Fq 'tray-icon' "$MANIFEST"; then
  fail "문제가 재현된 tray-icon 의존성이나 우회 패치가 다시 연결됐습니다."
fi

if grep -RqE 'tauri::tray|TrayIconBuilder|with_inner_tray_icon' "$PROJECT_ROOT/src-tauri/src"; then
  fail "문제가 재현된 Tauri 트레이 호출이 다시 들어왔습니다."
fi

test ! -d "$PROJECT_ROOT/vendor/tray-icon" || fail "사용하지 않는 tray-icon 우회 복사본이 다시 들어왔습니다."

echo "메뉴 막대 구조 검사 통과: AppKit 직접 경로만 사용합니다."
