# bd-w91d — clause 3 cannot decide half the corpus from one observation

Archive analysis only. The host is under a build freeze, and the bead's own instruction is that the
20 refused rows "must be re-run in an assigned quiet window; never re-pin or retune to force a pass".
Nothing here re-runs, re-pins or retunes anything. It recomputes clause 3's statistic from rows
already stored in `.benchmarks/headtohead/` and asks a question the bead itself raises: **how much
does that statistic move when nothing changes?**

## Method

Clause 3 refuses a row whose A/A null MEDIAN is more than 2% from 1.0. That statistic is stored on
every row as `null_control.median`, so the bias is recomputable as `|median - 1| x 100`.

Rows were grouped by `(case, engine, elf_sha256, thread_count_requested)` — a deliberately STRICT
key, because a looser one pools thread widths and binaries that are not the same run and would
inflate the apparent spread. Only groups with **three or more repeats** were scored.

## Result: 60 configurations, and half of them straddle the line

| outcome across every stored repeat of one configuration | configurations |
|---|---:|
| always below 2% — cleanly passing | 26 |
| **straddles 2% — the verdict FLIPS between repeats** | **30** |
| always at or above 2% — genuinely biased | **1** |

So clause 3 does discriminate at the extremes. Twenty-six configurations pass every time, and
`edit_trace_500x1000 / frankenmermaid` fails every time (4.171%–6.279% across 4 repeats) — that one
is a real effect, not a draw.

**For the other thirty, a single observation decides nothing.** The statistic's own run-to-run range
crosses the threshold it is being compared against:

```
flowchart_small_10  mermaid-js      thr=1     n=7    0.711% .. 11.765%   spread 11.05 pp
edit_trace_200x200  frankenmermaid            n=10   0.105% ..  5.171%   spread  5.07 pp
doc_build_40        frankenmermaid            n=6    0.177% ..  5.239%   spread  5.06 pp
ci_equiv_512        frankenmermaid  thr=1     n=10   0.007% ..  4.090%   spread  4.08 pp
```

The bead already suspected this, from three runs of `sequence_20` moving 0.299% → 1.289% → 1.990%.
The archive says it is not a quirk of that case: it is the normal behaviour of the statistic for half
the corpus.

## The highest-priority refusal is inside its own noise band

The bead names `ci_batch_500@t8` as the top requalification target: refused at **3.154%** null median
bias, taking the t=8 row out of a certified 64-thread sweep.

The archive holds **47** stored `ci_batch_500 / frankenmermaid` rows, and their bias spans
**0.000% – 4.008%**.

3.154% sits inside that range. Which means the refusal is not evidence that the measurement
environment was unfit; it is a draw from a distribution the same case produces routinely. That does
not make the row *certified* — it makes the refusal **uninformative**, which is a different and more
useful thing to know before spending an exclusive host booking on it.

⚠️ **Weaker grouping for that case, stated plainly.** Those 47 rows carry no `elf_sha256` and no
`thread_count_requested` in storage, so they could not be separated by binary or thread width. The
0.000–4.008 range therefore pools configurations that the strict key separates elsewhere, and the
true single-configuration spread may be narrower. The missing provenance is itself worth fixing —
a row that cannot say which binary produced it cannot be requalified from the archive at all.

## What follows

**Evaluate clause 3 over repeats, not over one observation.** A `k of n repeats exceed 2%` rule, or a
median across repeats, would keep the discrimination the extremes show (26 clean, 1 biased) while not
refusing thirty configurations on a coin flip. The current rule is not wrong about the statistic — it
is applied to a single draw of a statistic that needs several.

**Several of the 20 refusals may be requalifiable from stored data alone**, by pooling repeats that
already exist, before any exclusive booking is spent. That is cheaper than a quiet window and does
not touch the "never re-pin or retune" prohibition, because it changes nothing about how the rows
were measured.

⚠️ **This does not requalify anything by itself.** It is evidence about the INSTRUMENT, not about any
row's effect. Deciding which of the 20 to requalify, and on what rule, is a judgement for whoever
owns the gate — and adopting a k-of-n rule is a gate change, which by this project's own doctrine
needs its own evidence and cannot ride along on this note.
