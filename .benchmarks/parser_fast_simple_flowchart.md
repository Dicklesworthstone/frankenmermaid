# Simple flowchart parser fast path - KEPT (2026-06-25)

## Lever

`fm-parser` now recognizes the common single-statement flowchart cases before
building the chumsky statement parser:

- bare node ids: `A`
- simple rectangular labels: `A[Label]`
- simple bare-id edges: `A-->B`, `A---B`, `A-.->B`, `A==>B`, `A--oB`, `A--xB`

The fast path rejects chained, labeled, classed, quoted, grouped, parallel, or
shape-rich statements and falls through to the existing parser/recovery paths.

## Baseline

- Baseline worktree: `/data/projects/.worktrees/frankenmermaid-cod-b-parser-baseline-48bb15c`
- Baseline commit: `48bb15c`
- Target dir: `/data/projects/.rch-targets/frankenmermaid-cod-b`
- Build policy: per-crate only, package `frankenmermaid-cli`

## Parse-only A/B

Command shape:

```bash
CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b \
RUSTFLAGS='-C metadata=codbbaselinefastpath' \
rch exec -- cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench -- parse/flowchart --warm-up-time 1 --measurement-time 2

CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b \
RUSTFLAGS='-C metadata=codbcandidatefastpathlocal' \
cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench -- parse/flowchart --warm-up-time 1 --measurement-time 2
```

`rch` had no admissible worker for the baseline parse run and fell back local.
The candidate parse run was local too, so the keep/revert comparison used the
same machine.

| bench | baseline mean | candidate mean | speedup |
|---|---:|---:|---:|
| `parse/flowchart/small_10` | `71.856 us` | `32.963 us` | `2.180x` |
| `parse/flowchart/medium_100` | `662.18 us` | `271.47 us` | `2.439x` |
| `parse/flowchart/large_1000` | `7.3442 ms` | `3.9525 ms` | `1.858x` |

## Wide full-pipeline A/B

Command shape:

```bash
CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b \
RUSTFLAGS='-C metadata=codbbaselinefastpathwide' \
cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench -- full_pipeline_wide --warm-up-time 1 --measurement-time 2

CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b \
RUSTFLAGS='-C metadata=codbcandidatefastpathwide' \
cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench -- full_pipeline_wide --warm-up-time 1 --measurement-time 2
```

| bench | baseline mean | candidate mean | speedup |
|---|---:|---:|---:|
| `full_pipeline_wide/parse_layout_svg/8x16` | `3.0428 ms` | `2.1856 ms` | `1.392x` |
| `full_pipeline_wide/parse_layout_svg/12x24` | `7.8318 ms` | `5.2484 ms` | `1.492x` |
| `full_pipeline_wide/parse_layout_svg/16x32` | `13.182 ms` | `9.7081 ms` | `1.358x` |

The final-code absolute means remain materially faster than the detached
baseline. The checked-slice cleanup rerun showed no statistically detected
change versus the earlier candidate run.

## Mermaid.js ratio

Using the pinned Mermaid.js 11.12.0 denominator already recorded in
`evidence/ledger/mermaid-js-head-to-head.toml`:

| case | frankenmermaid mean | Mermaid.js mean | fm / Mermaid.js | Mermaid.js slower |
|---|---:|---:|---:|---:|
| `8x16` | `2.1856 ms` | `499.28 ms` | `0.004378x` | `228.4x` |
| `12x24` | `5.2484 ms` | `1077.69 ms` | `0.004870x` | `205.3x` |
| `16x32` | `9.7081 ms` | `3948.7 ms` | `0.002459x` | `406.7x` |

## Behavior proof

- Focused tests compare fast-path edge ASTs with the existing fallback edge
  parser and fast-path node ASTs with the chumsky statement parser.
- Complex statements remain rejected by the fast path and fall through to the
  existing parser/recovery sequence.
- `cargo test -p fm-parser flowchart_fast_path -- --nocapture` passed before
  full validation.

## Decision

Keep. The lever is a large parse-only win and a material full-pipeline win on
the wide Mermaid.js comparator inputs. Do not expand the fast path beyond simple
single statements without differential tests against the fallback path.
