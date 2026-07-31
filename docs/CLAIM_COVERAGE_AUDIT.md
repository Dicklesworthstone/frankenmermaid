# Claim coverage audit — how much of our KEEP base rests on an incumbent ratio

**Run:** 2026-07-30, cc lane (`LilacPike`), unprompted, per the fleet policy that a perf KEEP requires a
vs-incumbent ratio. Nothing was deleted or weakened; this is inventory only.

## The number

**225 KEEP claims total. 8 carry a vs-incumbent ratio measured with mermaid-js live in the same
invocation. 217 do not — 96.4%.**

Counted at the level of individual measured rows rather than claim sections, the certified incumbent
base is **20 ratio rows**.

| Source | KEEP claims | With live same-invocation incumbent arm |
|---|---:|---:|
| `docs/PERF_LEDGER.md` (`Campaign result class:` marked) | 11 | 8 |
| `docs/NEGATIVE_EVIDENCE.md` (`LANDED` / `WIN` / `KEPT` rows) | 214 | 0 |
| **Total** | **225** | **8** |

The marker counted is `**Legacy incumbent arm (same invocation):**`, the repo's own machine-readable
contract for "the legacy incumbent ran side-by-side in this invocation". 53 of the 214
negative-evidence KEEP rows *mention* mermaid-js; none carries that marker, and mentioning the
incumbent is not measuring against it.

## A second gap the headline number hides

Of the 8 incumbent-backed sections (20 ratio rows), **only 1 section / 3 rows carries a cross-engine
output-equivalence verdict.** The other 17 rows predate the `bd-evx6` gate. This matters more than it
sounds: `bd-4isi` has since proved that frankenmermaid drops class-diagram fields and methods, so any
corpus containing class diagrams was timing *our partial render against mermaid's full one*.

Contamination map, derived from the corpus generators:

| Corpus | Class-diagram content | Status |
|---|---|---|
| `ci_batch_500`, `doc_build_40` (`docBuild`) | 1 of every 5 diagrams | **contaminated** by bd-4isi |
| `docs_site_50`, `docs_site_200` (`docsSite`) | ~9% (`roll` 0.74–0.83) | **contaminated** by bd-4isi |
| `schema_catalog_25` | ER only | bd-4isi-clean; ER equivalence is text-tier only |
| `typing_trace_60` | flowchart only | expected clean, no verdict on file |
| `ci_equiv_512` | flowchart only, class deliberately excluded | verified 512/512 |

## Ranked conversion queue

Ordered by how load-bearing the claim is where a user could act on it.

**Tier 0 — published in README: nothing to convert.** The only numeric performance claims in
`README.md` are the `ci_equiv_512` sweep rows, which already carry an incumbent ratio, an equivalence
verdict, and the corrected null gate. Every other perf-shaped string in `README.md` and `CHANGELOG.md`
is a *disclaimer* ("no speedup ratio is stated for these rows", "a renderer that omits content is not
a faster implementation"), not a claim. **The public surface carries zero unsupported perf claims.**
The 217 unsupported claims are all ledger-internal.

**Tier 1 — certified, incumbent-backed, and contaminated.** These rank highest because they read as
our strongest results while comparing unequal work. They need re-measurement after `bd-4isi`, not
conversion:

1. 500-diagram CI caller-thread sweep — 7 rows, up to 16,321.565740×
2. 500-diagram CI render — 1 row, 923.056028×
3. `docs_site_50` / `docs_site_200` — 2 of the 6 "realistic whole jobs" rows

**Tier 2 — incumbent-backed, no equivalence verdict, contamination not established.** Cheapest fix in
the queue: run the existing gate against them.

4. `schema_catalog_25` (ER; tier-2 topology not claimed for ER)
5. `typing_trace_60`
6. 201-revision live-edit trace
7. two short-row quiet retries (2 rows)
8. 13-row bracketed base slice
9. head-to-head median-CI section

**Tier 3 — 214 ledger-internal levers, never published.** See below; most of these are not
convertible units at all.

## What cannot be converted, and why

This is a different problem from "nobody got around to measuring it", so it is stated separately.

**No incumbent arm exists for the surface (~30 claims).** mermaid-js has no terminal renderer
(3 claims), does not parse DOT (11), and no wasm arm is wired (16). These can never carry an incumbent
ratio and should stay labeled maintenance-only permanently rather than sitting in a queue implying
future conversion.

**No equal-work sub-phase boundary (~139 claims).** Layout (93), SVG render (28), and
incremental/live-edit (18) levers cannot *individually* carry an incumbent ratio, because mermaid-js
exposes no public API to run layout or render alone over the same input with equal work. Their only
valid competitive expression is the whole-job number they feed. **The convertible unit here is the
workload, not the lever** — which is why the queue above is ~9 workload certifications rather than 217
line items.

**Genuinely convertible but unwired (45 claims).** Parse levers are the one sub-phase with a real
incumbent API: `mermaid.parse()`. The harness already calls it — but only as an *untimed acceptance
probe* outside the timed region (`mermaid_bench.mjs` `PAGE_PROBE`). A timed parse-only arm does not
exist. Building one would convert the largest genuinely-convertible block in the repo.

**Method caveat.** The 214-row surface classification is keyword-based over section headings and
bodies, not hand-verified, so the per-bucket splits (93/45/28/18/16/11/3) are approximate. The
225 / 8 / 217 headline and the Tier 0–2 rows were counted from explicit contract markers and checked
individually.
