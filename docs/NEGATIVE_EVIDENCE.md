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
