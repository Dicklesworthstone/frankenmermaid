# Ledger Resurrection Audit — frankenmermaid

**Campaign:** `perf-campaign-20260725`, fleet Meta-Lever #1.
**Lane:** cc / STRUCTURAL (`BoldPanther`, Opus 5).
**Audited:** `docs/NEGATIVE_EVIDENCE.md` @ `ca4e1d65`, 667 entries / 19,084 lines.
**Re-audited 2026-07-26** under frankenfs's six-class taxonomy (fleet broadcast) — see §7, which
supersedes the four-grade scheme in §1 and reaches a different answer.

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

**Certified re-measurement (cod lane, `CreamGorge`, `bd-1buv.67`).** In parallel with this audit the
cod lane re-ran ranks 1–5 on strict-remote `ovh-a` under the corrected harness — same-invocation
A/A + A/B, bootstrap median 95% CI, mandatory 2× null-CI gate, exact parity on every arm, ELF
`42e6c3bf…6f3a`. All 15 comparisons clear the gate, weakest clearance 56.81×:

| Row | SCC 100 | SCC 300 | SCC 800 |
|---|---:|---:|---:|
| direct VOID adjacency `BTreeMap`→flat-CSR | 11.945× | 21.027× | 44.255× |
| flat-CSR | 3.032× | 4.566× | 8.437× |
| single-pass | 3.481× | 4.169× | 4.775× |
| dense-rank | 1.123× | 1.114× | 1.096× |
| packed crossings | 1.159× | 1.082× | 1.061× |

Those are the same rows that four separate reject entries dismissed for failing `cv<5%`. Under a
median-CI gate they clear by one to two orders of magnitude. The two columns of the table in §3 are
complementary: the "Status today" column records the commit that shipped, this one records the
certified ratio.

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

### 5.1 The prediction, tested

The standing frontier blocker (rank 8; `docs/NEGATIVE_EVIDENCE.md` L19037/L19061) rests on a
specific claim:

> "No unledgered frame reaches 8% self and no single contained call-chain reaches 10%."

That claim was measured on `flowchart_large_500`, and on that workload it is **true** — the top frame
there is `write_uint_into` at 6.13%. Profiling the new classes with the same instrument
(non-LTO, `strip=false`, `debug=true`, symbolized, single pinned core, `perf record -F 999`;
`sha2` frames excluded as harness self-hashing outside the timed region) gives:

| Workload | Top self-time frame | Self-time | Present at `flowchart_large_500`? |
|---|---|---:|---|
| `flowchart_large_500` (baseline) | `write_uint_into` | 6.13% | — |
| `flowchart_xl_5000` | `lower_flow_document_item` | **8.72%** | yes, at 2.63% — it more than triples with scale |
| `arch_100x50` | `FxHashSet<(usize, IrNodeId)>::insert` | **7.19–8.73%** | **absent entirely** |
| `er_schema_1000x6` | `parse_mermaid_with_detection_and_config` | **9.37%** | not in the top 20 |
| `doc_build_40` | `render_svg_with_layout` | **20.02%** | not in the top 20 |
| `doc_build_40` (2nd) | `memchr…Finder::find_impl` (theme-CSS post-pass) | **9.00%** | not in the top 20 |

Four workloads, five frames at or above the 8% admission threshold, four of which do not appear in
the baseline profile at all. The blocker is not wrong about the code; it is a true statement about
`flowchart_large_500` that was being read as a statement about frankenmermaid.

The `arch_100x50` frame is the sharpest case. `FxHashSet<(usize, IrNodeId)>::insert` is the dedup set
behind `add_node_to_cluster`/`add_node_to_subgraph`, and **no benchmark in this repo has ever routed
through it at scale** — the only subgraph fixture, `flowchart_subgraph.mmd`, has four nodes. It is
the same VOID predicate as the crossing-minimization rows, one scope out.

`doc_build_40` is the second: at 40 small diagrams in a batch, the per-render fixed cost (theme
`<style>` strip + minify) is ~34% of the profile once `memmove` is counted. The ledger already names
this as "the top unmined frame … ~20% of SMALL non-flowchart renders" and bead `bd-dh1c` already
proposes memoizing it. What was missing was never the idea; it was a workload where small
non-flowchart renders are the thing being measured.

---

## 6. Provenance

- Classifier and raw output: reproducible from `docs/NEGATIVE_EVIDENCE.md` at `ca4e1d65` by the
  predicates in §1; the generated table is `docs/LEDGER_RESURRECTION_TABLE.md`.
- Harness contract adoption (campaign §2) in the benches this lane touched: the head-to-head runner
  now **self-reports the SHA-256 of the ELF that is executing** (`crates/fm-cli/examples/headtohead.rs`,
  emitted as the first stdout record, verified against `sha256sum` of the on-disk binary:
  `f4d035f2cf4676424fe402728f4bcdbee008d96332a68f0964bcdab2ceca1289`). The A/A null control and the
  MAD-not-CV dispersion gate were already in place and are this repo's own contribution to the fleet.

---

## 7. Re-audit under the six-class taxonomy (2026-07-26)

The fleet broadcast directs every repo to adopt frankenfs's taxonomy verbatim, and corrects the
campaign's own prediction: **the CV gate is not the dominant void class.** frankenfs found only 4
VOID-CV against 214 VOID-NONULL. This section re-runs frankenmermaid's audit on that basis. It
supersedes §1–§2; the §3 ranked queue and its yield stand unchanged and are re-confirmed below.

### 7.1 Classes

| Class | Meaning | Sound? |
|---|---|---|
| `VALID-PROFILE` | Rejected before any source edit, on a named frame with non-zero self-time and a computed ceiling. | ✅ |
| `VALID-MECHANISM` | No A/A null, but refuted on a **counted** mechanism — instructions/cycles/syscalls/allocations/faults unchanged. A null cannot change "no work was removed". | ✅ |
| `VALID-AB` | A/B with a recorded A/A null; the effect sits inside it. | ✅ |
| `VOID-CV` | Killed **only** by a `cv < 5%` gate. | ❌ |
| `VOID-ZEROSELF` | Target frame ~0% self-time in the profile the bench actually ran. | ❌ |
| `VOID-NONULL` | Near-1.0 ratio, no null, no counted mechanism. Cannot distinguish lever from harness. | ❌ |

### 7.2 Mechanical screen

| Metric | frankenmermaid | frankenfs (for comparison) |
|---|---:|---:|
| Ledger entries parsed | 668 | 1,031 |
| **REJECT-verdict audited** | **250** | 276 |
| VALID-AB | 18 | 34 |
| VALID-PROFILE | 1 | 12 |
| VALID-MECHANISM | 52 | 11 |
| **VOID-NONULL** | **167** | **214** |
| VOID-CV | 2 | 4 |
| VOID-ZEROSELF | 10 | 1 |
| VOID total (screen) | 179 / 250 = **71.6%** | 219 / 276 = 79.3% |
| Rows carrying a binary sha256 | 26 / 250 = **10.4%** | 30 / 276 = 10.9% |

**The broadcast's correction is confirmed independently here.** VOID-NONULL is the epidemic (167);
VOID-CV is negligible (2). My earlier §1–§2 scheme graded "no null" rows by *effect size* and so
filed 155 of them as `SOUND-noNull`; grading on **whether a counted mechanism was recorded** is the
correct test and moves most of that population. The binary-sha figures — 10.4% here, 10.9% at
frankenfs — agreeing to within half a point across two independently written screens is the
strongest signal in this table that the provenance gap is a fleet-wide property, not a local habit.

`VALID-MECHANISM` is 52 here against 11 at frankenfs. That is this repo's instruction-count
discipline showing up as a rescue: a large minority of its rejections were refuted by counting work,
not by timing it.

### 7.3 Hand adjudication — where the screen is wrong

The broadcast is emphatic that the regex is triage. Reading the ranked head confirms it: **the top
of the queue is the least void part of the population**, because larger frames got more careful
write-ups. Of the top six VOID rows by target-frame self-time, **three are overturned by hand**:

| Rank | Row | Screen | Hand verdict |
|---:|---|---|---|
| 2 | L5352 CGA axis-aligned skip, "31% self" | VOID-NONULL | **Not void.** The row *withdraws its own 31% figure*: it was taken on a stale debug binary predating the spatial-index router. It then names why the lever is ~0 — the index already minimises CGA candidates. Ranking on that 31% was my error, not the row's. |
| 5 | L9613 parser `trim_ws`, 9.46% self | VOID-NONULL | **Not void.** Records the root cause: the win needs whitespace *to strip*, and real parser trims are already-clean short substrings. Also notes `trim_ws` is **not strictly-less-work**, so it cannot be justified as monotonic either. |
| 6 | L5020 path `d` raw serialization, 8.32% self | VOID-NONULL | **Not void — it is a measured regression**, +7.60%/+11.18%/**+25.54%** across two independent measurements, with a mechanism (a 4th `AttributeValue` variant de-optimises the `write_value` match that runs for every attribute). Nowhere near 1.0. |

**A caution for anyone ranking by self-time:** rows frequently quote a self-time *in order to refute
it*. Rank 2 quotes 31% precisely to explain that the number was stale. A screen that extracts the
largest percentage in the body will rank the most thoroughly-debunked rows highest. Extract the
self-time that is the lever's **attribution**, not every percentage in the prose.

### 7.4 A proposed refinement, offered back to the fleet

frankenfs defines `VALID-MECHANISM` on a **counted** mechanism. The justifying principle — *"a null
control cannot change the fact that no work was removed"* — holds just as well when a row proves no
work was removed **structurally** rather than by a counter. Ranks 2, 5 and 6 above are all of that
shape, and none would be rescued by the counted-only definition.

Screening the whole VOID-NONULL population for a causal-refutation paragraph (`**Mechanism**`,
`**Why ~0**`, `**Root cause**`, `**Why the profile lied**`) or a stated ceiling:

| | Rows | Share of VOID-NONULL |
|---|---:|---:|
| Causal mechanism paragraph | 21 | 13% |
| Stated ceiling / Amdahl | 18 | 11% |
| **Either — rescue candidates** | **34** | **20%** |
| Neither — true VOID-NONULL | 133 | 80% |

Applying that rescue honestly (it cuts both ways — 80% are *not* rescued):

| Class | Screen | After hand adjudication |
|---|---:|---:|
| VALID-AB | 18 | 18 |
| VALID-PROFILE | 1 | 1 |
| VALID-MECHANISM (counted + structural) | 52 | **86** |
| VOID-NONULL | 167 | **133** |
| VOID-CV | 2 | 2 |
| VOID-ZEROSELF | 10 | 10 |
| **VOID total** | 179 / 250 = 71.6% | **145 / 250 = 58.0%** |

frankenmermaid's void rate is genuinely lower than frankenfs's 79.3%, and the reason is a property
of this ledger's house style — rows here characteristically explain *why* a null result is null —
not an argument for grading its own homework leniently. The 133 true VOID-NONULL rows remain void.

### 7.5 Ranked re-run queue after hand adjudication

Ranks 1, 3 and 7 of the screen were already adjudicated in §3 and are **already re-won** (`460990ab`,
`aa4d10cf`) or belong to the cod lane's standing blocker; `CreamGorge` has since certified them
against a 2× null-CI gate (§3). Rank 4 is a survey row, not a lever, and is excluded exactly as
frankenfs excludes its SURVEY class. After removing those and the three hand-rescued rows, **two
genuine re-run candidates remain**:

1. **L5492 — `build_smooth_path` `d` capacity `n*24 → n*56`** (7.11% self). Verdict was literally
   *"INCONCLUSIVE — the box…"*, load-contaminated, with an acknowledged over-allocation trade-off.
   The textbook VOID-NONULL: an effect that may be real, on a measurement that could not decide.
2. **L8376 — extend `trim_fast` to `intern_node_auto` `id.trim()` + node-parser `label.trim()`**
   (3.00% self). Verdict: *"real but sub-threshold + noise-obscured, fails the reproducible-≥3% keep
   gate"*, non-reproducible back to back. The row itself says the effect is **real**.

**Both are decidable by instruction count, and that is the general point.** VOID-NONULL is defined
by a near-1.0 *wall* ratio with no null — and this repo measured an A/A null of **±0.011% on
instructions against ±0.145% on wall** (`bd-1buv.69`, §3 of `docs/PERF_LEDGER.md`). A lever that is
sub-threshold and noise-obscured on wall is routinely decidable on instructions, provided it
**removes work** (the §2.6 scope limit: not for ISA/LTO/allocator changes). So the fleet's dominant
void class is not only a documentation failure — it is an instrument failure, and the instrument
already exists.

### 7.6 Blocker — this lane cannot execute the re-runs

Both candidates were reverted or stashed, so re-running them means re-implementing and building two
arms. **This repo is Lane L (throttled, no worker) under the allocation addendum**, and the standing
rule is to request a window rather than take one. Both rows are handed to whichever lane next holds
measurement rights in this repo, with the instrument named above. Requested on Agent Mail thread
`perf-campaign-20260725`; not started here.

Unrelated to the queue: `VOID-ISA` (frankenfs §6) was checked and does not apply. This workspace
pins `target-cpu=x86-64-v2` in `.cargo/config.toml` for a documented, measured reason
(`round_ties_even`/`floor` lowering to hardware `roundsd`/`floorss`, instructions −5.3%), and its
levers are byte-scan and formatting shaped rather than SIMD-kernel shaped. No row here was rejected
because the binary could not emit AVX2.

---

## 8. Institutionalization — because ledger integrity decays

Fleet broadcast 2 (2026-07-26) supplies the decisive data point. Void rates across the fleet:
franken_networkx 91%, frankenfs 79.3%, frankenpandas 39.1%, frankenmermaid 24.7% (71.6% under the
§7 taxonomy), **frankensqlite 1.7%**. frankensqlite is not lenient — it ran this same audit four
months ago, triggered by *this repo's* crossing-minimization finding, then **strengthened** it: an
AUDIT v2 with an exact-dispatch-count reachability proof plus `sql_pipeline_candidate_preflight`
(exit 2 = BLOCKED) that greps the ledger before any source mutation.

**The lesson is that a one-time cleanup is worth ~4 months.** A repo that audits and institutionalizes
sits at 1.7%; repos that audited once sit at 25–91%. This repo has now audited twice in two days and
would decay identically without a gate.

### `scripts/ledger_preflight.mjs`

Two modes, both blocking, no build and no worker required:

```bash
# BEFORE mutating source — has this mechanism already been rejected?
node scripts/ledger_preflight.mjs --lever "<description>" --frame <symbol>
#   exit 0  no prior REJECT matches
#   exit 2  BLOCKED — prints each matching row with its retry predicate

# BEFORE committing — is every REJECT row I am adding falsifiable?
node scripts/ledger_preflight.mjs --lint --base origin/main
#   exit 0  every new REJECT row records a null, a counted mechanism, a structural
#           refutation, or a ceiling
#   exit 1  BLOCKED — lists the rows that cannot distinguish lever from harness
```

The `--lint` predicates are **deliberately the same regexes §7 audits with**, so the gate and the
audit agree by construction: a row the gate admits is a row the audit classifies `VALID-*`, and a row
it rejects is one the audit would classify `VOID-NONULL`. If the taxonomy changes, both change
together.

Verified against real history rather than a synthetic fixture:

- `--lever "numeric node index" --frame NodeIdIndex` → **exit 2**, surfacing both 2026-07-22 numeric-
  index rejections and their retry predicates. A nonsense lever exits 0.
- `--lint --base 243f9586` → **exit 0**; the one REJECT row added since is admitted, correctly, for
  recording an A/A null.
- `--lint` against a pre-2026-06-28 base → **exit 1**, admitting 90 rows and blocking 4 that record
  none of the four justifications.

### Enforcement

`.github/workflows/ci.yml` gains a `ledger-integrity-guard` job running `--lint` against the PR merge
base. A new REJECT row that cannot distinguish the lever from the harness is now a **build failure**,
not a style note — which is what "impossible rather than discouraged" requires.

The guard deliberately accepts four justifications, not one. Requiring an A/A null on *every* row
would be wrong: a row that refutes a lever on a counted mechanism does not need a null (a null cannot
change the fact that no work was removed), and §7.3 found three rows at the head of this queue that
are sound on a structural refutation alone. A gate that forced those rows to fabricate a null control
would degrade the ledger, not protect it.

---

## 9. Correction to §7.5 — the re-run queue is one row, not two

Written during the 2026-07-26 disk emergency, document-only. Reading both §7.5 candidates *in full*
— which §7.5 had not done, having promoted them from the screen plus their verdict lines — overturns
one of them. Recording it here rather than quietly dropping it, because the failure mode is the one
this whole audit is about.

### L8376 `trim_fast` extension — WITHDRAWN, it is VALID-MECHANISM

§7.5 listed it on the strength of its verdict line, *"real but sub-threshold + noise-obscured"*. The
full row carries a **"Why it's below the bar"** paragraph that refutes the lever structurally:

> `id` is already `trim_ascii`'d by the fast edge/node parsers and the labels here have no boundary
> whitespace, so `str::trim` is already a near-no-op (checks 1 char each end). The byte-vs-char
> saving is ~2%, below the noise floor.

That is a mechanism, and it is *quantified*. It also carries an explicit do-not-retry with a durable
lesson (trims on already-trimmed inputs are near-free; only raw source lines are worth converting,
which `a39648b` already did). Under this document's own §7.4 refinement the row is
**VALID-MECHANISM (structural)** and should never have entered the queue. **My screen mis-filed it
and §7.5 repeated the error.**

The irony is exact: §7.3 warns that the regex is triage and that the head of the queue is the least
void part of the population — and then §7.5 promoted a row on its verdict line without reading its
mechanism paragraph. A screen is not a verdict *even when you wrote the screen*.

### L5492 `build_smooth_path` capacity — stands, but NOT as originally written

This one is a genuine VOID-NONULL: the A/B is unusable (box load swung 90→55 mid-run, producing an
impossible **+266%** artifact, and 16x32 sign-flipped between orders). Nothing can be concluded from
it in either direction.

But the row also names a real trade-off that a naive re-run would walk straight back into:

> most wide edges are short (n=2–3 points, orthogonal routing) and never regrew at `n*24` (a 2-point
> path is ~32 bytes < 48), so `n*56` just OVER-ALLOCATES them with no benefit

So **re-running the original `n*24 → n*56` would likely reproduce the wash**, and would burn a worker
window to learn what the row already says. The row's own retry predicate is the design:

> size per-edge from the actual point count (only bump when n>=4) and measure on a quiet box

**Execution-ready handoff** for whoever holds the first window after the all-clear:

- **Lever:** in `fm-render-svg::path::build_smooth_path`, size the `d` `String` conditionally —
  keep `n*24` for `n < 4`, use `24 + (n-1)*56` for `n >= 4`. This is the shape already landed for
  `build_smooth_path_by` (see the KEEP row *"cubic-only `d` capacity (n>=3 -> 24+(n-1)*56)"*), so it
  is a port of an accepted design, not a new one.
- **Why it is admissible:** capacity-only ⇒ byte-identical output; `with_capacity` never touches the
  surplus, so over-reserving is free but *unnecessary* over-reserving still costs the allocator a
  larger block — which is exactly the trade-off the original tripped over and the conditional avoids.
- **Attribution to re-confirm first:** `__memmove_avx` at 7.11% render self. That figure predates the
  edge-streaming wins; **re-profile before touching source** — a stale attribution is what voided
  §7.3 rank 2.
- **Gate:** instructions, not wall. A capacity change that removes a realloc+copy is work removal, so
  it is in scope for the instruction gate (A/A null ±0.011% vs ±0.145% on wall — `bd-1buv.69`), and
  wall is precisely what the original measurement could not resolve. Add the `flowchart_large_500`
  negative control: it is orthogonally routed and short-edged, so it should sit inside the null.
- **Byte-identity:** all 21 corpus items via `scripts/headtohead/run.mjs`, comparing `output_sha256`.
- **Preflight:** `node scripts/ledger_preflight.mjs --lever "build_smooth_path d capacity per-edge point count" --frame build_smooth_path` — it will surface this row and its retry predicate. Satisfying that predicate (conditional sizing + quiet box) is the admission argument, and the new row must say so.

### Revised yield

| Metric | Count |
|---|---:|
| Genuine re-run candidates after full hand adjudication | **1** (L5492, redesigned) |
| Withdrawn on full reading | 1 (L8376 → VALID-MECHANISM) |
| Blocked on | a build; this repo is Lane L, and builds are halted fleet-wide (disk) |
