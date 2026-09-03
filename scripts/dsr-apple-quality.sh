#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root/ios"

build_root="${FRANKEN_APPLE_BUILD_ROOT:-$repo_root/ios/build/dsr-apple-quality}"
mkdir -p "$build_root/tmp"
sbh check --need 20G "$build_root"
command -v xcodegen >/dev/null
xcodegen generate --spec project.yml
git diff --exit-code -- FrankenMermaid.xcodeproj Sources/Info.plist
/Users/jemanuel/.local/bin/ensure-simulator-audio-safe prepare
TMPDIR="$build_root/tmp" xcodebuild -project FrankenMermaid.xcodeproj -scheme FrankenMermaid \
  -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath "$build_root/derived-data" \
  CODE_SIGNING_ALLOWED=NO build
TMPDIR="$build_root/tmp" xcodebuild -project FrankenMermaid.xcodeproj -scheme FrankenMermaid \
  -destination 'platform=macOS,variant=Mac Catalyst' \
  -derivedDataPath "$build_root/derived-data" \
  CODE_SIGNING_ALLOWED=NO test -only-testing:FrankenMermaidTests
