# bd-ecjg — the bracket's floor is proportional to its own noise, and the audit needs no new measurement

Source-reading and ledger work only: the host is under a build freeze (`/data` 34G, 99%), so nothing
here was measured. Everything below is recomputed from rows already in `.benchmarks/headtohead/`.

## The bead's four numbers reproduce exactly from the archive

Two prior comments closed this as BLOCKED pending a predeclared quiet-host TRJ booking, on the
grounds that "no valid measurement can start". **The observation does not require a new measurement.**
The runs are archived, they carry `affinity_cpus`, `batch`, `cv_pct` and `null_control`, and
`fmBracket` (`run.mjs:1317`) is a pure function of two p50s and the two A/A half-widths:

```
drift  = max(after/before, before/after)
floor  = max(MIN_CLAIM_RATIO, 1 + 2 * max(half_width_before, half_width_after))   # MIN_CLAIM_RATIO = 1.01
```

Recomputing from the stored rows for ELF `08cca9e1`, case `flowchart_small_10`:

| run | drift | floor | verdict | null half-width | cv% before / after |
|---|---:|---:|:--|---:|---:|
| cpu12 `…84185757` | **1.050140** | **1.024400** | FAIL | 0.0122 | 21.93 / 18.76 |
| cpu13 `…84198917` | **1.013775** | **1.036200** | PASS | 0.0181 | 16.27 / 14.61 |
| cpu12 `…83972865` | 1.038347 | 1.020000 | FAIL | 0.0092 | 11.84 / 5.15 |
| cpu13 `…83991351` | 1.032613 | 1.020000 | FAIL | 0.0040 | 16.46 / 4.40 |
| cpu0 `…88222409` | 1.010253 | 1.020000 | PASS | 0.0044 | 2.41 / 4.66 |

The first two rows are the bead's: drift 1.050140 against floor 1.024437, and drift 1.013775 against
floor 1.036138 (my floors differ in the fifth decimal only because I recomputed from the rounded
half-widths). **The report is accurate and re-derivable.**

## The mechanism: a noisier run is given a wider allowance

The floor is `1 + 2 × null_radius`, so **the threshold grows with the run's own A/A noise.** The two
cores in the bead differ in their nulls as much as in their drifts:

- cpu12 null half-width **0.0122** → floor 1.0244
- cpu13 null half-width **0.0181** → floor 1.0362

cpu13's null was **1.5x noisier**, which bought it a floor 1.5 percentage points wider. For any drift
there exists a null-noise level that admits it: the cpu12 run at drift 1.0383 would have PASSED had
its half-width been ≥ 0.0192 — a value cpu13 nearly reached in the same session.

So the instability is **not primarily cross-core.** All four cpu12/cpu13 runs are noisy (cv 4.4-21.9%
against 2.4-4.7% on cpu0); three fail and one passes, and the one that passes has the widest floor of
the set. Read as a group, this is one noisy population being thresholded against a limit computed
from its own noise, not two cores behaving differently.

## What this suggests, and what it does not

The bead asks whether the bracket needs "an effect CI plus a 2% null-median bias bound". This audit
points somewhere cheaper and strictly loss-only: **a precision PRECONDITION — refuse to decide when
`cv_pct` or the null half-width exceeds a bound — rather than an elastic threshold.**

That satisfies the bead's own constraints by construction:

- **Loss-only integrity control:** a refusal can only move a row from pass/fail to undecidable. It
  can never manufacture a pass, so it cannot inflate any banked claim.
- **`medianCiGate` untouched:** the precondition sits before the verdict, not inside the median CI.

It is also the same shape as the drift control landed in `abba_render.py` (`ee930f50`): when the
instrument is too noisy to decide, say so, instead of widening what counts as agreement.

⚠️ **Not recommended on this evidence: changing `MIN_CLAIM_RATIO` or the `2 ×` multiplier.** Nothing
here shows the floor is mis-scaled; it shows the floor is being *fed* a noise estimate from runs that
should not have been decided at all. Tuning the constant would move every verdict in the ledger on the
strength of five rows.

## A hypothesis that did NOT transfer, recorded so nobody re-derives it

Elsewhere in this campaign the calibrated `batch` was the reliable tell for a contended core (20-22
contended vs 37-39 clean on `sequence_20`). **It does not discriminate here:** every row above sits at
batch 105-111. `batch` is calibrated to a 3 ms target, and `flowchart_small_10` runs ~31 µs per
operation, so the batch size is pinned by the calibration target rather than by contention. The tell
in this case is `cv_pct`, which spans 2.41% to 21.93% across the same rows. **`batch` is comparable
only within a case, never across cases.**
