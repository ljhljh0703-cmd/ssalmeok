# 쌀먹

[![Verify and build](https://github.com/ljhljh0703-cmd/ssalmeok/actions/workflows/verify.yml/badge.svg)](https://github.com/ljhljh0703-cmd/ssalmeok/actions/workflows/verify.yml)

Codex와 Claude의 사용 한도, 다음 초기화 시각, 로컬 토큰 비용을 macOS 메뉴 막대에서 확인하는 개인용 앱입니다.

> 상태: 빠른 피드백을 위한 0.1 MVP
> 대상: Apple Silicon · macOS 14 이상

## 무엇을 보여주나

- Codex와 Claude의 공식 로고 옆에 남은 사용량을 메뉴 막대에 각각 표시
- 메뉴 막대 아이콘 우클릭으로 서비스별 한도·초기화 시간·토큰·원화 환산을 바로 확인
- 접이식 30일 기록에서 Codex 작업별·Codex/Claude 모델별 토큰과 원화 환산 확인
- 서비스가 제공하는 5시간·주간·모델별 한도와 초기화 카운트다운
- 로컬 세션 기록에서 계산한 최근 집계·30일 토큰과 API 정가 환산액
- 마지막 정상값, 오프라인·로그인 필요·오래된 값 구분
- 남은 양 20%·5%, 소진, 초기화 완료, 연결 오류 알림
- Codex 계정에 실제 초기화권이 있을 때만 초기화 버튼 표시

토큰 비용은 구독료 외 실제 청구액이 아닙니다. 로컬 토큰 수를 API 정가로 환산한 참고값입니다.

현재 데이터 원천은 Codex 작업 이름을 제공하지만 Claude 작업 이름은 제공하지 않습니다. 따라서 Claude는 모델별 기록만 표시하며, 작업별 직접 색인은 큰 로컬 기록을 매번 훑지 않도록 보류했습니다.

## 백그라운드 동작

- Mac 로그인 때 창 없이 자동으로 시작합니다.
- Dock에는 나타나지 않고 메뉴 막대의 Codex·Claude 항목만 유지합니다.
- 메뉴 막대 항목을 왼쪽 클릭하면 본창을 열거나 닫고, 오른쪽 클릭하면 상세 메뉴를 엽니다.
- 본창을 닫으면 화면 렌더링을 해제하고 메뉴 막대 수집기만 남깁니다.
- 한도는 1분마다 갱신하고, 무거운 토큰·비용 기록 스캔은 10분마다 실행합니다.
- `지금 갱신`은 한도와 토큰·비용 기록을 모두 즉시 새로 읽습니다.
- 자동 갱신은 로컬 조회 명령만 사용하므로 Codex·Claude 토큰을 소비하지 않습니다.
- 완전히 끝내려면 메뉴 막대 항목을 오른쪽 클릭한 뒤 `종료`를 선택합니다.

## 개인정보 원칙

- 계정 사용량 조회와 캐시는 이 Mac 안에서 처리합니다.
- 계정 이메일, 인증 토큰, 초기화권 식별자는 쌀먹 화면이나 캐시에 저장하지 않습니다.
- 브라우저 쿠키와 전체 디스크 접근을 요구하지 않습니다.
- 기존 Codex·Claude 로그인을 재사용하며 별도 비밀번호를 받지 않습니다.
- 최근 정상 스냅샷 하나와 초기화 요청의 중복 방지값만 앱 데이터 폴더에 저장합니다.
- 원화 환산은 유럽중앙은행의 공개 일일 환율을 6시간 동안 캐시합니다. 이 요청에는 계정 정보가 포함되지 않습니다.

## 개발 실행

필요 도구: Xcode, Rust, Node.js, pnpm

```sh
pnpm install
pnpm sidecar:ensure
pnpm tauri dev
```

`sidecar:ensure`는 고정된 CodexBar CLI 0.59.0 배포 파일을 내려받고 SHA-256을 검증합니다. 이미 올바른 파일이 있으면 네트워크를 사용하지 않습니다.

## 검사와 빌드

```sh
pnpm verify
pnpm package:mac
```

완성된 앱은 `src-tauri/target/release/bundle/macos/쌀먹.app`에 생성됩니다.
패키징할 때 로컬 조회 도구의 불필요한 기호를 제거한 뒤 앱 전체를 다시 서명해 설치 용량을 줄입니다.

## GitHub 자동 빌드

`main` 브랜치에 올리거나 변경 요청을 만들면 공개 macOS ARM 실행 환경에서 전체 검사를 다시 수행합니다. 성공한 `main` 빌드에서는 Actions 실행 화면의 `ssalmeok-macos-arm64` 결과물을 내려받을 수 있습니다.

자동 빌드 결과물은 개인 시험용 임시 서명 상태이며 Apple 공증을 받지 않았습니다. 불특정 사용자에게 배포하려면 별도의 Developer ID 서명과 공증이 필요합니다.

## Codex 초기화권 안전장치

초기화는 조회와 다른 계정 변경 작업입니다.

1. 현재 계정에서 사용 가능한 초기화권이 있을 때만 버튼이 나타납니다.
2. 별도 확인창에서 1장 사용을 다시 선택해야 합니다.
3. 실행 직전에 OpenAI의 공식 Codex App Server에서 보유량을 다시 확인합니다.
4. 요청값을 먼저 로컬에 기록합니다. 응답이 끊기더라도 같은 요청값을 재사용해 중복 사용을 막습니다.
5. 결과를 받은 뒤 공식 사용량을 다시 읽고 일반 갱신을 시작합니다.

자동 검사에서는 초기화권을 실제로 사용하지 않습니다.

## 제3자 구성요소

사용량 수집에는 MIT License인 [CodexBar](https://github.com/steipete/CodexBar)의 명령줄 도구를 포함합니다. 버전, 검증값, 전체 허가문은 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)에 있습니다.

OpenAI, Anthropic, Codex, Claude 또는 CodexBar의 공식 앱이 아니며 해당 프로젝트의 보증을 받지 않습니다.
서비스 로고는 각 서비스를 식별하는 용도로만 사용하며, 쌀먹의 자체 브랜드나 제휴 표시로 사용하지 않습니다.
OpenAI 로고 사용은 [OpenAI 브랜드 지침](https://openai.com/brand/)을 따르며, Claude 로고는 [Anthropic 공식 미디어 자료](https://www.anthropic.com/news)의 서비스 식별 자산으로만 사용합니다.

## License

MIT
