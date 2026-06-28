# Render output reduction — MEASURED breakdown (corrects the estimate); the `accessible` flag is NOT the lever

**Date:** 2026-06-28 — **Agent:** BlackThrush — **HEAD:** 791cb8d
Empirically measured by rendering `gen_wide(16,32)` and byte-counting the output (load-independent).

## Correction to 791cb8d

I assumed a config flag gated the a11y output. **It does not.** `SvgRenderConfig.accessible`
(`true` by default) toggles only **339 bytes (0.1%)** of top-level a11y — flipping it off changes
nothing meaningful and time is unaffected (±0%). The per-element a11y is emitted **unconditionally**;
dropping it needs a **code change** in `render_node`/`render_edge`, not a config flip.

## Measured breakdown (16x32, total output = 405,387 bytes)

| output | bytes | occ | % of output | nature |
|---|---:|---:|---:|---|
| node `<title>Node: …, shape</title>` | 19,520 | 513 | 4.8% | our descriptive a11y convention |
| node `role="graphics-symbol"` | 11,776 | 512 | 2.9% | a11y |
| node `aria-label="…"` | 10,272 | 512 | 2.5% | a11y |
| node `tabindex="0"` | 6,656 | 512 | 1.6% | a11y |
| `data-id="…"` (nodes) | 8,224 | 512 | 2.0% | derivable from `id` |
| `data-fm-edge-id="N"` (edges) | 21,010 | 960 | 5.2% | derivable from `id="fm-edge-N"` |

a11y ≈ **48 KB (~12%)**, `data-*` ≈ **29 KB (~7%)** → together ~**77 KB ≈ 19% of output**. Since render
is byte-writing-bound, the full reduction is ~11-14% render ≈ **~7-8% of the whole pipeline** — by far
the biggest remaining win. (Note: in `gen_wide` only nodes carry the a11y set; unlabeled edges don't
emit `<title>`/`role` — a labelled-edge corpus would have an even larger a11y share.)

## Two separable opportunities, both contract-gated (not mine to land)

1. **a11y (title/role/aria-label/tabindex, ~12%)** — needs cod-b's Mermaid comparator. Strong prior our
   `<title>Node: X, rectangle</title>` is non-Mermaid (Mermaid renders the label as text). If Mermaid
   omits these, gate them off in render_node/render_edge (fidelity fix + perf) + regen golden_svg
   (also clears the RED first-edge-title-drop 90446ae).
2. **`data-*` (~7%, `data-id` + `data-fm-edge-id`)** — provably redundant with `id`, BUT
   `frankentui_conformance_cases.json` asserts on `data-*` (e.g. `svg_not_contains: ["data-callback="]`),
   so FrankenTUI likely consumes `data-*` as interactivity hooks → a downstream-contract decision, not
   pure dead weight. Confirm with the FrankenTUI consumer before dropping.

## Net

The wide pipeline's parse + layout + cyclic are at the constraint-bound byte-identical floor; render's
micro-levers are floored; the only ≥3% win left is this ~7-8% output reduction, which is a code change
behind a contract/comparator decision — escalated with exact numbers, not a unilateral byte-identical
lever.
