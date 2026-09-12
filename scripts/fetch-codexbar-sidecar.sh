#!/usr/bin/env bash
set -euo pipefail

ssal_codexbar_version="0.59.0"
ssal_archive_sha="79c862c9fa484eae877bda816d280aad740939b4ee652b9014a6528fccae4171"
ssal_binary_sha="5495ea31ba65644fdb105129dc2be4001039e984f8a3d6b470b27482b6d8ad99"
ssal_repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ssal_target="$ssal_repo_root/src-tauri/binaries/codexbar-aarch64-apple-darwin"

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "쌀먹 0.1은 Apple Silicon macOS만 지원합니다." >&2
  exit 1
fi

if [[ -x "$ssal_target" ]]; then
  ssal_existing_sha=$(shasum -a 256 "$ssal_target" | awk '{print $1}')
  if [[ "$ssal_existing_sha" == "$ssal_binary_sha" ]]; then
    echo "CodexBar CLI ${ssal_codexbar_version}: verified"
    exit 0
  fi
  echo "기존 CodexBar 보조 프로그램의 검증값이 다릅니다. 자동 덮어쓰기를 중단합니다." >&2
  exit 1
fi

ssal_sidecar_tmp=$(mktemp -d /private/tmp/ssalmeok-sidecar.XXXXXX)
trap '/bin/rm -rf "$ssal_sidecar_tmp"' EXIT

ssal_archive="$ssal_sidecar_tmp/codexbar.tar.gz"
ssal_url="https://github.com/steipete/CodexBar/releases/download/v${ssal_codexbar_version}/CodexBarCLI-v${ssal_codexbar_version}-macos-arm64.tar.gz"

curl --fail --location --silent --show-error "$ssal_url" --output "$ssal_archive"
ssal_downloaded_sha=$(shasum -a 256 "$ssal_archive" | awk '{print $1}')
if [[ "$ssal_downloaded_sha" != "$ssal_archive_sha" ]]; then
  echo "CodexBar 배포 파일의 검증값이 일치하지 않습니다." >&2
  exit 1
fi

tar -xzf "$ssal_archive" -C "$ssal_sidecar_tmp"
ssal_extracted_sha=$(shasum -a 256 "$ssal_sidecar_tmp/CodexBarCLI" | awk '{print $1}')
if [[ "$ssal_extracted_sha" != "$ssal_binary_sha" ]]; then
  echo "CodexBar 실행 파일의 검증값이 일치하지 않습니다." >&2
  exit 1
fi

mkdir -p "$(dirname "$ssal_target")"
install -m 755 "$ssal_sidecar_tmp/CodexBarCLI" "$ssal_target"
echo "CodexBar CLI ${ssal_codexbar_version}: installed and verified"
