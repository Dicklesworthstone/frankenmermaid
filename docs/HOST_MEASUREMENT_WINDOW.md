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

**So the pre-flight check is a SEQUENCE, not a reading.** Take several one-second samples and require
them to agree; a window whose samples disagree is unmeasurable regardless of how good the best one
looks. That is also why the standing rule not to build in the window you intend to measure in
matters: immediately after a `cargo build` here, the count read 64/64 for four consecutive seconds.

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
