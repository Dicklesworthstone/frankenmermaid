#!/usr/bin/env bash
#
# Is the local release path ready to produce a verdict?
#
# WHY THIS EXISTS. This repo has a large set of quality gates that are enabled and marked blocking
# in .ci/quality-gates.toml -- golden checksums, property tests, invariant proofs, the
# cross-platform determinism compare, clippy. Several beads (and several of my own commits) defer a
# question to "automation will answer this". That deferral is only sound if the local release path
# is registered and ready.
#
# Usage:
#   scripts/ci_backlog.sh                 # verify the /dsr registration and host health
#   scripts/ci_backlog.sh --repo <name>   # check another registered /dsr repository

set -euo pipefail

REPO=frankenmermaid
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) REPO="$2"; shift 2 ;;
    *) echo "[ci] unknown argument: $1" >&2; exit 2 ;;
  esac
done

if ! command -v dsr > /dev/null 2>&1; then
  echo "[ci] /dsr is not installed; local release readiness cannot be verified" >&2
  exit 2
fi

if ! dsr repos info "$REPO" --json; then
  echo "[ci] /dsr has no registered release definition for ${REPO}; no release verdict exists." >&2
  echo "[ci] Register the repository and its required artifacts before treating release checks as available." >&2
  exit 1
fi

dsr health all --json
echo "[ci] /dsr registration and build-host health are available; run the named /dsr quality path for a verdict."
