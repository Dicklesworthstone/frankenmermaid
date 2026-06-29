# BLOCKER: render perf (the biggest gap) is fully gated — micro-levers sub-noise, the big lever owner+conformance-gated

**Date:** 2026-06-29 — **Agent:** BlackThrush — **HEAD:** fb1e977
**This is a blocker surface, not re-verification: it names the single prerequisite to unblock the
biggest remaining win + states I have a healthy worker ready to land it.**

## Render is the biggest gap (~60% of pipeline) but has no actionable lever for me

1. **Micro-levers are unmeasurable.** Null A/B on `wide_stages/render` on a cmake-**healthy** worker
   (hw1) reads ±3-6% (8x16/12x24) and +9.8% false-sig (16x32) from identical code — load-drift-bound,
   not magnitude-bound. So **no render change <~10% is confirmable on this fleet** (write_int itoa
   fb1e977 was byte-identical + mechanistically a win but sub-noise → reverted). Construction is
   already direct-byte (peer's node fragment 66ff940; the edge fast path too).

2. **The only ≥10% render lever is the full a11y+data-* output reduction** (~19% of output ≈ ~12%
   render; `render_a11y_data_reduction_MEASURED.md`). It is **double-blocked**:
   - **Owner contract decision** — is per-element `<title>`/`role`/`aria-label` (~12%, needs cod-b's
     Mermaid comparator) / `data-*` (~7%, functionally unconsumed but a public-attr decision) a
     committed API, or droppable for Mermaid-parity?
   - **golden_svg RED, 7 turns** — `BLESS=1` would regen 37 goldens; I cannot cleanly regen for a new
     output change without re-blessing the owner's intentional byte-reduction drifts. **And** the edge
     fast path itself diverges (root-caused ba81c3d, **verified this turn**: render_edge:6401 wraps
     unlabeled edges in `<g>`+`<title>` when `a11y.text_alternatives` is on; the bare fast path drops
     it — fix is a direct-byte g+title fragment, the peer's node-fragment pattern, OR gate on
     `!a11y.text_alternatives` which regresses the ~40%-of-render fast path).

## Prerequisite to unblock (single owner action) → then I land the ~12% win

1. Owner re-bless golden_svg (the intentional byte-reduction drifts) + fix the edge fast path per
   ba81c3d → golden_svg GREEN.
2. Owner decides the output-reduction contract (a11y via comparator; data-* via API call).

Then the ~12% render reduction is a clean, measurable, mergeable win — and **hw1 is a confirmed
cmake-healthy worker I can build+bench on right now** (the highs-sys outage is intermittent per-worker;
probe `cargo build -p fm-layout` across fresh dirs to find one).

## Everything else is at the byte-identical floor

parse (doc_parse + lower + detect, harvested + FxHashMap + alloc-free, fb1e977/41498f3), layout-tree
(edge_paths CSR, tree+spans CSR ~0, node_sizes LUT, CSR dropped 2a77afc), cyclic (dead, 9433d28). The
reliably-measurable lane is parse (large_1000, fm-parser, highs-sys-free) and it has no lever left.
