# Flat-CSR barycenter incidence — measured attempts

## Attempt 1 — REJECTED SAMPLE (loaded-worker dispersion; lever remains open)

### Ledger-first mechanism

The void 2026-06-26 `Vec<Vec<usize>>` adjacency row explicitly reopens a dense/CSR retry once a real
Sugiyama profile names barycenter reorder. The certified post-single-pass profile does: CAND
`reorder_rank_by_barycenter::<true, true>` was 76.84% self-time on `cyclic_scc_100`. This attempt therefore
tests a different allocation shape: one packed offsets allocation plus one packed neighbors allocation, built
once per crossing minimization and reused across all sweeps.

### Source and exact executable

- Pre/post `crates/fm-layout/src/lib.rs` SHA-256:
  `68ed92d27e4685bd2cfc2ed17793f465b751a4649461be67c2ca5c811d7554f7`.
- Pre/post `crates/fm-layout/benches/barycenter_sweep.rs` SHA-256:
  `e89a93c1c7b1e5e0b1b2a49bb9c81a0f8e24d7e32a852cbc9b95ba4d9a391834`.
- Command: `RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- cargo bench -p fm-layout
  --bench barycenter_sweep --features bench-internals --profile release`.
- Worker: `vmi1149989` (`root@212.90.121.76`).
- The running process self-reported ELF SHA-256
  `ff60655deccf2de0a8deb07a816bc8df2a89e5264fee0d1a695a8032d11e5e8e`; SSH verification found the same
  digest, **857,696 bytes**, x86-64 PIE, not stripped.
- One binary and one invocation contained the A/A null control and the real A/B. Every round timed both arms,
  alternated first arm, black-boxed all inputs and both complete results, and folded both outputs into the
  printed checksum.

### Timing output

| input | A/A ratio | A/A `cv_pct` | A/A MAD | single-pass p50 | flat-CSR p50 | median paired A/B | A/B `cv_pct` | A/B MAD |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `cyclic_scc_100` | 0.9885x | 17.42% | 6.15% | 106.482 us | 35.954 us | 2.802x | 23.95% | 8.25% |
| `cyclic_scc_300` | 0.9870x | 13.44% | 5.83% | 871.641 us | 201.208 us | 4.549x | 20.35% | 13.26% |
| `cyclic_scc_800` | 0.9966x | 31.09% | 8.07% | 3,753.420 us | 549.610 us | 6.788x | 47.30% | 8.28% |

All three directional effects are much larger than the null-control departure from 1.0, but neither the null
control nor A/B clears the strict `<5%` CV rule. A concurrent `frankenpandas` Cargo/rustc job was live on the
worker during the run. These rows are routing evidence only and carry no performance verdict.

### Exact-ELF profile integrity

The same ELF self-reported its digest in both profile modes. `perf record -F 2500 -e cycles:u
--call-graph=dwarf` recorded about 5K samples per arm with zero lost samples:

- ORIG single-pass: 60,000 iterations, 90,323 ns/invocation; target
  `reorder_rank_by_barycenter::<true, true, false>` = **76.57% self-time**.
- CAND flat CSR: 180,000 iterations, 33,029 ns/invocation; target
  `reorder_rank_by_barycenter::<true, true, true>` = **36.97% self-time**.

ORIG flat frames at or above 0.10% self-time:

| self | frame |
|---:|---|
| 76.57% | `reorder_rank_by_barycenter::<true,true,false>` |
| 8.67% | `total_crossings` |
| 2.22% | `malloc` |
| 1.83% | `cfree` |
| 1.76% | `nodes_by_rank` |
| 0.98% | unresolved `0x198b66` |
| 0.97% | `crossing_minimization_single_pass` |
| 0.91% | scored-node insertion sort |
| 0.79% | `count_inversions` |
| 0.57% | unresolved `0x198a85` |
| 0.29% | `BTreeMap::VacantEntry::insert_entry` |
| 0.27% | `RawVecInner::finish_grow` |
| 0.22% | crossing-pair insertion sort |
| 0.21% | `realloc` |
| 0.20% | `BTreeMap::bulk_build_from_sorted_iter` |
| 0.20% | `drop_glue<BTreeMap>` |
| 0.17% | second `RawVecInner::finish_grow` |
| 0.14% | unresolved `0x198a74` |
| 0.13% | `RawVec<Reverse<_>>::grow_one` |
| 0.13% | `RawVecInner::grow_amortized` |
| 0.11% | `BTreeMap::IntoIter::dying_next` |

CAND flat frames at or above 0.10% self-time:

| self | frame |
|---:|---|
| 36.97% | `reorder_rank_by_barycenter::<true,true,true>` |
| 24.20% | `total_crossings` |
| 5.47% | `nodes_by_rank` |
| 3.98% | `malloc` |
| 3.52% | `BarycenterScratch::new::<true,true>` (includes CSR build) |
| 2.71% | `cfree` |
| 2.56% | unresolved `0x198b66` |
| 2.42% | scored-node insertion sort |
| 2.42% | `crossing_minimization_flat_csr` |
| 2.01% | unresolved `0x198a85` |
| 1.97% | `count_inversions` |
| 0.78% | `BTreeMap::bulk_build_from_sorted_iter` |
| 0.72% | `RawVecInner::finish_grow` |
| 0.54% | crossing-pair insertion sort |
| 0.49–0.12% | unresolved allocator/code addresses (24 frames) |
| 0.40% | second `RawVecInner::finish_grow` |
| 0.32% | `realloc` |
| 0.32% | `BTreeMap::IntoIter::dying_next` |
| 0.28% | `drop_glue<BTreeMap>` |
| 0.25% | `RawVec<Reverse<_>>::grow_one` |
| 0.19% | `BTreeMap::VacantEntry::insert_entry` |
| 0.18% | rank-key `Vec::from_iter` |
| 0.11% | `RawVec<f64>::grow_one` |
| 0.11% | `__rust_alloc` |

Profile files are retained without deletion:

- `/tmp/cod_fm_csr_invalid_orig_ff60655d_vmi1149989.perf.data`
- `/tmp/cod_fm_csr_invalid_cand_ff60655d_vmi1149989.perf.data`

### Verdict and retry condition

**REJECT THE SAMPLE, NOT THE LEVER.** The exact function executes in both arms and direction is consistently
large, but the loaded worker makes the timing inadmissible. Retry this unchanged one-lever source only when the
selected worker has no concurrent Cargo/rustc/benchmark process, and keep only a run whose same-invocation A/A
null control and real A/B both have `cv_pct < 5`.
