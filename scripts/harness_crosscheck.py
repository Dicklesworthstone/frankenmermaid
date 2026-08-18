#!/usr/bin/env python3
"""Cross-check the three measurement harnesses on ONE primitive (bd-8bbq).

WHY THIS EXISTS
---------------
frankenlibc measured malloc/free on the SAME worker through two separately-sanctioned harnesses and
got 5.9459x and 12.385414x -- a ~2x spread -- with BOTH A/A nulls passing. A passing null does not
certify that a harness measures what its row says it measures. This project has three measurement
paths and has never compared them against each other:

  1. scripts/headtohead/run.mjs        wall time, whole-job phases      -> ns/op
  2. perf stat instruction A/B         retired instructions             -> instructions/op
  3. per-crate criterion benches       wall time, one function          -> ns/op

THE UNIT PROBLEM, WHICH DECIDES WHAT THIS SCRIPT MAY CONCLUDE
-------------------------------------------------------------
Two of the three measure TIME and one measures INSTRUCTIONS. Those are different physical
quantities: instructions/op cannot be divided by ns/op to produce anything meaningful, and a script
that printed such a ratio would be inventing a number nobody can interpret.

So the cross-check is deliberately asymmetric, and this is the substance of the design:

  * run.mjs vs criterion  -- SAME unit (ns/op). Their ratio IS the cross-check. Disagreement here
    is the finding the bead is looking for.
  * perf stat             -- reported alongside as instructions/op, and explicitly NOT divided into
    the others. It cross-checks only against another INSTRUCTION measurement, which is why every
    instruction-based row in the ledger can be compared to another instruction row and to nothing
    else.

WHAT COUNTS AS THE RESULT
-------------------------
If the two time harnesses disagree, THE DISAGREEMENT IS THE FINDING. This script therefore refuses
to pick a winner, refuses to average, and prints every number with its harness named. Averaging two
harnesses that disagree by 2x produces a number that is wrong in a new way and traceable to neither.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# Two runs of the same harness that differ by more than this are not measuring an engine, they are
# measuring the host. Same reasoning and same value as abba_render's drift control: the honest
# unpinned brackets on this host land at 1.009-1.048, the pathological ones at ~1.72.
MAX_SELF_DRIFT = 1.10


def host_conditions() -> dict:
    """loadavg and CPU MHz, recorded on every row because a row without them cannot be audited."""
    loadavg = Path("/proc/loadavg").read_text(encoding="utf-8").split()[:3]
    mhz = [
        float(m)
        for m in re.findall(r"cpu MHz\s*:\s*([\d.]+)", Path("/proc/cpuinfo").read_text(encoding="utf-8"))
    ]
    mhz.sort()
    return {
        "loadavg": [float(v) for v in loadavg],
        "mhz_min": mhz[0] if mhz else None,
        "mhz_max": mhz[-1] if mhz else None,
        "mhz_mean": round(sum(mhz) / len(mhz), 1) if mhz else None,
        # A worst-pair statistic pinned by ONE idle core: it describes the quietest core on the box,
        # not the cores this arm ran on. Recorded as provenance, never as a verdict.
        "mhz_spread": round(mhz[-1] / mhz[0], 3) if mhz and mhz[0] > 0 else None,
        "cores": len(mhz),
    }


def self_drift(values: list[float]) -> float | None:
    """Ratio of the two observations of one harness -- that harness's own A/A null."""
    usable = [v for v in values if v and v > 0]
    if len(usable) < 2:
        return None
    return max(usable) / min(usable)


def parse_criterion_ns(stdout: str) -> float | None:
    """Pull the median estimate out of criterion's human output, in nanoseconds.

    Criterion prints e.g. `time:   [1.2345 ms 1.2400 ms 1.2460 ms]`; the middle value is the point
    estimate. Units are normalised here rather than assumed, because a bench that crosses a
    magnitude boundary silently changes them.
    """
    match = re.search(
        r"time:\s*\[\s*[\d.]+\s+\w+\s+([\d.]+)\s+(ns|[uµ]s|ms|s)\s+[\d.]+\s+\w+\s*\]", stdout
    )
    if not match:
        return None
    value = float(match.group(1))
    scale = {"ns": 1.0, "us": 1e3, "µs": 1e3, "ms": 1e6, "s": 1e9}[match.group(2)]
    return value * scale


def parse_perf_instructions(stderr: str) -> float | None:
    """Retired instructions from `perf stat`. Separators are stripped; locale commas are not assumed."""
    match = re.search(r"^\s*([\d,.]+)\s+instructions", stderr, re.MULTILINE)
    if not match:
        return None
    # Retired instructions are integral, so every `,` and `.` in the field is a thousands separator
    # -- which locale uses which does not matter, and stripping both is unambiguous. An earlier
    # version tried to keep a decimal point and mangled the dot-separated form `1.234.567` into
    # 1234.567, silently under-reporting by three orders of magnitude.
    return float(re.sub(r"[,.]", "", match.group(1)))


def report(rows: list[dict]) -> int:
    """Print every harness side by side and name the disagreement. Never picks, never averages."""
    print("=== HARNESS CROSS-CHECK, one primitive, one machine, one session (bd-8bbq) ===")
    for row in rows:
        print(
            f"harness={row['harness']:<12} unit={row['unit']:<16} value={row['value']}"
            f" drift={row.get('drift')}"
        )
        print(f"    conditions: {row['conditions']}")

    time_rows = [r for r in rows if r["unit"] == "ns_per_op" and r["value"]]

    for row in rows:
        drift = row.get("drift")
        if drift is not None and drift > MAX_SELF_DRIFT:
            print(
                f"\nREFUSING TO COMPARE: harness={row['harness']} drifted {drift:.4f}x between two"
                f" runs of itself, over the {MAX_SELF_DRIFT}x ceiling."
            )
            print("  A harness that cannot reproduce itself cannot be cross-checked against another.")
            return 2

    if len(time_rows) < 2:
        print("\nINCOMPLETE: fewer than two time-based harnesses reported, so there is nothing to cross-check.")
        return 2

    a, b = time_rows[0], time_rows[1]
    ratio = max(a["value"], b["value"]) / min(a["value"], b["value"])
    print(f"\nTIME-HARNESS DISAGREEMENT: {a['harness']} vs {b['harness']} = {ratio:.4f}x")
    if ratio > 1.10:
        print("  THE DISAGREEMENT IS THE FINDING. Bank BOTH numbers with their harness named.")
        print("  Do not pick one, do not average them, and do not retire the loser: a ratio this")
        print("  size also invalidates comparing any existing row from one against a row from the other.")
    else:
        print("  The two time harnesses agree within 10% on this primitive.")
        print("  That is evidence for THIS primitive only -- agreement here does not license")
        print("  comparing rows on workloads with a different profile.")

    for row in rows:
        if row["unit"] == "instructions_per_op":
            print(
                f"\nNOT CROSS-CHECKED: harness={row['harness']} reports {row['value']}"
                " instructions/op."
            )
            print("  Instructions are not a time. This number is comparable only against another")
            print("  INSTRUCTION measurement, so it is reported as provenance, not divided into the above.")

    return 0


def self_test() -> int:
    """Check the parsing and the verdict logic without a host, a build, or a harness."""
    failures = 0

    cases = [
        ("time:   [1.2345 ms 1.2400 ms 1.2460 ms]", 1_240_000.0),
        ("time:   [980.12 ns 984.55 ns 990.01 ns]", 984.55),
        ("time:   [1.0 s 2.0 s 3.0 s]", 2e9),
        ("no timing here", None),
    ]
    for text, expected in cases:
        got = parse_criterion_ns(text)
        if expected is None:
            ok = got is None
        else:
            ok = got is not None and abs(got - expected) < 1e-6
        print(("  ok   " if ok else "  FAIL ") + f"criterion parse {text[:34]!r} -> {got}")
        failures += 0 if ok else 1

    for text, expected in [
        ("   12,345,678      instructions:u            #    1.20  insn per cycle", 12345678.0),
        # Dot-separated locale: the form that the first version of this parser mangled.
        ("   1.234.567      instructions            #    0.98  insn per cycle", 1234567.0),
        ("   4096      instructions", 4096.0),
        ("   no counter here", None),
    ]:
        got = parse_perf_instructions(text)
        ok = got == expected
        print(("  ok   " if ok else "  FAIL ") + f"perf parse {text.strip()[:26]!r} -> {got}")
        failures += 0 if ok else 1

    ok = self_drift([100.0, 172.0]) is not None and abs(self_drift([100.0, 172.0]) - 1.72) < 1e-9
    print(("  ok   " if ok else "  FAIL ") + "drift of a pathological pair is 1.72x")
    failures += 0 if ok else 1

    ok = self_drift([100.0]) is None
    print(("  ok   " if ok else "  FAIL ") + "a single observation has no drift")
    failures += 0 if ok else 1

    # A drifting harness must block the comparison, not be quietly excluded from it.
    conditions = {"loadavg": [0.0, 0.0, 0.0]}
    rows = [
        {"harness": "run.mjs", "unit": "ns_per_op", "value": 1000.0, "drift": 1.72, "conditions": conditions},
        {"harness": "criterion", "unit": "ns_per_op", "value": 1010.0, "drift": 1.01, "conditions": conditions},
    ]
    ok = report(rows) == 2
    print(("  ok   " if ok else "  FAIL ") + "a self-drifting harness refuses the cross-check")
    failures += 0 if ok else 1

    # Two disagreeing but self-consistent harnesses must REPORT, not refuse and not choose.
    rows = [
        {"harness": "run.mjs", "unit": "ns_per_op", "value": 1000.0, "drift": 1.02, "conditions": conditions},
        {"harness": "criterion", "unit": "ns_per_op", "value": 2400.0, "drift": 1.03, "conditions": conditions},
        {"harness": "perf-stat", "unit": "instructions_per_op", "value": 5_000.0, "drift": 1.0, "conditions": conditions},
    ]
    ok = report(rows) == 0
    print(("  ok   " if ok else "  FAIL ") + "a 2.4x disagreement is reported rather than resolved")
    failures += 0 if ok else 1

    print("self-test PASSED" if not failures else f"self-test FAILED ({failures})")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--self-test", action="store_true", help="check parsing and verdict logic; no host needed")
    parser.add_argument("--case", default="flowchart_large_500", help="corpus case id for run.mjs")
    parser.add_argument("--bench", default="crossing_minimization", help="criterion bench name")
    parser.add_argument("--fm-bin", help="content-pinned h2h binary for the run.mjs arm")
    parser.add_argument("--emit-json", help="write the collected rows here for the ledger")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    print(
        "NOT IMPLEMENTED: the three measurement arms are not wired up.\n"
        "\n"
        "This is deliberate rather than an oversight. Each arm has to run its harness twice for an\n"
        "A/A null, on a quiescent host, and the invocation details -- build flags, pinning policy,\n"
        "and which binary is content-pinned -- are exactly what this project has repeatedly gotten\n"
        "wrong when written from memory instead of from a live run. The parsing, the unit rules and\n"
        "the verdict logic ARE implemented and self-tested; wiring the subprocess calls is the part\n"
        "that must be done with a host in front of you.\n",
        file=sys.stderr,
    )
    return 3


if __name__ == "__main__":
    raise SystemExit(main())
