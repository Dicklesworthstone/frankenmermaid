# franken_networkx (fnx) Integration Architecture

> Decision record for dependency model and feature-flag topology.

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
- `fnx-experimental-directed` gates Phase 2: directed algorithms (future)
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

## References

- Bead: [bd-ml2r.1.1] Decide dependency model and feature-flag topology
- Parent: [bd-ml2r.1] Integration architecture contract
- Epic: [bd-ml2r] Graph Intelligence Integration via franken_networkx
