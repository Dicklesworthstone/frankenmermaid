# WIN: drop redundant per-node `to_ascii_lowercase()` before the CSS-token sanitizer — requirement render −6%

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** HEAD (`7379288`) · **File:** `crates/fm-render-svg/src/lib.rs`
**Verdict: KEEP** (byte-identical, removes a per-node String allocation, CI-disjoint at 256 & 512 nodes).

## Profile-first (mechanism)

The pipeline phase split (wide-flowchart stage bench) is render-dominated (~66%), and render's byte-production
primitives are mature/at-floor. Digging the requirement-node fast path (`write_requirement_node_fragment_into`,
the writer benched by `requirement_stages/render`) surfaced a **redundant allocation** rather than a byte cost:

```rust
if let Some(risk) = meta.risk.as_deref() {
    out.push_str(" fm-req-risk-");
    write_sanitized_css_token_into(out, &risk.to_ascii_lowercase());  // <- throwaway String per node
}
```

`write_sanitized_css_token_into` (lib.rs:1503) already lowercases **every** ASCII-alphanumeric char
(`ch.to_ascii_lowercase()`) and maps every other char to `-`, independent of case. So `risk.to_ascii_lowercase()`
allocates a per-node `String` whose only effect — lowercasing — the sanitizer redoes anyway. `gen_requirement_chain`
emits `risk: high` on every node, so this fires once per requirement node.

## The lever (one)

Pass the raw `&str` straight to the sanitizer at both requirement sites (`risk` line 5627, `requirement_type`
line 5631); delete the `.to_ascii_lowercase()`. Removes one heap `String` alloc+free per node.

## Byte-identical (proven)

For any input, `write_sanitized_css_token_into(out, s)` and `write_sanitized_css_token_into(out, &s.to_ascii_lowercase())`
emit the same bytes: ASCII-alphanumerics are lowercased by the sanitizer regardless; every non-alphanumeric (incl.
all non-ASCII, one `-` per `char`) maps to `-` regardless of case; and ASCII-lowercasing never changes which chars
are alphanumeric. Verified:

- **`cargo test -p fm-render-svg --lib`: 247 passed, 0 failed** — incl. `node_fast_fragment_matches_render`
  (streamed fragment ≡ slow-path Element).
- **`golden_svg_test`: only `gantt_basic` fails** — the documented pre-existing FNV mismatch; gantt renders no
  requirement nodes so this edit provably can't touch it. All other cases pass.

## Measurement — same-worker A/B, both arms on hz2, layout/parse = built-in null

`requirement_stages` bench (`pipeline_bench`). cand = worktree; base = `git show HEAD:lib.rs > lib.rs`
(lever reverted, bench unchanged). `RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- cargo bench`; **both
arms landed on hz2** (one base attempt hit a transient hz1 exit-101, retried onto hz2). `layout/*` = null rows
(render-only lever). Gate on median.

| stage/size | cand p50 (µs) | base p50 (µs) | ratio | note |
|---|---:|---:|---:|---|
| **render/512** | **589.27 [585.2, 594.8]** | **628.97 [622.9, 635.9]** | **0.937 (−6.3%)** | CIs disjoint |
| **render/256** | **311.33 [307.8, 316.1]** | **330.04 [325.1, 335.8]** | **0.943 (−5.7%)** | CIs disjoint |
| render/64 | 152.05 | 153.14 | 0.993 | small-N, overlap (noise) |
| layout/512 (null) | 120.25 | 120.68 | 0.996 | tight |
| layout/256 (null) | 60.89 | 61.07 | 0.997 | tight |
| layout/64 (null) | 62.99 | 62.49 | 1.008 | tight |

The layout null sits at ~1.00 (0.4–0.8% drift), so worker speed is ruled out. Render is **CI-disjoint at both 256
(−5.7%) and 512 (−6.3%)**, scaling with node count — a genuine per-node saving (~77 ns/node, consistent with one
small-String alloc+free removed). Larger than the accent digit-table win because an alloc+free costs more than a
Formatter call.

## Scope

Requirement diagrams with a `risk`/`type` field (one alloc removed per such node). Byte-identical +
monotonic-less-work. LEVER (reusable): grep `f(&x.to_ascii_lowercase())` / `f(&x.to_lowercase())` where the sink
`f` independently lowercases (or is case-insensitive) → drop the pre-lowercasing, pass the raw `&str`. Leaves
`match x.to_ascii_lowercase().as_str()` sites (6279/6309) alone — those consume the lowercased value.
