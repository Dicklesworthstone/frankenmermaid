# Proposal: should `A11yConfig::full()` stay the default output profile?

**Status:** OPEN — decision belongs to the owner. Nothing in this document has been applied.
**Author:** `cc_fm`, 2026-07-10 · **Prepared at:** `bc56f72`
**Supersedes the perf half of:** `.benchmarks/edge_a11y_is_19pct_render_MEASURED.md`,
`.benchmarks/BLOCKER_render_perf_double_gated.md`
**Does not supersede:** `.benchmarks/a11y_contract_SETTLED_mermaid_emits_none.md` (see "What changed and what didn't")

---

## TL;DR

The **performance argument is now dead on both sides.** Until today, choosing the lean profile cost you
1.5–2.02× render — that was a real reason to keep `full()`. After `2288c78` (nodes) and `bc56f72` (edges), lean
is **cheaper than default on 9 of 13 corpus items** and 27.7% smaller. So the choice is no longer
"accessibility vs speed". It is now purely **"is per-element accessibility worth 27.7% of output bytes?"** —
a product question, not an engineering one.

**My recommendation: keep `A11yConfig::full()` as the default.** Reasons below. But the counter-case is real
and I have laid it out fairly, because I do not think this is my call.

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

## The numbers (all measured at `bc56f72`, this box, 13-item pinned corpus)

Instruction ratio is `perf stat -e instructions:u`, two-point delta, `FM_H2H_FORCE_PROFILE` pinning both
harness passes, `batch=1`, core-pinned, median of 3. **These are full-pipeline (parse+layout+render) ratios**,
so the render-only effect is strictly larger than shown. Wall clock is median-of-3-rounds on a pinned core and
is corroboration only (`cv_pct` 9.5–24.8%, above the <5% bar) — note it tracks the instruction ratio to
within ~2% on every row, which is itself the reason to trust the instruction number.

| item | lean/default instr | lean/default wall | bytes default | bytes lean | byte Δ |
|---|---:|---:|---:|---:|---:|
| flowchart_small_10 | **0.904×** | 0.90× | 13,218 | 10,020 | −24.2% |
| flowchart_medium_100 | **0.913×** | 0.92× | 72,575 | 49,934 | −31.2% |
| flowchart_large_500 | **0.929×** | 0.93× | 343,946 | 232,778 | −32.3% |
| wide_8x16 | 1.330× ⚠ | 1.29× | 134,629 | 93,077 | −30.9% |
| wide_12x24 | **0.937×** | 0.94× | 299,617 | 208,934 | −30.3% |
| wide_16x32 | **0.935×** | 0.95× | 534,365 | 370,609 | −30.6% |
| dense_dag_200 | **0.953×** | 0.97× | 355,447 | 246,485 | −30.7% |
| cyclic_scc_100 | 1.092× ⚠ | 1.07× | 107,649 | 73,916 | −31.3% |
| sequence_20 | **0.962×** | 0.94× | 43,562 | 36,946 | −15.2% |
| class_50 | 1.047× | 1.11× | 45,992 | 34,990 | −23.9% |
| state_40 | **0.913×** | 0.91× | 35,447 | 24,590 | −30.6% |
| er_40 | 1.044× | 1.09× | 52,760 | 44,781 | −15.1% |
| edit_trace_60x20 | **0.912×** | 0.90× | 1,047,238 | 728,461 | −30.4% |
| **geomean** | **0.984×** | 0.99× | — | — | **−27.7%** |

⚠ `wide_8x16` and `cyclic_scc_100` are **not** an a11y cost. They are an artefact of a byte-size-gated render
post-pass; see "The two ⚠ rows" below. With that pass disabled they fall to 0.945× and 0.978×.

**Against mermaid** (`wide_8x16`): mermaid 292,024 B. Our default 134,629 B = **2.17× smaller**. Our lean
93,077 B = **3.14× smaller**. We are ~2174× faster in either profile.

---

## The case FOR flipping the default to lean

1. **Contract-matching.** Upstream mermaid emits no per-element a11y. Nobody's tooling can depend on ours.
2. **27.7% fewer bytes**, geomean, across the corpus. For the WASM/browser bundle these bytes cross the wire
   and get parsed by the DOM on every re-render.
3. **It is now also faster** (geomean 0.984× instructions; 9/13 items cheaper), so the historical objection
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
5. **The perf argument that motivated the flip is gone.** Keeping a11y now costs geomean **1.6%** of pipeline
   instructions (`1/0.984`), and on the largest items 5–7%. That is a cheap price for the feature.

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

| item | default bytes | lean bytes | lean/default (shipped) | lean/default (pass disabled) |
|---|---:|---:|---:|---:|
| wide_8x16 | 134,629 | 93,077 | 1.330× | **0.945×** |
| cyclic_scc_100 | 107,649 | 73,916 | 1.092× | **0.978×** |
| wide_12x24 *(control: both skip)* | 299,617 | 208,934 | 0.937× | 0.937× |

The control is unchanged; the two straddling items collapse into the 0.93–0.98× band of their siblings. The
remaining 0.978× on `cyclic_scc_100` is its 20 back-edges, which slow-path in both profiles.

**This is a lever, not a blocker for the decision above** — filed as a bead. The fix is to make the pass
single-scan (all ~20 needles share the prefixes `fm-node-` and `var(--fm-accent-`, so one `memmem` traversal
each collects every hit), which is byte-identical, removes the O(n·k), and **also speeds up the default
profile** on every diagram under 100 KB (`flowchart_small_10`, `medium_100`, `sequence_20`, `class_50`,
`state_40`, `er_40`, and every revision of `edit_trace`). Gating on a profile-invariant proxy instead (node+edge
count) would change output bytes and is therefore a separate contract decision.

`class_50` (1.047×) and `er_40` (1.044×) are below the cap in **both** profiles, so they are not affected by
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
