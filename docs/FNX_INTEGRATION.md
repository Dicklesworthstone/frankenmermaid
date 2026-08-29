# franken_networkx (fnx) Integration Architecture

> Authoritative technical contract for integrating franken_networkx into frankenmermaid.

This document defines:
- Phase boundaries and scope
- Success metrics and acceptance criteria
- Risk register and mitigation strategies
- Go/no-go gates for each phase
- Implementation map across crates

---

## 0. Executive Summary

frankenmermaid already has deterministic parsing/layout/rendering. fnx provides graph intelligence (centrality, cycles, connectivity) that can inform layout heuristics. This integration is **advisory only** — fnx analysis provides hints, not authoritative layout decisions.

**Key constraint**: fnx APIs are currently undirected. Directed graph support (critical for flow diagrams) is in development. This contract defines how to safely adopt undirected intelligence now while preparing for directed capabilities later.

---

## 1. Dependency Strategy

### Chosen: Git Dependency with Pinned Revision

```toml
fnx-runtime = { git = "https://github.com/Dicklesworthstone/franken_networkx.git", rev = "cb8bdb59...", default-features = false }
fnx-classes = { git = "https://github.com/Dicklesworthstone/franken_networkx.git", rev = "cb8bdb59...", default-features = false }
fnx-algorithms = { git = "https://github.com/Dicklesworthstone/franken_networkx.git", rev = "cb8bdb59...", default-features = false }
fnx-views = { git = "https://github.com/Dicklesworthstone/franken_networkx.git", rev = "cb8bdb59...", default-features = false }
```

### Rationale

| Alternative | Pros | Cons | Verdict |
|-------------|------|------|---------|
| **Git + pinned rev** | Reproducible builds; fast iteration; no publish overhead | CI clones repo each build; rev must be bumped manually | **Selected** |
| Workspace path | Zero network; instant iteration | Only works locally; breaks CI without conditional config | Rejected for CI |
| Published crates.io | Versioned releases; standard ecosystem | Requires publish cadence; premature for alpha | Deferred to 1.0 |

**Key decision**: Use git with pinned `rev` because:
1. franken_networkx is in active development alongside frankenmermaid
2. Both repos share the same maintainer, so coordinated updates are easy
3. Pinned revision guarantees reproducible builds
4. CI can cache the git fetch; incremental builds are fast

### Upgrade Workflow

```bash
# 1. Identify desired fnx commit
cd /data/projects/franken_networkx
git log --oneline -5

# 2. Update rev in frankenmermaid/Cargo.toml
# 3. cargo update -p fnx-runtime -p fnx-classes -p fnx-algorithms -p fnx-views
# 4. cargo check --workspace --features fnx-integration
# 5. cargo test --workspace --features fnx-integration
```

---

## 2. Feature Flag Topology

### Workspace Root (`Cargo.toml`)

Defines workspace-level fnx dependencies (all optional, git-pinned):
```toml
[workspace.dependencies]
fnx-runtime = { git = "...", rev = "...", default-features = false }
fnx-classes = { git = "...", rev = "...", default-features = false }
fnx-algorithms = { git = "...", rev = "...", default-features = false }
fnx-views = { git = "...", rev = "...", default-features = false }
```

### fm-layout (Core Integration Point)

```toml
[features]
default = []
fnx-integration = [
    "dep:fnx-runtime",
    "dep:fnx-classes",
    "dep:fnx-algorithms",
    "dep:fnx-views",
]
fnx-experimental-directed = ["fnx-integration"]

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
fnx-runtime = { workspace = true, optional = true }
fnx-classes = { workspace = true, optional = true }
fnx-algorithms = { workspace = true, optional = true }
fnx-views = { workspace = true, optional = true }
```

**Design notes:**
- `fnx-integration` enables Phase 1: undirected structural intelligence
- `fnx-experimental-directed` gates Phase 2: directed algorithms (experimental canary)
- fnx deps are `cfg(not(wasm32))` because fnx uses std features unavailable in WASM

### fm-cli / fm-wasm (Surface Crates)

Forward flags to fm-layout:
```toml
[features]
fnx-integration = ["fm-layout/fnx-integration"]
fnx-experimental-directed = ["fm-layout/fnx-experimental-directed"]
```

### Flag Propagation Diagram

```
fm-cli ─┬─> fnx-integration ─────────> fm-layout/fnx-integration
        └─> fnx-experimental-directed ─> fm-layout/fnx-experimental-directed

fm-wasm ─┬─> fnx-integration ─────────> fm-layout/fnx-integration
         └─> fnx-experimental-directed ─> fm-layout/fnx-experimental-directed

fm-layout:
  fnx-integration enables: fnx-runtime, fnx-classes, fnx-algorithms, fnx-views
  fnx-experimental-directed implies: fnx-integration
```

---

## 3. CI Matrix Configuration

CI tests both fnx-on and fnx-off builds (`.github/workflows/ci.yml`):

```yaml
jobs:
  core-check:
    name: Core Check (${{ matrix.fnx_mode }})
    strategy:
      fail-fast: false
      matrix:
        fnx_mode: [off, on]
    steps:
      - name: Clippy (fnx off)
        if: matrix.fnx_mode == 'off'
        run: cargo clippy --workspace --all-targets -- -D warnings

      - name: Clippy (fnx on)
        if: matrix.fnx_mode == 'on'
        run: cargo clippy --workspace --all-targets --features fnx-integration -- -D warnings

      - name: Test (fnx off)
        if: matrix.fnx_mode == 'off'
        run: cargo test --workspace --all-targets

      - name: Test (fnx on)
        if: matrix.fnx_mode == 'on'
        run: cargo test --workspace --all-targets --features fnx-integration
```

### Build Matrix Summary

| Target | fnx-off | fnx-integration | fnx-experimental-directed |
|--------|---------|-----------------|---------------------------|
| Native (x86_64) | ✓ Tested | ✓ Tested | ✓ (via fnx-integration) |
| WASM (wasm32) | ✓ Tested | N/A (deps gated) | N/A |

---

## 4. Building Without fnx

When fnx is unavailable (fnx-off mode):
- All existing layout algorithms work unchanged
- No fnx graph analysis or witness artifacts
- WASM builds always use fnx-off (deps are `cfg(not(wasm32))`)

```bash
# Default build (fnx-off)
cargo build --workspace

# Explicit fnx-off
cargo build --workspace --no-default-features
```

---

## 5. Building With fnx

```bash
# Enable fnx integration
cargo build --workspace --features fnx-integration

# Enable experimental directed algorithms (future)
cargo build --workspace --features fnx-experimental-directed
```

---

## 6. Determinism Contract

fnx integration must preserve frankenmermaid's determinism guarantees:

1. **Identical input → identical output**: fnx analysis may inform layout decisions, but the same IR + config must produce byte-identical SVG
2. **Fallback on fnx failure**: If fnx analysis fails or times out, layout proceeds with fallback heuristics and emits a diagnostic
3. **Witness artifacts**: When fnx is enabled, analysis witnesses (graph metrics, cycle detection results) are logged for audit

---

## 7. Rollback / Kill-Switch

The feature flag design provides immediate rollback:

```bash
# Disable fnx at build time
cargo build --workspace  # default is fnx-off

# Or at CI level: remove fnx-on from matrix
matrix:
  fnx_mode: [off]  # temporary fnx disable
```

No code changes required to disable fnx; it's purely a Cargo feature.

---

## 8. Future: Published Crates

When franken_networkx reaches 1.0, update to crates.io dependencies:

```toml
# Future (not yet)
fnx-runtime = { version = "1.0", optional = true, default-features = false }
```

This requires:
1. fnx crates published to crates.io
2. Stable API surface
3. Semver guarantees

---

## 9. Phase Definitions and Boundaries

### Phase 1: Undirected Structural Intelligence (Current Focus)

**Scope**: Use fnx undirected graph algorithms to provide advisory hints to layout:
- Cycle detection and reporting
- Connectivity analysis (components, bridges, articulation points)
- Centrality metrics (betweenness, PageRank projections)
- Structural complexity metrics for layout algorithm selection

**Boundaries**:
- fnx output is **advisory only** — layout makes final decisions
- Directed graph analysis (SCC, WCC, reachability, directed cycles) ships separately behind Phase 2
- Fallback to existing heuristics if fnx analysis fails or times out

**Feature flag**: `fnx-integration`

### Phase 2: Directed Graph Intelligence (Experimental Canary)

**Scope**: Directed graph algorithms, implemented natively in `fm-layout/src/fnx_directed.rs`
(no dependency on upstream fnx directed APIs):
- DAG detection and topological ordering
- Critical path analysis
- Directed centrality (in-degree, out-degree influence)
- Layered layout optimization using directed structure

- **Boundaries**: advisory only, canary rollout — enabled only by the
  `fnx-experimental-directed` feature flag
- Directed conformance tests must keep passing before any wider enablement

**Go/no-go** for widening beyond canary:
1. Directed conformance tests stay green
2. Performance regression budget met (<10% latency increase)

---

## 10. Success Metrics

### Quality Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Layout quality (crossing count) | ≤ current baseline | Golden snapshot tests |
| Diagnostic accuracy | 100% correct cycle detection | Property tests |
| Edge routing quality | No regression | Visual inspection + automated checks |

### Performance Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| fnx analysis latency | < 50ms for graphs < 100 nodes | Benchmark harness |
| Layout latency overhead | < 10% vs fnx-off | A/B comparison tests |
| Memory overhead | < 2x working set | Memory profiling |

### Determinism Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Output stability | Byte-identical across runs | Repeated execution tests |
| Witness hash stability | Identical for identical inputs | Hash comparison |
| Cross-platform parity | Same output on Linux/macOS/Windows | CI matrix |

### Diagnostics Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Cycle detection recall | 100% (no missed cycles) | Synthetic test suite |
| False positive rate | < 1% | Manual review of diagnostics |
| Diagnostic actionability | User can resolve issue from message | UX review |

---

## 11. Risk Register

### R1: Algorithm Mismatch

**Risk**: fnx undirected algorithms may produce different results than frankenmermaid's existing directed-aware heuristics.

**Impact**: High — could cause layout quality regression.

**Mitigation**:
- fnx output is advisory only; layout engine makes final decisions
- A/B testing before each feature enablement
- Golden snapshot regression tests

**Residual risk**: Low after mitigations.

### R2: Runtime Overhead

**Risk**: fnx analysis adds latency to the layout pipeline.

**Impact**: Medium — could affect perceived responsiveness.

**Mitigation**:
- Timeout budget (50ms default) with fallback
- Lazy evaluation: only run fnx analysis when feature is enabled
- Cache fnx results for repeated layouts of same graph

**Residual risk**: Low with timeout enforcement.

### R3: Maintenance Coupling

**Risk**: Changes in fnx upstream break frankenmermaid builds.

**Impact**: Medium — could block CI.

**Mitigation**:
- Pinned git revision (not floating branch)
- fnx-off mode always works (core functionality independent)
- Explicit upgrade workflow with validation

**Residual risk**: Low with pinned revisions.

### R4: Directed/Undirected Confusion

**Risk**: Applying undirected analysis to inherently directed diagrams (flowcharts) produces misleading results.

**Impact**: Medium — could confuse users or produce poor layouts.

**Mitigation**:
- Clear documentation that Phase 1 is undirected only
- Diagnostics warn when undirected projection loses information
- Phase 2 deferred until directed APIs available

**Residual risk**: Medium until Phase 2.

---

## 12. Go/No-Go Gates

### Gate 1: Phase 1 Enablement

**Criteria**:
- [ ] All golden tests pass with fnx-on
- [ ] Performance regression < 10%
- [ ] No new clippy warnings
- [ ] Documentation complete
- [ ] Fallback behavior validated

**Decision**: Product owner reviews test evidence and approves enablement.

### Gate 2: Phase 2 Enablement

**Criteria**:
- [ ] fnx directed APIs stable and documented
- [ ] Directed conformance tests pass
- [ ] Directed parity tests pass (compare to existing layout)
- [ ] Performance budget met
- [ ] Integration tests for directed diagrams pass

**Decision**: Requires explicit approval after Phase 1 stabilizes.

---

## 13. Implementation Map

### fm-core (IR Types)

- Add fnx witness fields to `MermaidDiagramIr` (optional, populated when fnx enabled)
- Add fnx-related diagnostic categories
- No fnx dependency (just type definitions)

### fm-parser (Parsing)

- No fnx dependency
- Parser remains fnx-agnostic; graph construction happens in layout

### fm-layout (Core Integration Point)

- Primary fnx integration location
- Conditional compilation: `#[cfg(feature = "fnx-integration")]`
- Adapter layer: `MermaidDiagramIr` → fnx graph
- Analysis dispatcher: run fnx algorithms, collect witnesses
- Fallback logic: timeout handling, error recovery
- Witness logging: structured output for audit

### fm-render-svg / fm-render-term / fm-render-canvas

- No fnx dependency
- Consume layout output (may include fnx-informed positions)
- Renderers remain fnx-agnostic

### fm-cli

- Forward fnx feature flags to fm-layout
- Add `--fnx-mode` flag for runtime control (future)
- Include fnx diagnostics in verbose output

### fm-wasm

- fnx disabled on WASM target (deps are `cfg(not(wasm32))`)
- Forward feature flags for native builds

---

## 14. Evidence Requirements

All fnx integration work must produce:

1. **Structured logs** with fields:
   - `scenario_id`, `input_hash`, `fnx_mode`, `projection_mode`
   - `parse_ms`, `analysis_ms`, `layout_ms`, `render_ms`
   - `diagnostic_count`, `fallback_reason`, `witness_hash`, `output_hash`

2. **Witness artifacts** (JSON):
   - Graph structure (nodes, edges, directed/undirected)
   - Cycle detection results
   - Centrality scores
   - Connectivity analysis

3. **Reproducibility assertions**:
   - Same input → same witness → same output
   - Verified by repeated execution (5 runs minimum)

---

## References

- Bead: [bd-ml2r.1.1] Decide dependency model and feature-flag topology
- Bead: [bd-ml2r.1.2] Deterministic decision contract and fallback semantics
- Parent: [bd-ml2r.1] Integration architecture contract
- Epic: [bd-ml2r] Graph Intelligence Integration via franken_networkx
