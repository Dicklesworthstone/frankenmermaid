# REJECT: dense single-ID endpoint slot (2026-07-22)

- **Third representation variant:** replaced the typed numeric endpoint map with a direct `Vec<Option<IrNodeId>>`
  slot table for canonical `N<number>` endpoints; all other IDs retained the generic collision-safe index.
- **Pinned release headtohead, CPU45:** flowchart-500 p50 **336,282 -> ~361,000 ns** (about **+7.4%**);
  wide-16x32 **570,280 -> ~628,000 ns** (about **+10.1%**). Output remained byte-identical at 343,946
  and 534,365 bytes. Candidate MADs were 0.7% and 1.9%; the unfavorable direction is decisive.
- **Verdict: REJECT, third consecutive in the numeric-index vein.** Candidate source was manually removed.
  Retry only after a profile proves direct endpoint lookup is independently hot and a no-parse token path
  exists; require one-binary A/B/null/B/A, CV<5%, >=3% on both pinned shapes, exact IR/SVG, conformance,
  and no generic-ID regression. Full artifact: this file.

