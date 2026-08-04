# WIN: the lean output profile now streams its EDGES too — the paradox is inverted (bd-6u9o)

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** `830d672`
**File scope:** `crates/fm-render-svg/src/lib.rs`, `render_edge_into` + the common-edge fragment writers only.
**Deliberately NOT touched:** `render_layout_to_svg` / `element.rs` / `attributes.rs` / buffer plumbing — the cod
pane owns those (large-diagram double-copy). `render_edge`'s own two fast paths are left a11y-gated **on
purpose** (see "Why the slow path stays reachable").

## The remaining half of the paradox

`bd-b2b6` (landed `2288c78`) fixed the NODE half: the lean (`A11yConfig::none()`) profile fell off the
streaming fast path, so the *smaller* SVG cost ~2× the render. After that landing, lean was still
**1.40–1.66× slower than default** on wide layered flowcharts, because `render_edge_into`'s whole-edge
streaming gate was still `text_alternatives && aria_labels && keyboard_nav`. On `wide_8x16` all **224** edges
(vs 128 nodes) still built per-element `Element` trees under lean. Edges outnumber nodes on every wide
layered flowchart, so this was the larger half.

## The lever

The same const-generic move as the node half, applied to the edge fragment writer:

- `write_common_edge_full_fragment_into` gained a const generic `A11Y: bool`.
  - `true` → `<g id … role="graphics-symbol" tabindex="0"><path …/><title>…</title></g>` (unchanged).
  - `false` → the **bare `<path … id="fm-edge-N"/>`**: no group, no title, no role/tabindex, and the trailing
    `id` that the slow path's final `elem.id(&mermaid_edge_element_id(edge_index))` appends to an *unwrapped*
    edge (last, because `Attributes::set` appends).
- `write_common_edge_path_tail_with_markers_into` gained a const generic `EDGE_ID: bool` to emit that trailing
  `id` before `/>`. Group-wrapped callers pass `false`; the lean whole-edge writer passes `true`.
- `render_edge_into`'s gate relaxed from the a11y triple to `uniform_a11y(&config.a11y)`, dispatching to the
  matching monomorphization. Mixed a11y (`A11yConfig::minimal()`) still takes the slow `Element` path — a raw
  fragment cannot express "role but no tabindex".
- The lean arm also **skips `edge_endpoint_accessible_labels` entirely** (it fed only the `<title>`), rather
  than computing endpoint labels and discarding them.

**Why const generic and not a runtime flag:** re-using the node half's measured result — a runtime a11y flag
inside the fragment writer cost **+0.1 … +0.33% instructions on the default path**. Not re-attempted here; the
do-not-retry note in `lean_node_streaming_a11y_const_generic.md` holds.

## Why the slow path stays reachable (a better oracle than the node half had)

The node half had to pin its lean fragment bytes as a **literal**, because once the fast path handled lean, no
configuration could reach the slow path to re-derive them. That is not the case for edges: `render_edge_into`
delegates to `render_edge` for every gated-out edge (labeled, back-edge, animated, source-spans, inline
`linkStyle`, mixed a11y), so `render_edge` remains a live, independent implementation of the same bytes.

So the new test asserts the streamed lean fragment against **what the `Element` path actually produces**, not
against a hand-written literal. That closes exactly the tautology that
`golden_svg_RED_root_cause_edge_fast_path.md` diagnosed in the old `edge_fast_fragment_matches_element` pin
("fragment == bare path == fragment", which passed while the fast and slow paths disagreed).

## Behaviour parity

**Byte-identity, the strong form:** the whole 13-item pinned head-to-head corpus rendered under **both**
profiles by a pristine `830d672` build and by the candidate — all **26 SHA-256s match** (13 default + 13 lean).

- `cargo test -p fm-render-svg --lib`: **246 passed** (244 before; +2 new).
  - `lean_edge_streaming_matches_element_render` — 9 arrow types × `A11yConfig::none()`:
    `render_edge_into(..) == render_edge(..).write_to_string(..)`, plus shape assertions (starts `<path d="`,
    ends `id="fm-edge-0"/>`, contains no `<g `, `<title>`, `role=`, `tabindex=`).
  - `mixed_a11y_edge_falls_back_to_slow_path` — guards the `uniform_a11y` gate against being widened to
    "any a11y": `minimal()` = role, no tabindex, no `<title>`, no `<g>` wrapper.
- `frankentui_conformance_test`: green. `golden_layout_test`: green (2 passed).
- `golden_svg_test`: 1 pass / 1 fail (`gantt_basic` FNV mismatch) — **reproduced identically at untouched
  `830d672`** in a detached worktree, i.e. pre-existing, not introduced here.
- `cargo clippy -p fm-render-svg --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.
- `ubs crates/fm-render-svg/src/lib.rs`: 14 criticals — the same 14 as HEAD (all the `ch == '-'`
  "secret compared with ==" false positive).

## Measurement

Wall clock cannot resolve the default path's ~0.1% on this box, so the **decision metric is the deterministic
instruction count**: `perf stat -e instructions:u`, two-point delta (`reps=36` − `reps=6`, `warmup=2` fixed) to
cancel process startup / parse / layout / warmup, `FM_H2H_FORCE_PROFILE` pinning both of the harness's passes
to one profile and forcing `batch = 1` so work is exactly proportional to reps. Same machine, core-pinned
(`taskset -c 7`), median of 3 rounds, both binaries copied out of their target dirs first.

> **Scope of these ratios:** the harness times `full_pipeline` = parse + layout + render. Every number below is
> therefore a **pipeline** ratio; the render-only effect is strictly larger. These are the conservative numbers.

| item | lean instr (cand/base) | default instr (cand/base) |
|---|---:|---:|
| flowchart_small_10 | 0.9699× | 0.9999× |
| flowchart_medium_100 | **0.9167×** | 0.9995× |
| flowchart_large_500 | **0.7952×** | 0.9987× |
| wide_8x16 | **0.8292×** | 0.9987× |
| wide_12x24 | **0.7415×** | 0.9984× |
| wide_16x32 | **0.7359×** | 0.9984× |
| dense_dag_200 | **0.7452×** | 0.9985× |
| cyclic_scc_100 | **0.9402×** | 0.9997× |
| sequence_20 | 1.0003× | 1.0001× |
| class_50 | **0.9531×** | 0.9996× |
| state_40 | **0.9411×** | 0.9996× |
| er_40 | 1.0004× | 1.0002× |
| edit_trace_60x20 | **0.9215×** | 0.9996× |

**Lean: up to 26.4% fewer instructions** (wide/dense flowcharts, where edges dominate). **Default: neutral**,
in fact very slightly monotonic-better on every large item (`uniform_a11y`'s single 3-bool match replaces a
3-load branch chain); worst case `er_40` **+0.02%**, inside the same ≤0.03% band the node half accepted.

`sequence_20` / `er_40` are ~1.000 because their edges are **labeled**, so they never reach this gate — the
labeled-edge fast fragment is still a11y-full-gated (follow-up below).

### Wall-clock corroboration (NOT the claim)

Same core (`taskset -c 9`), 3 rounds, median-of-rounds. `lean_cv_pct` ran **9.5–24.8%**, which does **not**
meet this project's `cv_pct < 5` bar, so this is reported as corroboration only:
lean speedup **geomean 1.171×**, max **1.444×** (`wide_16x32`); default cand/base geomean 0.9958×.

**The paradox is inverted.** `lean ÷ default` wall time — lean is now the *faster* profile on 10 of 13 items:

| item | before bd-b2b6 | after node half | after this (edge half) |
|---|---:|---:|---:|
| wide_8x16 | 2.07× | 1.66× | 1.29× |
| wide_16x32 | 1.82× | 1.40× | **0.95×** |
| wide_12x24 | — | 1.32× | **0.94×** |
| dense_dag_200 | — | 1.38× | **0.97×** |
| flowchart_large_500 | 1.89× | 1.20× | **0.93×** |
| edit_trace_60x20 | 1.21× | 1.03× | **0.90×** |

Lean output is 1.47–6.13× smaller than mermaid's and now also cheaper than our own default profile to produce.

## Follow-up profile (same session): why `wide_8x16` lean is still 1.29×

A symbolized `perf record` on `wide_8x16` under both profiles found frames present **only under lean**:
`is_contained_in` 6.96%, `StrSearcher` 4.93%, `replace_range` 1.41%, memmove 7.00% (vs 2.85% default). Cause:
`strip_unused_state_css` early-returns on `svg.len() > POST_PASS_MAX_SVG_BYTES` (100 KB), then full-scans the
document ~20 times. The lean profile shrinks output ~31%, dragging `wide_8x16` (134,629 B → 93,077 B) and
`cyclic_scc_100` (107,649 B → 73,916 B) *below* the cap, so they pay a pass their default counterparts skip.
Proven with a cap-disabled diagnostic build: `wide_8x16` 1.330× → **0.945×**, `cyclic_scc_100` 1.092× →
**0.978×**, control `wide_12x24` 0.937× → 0.937× unchanged. **Not an a11y cost.** Filed as `bd-w5sn`; full
write-up in `docs/PROPOSAL_default_output_profile.md`.

## What is left (follow-ups, not this lever)

1. **Labeled edges.** The whole-labeled-edge fast fragment (`<g><path/><rect/><text/><title/></g>`) is still
   gated on full a11y, so `sequence_20` / `er_40` / sankey-style label-heavy diagrams get nothing from this.
   Same const-generic move; note the lean labeled edge keeps its `<g id … class="fm-edge-labeled">` wrapper
   (the group is structural, not a11y) but drops `role`/`tabindex`/`<title>`.
2. **Back-edges / dashed-with-inline-style / animated** still fall to `Element` under both profiles.
3. **Class / requirement compartment gates** (`bd-1dj4`) remain a11y-gated.
4. `render_edge`'s inner `<path>` fast path (gate 2) stays `text_alternatives`-gated. It is reachable only for
   *mixed* a11y, and a `raw_svg` Element cannot take `.id()` — leave it.

## Do-not-retry

- Do **not** use a runtime a11y flag inside the fragment writer (measured +0.1…0.33% default instructions on
  the node path; same writer shape here).
- Do **not** widen the gate from `uniform_a11y(..).is_some()` to "any a11y flag": mixed configs have no raw
  fragment shape. `mixed_a11y_edge_falls_back_to_slow_path` pins this.
- Do **not** relax `render_edge`'s own gate 1 to lean. It is what keeps a live slow path for the parity test to
  compare against; making the fast path universal would turn that test back into a tautology.
- Do **not** attempt a wall-clock-only A/B: `lean_cv_pct` is 9.5–24.8% on this box and the default-path effect
  is ~0.1%. Use `FM_H2H_FORCE_PROFILE` + `perf stat -e instructions:u`.
- The corpus `reps`/`warmup` keys in `scripts/headtohead/corpus.mjs` are `reps_rs` / `warmup_rs`. A hand-built
  `corpus.json` that copies `i.reps` silently drops the field and the harness exits 2 with an empty stdout.
- rch prunes a sibling `CARGO_TARGET_DIR` when it builds into another one: **copy both A/B binaries out of the
  target dirs before comparing.**
