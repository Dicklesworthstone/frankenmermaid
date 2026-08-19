#!/usr/bin/env bash
#
# Drive the vs-incumbent measurement chain at ONE pinned revision, with the prechecks that turned
# out to matter in practice.
#
# WHY THIS EXISTS. run.mjs requires three things to name the same commit: the ELF's baked-in
# FM_H2H_BUILD_GIT_REV, the --fm-build-base argument, and checked-out HEAD. The equivalence proof
# it demands is rev-pinned too. On a shared main that is a RACE, and it is lost SILENTLY -- the
# failure surfaces as
#
#     INVALID: benchmark ELF build revision must match both --fm-build-base and checked-out HEAD
#
# after the build and a ~90s equivalence run have already completed, with nothing saying that a
# peer committed in the meantime. Measured on this repo, the median gap between commits to main is
# 272 seconds (minimum 8) while the chain needs roughly two minutes, so it is winnable and lost
# often. It was lost twice in one session.
#
# This pins HEAD once, re-checks before every step, and aborts at the CHEAPEST point with a reason.
#
# It also refuses to start while another project on this host is benchmarking. That check is a
# stand-in, NOT a substitute, for the fleet-wide `acquire_build_slot` lease: that tool currently
# refuses with "Build slots are disabled. Enable WORKTREES_ENABLED to use this tool.", so there is
# no lease to take. A ps scan cannot stop a peer STARTING mid-run, which a lease could.
#
# Usage:
#   scripts/headtohead/measure_chain.sh --only sequence_20 [--mode render|parse] [--attempts 3]
#   scripts/headtohead/measure_chain.sh --only sequence_20 --dry-run
#   scripts/headtohead/measure_chain.sh --only sequence_20 --wait 1200
#
# --wait SECONDS polls for a clear window instead of refusing on the first look. Windows on this
# host are REAL BUT BRIEF -- observed iowait moving 0.31% -> 9.2% -> 10.5% inside a minute, and a
# fleet peer's benchmark starting and finishing between two ticks. A single sample therefore misses
# windows that exist, which is why seven consecutive attempts here found the host busy while the
# reported aggregates looked fine. The wait is BOUNDED and prints every poll: no unbounded loop, and
# a refusal after the deadline is a result, not a hang.

set -euo pipefail

MODE=render
ONLY=""
ATTEMPTS=3
DRY_RUN=0
MIN_FREE_GB=42
WAIT_SECONDS=0
POLL_SECONDS=30

while [ $# -gt 0 ]; do
  case "$1" in
    --only)     ONLY="$2"; shift 2 ;;
    --mode)     MODE="$2"; shift 2 ;;
    --attempts) ATTEMPTS="$2"; shift 2 ;;
    --dry-run)  DRY_RUN=1; shift ;;
    --wait)     WAIT_SECONDS="$2"; shift 2 ;;
    *) echo "[chain] unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [ -z "$ONLY" ]; then
  echo "[chain] --only <case-id> is required" >&2
  exit 2
fi

cd "$(git rev-parse --show-toplevel)"

# -- precheck 1: disk --------------------------------------------------------------------------
free_gb=$(df -BG --output=avail /data | tail -1 | tr -dc '0-9')
if [ "$free_gb" -lt "$MIN_FREE_GB" ]; then
  echo "[chain] REFUSING: /data has ${free_gb}G free, below the ${MIN_FREE_GB}G floor" >&2
  exit 3
fi
echo "[chain] disk ${free_gb}G free"

# -- precheck 2: is anyone else on this host measuring? ----------------------------------------
# Deliberately matches BENCHMARK drivers, not compilers. A peer's cargo build perturbs a ratio but
# does not invalidate their run; two measurements at once invalidate BOTH. Nine projects
# benchmarking simultaneously is what took this host to run queue 122.
# ⚠️ THE COMMAND LINE IS TRUNCATED, and that is not cosmetic. A peer's benchmark can be an inline
# interpreter program: the first run of this script printed 74KB because a frankentorch arm passes
# its whole source with `python -c`. An unreadable refusal is a refusal people learn to ignore.
# ⚠️ COMPILERS ARE EXCLUDED EXPLICITLY, and the first version of this check got that wrong. It
# refused on `rustc` and `rustfmt`, because a crate name or a source path routinely contains the
# substring "bench" -- so the check contradicted the comment directly above it and would have
# blocked every measurement taken while any project in the fleet was compiling anything.
MAX_IOWAIT_PCT=5

# `window_check` prints its own findings and returns 0 only when the host is fit to measure on.
# It is a FUNCTION rather than straight-line code so that --wait can re-run it; with --wait 0 the
# behaviour is identical to refusing on the first look.
#
# WINDOW_EXIT carries which precheck refused, so the caller exits with the same code it would have
# without --wait: 4 for a competing benchmark, 6 for a disk-bound host.
WINDOW_EXIT=0
window_check() {
  WINDOW_EXIT=0

  local busy_bench
  busy_bench=$(ps -eo pcpu,args --no-headers | grep -E 'bench|harness|perf_|criterion|vs_pandas|vs_scipy|headtohead' | grep -vE 'measure_chain|/rustc |/rustfmt |/cargo |/ld |cc1plus' | while read -r pcpu rest; do
    if [ "${pcpu%%.*}" -gt 50 ]; then printf '    %s %.100s\n' "$pcpu" "$rest"; fi
  done)
  if [ -n "$busy_bench" ]; then
    echo "[chain] another benchmark is running on this host:"
    echo "$busy_bench"
    WINDOW_EXIT=4
    return 1
  fi

  local -A cpu_total cpu_idle
  local agg_total_before=0 agg_iowait_before=0 name rest total field
  while read -r name rest; do
    case "$name" in
      cpu) set -- $rest
           for field in "$@"; do agg_total_before=$((agg_total_before + field)); done
           agg_iowait_before=$5
           continue ;;
      cpu[0-9]*) ;;
      *) continue ;;
    esac
    set -- $rest
    total=0
    for field in "$@"; do total=$((total + field)); done
    cpu_total["$name"]=$total
    cpu_idle["$name"]=$(($4 + $5))
  done < /proc/stat

  sleep 3

  local busy_cpus=0 total_cpus=0 agg_total_after=0 agg_iowait_after=0 idle delta_total delta_idle
  while read -r name rest; do
    case "$name" in
      cpu) set -- $rest
           for field in "$@"; do agg_total_after=$((agg_total_after + field)); done
           agg_iowait_after=$5
           continue ;;
      cpu[0-9]*) ;;
      *) continue ;;
    esac
    set -- $rest
    total=0
    for field in "$@"; do total=$((total + field)); done
    idle=$(($4 + $5))
    delta_total=$((total - ${cpu_total["$name"]:-0}))
    delta_idle=$((idle - ${cpu_idle["$name"]:-0}))
    total_cpus=$((total_cpus + 1))
    if [ "$delta_total" -gt 0 ] && [ $(((delta_total - delta_idle) * 100 / delta_total)) -gt 20 ]; then
      busy_cpus=$((busy_cpus + 1))
    fi
  done < /proc/stat

  local agg_delta=$((agg_total_after - agg_total_before))
  # TENTHS, NOT WHOLE PERCENT. Integer division made the first version of this gate LOOSER than the
  # ceiling it claims to enforce: 5.9% truncates to 5, which is not `> 5`, so a window
  # abba_render.py would refuse passed here. Shell has no floats, so the comparison runs in tenths
  # against 50 and only the display is decimal.
  IOWAIT_TENTHS=0
  if [ "$agg_delta" -gt 0 ]; then
    IOWAIT_TENTHS=$(((agg_iowait_after - agg_iowait_before) * 1000 / agg_delta))
  fi

  # Printed on every poll, not only on refusal: a row banked from a clean window has to be able to
  # SHOW it was clean, and "iowait was fine" is not something a reader can verify after the fact.
  echo "[chain] ${busy_cpus} of ${total_cpus} cpus over 20% busy; iowait $((IOWAIT_TENTHS / 10)).$((IOWAIT_TENTHS % 10))%; loadavg $(cut -d' ' -f1-3 /proc/loadavg)"

  if [ "$IOWAIT_TENTHS" -gt $((MAX_IOWAIT_PCT * 10)) ]; then
    echo "[chain] host is disk-bound at $((IOWAIT_TENTHS / 10)).$((IOWAIT_TENTHS % 10))% iowait, over the ${MAX_IOWAIT_PCT}% ceiling"
    echo "[chain] A disk-bound host does not slow the two engines equally, so the ratio would be an"
    echo "[chain] artifact of the disk. Same ceiling abba_render.py refuses arms on."
    WINDOW_EXIT=6
    return 1
  fi

  return 0
}

# BOUNDED wait. With --wait 0 (the default) this exits on the first refusal, exactly as before.
window_deadline=$((SECONDS + WAIT_SECONDS))
while true; do
  if window_check; then
    break
  fi
  if [ "$SECONDS" -ge "$window_deadline" ]; then
    echo "[chain] REFUSING: the host was not fit to measure on." >&2
    if [ "$WAIT_SECONDS" -gt 0 ]; then
      echo "[chain] Waited ${WAIT_SECONDS}s. A window that never opened in that time is a finding" >&2
      echo "[chain] about fleet contention, not a reason to raise --wait." >&2
    fi
    exit "$WINDOW_EXIT"
  fi
  echo "[chain] not fit yet; polling again in ${POLL_SECONDS}s ($((window_deadline - SECONDS))s of budget left)"
  sleep "$POLL_SECONDS"
done

REV=$(git rev-parse HEAD)
echo "[chain] pinning revision ${REV}"

if [ "$DRY_RUN" -eq 1 ]; then
  echo "[chain] DRY RUN -- prechecks passed; would now build, prove equivalence and measure at ${REV}"
  exit 0
fi

# Every step re-checks, so a peer's commit aborts the chain at the cheapest point rather than after
# the equivalence run.
head_moved() {
  [ "$(git rev-parse HEAD)" != "$REV" ]
}

attempt=1
while [ "$attempt" -le "$ATTEMPTS" ]; do
  echo "[chain] attempt ${attempt}/${ATTEMPTS} at ${REV}"

  RCH_CARGO_WRAPPER_BYPASS=1 FM_H2H_BUILD_GIT_REV="$REV" \
    env -u CARGO_TARGET_DIR cargo build --release -p frankenmermaid-cli --example headtohead
  BIN=target/local/release/examples/headtohead
  [ -x "$BIN" ] || BIN=target/release/examples/headtohead

  if head_moved; then
    echo "[chain] HEAD moved during the build ($(git rev-parse --short HEAD)); re-pinning"
    REV=$(git rev-parse HEAD)
    attempt=$((attempt + 1))
    continue
  fi

  node scripts/headtohead/equivalence.mjs --only "$ONLY" --fm-bin "$BIN"

  if head_moved; then
    echo "[chain] HEAD moved during equivalence ($(git rev-parse --short HEAD)); re-pinning"
    REV=$(git rev-parse HEAD)
    attempt=$((attempt + 1))
    continue
  fi

  node scripts/headtohead/run.mjs \
    --mode "$MODE" --only "$ONLY" --fm-bin "$BIN" --fm-build-base "$REV"
  echo "[chain] completed at ${REV}"
  exit 0
done

echo "[chain] GAVE UP after ${ATTEMPTS} attempts: HEAD kept moving mid-chain." >&2
echo "[chain] That is a finding about main's commit rate, not a flake. Record it rather than" >&2
echo "[chain] raising --attempts, which only spends more builds losing the same race." >&2
exit 5
