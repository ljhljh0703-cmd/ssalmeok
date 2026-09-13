---
status: provisional
handoff_to: gpt-6-astra
date: 2026-09-13
---

# Astra 인수인계 — 쌀먹 메뉴 막대 비가시성

## Task

`/Users/godju/Downloads/AI App/ssalmeok`에서 macOS 메뉴 막대 항목이 실제 화면에 안정적으로 보이도록 문제를 끝까지 해결하라. 현재 미커밋된 “Tauri 본체 + 별도 Swift AppKit 도우미” 구현을 먼저 독립 검토하고, 필요하면 최소 수정한 뒤 전체 빌드·GitHub 결과물·최종 설치본을 실제 픽셀로 검증하라. 컴파일 성공이나 접근성 객체 존재만으로 완료 판정하지 마라.

## Context

- 사용자 목적: Dock 없이 로그인 때 자동 실행되고, 메뉴 막대에서 Codex·Claude 남은 사용량과 우클릭 상세를 상시 확인한다.
- 사용자 피드백: 처음에는 보였으나 이후 계속 안 보였고, 같은 층의 수정이 반복됐다. 원인 분석 후 재발 방지까지 요구했다.
- 현재 원격 `main`: `249fe1a51fbc43e12a8e1221da90b69e425218ef`.
- `249fe1a`의 GitHub Actions run `34731458566`은 녹색이지만, 내려받아 설치한 실제 앱은 상태 항목이 `(1653, -1)`에 고정되어 화면에 보이지 않았다. 이 커밋을 해결본으로 취급하면 안 된다.
- 현재 작업 폴더에는 그 이후의 분리형 도우미 구현이 미커밋 상태로 있다. 이 상태를 보존하고 이어서 작업한다.

## Relevant files

- `src-tauri/src/lib.rs` — Tauri 본체, 사용량 수집, 창 생명주기, 도우미 프로세스 상태.
- `src-tauri/src/menubar_helper.rs` — JSON 상태 전송, 클릭 명령 수신, 도우미 2초 자동 재시작.
- `src-tauri/native/menubar.swift` — 순수 AppKit 상태 항목·우클릭 메뉴·stdin/stdout 프로토콜.
- `scripts/build-menubar-helper.sh` — Swift 도우미를 target-triple 이름으로 빌드.
- `src-tauri/tauri.conf.json` — 도우미 외부 실행 파일과 두 로고 리소스 번들 설정.
- `package.json` — `helper:build`, `verify`, `package:mac`, `verify:menubar` 연결.
- `scripts/check-native-tray-boundary.sh` — 실패한 in-process/Tauri tray 경로의 재도입 방지.
- `scripts/verify-menubar-runtime.sh` — 지정 앱 숨김 시작, 도우미 상태 항목 좌표 안정화, 잘라낸 픽셀의 사람 확인.
- `.github/workflows/verify.yml` — 미검증 상태로 `macos-26-arm64` + Xcode 26.5로 변경됨.
- `docs/INCIDENT-2026-09-MENUBAR-VISIBILITY.md` — 실험·오판·원인 경계·재발 방지 초안. 최종 결과에 맞춰 정합성 감사 필요.
- `docs/VERIFICATION.md`, `docs/MVP-STATUS.md`, `README.md` — 아직 provisional 문구가 섞여 있으므로 최종 증거 후 정리.

## Current state

- Git 작업 트리: `main` 위에 미커밋 변경이 있다. 먼저 `git status --short`와 전체 diff를 읽고, 임의 restore/reset하지 마라.
- 실패한 `src-tauri/src/native_tray.rs`는 삭제로 스테이징되어 있다.
- 생성 파일 `src-tauri/binaries/ssalmeok-menubar-aarch64-apple-darwin`은 gitignore 대상이며 로컬에 존재한다.
- 저장소 `src-tauri/target`은 캐시 정리된 상태다. 시험 빌드는 `/private/tmp/ssalmeok-native-target`에 있다.
- 로컬 전체 검사 결과: Swift helper build 통과, 구조 검사 통과, UI 4개 통과, Rust 9개 통과, clippy 경고 0.
- 로컬 production 통합 앱: `/private/tmp/ssalmeok-native-target/release/bundle/macos/쌀먹.app`.
- 실제 픽셀 통과 영수증: `.runtime-evidence/menubar-20260913-144618/receipt.txt`와 같은 폴더의 `status-item.png`.
- 실제 우클릭 메뉴 캡처: `/private/tmp/ssalmeok-integrated-menu.png`.
- 로컬 관측 프레임: `(1115, 5, 58×24)` 및 `(1117, 5, 56×24)`; 숫자 폭 변화에 따른 차이.
- 왼쪽 클릭으로 Tauri 창 `520×760` 생성, 재클릭으로 창 0개 확인.
- 우클릭 메뉴에 Codex 주간/초기화/하루 토큰/원화/리셋권과 Claude 5시간/주간/각 초기화/하루 토큰/원화가 실제 픽셀로 표시됨.
- 메뉴의 `쌀먹 열기`를 눌러 도우미→Tauri 통신으로 창 생성 확인.
- 도우미 PID `69971`을 종료했을 때 본체 PID `69953`은 유지되고 도우미가 PID `72142`로 자동 재시작됨.
- 창 없는 상태 RSS: Tauri 23,760KB + AppKit 도우미 24,352KB = 합계 48,112KB.
- Codex 초기화권은 소비하지 않았다.
- 현재 `/Applications/쌀먹.app`은 `249fe1a` 기반의 실패한 in-process AppKit 빌드다. 최종 분리형 결과물로 아직 교체하지 않았다.
- 복구용 이전 설치본: `/private/tmp/ssalmeok-before-appkit-249fe1a.app`.
- 인수인계 직전 임시 쌀먹 본체·도우미 프로세스는 종료했다.

## What was tried

1. Tauri 트레이 2개, 1개, 52px, 24px, 아이콘만, 위치 저장, 강제 visible, Ready 재삽입
   - 접근성 객체와 메뉴는 생겼지만 프레임이 화면 하단/좌측 밖 또는 시계 뒤에 남았다.
2. 원래 처음 보였던 최적화 전 실행 파일 재시험
   - 현재 환경에서는 동일하게 안 보였다. 최근 코드 한 줄만의 회귀가 아니다.
3. 사용량 캐시 제거, 새 autosave 이름, 위치 기본값 주입
   - 변화 없음. 데이터 캐시나 위치 키 하나가 원인이 아니다.
4. 노치·메뉴 폭·PNG·고정/가변 폭·메뉴·오버레이·Accessory 정책을 순수 AppKit probe로 분해
   - 모두 화면 안에 정상 표시됐다.
5. Tauri 프로세스 안에서 objc2로 AppKit 상태 항목 직접 생성
   - 직접 `cargo build`한 개발 성격 바이너리에서는 보여 1차로 오판했다.
   - 정식 `tauri build`/`tauri/custom-protocol` 배포 바이너리에서는 다시 화면 밖으로 갔다.
6. SDK 14→26 표식 변경, 최소 OS 14→11 실제 재빌드, 1초 지연 생성, autosave 제거, 본창 동시 생성
   - 전부 실패. 이 축을 반복하지 마라.
7. 같은 `.app/Contents/MacOS` 안에서 별도 순수 AppKit 실행 파일 실행
   - 화면 안 표시 성공. 프로세스 경계가 유효한 차단선으로 확인됐다.
8. Swift AppKit 도우미 + Tauri stdin/stdout 연결
   - 로컬 production 통합에서 표시·클릭·우클릭·자동 재시작·메모리 검증까지 통과했다.

## Decisions

- 단일 메뉴 막대 항목을 사용한다. 가장 적게 남은 서비스 로고 + 남은 숫자를 표시한다.
- Tauri는 데이터/창/자동 시작/알림만 소유하고 상태 항목은 별도 AppKit 도우미가 소유한다.
- 컴파일/CI 녹색과 실제 화면 PASS를 별도 상태로 취급한다.
- AXMenuExtra 존재만으로 가시성을 주장하지 않는다. 화면 안 좌표 + 실제 픽셀 둘 다 필수다.
- 실패한 Tauri tray 및 같은 프로세스 AppKit 경로는 구조 검사로 재도입을 막는다.
- 최종 증거가 나오기 전 문서 상태를 완료/confirmed로 올리지 않는다.

## Acceptance criteria

- [ ] 현재 미커밋 diff를 독립 검토하고 IPC 수명·동시성·종료·재시작 문제를 수정 또는 PASS 판정한다.
- [ ] `pnpm verify` 전체 통과: helper build, 구조 검사, UI tests, frontend build, fmt, clippy, Rust tests.
- [ ] 기본 경로의 `pnpm package:mac` 통과 및 helper/codexbar 포함·strip·deep codesign 확인.
- [ ] GitHub Actions 새 run 성공. `macos-26-arm64`/Xcode 26.5 선택이 실제 사용 가능한지 확인하고 필요 시 근거 있게 조정한다.
- [ ] GitHub artifact를 내려받아 주 실행 파일·codexbar·ssalmeok-menubar 해시를 기록한다.
- [ ] 현재 설치본을 복구 가능하게 보존하고 GitHub artifact로 `/Applications/쌀먹.app`을 교체한다.
- [ ] 설치본에서 `pnpm verify:menubar` 통과하고 잘라낸 픽셀에서 로고+숫자를 직접 확인한다.
- [ ] 정상 종료→숨김 재실행을 최소 2회 반복해 화면 안 픽셀을 확인한다.
- [ ] 실제 왼쪽 클릭으로 창 열기/닫기, 실제 우클릭으로 상세 메뉴, 메뉴의 `지금 갱신`, `종료`를 확인한다.
- [ ] helper만 종료했을 때 자동 재시작하고 상태 항목이 다시 보이는지 확인한다.
- [ ] Dock 미표시, 로그인 항목 `/Applications/쌀먹.app/Contents/MacOS/ssalmeok --hidden`, 창 닫은 뒤 합산 RSS를 확인한다.
- [ ] 설치본 snapshot에서 Codex·Claude 오류 0, 데이터 존재를 확인한다.
- [ ] 초기화권을 실제로 소비하지 않는다.
- [ ] README/MVP/VERIFICATION/INCIDENT의 주장과 최종 증거를 맞추고 provisional을 해제할지 근거로 판단한다.
- [ ] 최종 커밋을 원격 `main`에 push하고 설치본 해시·GitHub run URL·남은 경계를 사용자에게 보고한다.

## Constraints

- `git reset --hard`, 강제 push, 사용자 변경 덮어쓰기 금지.
- 현재 미커밋 변경을 먼저 보존하고, 필요하면 작업 전 diff 패치를 별도 보관한다.
- `/Applications/쌀먹.app`을 교체하기 전 반드시 복구본을 만든다.
- `/private/tmp` 증거는 보조일 뿐이다. 최종 판정 영수증은 repo의 gitignored `.runtime-evidence/`에도 남긴다.
- 메뉴 표시 실패 때 AX 존재를 PASS로 바꾸지 않는다.
- 테스트 중 Codex reset credit 소비 금지.
- 저장소의 대용량 `src-tauri/target` 캐시를 다시 만들지 말고 필요하면 `CARGO_TARGET_DIR=/private/tmp/ssalmeok-native-target`을 사용한다. 단, `package:mac`은 경로가 하드코딩돼 있어 이 환경변수와 함께 쓰면 strip 단계가 실패한다. 기본 target으로 패키징하거나 스크립트를 먼저 안전하게 고쳐라.
- 임시 앱/프로세스를 남기지 말고 종료 상태를 확인한다.
- 완료 판정은 코드가 아니라 최종 설치본의 실제 픽셀 증거가 기준이다.
