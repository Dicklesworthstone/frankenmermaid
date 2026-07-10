# WIN: packed node-rank lookup in the hot barycenter sweep (`bd-9w78`)

**Date:** 2026-07-10
**Agent:** `cod_fm` / StormyEagle
**Lane:** `fm-layout` crossing minimization
**Verdict:** **KEEP** — 1.310x and 1.318x on the two `cv_pct < 5` rows, with exact output parity.

## Ledger-first routing and mechanism

The four old crossing-minimization rejections were void because they used `layout_wide` inputs that
auto-select Tree and execute `reorder_rank_by_barycenter` at **0.000%** self-time. The reopening profile on
real Sugiyama `cyclic_scc_100` put `reorder_rank_by_barycenter` at **47.640%** of the full pipeline
(`fm_layout` was 70.76%); `total_crossings` was only 0.810%, so the latter remains closed.

The selected one-lever change builds one node-indexed `Vec<u32>` per `crossing_minimization` and replaces
only the three inner `ranks: BTreeMap<usize, usize>` probes. ORIG and CAND share the original narrow-rank
edge rescan, wide-rank accumulator, all per-call allocations, edge order, integer accumulation, single `f32`
division, stable sort, and deterministic tie-breaks. An exact fallback retains BTreeMap lookup if a rank cannot
fit `u32`.

## Decision-grade paired A/B

One binary and one RCH invocation ran both arms. Each of 41 samples timed ORIG and CAND back-to-back; sample
order alternated O/C then C/O. Batch size was calibrated from the faster arm. Inputs and both result values
passed through `black_box`, and a printed checksum made a dead-code-eliminated arm impossible.

```text
RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- \
  cargo bench -p fm-layout --bench barycenter_sweep --features bench-internals --profile release
```

The command ran remotely on `ovh-a` (`ubuntu@51.222.245.56`). The exact executable was verified non-empty on
that worker: **803,704 bytes**, SHA-256
`89599720a6cf301a43202c52052c8cbb879fd427cfc70ea98183b08bf8bce7bb`, ELF x86-64, not stripped.

The remote pre-build source snapshot and local post-run files matched, ruling out the prior source-swap race:

| File | SHA-256 before/after |
|---|---|
| `crates/fm-layout/src/lib.rs` | `a14d7bc3883c7d9b02f7bdc2e108e07a520a7a93a79eb68109656c0bb3b264a6` |
| `crates/fm-layout/benches/barycenter_sweep.rs` | `6d02a6a5048a231e2e5adcffa7dbfb28f96f1d38ff17fe6afffda7c61df1e056` |

| Input | ORIG p50 | CAND p50 | ORIG/CAND | `cv_pct` | MAD | Gate |
|---|---:|---:|---:|---:|---:|---|
| `cyclic_scc_100` (100 nodes, 195 edges) | 341.7 us | 260.9 us | **1.310x** | **0.69%** | 0.27% | **KEEP** |
| `cyclic_scc_300` (300 nodes, 595 edges) | 2,935.1 us | 2,219.0 us | 1.323x | 7.29% | 0.22% | corroboration only |
| `cyclic_scc_800` (800 nodes, 1,595 edges) | 19,868.0 us | 15,077.1 us | **1.318x** | **4.57%** | 0.27% | **KEEP** |

The two decision rows reduce latency by about 24%, far beyond the 3% keep-gate ratchet. The 300-node row is
not used for the verdict because its per-pair ratio CV exceeds 5%, despite agreeing in direction and magnitude.

Raw 41-pair samples (`ORIG_ns:CAND_ns`) are retained below:

```text
cyclic_scc_100 batch=7
348487:266001,347090:264768,342472:261735,341632:261597,342198:262414,343374:261165,341479:260651,347214:263141,340974:262234,342805:260877,341654:262292,342068:260794,342272:259994,344751:268372,341462:261345,341492:260004,340672:260954,341217:260197,341356:260062,340184:260313,341196:260157,349563:260068,343620:261295,341479:260324,340110:260452,349102:260615,341685:261671,342606:260250,341236:261535,341409:260566,341690:260231,343079:261597,342940:262581,340831:261243,341217:259991,341778:260565,341796:260977,341034:260615,341260:261588,340719:260492,341723:260917

cyclic_scc_300 batch=1
2956948:2219979,2953773:2219357,2976766:2225970,2943163:2220319,2938735:2220109,2943844:2216241,2937864:2225248,2941129:2214308,2939206:2221060,3540151:2701361,2947201:2228534,2931501:2212614,2928215:2218655,2980954:2202365,2982417:3129933,2943934:2220670,2925730:2217634,3781363:2249324,2930790:2220119,2928947:2214678,3735387:2234977,2947692:2219738,2928726:2215961,2931652:2214648,2926973:2215610,2930840:2213045,2932844:2221822,2923536:2213416,2928256:2216491,2935749:2208688,2937502:2219438,2935549:2219026,2929618:2263720,2935068:2225108,2932974:2222683,2920991:2201985,2909129:2203056,2921643:2211412,2909750:2204069,2931441:2209739,2914921:2208667

cyclic_scc_800 batch=1
19989334:15040844,19869339:15072312,19833511:15135762,19881121:15076321,19811109:15170286,19937517:15084135,19865311:15141111,19882984:15068215,19800810:15199651,19855022:15062084,19833621:15079927,19924963:15061042,19861924:15082371,21181376:15073274,20885482:17197384,19851495:15049380,19868026:15180075,19677629:16148609,19709449:15128408,19798767:15038890,19800349:15037998,19810548:15108030,19827911:15008884,19812192:15037738,19843390:15057526,19937707:15077994,19880659:15147584,19905115:15077123,19894836:15073765,19903292:15097300,19817662:15042246,19877804:15047827,19863397:15083364,19907340:15060331,19822822:15058757,19950751:15098663,25095318:15117968,20852510:15211434,20700165:15076911,19975798:15108451,19752560:15043359
```

## Per-arm profile integrity

Both arms were profiled sequentially on `ovh-a` using the exact benchmark ELF above, 20,000 invocations of
`cyclic_scc_100`, `cycles:u`, 2,500 Hz, DWARF call graphs. The profiles are attribution evidence only; the
paired sampler above is the timing verdict.

- ORIG: 19,743 samples, **0 lost**, `reorder_rank_by_barycenter::<false>` **93.70% self-time**.
- CAND: 16,268 samples, **0 lost**, `reorder_rank_by_barycenter::<true>` **92.13% self-time**.

Every flat frame at or above 0.10% self-time:

| ORIG self | Frame |
|---:|---|
| **93.70%** | `reorder_rank_by_barycenter::<false>` |
| 2.10% | `total_crossings` |
| 0.64% | `malloc` |
| 0.59% | `cfree` |
| 0.52% | `BTreeMap::bulk_build_from_sorted_iter` |
| 0.38% | `__memmove_avx_unaligned_erms` |
| 0.35% | `nodes_by_rank` |
| 0.28% | tuple insertion sort |
| 0.27% | `drop_glue<BTreeMap>` |
| 0.25% | `count_inversions` |
| 0.24% | `_int_malloc` |
| 0.11% | `_int_free_merge_chunk` |

| CAND self | Frame |
|---:|---|
| **92.13%** | `reorder_rank_by_barycenter::<true>` |
| 2.27% | `total_crossings` |
| 0.85% | `malloc` |
| 0.75% | `cfree` |
| 0.73% | `BTreeMap::bulk_build_from_sorted_iter` |
| 0.45% | `drop_glue<BTreeMap>` |
| 0.45% | tuple insertion sort |
| 0.45% | `nodes_by_rank` |
| 0.34% | `__memmove_avx_unaligned_erms` |
| 0.32% | `crossing_minimization_dense_rank` wrapper |
| 0.28% | `count_inversions` |
| 0.22% | `_int_malloc` |
| 0.12% | `_int_free_merge_chunk` |
| 0.11% | `_int_free_chunk` |

Direct allocator frames total only **1.58% ORIG / 2.05% CAND**. Allocation traffic therefore does not
dominate this cyclic Sugiyama kernel; the live barycenter sweep itself does. The higher candidate percentage is
share-of-a-smaller-total behavior, not evidence of increased absolute allocator work.

Remote profile data was retained without deletion at:

- `/tmp/cod_fm_bary_dense_rank_orig_89599720_root.perf.data`
- `/tmp/cod_fm_bary_dense_rank_cand_89599720_root.perf.data`

## Behavior and quality gates

- Exact differential result parity: `dense_barycenter_sweep_matches_btreemap_sweep` passed remotely (1/1),
  covering narrow, threshold, wide, degenerate, acyclic, and heavily cyclic shapes over two seeds.
- The paired harness asserts the full `(crossing_count, ordering_by_rank)` result is equal before timing every
  corpus size.
- Full remote `cargo test -p fm-layout --features bench-internals`: **434 passed, 0 failed**; doctests 1 passed,
  1 ignored.
- `cargo clippy -p fm-layout --all-targets --features bench-internals -- -D warnings` passed remotely.
- `cargo fmt --all -- --check` and `git diff --check` passed locally; neither compiles code.
- UBS ran on every changed file with its local Cargo phases disabled; its nonzero static result was the existing
  broad fm-layout heuristic baseline (including token-comparison false positives), with no unsafe-code or
  resource-lifecycle finding. Compile/lint/test coverage came from the fail-closed RCH commands above.

## Verdict

**WIN / KEEP.** The packed node-rank primitive is deterministic and clears the ratchet on both gate-clean hot
Sugiyama rows. This does not close the crossing-minimization lane: reusable packed position/slot scratch and
flat CSR incidence remain distinct primitives for later one-lever attempts.
