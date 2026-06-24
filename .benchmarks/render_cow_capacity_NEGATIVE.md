# NEGATIVE result: render-svg zero-alloc attr names (Cow) + pre-sized buffer

**Crate:** `fm-render-svg` · **Date:** 2026-06-24 · **Agent:** frankenmermaid-cc
**Verdict:** ~0 gain (<2%, below the ≥3% keep threshold) → **do not land as a perf win.**

## The lever (collaborative swarm work-in-progress, uncommitted)

Two complementary, output-identical changes were in the working tree:
1. `Attributes.name: String` → `Cow<'static, str>` so static attribute names
   (`"x"`, `"width"`, `"stroke-width"`, …) stop heap-allocating per attribute
   (thousands of tiny allocs per render avoided).
2. `SvgDocument::to_string_with_capacity(hint)` + a layout-derived capacity hint,
   to pre-size the final SVG buffer and avoid growth copies.

Both compile and are correctness-clean: **`cargo test -p fm-render-svg` → 213 passed, 0 failed**
(incl. `capacity_hint_preserves_serialization` asserting byte-identical output).

## Measurement — clean same-machine A/B (this is the reusable part)

rch scatters runs across workers (ovh-a / hz2 / vmi1152480 …) whose absolute speed
differs ~1.3–2×, and **criterion `--save-baseline`/`--baseline` data does NOT travel
between workers** (panics: "Baseline 'render_orig' must exist…"). So cross-rch-run
comparison of sub-15% effects is invalid. Method that works:

1. Build the bench binary for each side via rch `--no-run` into *separate*
   `CARGO_TARGET_DIR`s (orig from a detached worktree at HEAD; opt from the working tree).
   rch syncs the 7.7 MB `pipeline_bench-*` binary back locally.
2. Run **both binaries directly on csd, back-to-back** (binary execution is not a compile
   command, so the rch hook doesn't offload it) → same machine, seconds apart.

| `render_svg/flowchart` | ORIG (String) | OPT (Cow+capacity) | change |
|------------------------|---------------|--------------------|--------|
| small_10  | 280.25 µs | 277.22 µs | −1.1% |
| medium_100| 1.4277 ms | 1.4299 ms | +0.2% |
| large_500 | 6.5446 ms | 6.5934 ms | +0.7% |

CIs were tight and overlapping (e.g. large: orig [6.49, 6.61] ms vs opt [6.55, 6.65] ms);
a ≥3% win could not hide there. Binaries verified distinct (md5 differ).

## Why it's ~0

Per-attribute name allocations and buffer regrowth are **not** the render bottleneck.
The dominant render cost is the per-attribute `Display` formatting (`write!` for every
attr), `escape_xml_*` char-by-char scanning, the O(k²) `Attributes::set` `retain` dedup,
and element-tree traversal/concatenation — none of which this lever touches. A future
render lever should target those (e.g. skip `retain` when names are known-unique, or
serialize numbers/escapes without per-attribute intermediate work).

## Disposition

Not committed (no measured win). The code is collaborative WIP authored mostly by another
agent across `element.rs`/`document.rs`/`lib.rs`; left untouched for its author to revert
or repurpose. This file records the measured negative result so the swarm does not land it
as a "win" off misleading cross-worker numbers.
