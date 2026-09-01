#!/usr/bin/env bash
#
# Build the head-to-head benchmark ELF and PROVE its provenance stamp names the source it compiled.
#
# ⚠️ WHY A SCRIPT AND NOT A LINE IN THE README (bd-vdrx9). `crates/fm-cli/build.rs` can no longer
# derive the revision on an RCH worker -- the worker builds the transferred source inside a
# directory carrying its OWN `.git`, and it once answered `43480807` for source at `aaa334d9`: a
# real commit, 35 behind, well-formed enough to pass every shape check downstream. The only correct
# value is the one the CALLER knows, so it has to travel with the command, and two things must hold
# on every build that neither the build nor the builder can see:
#
#   1. `FM_H2H_BUILD_GIT_REV` reaches the worker. RCH forwards a variable only if it is named in
#      `[environment] allowlist`; `.rch/config.toml` names this one. An allowlist that silently
#      stops applying looks exactly like a successful build.
#   2. The value that came back out of the ELF is the value that went in.
#
# `scripts/headtohead/assert_build_stamp.py` reads (2) out of the built ELF's own `__binary__`
# record, which is the only available evidence of (1). AGENTS.md: "the real failure is ENV
# PROPAGATION" -- verify it, never assume it.
#
# Usage: scripts/headtohead/build_h2h.sh [-o <repo-relative overlay path>]... [-a <attempts>]
#   With no -o the build is `--clean-overlay --no-overlay`: exactly HEAD, nothing from the tree.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

ATTEMPTS=8
OVERLAY_ARGS=()
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) OVERLAY_ARGS+=(--overlay-path "$2"); shift 2 ;;
    -a) ATTEMPTS="$2"; shift 2 ;;
    *) echo "[build_h2h] unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ "${#OVERLAY_ARGS[@]}" -gt 0 ] || OVERLAY_ARGS=(--no-overlay)

REV="$(git rev-parse HEAD)"

# The stamp names a COMMIT. If tracked source differs from that commit then no commit describes what
# is about to be compiled, and the honest stamp is not HEAD -- it is nothing. Refusing here is the
# whole point: the failure this replaces was a build that answered the question wrongly rather than
# not at all. (Untracked files are not checked: reaching a new file from an existing target requires
# editing a tracked one, which shows up below; a brand-new example or test is its own binary.)
DIRTY="$(git --no-optional-locks status --porcelain --untracked-files=no -- \
  Cargo.toml Cargo.lock crates .cargo rust-toolchain.toml)"
if [ -n "$DIRTY" ]; then
  echo "[build_h2h] REFUSING: tracked source differs from HEAD (${REV}); no commit describes it:" >&2
  echo "$DIRTY" >&2
  echo "[build_h2h] commit the source first; the ELF's revision stamp has to name something." >&2
  exit 3
fi

# `-j2` is not a tuning knob, it is an ADMISSION requirement. RCH reserves worker slots in
# proportion to the requested job count, and an unbounded `cargo build` asks for every local core:
# `rch diagnose` answers "no admissible workers (insufficient_total_slots=10)" against a fleet with
# 26 slots free, because no single worker is that wide. The build never runs and exit 103 looks like
# fleet pressure rather than the request being unsatisfiable by construction.
echo "[build_h2h] building ${REV} via strict-remote RCH (overlay: ${OVERLAY_ARGS[*]})"
attempt=1
while :; do
  set +e
  RCH_REQUIRE_REMOTE=1 FM_H2H_BUILD_GIT_REV="$REV" \
    env -u CARGO_TARGET_DIR rch exec --base "$REV" --clean-overlay "${OVERLAY_ARGS[@]}" -- \
    cargo build --profile release -p frankenmermaid-cli --example headtohead -j2
  status=$?
  set -e
  [ "$status" -eq 0 ] && break
  # 103 is RCH's admission refusal (no admissible worker). It is retryable and it writes NO
  # artifact, so it must never be mistaken for a build that is merely taking a while.
  if [ "$status" -ne 103 ] || [ "$attempt" -ge "$ATTEMPTS" ]; then
    echo "[build_h2h] rch exec failed with status ${status} after ${attempt} attempt(s)" >&2
    exit "$status"
  fi
  echo "[build_h2h] attempt ${attempt}/${ATTEMPTS}: RCH admission refused (103); retrying in 90s"
  attempt=$((attempt + 1))
  sleep 90
done

BIN=target/release/examples/headtohead
[ -x "$BIN" ] || BIN=target/local/release/examples/headtohead
[ -x "$BIN" ] || {
  echo "[build_h2h] no benchmark ELF at target/{,local/}release/examples/headtohead" >&2
  exit 4
}

exec python3 scripts/headtohead/assert_build_stamp.py "$BIN" "$REV"
