#!/usr/bin/env bash
#
# Is CI actually producing verdicts?
#
# WHY THIS EXISTS. This repo has a large set of quality gates that are enabled and marked blocking
# in .ci/quality-gates.toml -- golden checksums, property tests, invariant proofs, the
# cross-platform determinism compare, clippy. Several beads (and several of my own commits) defer a
# question to "CI will answer this". That deferral is only sound if CI RUNS.
#
# Measured on 2026-08-19: of the last 30 workflow runs, ZERO had a conclusion. Fifteen CI runs sat
# QUEUED, the oldest for 47 minutes, none started; the fourteen "cancelled" entries were all Pages
# deploys, which cancel by design through their own concurrency group. So at that moment no gate
# had produced a verdict for roughly ten commits' worth of main.
#
# That is the false-green family this project already documents (bd-7l56: gates that reported green
# while executing nothing). A queued gate is not a passing gate, and "CI is green" is not something
# anyone can infer from the absence of a red mark.
#
# Usage:
#   scripts/ci_backlog.sh            # human summary, exit 1 if no recent verdicts
#   scripts/ci_backlog.sh --limit 50

set -euo pipefail

LIMIT=30
while [ $# -gt 0 ]; do
  case "$1" in
    --limit) LIMIT="$2"; shift 2 ;;
    *) echo "[ci] unknown argument: $1" >&2; exit 2 ;;
  esac
done

if ! command -v gh > /dev/null 2>&1; then
  echo "[ci] gh is not installed; cannot inspect CI state" >&2
  exit 2
fi

# Counted over the CI workflow only. Pages deploys are excluded deliberately: they cancel each
# other by design, so including them makes a healthy repo look broken and a broken one look normal.
queued=$(gh run list --limit "$LIMIT" --workflow CI --json status --jq '[.[] | select(.status != "completed")] | length')
verdicts=$(gh run list --limit "$LIMIT" --workflow CI --json conclusion --jq '[.[] | select(.conclusion == "success" or .conclusion == "failure")] | length')
oldest=$(gh run list --limit "$LIMIT" --workflow CI --json status,createdAt --jq '[.[] | select(.status != "completed") | .createdAt] | sort | first // ""')

echo "[ci] over the last ${LIMIT} CI runs: ${verdicts} produced a verdict, ${queued} still pending"

if [ -n "$oldest" ]; then
  oldest_epoch=$(date -d "$oldest" +%s)
  now_epoch=$(date +%s)
  echo "[ci] oldest pending run has waited $(((now_epoch - oldest_epoch) / 60)) minutes"
fi

if [ "$verdicts" -eq 0 ]; then
  echo "[ci] NO VERDICTS. Every gate in .ci/quality-gates.toml is currently inert." >&2
  echo "[ci] Do not read the absence of a red mark as a pass, and do not defer a question to CI" >&2
  echo "[ci] while this is the case -- verify it locally instead." >&2
  exit 1
fi

echo "[ci] CI is producing verdicts"
