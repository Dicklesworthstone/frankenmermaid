#!/usr/bin/env python3
"""Score A/A null clause 3 over REPEATS instead of one observation (bd-w91d).

WHAT CLAUSE 3 DOES, AND WHY ONE OBSERVATION IS NOT ENOUGH
---------------------------------------------------------
The corrected A/A null gate refuses a row whose null MEDIAN is more than 2% from 1.0. Scoring the
stored artifact history showed that statistic moving across repeats of a single configuration by
more than the threshold itself: of 60 configurations with three or more stored repeats, 26 pass
every time, ONE fails every time, and **30 straddle the line** -- their verdict flips between runs
that differ in nothing.

A rule applied to one draw of a statistic that needs several will refuse rows for being unlucky. The
bead's own caveat says as much, from three runs of sequence_20 moving 0.299% -> 1.289% -> 1.990%.

WHAT THIS SCRIPT IS, AND IS NOT
-------------------------------
It REPORTS. It does not gate, and it does not certify: adopting a k-of-n rule is a gate change, and
by this project's doctrine a gate change needs its own evidence rather than riding along on a
scoring tool. Nothing here writes to the ledger, and no row is declared requalified by running it.

What it does give is the number a requalification decision actually needs: for each configuration,
how many of its stored repeats exceed the threshold, so "this row was refused" can be distinguished
from "this configuration exceeds the threshold consistently".

GROUPING IS STRICT ON PURPOSE
-----------------------------
Rows are keyed by (case, engine, elf, threads). A looser key pools thread widths and binaries that
are not the same run and inflates the apparent spread -- my first pass at this analysis grouped by
case alone and overstated it. Rows that cannot state their elf or thread width are reported
SEPARATELY rather than silently pooled, because a row that cannot say which binary produced it
cannot be requalified from the archive at all.
"""

from __future__ import annotations

import argparse
import glob
import json
import os
from collections import defaultdict

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_ROOT = os.path.join(REPO, ".benchmarks", "headtohead")

# Clause 3's own threshold, in percent. Not a knob to tune: it is the published rule, and this
# script exists to report against it, not to move it.
CLAUSE3_PCT = 2.0


def bias_percent(median: float) -> float:
    """Clause 3's statistic: distance of the null median from 1.0, in percent."""
    return abs(median - 1.0) * 100.0


def collect(root: str) -> tuple[dict, int]:
    """Group stored null medians by strict configuration key."""
    groups: dict[tuple, list[float]] = defaultdict(list)
    unprovenanced = 0

    for path in glob.glob(os.path.join(root, "**", "*.jsonl"), recursive=True):
        with open(path, encoding="utf-8") as handle:
            for line in handle:
                if '"null_control"' not in line:
                    continue
                try:
                    row = json.loads(line)
                except ValueError:
                    continue
                control = row.get("null_control")
                if not isinstance(control, dict):
                    continue
                median = control.get("median")
                if not isinstance(median, (int, float)):
                    continue

                elf = row.get("elf_sha256")
                threads = row.get("thread_count_requested")
                if elf is None or threads is None:
                    unprovenanced += 1

                key = (
                    str(row.get("id")),
                    str(row.get("engine")),
                    str(elf)[:12],
                    str(threads),
                )
                groups[key].append(bias_percent(float(median)))

    return groups, unprovenanced


def score(groups: dict, min_repeats: int) -> dict:
    """Split configurations into always-pass, always-fail and straddling."""
    always_pass, always_fail, straddle = [], [], []
    for key, values in groups.items():
        if len(values) < min_repeats:
            continue
        exceed = sum(1 for v in values if v >= CLAUSE3_PCT)
        entry = (key, values, exceed)
        if exceed == 0:
            always_pass.append(entry)
        elif exceed == len(values):
            always_fail.append(entry)
        else:
            straddle.append(entry)
    return {"always_pass": always_pass, "always_fail": always_fail, "straddle": straddle}


def report(result: dict, unprovenanced: int, min_repeats: int) -> int:
    total = sum(len(v) for v in result.values())
    print(f"=== clause 3 scored over repeats (>= {min_repeats} per configuration) ===")
    print(f"configurations scored: {total}")
    print(f"  always below {CLAUSE3_PCT}%: {len(result['always_pass'])}")
    print(f"  always at or above:  {len(result['always_fail'])}")
    print(f"  STRADDLING:          {len(result['straddle'])}")

    if result["always_fail"]:
        print("\nCONSISTENTLY EXCEEDS -- a real effect, not a draw:")
        for (case, engine, elf, threads), values, exceed in sorted(result["always_fail"]):
            print(
                f"  {case} / {engine} / elf={elf} / thr={threads}: "
                f"{exceed}/{len(values)} repeats, range {min(values):.3f}-{max(values):.3f}%"
            )

    if result["straddle"]:
        print("\nSTRADDLES -- a single observation cannot decide these:")
        for (case, engine, elf, threads), values, exceed in sorted(
            result["straddle"], key=lambda e: -(max(e[1]) - min(e[1]))
        )[:15]:
            print(
                f"  {case} / {engine} / thr={threads}: {exceed}/{len(values)} exceed, "
                f"range {min(values):.3f}-{max(values):.3f}%"
            )

    if unprovenanced:
        print(
            f"\n{unprovenanced} row(s) lack elf_sha256 or thread_count_requested. Those cannot be "
            "grouped by binary or width, so their configurations pool runs that are not the same "
            "run -- treat their ranges as upper bounds. run.mjs now stamps both (583d1c3b); rows "
            "stored before that cannot be recovered."
        )

    print(
        "\nThis REPORTS; it does not certify. Adopting a k-of-n rule is a gate change and needs its "
        "own evidence -- nothing here requalifies a row, and no row should be quoted because it "
        "appeared in this output."
    )
    return 0


def self_test() -> int:
    failures = 0

    checks = [
        (bias_percent(1.0), 0.0, "a perfect null has zero bias"),
        (bias_percent(1.02), 2.0, "2% above"),
        (bias_percent(0.98), 2.0, "2% below is the SAME distance -- the rule is two-sided"),
    ]
    for got, want, why in checks:
        ok = abs(got - want) < 1e-9
        print(("  ok   " if ok else "  FAIL ") + f"{why}: {got}")
        failures += 0 if ok else 1

    groups = {
        ("clean", "fm", "aaa", "1"): [0.1, 0.2, 0.3],
        ("biased", "fm", "aaa", "1"): [4.0, 5.0, 6.0],
        ("straddling", "fm", "aaa", "1"): [0.1, 3.0, 0.2],
        ("too_few", "fm", "aaa", "1"): [9.9, 9.9],
    }
    result = score(groups, 3)
    cases = [
        (len(result["always_pass"]), 1, "one always-pass"),
        (len(result["always_fail"]), 1, "one always-fail"),
        (len(result["straddle"]), 1, "one straddling"),
    ]
    for got, want, why in cases:
        ok = got == want
        print(("  ok   " if ok else "  FAIL ") + f"{why}: {got}")
        failures += 0 if ok else 1

    # A configuration below the repeat floor must be EXCLUDED, not scored as if it were decisive.
    # Scoring a 2-repeat group is exactly the single-observation mistake this script exists to avoid.
    scored = {k for group in result.values() for (k, _, _) in group}
    ok = ("too_few", "fm", "aaa", "1") not in scored
    print(("  ok   " if ok else "  FAIL ") + "a configuration below the repeat floor is excluded")
    failures += 0 if ok else 1

    print("self-test PASSED" if not failures else f"self-test FAILED ({failures})")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--self-test", action="store_true", help="check the scoring logic; reads nothing")
    parser.add_argument("--root", default=DEFAULT_ROOT, help="artifact root to scan")
    parser.add_argument("--min-repeats", type=int, default=3, help="repeats required to score a configuration")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.min_repeats < 3:
        raise SystemExit("--min-repeats must be at least 3; fewer cannot distinguish a draw from an effect")

    groups, unprovenanced = collect(args.root)
    return report(score(groups, args.min_repeats), unprovenanced, args.min_repeats)


if __name__ == "__main__":
    raise SystemExit(main())
