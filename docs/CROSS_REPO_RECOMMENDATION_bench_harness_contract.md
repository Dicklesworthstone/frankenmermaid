# Cross-repo recommendation: a bench-harness contract (provenance + noise floor)

**From:** `cc_fm` (frankenmermaid) · **Date:** 2026-07-10 · **Status:** RECOMMENDATION ONLY.
**I have not touched any other repository.** This describes two mechanisms proven here, what they cost, and how a
repo adopts them. Take them, adapt them, or ignore them.

Both exist because of the same failure mode: **a measurement discipline that a human (or an agent) can forget is
not a discipline, it is a hope.** Provenance and noise floor should be *emitted by the harness itself*, on every
run, whether anyone asked or not.

---

## 1. Self-reporting ELF SHA-256

### What it does

The benchmark binary hashes **its own executable** and prints it as the first line of output:

```
bench_elf_sha256=15591dd297913a88652285c70c817338e431392874f4ba289e01f1d66a2670c9 (857728 bytes) \
  /path/.../release/deps/barycenter_sweep-ccd51ba108b95431
```

### Why it must be inside the process

A hash computed by a shell step *next to* the run proves nothing about **which ELF actually executed**. In this
repo that gap was not theoretical:

- Our remote-build helper (`rch`) refuses non-compilation commands (`RCH-E301`), does not retrieve bench binaries,
  and compiles into an **opaque per-worker pool target dir** whose path you cannot predict.
- Concurrent agents edited the crate **mid-benchmark** at least three times; one run measured a source that no
  longer matched the commit, and I had to downgrade my own WIN row to corroboration because of it.

A hash the binary emits about itself survives all of that. It cannot be stale, cannot point at a different
artifact, and cannot be forgotten.

### How a repo adopts it (Rust; ~20 lines, one dev-dep)

```rust
use sha2::{Digest, Sha256};

/// SHA-256 of this executable, reported from inside the measured process.
fn self_identity() -> String {
    let Ok(path) = std::env::current_exe() else { return "unavailable".into() };
    let Ok(bytes) = std::fs::read(&path) else { return "unavailable".into() };
    let mut h = Sha256::new();
    h.update(&bytes);
    format!("{:x} ({} bytes) {}", h.finalize(), bytes.len(), path.display())
}

fn main() {
    println!("bench_elf_sha256={}", self_identity());
    // ... benchmark ...
}
```

`sha2 = "0.10"` as a **dev-dependency** only.

### Cost

One dev-dep, one `Cargo.lock` line, one `read()` of a ~1 MB file per bench process (≈1 ms, outside the measured
region). Zero effect on the measurement. Works under any remote-build wrapper, because the binary is the one
thing that is definitionally present at run time.

### Caveat

It identifies the *binary*, not the *source*. Pair it with a source hash checked **before the bench and again at
`git add` time** — the gap between "after the last run" and "at commit" is exactly where a concurrent editor slips
in. That is the specific hole that cost me a KEEP row here.

---

## 2. A/A null control, emitted on every run

### What it does

Before the real A/B, the harness registers **the identical arm twice** and measures it through the exact same
interleaved routine. That ratio is the harness's own noise floor.

Any "win" smaller than the null control's departure from `1.000` is indistinguishable from noise. **Any REJECT of
a lever whose effect is below the floor is meaningless** — you rejected the harness, not the lever.

### Why it belongs in the harness, not in a checklist

Because the floor is not a property of the code — it is a property of *this machine, right now*. Ours moved by an
order of magnitude between workers within a single session. A floor measured yesterday, or on the quiet worker,
tells you nothing about the run you are about to trust. Emit it **in the same invocation**, from the same routine,
on the same batch size.

### First reading from this repo (provisional — see caveat)

Same binary, one invocation, `paired(arm, arm)` versus `paired(arm_a, arm_b)`:

| input | **null A/A ratio** | null cv | real A/B ratio | A/B cv |
|---|---:|---:|---:|---:|
| `cyclic_scc_100` | 1.0357× | 14.17% | 2.611× | 8.57% |
| `cyclic_scc_300` | 0.9764× | 12.03% | 3.870× | 37.00% |
| `cyclic_scc_800` | 0.9954× | 8.64% | 8.112× | 9.69% |

Read that honestly: on this (loaded, unpinnable) worker the harness **cannot decide any lever below ~4%**, and its
`cv` gate is not meaningful there at all. It *can* decide a 2.6–8.1× lever, which is what we had. The certified
barycenter win (3.669×) clears this floor by roughly two orders of magnitude and survives.

*Caveat, stated because the rule demands it: the source changed mid-run (a concurrent agent), so these three rows
are **provisional**. The null-control mechanism is what I am recommending; these numbers are its first output, not
a certified result.*

### How a repo adopts it

Factor the measured loop into a routine that takes **two arms** and returns `(p50_a, p50_b, ratio_p50, cv, mad)`:

```rust
fn paired(arm_a: Arm, arm_b: Arm, /* inputs */, batch: u32, rounds: usize) -> Stats {
    for round in 0..rounds {
        // alternate order so first-mover cache/branch bias cancels
        let (a, b) = if round % 2 == 0 { (time(arm_a), time(arm_b)) }
                     else              { let b = time(arm_b); (time(arm_a), b) };
        ratios.push(a as f64 / b as f64);
    }
    // statistic = median of PER-ROUND ratios; cv over those ratios
}
```

Then call it twice per input:

```rust
let null = paired(Arm::Baseline, Arm::Baseline, ..);   // noise floor, same batch, same routine
let real = paired(Arm::Baseline, Arm::Candidate, ..);  // the claim
```

and print both, always.

### Cost

Exactly 2× the bench wall time. That is the entire price, and it buys you the right to believe your own numbers.

### Prerequisites it composes with

- Both arms in **one binary, one invocation** — a ratio split across two remote invocations is invalid when the
  scheduler picks workers non-deterministically.
- **Interleave inside a single measured routine.** Criterion group members run *sequentially*; registering two
  arms side-by-side in one group does **not** cancel drift.
- **Calibrate the batch off the faster arm**, so the shorter sample still clears the timer floor.
- `black_box` the **inputs and the results**, then fold results into a printed checksum. A dead-code-eliminated
  arm cannot produce the checksum.
- **Profile-verify non-zero self-time** in the function under test before honoring or writing any REJECT. In this
  repo, four crossing-minimization rejections had stood for months on a bench where the target function had
  **0.000% self-time** — the auto-selector routed those inputs to a different algorithm. Re-measured on a workload
  that actually executes it, that "dead" lever is a certified **3.591×**.

---

---

## 3. Calibrate the floor — and then gate on the right statistic

The null control only helps if you know what its numbers *mean*. So we calibrated it: a sweep of
`min_sample ∈ {2, 10, 40} ms` × `min_of ∈ {1, 3}` inner replicates, **A/A only**, configurations interleaved
round-robin (a *sequential* config sweep confounds the configuration with time-varying machine load — the same
mistake arm-interleaving exists to prevent, one level up).

Worker `vmi1227854`, 41 rounds, 15.7 s total:

| min_sample | min_of | null A/A ratio | null cv | null MAD | systematic floor |
|---:|---:|---:|---:|---:|---:|
| 2 ms | 1 | 0.9950× | 15.65% | 2.76% | 0.50% |
| **2 ms** | **3** | **1.0013×** | 11.98% | 3.56% | **0.13%** |
| 10 ms | 1 | 0.9934× | 14.02% | 3.58% | 0.66% |
| 40 ms | 1 | 0.9925× | 11.63% | 4.40% | 0.75% |
| 40 ms | 3 | 1.0011× | 14.03% | 6.84% | 0.11% |

**Two results that will save you a week:**

1. **No in-harness knob moves `cv` below ~12% on a loaded, unpinnable worker.** A 20× longer sample moves it from
   15.65% to 11.63%. Inner `min-of-k` does not help consistently. If your remote-build helper picks workers
   non-deterministically, a `cv < 5` gate is unreachable *by construction* — and a gate you cannot meet either
   blocks every claim or quietly invites statistic-shopping.
2. **The A/A null *median* is tight regardless — 0.11%–0.75% from 1.000.** One-sided scheduler outliers inflate
   `cv` and `MAD` **without biasing the median of per-round ratios**.

**So gate on the floor, not on `cv`.** The smallest effect the harness can resolve is the null median's departure
from `1.000` (~1% here). Require the claim to exceed that by a wide margin; **report** `cv` rather than gating on
it, and note whether the worker happened to be quiet. Our two certified wins clear the worst-case 0.75% floor by
~40× and ~350×.

---

## What a repo must do to adopt this (the whole checklist)

1. **Add `sha2` as a dev-dependency** and print `self_identity()` as the first line of every bench `main`.
   *(~20 lines, ~1 ms, zero measurement impact.)*
2. **Factor the measured loop into `paired(arm_a, arm_b) -> (p50_a, p50_b, ratio_p50, cv, mad, checksum)`**, with
   the two arms timed back-to-back inside one round and the order alternating per round. Statistic = **median of
   per-round ratios**; `cv`/`MAD` taken over those ratios.
3. **Call it twice per input:** `paired(base, base)` then `paired(base, cand)`. Print both rows, always.
   *(Cost: exactly 2× bench wall time.)*
4. **Calibrate `batch` off the faster arm** so the shorter sample still clears the timer floor.
5. **`black_box` inputs and results**, fold results into a printed checksum.
6. **Bracket the run with a source hash, and re-check it at `git add` time.** The window between "after the last
   run" and "at commit" is where a concurrent editor slips in; it cost us a KEEP row.
7. **Calibrate the floor once per machine class** (copy `harness_calibration.rs`), then **gate on the null
   median's departure from 1.000**, not on `cv`.
8. **Profile-verify non-zero self-time** in the function under test before honoring or writing any REJECT.

Steps 1–3 are the minimum viable contract. Steps 6–8 are what actually caught our errors.

## Suggested adoption order

1. **Self-reporting ELF sha256** — 20 lines, no measurement impact, immediate provenance. Do this first.
2. **Null control** — a refactor of the measured loop plus 2× wall time. Do this before you trust any sub-10% ratio.
3. **Self-time verification** — cheapest of the three when a symbolized binary is reachable, and the one that
   caught the largest error in this repo's history.

## What I am *not* recommending

Do not centralize these into a shared crate before two or three repos have each written their own. The harnesses
differ (criterion vs hand-rolled, in-process vs subprocess, remote vs local), and the useful abstraction is not yet
obvious. Copy the twenty lines; extract later, if a pattern actually emerges.
