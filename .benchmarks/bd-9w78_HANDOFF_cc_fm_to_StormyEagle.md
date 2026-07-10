# bd-9w78 HANDOFF — cc_fm ⇒ StormyEagle (cod)

**Date:** 2026-07-10 · **From:** cc_fm (render/a11y lane) · **To:** StormyEagle (`bd-9w78 dense-rank barycenter A/B and profile`)

> Sent as a file because **MCP agent-mail is DOWN**: *"Storage write-back queue has unrecoverable errors (1 op
> failed after retries); refusing to accept new messages until operator clears the durability flag via
> `am doctor repair`."* Operator: that flag needs clearing.

**You own `bd-9w78` execution. I am OFF `crates/fm-layout/**` for the rest of this session.**

I saw your in-flight refactor to a single const-generic `reorder_rank_by_barycenter<const DENSE_RANK>` /
`crossing_minimization_sweeps<const DENSE_RANK>`. That is a **better single-variable design** than the
two-function split I committed in `ddaa6f2`. Keep yours. I am not reverting or re-editing anything.

---

## Already landed (`ddaa6f2` + `f09da9a` on `main`), yours to build on

- `crossing_minimization_impl<const DENSE_RANK: bool>`, with the pre-`bd-9w78` `BTreeMap` sweep kept as a
  **live reference arm**.
- `bench_internals` behind feature `bench-internals` (`prepare_ranks`, `crossing_minimization_btreemap`,
  `crossing_minimization_dense_rank`).
- `crates/fm-layout/benches/barycenter_sweep.rs` — paired sampler on the exact `cyclic_scc_100` ring-of-five topology.
- `dense_barycenter_sweep_matches_btreemap_sweep` — differential equality test across narrow ranks (the old
  per-node rescan branch), wide ranks (the single-pass branch), the exact threshold, degenerate one-node ranks,
  heavily cyclic graphs, two seeds.
- **Gates green:** `cargo test -p fm-layout` 434 passed; `golden_layout_test` 2/2; `frankentui_conformance_test`
  green; `cargo fmt --check` clean; `ubs` 152→154 criticals (both new ones in the pre-existing `==`/`!=`
  "secret compared with" false-positive family).

**I deliberately did NOT claim a perf ratio.** Please don't cite one until the three traps below are cleared.

## The self-time that licenses this lever — record it in whatever you write

`perf record -F 2500 --call-graph=dwarf`, self-time as % of the whole parse+layout+render pipeline:

| input | `reorder_rank_by_barycenter` | `total_crossings` | `bk_horizontal_compaction::place_block` |
|---|---:|---:|---:|
| `wide_8x16` (= `layout_wide/8x16`) | **0.000%** | 0.000% | — |
| `wide_16x32` (= `layout_wide/16x32`) | **0.000%** | 0.000% | — |
| `cyclic_scc_100` (real Sugiyama) | **47.640%** | 0.810% | 0.74% |

`fm_layout` is 70.76% of pipeline on `cyclic_scc_100`, so barycenter is **48.45% of a 70.76% frame** — the
largest attributable target in the repo. Brandes-Köpf, the old reject note's nominated "live candidate", is **cold**.

## Three traps — I hit all three

**1. `rch exec` cannot pin a worker, and fails OPEN to a local build.**
No `--worker` flag; `RCH_WORKER=` is **silently ignored** (I passed `RCH_WORKER=hz1`; the log read
`[RCH] remote hz2` for *both* arms). An A/B split across two `rch exec` invocations is **invalid**. Without
`RCH_REQUIRE_REMOTE=1`, rch builds **locally** when slots are exhausted and drains the disk (~73 GB/h; we're at 77G).

Mandatory form for every build/bench/test:

```
RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- cargo <sub> ...
```

`env -u CARGO_TARGET_DIR` is required because `~/.zshrc` globally exports it, which makes rch bench artifact
retrieval return ~0 bytes. An rch error is a **blocker to surface**, never a license to build locally.

**2. Criterion group members run SEQUENTIALLY, not interleaved.**
Two `bench_with_input` arms in one `benchmark_group` do **not** cancel worker/thermal drift — each arm gets its
own time window. That's why `barycenter_sweep.rs` is a hand-rolled paired sampler: it times both arms
back-to-back per round, alternates which arm goes first, and reports the **median of per-round ratios** with `cv`
computed over those ratios. `black_box` the inputs *and* consume the results, then fold into a printed
`checksum` — a DCE'd arm cannot produce it.

See `calibrate()`: the batch must be sized off the **faster** arm. Sizing it off the slow arm gave `batch = 1`
and left the dense arm's samples at ~220 µs — 10× under the 2 ms floor — so `cv` was pure timer noise.

**3. ⚠️ SOURCE-SWAP RACE — the one that actually bit me.**
Two consecutive runs of the same bench returned **4.347×** then **1.161×** on `cyclic_scc_100`. Not worker
variance: the working tree changed between runs. Run 1 measured a *wider* lever of mine (dense scratch tables +
dropping `SINGLE_PASS_RANK_THRESHOLD` so every rank width takes the single edge pass); run 2 measured the
narrower `DENSE_RANK`-only lever. Earlier, the same file silently lost 137 lines between a verified `sha256sum`
and the next build.

**`sha256sum crates/fm-layout/src/lib.rs` before AND after every bench run. If it moved, discard the ratio.**
Since we were both live in this file, **every number I took is an anecdote, not a measurement.**

## Still un-measured, and the one lead worth chasing

- **`cv_pct` fails the gate.** 41 rounds, interleaved: `cyclic_scc_100` 1.161× (cv **13.36%**), `_300` 1.164×
  (cv **16.12%**), `_800` 1.125× (cv **9.45%**). MAD was 1.65–1.97% — one-sided outliers — but the stated gate is
  `cv < 5`. Needs a quiesced tree plus more rounds, or an explicitly-argued switch to a min/MAD statistic.
- **Per-arm perf self-time is currently unobtainable.** `rch` does not retrieve bench binaries and local builds
  are banned, so `perf` cannot see the bench. `FM_BARYCENTER_PROFILE_ARM` yields per-arm ns/iter — arm *cost*,
  not perf self-time. Say so plainly rather than implying otherwise.
- **The bigger lever, un-measured.** The narrow-rank branch (`SINGLE_PASS_RANK_THRESHOLD = 8`) rescans **all of
  `ir.edges` per node**. With ~100 nodes over ~25 ranks (~4 nodes/rank) the sweep is `O(rank_size · |E| · log|V|)`
  per call. Dense-rank alone removes the `log|V|`. **Dropping the threshold** — always take the single edge pass,
  with dense scratch for `adjacent_position` / `local_slot` — removes the `rank_size` factor too. That is the
  change that measured 4.347× once. It is **byte-identical by construction**: the two branches already compute the
  same integer position sum ÷ neighbor count, which is exactly what the threshold rested on. Worth a **second**
  const parameter so you can A/B it independently of dense-rank.

## Reservations

I hold **none** in `fm-layout`. Mine: `crates/fm-render-svg/**`, `scripts/headtohead/**`,
`crates/fm-cli/examples/**`. Yours: `crates/fm-layout/**`. If you want a corpus item re-profiled with the
symbolized binary I already have, ask — that costs no build.

## Ledger context — please don't over-read the invalidation

`.benchmarks/crossing_min_rejections_benched_dead_code.md`, plus the four invalidated REJECT rows in
`docs/NEGATIVE_EVIDENCE.md` (`1db8b00`). **Only ONE of the four is reopened.** `total_crossings` stays closed on a
0.810% ceiling; the `egraph_ordering` and `crossing_refinement` rows are **"unmeasured", not "reopened"**. Marking
all four reopened would repeat the original error from the other direction.
