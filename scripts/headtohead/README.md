# Pinned mermaid-js head-to-head harness (`bd-1buv.1`)

A repeatable comparator that measures frankenmermaid against the **original** mermaid-js on a fixed
corpus, with pinned provenance, warmup discipline, an environment fingerprint, and a measured
same-invocation noise floor.
Only a measured mermaid-js/frankenmermaid ratio produced by one driver invocation can be classified
as an incumbent win. Internal frankenmermaid before/after ratios are maintenance self-speedups.

## Run it

```bash
# 1. normal compile/test validation is strict-remote and never writes a local target.
#    --base/--clean-overlay pin the transferred tree to a commit plus ONLY the paths you name, so
#    the rch project hash stops moving every time the other agent in this shared checkout saves a
#    file. Without it the hash misses the remote target cache and every build is cold.
df -h /data  # below 150G means strict-remote-only; never fall back locally
RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec \
  --base "$(git rev-parse HEAD)" --clean-overlay --no-overlay -- \
  cargo test --profile release -p frankenmermaid-cli --example headtohead

# 2. Preserve the deterministic-overlay project identity for every build. Never mint a
#    task-specific target directory or permit silent local fallback. Route 1 retrieves only the
#    executable from the worker's .rch-target-<worker>-pool-* directory; a bounded local build is
#    also permitted by POLICY_local_perf_binaries.md only after its 150G free-space precheck.
df -h /data
RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec \
  --base "$(git rev-parse HEAD)" --clean-overlay \
  --overlay-path crates/fm-cli/examples/headtohead.rs -- \
  cargo build --profile release -p frankenmermaid-cli --example headtohead

# 3. run both engines over byte-identical inputs
node scripts/headtohead/run.mjs \
  --fm-bin target/release/examples/headtohead

# The 201-revision certification needs enough wall budget for nine full A/A pairs.
node scripts/headtohead/run.mjs \
  --fm-bin target/release/examples/headtohead \
  --only edit_trace_200x200 \
  --js-budget-scale 20

# 4. cross-engine output equivalence. Required before any ratio for that item can be certified:
#    run.mjs exits 7 unless a matching passing verdict exists for every measured row.
node scripts/headtohead/equivalence.mjs \
  --fm-bin target/release/examples/headtohead \
  --only ci_equiv_512 \
  --out .benchmarks/headtohead/ci-equiv-512-equivalence \
  --keep-dumps

# CI-scale caller concurrency must run under the exclusive trj booking, not on an rch worker.
# `ci_equiv_512` predeclares 20 balanced incumbent A/A pairs and nine whole-job effect samples;
# this stabilizes the arm-order median without changing the corrected verdict rule.
: "${TRJ_CLAIM_MESSAGE_ID:?set this to the Agent Mail CLAIM message id}"
: "${FM_H2H_BUILDER:?set this to the rch worker that built the executable}"
node scripts/headtohead/run.mjs \
  --fm-bin target/release/examples/headtohead \
  --fm-builder "${FM_H2H_BUILDER}" \
  --only ci_equiv_512 \
  --thread-sweep 1,8,32,64,128 \
  --allow-oversubscription \
  --exclusive-host-claim "trj-booking:${TRJ_CLAIM_MESSAGE_ID}" \
  --equivalence-dir .benchmarks/headtohead/ci-equiv-512-equivalence \
  --out .benchmarks/headtohead/ci-equiv-512-sweep \
  --pin-cpu off
```

Useful flags: `--only <corpus_id>[,<corpus_id>…]`, `--reps-scale 0.25` (fast smoke),
`--js-budget-scale 0.1` (shrink the mermaid wall budgets for a smoke run), `--skip-mermaid`,
`--thread-sweep 1,2,4,8,16,32,64,96,128`,
`--allow-oversubscription` (required when requested workers exceed visible logical CPUs),
`--fm-builder <rch-worker-id>`,
`--exclusive-host-claim trj-booking:<claim-message-id>`, `--pin-cpu auto|N|off`,
`--out <dir>`, `--update-pins`, `--allow-unverified-output` (permit a run whose rows have no passing
equivalence verdict; the admission is stamped on the summary and on every affected row),
`--equivalence-dir <dir>`.

Set `FM_CHROMIUM_BIN=/absolute/path/to/chrome` when the pinned Chromium path is not available on the
benchmark host. The override must be executable; every incumbent row records the selected path and
the browser-reported version.

### Building without paying a cold build every time

`cc` and `cod` share one checkout. rch folds working-tree state into its project hash, so every save
by either agent moves the hash, misses the remote target cache, and buys a full cold build. Pinning a
worker does not help — the cache *key* moved. `--base <sha> --clean-overlay` transfers that commit
plus only the paths you name, making the tree a deterministic function of (base, overlay paths,
contents) and immune to the other agent's churn.

Two traps, both specific to a benchmark repo:

- **`--clean-overlay` EXCLUDES your uncommitted edits unless you list them.** For a perf harness this
  is worse than a cold build: it silently produces a binary that does not contain your change, and
  you measure the baseline twice. The defence is already here — the runner self-reports the ELF
  SHA-256 of the process that executed (see "Which binary produced the numbers"). After a build that
  should have changed the binary, check that `env.fm_elf_sha256` actually moved. If it did not, your
  overlay list was incomplete.
- **Do not auto-derive the overlay list from `git diff --name-only`.** In a shared checkout that list
  contains the *other* agent's modifications too, which re-imports exactly the churn you are
  excluding. Enumerate the paths you changed, by name, and keep the list minimal.

**Local builds are frozen while `/data` free space is below 150G.** Check `df` before any Cargo
command. During the freeze, validation uses only
`RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- ...`; `force_local = true` stays banned.
When a benchmark executable is needed locally, Route 1 copies that single file from the assigned
worker's `.rch-target-<worker>-pool-*` directory instead of retrieving a target tree.

There is **no `release-perf` profile** in this workspace. The harness builds and measures
`--profile release` (workspace `opt-level="z"` with `opt-level=3` overrides on fm-core, fm-parser,
fm-layout and fm-render-svg), which is what every number here claims. Nothing is mislabeled.

Worker-built binaries are safe to time on this host, but note the fleet inventory that established
that missed us: `.cargo/config.toml` pins `-C target-cpu=x86-64-v2` on x86_64. That is a portable
~2009 baseline, not `native`, so a worker-built binary's ISA is a floor rather than a fingerprint of
the builder. A worker on a different architecture would produce a binary that fails to execute here
outright rather than mismeasuring quietly. Record the building worker's identity next to the ELF
SHA-256 regardless: a binary of unknown origin is not evidence.

Exit codes: `0` green · `1` an engine errored · `2` invalid arguments or missing mandatory
environment provenance · `3` corpus drift · `4` median-CI gate failed ·
`5` the bracketed Rust observations drifted outside their A/A median-CI floor · `6` host-wide
benchmark exclusivity was not clear immediately before a measured sweep phase · `7` a measured row
has no passing cross-engine output-equivalence verdict (see "Output equivalence" below).
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

## Output equivalence (`bd-evx6`)

A renderer that drops an edge or a class member is faster and wrong. Until this phase existed the
harness proved only that both engines *consumed byte-identical input* and that each was
*self-deterministic* — never that the two rendered the same diagram. Every ratio was therefore a
comparison of two possibly-different outputs.

```bash
node scripts/headtohead/equivalence.mjs \
  --fm-bin target/release/examples/headtohead \
  --only ci_batch_500
```

Exit `0` all equivalent · `7` the gate failed · `1` an engine errored or the dump/hash linkage broke.
`run.mjs` refuses to certify a measured row without a matching passing verdict and exits `7`;
`--allow-unverified-output` still permits the run but stamps
`output_equivalence_gate.verdict: "admitted_unverified"` on the summary and
`content_verified: false` on every affected row, so no number can be quoted without its admission.

**What is compared, and what is not.** Not byte equality: the engines emit deliberately different
SVG (mermaid carries labels in `<foreignObject>` HTML, we emit `<text>`; different class
vocabularies; different layout engines). Not a rasterized perceptual diff either — different fonts,
paddings and stroke widths would make a pixel diff report a large distance between two *correct*
renders, so it would measure styling, not content. What is compared is engine-neutral structural
content, extracted from both engines by **one shared extractor** (`svg_equivalence.mjs`); a
per-engine extractor pair can drift into agreeing by construction.

| Tier | Scope | Invariant |
|---|---|---|
| 1 | every syntax family | **Rendered-text token multiset, containment-gated.** Every text run — `<text>`, `<tspan>`, and the HTML inside a `<foreignObject>` — reduces to one carrier-agnostic leaf scan, then to tokens. Gate: every token mermaid renders must be present in ours. |
| 2 | flowchart, state | **Rendered-path edge topology.** Frankenmermaid path endpoints are resolved geometrically to node anchors. Mermaid-js uses the same reconstruction when unambiguous and otherwise requires every rendered path's `data-id` endpoints to resolve uniquely against the SVG's rendered node-id set. The endpoint multisets are compared cross-engine *and* against input-derived ground truth. |

Three deliberate asymmetries, each stated because each weakens or strengthens the claim:

- **The text gate is one-directional.** It fails on content *we* are missing, not on content we add:
  we render ER relationship cardinalities (`0..*`, `1`) that 11.15.0 omits, which is a feature
  difference, not a defect. The symmetric difference is still recorded as provenance.
- **Topology is tied to rendered paths, not trusted source metadata.** We emit only a positional
  `fm-edge-<i>`, so our endpoints are always reconstructed geometrically. Mermaid records
  `data-id="L_<src>_<dst>_<n>"` on each rendered path. Its geometry is cross-checked against those
  declarations whenever nearest-anchor resolution is unambiguous; otherwise a declaration is
  admitted only when every path resolves uniquely to two node ids rendered in that same SVG.
  Dropping a path therefore drops an endpoint declaration and fails the multiset. Checking both
  engines against the input makes this engine-vs-**spec**: two equally wrong renders cannot pass.
- **Undecidable is not a pass.** Displacing a node far from its edges does not produce a *wrong*
  topology, it produces an ambiguous one — every endpoint resolution becomes a coin flip. Collapsing
  that into "equivalent" would let a renderer evade the gate by degrading its own geometry, so a
  family that claims Tier 2 and cannot have it decided is reported `unverified`, and `unverified`
  fails the gate.

**The dumps are provably the measured bytes.** Writing 500 SVGs inside the timed region would measure
the harness's file I/O, so this phase is untimed and separate. To keep it from becoming "we checked
*some* render", the concatenation of the dumped revisions must hash to the same `output_sha256` the
engine reported for its timed rounds; combined with each engine's existing determinism gate, that
makes the inspected bytes the measured bytes. A mismatch is a hard failure, not a warning.

**The gate has been watched to fail.** `node scripts/headtohead/svg_equivalence.mjs --self-test`
runs 20 cases including four **mutation controls** — dropped label, dropped edge, rewired edge (same
count, different endpoints, which a count-only check would pass), displaced node — and two negative
controls (extra content, differing text segmentation). A gate that has never been observed to fail is
not evidence.

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
job and the result divides only by repeated whole jobs, never by its diagram count. Mermaid's items
are all ≥ 30 ms, so they need no batching.

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

**A/A null gate, never CV or MAD.** A row is decidable when all three clauses hold:

1. **the effect CI excludes 1.0.** Where each engine supplies at least nine raw whole-job samples,
   the driver independently bootstraps both medians 10,000 times and records the mermaid-js/Rust
   ratio CI. Items declaring `effect_ci_required` fail closed if that CI is absent or includes 1.0.
   Older budgeted rows without enough samples retain `not_computable`; they cannot satisfy an item
   that requires clause 1.
2. **the effect deviation exceeds 2× the larger null radius**, where
   `radius = max(abs(ci95_lo - 1), abs(ci95_hi - 1))` over the Rust-before, Rust-after and mermaid
   nulls, and the bar is `max(1.01, 1 + 2 * radius)`.
3. **every null MEDIAN is within 2% of 1.0** (`null_median_max_bias`), inclusive.

Each null has at least nine paired rounds. `cv_pct` and `mad_pct` remain provenance with
`cv_gate: "never"`; neither can block a verdict.

**Clause 3 exists because clause 2 alone cannot see arm-order bias.** A biased null inflates the
radius, which raises the bar — but against a 16,000× effect a raised bar is meaningless, so a null
reporting that two *identical* arms disagreed by 12% could not stop the row. That is a statement that
the measurement environment was unfit, and it now blocks on its own terms. Clause 3 is a
harness-health check, not an effect-size check: it says nothing about whether the effect is real.

**Null CIs are telemetry, never a veto.** A fleet-wide defect in the shared harness contract required
each null's CI to *include* 1.0 before a row could score, which couples the verdict to the null's
precision in the wrong direction — a tighter null is likelier to exclude 1.0 and veto its own row.
This harness never had that clause, and the audit is on the record: seven rows across the artifact
history carry a null whose CI excludes 1.0, and all seven scored. Every record keeps
`null_ci_straddles_1` so the absence of a straddle veto stays visible rather than assumed.

**`radius` is not the CI's half-width.** It is the distance from 1.0 to the *farther* endpoint. The
two differ whenever a null is off-centre and ours is always the larger — for a null of
`[1.011, 1.044]` ours is `0.044` against `0.0165`. The field is unfortunately named `half_width` in
the JSON for backwards compatibility; substituting the narrower reading would silently loosen
clause 2 for every off-centre null.

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
An arm above the host's logical CPU count is refused unless `--allow-oversubscription` is present.
Such a row reports `oversubscribed: true`, the host's logical CPUs, and the exact Rayon workers
observed executing diagrams. It is an OS-thread scheduling experiment, not a claim that the host
has that many hardware threads.
Every sweep additionally requires the complete host cpuset, an Agent Mail claim reference, and a
one-second idle sample of every logical CPU immediately before every measured phase. A phase waits
for at most 900 consecutive admission samples (15 minutes) and starts only when every affinity CPU
is at or below
20% busy with an unchanged power policy; no clear sample within that bounded window blocks the
invocation with exit 6. The artifact retains every rejected and accepted per-CPU sample, the final
admission for each phase, the baseline and pre-phase power-policy summaries, and the claim
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

**Current CI-batch status.** `ci_equiv_512` is a 512-diagram, 10,635-node / 10,123-edge
equivalence-clean job. Every row ran on `thinkstation1` (32 physical cores / 64 logical threads);
the process-level worker probe observed exactly the requested participation. The live
mermaid-js 11.15.0 arm measured 24,351.600 ms for the whole job.

| requested | observed | Rust whole-job median | effect ratio and bootstrap 95% CI | corrected gate |
|---:|---:|---:|---:|---|
| 1 | 1 | 34.182970 ms | 712.389825× [699.080772×, 736.410175×] | **pass** |
| 8 | 8 | 4.774816 ms | 5,100.008042× [5,028.063855×, 5,264.077738×] | **pass** |
| 32 | 32 | 1.683505 ms | 14,464.821904× [14,029.697310×, 15,648.501123×] | **fail** — Rust-before null median 1.025448 |
| 64 | 64 | 1.820520 ms | 13,376.178235× [13,083.153389×, 13,905.084595×] | **pass** |
| 128 | 128 (oversubscribed on 64) | 3.440187 ms | 7,078.568694× [6,654.366489×, 7,427.614224×] | **fail** — Rust null medians 0.964308 / 0.968836 |

Only the three passing rows are competitive claims. The two failed rows are retained because the
shape is part of the finding, but clause 3 makes them inconclusive. The output gate is SVG
structural rather than rasterized: 512/512 diagrams passed rendered-text containment, node-set
equality, cross-engine edge topology, and per-engine topology against input truth. The 128 arm is
an explicit oversubscription experiment, not a claim that the host has 128 hardware threads.

## Corpus

34 items in three tiers.

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

The 5,000- and 10,000-entity ER endpoints are `RangeError`/`CANNOT` rows. The 500-diagram CI batch
is blocked by its failed output-equivalence verdict. The 1,001-revision trace is a measured
`DNF-timeout`: frankenmermaid completes the job, while mermaid-js remains working at the 600-second
deadline. Every input remains deterministic and SHA-256-pinned in `pins.json`; a numeric
competitive result additionally requires a passing exact-corpus output verdict.

`architecture` uses `subgraph`, which is a different layout problem from the flat generators: the
cluster boundaries constrain placement and force the router around obstacles the flat shapes never
produce. `er_schema` carries attribute blocks, which makes it text-measurement-bound rather than
graph-bound.

**Realistic end-to-end tier (9 rows, 5 jobs).** These jobs use seeded, right-skewed distributions
instead of uniform `Node 123` fixtures:

| User job | Items / sizes | Realism carried by the input |
|---|---|---|
| Documentation-site render | `docs_site_50`, `docs_site_200` | 50 and 200 diagrams; flowchart-dominated type mix, right-skewed sizes, non-ASCII and escaping-heavy labels. |
| CI render farm | `ci_equiv_512`, `ci_docs_2000`, `ci_docs_5000` | `ci_equiv_512` is one 512-process-flowchart job (10,635 nodes, 4–59 per diagram) whose escaping-heavy visible text and every rendered path admit structural, input-grounded comparison. The 2,000/5,000 jobs retain the five-syntax distribution but remain blocked by class member/relationship and state-label correctness defects. |
| Live typing preview | `typing_trace_60` | 60 successive keystrokes inside one label of a 40-node flowchart. |
| Monorepo architecture review | `monorepo_arch_120`, `monorepo_arch_300` | 120 and 300 services across uneven domains; hub-skewed dependencies and cross-domain event links. |
| Database-catalog publish | `schema_catalog_25` | 25 bounded-context ER diagrams, 8–75 entities each, with skewed relationships and varied field counts/types. |

One sample is the complete named job: source strings in, parse + layout + render, serialized SVG
strings out. Corpus generation and the caller's final file copy are outside both engines' timers;
the library work and output serialization that differ between the implementations are inside. CI
rows are decided and published as whole-job wall times; a per-diagram mean is not used because it
would divide away the caller-concurrency effect.

Artifacts are under `.benchmarks/headtohead/realistic-*`,
`.benchmarks/headtohead/ci-equiv-512-*`, and `.benchmarks/headtohead/ci-docs-2k5k-equivalence/`.
The full 2k/5k output run found 1,705/2,000 and 4,291/5,000 equivalent, with zero unverified. Every
class diagram diverges because members are clipped; `o--` also creates phantom class nodes. State
labels containing `&`, `<config>`, or `(429)` either drop transitions or are misread as node-shape
syntax. The named mixed-family jobs therefore remain nonnumeric until `bd-4isi`, `bd-92b6`, and
`bd-yq3k` are fixed and fresh exact-corpus verdicts pass. Filtering to the passing diagrams would
change the pinned workload and must use a new corpus ID. Other previously timed rows likewise
require exact-corpus equivalence before publication.
Both monorepo maps pass `mermaid.parse()` but fail in `mermaid.render()` with
`TypeError: Cannot set properties of undefined (setting 'order')`; they are `CANNOT` and carry no
ratio.

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

This is also the only check that catches an incomplete `--clean-overlay` overlay list: that build mode
deliberately excludes uncommitted edits it was not told about, so it can hand you a binary without
your change in it. The self-reported hash moving (or not) is the evidence. It does **not** yet record
*which worker* compiled the binary; add that alongside it when a worker-built artifact is timed here.

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
`median_ci_gate` verdict, a same-ELF Rust-before/Rust-after bracket verdict, and an
`output_equivalence` verdict plus `content_verified` flag.

`.benchmarks/headtohead/equivalence/equivalence-<rev>-<ts>.json` — schema
`frankenmermaid.headtohead.equivalence.v1`. Carries host identity, the executing Rust ELF SHA-256, the
loaded mermaid bundle SHA-256, the dump↔measured-render hash linkage, a per-family breakdown, and the
full check detail for every divergent diagram. `run.mjs` matches an artifact to a row on all three of
input SHA-256, ELF SHA-256 and bundle SHA-256 — a stale artifact from a different binary is worse than
none, because it reads as verification.
