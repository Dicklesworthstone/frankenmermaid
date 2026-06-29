# Negative: simplify the edge fast-path 13-char bracket-check to a single `[` scan — ~0, REVERTED

**Crate:** `fm-parser` — **Date:** 2026-06-29 — **Agent:** BlackThrush
**Verdict:** byte-identical but ~0 (mechanistically) + unconfirmable under fleet noise. Do not retry.

## The lever (sound redundancy)

`parse_fast_simple_flowchart_edge_parts` runs, on **every** line (1472 at 16x32), an upfront
`trimmed.bytes().any(|b| matches!(b, b'[' | b']' | b'(' | b')' | b'{' | b'}' | b'"' | b'\'' | b'`' |
b'|' | b'&' | b':' | b','))`. Every one of those 13 chars is a **non-identifier** char, so any
statement containing one is *also* rejected by `is_fast_flow_identifier(left)/(right)` after the
operator split (or by the no-operator path). The check's only unique job is cheaply early-rejecting
the common node line (which contains `[`). So it's reducible to `trimmed.as_bytes().contains(&b'[')`
— **byte-identical** (verified: 405 fm-parser tests pass; every other special char still fails the
id-check or the operator check).

## Why ~0 (do not retry)

`matches!(b, <13 byte literals>)` compiles to a **~1-op/byte bitmask/range check**, not 13 sequential
compares — so reducing the set to a single byte does **not** reduce the per-byte cost; the cost is the
byte *scan* of each line, which is unchanged. Both forms are ~1 op/byte over the same bytes.
`<[u8]>::contains` is also a scalar `iter().any`, no better than the (likely auto-vectorized) original.

## Measurement

Both-order `parse_bench` A/B at fleet load ~13: **contradictory / noise-dominated** — ORDER_A showed
flowchart/medium_100 −24%, large_1000 −18% (p=0.00) but ORDER_B disagreed and wide/12x24 went +14%;
the runs drift faster each phase (warm-up), and a null A/B at this load is ±7-14% (d121ee1, 191935f).
No direction-consistent signal above the floor. Combined with the ~0 mechanism ⇒ not a win. Reverted
per REVERT-~0-gain. (If ever revisited: the redundancy is real, but only a quiet-fleet A/B could
detect a sub-bitmask difference, and there is no per-byte work to remove.)
