#!/bin/bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$PROJECT_ROOT/src-tauri/Cargo.toml"
APP_SOURCE="$PROJECT_ROOT/src-tauri/src/lib.rs"
BRIDGE_SOURCE="$PROJECT_ROOT/src-tauri/src/menubar_helper.rs"
NATIVE_SOURCE="$PROJECT_ROOT/src-tauri/native/menubar.swift"
TAURI_CONFIG="$PROJECT_ROOT/src-tauri/tauri.conf.json"

fail() {
  echo "메뉴 막대 구조 검사 실패: $1" >&2
  exit 1
}

test -f "$NATIVE_SOURCE" || fail "AppKit 메뉴 막대 도우미 소스가 없습니다."
test -f "$BRIDGE_SOURCE" || fail "Tauri와 도우미를 잇는 코드가 없습니다."
grep -Fq 'mod menubar_helper;' "$APP_SOURCE" || fail "앱이 메뉴 막대 도우미 모듈을 연결하지 않았습니다."
grep -Fq 'NSStatusBar.system.statusItem' "$NATIVE_SOURCE" || fail "AppKit NSStatusBar 생성 경로가 없습니다."
grep -Fq '.sidecar("ssalmeok-menubar")' "$BRIDGE_SOURCE" || fail "Tauri가 AppKit 도우미를 시작하지 않습니다."
grep -Fq 'binaries/ssalmeok-menubar' "$TAURI_CONFIG" || fail "AppKit 도우미가 앱 묶음에 포함되지 않습니다."

if grep -Fq 'tray-icon' "$MANIFEST"; then
  fail "문제가 재현된 tray-icon 의존성이나 우회 패치가 다시 연결됐습니다."
fi

if grep -RqE 'tauri::tray|TrayIconBuilder|with_inner_tray_icon' "$PROJECT_ROOT/src-tauri/src"; then
  fail "문제가 재현된 Tauri 트레이 호출이 다시 들어왔습니다."
fi

test ! -d "$PROJECT_ROOT/vendor/tray-icon" || fail "사용하지 않는 tray-icon 우회 복사본이 다시 들어왔습니다."
test ! -f "$PROJECT_ROOT/src-tauri/src/native_tray.rs" || fail "Tauri 프로세스 안의 실패한 AppKit 경로가 다시 들어왔습니다."

echo "메뉴 막대 구조 검사 통과: Tauri와 AppKit 도우미가 프로세스로 분리돼 있습니다."
