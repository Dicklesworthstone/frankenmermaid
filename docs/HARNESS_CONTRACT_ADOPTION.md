# Bench Harness Contract — fleet adoption status

**Owner:** frankenmermaid (Lane L, allocation addendum 2026-07-25 23:15Z).
**Contract:** [`CROSS_REPO_RECOMMENDATION_bench_harness_contract.md`](CROSS_REPO_RECOMMENDATION_bench_harness_contract.md),
campaign `perf-campaign-20260725` §2.
**Audited:** 2026-07-26, all 11 campaign repos, at their then-current working trees.
**frankenmermaid correction:** `bd-w1po`, after the rollout audit re-opened the live decision path.

## How this was audited

**Ground truth is code, not intent.** A repo has adopted a part when the mechanism exists in a
bench/harness source file — not when a ledger row says it plans to. Every automated hit below was
then hand-classified, because the naive greps are badly wrong in both directions:

- frankenmermaid's only `cv < 5` hit is a doc comment *explaining that cv is unreachable*.
- frankenscipy's `cv` hits are coefficient-of-variation in **statistics library code and fuzz
  targets** — it is a SciPy port, `cv` is a domain term there.
- frankensearch's are **unit tests of its own stats helper** (`assert!(summary.cv_pct < 2.0)`).
- franken_whisper gates `null_cv <= 0.03` on the **null arm**, which is validating the control, not
  rejecting the lever.

Counting those as live cv gates would have libelled four repos. Anyone re-running this audit must
hand-classify; the grep alone is not publishable.

## Status

Part 1 = self-reporting ELF SHA-256 · Part 2 = A/A null control in the same invocation ·
Part 3 = gate on the median-CI, never on `cv`.

| Repo | P1 ELF sha | P2 A/A null | P3 median-CI gate | Live `cv` gate still in code? |
|---|:--:|:--:|:--:|---|
| **frankensqlite** | ✅ | ✅ | ✅ | **No — explicitly `cv_gate=never`** |
| **frankenmermaid** | ✅ | ✅ | ✅ | No — `cv` and MAD are report-only |
| **frankenredis** | ✅ | ⚠️ | ✅ | ⚠️ `fr-bench` batch ratchet: `cv_pct > 5 is not keep-eligible` |
| **frankenlibc** | ✅ | ✅ | ✅ | ❌ **`fn cv_gate_pass() { …all(\|cv\| cv < 5.0) }`** in `malloc_bench.rs` |
| **frankenpandas** | ✅ | ✅ | ✅ | ❌ **`assert!(orig_cv < 5.0)` / `assert!(candidate_cv < 5.0)`** in `fp-frame` |
| **frankenfs** | ✅ | ✅ | ✅ | ⚠️ triage-only (`cv_percent > 15/20`), not a keep gate |
| **frankensearch** | ✅ | ✅ | ✅ | ⚠️ not in code, but `PERF_LEDGER.md` records arms discarded for "violating the CV gate" |
| **franken_whisper** | ✅ | ✅ | ✅ | No — gates the **null arm's** cv, which is the contract's own check |
| **frankenscipy** | ✅ | ✅ | ✅ | No live gate in code — but see below, this is the repo the campaign singled out |
| **franken_networkx** | ⚠️ 1 file | ✅ | ⚠️ 1 file | No live gate in code — 44 ledger verdicts still cite it |
| **franken_numpy** | ❌ none in bench code | ⚠️ 2 files | ⚠️ p10/p90 envelope only | No live gate in code — 18 ledger verdicts cite it |

## The finding that matters

**Mechanism adoption is nearly complete; gate adoption is not.** Ten of eleven repos now have all
three mechanisms in at least one bench. But **every single one of the eleven has ledger verdicts that
reject on `cv`** — **126 such lines in total**, from 5 (frankenredis) to 22 (franken_networkx) and 20
(frankenmermaid) per repo. The contract has been installed and, in several repos, is not yet what
decides.

Note those per-repo counts are cumulative history, not a live-defect count: frankenmermaid's 20 and
frankenlibc's 14 are mostly *old* rows plus the write-ups explaining why the gate was abandoned. The
number that matters is whether a REJECT written **after** the repo adopted part 3 still cites `cv`.

Three concrete, named blockages, in descending value:

1. **franken_numpy** — 0 self-hashing bench binaries, 2 files with a paired sampler, a p10/p90
   median envelope but no bootstrap median CI, and 10 verdicts citing cv, including *"direction
   never clears both effect and null CV gates"*. The
   addendum puts it in Lane M precisely because two levers are already directional
   (`loadtxt` usecols 1.82–1.98×, bool-token parse 3.09–3.69×). **Those are gate-blocked, not
   idea-blocked, and the gate is the least-adopted in the fleet.** Highest-value single fix here.
2. **frankenlibc and frankenpandas have a live `cv < 5` gate in code**, in `malloc_bench.rs` and
   `fp-frame/src/lib.rs` respectively. These are hard rejections that fire regardless of what the CI
   says. frankenpandas's is the mechanism behind its high-CV filter dropping **16 of 26** gauntlet
   rows. Both are a few lines to change.
3. **frankenredis's `fr-bench` batch ratchet** rejects on `cv_pct > 5`, which is notable because
   two other frankenredis benches carry comments stating cv is unreachable and *deliberately never
   gated*. The repo has both positions in-tree simultaneously.

**frankensqlite is the reference implementation.** Its benches print
`median_ci_gate={verdict} rule=null_ci95_2x_margin cv_gate=never null_radius=…` — the verdict, the
rule, the explicit refusal to gate on cv, and the null radius, on one line. Any repo fixing part 3
should copy that line's shape rather than invent one.

## What this repo did

The first rollout audit was too generous to frankenmermaid itself. It found A/A and bootstrap-CI
machinery elsewhere in the repo, then marked the repo complete without tracing the **live
head-to-head verdict**. That path self-hashed its ELF, but timed default and lean sequentially, ran no
same-invocation A/A control for either runtime, and exited on `fm.mad_pct > 5`. In other words, the
status table said P2/P3 while the published comparator still had neither. `bd-w1po` corrected the
code and this claim.

Part 1 remains in `crates/fm-cli/examples/headtohead.rs`: the runner hashes its own
`env::current_exe()` and emits it as its first record; the driver fails closed unless that record
contains a lowercase 64-hex SHA-256 and a positive ELF byte count. A hash computed by a shell step
*beside* the run proves nothing about which ELF executed — rch builds into an opaque per-worker
target dir.

Parts 2 and 3 now live in the actual decision path:

- Rust calls one paired routine as `(default, default)` and then `(default, lean)`, with both arms
  back-to-back in each round, alternating order, the same batch, output checksums, and the median of
  per-round ratios.
- The browser runner emits its own `(mermaid, mermaid)` paired null from the same Chromium
  invocation. Since a Rust process and a JavaScript runtime cannot be arms in one binary, the
  cross-runtime driver conservatively uses the larger of the two per-engine A/A CI radii.
- The only blocking gate is
  `claim magnitude >= max(1.01, 1 + 2 * max(null CI radius))`. Records say
  `cv_gate=never`; CV and MAD are provenance only. A DNF has no point ratio and is explicitly
  `not_applicable`, never smuggled through a dispersion gate.

This correction changed no parser, layout, or renderer behavior and made no performance claim. The
deterministic bootstrap/gate self-tests and Rust quality gates are the acceptance evidence; actual
corpus timing still requires the campaign-owned quiet window tracked by `bd-ktx5`.

### Prospective ledger-gate correction (`bd-ckm0`, 2026-07-27)

The first institutionalized preflight enforced explicit A/A/counted-mechanism markers in
`docs/NEGATIVE_EVIDENCE.md`, but it did not parse the split KEEP ledger
`docs/PERF_LEDGER.md`. Its KEEP provenance rule was therefore documented but unreachable for the
normal place where new KEEPs land. The corrected guard parses both heading schemes, compares added
or modified entries in both files, and has boundary tests that fail a `## KEEP` without the exact
process-self-reported executing-ELF marker. Repositories copying the rollout must verify the
verdict-bearing files their own tool actually parses; a correct evidence predicate behind an
unreachable path is not adoption.

One addition from `bd-1buv.69`, offered to the fleet: **for a work-removal lever, gate on
instructions, not wall.** Measured at load ~12, same corpus, arms alternated: the A/A null was
**±0.011% on instructions vs ±0.145% on wall**, and two of five wall rounds were contaminated past
this repo's own MAD gate. Instruction count is deterministic and load-immune. The scope limit is
real and is exactly campaign §2.6: this holds when the candidate **removes work**, and does *not*
hold for ISA/LTO/allocator changes, where fewer instructions is the mechanism rather than a neutral
proxy and wall/cycles must decide. Many cv-killed rows across the fleet are work-removal levers
judged on wall time inside a ~5% code-layout noise floor.

## Fleet rollout handoff

On 2026-07-26, `CreamGorge` sent the audited current state and exact next action to the active
performance owner in each of the other ten repositories under Agent Mail thread
`harness-contract-20260725`:

- **Fix required:** franken_numpy (add P1 and true P3), frankenredis (remove the `fr-bench` CV
  ratchet and broaden P2), frankenlibc (replace `malloc_bench.rs::cv_gate_pass`), frankenpandas
  (remove the two hard CV assertions), and franken_networkx (propagate its one complete harness).
- **Contract confirmed; preserve it:** frankensqlite, frankensearch, frankenscipy, frankenfs, and
  franken_whisper. These handoffs explicitly distinguish historical ledger debt from a live code
  gate so owners do not churn already-correct implementations.

The rollout owner does not call a repo complete merely because a helper exists. Completion means
the next verdict produced by that repo is self-identifying, carries a same-invocation A/A row, and
is decided by the median CI rather than CV. The 2026-07-27 correction adds one more mechanical
check: every split KEEP/REJECT ledger must be in the preflight's parsed path set.

## Lane L corpus admission (`bd-jil4`)

The earlier corpus extension had already measured a 5,000-node architecture diagram, a
2,500-entity ER schema, a 201-document edit trace, and a 40-diagram docs build. Lane L added the
still-unmeasured endpoints the allocation addendum actually requests:

| item | workload boundary | generated bytes / revisions | pinned SHA-256 |
|---|---|---:|---|
| `er_schema_5000x8` | 5,000 ER nodes, eight attributes each | 901,655 / 1 | `f43a2b41e66146eb4eba7634eba367ba5667fe15c084a0e919d8e18480e46c56` |
| `er_schema_10000x8` | 10,000 ER nodes, eight attributes each | 1,806,655 / 1 | `185a597d70c58a4ab3abacff4454dd87a4aa662c602f97fb2a027e3c16d7b341` |
| `edit_trace_500x1000` | 1,001 successive live-preview documents | 23,016,535 / 1,001 | `4813beb88f379af9942bceaa10af37b6b7dc01c2210663697dba9e0312daa2cb` |
| `ci_batch_500` | 500 diagrams across five syntax families | 201,534 / 500 | `65b8f69a7b2ee114cfb2fb49557b34cbc7e2c15f1414a81b7d94215a46de432f` |

These are **construction results, not benchmark results**. No engine was timed and no worker was
requested. Importing the generators produced all 25 items, all 25 hashes matched `pins.json`, and
all 21 pre-existing hashes remained byte-for-byte unchanged. A future measurement is admissible
only in a campaign-assigned quiet window using the existing self-ELF, DNF, determinism, and
dispersion protocol; that future window is tracked by `bd-ktx5`.

## Audit snapshot

The code audit was performed against these repository tips; later movement requires a re-audit:

| Repo | audited commit | Repo | audited commit |
|---|---|---|---|
| franken_numpy | `38f8acf34c30` | frankenredis | `4a1dc258d627` |
| franken_networkx | `45cb1925e7a2` | frankensqlite | `75436b7ddc80` |
| frankensearch | `3070c936e863` | frankenscipy | `9714ac99102a` |
| frankenlibc | `3aedb3f691e4` | frankenpandas | `cab977a66c33` |
| frankenfs | `8ebf45015ef3` | frankenmermaid | `83533872c0c5` |
| franken_whisper | `b17945cb4664` |  |  |

## Re-audit predicate

Re-run when a repo claims to have moved its gate. The check is not "does a bootstrap helper exist" —
several repos compute a CI and still reject on cv. The check is: **does a REJECT verdict written
after the claim cite `cv` as its reason?**
