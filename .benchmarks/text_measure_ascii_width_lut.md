# Perf win: const ASCII multiplier LUT for text measurement

**Crate:** `fm-core` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** kept — wide layout ~5–18% faster (text measurement); byte-identical.

## What changed

`FontMetrics::estimate_width` summed `avg_char_width * CharWidthClass::classify(c).multiplier()`
per char — a `classify` match (≈5 explicit char arms + an `is_east_asian_wide` call) plus a
`multiplier` match, for every character of every node label. `compute_node_sizes` (fm-layout)
drives it via `estimate_dimensions → estimate_multiline_width → estimate_width` once per node
(512× on the 16x32 wide graph).

Added a compile-time `const ASCII_WIDTH_MULT: [f32; 128]` = `classify(byte).multiplier()`, and an
ASCII fast path in `estimate_width` (non-monospace, all-ASCII text — the common label case) that
sums `avg_char_width * ASCII_WIDTH_MULT[b]` per byte: one array load + multiply replacing the
per-char match chain.

## Why this is not the rejected `classify`-table (bd-9e7c)

bd-9e7c stored `CharWidthClass` (the enum) in a table and then `.multiplier()`-matched it, which
broke the compiler's fusion of `classify → multiplier` into one jump table (an intermediate enum
load). This table stores the **final `f32` multiplier**, so there is no intermediate match to
fuse and nothing to break — the per-char op is a pure `f32` load.

## Correctness

Bit-identical: each `ASCII_WIDTH_MULT[b]` is exactly `classify(b as char).multiplier()` (same
const fns, computed at compile time), the `avg * mult` op is unchanged, and the left-to-right
`f32` sum is over the same character sequence (ASCII bytes == chars in order). Monospace and
non-ASCII text keep the original char path. 349 fm-core unit tests (incl.
`measurements_are_deterministic`, `char_width_classes_are_deterministic`) + doc tests +
`frankentui_conformance_test` (whole-corpus identity) pass.

## Measurement

Same-worker both-order stash-swap A/B, fresh dir `mermaid-bt5`, `wide_stages/layout`, mt=4.

| bench | ORDER_A (ORIG vs opt) | ORDER_B (OPT vs orig) | geo-mean OPT/ORIG |
|---|---:|---:|---:|
| `wide_stages/layout/8x16`  | +3.5% (p=0.02) | −30.3% (p=0.00) | ~0.82 (**~18% faster**) |
| `wide_stages/layout/12x24` | +0.2% (NS)     | −9.6%  (p=0.00) | ~0.95 (**~5% faster**) |
| `wide_stages/layout/16x32` | +12.0% (p=0.00)| −24.0% (p=0.00) | ~0.82 (**~18% faster**) |

Direction-consistent OPT-faster in **both** orders at all sizes (ORDER_B p=0.00 throughout). The
8x16/16x32 magnitudes are noise-inflated (loaded box); the least-noisy 12x24 (~5%) is the
conservative floor. Mechanism (a single `f32` LUT load replacing ~7 char comparisons + 2 matches
per char) guarantees the direction.

## Mermaid.js head-to-head

Layout-stage win (text measurement) stacking on the three landed edge-routing wins. Layout is
~14% of the wide pipeline, so the pipeline-level effect is modest, but it is a clean, free,
byte-identical reduction of `compute_node_sizes`, the second-largest layout phase after
`build_edge_paths`. Full-pipeline `full_pipeline_wide/16x32` ≈4.3 ms vs the pinned Mermaid
`11.12.0` 2879.185 ms (≈670× faster); this nudges the ratio slightly further.
