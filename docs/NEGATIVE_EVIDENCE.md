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
