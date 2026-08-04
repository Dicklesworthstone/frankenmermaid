# Perf win: pre-size the Attributes Vec (render +6–14%)

**Crate:** `fm-render-svg` · **Date:** 2026-06-25 · **Agent:** frankenmermaid-cc
**Verdict:** kept — render +5.6% (small) / +6.1% (medium) / **+13.8% (large)**, output-identical.

## Localizing the cost first

A no-op-the-serialization probe (`finish_layout_svg_document` returns `String::new()`)
split `render_layout_to_svg` into construction vs serialization:

| bench | serialization share | construction share |
|-------|---------------------|--------------------|
| `render_svg/small_10`   | ~52% | ~48% |
| `render_svg/medium_100` | ~44% | ~56% |
| `render_svg/large_500`  | ~36% | **~64%** |

So after the earlier float/escape/`write_into` serialization wins, **element
construction is now the dominant render cost** (~64% at large). Each `Element` builds
its attribute list by pushing onto a fresh `Vec` via chained setters; rect/text/path
carry ~8–12 attributes, so the `Vec` grows `0→4→8→16` — ~3 reallocs + element copies
per element, ×~1500 elements for large_500.

## What changed

`Attributes::new()` now allocates `Vec::with_capacity(12)` instead of `Vec::new()`,
sized to a typical element's attribute count. No realloc/copy churn while building.
Pure capacity change — zero behavioral difference.

## Correctness

All **fm-render-svg tests pass** (snapshots included) — byte-identical output.
Conformance GREEN; clippy clean.

## Measurement — same-worker A/B (stash-swap, measurement-time 3)

| `render_svg/flowchart` | pre-size faster by | p |
|------------------------|--------------------|---|
| large_500  | **+13.8%** | <0.05 |
| medium_100 | **+6.1%** | <0.05 |
| small_10   | **+5.6%** | <0.05 |

Unlike the earlier render-Cow lever (per-attribute *name* allocation, ~0) and
`write_into` (per-element buffer, ~0 at large), the per-element attribute-`Vec` realloc
churn is a real, size-scaling cost — largest at large_500 where construction is ~64% of
render. The biggest single render win since the escape/float optimizations.

## TanSparrow BOLD-VERIFY follow-up

This commit existed locally without the required mermaid.js ledger, so TanSparrow
reran a same-machine A/B and a fresh browser comparator before push.

Baseline worktree:
`/data/projects/.worktrees/frankenmermaid-cod-a-attrs-baseline-9a61d4c`
at `9a61d4c`.

Candidate checkout:
`/data/projects/frankenmermaid` at local commit `8ba0aba`.

Command shape:
`CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a cargo bench -p frankenmermaid-cli --bench pipeline_bench -- <bench> --warm-up-time 1 --measurement-time <N>`.

Render-stage A/B (`measurement-time 2`):

| `render_svg/flowchart` | baseline mean | candidate mean | candidate faster |
|------------------------|--------------:|---------------:|-----------------:|
| small_10   | 152.43 us | 151.71 us | 0.47% |
| medium_100 | 900.91 us | 914.00 us | -1.45% |
| large_500  | 4.6577 ms | 4.2747 ms | 8.22% |

Wide full-pipeline A/B (`measurement-time 4`):

| `full_pipeline_wide/parse_layout_svg` | baseline mean | candidate mean | candidate faster |
|---------------------------------------|--------------:|---------------:|-----------------:|
| 8x16  | 2.3034 ms | 2.2771 ms | 1.14% |
| 12x24 | 5.5680 ms | 5.3180 ms | 4.49% |
| 16x32 | 10.497 ms | 10.553 ms | -0.53% |

The `16x32` interval overlapped and Criterion reported no change, so this is not
treated as a regression.

Fresh Mermaid.js comparator:
Node `v24.14.0` drove `/snap/bin/chromium` through Chrome DevTools Protocol and
dynamically imported `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`.
Each wide case used 3 warmups and 20 timed render-to-SVG iterations.

| case | frankenmermaid mean | Mermaid.js mean | fm / Mermaid.js | Mermaid.js slower |
|------|--------------------:|----------------:|----------------:|------------------:|
| 8x16  | 2.2771 ms | 370.7550 ms | 0.006142x | 162.82x |
| 12x24 | 5.3180 ms | 1122.6200 ms | 0.004737x | 211.10x |
| 16x32 | 10.553 ms | 2917.1900 ms | 0.003618x | 276.43x |
