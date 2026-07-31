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

At the 2026-07-30 audit snapshot, of the 8 incumbent-backed sections (20 ratio rows), **only 1
section / 3 rows carried a cross-engine output-equivalence verdict.** The other 17 rows predated the
`bd-evx6` gate. This mattered more than it sounded: `bd-4isi` proved that frankenmermaid dropped
class-diagram fields and methods, so any corpus containing class diagrams was timing *our partial
render against mermaid's full one*.

**Current semantic-conversion update (2026-07-31).** The five demoted class-mixed jobs now pass the
current oracle but still require fresh timing. Current semantic artifacts also close the cheap
conversion queue for `schema_catalog_25`, the 201-revision live-edit job, the two short rows, the
13-row bracketed base slice, and the original 15 measurable median-CI rows. Those artifacts do not
retroactively validate historical ratios: each ledger row remains semantic-only until a fresh
same-invocation incumbent run clears the corrected null gate. `typing_trace_60` is the sole
remaining Tier-2 workload without a current output-equivalence verdict.

Contamination map, derived from the corpus generators:

| Corpus | Class-diagram content | Status |
|---|---|---|
| `ci_batch_500`, `doc_build_40` (`docBuild`) | 1 of every 5 diagrams | historical ratios demoted; current semantic artifact passes |
| `docs_site_50`, `docs_site_200` (`docsSite`) | ~9% (`roll` 0.74–0.83) | historical ratios demoted; current semantic artifact passes |
| `schema_catalog_25` | ER only | current artifact passes 25/25 |
| `typing_trace_60` | flowchart only | no current verdict on file; next conversion item |
| `ci_equiv_512` | flowchart only, class deliberately excluded | verified 512/512 |

## Ranked conversion queue

Ordered by how load-bearing the claim is where a user could act on it.

**Tier 0 — published in README.** Two blocks of numbers reach users, and they sit in different
categories:

- The `ci_equiv_512` sweep rows already carry an incumbent ratio, an equivalence verdict, and the
  corrected null gate. Nothing to convert.
- The structural-weakness table (7 rows: "2,000-node flowchart | 1.43 ms | `RangeError` after 6.5 s")
  publishes **absolute frankenmermaid timings with no ratio**. These are not unmeasured — the
  incumbent was run and *crashed*, so no ratio is derivable. That is a third category, distinct from
  both "measured against the incumbent" and "no incumbent arm exists": **incumbent attempted, could
  not complete.** README already states this ("This is a crash, not a timeout, so no speedup ratio is
  stated for these rows"). They cannot be converted, and correctly so — inventing a ratio from a
  crash budget would be a fabricated number. None of these workloads is class-contaminated
  (flowchart, architecture, and ER only).

Every other perf-shaped string in `README.md` and `CHANGELOG.md` is a *disclaimer*, not a claim. **No
unsupported ratio claim reaches the public surface**, and the 217 KEEP claims lacking an incumbent
ratio are all ledger-internal.

**Tier 1 — incumbent-backed and contaminated.** These rank highest because they read as our strongest
results while comparing unequal work. They need re-measurement, not conversion.

*Correction (2026-07-30):* this audit originally described these rows as standing certifications. They
are not. Commit `5bb2e044` ("Historical correction") already demoted `class_50`, `doc_build_40`,
`ci_batch_500`, `docs_site_50`, and `docs_site_200` to "known to contain this unequal-work class
surface and not current campaign output". The demotion landed before this audit and was missed on the
first pass. The contamination finding stands; the characterization of the rows as still-certified does
not. Their re-measurement is gated on three P0 bugs, not one: `bd-4isi` (members dropped — **fixed,
see below**), `bd-92b6` (`o--`/`*--` fall through to `--`, creating phantom nodes), and `bd-yq3k`
(state labels misparsed as endpoint syntax).

1. 500-diagram CI caller-thread sweep — 7 rows, up to 16,321.565740×
2. 500-diagram CI render — 1 row, 923.056028×
3. `docs_site_50` / `docs_site_200` — 2 of the 6 "realistic whole jobs" rows

*Current status (2026-07-31):* `class_50`, `doc_build_40`, `ci_batch_500`, `docs_site_50`, and
`docs_site_200` all pass the current linked oracle with zero divergent/unverified. They remain
demoted because semantic repair does not refresh a historical timing ratio.

**Tier 2 — incumbent-backed, no equivalence verdict at the audit snapshot, contamination not
established.** The existing gate has now closed every item except `typing_trace_60`.

4. `schema_catalog_25` — current artifact passes 25/25.
5. `typing_trace_60` — **still open; next conversion item.**
6. 201-revision live-edit trace — current artifact passes 201/201.
7. two short-row quiet retries (2 rows) — both current artifacts pass.
8. 13-row bracketed base slice — current artifact passes 13/13 and 72/72 revisions.
9. head-to-head median-CI section — unified current artifact passes all 15 measurable rows and
   74/74 revisions; one Rust null median fails the corrected 2% clause, so the result is expressly
   semantic-only.

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
