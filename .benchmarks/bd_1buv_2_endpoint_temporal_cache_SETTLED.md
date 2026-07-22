# bd-1buv.2 endpoint temporal cache — SURFACE / BLOCKER (2026-07-22)

## Profile-first target

Strict-remote release profiling was built on `vmi1149989` from `643b7cd4`. A symbolized
`profharness flowchart 5000 1800 parse` run recorded 4,499 samples with zero lost samples.
`NodeIdIndex::get_with_hash` was the second-largest parse self symbol at **9.39%**;
`parse_fast_simple_flowchart_edge_parts` was 8.53% and its scanning subloops were in the top five.
The full `wide 1024 3000 pipeline` profile recorded 2,572 samples with zero lost samples and
placed `NodeIdIndex::get_with_hash` at **3.10%** self.

Negative-evidence triage closed the obvious alternatives before edit: the identifier LUT is a
recorded ~0-gain reject; the forbidden-character edge LUT and single operator scan are already
landed; `write_uint_into`/`write_fixed2` variants are ledger-closed. The fresh candidate was a
verified two-entry temporal cache in `IrBuilder::intern_edge_endpoint_pretrimmed`, avoiding the
Swiss/Fx table probe when either of the last two endpoint IDs matches by hash and exact node-id
bytes. It does not alter insertion, collision handling, iteration order, or IR ownership.

## Measured opportunity

The pinned corpus's actual endpoint lookup stream has the following exact two-entry reuse rates:

| corpus item | endpoint lookups | exact cache hits | hit rate |
|---|---:|---:|---:|
| `flowchart_large_500` | 998 | 498 | 49.9% |
| `wide_16x32` | 1,920 | 959 | 49.9% |
| `dense_dag_200` | 1,580 | 592 | 37.5% |
| `cyclic_scc_100` | 390 | 99 | 25.4% |

This is a temporal-locality primitive, distinct from the already-landed hash-once, one-probe
insert, FxHashMap, and hash-keyed collision-bucket work.

## Fresh pinned head-to-head baseline

The Rust example was built fail-closed through RCH on `hz1` with target dir
`/data/projects/.rch-targets/frankenmermaid-cod-b`; the harness used mermaid-js **11.15.0**,
Chromium 150, strict security, pinned corpus SHA-256 inputs, output validation, and CPU0 for the
Rust side.

| item | fm p50 | fm min | fm MAD | fm CV | mermaid p50 | speedup p50 | speedup min |
|---|---:|---:|---:|---:|---:|---:|---:|
| `flowchart_large_500` | 336.275 us | 332.821 us | 0.52% | 5.69% | 1,339.7 ms | 3,983.94x | 3,685.46x |
| `wide_16x32` | 572.200 us | 562.246 us | 0.87% | 5.92% | 3,012.2 ms | 5,264.24x | 5,206.80x |

The Rust MAD gate passed both rows. These are baseline comparator numbers only, not a candidate
speedup claim; elevated host load and CV above 5% mean they cannot satisfy a KEEP gate by
themselves.

## Two independent blockers before edit

1. The mandated literal command
   `CARGO_TARGET_DIR=/data/projects/.rch-targets/frankenmermaid-cod-b rch exec -- cargo bench -p frankenmermaid-cli --bench pipeline_bench --release -- --warm-up-time 1 --measurement-time 2`
   was attempted fail-closed and Cargo rejected it before compilation/timing with
   `error: unexpected argument '--release' found`. No alternate bench form was silently scored.
2. Agent Mail reports both candidate implementation files, `crates/fm-parser/src/ir_builder.rs`
   and `crates/fm-parser/src/mermaid_parser.rs`, exclusively reserved by `CopperCliff` until
   2026-07-22T19:17:31Z. `643b7cd4` landed that agent's six-file WIP, but repeated normal/high
   priority release requests received no reply. The coordination contract forbids editing through
   an exclusive conflict. Peer-owned `fm-core` and `fm-layout` working-tree changes were untouched.

## Verdict and retry predicate

**SURFACE / BLOCKER — no candidate source was edited, so this is not a REJECT and no performance
claim is made.** Retry only after both parser leases are released (or expire) and the benchmark
invocation is corrected by the owner to Cargo's accepted release-profile syntax. Then implement
only the two-entry exact-verified endpoint cache and run interleaved same-worker `A/B/null/B/A`
with both arms and a null control in one binary; require every scored arm CV <5%, candidate/original
median <=0.97 on `flowchart_large_500` or `wide_16x32`, no pinned neighbor regression, exact IR/SVG
byte identity, parser tests, conformance, check, Clippy, rustfmt, and a fresh pinned mermaid-js ratio.
