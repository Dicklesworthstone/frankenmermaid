#!/usr/bin/env python3
"""Which `fm-` marker classes does fm-render-svg emit that NO golden pins?

WHY THIS EXISTS. Every cross-renderer defect found on 2026-08-20 (bd-4n5j2) sat on a marker class
with no golden coverage: state note text placement, the state note leader, the cluster divider, the
destroy marker, the mirror header. The one divergence that could be settled OUTRIGHT -- the gantt
axis, where the canvas put labels 24 units low and drew no tick marks -- was the one WITH a golden,
because `gantt_basic.svg` pinned the SVG side and made the disagreement decidable rather than a
matter of taste.

So an uncovered class is not merely untested. It is a place where two renderers can disagree and
nothing in the repo can say which is right.

TWO REASONS A CLASS CAN BE ABSENT, and they call for different work:

  CONFIG   the goldens are rendered with visual effects deliberately OFF (golden_svg_test.rs:3368),
           so effect-gated classes can never appear in one. Not a fixture gap; do not chase.
  FIXTURE  no `.mmd` in the golden corpus exercises the feature at all. This is the real hole.

Run:  python3 scripts/golden_class_coverage.py
Exit: 0 always -- this reports, it does not gate. Wiring a gate needs a decision about which of the
      FIXTURE rows are deliberate, which is a judgement this script must not make silently.
"""
from __future__ import annotations

import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
SVG_SRC = REPO / "crates/fm-render-svg/src/lib.rs"
GOLDEN_DIR = REPO / "crates/fm-cli/tests/golden"

# Absent because of the golden CONFIG, not a missing fixture. Effects are pinned off, so these
# cannot appear however many diagrams are added.
CONFIG_GATED = {
    "fm-animations-enabled",
    "fm-edge-flow-animated",
}


def emitted_classes(source: str) -> list[str]:
    """Every `fm-` class the SVG arm writes, from both the builder and raw-string spellings."""
    sites = re.findall(r'\.class\("fm-[a-z0-9-]+"\)|class=\\"fm-[a-z0-9-]+', source)
    return sorted(set(re.findall(r"fm-[a-z0-9-]+", " ".join(sites))))


def covered_by(class_name: str, goldens: dict[str, str]) -> list[str]:
    """Goldens carrying this class as a whole token.

    A WHOLE TOKEN, not a substring: `fm-node` is a prefix of `fm-node-inactive`, and a substring
    test would report the more specific class as covered by any file containing the general one.
    """
    pattern = re.compile(r'class="[^"]*\b' + re.escape(class_name) + r"\b")
    return sorted(name for name, text in goldens.items() if pattern.search(text))


def main() -> int:
    if not SVG_SRC.is_file():
        print(f"cannot read {SVG_SRC}", file=sys.stderr)
        return 0
    classes = emitted_classes(SVG_SRC.read_text(encoding="utf-8"))
    goldens = {
        path.name: path.read_text(encoding="utf-8")
        for path in sorted(GOLDEN_DIR.glob("*.svg"))
    }
    if not classes or not goldens:
        print("NOTHING SCANNED -- the class scan or the golden corpus moved; this is not a pass")
        return 0

    uncovered = [c for c in classes if not covered_by(c, goldens)]
    config = [c for c in uncovered if c in CONFIG_GATED]
    fixture = [c for c in uncovered if c not in CONFIG_GATED]

    print(f"{len(classes)} marker classes emitted, {len(goldens)} goldens")
    print(f"  covered      {len(classes) - len(uncovered)}")
    print(f"  config-gated {len(config)}  (effects off in goldens; not a fixture gap)")
    print(f"  NO FIXTURE   {len(fixture)}")
    print()
    for name in fixture:
        print(f"  {name}")
    stale = sorted(CONFIG_GATED - set(classes))
    if stale:
        print()
        print("CONFIG_GATED names no longer emitted (delete them, an entry naming nothing is a hole):")
        for name in stale:
            print(f"  {name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
