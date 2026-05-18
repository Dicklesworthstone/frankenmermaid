# Changelog

All notable changes to **frankenmermaid** are documented here.

> frankenmermaid is a Rust-first, Mermaid-compatible diagram engine with
> intent-aware parsing, 15 layout algorithms, and SVG / terminal / Canvas2D /
> WASM rendering from a single intermediate representation.
>
> Repository: <https://github.com/Dicklesworthstone/frankenmermaid>
> Live demo: <https://dicklesworthstone.github.io/frankenmermaid/>

There are no tagged releases yet — the workspace is at version `0.1.0` across
all crates and crates.io publishing is being prepared (see
[`CRATES_IO_PUBLISHING.md`](https://github.com/Dicklesworthstone/frankenmermaid/blob/main/CRATES_IO_PUBLISHING.md)).
The sections below are organized chronologically and grouped by capability
area. Every commit link points to the canonical GitHub history. Beads issue
identifiers (`bd-XXXX`) reference the dependency-aware task tracker in
[`.beads/`](https://github.com/Dicklesworthstone/frankenmermaid/tree/main/.beads).

---

## 2026-05-16 — Test fixture cleanup

- Removed the obsolete `frankentui_conformance` fixture directory; the
  conformance harness now lives in
  `crates/fm-cli/tests/frankentui_conformance_test.rs` with
  `frankentui_conformance_cases.json`
  ([a70587f](https://github.com/Dicklesworthstone/frankenmermaid/commit/a70587f))

---

## 2026-04-21 — Crates.io migration of `franken-kernel` and `asupersync`

The two upstream dependencies that historically blocked workspace publication
to crates.io are now consumed from crates.io rather than pinned to git.

- Replace git-pinned `franken-kernel` with crates.io `v0.3.0`
  ([41e7efa](https://github.com/Dicklesworthstone/frankenmermaid/commit/41e7efa)).
  `franken-kernel` provides `Budget`, `Cx`, `DecisionId`, `NoCaps`, `PolicyId`,
  `SchemaVersion`, and `TraceId` to `fm-core`; moving it off git removes the
  primary blocker called out in
  [`CRATES_IO_PUBLISHING.md`](https://github.com/Dicklesworthstone/frankenmermaid/blob/main/CRATES_IO_PUBLISHING.md).
- Bump `asupersync` from `0.3.0` to `0.3.1`
  ([fe575e2](https://github.com/Dicklesworthstone/frankenmermaid/commit/fe575e2))
- Drop the broken `legacy_mermaid_code/mermaid` gitlink and ignore the path
  ([8cf8d81](https://github.com/Dicklesworthstone/frankenmermaid/commit/8cf8d81))

The fnx-* crates remain git-pinned because they back an optional
(`fnx-integration`) feature; default builds remain crates.io-clean.

---

## 2026-04-15 — Adapton self-adjusting computation, layout-decision surface, conformance fixtures, crates.io strategy

### Incremental computation: Adapton framework

- **Adapton-style self-adjusting computation framework** in `fm-layout/src/adapton.rs`
  (epic `bd-12e`), giving the layout engine first-class memoization with
  dirty-tracking so partial graph edits avoid recomputing untouched subgraphs
  ([3e10c54](https://github.com/Dicklesworthstone/frankenmermaid/commit/3e10c54))

### Layout decision explainability

- **Layout decision explanation surface** (`bd-gy4.11`): the CLI can now emit
  a human-readable rationale describing why a particular algorithm, cycle
  strategy, and refinement plan were chosen for a given diagram
  ([65616ac](https://github.com/Dicklesworthstone/frankenmermaid/commit/65616ac))
- Config-aware layout algorithm dispatch with graph analysis: the dispatcher
  consults density, branching factor, and graph topology before picking
  Sugiyama vs. Force vs. Tree vs. specialized layouts
  ([7e73b20](https://github.com/Dicklesworthstone/frankenmermaid/commit/7e73b20))

### Conformance test harness

- **Fixture-backed FrankenTUI conformance suite** with coverage tracking,
  driven by `crates/fm-cli/tests/frankentui_conformance_cases.json` and
  documented in
  [`FEATURE_PARITY.md`](https://github.com/Dicklesworthstone/frankenmermaid/blob/main/FEATURE_PARITY.md)
  ([3921135](https://github.com/Dicklesworthstone/frankenmermaid/commit/3921135),
  [d639df0](https://github.com/Dicklesworthstone/frankenmermaid/commit/d639df0),
  [1ce6609](https://github.com/Dicklesworthstone/frankenmermaid/commit/1ce6609),
  [2319654](https://github.com/Dicklesworthstone/frankenmermaid/commit/2319654),
  [fea0360](https://github.com/Dicklesworthstone/frankenmermaid/commit/fea0360))
- Coverage matrix update for `sequence_notes`, `sequence_fragments`, and
  `flowchart_subgraph`
  ([26f3c42](https://github.com/Dicklesworthstone/frankenmermaid/commit/26f3c42))

### Publishing strategy

- **Crates.io publishing strategy** (`bd-b3h0`) with name availability check,
  workspace dependency unblock plan, and publish-order definition
  ([bf54d5c](https://github.com/Dicklesworthstone/frankenmermaid/commit/bf54d5c))

### Refactoring

- Simplify E-graph crossing minimization and improve CGA routing internals
  ([ed8d1a7](https://github.com/Dicklesworthstone/frankenmermaid/commit/ed8d1a7),
  [f4f3f7c](https://github.com/Dicklesworthstone/frankenmermaid/commit/f4f3f7c))
- Clone the pressure record to avoid a move in
  `build_layout_decision_ledger`
  ([7c906fa](https://github.com/Dicklesworthstone/frankenmermaid/commit/7c906fa))

---

## 2026-04-14 — Regression harness, E-graph crossing minimization, FxHash DoS hardening

### Regression harness with HTML report

- **Thumbnail-grid HTML report** for the regression harness with CI-friendly
  summary output and configurable performance thresholds (`bd-u785`)
  ([ec1e3ca](https://github.com/Dicklesworthstone/frankenmermaid/commit/ec1e3ca),
  [f4cd792](https://github.com/Dicklesworthstone/frankenmermaid/commit/f4cd792))

### E-graph equality-saturation crossing minimization

- **Egg-based equality saturation** for crossing minimization in
  `fm-layout/src/egraph_crossing.rs` and `egraph_ordering.rs` (`bd-1xma.2`).
  When a graph's rank ordering can be improved by rewriting node positions,
  the e-graph explores rewrites in parallel and extracts the lowest-crossing
  ordering
  ([da298ac](https://github.com/Dicklesworthstone/frankenmermaid/commit/da298ac))
- **Node-budget and timeout guards** for E-graph saturation (`bd-1xma.3`)
  ([e2143fb](https://github.com/Dicklesworthstone/frankenmermaid/commit/e2143fb))
- **Sugiyama fallback** when the E-graph exceeds its budget (`bd-1xma.4`)
  ([2e67a07](https://github.com/Dicklesworthstone/frankenmermaid/commit/2e67a07))
- **Criterion benchmarks** comparing E-graph vs greedy crossing minimization
  (`bd-1xma.5`)
  ([f2073c6](https://github.com/Dicklesworthstone/frankenmermaid/commit/f2073c6))
- E-graph budget overflow protection
  ([e70a3a0](https://github.com/Dicklesworthstone/frankenmermaid/commit/e70a3a0))
- E-graph memory explosion and budget exhaustion fault tests (`bd-1s1g.1`)
  ([3fe4956](https://github.com/Dicklesworthstone/frankenmermaid/commit/3fe4956))

### Adversarial-input hardening

- **FxHash collision and DoS resistance tests** (`bd-1s1g.7`) verifying that
  pathological IDs do not push hash-keyed structures into quadratic behavior
  ([c324df2](https://github.com/Dicklesworthstone/frankenmermaid/commit/c324df2))

### DOT parser fixes

- Handle quoted nodes in DOT edge groups
  ([f039c7a](https://github.com/Dicklesworthstone/frankenmermaid/commit/f039c7a))
- Handle single-quoted strings in `split_dot_by`
  ([4c0c824](https://github.com/Dicklesworthstone/frankenmermaid/commit/4c0c824))

---

## 2026-04-13 — Conformal Geometric Algebra (CGA) edge routing and SVG transforms

The geometric algebra epic (`bd-3cd8` family) replaces the ad-hoc matrix
pipeline with rotor composition and intersection queries.

### CGA edge routing

- **CGA geometric object types** (`bd-2q3f.3`) — points, lines, circles, and
  bivectors used for intersection queries
  ([f0d7414](https://github.com/Dicklesworthstone/frankenmermaid/commit/f0d7414))
- **CGA intersection queries wired into edge routing**, so segments are
  tested against node obstacles using the same algebra used for transforms
  ([16d33ac](https://github.com/Dicklesworthstone/frankenmermaid/commit/16d33ac))
- CGA routing now detects segments that pass through obstacles
  ([3503469](https://github.com/Dicklesworthstone/frankenmermaid/commit/3503469))

### CGA SVG transforms

- **TransformStack and CGA transform module** for SVG rendering (`bd-2q3f.2`)
  ([7645c80](https://github.com/Dicklesworthstone/frankenmermaid/commit/7645c80))
- Reject non-positive scale factors and improve SVG transform parity
  ([bb453eb](https://github.com/Dicklesworthstone/frankenmermaid/commit/bb453eb))
- Fix CGA rotor inverse and scale extraction bugs
  ([95fc5dd](https://github.com/Dicklesworthstone/frankenmermaid/commit/95fc5dd))

### Real-world corpus ingestion

- **Real-world Mermaid corpus ingestion pipeline** (`bd-2xl.15`) used to drive
  the regression harness against representative third-party diagrams
  ([62a02ca](https://github.com/Dicklesworthstone/frankenmermaid/commit/62a02ca))

---

## 2026-04-12 — FNX Phase-2 directed graph rollout (gates, drills, compatibility matrix)

The FNX integration moved from Phase 1 (undirected advisory) to Phase 2
(directed: SCC, WCC, directed cycles, reachability) under the
`bd-ml2r.7`/`bd-ml2r.8` epics. Directed algorithms are implemented natively in
`fm-layout/src/fnx_directed.rs` rather than waiting on upstream fnx.

### Phase-2 directed algorithm surface

- **Directed algorithm surface for FNX Phase 2** (`bd-ml2r.7.1`) wrapping
  Tarjan's SCC, WCC via BFS, directed cycle detection, and reachability
  analysis with deterministic output ordering
  ([a30b794](https://github.com/Dicklesworthstone/frankenmermaid/commit/a30b794))
- **FNX compatibility matrix** (`bd-ml2r.7.2`) tracking available vs
  pending fnx capabilities per phase; documented in
  [`docs/FNX_COMPATIBILITY_MATRIX.md`](https://github.com/Dicklesworthstone/frankenmermaid/blob/main/docs/FNX_COMPATIBILITY_MATRIX.md)
  ([662b86e](https://github.com/Dicklesworthstone/frankenmermaid/commit/662b86e))

### Rollout gates and canary drills

- **Phase-1 rollout gate tests** with go/no-go decision evidence (`bd-ml2r.8.1`)
  ([40ab2d3](https://github.com/Dicklesworthstone/frankenmermaid/commit/40ab2d3))
- **Phase-2 directed rollout gate tests and policy** (`bd-ml2r.8.2`),
  documented in
  [`docs/FNX_PHASE2_ROLLOUT.md`](https://github.com/Dicklesworthstone/frankenmermaid/blob/main/docs/FNX_PHASE2_ROLLOUT.md)
  ([997a526](https://github.com/Dicklesworthstone/frankenmermaid/commit/997a526))
- **Canary rollout state machine and drill tests** (`bd-ml2r.12.4`) in
  `fm-core/src/canary.rs`
  ([8fd602d](https://github.com/Dicklesworthstone/frankenmermaid/commit/8fd602d))

### Documentation and migration

- **FNX migration guide** (`bd-ml2r.12.3`) with risk-tiered adoption checklists
  ([6000a26](https://github.com/Dicklesworthstone/frankenmermaid/commit/6000a26))
- **FNX user guide and examples** (`bd-ml2r.9.4`)
  ([36e0be5](https://github.com/Dicklesworthstone/frankenmermaid/commit/36e0be5))
- **FNX differential quality/performance reporting** (`bd-ml2r.12.2`)
  ([bb0ec6b](https://github.com/Dicklesworthstone/frankenmermaid/commit/bb0ec6b))
- **FNX-off baseline snapshot and invariant tests** (`bd-ml2r.12.1`)
  ([45b3f15](https://github.com/Dicklesworthstone/frankenmermaid/commit/45b3f15))

### golden test infrastructure

- New `xychart_comprehensive` golden case + `FM_GOLDEN_CASE` filter
  ([5f770b6](https://github.com/Dicklesworthstone/frankenmermaid/commit/5f770b6))
- ClassDef/style golden test (`bd-65l0`) and edge-case coverage (`bd-b65r`)
  ([6d6f980](https://github.com/Dicklesworthstone/frankenmermaid/commit/6d6f980),
  [54b1858](https://github.com/Dicklesworthstone/frankenmermaid/commit/54b1858))
- Detect new golden cases not in blessed baselines
  ([8a2325d](https://github.com/Dicklesworthstone/frankenmermaid/commit/8a2325d))

---

## 2026-04-10 — FNX Phase 1: centrality tiers, witness metadata, CLI modes, cycle scoring

### Centrality-aware semantic styling (`bd-ml2r.9`)

- **Centrality cache and stable normalization** (`bd-ml2r.5.1`)
  ([b64db94](https://github.com/Dicklesworthstone/frankenmermaid/commit/b64db94))
- **Centrality tie-breaks in barycenter ordering** (`br-ml2r.5`) so hub
  nodes settle into stable rank positions
  ([b98deab](https://github.com/Dicklesworthstone/frankenmermaid/commit/b98deab))
- **Populate centrality tiers in Sugiyama layout** (`bd-ml2r.9.2`)
  ([c73324b](https://github.com/Dicklesworthstone/frankenmermaid/commit/c73324b))
- **Centrality tier CSS classes in SVG node output** so hub nodes can be
  styled semantically (`fm-node-centrality-high`, etc.)
  ([dd13980](https://github.com/Dicklesworthstone/frankenmermaid/commit/dd13980))
- O(n²) → O(n) centrality lookup optimization, plus xychart category padding
  ([3895c50](https://github.com/Dicklesworthstone/frankenmermaid/commit/3895c50))

### FNX witness metadata

- **FNX witness metadata in CLI JSON output** (`bd-ml2r.6.1`) — every render
  surface that records FNX analysis emits a `witness` block with the metrics
  used and the bookkeeping needed for differential reporting
  ([774a493](https://github.com/Dicklesworthstone/frankenmermaid/commit/774a493))
- **FNX witness in WASM API** (`bd-ml2r.6.2`)
  ([443b47b](https://github.com/Dicklesworthstone/frankenmermaid/commit/443b47b))
- Wire `fnx_witness` into render/validate paths
  ([ab87911](https://github.com/Dicklesworthstone/frankenmermaid/commit/ab87911))
- Include `fnx_enabled` in the layout cache key so cached layouts cannot leak
  across modes
  ([4504698](https://github.com/Dicklesworthstone/frankenmermaid/commit/4504698))

### CLI controls

- **CLI flags for FNX integration modes** (`bd-ml2r.9.1`): `--fnx-mode`,
  `--fnx-projection`, `--fnx-fallback` across `render` and `validate`
  ([341f281](https://github.com/Dicklesworthstone/frankenmermaid/commit/341f281))

### FNX diagnostics, cycle scoring, fallback ladder

- **FNX parser-side structural diagnostics** (`bd-ml2r.3.1`) and integration
  into `fm-cli validate` (`bd-ml2r.3.2`)
  ([3e8c948](https://github.com/Dicklesworthstone/frankenmermaid/commit/3e8c948),
  [c0a50ef](https://github.com/Dicklesworthstone/frankenmermaid/commit/c0a50ef))
- **FNX edge criticality scoring for cycle removal** (`bd-ml2r.4.1`)
  ([4c35b17](https://github.com/Dicklesworthstone/frankenmermaid/commit/4c35b17))
- **Deterministic FNX analysis cache** (`bd-ml2r.10.2`) and **analysis budget
  enforcement** (`bd-ml2r.10.1`)
  ([d838bb6](https://github.com/Dicklesworthstone/frankenmermaid/commit/d838bb6),
  [a4fe1de](https://github.com/Dicklesworthstone/frankenmermaid/commit/a4fe1de))
- **Fallback ladder and strict-mode behavior** when FNX analysis exhausts its
  budget (`bd-ml2r.10.3`)
  ([1e2c2e4](https://github.com/Dicklesworthstone/frankenmermaid/commit/1e2c2e4))
- **Bridge detection** in FNX diagnostics and cycle scorer
  ([5b7686b](https://github.com/Dicklesworthstone/frankenmermaid/commit/5b7686b))
- **FNX adapter stable ID projection** (`bd-ml2r.2.1`) and directed/undirected
  projection policy (`bd-ml2r.2.2`)
  ([bb34c64](https://github.com/Dicklesworthstone/frankenmermaid/commit/bb34c64),
  [464884a](https://github.com/Dicklesworthstone/frankenmermaid/commit/464884a))

### Quality and benchmarking

- **Ablation benchmark and adoption threshold** for FNX (`bd-ml2r.5.2`)
  ([d18525e](https://github.com/Dicklesworthstone/frankenmermaid/commit/d18525e))
- **Benchmark regression harness with determinism replay** (`bd-ml2r.11.3`)
  ([9071a8a](https://github.com/Dicklesworthstone/frankenmermaid/commit/9071a8a))
- **FNX E2E scenario tests** (`bd-ml2r.11.2`) and diagnostics edge cases
  (`bd-ml2r.11.1`)
  ([ec6283b](https://github.com/Dicklesworthstone/frankenmermaid/commit/ec6283b),
  [2ab2873](https://github.com/Dicklesworthstone/frankenmermaid/commit/2ab2873))
- **Structured recommendations with category/confidence** (`bd-ml2r.9.3`)
  ([2365426](https://github.com/Dicklesworthstone/frankenmermaid/commit/2365426))
- **Structured evidence log schema** for FNX QA (`bd-ml2r.11.4`)
  ([71b88b7](https://github.com/Dicklesworthstone/frankenmermaid/commit/71b88b7))

### npm publishing prep

- **npm-publish CI job** for the `@frankenmermaid/core` WASM bundle
  (`bd-hye0`)
  ([7853562](https://github.com/Dicklesworthstone/frankenmermaid/commit/7853562))

### README accuracy fixes

- Document the demo features actually shipped (`bd-3y9i`,
  [587233a](https://github.com/Dicklesworthstone/frankenmermaid/commit/587233a),
  [6fb868e](https://github.com/Dicklesworthstone/frankenmermaid/commit/6fb868e))

---

## 2026-04-07 — DOT parser hardening, style sanitization, classDef edge cases

### Security / sanitization hardening

- Harden CSS style sanitization (`bd-swtz`) against comment obfuscation
  ([49e29d0](https://github.com/Dicklesworthstone/frankenmermaid/commit/49e29d0))
- Harden style sanitizer casing (`bd-72ij`) against case-insensitive
  `javascript:` schemes
  ([ffe82e4](https://github.com/Dicklesworthstone/frankenmermaid/commit/ffe82e4))
- Enforce allowed inline style properties in `from_pairs` (`bd-20ot`)
- Harden SVG link emission against unsafe hrefs (`bd-b2l4`)
- Resolve SVG `var()` fallbacks during PNG rasterization (`bd-rk5u`); handle
  nested parentheses in fallback resolution (`bd-a240`)
- Harden DOT comment handling (`bd-udou`)
  ([7212217](https://github.com/Dicklesworthstone/frankenmermaid/commit/7212217))
- Fix DOT header detection with comments/brace adjacency (`bd-a80w`)
  ([72ea106](https://github.com/Dicklesworthstone/frankenmermaid/commit/72ea106))
- Fix DOT operator detection (`bd-3kxn`)
  ([3338aff](https://github.com/Dicklesworthstone/frankenmermaid/commit/3338aff))
- Hash DOT symbol-only identifiers (`bd-d4at`)
  ([a8055ee](https://github.com/Dicklesworthstone/frankenmermaid/commit/a8055ee))
- Fix DOT body extraction for braces in quoted graph IDs (`bd-kpkv`) and
  dispatch logging (`bd-7sb6`)
  ([abb7a03](https://github.com/Dicklesworthstone/frankenmermaid/commit/abb7a03))
- Fix `serve` request limit overflow (`bd-xzr3`)
  ([f267e65](https://github.com/Dicklesworthstone/frankenmermaid/commit/f267e65))

### Parser polish

- Tolerate trailing semicolons in flowchart direction headers
  ([fd6b8b1](https://github.com/Dicklesworthstone/frankenmermaid/commit/fd6b8b1))
- Handle `gitGraph` direction tokens with trailing punctuation (`bd-wvqt`)
  ([5d766da](https://github.com/Dicklesworthstone/frankenmermaid/commit/5d766da))
- Add golden tests for long-tail diagram types (`bd-k3bv`)
  ([346b6d9](https://github.com/Dicklesworthstone/frankenmermaid/commit/346b6d9))

### CLI input limits

- Enforce `core.max_input_bytes` via bounded reads on stdin and files
  ([894fdc1](https://github.com/Dicklesworthstone/frankenmermaid/commit/894fdc1))

---

## 2026-04-06 — Incremental layout engine, epoch-based concurrent IR, vEB rewrite, TOML config

### Incremental layout (`bd-12e` epic)

- **Massive incremental layout engine expansion** with dependency-graph
  caching, so subsequent renders of a near-identical diagram skip stages whose
  inputs have not changed
  ([80d3744](https://github.com/Dicklesworthstone/frankenmermaid/commit/80d3744))
- Incremental layout engine + benchmarks + proptest regressions
  ([da3f3e0](https://github.com/Dicklesworthstone/frankenmermaid/commit/da3f3e0))
- Incremental overlap alignment, east-asian width support
  ([18cc855](https://github.com/Dicklesworthstone/frankenmermaid/commit/18cc855))
- Memoized reuse marked as incremental in the trace (`bd-n3wn`)
  ([018d028](https://github.com/Dicklesworthstone/frankenmermaid/commit/018d028))
- Record layout recompute duration in trace (`bd-5pdg`)
  ([0c45bd4](https://github.com/Dicklesworthstone/frankenmermaid/commit/0c45bd4))

### Epoch-based concurrent IR

- **Epoch-based concurrent IR handle** in `fm-core/src/epoch.rs`, letting
  multiple readers observe a consistent IR snapshot while another agent
  prepares the next epoch
  ([c9cd75b](https://github.com/Dicklesworthstone/frankenmermaid/commit/c9cd75b))

### vEB layout rewrite + boundary smoothing

- **van Emde Boas (vEB) layout rewrite**, boundary edge smoothing, and a fix
  for the `click` callback wiring
  ([7c37e2f](https://github.com/Dicklesworthstone/frankenmermaid/commit/7c37e2f))

### TOML config + Cloudflare Pages ops

- **TOML config file support** with auto-discovery
  (`./frankenmermaid.toml` → `~/.config/frankenmermaid/config.toml`) and
  per-subcommand overrides
  ([4b238c6](https://github.com/Dicklesworthstone/frankenmermaid/commit/4b238c6))
- **E-graph crossing minimization** + wasm32 conditional compilation + Cloudflare
  Pages ops integration via `scripts/cloudflare_pages_ops.py`
  ([58b7721](https://github.com/Dicklesworthstone/frankenmermaid/commit/58b7721))

### WASM bindings expansion

- Extend wasm bindings with additional exported functions (`diagramLens`,
  `applyLensEdit`, `parseLens`, `applyParseLensEdit`, `describeDiagram`,
  `source_spans_js`, `capability_matrix_js`); refresh type declarations
  ([d0f00e4](https://github.com/Dicklesworthstone/frankenmermaid/commit/d0f00e4),
  [b4fc12f](https://github.com/Dicklesworthstone/frankenmermaid/commit/b4fc12f))
- Integrate the IncrementalLayoutEngine into the `Diagram` wasm struct
  ([96e9aca](https://github.com/Dicklesworthstone/frankenmermaid/commit/96e9aca))
- WebRenderer selection with canvas2d/WebGPU fallback and lens edit bindings
  ([c474f21](https://github.com/Dicklesworthstone/frankenmermaid/commit/c474f21))
- Slim wasm bindings with `cfg`-gated native-only features and JS source-spans
  helper
  ([e266bac](https://github.com/Dicklesworthstone/frankenmermaid/commit/e266bac))

### Conformance harness scaffolding

- **Fixture-backed FrankenTUI conformance harness** scaffolding
  ([34c2395](https://github.com/Dicklesworthstone/frankenmermaid/commit/34c2395))
- Showcase / e2e harness scripts and static web bootstrap pages
  ([531b2b1](https://github.com/Dicklesworthstone/frankenmermaid/commit/531b2b1))
- `fm-core` CGA tests, integration test updates
  ([eeb14db](https://github.com/Dicklesworthstone/frankenmermaid/commit/eeb14db))

---

## 2026-04-05 — Stress testing, criterion benchmarks, demo expansion, intent-reality closure

### Performance baselines

- **1K-node and dense-graph stress tests**, pipeline benchmark stub, and
  showcase expansion
  ([6f45f5d](https://github.com/Dicklesworthstone/frankenmermaid/commit/6f45f5d))
- **Criterion benchmark harness scaffold** for pipeline benchmarks
  ([121e38e](https://github.com/Dicklesworthstone/frankenmermaid/commit/121e38e))
- **Layout quality benchmarks** + CJK/emoji full-width support + cross-target
  E2E tests
  ([c2163d6](https://github.com/Dicklesworthstone/frankenmermaid/commit/c2163d6))
- Layout-aware source maps + expanded DOT parser coverage + SVG attribute
  fixes
  ([277f390](https://github.com/Dicklesworthstone/frankenmermaid/commit/277f390))

### Pressure-adaptive runtime (`bd-3uz` epic)

- **Deterministic degradation engine with operator algebra** in
  `fm-core` and `fm-layout` (`bd-3uz.7`/`bd-3uz.8`), giving layout/render a
  composable language for cost-vs-quality tradeoffs under pressure
  ([e5a9dff](https://github.com/Dicklesworthstone/frankenmermaid/commit/e5a9dff))
- Diagnostics panel, fallback preview, diff metadata tracking, and shared-core
  contracts for the showcase
  ([d26cab2](https://github.com/Dicklesworthstone/frankenmermaid/commit/d26cab2))
- DOT edge groups + port stripping + latest-edit-wins render pipeline
  ([aeba5c0](https://github.com/Dicklesworthstone/frankenmermaid/commit/aeba5c0))

### Showcase / editor

- Adversarial test corpus, split-shell editor layout
  ([679d4fe](https://github.com/Dicklesworthstone/frankenmermaid/commit/679d4fe))
- URL state sync, adoption-decision documentation
  ([688f27a](https://github.com/Dicklesworthstone/frankenmermaid/commit/688f27a))
- Narrative metadata on featured spotlight samples
  ([072f923](https://github.com/Dicklesworthstone/frankenmermaid/commit/072f923))
- Rich editor surface with syntax lens and structural hints
  ([f471772](https://github.com/Dicklesworthstone/frankenmermaid/commit/f471772))

---

## 2026-04-02 — Evidence ledger CLI, source maps, accessibility, dimension hardening

### Evidence ledger surface

- **Evidence ledger CLI** with structured pass/fail evidence persistence
  ([5bbb2dc](https://github.com/Dicklesworthstone/frankenmermaid/commit/5bbb2dc))
- Seeded alien-CS evidence ledger and demo strategy documentation
  ([345bf14](https://github.com/Dicklesworthstone/frankenmermaid/commit/345bf14))

### Source maps + accessibility

- **Source map artifact generation**, accessibility descriptions, and CLI
  flags `--embed-source-spans` / `--source-map-out`
  ([525d1e7](https://github.com/Dicklesworthstone/frankenmermaid/commit/525d1e7))

### Golden / dimension hardening

- **SVG viewBox dimension fallback**, 3 new golden tests, extracted dimension
  helpers
  ([7921e81](https://github.com/Dicklesworthstone/frankenmermaid/commit/7921e81))
- Width/height dimension validation, 4 new golden test diagrams, xychart bar
  width formatting fix
  ([bc5afbd](https://github.com/Dicklesworthstone/frankenmermaid/commit/bc5afbd))
- 6 new golden test diagrams + font-aware mock text metrics + edge-label font
  sizing + canvas theme presets
  ([29edc39](https://github.com/Dicklesworthstone/frankenmermaid/commit/29edc39))

### Render-stack fixes

- Validate font-size and numeric SVG config overrides — reject NaN/Inf/zero/
  negative values
  ([8c2955d](https://github.com/Dicklesworthstone/frankenmermaid/commit/8c2955d))
- Resolve bounds-checking panics and coordinate scaling bugs in layout +
  render-term
  ([1345858](https://github.com/Dicklesworthstone/frankenmermaid/commit/1345858))
- Prevent legend clamp panic; make quadrant chart adaptive
  ([6cdb2d1](https://github.com/Dicklesworthstone/frankenmermaid/commit/6cdb2d1))
- Count title labels in canvas render result; align quadrant SVG with layout
  engine dimensions
  ([433195e](https://github.com/Dicklesworthstone/frankenmermaid/commit/433195e))
- Guard force layout against NaN positions; align canvas typography with SVG
  ([64a9d46](https://github.com/Dicklesworthstone/frankenmermaid/commit/64a9d46))

### Canvas rendering

- Implement path marker drawing for the canvas backend
  ([02f6762](https://github.com/Dicklesworthstone/frankenmermaid/commit/02f6762))
- Diagram title rendering test; fix font format string layout
  ([6490bda](https://github.com/Dicklesworthstone/frankenmermaid/commit/6490bda))

### Constraint solver + SVG visual effects

- **Constraint-based layout solver**, SVG gradient/filter support, terminal
  Unicode box drawing
  ([13ab3cf](https://github.com/Dicklesworthstone/frankenmermaid/commit/13ab3cf))
- Curved cylinder caps, deterministic ordering, layout-aware gantt rendering
  ([2615c8f](https://github.com/Dicklesworthstone/frankenmermaid/commit/2615c8f))
- Harden numeric inputs, fix calendar validation, ensure deterministic ordering
  ([736037b](https://github.com/Dicklesworthstone/frankenmermaid/commit/736037b))

---

## 2026-03-28 — Recursive→iterative traversal, presenter mode, comprehensive showcase

### Stack-safety: recursive → iterative rewrites

- Replace recursive `cycle_removal_dfs_back` with iterative stack-based
  traversal so deep cyclic graphs never blow the stack
  ([18546de](https://github.com/Dicklesworthstone/frankenmermaid/commit/18546de))
- Replace recursive subgraph/tree traversal with iterative variants and
  expand the showcase
  ([921d885](https://github.com/Dicklesworthstone/frankenmermaid/commit/921d885))

### Demo showcase: presenter mode

- **Presenter mode with step-sequenced guided tour** in the showcase HTML;
  expands demo strategy
  ([2e02e91](https://github.com/Dicklesworthstone/frankenmermaid/commit/2e02e91))

---

## 2026-03-27 — Diagram coverage push: icons, animations, namespaces, evidence bundles, auto-layout, kanban metadata, Gantt SVG, Sankey weighting

This was the largest single-day capability wave since the initial Mermaid
parser. The shared theme is closing the gap between syntax coverage and
visually polished render output for every diagram family.

### CSS animations

- **CSS-only diagram animations** with entrance, flow, pulse, and hover
  effects, no JavaScript required
  ([2f6a5bd](https://github.com/Dicklesworthstone/frankenmermaid/commit/2f6a5bd))

### Node icons + custom SVG icons

- **Node icon extraction**, custom SVG icons, and left-position layout
  ([f2f692e](https://github.com/Dicklesworthstone/frankenmermaid/commit/f2f692e))
- Wasm bindings + bead for icon/emoji + `SvgCustomIconOverride: Default`
  derive
  ([a64e8de](https://github.com/Dicklesworthstone/frankenmermaid/commit/a64e8de))

### Class / state / namespace extensions

- **Class cardinality**, **namespace blocks**, state notes/guards, and node
  icons
  ([3b5d4c3](https://github.com/Dicklesworthstone/frankenmermaid/commit/3b5d4c3))

### CSS style system

- **Structured CSS style system** with sanitization (parser + core), the
  foundation for safely consuming `classDef`/`style`/`linkStyle` directives
  from untrusted input
  ([7e5d316](https://github.com/Dicklesworthstone/frankenmermaid/commit/7e5d316))

### Auto-layout engine + evidence bundles

- **Auto-layout engine**, evidence timestamps, and parser refactoring
  ([a8004f7](https://github.com/Dicklesworthstone/frankenmermaid/commit/a8004f7))
- **Release evidence bundles**, requirement styling, mindmap branch colors,
  and expanded CI quality gates
  ([1872305](https://github.com/Dicklesworthstone/frankenmermaid/commit/1872305))
- **Incremental layout engine**, release-signoff command, and CI gate
  aggregation
  ([791b2aa](https://github.com/Dicklesworthstone/frankenmermaid/commit/791b2aa))

### Diagram family coverage

- **Kanban metadata** (`@{wip, priority, assigned}`), priority colors
  ([7695316](https://github.com/Dicklesworthstone/frankenmermaid/commit/7695316))
- **FxHash collections**, **packet-beta field parsing**, **Sankey
  flow-weighted node sizing**
  ([36c3705](https://github.com/Dicklesworthstone/frankenmermaid/commit/36c3705))
- **Dedicated Gantt chart SVG renderer** with type-based task coloring
  ([11aa063](https://github.com/Dicklesworthstone/frankenmermaid/commit/11aa063))
- Promote diagram support levels, IR capacity hints, fix tree traversal
  ([d1de1c6](https://github.com/Dicklesworthstone/frankenmermaid/commit/d1de1c6))
- **Promote C4 diagrams** (Context/Container/Component/Dynamic/Deployment) to
  full support + demo evidence guard
  ([0d24c9a](https://github.com/Dicklesworthstone/frankenmermaid/commit/0d24c9a))

---

## 2026-03-23 — Sequence diagram comprehensive expansion

Sequence diagrams went from "participants and messages" to full mermaid-js
parity for fragments, notes, lifecycles, and styled arrows in one push.

### Sequence rendering

- **Major sequence diagram rendering expansion** — fragments, lifelines,
  activations, notes
  ([f6520ab](https://github.com/Dicklesworthstone/frankenmermaid/commit/f6520ab))
- Extended sequence rendering with notes, fragments, and lifecycle refinements
  ([0fa688d](https://github.com/Dicklesworthstone/frankenmermaid/commit/0fa688d))
- Sequence notes + fragments + theme config + dot parser improvements
  ([4117f68](https://github.com/Dicklesworthstone/frankenmermaid/commit/4117f68))
- Sequence lifecycle markers + DottedCross arrow + notes + fragments
  ([e4b2a64](https://github.com/Dicklesworthstone/frankenmermaid/commit/e4b2a64))
- Enhance sequence lifecycle and plan Sankey rendering
  ([66d55ad](https://github.com/Dicklesworthstone/frankenmermaid/commit/66d55ad))
- Sequence arrow parity (open/half/stick arrows) + ER cardinality labels +
  edge bundling
  ([cea92a9](https://github.com/Dicklesworthstone/frankenmermaid/commit/cea92a9))
- Coalesce multiple destroy lifecycle markers per participant
  ([ec00577](https://github.com/Dicklesworthstone/frankenmermaid/commit/ec00577))
- Refine sequence note geometry and enhance SVG/terminal rendering
  ([4ecc382](https://github.com/Dicklesworthstone/frankenmermaid/commit/4ecc382))

### New diagram coverage

- **Quadrant chart support**, accessibility directives, gitgraph layout
  ([ccff015](https://github.com/Dicklesworthstone/frankenmermaid/commit/ccff015))
- Half/stick arrows, central connection, regenerated golden SVGs
  ([804e58d](https://github.com/Dicklesworthstone/frankenmermaid/commit/804e58d))
- Register Pie in concrete layout algorithm list
  ([1d3783d](https://github.com/Dicklesworthstone/frankenmermaid/commit/1d3783d))

### Title extraction

- **Comprehensive diagram title extraction and rendering** across parser
  and SVG renderer
  ([23ab91d](https://github.com/Dicklesworthstone/frankenmermaid/commit/23ab91d))

### Parser fixes

- Fix HTML entity decoding for numeric entities
  ([eece7f2](https://github.com/Dicklesworthstone/frankenmermaid/commit/eece7f2))

### Gantt expansion

- Add gantt chart metadata and expand sequence rendering
  ([827ea95](https://github.com/Dicklesworthstone/frankenmermaid/commit/827ea95))

---

## 2026-03-21 — Class diagram generics, classDef/style pipeline, Gantt rendering, new node shapes

### Class diagrams

- **Generic type parameters** (`<T, U>`) on class diagram nodes, parsed and
  rendered across SVG, terminal, and canvas backends
  ([4ed63f2](https://github.com/Dicklesworthstone/frankenmermaid/commit/4ed63f212a1e447abb872946b3877b24406b6866),
  [9da93f6](https://github.com/Dicklesworthstone/frankenmermaid/commit/9da93f68e6212f9ec8ae19faa32f7a811df537f9),
  [05b4b03](https://github.com/Dicklesworthstone/frankenmermaid/commit/05b4b039f42de74c590895453a4bbcce87b32912))
- **UML three-compartment** class box rendering in terminal and canvas backends
  ([aa7d624](https://github.com/Dicklesworthstone/frankenmermaid/commit/aa7d6246ae58cb23e79f35df290a235bbd0ee7df),
  [d5fe116](https://github.com/Dicklesworthstone/frankenmermaid/commit/d5fe116eeceb60db481d2cb21ea4abec5d2aa69f))
- Dedicated **class diagram layout engine** with parser improvements
  ([a673510](https://github.com/Dicklesworthstone/frankenmermaid/commit/a673510c33abf84eaab9658e61dfb54a718ed64c))

### Flowchart styling: classDef / style / linkStyle

- `IrStyleTarget` and `IrStyleRef` core types for style directives
  ([ccfc3da](https://github.com/Dicklesworthstone/frankenmermaid/commit/ccfc3da939b2157b02d15ea8f0f2d00cba32799f))
- Parser extraction of `classDef`, `style`, and `linkStyle` for flowcharts
  ([a71819a](https://github.com/Dicklesworthstone/frankenmermaid/commit/a71819a03a3dc36268bbf41633f2d0ab0bb77ee1))
- End-to-end pipeline wiring through core, parser, and SVG renderer
  ([d2dfc92](https://github.com/Dicklesworthstone/frankenmermaid/commit/d2dfc9293888bc7ae3183b38c447b30490d22ddb))

### Gantt chart rendering

- Gantt chart IR types and `--font-size` CLI flag
  ([c540ea4](https://github.com/Dicklesworthstone/frankenmermaid/commit/c540ea409952654b18a07013566002437c46cfb9))
- Font-size passthrough for SVG rendering and Gantt IR metadata
  ([d663c5b](https://github.com/Dicklesworthstone/frankenmermaid/commit/d663c5bfef77dc996a5d08654171b88ea2e008c4))
- Section-aware Gantt layout with proper timeline positioning
  ([17c374e](https://github.com/Dicklesworthstone/frankenmermaid/commit/17c374e19f4f43ce42fe99585d83aa0479b92437))
- Band/axis-tick SVG rendering and serde tests
  ([dc702b8](https://github.com/Dicklesworthstone/frankenmermaid/commit/dc702b87c97f7b8b719f44920b739af6e07631f6))

### New node shapes and arrow types

- `FilledCircle` and `HorizontalBar` node shapes
  ([dbfe983](https://github.com/Dicklesworthstone/frankenmermaid/commit/dbfe9832d59493c1d0e0d0a3bf876e07889ae2ba))
- New arrow types and inline edge styles in SVG and terminal renderers
  ([e6ef6ad](https://github.com/Dicklesworthstone/frankenmermaid/commit/e6ef6ad03ea4f2330e843c3e03ca81249d7e53a3))
- Layout engine and renderer improvements for new shapes
  ([d2d233e](https://github.com/Dicklesworthstone/frankenmermaid/commit/d2d233e49c46b31d10e687660681834a0f23c31d))
- Cluster dividers in SVG output
  ([05b4b03](https://github.com/Dicklesworthstone/frankenmermaid/commit/05b4b039f42de74c590895453a4bbcce87b32912))

### Parser improvements

- Expanded Mermaid parser coverage and multi-renderer output refinement
  ([f8a3423](https://github.com/Dicklesworthstone/frankenmermaid/commit/f8a342341580cde0bb1826513a409a92b0b34008),
  [d05f694](https://github.com/Dicklesworthstone/frankenmermaid/commit/d05f694a9e3769cfbe7ee13accd2d44b58a4fbbf))
- Enhanced DOT parser with shape mapping and default attribute support
  ([94c26e2](https://github.com/Dicklesworthstone/frankenmermaid/commit/94c26e21150c1eb76ed9bb8e251051d83b6767a9))

### Testing

- E2E replay determinism and ledger trace continuity tests
  ([c32a75a](https://github.com/Dicklesworthstone/frankenmermaid/commit/c32a75a36c0d16edf6f067b7a33be7e212a3514a))

### Fixes

- Gantt axis tick count and LayoutRect construction fixes
  ([f4e8873](https://github.com/Dicklesworthstone/frankenmermaid/commit/f4e8873433888f60d3983939a8b3bd411c8d8a27))
- Explicit scale factor passthrough in rendering pipeline; golden SVGs updated
  ([018d96d](https://github.com/Dicklesworthstone/frankenmermaid/commit/018d96daf7781629b52d39049ec77b54509edc2b))
- Improved diagram detection heuristics and ANSI-aware truncation
  ([77c32cb](https://github.com/Dicklesworthstone/frankenmermaid/commit/77c32cb45d05012107328c7fc33836c9ee156a53))
- Use `is_none_or` for keyword check; suppress Clippy `too_many_arguments`
  ([842ee63](https://github.com/Dicklesworthstone/frankenmermaid/commit/842ee63259d2f3a77c33c634f43e78b9c3b806d4))
- Subgraph key stability, ANSI-aware diff widths, WASM API updates
  ([7de04c8](https://github.com/Dicklesworthstone/frankenmermaid/commit/7de04c893fa6fb00202f4ef590fd03c9676a136a))

---

## 2026-03-20 — Layout decision ledger, custom font metrics, parser hardening

### Observability: MermaidLayoutDecisionLedger

- New `MermaidLayoutDecisionLedger` type wired into CLI output for full
  pipeline introspection
  ([52c202d](https://github.com/Dicklesworthstone/frankenmermaid/commit/52c202d9c02b2d31c6ad7360e00746e984529235))
- Tracing field enforcement tests and observability output format tests
  ([9d8e89c](https://github.com/Dicklesworthstone/frankenmermaid/commit/9d8e89ca6a6f703071e647767160a9ba013d29a9),
  [85984df](https://github.com/Dicklesworthstone/frankenmermaid/commit/85984dfe6bcce6edc6ef40cc9ac3bbbfd8e7d1e6))

### Layout: LayoutConfig and custom font metrics

- `LayoutConfig` with pluggable font metrics, expanded Mermaid parser coverage,
  refactored SVG text rendering
  ([57b5d24](https://github.com/Dicklesworthstone/frankenmermaid/commit/57b5d2426303e63bc0e0c9c80286736f611d1ebf))

### Parser and IR refinement

- Cluster, subgraph, and label deduplication in IR builder
  ([acd301c](https://github.com/Dicklesworthstone/frankenmermaid/commit/acd301c88e59938ad8d5d81b68695728821871f6))
- Simplified cluster title backfill with let-chains; removed dead `Subgraph`
  AST variant
  ([423202e](https://github.com/Dicklesworthstone/frankenmermaid/commit/423202e48904004cfe5056575fe46b27301bb00a))
- DOT parser attribute handling simplified; terminal diff rendering added
  ([72f114e](https://github.com/Dicklesworthstone/frankenmermaid/commit/72f114e6825c7e0de2b999ba1d02ec3605c9ae3c))

### Testing

- Stress and fuzzy-recovery fixtures with resilience suite validation
  ([2c5c063](https://github.com/Dicklesworthstone/frankenmermaid/commit/2c5c063aa0b55f87cb7f5de20d6c831f3d7c3364))

### Fixes

- Quoted identifiers with spaces; hash function stabilization
  ([15823ef](https://github.com/Dicklesworthstone/frankenmermaid/commit/15823ef2db78f8effcb3efe1ee7dcd91ab37b4e4))
- Simplified synthetic_dag edge generation
  ([f3871ad](https://github.com/Dicklesworthstone/frankenmermaid/commit/f3871ad83d3d9f88d275b97cd72e0ab839c169e7))
- Three bugs found in deep code review
  ([02a8fdc](https://github.com/Dicklesworthstone/frankenmermaid/commit/02a8fdccafb8d95c1bb1f2842799bc8e2fc3c705))

---

## 2026-03-19 — Auto algorithm selection, orthogonal edge routing, fuzz testing, test infrastructure

### Layout: auto algorithm selection

- **Graph-metrics-based automatic layout algorithm selection** -- inspects
  density, branching factor, and cycle presence to pick Sugiyama vs.
  force-directed vs. tree vs. radial
  ([927dd7b](https://github.com/Dicklesworthstone/frankenmermaid/commit/927dd7bb4dadb12b0cc2745779c4701ee36031e0))

### Layout: orthogonal edge routing

- Node-aware orthogonal edge routing with bend minimization
  ([bc91f77](https://github.com/Dicklesworthstone/frankenmermaid/commit/bc91f77a1806e85bd8bcf355f80dd7ed258cf51f))

### SVG arrowhead markers

- Proper SVG `<marker>` definitions for arrowheads; parallel edge diff fix
  ([27228a6](https://github.com/Dicklesworthstone/frankenmermaid/commit/27228a6bf35b63495c4edd58ee24528dfd2113fd))

### Structured tracing

- Pipeline decision tracing with structured spans throughout the layout engine
  ([307bfcf](https://github.com/Dicklesworthstone/frankenmermaid/commit/307bfcf18f6c2e21026c1d911355812ec73bd31a))

### Fuzz testing infrastructure

- cargo-fuzz harness for parser and full pipeline
  ([0154b56](https://github.com/Dicklesworthstone/frankenmermaid/commit/0154b56fb996eeb1c3af27e5910865f45159dd85))
- Parser and detect fuzz corpora with tracing dependency
  ([473d258](https://github.com/Dicklesworthstone/frankenmermaid/commit/473d258696e950a2e44a7712a12d6ec9e471088c))

### Test infrastructure expansion

- E2E pipeline tests for all 24 diagram types
  ([e46dc88](https://github.com/Dicklesworthstone/frankenmermaid/commit/e46dc88a8a42baea4c3d7ecc6c18b3aad6bce59d))
- Golden layout checksum infrastructure for determinism verification
  ([e8e298b](https://github.com/Dicklesworthstone/frankenmermaid/commit/e8e298bb87fb05206b54666c2257825c04c6d2ce))
- Property-based roundtrip invariant tests for parser
  ([76efddc](https://github.com/Dicklesworthstone/frankenmermaid/commit/76efddcba8e760af83b917322e1f3e017a2309dc))
- Adversarial input security hardening tests
  ([bf06fcd](https://github.com/Dicklesworthstone/frankenmermaid/commit/bf06fcd1071e550a0e8fb75cf52ed9622fc71ece))
- Performance baseline tests for all layout algorithms
  ([d2d614b](https://github.com/Dicklesworthstone/frankenmermaid/commit/d2d614b11aae7f5413148a8873e1f2edde6d1443))
- Layout dispatch capability parity and fallback tests
  ([223b3b1](https://github.com/Dicklesworthstone/frankenmermaid/commit/223b3b15f5f34cceaf05b4f1ad8d75ab14667feb))
- Graph IR operations unit tests
  ([c05eea3](https://github.com/Dicklesworthstone/frankenmermaid/commit/c05eea3d0e11bbcbe42561a4f8d55e2f91d81cef))

### Refactoring

- Simplified BK algorithm guard clauses
  ([ed6fa7e](https://github.com/Dicklesworthstone/frankenmermaid/commit/ed6fa7e3e52273564ad685373e2f7ee77456dd8d))
- Optimized parser lookups, fixed multi-line text, added edge markers
  ([00f9d43](https://github.com/Dicklesworthstone/frankenmermaid/commit/00f9d43d44807b8d0600c3c50dd79e5327ac62ca))

### Fixes

- Guard `force_temperature` against zero `max_iterations`
  ([c060121](https://github.com/Dicklesworthstone/frankenmermaid/commit/c06012183beb259189ef0f0934fb47fee4b74b64))
- Guard `f32`-to-`i32` cast in SVG attribute formatting
  ([16b99a8](https://github.com/Dicklesworthstone/frankenmermaid/commit/16b99a882effad3b11159275527b3311fb5d0a9f))
- Use `INFINITY`/`NEG_INFINITY` for bounding box initialization
  ([8339f3f](https://github.com/Dicklesworthstone/frankenmermaid/commit/8339f3f74d601bdd778da37c4b33fe8ec0c244d5))
- Fix cluster CSS test by adding member nodes
  ([2517af4](https://github.com/Dicklesworthstone/frankenmermaid/commit/2517af443d43e77a7550fe4ec1cf80ca475cbec4))

---

## 2026-03-18 — Sequence/class/state IR, observability pipeline, Brandes-Kopf fixes

### Diagram-specific IR and parsing

- **Sequence diagram**: comprehensive IR with lifeline, activation, loop/alt
  fragments, and participant ordering
  ([cd9d35f](https://github.com/Dicklesworthstone/frankenmermaid/commit/cd9d35f4aa80aaec2036d590ef9b66bd848670b9))
- **Class diagram**: IR types and member (field/method) parsing
  ([b6adce8](https://github.com/Dicklesworthstone/frankenmermaid/commit/b6adce862ac054a4a999a0123c3454bb7357d497))
- **State diagram**: composite states and pseudo-states (fork, join, choice)
  ([d3665eb](https://github.com/Dicklesworthstone/frankenmermaid/commit/d3665eb708d456acd7280aa17fe5ae4571519705))

### Observability and pressure reporting

- Observability infrastructure, pressure reporting, parser improvements, and
  layout optimizations
  ([9f1b1ea](https://github.com/Dicklesworthstone/frankenmermaid/commit/9f1b1ea2cfa7119dc760f12601c74717f32cd1df))
- Capability matrix automation, BLESS mode for golden test updates, and
  security hardening
  ([ba90204](https://github.com/Dicklesworthstone/frankenmermaid/commit/ba90204dadbe305dffdcf084c3949e6a79ec7917))
- Budget event tracing and precomputed layout rendering
  ([c61e209](https://github.com/Dicklesworthstone/frankenmermaid/commit/c61e20924f5792446bfcdc0cde575ac38dc0753e))

### Layout engine improvements

- Expanded layout algorithms, parser robustness, and SVG rendering
  ([2c9dc54](https://github.com/Dicklesworthstone/frankenmermaid/commit/2c9dc54f605f612fcb3fbb9ee8a6d8e4bc903905))
- Fixed 4 bugs in Brandes-Kopf coordinate assignment
  ([3c5a2ac](https://github.com/Dicklesworthstone/frankenmermaid/commit/3c5a2ac50ee594ab543f1a098b2d3e27e5e9b1d2))
- Fixed BK compaction double-shift and improved kanban indent detection
  ([745f203](https://github.com/Dicklesworthstone/frankenmermaid/commit/745f203bbf98e6de5d105de701d7843a115830cf))

### Documentation

- Major README expansion with comprehensive feature documentation (+1,008
  lines) and diagram type coverage documentation (+573 lines)
  ([562b248](https://github.com/Dicklesworthstone/frankenmermaid/commit/562b248e296b3150c8a95c5211877caab169f79e),
  [eb3eeda](https://github.com/Dicklesworthstone/frankenmermaid/commit/eb3eeda341b62ee4aabca297da17464f790e55c7),
  [3ff59a9](https://github.com/Dicklesworthstone/frankenmermaid/commit/3ff59a9ef05d1ceb22d0762cfbb1611156f44034))

### Testing

- Updated integration tests, golden SVGs, and observability evidence
  ([8f0aa85](https://github.com/Dicklesworthstone/frankenmermaid/commit/8f0aa85ca176f9a5a0df3a2386e987841e46d535))

### Fixes

- Compact tier test updated for layout dimension changes
  ([7364b8d](https://github.com/Dicklesworthstone/frankenmermaid/commit/7364b8d00f2938aadaad722c46fa2efcc836e941))

---

## 2026-03-17 — Major parser expansion and layout improvements

### Parser and layout

- Major parser expansion and layout improvements (+681 lines) covering
  additional diagram types, edge cases, and IR builder refinements
  ([23bc3fc](https://github.com/Dicklesworthstone/frankenmermaid/commit/23bc3fc2c8a37d828e0d5ad7e76aab553154152c))

---

## 2026-03-16 — SVG visual polish, GitHub Pages showcase, WASM production rebuild

### GitHub Pages showcase

- Standalone browser showcase with live WASM rendering
  ([e07b519](https://github.com/Dicklesworthstone/frankenmermaid/commit/e07b5194a22823a36017d8e04addb7a90fbd5fc9))
- GitHub Pages publishing workflow
  ([3e9d98c](https://github.com/Dicklesworthstone/frankenmermaid/commit/3e9d98c6770de9a8127df029e5ed4ee267d4def6))
- Expanded to 80 realistic gallery samples
  ([70f92f5](https://github.com/Dicklesworthstone/frankenmermaid/commit/70f92f580d2e68b8f69428840146ef4a0cc5b863))
- Major expansion with additional diagram examples (+534 lines)
  ([b7d00e1](https://github.com/Dicklesworthstone/frankenmermaid/commit/b7d00e152e2477f9bdcae52cc53629f32efd08db))
- Mermaid.js fallback, mobile layout, diagnostics collapse
  ([d52d71c](https://github.com/Dicklesworthstone/frankenmermaid/commit/d52d71cebaa63bfed997188f31d87d6778e3bf36))

### SVG rendering polish

- Refined SVG theme system with regenerated golden snapshots
  ([b171e18](https://github.com/Dicklesworthstone/frankenmermaid/commit/b171e184341d186f2b858d2eb37f49639596d357))
- Refined SVG rendering with regenerated golden snapshots
  ([5804927](https://github.com/Dicklesworthstone/frankenmermaid/commit/5804927ba6323e57ca3efdbcc98d8da1d75c6ec6))
- Refined SVG/terminal rendering and refreshed golden snapshots
  ([a62d78d](https://github.com/Dicklesworthstone/frankenmermaid/commit/a62d78d5cbe6890ccb750e783f7b20eb28f13e17),
  [01695fe](https://github.com/Dicklesworthstone/frankenmermaid/commit/01695fea3b3df66bc90b0791e9d222cadcd908b9))

### WASM production rebuild

- Larger nodes, refined arrows, rebuilt WASM for production use
  ([ca53913](https://github.com/Dicklesworthstone/frankenmermaid/commit/ca53913e16a2aba6101c701b815183db347b36dd))

---

## 2026-03-15 — Rendering pipeline expansion, terminal minimap, diagram type coverage

### Rendering pipeline

- Extended rendering pipeline and WASM API (+317 lines)
  ([d0bf676](https://github.com/Dicklesworthstone/frankenmermaid/commit/d0bf6766626244f9c43e74f1d000b161a966eeef))
- Refactored WASM bindings and improved SVG rendering (+153 lines)
  ([53c46a6](https://github.com/Dicklesworthstone/frankenmermaid/commit/53c46a6c31b642c931a6a924d10fc23cf74ad115))
- Extended WASM API and layout algorithms (+145 lines)
  ([8880e7e](https://github.com/Dicklesworthstone/frankenmermaid/commit/8880e7e8655aa640582aa6129361dbbec9d60609))

### Terminal rendering

- Terminal minimap and diff rendering (+818 lines)
  ([56daaaf](https://github.com/Dicklesworthstone/frankenmermaid/commit/56daaaf01520c545afa64f44def22305bb02dcf5))

### Layout engine

- Major layout engine expansion with edge routing and cluster placement (+424
  lines)
  ([e8d6816](https://github.com/Dicklesworthstone/frankenmermaid/commit/e8d68169d0e6d00a30ca2aa1d88fffbd97ad5dce))

### Diagram type coverage

- Expanded diagram type coverage and updated capability matrix
  ([990e164](https://github.com/Dicklesworthstone/frankenmermaid/commit/990e164aa82d77ba327300c7af0dbc97af4714b1))
- Broadened diagram parsing and expanded capability evidence (+494 lines)
  ([ebaf8d6](https://github.com/Dicklesworthstone/frankenmermaid/commit/ebaf8d6b003429d26144f80c0c27c66489ffb28d))
- Expanded parser module API and diagram type support (+415 lines)
  ([51a1396](https://github.com/Dicklesworthstone/frankenmermaid/commit/51a139603b8bdd9da307d5635ccd5fb0d63801ee))
- Extended mermaid parser with additional diagram support (+96 lines)
  ([b37949b](https://github.com/Dicklesworthstone/frankenmermaid/commit/b37949b12db095da7e61f525a1ef407b6027dbb0))
- Expanded mermaid parser with additional diagram type handling (+121 lines)
  ([6b63e36](https://github.com/Dicklesworthstone/frankenmermaid/commit/6b63e36c0485230209dd6d526fe9a94fc3904446))

### Fixes

- Refined mermaid parser edge case handling
  ([402851f](https://github.com/Dicklesworthstone/frankenmermaid/commit/402851f0c05788d1d25ffbaf8fd8560d99053408))

---

## 2026-03-14 — Block-beta and gitGraph refinement, capability matrix, parser architecture

### Block-beta diagram support

- Two-phase block-beta parsing and centralized support metadata
  ([d349a49](https://github.com/Dicklesworthstone/frankenmermaid/commit/d349a49f3d236a10c47554c9d640f5efcf662ae9))
- Validated zero-span in block-beta groups and blocks
  ([ab69a17](https://github.com/Dicklesworthstone/frankenmermaid/commit/ab69a17085e9af940ccbe76d69d61a082febfd0e))

### gitGraph parser architecture

- Two-phase parse/lower architecture for gitGraph command parsing
  ([5f7bf74](https://github.com/Dicklesworthstone/frankenmermaid/commit/5f7bf74b8d9c9e750feda3d009ca79e280e318ec))
- Improved gitGraph command parsing robustness
  ([b309c8d](https://github.com/Dicklesworthstone/frankenmermaid/commit/b309c8d28683847ae2bc8114e84b1e78c4129d3f))

### Capability matrix

- Comprehensive diagram capability matrix and detection evidence in CLI
  ([6e6f22f](https://github.com/Dicklesworthstone/frankenmermaid/commit/6e6f22f64b3b2de08dac8bf066d801897c39f973))

### Parser and layout expansion

- Expanded mermaid parser with improved diagram support
  ([b789d5d](https://github.com/Dicklesworthstone/frankenmermaid/commit/b789d5d707e118af76b6c9b230102f39348d51b8),
  [4148577](https://github.com/Dicklesworthstone/frankenmermaid/commit/41485774a33450272fdbea6905c84d84cff598f8))
- Expanded parser coverage and CLI improvements with integration tests
  ([72c5c25](https://github.com/Dicklesworthstone/frankenmermaid/commit/72c5c258304a4b559d874de147d3c2df89d071ad))
- Extended layout algorithm and CLI integration tests
  ([f8c33da](https://github.com/Dicklesworthstone/frankenmermaid/commit/f8c33da0c8667528e567bcac6b9dca34ece74f48))
- Expanded layout engine with advanced placement strategies
  ([dccff1d](https://github.com/Dicklesworthstone/frankenmermaid/commit/dccff1d96125b9f3a5fbe3995910aab16b596d31))

### WASM

- Expanded WASM bindings and updated capability matrix with README refresh
  ([bb6013a](https://github.com/Dicklesworthstone/frankenmermaid/commit/bb6013a26df601c5f2285ac5f28721a447c906f2))

---

## 2026-03-13 — Block-beta grid layout, grid_span, diagram engine expansion

### Block-beta layout

- **Grouped block-beta grid placement** with subgraph-aware layout
  ([866b339](https://github.com/Dicklesworthstone/frankenmermaid/commit/866b3399f1e3d134e0fce55d384beb58ba2a237b))
- `grid_span` support for block-beta clusters and subgraphs
  ([7cfde1c](https://github.com/Dicklesworthstone/frankenmermaid/commit/7cfde1c76a07bb48fd87b8fc905c50cddd43a9a6))
- Promoted block-beta to basic support and added `block` alias
  ([45c2d7f](https://github.com/Dicklesworthstone/frankenmermaid/commit/45c2d7f9f64f634b4aa25ad974fd3daeac1caf36))

### Layout and rendering

- Expanded diagram layout engine and rendering support
  ([fe832f3](https://github.com/Dicklesworthstone/frankenmermaid/commit/fe832f30823ac88c3538147220fb6a29f50b45f0))

---

## 2026-03-12 — Graph-level IR, subgraph hierarchy, block-beta parsing, flowchart AST

### Graph-level IR

- **Graph-level IR** with subgraphs, typed nodes, and typed edges
  ([c65a835](https://github.com/Dicklesworthstone/frankenmermaid/commit/c65a8353bef0fd206004ccad0005392e7aa54e4a))
- Traversal helpers for subgraph hierarchy and node membership
  ([4570612](https://github.com/Dicklesworthstone/frankenmermaid/commit/45706123ee777b7098eb19608c3c1b5bebdc398c))
- Endpoint resolution, graph adjacency helpers, and `leaf_subgraphs` query
  ([26f0081](https://github.com/Dicklesworthstone/frankenmermaid/commit/26f0081083710c3fad24fddeead723febdea0c37))

### Flowchart parser architecture

- Document-level AST for flowchart parsing
  ([7a051b5](https://github.com/Dicklesworthstone/frankenmermaid/commit/7a051b56a31c8762aeb9103a62a997eea0d39992))
- Flowchart header direction propagation to IR builder
  ([991cf8f](https://github.com/Dicklesworthstone/frankenmermaid/commit/991cf8f6091620dbe2d69cdb89cc804157947289))

### New diagram type: block-beta

- Block-beta diagram parsing support
  ([a3c913e](https://github.com/Dicklesworthstone/frankenmermaid/commit/a3c913e9c2172c5b4aa700acd0f5d547d99645ca))

### Renderer improvements

- Accept pre-computed `DiagramLayout` in SVG, canvas, and WASM renderers
  ([e1e913b](https://github.com/Dicklesworthstone/frankenmermaid/commit/e1e913bc24f64c8482abec2a42e4acf62b99dfa4))

### Support level promotions

- Promoted gitGraph support level to basic
  ([e93f411](https://github.com/Dicklesworthstone/frankenmermaid/commit/e93f411a398d57167d016625e652882cb0b7f8c9))

### Fixes

- Allow duplicate subgraph and cluster keys instead of merging
  ([177f3e8](https://github.com/Dicklesworthstone/frankenmermaid/commit/177f3e8fefc169065ac1edbe47e4ff174c29c11d))
- Ignore nested flowchart headers inside subgraphs
  ([9faaef7](https://github.com/Dicklesworthstone/frankenmermaid/commit/9faaef732db72ef5a67350e08476de18f3f12f06))

---

## 2026-02-27 — Tree and radial layout, adaptive SVG detail tiers, render scene IR, diagnostics

### New layout algorithms: tree and radial

- **Tree layout** (Reingold-Tilford) and **radial layout** with bounds
  computation fix
  ([71505a8](https://github.com/Dicklesworthstone/frankenmermaid/commit/71505a8babd9ed06e6ed5f57691b40df991db302))
- Major layout engine expansion with force-directed improvements and new
  algorithms
  ([69dceec](https://github.com/Dicklesworthstone/frankenmermaid/commit/69dceec4039186f66e30182001e4887452c29c68))

### SVG rendering

- **Adaptive detail tiers** (compact, normal, rich), print-optimized CSS, and
  label truncation
  ([0675004](https://github.com/Dicklesworthstone/frankenmermaid/commit/06750042a767e8c0cdd2bfdcd01001d51c25fd65))
- Major SVG rendering expansion with `<defs>` module and golden tests
  ([843468f](https://github.com/Dicklesworthstone/frankenmermaid/commit/843468f37da7ff45be78c10e819c16ecce060988))

### Render scene IR

- Target-agnostic render scene IR and backend implementations
  ([a7141c8](https://github.com/Dicklesworthstone/frankenmermaid/commit/a7141c8711928c4ff7ff34a4b68b32aad5ebcb20))

### Parser configuration

- **YAML front-matter config** support, unified `%%{init}` directive handling,
  and DOT comment stripping fix
  ([e8b6997](https://github.com/Dicklesworthstone/frankenmermaid/commit/e8b6997c6d0a5a5f48e4a7637b48137382e324a3))
- Mermaid.js config adapter, structured diagnostics, and init config extensions
  ([07532f4](https://github.com/Dicklesworthstone/frankenmermaid/commit/07532f4f9374ea07eb2f7add10b869ce6d88658c))

### CLI: structured validate command

- Overhauled `validate` command with structured diagnostics pipeline
  ([d25408d](https://github.com/Dicklesworthstone/frankenmermaid/commit/d25408ddfa456174ff447b0da737475835cc4138))

### Fixes

- Fixed off-by-one in terminal diagram block boundary detection
  ([c6b8537](https://github.com/Dicklesworthstone/frankenmermaid/commit/c6b8537bdf12c3f0743f31fcb9361e9becacb341))
- Improved DOT edge attribute parsing and fixed SVG detail tier selection
  ([0f8ec9a](https://github.com/Dicklesworthstone/frankenmermaid/commit/0f8ec9a2473c0b13c92a1039798a6295a64a3a44))
- Corrected force-directed physics and Tarjan SCC, added proptest coverage
  across all crates
  ([007ebb5](https://github.com/Dicklesworthstone/frankenmermaid/commit/007ebb54e001bdce5820f6e6a7743be14fae49b9))

---

## 2026-02-26 — Subgraph/cluster parsing, visual design overhaul, security hardening

### Subgraph and cluster support

- **Subgraph/cluster parsing** and compact disconnected component layout
  ([55d08b7](https://github.com/Dicklesworthstone/frankenmermaid/commit/55d08b7d3fb9036bd62a2b730cc740484f835b83))
- Hardened subgraph parsing; prevented isolated nodes from exploding layout
  width
  ([3a988c8](https://github.com/Dicklesworthstone/frankenmermaid/commit/3a988c8f240292724963a12ee94a33cb2824d494))

### Visual design overhaul

- Overhauled visual design to modern aesthetic, added hyperlink support and
  font-aware node sizing
  ([4f08f5f](https://github.com/Dicklesworthstone/frankenmermaid/commit/4f08f5f1ab1a1f69e7a428752b41a6ffff9d6290))

### Security and robustness

- Hardened parsers against edge cases, added **SVG XSS prevention**, and fixed
  terminal renderer underflows
  ([03c6d23](https://github.com/Dicklesworthstone/frankenmermaid/commit/03c6d23d5e089fd328add7d3b9ea4e7582156267))
- Replaced `unwrap()` in `fuzzy_keyword_match` with safe pattern match
  ([1420f51](https://github.com/Dicklesworthstone/frankenmermaid/commit/1420f51d1cb3b3a9221d29b3b8960c4adfff2158))
- Preserved valid edge prefix when chain has malformed trailing segment
  ([2cc5a67](https://github.com/Dicklesworthstone/frankenmermaid/commit/2cc5a67e25a090285b5f46427add45631297722e))

---

## 2026-02-21 — Force-directed layout, cycle handling, crossing refinement, edge routing, node shapes

### Force-directed layout

- **Fruchterman-Reingold force-directed layout** algorithm
  ([a982da5](https://github.com/Dicklesworthstone/frankenmermaid/commit/a982da56e85bdf5a8d3ec37fb300597a1f4c7d00))

### Sugiyama cycle handling

- Complete cycle handling: **SCC collapse**, quality metrics, and comprehensive
  tests
  ([8148819](https://github.com/Dicklesworthstone/frankenmermaid/commit/81488199e4ff46cb25f1a4d338db1d7e674b3f51))

### Crossing minimization refinement

- **Transpose and sifting** heuristics added to Sugiyama crossing minimization
  pipeline
  ([fb8dd86](https://github.com/Dicklesworthstone/frankenmermaid/commit/fb8dd86d5f636a092dac3b0c211ceac03af23664),
  [fb2aef5](https://github.com/Dicklesworthstone/frankenmermaid/commit/fb2aef5efada742d3d5092ccf73836e54b6883b6))

### Edge routing

- **Self-loop routing**, parallel edge offsets, and `EdgeRouting` enum
  ([1257eae](https://github.com/Dicklesworthstone/frankenmermaid/commit/1257eae2e8556c39ce3f3e44270b418b975874a5))

### Node shapes

- **Parallelogram and inverse parallelogram** node shapes with Mermaid syntax
  and full renderer support
  ([f50afca](https://github.com/Dicklesworthstone/frankenmermaid/commit/f50afca3645451ee5bd73f60fde73f23a20077ce))

### Licensing

- Updated license to MIT with OpenAI/Anthropic Rider
  ([ecf2b2d](https://github.com/Dicklesworthstone/frankenmermaid/commit/ecf2b2db0811dce42085d2fed6582893dff14175))

---

## 2026-02-20 — Multi-line labels, theme overrides, Gantt fixes, Mermaid parser expansion

### Multi-line labels

- **Multi-line label rendering** in SVG and terminal; improved text measurement
  in WASM; DOT parser robustness fixes
  ([02f5081](https://github.com/Dicklesworthstone/frankenmermaid/commit/02f5081ed8642d622fd8c9542a4bcc2d948aa731))

### Layout and rendering

- Fixed layout coordinate assignment for reversed ranks, added Mermaid node
  shapes, and supported theme overrides in SVG
  ([95d679c](https://github.com/Dicklesworthstone/frankenmermaid/commit/95d679c9f05b3ad9828d0a7c88d6561c317853dd))
- Expanded Mermaid parser coverage and upgraded dependencies
  ([3f8c6d7](https://github.com/Dicklesworthstone/frankenmermaid/commit/3f8c6d7908e28b1c303d077bfc056137c4b18606))

### Fixes

- Fixed Gantt task ID collisions, improved edge label positioning, added
  multi-line support
  ([a5a4a03](https://github.com/Dicklesworthstone/frankenmermaid/commit/a5a4a035a54ae85b2dc3098a44c7010f9b103fe1))

---

## 2026-02-13 — Mindmap shape parsing, timeline rewrite

### Parser: mindmap and timeline

- Enhanced **mindmap shape parsing** and rewrote timeline as period-event model
  ([73a9e45](https://github.com/Dicklesworthstone/frankenmermaid/commit/73a9e45c89c2137e7b1d8d94e77736ceffc3c2a3))

---

## 2026-02-12 — Initial feature build: workspace, parsers, layout, all three renderers, WASM, CLI

This date represents the initial burst of development that stood up the
complete pipeline from parse through render across all backends.

### Workspace architecture

- **Scaffolded 8-crate Rust workspace** (`fm-core`, `fm-parser`, `fm-layout`,
  `fm-render-svg`, `fm-render-term`, `fm-render-canvas`, `fm-wasm`, `fm-cli`)
  ([328e84f](https://github.com/Dicklesworthstone/frankenmermaid/commit/328e84fef3fb7755ef585009218cc75235dbc23c))

### Parser

- **Modularized fm-parser** into `dot_parser`, `ir_builder`, `mermaid_parser`
  ([5d84e76](https://github.com/Dicklesworthstone/frankenmermaid/commit/5d84e767c2caca83baaba09d405362da48c45bd3))
- Expanded DOT/Mermaid parsers with subgraph, attribute, and diagram type
  support
  ([4833fd4](https://github.com/Dicklesworthstone/frankenmermaid/commit/4833fd4dd71c2d8a38d4426d5845f6e906f82c34))
- Comprehensive **25-type diagram detection** and rendering enhancements
  ([7837e1f](https://github.com/Dicklesworthstone/frankenmermaid/commit/7837e1f27e33d8ebf440acdc763f83a6b0289ae7))
- Enhanced Mermaid parser capabilities
  ([83fb575](https://github.com/Dicklesworthstone/frankenmermaid/commit/83fb575163c843802be4962af10ebba50f5b14d9))
- Comprehensive diagram type parsers and expanded SVG layout engine
  ([2f5b869](https://github.com/Dicklesworthstone/frankenmermaid/commit/2f5b869420c43779d222e0e4702f3d99aabd3e97))

### Core IR

- **ER diagram** entity attribute support
  ([3caf7f8](https://github.com/Dicklesworthstone/frankenmermaid/commit/3caf7f8a2f1d919d4856e1f2795359f21af99238))
- Font metrics, canvas renderer/shapes, SVG accessibility, and theming modules
  ([dcde402](https://github.com/Dicklesworthstone/frankenmermaid/commit/dcde402909155eccc5cb829d4d62c4724e36556e))

### Layout engine

- **Sugiyama layout** with proper cycle removal and crossing minimization
  ([303def5](https://github.com/Dicklesworthstone/frankenmermaid/commit/303def539030c62edfe6e1d51933f2b52150f1eb))
- Fixed rank coordinate assignment, added extended shapes, improved parser
  routing
  ([ed0c64b](https://github.com/Dicklesworthstone/frankenmermaid/commit/ed0c64b4bb393b98e67a39765a7c4a035ab31008))

### SVG renderer

- **Complete SVG generation core** with node rendering, edge paths, and
  viewBox calculation
  ([5feb20b](https://github.com/Dicklesworthstone/frankenmermaid/commit/5feb20bb52ab97d6474cff7b0bae7e29491ecbc0))
- Theming, accessibility (ARIA labels), and diamond arrowhead support
  ([94141fb](https://github.com/Dicklesworthstone/frankenmermaid/commit/94141fb874aa9e6d7e40af5d9ddb88b2cb0b8f54))

### Terminal renderer

- `TermRenderConfig` for terminal rendering options
  ([f381faf](https://github.com/Dicklesworthstone/frankenmermaid/commit/f381faf01ff32fe740ce9bb3436c2a96cb5636ef))
- Canvas and glyph modules for terminal rendering
  ([743150a](https://github.com/Dicklesworthstone/frankenmermaid/commit/743150a226051d2130ba1a668c41b6425608af9d))
- Core terminal diagram renderer
  ([3fdbebc](https://github.com/Dicklesworthstone/frankenmermaid/commit/3fdbebcf5109076b081129fadc85178bb85f7fea))
- Diagram diff and minimap modules
  ([fcaf9b3](https://github.com/Dicklesworthstone/frankenmermaid/commit/fcaf9b31b21ee0e1b54897d100ec0915f2d5174e))
- **ASCII art renderer** for text-only terminal output
  ([e22f404](https://github.com/Dicklesworthstone/frankenmermaid/commit/e22f40436a87ef3da73459e5bc5d9631b0d20352))
- Expanded ASCII renderer and integrated terminal rendering modules
  ([f4a4c44](https://github.com/Dicklesworthstone/frankenmermaid/commit/f4a4c44e71c84a321bb78ca63bb17ad8bbe5af78))
- Polished ASCII renderer and minimap visualization
  ([ba7502d](https://github.com/Dicklesworthstone/frankenmermaid/commit/ba7502d1f946f8f595d387ad0cb5d08fbbfc76e6))

### WASM bindings

- Complete WASM bindings with runtime config
  ([788f81a](https://github.com/Dicklesworthstone/frankenmermaid/commit/788f81ae745311ba6560004f4941ddbfc4b6f37c))
- `Serialize` derive to `ParseResult`, restructured WASM crate
  ([463316b](https://github.com/Dicklesworthstone/frankenmermaid/commit/463316b0c42db2042a646b3092669800378e8a5e))
- Simplified `RuntimeConfig` with derive `Default`
  ([593046e](https://github.com/Dicklesworthstone/frankenmermaid/commit/593046e80eb3cae5795f893a14f0e730930794bd))

### CLI

- **Comprehensive CLI rewrite** with full command suite (`render`, `parse`,
  `detect`, `validate`, `diff`, `capabilities`)
  ([b83e409](https://github.com/Dicklesworthstone/frankenmermaid/commit/b83e4091daf7375edca0f449c7dcbc07a4c4de9d))
- Integration test suite and dependency updates
  ([274d89c](https://github.com/Dicklesworthstone/frankenmermaid/commit/274d89ca705a6c80c3ea9043e8228d69d076c8da))

### Fixes

- Corrected right-border alignment for Unicode content in terminal renderer
  ([808f6e7](https://github.com/Dicklesworthstone/frankenmermaid/commit/808f6e776f04392bebfc089baae5ca23b4967071))

---

## 2026-02-11 — Project inception

### Foundation

- Initial commit with AGENTS.md
  ([a487793](https://github.com/Dicklesworthstone/frankenmermaid/commit/a4877939105eb405add227d48e0f5f5d054fcfec))
- Project foundation: README, `.gitignore`, illustration assets, and legacy
  reference code
  ([d6e1921](https://github.com/Dicklesworthstone/frankenmermaid/commit/d6e1921b47f26067cff1c5d808b1c2cc4ba7f826))
- Comprehensive bead set for project planning
  ([f3b28a0](https://github.com/Dicklesworthstone/frankenmermaid/commit/f3b28a0bec843e35a319ba1e0f5e56e78a91b408))
