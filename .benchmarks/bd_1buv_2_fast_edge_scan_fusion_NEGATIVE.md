# bd-1buv.2 fast-edge scan fusion — REJECT (2026-07-22)

## Profile and ledger boundary

A same-day symbolized large-flowchart parse profile placed
`parse_fast_simple_flowchart_edge_parts` at **8.53% self**, with its byte-scanning subloops in
the top five. The negative ledger and recent history were searched before editing. Already-landed
work includes the reject-byte LUT, a single leftmost operator scan, endpoint `trim_ascii`, and the
right-side anchor guard; the bracket-only simplification is a recorded zero-gain reject. This
candidate did not retry any of those in isolation.

The distinct Alien Graveyard §6.5 loop-fusion candidate traversed the statement once to perform
reject-byte classification, find the first of the same nine operators, and reject a later chained
operator. It skipped a matched operator as one token so `<-->` and `<-.->` could not be mistaken
for their overlapping inner suffix. Endpoint slicing, ASCII identifier validation, arrow values,
fallback routing, and all operator spellings were unchanged.

## Isomorphism and conformance

- Strict-remote focused tests on `ovh-a`: **3 passed**, including all nine simple-arrow forms,
  equality with the general parser, and rejection of chained/complex statements.
- Every pinned `A/B/null/B/A` arm produced exact SVG SHA-256
  `408ecdccfba04fb4aa84526b565e0397383bb4c0dca9184e33e01b7ef2dd2d21` for
  `flowchart_large_500` and
  `30d79510dbc4590b6346742560acc6d2af20b2439f166adc58a93d2529681fce` for
  `wide_16x32`; default/lean byte counts remained 343,946/232,778 and
  534,365/370,609.
- After measurement the candidate was manually removed. Parser source SHA-256
  `1f5576af8d1533dc778851f54ea64a1cd10c340514a9e0324d19cc19b5d78153`
  matches `HEAD` exactly.

## Isolated parse A/B/null

Both fail-closed RCH release binaries ran on the same host pinned to CPU45. The order was
`A/B/null/B/A`; linear flow used 1,000 samples at 5,000 nodes and wide used 2,000 samples at
1,024 nodes.

| item / arm | median ns | min ns |
|---|---:|---:|
| flow A1 | 1,292,171 | 1,247,096 |
| flow B1 | 1,283,074 | 1,260,952 |
| flow null (A) | 1,299,976 | 1,267,223 |
| flow B2 | 1,277,193 | 1,256,283 |
| flow A2 | 1,307,419 | 1,273,796 |
| wide A1 | 233,823 | 228,333 |
| wide B1 | 235,878 | 231,259 |
| wide null (A) | 234,385 | 228,504 |
| wide B2 | 235,477 | 230,397 |
| wide A2 | 240,817 | 230,858 |

Against the baseline/null median, the candidate midpoint was only **1.53% faster** on the linear
parse (1,280,134 vs 1,299,976 ns), below the 3% gate, and **0.55% slower** on wide parse
(235,678 vs 234,385 ns).

## Interleaved pinned full-pipeline result

ORIG head-to-head binary SHA-256 was
`3e8badb7cac03a44cc030b7732c82f5b715c414835262de0b8279da30c33aac4`; CAND was
`27c31a550d2cb0e55da96bd3e8158ce06bdc8a2b81878cf33cbfc4bd33707023`.
The pinned `scripts/headtohead/run.mjs` corpus ran on CPU45 at 10x Rust repetitions in
`A/B/null/B/A` order.

| item / arm | p50 ns | min ns | CV | MAD |
|---|---:|---:|---:|---:|
| flow A1 | 335,556 | 332,198 | 2.64% | 0.39% |
| flow B1 | 346,541 | 342,858 | 2.98% | 0.52% |
| flow null (A) | 341,761 | 332,165 | 4.10% | 1.58% |
| flow B2 | 347,585 | 343,323 | 4.04% | 0.60% |
| flow A2 | 337,097 | 332,420 | 5.80% | 0.87% |
| wide A1 | 567,436 | 558,629 | 2.43% | 0.70% |
| wide B1 | 581,873 | 570,392 | 4.19% | 1.15% |
| wide null (A) | 585,590 | 564,520 | 5.49% | 1.65% |
| wide B2 | 579,769 | 568,144 | 4.71% | 0.89% |
| wide A2 | 583,710 | 565,673 | 6.67% | 1.43% |

Flow baseline/null median was 337,097 ns versus candidate midpoint 347,063 ns:
**+2.96% slower**. Wide baseline/null median was 583,710 ns versus candidate midpoint
580,821 ns: **0.49% faster**, below the gate. Flow A2, wide null, and wide A2 also violate the
mandatory CV-under-5% KEEP rule.

## Verdict and retry predicate

**REJECT — third consecutive bd-1buv.2 reject after endpoint temporal caching and line-table
streaming.** The exact reject/operator/chained scan fusion is closed: its extra state and full-line
continuation erase the saved traversals, it misses 3% in both parse shapes, and it regresses the
headline linear pipeline. Retry only if a fresh full-pipeline profile attributes at least **8% self**
to this exact classifier (not parse-only), and a generated corpus with mean edge statement length at
least 64 bytes makes removed traversal volume materially larger. Pre-record a null CV below 5% and
require direction-consistent >=3% wins on both pinned shapes. Per the three-reject rule, the next dig
must switch away from parser byte scanning to a different Alien Graveyard primitive.
