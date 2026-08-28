# UBS warning baseline — core crates (bd-tp4z)

UBS (Ultimate Bug Scanner) reports raw counts dominated by two kinds of noise: findings inside
`#[cfg(test)]` regions (where `unwrap`/`expect`/`panic!` are the language of tests) and
name-shape false positives (an identifier containing `token` is not a secret). This document is
the triage baseline: what the scanner reports on the core crates, how it splits between test and
production code, which production findings were reviewed and accepted, and the policy that makes
the "exit 0 before every commit" rule meaningful.

Measured at `10c4ed25..4d0e896f` (2026-08-28) with `ubs <file> -v`; classification script walks
each reported location and tests membership in a `#[cfg(test)] mod` region.

## Measured split, production vs test regions

| File | Criticals | Unique locations | In test regions | In production code |
|---|---|---|---|---|
| fm-core/src/lib.rs | 2 | 24 | 6 | 18 |
| fm-parser/src/lib.rs | 1 | 21 | 10 | 11 |
| fm-parser/src/mermaid_parser.rs | 236 | 30 | 10 | 20 |
| fm-parser/src/ir_builder.rs | 0 | 14 | 5 | 9 |
| fm-layout/src/lib.rs | 177 | 47 | 23 | 24 |
| fm-render-svg/src/lib.rs | 3 | 30 | 9 | 21 |
| fm-render-term/src/lib.rs | 0 | 4 | 4 | 0 |
| fm-render-canvas/src/lib.rs | 0 | 3 | 1 | 2 |
| fm-wasm/src/lib.rs | 14 | 24 | 19 | 5 |

The raw "Critical" counts are NOT production defect counts. Every critical in these crates
resolved to a false positive (below) or a finding in test code.

## Production findings reviewed and ACCEPTED (with the reason)

Every production-code location in the table above was read in context on the baseline date. The
high-signal categories resolved as follows:

### False positives by name or shape

- **"Secret/token comparisons without timing-safe equality"** (fm-core:2923/2933,
  fm-parser-mermaid:1853/1939/1999, fm-layout:1071-1073): all are `key == key` comparisons on
  subgraph lookup keys and map keys. No secrets, tokens, or credentials exist in these crates.
- **"Hardcoded secrets/credentials"** (fm-parser-mermaid:10297): `const STATE_PSEUDO_TOKEN =
  "__state_start_end"` — a parser sentinel whose *name* contains "token".
- **"Security-sensitive non-crypto randomness"** (fm-layout:3592): the flagged region is
  `layout_diagram_traced_with_config_and_guardrails`; no randomness exists on the path — and
  determinism is a pinned guarantee of this crate (see `layout_fp_determinism.rs`).

### `as` casts, safe by construction (pre-clamped or value-bounded)

- fm-core:4530 `number.round().min(f64::from(u32::MAX)) as u32` — clamped to the target range
  immediately before the cast; the adjacent comment documents why.
- fm-core:4857/4863 `scaled.min(1_000) as u16` / `(lag_ms * 50).min(1_000) as u16` — clamped to
  a permille range before narrowing.
- fm-layout:8330 `rem_euclid(7) as u8` — remainder in 0..6.
- fm-layout:8467-8468, fm-parser-mermaid:9089-9091 — per-digit date decomposition into a fixed
  `[u8; 10]`; the mermaid copy is guarded by `year < 10000`, and gantt dates are parse-bounded.
- fm-render-svg:2595 `i as u8` under `while i < 256`; :9105/:9174 widening `as u64`.
- fm-parser:1644 `distance.saturating_sub(1) as f32` — widening of a small levenshtein distance.
- fm-wasm:1468 `.max(1) as u64` — floor-clamped widening.

### Indexing and slicing, bounds-proven at the site

- fm-parser:64-68 — quote stripping guarded by `trimmed.len() < 2` before `&trimmed[1..len-1]`.
- fm-parser:77 — `cleaned_bytes[len-1]` preceded by an `is_empty()` early return.
- fm-parser:281 — LCP fold: `compared = limit.min(input_bytes.len())` where `limit` begins at
  `first_bytes.len()` and only shrinks, so `first_bytes[..compared]` is always in range.
- fm-core:2113-2122 — fixed `[f32; 4]` indexed by `0..4`.
- fm-parser-ir_builder:285-290 — `0..shared_len` where `shared_len = min(target.len(),
  source.len())`.
- fm-layout:495-502 — `adjacency[edge.source]` sized by `node_count`; edge endpoints are
  interned node ids by IR construction (edges cannot name a node that was never interned).
- fm-render-svg:702/716/757 — memchr-bounded scans returning `Option`s; `.first()` is Option-safe.

### Intentional fail-loud or test-of-record patterns

- fm-render-svg:5240/5371 `handles.into_iter().map(|h| h.join().unwrap())` — render workers;
  a worker panic must fail the render loudly. The render path has no error channel to propagate
  into; converting this to a silent skip would hide a broken render behind an empty document.
- fm-parser:2186 `.expect("peeked deck directive must be available")` — immediately after a
  `.peek().is_some_and(...)` guard on the same iterator; the message states the invariant.
- fm-wasm:227/256/267 `config.clone()` — builder-pattern clones at API entry points (cold path).
- The `clone()`-in-loop, `collect::<Vec<_>>() then for`, `format!`-in-loop, and
  floating-point-equality INFO items in production code are performance *hints*, not defects:
  each identified site was either already measured in the perf campaign (see
  docs/PERF_LEDGER.md and docs/NEGATIVE_EVIDENCE.md before "optimizing" any of them) or is on a
  cold path. Per the ledger rules, none may be "fixed" without a counted measurement.

## Policy

1. **Test regions are out of scope.** Findings inside `#[cfg(test)]` modules are accepted
   unconditionally — `unwrap`/`expect`/`panic!`/`assert!` are how Rust tests express failure.
2. **Production findings are triaged against this document, not fixed blind.** A new production
   finding in a category listed above as "safe by construction" is accepted if the guard is
   visible at the site; anything else is a real finding — fix it or file a bead.
3. **The golden rule stays**: `ubs <changed-files>` before every commit; exit > 0 means diff your
   findings against this baseline. A finding that is new (not explainable by this document) is a
   regression you introduced — do not commit it.
4. **This baseline is dated.** Re-measure (the classification walk is ~4 minutes over the eight
   files) after any large refactor, and re-accept or amend the categories here.
5. **Scanner criticals are hypotheses, not defects.** The two "critical" families in these crates
   (timing-safe comparisons, hardcoded credentials) have a 100% false-positive rate on this
   codebase as of this baseline. Treat them as such — but re-verify if secret handling is ever
   actually added (auth tokens live outside this workspace).
