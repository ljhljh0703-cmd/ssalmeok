# Third-party notices

## CodexBar CLI 0.59.0

쌀먹은 Codex와 Claude의 로컬 사용량을 읽기 위해 CodexBar CLI 0.59.0을
보조 프로그램으로 포함합니다.

- Project: https://github.com/steipete/CodexBar
- Release: https://github.com/steipete/CodexBar/releases/tag/v0.59.0
- License: MIT
- Copyright (c) 2026 Peter Steinberger
- macOS arm64 release archive SHA-256:
  `79c862c9fa484eae877bda816d280aad740939b4ee652b9014a6528fccae4171`
- Verified source executable SHA-256:
  `5495ea31ba65644fdb105129dc2be4001039e984f8a3d6b470b27482b6d8ad99`
- Packaged executable SHA-256 after removing local symbols and ad-hoc signing:
  `b2e2dd0041fc15330ff5ad9a686a3bebe77455f78d25579072364f4b70db4da2`

The verified source executable remains unchanged in the development tree.
Only the copy inside the macOS application bundle has its local symbol table
removed before the bundle is signed.

The full license text is included in `src-tauri/resources/CodexBar-LICENSE`.

CodexBar, OpenAI, Anthropic, Codex, and Claude are names or marks of their
respective owners. This project is not affiliated with or endorsed by them.

## Provider identification icons

The Codex and Claude icons are copied from the official macOS applications
installed on the test device and are used only to identify the corresponding
services. They remain trademarks or copyrighted assets of OpenAI and
Anthropic respectively. They are not part of the MIT grant for 쌀먹, and their
presence does not imply affiliation or endorsement.

- OpenAI design guidelines: https://openai.com/brand/
- Anthropic official newsroom and media assets: https://www.anthropic.com/news
