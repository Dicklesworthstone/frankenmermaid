# WIN: always-single-pass barycenter sweep with packed position/slot scratch (`bd-9w78` follow-up)

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** `f8e6ce3` (cod's certified dense-rank win)
**Verdict:** **KEEP** — **3.591×** on the one `cv_pct < 5` row, exact output parity, determinism gates green.
**Lane:** owned solo (cod usage-walled). Source hash pinned before *and* after every run.

---

## Ledger-first

- The four old crossing-minimization rejections are **void** (they A/B'd `layout_wide`, where
  `reorder_rank_by_barycenter` has **0.000%** self-time — the auto-selector picks Tree). See
  `.benchmarks/crossing_min_rejections_benched_dead_code.md`.
- `f8e6ce3` (cod) certified the **dense-rank** primitive: 1.310× / 1.318× on two `cv < 5` rows. **I independently
  recomputed its statistics from the raw 41-pair samples published in `barycenter_dense_rank_WIN.md` and they
  reproduce exactly** (1.310× cv 0.69%, 1.323× cv 7.29%, 1.318× cv 4.57%). That win stands.
- cod's own note left this open: *"reusable packed position/slot scratch and flat CSR incidence remain distinct
  primitives for later one-lever attempts."* This is the first of those.

## The lever (one)

The sweep had **two** shapes selected by `SINGLE_PASS_RANK_THRESHOLD = 8`:

- **narrow ranks** — for *each node* of the current rank, rescan **all of `ir.edges`** ⇒ `O(rank_size · |E|)` per call;
- **wide ranks** — one accumulating pass over `ir.edges` ⇒ `O(|E| + rank_size)`, but with two per-call `BTreeMap`s
  (`adjacent_position`, `local_slot`) whose setup dominated at small widths. *That is the only reason the
  threshold existed.*

On `cyclic_scc_100` the rank width is ~4, so the narrow branch — the one that actually runs — pays a **4× edge
rescan**. The lever removes the setup cost with a node-indexed `BarycenterScratch` (`position_of`, `slot_of`,
`accumulators`) allocated **once per `crossing_minimization`** and reset in O(rank width) per call, then takes the
single accumulating pass **for every rank width**. Zero allocations after construction. `SINGLE_PASS` is a second
const parameter, so `DENSE_RANK` stays exactly as certified and the arms differ in one variable.

**Output-identical by construction.** The pre-existing code already contained both shapes and selected between
them on the explicit premise (old comment; KEPT `single_pass_barycenter.md`) that they *"compute the identical
result (integer position sum divided once by the neighbor count)"*. Accumulation order over `ir.edges` is
unchanged, sums are `usize`, so the `f32` barycenters and the stable sort are bit-for-bit unchanged.

## Decision-grade paired A/B

One binary, one `rch` invocation, both arms **interleaved inside a single measured routine** — 41 rounds, each
round timing `dense_rank` and `single_pass` back-to-back with **alternating order** (O/C then C/O), batch
calibrated off the *faster* arm, inputs and both results through `black_box`, folded into a printed checksum an
eliminated arm could not produce. The statistic is the **median of per-round ratios**; `cv` is over those ratios.

```
RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- \
  cargo bench --profile release -p fm-layout --features bench-internals --bench barycenter_sweep
```

**Source hash pinned before and after both runs — unchanged.** (`lib.rs`
`df464d8971ade674ce8665ea296fa33d4666447af641b25ce4ff70ad2aa1c70b`, bench
`0ccde29bb95cae75501878dd50fc1804ea947ab3dbb9fc155af43b913089a429`.) This is the guard the earlier source-swap
race demanded.

### Run 1 — worker `hz2` (95 s wall)

| input | dense p50 | single p50 | **ratio** | `cv_pct` | MAD | gate |
|---|---:|---:|---:|---:|---:|---|
| `cyclic_scc_100` (100 n / 195 e) | 327.9 µs | 91.6 µs | **3.591×** | **4.13%** | 1.44% | **KEEP** |
| `cyclic_scc_300` (300 n / 595 e) | 2,792.7 µs | 664.8 µs | 4.211× | 15.32% | 1.42% | corroboration |
| `cyclic_scc_800` (800 n / 1,595 e) | 19,641.5 µs | 4,011.6 µs | 4.874× | 6.08% | 2.43% | corroboration |

### Run 2 — worker `vmi1152480` (247 s wall — slow and loaded)

| input | dense p50 | single p50 | ratio | `cv_pct` | MAD | gate |
|---|---:|---:|---:|---:|---:|---|
| `cyclic_scc_100` | 413.7 µs | 111.7 µs | 3.851× | 68.24% | 7.63% | unusable |
| `cyclic_scc_300` | 3,543.9 µs | 727.5 µs | 4.871× | 17.50% | 5.81% | unusable |
| `cyclic_scc_800` | 24,577.0 µs | 4,096.4 µs | 5.986× | 15.97% | 2.29% | unusable |

**The claim is exactly one row: `cyclic_scc_100`, 3.591×, `cv_pct` 4.13% < 5.** Everything else is corroboration.

Honest reading of run 2: `rch` **cannot pin a worker**, so I could not choose. Run 2 landed on a machine 2.6×
slower in wall time with `cv` up to 68% — its samples cannot carry a `cv`-gated claim. What it *does* show is that
the **direction and magnitude reproduce on a second, independent machine** (3.59–3.85× / 4.21–4.87× / 4.87–5.99×),
which is the property the earlier source-swap race destroyed. Note that MAD stays 1.42–2.43% in run 1: the `cv`
failures are one-sided preemption outliers, not instability in the effect.

## Self-time per arm

- **Baseline arm (`reorder_rank_by_barycenter::<true, false>`) — 92.13% self-time.** Measured by cod on the
  certified build (`barycenter_dense_rank_WIN.md`: CAND profile, 16,268 samples, 0 lost, `cycles:u`, 2500 Hz,
  DWARF). `<true>` in that build is the same code path as `<true, false>` here.
- **Candidate arm (`::<true, true>`) — perf self-time NOT obtained.** ⚠️ Stated plainly rather than implied:
  under the mandated `rch exec -- cargo …` recipe I cannot run `perf` against a remotely-built bench ELF, and
  local builds are prohibited by the disk constraint. What *is* established: the arm executes (the differential
  test calls it and asserts equality; the printed checksum cannot be produced by a DCE'd arm), it is the
  **production path** exercised by `golden_layout_test` + `frankentui_conformance_test`, and its measured cost is
  91.6 µs vs the baseline's 327.9 µs.

## Behaviour parity + gates

- `dense_barycenter_sweep_matches_btreemap_sweep` extended to **three arms**: `btreemap` ≡ `dense_rank` ≡
  `single_pass`, across narrow ranks (the old per-node rescan branch), wide ranks, the exact threshold, degenerate
  one-node ranks, acyclic and heavily cyclic shapes, over two seeds.
- The paired harness asserts full `(crossing_count, ordering_by_rank)` equality before timing every corpus size.
- `cargo test -p fm-layout --features bench-internals`: **434 passed, 0 failed** (remote, fail-closed).
- `golden_layout_test` **2/2** and `frankentui_conformance_test` green **with `SINGLE_PASS` live in production** —
  i.e. the checked-in layout goldens directly certify the new arm's deterministic output.
- `cargo fmt --check` clean.

## What is still open in this lane

`SINGLE_PASS_RANK_THRESHOLD` is now dead for the production arm (both widths take one pass) but the constant and
the narrow branch remain, because the `!SINGLE_PASS` reference arm needs them. **Flat CSR incidence** — making the
per-call work proportional to the *incident* edges rather than to `|E|` — is the next distinct primitive, and it is
the one that removes the remaining `|E|` factor. `total_crossings` stays closed on a 0.810% ceiling.
