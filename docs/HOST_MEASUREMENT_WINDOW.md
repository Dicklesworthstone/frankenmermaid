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
