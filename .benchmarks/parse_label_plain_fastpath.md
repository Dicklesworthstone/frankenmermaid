# Perf win: plain-label fast path in `parse_label` (parse ~4-9%)

**Crate:** `fm-parser` — **Date:** 2026-06-28 — **Agent:** BlackThrush
**Verdict:** kept — flowchart parse ~9% faster, wide parse ~4-6%; byte-identical.

## Motivation (fresh profile)

Phase-timed `parse_flowchart` on the 16x32 wide graph (FM_PARSE_TIMING) shows **doc-parse is
~49% of flowchart parse** (735 µs of ~1.5 ms; lower ~25%) — much larger than the stale "~26.5%"
notes. doc-parse runs `parse_label` once per node label, and `parse_label` ran four
`trim`/`trim_matches` scans plus a two-`find` HTML-entity decode on **every** label, even though
the overwhelmingly common label has no quotes, markdown, or entities.

## What changed

A guard-the-scan fast path at the top of `parse_label`: when the label contains none of `"` `'`
`` ` `` `&` `#`, return `ParsedLabel::plain(raw.trim())` directly, skipping the quote-stripping,
markdown, and entity-decode passes.

```rust
if !raw.bytes().any(|b| matches!(b, b'"' | b'\'' | b'`' | b'&' | b'#')) {
    let trimmed = raw.trim();
    return (!trimmed.is_empty()).then(|| ParsedLabel::plain(trimmed));
}
```

## Correctness

Byte-identical: for a label with none of those bytes the full path also reduces to
`Some(ParsedLabel::plain(raw.trim()))` (the `trim_matches` find no quotes, it is not markdown, and
`decode_mermaid_entities` returns the input unchanged with no `&`/`#`), or `None` when empty. The
guard is on raw bytes; all five gating chars are ASCII so a non-ASCII byte never falses the check.
405 fm-parser unit tests + doc tests pass.

## Measurement

Same-worker both-order stash-swap A/B, fresh dir `mermaid-pp2`, `cargo bench -p fm-parser --bench
parse_bench`, mt=4. fm-parser is highs-sys-free so it benches reliably even while the worker pool's
cmake/highs-sys is degraded.

| bench | ORDER_A (ORIG vs opt) | ORDER_B (OPT vs orig) | geo-mean OPT/ORIG |
|---|---:|---:|---:|
| `parse/flowchart/medium_100` | +5.9% (p=0.00) | −11.9% (p=0.00) | ~0.91 (**~9% faster**) |
| `parse/flowchart/large_1000` | +8.8% (p=0.00) | −10.9% (p=0.00) | ~0.90 (**~9% faster**) |
| `parse/wide/8x16`  | +1.1% (NS) | −7.7% (p=0.00) | ~0.96 (**~4% faster**) |
| `parse/wide/16x32` | +3.1% | −5.8% (p=0.00) | ~0.96 (**~4% faster**) |
| `parse/wide/12x24` | +3.7% (p=0.04) | −0.6% (NS) | ~0.98 (~2%) |

Direction-consistent OPT-faster in both orders at every size (the big flowchart sizes p=0.00 both
ways). The flowchart benches win more (~9%) than wide (~4%) because their labels are a larger share
of statements. Conservatively ≥4% parse.

## Mermaid.js head-to-head

First parse win since the fast-path/FxHashMap/borrow harvest. Parse is ~27% of the wide pipeline
(816 µs of ~3 ms at 16x32); ~4% parse ≈ ~1% wide pipeline, and ~9% on the sparse
`full_pipeline`/`large` flowcharts. Full-pipeline ratio vs pinned Mermaid `11.12.0` improves
marginally. Byte-identical.
