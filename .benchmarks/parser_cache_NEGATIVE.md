# NEGATIVE result: caching the chumsky flow-statement parser = regression

**Crate:** `fm-parser` · **Date:** 2026-06-24 · **Agent:** frankenmermaid-cc
**Verdict:** ~5–9% **slower** → reverted (stash). Parser *construction* is not the parse bottleneck.

## Hypothesis (wrong)

`parse_flowchart_statement_asts` calls `flow_statement_parser().parse(statement)` for
every statement, rebuilding the (large) chumsky combinator tree per line. The classic
"construct-parser-in-loop" pitfall — so caching the parser should speed up parsing.

## What was tried

Enabled chumsky's `unstable` feature and cached the parser process-wide with
`chumsky::cache::{Cache, Cached}`:
```rust
type Parser<'src> = Arc<dyn Parser<'src, &'src str, FlowAst, extra::Err<Rich<'src, char>>> + Send + Sync + 'src>;
static FLOW_STATEMENT_PARSER: LazyLock<Cache<FlowStatementParserCache>> = LazyLock::new(Cache::default);
```
Built once, reused for every statement. Compiles; 402 fm-parser tests pass (output identical).

## Measurement — same-worker A/B (stash-swap, measurement-time 4)

`change` = per-statement (orig) relative to cached baseline; **negative = orig faster**:

| bench | orig vs cached | p |
|-------|----------------|---|
| `parse/flowchart/large_1000` | **−7.2%** (orig faster) | <0.05 |
| `parse/flowchart/small_10` | −4.5% (orig faster) | <0.05 |
| `parse/flowchart/medium_100` | +0.8% | 0.61 (n.s.) |
| `full_pipeline/large_500` | **−9.0%** (orig faster) | <0.05 |
| `full_pipeline/cyclic_50` | −4.0% (orig faster) | <0.05 |

The cached parser is **slower**. `Cache::get()` hands back an `Arc<dyn Parser>`, so every
`.parse()` goes through a vtable; that per-call dynamic-dispatch cost exceeds whatever is
saved by not rebuilding the combinator tree. The per-call `flow_statement_parser()` path
is fully monomorphized/inlined, and chumsky's construction is evidently cheap enough that
the compiler handles it well.

## Do-not-retry note

Don't cache this parser behind `dyn`. If parser construction is ever shown to dominate
(it isn't here), the only win would be a *monomorphic* reuse (thread the concrete
`impl Parser` through the call chain), not a type-erased `Arc<dyn>`/`Boxed` cache — and
the `unstable` chumsky feature this requires is not worth pulling in. Parse cost lives in
the actual per-line scanning/IR-building, not in building the parser.
