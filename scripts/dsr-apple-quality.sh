#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root/ios"

sbh check --need 20G
command -v xcodegen >/dev/null
xcodegen generate --spec project.yml
git diff --exit-code -- FrankenMermaid.xcodeproj Sources/Info.plist
/Users/jemanuel/.local/bin/ensure-simulator-audio-safe prepare
xcodebuild -project FrankenMermaid.xcodeproj -scheme FrankenMermaid \
  -destination 'generic/platform=iOS Simulator' \
  CODE_SIGNING_ALLOWED=NO build
xcodebuild -project FrankenMermaid.xcodeproj -scheme FrankenMermaid \
  -destination 'platform=macOS,variant=Mac Catalyst' \
  CODE_SIGNING_ALLOWED=NO test -only-testing:FrankenMermaidTests
