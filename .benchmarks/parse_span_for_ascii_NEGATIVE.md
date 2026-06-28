# Negative: `span_for` ASCII fast-path — ~0 / regression, REVERTED

**Crate:** `fm-parser` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** ~0 gain / slight regression — reverted (uncommitted), do not retry.

## Lever

`span_for(line_number, line) = Span::at_line(line_number, line.chars().count())` runs once per
parsed statement (~1472× on the 16x32 wide graph), so it looked like a hot spot where the
`chars().count()` UTF-8 work could be skipped for ASCII lines:

```rust
let char_count = if line.is_ascii() { line.len() } else { line.chars().count() };
```

Hypothesis: the same `trim_ascii` / `is_east_asian_wide` ASCII fast-path pattern that won
elsewhere — `is_ascii()` is auto-vectorizable and `len()` is O(1), vs a per-`char` decode.

## Measurement

Same-worker both-order stash-swap A/B, `cargo bench -p fm-parser --bench parse_bench`,
fresh dir `mermaid-bt3`, criterion mt=4. Byte-identical (405 fm-parser tests pass).

- `parse/flowchart/large_1000` (most stable, longest bench): **OPT slower in BOTH orders** —
  ORDER_A (ORIG vs opt) −7.7% (ORIG faster), ORDER_B (OPT vs orig) +29% (OPT slower), both
  p<0.05. Consistent direction = a real ~0/regression, not noise.
- `parse/wide/{8x16,12x24,16x32}`: sign-flipped between orders (e.g. 12x24 ORDER_A +3.5% vs
  ORDER_B +18.9%) = pure noise under the heavily-loaded box (local load ~37; the swarm also
  loads the remote workers).

## Why it does not pay (do-not-retry note)

`str::chars().count()` is **not** a per-char Unicode decode worth bypassing here — counting is
already a byte-boundary operation, so adding an `is_ascii()` pre-scan is *extra* work, not a
saving (an extra O(n) pass on top of the count the parser already does). Same class as the
reverted `write_int` / `classify`-table levers: the obvious "skip the Unicode path" assumption
is wrong because the std/compiler path is already byte-cheap. `span_for`'s char-count is at its
floor; the only way to cut it is to eliminate the count entirely (store byte-len), which was
separately tried and rejected (`source_line` refactor, ~0, breaks char-accurate non-ASCII
spans).

## Standing (why this turn dug parse at all)

After the three landed wide-layout wins (3d81eca dense-index, 2c09a38 pair-tracker, 9e19e51
CSR), layout is the smallest wide stage (~375 µs); render (~1.8 ms) is byte-identical-floored
(edge smoothing already has the n==2 straight fast-path + capacity sizing; node/edge streaming
+ auto-vectorized escape are done); parse (~0.82 ms) is the next-biggest measurable gap but its
fast-path (borrowed `FastEdge`, FxHashMap interning, pre-sized IR vecs) is harvested — `span_for`
was the last plausible per-statement lever and it does not pay. The byte-identical incremental
frontier for the wide flowchart pipeline is effectively reached; further gains need design/output
changes (e.g. render `data-*`/`<title>` emission) rather than byte-identical micro-levers.
