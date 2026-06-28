# FINDING: golden_svg_test is RED on main — first edge drops its `<g>`/`<title>` (a11y asymmetry)

**Date:** 2026-06-28 — **Agent:** BlackThrush — **HEAD:** 191935f
**Severity:** a byte-identity guard is RED on committed main; likely a render regression masked by a stale golden.

## What's red

`cargo test -p frankenmermaid-cli --test golden_svg_test` → `svg_golden_snapshots_are_stable`
**FAILED**: "FNV hash mismatch for case `dense_flowchart_stress`". Reproduced on a **clean worktree at
191935f** (not the peer's uncommitted render WIP, not my change), so it is genuinely on committed main.
`frankentui_conformance_test` and `golden_layout_test` are GREEN — only the SVG golden snapshot is stale.

## Root cause

Recent render byte-reduction perf wins changed SVG output: `733b2a8` (font-family → root `<svg>`),
`022f5f2` (drop `data-fm-node-id`), and 5× `…gate redundant … bytes` (edge `fill="none"`, base
strokes, drop-shadow). `c5be819` re-blessed 14 flowchart goldens but drift remains. `BLESS=1` on
clean HEAD regenerates **37 goldens (207+/795−)** — most diffs are the legit byte-reductions (removed
redundant CSS: `.fm-edge-labeled > rect`, the glassmorphism `@supports backdrop-filter`, `.edge-label`).

## The suspicious bit (needs a render owner)

The **first/common edge renders as a bare `<path>` with NO `<g>` wrapper and NO `<title>`.** In
`all_edge_types.svg` (current output): **9 edges (`data-fm-edge-id` 0–8) but only 8 edge `<title>`s** —
`fm-edge-0`'s title ("Start points to Arrow") is dropped; it emits `</defs><path d="M195.35 638 …"/>`
straight into edge-1's `<g>`. Edges 1–8 keep `<g …><path/><title>…</title></g>`.

This looks like a direct-stream/common-edge fast path that omits the wrapper **and the accessible
title** for the first edge only — an a11y/fidelity asymmetry Mermaid would not have.

## Action (render owner — the peer on render, or cod-b)

1. Decide: is the first-edge bare-`<path>` (no `<title>`) **intentional**? 
   - If yes → re-bless all stale goldens (`BLESS=1 cargo test -p frankenmermaid-cli --test golden_svg_test`)
     so the guard is GREEN again, and confirm the dropped `<title>` is acceptable for a11y.
   - If no → it's a regression: the common-edge fast path must still emit the per-edge `<title>`
     (and likely the `<g>` wrapper) like edges 1–8. Then re-bless.
2. Either way, the golden_svg guard must return to GREEN — it is currently masking this on every CI run.

I am **not** blindly re-blessing 37 goldens: that would bake in the title drop if it is a regression.

## Separately: a byte-identical layout lever is teed up

`build_tree_layout_structure` outgoing adjacency `Vec<Vec<usize>>` → flat CSR (offsets/flat + per-node
deduped length), avoiding ~512 inner Vec allocations. **Byte-identical** — validated in the clean
worktree: `golden_layout_test` (the layout-output guard this code feeds) and `frankentui_conformance_test`
both GREEN, and `golden_svg` failed *identically* to baseline (my change altered nothing). Estimated
~2–5% layout (~1% pipeline), uncertain (the allocs are hot-free-list recycled). **Deferred:** the fleet
noise floor is ±8–11% with false p<0.05 (191935f), so it is unmeasurable now — re-measure + land on a
quiet fleet. The diff is self-contained in `build_tree_layout_structure`.
