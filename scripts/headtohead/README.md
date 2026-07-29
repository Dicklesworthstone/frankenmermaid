# Pinned mermaid-js head-to-head harness (`bd-1buv.1`)

A repeatable comparator that measures frankenmermaid against the **original** mermaid-js on a fixed
corpus, with pinned provenance, warmup discipline, an environment fingerprint, and a measured
same-invocation noise floor.
Only a measured mermaid-js/frankenmermaid ratio produced by one driver invocation can be classified
as an incumbent win. Internal frankenmermaid before/after ratios are maintenance self-speedups.

## Run it

```bash
# 1. normal compile/test validation is strict-remote and never writes a local target
df -h /data  # abort and report if available space is below 120G
RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- \
  cargo test --profile release -p frankenmermaid-cli --example headtohead

# 2. strict-remote release builds retrieve the executable into this repo's existing target/.
#    Never mint a task-specific target directory and never permit silent local fallback.
df -h /data  # the same 120G floor applies
RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- \
  cargo build --profile release -p frankenmermaid-cli --example headtohead

# 3. run both engines over byte-identical inputs
node scripts/headtohead/run.mjs \
  --fm-bin target/release/examples/headtohead

# The 201-revision certification needs enough wall budget for nine full A/A pairs.
node scripts/headtohead/run.mjs \
  --fm-bin target/release/examples/headtohead \
  --only edit_trace_200x200 \
  --js-budget-scale 20

# CI-scale caller concurrency must run under the exclusive trj booking, not on an rch worker.
: "${TRJ_CLAIM_MESSAGE_ID:?set this to the Agent Mail CLAIM message id}"
node scripts/headtohead/run.mjs \
  --fm-bin target/release/examples/headtohead \
  --only ci_batch_500 \
  --thread-sweep 1,2,4,8,16,32,64,96,128 \
  --exclusive-host-claim "trj-booking:${TRJ_CLAIM_MESSAGE_ID}" \
  --pin-cpu off
```

Useful flags: `--only <corpus_id>[,<corpus_id>…]`, `--reps-scale 0.25` (fast smoke),
`--js-budget-scale 0.1` (shrink the mermaid wall budgets for a smoke run), `--skip-mermaid`,
`--thread-sweep 1,2,4,8,16,32,64,96,128`,
`--exclusive-host-claim trj-booking:<claim-message-id>`, `--pin-cpu auto|N|off`,
`--out <dir>`, `--update-pins`.

Exit codes: `0` green · `1` an engine errored · `2` invalid arguments or missing mandatory
environment provenance · `3` corpus drift · `4` median-CI gate failed ·
`5` the bracketed Rust observations drifted outside their A/A median-CI floor · `6` host-wide
benchmark exclusivity was not clear immediately before a measured sweep phase.
A comparator **DNF** is not an error and does not fail the run — see "Did not finish" below.

A gate failure (`4` or `5`) is **inconclusive**, not a regression. Re-run in an assigned quiet
window; never re-pin or retune an item to force a pass.

## What is pinned

`pins.json` records everything that can move a number:

| Pin | Why |
|---|---|
| mermaid `11.15.0` + bundle URL + SHA-256 | the comparator binary itself |
| `securityLevel: "strict"` | mermaid's own default |
| corpus SHA-256, one per item | a generator edit cannot silently move the baseline |

The bundle is fetched once to `~/.cache/fm-headtohead` and hash-checked on every run; a mismatch is a
hard failure, never a silent re-baseline. Re-pin deliberately with `--update-pins` (corpus) or
`node mermaid_bench.mjs --pin` (bundle).

**No npm install, no puppeteer.** `mmdc` (`@mermaid-js/mermaid-cli`) cannot render at all in 11.15.0 —
its bundled `dist/index.html` is an 81-byte stub. Instead we drive a system Chromium over the DevTools
Protocol using Node's built-in `WebSocket`/`fetch`, loading the exact CDN bundle a browser user would
load. This is both a stronger provenance pin than a `node_modules` tree and compliant with AGENTS.md's
prohibition on ad-hoc package installs.

## Fairness

Both engines consume **byte-identical input** (the driver cross-checks the SHA-256 each engine
reports, and fails the run on a mismatch). `mermaid.render()` does parse + layout + serialize to an
SVG string; the frankenmermaid side times exactly the same three phases into an SVG string. Neither
side writes to disk or touches the DOM afterwards.

Choices that deliberately understate our margin:

- `securityLevel: "strict"` is mermaid's default, with DOMPurify sanitization enabled.
- Normal scalar runs pin the frankenmermaid runner to **one** core; Chromium keeps the whole
  machine. A declared thread sweep disables affinity and records every caller-pool width.
- `maxEdges` / `maxTextSize` are raised above mermaid's defaults so the large items render at all.
  These are guardrails, not performance knobs.

A mermaid render that throws, or that returns mermaid's "Syntax error" placeholder SVG, is reported
as `status: "error"` and fails the whole run. A comparator that cannot render is never a silent win.

## Measurement methodology

**Warmup.** Every item runs untimed warmup iterations first (JIT warm on mermaid's side, allocator
and branch predictors on ours).

**Batching.** A 69 µs pipeline cannot be timed one iteration at a time on a shared box — a single
timer interrupt is a large fraction of the sample. The Rust runner therefore rounds the batch count
up until each normal timed sample spans ≥ 2 ms and divides. A thread sweep raises that floor to
50 ms because the high-width whole job can fall below 1 ms and its pre/post observations are
separated by the long Chromium phase. Calibration targets 75 ms to leave headroom for steady-state
speedup, while the joined result fails closed if either bracket's measured `batch × per-job p50`
falls below 50 ms. Batching is a timing device only: every iteration still renders the whole
diagram. mermaid's items are all ≥ 30 ms, so they need no batching.

**Same-invocation A/A.** The Rust runner factors timing into one paired routine. For every item it
first calls that routine with `(default, default)`, then with `(default, lean)`. Both arms are timed
back-to-back inside each round, order alternates, both calls use the same batch, and the statistic is
the median of per-round ratios. Each row prints the A/A and A/B ratio, bootstrap 95 % CI, CV/MAD, and
both output checksums. The default/lean A/B is a maintenance self-speedup, not campaign output.

The cross-runtime headline is a whole-runtime comparison: Rust and JavaScript cannot be two arms in
one binary. The mermaid runner therefore emits its own in-browser `(mermaid, mermaid)` paired null in
the same Chromium invocation. The driver uses the **larger** of the Rust and mermaid A/A CI radii, so
the claim must clear both runtimes' measured floors.

**Same-invocation phase bracket.** The driver runs the identical self-reporting Rust ELF immediately
before and after the Chromium phase. The two Rust outputs must be byte-identical, and their medians
must agree inside the larger Rust A/A median-CI floor. Every ratio uses the **slower** Rust median.
This makes phase-order drift conservative and fail-closed. Aggregate CPU-busy deltas remain
provenance only because they include each engine's own work and span phases of very different
lengths.

**Median-CI gate, never CV or MAD.** For each engine,
`radius = max(abs(ci95_lo - 1), abs(ci95_hi - 1))`. A ratio passes only when its magnitude is at least
`max(1.01, 1 + 2 * max(radius_rs_before, radius_rs_after, radius_js))`. Each null has at least nine
paired rounds. `cv_pct` and `mad_pct` remain in every record as provenance, with
`cv_gate: "never"`; neither can block a verdict.

On a budgeted XL item that cannot afford nine comparator null rounds, the harness still attempts one
real sample. A timeout can honestly establish DNF. If it completes, the row is inconclusive and the
median-CI gate fails because its null control is insufficient.

**Two report-only estimators.** The harness reports both p50-based and min-based speedup. Their
agreement is useful diagnostic context, but only the same-invocation null median-CI decides the row.

**CPU power-policy and ISA provenance.** Before either engine runs, the driver records the machine
ISA and its complete feature list. On Linux it enumerates every cpufreq policy, including affected
CPUs, scaling driver, governor, energy-performance preference, frequency limits, and boost state.
The policies must cover every online CPU and agree on driver, governor, and exposed EPP; missing or
mixed policy state makes cross-engine evidence invalid with exit 2. On macOS the corresponding
artifact records the platform-managed power settings and all exposed `hw.optional` ISA features.
Governor choice is provenance, not the acceptance statistic: a stable `powersave` run is labelled
as such rather than silently presented as `performance`. A thread sweep re-reads the whole policy
before every measured phase and exits 6 if it differs from the baseline.

**Libc-leaf attribution.** A hot `memmove`, `memcpy`, `memcmp`, allocator, or syscall leaf is not
itself a lever. Profile with call stacks, retain the exact symbol-bearing ELF that self-reported
during that profile, recover file-relative instruction addresses from `perf`, and resolve each one
with:

```bash
addr2line -a -f -C -i -e "$PROFILE_ELF" "$FILE_RELATIVE_IP"
```

`-i` is required so inlined Rust callers are preserved. For PIE samples reported as runtime virtual
addresses, normalize through the matching `PERF_RECORD_MMAP` mapping before calling `addr2line`.
Group the libc-family samples by the deepest project call site, report each call site's self-time,
and compute its Amdahl ceiling before proposing a source change. “libc leaf is hot” without that
caller attribution is incomplete profile evidence.

**Determinism.** Every timed iteration's output length is checked against a reference render, and the
full bytes are compared once outside the timed region. A nondeterministic render fails the run.

**Portable thread sweep.** `--thread-sweep` is accepted for one selected workload and must include
the scalar `1` arm. The driver starts one Rust invocation per requested width before the incumbent
phase, then mirrors that order after it so every width has symmetric placement around Chromium.
Every invocation builds one persistent Rayon pool and reuses it for warmup, A/A, A/B, and measured
rounds; the scalar arm does not enter Rayon. The driver fails
closed unless every pooled arm's input, default SVG, and lean SVG SHA-256 exactly match the scalar
arm in both brackets. It also requires every arm to self-report the same ELF, requires the incumbent
record to identify mermaid-js's single-page main-thread execution model, and gates each ratio on its
own bracket plus both engines' A/A median CIs. Every sweep arm also self-reports the 50 ms minimum
sample floor and 75 ms calibration target, and the driver verifies that `batch × per-job p50`
reaches the floor in both brackets. Each row embeds host identity, physical and logical topology,
RAM, NUMA count, inherited affinity, full ISA flags, cpufreq driver/governor/EPP/boost provenance,
requested caller threads, caller workers actually observed during an untimed batch of the exact
workload, the executing Rust ELF SHA-256, and the loaded mermaid-js bundle SHA-256. Requested
capacity is never substituted for observed participation.
Every sweep additionally requires the complete host cpuset, an Agent Mail claim reference, and a
one-second idle sample of every logical CPU immediately before every measured phase. Any affinity
CPU above 20% busy or any power-policy change blocks the invocation with exit 6. The artifact
retains every per-CPU sample, the baseline and pre-phase power-policy summaries, and the claim
reference. CV and MAD remain provenance only.

### Exclusive `trj` booking

Thread-scaling work on `trj` is exclusive. Before using that host, read Agent Mail thread
`trj-booking`; claim it with subject `[trj] CLAIM frankenmermaid`, including expected duration and
measurement scope, only when the six higher-priority repos have released it. Post
`[trj] RELEASE frankenmermaid` immediately after success or failure. While any sweep claim is
active, run neither sweep nor non-sweep frankenmermaid work on `trj`. A silent holder past its
declared duration receives `[trj] PROBE <repo>` and one full wait cycle before any takeover.
The harness requires the resulting message id as
`--exclusive-host-claim trj-booking:<claim-message-id>` and independently refuses a sweep unless
the complete host cpuset is quiet. The Agent Mail booking establishes ownership; the CPU samples
verify that ownership produced an uncontended measurement host.

The pool is deliberately outside renderer internals. The ledger shows that starting fresh scoped
threads inside each small render regresses, while a CI job supplies hundreds of independent
diagrams over which one pool can amortize startup. Rayon keeps the caller-concurrency mechanism
portable across x86-64 and aarch64; the harness contains no x86-specific intrinsics.

**Certified Threadripper sweep.** On the 32-core / 64-thread AMD Ryzen Threadripper PRO 5975WX,
artifact `.benchmarks/headtohead/ci-thread-sweep-v3/summary-ffeab05f-1785311982943.json` passed all
seven median-CI and same-ELF bracket gates. The 500-diagram job measured 21.441246 ms at one thread
and 1.132857 ms at 64 threads (18.926701× caller scaling), versus 18,490.000 ms for mermaid-js's
single-page main-thread API in the same invocation: 862.356600× and 16,321.565740× respectively.
The process self-reported ELF SHA-256
`600cd6b79113f01de7526df5a029b7ce5d57d4f06fb1d3772412fb29097bdcf7`; scalar/pooled SVG identity
passed in both brackets, and every integrated Rust sample exceeded the 50 ms floor.

## Corpus

33 items in three tiers.

**The pinned baseline (13).** Flowcharts (10/100/500 nodes), wide layered DAGs (8×16, 12×24, 16×32 —
up to 512 nodes / 960 edges), a dense DAG (200 nodes / 790 edges), an SCC-heavy cyclic graph, one
each of sequence, class, state and ER, and an **edit trace**. `flowchart` and `wide` reproduce
`crates/fm-cli/benches/pipeline_bench.rs`'s generators byte for byte, so harness numbers stay
comparable with the criterion history. Their input hashes have never moved.

**Extended workload classes (12).** The baseline tier tops out at 500-node flowcharts. The extended
tier covers three additional classes:

| Class | Items | What it is |
|---|---|---|
| **XL** | `flowchart_xl_2000`, `flowchart_xl_5000`, `arch_100x50`, `arch_200x50`, `er_schema_1000x6`, `er_schema_2500x8`, `er_schema_5000x8`, `er_schema_10000x8` | Thousands of nodes, through the campaign's explicit **5,000–10,000-node** architecture and ER range. Architecture maps use `subgraph`; schemas carry attribute blocks. |
| **EDIT** | `edit_trace_200x200`, `edit_trace_500x1000` | Live-preview sessions of 201 and 1,001 successive full documents, rather than a 21-edit sketch. |
| **DOC_BUILD / CI** | `doc_build_40`, `ci_batch_500` | A docs page and a repository-scale CI job: 40 and 500 diagrams across five syntax families, each timed as one batch. |

The 5,000- and 10,000-entity ER endpoints are certified `RangeError`/`CANNOT` rows. The
201-revision trace and 500-diagram CI batch have certified ratios. The 1,001-revision trace is a
measured `DNF-timeout`: frankenmermaid completes the job, while mermaid-js remains working at the
600-second deadline. Every input remains deterministic and SHA-256-pinned in `pins.json`.

`architecture` uses `subgraph`, which is a different layout problem from the flat generators: the
cluster boundaries constrain placement and force the router around obstacles the flat shapes never
produce. `er_schema` carries attribute blocks, which makes it text-measurement-bound rather than
graph-bound.

**Realistic end-to-end tier (8 rows, 5 jobs).** These jobs use seeded, right-skewed distributions
instead of uniform `Node 123` fixtures:

| User job | Items / sizes | Realism carried by the input |
|---|---|---|
| Documentation-site render | `docs_site_50`, `docs_site_200` | 50 and 200 diagrams; flowchart-dominated type mix, right-skewed sizes, non-ASCII and escaping-heavy labels. |
| CI render farm | `ci_docs_2000`, `ci_docs_5000` | 2,000 and 5,000 diagrams in one whole job, using the same right-skewed five-syntax distribution; traversal, allocation, output ownership, and persistent-pool scheduling stay inside the measured boundary. |
| Live typing preview | `typing_trace_60` | 60 successive keystrokes inside one label of a 40-node flowchart. |
| Monorepo architecture review | `monorepo_arch_120`, `monorepo_arch_300` | 120 and 300 services across uneven domains; hub-skewed dependencies and cross-domain event links. |
| Database-catalog publish | `schema_catalog_25` | 25 bounded-context ER diagrams, 8–75 entities each, with skewed relationships and varied field counts/types. |

One sample is the complete named job: source strings in, parse + layout + render, serialized SVG
strings out. Corpus generation and the caller's final file copy are outside both engines' timers;
the library work and output serialization that differ between the implementations are inside.

Certified artifacts are under `.benchmarks/headtohead/realistic-*`. The 50- and 200-diagram
documentation jobs measure 434.107779× and 534.368778×; the 60-keystroke session measures
1,193.149598×; and the 25-schema catalog measures 412.825519×. Both monorepo maps pass
`mermaid.parse()` but fail in `mermaid.render()` with `TypeError: Cannot set properties of
undefined (setting 'order')`; they are `CANNOT`, carry no ratio, and stay outside aggregates.

### Did not finish

At XL sizes the honest question is not "how much faster" but "does the comparator finish at all".
Items in the new tier carry `js_budget_ms` (a wall budget for the mermaid arm) and `dnf_allowed`.
The mermaid runner first does one untimed `mermaid.parse()` plus **probe** render of the item's
largest UTF-8 input under that budget. Parse acceptance is recorded separately, so a renderer
failure cannot be confused with invalid syntax, and an item that cannot be rendered is discovered
in one render rather than `warmup + reps` of them. Two outcomes are recorded, and they support
different claims:

- **`kind: "timeout"`** — mermaid was still working when the budget expired. That bounds the speedup
  from below (`budget / fm_p50`), reported as `>Nx`, explicitly a bound and not a measurement.
- **`kind: "failed"`** — mermaid raised: a stack overflow, its own size guardrail, an OOM. There is
  no bound to state. At that size mermaid does not render the diagram at any budget, and the table
  prints `CANNOT` rather than a ratio.

DNF rows are kept **out of the `speedup` aggregate** — a bound and a point estimate do not belong in
the same median — and reported in their own section. The 13 pinned items set neither field, so a
comparator failure there is still a hard run failure exactly as before.

Timing an item out wedges its page permanently (mermaid's layout is synchronous JavaScript and
cannot be interrupted from outside), so a DNF is followed by a fresh browser before the next item.
That is why the budgets are generous: a DNF must mean mermaid did not finish, never that the harness
was impatient.

### Which binary produced the numbers

The frankenmermaid runner hashes its own `env::current_exe()` and emits that SHA-256 as its first
stdout record; `run.mjs` copies it into the summary's environment fingerprint and fails closed unless
it is a lowercase 64-hex digest with a positive ELF byte count. A hash computed by a shell step
*next to* the run proves nothing about which ELF actually executed — `rch` compiles into an opaque
per-worker pool target dir, and agents have edited crates mid-benchmark in this fleet.

### Edit traces

`edit_trace_60x20` is an editing session: 21 successive full documents, the edits cycling through
appending a node, renaming a label, and adding an edge. **One timed sample renders all 21 revisions**,
because that is what a live preview does — mermaid has no incremental path, so an editor calls
`mermaid.render()` on every keystroke. The report prints the per-re-render cost, which is the number a
user actually feels.

`typing_trace_60` follows the same whole-session rule for 60 successive label keystrokes. It starts
at the first typed character, so every revision is valid input for both renderers.

Internally every corpus item is a trace; a single-shot item is just a one-revision one. That keeps one
code path in both engines; joining a one-element revision list yields the original item bytes.

Note this measures *full re-render* on both sides, which is the fair comparison. frankenmermaid's
incremental-layout path is a separate lever (`bd-1buv.3`) and is not exercised here.

## Output

`.benchmarks/headtohead/run-<rev>-<ts>.jsonl` — one event per engine per item.
`.benchmarks/headtohead/summary-<rev>-<ts>.json` — env fingerprint, pins, joined rows, ratios, gate.

Both use schema `frankenmermaid.headtohead.v2`; every ratio row carries per-engine null controls, a
`median_ci_gate` verdict, and a same-ELF Rust-before/Rust-after bracket verdict.
