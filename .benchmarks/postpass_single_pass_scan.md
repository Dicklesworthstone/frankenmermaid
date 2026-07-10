# WIN: `strip_unused_state_css` — 21 full-document scans collapsed to 2 (bd-w5sn)

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** `e6e0362`
**File scope:** `crates/fm-render-svg/src/lib.rs`, `strip_unused_state_css` + two new private scanners.
**Deliberately NOT touched (one lever):** the `replace_range` strips, the `format!` needles, the 100 KB cap,
and the other three post-passes (`strip_unused_markers` / `strip_dead_marker_css` / `minify_style_block`).

## Ledger-grep FIRST — this path has a standing do-not-retry

`docs/NEGATIVE_EVIDENCE.md` records `strip-theme-css-memmem-replace-range-RE-REJECTED` (2026-07-03): a
`strip_unused_*` micro-opt was landed (`5162ad0`) and reverted (`65033ad`) after being rejected once before
(`5b5e709`). The render-CSS post-pass path has a documented **~5% CODE-LAYOUT-NOISE floor** — byte-identical
edits re-lay-out the render's hot code and swing **wall time** ±5% *independent of the edit's logic*, so the
sign of a best-of-N wall A/B here is roulette.

Its recorded **retry-condition is explicit**: *"a kept win here needs >5% AND a code-layout control (does an
unrelated no-op edit to the same fn swing similarly?)"*. Both are satisfied below. Also note what this lever is
**not**: it is not "memmem beats `str::contains`" (that claim is rejected and stays rejected). It is
**21 scans → 2 scans**. The per-scan primitive is unchanged in kind.

## Profile → mechanism (the frame, not a guess)

Symbolized release (`--config profile.release.strip=false`), `perf record --call-graph=dwarf`, `wide_8x16`.
Self-time frames ≥0.1%, attributed by folded callchain:

| frame | self-time | maps to |
|---|---:|---|
| `<&str as Pattern>::is_contained_in` → `simd_contains` | **6.96%** | the per-needle `contains` chain |
| `<StrSearcher as Searcher>::next_match` → `TwoWaySearcher` | **4.93%** | `find(&String)` selectors/decls |
| `String::replace_range` | 1.41% | the strips (untouched) |
| `__memmove_avx_unaligned_erms` | 7.00% | `replace_range` tail shifts |

**Isolated ceiling.** An env-gated diagnostic build that disables *only* `strip_unused_state_css` (the other
three post-passes left on) prices the whole function, two-point instruction delta:

| item (default profile) | pass ON | pass OFF | pass costs |
|---|---:|---:|---:|
| flowchart_small_10 | 60,595,316 | 54,554,202 | **11.1%** of pipeline |
| flowchart_medium_100 | 229,124,298 | 202,793,535 | **13.0%** |
| sequence_20 | 133,472,177 | 112,179,980 | **19.0%** |
| class_50 | 178,945,097 | 161,829,524 | **10.6%** |
| state_40 | 136,635,374 | 121,893,580 | **12.1%** |
| er_40 | 118,055,682 | 94,219,478 | **25.3%** |
| wide_8x16 (lean) | 245,001,190 | 208,158,102 | **17.7%** |

Root cause: `str::contains` on an **absent** needle scans the entire body, and absent is the common case (a
typical flowchart carries no state class at all). 5 state classes + 8 accent classes over the body + 8
`var(--fm-accent-N)` over the whole document = worst-case `O(len * 21)`.

## The lever

All 13 body needles share the prefix `fm-node-`; the 8 var needles share `var(--fm-accent-`. Neither prefix
self-overlaps (no proper prefix is also a proper suffix), so a single **non-overlapping** `memmem` walk
observes the start of every occurrence of every needle. Two new private fns:

- `scan_body_fm_node_classes(body) -> (any_state_used, [bool; 9])` — one walk, reads the suffix at each hit.
- `scan_accent_var_refs(svg) -> [bool; 9]` — one walk, run *after* the accent-rule strips (that reference
  count is the point of the check).

Three byte-identity subtleties, each pinned by the differential test:

1. The accent needle `fm-node-accent-{n}` had **no terminator**, so `fm-node-accent-12` marked accent 1 used —
   the scanner reads the digit with no terminator check, preserving that.
2. The var needle `var(--fm-accent-{n})` **did** have a terminator, so `var(--fm-accent-12)` must NOT mark
   accent 1 referenced — the scanner requires the `)`.
3. A state class matched anywhere (`fm-node-inactive-foo` counted) — `starts_with(suffix)` accepts exactly the
   same set.
4. The body flags may be computed *before* the inactive-region strip because that strip only edits bytes inside
   `<style>`; the body text is unchanged. (The old code re-`find`s `</style>` afterwards and reads the same
   body — same invariant.) And a single `var` scan after the accent loop is equivalent to the old per-`n`
   scans inside the declaration loop, because an accent **declaration never contains a `var(`** (verified in
   `theme.rs`), so no iteration can change a later iteration's answer.

## Behaviour parity

- **Differential oracle, now a permanent unit test** (`single_pass_scanners_match_per_needle_contains`):
  the verbatim pre-`bd-w5sn` per-needle `contains` logic vs the new scanners over **200,000 generated strings**
  from an alphabet built to manufacture the failure modes (`fm-nod`, `fm-node-accent-12`, `fm-node-accent-0/9`,
  `var(--fm-accent-12)`, `fm-node-fm-node-accent-3`, `fm-node-inactive-foo`) plus 14 targeted cases. All agree.
- **26/26 SHA-256** across the 13-item pinned corpus under **both** output profiles, vs a pristine `830d672`
  build. ⚠️ *Caveat, stated precisely:* this dump was produced by a build made **before** a whitespace-only
  `cargo fmt` reflow of 3 lines in the new code (`cargo fmt --check` located exactly lines 445 / 10287 / 10299,
  all inside code added by this commit). rustfmt does not alter tokens. Under the concurrent disk emergency
  local builds were prohibited and `rch` does not retrieve example binaries, so the dump was not regenerated
  from the post-fmt source; every *test* below did run on the post-fmt source.
- `cargo test -p fm-render-svg --lib` (remote): **247 passed** (246 before; +1).
- `frankentui_conformance_test`: green. `golden_layout_test`: 2 passed.
- `golden_svg_test`: 1 pass / 1 fail (`gantt_basic` FNV mismatch) — **pre-existing**, reproduced at untouched
  `830d672` in a detached worktree earlier this session.
- `cargo clippy -p fm-render-svg --all-targets -- -D warnings` clean; `cargo fmt --check` clean;
  `ubs` 14 criticals = HEAD's 14 (the `ch == '-'` false positive).

## Measurement

### A. Instruction A/B + the code-layout control (the claim)

`perf stat -e instructions:u`, two-point delta (`reps=36` − `reps=6`, `warmup=2`), `FM_H2H_FORCE_PROFILE`
pinning both harness passes and forcing `batch=1`, `taskset -c 7`, median of 3, all three binaries copied out
of their target dirs first. **ORIG** = `e6e0362`. **noop** = ORIG + a work-identical no-op (`if svg.is_empty()
{ return; }`) inside the same function — the ledger-mandated code-layout control.

| item | profile | ORIG delta | **noop/ORIG (layout floor)** | **lever/ORIG** |
|---|---|---:|---:|---:|
| flowchart_small_10 | default | 60,542,814 | 1.0003× | **0.9393×** |
| flowchart_medium_100 | default | 229,104,364 | 1.0001× | **0.9310×** |
| sequence_20 | default | 133,429,937 | 1.0002× | **0.8822×** |
| class_50 | default | 178,908,321 | 1.0001× | **0.9442×** |
| state_40 | default | 136,596,038 | 1.0002× | **0.9321×** |
| er_40 | default | 118,017,314 | 1.0002× | **0.8547×** |
| wide_8x16 | lean | 245,009,416 | 1.0001× | **0.9030×** |
| *cyclic_scc_100 (107,649 B > cap)* | default | 535,137,371 | 1.0000× | **1.0000×** |
| *wide_16x32 (534,365 B > cap)* | default | 639,282,696 | 1.0000× | **1.0000×** |

**5.6 – 14.5% fewer pipeline instructions**, on the **DEFAULT** profile. Two things make this causal rather
than correlational:

- **The code-layout control is 1.0000–1.0003×.** The documented ±5% floor is a **wall-time** artifact
  (icache/alignment); *instruction counts are immune to it*. That is why this metric, not wall clock, is the
  claim — and it retires the "sign is layout roulette" objection for this path.
- **The two null controls are exactly 1.0000×.** `cyclic_scc_100` and `wide_16x32` exceed the 100 KB cap, so
  the function early-returns and never runs. If this were layout noise they would move. They do not move at all.

Captured **53–63% of the isolated ceiling** above; the remainder is the `replace_range` / `format!` / `find`
work and the two surviving passes, deliberately left alone (one lever per commit).

### B. Criterion, render stage, same worker (the confirmation)

`rch exec` with `RCH_WORKER=hz1 RCH_REQUIRE_REMOTE=1 RCH_FORCE_REMOTE=1`, `cargo bench -p frankenmermaid-cli
--bench pipeline_bench -- render_svg/flowchart --measurement-time 8 --warm-up-time 2 --sample-size 30 --noplot`.
Both arms on the **same** worker; ORIG obtained by `git show HEAD:<file> > <file>` with the lever restored from
a `cp` backup immediately after (sha256 verified equal).

| bench | output bytes | post-pass | ORIG | lever | speedup |
|---|---:|---|---|---|---:|
| `render_svg/flowchart/small_10` | 13,218 | **runs** | 57.955 µs [56.814, 59.043] | 53.544 µs [52.912, 54.276] | **1.082×** (−7.6%) |
| `render_svg/flowchart/medium_100` | 72,575 | **runs** | 112.82 µs [110.55, 115.77] | 100.53 µs [99.438, 101.78] | **1.122×** (−10.9%) |
| `render_svg/flowchart/large_500` | 343,946 | *skipped* | 181.31 µs [179.01, 184.29] | 180.67 µs [178.90, 183.21] | 1.004× (flat) |

Confidence intervals do not overlap on either pass-running row. Dispersion, reported as criterion's relative CI
half-width (criterion does not print `cv_pct`): ORIG ≤ **2.31%**, lever ≤ **1.27%** — inside the <5% bar.
`large_500` is a null control *within criterion itself*: its output exceeds the cap, the pass never runs, and it
is flat. Both pass-running rows clear the ledger's **>5%** retry-condition.

**Local wall clock could not be used and was not used for the claim:** on this box (11 concurrent cargo builds
during a disk emergency) the harness reported `cv_pct` **11.9–27.7%**, and the >100 KB null controls swung ±3%.
That is exactly the roulette the ledger warns about.

## Do-not-retry / notes for the next agent

- The ±5% code-layout-noise floor on `strip_unused_*` is **wall-time only**. Measured instruction floor on the
  same function: **0.03%**. Use `perf stat -e instructions:u` here, and always build the no-op control.
- This is **not** a "memmem beats `str::contains`" win — that remains rejected (`044f531` note). The win is
  21 scans → 2. Do not generalize it into re-attempting memmem swaps on `strip_unused_theme_css`.
- Do **not** raise or remove the 100 KB cap to "fix" the profile-dependence of the gate: that changes which
  diagrams get their CSS stripped ⇒ output bytes change ⇒ 37 goldens re-blessed + a contract decision.
- The remaining `strip_unused_state_css` cost is `replace_range` (a whole-tail memmove per strip) + the
  `format!` needles. `strip_unused_markers` already uses the right shape (collect ranges, one O(n) rebuild).
- `rch` does not retrieve `--example` binaries; `cargo test` runs entirely remote. Plan byte-identity dumps
  around that, or do them before any cosmetic reformat.
