#!/bin/bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_PATH="${1:-/Applications/쌀먹.app}"
RUN_ID="$(date '+%Y%m%d-%H%M%S')"
EVIDENCE_DIR="$PROJECT_ROOT/.runtime-evidence/menubar-$RUN_ID"
PROOF_IMAGE="$EVIDENCE_DIR/status-item.png"
RECEIPT="$EVIDENCE_DIR/receipt.txt"

fail() {
  echo "메뉴 막대 화면 검사 실패: $1" >&2
  exit 1
}

test -d "$APP_PATH" || fail "앱을 찾을 수 없습니다: $APP_PATH"
mkdir -p "$EVIDENCE_DIR"

RUNNING_PIDS="$(pgrep -x ssalmeok || true)"
RUNNING_HELPERS="$(pgrep -f '(^|/)ssalmeok-menubar([ -]|$)' || true)"
if [ -n "$RUNNING_PIDS" ] || [ -n "$RUNNING_HELPERS" ]; then
  fail "다른 쌀먹이나 메뉴 막대 도우미가 실행 중입니다. 메뉴의 ‘종료’로 완전히 끝낸 뒤 다시 실행하세요."
fi
open -n "$APP_PATH" --args --hidden

SCREEN_BOUNDS="$(osascript -e 'tell application "Finder" to get bounds of window of desktop' | tr -d ' ')"
IFS=',' read -r SCREEN_LEFT SCREEN_TOP SCREEN_RIGHT SCREEN_BOTTOM <<< "$SCREEN_BOUNDS"

TRAY_RECORD=""
LAST_TRAY_RECORD="없음"
LAST_VISIBLE_FRAME=""
STABLE_VISIBLE_READS=0
for _ in $(seq 1 20); do
  TRAY_RECORD="$(osascript \
    -e 'tell application "System Events"' \
    -e 'set helperProcesses to every process whose name starts with "ssalmeok-menubar"' \
    -e 'if (count helperProcesses) is not 1 then return ""' \
    -e 'set helperProcess to item 1 of helperProcesses' \
    -e 'tell helperProcess' \
    -e 'set trayItems to {}' \
    -e 'repeat with candidateBar in every menu bar' \
    -e 'set matchingItems to every menu bar item of candidateBar whose description is "status menu"' \
    -e 'repeat with candidateItem in matchingItems' \
    -e 'set end of trayItems to candidateItem' \
    -e 'end repeat' \
    -e 'end repeat' \
    -e 'if (count trayItems) is not 1 then return ""' \
    -e 'set trayItem to item 1 of trayItems' \
    -e 'set {itemX, itemY} to position of trayItem' \
    -e 'set {itemWidth, itemHeight} to size of trayItem' \
    -e 'return (name of trayItem as text) & "|" & itemX & "|" & itemY & "|" & itemWidth & "|" & itemHeight' \
    -e 'end tell' \
    -e 'end tell' 2>/dev/null || true)"
  if [ -n "$TRAY_RECORD" ]; then
    LAST_TRAY_RECORD="$TRAY_RECORD"
    IFS='|' read -r ITEM_NAME ITEM_X ITEM_Y ITEM_WIDTH ITEM_HEIGHT <<< "$TRAY_RECORD"
    if [[ "$ITEM_X" =~ ^-?[0-9]+$ && "$ITEM_Y" =~ ^-?[0-9]+$ && "$ITEM_WIDTH" =~ ^[0-9]+$ && "$ITEM_HEIGHT" =~ ^[0-9]+$ ]] && (( ITEM_X >= SCREEN_LEFT && ITEM_Y >= SCREEN_TOP && ITEM_X + ITEM_WIDTH <= SCREEN_RIGHT && ITEM_Y + ITEM_HEIGHT <= 40 )); then
      CURRENT_FRAME="$ITEM_X,$ITEM_Y,$ITEM_WIDTH,$ITEM_HEIGHT"
      if [ "$CURRENT_FRAME" = "$LAST_VISIBLE_FRAME" ]; then
        STABLE_VISIBLE_READS=$(( STABLE_VISIBLE_READS + 1 ))
      else
        LAST_VISIBLE_FRAME="$CURRENT_FRAME"
        STABLE_VISIBLE_READS=1
      fi
      if (( STABLE_VISIBLE_READS >= 2 )); then
        break
      fi
    else
      LAST_VISIBLE_FRAME=""
      STABLE_VISIBLE_READS=0
    fi
  fi
  sleep 1
done

if (( STABLE_VISIBLE_READS < 2 )); then
  fail "20초 안에 화면 상단의 안정된 메뉴 막대 항목을 찾지 못했습니다. 마지막 관측: $LAST_TRAY_RECORD"
fi
pgrep -x ssalmeok >/dev/null || fail "메뉴 막대는 보이지만 Tauri 본체가 실행 중이지 않습니다."

IFS='|' read -r ITEM_NAME ITEM_X ITEM_Y ITEM_WIDTH ITEM_HEIGHT <<< "$TRAY_RECORD"

[[ "$ITEM_X" =~ ^-?[0-9]+$ ]] || fail "항목 x 좌표를 읽지 못했습니다: $ITEM_X"
[[ "$ITEM_Y" =~ ^-?[0-9]+$ ]] || fail "항목 y 좌표를 읽지 못했습니다: $ITEM_Y"
[[ "$ITEM_WIDTH" =~ ^[0-9]+$ ]] || fail "항목 너비를 읽지 못했습니다: $ITEM_WIDTH"
[[ "$ITEM_HEIGHT" =~ ^[0-9]+$ ]] || fail "항목 높이를 읽지 못했습니다: $ITEM_HEIGHT"

if (( ITEM_X < SCREEN_LEFT || ITEM_Y < SCREEN_TOP || ITEM_X + ITEM_WIDTH > SCREEN_RIGHT || ITEM_Y + ITEM_HEIGHT > 40 )); then
  fail "항목이 화면 상단 안에 없습니다: ($ITEM_X, $ITEM_Y, ${ITEM_WIDTH}x${ITEM_HEIGHT}), 화면 ${SCREEN_RIGHT}x${SCREEN_BOTTOM}"
fi

CROP_X=$(( ITEM_X > 6 ? ITEM_X - 6 : 0 ))
CROP_WIDTH=$(( ITEM_WIDTH + 12 ))
if (( CROP_X + CROP_WIDTH > SCREEN_RIGHT )); then
  CROP_WIDTH=$(( SCREEN_RIGHT - CROP_X ))
fi
screencapture -x -R"$CROP_X,0,$CROP_WIDTH,40" "$PROOF_IMAGE"
test -s "$PROOF_IMAGE" || fail "화면 증거 이미지를 만들지 못했습니다."

IMAGE_SHA="$(shasum -a 256 "$PROOF_IMAGE" | awk '{print $1}')"
{
  echo "checked_at=$(date -Iseconds)"
  echo "app_path=$APP_PATH"
  echo "main_process=ssalmeok"
  echo "menubar_process=ssalmeok-menubar"
  echo "status_item_name=$ITEM_NAME"
  echo "status_item_frame=$ITEM_X,$ITEM_Y,$ITEM_WIDTH,$ITEM_HEIGHT"
  echo "screen_bounds=$SCREEN_BOUNDS"
  echo "proof_image=$PROOF_IMAGE"
  echo "proof_sha256=$IMAGE_SHA"
  echo "geometry_gate=pass"
  echo "pixel_gate=pending"
  for EXECUTABLE in ssalmeok codexbar ssalmeok-menubar; do
    echo "${EXECUTABLE}_sha256=$(shasum -a 256 "$APP_PATH/Contents/MacOS/$EXECUTABLE" | awk '{print $1}')"
  done
} > "$RECEIPT"

echo "좌표 검사는 통과했습니다: ($ITEM_X, $ITEM_Y, ${ITEM_WIDTH}x${ITEM_HEIGHT})"
echo "픽셀 증거: $PROOF_IMAGE"
open "$PROOF_IMAGE"

if [ ! -t 0 ]; then
  echo "실제 이미지에서 서비스 로고와 남은 숫자를 확인한 뒤 대화형 터미널에서 다시 실행하세요." >&2
  exit 2
fi

read -r -p "이미지에 서비스 로고와 남은 숫자가 둘 다 선명하게 보입니까? [y/N] " PIXEL_CONFIRMATION
if [ "$PIXEL_CONFIRMATION" != "y" ] && [ "$PIXEL_CONFIRMATION" != "Y" ]; then
  fail "직접 확인하는 픽셀 관문을 통과하지 못했습니다."
fi

sed -i '' 's/pixel_gate=pending/pixel_gate=pass/' "$RECEIPT"
echo "pixel_reviewer=${PIXEL_REVIEWER:-terminal-operator}" >> "$RECEIPT"
echo "메뉴 막대 화면 검사 통과. 영수증: $RECEIPT"
