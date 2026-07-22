# REJECT: canonical numeric node-index representation (2026-07-22)

- **Profile-first basis:** existing strict-remote profiles put `NodeIdIndex::get_with_hash` at 9.39% of
  flowchart-5000 parse self and 3.10% of wide-1024 full-pipeline self. The fresh candidate added an
  exact typed index for canonical `N<number>` and `N<number>_<number>` IDs, retaining the collision-safe
  hash index as fallback and preserving all generic identifiers.
- **Pinned headtohead, same CPU45, release binaries:** baseline/candidate were built separately from the
  exact same source except this one lever and run with `scripts/headtohead/run.mjs --skip-mermaid
  --reps-scale 0.5 --pin-cpu 45`. `flowchart_large_500` p50 **336,282 -> 348,084 ns (+3.51%)**;
  `wide_16x32` **570,280 -> 589,171 ns (+3.31%)**. Min values were 332,529 -> 342,068 ns and
  565,335 -> 584,958 ns respectively. Baseline/candidate CVs were 8.55%/13.52% (flow) and
  8.73%/9.31% (wide); MADs stayed below 1.4%, but CV failed the mandatory KEEP <5% gate.
- **Behavior proof:** output byte counts were unchanged (343,946 / 534,365; lean 232,778 / 370,609)
  and output SHA-256 matched exactly: flow `408ecdccfba04fb4aa84526b565e0397383bb4c0dca9184e33e01b7ef2dd2d21`,
  wide `30d79510dbc4590b6346742560acc6d2af20b2439f166adc58a93d2529681fce`. Remote parser check passed;
  candidate source was manually removed and `ir_builder.rs` is back to HEAD.
- **Verdict: REJECT.** Retry only if a profile proves numeric IDs account for >=8% full-pipeline self and
  the representation is reached only from an already-tokenized endpoint (no per-intern decimal parse),
  then require one-binary interleaved A/B/null/B/A on both pinned shapes with every scored arm CV <5%,
  >=3% direction-consistent improvement, exact IR/SVG identity, conformance, and no generic-ID regression.

