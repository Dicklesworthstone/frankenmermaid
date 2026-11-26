# FNX Phase-2 Directed Rollout Policy (bd-ml2r.8.2)

This document defines the rollout strategy, gates, and rollback triggers for
Phase-2 directed graph intelligence capabilities.

---

## Overview

Phase-2 extends FNX integration from undirected structural intelligence (Phase-1)
to directed graph semantics:

- **Strongly Connected Components (SCC)** via Tarjan's algorithm
- **Weakly Connected Components (WCC)** via BFS
- **Directed Cycle Detection** via DFS with back-edge tracking
- **Reachability Analysis** with source/sink identification

All algorithms are implemented natively in `fm-layout/src/fnx_directed.rs` to
avoid upstream dependency on fnx directed APIs while maintaining determinism.

---

## Enablement Gates

### Gate 1: Determinism Verification

All directed algorithms must produce identical results across repeated runs.

| Algorithm | Test | Threshold |
|-----------|------|-----------|
| SCC | `directed_scc_determinism_*` | 100% identical across 10 runs |
| WCC | `directed_wcc_determinism_*` | 100% identical across 10 runs |
| Cycles | `directed_cycle_detection_determinism_*` | 100% identical across 10 runs |
| Reachability | `directed_reachability_determinism` | 100% identical across 10 runs |

### Gate 2: Quality Correctness

Algorithms must produce mathematically correct results on known graph structures.

| Algorithm | Test | Requirement |
|-----------|------|-------------|
| SCC | `quality_scc_correctness_*` | Correct component count and membership |
| WCC | `quality_wcc_correctness_*` | Correct connectivity determination |
| Cycles | `quality_cycle_detection_*` | Correct cycle presence detection |
| Reachability | `quality_reachability_*` | Correct source/sink identification |

### Gate 3: Performance Budget

Each algorithm must complete within 100ms for graphs up to 100 nodes.

| Algorithm | Test | Budget |
|-----------|------|--------|
| SCC | `performance_scc_within_budget` | <= 100ms |
| WCC | `performance_wcc_within_budget` | <= 100ms |
| Cycles | `performance_cycle_detection_within_budget` | <= 100ms |
| Reachability | `performance_reachability_within_budget` | <= 100ms |

### Gate 4: Pipeline Parity

End-to-end layout pipeline must produce valid, deterministic output with
directed algorithms enabled.

| Test | Requirement |
|------|-------------|
| `parity_directed_layout_produces_valid_svg` | Valid SVG with nodes and edges |
| `parity_directed_layout_deterministic` | Identical output across 10 runs |
| `parity_fnx_enabled_vs_disabled_consistency` | Same node count in both modes |

---

## Rollout Phases

### Phase 2a: Shadow Mode (Current)

- Directed algorithms run but results are logged only, not used for layout
- Evidence collection validates determinism and performance in production
- No user-visible impact

### Phase 2b: Advisory Mode

- Directed analysis informs layout hints but native layout remains authoritative
- Mismatch logging enables parity validation
- Rollback to 2a if quality regressions detected

### Phase 2c: Full Integration

- Directed analysis actively influences layout decisions
- Requires sustained success in 2b with zero regressions
- Feature flag: `fnx-experimental-directed`

---

## Rollback Triggers

### Automatic Rollback

The following conditions trigger immediate rollback to Phase-1 (undirected only):

1. **Determinism Violation**: Any algorithm produces different results on
   identical input within a single process lifetime

2. **Performance Regression**: Any algorithm exceeds 10x the baseline budget
   (1000ms for 100-node graphs)

3. **Crash/Panic**: Any directed algorithm causes a panic

4. **Output Corruption**: Layout produces invalid SVG or missing elements

### Manual Rollback

Operators may trigger rollback via:

```bash
# Disable directed features
export FRANKENMERMAID_FNX_DIRECTED=off

# Or via config
fnx_enabled: true
fnx_experimental_directed: false
```

---

## Monitoring

### Evidence Fields

All directed algorithm invocations emit structured logs with:

```json
{
  "scenario_id": "...",
  "input_hash": "...",
  "fnx_mode": "phase2_directed",
  "fnx_algorithm": "scc|wcc|cycles|reachability",
  "node_count": 0,
  "edge_count": 0,
  "analysis_ms": 0,
  "pass_fail_reason": "...",
  "surface": "fnx-directed-rollout-gate"
}
```

### Health Metrics

| Metric | Alert Threshold |
|--------|-----------------|
| `directed_algorithm_p99_ms` | > 500ms |
| `directed_determinism_violations` | > 0 |
| `directed_algorithm_panics` | > 0 |
| `layout_quality_regression_pct` | > 5% |

---

## CI Integration

The rollout gate test suite runs on every PR:

```yaml
- name: FNX Phase-2 Rollout Gate
  run: |
    cargo test -p frankenmermaid-cli --test fnx_directed_rollout_gate -- --nocapture
```

All gate tests must pass before merging any layout-related changes.

---

## Version Compatibility

Phase-2 directed algorithms are implemented natively in frankenmermaid and do
not depend on fnx upstream. This ensures:

1. **No version coupling**: Directed features work regardless of fnx version
2. **Determinism guarantee**: Native Rust implementation with explicit ordering
3. **Performance control**: No FFI overhead or Python runtime dependency

See [FNX_COMPATIBILITY_MATRIX.md](./FNX_COMPATIBILITY_MATRIX.md) for capability tracking.

---

## References

- Bead: bd-ml2r.8.2 (this document)
- Parent: bd-ml2r.8 (Program rollout gate)
- Test suite: `crates/fm-cli/tests/fnx_directed_rollout_gate.rs`
