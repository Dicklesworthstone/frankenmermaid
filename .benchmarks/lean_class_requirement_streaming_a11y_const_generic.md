# WIN: lean profile now streams class + requirement compartment nodes (bd-1dj4)

**Date:** 2026-07-16 · **Agent:** BlackThrush · **File scope:** `crates/fm-render-svg/src/lib.rs`
(`write_class_node_fragment_into`, `write_requirement_node_fragment_into`, their two `render_node_into`
gates). Final child of the bd-b2b6 lean family (node half `2288c78`, edge halves, this).

## The gap

`render_node_into`'s class-compartment and requirement-node fast paths were gated on full a11y
(`aria_labels && keyboard_nav && text_alternatives`). Under the lean profile (`A11yConfig::none()`),
class/requirement diagrams fell entirely to the per-element `Element` slow path (group + rect + one
`Element` per compartment/attribute row + title). bd-b2b6 also left a **+0.31% class_50 lean regression**:
lean class nodes failed the class gate, fell to the common gate (relaxed to `uniform_a11y` by the node
half), and did wasted work there before failing at the class-meta check.

## The lever

`write_class_node_fragment_into` and `write_requirement_node_fragment_into` gained a **const generic
`A11Y: bool`** (mirroring the node/edge halves). `true` emits `role="graphics-symbol" aria-label=".."
tabindex="0"` + the trailing `<title>`; `false` skips exactly those two spots — the `<g id/class/data-id>`
wrapper, `<rect>`, and the compartment/subtitle rows are a11y-independent, so the lean fragment equals the
slow path's lean output **by construction**.

### Two gates, not one relaxed gate (default-regression avoidance)

A single `uniform_a11y(&config.a11y)` gate + runtime dispatch measured a **+0.37% default-`class`
regression** (holding the a11y bool live / a runtime branch across the per-node gate; even the node-half
function-pointer idiom only cut it to +0.37%). Instead each node type keeps **two** a11y-uniform gates: the
full-a11y gate is byte-for-byte as before (direct `::<true>`, no `uniform_a11y` in the hot default path),
and a second gate handles a11y-off (`::<false>`). `A11yConfig::minimal()` (mixed) matches neither → slow
path, as before. This makes streaming lean class nodes here *also* fix the +0.31% class_50 regression (they
no longer reach the common gate).

## Measured (non-LTO release opt=3, profharness `render`, 100 nodes, `perf stat -e instructions:u`)

| shape | LEAN instr | LEAN wall | DEFAULT instr |
|-------|-----------|-----------|---------------|
| requirement | **0.352x** (−64.8%) | **~0.263x** (−73.7%) | 0.9992x (neutral) |
| class | **0.624x** (−37.6%) | ~0.52–0.54x (−46–48%) | 1.0004x (neutral) |

Wall≫instr because the slow path's per-compartment/attribute `Element` allocations are eliminated.
DEFAULT profile is **neutral** for both (the two-gate keeps the full-a11y path unchanged).

## Byte-identity

SHA-256 of full SVG dump matched baseline (HEAD build) across class/classbad/classcard/requirement/er/
sankey/flowchart/state/seq/gantt/mindmap under **both** profiles (default a11y-full AND lean a11y-none).
256 fm-render-svg tests + clippy `-D` green.

C4-node compartment fast path (`write_c4_node_fragment_into`) is the same shape but out of this bead's
scope — left full-a11y-gated (C4 diagrams still slow-path under lean); a natural follow-up.
