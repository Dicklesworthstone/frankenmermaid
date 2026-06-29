# MEASURED: edge a11y output is ~19% of render — the single biggest render lever (a contract decision)

**Crate:** `fm-render-svg` — **Date:** 2026-06-29 — **Agent:** BlackThrush — **HEAD:** b573a47
**Measured on hw1 (cmake-healthy). This quantifies the golden_svg RED root cause as the biggest render
opportunity + states the exact owner decision. The +19% direction is NOT mine to force unilaterally.**

## The measurement

I implemented the correct fix for the edge fast-path regression (ba81c3d/b573a47): made
`build_common_edge_fragment` emit the full `<g id class data-fm-edge-id role tabindex><path/><title>…
</title></g>` that the slow path produces for unlabelled a11y edges, gated on the full-a11y default,
and **fixed the tautological pin test** to compare against the real group construction. **Verified
byte-identical: pin test + 226 fm-render-svg tests pass.** Then both-order `wide_stages/render/16x32`
A/B on hw1: the correct (a11y) output is **−19.1% (p=0.00) vs the buggy bare path** — i.e. the edge
a11y `<g>`/`role`/`tabindex`/`<title>` on ~960 edges is **~19% of render** (~11% of the whole pipeline).

That is *larger* than the node a11y (~12% of output, b97e1a8). So per-element a11y (nodes + edges) is
the dominant removable chunk of render — and it clears the render noise floor (±3-10%) decisively.

## The bare fast path is the swarm's inconsistent, un-finished output reduction

`render_edge`'s fast path emits bare edges (no a11y) "for perf"; the slow path AND the committed
golden (`dense_flowchart_stress.svg`: 25 `<g id="fm-edge-N" … role tabindex>`, 0 bare) still have the
full a11y. So the current state is internally inconsistent (fast bare / slow+golden a11y) — that is
the 7-turn golden_svg RED. The "Output is byte-identical" comment (lib.rs:6214) is simply wrong.

## The owner decision (either direction is implementable + I've verified the structure)

1. **Complete the reduction (−19% render WIN):** drop the a11y `<g>`/`<title>` from the SLOW path too
   (uniform bare edges) + re-bless the goldens. Requires **cod-b's Mermaid comparator** to confirm
   Mermaid omits per-edge `<title>`/`role`/`aria` (our `<title>` is the `describe_edge` convention,
   likely non-Mermaid). This is the biggest measurable render win on the board.
2. **Restore correctness (+19%, matches the current golden):** make the fast path emit the full a11y
   group (the fix I implemented + verified byte-identical — pin test fixed to be non-tautological).

I reverted my +19% "fix" because forcing a 19% render regression is the contract decision, not a
unilateral call — but the verified fragment + the corrected pin test are ready to re-apply for
direction (2), and direction (1) is the ~19% win once the comparator confirms it. golden_svg also has
stale reduced-CSS drift needing an owner re-bless regardless.
