# WIN (follow-up): accent digit-table harvest COMPLETE — class + requirement node writers

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** HEAD (`53fc3ecc` lib.rs) · **File:** `crates/fm-render-svg/src/lib.rs`
**Follow-up to `a38a61e`** (which converted the common-node + subroutine-node accent writers).
**Verdict: KEEP** (byte-identical, monotonic-less-work, CI-separated at 512 nodes).

## The lever (one, same as a38a61e)

The four `write_*_node_fragment_into` fast paths each emit `class="fm-node fm-node-accent-N …"`, where
`N = stable_accent_index(node_id)` — a small palette index (FNV-1a mod accent count). `a38a61e` replaced the
`write!(f, "{accent}")` at the **common** and **subroutine** writers with the digit-table
`crate::attributes::write_uint_into`. That grep missed two more sites that write the index via a *positional*
formatter rather than a named one:

```rust
// write_class_node_fragment_into   (lib.rs:5092)
// write_requirement_node_fragment_into (lib.rs:5623)
-    let _ = write!(out, "{}", stable_accent_index(node_id));
+    let _ = crate::attributes::write_uint_into(out, stable_accent_index(node_id) as u64);
```

This completes the harvest across **all four** node-fragment writers. `write!`'s `Display for usize` drags in the
`Formatter`/`pad_integral` machinery per node; `write_uint_into` is the branch-light DIGITS1/PAIRS2 digit-table
writer already used everywhere else in this file. The Element/slow path (`class_prefixed_usize` →
`push_usize`) is already a manual digit writer — no change needed there.

## Byte-identical

`write_uint_into(x as u64)` emits the same decimal digits as `write!("{}", x)` for a small `usize` — no padding,
sign, or width flags are in play. Proven:

- **`cargo test -p fm-render-svg --lib`: 247 passed, 0 failed** — includes `node_fast_fragment_matches_render`
  (the fast-fragment ≡ slow-path Element pin).
- **`golden_svg_test`: only `gantt_basic` fails** — the documented pre-existing FNV mismatch. Gantt charts render
  no class/requirement nodes, so this edit provably cannot touch that case; every other golden case passes.
- `usize as u64` is a lossless widening on 64-bit — no new clippy lint; both edited fns retain another `write!`
  so `use std::fmt::Write` stays live (no unused-import warning). The 4 pre-existing `fm-render-svg` lib warnings
  are unchanged (they predate the accent commits, which all landed over them).

## Measurement — same-worker A/B, both arms on hz2, layout/parse as built-in null

`requirement_stages` bench (`pipeline_bench`, fm-cli): `render/{64,256,512}` hits the requirement writer
(site 5623). cand = worktree; base = `git show HEAD:lib.rs > lib.rs` (lever reverted, bench unchanged). Both
`RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- cargo bench`; **both arms landed on hz2** (direct read,
no worker correction needed). `layout/*` and `parse/*` are **null rows** — the lever touches only render code.

| stage/size | cand p50 (µs) | base p50 (µs) | cand/base | note |
|---|---:|---:|---:|---|
| render/64  | 150.99 | 150.43 | 1.004 | treatment — inside null envelope (noise) |
| render/256 | 329.88 | 324.08 | 1.018 | treatment — inside null envelope (noise) |
| **render/512** | **616.25 [609.2, 624.6]** | **638.50 [629.6, 649.5]** | **0.965 (−3.5%)** | treatment — **CIs disjoint** |
| layout/64  | 63.95 | 62.44 | 1.024 | null — identical code drifts +2.4% |
| layout/256 | 61.36 | 61.26 | 1.002 | null |
| layout/512 | 120.80 | 119.67 | 1.009 | null |
| parse/512  | 321.39 | 323.50 | 0.993 | null |

**Read:** the null rows (identical code both arms) drift up to **+2.4%** (layout/64), which sets the per-arm noise
floor. render/64 (+0.4%) and render/256 (+1.8%) sit *inside* that envelope — noise, not regression. Only
**render/512 clears it: −3.5% with non-overlapping 95% CIs** (cand upper 624.6 < base lower 629.6), against a
~1% null at 512. This is the expected per-node scaling: the saving is one Formatter call per node, so it only
rises above the noise floor at large node counts — the same curve a38a61e measured (0.8%→3.0%→4.8%).

## Scope & honesty

Small per-node win. Helps class diagrams and requirement diagrams; neutral below ~500 nodes (sub-noise).
Byte-identical + monotonic-less-work (strictly removes a per-node Formatter/`pad_integral` call), so it can only
help; the class-node site (5092) has no dedicated bench but is the identical edit on the same writer family and
is covered for correctness by the lib suite. Framed as a **follow-up completing a38a61e**, not a new lever.
