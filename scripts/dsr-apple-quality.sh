#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root/ios"

build_root="${FRANKEN_APPLE_BUILD_ROOT:-${DSR_QUALITY_RUN_DIR:-$repo_root/ios/build/dsr-apple-quality}}"
mkdir -p "$build_root/tmp"
result_bundle="$build_root/frankenmermaid-iphone-ui-$(git rev-parse --short=12 HEAD)-$(date -u +%Y%m%dT%H%M%SZ).xcresult"
sbh check --need 20G "$build_root"
command -v xcodegen >/dev/null
xcodegen generate --spec project.yml
git diff --exit-code -- FrankenMermaid.xcodeproj Sources/Info.plist
git ls-files -z -- '*.swift' | xargs -0 xcrun swiftc -parse -enable-bare-slash-regex
plutil -lint Sources/Info.plist
plutil -lint Sources/PrivacyInfo.xcprivacy
/Users/jemanuel/.local/bin/ensure-simulator-audio-safe prepare
TMPDIR="$build_root/tmp" xcodebuild -project FrankenMermaid.xcodeproj -scheme FrankenMermaid \
  -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath "$build_root/derived-data" \
  CODE_SIGNING_ALLOWED=NO build
TMPDIR="$build_root/tmp" xcodebuild -project FrankenMermaid.xcodeproj -scheme FrankenMermaid \
  -destination 'platform=macOS,variant=Mac Catalyst' \
  -derivedDataPath "$build_root/derived-data" \
  CODE_SIGNING_ALLOWED=NO test -only-testing:FrankenMermaidTests

# Resolve a concrete phone only after re-proving the Simulator audio fence;
# the UI lane may boot a currently shut-down device.
/Users/jemanuel/.local/bin/ensure-simulator-audio-safe prepare
simulator_id="${FM_IOS_SIMULATOR_ID:-}"
if [[ -z "$simulator_id" ]]; then
  simulator_id="$({ xcrun simctl list devices available || true; } | awk -F '[()]' '
    /iPhone/ && /\(Booted\)$/ { print $2; found = 1; exit }
    /iPhone/ && fallback == "" { fallback = $2 }
    END { if (!found) print fallback }
  ')"
fi
if [[ -z "$simulator_id" ]]; then
  echo "No available iPhone Simulator for FrankenMermaid UI tests" >&2
  exit 1
fi

/Users/jemanuel/.local/bin/ensure-simulator-audio-safe prepare
TMPDIR="$build_root/tmp" xcodebuild -project FrankenMermaid.xcodeproj -scheme FrankenMermaid \
  -destination "platform=iOS Simulator,id=$simulator_id" \
  -derivedDataPath "$build_root/derived-data" \
  -resultBundlePath "$result_bundle" \
  -parallel-testing-enabled NO \
  -maximum-parallel-testing-workers 1 \
  CODE_SIGNING_ALLOWED=NO test \
  -only-testing:FrankenMermaidUITests

echo "FrankenMermaid UI result bundle: $result_bundle"
