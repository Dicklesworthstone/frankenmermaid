# Surface: remaining parse-stage levers (measured) + one dead-end

**Crate:** `fm-parser` · **Date:** 2026-06-24 · **Agent:** frankenmermaid-cc
After format-complement (0a65b61) and FxHashMap lookups (3444621), I probed what's
left of parse. Recording so the next effort targets the right thing.

## Dead-end: `looks_like_dot` (do NOT bother)

`detect_type` calls `looks_like_dot(input)` per parse, which does a full
`strip_all_comments(input)` (even `chars().collect::<Vec<char>>()` of the whole input)
just to check a DOT header and return false for the common non-DOT case. Wasteful,
but the same-worker A/B (skip it) is **only +1.7%–3.5%, n.s. for medium/large**
(p=0.10 / 0.25) — below the keep threshold. Not worth optimizing.

## Big remaining lever: the chumsky statement-parser **run** (~10–36% of parse)

`parse_flowchart_statement_asts` runs `flow_statement_parser().parse(statement)` for
every statement. A double-parse probe (run the combinator twice, measure the delta)
isolates the chumsky construct+run share:

| bench | chumsky construct+run ≈ | p |
|-------|-------------------------|---|
| `parse/flowchart/medium_100` | **~36% of parse** | <0.05 |
| `parse/flowchart/small_10`   | ~25% | <0.05 |
| `parse/flowchart/large_1000` | ~10% | <0.05 |

The earlier parser-**cache** experiment (44b512f) proved *construction* is cheap (caching
it behind `Arc<dyn>` was 5–9% **slower** — vtable dispatch > rebuild savings, and the
per-call build is fully monomorphized/inlined). So this share is the combinator
**execution**, not construction. The only way to cut it is a **hand-rolled fast path**
for the common simple statements (bare id, `id-->id`, `id[label]`) that falls back to
chumsky for anything complex.

**Why it's not a quick lever:** the fast path must produce a `FlowAst` byte-identical to
chumsky's for every form it accepts (node-id charset, label/shape extraction, edge
operator set `--> --- -.- ==>` etc., edge labels, whitespace) — any divergence breaks
snapshot conformance. It needs a dedicated differential-test harness comparing the
fast path against `flow_statement_parser().parse()` across a large generated corpus of
statements before it can ship. Reordering to try the existing recovery parsers
(`parse_edge_statement_asts`) before chumsky is NOT safe — those are only reached today
when chumsky fails, so their output equivalence to chumsky on valid edges is unverified.

This is the largest remaining parse opportunity; flagged for a focused, test-first effort.
