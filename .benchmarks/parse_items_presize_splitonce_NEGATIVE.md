# Negative: items-Vec pre-size + node split_once fold — ~0/below-noise, REVERTED

**Crate:** `fm-parser` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** ~0 (below the noise floor); the parse fast-path is near its floor after bbaf088 + 6a8d164. Do not retry these.

## Levers (two small, byte-identical, mechanistically ≤-work)

After the two landed parse wins (parse_label fast path bbaf088, edge right-contains guard 6a8d164),
a fresh doc-parse profile showed items_loop ~90% of doc-parse, lines_collect ~10%. Two residual
micro-levers in the items loop:
1. `parse_flowchart_document_items`'s `items` Vec was `Vec::new()` → `Vec::with_capacity(lines.len())`
   (avoids ~11 reallocs growing to ~1472 items).
2. node fast path scanned for `[` twice (`trimmed.contains('[')` then `trimmed.split_once('[')`) →
   folded into one `if let Some(..) = split_once('[')`.

## Measurement

Same-worker both-order A/B, `parse_bench`, mt=4, **at local load ~40**. 405 fm-parser tests pass
(byte-identical). The result was **load-drift-dominated and contradictory**: wide/12x24 showed
+6.9% (ORDER_A) vs +9.8% (ORDER_B) — opposite directions, both p=0.00; large_1000 ORDER_B +10.9%
"slower". The later run was uniformly slower (load climbing through the session), masking any real
effect. No direction-consistent signal ⇒ effect is below the noise floor.

## Why ~0 (do-not-retry)

Both are mechanistically ≤-work but tiny: the split_once fold removes one short `[` scan per node
(~512), and the items-Vec growth memcpy is ~10 µs (~1%). Unlike the two landed wins — which removed
*N substring-search/trim calls per statement* (parse_label's 4 trims + decode, the edge's 6
`contains`) — these remove only a single scan / a few reallocs, well under the per-statement
allocation/hashing floor. The parse fast-path's exploitable multi-scan redundancies are now
harvested; further parse gains need the inherent IR-ownership allocs (id/label string copies, both
required) or interning re-hash, which prior swarm work found ~0 (hot-free-list recycled). Re-measure
only on a quiet box if revisiting; not worth it at the current ~1% mechanistic ceiling.
