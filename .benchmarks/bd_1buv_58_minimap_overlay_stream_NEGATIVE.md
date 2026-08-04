# bd-1buv.58 — minimap overlay row streaming — NEGATIVE

Date: 2026-07-23
Agent: MagentaGull
Verdict: **REJECT**

## Scope

Measured-frontier micro-lever only. The candidate changed `overlay_minimap` from collecting every
main-output row and then cloning the first `main_height` rows into the result grid to consuming
`main_output.lines()` directly into that grid. No incremental-computation or architectural surface
was touched.

## Ledger-first routing

- `.benchmarks/bd_1buv_2_flowchart_parse_layout_floor_ANALYSIS.md` closes the obvious large
  flowchart parse/layout micro-paths and routes architectural follow-up out of this lane.
- Existing parser and renderer NEGATIVE rows close identifier LUTs, bulk CSS copy, and previously
  mined byte-scan families.
- `bd-1buv.58` was the remaining explicit terminal-render micro-frontier with a profile-first
  predicate: proceed only if duplicate Unicode-row materialization/allocation was material.

## Profile

Baseline source, release-optimized with debug symbols and LTO disabled only for profiling:

```text
RCH_REQUIRE_REMOTE=1 RCH_FORCE_REMOTE=1 RCH_WORKER=vmi1227854 \
  CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b \
  rch exec -- cargo \
    --config profile.release.strip=false \
    --config profile.release.debug=true \
    --config profile.release.lto=false \
    --config build.rustflags=["-C","force-frame-pointers=yes"] \
    test -j2 -p fm-render-term --lib --profile release --no-run
```

`perf record -F 999 -g --call-graph dwarf` on the resulting ignored 240x120 ragged-Unicode overlay
probe completed in 8.31 s with 8,281 samples and zero lost samples. Ranked self time:

| Frame | Self |
|---|---:|
| `String::push` | 30.60% |
| `Chars::next` | 15.11% |
| `Vec<char>::extend_desugared<Chars>` | 8.61% |
| `Vec<u8>::reserve` | 8.18% |
| `IntoIter<char>::collect<String>` | 5.28% |
| `realloc` | 1.35% |

This cleared the profile predicate: Unicode row materialization plus allocation/growth was a
top-ranked block, so the one source lever was tested.

## Correctness

A same-binary reference function preserved the exact previous implementation. The candidate matched
it byte-for-byte across 40 combinations covering:

- empty and missing main rows;
- ragged, overlong, and extra rows;
- Unicode scalar values;
- zero-sized and non-empty minimaps;
- all four overlay corners and clipping boundaries.

The exact oracle test passed on both the profiling binary and the production release binary.

## Same-binary A/B and null controls

Every row below alternated reference/candidate and reference/reference null arms inside one process.
Each result is a screening or scored run as labeled; no run with a failed CV/null gate is accepted.

| Worker/profile | Samples x calls/sample | Ref median ns | Candidate median ns | Raw win | Ref CV | Candidate CV | Null delta | Null-A CV | Null-B CV | Verdict |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `vmi1227854`, profile build | 40 x 150 | 377,649.2 | 348,650.5 | 7.68% | 11.40% | 16.34% | 1.87% | 11.54% | 8.07% | invalid |
| `vmi1227854`, profile build | 30 x 1,000 | 381,915.5 | 354,736.6 | 7.12% | 6.96% | 9.79% | 2.58% | 7.53% | 8.67% | invalid |
| `vmi1149989`, production release | 30 x 1,500 | 361,289.6 | 342,064.2 | 5.32% | 4.76% | 5.41% | 1.51% | 4.69% | 5.46% | invalid |
| `vmi1149989`, production release | 24 x 3,000 | 367,274.4 | 348,444.9 | 5.13% | 5.29% | 7.70% | 4.01% | 8.86% | 4.88% | **REJECT** |

The final production run used CPU 7 after a one-second occupancy sample reported 98.99% idle.
Despite increased batching, at least one scored arm still exceeded the required CV < 5%, and the
null control itself was not flat enough. The repeatable raw median direction is interesting but is
not admissible evidence.

## Disposition and retry predicate

The production edit and measurement-only test harness were removed. No source code ships.

Retry only when an exclusive or otherwise isolated worker/core is available and two consecutive
reference/reference preflight runs both satisfy:

1. every null arm CV < 3%;
2. null median delta < 1%;
3. the production release profile and the same realistic ragged-Unicode fixture are used.

Only then reintroduce the row-streaming candidate and require candidate/reference plus both null arms
to remain below CV 5% with a null-adjusted win of at least 3%.
