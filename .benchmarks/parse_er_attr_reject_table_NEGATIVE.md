# REJECT: table-ize the 4-byte reject scan in parse_simple_er_attribute (2026-07-24)

Agent: CopperCliff (cc). Base: `dd0ec0f5`. Lane: bd-1buv.

## Lever

`parse_simple_er_attribute`'s early-reject scan `line.as_bytes().iter().any(|&byte| matches!(byte,
b':' | b'"' | b'\'' | b'`'))` (a full-line scan per ER attribute line) was replaced with a
`[bool;256]` `ER_ATTR_REJECT` table (`line.bytes().any(|b| ER_ATTR_REJECT[b as usize])`), the same
pattern as the winning `FAST_ID_CHAR` / `FAST_EDGE_REJECT` tables. Byte-identical (416/416 tests).

## Measured — flat-to-slight-regression (interleaved one-binary A/B, load ~17)

`er_stages/parse/256`: CAND (table) ~153.3µs vs ORIG (matches!) ~151.8µs — **CAND ~1% SLOWER**,
distributions overlapping (ORIG's fastest run 148.5µs < all CAND). No win.

## Why (the table-lookup crossover)

The table beats a predicate only when the predicate is LONG. `FAST_ID_CHAR` replaced
`is_ascii_alphanumeric() || matches!(_ - . /)` ≈ **7 comparisons/byte** and won ~1-2%. This ER scan
replaces only a **4-byte `matches!`** — which the compiler lowers to a few compares that pipeline
well, so the table's single L1 load (with ~4-cycle latency) does NOT beat it. The crossover for the
`[bool;256]` table lever sits around a ~5-7-comparison predicate; below that it washes or regresses.

## Verdict: REJECT. Reverted to `dd0ec0f5`.

## Do Not Retry

Do not table-ize byte-set `.any()/.all()` scans whose predicate is ≤4 bytes / ≤~4 comparisons — the
compiler's compare sequence pipelines better than an L1 table load. Reserve `[bool;256]` tables for
≥~7-comparison predicates (alphanumeric ranges + a set), as in `FAST_ID_CHAR` / the ER validators.
For 2-byte sets use `memchr2` (won on `has_special`); for ≤4-byte sets on short haystacks, leave the
`matches!` predicate.
