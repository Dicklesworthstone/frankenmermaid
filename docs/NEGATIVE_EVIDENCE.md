# Negative Evidence Ledger — frankenmermaid perf swarm

> **Purpose.** Record perf levers that were tried and **reverted** (zero-gain, regression,
> or correctness/determinism risk) so the swarm does not waste cycles re-attempting them.
> Every entry must cite a *measured* head-to-head (cargo bench, per-crate) and the commit/
> revert that removed it. Positive, kept wins live in `evidence/ledger/` and the README perf
> tables; this file is exclusively for **what did not work and why**.

## Measurement protocol (so entries are comparable)

- Build & bench **per-crate only** for the active `frankenmermaid-cod-a` lane:
  ```bash
  CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a \
    rch exec -- cargo bench -p <package> --bench <bench> --release
  ```
- Cod-b lane used the same per-crate rule with its requested target directory. On this
  Cargo toolchain, the release-profile bench flag is `--profile release`:
  ```bash
  CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b \
    rch exec -- cargo bench --profile release -p <package> --bench <bench> -- --warm-up-time 1 --measurement-time 2
  ```
- Package names (note: crate dir ≠ package name):
  | dir | package |
  |-----|---------|
  | `crates/fm-cli` | `frankenmermaid-cli` |
  | `crates/fm-layout` | `fm-layout` |
  | `crates/fm-parser` | `fm-parser` |
  | `crates/fm-render-svg` | `fm-render-svg` |
- Existing benches: `pipeline_bench` (fm-cli: parse/layout/render_svg/full_pipeline),
  `incremental_layout` & `crossing_minimization` (fm-layout).
- A lever is **kept** only if it shows a reproducible ≥3% improvement on at least one
  realistic-size case (medium_100 / large_500 / large_1000) with **no** regression elsewhere
  and conformance still GREEN. Otherwise it is reverted and logged here.
- A dominance claim is valid only when it records a head-to-head ratio against the original
  mermaid-js renderer: `frankenmermaid_time / mermaid_js_time` for the same input, render target,
  warmup policy, and output-validity gate.

## Entry template

```
### <short-name> — REVERTED (<date>)
- **Lever:** what was changed (crate, function, technique)
- **Hypothesis:** expected win and why
- **Baseline → After:** measured numbers (bench id, ns/op or µs/op, frankenmermaid delta)
- **Original comparator:** mermaid-js version/source, mermaid-js time, frankenmermaid/mermaid-js ratio
- **Verdict:** regression | ~0 gain | correctness/determinism risk
- **Revert:** commit SHA that removed it
- **Do-not-retry note:** the specific reason this approach is a dead end here
```

## Entries

### Document XML streaming + conditional edge-label CSS - KEPT after same-worker remote proof (2026-06-27)
- **Lever:** `fm-render-svg::SvgDocument::write_to_string` streams XML attribute/text
  escaping into the output writer, `Theme::to_svg_style` emits edge-label CSS only when
  the diagram has labeled edges, `truncate_label` returns borrowed labels when no
  truncation is needed, and the measured bench worktree lands the small follow-up cleanup
  (`escape_xml_text` test-only, no redundant `mut`, and the `Cow<str>` text call site).
- **Hypothesis:** the wide-stage split shows SVG render dominates the Mermaid-facing gap.
  Removing per-attribute escape `String` temporaries, unused edge-label CSS bytes, and
  unchanged-label clones should reduce the per-element render cost without changing layout
  or SVG semantics.
- **Baseline -> After:** same-worker `ovh-a`, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-a`, package `frankenmermaid-cli`,
  bench `pipeline_bench`, filter `wide_stages/render`. Parent baseline worktree
  `/data/projects/.worktrees/frankenmermaid-tansparrow-701b-baseline-0cd` at `0cd6248`
  measured `1.1764 ms`, `3.0633 ms`, and `6.9645 ms` for `8x16`, `12x24`, and
  `16x32`. Candidate measured `760.81 us`, `1.7421 ms`, and `3.1475 ms`. That is
  `35.33%`, `43.13%`, and `54.81%` faster.
- **Original comparator:** latest pinned live-CDP Mermaid `11.12.0` denominator reused from
  the current ledger for identical generated wide inputs: `315.14 ms`, `981.73 ms`, and
  `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** candidate render-stage means versus Mermaid.js
  denominators give ratios `0.002414x`, `0.001775x`, and `0.001093x`; Mermaid.js is
  `414.22x`, `563.53x`, and `914.75x` slower on the same wide input sizes. These are
  render-stage ratios against full-pipeline Mermaid denominators, so they are conservative
  dominance context rather than a replacement for the standing full-pipeline ratio.
- **Verdict:** kept. This supersedes earlier local-fallback routing evidence for this
  micro-family because the same-worker remote parent/candidate pair is the decision-grade
  measurement.
- **Validation:** focused `fm-render-svg` truncate-label tests passed locally; `cargo test
  --profile release -p frankenmermaid-cli --test frankentui_conformance_test` passed locally;
  `cargo check --profile release -p fm-render-svg --all-targets` and
  `cargo clippy --profile release -p fm-render-svg --all-targets -- -D warnings` passed
  locally after reapplying the lever on top of `9aaaa6f`.
- **Tooling note:** `cargo fmt --check` remains blocked by already-committed rustfmt drift in
  bench files and unrelated renderer helper files; this commit does not broaden into a
  repo-wide format sweep. Agent Mail file reservations failed because the mail SQLite
  database reported corruption. Two `rch` focused-test attempts selected `vmi1264463` and
  failed before tests because that worker lacks `cmake`, so the final tests ran locally.

### Document XML streaming + conditional edge-label CSS - REJECTED (2026-06-27)
- **Lever:** `fm-render-svg::SvgDocument::write_to_string` was changed to stream
  root width/height, title, description, and style escaping directly into the
  output buffer; `Theme::to_svg_style` also accepted a `has_edge_labels` flag so
  unlabeled wide graphs could skip the `.fm-edge-labeled` / `.edge-label` CSS.
  The same candidate also made label truncation return `Cow<'_, str>`.
- **Hypothesis:** the widest Mermaid-facing gap is still SVG render output.
  Removing document-level escaping temporaries, dead edge-label CSS, and a few
  unchanged-label allocations should reduce the dominant wide render stage
  without changing layout or SVG semantics.
- **Render-stage baseline -> after:** clean baseline worktree
  `/data/projects/.worktrees/frankenmermaid-cod-b-doc-escape-baseline-0cd6248`
  at `0cd6248`, warm target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`, package
  `frankenmermaid-cli`, bench `pipeline_bench`, filter `wide_stages/render`.
  Same-local `rch exec` fallback baseline means were `1.1142 ms`, `2.8794 ms`,
  and `7.5438 ms` for `8x16`, `12x24`, and `16x32`; candidate means were
  `1.1570 ms`, `2.9054 ms`, and `4.8644 ms`. The largest render stage improved
  `35.51%`, but the smaller cases were `3.84%` and `0.90%` slower/no-change.
- **Full-pipeline gate:** the Mermaid-facing `full_pipeline_wide` same-local
  fallback gate measured baseline `1.6898 ms`, `4.1700 ms`, and `7.6135 ms`
  versus candidate `1.6772 ms`, `4.3165 ms`, and `8.0876 ms`. That is `0.75%`
  faster, `3.51%` slower, and `6.23%` slower. The largest end-to-end case is the
  stop rule, so the lever was reverted even though isolated `16x32` rendering
  looked better.
- **Post-revert bench:** after the forward revert, `rch exec` had no admissible
  workers and fell back local with the requested target dir. The same
  `full_pipeline_wide` filter measured `1.2980 ms`, `3.8305 ms`, and `7.0666 ms`;
  versus Mermaid.js those are `0.004119x`, `0.003902x`, and `0.002454x`
  (Mermaid.js `242.79x`, `256.29x`, and `407.44x` slower). This is confirmation
  that the final committed code is back on the retained path, not evidence for
  keeping the rejected candidate.
- **Original comparator:** latest pinned live-CDP Mermaid `11.12.0` denominator
  reused from the current main ledger for identical generated wide inputs:
  `315.14 ms`, `981.73 ms`, and `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** retained full-pipeline baseline ratios were
  `0.005362x`, `0.004247x`, and `0.002644x` (Mermaid.js `186.49x`, `235.43x`,
  and `378.17x` slower). The rejected candidate ratios were `0.005322x`,
  `0.004397x`, and `0.002809x` (Mermaid.js only `187.89x`, `227.44x`, and
  `356.00x` slower), worsening the two larger dominance ratios.
- **Verdict:** regression on the end-to-end gate; production code was restored in
  a forward revert commit and only this negative evidence remains.
- **Revert:** manual `apply_patch` restored document escaping allocation,
  unconditional edge-label CSS, and `String` label truncation on top of main.
- **Do-not-retry note:** document-level streaming escape can win a substage while
  still losing the full Mermaid-facing pipeline. Do not retry this micro-family
  without an adjacent full-pipeline win on the same worker/fallback mode.
- **Tooling note:** Agent Mail registration and file reservations were unavailable
  because the project mail SQLite corruption circuit breaker is open. Literal
  `cargo bench --release` is invalid on this Cargo toolchain, so per-crate bench
  commands used `--profile release`.

### Theme CSS sub-writer append path - REJECTED (2026-06-27)
- **Lever:** `fm-render-svg::Theme::to_svg_style` was changed to call private
  `ThemeColors::write_css_vars` and `FontConfig::write_css` helpers that wrote
  directly into the final style `String`. The existing `to_css_vars()` and
  `to_css()` public helpers were kept behavior-compatible by delegating to the
  same writers.
- **Hypothesis:** after the kept direct-write of the large utility CSS template,
  the remaining CSS hot path still allocated two intermediate strings and several
  `format!` temporaries for theme variables and font CSS. Alien Graveyard /
  FrankenSuite guidance explicitly calls out `format!` on hot paths and simple
  concat as candidates for `write!` into a reused buffer.
- **Render-stage baseline -> after:** clean baseline worktree
  `/data/projects/.worktrees/frankenmermaid-tansparrow-theme-subwriters-baseline-20260627`
  at `02cad1d`, candidate worktree
  `/data/projects/.worktrees/frankenmermaid-tansparrow-theme-subwriters-20260627`,
  warm target dir `/data/projects/.rch-targets/frankenmermaid-cod-b`, package
  `frankenmermaid-cli`, bench `pipeline_bench`, filter `wide_stages/render`.
  Same-local `rch exec` fallback baseline means were `1.0935 ms`, `2.6047 ms`,
  and `4.9912 ms` for `8x16`, `12x24`, and `16x32`; candidate means were
  `1.2194 ms`, `2.9315 ms`, and `5.2374 ms`. That is `11.51%`, `12.55%`, and
  `4.93%` slower, with Criterion reporting significant regressions on `8x16`
  and `12x24` and no significant change on `16x32`.
- **Fresh full-pipeline follow-up:** this run repeated the Mermaid-facing
  `full_pipeline_wide` gate with the requested warm target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-a`. The same-local baseline
  through `rch exec` fallback measured `1.8083 ms`, `4.7523 ms`, and `8.3740 ms`;
  same-local candidate measured `1.6505 ms`, `4.2361 ms`, and `8.7357 ms`.
  That is `8.73%` faster, `10.86%` faster, and `4.32%` slower. A remote
  candidate-only `hz2` run measured `1.5106 ms`, `3.6487 ms`, and `6.6555 ms`,
  but it had no matching remote baseline and is routing context only.
- **Original comparator:** latest pinned live-CDP Mermaid `11.12.0` denominator
  reused from the current main ledger for identical generated wide inputs:
  `315.14 ms`, `981.73 ms`, and `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** the retained render-stage baseline means
  versus Mermaid.js denominators give ratios `0.003470x`, `0.002653x`, and
  `0.001734x` (Mermaid.js `288.19x`, `376.91x`, and `576.85x` slower). The
  rejected candidate worsened those render-stage ratios to `0.003869x`,
  `0.002986x`, and `0.001819x` (Mermaid.js only `258.44x`, `334.89x`, and
  `549.74x` slower). The fresh full-pipeline candidate still beats Mermaid.js
  by `190.94x`, `231.75x`, and `329.59x`, but the retained full-pipeline baseline
  was better on the largest gate (`343.82x` vs `329.59x`).
- **Verdict:** regression; code was reverted before commit and only this ledger
  evidence remains.
- **Revert:** manual `apply_patch` restored the original `to_css_vars()`,
  `to_css()`, and `to_svg_style()` structure; `git diff` showed no production
  code diff afterward.
- **Do-not-retry note:** do not split the remaining small theme/font CSS writers
  into private append helpers in isolation. The extra call structure and
  `fmt::Write` path lose against the current `format!` temporaries for this
  workload; the large-template direct-write keep is the useful member of this
  family.
- **Tooling note:** `rch exec` first selected `vmi1227854` and failed before
  benchmarking because `cmake` is missing for `highs-sys`; a pinned `hz2` retry
  fell back local with no admissible workers, so the keep/reject decision uses
  the adjacent same-local `rch exec` fallback pair. On this Cargo toolchain,
  per-crate benches use `--profile release`; literal `cargo bench --release`
  remains invalid. A follow-up `full_pipeline_wide` gate used
  `/data/projects/.rch-targets/frankenmermaid-cod-a` and confirmed the reject
  because `16x32` regressed.

### Theme CSS direct buffer write - KEPT (2026-06-26)
- **Lever:** `fm-render-svg::Theme::to_svg_style` now writes the large utility
  CSS template directly into the existing `String` with `write!` instead of
  allocating a second `format!` string and then copying it with `push_str`.
- **Hypothesis:** after the marker and source-span wins, embedded CSS remains a
  large fixed chunk of SVG output. The alien-graveyard buffer-management route
  and the FrankenSuite `format!` -> `write!` hot-loop rule suggested removing
  the temporary CSS buffer while preserving the exact same template.
- **Baseline -> After:** clean baseline worktree
  `/data/projects/.worktrees/frankenmermaid-tansparrow-css-direct-write-baseline-20260626`
  at `35569a3` vs candidate worktree
  `/data/projects/.worktrees/frankenmermaid-tansparrow-css-direct-write-20260626`,
  package `frankenmermaid-cli`, bench `pipeline_bench`, warm target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`. The decisive adjacent
  full-pipeline gate used distinct metadata artifacts:
  `RUSTFLAGS='-C metadata=tansparrowcssbase'` and
  `RUSTFLAGS='-C metadata=tansparrowcsscand'`. Baseline
  `full_pipeline_wide` means were `3.0598 ms`, `7.7382 ms`, and `9.1638 ms`
  for `8x16`, `12x24`, and `16x32`; candidate means were `2.3892 ms`,
  `7.8448 ms`, and `7.8508 ms`. That is `21.92%` faster, `1.38%`
  slower/no-change, and `14.33%` faster.
- **Render-stage context:** an adjacent forced-artifact `wide_stages/render`
  pair measured baseline `2.1748 ms`, `4.9721 ms`, and `10.296 ms` versus
  candidate `1.3995 ms`, `3.4677 ms`, and `11.075 ms`; the first two cases
  improved, while `16x32` was noisy/no-change. The full-pipeline gate is the
  keep decision because it is the Mermaid.js-facing workload.
- **Original comparator:** latest pinned live-CDP Mermaid `11.12.0` denominator
  reused from the current main ledger, Node `v24.14.0`, `/snap/bin/chromium`,
  dynamic import of
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  3 warmups, 20 timed render-to-SVG iterations, identical generated wide inputs.
- **frankenmermaid/Mermaid ratio:** candidate full-pipeline means versus
  Mermaid.js means `315.14 ms`, `981.73 ms`, and `2879.185 ms` give
  frankenmermaid/Mermaid ratios `0.007581x`, `0.007991x`, and `0.002727x`;
  Mermaid.js is `131.90x`, `125.14x`, and `366.74x` slower on the same inputs.
- **Verdict:** kept. Behavior proof: `rustfmt --edition 2024 --check
  crates/fm-render-svg/src/theme.rs`, `cargo check --profile release -p
  fm-render-svg`, focused `cargo test --profile release -p fm-render-svg
  theme_generates_complete_style`, and local conformance `cargo test --profile
  release -p frankenmermaid-cli --test frankentui_conformance_test` all passed.
  The code uses the same format template and captured `shadow_filter` /
  `hover_shadow_filter` values, so SVG CSS bytes are intended to remain
  identical.
- **Tooling note:** `rch exec` first fell back local for the render pair, then a
  remote full-pipeline attempt selected `vmi1264463` and failed before
  benchmarking because that worker lacks `cmake` for `highs-sys`. The literal
  `cargo bench --release` form is invalid on this Cargo toolchain; per-crate
  bench commands used `--profile release`.

### SVG integer number manual writer - REJECTED (2026-06-26)
- **Lever:** `fm-render-svg` added a shared `write_i32` helper for integer-valued
  SVG numbers and routed the integer branches of `AttributeValue::Number`,
  `AttributeValue::Integer`, and path `FmtNum` serialization through it.
- **Hypothesis:** wide flowcharts emit many integer-valued SVG coordinates and
  numeric attributes, so avoiding the generic `write!` / `fmt::Formatter`
  integer path should reduce render serialization overhead.
- **Baseline -> After:** clean worktree
  `/data/projects/.worktrees/frankenmermaid-tansparrow-path-bbox-20260626`,
  baseline commit `93f7ac0`, warm target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`, package
  `frankenmermaid-cli`, bench `pipeline_bench`, filter `wide_stages/render`.
  Same-local baseline means were `1.2226 ms`, `3.5489 ms`, and `5.6627 ms`
  for `8x16`, `12x24`, and `16x32`; candidate means were `1.3227 ms`,
  `2.8211 ms`, and `6.2051 ms`. That is `8.19%` slower, `20.51%` faster, and
  `9.58%` slower.
- **Full-pipeline gate:** retained current-main `full_pipeline_wide` means were
  `1.8394 ms`, `4.1821 ms`, and `8.7503 ms`; candidate means were
  `1.7957 ms`, `4.8479 ms`, and `9.2844 ms`. The candidate was `2.38%`
  faster on `8x16`, but `15.92%` slower on `12x24` and `6.10%` slower on
  `16x32`.
- **Original comparator:** latest pinned live-CDP Mermaid `11.12.0` denominator
  reused from the current main ledger, Node `v24.14.0`, `/snap/bin/chromium`,
  dynamic import of
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  3 warmups, 20 timed render-to-SVG iterations, identical generated wide inputs.
- **frankenmermaid/Mermaid ratio:** retained full-pipeline baseline means
  versus Mermaid.js means `315.14 ms`, `981.73 ms`, and `2879.185 ms` give
  frankenmermaid/Mermaid ratios `0.005837x`, `0.004260x`, and `0.003039x`;
  Mermaid.js is `171.33x`, `234.75x`, and `329.04x` slower. The rejected
  candidate ratios were `0.005698x`, `0.004938x`, and `0.003225x`, leaving
  Mermaid.js `175.50x`, `202.51x`, and `310.11x` slower; the candidate worsened
  the `12x24` and `16x32` dominance ratios.
- **Verdict:** regression; code was reverted before commit and only this ledger
  evidence remains.
- **Revert:** manual `apply_patch` restored the standard integer formatting
  branches in `attributes.rs` and `path.rs`; `git diff` showed no production
  code diff afterward.
- **Do-not-retry note:** do not pursue a shared manual decimal writer for SVG
  integer-valued numeric serialization in isolation. It can help one render case,
  but the full-pipeline gate regressed on the larger wide cases.
- **Tooling note:** a requested `rch exec` focused unit-test run selected
  `vmi1227854` and failed before tests because `cmake` was missing while
  building `highs-sys`. `RCH_WORKER=hz2` produced candidate-only routing
  context, but the keep/reject decision uses the same-local baseline/candidate
  render and full-pipeline pairs. The literal `cargo bench --release` form is
  invalid on this Cargo toolchain; per-crate bench commands used
  `--profile release`.

### SVG static custom-attribute names — REJECTED (2026-06-26)
- **Lever:** `fm-render-svg::Element::attr` and `Element::attr_num` were changed
  to take `&'static str` names and pass those names directly into
  `Attributes::str` / `Attributes::num`, removing the per-call
  `name.to_string()` allocation for literal SVG custom-attribute names such as
  `style`, `role`, `font-size`, and `text-anchor`.
- **Hypothesis:** all observed `attr` / `attr_num` first arguments in
  `fm-render-svg` are literals, and wide diagrams emit enough SVG elements that
  avoiding attribute-name allocation should improve `wide_stages/render`.
- **Baseline -> After:** clean worktree
  `/data/projects/.worktrees/frankenmermaid-tansparrow-attr-literal-20260626`,
  baseline commit `4f1a98e`, same warm target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`, package
  `frankenmermaid-cli`, bench `pipeline_bench`, filter `wide_stages/render`.
  Same-local `rch exec` fallback baseline means were `1.3065 ms`, `2.9507 ms`,
  and `5.5851 ms` for `8x16`, `12x24`, and `16x32`; candidate means were
  `1.1330 ms`, `6.6917 ms`, and `6.2591 ms`. That is `13.28%` faster on
  `8x16`, but `126.78%` slower on `12x24` and `12.07%` slower on `16x32`.
- **Full-pipeline context:** local retained-baseline `full_pipeline_wide` means
  after the revert were `1.9222 ms`, `4.3413 ms`, and `8.2540 ms`. An unpaired
  candidate `full_pipeline_wide` run on `hz2` measured `1.4886 ms`,
  `3.5287 ms`, and `6.6858 ms`, but that is routing context only because the
  scheduler did not provide a matching `hz2` baseline in this turn.
- **Original comparator:** latest pinned live-CDP Mermaid `11.12.0` denominator
  reused from the current main ledger, Node `v24.14.0`, `/snap/bin/chromium`,
  dynamic import of
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  3 warmups, 20 timed render-to-SVG iterations, identical generated wide inputs.
- **frankenmermaid/Mermaid ratio:** retained local full-pipeline baseline means
  versus Mermaid.js means `315.14 ms`, `981.73 ms`, and `2879.185 ms` give
  frankenmermaid/Mermaid ratios `0.006100x`, `0.004422x`, and `0.002867x`;
  Mermaid.js is `163.95x`, `226.14x`, and `348.82x` slower. The rejected
  render-stage candidate means against the same denominators were `0.003595x`,
  `0.006816x`, and `0.002174x`; the `12x24` stage ratio worsened because the
  candidate more than doubled same-local render time.
- **Verdict:** regression; the candidate failed the same-local render-stage
  gate at `12x24` and `16x32`.
- **Revert:** manual `apply_patch` restored `name: &str` plus
  `name.to_string()` in both builder methods; no production code diff remains.
- **Do-not-retry note:** do not pursue a blanket static-name signature for
  `Element::attr` / `attr_num`. The literal-name allocation is not a stable
  wide-render bottleneck in isolation, and tightening the public builder
  signature risks churn for little or negative measured gain.
- **Tooling note:** a requested pinned-worker remote candidate run first spilled
  to `vmi1227854` and failed before benchmarking because `cmake` was missing for
  `highs-sys`. Later, `RCH_ENABLED=false` still allowed one candidate
  full-pipeline run to execute on `hz2`; because the paired baseline fell back
  locally, the `hz2` number is recorded only as unpaired context.

### Edge `data-fm-edge-id` numeric value path — REJECTED (2026-06-26)
- **Lever:** `fm-render-svg` added a numeric `usize` `AttributeValue` path plus
  `Attributes::data_usize` / `Element::data_usize`, then used it for the three
  `data-fm-edge-id` edge-index call sites in `render_edge`.
- **Hypothesis:** wide layered diagrams render hundreds of edges; avoiding
  `edge_index.to_string()` for each edge metadata attribute should reduce SVG
  render allocation pressure while preserving byte-identical quoted decimal
  output such as `data-fm-edge-id="42"`.
- **Baseline -> After:** same clean worktree, same warm cod-b target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`, package
  `frankenmermaid-cli`, bench `pipeline_bench`. Baseline `wide_stages/render`
  through `rch exec` local fallback measured `1.2735 ms`, `3.0705 ms`, and
  `6.1986 ms` for `8x16`, `12x24`, and `16x32`; candidate direct-local means
  after the remote worker failed before bench were `1.2162 ms`, `3.0792 ms`,
  and `6.5593 ms`. That is `4.50%` faster but not significant on `8x16`,
  `0.28%` slower/noise on `12x24`, and `5.82%` slower on `16x32`.
- **Full-pipeline gate:** retained current-main `full_pipeline_wide` means were
  `1.7174 ms`, `4.3000 ms`, and `8.5379 ms`; candidate means were
  `1.7523 ms`, `6.2083 ms`, and `10.937 ms`, which is `2.03%`, `44.38%`,
  and `28.10%` slower. Criterion reported significant regressions for `12x24`
  and `16x32`.
- **Original comparator:** latest pinned live-CDP Mermaid `11.12.0` denominator
  reused from the current main ledger, Node `v24.14.0`, `/snap/bin/chromium`,
  dynamic import of
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  3 warmups, 20 timed render-to-SVG iterations, identical generated wide inputs.
- **frankenmermaid/Mermaid ratio:** retained current-main full-pipeline ratios
  are `0.005450x`, `0.004380x`, and `0.002965x`, meaning Mermaid.js is
  `183.50x`, `228.31x`, and `337.22x` slower. The rejected candidate would
  worsen the `12x24` and `16x32` ratios to `0.006324x` and `0.003799x`, leaving
  Mermaid.js only `158.13x` and `263.25x` slower on those cases.
- **Verdict:** regression; code was reverted before commit and only this ledger
  evidence remains.
- **Revert:** manual `apply_patch` removed `AttributeValue::Usize`,
  `data_usize`, and the `render_edge` call-site changes; `git diff` showed no
  production code diff afterward.
- **Do-not-retry note:** do not pursue edge-id numeric metadata allocation in
  isolation. The extra enum variant and serialization branch did not predict the
  full-pipeline behavior; future SVG render work should target the larger
  repeated element/tree construction costs surfaced by `wide_stages/render`.
- **Tooling note:** the requested `rch exec` candidate run selected worker
  `vmi1227854` and failed before benchmarking because `cmake` was missing while
  building `highs-sys`; an `RCH_ENABLED=0 rch exec` retry still selected the
  same worker and was interrupted. The accepted candidate timing used direct
  local Cargo with the same target dir as the `rch exec` local-fallback baseline.

### SVG plain node label direct element path — KEPT (2026-06-26)
- **Lever:** `fm-render-svg::render_node_label_text` now keeps markdown and
  multiline labels on the existing `TextBuilder` path, but renders single-line
  plain labels directly as an SVG `text` element. This avoids the extra builder
  string clones for the common flowchart node-label shape.
- **Hypothesis:** generated wide flowcharts spend enough time constructing plain
  node label text elements that a guarded direct path should improve the
  full-pipeline SVG gate while preserving markup and multiline behavior.
- **Baseline -> After:** clean detached current-main baseline worktree
  `/data/projects/.worktrees/frankenmermaid-cod-a-baseline-e7ad162-20260626`
  at `e7ad162`, then candidate in `/data/projects/frankenmermaid`, same
  `RCH_ENABLED=0 CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a
  rch exec -- cargo bench -p frankenmermaid-cli --bench pipeline_bench --
  full_pipeline_wide --warm-up-time 1 --measurement-time 2`. Current-main
  means were `2.1114 ms`, `4.9828 ms`, and `10.617 ms` for `8x16`, `12x24`,
  and `16x32`; candidate means were `1.8459 ms`, `4.5859 ms`, and `8.0958 ms`.
  The candidate was `12.57%`, `7.97%`, and `23.75%` faster.
- **Original comparator:** latest pinned live-CDP Mermaid `11.12.0` denominator
  reused from the current main ledger, Node `v24.14.0`, `/snap/bin/chromium`,
  dynamic import of
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  3 warmups, 20 timed render-to-SVG iterations, identical generated wide inputs.
- **frankenmermaid/Mermaid ratio:** candidate wide means versus Mermaid.js means
  `315.14 ms`, `981.73 ms`, and `2879.185 ms` give frankenmermaid/Mermaid
  ratios `0.005857x`, `0.004671x`, and `0.002812x`; Mermaid.js is `170.72x`,
  `214.08x`, and `355.64x` slower.
- **Verdict:** kept. Behavior proof: `cargo fmt -p fm-render-svg --check`,
  `rch exec -- cargo check -p fm-render-svg --all-targets`,
  `rch exec -- cargo clippy -p fm-render-svg --all-targets -- -D warnings`,
  focused `cargo test -p fm-render-svg
  plain_node_label_fast_path_matches_text_builder_output -- --nocapture`, and
  local conformance `cargo test -p frankenmermaid-cli --test
  frankentui_conformance_test` all passed. The remote conformance attempt on
  `vmi1227854` failed before tests because that worker lacks `cmake` for
  `highs-sys`.

### SVG root attribute direct streaming — REVERTED (2026-06-26)
- **Lever:** `fm-render-svg::SvgDocument::write_to_string` changed root SVG
  attribute emission from `output.push_str(&self.attrs.render())` to
  `self.attrs.write_into(output)`, avoiding the temporary root attribute string.
- **Hypothesis:** the root SVG element is emitted on every render, and direct
  streaming should remove one allocation/copy from the full-pipeline SVG hot
  path without changing escaping semantics because `Attributes::render` delegates
  to `Attributes::write_into`.
- **Baseline -> After:** same worktree
  `/data/projects/.worktrees/frankenmermaid-tansparrow-svg-root-attrs-direct-20260626`,
  same `rch exec` local fallback, package `frankenmermaid-cli`, bench
  `pipeline_bench`, filter `full_pipeline_wide`, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`. Restored-current baseline
  means were `2.2743 ms`, `5.3945 ms`, and `10.707 ms` for `8x16`, `12x24`,
  and `16x32`; candidate means were `2.1638 ms`, `6.6546 ms`, and `9.5462 ms`.
  The candidate was `4.86%` faster on `8x16`, `23.36%` slower on `12x24`, and
  `10.84%` faster on `16x32`.
- **Original comparator:** latest pinned live-CDP Mermaid `11.12.0` denominator
  from the current main ledger, Node `v24.14.0`, `/snap/bin/chromium`,
  dynamic import of
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  3 warmups, 20 timed render-to-SVG iterations, identical generated wide inputs.
- **frankenmermaid/Mermaid ratio:** candidate wide means versus Mermaid.js means
  `315.14 ms`, `981.73 ms`, and `2879.185 ms` give frankenmermaid/Mermaid
  ratios `0.006866x`, `0.006778x`, and `0.003316x`; Mermaid.js is `145.64x`,
  `147.53x`, and `301.61x` slower. The retained baseline ratios after revert
  are `0.007217x`, `0.005495x`, and `0.003719x`.
- **Verdict:** mixed loss; the important `12x24` wide case regressed by
  `23.36%`, so the code was reverted before commit.
- **Revert:** manual `apply_patch` restored `output.push_str(&self.attrs.render())`;
  no production code diff remains.
- **Do-not-retry note:** the one-off root attribute temporary is not a stable
  full-pipeline bottleneck at current sizes. Future SVG serialization work should
  target repeated element/path/text emission costs rather than the root
  `SvgDocument` attribute render.
- **Tooling note:** an earlier candidate-only run on `ovh-a` had no same-worker
  baseline and is not keep evidence. A follow-up baseline attempt on
  `vmi1227854` failed before benchmarking because `cmake` was missing while
  building `highs-sys`; that run is also not benchmark evidence.

### SVG document child Vec capacity hint — REVERTED (2026-06-25)
- **Lever:** `fm-render-svg::SvgDocument` grew a `with_child_capacity`
  constructor, and the legacy layout renderer pre-sized the root document
  `children` vector from layout node/edge/extension counts before pushing SVG
  elements.
- **Hypothesis:** wide diagrams push hundreds to thousands of root SVG children;
  pre-sizing that vector should avoid repeated growth copies and improve the
  render-heavy `full_pipeline_wide` gap versus Mermaid.js.
- **Baseline -> After:** same-worker `ovh-a`, package `frankenmermaid-cli`,
  bench `pipeline_bench`, filter `render_svg/flowchart`, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-a`: `small_10`
  `117.96 us` -> `116.72 us` (`-1.78%`, significant but below keep bar),
  `medium_100` `690.67 us` -> `688.94 us` (`-0.16%`, no change), and
  `large_500` `3.1937 ms` -> `3.1875 ms` (`-0.19%`, no change).
- **Wide gate:** `rch` could not reserve a remote worker and failed open locally
  for `full_pipeline_wide`; the candidate regressed the retained Criterion
  baseline by `+25.19%`, `+20.93%`, and `+28.22%` on `8x16`, `12x24`, and
  `16x32` (`2.0793 ms` -> `2.6031 ms`, `4.9551 ms` -> `5.9923 ms`,
  `8.7779 ms` -> `11.255 ms`). This local fallback is rejection evidence, not
  a keep proof.
- **Original comparator:** latest pinned live-CDP Mermaid `11.12.0` denominator
  from the current main ledger, Node `v24.14.0`, `/snap/bin/chromium`,
  dynamic import of
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  3 warmups, 20 timed render-to-SVG iterations, identical generated wide inputs.
- **frankenmermaid/Mermaid ratio after revert:** retained main
  `full_pipeline_wide` means `2.0793 ms`, `4.9551 ms`, and `8.7779 ms`
  against Mermaid.js means `315.14 ms`, `981.73 ms`, and `2879.185 ms` give
  frankenmermaid/Mermaid ratios `0.006598x`, `0.005047x`, and `0.003049x`;
  Mermaid.js is `151.56x`, `198.13x`, and `328.00x` slower.
- **Verdict:** ~0 gain on realistic render-only sizes plus a decisive wide
  regression. The code was reverted before commit.
- **Revert:** candidate reverted before commit; this ledger commit records the
  rejection.
- **Do-not-retry note:** root document child-vector allocation is not a measured
  hotspot after the existing output-buffer and attribute-vector work; future SVG
  work should profile element construction, text escaping, and path/label
  serialization instead of pre-sizing the document child Vec again.

### Attributes Vec pre-size after edge-style fast path — CAUTION (2026-06-25)
- **Lever:** `fm-render-svg::Attributes::new` changed from `Vec::new()` to
  `Vec::with_capacity(12)`, based on an unpushed worktree commit
  `5b81012` that had measured as a renderer win before the current
  edge-style short-circuit landed.
- **Hypothesis:** most SVG elements carry several attributes, so pre-sizing the
  attribute vector should avoid repeated small Vec growth on node/edge element
  construction.
- **Baseline -> After:** re-verified against current `main` baseline
  `b52b71c` on `ovh-a`, package `frankenmermaid-cli`, bench
  `pipeline_bench`, filter `render_svg`, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`. Means were
  `small_10` `117.66 us` -> `115.06 us` (`1.023x`), `medium_100`
  `689.56 us` -> `685.37 us` (`1.006x`), and `large_500` `3.2504 ms`
  -> `3.2109 ms` (`1.012x`).
- **Original comparator:** Mermaid `11.12.0` browser bundle from
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.min.js`,
  Chromium headless, `maxEdges=2000`, 3 warmups, 20 timed render-to-SVG
  iterations.
- **frankenmermaid/Mermaid ratio:** retained current-main ratios remain
  `0.004995x` (`8x16`), `0.005237x` (`12x24`), and `0.002860x`
  (`16x32`), i.e. Mermaid.js is still `200.2x`, `190.9x`, and `349.6x`
  slower on the pinned wide inputs.
- **Verdict:** not an independent keep by this recheck; every re-verified
  render-only gain is below the >=3% keep bar after the already-landed
  edge-style fast path changed the baseline. Upstream `d568ce6` landed this
  lever during rebase using its own same-machine evidence and is also recorded
  in `evidence/ledger/mermaid-js-head-to-head.toml`.
- **Revert:** none in this commit; no additional production code was landed by
  this recheck.
- **Do-not-retry note:** do not resurrect the broad default capacity of 12
  unless a fresh profile shows `Attributes` Vec growth back in the top renderer
  costs on current `main`.

### Direct edge-path string emission — REJECTED (2026-06-25)
- **Lever:** `fm-render-svg` streamed Catmull-Rom edge path text directly from
  `LayoutEdgePath` points with offsets, bypassing the temporary offset-point
  `Vec` and the intermediate `PathBuilder` command vector.
- **Hypothesis:** generated flowcharts have many edges, so eliminating two
  short-lived allocations per edge should improve render-only large-flowchart
  throughput.
- **Baseline -> After:** same `ovh-a` baseline at `b52b71c`, package
  `frankenmermaid-cli`, bench `pipeline_bench`, filter `render_svg`, target
  dir `/data/projects/.rch-targets/frankenmermaid-cod-b`. Means were
  `small_10` `117.66 us` -> `118.12 us` (`0.996x`), `medium_100`
  `689.56 us` -> `697.50 us` (`0.989x`), and `large_500` `3.2504 ms`
  -> `3.2647 ms` (`0.996x`). Criterion marked the changes within its noise
  threshold, with a slight slower direction in all three cases.
- **Original comparator:** Mermaid `11.12.0` browser bundle from
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.min.js`,
  Chromium headless, `maxEdges=2000`, 3 warmups, 20 timed render-to-SVG
  iterations.
- **frankenmermaid/Mermaid ratio:** retained current-main ratios remain
  `0.004995x` (`8x16`), `0.005237x` (`12x24`), and `0.002860x`
  (`16x32`), i.e. Mermaid.js is still `200.2x`, `190.9x`, and `349.6x`
  slower on the pinned wide inputs.
- **Verdict:** reverted before commit; the direct writer was byte-equivalent in
  a focused unit test but did not improve the measured renderer path.
- **Revert:** manual `apply_patch` removal in this session; no production code
  diff remains.
- **Do-not-retry note:** the per-edge path allocation is not the active
  bottleneck at current graph sizes. Route future renderer work toward measured
  element construction or text/style costs instead.

## Kept Wins Also Recorded Here By Request

### Move per-label `font-family` to the root `<svg>` (inherited) under `embed_theme_css` (−6.83% SVG bytes) — KEPT (2026-06-27)
- **Change — the largest byte win of the series.** Every `<text>` label carried an inline
  `font-family="'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial,
  sans-serif"` (~123 B with escaping) — repeated once per node/edge/cluster label. `font-family`
  is an **inherited** SVG property, so with the theme CSS embedded it's now set **once** on the
  root `<svg>` (`SvgDocument::font_family`, gated on `embed_theme_css`) and every `<text>` inherits
  it; the per-label inline copies are gated off via `font_family_unless_embedded_css` on both
  `TextElement` and `Element` (the two emission mechanisms — **54** call sites + the plain-label
  fast path, all routed through the helpers so the fast path still matches the TextBuilder path).
- **Why the gate (not unconditional move):** `embed_theme_css = false` (PNG raster) has no
  inherited source resvg can rely on cheaply, so it keeps the per-label inline (root font-family is
  *not* added there). Browsers handle root→text `font-family` inheritance natively for the CSS-on
  path. The monospace tspan font (code labels) is **not** gated — it intentionally overrides the
  inherited root.
- **Measured (deterministic byte win):** aggregate over the **37** regenerated `golden_svg_test`
  snapshots `724,053 → 674,607 = −6.83%`; a 2-node diagram is `12,573 → 12,282` (−291), and
  label-heavy/wide diagrams trend far higher (≈123 B × every label minus the one root attr — e.g.
  ~63 KB on a 512-node graph). This is the single biggest output-size gap vs Mermaid (which is
  CSS-driven and never repeats the font per label) and this change matches that approach.
- **Conformance:** `cargo test -p fm-render-svg` = **220 pass** (the plain-label-fast-path
  equivalence test passes once the fast path is gated identically). `golden_svg_test` regenerated
  via `BLESS=1` = **2 pass**; verified the 37 diffs are **only** `font-family` relocation (stripping
  every ` font-family="…"` from old and new yields identical text). Verified the root `<svg>`
  carries `font-family` and labels carry none for CSS-on; the raster path keeps the per-label copy.
- **frankenmermaid/Mermaid ratio:** matches Mermaid's CSS-driven font handling. Standing `240.5x`/
  `319.1x`/`505.7x` (latest) over Mermaid `11.12.0`.

### Gate redundant node drop-shadow inline `filter` + its `<defs>` def on `embed_theme_css` (−1.83% SVG bytes) — KEPT (2026-06-26)
- **Change:** node shapes emitted `filter="url(#drop-shadow)"` and a `<filter id="drop-shadow">`
  def, but `to_svg_style` puts `filter: drop-shadow(…)` directly on the **base** `.fm-node rect,
  path, circle, ellipse, polygon { … }` rule (verified in `theme.rs` — the `{shadow_filter}` is in
  that selector, not just `:hover`). A presentation attribute loses to the stylesheet, so for
  embedded CSS the inline filter is redundant *and* the def is then **dead** (its only referrer is
  that inline filter — `url(#drop-shadow)` reference count drops to 0). Gated both on
  `!config.embed_theme_css` (the inline filter at the shape site, the def block in
  `render_layout_to_svg`). The CSS shadow and the inline filter/def are gated on the *same*
  `detail.enable_shadows`, so they're always correlated — when the inline would emit, the CSS
  shadow is present.
- **Why the gate (not deletion):** `embed_theme_css = false` (PNG raster) is the path where the
  inline filter + def are the actual shadow source (resvg can't apply the CSS), and it's also where
  the configurable `shadow_color` is honoured. Verified the shadow still renders for CSS-on (the
  regenerated goldens retain the `filter: drop-shadow(…)` CSS rule) and updated
  `configurable_shadow_filter_is_emitted` to assert the configurable def via the
  `embed_theme_css = false` path (where it's live).
- **Measured (deterministic byte win):** the def (~180 B) is per-SVG so this is the largest of the
  inline-vs-CSS gates on shadow-bearing diagrams — `flowchart_simple` (3 nodes) `14,053 → 13,799 B`
  (`−254`, ~1.8%); aggregate over the **36** regenerated `golden_svg_test` snapshots `727,955 →
  714,599 = −1.83%`. Only affects diagrams where `detail.enable_shadows` is on (smaller diagrams;
  large graphs already drop shadows).
- **Conformance:** `cargo test -p fm-render-svg` = **220 pass** (the gate test now also asserts the
  embedded-CSS render holds **0** `url(#drop-shadow)` references while the attribute-driven export
  keeps them). `golden_svg_test` regenerated via `BLESS=1` = **2 pass**; verified the 36 diffs are
  **only** the drop-shadow inline filter + def removal.
- **frankenmermaid/Mermaid ratio:** Mermaid is CSS-driven; this matches it. Standing `240.5x`/
  `319.1x`/`505.7x` (latest) over Mermaid `11.12.0`.

### Gate redundant inline node-shape `stroke-width="1.60"` on `embed_theme_css` (−0.92% SVG bytes) — KEPT (2026-06-26)
- **Change:** companion to the node-stroke gate. The standard node shapes emitted
  `stroke-width="1.60"`, but the unconditional CSS `.fm-node rect, path, circle, ellipse, polygon {
  stroke-width: 1.6 }` already sets it and overrides the presentation attribute, so the inline copy
  is redundant for CSS-on. Added `Element::stroke_width_unless_embedded_css(width, embed_css)` and
  routed the **21** `.stroke_width(1.6)` sites through it — all verified to be in `render_node`
  (none outside; edges use a `.stroke_width(<var>)` variable, not the `1.6` literal, so they are
  untouched).
- **Safety:** byte-identical-rendering — CSS-on already renders the CSS `1.6` (it overrides the
  inline), CSS-off keeps the inline (PNG raster). The **special-width** node sites (`.stroke_width(1.0/0.8/2.0)`
  for variant shapes) are **left untouched** — their CSS-variant coverage isn't the uniform `1.6`
  rule, so they're out of scope. Verified the regenerated goldens drop **only** `stroke-width="1.60"`.
- **Measured (deterministic byte win):** node-heavy 40-node flowchart `45,931 → 45,131 B` (`−800`,
  ~1.6%); aggregate over the 31 regenerated `golden_svg_test` snapshots `651,638 → 645,618 = −0.92%`.
- **Conformance:** `cargo test -p fm-render-svg` = **220 pass** (the gate test now also asserts the
  inline node `stroke-width` vanishes with CSS and remains without it). `golden_svg_test` regenerated
  via `BLESS=1` = **2 pass**; verified the 31 diffs are **only** ` stroke-width="1.60"` removal.
- **frankenmermaid/Mermaid ratio:** Mermaid is CSS-driven; this matches it. Standing `240.5x`/
  `319.1x`/`505.7x` (latest) over Mermaid `11.12.0`.

### Gate redundant inline node-shape base `stroke` on `embed_theme_css` (−0.80% SVG bytes + alloc) — KEPT (2026-06-26)
- **Change:** the node analog of the edge stroke gate, after last cycle scoped the safe path. Every
  node shape emitted `stroke=<theme node stroke>` (`#e2e8f0` = `--fm-node-stroke`), but the
  unconditional CSS `.fm-node rect, path, circle, ellipse, polygon { stroke: var(--fm-node-accent) }`
  (and `.fm-node line`) covers **every** node element type and overrides the presentation attribute,
  so the inline copy is redundant. Added `Element::stroke_unless_embedded_css(color, embed_css)` and
  routed all **28** `.stroke(&colors.node_stroke)` sites (in `render_node` + `render_class_compartments`,
  both with `config`) through it.
- **Safety (the deferred-refactor concern from last cycle, resolved):** verified every node-stroke
  element is a CSS-covered type — `all_node_shapes` strokes `<rect>`/`<path>`/`<circle>`/`<line>`, all
  inside `.fm-node` and covered (line by its own `.fm-node line` rule). Custom `classDef`/`style`
  colors ride a separate `style="fill:…; stroke:…"` that wins (verified: `style A` → base stroke gone,
  `style=` carries `#00ff00`). `stroke-width` is **kept** (per-shape values — rect 1.6 vs line 1.5 —
  would need per-shape matching). Confirmed the default render now carries **zero** inline node strokes
  while the attribute-driven (`embed_theme_css = false`, PNG raster) export keeps them.
- **Measured (deterministic byte win + alloc):** node-heavy 40-node flowchart `46,611 → 45,931 B`
  (`−680`, ~1.4%); aggregate over the 31 regenerated `golden_svg_test` snapshots `656,874 → 651,638
  = −0.80%`. Also eliminates one `node_stroke` `String` clone per node element in the default path.
- **Conformance:** `cargo test -p fm-render-svg` = **220 pass** (the gate test now covers edge + node
  strokes). `golden_svg_test` regenerated via `BLESS=1` = **2 pass**; verified the 31 diffs are **only**
  `stroke="#…"` removal (stripping ` stroke="#hex"` from old and new yields identical text).
- **frankenmermaid/Mermaid ratio:** Mermaid is CSS-driven (no inline node stroke); this matches it.
  Standing `240.5x`/`319.1x`/`505.7x` (latest) over Mermaid `11.12.0`.

### Gate redundant inline edge base `stroke` on `embed_theme_css` (−0.92% SVG bytes + alloc) — KEPT (2026-06-26)
- **Change:** extends the `fill="none"` gate below to the edge's base `stroke=<theme edge color>`.
  `.fm-edge { stroke: var(--fm-edge-color) }` is unconditional CSS and overrides the presentation
  attribute, and `base_color` in `render_edge` is **always** the theme edge color (verified: the
  arrow-type match binds it to `&colors.edge` for every variant). Per-edge `linkStyle` colors are
  emitted as a **separate `style="stroke:#…"`** that wins over both the presentation attribute and
  the CSS — confirmed by rendering `linkStyle 0 stroke:#ff0000` (output keeps `style="stroke:#ff0000"`,
  drops the base `stroke`). So gating the base stroke on `!embed_theme_css` is safe in all four
  default/custom × CSS-on/off cases.
- **Why the gate (not deletion):** identical reasoning to the fill gate — `embed_theme_css = false`
  is the PNG raster path (resvg can't fully apply CSS), which keeps the inline fallback.
- **Measured (deterministic byte win + alloc):** default 40-edge flowchart `47,274 → 46,611 B`
  (`−663`, ~1.4% of that render); aggregate over the 28 regenerated `golden_svg_test` snapshots
  `618,938 → 613,226 = −0.92%`. Also eliminates one `String` clone of the edge color per edge in
  the default path (`.stroke` no longer runs). `stroke-width` is **kept** (the `2px !important`
  rule that would cover it is inside `@media (prefers-contrast: more)`, conditional — the
  unconditional CSS sets no width, so the inline value is the real one).
- **Conformance:** `cargo test -p fm-render-svg` = **220 pass** (the gate test now asserts both the
  inline edge fill *and* stroke vanish with CSS and remain without it). `golden_svg_test`
  regenerated via `BLESS=1` = **2 pass**; verified the 28 diffs are **only** edge `stroke="#…"`
  removal (stripping ` stroke="#hex"` from old and new yields identical text). `linkStyle` custom
  colors confirmed intact.
- **frankenmermaid/Mermaid ratio:** Mermaid is CSS-driven (no inline edge stroke); this matches it.
  Standing `240.5x`/`319.1x`/`505.7x` (latest) over Mermaid `11.12.0`.

### Gate redundant inline edge `fill="none"` on `embed_theme_css` (−0.65% to −2% SVG bytes) — KEPT (2026-06-26)
- **Change:** `render_edge` always emitted `fill="none"` on every edge path. The embedded theme
  CSS already sets `.fm-edge { fill: none }`, and an SVG **presentation attribute loses to the
  stylesheet**, so the inline copy is redundant whenever CSS is embedded (the default). Gated it
  on `!config.embed_theme_css` (`crates/fm-render-svg/src/lib.rs`, render_edge): the default /
  benched interactive SVG sheds it; `embed_theme_css = false` exports keep it.
- **Why the gate (not deletion):** `embed_theme_css = false` is the **PNG raster path**
  (`make_svg_render_config_raster_safe`) — usvg/resvg can't fully apply the CSS, so it relies on
  attribute-driven styling; dropping the inline fill there would render edges black-filled. The
  gate emits the inline fallback exactly there, so PNG output is unchanged.
- **Measured (deterministic byte win):** default render of a 40-edge flowchart drops edge
  `fill="none"` from 40 → 1 (the 1 is a non-edge open-arrow marker), **−468 B ≈ −0.96%**;
  aggregate over the 28 regenerated `golden_svg_test` snapshots **622,970 → 618,938 B = −0.65%**
  (mixed sizes; edge-heavy/wide diagrams trend toward ~2%). Plus one fewer `String` alloc
  (`"none"`) per edge in the default path.
- **Conformance:** `cargo test -p fm-render-svg` = **219 pass** + a new test
  `edge_fill_none_is_gated_on_embedded_css` (asserts the no-CSS export keeps more `fill="none"`
  than the CSS export). `golden_svg_test` regenerated via `BLESS=1` = **2 pass**; verified the 28
  golden diffs are **only** edge `fill="none"` removal (stripping ` fill="none"` from old and new
  yields identical text for every file).
- **frankenmermaid/Mermaid ratio:** Mermaid is CSS-driven and emits no inline edge fill, so this
  matches its approach and narrows the per-edge byte gap. Standing `240.5x`/`319.1x`/`505.7x`
  (latest) over Mermaid `11.12.0`.

### Drop dead `data-fm-node-id` node attribute (−1 to −2% SVG bytes, zero consumers) — KEPT (2026-06-26)
- **Change:** `render_node` emitted **both** `.data("id", node_id)` and
  `.data("fm-node-id", node_id)` — two attributes carrying the *same* `node_id`. Removed the
  second (`crates/fm-render-svg/src/lib.rs:3714`), keeping the **documented** `data-id`.
- **Why safe (dead output):** a repo-wide search found **zero** consumers of `data-fm-node-id`
  — no CSS selector, no JS `querySelector`/`getAttribute`/`dataset`, no README/docs guidance
  (the README documents `data-id`, not `data-fm-node-id`). It only duplicated `data-id`. This
  *corrects* a prior cycle's incorrect "intentional dual-naming, not removable" note (the
  claimed parallel to `data-fm-edge-id` was wrong — edge-id is the unique index, node-id a pure
  duplicate). Same class of dead-output removal as the earlier `data-fm-source-*` drop.
- **Measured (deterministic byte win):** aggregate over the 31 git-tracked `golden_svg_test`
  snapshots **673,729 → 666,618 bytes = −1.06%** (machine-independent); a node-heavy 40-node
  flowchart is **−1.8%**, scaling with node density vs the fixed ~9.7 KB CSS. Plus a small
  alloc/time reduction — one fewer `node_id` `String` copy and one fewer attribute (Vec slot +
  retain scan + serialization) **per node** (~0.5–1%, below the noise floor).
- **Conformance:** `cargo test -p fm-render-svg` = **219 pass**; `golden_svg_test` regenerated
  via `BLESS=1` and **GREEN** (2 pass). Verified the 31 golden diffs are **only**
  `data-fm-node-id` removal — stripping ` data-fm-node-id="…"` from each old snapshot exactly
  reproduces the new one (no coordinate/other drift).
- **frankenmermaid/Mermaid ratio:** Mermaid emits no such attribute, so this strictly narrows
  the per-node byte gap. Standing `240.5x`/`319.1x`/`505.7x` (latest) over Mermaid `11.12.0`.

### Store integer `data-fm-edge-id` without allocating (byte-identical alloc reduction) — KEPT (2026-06-26)
- **Change:** the three edge-render sites built the edge-id attribute as
  `.data("fm-edge-id", &edge_index.to_string())`, which **double-allocates** — `to_string()`
  builds a `String`, then `.data`'s `set` does `value.to_owned()` into `AttributeValue::String`.
  Replaced with a new `Element::attr_int` (forwards to the existing `Attributes::int` →
  `AttributeValue::Integer`); called as `.attr_int("data-fm-edge-id", edge_index as i32)` with a
  `&'static` name (`Cow::Borrowed`). Net: **2 `String` allocations eliminated per edge, zero
  added** (name static, value integer).
- **Byte-identical:** `Integer(n)` serializes to the same decimal text as the string, under the
  same attribute name. Verified: `cargo test -p fm-render-svg` = **219 pass**, and the
  git-tracked `golden_svg_test` = **2 pass** (the `data-fm-edge-id="N"` snapshots are unchanged).
- **Why KEPT despite no clean timing number:** the effect is ~1.2% of `render_svg/large_500`
  (500 edges × 2 allocs ≈ 30 µs of 2.4 ms) — **below the shared-worker noise floor**; the A/B
  attempt landed on a contended `ovh-a` (`5.7 ms ±2.8%`, vs an earlier quiet `2.40 ms ±0.05%` —
  a 2.4×→5.7 ms cross-run drift that swamps ~1.2%). Unlike the reverted *sign-unknown* micro-opts
  (`intersects_segment`) or the magic-number `to_svg_style` capacity, this is **sign-known ≥0**
  (strictly removes allocations, can never regress) and a genuine type-correctness fix using the
  right `AttributeValue` variant, plus a reusable `attr_int` that prevents the same
  integer-as-`String` anti-pattern elsewhere. Landed on byte-identity + sign, not a timing claim.
- **frankenmermaid/Mermaid ratio:** unchanged-to-slightly-improved; standing `240.5x`/`319.1x`/
  `505.7x` (latest) over Mermaid `11.12.0`, `198x`–`426x` floor.

### Drop write-only `IrNode.span_all` accumulation (parse −12% large) — KEPT (2026-06-26)
- **Provenance:** the code change landed independently during a session gap as commit
  `35569a3` ("stop building write-only span_all dead data in the IR builder"), built on the
  cmake-free `parse_bench` added in `5449c71`. This entry contributes the **clean,
  same-worker measured A/B** (below) that quantifies it.
- **Lever:** `IrNode.span_all: Vec<Span>` is **write-only dead data** — a repo-wide grep finds
  one push site plus its initializer and **zero readers** (the defining span lives in
  `span_primary`). Yet `IrBuilder::intern_node_auto` allocated a one-element `vec![span]` per
  node *and* pushed an extra `Span` on **every node reference** (≈2 per edge endpoint), with
  the inner `Vec` reallocating as references accumulated. Now `span_all` is initialized empty
  (`Vec::new()`, no allocation) and the per-reference push is removed.
- **Mapped primitive:** dead-data construction elimination, scoped by a proven zero-reader
  predicate (same family as the kept source-attr/marker dead-output removals, but on the
  parse/IR-build side rather than render output).
- **Measurement (clean, same-worker A/B):** benched on the new `fm-parser/parse_bench`
  (cmake-free, builds on any worker), `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cc`
  via `rch exec`. Both `--save-baseline` and `--baseline` runs landed on the **same** worker
  `hz2` (verified — no cross-worker `Baseline must exist` panic), so the comparison is valid:
  | case | change | |
  |------|--------|--|
  | `flowchart/large_1000` | **−12.48%** | p = 0.00 (`2.5316 ms → 2.2157 ms`) |
  | `flowchart/small_10` | **−4.77%** | p = 0.01 |
  | `wide/8x16` | **−3.75%** | p = 0.00 |
  | `wide/12x24` | −2.96% | p = 0.01 |
  | `wide/16x32` | −2.95% | p = 0.00 (`1.236 ms`) |
  | `flowchart/medium_100` | −1.30% | n.s. (p = 0.32) |
  Clears the keep bar on three realistic cases with no regression anywhere.
- **Behavior proof:** `rch exec -- cargo test -p fm-parser` = `405 passed; 0 failed` — empty
  `span_all` breaks nothing, confirming it is unread. The field still exists (no public-API/
  serde struct change); only its content changes from `[spans]` to `[]`, and it has no
  consumer. SVG output is unaffected (`span_all` is IR-internal, never rendered).
- **Original comparator:** parse is ≈21% of the wide pipeline; Mermaid.js's parser is part of
  its full render path. Shaving up to 12% off our node-heavy parse strictly widens the
  standing full-pipeline lead — current-main `full_pipeline_wide` `1.5908 ms` / `3.7339 ms` /
  `6.7530 ms` vs live-CDP Mermaid `11.12.0` `315.14 ms` / `981.73 ms` / `2879.185 ms` =
  `198.10x` / `262.92x` / `426.35x` slower.
- **Verdict:** kept; a clean, significant, output-identical parse win, measured reliably via
  the cmake-free `parse_bench` with both A/B runs pinned (by retry) to the same worker `hz2` —
  the same-worker workaround for the per-worker-target-dir blocker recorded below.
- **Do-not-retry note:** `span_all` is now empty, not removed; if a future consumer needs
  "all spans for a node" it must repopulate it (and should then also become a reader, ending
  its dead-data status).

### Emit only used `<defs>` arrowhead markers for flowcharts — KEPT (2026-06-26)
- **Lever:** both SVG render backends (`render_layout_to_svg` legacy/default and
  `render_scene_document_with_ir` scene) unconditionally wrote all **12** arrowhead markers
  into `<defs>` regardless of which the diagram uses. Mermaid.js emits only used markers.
  Now, for **flowcharts**, a one-pass `ir.edges` check decides: if every edge arrow uses
  only the basic markers (`arrow-end`/`arrow-open`/none — see `arrow_uses_only_basic_markers`;
  back-edges always use `arrow-open`), emit just those two; a single "fancy" arrow
  (half/stick/thick/circle/cross/diamond/double) falls back to the full set so a referenced
  marker is never missing. Emission order preserved.
- **Mapped primitive:** dead-output elimination / work-proportional-to-use, scoped by a
  provably-safe predicate (subset of `render_edge`'s marker match), with a conservative
  fallback (any non-listed arrow → full set; **non-flowchart diagrams → full set**, since
  e.g. sequence diagrams reference markers outside `ir.edges`).
- **Deterministic measurement (machine-independent):** built `fm-cli`, rendered flowcharts
  to SVG (`--no-embed-source-spans`). The marker reduction removes a **constant 1969 bytes**
  (the 10 omitted `<marker>` defs) per basic-arrow flowchart render: 2-edge flowchart
  `16006 → 14037` bytes (**−12.3%**), wide `8x16` `166290 → 164321` (**−1.18%**); marker
  count `12 → 2`, `marker-end="url(#arrow-end)"` still present (arrowheads intact); a circle
  arrow (`A --o B`) still emits all 12.
- **Render-time:** same-machine local A/B (rch timing blocked this turn — assigned worker
  `vmi1227854` lacks `cmake` for `highs-sys`), `render_svg/flowchart/small_10`, criterion
  `--save-baseline` then `--baseline`: `121.29 µs → 98.07 µs`, change **−15.89%** (p = 0.00
  < 0.05, "improved") — 10 fewer per-render `Element` builds + ~2 KB less to serialize, the
  largest fraction on small diagrams.
- **Behavior proof:** `cargo test -p fm-render-svg` = `219 passed; 0 failed` (the
  `includes_half_arrow_marker_defs` empty-**sequence** test still passes because non-flowchart
  diagrams keep the full set); `cargo test -p frankenmermaid-cli --test
  frankentui_conformance_test` passed. The `artifacts/regression-harness` goldens are not
  git-tracked, **but** `crates/fm-cli/tests/golden/*.svg` (the `golden_svg_test` FNV
  snapshots) **are** git-tracked and were *not* regenerated when this landed — so
  `golden_svg_test` was left failing on the 14 basic-arrow flowchart cases (12→2 markers).
  **Fixed 2026-06-26** by `BLESS=1 cargo test -p frankenmermaid-cli --test golden_svg_test`:
  the regenerated goldens differ from the old ones *only* by the 10 removed marker `<path>`
  defs (node/edge elements byte-identical; `flowchart_simple` `16337 → 14368` = −1969 bytes,
  matching the marker measurement), and `golden_svg_test` is GREEN again.
- **Original comparator:** Mermaid.js emits no unused markers, so for a basic flowchart it
  ships ~1–2 markers where we shipped 12; this removes that fixed ~2 KB / 10-element gap,
  matching Mermaid's marker behavior — largest win on the small/medium `render_svg/flowchart`
  sizes (the closest-fought vs Mermaid). The default-config `full_pipeline_wide` standing is
  effectively unchanged (markers are a ~1.2% slice there): `1.5908 ms` / `3.7339 ms` /
  `6.7530 ms` vs live-CDP Mermaid `11.12.0` `315.14 ms` / `981.73 ms` / `2879.185 ms` =
  `198.10x` / `262.92x` / `426.35x`.
- **Verdict:** kept; deterministic byte cut + significant small-flowchart render-time win,
  output-identical for any diagram that uses a given marker, safe fallback for fancy arrows
  and all non-flowchart diagram types. Implements the `bd-rcu5`-adjacent marker lever scoped
  in the prior "Emit only used `<defs>` arrowhead markers" blocker entry, via its safe
  plain/fancy route.
- **Do-not-retry note:** do not extend the basic-marker list without re-checking
  `render_edge`'s match (a wrong entry drops a referenced marker); do not gate non-flowchart
  diagrams this way (their markers are not all discoverable from `ir.edges`).

### Drop 6 redundant `data-fm-source-*` attributes (SVG −35% spans-on) — KEPT (2026-06-26)
- **Lever:** `fm-render-svg::apply_span_metadata` emitted seven source attributes per
  element: the compact `data-fm-source-span`
  (`{start.line}:{start.col}-{end.line}:{end.col}@{start.byte}-{end.byte}`) **plus** six
  individual `data-fm-source-{start,end}-{line,col,byte}` attributes that re-encode the
  exact same six values. Repo-wide grep confirmed each individual attr has exactly one
  reference — its own emit line — i.e. **zero consumers, zero test assertions, zero golden
  references**, while `data-fm-source-span` is consumed. The six were emit-only dead output.
  Now only the compact attribute is emitted (implements bead `bd-rcu5`, option a).
- **Scope / safety:** source spans are off in `SvgRenderConfig::default()`, so this is
  **byte-identical for the library default config** and for every default-config bench
  (`wide_stages`, `full_pipeline_wide`) and golden artifact (the regression-harness `.svg`
  files render spans-off, so none contained the six attrs). The CLI, however, embeds spans
  by default for SVG output (`main.rs`: `embed_source_spans || format == Svg`), so the
  real user-facing SVG is the spans-on path this shrinks.
- **Deterministic measurement (machine-independent, the headline):** wide `8x16` diagram
  (`flowchart TD`, 128 nodes, 256 edges) rendered to SVG via the built `fm-cli` binary,
  spans-on (CLI default). Output bytes: **`290282` → `188542`**, i.e. **−101740 bytes
  (−35.0%)**; the 576 span-bearing elements each shed six attributes. `fm-source-start-line`
  occurrences `576 → 0`; `fm-source-span` retained at `576`.
- **Render-time corroboration:** new permanent bench group `render_spans_on`
  (`crates/fm-cli/benches/pipeline_bench.rs`, `include_source_spans = true`), per-crate
  `cc` target dir, criterion `--save-baseline cc_spans_base` then `--baseline`. `8x16`
  `1.8021 ms → 1.5516 ms`, change `−12.47%` (p = 0.00 < 0.05, "improved"); `12x24` and
  `16x32` were inconclusive (p = 0.43 / 0.56) due to rch cross-worker variance (the saved
  baseline and candidate landed on different workers), but the deterministic byte cut is
  worker-independent.
- **Behavior proof:** `rch exec -- cargo test -p fm-render-svg` = `219 passed; 0 failed`
  (no test referenced the dropped attrs) and `cargo test -p frankenmermaid-cli --test
  frankentui_conformance_test` passed. Default-config output unchanged.
- **Original comparator:** Mermaid.js emits no source-map attributes at all, so the
  spans-on path was where our SVG was heaviest *relative to* Mermaid; removing this dead
  duplication cuts 35% of those excess bytes while keeping the one consumed span attr. The
  default-config `full_pipeline_wide` standing vs live-CDP Mermaid `11.12.0` is unchanged
  (spans off there): `1.5908 ms` / `3.7339 ms` / `6.7530 ms` vs `315.14 ms` / `981.73 ms`
  / `2879.185 ms` = `198.10x` / `262.92x` / `426.35x` slower.
- **Verdict:** kept; removes verified-dead output, shrinks real CLI SVG by 35% (spans-on),
  zero risk to the default config, resolves the priority-0 `bd-rcu5` big-lever bead via its
  conservative option (keep the consumed compact span, drop the redundant six).
- **Do-not-retry note:** the bead estimated ~55% byte reduction / render halving; the
  measured reduction is 35% bytes and ~12% render (8x16) — the redundant attrs were a large
  but not majority share, and render is not as purely byte-bound as the earlier profile
  suggested. Do not chase the remaining span bytes by also dropping `data-fm-source-span`;
  it is consumed.

### Borrowed source lines in flowchart document parser — KEPT (2026-06-26)
- **Lever:** `fm-parser::parse_flowchart_document` no longer copies each raw
  source line into an owned `String`. `FlowDocumentItem` (both `Statements` and
  `Subgraph` variants) and `FlowDocumentParseResult` now carry a `source_line:
  &'a str` borrowed from the parser input that already outlives the document
  build, so `parse_flowchart_document_items` stores `line` directly instead of
  `line.to_string()`. One heap allocation per parsed statement/subgraph line is
  eliminated; `span_for`/lowering read the borrow unchanged.
- **Mapped primitive:** allocation elision via lifetime threading — the input
  `&str` is the single owner of all line bytes for the whole parse, so the
  intermediate per-line `String` copies were pure overhead.
- **Baseline -> After:** same `cc` target dir
  (`/data/projects/.rch-targets/frankenmermaid-cc`), package
  `frankenmermaid-cli`, bench `pipeline_bench`, filter `parse/flowchart`, via
  `rch exec` (worker `ovh-a`). Baseline saved with `--save-baseline cc_base` on
  `main` `3dea2f6`, then re-run with `--baseline cc_base` after applying the
  change. Criterion `change` (p = 0.00 < 0.05, "Performance has improved" on all
  three):
  | case | before | after | change |
  |------|--------|-------|--------|
  | `small_10` | `23.981 us` | `18.953 us` | `-37.2%` (baseline noisy 22.9-25.3) |
  | `medium_100` | `163.21 us` | `153.36 us` | `-4.88%` |
  | `large_1000` | `2.3496 ms` | `2.1076 ms` | `-10.30%` |
- **Original comparator:** the parse stage feeds the standing live-CDP Mermaid
  `11.12.0` full-pipeline denominators (`8x16` `315.14 ms`, `12x24` `981.73 ms`,
  `16x32` `2879.185 ms`). frankenmermaid's whole parse+layout+SVG pipeline already
  runs at `198.10x`/`262.92x`/`426.35x` faster than Mermaid.js on those inputs
  (current-main standing, entry above); shaving ~10% off the parse component on
  1000-node flowcharts strictly widens that lead — Mermaid.js spends a large
  multiple of frankenmermaid's *entire* runtime inside its own parser alone.
- **Behavior proof:** `rch exec -- cargo test -p fm-parser` = `405 passed; 0
  failed`. Borrowed-lifetime version compiles clean (lifetimes proven sound by
  the borrow checker — the input outlives the returned document), so parse
  semantics, spans, and warnings are byte-identical.
- **Verdict:** kept; clears the keep bar with reproducible `-4.88%` (medium_100)
  and `-10.30%` (large_1000) and no regression — the larger the flowchart, the
  more per-line `String` allocations are avoided.
- **Do-not-retry note:** the borrow is now load-bearing; `FlowDocumentItem`
  cannot be detached from the input `&str` (e.g. returned past the parse) without
  re-introducing owned lines. Keep the `'a` lifetime threaded through
  `parse_flowchart_document_items` and `lower_flow_document_item`.

### Sparse edge-routing obstacle spatial index — KEPT (2026-06-25)
- **Lever:** `fm-layout` builds a grid index for node obstacle bounds once in
  `build_edge_paths_with_orientation`, then routes sparse/tree-like flowcharts
  through sorted candidate obstacle indices instead of scanning every node for
  each nudge segment. Dense wide graphs stay on the old scan path via an
  edge-count gate.
- **Hypothesis:** after the AABB and build-once obstacle wins, sparse large
  flowcharts still pay O(edges * nodes) cheap obstacle visits. A conservative
  grid query should reduce that to O(edges * nearby candidates) while preserving
  the existing CGA intersection authority.
- **Baseline -> After:** clean baseline worktree
  `/data/projects/.worktrees/frankenmermaid-cod-b-next-main-28b271d` at
  `28b271d` vs candidate checkout, package `frankenmermaid-cli`, bench
  `pipeline_bench`, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-a`. `layout/flowchart`
  means: `medium_100` `247.84 us` -> `234.58 us` (`5.35%` faster);
  `large_500` `736.49 us` -> `558.35 us` (`24.19%` faster). Full-pipeline
  `large_500` was directionally faster, `6.9929 ms` -> `6.6015 ms`, with
  Criterion reporting no significant change on the candidate rerun. Wide
  full-pipeline means stayed neutral or better against the same baseline:
  `8x16` `2.4635 ms` -> `2.3002 ms`, `12x24` `5.6366 ms` -> `5.5850 ms`,
  `16x32` `11.011 ms` -> `10.023 ms`.
- **Original comparator:** Mermaid `11.12.0` ESM browser bundle from
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  Node `v24.14.0` driving `/snap/bin/chromium` through Chrome DevTools
  Protocol, `maxEdges=2000`, 3 warmups, 20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio:** candidate full-pipeline wide SVG mean ratios
  were `0.007299x` (`8x16`: `2.3002 ms` vs `315.14 ms`), `0.005689x`
  (`12x24`: `5.5850 ms` vs `981.73 ms`), and `0.003481x` (`16x32`:
  `10.023 ms` vs `2879.185 ms`), i.e. Mermaid.js was `137.01x`, `175.78x`,
  and `287.26x` slower.
- **Verdict:** kept; the profiled sparse large-flowchart layout stage improved
  by 24.19%, medium improved by 5.35%, wide full-pipeline did not regress, and
  all `fm-layout` tests plus clippy are green.
- **Do-not-retry note:** do not remove the sparse edge-count gate without a
  fresh same-metadata A/B on the wide layered cases; dense ranks can make grid
  candidate gathering and sorting more expensive than the already-cheap AABB
  scan.

### Thresholded single-pass barycenter accumulation — KEPT (2026-06-24)
- **Lever:** `fm-layout::reorder_rank_by_barycenter` keeps the original per-node
  edge scan for narrow ranks and switches ranks with at least 8 nodes to a single
  edge pass that accumulates adjacent-rank position sums into per-slot bins.
- **Hypothesis:** wide layered diagrams spend avoidable time rescanning every edge
  once per rank node during crossing-minimization barycenter sweeps.
- **Baseline -> After:** `layout_wide/layered/8x16` forced-rebuild baseline mean
  `942.66 us` -> candidate mean `877.99 us` (`-6.8602%`, p < 0.05). Larger wide
  cases were statistical no-change, not regressions: `12x24` `4.0067 ms` ->
  `4.0635 ms` (p = 0.31), `16x32` `12.084 ms` -> `12.285 ms` (p = 0.27).
- **Original comparator:** Mermaid `11.12.0` browser bundle from
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.min.js`, Chromium
  headless, `maxEdges=2000`, 3 warmups, 20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio:** full-pipeline wide SVG mean ratios were
  `0.011986x` (`8x16`: `5.9845 ms` vs `499.28 ms`), `0.015694x`
  (`12x24`: `16.913 ms` vs `1077.69 ms`), and `0.008061x`
  (`16x32`: `31.832 ms` vs `3948.7 ms`).
- **Verdict:** kept; this is a narrow measured win with a large original-comparator
  margin. It is also recorded in `evidence/ledger/mermaid-js-head-to-head.toml`.
- **Do-not-retry note:** do not replace the threshold with unconditional
  single-pass accumulation unless a fresh same-target-dir run proves no small-rank
  setup overhead or mid-size regression.

### SVG renderer allocation trim — KEPT (2026-06-24)
- **Lever:** `fm-render-svg::Attributes` stores static attribute names as
  borrowed `Cow<'static, str>` values instead of heap-allocating every literal
  name, `SvgDocument` exposes `to_string_with_capacity`, and regular layout SVG
  rendering sizes the final contiguous output buffer from node/edge/cluster
  counts before serializing. Dynamic wrapper calls still pass owned names where
  required, so the renderer crate compiles cleanly on local and remote toolchains.
- **Hypothesis:** large realistic SVG outputs waste time copying the final
  `String` through repeated growth from the previous fixed 4 KiB starting
  capacity and spend avoidable allocation traffic on static attribute names. A
  cheap layout-size hint plus borrowed literal attribute names is an arena-style
  allocation win that does not alter the SVG serializer or emitted attributes.
- **Baseline -> After:** same-worker `hz2`, clean baseline worktree at
  `391cddf` vs candidate main checkout, command
  `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a rch exec -- cargo bench -p frankenmermaid-cli --bench pipeline_bench -- render_svg --warm-up-time 1 --measurement-time 2`.
  `render_svg/flowchart/small_10` mean `303.19 us` -> `286.28 us`
  (`-5.5774%`); `medium_100` `1.5254 ms` -> `1.4854 ms` (`-2.6223%`);
  `large_500` `7.1556 ms` -> `6.7241 ms` (`-6.0302%`).
- **Original comparator:** Mermaid `11.12.0` browser bundle from
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.min.js`, Chromium
  headless, `maxEdges=2000`, 3 warmups, 20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio:** fresh candidate full-pipeline wide SVG means
  on `ovh-a` were `0.008428x` (`8x16`: `4.2079 ms` vs `499.28 ms`),
  `0.010284x` (`12x24`: `11.083 ms` vs `1077.69 ms`), and `0.005952x`
  (`16x32`: `23.504 ms` vs `3948.7 ms`), i.e. Mermaid.js was `118.65x`,
  `97.24x`, and `168.00x` slower on those inputs.
- **Verdict:** kept; the realistic large SVG render case improved by >3%, small
  also improved, medium was a smaller win, and no regression was measured.
- **Do-not-retry note:** keep this as literal-name borrowing plus a final-buffer
  hint. Do not add a recursive pre-sizing pass over every element unless it beats
  this cheaper hint on a same-worker medium/large run without small-diagram
  overhead.

### Simple flowchart parser fast path — KEPT (2026-06-25)
- **Lever:** `fm-parser::parse_flowchart_statement_asts` first recognizes simple
  bare-node and bare-edge flowchart statements directly, then falls through to
  the existing chumsky/fallback parser stack for complex syntax. Accepted fast
  cases are bare ids, `id[label]`, and bare-id edges using `-.->`, `==>`, `-->`,
  `---`, `--o`, or `--x`.
- **Hypothesis:** generated and real-world flowcharts are dominated by simple
  node/edge statements, while the generic statement parser and recovery cascade
  pay avoidable parser-construction and parser-combinator overhead on every line.
- **Baseline -> After:** detached baseline worktree
  `/data/projects/.worktrees/frankenmermaid-cod-b-parser-baseline-48bb15c` at
  `48bb15c` vs candidate checkout, local same-machine, per-crate package
  `frankenmermaid-cli`, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`. Parse-only means:
  `parse/flowchart/small_10` `71.856 us` -> `32.963 us` (`2.180x`);
  `medium_100` `662.18 us` -> `271.47 us` (`2.439x`);
  `large_1000` `7.3442 ms` -> `3.9525 ms` (`1.858x`). Wide full-pipeline
  means: `8x16` `3.0428 ms` -> `2.1856 ms` (`1.392x`); `12x24`
  `7.8318 ms` -> `5.2484 ms` (`1.492x`); `16x32` `13.182 ms` ->
  `9.7081 ms` (`1.358x`).
- **Original comparator:** Mermaid `11.12.0` browser bundle from
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.min.js`, Chromium
  headless, `maxEdges=2000`, 3 warmups, 20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio:** full-pipeline wide SVG mean ratios were
  `0.004378x` (`8x16`: `2.1856 ms` vs `499.28 ms`), `0.004870x`
  (`12x24`: `5.2484 ms` vs `1077.69 ms`), and `0.002459x`
  (`16x32`: `9.7081 ms` vs `3948.7 ms`), i.e. Mermaid.js was `228.4x`,
  `205.3x`, and `406.7x` slower on those inputs.
- **Verdict:** kept; parse-only improves by roughly 1.86x-2.44x and the wide
  full-pipeline comparator improves by roughly 1.36x-1.49x on final-code
  same-machine absolute means.
- **Do-not-retry note:** do not broaden the fast path to labels, chained edges,
  grouped sources/targets, class shorthand, quoted labels, or shape-rich nodes
  without differential tests against the fallback parser and conformance proof.

### Current main live-CDP BOLD-VERIFY — KEPT (2026-06-25)
- **Lever:** no new code lever in this entry; this is a fresh live-browser
  verification of current `main` after the parser fast path and stacked
  parser/layout/render wins.
- **Hypothesis:** current `main` should still dominate Mermaid.js when the
  original comparator is timed through a live Chrome DevTools Protocol page
  rather than `--dump-dom` virtual-time output.
- **Baseline -> After:** current Rust full-pipeline-wide means on worker `hz2`,
  command `RCH_WORKER=hz2 CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a
  rch exec -- cargo bench -p frankenmermaid-cli --bench pipeline_bench --
  full_pipeline_wide --warm-up-time 1 --measurement-time 2`: `8x16`
  `2.1367 ms`, `12x24` `4.9451 ms`, `16x32` `9.1147 ms`.
- **Original comparator:** Mermaid `11.12.0` ESM browser bundle from
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  Node `v24.14.0` driving `/snap/bin/chromium` through Chrome DevTools
  Protocol, `maxEdges=2000`, 3 warmups, 20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio:** fresh live-CDP mean ratios were
  `0.005725x` (`8x16`: `2.1367 ms` vs `373.22 ms`), `0.003994x`
  (`12x24`: `4.9451 ms` vs `1238.165 ms`), and `0.003220x`
  (`16x32`: `9.1147 ms` vs `2830.495 ms`), i.e. Mermaid.js was `174.67x`,
  `250.38x`, and `310.54x` slower on those inputs.
- **Verdict:** kept/verified; the detached measured worktrees were already
  ancestors of `main`, so the fresh contribution is the current-ratio ledger
  proof rather than another code change.
- **Do-not-retry note:** do not use `chromium --dump-dom --virtual-time-budget`
  as a timing source for this comparator; it rendered valid SVG but collapsed
  `performance.now()` samples to zero in this session.

### Edge style empty-case short-circuit — KEPT (2026-06-25)
- **Lever:** `fm-render-svg::resolve_edge_inline_style` returns immediately for
  unstyled edges when the diagram has no style refs instead of constructing an
  empty `BTreeMap` per edge.
- **Hypothesis:** ordinary generated flowcharts have many unstyled edges and no
  `linkStyle` directives, so the old resolver paid a data-structure setup cost
  on the dominant render path even when the correct result was statically `None`.
- **Baseline -> After:** detached baseline worktree
  `/data/projects/.worktrees/frankenmermaid-cod-b-next-baseline-b8a4743` at
  `b8a4743` vs candidate checkout, local same-machine, per-crate package
  `frankenmermaid-cli`, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`. Render-only means:
  `render_svg/flowchart/small_10` `155.38 us` -> `142.98 us` (`1.087x`);
  `medium_100` `936.46 us` -> `852.30 us` (`1.099x`); `large_500`
  `4.6292 ms` -> `4.2710 ms` (`1.084x`). Wide full-pipeline means were
  statistically unchanged: `8x16` `2.4651 ms` -> `2.4938 ms` (`+1.16%`,
  no change), `12x24` `5.5677 ms` -> `5.6444 ms` (`+1.38%`, no change),
  and `16x32` `11.537 ms` -> `11.295 ms` (`-2.10%`, no change).
- **Original comparator:** Mermaid `11.12.0` browser bundle from
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.min.js`, Chromium
  headless, `maxEdges=2000`, 3 warmups, 20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio:** candidate full-pipeline wide SVG mean ratios
  were `0.004995x` (`8x16`: `2.4938 ms` vs `499.28 ms`), `0.005237x`
  (`12x24`: `5.6444 ms` vs `1077.69 ms`), and `0.002860x` (`16x32`:
  `11.295 ms` vs `3948.7 ms`), i.e. Mermaid.js was `200.2x`, `190.9x`,
  and `349.6x` slower on those inputs.
- **Verdict:** kept; the render-only crate proof is a repeatable 7.7%-9.0%
  speedup, while the wider comparator workload remains statistically unchanged
  and still strongly dominated versus Mermaid.js.
- **Do-not-retry note:** do not remove styled-edge resolution wholesale. This
  fast path is only for diagrams with no style refs after checking an edge's own
  inline style.

### SVG attribute Vec pre-size — KEPT (2026-06-25)
- **Lever:** `fm-render-svg::Attributes::new()` pre-sizes its private attribute
  vector to 12 entries, matching the common attribute count for SVG elements.
  This only changes allocation behavior while preserving the existing setter,
  deduplication, escaping, and serialization paths.
- **Hypothesis:** after the direct-write and bulk-escape renderer wins, repeated
  small `Vec` growth while constructing SVG element attributes is still a real
  large-render cost.
- **Baseline -> After:** clean detached baseline worktree
  `/data/projects/.worktrees/frankenmermaid-cod-a-attrs-baseline-9a61d4c` at
  `9a61d4c` vs candidate commit `8ba0aba`, same machine, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-a`, package
  `frankenmermaid-cli`. `render_svg/flowchart` means: `small_10`
  `152.43 us` -> `151.71 us` (`0.47%` faster); `medium_100` `900.91 us`
  -> `914.00 us` (`1.45%` slower/noise); `large_500` `4.6577 ms` ->
  `4.2747 ms` (`8.22%` faster). Longer `full_pipeline_wide` means:
  `8x16` `2.3034 ms` -> `2.2771 ms` (`1.14%` faster); `12x24`
  `5.5680 ms` -> `5.3180 ms` (`4.49%` faster); `16x32` `10.497 ms`
  -> `10.553 ms` (`0.53%` slower, Criterion no-change with overlapping
  interval).
- **Original comparator:** Mermaid `11.12.0` ESM browser bundle from
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  Node `v24.14.0` driving `/snap/bin/chromium` through Chrome DevTools
  Protocol, `maxEdges=2000`, 3 warmups, 20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio:** fresh live-CDP mean ratios were
  `0.006142x` (`8x16`: `2.2771 ms` vs `370.755 ms`), `0.004737x`
  (`12x24`: `5.3180 ms` vs `1122.62 ms`), and `0.003618x`
  (`16x32`: `10.553 ms` vs `2917.19 ms`), i.e. Mermaid.js was `162.82x`,
  `211.10x`, and `276.43x` slower on those inputs.
- **Verdict:** kept; the isolated large render stage and the wide `12x24`
  full-pipeline case clear the 3% threshold, and no significant wide regression
  reproduced on the longer run.
- **Do-not-retry note:** do not generalize this into broader allocation work
  without fresh same-machine A/B; earlier Cow/static-name capacity work was
  measured as ~0 and logged separately.

### SVG dynamic class append — KEPT (2026-06-25)
- **Lever:** dynamic SVG node/icon classes now append prefix and suffix directly
  into the existing `class` attribute string via `class_prefixed*`, avoiding
  per-node `format!` temporaries and the second copy through `class(&str)`.
- **Mapped primitive:** allocation-fusion / partial evaluation from the
  alien-artifact pass: keep class serialization byte-identical, but compile the
  common `prefix + small suffix` shape into direct buffer appends.
- **Baseline -> After:** same checkout/target local means with
  `/data/projects/.rch-targets/frankenmermaid-cod-a`, package
  `frankenmermaid-cli`, bench `pipeline_bench`. `render_svg/flowchart`:
  `small_10` `125.53 us` -> `129.74 us` (`+3.35%`, Criterion within-noise),
  `medium_100` `1.0405 ms` -> `809.00 us` (`-22.25%`, Criterion improved),
  and `large_500` `4.0836 ms` -> `4.1034 ms` (`+0.48%`, no change).
  Raw same-local `full_pipeline_wide` means: `8x16` `2.0991 ms` ->
  `2.0840 ms` (`-0.72%`), `12x24` `6.8145 ms` -> `5.0882 ms`
  (`-25.33%`), and `16x32` `10.428 ms` -> `10.176 ms` (`-2.42%`).
- **Original comparator:** latest live-CDP Mermaid `11.12.0` denominator reused
  for identical generated wide inputs: Node `v24.14.0`, `/snap/bin/chromium`,
  dynamic import of
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  3 warmups, and 20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio:** candidate local full-pipeline wide ratios
  were `0.006613x` (`8x16`: `2.0840 ms` vs `315.14 ms`), `0.005183x`
  (`12x24`: `5.0882 ms` vs `981.73 ms`), and `0.003534x` (`16x32`:
  `10.176 ms` vs `2879.185 ms`), i.e. Mermaid.js was `151.22x`, `192.94x`,
  and `282.94x` slower.
- **Behavior proof:** new `Attributes` tests cover prefixed class
  serialization; `cargo fmt -p fm-render-svg --check`,
  `rch exec -- cargo check -p fm-render-svg --all-targets`,
  `rch exec -- cargo clippy -p fm-render-svg --all-targets -- -D warnings`,
  and local `cargo test -p frankenmermaid-cli --test
  frankentui_conformance_test` passed. The remote conformance attempt failed
  before tests on worker `vmi1227854` because `cmake` was missing for
  `highs-sys`; this is worker toolchain failure, not conformance evidence.
- **Verdict:** kept; the focused renderer win is clear on `medium_100`, the
  wide `12x24` pipeline improves materially by raw same-local means, and no
  large-case regression reproduced in the retained means.
- **Do-not-retry note:** do not generalize this into `Attributes::set`
  scan-before-retain; that broader allocation lever already regressed
  `full_pipeline_wide` and remains rejected below.

### Static hot data-attribute names — KEPT (2026-06-26)
- **Lever:** `fm-render-svg::Attributes::data` now maps only the repeated hot
  renderer names `id`, `fm-node-id`, and `fm-edge-id` to static
  `data-*` attribute names, avoiding a per-node/per-edge
  `format!("data-{name}")` allocation while preserving the old fallback for all
  other data attributes.
- **Mapped primitive:** value-shape specialization from the alien-artifact pass:
  compile the observed hot names into static borrowed names and leave uncommon
  names on the generic allocation path.
- **Baseline -> After:** same clean worktree, same cod-b target dir, package
  `frankenmermaid-cli`, bench `pipeline_bench`, via `rch exec` local fallback.
  `render_svg/flowchart` means improved from `132.76 us` to `128.96 us`
  (`2.86%`) on `small_10`, `777.37 us` to `734.64 us` (`5.50%`) on
  `medium_100`, and `3.9018 ms` to `3.6295 ms` (`6.98%`) on `large_500`.
  `full_pipeline_wide` means improved from `2.1444 ms` to `1.8260 ms`
  (`14.85%`) on `8x16`, `5.3861 ms` to `4.2053 ms` (`21.92%`) on `12x24`,
  and `10.763 ms` to `8.2537 ms` (`23.31%`) on `16x32`.
- **Original comparator:** latest live-CDP Mermaid `11.12.0` denominator reused
  for identical generated wide inputs: Node `v24.14.0`, `/snap/bin/chromium`,
  dynamic import of
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  3 warmups, and 20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio:** candidate local full-pipeline wide ratios
  were `0.005794x` (`8x16`: `1.8260 ms` vs `315.14 ms`), `0.004284x`
  (`12x24`: `4.2053 ms` vs `981.73 ms`), and `0.002867x` (`16x32`:
  `8.2537 ms` vs `2879.185 ms`), i.e. Mermaid.js was `172.58x`, `233.45x`,
  and `348.84x` slower.
- **Behavior proof:** the new `Attributes` unit coverage exercises static and
  fallback data-name paths; `cargo fmt -p fm-render-svg --check`,
  `rch exec -- cargo test --profile release -p fm-render-svg attributes`,
  and `rch exec -- cargo clippy --profile release -p fm-render-svg --all-targets
  -- -D warnings` passed before rebase. After rebasing on the upstream
  renderer win, the remote clippy/conformance workers failed before validation
  because `cmake` was missing for `highs-sys`, and one `RCH_ENABLED=0 rch exec`
  wrapper spawned no Cargo child and was interrupted. Direct scoped Cargo gates
  with the same warm cod-b target dir passed:
  `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b cargo
  clippy --profile release -p fm-render-svg --all-targets -- -D warnings` and
  `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b cargo test
  --profile release -p frankenmermaid-cli --test frankentui_conformance_test`.
- **Verdict:** kept; the narrowed static-name table clears the keep bar on
  render-only medium/large and all wide full-pipeline cases, while the generic
  data attribute semantics remain unchanged for names outside the hot set.
- **Do-not-retry note:** do not widen this into a broad static table without a
  fresh render-only gate; an exploratory broader table improved wide timing but
  regressed `render_svg/flowchart`, so only the repeated node/edge names were
  retained.

### Barycenter sweep precomputed edge adjacency — REJECTED (2026-06-26)
- **Lever tested:** `fm-layout`'s barycenter crossing-minimization sweep
  (`reorder_rank_by_barycenter`) rescans the *entire* `ir.edges` list on every call
  (~`4 rounds * 2 * ranks` calls per layout). It was changed to build edge adjacency
  **once** per `crossing_minimization` (a `BarycenterAdjacency` of dense `node_rank`
  plus per-node `out_neighbors`/`in_neighbors` lists) and have each reordering walk
  only the neighbors of its rank's nodes — turning the sweep from
  O(rounds * ranks * edges) into O(rounds * edges). A thin wrapper kept the old
  signature for the unit test; the hot loop called a new `_with_adjacency` variant.
- **Mapped primitive:** build-once adjacency / work-proportional-to-incidence on the
  ordering hot path (same family as the KEPT [[sparse-edge-routing-obstacle-spatial-index]]
  obstacle index, applied to crossing minimization instead of edge routing).
- **Correctness:** output-identical — neighbor lists preserve parallel-edge
  multiplicity and `node_rank[n] == ranks.get(&n).unwrap_or(0)`, so each node's
  barycenter (integer position sum / neighbor count) and the downstream stable sort
  are unchanged. `rch exec -- cargo test -p fm-layout` = `428 passed; 0 failed`.
- **Outcome:** rejected and reverted. Per-crate `cc` target dir, `frankenmermaid-cli`/
  `pipeline_bench`, filter `layout_wide`, criterion `--baseline cc_xc_base` (baseline
  captured on identical-layout `main`). `8x16` `129.51 us` -> `125.55 us` change
  `+1.40%` **No change** (p=0.29), `12x24` `466.66 us` -> `471.00 us` change `+1.70%`
  **No change** (p=0.32), `16x32` `1.1338 ms` -> `1.1859 ms` change `+5.84%`
  **regressed** (p=0.00). Net zero-gain with a regression on the largest case.
- **Root cause:** the full-edge rescan is *not* the `layout_wide` bottleneck. The
  `Vec<Vec<usize>>` adjacency costs ~`2 * node_count` outer + ~`node_count` inner Vec
  allocations per layout, and that fixed build cost cancels (8x16/12x24) or exceeds
  (16x32) whatever scanning it saves — so the asymptotic win never shows at these
  sizes because the constant it replaced was already cheap.
- **frankenmermaid/Mermaid ratio after revert:** main unchanged — retained current-main
  `full_pipeline_wide` standing `1.5908 ms` / `3.7339 ms` / `6.7530 ms` vs pinned
  live-CDP Mermaid `11.12.0` `315.14 ms` / `981.73 ms` / `2879.185 ms` = Mermaid.js
  `198.10x` / `262.92x` / `426.35x` slower (`8x16` / `12x24` / `16x32`).
- **Do-not-retry note:** this is the **4th** data-structure rewrite of the
  crossing-minimization / ordering area to fail — see
  [[flat-array-total-crossings-position-edge-tables]],
  [[dense-crossing-count-position-maps]], and the stashed "local-delta
  crossing_refinement ~0 gain". Stop guessing at this stage: do **not** attempt
  further container/adjacency rewrites of barycenter or crossing counting without a
  CPU profile (e.g. `perf`/`samply` on `layout_wide/16x32`) that names the actual
  dominant function — the live candidates are Brandes-Köpf coordinate assignment and
  edge routing, not the ordering scans.

### Edge path offset Vec elision — REJECTED (2026-06-25)
- **Lever tested:** `fm-render-svg::render_edge` was changed locally to skip the
  temporary `Vec<(f32, f32)>` used to add `offset_x`/`offset_y` before calling
  `smooth_edge_path`, and instead serialize directly from `LayoutEdgePath.points`.
- **Mapped primitive:** buffer-reuse/allocation-elision from the render hot path;
  this was the fresh one-lever follow-up after the earlier SVG allocation wins.
- **Hypothesis:** wide and large flowcharts have many edges, so one fewer edge-path
  vector allocation should reduce SVG render time while preserving byte output.
- **Outcome:** rejected and reverted. Same-machine A/B against baseline worktree
  `/data/projects/.worktrees/frankenmermaid-cod-a-edge-offset-baseline-d568ce6`
  using `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a cargo bench
  -p frankenmermaid-cli --bench pipeline_bench -- render_svg/flowchart --warm-up-time 1
  --measurement-time 3` measured `small_10` `145.98 us` -> `151.50 us`
  (`3.78%` slower), `medium_100` `883.46 us` -> `1.1335 ms` (`28.30%`
  slower), and `large_500` `4.4858 ms` -> `4.5743 ms` (`1.97%` slower,
  Criterion no-change).
- **Original comparator:** fresh Mermaid `11.12.0` ESM browser bundle from
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  Node `v24.14.0` driving `/snap/bin/chromium` through Chrome DevTools
  Protocol, `maxEdges=2000`, 3 warmups, 20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio after revert:** current-main wide SVG means were
  `0.007076x` (`8x16`: `2.4348 ms` vs `344.0700 ms`), `0.006078x`
  (`12x24`: `6.2641 ms` vs `1030.5500 ms`), and `0.004201x`
  (`16x32`: `11.394 ms` vs `2711.9150 ms`), i.e. Mermaid.js was `141.31x`,
  `164.52x`, and `238.01x` slower.
- **Do-not-retry note:** do not retry this direct-offset serializer unless a fresh
  CPU/allocation profile shows the per-edge point vector allocation has returned
  to the top renderer costs; the simpler tuple-slice path currently wins.

### Dense crossing-count position maps — REJECTED (2026-06-25)
- **Lever tested:** `fm-layout::egraph_ordering::crossing_count` was changed
  locally to build dense node-position vectors where node IDs were compact and to
  count lower-position inversions with a Fenwick tree instead of the existing
  `BTreeMap` position maps plus merge-sort inversion counter.
- **Mapped primitive:** dense-index lookup and alternate inversion-count data
  structure for the e-graph/crossing-refinement hot path.
- **Outcome:** rejected and reverted. Same-worktree candidate run using
  `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b cargo bench
  -p frankenmermaid-cli --bench pipeline_bench -- layout_wide --warm-up-time 1
  --measurement-time 2` regressed every wide layout case: `8x16` `132.67 us`
  -> `153.33 us` (`15.57%` slower), `12x24` `470.95 us` -> `583.05 us`
  (`23.80%` slower), and `16x32` `1.0877 ms` -> `1.2930 ms` (`18.87%`
  slower).
- **frankenmermaid/Mermaid ratio after revert:** retained current-main
  `full_pipeline_wide` medians were `2.2269 ms`, `5.3551 ms`, and `10.986 ms`.
  Reusing the pinned live-CDP Mermaid `11.12.0` denominator from the current-main
  BOLD-VERIFY entry for identical generated wide inputs, frankenmermaid/Mermaid
  ratios are `0.005967x`, `0.004325x`, and `0.003881x`; Mermaid.js is `167.60x`,
  `231.21x`, and `257.65x` slower.
- **Do-not-retry note:** the existing `BTreeMap` plus merge-sort path wins on the
  current crossing-refinement workload; do not replace it with dense/Fenwick
  bookkeeping unless a profile shows different crossing-count shape.

### Flat-array `total_crossings` position/edge tables — REJECTED (2026-06-26)
- **Lever tested:** `fm-layout::lib::total_crossings` (the crossing counter driving
  the transpose + sifting `crossing_refinement` and the e-graph ordering pass) was
  changed locally to drop its per-call nested
  `BTreeMap<rank, BTreeMap<node, position>>` and `BTreeMap<(usize, usize), Vec<_>>`
  rebuilds in favour of flat node-indexed tables: `position: Vec<usize>` +
  `appears_rank: Vec<Option<usize>>` sized `ir.nodes.len()`, and a per-source-rank
  `Vec<Vec<(usize, usize)>>` edge bucket. Merge-sort inversion counter kept; a single
  reused `target_positions` buffer fed it.
- **Mapped primitive:** dense O(1) node-id lookup replacing tree lookups on the
  crossing-count hot path — the same family as the prior
  [[dense-crossing-count-position-maps]] reject, applied to the *sibling* counter in
  `lib.rs` rather than `egraph_ordering::crossing_count`.
- **Correctness:** output-identical — `appears_rank[n] == Some(r)` + `position[n]`
  reproduces the old `positions_by_rank[r].get(n)` lookup exactly; grouping by source
  rank is a bijection with the old `(r, r+1)` layer pairs and inversion sums are
  order-independent. `rch exec -- cargo test -p fm-layout` = `428 passed; 0 failed`,
  so the delta is pure overhead, not a behavior change.
- **Outcome:** rejected and reverted. Per-crate `cc` target dir
  (`/data/projects/.rch-targets/frankenmermaid-cc`), `frankenmermaid-cli`/
  `pipeline_bench`, filter `layout_wide`, criterion `--save-baseline cc_xc_base` on
  `main` `7b6b80c` then `--baseline cc_xc_base` after the change. Every wide case
  regressed (p < 0.05, "Performance has regressed"): `8x16` `129.51 us` ->
  `128.91 us` change `+14.53%`, `12x24` `466.66 us` -> `486.95 us` change
  `+5.59%`, `16x32` `1.1338 ms` -> `1.1688 ms` change `+4.24%`.
- **Root cause:** `total_crossings` is invoked thousands of times across the
  transpose/sifting/egraph inner loops; the fixed per-call cost of zeroing two
  `node_count`-sized flat arrays (`Option<usize>` is 16 bytes — ~12 KB zeroed per
  call at 512 nodes) plus the `Vec<Vec<_>>` outer allocation exceeds the small,
  cache-resident `BTreeMap` rebuild it replaced. Swapping the data structure does not
  help because allocation, not lookup, dominates.
- **frankenmermaid/Mermaid ratio after revert:** main is unchanged, so the retained
  current-main `full_pipeline_wide` standing holds — `1.5908 ms` / `3.7339 ms` /
  `6.7530 ms` vs pinned live-CDP Mermaid `11.12.0` `315.14 ms` / `981.73 ms` /
  `2879.185 ms` = Mermaid.js `198.10x` / `262.92x` / `426.35x` slower (`8x16` /
  `12x24` / `16x32`).
- **Do-not-retry note:** do not re-attempt a flat/dense rewrite of either crossing
  counter that allocates fresh per call — both `total_crossings` (this entry) and
  `egraph_ordering::crossing_count` ([[dense-crossing-count-position-maps]]) regress.
  A real win here must eliminate the per-call allocation entirely (scratch buffers
  reused across the refinement loop, cleared+refilled rather than re-`vec!`'d), not
  just change the container; revisit only with a CPU/alloc profile in hand.

### Borrowed SVG attribute names — REJECTED (2026-06-25)
- **Lever tested:** `fm-render-svg::Element::attr` and `attr_num` were changed
  locally to accept `Cow<'static, str>` names so literal attribute names could be
  borrowed instead of converted through `name.to_string()`.
- **Mapped primitive:** allocation elision on SVG element construction.
- **Outcome:** rejected and reverted. Fresh current-main render baseline from
  `/data/projects/frankenmermaid` measured `render_svg/flowchart` at `147.98 us`,
  `959.06 us`, and `4.6302 ms`. The candidate measured `190.09 us`,
  `953.67 us`, and `4.8435 ms`: `small_10` regressed `28.46%`, `medium_100`
  was effectively noise (`0.56%` faster), and `large_500` regressed `4.61%`.
- **frankenmermaid/Mermaid ratio after revert:** same retained current-main wide
  ratios as above: Mermaid.js remains `167.60x`, `231.21x`, and `257.65x`
  slower on 8x16, 12x24, and 16x32 generated wide flowcharts.
- **Do-not-retry note:** the generic `Cow` setter shape does not pay for itself
  on this renderer; literal-name allocation is not the next keepable lever.

### Guarded SVG attribute retain skip — REJECTED (2026-06-25)
- **Lever tested:** `fm-render-svg::Attributes::set` was changed locally to scan
  for an existing attribute name before calling `Vec::retain`, so the common
  unique-name path could skip the full retain pass while duplicate names still
  fell back to the old remove-then-push semantics.
- **Mapped primitive:** guarded shape specialization / expected-loss guard from
  the alien-artifact pass: make the rare duplicate-attribute case pay the
  removal cost, and keep the common append-only case linear in one scan.
- **Outcome:** rejected and reverted. Same-worktree A/B used target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`, package
  `frankenmermaid-cli`, bench `pipeline_bench`, and `rch exec -- cargo bench
  --profile release -p frankenmermaid-cli --bench pipeline_bench`. The isolated
  `render_svg/flowchart` filter looked strong: `small_10` `167.30 us` ->
  `148.48 us` (`11.25%` faster), `medium_100` `1.0674 ms` -> `904.51 us`
  (`15.26%` faster), and `large_500` `6.2082 ms` -> `4.5859 ms` (`26.13%`
  faster). The actual gate, `full_pipeline_wide`, failed on rerun:
  retained current-main `2.2419 ms`, `5.1960 ms`, and `9.9276 ms` vs candidate
  `2.6149 ms`, `6.4114 ms`, and `12.629 ms`, which is `16.64%`, `23.39%`,
  and `27.21%` slower.
- **Original comparator:** latest live-CDP Mermaid `11.12.0` denominator from the
  current-main BOLD-VERIFY entry, using Node `v24.14.0`, `/snap/bin/chromium`,
  dynamic import of
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  3 warmups, and 20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio after revert:** retained current-main
  `full_pipeline_wide` means were `2.2419 ms`, `5.1960 ms`, and `9.9276 ms`.
  Reusing the same generated-input Mermaid.js means from the latest main entry,
  frankenmermaid/Mermaid ratios are `0.007114x`, `0.005293x`, and `0.003448x`;
  Mermaid.js is `140.57x`, `188.94x`, and `290.02x` slower. The rejected
  candidate would have worsened those to `0.008298x`, `0.006531x`, and
  `0.004386x`, i.e. Mermaid.js only `120.52x`, `153.12x`, and `227.98x`
  slower.
- **Do-not-retry note:** do not land `Attributes::set` scan-before-retain on
  render-only evidence. Any future attribute dedup work must clear
  `full_pipeline_wide` because the renderer microbench result did not predict
  the end-to-end gap.

### Removing `Attributes::set` dedup entirely — REJECTED (correctness) (2026-06-26)
- **Lever tested:** following the [[wide-pipeline-stage-split-svg-render-dominates-not-layout]]
  finding (render is 63–70% of the wide pipeline; per-element cost ~`2.34 us`), the
  per-insert `self.attrs.retain(|a| a.name != name)` in `fm-render-svg::Attributes::set`
  was removed outright (push directly, O(attrs²)→O(attrs) element construction), guarded
  by a `debug_assert` that no name is set twice. The hypothesis was that `class`/`style`
  have their own merge paths so plain setters never overwrite, making the dedup pure
  overhead.
- **Outcome:** rejected — **not output-preserving**, so never benched. With the
  `debug_assert` active, `rch exec -- cargo test -p fm-render-svg` failed ~all render
  tests: many render paths *do* call `set` twice for the same attribute and rely on
  last-wins (e.g. a default `fill`/`stroke`/`style` later overridden). The `retain` is
  **load-bearing**, not overhead.
- **Why no output-identical speedup exists:** `retain`+push gives last-wins *and* moves
  the overwritten attribute to the end of the list; any cheaper scheme that skips the scan
  (push-only) emits duplicate attributes, and any in-place replace changes attribute order
  — both alter the serialized tag. So the O(n) scan per `set` cannot be removed without
  changing bytes. This complements [[guarded-svg-attribute-retain-skip]] (which kept dedup
  but regressed `full_pipeline_wide`): the dedup can be neither removed (breaks output) nor
  skipped (regresses end-to-end).
- **frankenmermaid/Mermaid ratio:** unchanged — main untouched; retained current-main
  `full_pipeline_wide` standing `1.5908 ms` / `3.7339 ms` / `6.7530 ms` vs pinned live-CDP
  Mermaid `11.12.0` `315.14 ms` / `981.73 ms` / `2879.185 ms` = Mermaid.js `198.10x` /
  `262.92x` / `426.35x` slower.
- **Do-not-retry note:** stop probing `Attributes::set` dedup — both removal (this entry)
  and guarded-skip ([[guarded-svg-attribute-retain-skip]]) are closed. The wide-render
  per-element cost lives elsewhere (per-attribute value allocation in `AttributeValue`,
  `TextBuilder` owned-String fields, the Element tree build itself); target those with a
  `wide_stages render` gate, not the attribute dedup.

### Owned accessibility title element path — REJECTED (2026-06-26)
- **Lever tested:** `describe_node` / `describe_edge` already return owned
  accessibility title strings, but `Element::title(&desc)` cloned them again into
  `text_content`. A new `Element::title_owned(String)` path moved those generated
  strings directly into the title element, then the node and edge text-alternative
  call sites used it.
- **Mapped primitive:** value-shape specialization / finite-state template emission from
  the alien-artifact pass: avoid a generic borrowed-to-owned conversion when the hot
  producer already owns the value.
- **Baseline -> After:** per-crate `frankenmermaid-cli`, `pipeline_bench`, warm target
  dir `/data/projects/.rch-targets/frankenmermaid-cod-a`. Fresh local-fallback
  `wide_stages/render/16x32` baseline from current main was `5.2354 ms`; candidate
  focused rerun was `6.1373 ms` (`+17.23%`). Candidate `full_pipeline_wide` means
  were `1.7957 ms`, `4.3769 ms`, and `8.7596 ms`; Criterion reported no change on
  `8x16`/`12x24` and a `+8.20%` regression on `16x32`.
- **Original comparator:** latest live-CDP Mermaid `11.12.0` denominator reused from
  the current-main BOLD-VERIFY entry for identical generated wide inputs: Node
  `v24.14.0`, `/snap/bin/chromium`, dynamic import of
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`, 3 warmups,
  20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio:** candidate full-pipeline wide means versus Mermaid.js
  means `315.14 ms`, `981.73 ms`, and `2879.185 ms` give ratios `0.005698x`,
  `0.004458x`, and `0.003042x`; Mermaid.js is `175.50x`, `224.30x`, and `328.69x`
  slower. The retained current-main standing remains `198.10x`, `262.92x`, and
  `426.35x` slower.
- **Verdict:** rejected; the clone removal is too small to help the common cases and
  regressed the large wide render/full-pipeline gate. Code was manually reverted before
  commit; no production source diff remains.
- **Do-not-retry note:** accessibility title clone elision is not the wide-render
  lever. The remaining per-element cost is dominated by larger Element/tree/attribute
  construction, not this final owned-string handoff.

### TextBuilder multiline Vec removal — REJECTED (2026-06-26)
- **Lever tested:** `TextBuilder::build` eagerly collected `self.text.lines()` into a
  `Vec<&str>` even for the common single-line label path. The candidate consumed the
  first two lines directly from the iterator, preserving the old single-line content
  behavior and only building `tspan` children for real multiline labels.
- **Mapped primitive:** allocation removal / hot-path specialization from the
  wide-render pass: remove one per-text temporary collection while keeping the
  serialized SVG byte shape for one-line labels.
- **Baseline -> After:** per-crate `frankenmermaid-cli`, `pipeline_bench`, warm target
  dir `/data/projects/.rch-targets/frankenmermaid-cod-a`. The RCH run selected different
  workers for current-main baseline (`hz2`: `1.0273 ms`, `2.3335 ms`, `4.3177 ms`) and
  candidate (`ovh-a`: `0.96134 ms`, `2.1719 ms`, `3.9782 ms`), so it was routing
  context only. A same-machine toggle rejected the candidate: candidate render means
  `1.1551 ms`, `3.1485 ms`, `5.4789 ms` versus reverted current-main baseline
  `1.1304 ms`, `2.9145 ms`, `5.8246 ms`, i.e. `+2.18%`, `+8.03%`, and `-5.94%`.
- **Original comparator:** latest live-CDP Mermaid `11.12.0` denominator reused from
  the current-main BOLD-VERIFY entry for identical generated wide inputs: Node
  `v24.14.0`, `/snap/bin/chromium`, dynamic import of
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`, 3 warmups,
  20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio:** retained current-main `full_pipeline_wide` standing
  remains `1.5908 ms`, `3.7339 ms`, and `6.7530 ms` vs Mermaid.js `315.14 ms`,
  `981.73 ms`, and `2879.185 ms`, so Mermaid.js is `198.10x`, `262.92x`, and
  `426.35x` slower. The rejected render-stage candidate alone was `272.82x`,
  `311.82x`, and `525.51x` faster than those full Mermaid denominators, but it was not a
  same-machine win over current main.
- **Verdict:** rejected; the small allocation removal did not beat current main on the
  common `8x16` and `12x24` wide-render gates. Code was manually reverted before commit;
  no production source diff remains.
- **Do-not-retry note:** do not revisit `TextBuilder::build` line-collection elision as a
  standalone wide-render lever. The next candidate needs to remove larger Element/tree
  construction work or change the generated element count.

### Common `-->` flowchart parser shortcut — REJECTED (2026-06-25)
- **Lever tested:** `parse_fast_simple_flowchart_edge_ast` was changed locally to
  try a guarded exact `-->` shortcut before scanning the full fast-operator table.
- **Mapped primitive:** branch-specialized parser shortcut for the benchmark's
  most common plain flowchart edge operator.
- **Outcome:** rejected and reverted. Candidate run
  `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b cargo bench
  -p frankenmermaid-cli --bench pipeline_bench -- parse/flowchart --warm-up-time 1
  --measurement-time 2` measured `small_10` `31.280 us`, `medium_100`
  `550.33 us`, and `large_1000` `3.7202 ms`; Criterion reported no change for
  `small_10`/`large_1000` and a clear `medium_100` regression. Compared with the
  current-head routing baseline (`31.455 us`, `270.26 us`, `3.6663 ms`), the
  candidate moved `-0.56%`, `+103.63%`, and `+1.47%`.
- **frankenmermaid/Mermaid ratio after revert:** same retained current-main wide
  ratios as above: Mermaid.js remains `167.60x`, `231.21x`, and `257.65x`
  slower on the generated wide flowchart pipeline.
- **Do-not-retry note:** the extra guarded shortcut adds branch/overlap cost
  without a stable parse win; keep using the existing fast-operator table until a
  profile isolates a different parser primitive.

### Plain flowchart label shortcut — REJECTED (2026-06-25)
- **Lever tested:** `parse_fast_simple_flowchart_node_ast` was changed locally to
  bypass `parse_label`, icon-prefix extraction, and empty-label cleanup for ASCII
  bracket labels with no entity, quote, markdown marker, icon delimiter, or
  non-ASCII leading emoji.
- **Mapped primitive:** guarded shape specialization / partial-evaluation fast
  path from the alien-artifact pass: use a static plain-label variant only when
  the value-shape guard is exact, and keep the generic parser as deopt fallback.
- **Outcome:** rejected and reverted. Same-machine A/B used target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`, package
  `frankenmermaid-cli`, bench `pipeline_bench`, filter `parse/flowchart`.
  Baseline command was attempted through `rch exec` and fell back locally because
  no worker was admissible:
  `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b rch exec
  -- cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench
  -- parse/flowchart --warm-up-time 1 --measurement-time 2`. A candidate rerun
  through `rch exec` selected worker `vmi1264463` but failed before benchmarks
  because `cmake` was missing while building `highs-sys`; that run is a toolchain
  blocker, not benchmark evidence. The valid candidate proof was local with the
  same target dir and filter.
- **Baseline -> After:** `parse/flowchart/small_10` `29.895 us` -> `30.151 us`
  (`0.86%` slower/noise), `medium_100` `240.60 us` -> `281.80 us` (`17.12%`
  slower), and `large_1000` `3.0699 ms` -> `4.0402 ms` (`31.61%` slower).
- **Original comparator:** latest live-CDP Mermaid `11.12.0` denominator from the
  current-main BOLD-VERIFY entry, using Node `v24.14.0`, `/snap/bin/chromium`,
  dynamic import of
  `https://cdn.jsdelivr.net/npm/mermaid@11.12.0/dist/mermaid.esm.min.mjs`,
  3 warmups, and 20 timed render-to-SVG iterations.
- **frankenmermaid/Mermaid ratio after revert:** fresh current-main
  `full_pipeline_wide` means were `2.2419 ms`, `5.1960 ms`, and `9.9276 ms`.
  Reusing the same generated-input Mermaid.js means from the latest main entry,
  frankenmermaid/Mermaid ratios are `0.007114x`, `0.005293x`, and `0.003448x`;
  Mermaid.js is `140.57x`, `188.94x`, and `290.02x` slower.
- **Do-not-retry note:** even the narrow plain-label guard added enough extra
  branch/scanning cost to regress medium and large flowchart parsing; do not
  retry label-shape specialization unless a new parser profile shows `parse_label`
  itself, not line/edge dispatch, back at the top.

## Blocked/Invalid Evidence Attempts

### IrGraph adapter build is dead in the render pipeline — but cheap, so a low-priority parse lever (2026-06-27)
- **Rigorously traced:** the production IR builder eagerly populates `ir.graph` (the FNX adapter) —
  `ir.graph.nodes` per node (`fm-parser/src/ir_builder.rs:872`) and `ir.graph.edges` per edge
  (`:1265`), ~1,536 structs for a 16x32 wide diagram. But **every production read path rebuilds from
  `ir.edges` instead**: `fnx_adapter::ir_to_graph` iterates `ir.edges` (`fnx_adapter.rs:181/554/614`),
  `fnx-integration` is a **non-default** feature (`fm-layout/Cargo.toml: default = []`), and the only
  `ir.graph.{nodes,edges}` *reads* anywhere are in **test modules** (fm-core `mod tests` @5008+;
  mermaid_parser.rs:9135/9877 asserts) — never the default parse→layout→render pipeline, never the
  SVG renderer (its `ir.graph.*` lines are test-fixture `push`es). The 3 `ir.graph.edges =` rewrites
  in fm-layout are all test fixtures (`sample_layout_dependency_ir`, etc.).
- **Why it's parked, not landed:** the structs are **cheap** — `IrGraphNode` = {node_id, kind, two
  `Vec`s that stay empty for non-subgraph nodes}; `IrGraphEdge` = all `Copy` fields. So the dead work
  is ~1,536 struct moves + two `Vec` growths, **no per-struct heap allocs** for a plain flowchart →
  an estimated ~2-5% of parse (sub-1% of pipeline). Eliminating it needs a `ParserConfig`
  `build_graph_adapter` flag (default true to preserve the tested `ir.graph` output contract) wired
  through to the fm-cli render path (set false) — a cross-crate change whose full-pipeline validation
  is **blocked by the concurrent broken fm-render-svg working tree** (fm-cli won't build). Modest win
  vs real landing cost → parked. Recorded so it isn't re-investigated.
- **frankenmermaid/Mermaid ratio:** unchanged — no source change. Standing `226x`–`506x`
  (worker-dependent) over Mermaid `11.12.0`.

### Dead-CSS prune VALIDATED — landed concurrently; standing down from fm-render-svg to avoid collision (2026-06-27)
- **The lever works and is partly landed.** Last cycle's dead-CSS-prune finding (~27% of the
  `<style>` is unused per diagram; emit feature rule groups only when the feature is present) was
  **landed concurrently by another agent** as `701b265` *"fm-render-svg: stream XML escaping and
  emit edge-label CSS only when edges are labeled"* — the exact edge-label group I implemented this
  cycle (gated on `ir.edges` having labels). My parallel implementation is **byte-identical to HEAD**
  (`git diff theme.rs` empty), i.e. fully redundant. The same agent also landed an **SVG streaming
  render** win (`c605ab5`) — the big lever I had deferred for many cycles. Both confirm the
  directions in this ledger were correct.
- **Why I produced no source change this turn:** HEAD advanced `b9a9819 → c605ab5` mid-cycle (same
  `main`, same working dir — a concurrent agent is actively committing here). The working tree also
  holds that agent's **in-flight, uncommitted** refactor (`attributes.rs` privatised
  `escape_xml_text`; `lib.rs:4500` `.content(&label_text)` currently fails `String: From<&Cow>`),
  which **breaks the fm-cli build** and therefore blocks `golden_svg_test` regeneration. That break
  is **not on HEAD** (`HEAD:lib.rs:4500` is unrelated) — transient working-tree state that resolves
  when they commit. I did not touch their files (`git add` doc only).
- **Continuation for whoever owns fm-render-svg byte work:** the remaining dead-CSS groups beyond
  edge-label are the next wins — **cluster** rules (needs a *render-matched* detector, NOT
  `!layout.clusters.is_empty()` which is over-inclusive for the layered wide layout; use the IR's
  real subgraph presence, verified against `class="fm-cluster"` in the body), **dashed/thick edge**
  rules (`ir.edges` arrow types), and diagram-type-specific groups (block-beta/c4/swimlane via
  `ir.diagram_type`). Same append-after-core pattern as `701b265`.
- **frankenmermaid/Mermaid ratio:** unchanged — no source change. Standing `226x`–`506x`
  (worker-dependent) over Mermaid `11.12.0`.

### Dead-CSS prune (~27% of the `<style>` is unused per diagram) — HIGH VALUE, blocked on reliable feature detection (2026-06-27)
- **The lever (biggest remaining byte gap):** the embedded `<style>` is **universal** — it carries
  rules for features/diagram-types not present in the current render. Measured on a flowchart (`w40`):
  **~2,571 B (~27%) of the ~9.5 KB CSS is dead** — `.fm-cluster*`, `.edge-label`/`.fm-edge-labeled`,
  `.fm-edge-dashed`/`.fm-edge-thick`/`.fm-edge-back`, `.fm-node-block-beta`, `.fm-cluster-c4`/
  `-swimlane`, `.fm-node-inactive`, `.fm-label`. For a CSS-dominated small SVG (CSS is 79%) that's
  **~20% of the whole file**; ~1.5% of wide. Pruning is also a small *time* win (less CSS to build).
- **Implementation that works structurally:** the feature groups are **contiguous blocks** in the
  single 330-line format-arg `write!`. They can be pruned without splitting that `write!` — *remove*
  the block from the template and conditionally **`push_str` it back after the core** (order is
  irrelevant; distinct selectors). Verified this compiles + 220 tests pass + clustered diagrams keep
  the rules.
- **Blocked on feature detection (found by attempting the cluster group):** the obvious detector
  `!layout.clusters.is_empty()` is **over-inclusive** — it is *true* for `w40` (the layered wide
  layout populates `layout.clusters` with groupings that are **not** rendered as `.fm-cluster`
  elements), so the prune never fires for the benched flowchart. The render is still correct
  (conservative = keep), but there's no win. A reliable detector must reflect what is *actually
  emitted* (e.g. the IR's real subgraph presence, or edge-label/edge-style scans), and a wrong
  detector in the *other* direction (under-inclusive) would silently drop a needed rule — which the
  golden snapshot would absorb. Reverted pending an accurate per-feature detector.
- **Next step:** build a `CssFeatures { has_clusters, has_labeled_edges, has_styled_edges, … }` from
  the **IR** (not the layout), verified against `class="fm-…"` presence in the rendered body for each
  feature, then prune each contiguous block via the append-after-core pattern above. Estimated
  ~2.5 KB (~20% small / ~1.5% wide) + small time. This is the single biggest remaining byte lever.
- **frankenmermaid/Mermaid ratio:** unchanged — reverted. Standing `226x`–`506x` (worker-dependent)
  over Mermaid `11.12.0`.

### Byte-reduction frontier: CSS minify blocked; attr levers exhausted; post-gates standing holds — FINDING (2026-06-27)
- **CSS minify (the remaining byte lever) is blocked.** After the inline-vs-CSS gates the embedded
  `<style>` is the largest single chunk (~9.7 KB, **79% of a small SVG**), and it's pretty-printed
  vs Mermaid's minified CSS. Measured minify headroom: ~426 B indentation + 209 B comments + 357 B
  newlines ≈ **~635–992 B** (~5–8% of small/CSS-dominated SVGs, ~0.4% of wide). But: (a) a gen-time
  minify needs the **~330-line `write!` raw-string template** in `to_svg_style` rewritten as a
  minified single line — too large/error-prone to do safely (placeholders, format args); (b) a
  post-build minify *pass* reprocesses the 9.7 KB string and **regresses render time** (documented
  +19% earlier). So the clean, time-neutral path isn't tractable. Comments can't be dropped without
  losing source documentation (output == raw-string source). Deferred.
- **The clean attr levers are exhausted.** The inline-vs-CSS/root series (edge fill/stroke, node
  stroke/stroke-width, drop-shadow filter+def, **font-family root-move −6.83%**) is complete; the
  remaining label attrs are blocked (`fill` per-class CSS coverage missing — prior entry) or
  per-label varying (`text-anchor`, `font-size`). Remaining inline attrs are geometry (`x`/`y`/
  `transform`) or functional (`<title>` a11y).
- **Post-gates standing — holds (BOLD-VERIFY).** `full_pipeline_wide` on `hz2`: `1.3926` / `3.3312`
  / `6.2023 ms` = **`226x` / `295x` / `464x`** vs live-CDP Mermaid `11.12.0`. Lower than the prior
  `ovh-a` standing (`240x`/`319x`/`506x`) purely because `hz2` is a slower worker (cross-worker, plus
  ±2–3% variance) — **not a regression**. The byte wins are gated on the default CSS-on path so they
  *are* in this pipeline; their render-time benefit (fewer per-element attr builds) is real but
  sub-noise, so the headline ratio is bounded by worker speed, not the codebase.

### Label-text `fill` is NOT cleanly gateable like font-family — per-class CSS coverage is mostly absent (2026-06-27)
- **Why investigated:** after the `font-family` root-move (−6.83%), the label `<text>` still carries
  `fill="#1a1a2e"` (= `--fm-text-color`); the obvious next step was to gate it like the node stroke.
- **Blocked — coverage is per-label-class and mostly missing.** Unlike `font-family` (uniform across
  every label, so movable to the root via inheritance), `fill` is gated only if the label class has a
  CSS `fill` rule that overrides the inline for CSS-on. Verified coverage: `.fm-node text`,
  `.edge-label`, `.fm-cluster-label` = **covered**; `.fm-sequence-fragment-label`, `.fm-er-cardinality`,
  `.fm-sequence-mirror-header`, `.fm-er-attr`, `.fm-state-label` (and the gantt/pie/quadrant/xychart
  label classes) = **NOT covered**. Gating an uncovered class's inline fill would drop its color (it
  has no CSS/inherited source) — a silent visual regression the golden snapshot would absorb.
- **Also sprawling:** label fill is emitted at **~42 sites** across *two* variables
  (`.fill(&colors.text)` ×16 in node/compartment contexts, `.fill(&theme.colors.text)` ×26 in
  diagram-specific contexts) **plus** the plain-label fast path — and the per-element root-fill trick
  used for font-family is unsafe here because `fill` (unlike `font-family`) is also used by *shapes*,
  so a root `fill` would be wrongly inherited by any shape lacking its own fill.
- **What it would take:** add CSS `fill` rules for *all* ~15 label classes in `to_svg_style`, then gate
  all 42 sites + the fast path with per-site coverage verification — a large, error-prone change for an
  estimated ~1–2% (vs the font-family's −6.83%). Deferred. The clean, uniform inline-vs-CSS/root win was
  `font-family`; the remaining label attrs (`fill` per-class, `text-anchor`/`font-size` per-label varying)
  do not transfer cleanly.
- **frankenmermaid/Mermaid ratio:** unchanged — investigation only. Standing `240.5x`/`319.1x`/`505.7x`
  (latest) over Mermaid `11.12.0`.

### Node inline-style gating (the node analog of the landed edge fill/stroke gates) — BLOCKED, deferred refactor (2026-06-26)
- **Why investigated:** the edge `fill`/`stroke` gates (Kept Wins) landed cleanly because each is a
  single site under one CSS rule. Nodes carry far more inline presentation bytes
  (`fill="url(#fm-node-gradient)"` ~30 B, `stroke` ~17 B, `stroke-width`), so the same lever would
  be a bigger win — *if* it transfers.
- **Node `fill` — NOT redundant.** The inline is the **gradient** `url(#fm-node-gradient)`, but the
  unconditional CSS `.fm-node rect, path, circle, ellipse, polygon { fill: var(--fm-node-fill) }`
  is a **solid** color (`--fm-node-fill: #ffffff`). They differ, so gating the inline fill would
  change the look (gradient → solid), not a no-op. Cannot gate.
- **Node `stroke` / `stroke-width` — redundant but not 1-site.** They *are* CSS-redundant: the
  unified rule sets `stroke: var(--fm-node-accent); stroke-width: 1.6`, the inline `stroke="#e2e8f0"`
  equals `--fm-node-stroke` (already overridden by the accent for CSS-on), and custom `classDef`/
  `style` colors are emitted as a **separate `style="fill:…; stroke:…"`** that wins (verified:
  `style A fill:#ff0000,stroke:#00ff00` → base stroke stays `#e2e8f0`, separate `style=` carries the
  custom). **But** `.stroke(&colors.node_stroke)` appears at **~28 shape-builder sites** in
  `render_node`'s `match shape`, spanning multiple element types — `all_node_shapes` emits node
  strokes on `<rect>`, `<path>`, `<circle>` **and `<line>`**, and `<line>` is covered by a *separate*
  `.fm-node line` rule (different stroke-width), so coverage is non-uniform. Safe gating needs a
  shared `apply_node_base_styling(elem, colors, config)` helper that every branch routes through,
  plus exhaustive per-element-type CSS-coverage verification (a non-covered element would silently
  lose its stroke under CSS — the golden snapshot would absorb the regression without flagging it).
- **Conclusion:** node gating is a **deferred refactor**, not the quick win the edge gates were. The
  edge inline-vs-CSS lever is complete (fill + stroke landed); the node extension is parked behind
  the shape-builder-helper refactor.
- **frankenmermaid/Mermaid ratio:** unchanged — investigation only. Standing `240.5x`/`319.1x`/
  `505.7x` (latest) over Mermaid `11.12.0`.

### Dead-output sweep of the benched render is exhausted after the `data-fm-node-id` drop — FINDING (2026-06-26)
- **Why:** last cycle's `data-fm-node-id` removal (a pure dead duplicate) worked via a repo-wide
  consumer search; this cycle applies the same technique to *every* remaining node/edge attribute
  and class emitted by the legacy flowchart render, to see if more byte can be shed.
- **Systematic consumer-check result — nothing else is dead:**
  - `data-id` — documented (`README.md`), the node identifier → **keep**.
  - `data-fm-edge-id` — undocumented and **zero** JS/CSS/doc consumers, *but* it carries the
    unique edge **index** (the only thing that disambiguates parallel `A-->B` edges; not a
    duplicate like node-id was), so removing it is a feature removal, not a redundancy cleanup →
    **keep** (also: just alloc-optimized via `attr_int` two cycles ago).
  - `fm-node-accent-N`, `fm-node-shape-X` — both have real emitted CSS rules
    (`.fm-node-accent-* { --fm-node-accent: color-mix(...) }`, `.fm-node-shape-*`) and are
    user-facing styling hooks → **keep**.
  - `id` (element id) — DOM targeting + mermaid-compat → **keep**.
  - `fm-source-span` / `fm-source-index` / `fm-source-kind` — the intentional source-map set
    (the redundant individual span attrs were already dropped in an earlier cycle); scene-path
    only (not in the benched flowchart), a feature with possible external (IDE) consumers → **keep**.
- **Conclusion:** the benched flowchart render now emits **no dead output**; `data-fm-node-id`
  was the last clear redundancy. Further SVG-byte reduction vs Mermaid is bounded by (a) the
  fixed ~9.7 KB CSS (byte-only / time-neutral), (b) frankenmermaid *enhancements* over Mermaid
  (per-node accent/shape styling — intentional, not bloat), and (c) contract-bound attrs
  (`data-fm-edge-id`, source-map). **Do not re-hunt benched-render dead-output.**
- **frankenmermaid/Mermaid ratio:** unchanged — sweep only. Standing `240.5x`/`319.1x`/`505.7x`
  (latest) over Mermaid `11.12.0`; `198x`–`426x` floor.

### CSS-building is sub-bar: stale 4 KB `to_svg_style` capacity (2 reallocs/render) saves only ~1.6% small / ~0% wide — REVERTED (2026-06-26)
- **Lever:** `Theme::to_svg_style` (theme.rs:469) builds the ~**9,741-byte** theme CSS (verified
  by measuring the `<style>` block of a default render) starting from `String::with_capacity(4096)`
  — a value from Feb (`4f08f5f1`), stale since the utility-class block grew. Building 9.7 KB from
  4 KB forces **2 reallocs/render** (4 K→8 K→16 K). Bumped it to `12 * 1024`. Byte-identical
  (`cargo test -p fm-render-svg` = **219 pass**), sign-known ≥0 (a capacity increase can't regress).
- **Why reverted (measured + derived):** the `render_svg/flowchart/small_10` baseline is
  **76.557 µs** (rch/ovh-a, cmake OK, ±0.05%). The 2 avoided reallocs are ≈1.2 µs of memcpy →
  **~1.6% of small_10**, and `large_500` is `2.399 ms` so the same ~1.2 µs is **~0.05%**. The CSS
  block is a *fixed* ~9.7 KB, so its build cost is a vanishing fraction of any non-trivial render,
  and even `small_10` is dominated by node/edge/defs building, not the CSS. **Below the 3% keep
  bar and the ~5–15% shared-worker noise floor** → ~0-gain on the headline; reverted per policy.
- **Closes the CSS-building lever:** all `to_svg_style`-side micro-opts (capacity, write-direct to
  avoid the intermediate-String copy, default-theme caching) are bounded by the same few-µs CSS
  cost — **sub-bar on small, negligible on wide.** Do not pursue. (The CSS *byte* size is a real
  gap vs Mermaid's minified CSS, but byte-only/time-neutral → ~0-gain on a time bench; see prior
  CSS-minify entries.)
- **frankenmermaid/Mermaid ratio:** unchanged — reverted. Standing `240.5x`/`319.1x`/`505.7x`
  (latest run) over Mermaid `11.12.0`; `198x`–`426x` floor.

### Full `fm-cli` suite regression check after the session's wins — HEALTHY; `fnx_differential` is flaky-under-contention — FINDING (2026-06-26)
- **Why:** the session landed several render/parse wins (span_all, markers, spans, parser); a
  comprehensive check confirms no git-tracked fixture was left stale (as `golden_svg_test` had
  been) and nothing regressed.
- **Result:** `cargo test -p frankenmermaid-cli` (full suite, parallel) → **125 tests pass**,
  with **3 apparent failures** all in `fnx_differential_report.rs`
  (`differential_all_golden_cases_pass_gate`, `…_summary_statistics`, `…_hub_spoke_quality`),
  reporting e.g. `all_node_shapes: render_time_regression 277.6% > 100% threshold`.
- **Diagnosis — flaky, NOT a regression:** that test measures `render_us` with a **single
  `Instant::now()`** around one render call (in-process FNX-on vs FNX-off; see
  `fnx_differential_report.rs:165`), gated at a 100% delta threshold. Single-shot timing spikes
  2–3× from a context switch on a contended host — and my box was saturated with the session's
  builds/benches during the *parallel* full-suite run. Re-running the binary **in isolation 3×
  → 11 passed / 0 failed every time** (deterministic green). My render changes only *reduce*
  render work (markers 12→2, fewer attrs), so they cannot cause a render-time *regression*.
- **Conclusion:** the codebase is **healthy** after the session's wins; the 3 failures are
  single-shot-timing flakiness under parallel-suite CPU contention, not a code regression.
- **Flagged for the test owner (not changed here — passes in isolation, so not "broken"):**
  `fnx_differential_report` render/layout timing should use **min-of-N** samples (the minimum
  is the least-contended sample, closest to true cost) instead of a single shot, to stop false
  `render_time_regression` failures when the suite runs under load.
- **frankenmermaid/Mermaid ratio:** unchanged — verification only. Standing `240.5x` / `319.1x`
  / `505.7x` (this run) over Mermaid `11.12.0`; `198x`–`426x` floor.

### Frontier closed: the last two candidate levers are not viable; git-tracked snapshots GREEN — FINDING (2026-06-26)
- **Parse dedup-map hash-key (the lever scoped in the entry below) — NOT VIABLE.** On
  inspection, `node_index_by_id` has **11 lookup sites** (`intern_node_auto`,
  class/style/click/link handlers, etc.), all of which already use *borrow-based* `get(&str)`
  (no clone) — only the single `insert` clones the id. Converting that map to a hash-keyed
  `FxHashMap<u64, SmallVec<IrNodeId>>` with verify-on-hit would touch all 11 sites and add a
  per-lookup id comparison, for a ~1.2% saving (insert clones only). The `label_index_by_text`
  map is contained (one function) but is only ~1.5–2%. Neither alone clears the 3% bar, and
  the node half is far too invasive/correctness-critical (a wrong-dedup bug merges nodes) for
  its size. **Do not pursue.** (`SmallVec` would also be a new direct dep; `IrLabelSegment`
  does derive `Hash`/`Eq`, so that part is feasible — but moot.)
- **`data-id` / `data-fm-node-id` duplicate node-id attrs — ~~NOT REMOVABLE~~ → SUPERSEDED,
  `data-fm-node-id` DROPPED (see Kept Wins, 2026-06-26).** This bullet's "keep" reasoning was
  **wrong**: `data-fm-edge-id` carries the *unique edge index*, whereas `data-fm-node-id` is a
  *pure duplicate* of `node_id` (= the documented `data-id`), so the "parallel selection scheme"
  argument does not hold. A repo-wide consumer search (`*.js/ts/tsx/css`, `querySelector`/
  `getAttribute`/`dataset`, README/docs) found **zero** consumers of `data-fm-node-id` — it is
  dead output, exactly like the `data-fm-source-*` set. Dropped it (kept the documented
  `data-id`).
- **Git-tracked snapshot integrity — GREEN.** After fixing `golden_svg_test` last cycle, ran
  the other git-tracked snapshot/checksum tests: `golden_layout_test` (2), `mermaid_compat_test`
  (2), `fnx_baseline_invariants` (7) all pass — no other fixture was left stale by the landed
  render/parse wins.
- **Net frontier status:** the codebase is optimized to the point where remaining levers are
  (a) **sub-5% and noise-bound** on the shared bench hosts (layout edge-routing — see
  `intersects_segment` ~0-gain), (b) **byte-only / time-neutral** so reverted on a time bench
  (CSS minification), (c) **contract-bound** (`data-id`), or (d) a **large streaming-render
  refactor** (eliminate the Element tree) — the only remaining >5%-ceiling lever, deferred as
  high-effort/high-risk. The gating constraint is no longer the codebase; it is a
  **dedicated low-contention bench host** to verify sub-5% effects.
- **frankenmermaid/Mermaid ratio:** unchanged — no source change. Retained `full_pipeline_wide`
  standing `198.10x` / `262.92x` / `426.35x` vs live-CDP Mermaid `11.12.0`.

### Parse profile post-`span_all`: detection ~0%, IR-build 99%; next lever is dedup-map key double-alloc — FINDING (2026-06-26)
- **Profiled (via `PARSE_PROFILE` env split in `parse_with_mode_and_config`, run on the
  cmake-free `parse_bench`):** for `flowchart/large_1000` (≈30.6 KB input) the stages split
  `detect ≈ 6 µs` vs `parse_build ≈ 2.5 ms` — **detection is ~0.3%; the IR build is 99%+** of
  parse. So further parse wins must come from the IR build (`parse_flowchart` →
  `IrBuilder`), not detection (already guarded).
- **What's left in the IR build (inspected, all sub-3% or contract-bound):**
  - **node-id double-alloc:** `intern_node_auto` allocates the id once for `IrNode.id`
    (`String`, public field) and again as the `node_index_by_id: FxHashMap<String, …>` key —
    two `String`s per unique node (~1000 for `large_1000`). The map key is transient (freed
    after parse) but still allocates.
  - **label text double-clone:** `intern_label` builds `key = (text.clone(), segments.clone())`
    *before* the lookup, then clones `text` again into `IrLabel.text` — two text clones per
    unique label.
  - `ir.graph` (the FNX-adapter `IrGraphNode/IrGraphEdge` built in `push_edge`/node intern) is
    **not** dead — it is read by the default layout (`primary_region_owners`,
    `node_label_text`, block-beta grid), so it cannot be dropped like `span_all` was.
- **Next lever (specific, estimated ~3–4% combined, moderate refactor):** replace the two
  owned-`String`/tuple dedup-map keys (`node_index_by_id`, `label_index_by_text`) with
  **hash-keyed** maps (`FxHashMap<u64, SmallVec<IrLabelId|IrNodeId>>`, hash of the
  id/label-text), resolving collisions by comparing against the already-owned `ir.nodes[i].id`
  / `ir.labels[i].text`. This eliminates the per-unique-node/label **key** `String` allocation
  (the `IrNode.id` / `IrLabel.text` allocation stays). Correctness-critical (a wrong-dedup bug
  merges nodes/labels), so it needs the full `fm-parser` test suite + a clean same-worker
  `parse_bench` A/B — both now available. Deferred rather than rushed.
- **frankenmermaid/Mermaid ratio:** unchanged — measurement/finding only, no source change.
  Retained `full_pipeline_wide` standing `198.10x` / `262.92x` / `426.35x` vs live-CDP Mermaid
  `11.12.0`.
- **Do-not-retry note:** detection is not worth touching (~0.3%); `ir.graph` is not dead; the
  hash-keyed dedup is the one remaining ≥3%-candidate parse lever and must be measured, not
  assumed.

### cmake-free `fm-parser` parse bench + the per-worker-target-dir A/B blocker — INFRA (2026-06-26)
- **Shipped (infra):** `crates/fm-parser/benches/parse_bench.rs` (+ criterion dev-dep). The
  full-pipeline `pipeline_bench` lives in `fm-cli`, which pulls in `fm-layout → highs-sys →
  cmake`; the recurring `cmake`-less worker `vmi1227854` fails that build (`exit 101`). But
  `fm-parser` has **no** `highs-sys`/`fm-layout` dependency, so this bench builds and runs on
  **every** worker — verified: `rch exec -- cargo bench -p fm-parser` completed with **zero**
  `cmake`/`highs-sys` references, parse times on `ovh-a` `flowchart/small_10` `18.54 µs`
  (±0.15%, very stable), `flowchart/large_1000` `2.109 ms`. Parse (≈21% of the wide pipeline)
  is now independently, reliably benchable.
- **Deeper blocker now diagnosed (the real reason A/B has been unreliable all along):** rch
  gives each worker its **own** `CARGO_TARGET_DIR` suffix (`.rch-target-<worker>-pool-…`), and
  `rch exec` has **no worker-pin flag** (confirmed: `exec` exposes only `-v`; a top-level
  `--worker` is rejected by `exec`). So a criterion baseline saved on one worker is invisible
  to another: a `--baseline` run that lands on a different worker panics
  `Baseline '…' must exist`. Reliable A/B therefore requires *both* runs to land on the same
  worker — an uncontrollable lottery. This cycle, the baseline saved on `ovh-a` but both
  `--baseline` retries routed to `vmi1227854` → comparison impossible. **This — not just the
  missing cmake — is why cross-cycle layout/render A/Bs have been noisy/uncomparable.**
- **`span_all` is write-only dead data (noted, unverified-this-cycle):** `IrNode.span_all:
  Vec<Span>` (fm-core, serialized) is **never read** anywhere in the workspace (1 push site +
  init; 0 readers), yet `IrBuilder::intern_node_auto` pushes one `Span` per node *reference*
  (~2/edge). Removing the per-reference push was attempted but could not be A/B'd (the
  blocker above) and changes a serialized field's content, so it was reverted. A future cycle
  with same-worker benching should re-measure removing `span_all` entirely (field + init +
  push) on `parse_bench` — the bigger cost is the per-node `vec![span]` init alloc, not the
  pushes.
- **frankenmermaid/Mermaid ratio:** unchanged — no source perf change landed (bench infra +
  docs only). Retained `full_pipeline_wide` standing `198.10x` / `262.92x` / `426.35x` vs
  live-CDP Mermaid `11.12.0`.
- **Do-next / fix:** the highest-leverage infra fix for the whole swarm is **same-worker
  benching** — either an `rch exec --worker <id>` pin (feature request) or, per cycle,
  save-baseline and compare in immediate succession and *discard runs that report a different
  `remote <worker>`*. Until then, prefer `fm-parser`'s cmake-free bench for parse work
  (builds never fail) and accept that sub-5% layout/render effects remain unverifiable.

### Edge routing is 85% of tree-path layout; `intersect_segment` bool variant — ~0-GAIN (2026-06-26)
- **Finding (follow-up to the tree-fallback entry below):** instrumenting
  `layout_diagram_tree_traced` on `16x32` (512 nodes, 960 edges, the tree-path case) split
  the ~1 ms layout into **tree-build+placement ≈ `140 µs` (15%)** and **`build_edge_paths`
  (obstacle-routed edges) ≈ `785–862 µs` (85%)**. Edge routing — *shared with Sugiyama* — is
  the layout bottleneck for wide graphs. So the next layout lever is edge routing, not the
  tree builder.
- **Lever tested:** the obstacle check in `cga_routing::find_{vertical,horizontal}_segment_nudge`
  did `!expanded.intersect_segment(&seg).is_empty()` — `CgaRect::intersect_segment` allocates a
  `Vec<CgaPoint>` and dedups corner points just to test emptiness. Added a non-allocating
  `CgaRect::intersects_segment(&self,&seg)->bool` (early-exit `any`) and routed the call-sites
  through it. Output-identical (`cargo test -p fm-core -p fm-layout` = `349`+`428` passed).
- **Outcome:** reverted as ~0-gain. Same-machine local A/B (rch timing still `cmake`-blocked),
  `layout_wide` `--save-baseline`/`--baseline`: `8x16` `-4.2%`, `12x24` `+4.1%`, `16x32`
  `+11.0%` — **incoherent** (a strictly-less-work change cannot regress +11%; the moves are not
  even same-direction, ruling out a uniform load shift). The local box is too contended to
  resolve a sub-noise effect. Root cause of the ~0: `intersect_segment` only runs on
  AABB-overlapping candidates (rare on cleanly-routed edges), so the removed alloc is **not**
  on the hot path. The `~850 µs` edge-routing cost lives in the per-edge index `query_segment`
  traversal, the per-edge output `Vec<LayoutPoint>`, and the per-candidate f64 AABB rejects —
  not the rare `intersect_segment` allocation.
- **frankenmermaid/Mermaid ratio:** unchanged — reverted, main untouched. Retained
  `full_pipeline_wide` standing `198.10x` / `262.92x` / `426.35x` vs live-CDP Mermaid `11.12.0`.
- **Do-next:** to move edge routing, target the per-edge cost that actually dominates — the
  obstacle-index `query_segment` traversal and the output point-vector — not the
  obstacle-intersection allocation. And it needs a low-noise bench: the recurring `cmake`-less
  rch worker blocks clean rch timing, and the local box is too contended for sub-5% layout
  effects, so any edge-routing micro-opt is currently **unmeasurable** here — a real
  infra blocker for further layout/edge-routing perf work.
- **UPDATE (2026-06-26, definitive ~0-gain via same-worker A/B):** re-ran the
  `intersects_segment` change with the same-worker workaround — both `--save-baseline` and
  `--baseline` runs verified on worker `hz2` (`layout_wide`, criterion). Result was
  **incoherent**: `8x16 −2.3%` (n.s., p=0.58), `12x24 +15.4%`, `16x32 +12.3%` (p=0.00). A
  change that does *strictly less work* (`edges().any(intersect.is_some())` early-exit vs the
  full `intersect_segment` `Vec`-build + corner-dedup) **cannot** regress +15%, and the moves
  are not same-direction → this is **shared-worker contention drift** between the sequential
  baseline and candidate runs (hz2 is shared by the swarm). `cargo test -p fm-core -p
  fm-layout` = `349`+`428` passed (output-identical), so the delta is pure noise around zero.
  Reverted again. **Measurement-floor finding:** the same-worker workaround that cleanly
  resolved the `span_all` `−12%` parse win has a **noise floor of ≈5–15%** on shared workers
  (`hz2`/`ovh-a` serve the whole swarm), so it resolves *large* effects but **cannot verify
  sub-5% layout/render levers**. `span_all` was measurable only because it was big. **Do not
  re-attempt `intersects_segment` or any sub-5% layout/edge-routing micro-opt** until a
  dedicated low-contention bench host exists — its sign cannot be established here.

### `layout_wide/16x32` runs the TREE algorithm, not Sugiyama — OPTIMIZATION-TARGETING FINDING (2026-06-26)
- **What was measured:** the layout guardrail forces a fallback from Sugiyama to the **tree**
  algorithm for the largest wide bench case. Built `fm-cli`, rendered the generated wide
  inputs, read the layout WARN on stderr:
  - `8x16` → Sugiyama (no fallback)
  - `12x24` → Sugiyama (no fallback)
  - `16x32` → **`selected_algorithm="tree"`**, `estimated_time_ms=10481`,
    `reason="guardrail_forced_multi_budget"`
  Temporary `LAYOUT_PROFILE` instrumentation in `layout_diagram_sugiyama_traced_with_config`
  emitted **zero** `LAYOUT_SPLIT` lines for `16x32`, confirming the Sugiyama pipeline
  (`crossing_minimization` / `crossing_refinement` / `coordinate_assignment`) never executes
  for that input — it dispatches to `layout_diagram_tree_traced`.
- **Why this matters (redirects effort):** every crossing-minimization lever the swarm has
  tried — [[barycenter-sweep-precomputed-edge-adjacency]],
  [[flat-array-total-crossings-position-edge-tables]],
  [[dense-crossing-count-position-maps]], and the stashed local-delta refinement — lives on
  the **Sugiyama** path. That path runs for `8x16`/`12x24` but **not** for `16x32`, the
  largest and most prominent `layout_wide` case. So those four attempts could never have
  moved the heaviest wide layout number; their neutral/negative results are partly explained
  by this. The benched `layout_wide/16x32` (≈`1.13 ms`) is the **tree** algorithm:
  `build_tree_layout_structure` + subtree-span/center passes + the **shared
  `build_edge_paths`** (1024-edge obstacle-routed). Edge routing (already AABB/obstacle-index
  optimized — see [[sparse-edge-routing-obstacle-spatial-index]]) is the likely hot share.
- **frankenmermaid/Mermaid ratio:** unchanged — measurement only, no source change landed.
  Retained current-main `full_pipeline_wide` standing `1.5908 ms` / `3.7339 ms` / `6.7530 ms`
  vs live-CDP Mermaid `11.12.0` `315.14 ms` / `981.73 ms` / `2879.185 ms` = `198.10x` /
  `262.92x` / `426.35x`.
- **Do-next (not do-not-retry):** to move `layout_wide/16x32`, profile and optimize the
  **tree** path (`layout_diagram_tree_traced`) and the shared `build_edge_paths`, NOT the
  Sugiyama crossing-min code. To move `8x16`/`12x24`, Sugiyama is in play but its
  crossing-min is already heavily mined (4 rejects). Separately worth checking whether the
  `estimated_time_ms=10481` guardrail estimate is realistic (the whole `16x32` layout is
  ~`1.13 ms` via tree) — if Sugiyama would actually be fast, the fallback may be degrading
  layout *quality* (tree vs crossing-minimized) unnecessarily, a correctness/quality angle
  distinct from speed.

### Post-process `<style>` CSS minification — REJECTED (render +19%) (2026-06-26)
- **Byte-composition finding (kept for targeting):** for a small flowchart (3 nodes, 2
  edges) the rendered SVG is `13489` bytes, of which the embedded `<style>` CSS is **`9734`
  bytes (72%)** — by far the largest chunk now that markers are gated. The CSS is
  pretty-printed (352 non-empty lines, ~195 indented). The gradient and drop-shadow filter
  in `<defs>` are *used* (`fill="url(#fm-node-gradient)"`, `filter="url(#drop-shadow)"` on
  nodes), so not dead. The CSS rule bodies are theme-independent (they use `var(--fm-*)`);
  only `:root{…}` carries actual colors.
- **Lever tested:** a `minify_css` post-process (trim each line, join, inserting a space only
  where it would merge two word bytes — byte-safe for any CSS) applied at the three
  `doc.style(css)` sites. Deterministically shrank output: small flowchart `13489 → 12706`
  (**−783 bytes, −5.8%**), wide `8x16` `164321 → 163538` (−0.48%); CSS stays valid
  (`:root`, `.fm-node` rules, gradient ref all present; well-formed SVG). `cargo test -p
  fm-render-svg` = `219 passed; 0 failed`.
- **Outcome:** rejected — **render-time regression**. Same-machine local A/B (rch timing
  blocked by `cmake`-less worker), `render_svg/flowchart/small_10`: `100.83 µs → 123.09 µs`,
  change **+19.39%** (p = 0.00 < 0.05, "regressed"). Reprocessing the 9.7 KB CSS string
  (allocate + per-line scan) costs far more (~+22 µs) than the 783 fewer output bytes save.
  A byte win but a clear time loss on the very bench that fronts the Mermaid comparison.
- **frankenmermaid/Mermaid ratio:** unchanged — reverted, main untouched. Retained
  current-main `full_pipeline_wide` standing `1.5908 ms` / `3.7339 ms` / `6.7530 ms` vs
  live-CDP Mermaid `11.12.0` `315.14 ms` / `981.73 ms` / `2879.185 ms` = `198.10x` /
  `262.92x` / `426.35x`.
- **Do-not-retry note:** do **not** minify CSS as a post-process — the reprocessing pass
  dominates. The 783-byte (and potentially more, via used-only rule pruning) reduction is
  only worth taking if emitted **at generation time**: change `Theme::to_svg_style` (and the
  `effects/animation/accessibility` CSS builders) to push compact CSS directly (no
  indentation/newlines in the `format!`/`push_str` templates), so no second pass is needed.
  That is a larger, template-by-template edit with golden churn (harness goldens are not
  git-tracked) — a focused follow-up, not a quick win. CSS is the 72%-of-small-SVG frontier
  vs Mermaid (which ships minified CSS); generation-time compaction is the route.

### Emit only used `<defs>` arrowhead markers — HIGH-VALUE LEVER, IMPLEMENTATION-BLOCKED (2026-06-26)
- **The gap (measured):** every SVG render emits the full fixed set of **12** arrowhead
  markers (`arrow-end`, `-filled`, `-open`, `-half-{top,bottom}`, `-stick-{top,bottom}`,
  `-start`, `-start-filled`, `-circle`, `-cross`, `-diamond`) regardless of which the
  diagram uses. Verified: a trivial `flowchart TD; A-->B` (uses only `arrow-end`) emits all
  12. Mermaid.js emits **only used** markers. The 11 dead markers are fixed per-render
  output bytes + build cost — a *large* share of a small/medium diagram's render (where defs
  is a big fraction of total), and where our render-vs-Mermaid multiple is smallest. This is
  a real fair-fight (spans-off) win, unlike source-metadata (absent on that path).
- **Why blocked this turn (not a perf result — an architecture blocker):** the default
  `SvgBackend::LegacyLayout` renderer `render_layout_to_svg` builds `<defs>` (all 12 markers
  at once) and assigns it into the document **before** it builds the node/edge body, and it
  has several early `return doc.to_string()` paths in between. So the clean "build body →
  collect referenced markers → emit only those" reorder is unsafe (early returns would ship
  no defs), and the markers cannot be discovered up-front without the body.
- **Scoped fix for a fresh session (do this next):**
  1. Extract `render_edge`'s arrow→style match into a shared
     `fn edge_style(arrow: ArrowType, is_back_edge: bool, colors: &ThemeColors) ->
     (dasharray, marker_start, marker_end, color)` — single source of truth. `render_edge`
     calls it; correctness of marker selection stays in one place.
  2. In `render_layout_to_svg`, **before** the marker block, pre-scan `layout.edges`:
     `arrow = ir.edges[edge_path.edge_index].arrow`, `is_back = edge_path.reversed`, call
     `edge_style`, collect the referenced marker ids (`marker_id_from_url`) into a set.
  3. Gate each `defs.marker(...)` on `used.contains(id)`, preserving the existing emission
     order so output is byte-identical for any marker a diagram actually uses.
  4. The Scene backend (`render_scene_document_with_ir`) can gate the same way but more
     simply — it builds `scene_root` first, so walk it with an `Element::for_each_marker_ref`
     helper (drafted this turn) and gate. Do both backends together or the
     `explicit_legacy_backend_matches_default_output` / scene-vs-legacy parity expectations
     diverge.
  5. Regenerate the regression-harness goldens (`artifacts/regression-harness/latest`),
     which currently embed the full 12-marker set, and re-run `cargo test -p fm-render-svg`.
- **frankenmermaid/Mermaid ratio:** unchanged — main untouched (the half-done Scene-only
  gating was reverted to avoid backend divergence). Retained current-main
  `full_pipeline_wide` standing `1.5908 ms` / `3.7339 ms` / `6.7530 ms` vs live-CDP Mermaid
  `11.12.0` `315.14 ms` / `981.73 ms` / `2879.185 ms` = `198.10x` / `262.92x` / `426.35x`.
- **Simpler safe fallback (if the per-marker pre-scan is too much):** classify edges as
  "plain" iff their arrow uses only `arrow-end`/`arrow-open`/no-marker (`Arrow`, `Line`,
  `ThickLine`, `OpenArrow`, `DottedArrow`, `DottedLine`, `DottedOpenArrow`; back-edges always
  use `arrow-open`). If **all** `ir.edges` are plain → emit just `{arrow-end, arrow-open}`;
  otherwise emit all 12 (unchanged). This needs only an `ir.edges` scan before the marker
  block (no reorder, no `edge_style` extraction), is correctness-safe (any fancy arrow falls
  back to the full set), and already captures the common flowchart case (the wide bench is
  all `Arrow` → 2 markers instead of 12). Still requires golden regen.
- **Do-not-retry note:** do not gate only the Scene backend (diverges from the default
  Legacy backend that the benches/CLI actually use); do not reorder `render_layout_to_svg`'s
  defs past its early returns. The shared-`edge_style` pre-scan (or the plain/fancy fallback)
  is the safe route.

### `fm-source-span` static-name + `data_owned` allocation trim — ZERO-GAIN (2026-06-26)
- **Lever tested:** in the spans-on render path, `apply_span_metadata` does
  `.data("fm-source-span", &span.compact_display())`, which cost three allocations per
  span-bearing element: the formatted span value, the `format!("data-fm-source-span")`
  name (the attr was not in the static-name table), and the value copy inside
  `data`/`From<&str>`. Added `"fm-source-span"` to `static_data_attr_name` and a
  `data_owned`/`Attributes::data_owned` path that moves the formatted string in — reducing
  it to one allocation per element. Output-identical (`cargo test -p fm-render-svg` = `219
  passed; 0 failed`).
- **Outcome:** reverted as zero-gain. Same-machine local A/B (rch timing was unusable this
  cycle — the assigned worker `vmi1227854` lacks `cmake` for `highs-sys`, and cross-worker
  baselines are noisy), `render_spans_on/render/8x16`, criterion `--save-baseline` then
  `--baseline`: `1.3688 ms → 1.3634 ms`, change `−2.44%` (p = 0.11 > 0.05, **No change**).
  Two saved allocations across ~576 elements are too small a share of total render to clear
  the ≥3% bar.
- **Scope note (dead-end recorded):** this only ever touched the *spans-on* path. The
  flowchart fair-fight render (`SvgRenderConfig::default()`, spans **off**, the config the
  Mermaid head-to-head benches use) emits **no** `data-fm-source-*` attributes at all —
  flowchart nodes/edges render via the legacy `render_node`/`render_edge` path
  (`apply_span_metadata`, span only, gated off by default), not the scene path's
  `apply_source_metadata` (which emits the unconsumed `fm-source-kind`/`fm-source-index`
  only for non-flowchart scene items). So source-metadata trimming cannot move the
  fair-fight ratio; do not chase it there.
- **frankenmermaid/Mermaid ratio:** unchanged — retained current-main `full_pipeline_wide`
  standing `1.5908 ms` / `3.7339 ms` / `6.7530 ms` vs live-CDP Mermaid `11.12.0` `315.14 ms`
  / `981.73 ms` / `2879.185 ms` = `198.10x` / `262.92x` / `426.35x` slower.
- **Do-not-retry note:** the spans-on per-element allocation count is no longer the
  bottleneck worth chasing; and source-metadata is absent from the spans-off fair-fight
  render entirely. Recurring infra blocker: rch keeps routing to `cmake`-less workers
  (`vmi1227854`) that fail `highs-sys`, forcing local builds for any deterministic check —
  this is documented repeatedly above and needs a pool fix (install `cmake` or pin a
  cmake-equipped worker) to unblock reliable per-crate timing across the swarm.

### Offset edge-point streaming path builder — REJECTED (2026-06-26)
- **Lever tested:** `render_edge` and the Gantt dependency renderer still allocated a
  fresh `Vec<(f32, f32)>` per edge solely to add `offset_x`/`offset_y` before calling the
  smooth path serializer. A candidate `build_smooth_path_with_offset(&[LayoutPoint], dx,
  dy)` streamed offset points directly into the Catmull-Rom path writer, matching the
  collected-vector arithmetic and adding a byte-equivalence unit test.
- **Mapped primitive:** region/arena allocation removal from §5.10: eliminate a
  request-scoped temporary region in the hot SVG-render edge loop without changing
  rendered bytes.
- **Outcome:** rejected and reverted. The first candidate render run looked promising:
  `wide_stages/render` means `1.0853 ms`, `2.6163 ms`, `5.0474 ms`, with Criterion
  reporting significant wins for `12x24` and `16x32`. A same-machine baseline toggle then
  measured current-main render at `1.1173 ms`, `2.7479 ms`, `5.1672 ms`; but the final
  re-applied candidate rerun regressed `8x16` to `1.1887 ms` (`+6.39%`, p=0.00) and showed
  no significant change on `12x24` (`2.7125 ms`) or `16x32` (`5.2766 ms`). Full-pipeline
  candidate context was similarly small/noisy: candidate `1.8362 ms`, `4.6470 ms`,
  `8.4002 ms` vs toggled baseline `1.8932 ms`, `4.7857 ms`, `8.4282 ms`.
- **Original comparator:** latest live-CDP Mermaid `11.12.0` denominator reused from
  the current-main BOLD-VERIFY entry for identical generated wide inputs: `315.14 ms`,
  `981.73 ms`, and `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** toggled current-main baseline full-pipeline means give
  Mermaid.js `166.45x`, `205.14x`, and `341.62x` slower. The rejected candidate
  full-pipeline context was `171.63x`, `211.26x`, and `342.75x` slower, too small/noisy to
  keep and contradicted by the final render-stage regression.
- **Validation:** while candidate code was applied, `cargo test -p fm-render-svg`
  passed `220` unit tests plus doctests, `cargo check -p fm-render-svg --all-targets`
  passed, `cargo clippy -p fm-render-svg --all-targets -- -D warnings` passed, and
  `rustfmt --edition 2024 --check crates/fm-render-svg/src/lib.rs
  crates/fm-render-svg/src/path.rs` passed. Code was manually reverted before commit; no
  production source diff remains.
- **Tooling:** the literal requested `rch exec -- cargo bench --release ...` still fails
  before benchmarking because this Cargo rejects `--release` for `cargo bench`. Valid
  per-crate `rch exec -- cargo bench -p frankenmermaid-cli --bench pipeline_bench ...`
  also fell back local because RCH reported `no admissible workers:
  insufficient_slots=4,hard_preflight=1`.
- **Do-not-retry note:** do not retry the per-edge offset-Vec elision as a standalone
  lever. The allocation is real, but its measured runtime signal is below the noise floor
  after recent renderer wins. Target larger output-generation work instead, especially
  generation-time CSS compaction or direct element serialization that removes whole
  Element/Attributes objects.

### Agent Mail registration/reservation — BLOCKED (2026-06-24)
- **Attempt:** Register `frankenmermaid-cod-a` and reserve `docs/NEGATIVE_EVIDENCE.md`,
  `evidence/ledger/**`, `.beads/**`, and bench files through MCP Agent Mail before edits.
- **Observed:** MCP Agent Mail tools were not exposed in this Codex session. The local `am`
  daemon had split/stale endpoint state: `/health` answered on port 43091, CLI proxy expected
  `/mcp/`, and mutating operations were refused by the mailbox activity lock owned by live
  Agent Mail processes.
- **Evidence:** `am macros start-session ...` failed first because descriptive agent names are
  rejected, then failed with `malformed HTTP response`; `am file_reservations reserve ...`
  failed with `Resource is temporarily busy`.
- **Verdict:** coordination blocker, not benchmark evidence.
- **Do-not-retry note:** do not kill shared Agent Mail processes from this repo session; retry
  registration only after the mailbox owner/port state is repaired by the coordination operator.

### Local mermaid-js reference corpus — BLOCKED (2026-06-24)
- **Attempt:** Read the documented `legacy_mermaid_code/` original reference corpus.
- **Observed:** `legacy_mermaid_code/` is absent from this checkout, and `git ls-files` plus
  `git submodule status` show no tracked corpus/submodule entry.
- **Evidence:** README states the historical gitlink was retired and the reference corpus is
  gitignored; local `find legacy_mermaid_code` returned no such path.
- **Verdict:** local original-comparator corpus unavailable.
- **Do-not-retry note:** the head-to-head harness must pin mermaid-js package/source provenance
  explicitly instead of assuming a checked-in `legacy_mermaid_code/` tree exists.

### `cargo bench --release` flag — BLOCKED (2026-06-24)
- **Attempt:** Follow the requested per-crate bench shape literally:
  `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a rch exec -- cargo bench -p frankenmermaid-cli --bench pipeline_bench --release`.
- **Observed:** Cargo rejected `--release` for `cargo bench` in this toolchain with
  `error: unexpected argument '--release' found`.
- **Evidence:** rch worker `vmi1153651`, command start `2026-06-24T21:51:45Z`, exit 1 before
  running benchmarks.
- **Verdict:** command-shape blocker, not benchmark evidence.
- **Do-not-retry note:** use per-crate `cargo bench -p <package> --bench <bench>` through rch;
  Cargo bench already uses the optimized bench profile. Keep the dedicated
  `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a`.

### Cod-b mermaid-js denominator check — BLOCKED (2026-06-24)
- **Attempt:** Produce a frankenmermaid/mermaid-js ratio for the BOLD-VERIFY lane after
  reading README/AGENTS and registering with Agent Mail as `TanSparrow`
  (`frankenmermaid-cod-b` alias in the task description).
- **Observed:** The original JavaScript renderer was unavailable in this checkout/runtime:
  `legacy_mermaid_code/` absent, `mmdc` not on PATH, `node` present but neither `mermaid`
  nor `@mermaid-js/mermaid-cli` is installed. AGENTS.md says this project uses Cargo only,
  so cod-b did not run `npm`/`npx` to install a comparator ad hoc.
- **Evidence:** `which mmdc` failed; `node -e "require.resolve('mermaid')"` failed with
  `Cannot find module 'mermaid'`; `node -e "require.resolve('@mermaid-js/mermaid-cli')"`
  failed with `Cannot find module '@mermaid-js/mermaid-cli'`.
- **frankenmermaid side measurement:** per-crate rch run completed on worker `ovh-a`:
  `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b rch exec -- cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench -- --warm-up-time 1 --measurement-time 2`.
- **Representative Rust baselines:** `parse/flowchart/large_1000` mean `13.683 ms`;
  `layout/flowchart/large_500` mean `7.8640 ms`; `render_svg/flowchart/large_500`
  mean `11.759 ms`; `full_pipeline/parse_layout_svg/large_500` mean `20.623 ms`;
  `full_pipeline/parse_layout_svg/cyclic_50` mean `1.9015 ms`; `typical_7_nodes`
  full pipeline mean `393.75 us`.
- **Original comparator ratio:** BLOCKED; mermaid-js denominator unavailable.
- **Verdict:** Rust-only baseline exists, but it is not dominance evidence.
- **Do-not-retry note:** complete `bd-1buv.1` first: pin mermaid-js source/package
  provenance, normalize equivalent render-to-SVG calls, and emit ratios before closing
  any optimization bead or claiming a win.

### Current-main wide pipeline standing after the session's wins — VERIFIED (2026-06-26)
- **Kind:** BOLD-VERIFY standing measurement; **no source changed this turn**. Re-measures
  the headline `full_pipeline_wide` (parse + layout + SVG, `SvgRenderConfig::default()` =
  spans-off, the fair fight vs Mermaid) after this session landed `span_all` (parse −12%),
  the flowchart marker gating, the spans-on attr trim, and the parser borrowed-lines win.
- **Measurement:** `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cc rch exec
  -- cargo bench -p frankenmermaid-cli --bench pipeline_bench -- full_pipeline_wide` on worker
  `ovh-a` (cmake OK, criterion variance ±0.1%): `8x16` **`1.3105 ms`**, `12x24` **`3.0769
  ms`**, `16x32` **`5.6932 ms`**. That is **−17.6% / −17.6% / −15.7%** vs the prior recorded
  standing (`1.5908` / `3.7339` / `6.7530 ms`), consistent with the landed parse+render wins.
- **frankenmermaid/Mermaid ratio:** vs the pinned live-CDP Mermaid `11.12.0` denominators
  (`315.14` / `981.73` / `2879.185 ms`): **`240.5x` / `319.1x` / `505.7x`** faster — up from
  the prior `198.10x` / `262.92x` / `426.35x`.
- **Caveat (honest):** the prior standing was measured on worker `hz2`; this run is on `ovh-a`,
  so part of the absolute delta may be worker-speed, not solely the landed wins (cross-worker
  confound — the same ≈5–15% noise floor recorded elsewhere). The *direction* (faster) and the
  span_all parse win (−12%, measured clean same-worker) are real and are in this pipeline; treat
  the headline ratios as a conservatively-bounded improvement, with `198x`–`426x` as the
  floor and `240x`–`506x` as this run's reading.

### Current-main wide pipeline standing — VERIFIED (2026-06-26)
- **Kind:** BOLD-VERIFY standing measurement; **no source changed this turn**. The
  worktree commit `290adec` ("append dynamic classes without temp strings") was found
  already landed on `main` as commit `45015be`, so there was no unlanded win to ship.
- **Measurement:** fresh `full_pipeline_wide` (parse + layout + SVG) means on `main`
  `52109a1` via `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cc rch exec
  -- cargo bench -p frankenmermaid-cli --bench pipeline_bench -- full_pipeline_wide
  --warm-up-time 1 --measurement-time 2` on worker `hz2` (toolchain matches the pool):
  `8x16` `1.5908 ms`, `12x24` `3.7339 ms`, `16x32` `6.7530 ms`. Criterion reported
  `-24.6%` (`12x24`) and `-23.1%` (`16x32`) versus the prior stored hz2 baseline —
  attributable to the cumulative last-5 `fm-render-svg` perf commits.
- **Original comparator:** latest live-CDP Mermaid `11.12.0` denominators reused from
  prior BOLD-VERIFY entries for identical generated wide inputs: `8x16` `315.14 ms`,
  `12x24` `981.73 ms`, `16x32` `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** `0.005048x` (`8x16`), `0.003803x` (`12x24`),
  `0.002345x` (`16x32`) — i.e. Mermaid.js is `198.10x`, `262.92x`, and `426.35x`
  slower, exceeding the previously recorded `170x-356x` standing.
- **Verdict:** verified; current main holds and extends wide-pipeline dominance after the
  recent renderer optimizations. Recorded as standing, not as a new lever.
- **Do-not-retry note:** the dynamic-class-append lever is already on main (`45015be`);
  do not re-attempt it as if unlanded. Its pre-rebase worktree copy `290adec` only looks
  novel to `git cherry` because its diff context targets the old `10b1654` baseline.

### Wide-pipeline stage split — SVG render dominates, not layout — VERIFIED (2026-06-26)
- **Kind:** measurement/finding that redirects optimization targeting; ships a new
  permanent bench group `wide_stages` (`crates/fm-cli/benches/pipeline_bench.rs`) and no
  source perf change.
- **Why this was hidden:** the existing benches measure layout in isolation
  (`layout_wide`), the whole pipeline fused (`full_pipeline_wide`), or render of *linear*
  flowcharts only (`render_svg`/`gen_flowchart`). None isolate render on the *wide*
  (edge-heavy) corpus, so per-stage cost on realistic fan-out graphs was never visible —
  and prior perf effort went heavily into layout and linear-render micro-ops.
- **Measurement:** per-crate `cc` target dir
  (`/data/projects/.rch-targets/frankenmermaid-cc`), `frankenmermaid-cli`/`pipeline_bench`,
  filter `wide_stages`, via `rch exec` (worker `ovh-a`), criterion means:
  | size | parse | layout | render | render share |
  |------|-------|--------|--------|--------------|
  | `8x16`  | `271.6 us` | `117.7 us` | `922.6 us` | `70%` |
  | `12x24` | `627.7 us` | `408.4 us` | `2.1264 ms` | `67%` |
  | `16x32` | `1.1854 ms` | `959.5 us` | `3.6208 ms` | `63%` |
- **Implication:** for wide graphs SVG render is the dominant stage (≈63–70%), layout is
  only ≈12–17%. Render cost is essentially per-element: a fit over the three sizes gives
  ≈`23 us` fixed + ≈`2.34 us` per emitted element (`d`/attribute construction +
  serialization), so the lever is the per-node/per-edge `Element` build, not `defs` or
  layout. This is the single most reliable place to find the next real win against
  Mermaid.js on fan-out diagrams.
- **frankenmermaid/Mermaid ratio:** unchanged — retained current-main `full_pipeline_wide`
  standing `1.5908 ms` / `3.7339 ms` / `6.7530 ms` vs pinned live-CDP Mermaid `11.12.0`
  `315.14 ms` / `981.73 ms` / `2879.185 ms` = Mermaid.js `198.10x` / `262.92x` / `426.35x`
  slower (`8x16` / `12x24` / `16x32`).
- **Do-not-retry note:** stop targeting `layout_wide` for headline wide-pipeline wins — at
  ≈12–17% of the budget its ceiling is small and four data-structure rewrites of the
  crossing-min/ordering path have already failed (see
  [[barycenter-sweep-precomputed-edge-adjacency]],
  [[flat-array-total-crossings-position-edge-tables]],
  [[dense-crossing-count-position-maps]]). Target `render` on `wide_stages` instead, and
  measure render levers against this group (not the linear-only `render_svg` group, which
  hides edge-heavy cost).

### Generated SVG id ownership — REVERTED (2026-06-27)
- **Lever tested:** `fm-render-svg::Attributes` and `Element` briefly grew
  `id_owned(String)` so generated Mermaid element ids (`fm-node-*`,
  `fm-edge-*`, `fm-cluster-*`) could move the already-built `String` into the
  `id` attribute instead of allocating once in the helper and copying again
  through `id(&str)`.
- **Mapped primitive:** allocation-fusion / partial evaluation from the
  alien-artifact pass: specialize the generated-id path while leaving borrowed
  literal ids on the existing API.
- **Baseline -> After:** per-crate package `frankenmermaid-cli`, bench
  `pipeline_bench`, filter `wide_stages/render`, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`, via
  `RCH_WORKER=ovh-a rch exec -- cargo bench --profile release -p
  frankenmermaid-cli --bench pipeline_bench -- wide_stages/render --warm-up-time
  1 --measurement-time 2`. Clean `HEAD` baseline worktree
  `/data/projects/.worktrees/frankenmermaid-tansparrow-id-owned-baseline-20260627`
  at `af4e091` measured `8x16` `0.77240 ms`, `12x24` `1.7722 ms`, and
  `16x32` `3.1487 ms`. The candidate measured `0.78512 ms`, `1.7588 ms`,
  and `3.1582 ms`. That is `+1.65%`, `-0.76%`, and `+0.30%`: mixed and below
  the keep bar. Earlier local-fallback evidence for the same lever is superseded
  by this same-worker `ovh-a` pair.
- **Original comparator:** pinned live-CDP Mermaid `11.12.0` denominators reused
  for identical generated wide inputs: `8x16` `315.14 ms`, `12x24`
  `981.73 ms`, `16x32` `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** baseline render stage was `0.002451x`,
  `0.001805x`, and `0.001094x` Mermaid.js time (`408.00x`, `553.96x`,
  `914.40x` faster than Mermaid.js). Candidate render stage was `0.002491x`,
  `0.001792x`, and `0.001097x` Mermaid.js time (`401.39x`, `558.18x`,
  `911.65x` faster). These render-stage ratios are conservative context against
  full-pipeline Mermaid denominators, not a replacement for the standing
  full-pipeline ratio.
- **Behavior proof:** the candidate source added a focused serialization test
  for `id_owned` while measured. The source code is restored in the final tree;
  no production code from this lever remains. Final conformance rerun:
  `AGENT_NAME=TanSparrow CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b
  rch exec -- cargo test --profile release -p frankenmermaid-cli --test
  frankentui_conformance_test` passed (`1` test).
- **Verdict:** reverted; the same-worker wide render gate showed no reliable win,
  so the generated-id copy is not the active bottleneck. Do not retry this as
  `id_owned`, `attr_owned`, or a cross-crate generated-id helper without a fresh
  allocation profile showing generated id copies in the top renderer costs.
