# Perf win: write SVG attributes directly into the output buffer

**Crate:** `fm-render-svg` · **Date:** 2026-06-24 · **Agent:** frankenmermaid-cc
**Verdict:** kept — +7% (medium) / +15% (small) render, output byte-identical.

## What changed

`Element::write_to_string` did `output.push_str(&self.attrs.render())` — `render()`
allocated a fresh `String` per element, wrote the attributes into it, returned it, and
the caller then copied it into `output`. Added `Attributes::write_into(&mut W)` that
writes the attributes straight into the output buffer; `render()` now delegates to it,
and `write_to_string` calls `write_into` directly — no per-element `String` allocation
or extra copy.

## Correctness

Trivially output-identical (the exact same bytes are written, just to the destination
buffer instead of a temporary). All **215 fm-render-svg tests pass** (snapshots
included). Conformance GREEN; clippy clean.

## Measurement — same-worker A/B (stash-swap, measurement-time 4)

| `render_svg/flowchart` | write_into faster by | p |
|------------------------|----------------------|---|
| small_10   | **+15.1%** | <0.05 |
| medium_100 | **+7.1%** | <0.05 |
| large_500  | +0.4% (CI ±7%, inconclusive) | 0.91 (n.s.) |

The per-element `String` allocation is a larger relative share for small/medium
diagrams; for large_500 the float/escape/`fmt` work (already optimized) dominates and
the run was noisy, so the effect there is inconclusive (not a regression). Kept on the
≥3% medium_100 win with no regression elsewhere. Unlike the earlier render-Cow lever
(per-attribute *name* allocation, ~0), removing the per-*element* buffer + copy is a
measurable cut on small/medium.
