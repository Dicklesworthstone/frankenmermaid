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
