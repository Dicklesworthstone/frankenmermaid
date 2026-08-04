# WIN: stream requirement-node subtitle text — requirement render ~4-9%, byte-identical

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** HEAD (`9490378`) · **File:** `crates/fm-render-svg/src/lib.rs`
**Verdict: KEEP** (byte-identical, CI-disjoint faster at all three sizes vs a neutral null).

## Profile-first (mechanism)

`requirement_stages` is render-dominated (~616µs render vs ~321µs parse at 512). The requirement node fast path
(`write_requirement_node_fragment_into`) built each subtitle line via `format!` before handing it to
`write_req_subtitle_into`, which escapes and writes it: `format!("Risk: {risk} | Verify: {verify_method}")` (and
the `«{type}»` and single-field variants) — **a throwaway `String` alloc per node**. `gen_requirement_chain` sets
both `risk` and `verifymethod`, so every node hits the two-field branch (512 allocs on the dominant stage).

## The lever (one)

Add `write_req_subtitle_body_into` — the same `<text …>…</text>` envelope but with the body left to a caller
closure. The four subtitle sites now stream their fixed labels + escaped fields straight into the buffer
(`push_str("Risk: ")` → `write_escaped_text(risk)` → `push_str(" | Verify: ")` → `write_escaped_text(vm)`),
dropping the per-node `format!` String. `write_req_subtitle_into` (still used for other callers) delegates to the
new function.

Also removed 4 now-unused `use std::fmt::Write as _;` imports in the accent-harvested node writers
(class/subroutine/common/requirement) — the accent digit-table commits (`a38a61e`/`7379288`) replaced each
function's last `write!` with `write_uint_into`, leaving the trait import unused (a latent `-D warnings` break).
Housekeeping, zero codegen effect.

## Byte-identical (proven)

`write_escaped_text` escapes per char, so `escape(a ++ b) == escape(a) ++ escape(b)`; the fixed labels
(`Risk: `, ` | Verify: `, `Verify: `, `«`, `»`) hold no XML specials, so streaming the parts equals escaping the
old joined `format!` whole.

- **`cargo test -p fm-render-svg --lib`: 247 passed, 0 failed** — incl. `node_fast_fragment_matches_render`
  (streamed fragment ≡ slow-path Element). Build is warning-clean (the 4 `std::fmt::Write` warnings are gone).
- **`golden_svg_test`: only `gantt_basic` fails** (pre-existing); `requirement_basic` and all others pass — the
  requirement-diagram SVG is byte-identical.

## Measurement — clean same-worker A/B on hz2, layout+parse null

`requirement_stages` bench. cand = worktree; base = `git show HEAD:lib.rs > lib.rs`. Both arms on **hz2** (hz1 was
persistently failing this bench with exit 101 — routed around it by retrying). `layout/*` and `parse/*` are null
rows (a render-only change can't touch them). Gate on median.

| stage/size | cand p50 (µs) | base p50 (µs) | cand/base | note |
|---|---:|---:|---:|---|
| **render/256** | **293.40 [289.7, 298.0]** | **322.32 [317.3, 328.4]** | **0.910 (−9.0%)** | CIs disjoint |
| **render/64**  | **144.59 [140.2, 149.8]** | **153.63 [151.7, 155.9]** | **0.941 (−5.9%)** | CIs disjoint |
| **render/512** | **580.91 [568.6, 595.3]** | **606.24 [598.9, 615.9]** | **0.958 (−4.2%)** | CIs disjoint |
| layout/512 (null) | 119.74 | 120.42 | 0.994 | neutral |
| layout/256 (null) | 61.71 | 60.20 | 1.025 | mildly adverse |
| parse/512 (null)  | 325.55 | 327.15 | 0.995 | neutral |

**Read:** the null rows sit at ~1.00–1.025 (cand neutral to mildly *slower* on identical code), yet **all three
render sizes are CI-disjoint faster** — the win is unambiguous, not worker drift. Drift-corrected ≈ −4 to −11%. A
first cross-worker pair (cand on the faster ovh-a) drift-corrected to ~6–9%, corroborating. Same per-node
alloc-removal family as the earlier requirement `to_ascii_lowercase` win (−6%).

## Scope

Requirement diagrams (one `format!` String removed per node with risk/type/verify). Byte-identical +
monotonic-less-work. LEVER (reusable): an interpolated `format!(...)` immediately consumed by a `write_escaped_*`
call → split the `<text>` writer into an envelope + body-closure and stream the fixed-label/escaped-field parts.
Same family as the sanitize-token write-into and the (rejected, parse-dominated) class-member streaming — it pays
here because requirement render is the *dominant* stage.
