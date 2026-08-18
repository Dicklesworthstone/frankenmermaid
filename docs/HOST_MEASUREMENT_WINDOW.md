# Reading the host before a timed run — loadavg, idle, and volatility

**Not a verdict row: no ratio, no arm, no ELF timing.** This is a method note about *when* a timed
run is worth starting, recorded because three different instruments disagree on this host and two of
them are misleading.

## The three readings, measured together (2026-08-17)

| instrument | reading | what it suggested |
|---|---|---|
| `uptime` loadavg | 27.96 / 31.09 / 26.88 | host hammered |
| overall `%idle` | 62–83%, mean 73%, iowait ~0–1% | host mostly free |
| per-CPU busy > 20% | 16 of 64 (6 over 50%, 10 at 20–50%) | gate would refuse |

**Loadavg is the worst of the three here and should not gate anything.** A pandas-vs-PyTorch bench
mid-flight put the 1-minute average at 73 while the machine was ~88% idle: loadavg counts runnable
*and* uninterruptible tasks, so a job that forks widely inflates it without consuming CPU.

**Overall `%idle` is better but still insufficient on its own**, for the reason below.

## The finding that actually decides it: VOLATILITY

Twelve consecutive one-second samples, counting CPUs above 20% busy:

```
64 64 64 64 20 17 11 14 14 64 64 64
```

The host swings between fully saturated and nearly idle **within seconds**. Mean idle over the same
span was 40.8%, a number that describes none of those samples.

**Consequence for measurement, and it is stronger than "the host is busy":** an A/B arm that lands in
a 64-busy second against one that lands in an 11-busy second is comparing phases of someone else's
job, not the two engines. Interleaving A/B/B/A shortens but does not remove the exposure — the swing
period here is comparable to a single arm's duration. A single spot-check of loadavg, of `%idle`, or
even of the per-CPU count would have called this window acceptable at sample 7 and unacceptable at
sample 1.

**There is now a script: `scripts/window_check.sh [samples]`.** It exits 0 only when the samples
AGREE and every CPU is under the ceiling, and prints the loadavg, per-sample idle range and CPU MHz
to record on a banked row. Run it instead of eyeballing `uptime`.

⚠️ **Its first version was wrong in an instructive way.** It took the count sequence first and the
idle figure afterwards, then printed them together — so it reported "64 CPUs over the ceiling"
beside "82.5% idle", which cannot describe the same second. Both numbers were real; they were from
different instants. It now derives the count AND that sample's aggregate idle from the SAME `mpstat`
call, and reports idle as a RANGE rather than a mean, because a mean of 40% across samples of 82%
and 25% describes neither — which is exactly how a volatile window reads as acceptable.

**So the pre-flight check is a SEQUENCE, not a reading.** Take several one-second samples and require
them to agree; a window whose samples disagree is unmeasurable regardless of how good the best one
looks. That is also why the standing rule not to build in the window you intend to measure in
matters: immediately after a `cargo build` here, the count read 64/64 for four consecutive seconds.

## The interference is PERIODIC, and each arm is far shorter than its period

Two sequences taken minutes apart both show the same shape rather than random noise — roughly three
seconds quiet, then seven busy:

```
64 64 64 64 64 64 45 33 21 19     idle 22.5 -> 79.8%
19 20 20 64 64 64 64 64 64 64     idle 82.8 -> 0.2%
```

So the interference has a period of about **ten seconds** and swings between ~84% and ~0.2% idle.
Now compare that against how long an arm actually measures for, from the shipped corpus and harness
defaults:

| arm | reps | per rep | measured span | fraction of one noise period |
|---|---:|---:|---:|---:|
| frankenmermaid | 100 (`reps_rs`) | ~93 µs | **~9 ms** | ~1/1000 |
| mermaid-js | 15 (`reps_js`) | ~35 ms | **~0.5 s** | ~1/20 |

**Each arm therefore samples one instantaneous PHASE of a slow oscillation, not an average of it.**
Landing in the quiet trough or the busy peak is luck, and A/B/B/A interleaving does not fix it: with
each arm 20x to 1000x shorter than the period, the four arms are four point-samples of a
slowly-varying signal, which may all fall in one phase or scatter across several.

**The two arms are not even comparably exposed.** The frankenmermaid arm integrates over ~9 ms and
the incumbent over ~500 ms, so the incumbent averages ~50x more of the interference than the arm it
is being compared against. That asymmetry biases in whichever direction the phase happens to fall,
and nothing in the current row records which.

This is the most likely explanation for the residuals already visible on the banked `sequence_20`
row: frankenmermaid drift of 1.0204x and 1.0481x across two runs, and A/A null radii up to ±9% on an
arm compared against *itself*. Those are large for a pure timing repeat, and phase alignment accounts
for them without needing any engine-side cause.

### What would actually fix it

**CORRECTION to an earlier version of this note**, which said lengthening the arms meant editing the
corpus to `reps_rs` 322,000 and `reps_js` 860. It does not: **`--reps-scale` already exists and
scales BOTH arms** — `run.mjs:2229-2230` multiplies `reps_rs`/`warmup_rs`, `mermaid_bench.mjs:1528`
multiplies `reps_js`/`warmup_js`, and `run.mjs:2672` forwards the flag. No corpus change is needed,
and none should be made: an unpinned or re-pinned case is a hard `exit 3` for every run, and editing
existing cases would silently move the baseline every banked row was measured against.

**But the knob is a SINGLE multiplier, and that is the real obstacle.** The two arms differ ~375x in
per-rep cost, so one scale cannot bring both above the ~10 s interference period:

| scale | frankenmermaid arm | mermaid-js arm | verdict |
|---:|---:|---:|---|
| 1 (today) | ~9 ms | ~0.5 s | both far below the period |
| 60 | ~0.54 s | ~30 s | incumbent covered; OUR arm still inside one phase |
| 3,300 | ~30 s | ~28 min | both covered; ~1.9 h for one A/B/B/A |

So the choice is a genuine cost, not an oversight: covering the SHORT arm forces the long arm to
~28 minutes each, i.e. roughly two hours for a single interleaved run. Covering only the incumbent
is cheap and leaves the frankenmermaid arm exactly as phase-exposed as it is now — which is the arm
whose drift (1.0204x, 1.0481x) prompted this investigation.

Two ways forward, and they are not equivalent:

1. **Run at `--reps-scale 3300` once, in a window that stays stationary for two hours.** Expensive,
   and the stationarity requirement is stronger than anything this host has shown.
2. **Repeat the whole A/B/B/A many times at scale 1 and take medians across repeats**, so the phase
   is sampled rather than fixed. Cheap per run, but it needs enough repeats that phases decorrelate,
   and nothing currently records which phase a given run landed in.

A third option — scaling the arms INDEPENDENTLY so both reach ~30 s without the 28-minute incumbent —
would need a per-arm flag that does not exist today. That is the change worth making if this
comparison is to be trusted on a shared host, and it is a harness change, not an engine one.

Neither is a code change to the engines, and neither should be attempted while `window_check.sh`
reports NOT MEASURABLE — a longer arm on an oscillating host still needs the oscillation to be
stationary for the duration of the run.

## A run COMPLETED in the best window of the campaign, and its own A/A invalidates it (2026-08-17)

The window was the calmest measured: per-CPU counts `8 9 10 13 15 19 18 8 11 11 10 9`, idle
**82.2-89.0%** across the same samples, iowait 0.00%, loadavg 11.25/12.79/18.23. Binary rebuilt at
`HEAD 816a781e` with the revision embedded, sha256 `289d0377…`; corpus `sequence_20` generated from
`corpus.mjs`, 1257 bytes, sha256 `31c0dd6b…`.

```
A1 fm       93,454 ns     load [23.35, 16.10, 18.92]   mean 2918 MHz (spread 2.861x)
B1 mermaid  37,600,000 ns load [23.35, 16.10, 18.92]   mean 2931 MHz (spread 2.833x)
B2 mermaid  36,400,000 ns load [23.35, 16.10, 18.92]   mean 3008 MHz (spread 2.929x)
A2 fm      159,599 ns     load [22.36, 16.02, 18.88]   mean 2605 MHz (spread 2.865x)
```

pins: fm on `cpu19 @ 4073 MHz` (fastest_clock_among_idle); incumbent 8 CPUs, slowest 2832 MHz,
`starved=False`; 19/64 CPUs busy at the start.

**THE RATIO IS NOT QUOTABLE, AND THE REASON IS IN THE ROW ITSELF.** Worst bound came out at 228.1x
and headline 292.4x, but:

| | drift between its two identical repeats |
|---|---|
| mermaid-js arm | **1.033x** |
| frankenmermaid arm | **1.7078x** |

**Our own arm varied by 71% between two runs of the same binary on the same input inside one
invocation.** A ratio whose numerator moves 71% under repetition is not a measurement of an engine.
The frankenmermaid drift IS the A/A null for our side, and 1.71x fails any threshold worth having —
the incumbent's four-observation null (`0.939-1.122`) is comfortably tighter than our arm's
two-observation drift.

**The recorded CPU MHz rules out the obvious explanation.** A2 ran at mean 2605 MHz against A1's
2918 MHz — 12% slower clocks — but took **71%** longer. Clock alone does not span that gap, so the
extra time is interference the 9 ms arm happened to land in, not a slower core. This is exactly what
the per-arm MHz recording is for, and it is the first row where it decided something.

**This CONFIRMS the arm-duration prediction empirically**, with the harness's own numbers rather than
by argument: an arm measuring ~9 ms against ~10 s of interference period is a lottery, and it came up
badly here even though every host-level indicator said the window was quiet. The banked `sequence_20`
row's much tighter drift (1.0204x, 1.0481x) is therefore better read as two lucky draws than as
evidence of stability — which also means its ±9% A/A radius understates the true exposure.

**What this run does NOT show.** It is not evidence that frankenmermaid is slower than previously
recorded, and 228.1x must not be quoted as a competing figure: both numbers come from the same
lottery, and this one simply drew worse. Nothing here changes what the engines do; it changes what
the harness can currently prove.

## The per-arm rep override is safe: what it costs, and why the work proof survives it

Analysis only — `scripts/headtohead/abba_render.py` was leased by BeigeHill through 21:06Z and I
did not touch it. This is the homework so the change can be made once, correctly.

### `ns` is a per-rep MEDIAN, not an arm total (verified by reproduction)

Not inferred from the field name — the reported figures are reproduced exactly from the raw
per-arm values:

```
worst bound   36,400,000 / 159,599 = 228.06   (harness printed 228.1x)
headline      37,000,000 / 126,526 = 292.44   (harness printed 292.4x)   [medians of each pair]
```

Both land on the printed values, so `ns` is the median of one render. That fixes the arm duration
as `ns x reps`: **93.4 µs x 100 = ~9.3 ms** for frankenmermaid and **37.6 ms x 15 = ~0.56 s** for
mermaid-js, confirming the arm-duration table above from the run's own numbers rather than from
estimates.

### The knob is one line, and it is in the corpus hand-off, not the timer

`corpus.mjs:553` pins `sequence_20` at `reps_rs: 100` / `reps_js: 15`. The fm arm carries that value
through the generated hand-off at `abba_render.py:262` (`reps: item.reps_rs`); `--reps` (line 312)
is forwarded only to `mermaid_arm` (lines 392/394). So a `--fm-reps` override is a single
substitution at line 262, independent of `--reps`, and needs no corpus edit — which matters,
because re-pinning a case is a hard `exit 3` for every run and would move the baseline every banked
row was measured against.

### Cost, computed from the measured per-rep medians

To put BOTH arms above the ~10 s interference period at ~30 s each:

| arm | per-rep median | reps for ~30 s | flag |
|---|---:|---:|---|
| frankenmermaid | 93.4 µs | ~322,000 | `--fm-reps 322000` |

⚠️ **THE REP FIGURES IN THIS TABLE ARE WRONG.** See "the rep cost model" below: an arm's wall time
is `reps × the 3 ms calibration target`, not `reps × per-op ns`. A ~30 s arm is **~10,000 reps**;
322,000 reps runs for roughly sixteen minutes per arm.
| mermaid-js | 37.6 ms | ~800 | `--reps 800` |

Four arms at ~30 s is **~2 minutes** of measurement per A/B/B/A. That is the whole point of
splitting the knob: the single `--reps-scale` multiplier forces the incumbent to ~28 min/arm
(~1.9 h per run) to buy the same coverage, because the two arms differ ~400x in per-rep cost.

### The work proof stays valid under scaling — but it cannot police the new failure mode

`check_work_proof` computes `rate = work.bytes / ns`. Measured: `43,368 / 93,454 = 0.46 bytes/ns`,
comfortably under `MAX_BYTES_PER_NS`. Both terms are per-render, so **the rate is invariant under
rep scaling** — a longer arm neither weakens nor trips the memo check, and the gate keeps doing its
job.

**What it does NOT cover, and this is the thing to watch.** The rate check is one-sided: it fails
only when the rate is too HIGH. An arm that ran longer *without doing proportionally more work*
drives the rate DOWN, straight into the passing region. So the gate cannot, by construction, catch
a `--fm-reps` that inflated wall time without re-rendering. The counted proof that closes that hole
is cheap and should land with the flag: assert the arm's total render COUNT equals the requested
reps, rather than trusting that a bigger number was honoured. Raising or relaxing the rate ceiling
would be the wrong repair — the same conclusion the ledger reached about this gate before.

## CORRECTION: the 1.71x drift was SINGLE-CORE PINNING, not the host's interference phase

I attributed the frankenmermaid arm's 1.7078x drift to phase exposure — an arm ~1/1000 of the
interference period sampling one instant of a slow oscillation. **That explanation was wrong**, or at
best irrelevant at this scale. Re-running the identical arm UNPINNED collapses the drift:

| | pinned to cpu19 | unpinned (`--no-pin`) |
|---|---:|---:|
| A1 / A2 | 93,454 / 159,599 ns | 88,899 / 89,739 ns |
| **fm drift** | **1.7078x** | **1.0094x** |
| calibrated batch | **22** | **39** |
| worst bound | 228.1x | **368.8x** |
| headline | 292.4x | 371.7x |

The arm is the SAME ~9 ms against the SAME ~10 s interference period in both runs. If phase exposure
were driving the drift, unpinning could not have removed it. It went from 71% to **0.94%**.

### The batch count was a pre-registered prediction, and it hit

A peer's finding (`project_single_core_pinning_is_a_noise_source`) records that the harness's
calibrated batch is the tell — **21-25 = slow regime, 37-39 = fast** — and that it is recorded and
never read. My two runs land on opposite sides of exactly that split: **batch 22 pinned, batch 39
unpinned**. That is a mechanism confirmation, not a correlation I went looking for after the fact,
and it is why this correction is stated as a cause rather than an association.

### What this does to the banked row

It **corroborates** it. `368.8x` worst bound unpinned sits close to the banked **362.4x**, from an
independent invocation. Both arms' A/A nulls are tight (`0.982` and `1.003`, radii ~±2-6%), and the
incumbent arms agree within 0.6% (33.1 vs 33.3 ms).

So the earlier `228.1x` should be read as **the pinned artifact depressing our own arm**, not as
evidence against the banked figure — and my previous note's suggestion that the banked row's tight
drift was "two lucky draws" is withdrawn. It was tight because it was measured in a regime that
behaves.

### Consequences for the `--fm-reps` recommendation

Downgraded from blocking to merely desirable. It was justified on the claim that our arm cannot hold
still at ~9 ms; unpinned, it holds still to **0.94%**. Longer arms remain defensible for margin, but
they are no longer the thing standing between this campaign and a credible row — **running unpinned
is**, and that costs nothing.

### Standing conditions on this row

- **UNCERTIFIED**: no host-exclusivity gate; 20/64 CPUs below 80% idle at the start.
- **STALE ELF, declared**: the harness reported the binary does not embed HEAD `c37579d3`. The SVG
  render path (`fm-core`, `fm-parser`, `fm-layout`, `fm-render-svg`) is untouched since `816a781e`,
  and BOTH arms use the same ELF, so staleness cannot bias a pinned-vs-unpinned comparison — but the
  absolute ratio describes `816a781e`, not HEAD.
- Unpinned means **clocks are uncontrolled**: cross-core spread ~3.0x, per-arm mean MHz
  3160 / 2858 / 3363 / 3668. This is why it is a bound.
- Window, verified: idle 84.94%, iowait 0.03%, loadavg 9.68/12.15/14.78, per-arm
  `[9.59,12.05,14.72]` → `[9.51,11.94,14.66]`.

**The general lesson, worth more than the number: pinning to one core to REDUCE noise added ~70% of
it.** Before blaming a shared host for a ~1.5x swing, re-run unpinned — it is one flag and no build.

## REFINEMENT: pinning is a BET on one core, not a deterministic pathology

Last note said pinning to one core "contributed ~70% of the noise". That was drawn from two pinned
brackets (1.7078x and 1.7175x) against several unpinned ones. A **third pinned bracket, measured live
on 2026-08-17, behaved perfectly**:

```
PINS: fm cpu50 @ 4195 MHz; incumbent 8 cpus, starved=False; 6/64 cpus busy
A1 fm  90,443 ns  batch 38      A2 fm  92,758 ns  batch 33
fm drift 1.0256x  ->  worst bound 374.1x
```

Pinning was genuinely applied, and the bracket still drifted only 2.6%. So the earlier claim needs
narrowing: **pinning does not always produce the pathology.**

### What separates the good pinned run from the bad ones

The `batch` does, and it does so directly:

| bracket | pinned? | cpus busy at pin time | batch | drift |
|---|---|---:|---:|---:|
| bd-8557 bad | yes | 19/64 | **22** | 1.7078x |
| bd-hmfi bad | yes | — | **20** | 1.7175x |
| this one | yes | **6/64** | **38 / 33** | 1.0256x |
| unpinned rows | no | 16-20/64 | 37-39 | 1.0094-1.0481x |

The coherent reading: pinning is a **bet on a single core**, and it loses when that core has a
competing tenant. Unpinned, the scheduler simply migrates away from contention; pinned, the arm is
stuck with whatever else wants that core. In a window with 6/64 busy the bet wins; at 19/64 it lost.

### Why this strengthens the case for the gate over a policy change

It vindicates leaving the pinning default alone and gating the **symptom** instead. A rule of
"always pass `--no-pin`" would be superstition dressed as method — it would have rejected the
374.1x bracket above, which is fine. The drift control keys on what actually went wrong, so it
catches a bad bracket whether or not pinning caused it, and admits a good one whether or not it was
pinned.

That case is now pinned into the self-test as `("pinned but healthy", …, must_refuse=False)`
specifically so nobody later "simplifies" the gate into *refuse if pinned*. The self-test therefore
constrains the gate from both sides: it must refuse the two pathological brackets and admit a
**pinned** one that was fine.

⚠️ The `--no-pin` advice is still the right default action when a bracket DOES drift — it removes the
bet entirely, and it costs one flag and no build. It is just not a diagnosis.

## The cross-core clock spread is WORST in the quiet windows we wait for — and it costs less than it looks

Every A/B/B/A row this campaign has produced ends with the harness's caveat: *"a cross-core spread
near 3x is why this is a bound and not an estimate."* Two things about that are worth recording,
because they point in opposite directions.

### 1. Waiting for a quiet window MAXIMISES the spread

A peer's calibration (`ScarletMeadow`, 2026-08-16) measured every core at 3914-3917 MHz under load
~42 — a spread of **1.001x** — and explained the mechanism: idle cores drop toward 1429 MHz while
busy ones boost, so the spread is a function of how UNEVEN the load is, not a fixed property of the
box. Pooling that with every spread this pane has recorded:

| loadavg (1-min) | cross-core spread | source |
|---:|---:|---|
| 7.4 | 3.003x | banked run 3 |
| 9.5 | 3.003x | unpinned bracket |
| 11.2 | 2.922x | pinned bracket |
| 13.7 | 2.881x | direct `/proc/cpuinfo` sample, 36/64 cores above 2500 MHz |
| 23.4 | 2.861x | first pinned bracket |
| ~42 | **1.001x** | peer calibration |

So the campaign's core strategy — hold out for the quietest window before certifying — **puts every
row in the regime where clocks are least uniform.** That is the opposite of the intuition the
strategy runs on.

⚠️ **Do not over-read the trend.** Five of the six points are mine and all sit in load 7-23, where
the spread is essentially FLAT at 2.86-3.00x with only a slight decline. The collapse to 1.001x rests
on a single borrowed observation, and **the range between load 23 and 42 is unmeasured.** The
mechanism is coherent and both endpoints are real; the shape in between is not established, and a
"certify under heavy load instead" recommendation would be running ahead of the evidence.

### 2. The realised variation is ~30x smaller than the spread permits

The more useful observation is that the spread is a bound on *potential* variation, not a measurement
of realised variation — and the realised numbers are far tighter. In exactly those ~3x-spread quiet
windows:

| bracket | spread | fm drift between identical repeats |
|---|---:|---:|
| banked run 3 | 2.86-3.01x | **1.0415x** |
| unpinned | 3.00x | **1.0094x** |
| banked runs 1, 2 | ~3x | 1.0204x, 1.0481x |

A 3x cross-core spread would permit enormous swing if an arm actually migrated between a 1429 MHz
core and a 4300 MHz one mid-measurement. **It does not happen**: the same binary on the same input
reproduces to within 1-4%. The scheduler evidently keeps a short arm on comparable cores, so the
spread over-states the exposure by roughly an order of magnitude.

**This is an argument for keeping the caveat, not dropping it.** It stays honest about a mechanism
that COULD bite, and the cheap empirical check on whether it DID bite already exists and is now
gated: the fm drift itself (`check_drift_control`, `ee930f50`). Spread says what the host could do to
a row; drift says what it did. Quote both, and let the drift carry the weight.

## CORRECTION: the spread tracks CORE OCCUPANCY, not loadavg — and it is a single-core statistic

Last note tabulated cross-core clock spread against **loadavg** and observed it collapsing to 1.001x
at load ~42. I flagged load 23-42 as unmeasured. The host has now sat in that range and the
prediction FAILED:

```
load=30.22  min=1429  max=4102  spread=2.871x  mean=2374   cores >2.5GHz: 28/64
load=30.22  min=1429  max=4093  spread=2.864x  mean=2635   cores >2.5GHz: 34/64
load=29.01  min=1429  max=4104  spread=2.872x  mean=2635   cores >2.5GHz: 30/64
load=29.01  min=1429  max=4093  spread=2.864x  mean=2340   cores >2.5GHz: 25/64
load=29.01  min=1429  max=4178  spread=2.923x  mean=3347   cores >2.5GHz: 61/64
```

At load ~30 the spread is **2.86-2.92x** — indistinguishable from the load 7-23 rows. There is no
smooth decline toward the peer's 1.001x, so **loadavg was the wrong variable and my table was
mis-framed.**

### What the discriminating variable actually is

The peer's load-42 observation had every core at **3914-3917 MHz** — a *minimum* of 3914, meaning no
core was idle. Here at load 30 the minimum is 1429 MHz with only 25-34 of 64 cores above 2.5 GHz.
Same loadavg range, completely different occupancy.

The reason is that **loadavg counts runnable AND uninterruptibly-blocked tasks.** With three or four
concurrent builds doing heavy I/O, most of that load is blocked in D-state, not occupying a core —
which is exactly the high-load/high-idle pattern seen repeatedly this campaign (load 35 at 83% idle).
So the right variable is **how many cores are actually occupied**, and `idle %` or a busy-core count
measures it; loadavg does not.

### The statistic is fragile in a way that matters more than the trend

The fifth sample is the instructive one: **61 of 64 cores above 2.5 GHz, mean 3347 MHz, and the
spread still 2.923x** — because a single core sat at 1429. `max/min` is a **worst-pair** statistic
pinned by one idle core, so it barely moves until the very last core is occupied.

That has a direct consequence for every row this campaign has banked. The harness caveat — *"a
cross-core spread near 3x is why this is a bound and not an estimate"* — is computed from the two
most extreme cores on the box, **not from the cores an arm ran on**. An arm pinned or scheduled onto
busy cores can see a near-uniform 3.3-4.1 GHz while the reported spread reads 2.9x purely because one
unrelated core is parked at its floor.

This is the mechanism behind the previous note's other half: realised fm drift of 1.0094-1.0481x in
windows reporting ~3x spread. The two numbers were never in tension. **The spread is not a property
of the measurement; it is a property of the quietest core on the machine.**

⚠️ Still do not act on this by deleting the caveat. A worst-pair bound that is usually loose is still
a bound, and the honest fix is the one already in place: quote the spread as provenance, and let the
**drift** — which is computed from the arms that actually ran, and is now gated — carry the verdict.
What should change is the *reading*: a 3x spread is not evidence that a row is shaky.

## CORRECTION: the rep cost model was wrong by ~32x, and USING the flag is what showed it

I published a cost table deriving rep counts from the measured per-op median — 93.4 µs per render,
so ~322,000 reps for a ~30 s arm — and then built `--fm-reps` from it. The arithmetic is wrong, and
I found out only by running the flag I had justified with it.

`--fm-reps 100000` was expected to take about nine seconds per arm. It ran past **600 seconds** and
had to be backgrounded.

### What the harness actually does

`rounds = item.reps` (`headtohead.rs:1886`, `:1987`) drives the measurement loop, and each round runs
a CALIBRATED BATCH: `calibration_target_ns` is 3 ms, and `batch` is sized so one round takes about
that long. The per-op median is what a batch REPORTS, not what a rep COSTS.

```
arm wall time  ~=  reps x 3 ms      correct
arm wall time  ~=  reps x 93 us     what I published, out by the batch factor (~32x here)
```

| target arm | reps, correctly | reps, as published | what the published figure really costs |
|---|---:|---:|---:|
| 30 s | **~10,000** | 322,000 | ~16 minutes |
| 60 s | ~20,000 | — | — |

The corpus default of `reps_rs` 100 is therefore a **~0.3 s** arm, not the ~9 ms stated earlier in
this document. That 9 ms was the per-op median times the rep count, which corresponds to nothing the
harness does.

### What survives the correction, and what does not

The ARGUMENT for a per-arm override survives: at 0.3 s the fm arm is still far below the host's ~10 s
interference period, so scaling the short arm alone is still the right lever. What does not survive
is the number attached to it — someone following my table would have booked a quiet window for a run
taking sixteen minutes an arm and concluded the harness had hung.

⚠️ The lesson is NOT "check the arithmetic". The per-op median was correctly measured; I combined a
real number with a wrong model of the loop it came from, and no amount of re-checking the
multiplication would have exposed that. Reading `paired()` and `calibration_target_ns` would have,
in about two minutes.

## Ready for the next window

A fresh `headtohead` binary exists and is provenance-checked, so a genuinely quiet window needs no
build first:

```
path    target/local/release/examples/headtohead   (path from --message-format=json)
sha256  2ff98c0b1a5640f76b984086db56d8278532bcfa04c085867eebd9bb25330d46
HEAD    5859158d
```

Freshness asserted rather than assumed: its mtime exceeds the last commit to every one of
`fm-core`, `fm-parser`, `fm-layout`, `fm-render-svg` and `fm-cli`. Note that the same binary went
stale within hours on the previous two occasions this was recorded — re-assert before measuring
rather than trusting this block.

**⚠️ Do not use `target/release/examples/headtohead`.** A second, stale copy lives there; the
`--message-format=json` path is `target/local/release`. That stale copy previously predated 24
commits to the measured crates.
