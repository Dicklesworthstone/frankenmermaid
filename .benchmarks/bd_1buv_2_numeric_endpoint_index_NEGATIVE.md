# REJECT: endpoint-only numeric node-index representation (2026-07-22)

- **Distinct follow-up lever:** after the all-intern numeric index failed, this variant limited typed
  `N<number>` / `N<number>_<number>` lookup to the already fast-path flowchart edge endpoint method;
  ordinary node declarations stayed on the existing collision-safe hash index.
- **Pinned release headtohead, CPU45, same baseline binary:** `flowchart_large_500` baseline p50
  **336,282 ns**, candidate **368,000 ns** (about **+9.4% slower**); `wide_16x32` baseline **570,280 ns**,
  candidate **627,000 ns** (about **+10.0% slower**). Candidate MADs were 1.1% and 3.2%; output bytes
  remained 343,946 and 534,365 with the previously established exact SVG hashes.
- **Verdict: REJECT.** Candidate source was manually removed. Retry only with a profile proving the
  endpoint key construction itself is below 1% self and a direct-address representation eliminates the
  typed-map overhead; require one-binary A/B/null/B/A, CV<5%, >=3% on both pinned shapes, exact IR/SVG,
  conformance, and no generic-ID regression. Full artifact: this file.

