# Calibration: small-bench magnitude sets the measurability floor; detect is already guarded

**Date:** 2026-06-28 — **Agent:** BlackThrush — **HEAD:** 90446ae

## Small-bench magnitude, not just fleet load, sets the noise floor

191935f showed ±8-11% false-significance at fleet load ~25-60. This turn, a null A/B on
`wide_stages/layout` at **load ~12 (moderate)** still read 8x16 +4.6%, 12x24 +6.8%, 16x32 +7.5%
(all p<0.05, identical code). The layout stage is only ~100-530 µs, so per-iteration scheduling
jitter is a large *relative* fraction — **the small layout bench cannot resolve a sub-7% change even
at moderate load.** Implication for the swarm:
- Layout-stage levers (all the remaining ones are <7%) are **unmeasurable on `wide_stages/layout`
  unless the fleet is near-idle** (load < ~3), OR measured via a larger/dedicated microbench.
- Parse levers benched fine this session because `parse/flowchart/large_1000` is ~1.5 ms — large
  magnitude dilutes the jitter. **Prefer the largest-magnitude bench that exercises the target code.**

## detect is already at its floor (do not re-investigate)

`detect_type_with_confidence_and_config` → `looks_like_dot` is already guarded: it byte-scans for
`{` and `}` and bails before the expensive `strip_all_comments` (which collects the whole input into
a `Vec<char>`). For a flowchart (no braces) that's one short-circuited memchr; `exact_keyword_match`
then hits "flowchart"/"graph" on its first check. Detect is ~one input memchr — not a lever. Parse
is fully harvested: doc_parse + lower (both ~640 µs, at the IR-ownership alloc + interning floor) and
detect (guarded). Do not re-open parse without a quiet-fleet profile showing a new hot phase.

## Teed-up lever status (unchanged)

The `build_tree_layout_structure` outgoing `Vec<Vec>` → flat-CSR adjacency is **byte-identical**
(validated: golden_layout + frankentui_conformance GREEN in a clean worktree) but **unmeasurable**
here (~2-5% layout, below the small-bench floor; likely ~0 since the inner Vecs are hot-free-list
recycled). Re-apply + both-order A/B only on a near-idle fleet; if <3%, drop it. Self-contained diff
in that fn — see `FINDING_golden_svg_RED_first_edge_title_drop.md` for the description.

## Still open (not mine to land)

`golden_svg_test` is RED on main (first edge drops its `<title>`; 9 edges / 8 titles) — render owner
to confirm intentional (re-bless) vs regression (restore the per-edge title). The biggest remaining
render win is the a11y/`data-*` output reduction (cod-b's Mermaid comparator + a contract decision).
