# bd-1buv.2 two-entry endpoint temporal cache — REJECT (2026-07-22)

## Profile and ledger boundary

This retry started only after the stale parser leases recorded in
`bd_1buv_2_endpoint_temporal_cache_SETTLED.md` were released. The source was clean `4de7ffc0`.
A symbolized large-flowchart parse profile put `NodeIdIndex::get_with_hash` at **9.39% self**;
the full wide pipeline put it at **3.10% self**. The pinned endpoint stream reuses one of its two
most recent node IDs on 49.9% of lookups for both `flowchart_large_500` and `wide_16x32`.

The candidate was distinct from closed hash/index work: the Fx/hash-once index, one-probe insert,
and collision buckets remain unchanged. It added two `Option<IrNodeId>` slots to `IrBuilder` and,
only for `intern_edge_endpoint_pretrimmed`, compared the requested ID against the exact node bytes
at those stable indices before falling back to the existing hash-table path. Hits were moved to the
front.

## Isomorphism proof

- Ordering preserved: yes; cache entries are never iterated into IR output.
- Tie-breaking unchanged: yes; a hit returns the same `IrNodeId` the collision-checked index returns.
- Node updates preserved: yes; an ID enters the cache only after the full intern path has run once.
  That first call upgrades any implicit node. Later fast-edge calls always request `label=None`,
  `shape=Rect`, and `is_auto_created=false`, so the normal table-hit mutation is a no-op.
- Collisions safe: yes; cached candidates compare exact node-ID bytes, not hashes alone.
- Floating point / RNG: N/A.
- Candidate proof: the focused upgrade/cache-order test passed remotely on `ovh-a`. An earlier
  `--exact` invocation ran zero tests and was discarded rather than counted.

## Interleaved pinned A/B/null result

Both release binaries were compiled fail-closed through RCH and executed on the same local host,
pinned to CPU0. ORIG binary SHA-256 was
`2d193b3fc96c38d0f3ce0dec42ba3abd72885b33752df42b63a8c69890d2ecb0`; CAND was
`33990e8c558635ce9b3581fc277cb0125108bad3b7f87a2114c8046e641ec611`. Every arm used the pinned
`scripts/headtohead/run.mjs` corpus at 10x Rust repetitions. The order was `A/B/null/B/A`.

| item / arm | p50 ns | min ns | CV | MAD |
|---|---:|---:|---:|---:|
| flow A1 | 352,762 | 348,592 | 20.39% | 0.52% |
| flow B1 | 341,458 | 337,232 | 2.67% | 0.53% |
| flow null (A) | 348,711 | 344,128 | 3.24% | 0.45% |
| flow B2 | 340,916 | 335,277 | 5.12% | 0.58% |
| flow A2 | 349,993 | 344,448 | 2.09% | 0.49% |
| wide A1 | 581,472 | 571,994 | 5.26% | 0.40% |
| wide B1 | 593,064 | 573,283 | 4.98% | 2.21% |
| wide null (A) | 592,646 | 581,876 | 5.52% | 0.80% |
| wide B2 | 582,043 | 572,097 | 8.17% | 0.80% |
| wide A2 | 583,392 | 575,093 | 5.12% | 0.53% |

Flow B1/A1 was `0.96796x` (-3.20%), but B2/null was only `0.97765x` (-2.24%). Using the median
of the three baseline/null p50s (349,993 ns) and the midpoint of the two candidate p50s
(341,187 ns), the central delta is **-2.52%**, below the 3% keep gate. Wide B1/A1 regressed
**+1.99%** while B2/null improved **-1.79%**; its central delta is **+0.71% slower**. Several
arms also violate the mandatory CV-under-5% gate. Output sizes were identical in every arm:
343,946/232,778 bytes default/lean for flow and 534,365/370,609 for wide.

## Restored comparator context

After manually removing the candidate, `ir_builder.rs` matched `HEAD` byte-for-byte at SHA-256
`c6fcdc2e12cd735bc47a30eda82b777c5f1a22eb29d3ec926f1b4f7fbc8dc1e8`. Fresh live pinned
mermaid-js 11.15.0 runs on the restored ORIG measured:

- `flowchart_large_500`: fm 349.488 us vs mermaid 1,231.1 ms = **3,522.58x** p50 speedup;
  Rust MAD 0.69%, output 343,946 bytes.
- `wide_16x32`: fm 588.025 us vs mermaid 3,430.2 ms = **5,833.43x** p50 speedup;
  Rust MAD 0.79%, output 534,365 bytes.

## Verdict and retry predicate

**REJECT.** The exact two-entry move-to-front endpoint cache is closed: it misses the 3% gate on
the linear flowchart, is flat/slower on wide, and does not satisfy the all-arms CV gate. Retry an
endpoint-locality primitive only if a fresh full-pipeline profile raises node-index lookup above
8% self, or a different representation can resolve the common endpoint without two candidate
string comparisons. Pre-record a null arm with CV <5% before scoring the candidate; require
direction-consistent >=3% on both pinned large flowchart shapes.
