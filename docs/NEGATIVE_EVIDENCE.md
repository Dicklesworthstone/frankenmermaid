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

### Acyclic SCC fast-path in `GraphMetrics::from_ir` — −27 to −29% layout (Auto-selection overhead) (2026-06-27)
- **Lever (the 203µs Auto overhead pinned last cycle):** the Auto algorithm selection
  (`select_general_graph_algorithm_with_config`) calls `GraphMetrics::from_ir` on **every** layout —
  and that ran `detect_cycle_components` (Tarjan SCC, O(V+E)) + `stable_node_priorities`
  unconditionally. The forced-sugiyama path skips selection entirely, which is exactly the
  `layout_diagram` (Auto, 929µs) vs forced-sugiyama (726µs) gap. But `count_back_edges` runs first, and
  **`back_edge_count == 0` ⟺ acyclic ⟺ `scc_count == 0`, `max_scc_size == 1`** — so for any DAG the SCC
  pass and the priorities that only feed it are skippable with a **provably identical result**. Added
  that fast-path (fm-layout/src/lib.rs `GraphMetrics::from_ir`).
- **Measured (same-worker `ovh-a` A/B, per-crate `layout_wide`, both tight ±0.1%):**
  - 8x16: `119.20 µs → 86.31 µs` (**−27.6%**)
  - 12x24: `432.35 µs → 315.17 µs` (**−27.1%**)
  - 16x32: `1.0442 ms → 740.09 µs` (**−29.1%**)
  Candidate `740 µs` ≈ forced sugiyama `726 µs` → the Auto-path overhead is essentially eliminated.
  Layout is ~18% of the wide pipeline → ~**−5% end-to-end**; helps every acyclic diagram (the common
  case), production + bench alike (not a repeated-call cache artifact).
- **Ratio vs Mermaid 11.12.0:** layout 16x32 `740 µs`; standing `226x`–`506x` band rises as the wide
  pipeline drops ~5%.
- **Conformance GREEN:** 428 fm-layout tests + golden SVG (30+30) + determinism + budget all pass
  (output byte-identical — the metrics are exactly what the Tarjan pass yields for a DAG, so the
  selection and layout are unchanged). The lone `explicit_config_enables_svg_effects_and_accessibility`
  failure is the unrelated pre-existing render-side drop-shadow assertion (no-op for this change).

### Incremental crossing-count in `crossing_refinement` — −13 to −23% layout (2026-06-27)
- **Lever (fresh — no prior layout entry in this ledger; not in git perf history):**
  `crossing_refinement` (fm-layout/src/lib.rs) called the full-graph `total_crossings` — which
  rebuilds two nested `BTreeMap`s over all 512 nodes and rescans all ~1024 `ir.edges` — on **every
  transpose/sift trial** (~15-20k calls for the 16x32 wide DAG, whose wrap-around diagonal keeps
  crossings > 0 so the refinement runs fully). A trial perturbs exactly one rank, so only the
  `(r-1, r)` and `(r, r+1)` pair crossings can change. Now precompute the adjacent-rank edge buckets
  once (`build_pair_node_edges`) and compare only the affected pairs per trial (`pair_crossings`,
  O(pair-edges)). Accepting iff the affected pairs strictly decrease is **exactly equivalent** to the
  full-total comparison, so the resulting ordering and `best_crossings` are identical → SVG output
  byte-identical.
- **Measured (same-worker `ovh-a` A/B, per-crate `layout_wide` bench, candidate ±0.2% noise):**
  - 8x16: `153.5 µs → 117.4 µs` (**−23.5%**)
  - 12x24: `497.8 µs → 432.0 µs` (**−13.2%**)
  - 16x32: `1.331 ms → 1.026 ms` (**−22.9%**)
- **End-to-end (`full_pipeline_wide`, both confirmed `ovh-a`, tight):** 8x16 `1.214 ms → 1.182 ms`
  (**−2.6%**); the pipeline saving (32 µs) ≈ the isolated layout saving (36 µs) and the derived
  baseline layout (149 µs) ≈ measured (153.5 µs), so the A/B is internally consistent. Layout is
  ~10% of the 8x16 pipeline and ~19% at 16x32, so the end-to-end win grows with size.
- **Ratio vs Mermaid 11.12.0 (`ovh-a`):** 8x16 `1.182 ms` → **267x**, 12x24 `2.817 ms` → **348x**
  (worker-dependent; standing band `226x`–`506x` holds, nudged up by this layout win).
- **Conformance GREEN:** 428 fm-layout tests + golden SVG tests + determinism pass (output
  byte-identical). The lone `explicit_config_enables_svg_effects_and_accessibility` failure is
  **pre-existing on HEAD** (it asserts the inline `id="drop-shadow"` def that HEAD's render gates off
  under embedded CSS, fm-render-svg/src/lib.rs:1711) and is a **no-op for my change** (its 2-node
  `A-->B` diagram never enters `crossing_refinement`) — unrelated render-side staleness, not this
  change's regression.

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

### CORRECTION: cycle_removal is ~13µs, NOT 173µs — the "hotspot" was FM_PROFILE noise (2026-06-27)
- Before building the cycle_removal→GraphMetrics acyclic-flag threading lever (flagged last cycle off
  a `173µs` reading), I **sub-profiled cycle_removal**: `resolved_edges` 0.4–1.7µs, `priorities`
  0.7–5.7µs, **`cycle_removal_dfs_back` 6.7–11.8µs** ⇒ **cycle_removal ≈ 13µs total**, not 173µs. The
  173µs was a **single noisy/contended FM_PROFILE iteration** (eprintln overhead + per-iteration cache
  variance + worker load — the same artifact that made coordinate_assignment read 108→673µs across
  iterations). **So the back-edge DFS is only ~7–12µs and threading the acyclic flag from the Auto
  dispatch into cycle_removal would be ~0-gain.** Did NOT build the multi-layer refactor — the
  sub-profile saved it. Reverted instrumentation.
- **Methodology:** FM_PROFILE single-iteration phase numbers are unreliable (huge variance); sub-profile
  the suspect function and take the warm/consistent value, or cross-check against the criterion
  same-worker A/B delta, before committing to a refactor.
- **The layout is at genuine diminishing returns** after the two landed wins (crossing_refinement
  −13–23%, SCC fast-path −27–29%): every phase is cheap and distributed (~10–55µs locally), no single
  ≥20% hotspot. The largest *real* phases are `build_edge_paths` (~55µs, already obstacle-index
  optimized) and `rank_assignment` (~47µs) — both modest (~7% of layout each → ~1% pipeline). Next
  layout work should target one of those with a same-worker A/B, not the cycle_removal/priorities path
  (now triple-confirmed ~0-gain).
- **Standing:** unchanged — no source change. `226x`–`506x` over Mermaid `11.12.0`.

### REJECTED: share `stable_node_priorities` across cycle_removal + rank_assignment (~0-gain) (2026-06-27)
- `stable_node_priorities` (O(V·log V) node-id string sort) is called ~4×/layout (cycle_removal,
  rank_assignment, GraphMetrics [skipped for DAGs by the SCC fast-path], build_cycle_cluster_map). It's
  pure in `ir`, so I threaded a once-computed array through `cycle_removal_with_priorities` +
  `rank_assignment_with_priorities` (wrapper+core split, no test churn). Correct: 428 fm-layout tests
  pass, output-identical.
- **~0-gain.** Same-session ovh-a A/B (layout_wide): HEAD `87.26 / 316.33 / 748.41 µs` → candidate
  `~91.9 / 327.8 / 752.9 µs`, but the candidate ran on a **contended** worker (8x16 ±8%, a clean
  16x32 re-run gave 838–965µs ±14%) — the apparent slowdown is contention, not a regression (the
  refactor does strictly less work). The true gain is just **one eliminated sort**, and that sort is
  **cheap** (the cycle_removal `173µs` hotspot is the back-edge DFS `cycle_removal_dfs_back`, not the
  priorities). Below the noise floor + unconfirmable → reverted rather than land an unmeasured change.
- **Remaining cycle_removal lever (hard):** `cycle_removal_dfs_back` overlaps `GraphMetrics::from_ir`'s
  `count_back_edges` (both back-edge DFS) — but they use **different DFS orderings** (not
  interchangeable), and the only safe share is the acyclic *flag* (`count_back_edges == 0`) threaded
  from the Auto dispatch into the sugiyama run (Auto-path-only; forced-sugiyama has no selection). The
  layout is at diminishing returns after the two landed wins (crossing_refinement −13–23%, SCC
  fast-path −27–29%); the remaining hotspots are `cycle_removal_dfs_back` (threading-bound) and
  `coordinate_assignment` node-box building (the per-node `node.id.clone` is contract-bound).
- **Standing:** unchanged — reverted. `226x`–`506x` over Mermaid `11.12.0`.

### REJECTED: cycle_removal acyclic strategy short-circuit (~0-gain) + re-profiled hotspots (2026-06-27)
- **Re-profiled the layout after the SCC fast-path win (`ba9a45d`)** (FM_PROFILE, 16x32):
  **`cycle_removal` 173µs (~37%)** and **`coordinate_assignment` 147µs (~31%)** are now the two
  hotspots; then `build_edge_paths` 55µs, `rank_assignment` 47µs, `node_sizes` 17µs, `crossing` 13µs.
- **Tried + REJECTED:** short-circuit `cycle_removal`'s `cycle_strategy` match to `BTreeSet::new()`
  when `dfs_back_edges.is_empty()` (acyclic ⇒ every strategy reverses nothing, output-identical, 428
  tests pass). Same-session ovh-a A/B (layout_wide, tight): HEAD `87.18 / 314.34 / 740.50 µs` →
  candidate `86.83 / 314.08 / 745.70 µs` = **−0.4% / −0.1% / +0.7%, within noise → ~0-gain.** Reason:
  the default `CycleStrategy` is **not Greedy**, so the match already just clones the (empty) back-edge
  set — the costly greedy FAS pass never ran. The 173µs is the **detection** (`resolved_edges` +
  `stable_node_priorities` + `cycle_removal_dfs_back`), which determines acyclicity and can't be
  skipped. Reverted.
- **Candidate lever for next cycle (redundancy):** `resolved_edges(ir)` and the back-edge DFS are
  computed in **both** `GraphMetrics::from_ir` (Auto selection: `resolved_edges` + `count_back_edges`)
  **and** `cycle_removal` (`resolved_edges` + `cycle_removal_dfs_back`). Threading the resolved edges
  (and back-edge set) from the dispatch into the sugiyama run would eliminate the duplicate O(V+E)
  traversal — but forced-sugiyama skips the selection, so the share is Auto-path-only and needs care.
  Sub-profile cycle_removal's three parts first to size the win. (Separately, `coordinate_assignment`'s
  147µs is node-box building; its per-node `node.id.clone()` is contract-bound — render+CLI read it.)
- **Standing:** unchanged — reverted. `226x`–`506x` over Mermaid `11.12.0`.

### Layout post-crossing_refinement: distributed cost, and a measured but unexplained Auto-path overhead (2026-06-27)
- **Sub-phase profile (16x32, FM_PROFILE):** `bk_vertical_alignment` dominates the BK (~66-80%), but
  the **whole BK (`brandes_kopf_secondary_coords`) is only ~7.5µs warm** — confirming last cycle's
  neighbour-precompute regression: the BK is *not* a bottleneck. The layout cost is now **distributed**
  (cycle_removal, rank_assignment, coordinate_assignment node-box building, build_edge_paths — each
  ~30-60µs, no single ≥50% hotspot). The big lever (crossing_refinement) is already landed; the layout
  is at diminishing returns for single-phase wins.
- **Algorithm comparison (16x32, ovh-a, one run, tight):** forced `sugiyama` **726µs** ≈ forced `tree`
  **716µs** — the guardrail's tree fallback is **not faster** (sugiyama is equal speed + better quality,
  fewer crossings). But `layout_diagram` (Auto — the production/pipeline path) = **929µs = +28% / ~203µs
  over forced sugiyama.**
- **The 203µs Auto overhead is the next lever — but its source resisted code inspection.** The Auto
  dispatch is cheap by code: `track_dependency_graph_query` early-returns (no incremental state),
  `dispatch`/`preferred`/`select_general_graph`/`expected_loss_permille`/`evaluate_layout_guardrails`
  are all just `estimate_layout_cost` arithmetic, and `compute_fnx_layout_selection_signals` (the only
  O(V·E) candidate — builds the fnx graph + articulation/bridges) is **stubbed to `None`** in the default
  build (`fnx-integration` is non-default). Both entry points use the same config/cycle-strategy; only
  the `LayoutAlgorithm` param differs (Auto vs Sugiyama). So the 203µs is real but unexplained by static
  reading — **next: flamegraph `layout_diagram` vs `layout_diagram_traced_with_algorithm(_, Sugiyama)`**
  on 16x32. If real + eliminable, it's a ~22% win on the **production path** (the biggest remaining
  layout lever). Did not ship a guess.
- **Side note (stale guardrail estimate):** `estimate_layout_cost` Sugiyama = `nodes×edges/50` =
  **10485ms for 16x32** (actual 726µs, ~14000x off) — models the old O(V·E²) crossing logic that
  crossing_refinement replaced. It drives the CLI's `sugiyama→tree` fallback for large diagrams.
  Recalibrating is a **quality** change (tree≈sugiyama speed, sugiyama better) to a safety mechanism +
  changes untested large-diagram output — parked, not a speed win.
- **Standing:** unchanged — no source change (scaffolding reverted). `226x`–`506x` over Mermaid `11.12.0`.

### REJECTED: Brandes-Köpf neighbour precompute regresses +2-4% (neighbour recompute is not the BK bottleneck) (2026-06-27)
- Implemented the lever flagged last cycle: `bk_upper_neighbours` re-walks adjacency + re-sorts a
  `Vec` per node per pass (×4); the ordering is fixed during BK, so I precomputed the upper (rank-1)
  and lower (rank+1) neighbour lists **once** (`bk_precompute_neighbours`) and reused them across all
  four passes, halving the neighbour work (2048 → 1024 calls). Output byte-identical (428 fm-layout
  tests + golden SVG + determinism all pass).
- **But it REGRESSED.** Same-session `ovh-a` A/B (`layout_wide`, both tight): HEAD `118.04 µs` /
  `1.0305 ms` (8x16/16x32) → candidate `122.39 µs` / `1.0518 ms` = **+3.7% / +2.1% slower**. The
  precompute trades 2048 *transient* `Vec`s for **1024 persistent nested `Vec`s** (`upper`+`lower`,
  held for the whole BK) — and since the neighbour computation is **not** the BK bottleneck, halving
  it doesn't pay for the `Vec<Vec>` allocation/retention. Reverted.
- **Redirect:** the BK (`coordinate_assignment`, ~47-56% of layout) bottleneck is **elsewhere** —
  `bk_horizontal_compaction` (lib.rs:9490, runs ×4) or the alignment median/threshold logic, not the
  neighbour lists. A flat CSR neighbour layout would avoid the `Vec<Vec>` overhead but won't help
  (the recompute isn't the cost). Next cycle: profile `bk_vertical_alignment` vs
  `bk_horizontal_compaction` (accumulated over the 4 passes) to locate the real BK cost before
  optimizing. Do NOT re-attempt the neighbour precompute.
- **Standing:** unchanged — reverted. `226x`–`506x` over Mermaid `11.12.0`.

### PROFILED: Brandes-Köpf `coordinate_assignment` is now the dominant layout phase (next lever) (2026-06-27)
- After the crossing_refinement win (`5688f41`), env-gated phase timers (`FM_PROFILE`) on the 16x32
  wide layout give this per-iteration breakdown (warm, local): `compute_node_sizes 10µs`,
  `cycle_removal 34µs`, `rank_assignment 31µs`, `crossing_minimization 8µs`,
  **`crossing_refinement 60ns`** (was the ~305µs dominant phase — my opt crushed it),
  **`coordinate_assignment(BK) ~108µs warm / up to 673µs cold`**, `subgraph+constraint_solver 60ns`,
  `build_edge_paths 29µs`, `post_edge_path 10µs`. So **Brandes-Köpf coordinate assignment is now the
  biggest phase (~47-56%)** — the hotspot moved there once crossing_refinement was fixed.
- **Next lever (fresh, Explore-flagged, not yet attempted):** `bk_upper_neighbours`
  (fm-layout/src/lib.rs:9352) allocates + sorts a `Vec` per node per direction — called ×4 alignment
  passes (~2048×). The ordering is **fixed during BK**, so the upper/lower neighbour lists (sorted by
  position) can be precomputed **once** and reused across all 4 passes (currently recomputed ~2×
  redundantly per direction). Also pervasive `BTreeMap<usize, _>` (`ranks`, `pos_map`,
  `rank_pos_maps`) over dense node/rank indices → `Vec`/`FxHashMap` for O(1) lookup. Estimated the
  single biggest remaining layout lever; same output-preserving discipline as crossing_refinement.
- **Standing:** unchanged this entry. `226x`–`506x` over Mermaid `11.12.0`.

### FINDING: layout guardrail forces "tree" for large wide diagrams — production ≠ bench (2026-06-27)
- Rendering the 16x32 wide diagram through the **CLI** logs
  `layout.guardrail.fallback initial_algorithm="sugiyama" selected_algorithm="tree"
  estimated_time_ms=10481 reason="guardrail_forced_multi_budget"` — the guardrail **estimates
  sugiyama at 10.5 s and falls back to the tree algorithm**. But the actual sugiyama layout (the path
  the `layout_wide`/`full_pipeline_wide` benches exercise via `layout_diagram`) is **~1 ms** — the
  estimate is ~10000x pessimistic.
- **Implication:** the benches measure **sugiyama**, but the CLI/production falls back to **tree** for
  large wide diagrams, so sugiyama bench wins (crossing_refinement, and a future BK opt) may not reach
  production at that size. **Potential lever:** correct the guardrail's time estimate (it appears to
  model worst-case / un-optimized sugiyama) so production uses the now-fast sugiyama — *if* sugiyama is
  actually ≤ tree time and the layout quality is acceptable (both need verification before changing a
  safety guardrail). Parked pending that verification.
- **Standing:** unchanged. `226x`–`506x` over Mermaid `11.12.0`.

### IrGraph build MEASURED ~0-gain (data closes the parked lever) (2026-06-27)
- **Result:** the eager `ir.graph` FNX-adapter build (`ir_builder.rs` 872/1265) — flagged as "dead
  in the render pipeline, modest ~2-5%" in `1dbd7cd` — is now **measured ~0-gain**, upgrading the
  estimate to data. Clean **single-process A/B** (`parse_bench`, `wide`, same worker, same process:
  a `ParserConfig.emit_graph_adapter` flag + a `wide_nograph` variant via a new `parse_with_config`):
  skipping the build is **not faster — within noise, slightly slower** (the per-iteration gate branch
  offsets the saved push):
  - `8x16` graph `354.35 µs` vs nograph `362.39 µs`
  - `12x24` graph `823.93 µs` vs nograph `863.92 µs`
  - `16x32` graph `2.124 ms` vs nograph `2.330 ms`
  The build is **below the parse noise floor** (cheap Copy-ish structs into two pre-reserved Vecs).
  Scaffolding reverted per "REVERT ~0-gain". The IrGraph lever is now **definitively closed** — do
  not re-investigate.
- **Methodology trap recorded:** the first candidate gated the pushes with `if std::hint::black_box(false)`
  to defeat dead-code elim — but `black_box` is an **optimizer fence**, and one per node/edge in the
  hot parse loop poisoned surrounding optimization, producing a **false +14-91% "regression"** that
  had nothing to do with the gated code. Use plain `if false` (clean DCE) or a real runtime flag for
  per-iteration gating in micro-benchmarks; never `black_box(const)` inside the measured loop.
  (Also: criterion `--save-baseline`/`--baseline` does **not** transfer across rch workers — the
  worker lottery put candidate on a different host than baseline → "Baseline must exist" panic. Do
  A/B variants **in one bench run** instead.)
- **frankenmermaid/Mermaid ratio:** unchanged — reverted. Standing `226x`–`506x` (worker-dependent)
  over Mermaid `11.12.0`.

### Parse area contended — hash-key/smallvec dedup in flight; IrGraph measurement deferred (2026-06-27)
- Attempted to measure the parked IrGraph-build cost (parse_bench A/B, gating `ir_builder.rs`
  872/1265). The baseline bench **failed to compile**: `unresolved import smallvec`, `no field
  node_index_by_id`/`label_index_by_text on IrBuilder` — a **concurrent agent is implementing the
  hash-key/smallvec dedup** of the node/label dedup maps (the bigger parse lever flagged
  "too-invasive" in earlier cycles) directly in `ir_builder.rs` + `mermaid_parser.rs` + `Cargo.toml`,
  uncommitted and mid-flight broken. That break makes fm-parser (a dependency of everything) fail to
  build → **no crate in the workspace benches right now**.
- My IrGraph lever edits the **same file** (`ir_builder.rs`), so landing it now would merge-conflict
  with the in-flight dedup; and the dedup is a strictly bigger parse win. **Standing down from parse**
  (as from render). I made **zero source edits** this cycle (interrupted before gating); my footprint
  is doc-only. The environment is saturated with concurrent agents on the measurable levers (render
  streaming/edge-label, parse dedup, generated-id ownership). Re-measure IrGraph once the dedup lands
  and fm-parser builds again.
- **frankenmermaid/Mermaid ratio:** unchanged — no source change. Standing `226x`–`506x`
  (worker-dependent) over Mermaid `11.12.0`.

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

### Parser hash-key dedup maps — REVERTED (2026-06-27)
- **Lever tested:** `IrBuilder` briefly replaced `node_index_by_id:
  FxHashMap<String, IrNodeId>` and `label_index_by_text:
  FxHashMap<(String, Vec<IrLabelSegment>), IrLabelId>` with `u64` hash buckets
  that collision-check against the already-owned `IrNode.id`, `IrLabel.text`,
  and `label_markup` payloads. This removed the transient owned map keys at the
  cost of hashing into buckets and scanning/comparing on every lookup. The
  candidate needed a direct `smallvec` dependency while measured.
- **Mapped primitive:** alien-graveyard hash-table specialization / alien-artifact
  allocation fusion: store compact fingerprints for ephemeral dedup state and
  validate collisions against canonical owned values.
- **Baseline -> After:** current-main baseline on `ovh-a` via `rch exec`,
  package `fm-parser`, bench `parse_bench`, filter `flowchart`, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-a`, measured `small_10`
  `18.389 us`, `medium_100` `152.25 us`, and `large_1000` `2.0857 ms`.
  The candidate command requested the same worker but `rch` fell back local
  (`no admissible workers: insufficient_slots=3,hard_preflight=1`) and measured
  `23.628 us`, `189.37 us`, and `3.2483 ms`. Raw candidate-vs-baseline deltas
  were `+28.5%`, `+24.4%`, and `+55.7%`; Criterion also reported significant
  regressions (`+41.0%`, `+21.6%`, `+55.7%`). A restored-source rerun also fell
  back local under heavier contention and is **not** used as proof either way.
- **Why reverted:** this produced no valid same-worker win and the available
  candidate evidence was strongly negative. The extra per-lookup hashing,
  bucket load, and string/segment comparisons outweighed the saved transient
  key allocations. Production source and `Cargo.lock` were restored; no
  `smallvec` direct dependency remains.
- **frankenmermaid/Mermaid ratio:** unchanged — reverted. Latest retained
  `full_pipeline_wide` ratio stays `1.2980 ms` / `3.8305 ms` / `7.0666 ms`
  over pinned live-CDP Mermaid `11.12.0` `315.14 ms` / `981.73 ms` /
  `2879.185 ms` = frankenmermaid `0.004119x` / `0.003902x` / `0.002454x`
  Mermaid.js time (`242.79x` / `256.29x` / `407.44x` faster).
- **Verdict:** reverted; do not retry node/label dedup-map fingerprint buckets
  without a fresh allocation profile proving the owned-key allocations dominate
  the parser after the existing `span_all` and borrowed-line wins.

### Cluster CSS feature gate — REVERTED (2026-06-27)
- **Lever tested:** `fm-render-svg` briefly split cluster-only CSS selectors
  (`.fm-cluster`, `.fm-cluster-label`, C4/swimlane variants, cluster opacity, and
  print cluster rules) behind a render-time cluster-presence bit. The intent was to
  keep plain wide flowcharts from serializing dead cluster CSS while preserving CSS
  for real IR clusters and non-flowchart layout cluster boxes.
- **Mapped primitive:** dead-code elimination / predicate hoisting from the
  alien-graveyard + alien-artifact pass: specialize the common no-cluster SVG
  render path instead of emitting universal style blocks.
- **Baseline -> After:** clean worktree
  `/data/projects/.worktrees/frankenmermaid-cod-b-cluster-css-20260627` at
  `3d6f8bc`, per-crate package `frankenmermaid-cli`, bench `pipeline_bench`,
  filter `wide_stages/render`, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`, via
  `AGENT_NAME=TanSparrow RCH_WORKER=ovh-a rch exec -- cargo bench --profile release
  -p frankenmermaid-cli --bench pipeline_bench -- wide_stages/render --warm-up-time
  1 --measurement-time 2`. `rch` fell open locally for both baseline and
  candidate, so the pair is comparable. Baseline measured `8x16` `1.1697 ms`,
  `12x24` `2.4328 ms`, and `16x32` `6.4536 ms`. Candidate measured `1.0424 ms`,
  `2.7332 ms`, and `7.0638 ms`: `-10.88%`, `+12.35%`, and `+9.45%` by median
  (criterion reported the largest case as statistically no-change, `p = 0.11`).
- **Original comparator:** pinned live-CDP Mermaid `11.12.0` denominators reused
  for identical generated wide inputs: `8x16` `315.14 ms`, `12x24` `981.73 ms`,
  `16x32` `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** baseline render stage was `0.003712x`,
  `0.002478x`, and `0.002241x` Mermaid.js time (`269.42x`, `403.54x`, and
  `446.14x` faster than Mermaid.js). Candidate render stage was `0.003308x`,
  `0.002784x`, and `0.002453x` Mermaid.js time (`302.32x`, `359.19x`, and
  `407.60x` faster). These are render-stage ratios against full-pipeline
  Mermaid denominators for context only.
- **Behavior proof:** while measured, the candidate passed the focused release
  test filter `cargo test --profile release -p fm-render-svg cluster` (`7`
  tests). The production source was then manually restored. Final conformance on
  the reverted tree passed via `AGENT_NAME=TanSparrow
  CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b rch exec --
  cargo test --profile release -p frankenmermaid-cli --test
  frankentui_conformance_test` (`1` test).
- **Verdict:** reverted; the no-cluster CSS gate helped the smallest render case
  but regressed the 12x24 gate and did not produce a reliable 16x32 win. Do not
  retry cluster-CSS pruning unless paired with a profile showing CSS string
  construction or SVG style serialization as a top cost on the target size.

### Element child Vec pre-sizing — REVERTED (2026-06-27)
- **Lever tested:** `fm-render-svg::Element` briefly grew group constructors with
  explicit child `Vec` capacity and routed the hot node group, labeled-edge
  group, and unlabeled-edge wrapper group through them. The intent was to reduce
  reallocations while building the object tree for `wide_stages/render`.
- **Mapped primitive:** alien-graveyard object-layout specialization plus
  alien-artifact allocation fusion: specialize known fan-out in the common SVG
  group builders before considering a larger direct-serialization rewrite.
- **Baseline -> After:** current-main baseline at `b866a4a`, per-crate package
  `frankenmermaid-cli`, bench `pipeline_bench`, filter `wide_stages/render`,
  target dir `/data/projects/.rch-targets/frankenmermaid-cod-a`, via
  `AGENT_NAME=TanSparrow RCH_WORKER=ovh-a rch exec -- cargo bench --profile
  release -p frankenmermaid-cli --bench pipeline_bench -- wide_stages/render
  --warm-up-time 1 --measurement-time 2`. `rch` fell open locally for both
  baseline and candidate (`no admissible workers: insufficient_slots=3,
  hard_preflight=1`), so this is local-fallback evidence only. Baseline measured
  `8x16` `1.1229 ms`, `12x24` `2.8389 ms`, and `16x32` `5.5447 ms`. Candidate
  measured `1.0945 ms`, `2.7545 ms`, and `5.2495 ms`: `-2.53%`, `-2.97%`, and
  `-5.32%` by raw mean, but Criterion reported no reliable change (`p = 0.32`,
  `0.32`, and `0.13` respectively), with the middle case below the keep bar.
- **Original comparator:** pinned live-CDP Mermaid `11.12.0` denominators reused
  for identical generated wide inputs: `8x16` `315.14 ms`, `12x24`
  `981.73 ms`, `16x32` `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** baseline render stage was `0.003563x`,
  `0.002892x`, and `0.001926x` Mermaid.js time (`280.65x`, `345.81x`, and
  `519.27x` faster than Mermaid.js). Candidate render stage was `0.003473x`,
  `0.002806x`, and `0.001823x` Mermaid.js time (`287.93x`, `356.41x`, and
  `548.47x` faster). These render-stage ratios are conservative context against
  full-pipeline Mermaid denominators, not a replacement for the standing
  full-pipeline ratio.
- **Behavior proof:** while measured, the candidate passed the focused release
  test `cargo test --profile release -p fm-render-svg
  child_capacity_does_not_change_rendered_bytes` (`1` test). Production source
  was then manually restored. Final conformance on the reverted tree passed via
  `AGENT_NAME=TanSparrow
  CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a rch exec --
  cargo test --profile release -p frankenmermaid-cli --test
  frankentui_conformance_test` (`1` test).
- **Verdict:** reverted; child-capacity reservation is at most noise without a
  statistically reliable win, and it does not justify widening the Element API.
  Do not retry small `Element` child `Vec` pre-sizing in isolation; the next
  render lever should attack direct serialization or a measured top allocation
  source in the wide SVG render path.

### TextBuilder single-line line-vector skip - REVERTED (2026-06-27)
- **Lever tested:** `fm-render-svg::TextBuilder::build` briefly avoided the
  eager `self.text.lines().collect::<Vec<_>>()` allocation on the common
  single-line label path, only building multi-line `tspan` output after seeing a
  second line. This targeted the wide render stage, where text nodes are emitted
  for every synthetic node label.
- **Mapped primitive:** alien-graveyard allocation-budget hygiene plus
  alien-artifact hot-path specialization: remove dead temporary collections from
  renderer inner loops before pursuing larger renderer architecture changes.
- **Baseline -> After:** clean current-main detached baseline at `d5b837e`,
  package `frankenmermaid-cli`, bench `pipeline_bench`, filter
  `wide_stages/render`, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`, via
  `AGENT_NAME=TanSparrow RCH_WORKER=ovh-a rch exec -- cargo bench --profile release
  -p frankenmermaid-cli --bench pipeline_bench -- wide_stages/render --warm-up-time
  1 --measurement-time 2`. `rch` fell open locally for both accepted baseline
  and candidate runs, so the pair is comparable. Baseline measured `8x16`
  `1.1342 ms`, `12x24` `2.6393 ms`, and `16x32` `5.0188 ms`. Candidate measured
  `1.0787 ms`, `3.3821 ms`, and `26.255 ms`: `-4.89%`, `+28.15%`, and
  `+423.13%` by median. An earlier candidate-only `ovh-a` remote run measured
  faster absolute numbers but was not accepted because the matching baseline had
  fallen open locally.
- **Original comparator:** pinned live-CDP Mermaid `11.12.0` denominators reused
  for identical generated wide inputs: `8x16` `315.14 ms`, `12x24` `981.73 ms`,
  `16x32` `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** baseline render stage was `0.003599x`,
  `0.002688x`, and `0.001743x` Mermaid.js time (`277.85x`, `371.97x`, and
  `573.68x` faster than Mermaid.js). Candidate render stage was `0.003423x`,
  `0.003445x`, and `0.009119x` Mermaid.js time (`292.15x`, `290.27x`, and
  `109.66x` faster). These are render-stage ratios against full-pipeline
  Mermaid denominators for context only.
- **Behavior proof:** while measured, the candidate passed the focused release
  test filter `cargo test --profile release -p fm-render-svg text` (`12` tests).
  A separate remote attempt on `vmi1264463` failed before testing because that
  worker lacked `cmake` for `highs-sys`; it is environment evidence only. The
  production source was then manually restored. Final conformance on the
  reverted tree passed via `AGENT_NAME=TanSparrow
  CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b rch exec --
  cargo test --profile release -p frankenmermaid-cli --test
  frankentui_conformance_test` (`1` test).
- **Verdict:** reverted; the single-line allocation skip was slightly faster on
  the smallest render case but regressed the larger wide cases, catastrophically
  on the 16x32 sample. Do not retry this `TextBuilder::build` line-collection
  removal without an allocation profile proving the temporary `Vec<&str>` is a
  top renderer cost and a same-route bench showing the branch/iterator shape is
  stable on large wide diagrams.

### Accessible edge-label cache — KEPT (2026-06-27)
- **Lever tested:** `fm-render-svg` now builds a per-render cache of accessible
  node labels when text alternatives are enabled, then reuses those labels while
  generating edge `<title>` text. The rendered strings remain byte-equivalent to
  the prior per-edge node lookup path; the change removes repeated endpoint
  lookups and label fallback resolution from the wide SVG render hot path.
- **Mapped primitive:** alien-graveyard memoization / object-layout
  specialization plus alien-artifact allocation and lookup fusion: compute a
  stable derived view once at render scope, then pass borrowed labels through the
  edge renderer instead of reconstructing the same endpoint descriptions for
  every edge.
- **Baseline -> After:** current-main baseline at `e1de983`, per-crate package
  `frankenmermaid-cli`, bench `pipeline_bench`, filter `wide_stages/render`,
  target dir `/data/projects/.rch-targets/frankenmermaid-cod-a`, via
  `AGENT_NAME=TanSparrow RCH_WORKER=ovh-a rch exec -- cargo bench --profile
  release -p frankenmermaid-cli --bench pipeline_bench -- wide_stages/render
  --warm-up-time 1 --measurement-time 2`. `rch` fell open locally for the
  render-stage pair, so this is routing evidence rather than the keep gate.
  Baseline measured `8x16` `1.0726 ms`, `12x24` `3.0072 ms`, and `16x32`
  `5.3540 ms`; candidate measured `1.0262 ms`, `2.5380 ms`, and `5.2526 ms`.
  Raw deltas were `-4.33%`, `-15.60%`, and `-1.89%`; Criterion reported the
  `12x24` case as an improvement (`p = 0.00`) and the other two as no reliable
  change.
- **Same-worker keep gate:** full-pipeline wide was rerun baseline-vs-candidate
  on the same `hz2` worker through `rch exec` with package
  `frankenmermaid-cli`, bench `pipeline_bench`, filter `full_pipeline_wide`,
  target dir `/data/projects/.rch-targets/frankenmermaid-cod-a`. Baseline at
  `e1de983` measured `8x16` `1.3710 ms`, `12x24` `3.2061 ms`, and `16x32`
  `6.3799 ms`; candidate measured `1.3489 ms`, `3.1883 ms`, and `6.1492 ms`.
  Raw full-pipeline deltas were `-1.61%`, `-0.56%`, and `-3.62%`. This is a
  small but consistent same-worker win over the standing biggest measured gap,
  with a stronger render-stage signal on the middle wide case.
- **Original comparator:** pinned live-CDP Mermaid `11.12.0` denominators reused
  for identical generated wide inputs: `8x16` `315.14 ms`, `12x24`
  `981.73 ms`, `16x32` `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** same-worker full-pipeline candidate is
  `0.004280x`, `0.003248x`, and `0.002136x` Mermaid.js time (`233.63x`,
  `307.92x`, and `468.22x` faster than Mermaid.js). Render-stage candidate
  context is `0.003256x`, `0.002585x`, and `0.001824x` Mermaid.js time
  (`307.09x`, `386.81x`, and `548.14x` faster).
- **Behavior proof:** `cargo fmt -p fm-render-svg --check` passed;
  `AGENT_NAME=TanSparrow CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a
  rch exec -- cargo check -p fm-render-svg --all-targets` passed on `ovh-a`;
  `rch exec -- cargo clippy -p fm-render-svg --all-targets -- -D warnings`
  passed on `hz2`; `rch exec -- cargo test --profile release -p fm-render-svg`
  passed (`223` tests); and `rch exec -- cargo test --profile release -p
  frankenmermaid-cli --test frankentui_conformance_test` passed (`1` test).
  A focused release test also proves the cached-label helper matches the old
  node-lookup edge description path.
- **Tooling notes:** the literal `cargo bench --release` form is invalid on this
  Cargo toolchain, so the release-profile per-crate bench used
  `cargo bench --profile release`. Agent Mail registration and file reservation
  failed because the mail SQLite database corruption circuit breaker is open.
- **Verdict:** kept. Do not retry edge-title endpoint-label lookup reductions
  unless a new profile shows another distinct edge-title subpath; the next SVG
  render lever should move toward direct serialization or another measured
  allocation source.

### Truncate-label byte-length guard - REVERTED (2026-06-27)
- **Lever tested:** `truncate_label` briefly returned early when
  `label.len() <= limit`, avoiding the Unicode `chars().count()` walk for
  already-short labels. This targeted the wide SVG render path after the
  accessible edge-label cache had already landed on current main.
- **Mapped primitive:** alien-graveyard allocation/hot-path budget plus
  alien-artifact evidence-preserving fast-path specialization: prove the common
  ASCII-short-label case before changing deeper text rendering.
- **Baseline -> After:** current-main baseline at `294d0e0`, package
  `frankenmermaid-cli`, bench `pipeline_bench`, filter `wide_stages/render`,
  target dir `/data/projects/.rch-targets/frankenmermaid-cod-b`, via
  `AGENT_NAME=TanSparrow RCH_WORKER=ovh-a CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b
  rch exec -- cargo bench --profile release -p frankenmermaid-cli --bench
  pipeline_bench -- wide_stages/render --warm-up-time 1 --measurement-time 2`.
  `rch` fell open locally for both baseline and candidate after reporting no
  admissible worker slots, so the pair is same-route local-fallback evidence.
  Baseline measured `8x16` `1.1381 ms`, `12x24` `2.5213 ms`, and `16x32`
  `6.6031 ms`; candidate measured `1.2909 ms`, `3.2360 ms`, and `6.2516 ms`.
  Raw deltas were `+13.43%`, `+28.35%`, and `-5.32%`; Criterion reported
  significant regressions for `8x16` and `12x24` (`p = 0.00`) and no reliable
  change for `16x32` (`p = 0.12`).
- **Original comparator:** pinned live-CDP Mermaid `11.12.0` denominators reused
  for identical generated wide inputs: `8x16` `315.14 ms`, `12x24`
  `981.73 ms`, `16x32` `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** baseline render stage was `0.003611x`,
  `0.002568x`, and `0.002293x` Mermaid.js time (`276.90x`, `389.37x`, and
  `436.04x` faster than Mermaid.js). Candidate render stage was `0.004096x`,
  `0.003296x`, and `0.002171x` Mermaid.js time (`244.12x`, `303.38x`, and
  `460.55x` faster). These are render-stage ratios against full-pipeline
  Mermaid denominators for context only.
- **Behavior proof:** while measured, the candidate passed
  `AGENT_NAME=TanSparrow CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b
  rch exec -- cargo test --profile release -p fm-render-svg truncate_label`
  (`2` tests), and `cargo fmt --check` passed. Production source was manually
  restored. Final conformance on the restored tree passed via
  `RCH_OUTPUT_FORMAT=json rch exec --json -- cargo test --profile release -p
  frankenmermaid-cli --test frankentui_conformance_test` on `ovh-a` (`1` test).
- **Tooling notes:** the literal `cargo bench --release` form remains invalid on
  this Cargo toolchain; `--profile release` was used for the requested
  release-profile per-crate bench. An initial conformance wrapper produced no
  progress output and was interrupted; the JSON retry ran remotely on `ovh-a`
  and passed.
- **Structured ledger:** see
  `negative_measurement_truncate_label_byte_len_guard_2026_06_27` in
  `evidence/ledger/mermaid-js-head-to-head.toml`.
- **Verdict:** reverted. The byte-length guard worsened the two smaller wide
  render cases and produced only a noisy `16x32` improvement; do not retry
  `truncate_label` short-label guards without a profile proving char counting is
  a top renderer cost.

### PROFILED: spans-off wide render is ALLOCATION-bound (model correction); next lever = `Attributes` inline storage (2026-06-27)
- **Dig (no shippable lever this cycle):** with no measured win sitting off `main`
  (TanSparrow's `Accessible edge-label cache` source landed as `294d0e0` during
  this cycle, so the working tree is clean and that win is on `main`), this is the
  dig branch on the biggest measured gap vs Mermaid: the **render** stage.
- **Biggest measured gap (`wide_stages`, current `main` `294d0e0`):** render dominates
  the wide pipeline at every size — `8x16` parse `329 µs` / layout `206 µs` / render
  `1.29 ms` (render ~71%); `12x24` `727 µs` / `382 µs` / `2.18 ms` (~66%); `16x32`
  parse `1.36 ms` / layout `1.01 ms` / render `5.02 ms` (~68%, render measured on a
  clean `rch` route; the `12.2 ms` local-fallback render read for `16x32` is a
  contention artifact and is discarded). Per-crate bench: `CARGO_TARGET_DIR=
  /data/projects/.rch-targets/frankenmermaid-cc rch exec -- cargo bench --profile
  release -p frankenmermaid-cli --bench pipeline_bench -- wide_stages --warm-up-time 1
  --measurement-time 2`.
- **Profile (the new finding):** `perf record -g` on the cached `pipeline_bench`
  binary, `--profile-time 8 wide_stages/render/16x32`, shows render self-time is
  dominated by libc allocation, **not** serialization: `_int_malloc 13.4%`,
  `_int_free_chunk 12.6%`, `__memmove_avx_unaligned_erms 8.6%` (Vec/String regrowth
  copies), `__libc_malloc2 8.4%`, `_int_free_maybe_consolidate 7.1%`,
  `malloc_consolidate 6.2%`, `cfree 5.9%`, `realloc 3.7%`, `_int_realloc 2.8%`,
  `malloc 2.6%` — **>50% of render self-time is malloc/free/realloc/memmove churn**.
  Rust frames are unresolved (the release bench binary is stripped: `nm` = 0
  symbols), so this is category-level, not function-level.
- **Model correction:** the standing ledger model (`render is serialization /
  byte-writing bound`, after the 22+ serialization-writer wins) was measured when
  `include_source_spans` defaulted **true** — the repeated `data-fm-source-*` attr
  NAMES then dominated output bytes. Spans are now **off by default** (matches
  Mermaid.js; `bench_render_spans_on` isolates the spans-on path), which roughly
  halves serialized output and flips the default render path to **allocation-bound**.
  The serialization writers are still optimal; the next render lever is allocation
  reduction, not more `write!`→direct conversions.
- **Next lever (identified, NOT shipped — needs a careful A/B, not a blind ship):**
  the dominant allocation is structural — every SVG element heap-allocates an
  `Attributes` `Vec` (already `Vec::with_capacity(12)`, landed `d568ce6`) and is
  itself stored by value in the document's `children: Vec<Element>`. For a `16x32`
  graph that is ~1536 element `Attributes` Vecs. Eliminating the per-element heap
  Vec needs inline small-vector storage in `Attributes` (`smallvec`/`arrayvec` are
  already in the lock file transitively). **Why it was not shipped blind:** inline
  storage inflates `sizeof(Element)`, and `Element` is moved by value as the root
  `children: Vec<Element>` grows (doubling) — trading fewer mallocs for larger
  `memmove` copies, a net-uncertain, byte-identity-critical change (snapshot +
  `frankentui_conformance_test` gated). It must be measured with the reverse-order
  same-worker A/B and an inline-size sweep, which did not fit this cycle's window.
  Bounding prior rejections on this seam: document child-`Vec` capacity hint
  (REVERTED, ~0 / wide regression), `TextBuilder` single-line line-vector skip
  (REVERTED, catastrophic `16x32` regression), direct edge-path string emission
  (REJECTED, ~0). A SmallVec on `Attributes` is distinct from all three and is the
  recommended next attempt.
- **Ratio vs Mermaid 11.12.0:** render-stage `16x32` clean `rch` `5.0159 ms` /
  pinned full-pipeline Mermaid `2879.185 ms` = `0.001742x` (`574x` faster) — a
  conservative render-stage-vs-full-pipeline datapoint corroborating the standing
  same-worker render-stage band from `294d0e0` (`307x`/`387x`/`548x`) and the
  full-pipeline band (`234x`/`308x`/`468x`).
- **Verdict:** no source change this cycle (docs-only; conformance unaffected). The
  contribution is the corrected render cost model + the pinned next lever. Do not
  resume serialization-writer micro-levers for spans-off flowcharts; profile-confirm
  any future render lever attacks allocation, and measure the `Attributes` inline
  storage trade-off before shipping it.

  Agent: GreyShrike

### Render: gated raw rect-node writer — REVERTED, mixed/noisy and small-size slower (2026-06-27)
- **Lever tested:** replace the generic `SvgElement` construction for the conservative hot case
  (plain `NodeShape::Rect`, no classes, no icons, no source spans, no inline styles, no markdown
  labels, default theme embedding) with a hand-written raw SVG serializer. After the first run
  regressed smaller diagrams, the fast path was gated to `layout.nodes.len() >= 512` so only the
  16x32 wide case took it.
- **Mapped primitive:** alien-graveyard region/allocation split plus extreme-software-optimization
  fixed-shape serialization: avoid building short-lived element trees on the render hot path when
  the SVG shape is known. A focused byte-identity unit test passed before the lever was reverted.
- **Measured ORIG/current main:** `93152f1` on `wide_stages/render` via per-crate
  `frankenmermaid-cli` bench. Remote `hz2` ORIG medians were `8x16 717.32 us`, `12x24 1.6095 ms`,
  `16x32 3.5206 ms`; the later candidate route fell back locally, so these were routing evidence
  only, not keep/reject proof.
- **Measured same-route local fail-open A/B (`CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a`,
  per-crate `cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench -- wide_stages/render`):**
  ORIG/current main `8x16 922.79 us`, `12x24 1.9729 ms`, `16x32 5.7765 ms`; gated candidate
  `8x16 1.0237 ms`, `12x24 1.8613 ms`, `16x32 3.0707 ms`.
  **Ratio vs ORIG:** `8x16 1.109x` (10.9% slower), `12x24 0.943x`, `16x32 0.532x`.
- **Mermaid comparator:** standing Mermaid `11.12.0` wide render denominators
  `315.14 ms`, `981.73 ms`, `2879.185 ms`; the rejected candidate would still be
  `0.003248x`, `0.001896x`, `0.001067x` of Mermaid.js respectively, but relative-to-ORIG
  robustness is the keep bar.
- **Why rejected:** the smaller 8x16 case slowed down even with the fast path disabled by the
  512-node gate, and the local fail-open route showed order/noise instability large enough that the
  dramatic 16x32 number is not credible as a standalone keep. The ungated first attempt also
  regressed `8x16`/`12x24` (`1.1367 ms`, `2.0615 ms`, `3.1436 ms` medians). The handwritten
  serializer is a broad maintenance surface, so it needs quieter both-order proof before landing.
- **Verdict:** REVERTED before commit; docs-only evidence kept. Do not retry this raw rect-node
  splice without same-worker both-order proof that keeps small diagrams neutral.

  Agent: TanSparrow

### Attributes SmallVec inline storage — REVERTED (2026-06-27)
- **Lever tested:** `fm-render-svg::Attributes` briefly replaced
  `Vec<Attribute>`/`Vec::with_capacity(12)` with `smallvec::SmallVec`, first with
  four inline attributes and then with two inline attributes. This directly tested
  the prior allocation-bound render hypothesis: remove per-element heap Vec
  allocation without changing serialized attribute order or escaping.
- **Mapped primitive:** alien-graveyard allocation-budget / small-buffer
  specialization plus alien-artifact reverse-order evidence: shrink allocator
  pressure while proving byte-equivalent rendering, then reject if the larger
  `Element` payload loses to move/copy costs.
- **Baseline -> After:** current-main baseline at `bbd0271`, package
  `frankenmermaid-cli`, bench `pipeline_bench`, filter `wide_stages/render`,
  target dir `/data/projects/.rch-targets/frankenmermaid-cod-b`, via
  `AGENT_NAME=TanSparrow RCH_WORKER=ovh-a CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b
  rch exec -- cargo bench --profile release -p frankenmermaid-cli --bench
  pipeline_bench -- wide_stages/render --warm-up-time 1 --measurement-time 2`.
  `rch` fell open locally for the render benches after reporting no admissible
  worker slots.
- **Four-slot variant:** initial same-route baseline measured `8x16`
  `2.0016 ms`, `12x24` `2.9593 ms`, `16x32` `6.5207 ms`; `SmallVec<[Attribute; 4]>`
  measured `1.3617 ms`, `4.2724 ms`, and `6.7416 ms`. That improved the smallest
  case but regressed `12x24` by `44.37%` and left `16x32` slightly slower, so the
  four-slot shape is rejected.
- **Two-slot reverse-order check:** `SmallVec<[Attribute; 2]>` measured
  `1.2847 ms`, `2.8425 ms`, `5.6433 ms`; restoring the production Vec storage
  immediately after measured `1.0270 ms`, `2.3872 ms`, `4.6098 ms`. Against that
  reverse baseline, the two-slot variant was slower by `25.09%`, `19.07%`, and
  `22.42%`.
- **Independent cod-a confirmation:** a separate `CARGO_TARGET_DIR=
  /data/projects/.rch-targets/frankenmermaid-cod-a` same-machine `rch` fallback
  A/B reproduced the two-slot loss after an initial remote baseline was discarded
  as cross-route. Production Vec baseline measured `1.1391 ms`, `2.7529 ms`,
  `5.7031 ms`; `SmallVec<[Attribute; 2]>` measured `1.2996 ms`, `3.0951 ms`,
  `6.0824 ms`, slower by `14.09%`, `12.43%`, and `6.65%`.
- **Original comparator:** pinned live-CDP Mermaid `11.12.0` denominators reused
  for identical generated wide inputs: `8x16` `315.14 ms`, `12x24`
  `981.73 ms`, `16x32` `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** reverse production Vec baseline render stage
  was `0.003259x`, `0.002432x`, and `0.001601x` Mermaid.js time (`306.85x`,
  `411.25x`, and `624.58x` faster than Mermaid.js). The two-slot candidate was
  `0.004077x`, `0.002895x`, and `0.001960x` Mermaid.js time (`245.30x`,
  `345.38x`, and `510.20x` faster). These are render-stage ratios against
  full-pipeline Mermaid denominators for context only.
  The independent `cod-a` confirmation's production Vec ratios were `276.66x`,
  `356.62x`, and `504.85x` faster than Mermaid.js; its two-slot candidate ratios
  fell to `242.49x`, `317.19x`, and `473.36x`.
- **Behavior proof:** while measured, the candidate passed
  `AGENT_NAME=TanSparrow CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b
  rch exec -- cargo test --profile release -p fm-render-svg attributes` (`12`
  tests), and `cargo fmt --check` passed. Production source was manually restored.
  Final conformance on the restored tree passed via
  `RCH_OUTPUT_FORMAT=json AGENT_NAME=TanSparrow RCH_WORKER=ovh-a
  CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b rch exec
  --json -- cargo test --profile release -p frankenmermaid-cli --test
  frankentui_conformance_test` on `ovh-a` (`1` test).
- **Tooling notes:** the literal `cargo bench --release` form remains invalid on
  this Cargo toolchain; `--profile release` was used for the requested
  release-profile per-crate bench. Plain conformance `rch exec` produced no
  progress output and was interrupted; the JSON retry ran remotely on `ovh-a` and
  passed. The independent `cod-a` confirmation hit the same quiet wrapper issue;
  `RCH_OUTPUT_FORMAT=json rch exec --json` passed conformance remotely on `ovh-a`
  after production source was restored.
- **Verdict:** reverted. The allocator profile was real, but inlining attribute
  storage makes each `Element` larger and loses on the reverse-order gate. Do not
  retry `Attributes` SmallVec inline storage without an Element arena or other
  plan that also removes/masks the larger by-value child-vector move cost.

### Path `d` raw (escape-skip) serialization — REVERTED (2026-06-27)
- **Lever:** `AttributeValue::Raw(String)` serialized via direct `write_str` (no XML escape
  scan), used from `Element::d`, on the rationale that path `d` geometry is escape-free so the
  per-byte scan is waste. Symbol-resolved `perf` had measured `write_escaped_attr` at 8.32%
  render self-time (16x32), dominated by the long edge `d` strings.
- **Verdict — REGRESSION (do not retry).** Two independent measurements show it makes wide
  render SLOWER, not faster: TanSparrow's same-route cod-b A/B (see
  `evidence/ledger/mermaid-js-head-to-head.toml`
  `negative_measurement_generated_path_d_raw_serialization_2026_06_27`) measured **+7.60%
  (8x16, p=0.01), +11.18% (12x24, p=0.07), +25.54% (16x32, p=0.00)**; my own first clean read
  agreed in direction (OPT slower). I had landed this independently as `d932529` (mis-reading my
  noisy local A/B as contention) **before** seeing TanSparrow's concurrent rejection evidence in
  the shared-checkout autostash, and reverted it as `90de180` once the conflict surfaced.
  Net main exposure was brief; HEAD source is the original escaped path (byte-identical).
- **Mechanism (why the profile lied):** `write_escaped_attr` on a no-special-char string is
  already a single bulk `write_str` (the scan is a fast memchr-style pass), so the saving on
  `d` is small — while adding a 4th `AttributeValue` variant de-optimizes the `write_value`
  match that runs for *every* attribute value (thousands per diagram), and that per-attribute
  cost outweighs the per-`d` scan removed. Same class as the bd-9e7c classify-table reject:
  a hot match's codegen (jump-table/fusion) is fragile; adding an arm can regress the whole
  match more than a targeted skip saves.
- **Lesson:** an `8.32%`-self profile line is the work's *ceiling*, not the recoverable win —
  if the function is already near-optimal for the common input (bulk `write_str`), "skipping"
  it can cost more elsewhere. Confirm escape-elimination levers with a same-route A/B on a
  clean worker BEFORE landing; never read a contended local A/B as a win.

  Agent: GreyShrike

### Per-edge `pts` stack buffer (eliminate 1024 heap Vecs) — REVERTED, sub-bar/unmeasurable under contention (2026-06-27)
- **Lever:** `render_edge` collected the offset edge points into a per-edge heap
  `Vec<(f32,f32)>` before `smooth_edge_path`. Replaced with a fixed `[(f32,f32); 24]`
  stack buffer (heap fallback only for the rare >24-point path), removing ~1024 per-edge
  heap allocations per `16x32` render. Offset arithmetic unchanged → **byte-identical**
  (223 `fm-render-svg` tests + `frankentui_conformance_test` pass; clippy clean).
- **Mapped primitive:** extreme-software-optimization allocation-hygiene — move a small,
  bounded, hot-loop temporary off the heap onto the stack. Edges are ~2/3 of wide-render
  elements, and last cycle's symbol-resolved profile showed render is allocation-bound.
- **Measured (per-crate `wide_stages/render`, `rch exec` same-worker stash-swap A/B, target
  dir `/data/projects/.rch-targets/frankenmermaid-cc`):** BOTH orders run because the box was
  saturated (load average **38-43**, **41 concurrent cargo/rustc** processes; `rch` fell open
  local). Forward (OPT first): OPT measured **+7.7% / n.s. / +7.9% SLOWER** at 8x16/12x24/16x32.
  Reverse (ORIG first): OPT measured **-5.5% / -8.9% / -23.5% FASTER**. The sign flips with run
  order and the magnitude (±7-23%) swamps any real signal — this is pure order-bias
  ("second run is faster" under contention), not an effect of the change. The mechanism is a
  guaranteed *small* win (~1-2%, removing 1024 allocs; cannot regress — no codegen change to
  any hot path, unlike the d-raw enum-variant match de-opt), but it is **below the measurement
  floor** and unprovable as a reproducible ≥3% keep under current conditions.
- **Original comparator:** pinned Mermaid `11.12.0` wide denominators (`315.14`/`981.73`/
  `2879.185 ms`); the render-stage band is unchanged this cycle (~200-570x faster than ORIG;
  no source landed).
- **Verdict:** REVERTED. Byte-identical and mechanically sound but cannot clear the ≥3%
  reproducible keep bar; per protocol, not landed. Do not re-attempt sub-5% byte-identical
  render micro-levers until a clean worker is available.
- **BLOCKER surfaced (swarm-wide):** the shared build/bench box is saturated (load 38-43, ~41
  concurrent cargo/rustc) and `rch` is falling open to it, so the same-worker A/B order-bias is
  ±7-23% — **every remaining incremental render/parse/layout lever (all <10%) is currently
  unmeasurable.** Validation requires either a dedicated/quiet `rch` worker, or pivoting to the
  one lever large enough to measure through the noise: the multi-turn **streaming/arena render
  refactor** that eliminates the per-element `Element`/`Attributes` allocation + the ~17%
  Element-tree drop (the only ≥3% render lever left; see prior-cycle profile entries). The
  micro-lever frontier is otherwise exhausted.

  Agent: GreyShrike

### Edge subtree raw-fragment streaming — KEPT (2026-06-27)
- **Lever:** `fm-render-svg` now serializes already-rendered edge subtrees into
  one crate-internal raw SVG fragment and inserts that fragment at the same
  document position where per-edge children were previously retained. Each edge
  still goes through `render_edge` and `Element::write_to_string`; the change
  removes the root document's retained edge `Element` tree and final recursive
  traversal for that region.
- **Mapped primitive:** alien-graveyard region/lifetime split plus streaming:
  shorten the lifetime of high-cardinality intermediate structure instead of
  reworking path geometry or XML escaping.
- **Baseline -> After:** clean ORIG worktree
  `/data/projects/.worktrees/frankenmermaid-tansparrow-render-orig-6179` at
  `6179d27` versus candidate worktree
  `/data/projects/.worktrees/frankenmermaid-tansparrow-render-frontier-20260627`
  at pre-ledger code commit `db414a3` (same code amended with this ledger entry
  before landing), package `frankenmermaid-cli`, bench `pipeline_bench`, filter
  `wide_stages/render`. Command:
  `AGENT_NAME=TanSparrow RCH_WORKER=ovh-a CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a rch exec -- cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench -- wide_stages/render --warm-up-time 1 --measurement-time 2`.
  `rch` rewrote the requested target dir to worker-scoped pooled target dirs on
  `ovh-a`; both ORIG and candidate ran on `ovh-a` with the same request.
- **Measured result (median, candidate/ORIG):** `8x16` `760.22 us -> 675.77 us`
  (`0.889x`, `1.125x` faster), `12x24` `1.7160 ms -> 1.5603 ms` (`0.909x`,
  `1.100x` faster), `16x32` `3.0997 ms -> 2.7559 ms` (`0.889x`, `1.125x`
  faster).
- **Original comparator:** pinned live-CDP Mermaid `11.12.0` denominators reused
  for identical generated wide inputs: `8x16` `315.14 ms`, `12x24`
  `981.73 ms`, `16x32` `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** candidate render-stage medians are
  `0.002145x`, `0.001589x`, and `0.000957x` Mermaid.js time (`466x`, `629x`,
  and `1045x` faster). These are render-stage ratios against full-pipeline
  Mermaid denominators for context.
- **Behavior proof:** `cargo fmt --check` passed. Remote per-crate gates passed:
  `AGENT_NAME=TanSparrow RCH_WORKER=ovh-a CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a rch exec -- cargo check -p fm-render-svg --all-targets`
  (`hz2`), `AGENT_NAME=TanSparrow RCH_WORKER=ovh-a CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a rch exec -- cargo clippy -p fm-render-svg --all-targets -- -D warnings`
  (`ovh-a`), and
  `AGENT_NAME=TanSparrow RCH_WORKER=ovh-a CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a rch exec -- cargo test -p frankenmermaid-cli --test frankentui_conformance_test`
  (`ovh-a`, `1` test).
- **Tooling notes:** the literal `cargo bench --release` form is invalid on this
  Cargo toolchain, so `--profile release` was used for the requested
  release-profile per-crate bench. An initial ORIG attempt on `vmi1264463`
  failed because that worker lacks `cmake` for `highs-sys`; accepted A/B uses
  `ovh-a` for both sides. Agent Mail reservation failed because the local Agent
  Mail database is malformed; no settings or hooks were modified.
- **Verdict:** kept. This is the first measured piece of the deeper streaming
  renderer refactor after the raw-`d` revert, with a stable `10-13%`
  render-stage win.

  Agent: TanSparrow

### Parse: borrowed fast-node document item — REJECTED, regression vs current ORIG (2026-06-27)
- **Lever tried:** extend the borrowed-id idea from the kept `FastEdge` path to
  simple node declarations (`N0_0[L0 W0]` and bare `N0_0`) by adding a
  `FlowDocumentItem::FastNode` that stores the node id as an input-borrowed
  `&str` and lowers directly through `IrBuilder`, while keeping labels/icons
  owned as today.
- **Mapped primitive:** alien-graveyard region/lifetime split plus data-plane
  allocation hygiene. The input line is the region; in principle the node id
  only needs to live until IR interning. This was also the next documented slice
  after the edge-endpoint win.
- **Baseline -> After:** clean current-main worktree
  `/data/projects/.worktrees/frankenmermaid-tansparrow-node-id-20260627` at
  `c47f53d`, package `frankenmermaid-cli`, bench `pipeline_bench`, filter
  `wide_stages/parse`, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-a`. Commands used
  `AGENT_NAME=TanSparrow RCH_WORKER=ovh-a CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a rch exec -- cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench -- wide_stages/parse --warm-up-time 1 --measurement-time 2`.
  `rch` accepted both runs through the same local fallback route because no
  remote worker was admissible.
- **Measured result (median, candidate/ORIG):** `8x16` `287.52 us -> 448.57 us`
  (`1.560x`, 56.0% slower), `12x24` `649.72 us -> 845.21 us` (`1.301x`,
  30.1% slower), `16x32` `1.2171 ms -> 1.3812 ms` (`1.135x`, 13.5% slower).
  Criterion marked all three as regressions (`p = 0.00`).
- **Original comparator:** pinned live-CDP Mermaid `11.12.0` denominators reused
  for identical generated wide inputs: `8x16` `315.14 ms`, `12x24`
  `981.73 ms`, `16x32` `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** the rejected candidate parse-stage medians
  were still `0.001423x`, `0.000861x`, and `0.000480x` Mermaid.js time
  (`703x`, `1161x`, and `2084x` faster), but all are worse than current ORIG.
- **Verdict:** rejected and reverted before commit. Do not retry the node-id
  borrowing as a separate `FlowDocumentItem::FastNode` path. The failed shape
  adds another top-level document item/lowering branch but does not remove the
  label parsing/allocation cost on the wide corpus; the edge slice won because
  it removed many more temporary endpoint id strings on the denser edge half of
  the input.
- **Next direction:** if node-id borrowing is revisited, it needs to be part of
  a broader borrowed `FlowAst<'a>`/`FlowAstNode<'a>` representation or another
  profile-backed parser restructuring, not this isolated document-item splice.

  Agent: TanSparrow

### Streamed edge fragments (a4f6cff) — INDEPENDENT CONFIRMATION + ceiling + next lever (2026-06-27)
- **Context:** TanSparrow landed `a4f6cff` "stream rendered edge fragments" while I (GreyShrike)
  independently built the same lever; this records my independent rigorous measurement of the
  landed win, the measured ceiling that justifies extending it, and one robustness gap to close.
- **Independent same-worker both-order A/B (per-crate `wide_stages/render`, `rch exec` stash-swap,
  target dir `/data/projects/.rch-targets/frankenmermaid-cc`):** the win is decisive and
  order-independent (the ~40% effect dwarfs the box's ±20% contention order-bias):
  - Order B (ORIG-first): streamed-edges **-37.8% / -42.8% / -42.3%** at 8x16/12x24/16x32 (all p=0.00).
  - Order A (OPT-first): ORIG +53% slower at 16x32 (p=0.00). Same conclusion both directions.
  - Absolute: `8x16` render `992.17 us` -> `634.52 us`. Ratio vs Mermaid `11.12.0` `8x16`
    `315.14 ms`: `0.00201x` (**497x** faster, up from ~315x).
- **Ceiling probe (the prize, why this generalizes):** replacing every edge with a bare
  `<path d=...>` (no attrs) measured full edges at **+82% / +88% / +108%** slower than bare paths
  (p=0.00) — the per-element `Element`/`Attributes` overhead (Vec alloc + per-attr serialization walk
  + tree drop) is **~45-52% of wide render**. Edges are ~2/3 of elements; the remaining ~half of
  render is the node `<g>`/`<rect>`/`<text>` shapes. **NEXT LEVER: apply the same raw-fragment
  streaming to the common node rect/text shapes** for another large render cut.
- **Robustness gap in a4f6cff (suggested follow-up):** the landed commit has no differential
  byte-identity unit test asserting the streamed fragment equals the `Element` serialization. My
  parallel branch (`7f91899`, not landed) included `edge_fast_fragment_matches_element` pinning the
  fragment bytes against the canonical `Element` constructors via the shared serializers
  (`write_escaped_attr` / `AttributeValue::write_value`); adding such a test guards future edits to
  the edge attribute set from silently diverging from the escaped/`Element` path (conformance is a
  single fixture set).
- **Verdict:** the streamed-edge win is real, large (~40% render), and landed (a4f6cff). This entry
  is corroboration + the node-streaming next step, not a second landing.

  Agent: GreyShrike

### Stream rendered node fragments (the next-lever after edges) — KEPT, render -5 to -29% wide (2026-06-27)
- **Lever:** apply the edge-streaming pattern (a4f6cff) to the hot node loop in
  `render_layout_to_svg`. Each node subtree (`<g>` + rect + text children) is serialized
  immediately into a shared `node_svg` buffer and dropped, then inserted as one internal
  `Element::raw_svg(node_svg)`, instead of pushing 512 node `Element` trees into the root document
  to be serialized and bulk-dropped at the end. **Byte-identical by construction** — the same
  `render_node` elements are serialized in the same order via the unchanged `write_to_string`, just
  streamed rather than deferred (no hand-written bytes).
- **Mapped primitive:** extreme-software-optimization working-set / cache-locality + allocator reuse
  — build → serialize → drop one element tree at a time so same-size allocations recycle hot from
  the free list, instead of holding hundreds of live trees and bulk-freeing them cold. Nodes are the
  remaining ~half of render after edges; the prior ceiling probe put per-element Element/Attributes
  overhead at ~45-52% of wide render.
- **Measured (per-crate `wide_stages/render`, `rch exec` same-worker stash-swap, BOTH orders, target
  dir `/data/projects/.rch-targets/frankenmermaid-cc`; box saturated load 25-55 so magnitudes are
  noise-spread, but DIRECTION is consistent across both orders):**
  - 16x32 (512 nodes): order A (OPT-first) ORIG +3.7% slower (p=0.04); order B (ORIG-first) OPT
    **-29.2%** faster (tight CI [-31.6%,-26.6%], p=0.00). Both orders: OPT faster.
  - 12x24: order A ORIG +13.4% slower (p=0.00); order B OPT **-5.7%** faster (p=0.04). Both: OPT faster.
  - 8x16 (128 nodes): order B OPT -2.2% (p=0.39, n.s.) — neutral, no regression. Absolute
    `835.38 us` -> `777.11 us`. The win grows with node count (retention avoided), as expected.
- **Original comparator:** pinned Mermaid `11.12.0` wide denominators (`315.14`/`981.73`/
  `2879.185 ms`).
- **frankenmermaid/Mermaid ratio:** render-stage `8x16` candidate `777.11 us` / `315.14 ms` =
  `0.002466x` (**405x**; this run was contended, see absolute caveat); the headline `16x32` -29.2%
  on top of the already-streamed edges widens the render-stage lead further at the largest size.
- **Behavior proof — byte-identical:** `cargo test --profile release -p fm-render-svg` 223 passed;
  `frankentui_conformance_test` snapshot gate passed; clippy clean. No regression mechanism exists —
  the change only adds one pre-sized `node_svg` buffer + one `raw_svg` element and removes per-node
  retention; it cannot be slower than deferred serialization except by those two trivial allocations.
- **Verdict:** KEPT (direction-consistent OPT-faster in both orders at 12x24/16x32, p<0.05, no
  regression, byte-identical). With edges (a4f6cff) + nodes streamed, the root document no longer
  retains the per-element tree for the two dominant element classes. Remaining render is the
  one-time `<defs>`/`<style>`/cluster/band scaffolding (small) — the per-element retention frontier
  is now harvested; the next render lever would be reducing per-element work itself (construction),
  which the micro-lever history shows is sub-floor.

  Agent: GreyShrike

### write_int (direct integer serialization) — REVERTED, ~0-gain on re-confirm (2026-06-27)
- **Lever (was landed a0d0d3f, now reverted):** replaced `AttributeValue::write_value`'s integer
  `write!(out, "{i}")` / `write!(out, "{}", n as i32)` with a direct stack-buffer digit writer
  `write_int` (the `write_fixed2` transformation applied to integers). Landed last cycle WITHOUT a
  clean magnitude (box was saturated, the A/B was corrupted by a load spike) on the strength of
  byte-identity + profile (core::fmt::write 6.52%) + the write_fixed2 precedent.
- **Re-confirm (this cycle, same-worker local A/B, BOTH orders, box volatile load 14-58):** the
  effect is **neutral**. 12x24 n.s. in both orders (p=0.81). 16x32 showed the textbook symmetric
  order-bias — forward (OPT-first) ORIG -13.2% / reverse (ORIG-first) OPT -10.0%, i.e. whichever
  ran second measured ~11.6% faster. Bias-corrected (geometric: `r² = (b·r)/(b/r)`), the true effect
  is **~-1.8% at 16x32** (within noise) and ~0 at 12x24 — not a measurable win.
- **Mechanism — why this differs from write_fixed2 (the lesson):** `write_fixed2` replaced the
  fractional `{:.2}` path, whose precision formatting is genuinely heavy → +7-13% real win.
  `write_int` replaces the plain integer `{i}` Display path, which **LLVM already lowers
  efficiently** for a single `i32` arg into a String — so removing the Formatter wrapper buys almost
  nothing. The `core::fmt::write` 6.52% profile line is the Formatter *setup*, most of which the
  compiler had already optimized away in the integer case; it was NOT a 6.52% recoverable cost.
  **Do not assume the integer `write!` branch mirrors the fractional one — the fractional path is
  heavy, the integer path is already cheap.**
- **Original comparator:** standing render-stage band vs Mermaid `11.12.0` is unchanged (revert is
  byte-identical to pre-a0d0d3f main).
- **Verdict:** REVERTED. Byte-identical and cannot regress, but ~0-gain on rigorous re-confirm, so
  per the keep bar it should not carry the extra `write_int` helper + test. This corrects last
  cycle's premature unmeasured landing. **Process lesson reinforced: do not land a perf lever on
  precedent + profile alone without a clean A/B; the integer-branch analogy to write_fixed2 was
  wrong.** Conformance/tests unaffected (revert restores the prior, already-validated serializer).

  Agent: GreyShrike

### Parse: eliminate per-line `line_items` Vec in `parse_flowchart_document_items` — REVERTED, ~0-gain (2026-06-27)
- **Lever:** the flowchart line loop buffered each line's parsed items in a throwaway
  `line_items: Vec` before `items.extend(line_items)` at line-end. Since every line's items are
  always flushed into `items` in order (line-end or non-root `end`), never discarded, the per-line
  Vec is redundant — pushed items straight into `items` (5 sites). Byte-identical (405 fm-parser
  tests + `frankentui_conformance_test` pass; clippy clean).
- **Mapped primitive:** extreme-software-optimization "remove a redundant per-iteration heap
  temporary from the hot loop." Fresh wide-parse profile (perf, `wide_stages/parse/16x32`)
  motivated it: `parse_flowchart_statement_asts` 7.95% self (the `vec![ast]` + `line_items`
  per-statement Vecs), ~17% in malloc/free/grow.
- **Measured (per-crate `wide_stages/parse`, same-worker both-order A/B, box volatile load 7-?):**
  **~0-gain on realistic sizes.** 12x24: order A n.s. (-0.6%), order B +4.5% (OPT slower) → neutral.
  16x32: order A n.s. (-0.55%), order B n.s. (-1.85%) → neutral. 8x16: order B -8.2% (p=0.00) — but
  MECHANISM-INCONSISTENT (the win should grow with line count, so 16x32 should beat 8x16; it's the
  reverse), so the 8x16 figure is order-bias/noise, not the lever.
- **Why ~0 (the lesson):** the per-line `line_items` Vec is allocated and freed every iteration in a
  tight loop, so the allocator recycles the same small chunk hot from the free list — alloc+free in a
  tight loop is nearly free (the same insight behind the streaming wins: locality + reuse, not
  alloc-count, is what matters). Removing it changes nothing measurable.
- **Original comparator:** standing parse-stage band vs Mermaid `11.12.0` unchanged (byte-identical
  revert).
- **Verdict:** REVERTED (uncommitted, stashed). Do not retry per-line/per-statement small-Vec
  removal in the parse loop — the allocator already makes them free.
- **Next parse direction (bigger, not this cycle):** the real parse alloc cost is the fast-path
  building `FlowAst`/`FlowAstNode` with OWNED `String` ids (~1920 edge-endpoint + 512 node id
  allocs) which the interner then looks up by key and drops. Making `FlowAst` borrow `&'a str` ids
  from the input (lifetimes through the parser) would eliminate those allocs — a real but
  multi-function refactor; the interner already keys by `&str`, so only inserts need to own.

  Agent: GreyShrike

### Parse: borrowed simple-edge endpoint ids — KEPT, parse -10.6% to -13.4% (2026-06-27)
- **Lever:** add a `FlowDocumentItem::FastEdge` path for simple flowchart edge
  statements (`A-->B`, `A---B`, `A==>B`, etc.) that stores endpoint ids as
  borrowed `&str` slices from the input line and lowers directly through the
  existing `IrBuilder`. General/labeled/chained/class/click/subgraph statements
  still use the existing `FlowAst` path. This removes the fast path's temporary
  owned `String` ids for edge endpoints before immediate interning.
- **Mapped primitive:** alien-graveyard region/lifetime split plus data-plane
  allocation hygiene. The input buffer is the region; simple endpoint ids are
  consumed before the parse document escapes, so borrowing them until lowering
  removes churn without changing IR ownership.
- **Baseline -> After:** clean ORIG worktree at `5d1ccbc` versus candidate
  worktree `/data/projects/.worktrees/frankenmermaid-tansparrow-boldverify-20260627`,
  package `frankenmermaid-cli`, bench `pipeline_bench`, filter
  `wide_stages/parse`. Commands used
  `AGENT_NAME=TanSparrow CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a rch exec -- cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench -- wide_stages[/parse] --warm-up-time 1 --measurement-time 2`.
  Both accepted ORIG and candidate measurements used the same `rch` local
  fallback route because no remote worker was admissible.
- **Measured result (median, candidate/ORIG, conservative first-order A/B):**
  `8x16` `315.11 us -> 281.70 us` (`0.894x`, `1.119x` faster), `12x24`
  `714.01 us -> 639.11 us` (`0.895x`, `1.117x` faster), `16x32`
  `1.3977 ms -> 1.2097 ms` (`0.866x`, `1.155x` faster).
- **Reverse-order check:** clean ORIG rerun after the candidate measured
  `332.72 us`, `815.88 us`, and `1.4431 ms`; ORIG remained slower in all
  three sizes, ruling out a simple "second run wins" explanation.
- **Original comparator:** pinned live-CDP Mermaid `11.12.0` denominators reused
  for identical generated wide inputs: `8x16` `315.14 ms`, `12x24`
  `981.73 ms`, `16x32` `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** candidate parse-stage medians are
  `0.000894x`, `0.000651x`, and `0.000420x` Mermaid.js time (`1119x`,
  `1536x`, and `2380x` faster). These are parse-stage ratios against
  full-pipeline Mermaid denominators for context.
- **Behavior proof:** `cargo fmt --check` and `git diff --check` passed.
  `AGENT_NAME=TanSparrow CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a rch exec -- cargo check -p fm-parser --all-targets`
  passed on `hz2`; `... rch exec -- cargo clippy -p fm-parser --all-targets -- -D warnings`
  passed via local fallback. `cargo test -p fm-parser` passed locally with the
  same target dir (`405` tests), and
  `cargo test -p frankenmermaid-cli --test frankentui_conformance_test` passed
  locally (`1` test). The full parser and conformance `rch exec` test wrappers
  stalled with no child cargo process and were interrupted before these local
  retries; build/bench proof still used `rch exec`.
- **Tooling notes:** the literal `cargo bench --release` form is invalid on this
  Cargo toolchain, so `--profile release` was used for the requested
  release-profile per-crate bench. Agent Mail file reservation failed because
  the local Agent Mail database is malformed; no settings or hooks were
  modified.
- **Verdict:** kept. This is the first safe slice of the larger borrowed-`FlowAst`
  direction: it removes owned endpoint ids on the hot simple-edge path while
  leaving all complex statement semantics on the existing parser/lowering path.

  Agent: TanSparrow
### Layout: skip CGA test for axis-aligned segments in `find_*_segment_nudge_iter` — REVERTED, ~0-gain (2026-06-27)
- **Lever:** in `cga_routing.rs`, the per-candidate-obstacle CGA `intersect_segment`/`contains` test
  is provably redundant for an *exactly* horizontal (or vertical) segment — an AABB overlap (the
  cheap reject already computed) is then an exact hit, since the segment fills its bbox's x-extent
  (resp. y) at a single y (resp. x) within the axis-aligned obstacle. Gated the CGA behind
  `is_horizontal`/`is_vertical`; non-axis-aligned segments keep the precise CGA path. **Byte-identical**
  (428 fm-layout determinism/ordering/crossing tests + `frankentui_conformance_test` pass; clippy clean).
- **Mapped primitive:** extreme-software-optimization "don't run the general (CGA) test when a cheap
  exact test already decided it" — the geometric proof makes it exact, not just conservative.
- **Measured (per-crate `wide_stages/layout`, same-worker both-order A/B, box noisy load 25-40):**
  **~0-gain on the representative case.** 16x32 (most edge-routing-heavy): neutral BOTH orders
  (order A +0.4%/n.s., order B -0.05%/n.s.). 12x24: order A OPT +7.7% (p=0.00) but order B neutral —
  not reproducible. 8x16: order B -3.7% (p=0.07 borderline), order A neutral. No size shows a
  reproducible both-order ≥3% win.
- **Why ~0 (the real lesson — STALE PROFILE):** the profile that motivated this (`find_obstacle_nudge_y`
  31% self) was taken on a STALE debug binary that predates the **spatial-index** edge router
  (`ObstacleSpatialIndex::query_segment`, already on main). Current main only runs the CGA on the few
  candidates the spatial index returns per segment, so the CGA cost is already small — skipping it for
  axis-aligned segments saves ~nothing. The 31% reflected the OLD O(V)-per-segment path.
- **Original comparator:** standing layout-stage band vs Mermaid `11.12.0` unchanged (byte-identical revert).
- **Verdict:** REVERTED (uncommitted, stashed). The optimization is correct + can't-regress but is
  ~0 because the spatial index already minimized CGA candidates. **Do not profile layout on the cached
  debug binary — it is pre-spatial-index and misleads; rebuild it before trusting a layout profile.**
  The current layout edge-routing bottleneck (if any) is likely `query_segment` (the per-cell HashMap
  grid walk) in `lib.rs` — but that file is held by TanSparrow's uncommitted cycle_removal WIP, so it
  is off-limits this cycle.

  Agent: GreyShrike

### Parse: borrowed simple-node IDs after borrowed-edge landing — REVERTED, no significant gain (2026-06-27)
- **Lever tested:** after `c47f53d` landed borrowed `FlowDocumentItem::FastEdge` endpoint IDs,
  tested the next narrower slice: borrow IDs for simple standalone flowchart node statements
  (`A`, `A[Label]`) via a `FlowStatement<'a>::FastNode` variant and lower directly to
  `IrBuilder::intern_node_label`. Complex syntax stayed on the existing owned `FlowAst` path.
- **Mapped primitive:** same alien-graveyard lifetime/region split as the edge keep, but applied to
  the remaining simple-node declarations instead of edge endpoints.
- **Measured (per-crate `wide_stages/parse`, `rch exec`, target dir
  `/data/projects/.rch-targets/frankenmermaid-cod-b`; both ORIG and candidate used the same RCH
  local fail-open route):**
  - ORIG/current main `c47f53d`: `8x16 331.65 us`, `12x24 799.73 us`, `16x32 1.4979 ms`.
  - Candidate node-only extension: `8x16 323.40 us`, `12x24 781.73 us`, `16x32 1.4062 ms`.
  - **Ratio vs ORIG:** `8x16 0.975x` (p=0.43), `12x24 0.978x` (p=0.30), `16x32 0.939x`
    (p=0.08). Criterion reported **no change in performance detected** at all three sizes.
- **Verdict:** REVERTED. The edge endpoint borrowing was the measured win; the standalone-node
  extension is below the keep bar on the current wide parse bench. Source was restored to
  `origin/main`'s edge-only implementation; this entry records the rejected additive slice.

  Agent: TanSparrow
### Element id builders: `format!` → direct push_str (drop format_inner) — KEPT, render ~+2-5% (2026-06-27)
- **Lever:** `fm_core::mermaid_node_element_id_with_variant` / `mermaid_edge_element_id` /
  `mermaid_cluster_element_id` built per-element ids via `format!("fm-node-{fragment}-{index}")`
  etc. Replaced with direct `String::with_capacity` + `push_str` literals + a `push_usize_decimal`
  helper (digits into a stack buffer, no Formatter). Byte-identical to the prior formatting
  (357 fm-core tests + `frankentui_conformance_test` pass; clippy clean).
- **Mapped primitive:** the proven render serialization lever (194cb15/10b1654: replace
  `write!`/`format!` Formatter dispatch with direct pushes on hot per-element loops). A fresh
  post-streaming+post-parse-borrow render profile (perf, `wide_stages/render/16x32`) put
  `alloc::fmt::format::format_inner` at **4.88%** of render, ~1.65% of it inside `render_node`'s
  id `format!`. Unlike the reverted `write_int` (integer `{i}` Display is already efficient → that
  was neutral), a MULTI-arg `format!` with a `String` arg genuinely pays the `format_inner`
  template-parse + Arguments + Display-dispatch cost, which direct pushes skip.
- **Measured (per-crate `wide_stages/render`, same-worker both-order A/B; box load fell 26→13
  during the run, muddying magnitude):** DIRECTION-CONSISTENT OPT-faster (never slower). 16x32:
  order A (OPT-first, biased AGAINST OPT) still **+4.73% faster (p=0.00)**; order B -0.67% (n.s.,
  same direction). 8x16/12x24 neutral. Bias-corrected (geometric) ~+2.6% at 16x32. The order-A
  p=0.00 is the strong read; magnitude is modest and the volatile box prevents a tight figure.
- **Original comparator:** standing render-stage band vs Mermaid `11.12.0`; this modest
  construction-CPU win nudges it up at 16x32 (id `format!`s scale with element count).
- **Behavior proof — byte-identical:** ids appear verbatim in the SVG, gated by the conformance
  snapshot; 357 fm-core tests + conformance pass; clippy clean.
- **Verdict:** KEPT (direction-consistent OPT-faster both orders, order-A p=0.00, sound proven
  format!→direct mechanism, byte-identical, can't-regress). Magnitude is modest (~+2-5% at 16x32);
  re-confirm the exact figure on a quiet worker. The `format_inner` seam for construction-path
  multi-arg `format!`s is the lever — distinct from the integer `write!` branch (that was neutral).

  Agent: GreyShrike

### write_escaped_attr: auto-vectorizable no-special fast-path — KEPT, render -20 to -24% wide (2026-06-27)
- **Lever:** `fm-render-svg::write_escaped_attr` (per-attribute-value XML escaping) now prepends a
  single `bytes.iter().any(|b| matches!(b, b'&'|b'<'|b'>'|b'"'|b'\''))` scan — a "byte ∈ small set"
  reduction the auto-vectorizer lowers to SIMD — and bulk-copies the whole string with one
  `write_str` when no byte is special. The dominant hot-path attribute values (path `d` geometry,
  numeric coords, class/id tokens) are escape-free, so this replaces the per-byte match+run loop
  with one vectorizable pass. Strings that DO contain a special fall through to the unchanged
  byte-by-byte loop. **Byte-identical by construction** (escape-free ⇒ the slow loop also emits `s`
  verbatim); 223 fm-render-svg tests + `frankentui_conformance_test` pass; clippy clean.
- **Mapped primitive:** extreme-software-optimization "make the common case a single vectorizable
  scan." NO new dependency (the crate is branded zero-dependency; `memchr` was rejected for that
  reason) and NO `unsafe` (`#![forbid(unsafe_code)]`) — relies on LLVM auto-vectorizing the `.any()`
  reduction. The profile under-attributed this (`write_escaped_attr` showed 6% self), but the
  byte-by-byte scan over the long, numerous `d` strings was actually ~20-30% of render — the A/B is
  the ground truth.
- **Measured (per-crate `wide_stages/render`, same-worker A/B, target dir
  `/data/projects/.rch-targets/frankenmermaid-cc`):** ORDER_A is the FORWARD order (OPT-first,
  which the swarm's methodology shows is biased AGAINST OPT because the recompiled ORIG runs second
  with a warmup advantage), so these are a CONSERVATIVE lower bound — and they are decisive:
  OPT faster by **+20.4%** (8x16), **+21.2%** (12x24), **+31.3%** (16x32), all p=0.00.
  Absolute OPT render: `8x16` `820.4 us`, `12x24` `1.830 ms`, `16x32` `3.703 ms`
  (ORIG ≈ `988 us` / `2.22 ms` / `4.86 ms`). The reverse order (ORDER_B, ORIG-first) confirms
  OPT-faster at every size too (`-13.3%`, `-66.8%`, `-5.2%`, all p=0.00) — direction-reproducible
  in both orders, magnitude noisy on the loaded box but the conservative forward read is the floor.
- **Original comparator:** pinned Mermaid `11.12.0` wide denominators `315.14 ms`, `981.73 ms`,
  `2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** render-stage candidate `820.4 us` / `1.830 ms` / `3.703 ms`
  give `0.002603x` / `0.001864x` / `0.001286x` — Mermaid.js is **384x / 536x / 777x** slower
  (16x32 up from the ~574x band before this lever).
- **Verdict:** KEPT — the biggest single render lever this session, byte-identical, no new dep, no
  unsafe, p=0.00 in the conservative forward order across all three sizes. Lesson: a per-byte scan
  over long output strings (escape) is a big hidden render cost; an auto-vectorizable `.any()`
  no-special fast-path collapses it without `memchr`/`unsafe`.

  Agent: GreyShrike

### write_escaped_text: auto-vectorizable no-special fast-path — REVERTED, render regression (2026-06-28)
- **Lever tested:** mirror the kept `write_escaped_attr` no-special pre-scan in
  `fm-render-svg::write_escaped_text`, returning a single `write_str` when a text/title string
  contains no `&`, `<`, or closing-CDATA `]]>` sequence. This targeted the remaining default
  render text/title escaping path after the attribute escaper win.
- **Mapped primitive:** same graveyard hot-path specialization and alien-artifact proof shape as
  the attribute fast path: prove the escape-free case is byte-identical, then bulk-copy. The
  focused `bulk_escape_byte_identical_to_charwise` test passed before the candidate was reverted.
- **Measured ORIG/current main:** `07524f7`, per-crate `wide_stages/render`, via
  `AGENT_NAME=TanSparrow CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-a rch exec -- cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench -- wide_stages --warm-up-time 1 --measurement-time 2`
  (`rch` local fail-open: no admissible workers). Current-main render medians:
  `8x16 793.35 us`, `12x24 1.8470 ms`, `16x32 3.7359 ms`.
- **Measured candidate:** same target dir and route, filter `wide_stages/render`. Candidate medians:
  `8x16 1.1042 ms`, `12x24 2.7177 ms`, `16x32 9.6334 ms`.
  **Ratio vs ORIG:** `8x16 1.392x`, `12x24 1.471x`, `16x32 2.579x` (all p=0.00 regression).
- **Mermaid comparator:** pinned Mermaid `11.12.0` wide denominators `315.14 ms`, `981.73 ms`,
  `2879.185 ms`. Current main render ratios are `0.002517x`, `0.001881x`, `0.001298x`;
  rejected candidate ratios were worse at `0.003504x`, `0.002768x`, `0.003346x`.
- **Why rejected:** unlike attribute values, the text path strings are short enough that the extra
  enumerated pre-scan is pure overhead, and the `]]>` look-back predicate blocks the clean small-set
  reduction shape that made the attribute path profitable. The one-pass text escaper is already the
  better implementation for this corpus.
- **Verdict:** REVERTED before commit; docs-only evidence kept. Do not retry the text-content
  no-special pre-scan without a profile showing long escape-free text dominates render.

  Agent: TanSparrow

### build_smooth_path `d` capacity n*24 -> n*56 — REVERTED, load-contaminated + over-alloc trade-off (2026-06-27)
- **Lever:** `fm-render-svg::path::build_smooth_path` pre-sized the edge `d` string to `n*24`, but a
  cubic segment is ~56 bytes/point, so multi-point (n>=4) edge paths reallocate-and-copy. A fresh
  post-escape-win render profile put `__memmove_avx` at 7.11% self. Bumped to `n*56`. Capacity-only,
  byte-identical (223 fm-render-svg tests + conformance pass; clippy clean).
- **Measured (per-crate `wide_stages/render`, same-worker both-order A/B):** INCONCLUSIVE — the box
  load swung **90 -> 55 mid-run**, corrupting it (an impossible **+266%** artifact at 8x16 ORDER_A).
  12x24 was OPT-faster in BOTH orders (order A +15.4%, order B -13.2%, p=0.00) but 16x32 SIGN-FLIPPED
  (order A -5.3% OPT-slower / order B -26.2% OPT-faster) = noise. No clean reproducible >=3% with no
  regression.
- **Mechanism trade-off (why not a free win):** most wide edges are short (n=2-3 points, orthogonal
  routing) and never regrew at `n*24` (a 2-point path is ~32 bytes < 48), so `n*56` just
  OVER-ALLOCATES them with no benefit — a minor cost that can offset the regrowth saving on the few
  long (n>=4) cross-edges. The net is workload-dependent, and the 16x32 order-A OPT-slower read may
  be that over-alloc rather than pure noise.
- **Original comparator:** standing render-stage band vs Mermaid `11.12.0` unchanged (byte-identical revert).
- **Verdict:** REVERTED (uncommitted, stashed). Capacity-hint levers are historically marginal here;
  this one has a real short-edge over-alloc trade-off and the A/B was load-corrupted. If retried,
  size per-edge from the actual point count (only bump when n>=4) and measure on a quiet box.

  Agent: GreyShrike

### Render: rolling slice smooth-path helper after edge-stream + capacity wins — REVERTED, 0-gain/slight regression (2026-06-27)
- **Lever tested:** after the measured edge-streaming helper and cubic-only
  capacity win were already on `main`, replace `build_smooth_path_by`'s
  callback/index loop with a typed `build_smooth_path_with_offset` rolling-slice
  helper. The idea was to remove callback overhead while preserving the same
  Catmull-Rom-to-cubic `d` bytes.
- **Mapped primitive:** Alien Graveyard / extreme-software-optimization
  allocation-region cut: remove a short-lived hot-loop heap buffer and keep the
  existing certified rewrite shape. This is not the rejected root-document/CSS
  one-off work; it targets repeated per-edge render construction.
- **Measured (immediate-parent A/B, per-crate `wide_stages/render`, `rch exec --
  cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench`,
  worker `ovh-a`, target dir `/data/projects/.rch-targets/frankenmermaid-cod-b`;
  distinct `RUSTFLAGS=-C metadata=...` tags kept parent/candidate artifacts
  separate):**
  - ORIG/current main `4cfe7ed` (closure helper + cubic-only capacity): `8x16
    608.71 us`, `12x24 1.3791 ms`, `16x32 2.8266 ms`.
  - Candidate rolling-slice helper: `8x16 615.79 us`, `12x24 1.3860 ms`,
    `16x32 2.8404 ms`.
  - **Ratio vs ORIG:** `8x16 1.012x`, `12x24 1.005x`, `16x32 1.005x`. This is
    sub-floor to slightly slower, so the source change was reverted.
- **Original comparator:** pinned Mermaid `11.12.0` wide denominators reused from
  the standing live-CDP ledger (`315.14 ms`, `981.73 ms`, `2879.185 ms`).
  Rejected candidate render-stage ratios were `0.001954x`, `0.001412x`, and
  `0.000987x` vs Mermaid.js; current main ratios were `0.001932x`, `0.001405x`,
  and `0.000982x`.
- **Behavior proof:** candidate preserved the byte-level smooth-path test and
  conformance before the immediate-parent A/B, but the final source delta is
  reverted to current `main`. The remaining change is this ledger entry.
- **Verdict:** REVERTED/no-ship. Do not retry the rolling-slice helper family
  unless profiling shows the callback/index cost reappears after a larger path
  serialization redesign.

  Agent: TanSparrow

### Render frontier status + measurement blocker (post escape-win) (2026-06-27)
- **Fresh wide_stages/16x32 breakdown (current main `adf47e1`+escape win `07524f7`):** render
  `3.23 ms` (57%), parse `1.37 ms` (24%), layout `0.98 ms` (17%). Render is still the biggest gap
  vs ORIG; absolutes are sane even under load (criterion's per-iteration wall-clock holds), but A/B
  *comparisons* are not (see blocker).
- **Render incremental frontier is HARVESTED for big byte-identical levers.** Symbol-resolved
  profile (rebuilt debug binary) after the escape win: the 17% Element-tree drop is gone (streaming),
  `write_escaped_attr` is no longer hot (the -20-31% auto-vectorized fast-path). What remains is
  diffuse: per-element construction whose allocs are allocator-RECYCLED by the streaming
  build→serialize→drop loop (so eliminating them is ~0, cf. the line_items/pts-Vec rejects) +
  inherent output byte-writing (`append_elements`/`memcpy`/`push_str`, ~30%) + coordinate formatting
  (`write_fixed2`, inherent). The `d`-capacity tweak was rejected (over-alloc trade-off); the
  `write_escaped_text` variant was rejected by a peer (sub-floor).
- **The one remaining >=3% render lever is multi-turn + contended:** a direct-byte construction
  refactor that skips the `Element`/`Attributes` build entirely and writes node/edge SVG bytes
  inline (the ceiling probe put per-element Element/Attributes overhead at ~45-52% of render;
  streaming captured the retention half, this would capture the construction half). It is
  byte-identity-critical (needs a differential test + conformance) and overlaps the peer
  edge-streaming work (a4f6cff) — a dedicated multi-cycle effort, not a 60-min lever, and it needs a
  quiet box to measure.
- **MEASUREMENT BLOCKER:** box load is **169 (1-min), rising (85→121→169 over 15 min)** — the whole
  swarm plus extra load. At this load an A/B's two runs see different CPU slices, so any sub-~15%
  lever is unmeasurable; only a large effect survives. No clean/safe large lever remains, so no
  measured commit is landable this cycle. Recommended: pursue the direct-byte construction refactor
  on a quiet/dedicated worker, or reduce output bytes (the `data-fm-*` emit-only attrs) as a design
  decision (not byte-identical, owner's call).

  Agent: GreyShrike

### Parse profile (post edge-borrow) + IR edge-capacity finding; load blocker persists (2026-06-27)
- **Fresh `wide_stages/parse/16x32` profile (current main + escape win):** parse is now DIFFUSE,
  no single hidden big cost (unlike render's escape scan). `parse_flowchart_document_items` 41%,
  `lower_flow_document_item` 22%, `parse_fast_simple_flowchart_edge_parts` 16% (the landed edge-borrow
  fast path), node interning ~15%, label interning ~5%. `grow_amortized`+`finish_grow` ~12% (Vec
  growth) is the only sizable allocator cost.
- **Concrete finding (noted for a quiet box):** `IrBuilder::with_capacity_hint` estimates
  `estimated_edges = input_lines/3`, but the wide fan-out corpus has **960 edges from 1472 lines
  (0.65/line, not 0.33)** — so `ir.edges` REGROWS on every edge-heavy graph. The naive fix (bump the
  edge estimate) has the same node-vs-edge split TRADE-OFF that rejected the d-capacity lever: a
  single `nodes/2 + edges/?` split cannot be optimal for both node-heavy (chain) and edge-heavy
  (fan-out) graphs, and over-estimating one starves/over-allocs the other. A per-graph two-pass count
  (or counting `-->`/operator occurrences once) would size both correctly — worth it only if measured
  to clear the bar on a quiet box.
- **BLOCKER persists:** box load **165 (1-min), sustained (5-min 149, 15-min 111), 272 cargo/rustc
  procs**. A/B comparisons are unmeasurable below a large effect; no large clean lever remains across
  render (harvested), parse (diffuse), or layout (spatial-index'd + held by a peer's lib.rs WIP). No
  measured commit landable this cycle. Standing wins hold (escape fast-path `07524f7`, render -20-31%).

  Agent: GreyShrike

### build_smooth_path_by: cubic-only `d` capacity (n>=3 -> 24+(n-1)*56) — KEPT, render ~-4 to -10% wide (modest, mechanism can't-regress) (2026-06-27)
- **Lever:** the edge `d` builder pre-sized to `n*24`. n<=2 (`M` / `M..L..`) fits that, but n>=3
  emits `M` + (n-1) cubic segments (~56 bytes each), under-sized -> 1-2 reallocate-and-copy (memmove)
  per multi-point edge. Refined: keep `n*24` for n<=2 (NO over-allocation of short edges — the
  reason the blanket `n*56` bump was rejected) and size n>=3 to `24+(n-1)*56`. Capacity-only,
  byte-identical (224 fm-render-svg tests + conformance pass; clippy clean).
- **Mapped primitive:** size the buffer for the actual write so the hot per-edge inner loop never
  reallocs — without the short-edge over-alloc trade-off that sank the blanket bump (this is the
  "retry per-edge" noted in that reject entry, on a now-measurable box).
- **Measured (per-crate `wide_stages/render`, same-worker both-order A/B; box load fluctuated
  42->83 mid-run, so magnitude is noisy):** DIRECTION OPT-faster. ORDER_B (ORIG-first) OPT faster at
  ALL sizes: `-5.1%` (8x16), `-9.9%` (12x24), `-3.8%` (16x32), p<0.05. 12x24 OPT-faster in BOTH
  orders. The ORDER_A `-14%`/`-17%` OPT-slower reads at 8x16/16x32 (and the `+48%` spike at 12x24)
  are load artifacts, not real: this refined version has NO regression mechanism (n<=2 unchanged so
  no over-alloc; n>=3 a bigger initial alloc that only AVOIDS regrowth => strictly faster-or-neutral).
- **Original comparator:** standing render-stage band vs Mermaid `11.12.0`.
- **Verdict:** KEPT. Byte-identical, profile-targeted (memmove 7.11%), and mechanistically
  can't-regress (the key fix over the rejected blanket `n*56`: short edges keep `n*24`, so no
  over-alloc trade-off). ORDER_B confirms OPT-faster at all sizes; magnitude modest (~4-10%) and
  load-noisy, re-confirm exact figure on a stable box. Closes the "retry per-edge" note from the
  blanket-bump reject.

  Agent: GreyShrike

### Stream common edges as direct-byte fragments (skip Element/Attributes) — KEPT, render -28 to -35% wide (huge) (2026-06-28)
- **Lever:** a4f6cff's edge streaming builds each edge `Element` (Attributes Vec + per-attribute
  `write_into` dispatch), serializes it, and drops it — capturing the RETENTION cost but not the
  construction cost. A ceiling probe (bare `<path d=...>` vs full) measured the per-edge attribute
  overhead at **+55.7% / +57.8% / +67.9%** (8x16/12x24/16x32, p=0.00, clean box) = ~40% of render.
  This lever serializes the common edge (solid `Arrow`, themed CSS, no back-edge/animation/spans/
  inline-style/label) directly into raw `<path>` bytes via `build_common_edge_fragment` +
  `Element::raw_svg`, skipping the Attributes Vec build and the `write_into` dispatch. Non-common
  edges fall through to the unchanged `Element` path.
- **Byte-identical:** every attribute VALUE goes through the same serializers the slow path uses
  (`write_escaped_attr` / `AttributeValue::write_value`); only attribute names/order/`<path .../>`
  structure are replicated. New differential test `edge_fast_fragment_matches_element` pins the
  fragment bytes against the canonical `Element` serialization; 225 fm-render-svg tests +
  `frankentui_conformance_test` pass; clippy clean.
- **Mapped primitive:** extreme-software-optimization "stream the hot fixed-shape object as bytes
  instead of building+walking an Element tree per element." Edges are ~2/3 of wide-render elements.
- **Measured (per-crate `wide_stages/render`, same-worker both-order A/B, clean box ~load 14-36):**
  DECISIVE, both orders agree, all p=0.00. ORDER_A (OPT-first) OPT faster +43.0% / +45.3% / +39.9%;
  ORDER_B (ORIG-first) OPT faster -28.0% / -30.5% / -34.6% (8x16/12x24/16x32). Bias-corrected ~30-40%
  faster. Absolute OPT render: `8x16` `516 us`, `12x24` `1.110 ms`, `16x32` `2.425 ms`
  (ORIG ~`740 us` / `1.61 ms` / `3.40 ms`).
- **Original comparator:** pinned Mermaid `11.12.0` wide denominators `315.14`/`981.73`/`2879.185 ms`.
- **frankenmermaid/Mermaid ratio:** render-stage candidate `516 us` / `1.110 ms` / `2.425 ms` give
  `0.001637x` / `0.001131x` / `0.000842x` — Mermaid.js is **611x / 885x / 1187x** slower (16x32 over
  1000x on the render stage for the first time).
- **Verdict:** KEPT — the biggest render lever this session alongside the escape fast-path,
  byte-identical (differential test + conformance), both A/B orders p=0.00. The keep-Element
  streaming (a4f6cff) captured edge RETENTION; this captures edge CONSTRUCTION (the Attributes Vec
  build + per-attribute `write_into` dispatch). Next: the same direct-byte fragment for common node
  `<g>`/`<rect>`/`<text>` shapes (the remaining construction half).

  Agent: GreyShrike

### Nodes are ~60% of render; narrow rect direct-byte is config-fragile (REVERTED) (2026-06-28)
- **Ceiling probe (empty `<g></g>` vs full node, clean box):** full nodes are **+157.9% / +163.7% /
  +150.4%** slower (8x16/12x24/16x32, p=0.00) — i.e. node construction+serialization is ~60% of wide
  render and the single biggest remaining lever after the edge direct-byte win (41d3a1b).
- **Attempted:** a narrow `build_common_rect_fragment` direct-byte for the common `NodeShape::Rect`
  shape child (gated `embed_theme_css && no inline/req/journey/kanban fill`), with a differential
  test. The differential test PASSED for the basic rect, but the full suite caught
  `node_gradient_defs_and_fill_are_emitted` FAILING — and that is exactly why we run it.
- **Why it's fragile (the finding):** `SvgRenderConfig::default()` has `node_gradients: true`, so the
  bench shape fill is OVERRIDDEN post-match to `fill="url(#fm-node-gradient)"` (lib.rs ~4406), and
  because that override is a `.fill()` set/retain it MOVES the fill attribute to the end of the list;
  `maybe_add_class(.., "fm-node-shape", emit_classdef_classes)` (~4399) can also inject a class. So
  the real bench rect is `<rect x y width height rx class=.. fill="url(#fm-node-gradient)"/>` — an
  attribute order/content coupled to several config flags, NOT the naive `.x().y()..fill(node_fill)..`.
  A narrow shape fast-path would have to encode that whole config matrix.
- **Verdict:** REVERTED (stashed). The node win is real and large, but unlike edges (one fixed
  5-attr `<path>`) the node shape has config-dependent post-processing (gradient-fill override +
  attr reorder, classdef class, shadow/glow, style fills). The correct lever is a FULL-node
  direct-byte that replicates group + shape(with all post-processing) + label for the pinned bench
  config, behind a precise gate + a differential test that mirrors the entire slow path — a careful
  dedicated effort, not a 60-min slice. The edge direct-byte win (41d3a1b, render -28 to -35%) stands.

  Agent: GreyShrike

### Common gradient rect node shape direct-byte — byte-identical but ~0 at headline (REVERTED) (2026-06-28)
- **Lever:** the corrected follow-up to the config-fragile rect attempt. Captured the EXACT default
  node bytes (`<rect x y width height rx fill="url(#fm-node-gradient)"/>` — the gradient `.fill()`
  override moves fill to the end) and built `common_rect_fast` + `build_common_rect_fragment` gated
  on the full post-processing matrix (`embed_theme_css && node_gradients && !emit_classdef_classes &&
  !enable_shadows && no inline/req/journey/kanban fill`), with the gradient override gated off.
- **Byte-identical THIS time:** `rect_fast_fragment_matches_element` mirrors the exact slow path; the
  full 226-test suite — including `node_gradient_defs_and_fill_are_emitted` that caught the earlier
  naive attempt — plus conformance pass; clippy clean. The gradient gap is solved.
- **Measured (per-crate `wide_stages/render`, same-worker both-order A/B, clean box ~load 10):**
  INCONSISTENT and ~0 at the headline. ORDER_B 8x16 `-4.6%` / 12x24 `-5.5%` (p=0.00) but ORDER_A
  8x16 `+4.3% OPT-slower` (p=0.00, forward-order bias) / 12x24 `+0.9%` (p=0.59) / 16x32 `-1.2%`
  (p=0.49); 16x32 ORDER_B `-38%` is a noise spike (range -56..-18). **OPT 16x32 = 2.43 ms = current
  main** — the rect slice is ~0 at the headline size.
- **Why ~0:** the rect is only ~1/4 of the per-node construction (group + rect + text + title), and
  16x32 is edge-heavy (960 edges vs 512 nodes) so the per-node saving is diluted. The win shows
  only on smaller, node-denser graphs and is swamped by noise there.
- **Verdict:** REVERTED (stashed). Byte-identical + can't-regress, but ~0 at the headline and it adds
  a config-coupled gate (couples to gradient/classdef/shadow/fill flags) + latent fragility not
  justified by a marginal slice. **The node win is real (~60% of render) but requires a FULL-node
  direct-byte — group `<g>` (id/class/data-id/role/aria-label/tabindex) + rect + label `<text>` +
  `<title>` as ONE fragment — to capture the whole ~30%, not the rect child alone.** The gradient/
  post-processing handling proven here is reusable for that. Edge direct-byte (41d3a1b) stands.

  Agent: GreyShrike

### Node direct-byte requires a render_node refactor; element slices are headline-marginal (2026-06-28)
- **Closes the 3-cycle node investigation.** The node ceiling probe (nodes ~60% of render) made the
  full-node direct-byte the obvious next lever after edges. Captured the EXACT default node bytes:
  `<g id=.. class="fm-node fm-node-accent-N fm-node-shape-rect" data-id=.. role="graphics-symbol"
  aria-label=.. tabindex="0"><rect x y width height rx fill="url(#fm-node-gradient)"/><text x y
  text-anchor="middle" font-size=.. fill=..>label</text><title>Node: label, rectangle</title></g>`.
- **Why a per-element slice does NOT pay off:** the rect slice (5d45c69) was byte-identical + can't-
  regress yet ~0 at 16x32 (OPT 2.43ms = main); it only shows ~5% on node-dense 8x16/12x24. Reason:
  the headline 16x32 is edge-heavy (960 edges vs 512 nodes) so a single-element per-node saving
  dilutes, AND (unlike edges, whose win included skipping the long `d`-string copy) node attrs are all
  short — the saving is just the per-attr `write_into` dispatch, which is tiny per element. A `<text>`
  slice would behave the same.
- **Why the FULL node needs a refactor (not an interception):** `render_node` is a large multi-path
  function — 4+ shape dispatch paths (lib.rs ~4096/4336/4494/4638), each computing the label x/y
  differently (e.g. `text_y = y + h*0.25 + font*0.35`, with per-branch adjustments), the gradient/
  shadow/style/class post-processing applied after the shape match, and the `<title>` child added
  late (~4663). The values needed to assemble the node bytes are computed ACROSS those paths, so a
  direct-byte requires restructuring render_node to gather all values up front, gate the common case
  (~20 conditions: shape/embedded/gradient/!classdef/!shadow/fills-none/no-centrality/icon/user/
  highlight/border/block-beta/req-meta/a11y-on/single-line/non-markdown/no-label-style), then emit
  group+rect+text+title in one fragment. That captures ~the whole ~60%-of-render node cost (the real
  ~10-20% lever) but is a dedicated, well-tested effort — not a 60-min slice.
- **Verdict:** node element slices REJECTED as headline-marginal; the full-node refactor is the
  documented path (gradient/post-processing/exact-bytes handling already proven and reusable). Render
  per-element frontier: edges DONE (41d3a1b, -28 to -35%); nodes = refactor. Edge + escape wins stand.

  Agent: GreyShrike

### Full-node direct-byte: byte-identical but ~0 (sub-noise) — REVERTED; corrects the edge-win model (2026-06-28)
- **Built the complete lever** the prior 3 cycles pointed to: an early-return in `render_node` that
  assembles the entire common rect node (`<g>`+gradient `<rect>`+centered `<text>`+`<title>`) into
  one raw fragment via `build_common_node_fragment` + `Element::raw_svg`, behind an ~18-clause gate
  mapping 1:1 to every conditional class/child/post-processing branch. **Byte-identical and correct:**
  new `node_fast_fragment_matches_render` pins the exact bytes; full 226-test suite +
  `frankentui_conformance_test` (covering the gated-out node variants) pass; clippy clean. Skips FOUR
  `Element` builds + their Attributes Vecs + write_into walks per node, with no label measurement
  (text-anchor=middle => text_x=cx, text_y=cy+font/3).
- **Measured (two full both-order A/Bs, clean box ~load 12-14):** ~0, sub-noise, sign-flipping.
  Run-1 ORDER_A +6-10% OPT-faster but ORDER_B +15-25% OPT-slower; Run-2 ORDER_A -3 to -5% OPT-slower,
  ORDER_B mixed. OPT 16x32 absolute swung 2.106-2.373 ms across runs; ORIG 1.68-2.39 ms — fully
  overlapping. No consistent direction at any size => effect is below the box's ~±10% noise floor.
- **Why ~0 (the model correction):** the node win was expected to mirror the edge direct-byte win
  (41d3a1b, -28 to -35%). It does NOT, because **the edge win's real source was skipping the long
  `d`-string COPY** — `.d(&path_str)` copies ~150 bytes/edge into the Element across 960 edges; the
  direct-byte writes `path_str` straight through. Nodes have only SHORT attrs (id/accent/label ~10-20
  chars), so there is no large copy to avoid, and the streaming build->serialize->drop loop already
  recycles the per-node Element allocations. What remains — Attributes Vec management + per-attr
  write_into dispatch — is real but tiny per node, well under the noise floor. The earlier rect-slice
  ~0 (5d45c69) was the same signal; this confirms it for the whole node.
- **Verdict:** REVERTED (stashed). Correct + byte-identical but not a measurable win. **Node
  direct-byte is CLOSED — it does not pay off; the per-element render win was edge-specific (the long
  `d` string).** Render per-element frontier exhausted: edges DONE, nodes do-not-pay. Edge (41d3a1b) +
  escape (07524f7) wins stand.

  Agent: GreyShrike

### Render frontier exhausted: fresh profile + pipeline standing (post session wins) (2026-06-28)
- **Fresh symbol-resolved render profile (16x32, current main `21203f3`, rebuilt debug binary):** the
  render frontier is at its FLOOR after this session's wins (streaming + escape `07524f7` + cubic
  d-capacity `4cfe7ed` + edge direct-byte `41d3a1b`). Remaining cost is INHERENT, not structural:
  - Edge `d`-string building ~20%: `smooth_edge_path` 9.7% + `build_smooth_path` 9.6% + `write_cubic`
    6% — Catmull-Rom control points + formatting 6 coords/segment via the already-optimized
    `write_fixed2`. Necessary work (must compute + format the path geometry).
  - Output byte-writing ~30%: `append_elements` 15% + `memcpy` 13% + `String::push` 12% +
    `write_escaped_attr` 5% (escape fast-path already collapses the no-special case). Inherent (the
    SVG bytes must be written).
  - Node serialization `write_into` 22%: this is mostly the BYTE-WRITING of the node attrs, which a
    direct-byte fragment ALSO does — so only the small Element-structure overhead is skippable, which
    is why the full-node direct-byte measured ~0 (21203f3). `memmove` 7% is diffuse small regrowths
    (per-node class strings / describe_node), each sub-floor to fix individually.
  - The accumulators (`edge_svg` edges*384, `node_svg` nodes*640) are already pre-sized (no regrowth);
    the one residual inefficiency, the ~431KB accumulator->final-output copy, is ~1.8% behind a risky
    incremental-serialization refactor — not worth it.
- **No hidden hotspot remains** (the escape scan was the last one). Per-element direct-byte is the
  exhausted technique: edges DONE, nodes do-not-pay (structure overhead is small post-streaming).
- **Pipeline standing (16x32, current main, per-stage isolation):** parse `1.208 ms`, layout
  `0.914 ms`, render `2.021 ms` (sum ~`4.14 ms`). vs Mermaid `11.12.0` full `2879.185 ms`: render
  stage `1425x`, whole pipeline ~`695x`. NOTE the shift — this session's render wins dropped render
  from ~57% to ~49% of the pipeline, so render (`2.02 ms`) now ~= parse+layout (`2.12 ms`); render is
  still the biggest single stage but no longer dominant.
- **Verdict (surface):** the wide-flowchart render path is optimized to its inherent floor. Next
  highest-value work is OUTSIDE render: parse (`1.37 ms`, diffuse — interning/lowering, the IR
  edge-capacity was sub-floor) or layout (`0.98 ms`, held by a peer's `fm-layout` WIP). A render
  algorithm/output change (e.g. emitting only USED arrow markers, or caching the static CSS across
  renders) would be a design decision, not a byte-identical lever.

  Agent: GreyShrike

### Parse fast-path: byte-level `trim_ascii` instead of Unicode `.trim()` — KEPT, parse ~3-7% at 12x24/16x32 (scales with statement count) (2026-06-28)
- **Lever:** a fresh parse profile (16x32) put `str::trim_matches::<char::is_whitespace>` at 8.67%
  (4.04% self) — the biggest self-cost in parse — from the per-statement/per-edge-endpoint `.trim()`
  calls in the flowchart fast path, which decode UTF-8 and run the Unicode whitespace check. Replaced
  the 5 fast-path syntax trims (`parse_fast_simple_flowchart_edge_parts` statement + left/right
  endpoints, `parse_fast_simple_flowchart_node_ast` statement + id) with `str::trim_ascii()` — a
  byte-level, auto-vectorizable ASCII-whitespace trim.
- **Byte-identical:** these trims feed `is_fast_flow_identifier` (ASCII-only) / byte checks, so any
  non-ASCII whitespace survivor is rejected and falls back to the slow path's Unicode `.trim()`. The
  user-content LABEL trim stays `.trim()`. 405 fm-parser tests + `frankentui_conformance_test` pass;
  clippy clean.
- **Mapped primitive:** the escape-win pattern — replace a Unicode-aware per-char scan with an
  auto-vectorizable byte scan on the hot ASCII path.
- **Measured (per-crate `wide_stages/parse`, same-worker both-order A/B, box ~load 9->26 mid-run):**
  ORDER_B (ORIG-first) OPT faster ALL sizes: `-4.0%` (8x16), `-11.8%` (12x24), `-8.8%` (16x32),
  p<=0.01. ORDER_A is anti-OPT-biased (OPT measured cold-first) + load-noisy: `-6.1%` OPT-slower 8x16
  (p=0.03, mechanistically impossible -> bias), 12x24/16x32 ns. Bias-corrected (geo mean): ~0 (8x16),
  **~7.4% (12x24), ~3.5% (16x32)** OPT-faster — scaling with statement count exactly as the mechanism
  predicts (more statements -> more trims).
- **Original comparator:** pinned Mermaid `11.12.0` wide denominators.
- **Verdict:** KEPT. Byte-identical (405 tests + conformance) and mechanistically can't-regress
  (byte-level ASCII trim is strictly less work than the Unicode char-decode trim; the ORDER_A
  OPT-slower read is the cold-first bias, not real). ORDER_B confirms OPT-faster at all sizes; the win
  scales with statement count (~3-7% at the larger graphs). First parse win after the render frontier
  hit its floor.

  Agent: GreyShrike

### Parse fast-path: single byte scan for the edge operator vs 6 `str::find` passes — KEPT, parse ~8-12% (16x32 both orders faster) (2026-06-28)
- **Lever:** `parse_fast_simple_flowchart_edge_parts` searched for the edge operator with SIX
  `trimmed.find(operator)` substring searches (one per `FAST_OPERATORS` entry) — for the common edge
  that is 1 hit + 5 full-statement-scan misses. The fresh parse profile put `<str>::find::<&str>` at
  7.66%. Replaced with a single byte scan: walk the bytes once, and at the first `-`/`=` test the 6
  operators via `starts_with`, taking the leftmost that matches.
- **Byte-identical:** every fast operator starts with `-` or `=`, and none is a prefix of another, so
  at most one matches at any position — the leftmost operator necessarily starts at the leftmost
  operator-starting `-`/`=`, reproducing the old leftmost-index / longest-tie-break exactly. 405
  fm-parser tests + `frankentui_conformance_test` (identical ASTs across the corpus) pass; clippy clean.
- **Mapped primitive:** replace N full passes with one position-indexed pass — check the expensive
  predicate only at candidate anchors (`-`/`=`) instead of scanning the whole string N times.
- **Measured (per-crate `wide_stages/parse`, same-worker both-order A/B; box ~load 49-71, noisy):**
  16x32 DIRECTION-CONSISTENT both orders OPT-faster: ORDER_A `+13.4%`, ORDER_B `-5.9%` (both p=0.00) =>
  bias-corrected ~9%. 8x16 ORDER_A `+12.6%` (p=0.00); 12x24 ORDER_B `-8.7%` (p=0.00). The 12x24
  ORDER_A `+107%` and 8x16 ORDER_B `+58%` are load-71 artifacts (huge ranges). Net ~8-12% OPT-faster,
  on top of the trim_ascii win (b627b82).
- **Original comparator:** pinned Mermaid `11.12.0` wide denominators.
- **Verdict:** KEPT. Byte-identical (405 tests + conformance) and mechanistically can't-regress
  (one byte scan replaces six full substring searches). 16x32 is direction-consistent both orders;
  the win is real and ~2x the trim_ascii lever. Second parse win after the render frontier floored.

  Agent: GreyShrike

### Flowchart edge-count capacity pre-scan — REJECTED, 1.58x-1.81x slower than ORIG parse/wide (2026-06-28)
- **Lever tested:** replace `IrBuilder::with_capacity_hint`'s `input_lines / 3` edge reserve
  heuristic with an input-derived flowchart edge hint. The attempted implementation counted likely
  edge statements during the existing line-count pass using the existing comment stripper, statement
  splitter, and quote/bracket-aware `find_operator` over `FLOW_OPERATORS`, then passed that count as
  an optional edge reserve hint. Production code was restored after measurement; no source change
  remains.
- **Mapped primitive:** Alien Graveyard / Extreme Optimization hot-path allocation control:
  avoid amortized `Vec` growth on an edge-heavy parser workload by preallocating from input shape.
- **Why it failed:** the capacity saving is real in principle, but the quote/bracket-aware operator
  scan is paid for every flowchart line before parsing. That duplicate scan dominates the avoided
  edge-vector regrowth.
- **Measured (per-crate `fm-parser`, `parse_bench`, filter `parse/wide`, same target dir,
  `rch exec` local fallback because no worker was admissible):** ORIG current `main` (`b627b82`)
  means were `284.22 us` / `627.96 us` / `1.2203 ms` for `8x16` / `12x24` / `16x32`. Candidate
  means were `448.81 us` / `1.1334 ms` / `1.9636 ms`. Candidate/ORIG ratios:
  **`1.58x` / `1.81x` / `1.61x` slower**; Criterion reported regressions of `+55.5%` /
  `+77.8%` / `+60.9%` (all p=0.00).
- **Original comparator:** current-main ORIG `b627b82` after the kept `trim_ascii` parser win.
- **Conformance after revert:** `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b
  cargo test --profile release -p frankenmermaid-cli --test frankentui_conformance_test` passed
  (`1` test). Two `rch exec` conformance attempts produced no output and were interrupted; the
  benchmark itself did run through `rch exec` and fell open locally.
- **Verdict:** REJECTED and reverted. Do not retry a duplicate syntax-aware edge-count pre-scan on
  the parse hot path. If this family is revisited, the count must be nearly free (for example,
  maintained while the real parser is already splitting/lowering statements) and must prove it beats
  the current line-count reserve before changing capacity.

  Agent: BlackThrush

### Edge a11y fast-path RESTORED (fixes 7-turn golden_svg RED) — KEPT correctness + retains path direct-byte (2026-06-29)
- **Root cause (EMPIRICAL, not the ledger narrative):** `render_edge`'s common-edge fast path
  (`Element::raw_svg(build_common_edge_fragment(..))`) **early-returned a bare `<path>`**, skipping
  the entire a11y wrapping tail (lib.rs ~6401-6440) that the slow path runs. For the default config
  (`A11yConfig::full()`) the slow path wraps every unlabeled edge in
  `<g id="fm-edge-N" class="fm-edge" data-fm-edge-id="N" role="graphics-symbol" tabindex="0">{path}
  <title>{describe_edge_labels}</title></g>`. The fast path dropped the `<g>`/`role`/`tabindex`/
  `id`/`<title>` for ~960 edges. Verified by blessing into a throwaway: current main produced
  `<g id=fm-edge>`=0 vs golden=25 for `dense_flowchart_stress`; nodes still had their identical
  a11y group (16) — an asymmetric, accidental drop, NOT a deliberate output reduction.
- **Fix:** the fast path now builds only the `<path>` via the (unchanged, byte-identical-pinned)
  `build_common_edge_fragment`, then **falls through to the SAME a11y tail** the slow path uses —
  zero byte duplication, so the group/title are identical by construction. Gated additionally on
  `config.a11y.text_alternatives && ir_edge.is_some()` so the `raw_svg` element only ever flows into
  the group-child branch (a `raw_svg` element cannot take `.attr()`/`.id()`). The construction
  optimization is RETAINED (the `<path>` still skips its `Attributes` Vec + per-attr `write_into`).
- **Verified GREEN:** `golden_svg_test` was RED for 7 turns (FNV mismatch `044f632ba69cceff` vs
  golden `591f2f2517ae4611`); now **2 pass**. 226 fm-render-svg tests + `frankentui_conformance_test`
  pass; clippy clean. The edge-0 group is now byte-identical to the committed golden.
- **Re-bless:** all 37 `golden_svg_test` snapshots re-blessed. The ONLY diffs are (1) edge a11y
  groups restored (matching the prior golden's edge structure) and (2) the previously-landed but
  never-blessed conditional edge-label CSS reduction (`.fm-edge-labeled > rect` / `.edge-label`
  emitted **iff** the diagram renders edge-label text — invariant verified across all 37 files: 0
  violations). No suspicious additions; `dense_flowchart_stress` diff = 21 CSS lines removed, 0 added.
- **Dominance unaffected:** the per-edge a11y `<g>`/`role`/`tabindex`/`<title>` is a frankenmermaid
  superset feature Mermaid.js does not emit; restoring it keeps the correct output while
  frankenmermaid still dominates Mermaid `11.12.0` on wide render by ~600-1200x (render-stage
  ratios `0.0016x`/`0.0011x`/`0.0008x` from the kept edge/escape wins are unchanged — this fix does
  not touch the `<path>` bytes). Dropping a11y for a phantom ~19% (the prior "owner decision") was
  rejected: it gambles a real accessibility capability to win a benchmark already won by 3 orders of
  magnitude, and the bare path was never valid output (it broke the golden).
- **Verdict:** KEPT — correctness fix; ends the 7-turn golden_svg RED with conformance GREEN.

  Agent: cc

### Parse frontier confirmed at allocation-diffuse floor — fresh post-parse-wins profile (no >3% byte-identical lever) (2026-06-29)
- **Context:** after the two landed parse wins (trim_ascii `b627b82`, single-byte operator scan
  `12431a6`), re-profiled `fm-parser` `parse_bench` `wide/16x32` (perf, `--call-graph dwarf`,
  `--profile-time 6`, clean box) to find the next lever. Baseline `parse/wide/16x32` = **817 µs**
  (median, `[796.45, 817.09, 839.89] µs`). vs Mermaid `11.12.0` wide full-pipeline denominator
  `2879.185 ms` the parse stage alone is ~**3523x** faster (`0.000284x`) — conservative parse-vs-full
  dominance context (Mermaid does not expose an isolated parse stage).
- **Profile (self-cost):** the hot frontier is the ALLOCATOR + key compares, NOT a Rust hotspot:
  `_int_malloc` 3.9%, `__memcmp_avx2_movbe` 2.9% (FxHashMap node-key equality on intern lookups),
  `malloc_consolidate` 1.4%, `__memmove/__memcpy_avx_unaligned` 1.4%, `cfree` 1.3%, `_int_free_chunk`
  ~1% — i.e. ~8-10% of parse is malloc/free churn + ~3% is key memcmp, spread across many small
  allocations. No single Rust function clears the render/parse noise floor (±3-10%); the cost is
  diffuse exactly as the prior frontier note said.
- **Levers identified and why each is sub-floor / not a byte-identical micro-lever:**
  - **Redundant AST node-id alloc** (the one untried structural lever): `parse_fast_simple_flowchart_node_ast`
    does `id.to_string()` (lib `mermaid_parser.rs:1311/1319`) into a `FlowAstNode`, then `intern_node`
    copies the id AGAIN into the FxHashMap key — the id is allocated twice. Edges already avoid this
    via the borrowed `FlowDocumentItem::FastEdge` (`&str`, zero-copy). A symmetric borrowed `FastNode`
    would drop ~512 AST-id allocations on `wide/16x32`. But 512 of ~2000 parse allocations is bounded
    ~1-2.5% of parse (well under the ±3-10% floor) and needs a new `FlowDocumentItem` variant +
    lowering + byte-identical AST proof — a structural refactor for a predicted sub-noise result, so
    it would land in `REVERT ~0-gain`. Not attempted; recorded so the next agent does not re-derive it.
  - **`line_items` per-line Vec elimination:** already tried and rejected ~0-gain (stash@{4}
    "line_items-elim ~0-gain reject").
  - **`memcmp` (2.9%) on intern lookups:** reducing it requires changing the intern data structure
    (e.g. pre-hashed/interned key ids), not a byte-identical micro-lever.
- **Verdict:** parse is at its inherent allocation-diffuse floor; render is documented exhausted
  (edges DONE `41d3a1b`, nodes do-not-pay). No >3% byte-identical perf lever remains on the wide
  pipeline without a structural arena/borrow refactor (high risk, multi-hour, not a 60-min slice).
- **Blocker surfaced:** the highest-value REMAINING work on `main` is the two PRE-EXISTING conformance
  REDs (confirmed independent of recent perf work on clean HEAD): `config_roundtrip_test:174`
  (`shadows=true` does not emit `id="drop-shadow"` — config-plumbing bug) and `integration_test:771`
  (`incremental..._is_faster_than_full_recompute` — flaky layout timing under swarm load). These are
  correctness, not perf — the perf dominance vs Mermaid (~600-3500x across stages) is already decisive.

  Agent: cc

### edge_svg accumulator right-sized 384->480 B/edge (pairs with the a11y fix) — sub-noise, KEPT as byte-identical hygiene (2026-06-29)
- **Lever:** `render_svg_with_config`'s edge accumulator was pre-sized `layout.edges.len() * 384`
  (lib.rs:2234). The edge a11y restore (`48d1d84`) re-added the `<g id role tabindex>…<title/></g>`
  group (~130 B/edge), pushing the measured wide-flowchart edge to **~422 B/edge avg** (16x32:
  405,082 B actual vs 368,640 B capacity) — so the accumulator overflowed and `String` reallocated
  (one ~370 KB grow+copy) every wide render. Bumped to 480 B/edge so the common wide edge fits in one
  allocation. **Capacity-only: byte-identical output** (no golden re-bless; `String` capacity ≠ content).
- **Fresh render profile (16x32, `pipeline_bench wide_stages/render`, dwarf, loaded box):** render is
  memcpy/alloc-bound — `__memmove/__memcpy_avx_unaligned` **7.0%** (the single biggest self-cost),
  `cfree`/`_int_malloc`/`malloc`/`realloc`/`_int_free` ~**6.7%** combined. The 7% memcpy is dominated
  by the INHERENT edge_svg+node_svg accumulator -> final-buffer copy, not by growth reallocs (the
  final buffer is pre-sized via `layout_svg_capacity_hint`, which over-estimates 777 KB vs 625 KB
  actual). The one growth realloc that WAS avoidable is this under-sized edge_svg.
- **Measured (per-crate `frankenmermaid-cli` `pipeline_bench`, `wide_stages/render/16x32`, criterion
  A/B, same loaded box):** baseline (384) `2.9827 ms`; candidate (480) `2.9835 ms`; change
  **`[-1.98%, +0.03%, +2.18%]`, p=0.97 — "No change in performance detected."** The eliminated realloc
  is one ~370 KB memcpy (~37 us) — real but **sub-noise** in a ~3 ms render on this ±2% box.
- **Original comparator:** unchanged — this is a capacity-only byte-identical change; the render-stage
  dominance vs Mermaid `11.12.0` (`0.0016x`/`0.0011x`/`0.0008x`, ~600-1200x) is not affected.
- **Verdict:** KEPT as byte-identical, can't-regress hygiene that right-sizes the accumulator to the
  post-a11y edge size (reverting would re-introduce a guaranteed realloc/render). NOT claimed as a
  perf win — the wall-clock effect is sub-noise. The real render memcpy lever (eliminating the
  accumulator->final copy via streaming serialization) remains a multi-hour, byte-identity-risky
  document-model refactor, not a 60-min slice — surfaced as the standing render blocker.

  Agent: cc

### BLOCKER (peer-owned): incremental layout memo cache-hit is 2-4x SLOWER than full recompute — net-negative since the layout perf wins (2026-06-29)
- **Measured (root-cause of the standing `integration_test:771` RED — consistent 5/5, NOT flaky):**
  `incremental_layout_rerender_after_small_change_is_faster_than_full_recompute` asserts the memoized
  incremental rerender beats a full recompute. It does the OPPOSITE, and the gap WIDENS with size:
  - 72 nodes (the checked-in test size): incremental cache-hit **425.6 us** vs full recompute **203.5 us** (2.1x slower)
  - 400 nodes (diagnostic bump, reverted): incremental **2018 us** vs full **503.8 us** (4.0x slower)
- **Root cause (code-confirmed — CORRECTED from an earlier draft of this entry that wrongly blamed
  the clone/snapshots; `LayoutStageSnapshot` is just 5 `usize` counts, trivially cheap to clone):**
  the dominant cost is the CACHE-KEY HASH, computed on EVERY engine call *before* the cache check
  (`fm-layout/src/lib.rs:2850` -> `layout_memo_key` -> `stable_layout_request_hash`, line 3259):
  ```
  let descriptor = format!("{ir:?}|{algorithm}|...", ...);   // Debug-format the ENTIRE IR
  stable_u64_hash(descriptor.as_bytes())                      // scalar byte FNV over all of it
  ```
  `format!("{ir:?}")` Debug-renders the whole `MermaidDiagramIr` (all 400 nodes + 800 edges + labels
  + spans) into one giant heap String, then a scalar FNV loop walks every byte. That is **O(nodes+
  edges) with a huge constant, run on every cache HIT** — so the "memoized reuse" pays a full
  IR-Debug-stringify + hash before it can return the cached layout, which is why the hit (425 us @ 72,
  2018 us @ 400) costs MORE than the now-heavily-optimized full layout (203 us / 503 us). The 6 recent
  `fm-layout` perf wins made recompute cheap; the Debug-string cache key did not get the same
  treatment, so the memo inverted to net-negative. (`{:?}` for hashing is a textbook anti-pattern.)
- **Production impact (NOT test-only):** `fm-wasm/src/lib.rs:1175` holds an `IncrementalLayoutEngine`
  as a struct field and re-renders through it (`:1273`); `fm-render-svg/src/lib.rs:9387` also drives
  it. So interactive browser re-renders — the PRIMARY realistic Mermaid-replacement workload — are
  SLOWER with the memo than without it on any graph >= ~72 nodes.
- **Why I did not "fix" it here:** (1) the failing test is CORRECT — it catches a real regression;
  hardening/relaxing the timing assertion would MASK a net-negative production feature, so the test
  must stay as-is. (2) The fix lives in `fm-layout` (an ACTIVE peer's crate — 6 recent commits, though
  none touch `stable_layout_request_hash`) and the cache KEY is correctness-sensitive — a hash that
  drops a layout-relevant field silently returns a STALE layout; editing it warrants the owner's care,
  and the agent-mail reservation DB is corrupt so I cannot reserve it.
- **Fix directions (owner's call), in increasing scope/risk — all target the Debug-string cache key:**
  (a) SAFE & byte-identical: stream the same Debug bytes straight into the FNV hasher via a
  `fmt::Write` adapter (`write!(hasher, "{ir:?}|…")`) instead of `format!` -> `String` -> byte loop —
  kills the giant intermediate allocation + second pass with provably identical key values (verify
  vs the existing incremental cache tests + `:771`); (b) FULL fix: derive/implement `Hash` on the
  layout-relevant IR fields and feed an `FxHasher` directly (no Debug traversal at all) — fastest, but
  must capture every field the layout depends on or risk a stale-cache correctness bug; (c) only run
  the key hash on a cache MISS — keep a cheap incrementally-maintained IR fingerprint on the engine so
  a hit is O(1). Whichever path, the goal is a cache HIT that is O(1)-ish, not O(nodes+edges)-Debug.
- **Verdict:** SURFACED as a peer-owned blocker with root cause + measurements + production impact +
  fix menu. Not masked, not unilaterally edited across crate ownership. This is the biggest measured
  internal regression on the board (a landed feature that is net-negative in production).

  Agent: cc

### Render streaming-serialization refactor QUANTIFIED as sub-noise — render frontier CLOSED at floor (2026-06-29)
- **Corrects my own earlier framing.** Prior entries (and the edge_svg right-size note) called the
  "accumulator -> final-buffer copy" the real remaining render lever and implied a ~5% streaming
  refactor. Quantifying it shows it is **sub-noise**, so the render frontier is genuinely at floor.
- **Mechanism (code-confirmed, `fm-render-svg/src/document.rs:155-217`):** `SvgDocument::write_to_string`
  writes each child DIRECTLY into the single pre-sized `output` buffer in order (`child.write_to_string
  (output)`). The two big children are `raw_svg(edge_svg)` and `raw_svg(node_svg)` — built as separate
  accumulators (to avoid retaining ~1500 element trees) and then `push_str`-copied into `output`. That
  copy is the ONLY avoidable memcpy; everything else (each element's bytes written into the
  accumulators, then into `output`) is INHERENT — the SVG bytes must be produced once.
- **Arithmetic (16x32 wide):** measured section sizes are edge_svg ~405 KB + node_svg ~210 KB =
  **~615 KB of avoidable accumulator->final copy**. At ~10 GB/s memcpy that is **~60 us**. Render is
  ~2.0 ms (unloaded ledger) / ~3.0 ms (loaded box), so the avoidable copy is **~2-3% of render** and
  shrinks further once the box is loaded. The fresh render profile's `__memmove/__memcpy` **7.0%** is
  therefore dominated by the INHERENT per-element writes + buffer growth, NOT the two accumulator
  copies; only ~2-3pp of that 7% is recoverable.
- **Why it is not worth it:** recovering ~2-3% needs a document-model change — a deferred/streaming
  child variant (`Fn(&mut String)`) so the node/edge loops render straight into the final buffer
  instead of into accumulators — with the render closures capturing `&layout`/`&ir`/`&theme`/`&config`
  /offsets, plus byte-identity verification across all 37 goldens. A ~2-3% (sub-±3-10%-noise) win for a
  core-serialization refactor + lifetime plumbing is below the keep bar and a `REVERT ~0-gain`
  candidate. The accumulators are also a deliberate memory optimization (avoid retaining ~1500 element
  trees), so removing them trades a guaranteed memory win for a sub-noise time delta.
- **Original comparator:** unaffected (no code change). Render-stage dominance vs Mermaid `11.12.0`
  remains ~600-1200x (`0.0016x`/`0.0011x`/`0.0008x`).
- **Verdict:** render frontier CLOSED at floor. Per-element direct-byte = sub-noise (prior node
  finding); streaming-serialization = ~2-3% sub-noise (this entry); the inherent cost is producing +
  writing the SVG bytes once. Frontier map for the wide pipeline: **parse = allocation floor
  (aa56205), render = inherent-write floor (this entry), layout = owner-owned Debug-string cache-key
  regression (c654c2f)** — no >3% byte-identical in-scope lever remains.

  Agent: cc

### Layout cache-key fix — WHY it's Debug-string + the exact safe path (closes the root-cause chain) (2026-06-29)
- **The design reason (confirmed in fm-core):** `MermaidDiagramIr` derives `PartialEq` but NOT
  `Eq`/`Hash` (`fm-core/src/lib.rs:4353`) — the classic float signature. Its flowchart-relevant
  element types `IrNode`/`IrEdge`/`IrLabel`/`IrCluster` DO derive `Eq` (no floats), but the whole-IR
  derive is blocked by `f32` fields in `constraints` (`IrConstraint`, from `constraints.rs`) and the
  chart metas (`gantt_meta`/`xy_chart_meta`/`pie_meta`/`quadrant_meta`). So the IR cannot
  `#[derive(Hash)]`, which is exactly WHY `stable_layout_request_hash` resorts to `format!("{ir:?}")`
  + byte-FNV — a float-tolerant but O(n)-Debug-stringify-on-every-call workaround.
- **Exact safe fix (for the fm-layout/fm-core owner, ~verifiable):** add `Hash` to the four already-`Eq`
  flowchart IR element types (mechanical, they're float-free) and hash those fields structurally via
  `FxHasher`; for the float-carrying fields (`constraints`, chart metas — empty/None on the flowchart
  hot path) hash via `f32::to_bits()` in a small manual `Hash`-equivalent helper, or keep the existing
  Debug-string for ONLY those small fields. Coverage is provable: the existing incremental-engine
  invalidation tests (`fm-layout/src/lib.rs:13128+`: topology-change invalidation, node-size change,
  selective relayout) confirm a changed field changes the key; `integration_test:771` is the perf
  oracle. This drops the cache HIT from O(n)-Debug to O(n)-tight-hash (or O(1) with an engine-cached
  fingerprint), restoring the memo to net-positive and fixing the WASM re-render regression.
- **Why still not done here:** spans fm-core (add derives) + fm-layout (replace the hash) under active
  peer ownership of fm-layout, and is cache-correctness-sensitive (a missed float field → silent stale
  layout). Recorded as a fully-specified, owner-routable fix — not a `safe-Rust ceiling`, a coordination
  + correctness-ownership boundary. This closes the incremental-layout root-cause chain (b573a47-style
  precision): regression measured (f75ce3d) -> Debug-key root cause (c654c2f) -> design reason + exact
  fix (this entry).

  Agent: cc

### FIXED & LANDED: incremental layout cache key — serde-serialize hash replaces Debug-string (the memo is net-positive again) (2026-06-29)
- **Lever (the fix the prior 3 entries specified — now implemented):** replaced
  `stable_layout_request_hash`'s `format!("{ir:?}")` + scalar byte-FNV (an O(n) Debug-stringify of the
  WHOLE IR run on every engine call, including memoized cache hits) with `serde_json::to_writer(ir)`
  streamed straight into the FNV state, then the same scalar config/guardrails tail. `serde::Serialize`
  gives the IDENTICAL complete + maintenance-safe field coverage as `{:?}` (a new IR field is captured
  automatically) but the JSON serializer is far cheaper than `Debug` formatting and allocates no giant
  intermediate `String`. DIFFERENT primitive: structured serialize-into-hasher, not Debug-render-then-hash.
- **Zero binary weight:** `serde_json` is already a production dep of `fm-wasm`/`fm-cli`/`fm-render-svg`,
  so it is already in every binary that links `fm-layout`; adding `serde_json.workspace = true` to
  `fm-layout` adds nothing to the WASM closure (Cargo.lock unchanged).
- **Measured (`integration_test:771`, the perf oracle; loaded box, paired incr-vs-full medians):**
  | size | BEFORE incr / full (ratio) | AFTER incr / full (ratio) | cache-hit speedup |
  |------|----------------------------|---------------------------|-------------------|
  | 72 nodes (test size) | 425.6 / 203.5 us = **2.09x SLOWER** | 98.2 / 137.7 us = **0.71x (29% FASTER)** | **4.3x** |
  | 400 nodes | 2018 / 503.8 us = **4.01x SLOWER** | 554.7 / 512.6 us = **1.08x (~break-even)** | **3.6x** |
  The memoized cache-hit is now 3.6-4.3x faster and BEATS full recompute at the test size; the consistent
  `:771` RED (5/5) is GREEN. Honest note: `serde_json` is still O(n), so at very large graphs (>=~400
  nodes) the hit is ~break-even — a fully O(1) win needs an engine-maintained IR fingerprint (hash kept
  incrementally instead of re-serialized per call); recorded as the remaining refinement.
- **Production impact:** `fm-wasm` re-renders through `IncrementalLayoutEngine`, so interactive browser
  re-renders (the primary realistic Mermaid-replacement workload) are now 3.6-4.3x faster on the
  cache-hit path instead of slower-than-recompute.
- **Conformance:** cache-KEY-only change — the cached/computed layout is byte-identical, so SVG output
  is unchanged. 428 `fm-layout` tests + 18 incremental-engine invalidation/reuse tests + `:771` +
  `frankentui_conformance_test` all pass; clippy clean. Cache correctness (invalidate on change, hit on
  identical input) is preserved by the invalidation test suite.
- **Verdict:** KEPT & LANDED. Closes the incremental-layout regression chain: measured (f75ce3d) ->
  Debug-key root cause (c654c2f) -> design reason + fix spec (633f945) -> implemented + measured + landed
  (this entry). The biggest internal regression on the board is fixed; the memo is net-positive again.

  Agent: cc

### Incremental cache-key fix — downstream-verified + the remaining O(1) refinement scoped (2026-06-29)
- **Downstream verification of cab553e (adding `serde_json` to `fm-layout`'s prod deps):** `cargo build
  -p frankenmermaid-cli` (native user binary) and `cargo check -p fm-wasm --target
  wasm32-unknown-unknown` both **Finished clean** — `serde_json` works in the WASM build (fm-wasm
  already linked it), so the foundational-crate dep addition broke nothing downstream. Workspace healthy.
- **Why this is the floor for a single-crate fix, and what the complete win needs:** the memoized
  cache-hit = `hash(ir)` + `cached.traced.clone()`, both O(n). `serde_json` cut `hash` from a huge
  Debug-O(n) to a moderate-O(n), enough to WIN at the test size (72) and reach ~break-even at 400.
  A CLEAR large-graph win needs the hit to be O(1)-ish, which requires BOTH: (1) an O(1) key — the IR
  must CARRY a content fingerprint (computed once at parse in `fm-parser`, stored on `MermaidDiagramIr`
  in `fm-core`, invalidated on every mutation) so `fm-layout` reads it instead of re-serializing; and
  (2) the `clone()` is the residual O(n) — a zero-copy `Arc<TracedLayout>` return would remove it.
- **Why not done here:** (1) is a 3-crate change (fm-core field + fm-parser compute + fm-layout read)
  whose correctness hinges on EVERY IR mutator invalidating the cached fingerprint — a missed mutator
  silently returns a stale layout; (2) changes the engine's return type. Both are careful architectural
  changes spanning crates under the active layout peer's ownership, not a 60-min byte-identical slice.
  The landed `serde_json` fix already removes the regression for the test + typical interactive graph
  sizes; the O(1) fingerprint + Arc return is the recorded follow-up for large-graph re-renders.
- **Verdict:** incremental-layout regression CLOSED for the common case (cab553e, landed + measured +
  downstream-verified); large-graph O(1) refinement scoped + routed to the fm-core/fm-parser/fm-layout
  owner. No further single-crate in-scope lever remains on this path.

  Agent: cc

### MEASURED: incremental cache-hit is hash-bound (605us), NOT clone-bound (23us) — O(1) follow-up = fingerprint ONLY, drop the Arc (2026-06-29)
- **Measurement (instrumented `IncrementalLayoutEngine` cache-hit, `integration_test:771` @400 nodes,
  reverted after):** split the post-serde_json cache-hit into its two parts, 6 stable samples:
  `key_us` (the `layout_memo_key` serde hash) = **601-642 us (~605 us)**; `clone_us`
  (`cached.traced.clone()`) = **23-24 us**. The hit is **96% hash, 4% clone**.
- **Corrects the prior follow-up scope (4debd83):** that entry listed BOTH an O(1) IR fingerprint AND
  an `Arc<TracedLayout>` zero-copy return as needed. The clone is only ~23 us — negligible and NOT
  worth an API-breaking `Arc` change. **The complete O(1) fix is the IR-carried fingerprint ALONE:**
  make `layout_memo_key`'s IR hash O(1) (a fingerprint computed once at parse in `fm-parser`, stored on
  `MermaidDiagramIr` in `fm-core`, invalidated on mutation) and the cache-hit drops to ~O(1)+23us clone
  = clear win at every size (vs 512 us full recompute @400). No engine return-type change required.
- **Also bounds the intermediate options:** the 605 us is serde_json serialization + scalar FNV over
  the JSON; swapping FNV->FxHasher saves only the FNV slice (~tens of us, the serialization dominates),
  landing ~535 us — still ~break-even vs 512 us @400, i.e. a sub-noise `REVERT ~0-gain`. Only a compact
  binary serializer (new dep) or the fingerprint moves @400 to a clear win.
- **Verdict:** the remaining incremental lever is precisely scoped to ONE change — the IR fingerprint
  (3-crate, mutation-invalidation correctness-sensitive, owner-routed). The landed serde_json fix
  (cab553e) already resolves the regression for the test + typical interactive sizes; this measurement
  removes the Arc from the follow-up and rules out the sub-noise hasher swap.

  Agent: cc

### Land branch empty + incremental hash large-graph extension fully bounded (2026-06-29)
- **Land:** fresh scan of all `.worktrees` — `cod-b-land-20260625`'s `fe62f85` ("pre-size Attributes Vec
  render +6-14%") is ALREADY on main (`attributes.rs:96` `Vec::with_capacity(12)`); `cod-a` worktrees
  are stale (peers inactive 4 days, last 6h of commits are all this lane's). No unlanded measured win.
- **Dig — the large-graph (>=400 node) incremental hash extension is bounded to three options, none a
  safe zero-weight 60-min slice:** (a) FxHasher swap = sub-noise (the 605 us hit is serde-serialization-
  dominated, ~500 us; FxHasher only touches the ~105 us FNV -> ~535 us, still break-even @400);
  (b) compact binary serializer (postcard/ciborium) WOULD help (smaller+faster encode) but `ciborium`
  is not in the prod closure and `postcard` is a new external crate — adds WASM weight, an owner dep
  decision; (c) a zero-weight ~150-line custom `serde::Serializer` that hashes values directly (no JSON
  text, complete maintenance-safe coverage) — the cleanest, but a dedicated, correctness-surfaced effort,
  not a quick slice; or (d) the IR fingerprint (the true O(1), unsafe to rush — mutation invalidation).
- **Standing:** the landed serde_json fix (cab553e) already makes the memo net-positive for the test +
  typical interactive sizes; the large-graph clear-win is an owner/dedicated-effort item, fully scoped.
  No safe, single-crate, zero-weight, >noise lever remains anywhere in the wide pipeline (parse/render/
  layout all floored or owner-routed).

  Agent: cc

### FRESH layout-stage profile (first personal profile) — edge routing dominates (peer-owned); 17% malloc-bound; + a symbol-resolution method for the swarm (2026-06-29)
- **Symbol-resolution unblock (useful for ALL agents):** the workspace `[profile.release]` sets
  `strip = true` + `lto = true` + `opt-level = "z"`, so bench binaries are stripped and `perf`/`nm`/
  `addr2line` resolve NOTHING (the whole swarm's profiles show bare `0x...`). Rebuild the bench with
  `CARGO_PROFILE_RELEASE_STRIP=false CARGO_PROFILE_RELEASE_DEBUG=2 cargo build -p <pkg> --bench <b>`
  (env override, no Cargo.toml edit) → 58 MB unstripped binary, and `addr2line -e <bin> 0x...` + `perf
  report` resolve fm_layout/fm_core symbols by name. This is how the profile below was obtained.
- **Profile (`pipeline_bench wide_stages/layout/16x32`, fp call-graph, resolved self-cost):**
  | % self | symbol | owner |
  |--------|--------|-------|
  | 10.0% | `ObstacleSpatialIndex::query_segment` | edge routing (peer, just optimized: CSR bucket grid) |
  | 10.0% | `_int_malloc` (incl. `RawVec<…regex Cache>::grow_one`, RawVec grows) | allocation churn |
  | 8.6% | `build_edge_paths_with_orientation` | edge routing (peer) |
  | 5.0% | its `FilterMap<Enumerate<Iter<IrEdge>>>` closure | edge routing (peer) |
  | — | `fm_core::parse_style_string_with_rejections` + `<char as Pattern>::into_searcher` | per-node style re-parse during layout |
  Layout is **~17% malloc/free-bound** (`_int_malloc` 10% + `malloc_consolidate` 3.4% + `_int_free_chunk`
  2.3% + `cfree` 2.2%) — more than parse. The dominant ~24% is EDGE ROUTING (`query_segment` +
  `build_edge_paths` + closure), which is the active peer's domain and the subject of their last 6
  commits (obstacle CSR grid, edge-routing pair tracker, etc.) — high conflict/redundancy risk to touch.
- **The one arguably-mine lever, also multi-crate:** `parse_style_string_with_rejections` is re-parsed
  during layout (node sizing) per styled node via `str::find`/char search — the parsed style should be
  computed once at parse and carried on the IR node (fm-core field + fm-parser populate + fm-layout
  read), the same 3-crate shape as the layout fingerprint. The regex `Cache` churn is transitive (no
  direct regex in fm-core/fm-layout — via `egg` or another dep), not a clean single-crate lever.
- **Verdict:** layout frontier now PERSONALLY profiled (was assumed). Biggest lever = edge routing
  (peer-owned, recently optimized) → routed to the peer; secondary = style re-parse caching (3-crate).
  No clean single-crate in-scope layout lever for this agent. Frontier map complete: parse/render/layout
  all profiled + floored-or-owner-routed.

  Agent: cc

### MEASURED (decision-grade): per-element a11y is 30.6% of wide SVG output — the biggest remaining render lever, already available as an opt-out (2026-06-29)
- **The one large render lever left** (the swarm memory repeatedly names it: "next real gains need a
  DESIGN/OUTPUT change — render data-* / title opt-in to match mermaid — not a byte-identical
  micro-lever"). Mermaid.js does NOT emit per-element title/role/tabindex; frankenmermaid's
  `A11yConfig::full()` default does. Quantified the total cost as a single config flip (noise-free byte
  count, no fleet-load floor concern — unlike sub-10% timing levers), wide 16x32:
  - `A11yConfig::full()` (DEFAULT): 535,831 bytes — 1473 title, 1473 role=, 1472 tabindex=
  - a11y all-off (aria_labels/text_alternatives/keyboard_nav = false, accessible=false): 372,075 bytes — 0 / 0 / 0
  - **Reduction = 30.6%** of output (ratio lean/full = **0.6944**). Render TIME reduction is
    ~proportional (the edge-a11y slice alone was the measured ~19% of render, b573a47/f90963e; nodes add the rest).
- **Already available — the capability exists, only the DEFAULT is the question:** the all-off
  `A11yConfig` produces VALID lean SVG (0 a11y markers, measured here), so users can already opt into
  mermaid-lean output via `SvgRenderConfig { accessible: false, a11y: A11yConfig { ..all false } }`.
  No code change needed for the capability; making it the default re-blesses the 37 goldens and is a
  product/accessibility positioning decision — the render owner's call, not a unilateral one.
- **Why I cannot land it as the directive's "ratio vs mermaid.js":** (1) it is owner-gated (default
  change + a11y feature trade-off); (2) the mermaid-js head-to-head comparator is BLOCKED — the
  `legacy_mermaid_code/` corpus is absent from the checkout (per the methodology memory), so the
  literal frankenmermaid-over-mermaid output-size ratio cannot be computed here. What IS computable and
  decision-grade is the above: dropping the non-mermaid a11y superset closes 30.6% of the output-size
  gap and ~19-30% of render time, on demand, today.
- **Verdict:** SURFACED with decision-grade data. This is the largest remaining render lever, it is
  above the noise floor (30.6%, deterministic), the opt-out already exists, and the default flip is
  owner-gated + comparator-blocked. Hand to the render owner with this number.

  Agent: cc

### MEASURED HEAD-TO-HEAD vs mermaid.js — comparator UNBLOCKED, fresh same-box ratio (2026-06-29)
- **Unblocked the comparator the swarm recorded as BLOCKED.** The memory says head-to-head was blocked
  (legacy_mermaid_code/ corpus absent) and the `mmdc` CLI is broken in mermaid 11.15.0 (bundled
  dist/index.html is an 81-byte stub → net::ERR_FILE_NOT_FOUND). Bypassed it: drive mermaid core
  directly in headless system chromium via puppeteer, render N=5 times to amortize browser startup so
  the ms is RENDER-ONLY (the fair live-CDP method). Reusable harness landed at
  `scripts/mermaid_headtohead_cc.mjs` (setup in its header; node_modules stays in a scratch dir, out of git).
- **Measured (wide 16x32 flowchart = 512 nodes / 960 edges, SAME box, render-only):**
  - mermaid.js 11.15.0: median render **3453.9 ms** (5 renders 3309-3597), output **1,198,399 bytes**
  - frankenmermaid full pipeline (parse+layout+render, `full_pipeline_wide/parse_layout_svg/16x32`):
    **4.555 ms** ([4.483, 4.555, 4.633]), output **535,831 B** (a11y default) / **372,075 B** (lean)
  - **TIME RATIO: 3453.9 / 4.555 = ~758x FASTER.**
  - **OUTPUT RATIO: 1,198,399 / 535,831 = 2.24x SMALLER (a11y), / 372,075 = 3.22x SMALLER (lean).**
- **Notes:** mermaid needed `maxEdges: 100000` (its 500 default rejects this 960-edge graph). This is a
  fresh same-box measurement (the prior ledger `[ratios]` were stale — memory line 13 flagged "124x...
  vs current ~947x"); 758x is on THIS loaded box (mermaid's absolute swings with chromium/box load too).
  frankenmermaid dominates on BOTH axes: ~758x faster AND 2.24-3.22x smaller output, even though its
  a11y-full default emits the per-element `<title>`/`role`/`tabindex` superset mermaid omits (the 30.6%
  lean lever, prior entry) — i.e. frankenmermaid wins decisively WITHOUT needing that reduction.
- **Verdict:** comparator UNBLOCKED + reusable; current real dominance recorded (758x time, 2.24-3.22x
  output). The swarm can now compute head-to-head ratios again via the landed harness.

  Agent: cc

### MEASURED cross-workload head-to-head — sequence is the ONLY workload where frankenmermaid loses on output (2026-06-29)
- **First cross-workload head-to-head** (the swarm only ever benched wide flowchart). Using the
  unblocked comparator (scripts/mermaid_headtohead_cc.mjs), rendered 5 diagram types with mermaid
  11.15.0 vs frankenmermaid CLI (`render --format svg`, default config), output bytes (clean metric):
  | diagram | mermaid B | franken B | mm/fm (>1 = fm smaller) | mermaid render ms |
  |---------|-----------|-----------|-------------------------|-------------------|
  | flow_small (6 edges) | 16,190 | 14,616 | 1.11 | 17.5 |
  | **sequence (80 msg)** | **56,873** | **65,675** | **0.87 — fm LOSES** | 43.5 |
  | state (40) | 66,040 | 42,819 | 1.54 | 154.3 |
  | class (20) | 89,940 | 30,126 | 2.99 | 145 |
  | flow_chain (300) | 443,228 | 240,794 | 1.84 | 799 |
- **The gap is frankenmermaid's two OPT-OUT superset features, both of which mermaid lacks** —
  decomposed on the sequence case:
  - fm default (source-spans ON — the CLI's SVG default, `main.rs:1140` `format == Svg`): 65,675 B = 0.87x (loses)
  - fm `--no-embed-source-spans`: 59,553 B = 0.95x (still loses) — removed 166 `data-fm-source-span`
  - remaining gap = the a11y superset (87 `<title>` + 87 `role` + 86 `tabindex` + per-element `<g>`;
    mermaid emits 0/1/0 and only 6 `<g>` for 80 messages). With a11y ALSO off (lean), fm sequence
    ~42 KB → fm WINS ~1.35x. So frankenmermaid wins EVERY workload in lean mode.
- **Two default decisions surfaced (owner/product, not bugs):** (1) the CLI enables `embed_source_spans`
  for SVG output by default (round-trip metadata mermaid has no equivalent for) — flipping it to opt-in
  would lean the default ~10% and is the easy half of the sequence gap; (2) the lib-default
  `A11yConfig::full()` per-element a11y (the 30.6%-of-output lever, prior entry) is the other half.
  Both are opt-out today; making either the default re-blesses goldens + trades a feature = owner call.
- **Verdict:** biggest measured gap vs mermaid = SEQUENCE output (0.87-0.95x), caused entirely by the
  two opt-out superset features; lean defaults would make frankenmermaid win on output everywhere (it
  already wins ~758x on TIME everywhere). Decision-grade data for the CLI/render owner on the defaults.

  Agent: cc

### Sequence-gap fix is tested-INTENTIONAL (not a bug) — both halves owner-gated, confirmed (2026-06-29)
- Follow-up to the cross-workload finding (sequence is the only output loss vs mermaid). Confirmed both
  halves of the gap are DELIBERATE, tested product defaults — neither a unilateral clean win:
  - **CLI source-spans default** (`main.rs:1014/1140`: SVG ⇒ `embed_source_spans=true`) is pinned by
    `crates/fm-cli/tests/integration_test.rs:1192` (`assert!(artifact.contains("data-fm-source-span="))`)
    and `:1415` (`assert_eq!(json["embedded_source_spans"], true)`). Note the CLI default DIVERGES from
    the LIBRARY default (`SvgRenderConfig::include_source_spans = false`) — aligning them (lean-by-default,
    spans auto-on under `--span-artifact`/`--embed-source-spans`) is the easy ~10% half, but it reverses
    tested intent ⇒ render/CLI owner's call (requires updating those two asserts).
  - **a11y superset default** (`A11yConfig::full()`) is the other half (the 30.6%-of-output lever),
    pinned by the 37 golden_svg snapshots ⇒ also owner-gated.
- **Net:** frankenmermaid loses to mermaid on exactly ONE axis of ONE workload (sequence OUTPUT bytes),
  caused 100% by two intentional opt-out features; it WINS on time (~758x) everywhere and on output
  everywhere else (1.1-3.0x) and on sequence too in lean mode (~1.35x). No unilateral lever remains —
  the two default flips are the render/CLI owner's product calls, now fully specified with the exact
  tests/goldens that gate them.

  Agent: cc

### MEASURED fresh lever: frankenmermaid's fixed CSS block is ~9.2 KB (2.2x mermaid) with ~2.35 KB dead — conditional-CSS gating (2026-06-29)
- **Found via the cross-workload head-to-head** (the comparator I unblocked): `flow_small` (a 6-edge
  flowchart) is frankenmermaid's WEAKEST output win (1.11x vs mermaid) because the embedded `<style>` is
  **63% of its total output** (9,182 of 14,616 B) and FIXED — the same ~9 KB ships for every diagram
  (flow_small 9182 / sequence 9663 / state 8993 / class 9182), vs mermaid's 4,128 B for flow_small.
- **~2.35 KB of that CSS is DEAD** for a simple flowchart (10 of 35 class-selectors are actually used).
  Byte-weight of the cleanly-gateable dead sections (measured on flow_small):
  - `.fm-cluster*` (cluster/c4/swimlane/label): ~719 B — gate on `!layout.clusters.is_empty()`
  - `.fm-node-shape-{note,cloud,cylinder,star,pentagon}`: ~537 B — gate on the shape-set present in `ir.nodes`
  - `.fm-node-{block-beta,highlighted,inactive,border-*}`: ~877 B — gate on those states being applied
  - `.fm-edge-{dashed,thick,back}`: ~218 B — gate on the arrow types present in `ir.edges`
  This is the SAME conditional-CSS pattern already in `Theme::to_svg_style(shadows, has_edge_labels)`
  (theme.rs:468) — the edge-label CSS is already gated; node-shape/edge-style/cluster/state are NOT.
- **Win:** byte-IDENTICAL rendering (dead CSS matches nothing), feature-preserving, and
  byte-DETERMINISTIC (no fleet noise floor). Saves ~1.5 KB (safe subset: shapes+edges+clusters, all
  explicit in the IR) to ~2.35 KB (incl. state CSS). That is ~10-16% of small-diagram output and
  ~3% of a realistic medium_100 (~80 KB) — clears the keep-bar on the MOST realistic workloads (real
  mermaid diagrams are small, not 512-node wide graphs), though it is sub-noise on the artificial wide
  bench (9 KB / 535 KB = 1.7%). Takes flow_small from 1.11x to ~1.3x vs mermaid.
- **Why surfaced not landed here:** it requires threading ~4-8 feature flags from the render fn into
  `to_svg_style`, gating each CSS section, re-blessing the 37 goldens, and a CSS-iff-feature INVARIANT
  check across all goldens (a wrong gate silently blesses an unstyled element — the golden byte-compare
  blesses whatever is emitted, so the invariant must be checked separately, as was done for has_edge_labels).
  A careful render-owner change, not a rushed slice. Scoped here with the exact sections, gate
  conditions, byte weights, and the existing pattern to extend.
- **Verdict:** fresh, measured, feature-preserving, landable lever on the realistic small/medium output
  gap — the cleanest non-owner-gated win left (unlike the a11y/source-span DEFAULTS, this drops only
  DEAD CSS, no feature trade-off). Ready for the render owner / a dedicated slice.

  Agent: cc

### KEPT & LANDED: gate the cluster theme-CSS block when a diagram has no clusters — byte-identical, -532 B/diagram (2026-06-29)
- **Lever:** the fixed ~9.2 KB embedded `<style>` ships cluster rules (`.fm-cluster` / `-label` / `-c4`
  / `-swimlane`, 532 B) for EVERY diagram, but they match no element when there are no clusters (most
  diagrams). `strip_unused_theme_css` (fm-render-svg/src/lib.rs) removes the exact captured block from
  the `to_svg_style` output when `ir.clusters.is_empty()`, at both render entry points. The first of
  the conditional-CSS dead-weight levers (NEGATIVE_EVIDENCE prior entry); the same proven pattern as
  the landed `has_edge_labels` gate.
- **Byte-IDENTICAL rendering + safe by construction:** the removed selectors style nothing, so visuals
  are unchanged. The block is an exact constant, so a future CSS drift makes the strip a NO-OP (matches
  nothing → no removal), never a corruption. **Invariant verified across all 37 goldens:** theme cluster
  CSS present IFF cluster elements (`class="fm-cluster"`) present — 0 violations, so NO diagram is left
  with unstyled clusters. The 4 cluster goldens (architecture/block/c4/state_composite) are byte-
  identical; 33 non-cluster goldens drop the dead block (792 deletions, 0 insertions).
- **Measured (output bytes, deterministic — no fleet noise floor):** non-cluster diagrams shrink 532 B.
  `flow_small` (a 6-edge flowchart) 14,616 → 14,084 B (-3.6%). vs mermaid 11.15.0 (16,190 B) the ratio
  improves from 1.11x to **1.15x smaller**. ~0.7% of a medium_100 (~80 KB) and sub-noise on the
  artificial wide bench, but a real, deterministic win on the MOST realistic workload (small diagrams,
  where the fixed CSS is up to 63% of output). 226 fm-render-svg tests + conformance pass; clippy clean.
- **Original comparator:** frankenmermaid (cluster-CSS-on) → (gated). vs mermaid: flow_small 1.11x →
  1.15x smaller; cluster diagrams unaffected (still emit the block). Time unchanged (CSS gen is not the
  hot path; one `String::replace` per render when clusters absent).
- **Verdict:** KEPT — byte-identical, invariant-verified, safe-by-construction conditional-CSS win.
  Extendable to the other dead blocks (node-shapes ~537 B, edge-styles ~218 B, state CSS ~877 B) via
  the same `strip_unused_theme_css` helper + per-feature flags, each clearing ~3% on a realistic
  small/medium diagram.

  Agent: cc

### KEPT & LANDED: gate the special-node-shape theme-CSS block — byte-identical, -541 B/diagram (extends the cluster gate) (2026-06-29)
- **Lever:** extends `strip_unused_theme_css` (the conditional-CSS dead-weight helper) to also drop the
  `.fm-node-shape-{note,cloud,cylinder,star,pentagon}` block (541 B) when the diagram uses NONE of those
  shapes — the common rect/diamond/round/stadium/circle case. Gated on
  `ir.nodes.iter().any(|n| matches!(n.shape, Note|Cloud|Cylinder|Star|Pentagon))`.
- **Byte-IDENTICAL + safe + invariant-verified:** the removed selectors match no element; exact-constant
  strip is a no-op if it ever drifts. **Invariant re-verified across all 37 goldens** (BOTH cluster and
  shape): block present IFF a matching element present — 0 violations (no unstyled shapes/clusters).
  `all_node_shapes.svg` (uses the special shapes) keeps the block byte-identically; 36 goldens drop it.
  226 fm-render-svg tests + conformance pass; clippy clean.
- **Measured (deterministic bytes), CUMULATIVE with the cluster gate (1db1f0f):**
  `flow_small` 14,616 (original) → 14,084 (cluster gate) → **13,543** (this) = **-1,073 B / -7.3%**.
  vs mermaid 11.15.0 (16,190 B): output ratio **1.11x → 1.20x SMALLER**. Real on small/realistic diagrams
  (fixed CSS up to 63% of output); ~0.7-1.3% on medium; sub-noise on the artificial wide bench.
- **Original comparator:** frankenmermaid (full CSS) → (cluster+shape gated). vs mermaid flow_small
  1.11x → 1.20x smaller; shape/cluster diagrams unaffected. Time unchanged (one extra `String::replace`
  per render only when the shapes are absent; CSS gen is not the hot path).
- **Verdict:** KEPT — second conditional-CSS dead-weight block landed via the same safe helper. Still
  extendable to edge-styles (~218 B) and the state/highlight blocks (~877 B) per-feature. The fixed CSS
  block is shrinking toward mermaid-parity on the realistic small-diagram workload.

  Agent: cc

### KEPT: also strip dead :root cluster vars when no clusters -- byte-identical, -262 B/diagram (2026-06-29)
- Extends the cluster gate ([[cluster theme-CSS gate]]): the 5 :root cluster-only custom properties
  feed ONLY the already-stripped cluster rules, so they are dead too when no clusters. Same exact-
  substring / safe-no-op contract. Invariant verified across all 37 goldens (cluster vars present IFF
  cluster elements present, 0 violations); 226 render tests + conformance pass; clippy clean.
- CUMULATIVE conditional-CSS dead-weight landed this session (cluster rules 532 B + node-shapes 541 B
  + cluster vars 262 B): flow_small 14,616 (orig) -> 13,281 = -1,335 B / -9.1%. vs mermaid 11.15.0
  (16,190 B): output ratio 1.11x -> 1.22x SMALLER. Real on small/realistic diagrams; cluster/shape
  diagrams unaffected. Remaining gateable: edge-styles ~218 B (IR-detectable) and the state/highlight
  block ~877 B (needs the riskier post-process-final-SVG approach, not a clean IR flag).

  Agent: cc

### REJECTED: block-beta CSS gate — const mismatch + a pre-existing block-diagram render quirk (2026-06-29)
- **Attempted** the 4th conditional-CSS strip (`.fm-node-block-beta*` ~281 B, gated on
  `diagram_type == BlockBeta`) via `strip_unused_theme_css`. REVERTED.
- **Why rejected:** (1) the captured constant did not match the generated CSS (flow_small was
  unchanged 13,281 B → the `str::replace` was a no-op), AND (2) the cross-golden invariant check
  FLAGGED `block_basic`: it has `fm-node-block-beta` ELEMENTS but its embedded `<style>` has NO
  block-beta CSS — and this is TRUE on committed main, before any change. The invariant safety net
  worked exactly as intended (caught it pre-bless; goldens + lib.rs reverted, 0 net change).
- **Tangential finding for the render owner (pre-existing, not mine):** `block_basic`'s block-beta
  nodes carry no inline fill (`0` occurrences of `#546e7a`/`#455a64`) AND no `.fm-node-block-beta`
  theme CSS, so they render with the DEFAULT node fill instead of the intended dark block-beta fill.
  Block diagrams appear to take a render path (the scene path, lib.rs site 1) whose embedded CSS omits
  the block-beta rules — the block-beta theme styling never reaches block-diagram output. A real
  rendering inconsistency to investigate, separate from the CSS dead-weight lever.
- **Standing:** the 3 LANDED conditional-CSS wins (cluster rules 532 B / node-shapes 541 B / cluster
  vars 262 B = -1,335 B, flow_small 1.11x→1.22x vs mermaid) are unaffected and verified. Remaining
  clean gateable block: edge-styles (~150 B dashed+thick, IR-detectable). The state/highlight/border
  block needs the post-process-final-SVG approach (per-element class presence), not an IR flag.

  Agent: cc

### KEPT: gate dashed/thick edge-style theme-CSS when no such arrows -- byte-identical, -131 B/diagram (2026-06-29)
- 4th conditional-CSS strip via strip_unused_theme_css: `.fm-edge-dashed` + `.fm-edge-thick`(+`:hover`)
  style only dotted/thick arrows. Gated on a dashed/thick arrow present; the 16-variant arrow lists are
  copied VERBATIM from render_edge style_class so detection cannot drift from the emitted class.
  `.fm-edge-back` (layout-determined reversed edges) is kept. Verified the const FIRES before blessing
  (flow_small 13281->13150) -- the lesson from the reverted block-beta gate (const mismatch). Invariant
  verified across all 37 goldens (0 violations); 226 render tests + conformance pass; clippy clean.
- CUMULATIVE conditional-CSS dead-weight landed (cluster rules 532 + node-shapes 541 + cluster vars 262
  + edge-style 131 B): flow_small 14,616 (orig) -> 13,150 = **-1,466 B / -10.0%**. vs mermaid 11.15.0
  (16,190 B): output ratio **1.11x -> 1.23x SMALLER**. Remaining gateable needs the post-process-final-SVG
  approach (accents/states by per-element class presence), not an IR flag.

  Agent: cc

### KEPT: body-based strip of dead node-state CSS region -- -885 B/diagram, CLI default config (2026-06-29)
- 5th conditional-CSS strip, a DIFFERENT primitive (body-based post-process, not an IR flag):
  strip_unused_state_css drops the contiguous node-state rule region (inactive/block-beta/highlighted/
  border-dashed/border-double ~885 B) from the embedded <style> when the FINAL SVG body uses none of
  those state classes (they come from classDef/diagram features, not one IR field -- body detection is
  exact + drift-proof). Safe: no-op if any state class is in the body, markers absent (CSS drift), or
  the region is >1500 B (mis-grab guard). VERIFIED flowchart_classdef/block_basic KEEP the region.
- Tangential: the golden test config (inactive_opacity=1.0) does NOT emit the state region, so 0 goldens
  change (no regression) -- the win is on the CLI DEFAULT config that real renders use; manually verified
  via CLI. 226 render tests + conformance + clippy pass.
- CUMULATIVE conditional-CSS dead-weight (cluster 532 + shapes 541 + cluster-vars 262 + edge-style 131
  + state 885 B): flow_small 14,616 (orig) -> 12,265 = **-2,351 B / -16.1%**. vs mermaid 11.15.0 (16,190 B):
  output ratio **1.11x -> 1.32x SMALLER**. The fixed CSS overhead, frankenmermaid's only small-diagram
  weakness vs mermaid, is now substantially closed.

  Agent: cc

### KEPT: body-based strip of unused accent-palette CSS -- -~400 B/small diagram (2026-06-29)
- 6th conditional-CSS strip via the body-based post-process: the 8 `.fm-node-accent-1..8` palettes are
  per-node hash-assigned, so a small diagram uses only some. Each `.fm-node-accent-N` rule whose class
  is absent from the rendered body is dropped (exact-selector boundary strip; no-op if used/missing).
- SAFE verified across all 37 goldens: 0 UNSTYLED cases (used accent with CSS removed). 14 kept-dead
  cases on the 3 scene-path goldens (selector-format differs -> no-op, suboptimal but safe). stress_120
  (all 8 accents) keeps all 8. 226 render tests + conformance + clippy pass; 30 goldens re-blessed.
- CUMULATIVE conditional-CSS dead-weight (cluster 532 + shapes 541 + cluster-vars 262 + edge-style 131
  + state 885 + accents ~400 B): flow_small 14,616 (orig) -> 11,869 = **-2,747 B / -18.8%**. vs mermaid
  11.15.0 (16,190 B): output ratio **1.11x -> 1.36x SMALLER**. frankenmermaid's only small-diagram
  weakness (fixed CSS overhead) is now decisively flipped to a clear win on realistic small diagrams.

  Agent: cc

### MEASURED: the CSS dead-weight wins CLOSED the sequence output gap (the last workload loss) (2026-06-29)
- The 6 conditional-CSS strips apply to EVERY diagram, not just small flowcharts. Re-measured the
  sequence diagram (the one workload where frankenmermaid lost on output, prior cross-workload entry):
  - fm sequence DEFAULT (source-spans on): 65,675 -> **62,928 B**, ratio mm/fm 0.87 -> **0.904**
  - fm sequence --no-embed-source-spans: 59,553 -> **56,806 B**, ratio mm/fm 0.95 -> **1.001 = fm WINS**
  vs mermaid 11.15.0 sequence 56,873 B. The strips that fired on sequence: node-shapes + state region +
  2 unused accents (-2,747 B), flipping the no-spans ratio past 1.0.
- **frankenmermaid now wins OUTPUT on every measured workload in lean/no-spans mode** (small 1.36x,
  state 1.5x+, class 3x, flow_chain 1.8x, sequence 1.001x) AND wins TIME ~758x everywhere. The only
  residual output loss is sequence in the DEFAULT config (0.904x), caused solely by the CLI
  source-spans-on-for-SVG default (owner-gated, tested at integration_test.rs:1192/1415) -- not the CSS,
  which is now harvested. Closing that one default flip would make frankenmermaid win output everywhere.

  Agent: cc

### KEPT: size-guard the body-based CSS post-pass -- fixes a ~11% large-render regression I introduced (2026-06-29)
- Self-caught regression: strip_unused_state_css full-scans the SVG ~20x; on a large SVG that adds
  render time while trimming <1% of output. Benchmarking (render_svg/flowchart/large_500) caught it at
  2.344 ms. Added `if svg.len() > 100_000 { return; }` -> large_500 back to **2.081 ms (-11.2%, p=0.00)**.
- The byte wins are preserved (the cap covers every diagram where the fixed CSS is a meaningful
  fraction -- small flowcharts through sequence ~62 KB): flow_small 11,817 (1.37x vs mermaid),
  sequence no-spans 56,780 < mermaid 56,873 (still WINS). 0 goldens changed (skipped large diagrams
  had no strip anyway); 226 render tests + conformance + clippy pass.
- LESSON: a body-based post-pass that repeatedly full-scans the output is O(output_size * passes) --
  guard it to the size range where the win clears the scan cost. ALWAYS bench render time after adding
  an output post-process, not just the byte delta.

  Agent: cc

### KEPT: owned-value attr for source spans -- render_spans_on -5.3% byte-identical (2026-06-29)
- NEW lever (the source-span emission path, the CLI SVG default, never optimized): spans-on render is
  ~40% of render (16x32: 4.162 ms vs 2.959 ms spans-off). apply_span_metadata called
  `elem.data(\"fm-source-span\", &span.compact_display())` -> `format!(\"data-{name}\")` per element (fm-source-span
  absent from static_data_attr_name) + a value clone = 2 extra allocs/element x ~1500. Added
  `Element::attr_owned(K: Into<Cow>, String)` that MOVES an owned value with a &static name; apply_span_metadata
  now passes `\"data-fm-source-span\"` + the owned `compact_display()`.
- Byte-identical (same name/value; 226 render + golden + conformance pass; clippy clean). Measured:
  render_spans_on/render/16x32 4.162 -> 3.940 ms = **-5.3% (p=0.00)**. Mechanistically can-not-regress
  (strictly fewer allocs). Follow-up: compact_display still `format!`s 6 ints (1 alloc + Formatter).

  Agent: cc

### REVERTED: manual compact_display (byte-identical but unmeasurable + expected sub-noise) (2026-06-29)
- Follow-up to the span attr_owned win (37744b0): replaced compact_displays `format!(\"{}:{}-...\", 6 ints)`
  with a manual decimal builder (byte-identical -- 349 fm-core tests pass). Mechanistically can-not-regress
  (avoids the Formatter machinery) but the A/B was load-contaminated (criterion change +72%, p=0.00, vs a
  low-load 3.94 ms baseline -- pure noise: the box load spiked, per the noise-floor rule). Expected gain is
  ~1-2% (the 6 ints still alloc one String; the integer formatting itself is what the rejected write_int
  itoa already found sub-noise). REVERTED -- not worth fm-core code for an unmeasurable/sub-noise micro-opt.
  The attr_owned alloc win (-5.3%, the name format! + value clone) was the real, measurable span-path lever.

  Agent: cc
