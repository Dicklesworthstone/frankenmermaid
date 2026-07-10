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

## Attempt 2 — REJECTED SAMPLE (2 ms sampler floor is below worker noise)

Attempt 2 satisfied attempt 1's worker-quiescence condition: `vmi1264463` (`root@38.242.209.154`) had no other
RCH job during the run and no Cargo, rustc, or benchmark process after it. The production and benchmark source
hashes remained exactly the same as attempt 1.

- Exact self-reporting and SSH-verified ELF: **857,696 bytes**, SHA-256
  `291a78bbe9695abaa23318192fc29c5c53db3fd14a0ed28f7b8a5de18208bc9b`, x86-64 PIE, not stripped.
- Same command, 41 paired alternating rounds, input/output `black_box`, checksums, and same-invocation A/A.

| input | A/A ratio | A/A `cv_pct` | A/A MAD | single-pass p50 | flat-CSR p50 | median paired A/B | A/B `cv_pct` | A/B MAD |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `cyclic_scc_100` | 1.0016x | 12.82% | 6.00% | 187.635 us | 70.331 us | 2.573x | 15.11% | 12.34% |
| `cyclic_scc_300` | 0.9704x | 12.08% | 8.19% | 1,262.189 us | 326.463 us | 3.979x | 9.07% | 3.46% |
| `cyclic_scc_800` | 1.0020x | 11.78% | 7.65% | 7,066.016 us | 893.422 us | 7.845x | 10.03% | 6.68% |

The worker is no longer the confound, but the sampler calibrates the faster arm to only 2 ms. A single scheduler
interrupt is therefore a large fraction of a sample. The A/A ratios center near 1.0 while their CV remains
11.78–12.82%, naming sampler duration rather than implementation variance as the mechanism.

Exact-ELF profiles on the quiescent worker again prove both arms execute:

- ORIG: 40,000 iterations, 187,354 ns/invocation, about 7K samples / 0 lost; target
  `reorder_rank_by_barycenter::<true,true,false>` **69.64% self-time**.
- CAND: 100,000 iterations, 72,462 ns/invocation, about 7K / 0 lost; target
  `reorder_rank_by_barycenter::<true,true,true>` **25.78% self-time**.

| ORIG self | named frame |
|---:|---|
| 69.64% | `reorder_rank_by_barycenter::<true,true,false>` |
| 11.94% | `total_crossings` |
| 3.45% | `malloc` |
| 2.39% | `nodes_by_rank` |
| 1.90% | `cfree` |
| 1.44% | `crossing_minimization_single_pass` |
| 0.91% | `count_inversions` |
| 0.79% | scored-node insertion sort |
| 0.29% | `BTreeMap::bulk_build_from_sorted_iter` |
| 0.21% | `realloc` |
| 0.21%, 0.20% | `RawVecInner::finish_grow` |
| 0.20% | crossing-pair insertion sort |
| 0.16% | `BTreeMap::VacantEntry::insert_entry` |
| 0.14% | `BTreeMap::IntoIter::dying_next` |
| 0.13% | `drop_glue<BTreeMap>` |
| 0.11% | `RawVec<Reverse<_>>::grow_one` |
| 0.89–0.11% | unresolved code addresses (9 frames) |

| CAND self | named frame |
|---:|---|
| 27.05% | `total_crossings` |
| 25.78% | `reorder_rank_by_barycenter::<true,true,true>` |
| 6.19% | `malloc` |
| 4.87% | `nodes_by_rank` |
| 4.22% | `cfree` |
| 3.75% | `crossing_minimization_flat_csr` |
| 3.10% | `count_inversions` |
| 2.24% | `BarycenterScratch::new::<true,true>` |
| 2.06% | scored-node insertion sort |
| 0.96% | `realloc` |
| 0.92% | `BTreeMap::bulk_build_from_sorted_iter` |
| 0.51%, 0.39% | `RawVecInner::finish_grow` |
| 0.48% | `drop_glue<BTreeMap>` |
| 0.44% | `BTreeMap::VacantEntry::insert_entry` |
| 0.36% | crossing-pair insertion sort |
| 0.31% | `RawVec<Reverse<_>>::grow_one` |
| 0.22%, 0.21% | `BTreeMap::IntoIter::dying_next` |
| 0.21% | `RawVecInner::grow_amortized` |
| 0.13% | `barycenter_sweep::time_arm` |
| 2.18–0.10% | unresolved code addresses (20 frames) |

Retained profiles:

- `/tmp/cod_fm_csr_invalid2_orig_291a78bb_vmi1264463.perf.data`
- `/tmp/cod_fm_csr_invalid2_cand_291a78bb_vmi1264463.perf.data`

**REJECT THE SAMPLE, NOT THE LEVER.** Retry condition: leave production code and the paired algorithm unchanged,
raise only `MIN_SAMPLE` from 2 ms to 20 ms, then require A/A and A/B `cv_pct < 5` in the same invocation.

## Attempt 3 — REJECTED SAMPLE (20 ms still exposes a co-tenant benchmark)

RCH selected `hz2` (`root@178.104.77.29`) while a separate `fnp-python` benchmark had already occupied the
worker for more than ten minutes. The 20 ms sampler reduced dispersion but did not clear the gate.

- Production SHA-256 remained `68ed92d2...d7554f7`; the only source change from attempt 2 was bench
  `MIN_SAMPLE = 20 ms`, producing bench SHA-256
  `302420c7c5b6f3291178c21169455f60ca025550ebd6759267a9ffa622b59099`.
- Exact running ELF: **858,832 bytes**, SHA-256
  `adcd806a20f7c01192a0d69d3f573373d079c57e088d39e7d10efae16ef83570`, x86-64 PIE, not stripped.

| input | A/A ratio | A/A `cv_pct` | A/A MAD | single-pass p50 | flat-CSR p50 | median paired A/B | A/B `cv_pct` | A/B MAD |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `cyclic_scc_100` | 1.0007x | 11.40% | 1.31% | 84.185 us | 29.686 us | 2.800x | 7.40% | 2.61% |
| `cyclic_scc_300` | 0.9955x | 13.51% | 2.99% | 653.662 us | 108.420 us | 6.087x | 19.11% | 4.93% |
| `cyclic_scc_800` | 0.9872x | 8.23% | 2.63% | 3,773.937 us | 482.243 us | 7.774x | 12.04% | 3.39% |

`hz2` has no `perf`; the exact ELF was streamed without a local artifact to `vmi1264463`, re-verified there by
size, SHA-256, and build ID, and retained at `/tmp/cod_fm_barycenter_adcd806a20f7c01192a0d69d3f573373`.
Exact-ELF profiles there produced 10K/12K samples with zero lost:

- ORIG: target `reorder_rank_by_barycenter::<true,true,false>` **68.36% self-time**.
- CAND: target `reorder_rank_by_barycenter::<true,true,true>` **25.93% self-time**; CSR construction 3.11%.

Ranked named frames at or above 0.10%:

| ORIG self | frame |
|---:|---|
| 68.36% | `reorder_rank_by_barycenter::<true,true,false>` |
| 12.57% | `total_crossings` |
| 3.75% | `malloc` |
| 1.81% | `nodes_by_rank` |
| 1.64% | `crossing_minimization_single_pass` |
| 1.61% | `cfree` |
| 1.05% | `count_inversions` |
| 0.85% | scored-node insertion sort |
| 0.38% | `BTreeMap::bulk_build_from_sorted_iter` |
| 0.32% | `realloc` |
| 0.25%, 0.20% | `RawVecInner::finish_grow` |
| 0.20% | `BTreeMap::IntoIter::dying_next` |
| 0.18% | crossing-pair insertion sort |
| 0.15% | `RawVec<Reverse<_>>::grow_one` |
| 0.12% | `BTreeMap::VacantEntry::insert_entry` |
| 0.11% | `barycenter_sweep::time_arm` |
| 0.82–0.13% | unresolved code addresses (8 frames) |

| CAND self | frame |
|---:|---|
| 27.11% | `total_crossings` |
| 25.93% | `reorder_rank_by_barycenter::<true,true,true>` |
| 6.29% | `malloc` |
| 4.70% | `nodes_by_rank` |
| 4.02% | `cfree` |
| 3.15% | `crossing_minimization_flat_csr` |
| 3.11% | `BarycenterScratch::new::<true,true>` |
| 2.60% | `count_inversions` |
| 2.14% | scored-node insertion sort |
| 0.84% | `BTreeMap::bulk_build_from_sorted_iter` |
| 0.79% | `realloc` |
| 0.58%, 0.55% | `RawVecInner::finish_grow` |
| 0.49% | `BTreeMap::VacantEntry::insert_entry` |
| 0.41% | crossing-pair insertion sort |
| 0.38% | `drop_glue<BTreeMap>` |
| 0.32%, 0.28% | `BTreeMap::IntoIter::dying_next` |
| 0.26% | `RawVec<Reverse<_>>::grow_one` |
| 0.20% | `barycenter_sweep::time_arm` |
| 0.18% | `RawVecInner::grow_amortized` |
| 2.44–0.10% | unresolved code addresses (16 frames) |

Profiles retained on `vmi1264463`:

- `/tmp/cod_fm_csr_invalid3_orig_adcd806a_vmi1264463.perf.data`
- `/tmp/cod_fm_csr_invalid3_cand_adcd806a_vmi1264463.perf.data`

**REJECT THE SAMPLE, NOT THE LEVER.** Retry condition: change only the bench sampler floor from 20 ms to
200 ms so each paired observation amortizes co-tenant scheduling; production and paired logic stay unchanged.

## Attempt 4 — REJECTED SAMPLE (200 ms whole-arm batches remain phase-sensitive)

RCH selected `vmi1227854` (`root@109.123.245.77`) alongside a long-running `fnp-python` benchmark. Raising the
faster-arm floor to 200 ms narrowed the 800-node row to the gate boundary, but whole-arm batches still expose
co-tenant phase changes asymmetrically.

- Production hash unchanged; bench SHA-256 with `MIN_SAMPLE = 200 ms`:
  `85fe892f865b90bd56548973a3708494c564b4364a819800080120724c6a541b`.
- Exact running/SSH-verified ELF: **856,208 bytes**, SHA-256
  `104445ca4d1d43b852c5572d92cf789b329108210920404b5766274e3e7bb74b`, x86-64 PIE, not stripped.

| input | A/A ratio | A/A `cv_pct` | A/A MAD | single-pass p50 | flat-CSR p50 | median paired A/B | A/B `cv_pct` | A/B MAD |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `cyclic_scc_100` | 1.0090x | 12.37% | 5.52% | 81.665 us | 31.031 us | 2.656x | 8.82% | 4.55% |
| `cyclic_scc_300` | 1.0030x | 7.77% | 6.51% | 596.688 us | 118.401 us | 5.003x | 9.05% | 4.99% |
| `cyclic_scc_800` | 0.9909x | 5.96% | 4.12% | 3,400.542 us | 527.061 us | 6.532x | 5.60% | 3.77% |

Exact-ELF profiling after the co-tenant ended produced about 13K samples per arm with zero lost:

- ORIG target `reorder_rank_by_barycenter::<true,true,false>` **74.66% self-time**.
- CAND target `reorder_rank_by_barycenter::<true,true,true>` **33.28% self-time**; CSR construction 3.32%.

| ORIG self | named frame |
|---:|---|
| 74.66% | `reorder_rank_by_barycenter::<true,true,false>` |
| 9.38% | `total_crossings` |
| 2.33% | `malloc` |
| 1.66% | `nodes_by_rank` |
| 1.52% | `cfree` |
| 1.11% | scored-node insertion sort |
| 1.10% | `crossing_minimization_single_pass` |
| 0.71% | `count_inversions` |
| 0.33% | `BTreeMap::bulk_build_from_sorted_iter` |
| 0.29% | `drop_glue<BTreeMap>` |
| 0.26%, 0.16% | `RawVecInner::finish_grow` |
| 0.24% | `realloc` |
| 0.22% | `BTreeMap::VacantEntry::insert_entry` |
| 0.21% | crossing-pair insertion sort |
| 0.12% | `BTreeMap::IntoIter::dying_next` |
| 0.12% | `RawVec<Reverse<_>>::grow_one` |
| 1.22–0.15% | unresolved code/kernel addresses (5 frames) |

| CAND self | named frame |
|---:|---|
| 33.28% | `reorder_rank_by_barycenter::<true,true,true>` |
| 23.75% | `total_crossings` |
| 5.48% | `malloc` |
| 4.36% | `nodes_by_rank` |
| 3.32% | `BarycenterScratch::new::<true,true>` |
| 2.79% | `cfree` |
| 2.67% | `crossing_minimization_flat_csr` |
| 2.58% | scored-node insertion sort |
| 1.49% | `count_inversions` |
| 0.87% | `BTreeMap::bulk_build_from_sorted_iter` |
| 0.59%, 0.56% | `RawVecInner::finish_grow` |
| 0.57% | `drop_glue<BTreeMap>` |
| 0.56% | `realloc` |
| 0.43% | `BTreeMap::VacantEntry::insert_entry` |
| 0.42% | crossing-pair insertion sort |
| 0.28% | `RawVec<Reverse<_>>::grow_one` |
| 0.26% | `RawVecInner::grow_amortized` |
| 0.20% | rank-key `Vec::from_iter` |
| 0.18%, 0.17% | `BTreeMap::IntoIter::dying_next` |
| 0.15% | `barycenter_sweep::time_arm` |
| 4.09–0.11% | unresolved code addresses (8 frames) |

Profiles retained:

- `/tmp/cod_fm_csr_invalid4_orig_104445ca_vmi1227854.perf.data`
- `/tmp/cod_fm_csr_invalid4_cand_104445ca_vmi1227854.perf.data`

**REJECT THE SAMPLE, NOT THE LEVER.** Increasing a whole-arm batch by 100x did not make it symmetric. Retry by
alternating ORIG/CAND at each invocation inside every paired sample and summing per-arm time; keep the 200 ms
floor, production code, 41-round statistic, and all black-box/checksum rules unchanged.

## Attempt 5 — WIN / KEEP (per-invocation interleaving, 6.880x at `cyclic_scc_800`)

### Retry condition and one-lever mechanism

Attempt 5 satisfied attempt 4's recorded retry condition without changing the production candidate: every paired
sample now alternates ORIG and CAND at **each individual invocation**, alternates the first arm again by round and
iteration parity, and sums per-arm nanoseconds. The code lever remains only the two-array flat CSR incidence index:
incoming and outgoing offsets plus stable edge-order neighbor slices are built once, then each rank sweep visits
only that rank's incident edges instead of rescanning all of `ir.edges`.

The ledger-first routing remains valid. The void `Vec<Vec<usize>>` adjacency row explicitly reopened a packed
CSR retry once a real Sugiyama profile named barycenter reorder. The certified single-pass profile did so at
76.84% self-time, and the fresh ORIG exact-ELF profile below independently reports 75.05%.

### Exact source, worker, binary, and substrate

- `crates/fm-layout/src/lib.rs` pre/post SHA-256:
  `68ed92d27e4685bd2cfc2ed17793f465b751a4649461be67c2ca5c811d7554f7`.
- `crates/fm-layout/benches/barycenter_sweep.rs` pre/post SHA-256:
  `88a7957e579e99856e67b1009ec51483e2e1381525b214c978e5dff2bb2b4dd5`.
- Command: `RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- cargo bench -p fm-layout
  --bench barycenter_sweep --features bench-internals --profile release`.
- Worker: `vmi1152480` (`root@109.205.181.92`). A separate `fnp-python` process was active, so worker load is
  disclosed rather than called quiescent; the source tree itself stayed hash-identical throughout measurement.
- Exact running and worker-verified executable:
  `/data/projects/frankenmermaid/.rch-target-vmi1152480-pool-100fc87c1da894108803e570d2fc138d/release/deps/barycenter_sweep-ccd51ba108b95431`,
  **858,528 bytes**, SHA-256
  **`9ba362e7d7f5243761a0f6f85638d886d711490bf3782947088048d09f86394e`**, x86-64 PIE, not stripped.
- One binary and one RCH invocation contained both the identical-arm A/A control and the real ORIG/CAND pair.
  Each of 41 rounds micro-interleaved the arms per invocation; all inputs and complete results went through
  `black_box`, and both results contributed to the printed checksum.

### Honest paired timing

| input | A/A ratio | A/A `cv_pct` | A/A MAD | single-pass p50 | flat-CSR p50 | median paired A/B | A/B `cv_pct` | A/B MAD | verdict |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `cyclic_scc_100` | 0.9982x | **4.83%** | 1.37% | 112.997 us | 41.988 us | 2.757x | 5.41% | 3.39% | corroboration only |
| `cyclic_scc_300` | 0.9976x | **3.74%** | 2.11% | 865.465 us | 240.859 us | 3.630x | 6.72% | 2.38% | corroboration only |
| `cyclic_scc_800` | 1.0006x | **3.05%** | 1.45% | 4,815.561 us | 705.610 us | **6.880x** | **4.52%** | 1.47% | **WIN / KEEP** |

The claim is exactly the 800-node row: both same-invocation null and A/B dispersion clear the strict `<5%` rule,
the ratio clears the 3% keep ratchet by orders of magnitude, and the null median is 0.06% from unity. The 100- and
300-node directions reproduce but their A/B CV values miss the gate, so they carry no keep claim.

### Exact-binary profile integrity and mechanism

Both arms were profiled from the exact SHA-256-pinned ELF on the same worker with
`perf record -F 999 -e cycles:u --call-graph=dwarf`. Both reports had zero lost samples:

- ORIG single-pass: about 6K samples, 120,333 ns/invocation; target
  `reorder_rank_by_barycenter::<true,true,false>` = **75.05% self-time**.
- CAND flat CSR: about 7K samples, 41,503 ns/invocation; target
  `reorder_rank_by_barycenter::<true,true,true>` = **36.15% self-time**; one-time CSR construction = 2.97%.

ORIG frames at or above 0.10% self-time:

| self | frame |
|---:|---|
| 75.05% | `reorder_rank_by_barycenter::<true,true,false>` |
| 9.11% | `total_crossings` |
| 3.42% | `malloc` |
| 1.78% | `nodes_by_rank` |
| 1.16% | `count_inversions` |
| 1.15% | `cfree` |
| 1.10% | scored-node insertion sort |
| 0.94% | unresolved `0x198b66` |
| 0.84% | `crossing_minimization_single_pass` |
| 0.48% | unresolved `0x198a85` |
| 0.31% | `realloc` |
| 0.28% | `drop_glue<BTreeMap>` |
| 0.27% | `BTreeMap::bulk_build_from_sorted_iter` |
| 0.24% | `RawVecInner::finish_grow` |
| 0.21% | crossing-pair insertion sort |
| 0.21% | second `RawVecInner::finish_grow` |
| 0.18% | unresolved `0x198a74` |
| 0.17% | `BTreeMap::IntoIter::dying_next` |
| 0.16% | unresolved `0x198a70` |
| 0.11% | unresolved `0xb28b1` |
| 0.11% | unresolved kernel address |
| 0.10% | pair-map `BTreeMap::IntoIter::dying_next` |

CAND frames at or above 0.10% self-time:

| self | frame |
|---:|---|
| 36.15% | `reorder_rank_by_barycenter::<true,true,true>` |
| 23.80% | `total_crossings` |
| 4.54% | `nodes_by_rank` |
| 4.17% | `malloc` |
| 3.25% | scored-node insertion sort |
| 2.97% | `BarycenterScratch::new::<true,true>` (CSR build) |
| 2.78% | `cfree` |
| 2.48% | `crossing_minimization_flat_csr` |
| 2.44% | unresolved `0x198b66` |
| 1.90% | `count_inversions` |
| 1.71% | unresolved `0x198a85` |
| 1.02% | `drop_glue<BTreeMap>` |
| 0.84% | `BTreeMap::bulk_build_from_sorted_iter` |
| 0.70% | `realloc` |
| 0.48% | `RawVecInner::finish_grow` |
| 0.46% | crossing-pair insertion sort |
| 0.41% | `BTreeMap::VacantEntry::insert_entry` |
| 0.36% | second `RawVecInner::finish_grow` |
| 0.34% | `RawVec<Reverse<_>>::grow_one` |
| 0.33% | pair-map `BTreeMap::IntoIter::dying_next` |
| 0.27% | unresolved `0x198a70` |
| 0.26% | unresolved `0x198a74` |
| 0.22% | `RawVecInner::grow_amortized` |
| 0.18% | unresolved `0xb146d` |
| 0.17% | unresolved `0x19504` |
| 0.17% | unresolved `0xb3344` |
| 0.16% | rank-key `Vec::from_iter` |
| 0.16% | unresolved `0x19500` |
| 0.16% | unresolved `0xb3086` |
| 0.15% | unresolved `0xb28b1` |
| 0.13% | unresolved `0x1952a` |
| 0.12% | unresolved `0xb3354` |
| 0.12% | unresolved `0xb2e76` |
| 0.12% | unresolved `0x198aa2` |
| 0.12% | unresolved `0xb2784` |
| 0.11% | unresolved `0xb4c2d` |
| 0.11% | unresolved `0xb334e` |
| 0.10% | unresolved `0xb1383` |

Allocator traffic does **not** dominate this live Sugiyama target: ORIG direct allocator frames are 3.42%
`malloc`, 1.15% `cfree`, and 0.31% `realloc`, versus 75.05% in the barycenter reorder. The causal mechanism is
therefore the packed incident-edge frontier and avoided full-edge scans, not an allocation-only hypothesis.

### Behavior proof and quality gates

- The four arms (`BTreeMap`, packed rank, packed single pass, flat CSR) return exact
  `(crossing_count, ordering_by_rank)` equality across narrow, wide, threshold, degenerate, acyclic, and heavily
  cyclic fixtures. A structural test also proves stable per-node edge order, parallel-edge multiplicity, self-edge
  handling, and invalid-endpoint filtering in both CSR directions.
- The timed harness asserts exact ORIG/CAND full-result equality before every case. Production uses the flat-CSR
  arm, with the exact single-pass fallback if packed offsets cannot be represented.
- Fail-closed remote `fm-layout` test: **435/435 passed**, plus doctests. An initial untouched time-budget e-graph
  test varied its strategy; the exact test passed alone and the clean full rerun passed all 435.
- Fail-closed remote all-target Clippy with `-D warnings`: clean. Direct nightly rustfmt check: clean.
- Fail-closed remote golden/conformance: FrankenTUI fixture **1/1**, stable layout checksums and repeated-run
  determinism **2/2**.

**VERDICT: WIN / KEEP.** Flat packed CSR incidence preserves deterministic output and cuts the gate-clean
`cyclic_scc_800` crossing-minimization arm by **6.880x** at **4.52% CV**, with a **3.05% CV** same-invocation null.

## Packed crossing-count follow-up — Attempt 1: REJECT BUILD / RETRY OPEN

### Ledger-first route and primitive

The post-flat-CSR exact-ELF profile ranked `total_crossings` second at 23.80% self-time. That satisfies the old
flat-table row's explicit retry condition: a new attempt is allowed only with a live CPU/allocation profile and
only if it eliminates fresh per-call tables. This candidate does so without retrying the old `Vec<Vec<_>>` shape.
After barycenter sweeps finish, it destructively reuses the persistent CSR allocations as canonical upper-to-lower
edge buckets plus a `u32` Fenwick frontier. Repeated e-graph recounts clear and refill the same storage.

### Exact source, worker, binary, and substrate

- `crates/fm-layout/src/lib.rs` pre/post SHA-256:
  `035aa493a50471bd577abe72ae7e13740aee0c9b608dcde13e485dc57b17eea6`.
- `crates/fm-layout/benches/barycenter_sweep.rs` pre/post SHA-256:
  `28332ab40d3eed3162f813296f23f88eedc10f8dbf10107b88453fcb9814a47f`.
- Command: `RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- cargo bench -p fm-layout
  --bench barycenter_sweep --features bench-internals --profile release`.
- Timing worker: `hz2` (`root@178.104.77.29`). One binary/invocation contained same-routine A/A and A/B;
  every one of 41 rounds interleaved the arms per invocation with rotating first-arm parity. Inputs and complete
  results were black-boxed and checksummed.
- The process self-reported, and SSH independently verified, a non-empty **826,208-byte**, unstripped x86-64
  PIE with SHA-256 **`eee5ce68cdc63f52428d5f98ce372cce8c64a9d6654c50c1cd55fe4c8ed8af7a`** and build ID
  `58766f3e1a7962fe711d53fdfcb6fd4f75d20bfe`.

### Honest timing

| input | batch | A/A ratio | A/A `cv_pct` | A/A MAD | flat-CSR p50 | packed-count p50 | median paired A/B | A/B `cv_pct` | A/B MAD | performance gate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `cyclic_scc_100` | 1647 | 1.0000x | 0.32% | 0.17% | 30.092 us | 27.518 us | **1.094x** | **0.61%** | 0.22% | pass |
| `cyclic_scc_300` | 1311 | 0.9996x | 0.47% | 0.29% | 136.101 us | 130.897 us | **1.043x** | **0.49%** | 0.35% | pass |
| `cyclic_scc_800` | 409 | 1.0004x | 0.34% | 0.13% | 497.571 us | 489.124 us | 1.017x | 0.49% | 0.24% | below 3% ratchet |

The 100- and 300-node rows clear both `<5%` dispersion and the 3% keep ratchet. The 800-node row is precise but
sub-ratchet corroboration only.

### Exact-ELF profile integrity

`hz2` has no `perf`, so the exact ELF was streamed directly, without a local artifact, to quiescent
`vmi1149989` (`root@212.90.121.76`), where size, SHA-256, build ID, and unstripped status were re-verified.
Both 300,000-iteration profiles used `perf record -F 999 -e cycles:u --call-graph=dwarf` and lost zero samples:

- ORIG flat CSR: 51,537 ns/invocation under profiling; **11,655 samples / 0 lost**;
  `total_crossings` = **24.06% self-time**.
- CAND packed counter: 32,553 ns/invocation under profiling; **9,582 samples / 0 lost**;
  `total_crossings_packed` = **11.51%** and `packed_crossing_edge` = **22.47% self-time**.

ORIG frames at or above 0.10% self-time:

| self | frame |
|---:|---|
| 33.92% | `reorder_rank_by_barycenter::<true,true,true>` |
| 24.06% | `total_crossings` |
| 5.16% | `nodes_by_rank` |
| 4.79% | `malloc` |
| 3.06% | unresolved libc `0x198b66` |
| 3.01% | `BarycenterScratch::new::<true,true>` |
| 2.90% | `crossing_minimization_flat_csr` |
| 2.68% | scored-node insertion sort |
| 2.59% | `cfree` |
| 1.78% | `count_inversions` |
| 1.54% | unresolved libc `0x198a85` |
| 0.92% | `BTreeMap::bulk_build_from_sorted_iter` |
| 0.71% | `realloc` |
| 0.61% | `RawVecInner::finish_grow` |
| 0.48% | second `RawVecInner::finish_grow` |
| 0.48% | crossing-pair insertion sort |
| 0.45% | unresolved libc `0x198a70` |
| 0.45% | `BTreeMap::VacantEntry::insert_entry` |
| 0.43% | `drop_glue<BTreeMap>` |
| 0.43% | `RawVecInner::grow_amortized` |
| 0.37% | unresolved libc `0x199504` |
| 0.33% | `RawVec<Reverse<_>>::grow_one` |
| 0.32% | unresolved libc `0x198a74` |
| 0.29% | unresolved libc `0x199500` |
| 0.29% | pair-map `BTreeMap::IntoIter::dying_next` |
| 0.21% | rank-key `Vec::from_iter` |
| 0.20% | `barycenter_sweep::time_arm` |
| 0.17% | unresolved libc `0xb146d` |
| 0.15% | position-map `BTreeMap::IntoIter::dying_next` |
| 0.14% | unresolved libc `0x198b33` |
| 0.13% | unresolved libc `0xb2bd1` |
| 0.13% | unresolved kernel frame |
| 0.12% | unresolved libc `0x19952a` |
| 0.12% | `RawVec<f64>::grow_one` |
| 0.12% | unresolved libc `0xb1383` |
| 0.11% | unresolved libc `0xb28b1` |
| 0.11% | unresolved libc `0xb2977` |
| 0.11% | unresolved libc `0x198aa2` |

CAND frames at or above 0.10% self-time:

| self | frame |
|---:|---|
| 39.24% | `reorder_rank_by_barycenter::<true,true,true>` |
| 22.47% | `packed_crossing_edge` |
| 11.51% | `total_crossings_packed` |
| 5.18% | `nodes_by_rank` |
| 3.81% | `malloc` |
| 3.39% | `BarycenterScratch::new::<true,true>` |
| 2.93% | scored-node insertion sort |
| 2.76% | `crossing_minimization_packed_crossings` |
| 2.48% | `cfree` |
| 0.70% | `RawVecInner::finish_grow` |
| 0.41% | `BTreeMap::VacantEntry::insert_entry` |
| 0.38% | unresolved libc `0x199500` |
| 0.32% | rank-key `Vec::from_iter` |
| 0.31% | `RawVecInner::grow_amortized` |
| 0.29% | `realloc` |
| 0.27% | unresolved libc `0x199504` |
| 0.19% | unresolved libc `0x198a70` |
| 0.14% | `RawVec<f64>::grow_one` |
| 0.13% | unresolved libc `0xb2c75` |
| 0.12% | unresolved libc `0xb146d` |
| 0.11% | unresolved libc `0x198aa2` |
| 0.10% | unresolved libc `0x199604` |
| 0.10% | unresolved libc `0x198a74` |

Retained remote profiles:

- `vmi1149989:/tmp/cod_fm_packed_cross_orig_eee5ce68_vmi1149989.perf.data`
- `vmi1149989:/tmp/cod_fm_packed_cross_cand_eee5ce68_vmi1149989.perf.data`

### Behavior and build verdict

- Five-arm exact `(crossing_count, ordering_by_rank)` equality passed on the cyclic/adversarial corpus.
- Direct packed/reference tests cover reversed and parallel edges, same-source and equal-target non-crossings,
  independent rank-pair sums, integer-boundary ranks, malformed ordering, repeated recounts, and unchanged
  vector pointers/capacities. The full fail-closed remote suite passed **437/437** plus doctests.
- Strict all-target Clippy rejected only the cursor-copy loop as `clippy::manual_memcpy`, prescribing
  `scratch.slot_of[..node_count].copy_from_slice(&normalized_offsets[..node_count])`.

**REJECT THIS EXACT BUILD; RETRY THE LEVER.** The performance and profile gates pass, but the exact source is not
quality-gate clean. Retry condition: apply only Clippy's behavior-equivalent slice copy, then rerun the full
one-binary A/A+A/B and exact-ELF per-arm profiles because the executable source hash changes.
