# Published calibration: which harness config decides an effect of size X, per function

**Date:** 2026-07-10 · **Agent:** cc_fm · **Bench:** `crates/fm-layout/benches/harness_calibration.rs`
**Worker:** `hz2` · **Bench ELF sha256:** `245f949bb88038248b248c559609f52e31b39d005c13458e7c9fc833cd7b3278`
(844,504 bytes) · **Source sha256 verified unchanged across the run** · **`total_wall` 19.8 s**

The paired null control tells you the harness's floor; this tells you **which knob setting to pick for the
effect you are chasing** — so an agent configures for its lever instead of guessing. All numbers are the
**A/A null** (same arm both sides), so every deviation is the harness, never a lever.

## Method

- Each configuration is `(arm, min_sample, min_of)`. 3 arms × 3 `min_sample` × 2 `min_of` = 18 configs.
- Configs are measured **interleaved round-robin**, one per round with a rotating start (a sequential sweep
  confounds config with load drift — the error arm-interleaving prevents, one level up).
- Per config: 41 per-round A/A ratios → median + a **percentile bootstrap 95% CI** on that median (2000
  resamples, deterministic xorshift). `min_decidable = 1 + max(|ci_hi − 1|, |ci_lo − 1|)`.

## Raw floors (95% CI on the A/A null median)

| arm | min_sample | min_of | null median | null 95% CI | null cv | min_decidable |
|---|---:|---:|---:|---|---:|---:|
| btreemap | 2 ms | 1 | 1.0343× | [0.9560, 1.0480] | 11.30% | 1.048× |
| btreemap | 2 ms | 3 | 0.9974× | [0.9876, 1.0088] | 7.40% | 1.012× |
| btreemap | 10 ms | 1 | 1.0046× | [0.9900, 1.0075] | 7.66% | 1.010× |
| btreemap | 10 ms | 3 | 0.9983× | [0.9962, 0.9995] | 4.33% | 1.004× |
| btreemap | 40 ms | 1 | 1.0095× | [0.9977, 1.0180] | 16.40% | 1.018× |
| btreemap | 40 ms | 3 | 1.0007× | [0.9979, 1.0044] | 7.50% | 1.004× |
| dense_rank | 2 ms | 1 | 1.0089× | [0.9905, 1.0229] | 6.13% | 1.023× |
| dense_rank | 2 ms | 3 | 0.9987× | [0.9921, 1.0029] | 2.37% | 1.008× |
| dense_rank | 10 ms | 1 | 1.0034× | [0.9969, 1.0094] | 2.88% | 1.009× |
| dense_rank | 10 ms | 3 | 1.0005× | [0.9971, 1.0032] | 9.66% | 1.003× |
| dense_rank | 40 ms | 1 | 0.9959× | [0.9890, 1.0010] | 9.85% | 1.011× |
| dense_rank | 40 ms | 3 | 1.0024× | [0.9977, 1.0073] | 4.24% | 1.007× |
| single_pass | 2 ms | 1 | 0.9934× | [0.9693, 1.0334] | 10.39% | 1.033× |
| single_pass | 2 ms | 3 | 0.9994× | [0.9957, 1.0036] | 3.09% | 1.004× |
| single_pass | 10 ms | 1 | 1.0000× | [0.9948, 1.0062] | 4.73% | 1.006× |
| single_pass | 10 ms | 3 | 0.9986× | [0.9946, 1.0014] | 5.92% | 1.005× |
| single_pass | 40 ms | 1 | 1.0070× | [1.0001, 1.0157] | 10.96% | 1.016× |
| single_pass | 40 ms | 3 | 1.0013× | [0.9976, 1.0040] | 4.33% | 1.004× |

## PUBLISHED SETTINGS — cheapest config that decides an effect of size X

Decidable = the claimed effect exceeds the floor by a **2× margin** (`X ≥ 1 + 2·half_width`), not merely by an
epsilon. "Cost" is `min_sample × min_of` (total sample time). Read the row for **the function you are about to
benchmark** — the floor is per-function.

| function | 1.02× | 1.05× | 1.10× | 1.25× | 1.50× |
|---|---|---|---|---|---|
| `btreemap` | 10 ms / ×1 | 2 ms / ×3 | 2 ms / ×1 | 2 ms / ×1 | 2 ms / ×1 |
| `dense_rank` | 2 ms / ×3 | 2 ms / ×1 | 2 ms / ×1 | 2 ms / ×1 | 2 ms / ×1 |
| `single_pass` | 2 ms / ×3 | 2 ms / ×3 | 2 ms / ×1 | 2 ms / ×1 | 2 ms / ×1 |

Reading it: a **≥ 1.10× claim is decidable in the cheapest possible config** (`2 ms / ×1`) for every function
here. A **1.02× claim needs `min_of = 3`** (or a longer sample for `btreemap`), and even then sits close to the
floor — treat sub-1.05× wins in this lane with suspicion regardless of config. **Nothing sub-1.01× is decidable
at all** on this hardware; do not claim it.

## Three findings that overturn the naive defaults

1. **`min_of` is the dominant knob, not `min_sample`.** At 2 ms, going `×1 → ×3` moves the floor
   `1.048× → 1.012×` (btreemap), `1.023× → 1.008×` (dense_rank), `1.033× → 1.004×` (single_pass). The minimum of
   3 back-to-back timings discards one-sided preemption outliers, which is exactly what a longer sample cannot do.
2. **`min_sample` beyond ~10 ms buys nothing** — and 40 ms can be *worse* (`btreemap` 10 ms/×1 = 1.010× vs
   40 ms/×1 = 1.018×), because a longer sample is a bigger target for a preemption. The `barycenter_sweep`
   default of `min_sample = 2 ms, min_of = 1` is the **worst** cell in the table for two of three functions;
   adding `min_of = 3` is a two-line change that roughly quarters the floor.
3. **`cv` does not track decidability.** `dense_rank 2 ms/×3` has `cv 2.37%` and floor `1.008×`; `dense_rank
   10 ms/×3` has `cv 9.66%` — 4× worse — yet a *better* floor `1.003×`. Gating on `cv` would have picked the
   wrong config. This is the empirical proof that **the median-CI floor, not `cv`, is the gate.**

## What this certifies about the existing wins

- Single-pass **3.669×** and dense-rank **1.310×** clear even the *worst* (`btreemap 2 ms/×1`, 1.048×) floor by
  ~78× and ~6.5× respectively. Certified beyond any configuration question.
- Recommended default for this lane going forward: **`min_sample = 2 ms, min_of = 3`** — floor ≤ 1.012× for
  every function, whole 18-config sweep costs 19.8 s.

## Reproduce

```
RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- \
  cargo bench --profile release -p fm-layout --features bench-internals --bench harness_calibration
```

The bench prints its own ELF sha256 as line 1. Bracket with a source hash and re-check at `git add` time;
concurrent editors on a shared tree are the one hazard this cannot see.
