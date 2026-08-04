# WIN: lean profile now streams the LABELED edge fragment too (bd-u63b)

**Date:** 2026-07-16 · **Agent:** BlackThrush · **File scope:** `crates/fm-render-svg/src/lib.rs`
(labeled-edge fragment writer + `render_edge_into` gate only). Follow-up to bd-b2b6 node half (`2288c78`)
and the unlabeled-edge half (`write_common_edge_full_fragment_into`).

## The gap

`render_edge_into`'s whole-LABELED-edge fast fragment (`<g><path/><rect/><text/><title/></g>`) was gated on
`config.a11y.aria_labels && keyboard_nav && text_alternatives`. Under the lean profile (`A11yConfig::none()`)
every labeled edge fell back to the per-element `Element` builder (group + path + rect + text + title,
~5 allocs/edge). Label-heavy diagrams (sequence / ER / sankey) are dominated by labeled edges, so they got
**zero** benefit from the earlier unlabeled-edge lever (bd-6u9o measured sequence_20/er_40 lean at 1.0003x).

## The lever

`write_labeled_edge_fragment_into` gained a **const generic** `A11Y: bool` (mirroring the node half and
`write_common_edge_full_fragment_into`). `A11Y = true` emits `role="graphics-symbol" tabindex="0"` + the
trailing `<title>`; `A11Y = false` skips exactly those two spots — everything else (`<g id/class/
data-fm-edge-id>` wrapper, `<path>`, `<rect>`, `<text>`) is a11y-independent, so the lean fragment is the
slow `Element` path's lean output **by construction**. The `render_edge_into` gate relaxed from the
three-flag `&&` to `uniform_a11y(&config.a11y)` and dispatches `::<true>`/`::<false>`. The full-a11y-only
caller inside `render_edge` (raw_svg fast path) is pinned to `::<true>`.

### Regression avoided (a cheap label-presence pre-check)

Relaxing the gate to `uniform_a11y` first regressed lean flowchart/state **+1.0%**: under lean the old
`aria_labels` flag short-circuited the block immediately, but `uniform_a11y` passes, so unlabeled Arrow edges
now reached `compute_edge_label` (called, returns `None`, discarded). Added a cheap
`detail.show_edge_labels && ir_edge.is_some_and(|e| e.label.is_some())` pre-check ahead of
`uniform_a11y`/`compute_edge_label`. This short-circuits unlabeled edges early — a necessary condition for
`compute_edge_label` to return `Some`, byte-identical, and it also **speeds default flowchart −1.5%** (the
default path used to compute+discard the label for unlabeled edges too).

## Measured (non-LTO release opt=3, profharness `render`, 200 nodes, `perf stat -e instructions:u`)

| shape | LEAN instr ratio | LEAN wall ratio |
|-------|------------------|-----------------|
| sankey | **0.395x** (−60.5%) | ~0.35x (−65%) |
| er | **0.441x** (−55.9%) | ~0.37–0.45x |
| seq | **0.826x** (−17.4%) | ~0.82x (−18%) |
| flowchart | 0.9994x (neutral) | — |
| state | 0.9994x (neutral) | — |
| class | 0.9996x (neutral) | — |
| gantt | 1.0000x (neutral) | — |

DEFAULT profile: flowchart **0.985x** (−1.5%, pre-check bonus), seq 1.0002x, er 1.0011x — all
**byte-identical** (default path behavior unchanged).

Wall≫instr on the big wins because the slow `Element` path's ~5 heap allocs/labeled-edge are eliminated.

## Byte-identity

SHA-256 of full SVG dump matched baseline (HEAD build) across seq/er/sankey/flowchart/state/class/gantt under
**both** profiles (default a11y-full AND lean a11y-none). 256 fm-render-svg tests + clippy `-D` green.
Mixed a11y (`A11yConfig::minimal()`) still takes the slow `Element` path, exactly as before.
