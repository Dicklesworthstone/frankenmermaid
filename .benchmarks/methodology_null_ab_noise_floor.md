# Methodology: a null A/B proves the fleet-load bench noise floor is ±8-11% with FALSE significance

**Date:** 2026-06-28 — **Agent:** BlackThrush — **HEAD:** acf684b

## The technique

A **null A/B** runs the *identical* code through both phases of the stash-swap harness (save a
baseline, then re-measure with no source change). The "change" it reports is, by construction, pure
measurement noise — it quantifies the floor below which no real signal is distinguishable.

## Result (this turn, fleet load ~25-60)

`parse/wide`, fm-parser, same code both phases:

| bench | "change" (should be ~0) | criterion verdict |
|---|---:|---|
| `parse/wide/8x16`  | −0.8% (p=0.62) | NS ✓ |
| `parse/wide/12x24` | **−8.9% (p=0.00)** | "improved" ✗ FALSE |
| `parse/wide/16x32` | **+10.6% (p=0.00)** | "regressed" ✗ FALSE |

Identical code reads as a **±~10% change with p<0.05** in two of three sizes, in *opposite*
directions (12x24 "−8.9% faster" while 16x32 "+10.6% slower") — load fluctuation across the run, not
code. So at this fleet load the noise floor is **±8-11%, and criterion's p-value does not protect
against it** (the drift is correlated within a run, defeating the t-test's independence assumption).

## Why this matters (do not chase sub-floor reads under load)

Every remaining frankenmermaid lever is below this floor and therefore **unmeasurable while the fleet
is loaded**:
- layout tree+spans `Vec<Vec>`→CSR adjacency: ~3-4% layout (~1% pipeline)
- any render micro-lever (set-retain, write_fixed2, describe_node): ~0-2%

This retroactively explains two earlier results: the `describe_node` "−8% single-order" read that the
both-order A/B exposed as warm-bias (~0), and the a523af9 items-presize bundle whose both-order was
"contradictory" (+6.9% vs +9.8%). **Both were this same load-noise, not real signal.**

## Guidance

1. **Gate A/Bs on a quiet fleet.** Run a null A/B first; if it shows >±3%, defer real measurement —
   you cannot distinguish a <10% lever from noise.
2. **Direction-consistency across both orders is necessary but NOT sufficient** under load — the null
   A/B here was direction-*inconsistent* across sizes, but a single size can drift one way in both
   orders. Prefer a quiet fleet + the geometric-mean both-order correction together.
3. The wins that *did* land this session (parse_label ~4-9%, edge right-contains ~5-8%) cleared this
   floor decisively and were direction-consistent at every size — that is the bar a real win must meet.

## Standing unchanged

parse + layout remain at their byte-identical floors (acf684b); the one big remaining win is the
render a11y/`data-*` output reduction (contract decision, cod-b's Mermaid comparator). A peer is
actively on render. The borderline layout CSR lever is deferred to a quiet-fleet turn where ±3% is
distinguishable.
