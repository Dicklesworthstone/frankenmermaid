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

_(none yet — first measured experiments in progress)_

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
