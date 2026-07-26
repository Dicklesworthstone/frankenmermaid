# Bench Harness Contract — fleet adoption status

**Owner:** frankenmermaid (Lane L, allocation addendum 2026-07-25 23:15Z).
**Contract:** [`CROSS_REPO_RECOMMENDATION_bench_harness_contract.md`](CROSS_REPO_RECOMMENDATION_bench_harness_contract.md),
campaign `perf-campaign-20260725` §2.
**Audited:** 2026-07-26, all 11 campaign repos, at their then-current working trees.

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
| **frankenmermaid** | ✅ | ✅ | ✅ | No (MAD gate; cv reported only) |
| **frankenredis** | ✅ | ⚠️ | ✅ | ⚠️ `fr-bench` batch ratchet: `cv_pct > 5 is not keep-eligible` |
| **frankenlibc** | ✅ | ✅ | ✅ | ❌ **`fn cv_gate_pass() { …all(\|cv\| cv < 5.0) }`** in `malloc_bench.rs` |
| **frankenpandas** | ✅ | ✅ | ✅ | ❌ **`assert!(orig_cv < 5.0)` / `assert!(candidate_cv < 5.0)`** in `fp-frame` |
| **frankenfs** | ✅ | ✅ | ✅ | ⚠️ triage-only (`cv_percent > 15/20`), not a keep gate |
| **frankensearch** | ✅ | ✅ | ✅ | ⚠️ not in code, but `PERF_LEDGER.md` records arms discarded for "violating the CV gate" |
| **franken_whisper** | ✅ | ✅ | ✅ | No — gates the **null arm's** cv, which is the contract's own check |
| **frankenscipy** | ✅ | ✅ | ✅ | No live gate in code — but see below, this is the repo the campaign singled out |
| **franken_networkx** | ⚠️ 1 file | ✅ | ⚠️ 1 file | No live gate in code — 44 ledger verdicts still cite it |
| **franken_numpy** | ❌ none in bench code | ⚠️ 2 files | ⚠️ 1 file | No live gate in code — 18 ledger verdicts cite it |

## The finding that matters

**Mechanism adoption is nearly complete; gate adoption is not.** Nine of eleven repos now have all
three mechanisms in at least one bench. But **every single one of the eleven has ledger verdicts that
reject on `cv`** — **126 such lines in total**, from 5 (frankenredis) to 22 (franken_networkx) and 20
(frankenmermaid) per repo. The contract has been installed and, in several repos, is not yet what
decides.

Note those per-repo counts are cumulative history, not a live-defect count: frankenmermaid's 20 and
frankenlibc's 14 are mostly *old* rows plus the write-ups explaining why the gate was abandoned. The
number that matters is whether a REJECT written **after** the repo adopted part 3 still cites `cv`.

Three concrete, named blockages, in descending value:

1. **franken_numpy** — 0 self-hashing bench binaries, 2 files with a paired sampler, 1 with a CI, and
   10 verdicts citing cv, including *"direction never clears both effect and null CV gates"*. The
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

frankenmermaid wrote the contract and had parts 2 and 3 already (A/A null; MAD rather than cv, on
the argument that timing noise is one-sided so MAD measures the uncontaminated regime while sd does
not). Part 1 landed 2026-07-25 in `crates/fm-cli/examples/headtohead.rs`: the runner hashes its own
`env::current_exe()` and emits it as its first record, verified against `sha256sum` of the on-disk
binary. A hash computed by a shell step *beside* the run proves nothing about which ELF executed —
rch builds into an opaque per-worker target dir.

One addition from `bd-1buv.69`, offered to the fleet: **for a work-removal lever, gate on
instructions, not wall.** Measured at load ~12, same corpus, arms alternated: the A/A null was
**±0.011% on instructions vs ±0.145% on wall**, and two of five wall rounds were contaminated past
this repo's own MAD gate. Instruction count is deterministic and load-immune. The scope limit is
real and is exactly campaign §2.6: this holds when the candidate **removes work**, and does *not*
hold for ISA/LTO/allocator changes, where fewer instructions is the mechanism rather than a neutral
proxy and wall/cycles must decide. Many cv-killed rows across the fleet are work-removal levers
judged on wall time inside a ~5% code-layout noise floor.

## Re-audit predicate

Re-run when a repo claims to have moved its gate. The check is not "does a bootstrap helper exist" —
several repos compute a CI and still reject on cv. The check is: **does a REJECT verdict written
after the claim cite `cv` as its reason?**
