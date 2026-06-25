# Perf win: parse() skips format-complement on the hot path

**Crate:** `fm-parser` · **Date:** 2026-06-24 · **Agent:** frankenmermaid-cc
**Verdict:** kept — reproducible ~8.5–8.8% parse speedup (p<0.05), output-correct.

## What changed

`parse()` unconditionally called `capture_format_complement(input)` — a second full
pass over the source that records whitespace/comment/directive/quoted-literal spans
(plus per-span byte→line/col mapping and a `collect_quoted_literals` scan) purely for
**round-trip editing** and the evidence summary. The parse → layout → render hot path
never reads it. Now `parse*` leaves `format_complement` empty and the two real
consumers capture it explicitly:
- `build_parse_lens` (the round-trip/lens feature) captures after parsing.
- the CLI `parse` evidence command captures before building its summary.

Output of `parse()` is otherwise unchanged (IR identical); the lens/evidence paths
produce the exact same `format_complement` as before. **All tests pass**
(`fm-parser` 402 + `fm-cli` suites, 0 failed — incl. the lens/format-complement
assertions via `build_parse_lens`).

## Measurement — same-worker A/B (stash-swap, one rch session)

```
cargo bench ... --save-baseline xfast    # OPT: parse skips capture
git stash push -- crates/fm-parser/src/lib.rs   # -> orig (parse captures)
cargo bench ... --baseline xfast         # ORIG vs OPT, same worker
```

| bench | ORIG vs OPT (orig slower ⇒ opt faster) | p |
|-------|----------------------------------------|---|
| `parse/flowchart/medium_100` | **+8.8%** | <0.05 ✓ |
| `parse/flowchart/large_1000` | **+8.6%** | <0.05 ✓ |
| `full_pipeline/parse_layout_svg/typical_7_nodes` | +8.8% | <0.05 ✓ |
| `full_pipeline/parse_layout_svg/cyclic_50` | +4.8% | <0.05 ✓ |
| `parse/flowchart/small_10` | +1.7% | 0.30 (n.s., tiny input) |
| `full_pipeline/*/{medium_100,large_500}` | within noise | n.s. |

The parse *stage* is ~8.5–8.8% faster on medium/large; full-pipeline cases where parse
is a smaller share (large_500: render dominates) stay within noise, as expected.

## Why this one worked (vs the two ~0-gain levers before it)

`capture_format_complement` is a real, **eliminable** chunk of the *dominant* stage
(parse is the biggest per-node cost). The earlier render-Cow and crossing-refinement
levers were ~0 because they touched non-dominant / lightly-exercised work. Lesson held:
measure the dominant path same-worker, cut whole passes — not micro-allocations.
