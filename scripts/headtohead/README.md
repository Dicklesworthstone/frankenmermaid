# Pinned mermaid-js head-to-head harness (`bd-1buv.1`)

A repeatable comparator that measures frankenmermaid against the **original** mermaid-js on a fixed
corpus, with pinned provenance, warmup discipline, an environment fingerprint, and a measured
same-invocation noise floor.
Every dominance claim in `evidence/ledger/mermaid-js-head-to-head.toml` should be reproducible with
one command here.

## Run it

```bash
# 1. normal compile/test validation is strict-remote and never writes a local target
df -h /data  # abort and report if available space is below 120G
RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- \
  cargo test --profile release -p frankenmermaid-cli --example headtohead

# 2. a full browser head-to-head genuinely needs the executable locally. Only in an assigned
#    measurement window, reuse this repo's one existing target/ directory; never mint a task target.
df -h /data  # the same 120G floor applies
env -u CARGO_TARGET_DIR cargo build --profile release \
  -p frankenmermaid-cli --example headtohead

# 3. run both engines over byte-identical inputs
node scripts/headtohead/run.mjs \
  --fm-bin target/release/examples/headtohead
```

Useful flags: `--only <corpus_id>[,<corpus_id>…]`, `--reps-scale 0.25` (fast smoke),
`--js-budget-scale 0.1` (shrink the mermaid wall budgets for a smoke run), `--skip-mermaid`,
`--pin-cpu auto|N|off`, `--out <dir>`, `--update-pins`.

Exit codes: `0` green · `1` an engine errored · `3` corpus drift · `4` median-CI gate failed.
A comparator **DNF** is not an error and does not fail the run — see "Did not finish" below.

A gate failure (`4`) means either the claimed ratio did not clear the measured A/A floor or an arm
could not produce the minimum null sample. It is **inconclusive**, not a regression. Re-run in an
assigned quiet window; never re-pin or retune an item to force a pass.

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

- `securityLevel: "strict"` is mermaid's default, but slower than the `loose` earlier ad-hoc
  comparators used (DOMPurify sanitization stays on).
- The frankenmermaid runner is pinned to **one** core; Chromium keeps the whole machine.
- `maxEdges` / `maxTextSize` are raised above mermaid's defaults so the large items render at all.
  These are guardrails, not performance knobs.

A mermaid render that throws, or that returns mermaid's "Syntax error" placeholder SVG, is reported
as `status: "error"` and fails the whole run. A comparator that cannot render is never a silent win.

## Measurement methodology

**Warmup.** Every item runs untimed warmup iterations first (JIT warm on mermaid's side, allocator
and branch predictors on ours).

**Batching.** A 69 µs pipeline cannot be timed one iteration at a time on a shared box — a single
timer interrupt is a large fraction of the sample. The Rust runner therefore batches iterations until
each timed sample spans ≥ 2 ms and divides. Batching is a timing device only: every iteration still
renders the whole diagram. mermaid's items are all ≥ 30 ms, so they need no batching.

**Same-invocation A/A.** The Rust runner factors timing into one paired routine. For every item it
first calls that routine with `(default, default)`, then with `(default, lean)`. Both arms are timed
back-to-back inside each round, order alternates, both calls use the same batch, and the statistic is
the median of per-round ratios. Each row prints the A/A and A/B ratio, bootstrap 95 % CI, CV/MAD, and
both output checksums.

The cross-runtime headline is a whole-runtime comparison: Rust and JavaScript cannot be two arms in
one binary. The mermaid runner therefore emits its own in-browser `(mermaid, mermaid)` paired null in
the same Chromium invocation. The driver uses the **larger** of the Rust and mermaid A/A CI radii, so
the claim must clear both runtimes' measured floors.

**Median-CI gate, never CV or MAD.** For each engine,
`radius = max(abs(ci95_lo - 1), abs(ci95_hi - 1))`. A ratio passes only when its magnitude is at least
`max(1.01, 1 + 2 * max(radius_rs, radius_js))`. Each null has at least nine paired rounds. `cv_pct`
and `mad_pct` remain in every record as provenance, with `cv_gate: "never"`; neither can block a
verdict.

On a budgeted XL item that cannot afford nine comparator null rounds, the harness still attempts one
real sample. A timeout can honestly establish DNF. If it completes, the row is inconclusive and the
median-CI gate fails because its null control is insufficient.

**Two report-only estimators.** The harness reports both p50-based and min-based speedup. Their
agreement is useful diagnostic context, but only the same-invocation null median-CI decides the row.

**Determinism.** Every timed iteration's output length is checked against a reference render, and the
full bytes are compared once outside the timed region. A nondeterministic render fails the run.

## Corpus

25 items in two tiers.

**The pinned baseline (13).** Flowcharts (10/100/500 nodes), wide layered DAGs (8×16, 12×24, 16×32 —
up to 512 nodes / 960 edges), a dense DAG (200 nodes / 790 edges), an SCC-heavy cyclic graph, one
each of sequence, class, state and ER, and an **edit trace**. `flowchart` and `wide` reproduce
`crates/fm-cli/benches/pipeline_bench.rs`'s generators byte for byte, so harness numbers stay
comparable with the criterion history. Their input hashes have never moved.

**Workload classes the baseline never covered (12).** The items above top out at 500-node flowcharts.
That is neither where mermaid is used nor where it hurts, and a self-time profile measured only
there is a statement about the corpus as much as about the code. Three classes were added:

| Class | Items | What it is |
|---|---|---|
| **XL** | `flowchart_xl_2000`, `flowchart_xl_5000`, `arch_100x50`, `arch_200x50`, `er_schema_1000x6`, `er_schema_2500x8`, `er_schema_5000x8`, `er_schema_10000x8` | Thousands of nodes, through the campaign's explicit **5,000–10,000-node** architecture and ER range. Architecture maps use `subgraph`; schemas carry attribute blocks. |
| **EDIT** | `edit_trace_200x200`, `edit_trace_500x1000` | Live-preview sessions of 201 and 1,001 successive full documents, rather than a 21-edit sketch. |
| **DOC_BUILD / CI** | `doc_build_40`, `ci_batch_500` | A docs page and a repository-scale CI job: 40 and 500 diagrams across five syntax families, each timed as one batch. |

The 5k/10k ER endpoints, 1,001-revision trace, and 500-diagram CI batch were admitted and pinned by
Lane L without running either engine. They are **unmeasured workloads**, not performance claims.
Run them only when the campaign assigns a quiet measurement window; until then their only certified
facts are deterministic generation, stable hashes, and unchanged hashes for every older item.

`architecture` uses `subgraph`, which is a different layout problem from the flat generators: the
cluster boundaries constrain placement and force the router around obstacles the flat shapes never
produce. `er_schema` carries attribute blocks, which makes it text-measurement-bound rather than
graph-bound.

### Did not finish

At XL sizes the honest question is not "how much faster" but "does the comparator finish at all".
Items in the new tier carry `js_budget_ms` (a wall budget for the mermaid arm) and `dnf_allowed`.
The mermaid runner first does one untimed **probe** render of the item's largest document under that
budget, so an item that cannot be rendered is discovered in one render rather than `warmup + reps`
of them. Two outcomes are recorded, and they support different claims:

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

Internally every corpus item is a trace; a single-shot item is just a one-revision one. That keeps one
code path in both engines, and it is why adding traces left all 12 pre-existing corpus hashes
byte-identical (joining a one-element array yields the element).

Note this measures *full re-render* on both sides, which is the fair comparison. frankenmermaid's
incremental-layout path is a separate lever (`bd-1buv.3`) and is not exercised here.

## Output

`.benchmarks/headtohead/run-<rev>-<ts>.jsonl` — one event per engine per item.
`.benchmarks/headtohead/summary-<rev>-<ts>.json` — env fingerprint, pins, joined rows, ratios, gate.

Both use schema `frankenmermaid.headtohead.v2`; v2 adds per-engine null controls and replaces the
old MAD verdict fields with `median_ci_gate`.
