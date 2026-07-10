# WIN: accent index via digit-table, not `write!` — flowchart render up to 4.8×%, byte-identical

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** `fa06b1d` · **Files:** `fm-render-svg/src/{lib.rs,attributes.rs}`
(render lane — cod on parser/edge-routing). **Verdict: KEEP.**

## Profile-first

`perf --call-graph=dwarf` on mindmap render put `<core::fmt::Formatter>::pad_integral::write_prefix` at
**1.87%** inside `write_common_node_fragment_into` — the std `Formatter` machinery for an integer, not the fast
digit-table path. The integer is the **accent palette index**, written per node as `fm-node-accent-{accent}`
via `write!(f, "{accent}")`.

## Root cause + prior art

The ledger already established this exact anti-pattern as a large win — *"`<i32 as Display>::fmt` 4.66% +
`pad_integral` 3.85% = ~8.5% of render... the full `fmt::Formatter` + `pad_integral` machinery"* — and fixed it
for **coordinates** (digit-table, `e79a7bd`). But the **accent index** write was never covered: two
`write!(f, "{accent}")` sites in `write_common_node_fragment_into` (the common rect/circle/mindmap path) and
`write_subroutine_node_fragment_into`, each hit once per node.

## The lever (one)

Make `attributes::write_uint_into` (the existing digit-table `u64` writer) `pub(crate)`, and replace both
`write!(<buf>, "{accent}")` with `write_uint_into(<buf>, accent as u64)`. Removes the per-node `format_args!` +
`Formatter` + `pad_integral` chain in favour of a `DIGITS1`/`PAIRS2` table lookup.

## Byte-identical

`write_uint_into` emits the decimal digits of `accent as u64` identically to `write!("{accent}")` for every
value (including 0 → `"0"`); `accent as u64` from `usize` is exact. Proven:
- `node_fast_fragment_matches_render` — the exact-byte pin comparing the streamed node fragment to the slow
  `Element` render — **passes** (directly covers the `fm-node-accent-N` bytes).
- `cargo test -p fm-render-svg --lib` **247 passed**.
- `golden_svg_test`: only the known pre-existing `gantt_basic` FNV mismatch (gantt renders bars, not
  common-node accent classes — confirmed pre-existing at untouched HEAD repeatedly this session).
- `cargo fmt --check` clean; `ubs` 14→14 (unchanged).

## Measurement — same-worker A/B, gate on median

`render_svg/flowchart` (nodes carry an accent, so each `bench_with_input` size exercises the accent write per
node), cand vs base (`git show HEAD:{lib,attributes}.rs > …`), same worker `ovh-a`:

| bench | cand p50 | base p50 | ratio | CIs |
|---|---:|---:|---:|---|
| `small_10` (10 nodes) | 47.713 µs | 48.099 µs | 0.992× (0.8%) | overlap |
| `medium_100` (100 nodes) | 86.602 µs | 89.250 µs | **0.970× (3.0%)** | non-overlapping |
| `large_500` (500 nodes) | 149.77 µs | 157.34 µs | **0.952× (4.8%)** | non-overlapping |

**The effect scales cleanly with node count** (0.8% → 3.0% → 4.8%), which is the signature of a real per-node
mechanism, not noise — noise does not rise monotonically with N. CIs are non-overlapping on the two decision
rows, well above the ~1% median-CI floor calibrated earlier. (A confirmatory second pair landed on a slow/loaded
worker with a 40%-wide CI and timed out — discarded; the same-worker ovh-a pair + the scaling is the read.)

## Scope

Helps every diagram whose nodes take the common-node streaming path (flowchart, mindmap, class, state, ER,
sequence actors) — one fewer `Formatter` setup per node. Larger diagrams benefit more (4.8% at 500 nodes). The
byte-production floor is otherwise mature; this was the one remaining `write!`-Formatter instance the coordinate
digit-table win missed.
