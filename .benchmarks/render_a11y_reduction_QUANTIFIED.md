# Render a11y output reduction — QUANTIFIED (~9-11% pipeline) + feasibility resolved as far as possible

**Date:** 2026-06-28 — **Agent:** BlackThrush — **HEAD:** 9433d28
**Status:** the single biggest remaining win; blocked only on a one-shot Mermaid-comparator check (cod-b).

## Why this is the biggest remaining win

Render is ~60% of the wide pipeline and byte-writing-bound. Each node emits, by default:
`role="graphics-symbol" aria-label="…" tabindex="0"` + `<title>Node: …, rectangle</title>` ≈ **90
bytes/node**; each edge ≈ 70 bytes (`role`/`tabindex`/`<title>…</title>`). Measured byte share in the
goldens: `all_node_shapes.svg` (15 nodes) = 1352 a11y bytes / 15387 total = **8.8%**, but the CSS/defs
block is fixed overhead — for **16x32 (512 nodes + 960 edges)** the per-element a11y is ≈ 46 KB + 67 KB
≈ **113 KB of ~600 KB output ≈ 19% of bytes** → since render is byte-writing-bound, **~15-19% render ≈
~9-11% of the whole pipeline.** Larger than any single micro-lever this session.

## Feasibility: no existing test resolves it — the comparator is definitively required

- `frankentui_conformance_test` is **FrankenTUI-fixture-based** (`svg_contains` / `svg_not_contains` +
  parse counts; source_refs point at `ftui-extras/src/mermaid.rs`), **not** a Mermaid byte-comparison.
- `golden_svg_test` compares against **our own** snapshots.
- So neither tells us whether Mermaid `11.12.0` emits per-node `<title>`/`role`/`aria-label`/`tabindex`.

## Strong prior that it is droppable (not Mermaid-faithful)

Our `<title>Node: Rectangle, rectangle</title>` is a **descriptive a11y convention of ours**
(`"Node: {label}, {shape}"`). Mermaid renders the node's *label* as text and does not emit a
`"Node: X, shape"` title per node. So this title (≈38 B/node) is almost certainly extra output Mermaid
does not produce. `role="graphics-symbol"`/`tabindex="0"` per element are likewise our additions to
verify.

## Action (cod-b — owns the Mermaid 11.12.0 Node+Chromium+CDP comparator)

One-shot check: render a single `A[Foo] --> B[Bar]` in Mermaid and grep the node/edge `<g>` for
`<title>`, `role`, `aria-label`, `tabindex`.
- **If Mermaid omits them** → gate them off-by-default (a config already exists for some). This is BOTH
  a fidelity fix (match Mermaid) AND the biggest render perf win (~9-11% pipeline). Regen `golden_svg`
  in the same commit — which also clears the currently-RED `golden_svg_test` (the first-edge `<title>`
  drop, 90446ae, is the swarm already fumbling toward this; make it a clean, uniform decision).
- **If Mermaid emits them** → they stay (byte-identity), render is at its true floor, and the swarm
  should stop chasing render output reduction.

This is a contract decision + one comparator run, not a byte-identical micro-lever — which is why it is
escalated here rather than landed. Everything else on the wide pipeline (parse, layout-tree, cyclic) is
at its constraint-bound byte-identical floor (zero-dep / forbid-unsafe / byte-identical cap the rest).
