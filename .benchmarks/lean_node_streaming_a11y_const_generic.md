# WIN: the lean output profile now streams — smaller SVG is no longer more expensive (bd-b2b6)

**Date:** 2026-07-09 · **Agent:** cc_fm · **Base:** `a5dee40` (rebased onto `5003c27`, docs-only)
**File scope:** `crates/fm-render-svg/src/lib.rs` (node fast path + gates only), `crates/fm-cli/examples/headtohead.rs`
(measurement mode). **Deliberately NOT touched:** `render_layout_to_svg` / buffer plumbing / `element.rs` /
`attributes.rs` — the cod pane owns the large-diagram double-copy (rope/arena buffers, writer fusion).

## The paradox

The head-to-head harness (bd-1buv.1) measured that `A11yConfig::none()` produces **1.47–6.13× smaller SVG
than mermaid** but was **1.5–2.02× SLOWER than our own default profile**. Less output cost more work.

**Cause (confirmed by profile, not assumed):** `perf record` on a symbolized build showed
`fm_render_svg::render_node` — the slow per-element `Element` builder — consuming samples *only* in the lean
pass. The streaming node fast paths in `render_node_into` and `render_node` were gated on
`a11y.aria_labels && a11y.keyboard_nav && a11y.text_alternatives`, so a11y-off fell all the way back to
building a `<g>` + shape + `<text>` `Element` tree per node.

A 2026-06-29 ledger entry had asserted "render TIME reduction is ~proportional" to the 30.6% byte reduction.
That was never measured. It is the opposite: the byte reduction *cost* ~2× render.

## The lever

`write_common_node_fragment_into` / `build_common_node_fragment` gained a **const generic** `A11Y: bool`.
`true` emits the `role`/`aria-label`/`tabindex`/`<title>` set; `false` emits none of it. Both node gates now
accept uniformly-on **or** uniformly-off a11y (`uniform_a11y()`), dispatching to the matching
monomorphization. Mixed combinations (e.g. `A11yConfig::minimal()`) still take the slow path, exactly as before.

**Why const generic and not a runtime flag:** the runtime-flag version was implemented and measured first. It
cost **+0.1 … +0.33% instructions on the default path** (deterministic, reproducible: 1.0011 / 1.0033 / 1.0018 /
1.0021). That fails this project's monotonic-less-work bar. The const parameter makes the default
monomorphization exactly as branch-free as before: **1.0001 … 1.0003×**, i.e. neutral.

## Behaviour parity

**Byte-identity, the strong form:** the whole 13-item pinned head-to-head corpus was rendered under **both**
profiles by a pristine `HEAD` build and by the candidate, and all **26 SHA-256s match** (13 default + 13 lean).
The lean bytes the fast path now streams are exactly the bytes the slow `Element` path used to produce.

- `cargo test -p fm-render-svg --lib`: **244 passed** (242 before; +2 new pin tests).
  - `node_lean_fast_fragment_omits_a11y` pins the lean fragment bytes (no `role`/`tabindex`/`<title>`).
    They are pinned as a literal because, with the fast path now handling lean, **no configuration can reach the
    slow path to re-derive them** — they were derived from the 26-hash comparison above.
  - `mixed_a11y_falls_back_to_slow_path_and_honours_each_flag` pins `A11yConfig::minimal()` behaviour.
- `frankentui_conformance_test`: green. `golden_layout_test`: green.
- `golden_svg_test`: 1 pass / 1 fail (`gantt_basic` FNV mismatch) — **verified identical at untouched HEAD in a
  clean worktree**, i.e. pre-existing, not introduced here.
- `cargo clippy -p fm-render-svg --all-targets -D warnings`: clean. `cargo fmt --check`: clean.
- `ubs crates/fm-render-svg/src/lib.rs`: 14 criticals, **the same 14 as untouched HEAD** (all the `ch == '-'`
  "secret compared with ==" false positive). The first draft added 2 more by writing
  `uniform_a11y(..) == Some(true)`; rewritten as `matches!(..)`, which is clearer anyway.

## Measurement

Wall clock on this box is unusable for a <5% claim right now (load average 50), so the **decision metric is the
deterministic instruction count**: `perf stat -e instructions:u`, two-point delta (reps 36 − reps 6) to cancel
startup, `FM_H2H_FORCE_PROFILE` pinning both measured passes to one profile and forcing `batch = 1` so work is
exactly proportional to reps. Same machine, same binaries, reproducible to 4 decimal places.

| item | lean instr (cand/base) | default instr (cand/base) |
|---|---:|---:|
| flowchart_medium_100 | **0.8562×** | 1.0001× |
| flowchart_large_500 | **0.7099×** | 1.0003× |
| wide_8x16 | **0.8578×** | 1.0002× |
| wide_16x32 | **0.8052×** | 1.0002× |
| dense_dag_200 | **0.9030×** | 1.0001× |
| edit_trace_60x20 | **0.8695×** | 1.0001× |
| class_50 | 1.0031× | 0.9999× |

**Lean: 10–29% fewer instructions. Default: neutral (≤0.03%).**

Wall-clock corroboration (same core, order-alternating base/cand, 3 rounds, median-of-rounds; load 50 so
`cv_pct` ran 3.3–11.8% and does **not** meet the <5% bar — reported as corroboration, not as the claim):
lean speedup **geomean 1.197×**, min 1.007×, max **1.517×** (`flowchart_large_500`); default ratio geomean
0.9906× (noise around the instruction-measured 1.0002×).

The paradox is now much smaller. `lean ÷ default` wall time:

| item | before | after |
|---|---:|---:|
| wide_8x16 | 2.07× | 1.66× |
| flowchart_large_500 | 1.89× | **1.20×** |
| wide_16x32 | 1.82× | 1.40× |
| edit_trace_60x20 | 1.21× | **1.03×** |

## What is left (follow-ups, not this lever)

1. **Edges.** `render_edge`'s whole-edge and inner-`<path>` fast paths are still gated on
   `text_alternatives && aria_labels && keyboard_nav`. In lean, all 224 edges of `wide_8x16` still build
   `Element`s. This is the larger remaining half on wide layered flowcharts and is the same const-generic move.
2. **Class / requirement compartment fast paths** (the sibling gates near the common one) remain a11y-gated.
   `class_50` lean is **+0.31%** because the common gate now evaluates further before failing for class nodes;
   it is the only item that regressed, and it disappears once those gates get the same treatment.

## Do-not-retry

- Do **not** use a runtime a11y flag inside the fragment writer: measured +0.1…0.33% default instructions.
- Do **not** attempt a wall-clock-only A/B for this on a loaded box; the effect on the default path (~0.02%)
  is three orders of magnitude below the noise. Use `FM_H2H_FORCE_PROFILE` + `perf stat` instructions.
- Do **not** re-derive the lean fragment bytes from the slow path — the fast path now owns them. Change them
  only together with `node_lean_fast_fragment_omits_a11y`.
- rch prunes a sibling `CARGO_TARGET_DIR` when it builds into another one: **copy both A/B binaries out of the
  target dirs before running the comparison.** This silently deleted a freshly built baseline twice.
