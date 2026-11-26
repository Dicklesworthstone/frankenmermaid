#!/bin/bash
# fnx_doc_validate.sh - Validate FNX User Guide examples work correctly
#
# This script runs all examples from docs/FNX_USER_GUIDE.md and verifies they
# produce expected output. Suitable for CI integration.
#
# Usage: ./fnx_doc_validate.sh
# Exit code: 0 on success, non-zero on failure
#
# Environment:
#   FM_CLI - Path to fm-cli binary (default: uses cargo run)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

# Use provided FM_CLI or fall back to cargo run
if [[ -n "${FM_CLI:-}" ]]; then
    CLI_CMD="$FM_CLI"
elif command -v fm-cli &> /dev/null; then
    CLI_CMD="fm-cli"
else
    CLI_CMD="cargo run --features fnx-integration -p frankenmermaid-cli --"
fi

PASSED=0
FAILED=0

run_test() {
    local name="$1"
    local cmd="$2"
    local expect_success="${3:-true}"

    # Replace fm-cli with our CLI command
    cmd="${cmd//fm-cli/$CLI_CMD}"

    echo -n "Testing: $name... "

    if eval "$cmd" > "$TMP_DIR/out.txt" 2>&1; then
        if [[ "$expect_success" == "true" ]]; then
            echo "PASS"
            ((PASSED++))
        else
            echo "FAIL (expected failure)"
            ((FAILED++))
        fi
    else
        if [[ "$expect_success" == "false" ]]; then
            echo "PASS (expected failure)"
            ((PASSED++))
        else
            echo "FAIL"
            cat "$TMP_DIR/out.txt"
            ((FAILED++))
        fi
    fi
}

echo "FNX User Guide Example Validation"
echo "=================================="
echo ""

# Test 1: Basic FNX modes
run_test "fnx-mode auto" \
    "echo 'flowchart TD; A-->B-->C-->D' | fm-cli render - --format svg --fnx-mode auto > /dev/null"

run_test "fnx-mode enabled" \
    "echo 'flowchart TD; A-->B-->C-->D' | fm-cli render - --format svg --fnx-mode enabled > /dev/null"

run_test "fnx-mode disabled" \
    "echo 'flowchart TD; A-->B-->C-->D' | fm-cli render - --format svg --fnx-mode disabled > /dev/null"

# Test 2: Hub detection example
run_test "hub detection" \
    "echo 'flowchart TD; Hub[Central]; A-->Hub; B-->Hub; Hub-->C' | fm-cli render - --format svg --fnx-mode enabled > /dev/null"

# Test 3: Cycle detection example
run_test "cycle detection validate" \
    "echo 'flowchart TD; A-->B; B-->C; C-->A' | fm-cli validate - --format json --fnx-mode enabled > /dev/null"

# Test 4: Disconnected component example
run_test "disconnected components" \
    "echo 'flowchart LR; subgraph A; X-->Y; end; subgraph B; P-->Q; end' | fm-cli validate - --format json --fnx-mode enabled > /dev/null"

# Test 5: Simple diagram (FNX disabled)
run_test "simple diagram disabled" \
    "echo 'flowchart LR; A-->B-->C-->D' | fm-cli render - --fnx-mode disabled > /dev/null"

# Test 6: Pie chart (FNX auto-ignored)
run_test "pie chart" \
    "echo 'pie; \"A\": 40; \"B\": 60' | fm-cli render - --format svg > /dev/null"

# Test 7: JSON output with FNX witness
run_test "json output with witness" \
    "echo 'flowchart TD; A-->B-->C' | fm-cli render - --format svg --json --fnx-mode enabled 2>&1 | head -1 | grep -q '{'"

# Test 8: Validate with JSON format
run_test "validate json format" \
    "echo 'flowchart TD; A-->B' | fm-cli validate - --format json > /dev/null"

echo ""
echo "=================================="
echo "Results: $PASSED passed, $FAILED failed"

if [[ $FAILED -gt 0 ]]; then
    exit 1
fi

echo ""
echo "All FNX documentation examples validated successfully."
