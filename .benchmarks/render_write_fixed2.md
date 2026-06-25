# Perf win: fast fixed-2-decimal SVG number formatting (write_fixed2)

**Crate:** `fm-render-svg` · **Date:** 2026-06-24 · **Agent:** frankenmermaid-cc
**Verdict:** kept — reproducible 7–13% render speedup (p<0.05), byte-identical output.

## What changed

SVG serialization formats every fractional coordinate with `write!(f, "{n:.2}")` —
the general float→decimal machinery (Grisu/Dragon) — at two hot sites:
`AttributeValue::Display` (rect/text/etc. numeric attrs) and `path.rs` `FmtNum`
(every point in a path `d` string). A probe that replaced the fractional format with
an integer cast showed this is **~11% of render (medium), ~14% (large)**.

Replaced both with `write_fixed2()`: promote the `f32` to `f64` (lossless), scale by
100, round ties-to-even, and emit `int.frac` directly — skipping the float-decimal
algorithm. Non-finite / out-of-i64-range inputs fall back to `{:.2}`.

## Correctness — proven byte-identical

A differential test (`write_fixed2_byte_identical_to_std_format`) asserts
`write_fixed2(v) == format!("{v:.2}")` over a **dense 6M-value sweep** of the
coordinate range (both signs) plus rounding-tie / large-magnitude edge cases — it
passes, so `round_ties_even` reproduces `{:.2}`'s rounding exactly. All
**214 fm-render-svg tests pass** (snapshot/render tests unchanged) → SVG output is
byte-for-byte identical, conformance GREEN.

## Measurement — same-worker A/B (stash-swap, measurement-time 4)

| `render_svg/flowchart` | opt (write_fixed2) faster by | p |
|------------------------|------------------------------|---|
| small_10  | **+7.0%** | <0.05 ✓ |
| medium_100| **+7.3%** | <0.05 ✓ |
| large_500 | **+13.4%** | <0.05 ✓ |

(A first run at measurement-time 2 was noisy on the render_svg group; the longer run
is clean and consistent, matching the probe's ~11–14% float-cost ceiling.)

Render is the largest pipeline stage for large diagrams, so this lands on the
dominant cost — the second confirmed render-stage lever after the parse fast-path.
The earlier render-Cow lever was ~0 because allocation isn't the bottleneck; the
**CPU cost of float formatting is**, and it's eliminable without changing output.
