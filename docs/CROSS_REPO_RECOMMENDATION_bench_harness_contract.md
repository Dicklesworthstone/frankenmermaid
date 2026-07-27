# Cross-repo recommendation: a bench-harness contract

**From:** frankenmermaid · **Status:** current fleet contract.

All three exist because of the same failure mode: **a measurement discipline that a human (or an agent) can
forget is not a discipline, it is a hope.** Provenance, noise floor, and the floor's own resolution limit should
be *emitted by the harness itself*, on every run, whether anyone asked or not.

The three parts compose: **(1)** the ELF hash tells you *which binary ran*; **(2)** the A/A null control tells
you *the harness's floor on this run*; **(3)** the calibration sweep tells you, ahead of time, *which knob
setting makes an effect of the size you are chasing decidable at all*. Adopt them in that order.

---

## 1. Self-reporting ELF SHA-256

### What it does

The benchmark binary hashes **its own executable** and prints it as the first line of output:

```
bench_elf_sha256=15591dd297913a88652285c70c817338e431392874f4ba289e01f1d66a2670c9 (857728 bytes) \
  /path/.../release/deps/barycenter_sweep-ccd51ba108b95431
```

### Why it must be inside the process

A hash computed by a shell step *next to* the run proves nothing about **which ELF actually executed**:

- Our remote-build helper (`rch`) refuses non-compilation commands (`RCH-E301`), does not retrieve bench binaries,
  and compiles into an **opaque per-worker pool target dir** whose path you cannot predict.
- Concurrent edits can move source state while a benchmark is running.

A hash the binary emits about itself survives all of that. It cannot be stale, cannot point at a different
artifact, and cannot be forgotten.

### How a repo adopts it (Rust; ~20 lines, one dev-dep)

```rust
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// SHA-256 of this executable, reported from inside the measured process.
fn self_identity() -> String {
    let Ok(path) = std::env::current_exe() else { return "unavailable".into() };
    let Ok(bytes) = std::fs::read(&path) else { return "unavailable".into() };
    let mut h = Sha256::new();
    h.update(&bytes);
    let digest = h.finalize();
    let mut sha256 = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(sha256, "{byte:02x}").expect("writing to String cannot fail");
    }
    format!("{sha256} ({} bytes) {}", bytes.len(), path.display())
}

fn main() {
    println!("bench_elf_sha256={}", self_identity());
    // ... benchmark ...
}
```

`sha2 = "0.11"` as a **dev-dependency** only.

### Cost

One dev-dep, one `Cargo.lock` line, one `read()` of a ~1 MB file per bench process (≈1 ms, outside the measured
region). Zero effect on the measurement. Works under any remote-build wrapper, because the binary is the one
thing that is definitionally present at run time.

### Caveat

It identifies the *binary*, not the *source*. Pair it with a source hash checked **before the bench and again at
`git add` time** so the measured source and committed source remain identical.

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
  situation the workload must route through the named target frame and the frame must carry measurable self-time.

---

## 3. Calibrate the floor, gate on the median, and publish per-function settings

The null control only helps if you know what its numbers *mean*. So calibrate it: sweep
`min_sample ∈ {2, 10, 40} ms` × `min_of ∈ {1, 3}` inner replicates × **every function you bench**, **A/A only**,
configurations interleaved round-robin (a *sequential* config sweep confounds the configuration with
time-varying machine load — the same mistake arm-interleaving exists to prevent, one level up). Per config, take
41 A/A ratios → median → a **bootstrap 95% CI on that median**. Let
`half_width = max(|ci_hi − 1|, |ci_lo − 1|)` and derive the certification threshold
`min_decidable_2x = 1 + 2·half_width`.

### Two results that will save you a week

1. **`cv` does not track decidability, and no in-harness knob makes `cv < 5` reachable on a loaded, unpinnable
   worker.** A 20× longer sample moves `cv` only ~4 points. Two configs of the same function: `cv 2.37%` → floor
   1.008×, vs `cv 9.66%` (4× worse `cv`) → floor 1.003× (*better*). Gating on `cv` picks the wrong config. **Gate
   on the median-CI floor.**
2. **The floor is per-function.** On the same worker, at the naive `2 ms / ×1` default, the A/A floor was 1.048×
   (`btreemap`), 1.033× (`single_pass`), 1.023× (`dense_rank`) — a 2.1× spread. A config that decides a lever for
   one function may not for another. Read the row for the function you are about to bench.

### Two knobs, and which one matters

- **`min_of` (inner replicates, keep the minimum) is the dominant knob.** At 2 ms, `×1 → ×3` moved the floor
  1.048× → 1.012×, 1.033× → 1.004×, 1.023× → 1.008×. The minimum of k back-to-back timings discards the
  one-sided preemption outliers a longer sample cannot.
- **`min_sample` beyond ~10 ms buys nothing, and 40 ms can be worse** (a longer sample is a bigger target for a
  preemption). Do not reach for longer samples; reach for `min_of`.

### PUBLISHED SETTINGS — cheapest config that decides an effect of size X (per function)

Decidable = claim exceeds the floor by a **2× margin** (`X ≥ 1 + 2·half_width`). Worker `hz2`, 18 configs, 19.8 s.
Full table + CIs: `.benchmarks/harness_calibration_published_settings.md`.

| function | 1.02× | 1.05× | 1.10× | 1.25× | 1.50× |
|---|---|---|---|---|---|
| `btreemap` | 10 ms / ×1 | 2 ms / ×3 | 2 ms / ×1 | 2 ms / ×1 | 2 ms / ×1 |
| `dense_rank` | 2 ms / ×3 | 2 ms / ×1 | 2 ms / ×1 | 2 ms / ×1 | 2 ms / ×1 |
| `single_pass` | 2 ms / ×3 | 2 ms / ×3 | 2 ms / ×1 | 2 ms / ×1 | 2 ms / ×1 |

A **≥ 1.10× claim is decidable in the cheapest config** for every function. A **1.02× claim needs `min_of = 3`**
and still sits near the floor — treat sub-1.05× wins here with suspicion regardless of config. **Nothing sub-1.01×
is decidable on this hardware; do not claim it.** Sensible lane default: **`min_sample = 2 ms, min_of = 3`**
(floor ≤ 1.012× for every function).

**Gate rule to adopt:** report `cv`, but gate the claim on the null-median CI — *a claim of size X is
certifiable only when its distance from 1.0 clears the arm's A/A null 95% CI half-width by a 2× margin*.
Note whether the worker was quiet; `rch` cannot pin one, so quietness is luck, which is why the null must be
emitted **in the same invocation** as the claim.

---

## 4. Campaign result classification

A repository's own code before versus after is a `maintenance-self-speedup`. It can justify landing
an improvement, but it is not campaign output and must not be quoted as a competitive claim.

An `incumbent-win` requires the **actual legacy incumbent** as one arm, side-by-side with the
candidate in the same harness invocation. The ledger row must pin the incumbent name, version, and
artifact SHA-256; record the shared invocation ID and measured ratio; and carry the A/A null and the
candidate process's self-reported ELF SHA-256. A previous revision of the candidate project is not
the incumbent.

For frankenmermaid the actual incumbent is mermaid-js. Its exact ledger markers are:

```markdown
**Campaign result class:** incumbent-win
**A/A null control (same invocation):** baseline/null median ratio ..., CI ...
**Legacy incumbent arm (same invocation):** name=mermaid-js version=<pin> artifact_sha256=<64 lowercase hex> invocation_id=<id> measured_ratio=<number>x
```

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
6. **Bracket the run with a source hash, and re-check it at `git add` time.**
7. **Calibrate the floor once per machine class and per function** (copy `harness_calibration.rs`): sweep
   `min_sample × min_of × arm`, interleaved round-robin, and read off the per-function published-settings table.
   Then **gate on the null-median 95% CI** (the claim must clear the CI half-width by a 2× margin), not on `cv`.
8. **Profile-verify non-zero self-time** in the function under test before honoring or writing any REJECT.
9. **Classify every kept result** as `maintenance-self-speedup` or `incumbent-win`; require a pinned,
   same-invocation actual-incumbent arm for the latter.

Steps 1–3 are the minimum viable measurement contract. Steps 6–9 make source attribution, target
routing, and campaign-output classification enforceable.

## Suggested adoption order

1. **Self-reporting ELF sha256** — 20 lines, no measurement impact, immediate provenance. Do this first.
2. **Null control** — a refactor of the measured loop plus 2× wall time. Do this before you trust any sub-10% ratio.
3. **Self-time verification** — confirm the named target is on the measured path before accepting a verdict.
4. **Result classification** — make self-speedup maintenance and incumbent-win distinct machine values.

## 5. Corollary — whole-binary A/Bs (ISA, LTO, allocator) via same-worker matching

Some comparisons cannot use the single-binary `paired()` substrate at all: `-C target-cpu`, `lto`, a global
allocator swap — each is a **whole-binary** property, so the two arms are two *binaries*. On a remote fleet that
picks workers non-deterministically, the naive two-invocation A/B is invalid (absolute times aren't comparable
across workers). The required protocol is:

1. Build both binaries remotely (`RUSTFLAGS="-C target-cpu=x86-64-v3" rch exec -- cargo bench …` vs default). It
   needs **zero local disk**.
2. **Confirm the flag actually reached the remote compiler** — otherwise you measure the same binary twice. The
   ELF-sha self-report (§1) is exactly the check: same source + same worker + *different* sha ⇒ codegen changed.
3. **Capture the worker id each run lands on, and compare only same-worker pairs.** That controls the one
   confound (worker identity) that makes two-invocation A/Bs invalid.
4. **Do not gate on instruction count** — an ISA change retires more work per instruction, so fewer instructions
   is the *mechanism*, not a neutral proxy. Use wall/cycles on the matched worker, gated on CI overlap.

## What I am *not* recommending

Do not centralize these into a shared crate before two or three repos have each written their own. The harnesses
differ (criterion vs hand-rolled, in-process vs subprocess, remote vs local), and the useful abstraction is not yet
obvious. Copy the twenty lines; extract later, if a pattern actually emerges.

---

## Adoption in one paragraph (the owner ask)

**To adopt this contract, a repo does six things, in order.** (1) Add `sha2` as a dev-dep and print the bench's
own `env::current_exe()` SHA-256 as line one of every bench — provenance that cannot be forgotten or faked.
(2) Factor the measured loop into `paired(arm_a, arm_b)` that times both arms **interleaved inside one round**,
alternating order, and reports the **median of per-round ratios**; `black_box` inputs *and* results into a printed
checksum. (3) Call it twice — `paired(base, base)` then `paired(base, cand)` — so every run emits its own noise
floor next to its claim (cost: 2× wall time). (4) Once per machine class, run a calibration sweep
(`min_sample × min_of × function`, interleaved) to publish which config decides which effect size, and **gate the
claim on the null-median 95% CI at a mandatory 2× half-width margin, never on `cv`** (`cv < 5` is unreachable on
a shared, unpinnable fleet; the median is tight regardless). (5) Bracket every run with a source hash re-checked
at commit time, and profile-verify non-zero self-time before honoring any REJECT. (6) Classify a
before/after self-comparison as maintenance and reserve campaign-win status for a pinned actual-incumbent arm
run in the same invocation. Steps 1–3 are the minimum viable measurement contract. **Recommendation: adopt
1–3 fleet-wide now; make steps 4–6 mandatory before publishing a result. Do not centralize into a crate yet.**
Reference implementation lives in this repo:
`crates/fm-layout/benches/{barycenter_sweep,harness_calibration}.rs`.
