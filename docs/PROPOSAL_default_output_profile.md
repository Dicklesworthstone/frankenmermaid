# Proposal: should `A11yConfig::full()` stay the default output profile?

**Status:** OPEN — decision belongs to the owner. Nothing in this document has been applied.
**Author:** `cc_fm` · **Written** 2026-07-10 at `bc56f72` · **Re-measured** 2026-07-10 at `0f9efd4` (post-`bd-w5sn`)
**Supersedes the perf half of:** `.benchmarks/edge_a11y_is_19pct_render_MEASURED.md`,
`.benchmarks/BLOCKER_render_perf_double_gated.md`
**Does not supersede:** `.benchmarks/a11y_contract_SETTLED_mermaid_emits_none.md` (see "What changed and what didn't")

---

## TL;DR

The **performance argument is now dead on both sides.** Until today, choosing the lean profile cost you
1.5–2.02× render — that was a real reason to keep `full()`. After `2288c78` (nodes), `bc56f72` (edges) and
`0f9efd4` (single-pass CSS post-pass), lean is **cheaper than default on 9 of 13 corpus items** and **27.7%
smaller**. So the choice is no longer "accessibility vs speed". It is now purely **"is per-element
accessibility worth 27.7% of output bytes?"** — a product question, not an engineering one.

Concretely: **keeping per-element a11y in the default now costs 1.55% of pipeline instructions** (geomean).

**My recommendation: keep `A11yConfig::full()` as the default.** Reasons below. But the counter-case is real
and I have laid it out fairly, because I do not think this is my call.

---

## The evidence pack (four independent pillars)

Everything below is measured, landed, and reproducible from the repo. Nothing is projected.

| # | Evidence | Where | What it establishes |
|---|---|---|---|
| 1 | **Upstream mermaid emits ZERO per-element a11y.** On `wide_8x16` (128 nodes / 224 edges) mermaid 11.15.0 emits `role=` ×1 (root only), `tabindex` ×0, `aria-label` ×0, per-element `<title>` ×0, `<desc>` ×0. We emit 353 / 352 / 128 / 353. | `.benchmarks/a11y_contract_SETTLED_mermaid_emits_none.md` · `a5dee40` | The lean profile is **contract-matching**, not contract-breaking. No consumer that works against mermaid's own output can depend on our per-element a11y. |
| 2 | **Lean edge-fragment streaming: −8…26% instructions.** Const-generic `A11Y` on the whole-edge writer; lean instr 0.7359–1.0004× vs base, default neutral (0.9984–1.0002×). 26/26 corpus SHA-256 identical under both profiles. | `.benchmarks/lean_edge_streaming_a11y_const_generic.md` · `bc56f72` | The 1.5–2.02× "lean is slower" penalty was a **renderer bug**, now fixed. Lean stopped being mispriced. |
| 3 | **Single-pass CSS post-pass: −7.6…−10.9% render, byte-identical.** 21 full-document `str::contains` scans → 2 `memmem` walks. Instruction A/B 0.8547–0.9442× on the **default** profile, with a code-layout control at 1.0000–1.0003× and two null controls at exactly 1.0000×. | `.benchmarks/postpass_single_pass_scan.md` · `0f9efd4` | A **default-profile** win, independent of this decision. It also halved the artefact in pillar 4. |
| 4 | **The byte-size post-pass gate.** `strip_unused_state_css` and three sibling passes early-return above `POST_PASS_MAX_SVG_BYTES = 100_000`. That gate measures **output bytes** — a quantity the *output profile* controls. Lean's ~31% shrink drags mid-size diagrams *below* the cap, so lean pays a pass default skips. | this document, "The ⚠ rows" | The two rows where lean looks slower are **an artefact of a size heuristic, not a cost of accessibility.** Disabling the pass collapses them to 0.945× / 0.978×. |

Read together: pillar 1 removes the compatibility objection to lean, pillars 2–3 remove the performance
objection to lean, and pillar 4 explains away the only rows that still look bad for lean. **The decision is
therefore no longer technical.** It is a values call about whether per-element accessibility belongs in the
default output, and that is why I am presenting it rather than making it.

---

## What changed and what didn't

**Did not change:** the comparator finding. Mermaid 11.15.0 emits **zero per-element accessibility** on
`wide_8x16` (128 nodes / 224 edges): `role=` ×1 (root only), `tabindex` ×0, `aria-label` ×0, per-element
`<title>` ×0, `<desc>` ×0. We emit 353 / 352 / 128 / 353. That is still true, and it means the lean profile is
**contract-matching against upstream mermaid**, not contract-breaking. Adopting lean would break no consumer
that today works against mermaid's own output.

**Did change:** the cost of choosing lean. Previously `A11yConfig::none()` fell off every streaming fast path,
so the *smaller* SVG cost ~2× the render. Both halves are now streamed.

---

## The numbers (re-measured at `0f9efd4`, this box, 13-item pinned corpus)

Instruction ratio is `perf stat -e instructions:u`, two-point delta (reps 36−6), `FM_H2H_FORCE_PROFILE` pinning
both harness passes, `batch=1`, core-pinned. **These are full-pipeline (parse+layout+render) ratios**, so the
render-only effect is strictly larger than shown. Wall clock is *not* reported: on this box it runs `cv_pct`
9.5–27.7%, far above the <5% bar. (When it was measurable, earlier this session, it tracked the instruction
ratio to within ~2% on every row — which is why the instruction number is the one to trust.)

Output bytes are unchanged by any of the three landings (all byte-identical).

| item | lean/default instr | bytes default | bytes lean | byte Δ | post-pass runs? |
|---|---:|---:|---:|---:|---|
| flowchart_small_10 | **0.913×** | 13,218 | 10,020 | −24.2% | both |
| flowchart_medium_100 | **0.939×** | 72,575 | 49,934 | −31.2% | both |
| flowchart_large_500 | **0.929×** | 343,946 | 232,778 | −32.3% | neither |
| wide_8x16 | 1.201× ⚠ | 134,629 | 93,077 | −30.9% | **lean only** |
| wide_12x24 | **0.937×** | 299,617 | 208,934 | −30.3% | neither |
| wide_16x32 | **0.935×** | 534,365 | 370,609 | −30.6% | neither |
| dense_dag_200 | **0.953×** | 355,447 | 246,485 | −30.7% | neither |
| cyclic_scc_100 | 1.055× ⚠ | 107,649 | 73,916 | −31.3% | **lean only** |
| sequence_20 | **0.974×** | 43,562 | 36,946 | −15.2% | both |
| class_50 | 1.068× † | 45,992 | 34,990 | −23.9% | both |
| state_40 | **0.931×** | 35,447 | 24,590 | −30.6% | both |
| er_40 | 1.073× † | 52,760 | 44,781 | −15.1% | both |
| edit_trace_60x20 | **0.935×** | 1,047,238 | 728,461 | −30.4% | both |
| **geomean** | **0.9847×** | — | — | **−27.7%** | |

**Lean is cheaper on 9 of 13.** Inverting the geomean: **per-element a11y costs the default profile 1.55% of
pipeline instructions.**

⚠ `wide_8x16` and `cyclic_scc_100` are **not** an a11y cost. Both sit astride the 100 KB output-size gate on the
render post-passes: the default output is *above* the cap (pass skipped) and the lean output is *below* it (pass
runs). So lean pays a pass default never runs. `bd-w5sn` (`0f9efd4`) cut that pass's cost and these rows moved
1.330×→**1.201×** and 1.092×→**1.055×** accordingly; the residue is the `replace_range`/`format!` work still in
it plus the other three post-passes. See "The ⚠ rows" below.

† `class_50` and `er_40` are below the cap in **both** profiles, so both pay the post-pass. They got *slightly
worse* as a ratio after `bd-w5sn` (1.047→1.068, 1.044→1.073) for an unsurprising reason: the single-pass scan
saved more absolute work on the **larger** (default) document than on the smaller lean one. Their residue is the
class-compartment gate (`bd-1dj4`) and the labeled-edge gate (`bd-u63b`), not a11y.

**Against mermaid** (`wide_8x16`): mermaid 292,024 B. Our default 134,629 B = **2.17× smaller**. Our lean
93,077 B = **3.14× smaller**. We are ~2174× faster in either profile.

---

## The case FOR flipping the default to lean

1. **Contract-matching.** Upstream mermaid emits no per-element a11y. Nobody's tooling can depend on ours.
2. **27.7% fewer bytes**, geomean, across the corpus. For the WASM/browser bundle these bytes cross the wire
   and get parsed by the DOM on every re-render.
3. **It is now also faster** (geomean 0.9847× instructions; 9/13 items cheaper), so the historical objection
   ("smaller output costs 2× render") no longer applies.
4. `id=` and `data-fm-edge-id` / `data-id` **survive under lean** — only `role`, `tabindex`, `aria-label` and
   per-element `<title>` are dropped. Tooling that hooks elements by id is unaffected.
5. Root `<title>`/`<desc>` are a **separate knob** (`SvgRenderConfig::accessible`, lib.rs:866/2222), so
   document-level accessibility survives a flip of `a11y` alone.

## The case AGAINST (why I recommend keeping `full()`)

1. **It is a real feature, and it is ours.** Per-element `role="graphics-symbol"` + `aria-label` + `<title>` is
   what lets a screen reader read a diagram node-by-node, and `tabindex="0"` is what lets a keyboard user walk
   it. Mermaid not shipping this is an argument that **we are better**, not that we should stop.
2. **We already win on bytes with a11y on.** 2.17× smaller than mermaid *including* the 41,552 B of
   per-element a11y. We are not paying a competitive price for it.
3. **README states it as a project value:** "Clean, inspectable output — accessibility and metadata are part of
   correctness"; the HighContrast theme is advertised as WCAG AAA. Silently dropping a11y from the default
   contradicts a documented design principle.
4. **Cost of the flip is not zero:** 37 goldens re-blessed; any downstream consumer of `role`/`<title>` breaks;
   and the `accessible = true` / `a11y = full()` pairing in `SvgRenderConfig::default()` becomes incoherent
   (root `<desc>` describing a diagram whose elements are unlabelled).
5. **The perf argument that motivated the flip is gone.** Keeping a11y now costs geomean **1.55%** of pipeline
   instructions (`1/0.9847`), and on the largest items 5–7%. That is a cheap price for the feature.

## Recommendation

**Keep `A11yConfig::full()` as the `SvgRenderConfig::default()`.** The reason to flip was that lean was
mispriced; it was mispriced because of a renderer bug, and the bug is fixed. Accessibility now costs ~1.6% of
the pipeline geomean, and we remain 2.17× smaller and ~2174× faster than mermaid with it on.

**If the owner wants the bytes**, the surgical move is to default the **WASM / browser bundle** to lean (where
bytes cross the wire and the consumer is a DOM, not a screen reader hooked to a CLI artefact) while the CLI and
library keep `full()`. That captures the −27.7% where it is worth money and keeps the feature where it is worth
correctness. This is a two-line change in the wasm surface's config construction and needs no golden re-bless,
because the goldens render through the CLI/library default.

**Do not** adopt the older `edge_a11y_is_19pct_render_MEASURED.md` recommendation (uniform bare edges in *both*
profiles). It trades the feature away permanently to buy render time on a path that is already three orders of
magnitude ahead of upstream.

---

## The two ⚠ rows: a byte-size gate the lean profile silently inverts

Profiling `wide_8x16` under both profiles (symbolized release, `perf record --call-graph=dwarf`, self-time
frames ≥0.1%) showed frames present **only** in lean: `<&str as Pattern>::is_contained_in` **6.96%**,
`StrSearcher` **4.93%**, `String::replace_range` 1.41%, and `__memmove_avx_unaligned_erms` **7.00% vs default's
2.85%**.

Cause: `strip_unused_state_css` (lib.rs:355) begins with

```rust
if svg.len() > POST_PASS_MAX_SVG_BYTES { return; }   // = 100_000
```

It then full-scans the document ~20 times (5 state classes + 8 `fm-node-accent-N` + 8 `var(--fm-accent-N)`),
and `str::contains` on an **absent** needle scans the entire document — the common case. The cap exists to
"skip the 200 KB+ chain / wide renders", but it measures **output bytes**, and the lean profile shrinks output
by ~31%. So `wide_8x16` (default 134,629 B → skips; lean 93,077 B → **runs**) and `cyclic_scc_100`
(107,649 B → skips; 73,916 B → **runs**) pay a pass their default counterparts never see.

**The mechanism is proven, not inferred.** A diagnostic build with the cap set to 0 (pass disabled for both
profiles), same A/B protocol:

| item | default bytes | lean bytes | lean/default @`bc56f72` | @`0f9efd4` (after `bd-w5sn`) | pass disabled entirely |
|---|---:|---:|---:|---:|---:|
| wide_8x16 | 134,629 | 93,077 | 1.330× | **1.201×** | **0.945×** |
| cyclic_scc_100 | 107,649 | 73,916 | 1.092× | **1.055×** | **0.978×** |
| wide_12x24 *(control: both skip)* | 299,617 | 208,934 | 0.937× | 0.937× | 0.937× |

The control is unchanged; the two straddling items collapse into the 0.93–0.98× band of their siblings once the
pass is off. The remaining 0.978× on `cyclic_scc_100` is its 20 back-edges, which slow-path in both profiles.

**`bd-w5sn` has since landed** (`0f9efd4`): the ~21 full-document scans became 2 `memmem` walks, byte-identical,
worth **−7.6% / −10.9% render** on `small_10` / `medium_100` by same-worker criterion. That closed roughly half
the gap on the two ⚠ rows. What is left inside the pass is `String::replace_range` (a whole-tail memmove per
strip) and the `format!` needles, plus the other three post-passes — all still gated on the same byte-size cap.

**This was a lever, not a blocker for the decision above.** Note what was *not* done: gating on a
profile-invariant proxy (node+edge count) instead of output bytes would change which diagrams get their CSS
stripped, hence output bytes, hence 37 goldens — that is a separate contract decision and belongs to the owner
too.

`class_50` (1.068×) and `er_40` (1.073×) are below the cap in **both** profiles, so they are not affected by
this; their residue is the class-compartment gate (`bd-1dj4`) and the labeled-edge gate (`bd-u63b`).

## Did the large-diagram double-copy frame move?

**No — and it is now visibly distinct from the post-pass memmove.** Under the default profile at `wide_8x16`,
`__memmove_avx_unaligned_erms` sits at **2.85%** self-time, consistent with the structural
`raw_svg → doc → final String` double copy already ledgered NO-SHIP in `41948f2`
(`.benchmarks/render_memmove_is_structural_doublecopy` / the `render_large_raw_part_body_fusion_NEGATIVE`
entry). The 7.00% seen under lean is **not** that copy; it is `replace_range` shifting the document tail inside
the post-pass. The double-copy frame is unchanged by the streaming work, exactly as that ledger entry predicts.
The top render frame under default is now `write_common_node_fragment_into::<true>` (3.73%) — i.e. the
streaming writer itself, which is where the time *should* be.
