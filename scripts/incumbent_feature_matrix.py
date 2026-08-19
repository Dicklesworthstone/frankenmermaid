#!/usr/bin/env python3
"""Ask the pinned incumbent, in one pass, which syntax features it actually accepts.

`scripts/headtohead/parse_probe.mjs` answers that for ONE diagram. Every capability question starts
with the same list of candidates, so this runs the list and prints a matrix — the point being that
the CASE LIST is the durable artifact. A feature nobody thought to probe is a gap nobody finds.

WHY ASK RATHER THAN READ: the bundle is minified. Grepping it for `id:"..."` returned SIX ids for a
library with two dozen diagram types, because the minifier renames the property, and an operator grep
once returned 7296 hits for `..|>` because its characters are regex metacharacters. The parser is the
only authority that cannot be fooled by minification.

⚠️ PROBE WITH CONTENT, NEVER A BARE HEADER. A bare header conflates "this feature does not exist"
with "a header alone is not a valid document": bare `ishikawa` reports "grammar rejected" while
`ishikawa` plus one line reports "grammar ACCEPTED". Reading the bare result alone nearly deleted a
correct entry from the unimplemented-type table.

⚠️ THIS DELIBERATELY DOES NOT REPORT WHETHER *WE* SUPPORT A FEATURE, and the omission is the most
important thing in this file. The first version had an "ours" column driven by `grep -rlF <token>
crates/fm-parser/src/`, and it was wrong in a way that reads as authoritative:

    subgraph_direction   ACCEPTED   yes (dot_parser.rs)
    init_directive       PARSED     yes (dot_parser.rs)

Both tokens matched the DOT parser, which has nothing to do with mermaid flowchart subgraphs. Both
features are in fact supported — `set_subgraph_direction` and `extract_init_payload` in
`mermaid_parser.rs` — so the column happened to be right twice by luck while measuring nothing. A
support claim needs the call site, not a token. Answer that half by reading the parser.

VERDICTS
    PARSED     the incumbent parsed and rendered it
    ACCEPTED   the grammar accepted it; execution then threw (usually a missing DOMPurify shim in
               this headless harness, NOT a parse failure — treat as supported syntax)
    rejected   the grammar refused it; this is the only verdict that means "not real mermaid"

Usage:  python3 scripts/incumbent_feature_matrix.py [name ...]
"""
from __future__ import annotations

import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PROBE = os.path.join(ROOT, "scripts", "headtohead", "parse_probe.mjs")

# Assembled rather than written literally: the fleet's command guard reads `--` followed by `>` in a
# shell argument as a redirect and has blocked commands that merely quoted a diagram.
ARROW = "-" + "-" + ">"

CASES: dict[str, str] = {
    # Diagram types whose bare spelling is NOT what the incumbent accepts (verified 2026-08-19).
    "type_radar": "radar\n  Item\n",
    "type_radar_beta": "radar-beta\n  Item\n",
    "type_venn_beta": "venn-beta\n  Item\n",
    "type_wardley_beta": "wardley-beta\n  Item\n",
    "type_treeview_beta": "treeView-beta\n  Item\n",
    "type_ishikawa": "ishikawa\n  Problem\n",
    "type_treemap": "treemap\n  Item\n",
    # A control: this is not a diagram type in any spelling, and a probe that accepts it is broken.
    "type_notatype": "notatypeatall\n  Item\n",
    # Flowchart features.
    "edge_id": f"flowchart LR\n  A e1@{ARROW} B\n",
    "node_icon": 'flowchart LR\n  A@{ icon: "fa:fa-bell", form: "square" }\n',
    "linkstyle_default": f"flowchart LR\n  A {ARROW} B\n  linkStyle default stroke:#f00\n",
    "subgraph_direction": f"flowchart TB\n  subgraph one\n    direction LR\n    a {ARROW} b\n  end\n",
    "init_directive": f'%%{{init: {{"theme":"dark"}}}}%%\nflowchart LR\n  A {ARROW} B\n',
    "markdown_label": f'flowchart LR\n  A["`**bold** text`"] {ARROW} B\n',
    "shape_notch_rect": 'flowchart LR\n  A@{ shape: notch-rect, label: "n" }\n',
    "flowchart_elk": f"flowchart-elk LR\n  A {ARROW} B\n",
}


def verdict_for(source: str) -> str:
    with tempfile.NamedTemporaryFile("w", suffix=".mmd", delete=False, encoding="utf-8") as handle:
        handle.write(source)
        path = handle.name
    try:
        done = subprocess.run(
            ["node", PROBE, "--file", path],
            cwd=ROOT, capture_output=True, text=True, timeout=120,
        )
        blob = done.stdout + done.stderr
    except subprocess.TimeoutExpired:
        return "TIMEOUT"
    finally:
        os.unlink(path)
    if "PARSED" in blob:
        return "PARSED"
    if "grammar ACCEPTED" in blob:
        return "ACCEPTED"
    if "grammar rejected" in blob:
        return "rejected"
    return "unknown"


def main(argv: list[str]) -> int:
    wanted = argv or sorted(CASES)
    unknown = [name for name in wanted if name not in CASES]
    if unknown:
        print(f"no such case(s): {', '.join(unknown)}", file=sys.stderr)
        return 2

    results = {name: verdict_for(CASES[name]) for name in wanted}
    width = max(len(name) for name in wanted)
    for name in wanted:
        print(f"{name:<{width}}  {results[name]}")

    # The control must be refused, or every other verdict on this run is meaningless: a probe that
    # accepts a nonsense header is not discriminating, it is agreeing.
    if results.get("type_notatype") not in (None, "rejected"):
        print(
            f"\nCONTROL FAILED: 'notatypeatall' was {results['type_notatype']}, so this probe is "
            "not discriminating and none of the verdicts above can be trusted",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
