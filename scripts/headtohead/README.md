# Pinned mermaid-js head-to-head harness (`bd-1buv.1`)

A repeatable comparator that measures frankenmermaid against the **original** mermaid-js on a fixed
corpus, with pinned provenance, warmup discipline, an environment fingerprint, and a dispersion gate.
Every dominance claim in `evidence/ledger/mermaid-js-head-to-head.toml` should be reproducible with
one command here.

## Run it

```bash
# 1. build the frankenmermaid side (per-crate; never a workspace-wide cargo command)
CARGO_TARGET_DIR=/data/projects/.rch-targets/<yours> \
  cargo build --release -p frankenmermaid-cli --example headtohead

# 2. run both engines over byte-identical inputs
node scripts/headtohead/run.mjs \
  --fm-bin /data/projects/.rch-targets/<yours>/release/examples/headtohead
```

Useful flags: `--only <corpus_id>`, `--reps-scale 0.25` (fast smoke), `--skip-mermaid`,
`--pin-cpu auto|N|off`, `--out <dir>`, `--update-pins`.

Exit codes: `0` green · `1` an engine errored · `3` corpus drift · `4` dispersion gate failed.

A gate failure (`4`) means *the environment was too noisy for that item*, not that the code regressed —
re-run it. Never re-pin or retune an item to make a gate pass.

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

**Dispersion gate: MAD, not CV.** Timing noise on a shared machine is *one-sided* — preemption,
interrupts and frequency dips only ever make an iteration slower. That right tail inflates the
standard deviation (and so the coefficient of variation) even when the bulk of iterations are tightly
clustered. The harness therefore gates on **median absolute deviation** ≤ 5 % of the median, which
measures dispersion of the uncontaminated regime. `cv_pct` is still recorded, just not gated on. The
gate is blocking for frankenmermaid and advisory for mermaid, whose slowest item cannot afford enough
reps to tighten its dispersion (and whose variance is dwarfed by a 1000× ratio).

**Two estimators.** Because noise is one-sided, `min` is the least-contaminated estimate of the true
cost. The harness reports both the `p50`-based and the `min`-based speedup. **If the two disagree
materially, the run was noisy and the claim is not robust** — this is the harness's own check on
itself, and it is how the `wide_12x24` p50 outlier (4747× vs 2976× by min) was caught.

**Determinism.** Every timed iteration's output length is checked against a reference render, and the
full bytes are compared once outside the timed region. A nondeterministic render fails the run.

## Corpus

13 items: flowcharts (10/100/500 nodes), wide layered DAGs (8×16, 12×24, 16×32 — up to 512 nodes /
960 edges), a dense DAG (200 nodes / 790 edges), an SCC-heavy cyclic graph, one each of sequence,
class, state and ER, and an **edit trace**. `flowchart` and `wide` reproduce
`crates/fm-cli/benches/pipeline_bench.rs`'s generators byte for byte, so harness numbers stay
comparable with the criterion history.

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

Both are schema-stable (`frankenmermaid.headtohead.v1`) for the evidence perf-report path.
