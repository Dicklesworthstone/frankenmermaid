# Ledger Resurrection Audit — frankenmermaid

**Campaign:** `perf-campaign-20260725`, fleet Meta-Lever #1.
**Lane:** cc / STRUCTURAL (`BoldPanther`, Opus 5).
**Audited:** `docs/NEGATIVE_EVIDENCE.md` @ `ca4e1d65`, 667 entries / 19,084 lines.

The premise of the fleet-wide audit is that a large fraction of negative-evidence rows are **VOID** —
not "the lever did not work" but "the measurement could not have detected the lever". frankenlibc
found 39 of 93 rows void and turned rank #1 into a shipped 17.5–18.7× win. This is that audit, run
against frankenmermaid's ledger.

**Headline: the yield here is already realized, and it was large.** frankenmermaid independently
discovered both void classes in July 2026 and worked the queue. The four highest-self-time void rows
in this ledger have all been re-run and four distinct levers were re-won and shipped. What remains
void is a long tail of small-effect rows, plus one class nobody has mined — and that class is not a
measurement bug, it is a **corpus** bug. See §5, which is the part of this document that is new.

---

## 1. Method

`docs/NEGATIVE_EVIDENCE.md` is split on `### ` headings (667 entries). An entry is **reject-class**
if its heading or its `Verdict:` line carries a rejecting word (REJECT/REJECTED/NO-SHIP/REVERTED/
NEGATIVE/WASH/ZERO-GAIN/BLOCKED/INVALID/…), and its heading does not lead with a keeping word
(WIN/KEPT/LANDED/VERIFIED). That yields **251 reject-class rows**, of which 21 are profile/blocker
findings rather than levers under test.

Each row is then tested against the campaign's VOID predicates, mechanically:

| Predicate | How it is detected |
|---|---|
| No A/A null control | body contains none of `A/B/null`, `null control`, `null arm`, `A/A`, `noop floor` |
| Killed by the `cv<5%` gate | body states a CV-gate failure **and** states no measured regression |
| Target frame ~0% self-time | body says `0.000%`, "never runs", "not exercised", "dead code" |
| Effect inside the null floor | largest quoted delta < **1.2%**, this repo's calibrated A/A floor at `min_sample=2 ms, min_of=3` |
| No bench-binary sha256 | body records no ELF/binary hash (the SVG output hashes this repo records are byte-identity proof, not provenance) |

Grades:

- **VOID-A** — the measurement *could not* have detected the lever. Resurrect.
- **VOID-B** — no null control **and** the quoted effect is inside the 2× decidability margin
  (< 2.4%). Undecidable as recorded; not evidence either way.
- **SOUND-noNull** — no null control, but the measured effect (usually a regression of 3–40%) is so
  far outside any plausible floor that the missing control does not change the verdict.
- **SOUND** — null control recorded and effect outside the floor.

The classifier is deliberately mechanical so the result is reproducible rather than a matter of
taste; the top of the ranked queue is then hand-verified against the source tree and git history
(§3), and that hand check is what the yield numbers below are based on.

---

## 2. Result

| Bucket | Rows |
|---|---:|
| Ledger entries total | 667 |
| Reject-class rows audited | **251** |
| — of those, findings not levers | 21 |
| **VOID-A** (measurement could not detect the lever) | **19** |
| **VOID-B** (undecidable as recorded) | **43** |
| SOUND-noNull | 155 |
| SOUND | 13 |
| **Void rate** | **62 / 251 = 24.7%** |

Two provenance facts about the ledger as a whole, both consistent with what the rest of the fleet
found:

- **22 of 251** reject-class rows record an A/A null control (8.8%). The discipline arrives with
  `4aa7911b` (2026-07-10); essentially everything before that date is null-free.
- **5 of 251** record a bench-binary sha256 (2.0%). Same origin — `ae879055` introduced
  self-reporting ELF hashing on 2026-07-10.

Neither number is an indictment: they date the adoption of the harness contract this repo then wrote
up for the rest of the fleet (`docs/CROSS_REPO_RECOMMENDATION_bench_harness_contract.md`).

The full 251-row table is in [`LEDGER_RESURRECTION_TABLE.md`](LEDGER_RESURRECTION_TABLE.md).

---

## 3. The ranked queue, and what happened to it

Ranked by the profile self-time of the target frame, as the campaign specifies. Every row here was
hand-verified against the current source tree and `git log`.

| # | Ledger row | Target frame self-time | Why void | Status **today** |
|---|---|---:|---|---|
| 1 | flat-CSR barycenter, attempts 1–4 (L13338/13362/13380/13397) | **76.84%** | 4 consecutive rejects of a **2.57–7.85×** effect whose A/A nulls sat at 0.987–1.002×; every row states "REJECT THE SAMPLE, NOT THE LEVER" and dies on `cv<5%` | **RE-WON** — landed `460990ab` "index barycenter sweeps with flat CSR (bd-1buv.4)". `FLAT_CSR` is a const-generic arm of `reorder_rank_by_barycenter` in `crates/fm-layout/src/lib.rs` today. |
| 2 | Barycenter sweep precomputed edge adjacency (L3261) | **47.64%** | benched on `layout_wide/*`, whose inputs route to the **Tree** algorithm — self-time of the function under test was **0.000%** | **RE-WON** — reopened by `5feb977b`/`1db8b00e`, redesigned onto a CSR primitive, shipped as #1. |
| 3 | Packed crossing count (L13590) | **24.56%** | 1.061–1.111× measured with A/A at 1.0003×/0.9991×; "every median clears 3%, but no row has both CVs below 5%" | **RE-WON** — landed `aa4d10cf` "reuse packed frontier for crossing count". `total_crossings_packed` is on the live path (`lib.rs:9917`, `:11382`). |
| 4 | `write_escaped_text` clean-scan fast path (L2355) | 0.00% render as measured | rejected as a wash on a corpus whose labels never hit the path | **RE-WON** — landed `bdbff236` (bd-1buv.61), **−5.0%** on flowchart-300, byte-identical. |
| 5 | Canvas dotted-edge dash slice (L16912) | n/a — never timed | the strict-remote gate never reached timing; the worker timed out building `highs-sys` and neither arm executed | **RE-WON** — re-run after an untimed warm-up build; the very next ledger entry is its LANDED row. |
| 6 | Dense crossing-count position maps (L3341) | 0.000% | same dead-bench root cause as #2 | **RE-RUN, correctly closed** — headroom re-measured and not proven; the row now says so. |
| 7 | Flat-array `total_crossings` tables (L3373) | 0.000% | same dead-bench root cause | **RE-RUN, correctly closed** — re-measured headroom ≤0.81%, below the floor. |
| 8 | `bd-1buv.2` micro-frontier blocker (L19037/L19061) | top frame 6.95% → 5.86% | gated on `cv<5%`; two candidate rows discarded at CV 5.64%/5.44% | **STANDING** — this is the cod lane's frontier blocker, not a lever. Under the median-CI gate those two discarded rows are re-decidable; flagged to that lane. |
| 9 | Dead per-node `outgoing.sort_by` in `rank_assignment` (L16634) | not quoted | the removal is **provably byte-identical** but was decided on Criterion **wall time**, which produced opposite-signed significant results (−7.60% / +4.99% / +1.32%) — i.e. it measured code layout, not work | **VOID-instrument, low yield** — see §4. |
| 10–19 | remaining VOID-A | mostly "dead on the benched workload" | see §5 | **the new class** |

### Resurrection yield

| Metric | Count |
|---|---:|
| Entries audited | 251 |
| Void | 62 (24.7%) |
| Void rows re-run under a corrected harness | 7 distinct levers (ranks 1–7) |
| **Re-won and shipped** | **4** (`460990ab`, `aa4d10cf`, `bdbff236`, Canvas dash) |
| Re-run and correctly closed | 3 |
| Re-won by this audit (new) | 0 — the queue was already worked; see §5 for where the remaining value is |

**The method works and it paid here.** Four shipped levers, one of them a 2.57–7.85× layout win,
came out of rows that had been sitting in this ledger marked REJECT. That is the frankenlibc result
reproduced independently, and it is the strongest available argument for running this audit in the
eight repos that have not.

---

## 4. Rows that are void but not worth resurrecting

Honesty requires separating "the measurement was invalid" from "the lever is worth another turn".

- **Rank 9, the dead `outgoing.sort_by`.** The gate was wrong: a byte-identical removal of work was
  decided on wall time inside this repo's own documented ~5% code-layout noise floor, with no null
  control and no instruction count — and this repo has separately established that the instruction
  floor is 0.03% while the wall floor is ~5%, so instruction count is the instrument that decides
  it. But the work removed is ~600 one-time sorts of ≤4 elements. The correct verdict is
  *undecidable-and-negligible*, and the lesson recorded on that row ("dead-code removal is only a
  perf lever when the dead work is actually hot") is right. Retry predicate: only alongside another
  edit to `rank_assignment`, gated on instruction count, never on wall.
- **The 43 VOID-B rows.** By construction their quoted effects are under 2.4%, i.e. at or inside the
  decidability band. Re-running them one at a time costs a turn each for an expected yield at the
  floor. Retry predicate: re-decide a VOID-B row only when a source change puts its frame back above
  3% self-time in a profile of the workload that actually routes through it.

---

## 5. The class nobody has mined: **the corpus is the void-maker**

This is the finding this audit contributes that the July sweep did not.

Both void classes discovered so far are the same mistake at different scopes:

> *the benchmark did not exercise the code under test.*

For the crossing-minimization rows the scope was an **algorithm**: `gen_wide()` routed to Tree, so
the Sugiyama function under test ran 0.000% of the time. For `write_escaped_text` it was an **input
property**: the corpus labels never took the escape path.

There is a third scope, and every row in this ledger is exposed to it: **workload scale**.

The head-to-head corpus tops out at 500-node flowcharts and 512-node layered DAGs. Every self-time
percentage in this ledger — including the ones that justify the standing "no admissible lever" blocker
at rank 8 — is measured on that corpus. A self-time distribution is not a property of the code; it is
a property of the code *and the input*. A frame that is 2% at n=500 can be 20% at n=10,000 if its
cost is superlinear in the input, and a frame that is "dead on all benches" is dead only on the
benches that exist.

Ten of the 19 VOID-A rows are of exactly the form "the frame is dead / gated out on the benched
workload" — for example the e-graph `layer_edges_between_ranks` probe rows (L16747, L16771: "egraph
gated out of scc", "dead on all benches") and `graph_metrics_cache_key` (L8089: "DEAD on hot path").
Those rows are not evidence that the levers are worthless. They are evidence that **no benchmark in
this repo has ever routed through them**, which is the exact predicate the campaign names as VOID.

**Therefore the resurrection queue for frankenmermaid is not a list of levers. It is a list of
workloads.** That is why this lane's structural assignment — extend the corpus into 5–10k-node
architecture and schema diagrams, 200-revision editing sessions, and multi-diagram CI batches — is
the resurrection work, not a separate project. The extended corpus landed alongside this document
(`scripts/headtohead/corpus.mjs`, six new items across three classes, with a DNF protocol for the
regime where mermaid-js does not finish at all).

**Re-audit predicate.** Once a profile of the new workload classes exists, every VOID-A row of the
"dead frame" form must be re-checked against *that* profile before it may be cited as closed. A row
that says "0.000% self-time" is a statement about a corpus, and the corpus has changed.

---

## 6. Provenance

- Classifier and raw output: reproducible from `docs/NEGATIVE_EVIDENCE.md` at `ca4e1d65` by the
  predicates in §1; the generated table is `docs/LEDGER_RESURRECTION_TABLE.md`.
- Harness contract adoption (campaign §2) in the benches this lane touched: the head-to-head runner
  now **self-reports the SHA-256 of the ELF that is executing** (`crates/fm-cli/examples/headtohead.rs`,
  emitted as the first stdout record, verified against `sha256sum` of the on-disk binary:
  `f4d035f2cf4676424fe402728f4bcdbee008d96332a68f0964bcdab2ceca1289`). The A/A null control and the
  MAD-not-CV dispersion gate were already in place and are this repo's own contribution to the fleet.
