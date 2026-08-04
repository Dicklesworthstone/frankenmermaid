# Finding: the parse LOWER phase is at its floor (FxHashMap + alloc-free lookups) — parse fully harvested

**Crate:** `fm-parser` — **Date:** 2026-06-29 — **Agent:** BlackThrush
**Verdict:** lower (the one phase I'd never optimized) is at the language/algorithm floor. No lever. Do not re-open parse.

## Why lower is at floor (verified this turn)

lower (~501 µs, ~32% of doc+lower at 16x32) is interning + IR construction:
- The interning maps (`node_index_by_id`, `cluster_index_by_key`, `subgraph_index_by_key`,
  `label_index_by_text`) are **already `FxHashMap`** — no SipHash to swap out.
- The flowchart hot path `intern_node_auto` does `let normalized_id = id.trim();` (a `&str` slice, no
  alloc) then `self.node_index_by_id.get(normalized_id).copied()` — a **`&str` Borrow lookup with NO
  per-lookup String allocation** (it allocates only the id `to_string` + `IrNode` for genuinely-new
  nodes; edge-endpoint re-lookups of existing nodes are alloc-free).
- `span_for(line, src)` = `Span::at_line(line, src.chars().count())` — `chars().count()` on an ASCII
  line is compiler-counted from non-continuation bytes (vectorized), so an `is_ascii`+`len` fast path
  is ~0 (same bitmask/vectorization lesson as 49d65f1).
- The `id.trim()` Unicode trim on already-`trim_ascii`'d FastEdge endpoints is redundant but ~0 (it
  checks ~2 chars; the scan, not the char count, is the cost).

What's left is inherent: ~2944 FxHash lookups (string-keyed, the same id hashed ~5×, unavoidable with
forward-ref resolution) + IR struct construction into pre-sized Vecs. No exploitable redundancy.

## Parse is fully harvested (all phases at floor)

- **doc_parse**: 2 wins landed (parse_label fast path bbaf088, edge right-contains guard 6a8d164);
  residual micro-levers all ~0 (items-presize a523af9, bracket-check single-char 49d65f1, span_for) —
  the per-byte scans compile to ~1-op/byte bitmask/vectorized loops, so trimming char-sets is ~0.
- **lower**: this finding — FxHashMap + alloc-free lookups, hashing/IR-alloc inherent.
- **detect**: already guarded (`looks_like_dot` byte-checks `{`/`}` before `strip_all_comments`).

Further parse gains would need a different language primitive (arena interning, etc.) that the
zero-dep / forbid-unsafe / byte-identical constraints forbid. Parse is at its constraint-bound floor.

## Infra

Standing/ratio re-measure is blocked: the highs-sys/cmake worker outage persists (full_pipeline_wide /
all fm-cli benches fail to build); only fm-parser builds. Remaining ≥3% headroom is unchanged — the
render output reduction (a11y ~12% cod-b comparator; data-* ~7% owner API), both owner-decisions.
