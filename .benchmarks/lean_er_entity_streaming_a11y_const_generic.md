# WIN: lean profile now streams ER-entity nodes — last shape off the fast path (bd-b2b6.1)

**Date:** 2026-07-16 · **Agent:** BlackThrush · **File scope:** `crates/fm-render-svg/src/lib.rs`
(`write_er_node_fragment_into` + its `render_node_into` gate). Completes the bd-b2b6 lean family
(node `2288c78`, edges, class/requirement `3553b04b`, ER here).

## Data-driven target

A lean/default render-instruction ratio scan across 17 shapes found **erattr** (ER entities WITH attribute
lists) was the **only** shape still >1.0 — **1.540x** (lean 54% slower than default). Every other shape was
≤0.94 (streaming). `write_er_node_fragment_into`'s fast path in `render_node_into` was full-a11y-gated
(`aria_labels && keyboard_nav && text_alternatives`), so under lean ER entities fell to the ~1-Element-
per-attribute slow `Element` path.

## The lever (identical to bd-1dj4)

`write_er_node_fragment_into` gained a **const generic `A11Y: bool`**: `true` emits `role`/`aria-label`/
`tabindex` + the trailing `<title>`; `false` skips exactly those two spots (the `<g id/class/data-id>`
wrapper, `<rect>`, and the entity attribute body via `write_er_entity_into` are a11y-independent, so the
lean fragment equals the slow path's lean output by construction). The `render_node_into` ER path uses **two
a11y-uniform gates**: the full-a11y gate is unchanged (direct `::<true>`, no default regression), a second
gate handles a11y-off (`::<false>`). `A11yConfig::minimal()` matches neither → slow path.

## Measured (non-LTO release opt=3, profharness `render`, 60 entities, `perf stat -e instructions:u`)

| shape | LEAN instr | LEAN wall | DEFAULT instr | lean/default (after) |
|-------|-----------|-----------|---------------|----------------------|
| erattr | **0.599x** (−40.1%) | **~0.48x** (−51%) | 1.0000x (neutral) | **0.922** (was 1.540) |

erattr's lean/default ratio dropped from **1.540 → 0.922**, so **every corpus shape now streams under lean**
— bd-b2b6's "lean_slowdown ≤ 1.0 on every item" finish line is met.

## Byte-identity

SHA-256 of full SVG dump matched baseline (HEAD `3553b04b`) across er/erattr/class/requirement/flowchart/
state/sankey/seq/gantt/mindmap/classcard under **both** profiles. 256 fm-render-svg tests (incl.
`er_entity_node_streaming_matches_slow_render`) + clippy `-D` green.

Remaining bd-b2b6 family piece: C4 node (`write_c4_node_fragment_into`) — same shape, but C4 diagrams did
not appear >1.0 in the scan (they fall through to other streamed paths), so lower priority.
