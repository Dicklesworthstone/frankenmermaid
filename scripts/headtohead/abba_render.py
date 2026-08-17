#!/usr/bin/env python3
"""Render-scoped A/B/B/A against pinned mermaid-js, in ONE invocation.

This is the script that produced the PROVISIONAL 362.4x row in docs/NEGATIVE_EVIDENCE.md
("PROVISIONAL MEASUREMENT: sequence_20 render, worst bound 362.4x"). It lived in a scratchpad, which
made that row unreproducible by anyone else; it is here so the number can be re-derived or refuted.

WHAT IT IS NOT
--------------
It is NOT a certification harness and it must not be used as one. `scripts/headtohead/run.mjs` owns
certification, and its host-wide exclusivity gate -- every one of 64 CPUs below 20% busy in a single
1-second sample -- has refused eight consecutive attempts on this host and has no override flag. This
script deliberately runs the same two arms WITHOUT that gate so a number exists at all, and records
the conditions per arm so a reader can discount it appropriately. Any row it produces is PROVISIONAL.

WHY A/B/B/A IN ONE INVOCATION
-----------------------------
Interleaved in a single process so both arms see the same machine state, and so fm is measured on
BOTH sides of the incumbent. The bracket yields a drift figure: if the two fm observations disagree
by much, the window was not stable and the ratio is not worth quoting.

WHY THE RATIO IS A BOUND, NOT A POINT ESTIMATE
----------------------------------------------
It divides the FASTER mermaid observation by the SLOWER fm observation -- the worst bound either arm
produced, per the fleet's replicated-standing convention. The headline median/median is printed too,
but the bound is the number to quote. Per-core clocks on this host span ~1429-4300 MHz simultaneously
(2.88-3.01x spread), and that confounder is unaccounted for, which is the other reason for a bound.

RENDER MODE, NOT PARSE
----------------------
This measures RENDER and asserts its work proof in band rather than trusting the timing.

Do NOT reach for `parse_accepted_revisions` as that work proof. It is a parse-QUALITY counter, not a
liveness counter: it increments only when a revision has no errors, no recovery, no warnings, AND
`support_label() == "full"` -- and `Sequence` is the one type labelled `partial`, so it is
identically ZERO for every sequence diagram however well the parse ran. Gating on it would silently
refuse every sequence row, including `sequence_20`, the project's worst measured ratio. See the
RETRACTED entry in docs/NEGATIVE_EVIDENCE.md; I made exactly that mistake.

Separately and still unexplained: `FM_H2H_MODE=parse` reports `parse_ns.p50 = 8` for a 1,257-byte
diagram, which is ~25 cycles and not plausible. That is a reason to distrust the parse arm's timing
on its own merits, not a reason to trust the counter above.

USAGE
-----
    python3 scripts/headtohead/abba_render.py --fm-bin <path> --corpus <corpus.json>

`--fm-bin` should be a binary you have PINNED BY CONTENT (copy it to `<exe>.<agent>.<sha8>` and pass
that), because the shared build path can be rebuilt by a peer mid-run -- that has happened to this
harness before.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import statistics
import subprocess
import sys

BENCH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "mermaid_bench.mjs")
PICK_PINS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "pick_pins.mjs")

# Physics ceiling from the work-proof gate: no single observed thread emits more than this.
MAX_BYTES_PER_NS = 512.0


def loadavg() -> list[float]:
    with open("/proc/loadavg", encoding="utf-8") as handle:
        return [float(x) for x in handle.read().split()[:3]]


def cpu_mhz() -> dict | None:
    """Observed per-core clock right now, not the governor's policy limits.

    The policy limits are what an environment block usually records, and they cannot distinguish an
    arm that ran at 1.4 GHz from one that ran at 4.3 GHz. Both are reachable within a single run.
    """
    vals = []
    for entry in os.listdir("/sys/devices/system/cpu"):
        if not re.fullmatch(r"cpu\d+", entry):
            continue
        path = f"/sys/devices/system/cpu/{entry}/cpufreq/scaling_cur_freq"
        try:
            with open(path, encoding="utf-8") as handle:
                vals.append(int(handle.read()) // 1000)
        except OSError:
            pass
    if not vals:
        return None
    lo, hi = min(vals), max(vals)
    return {
        "min_mhz": lo,
        "max_mhz": hi,
        "mean_mhz": round(sum(vals) / len(vals)),
        "spread": round(hi / max(lo, 1), 3),
        "cores": len(vals),
    }


def conditions() -> dict:
    return {"loadavg": loadavg(), "mhz": cpu_mhz()}


def pick_pins(size: int = 8) -> dict | None:
    """Cores for both arms, chosen by the SAME rule run.mjs uses.

    Delegates to `pick_pins.mjs`, which imports `cpu_selection.mjs`, rather than reimplementing the
    choice here. Two implementations of "which core" is precisely how the arms ended up under
    different clock regimes (bd-hmfi): ours pinned to the 1429 MHz floor while the incumbent ran
    unpinned on boosted cores.
    """
    try:
        out = subprocess.run(
            ["node", PICK_PINS, str(size)], capture_output=True, text=True, check=False
        )
        return json.loads(out.stdout.strip().splitlines()[-1])
    except (OSError, ValueError, IndexError):
        return None


def _records(stdout: str, case_id: str):
    for line in stdout.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            record = json.loads(line)
        except ValueError:
            continue
        if record.get("id") == case_id:
            yield record


def fm_arm(fm_bin: str, corpus: str, case_id: str, pins: dict | None = None) -> dict:
    """FrankenMermaid full pipeline, with the counted work proof read from the same record."""
    before = conditions()
    argv = [fm_bin, corpus]
    if pins and pins.get("fm_cpu") is not None:
        argv = ["taskset", "-c", str(pins["fm_cpu"]), *argv]
    proc = subprocess.run(argv, capture_output=True, text=True, check=False)
    after = conditions()
    ns = None
    work = None
    for record in _records(proc.stdout, case_id):
        ns = (record.get("pipeline_ns") or {}).get("p50")
        work = {
            "bytes": record.get("svg_bytes") or record.get("output_bytes"),
            "batch": record.get("batch"),
            "revisions": record.get("revisions"),
        }
    return {"ns": ns, "work": work, "before": before, "after": after, "code": proc.returncode}


def mermaid_arm(case_id: str, reps: int, pins: dict | None = None) -> dict:
    """Pinned mermaid-js render through chromium via CDP -- the same boundary as the fm arm."""
    before = conditions()
    argv = ["node", BENCH, "--only", case_id, "--reps", str(reps)]
    if pins and pins.get("incumbent_cpus"):
        # A cpuset, not a single core: Chromium is multi-process, and starving it would slow the
        # INCUMBENT and inflate our ratio -- an over-claim in our own favour.
        argv = ["taskset", "-c", ",".join(str(c) for c in pins["incumbent_cpus"]), *argv]
    proc = subprocess.run(argv, capture_output=True, text=True, check=False)
    after = conditions()
    ns = None
    null_ci = None
    for record in _records(proc.stdout, case_id):
        if record.get("status") != "ok":
            continue
        # render_ns, NOT parse_ns: the fm arm above is a full pipeline, so the incumbent must be
        # measured at the same boundary or the ratio compares two different quantities.
        ns = (record.get("render_ns") or {}).get("p50")
        null = record.get("null_control") or {}
        null_ci = (null.get("median"), null.get("ci95_lo"), null.get("ci95_hi"))
    return {"ns": ns, "null": null_ci, "before": before, "after": after, "code": proc.returncode}


def head_revision() -> str | None:
    """The checked-out revision, or None if this is not a git tree."""
    try:
        out = subprocess.run(
            ["git", "rev-parse", "HEAD"], capture_output=True, text=True, check=False
        )
        rev = out.stdout.strip()
        return rev if len(rev) == 40 and all(c in "0123456789abcdef" for c in rev) else None
    except OSError:
        return None


def check_elf_provenance(fm_bin: str, expected_rev: str | None) -> str | None:
    """Refuse a binary that was not built from the checked-out revision.

    THIS EXISTS BECAUSE ITS ABSENCE COST A CERTIFICATION. `run.mjs` enforces it -- "INVALID: benchmark
    ELF build revision must match both --fm-build-base and checked-out HEAD" -- and this script did
    not, so a row could be taken here with a binary built from a different revision and nothing would
    say so. I hit exactly that: a binary at rev 07768c81 measured against a tree at 6b78da29, and only
    the certified driver caught it.

    Checked by searching the ELF for the revision string the build embeds, which is the same evidence
    the provenance gate uses -- the rev must be IN the binary, not merely claimed beside it. An
    unreadable binary or a non-git tree returns a reason rather than passing silently: a check that
    cannot run must not report success.
    """
    if expected_rev is None:
        return "cannot determine HEAD revision, so provenance cannot be established"
    try:
        with open(fm_bin, "rb") as handle:
            blob = handle.read()
    except OSError as error:
        return f"cannot read {fm_bin}: {error}"
    if expected_rev.encode("ascii") not in blob:
        return (
            f"the binary does not embed HEAD {expected_rev[:8]}; it was built from a different "
            "revision, so any ratio it produces describes code that is not checked out"
        )
    return None


def check_work_proof(arm: dict) -> str | None:
    """Refuse to quote a timing the arm did not earn.

    A gate that only checks "did it produce a number" passes while the arm measures nothing. The two
    load-bearing checks here are properties of the WORK: bytes actually emitted, and a
    bytes-per-nanosecond rate below what one thread can physically sustain. The `revisions` check is
    only a malformed-record guard -- that field is the corpus item's revision count, so it says the
    record is well formed, NOT that the engine did anything.
    """
    work = arm.get("work") or {}
    ns = arm.get("ns")
    if not ns:
        return "no timing"
    revisions = work.get("revisions")
    if not revisions:
        return f"revisions={revisions!r} -- malformed record, the corpus item claims no revisions"
    written = work.get("bytes")
    if not written:
        return f"bytes={written!r} -- nothing was emitted"
    rate = written / ns
    if rate > MAX_BYTES_PER_NS:
        return f"{rate:.1f} bytes/ns exceeds {MAX_BYTES_PER_NS} -- a memo hit, not real work"
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fm-bin", required=True, help="content-pinned frankenmermaid h2h binary")
    parser.add_argument("--corpus", required=True, help="corpus json consumed by --fm-bin")
    parser.add_argument("--case", default="sequence_20", help="corpus case id (default sequence_20)")
    parser.add_argument("--reps", type=int, default=9, help="mermaid reps per arm")
    parser.add_argument(
        "--incumbent-cpus", type=int, default=8, help="cpuset size for the incumbent arm"
    )
    parser.add_argument(
        "--no-pin", action="store_true", help="run both arms unpinned (the pre-bd-hmfi behaviour)"
    )
    parser.add_argument(
        "--allow-stale-elf",
        action="store_true",
        help="measure a binary not built from HEAD; the row must then state which revision it was",
    )
    args = parser.parse_args()

    print("=== A/B/B/A, one invocation, RENDER-scoped, UNCERTIFIED (no host-exclusivity gate) ===")
    rev = head_revision()
    provenance = check_elf_provenance(args.fm_bin, rev)
    if provenance is not None and not args.allow_stale_elf:
        print(f"REFUSING TO MEASURE: {provenance}")
        print("Rebuild with FM_H2H_BUILD_GIT_REV=$(git rev-parse HEAD), or pass --allow-stale-elf")
        print("if you are deliberately measuring an older revision and will say so in the row.")
        return 2
    if provenance is None:
        print(f"PROVENANCE: binary embeds HEAD {rev[:8]}")
    else:
        print(f"PROVENANCE OVERRIDDEN: {provenance}")
    pins = None if args.no_pin else pick_pins(args.incumbent_cpus)
    if pins is None:
        print("PINS: none -- both arms run unpinned (symmetric, but clocks uncontrolled)")
    else:
        print(
            f"PINS: fm cpu{pins['fm_cpu']} @ {pins['fm_mhz']} MHz ({pins['fm_rule']}); "
            f"incumbent {len(pins['incumbent_cpus'])} cpus {pins['incumbent_cpus']}, "
            f"slowest {pins['incumbent_min_mhz']} MHz, starved={pins['incumbent_starved']}; "
            f"host spread {pins['host_spread']}x, {pins['busy_cpus_over_20pct']}/{pins['total_cpus']} cpus busy"
        )
    a1 = fm_arm(args.fm_bin, args.corpus, args.case, pins)
    print(f"A1 fm      ns={a1['ns']} work={a1['work']} load={a1['before']['loadavg']} mhz={a1['before']['mhz']}")
    b1 = mermaid_arm(args.case, args.reps, pins)
    print(f"B1 mermaid ns={b1['ns']} load={b1['before']['loadavg']} mhz={b1['before']['mhz']}")
    b2 = mermaid_arm(args.case, args.reps, pins)
    print(f"B2 mermaid ns={b2['ns']} load={b2['before']['loadavg']} mhz={b2['before']['mhz']}")
    a2 = fm_arm(args.fm_bin, args.corpus, args.case, pins)
    print(f"A2 fm      ns={a2['ns']} work={a2['work']} load={a2['before']['loadavg']} mhz={a2['before']['mhz']}")

    for name, arm in (("A1", a1), ("A2", a2)):
        why = check_work_proof(arm)
        if why is not None:
            print(f"\nREFUSING TO QUOTE A RATIO: fm arm {name} failed its work proof -- {why}")
            return 2

    fm_vals = [arm["ns"] for arm in (a1, a2) if arm["ns"]]
    mj_vals = [arm["ns"] for arm in (b1, b2) if arm["ns"]]
    if not fm_vals or not mj_vals:
        print(f"\nINCOMPLETE fm={fm_vals} mjs={mj_vals}")
        return 2

    print()
    print(f"fm  observations ns: {fm_vals}  drift {max(fm_vals) / min(fm_vals):.4f}x")
    print(f"mjs observations ns: {mj_vals}")
    # Worst bound: slower fm against faster mermaid.
    print(f"WORST-BOUND RATIO: {min(mj_vals) / max(fm_vals):.1f}x")
    print(f"headline (median/median): {statistics.median(mj_vals) / statistics.median(fm_vals):.1f}x")
    print(f"mermaid A/A null: {b1['null']}  {b2['null']}")
    print(f"conditions at end: load={loadavg()} mhz={cpu_mhz()}")
    print()
    print("PROVISIONAL. Quote the worst bound, cite the executing ELF sha, and record per-arm loadavg")
    print("and CPU MHz -- a cross-core spread near 3x is why this is a bound and not an estimate.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
