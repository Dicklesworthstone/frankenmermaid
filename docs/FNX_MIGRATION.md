# FNX Migration Guide

> Risk-tiered guidance for adopting FNX graph intelligence features.

## Overview

This guide helps you migrate from FNX-disabled to FNX-enabled operation safely, with clear risk assessment at each step.

**Migration Philosophy**: FNX integration is designed to be zero-risk by default. The native layout engine always has final authority, and FNX provides advisory hints only. You can enable FNX incrementally without breaking existing workflows.

---

## Migration Tiers

### Tier 0: No Change Required (Zero Risk)

If you're using frankenmermaid with default settings:

```bash
fm-cli render diagram.mmd --format svg
```

**You're already using FNX** in auto mode. The engine:
- Enables FNX analysis when available and beneficial
- Falls back gracefully to native-only when FNX is unavailable
- Produces deterministic output regardless of FNX status

**Action**: None. Your existing workflow is migration-complete.

---

### Tier 1: Explicit FNX Control (Low Risk)

To explicitly control FNX behavior:

| Goal | Command | Risk |
|------|---------|------|
| Always use FNX | `--fnx-mode enabled` | Low - fails if unavailable |
| Never use FNX | `--fnx-mode disabled` | Zero - guaranteed baseline |
| Auto-select (default) | `--fnx-mode auto` | Zero - graceful fallback |

**Recommended for**:
- CI pipelines that need consistent behavior
- Batch processing where FNX overhead matters
- Testing to compare FNX vs non-FNX output

**Example**:
```bash
# CI: explicit FNX for test reproducibility
fm-cli render diagram.mmd --fnx-mode enabled --format svg

# Batch: disable FNX for throughput
for f in *.mmd; do
    fm-cli render "$f" --fnx-mode disabled --format svg -o "out/${f%.mmd}.svg"
done
```

---

### Tier 2: Projection Mode Selection (Medium Risk)

FNX currently supports undirected graph analysis only:

| Projection | Status | Use Case |
|------------|--------|----------|
| `undirected` | Supported | Connectivity, centrality, cycles |
| `directed` | Future | DAG analysis, topological sort |
| `auto` | Future | Engine selects based on diagram type |

**Current limitation**: All directed diagrams (flowcharts, state, etc.) are projected to undirected graphs for analysis. This is semantically correct for connectivity analysis but loses directional information.

**Migration path**:
1. Use `--fnx-projection undirected` (current default)
2. When `directed` becomes available, test with a subset of diagrams
3. Enable `directed` only after validating output quality

**Risk mitigation**:
```bash
# Compare undirected vs future directed
fm-cli render diagram.mmd --fnx-projection undirected -o baseline.svg
# (future) fm-cli render diagram.mmd --fnx-projection directed -o directed.svg
# diff baseline.svg directed.svg
```

---

### Tier 3: Fallback Behavior (High Risk)

Fallback modes control what happens when FNX analysis fails:

| Fallback | Behavior | Risk Level |
|----------|----------|------------|
| `graceful` | Continue with native engine | Zero |
| `warn` | Continue but emit warning | Low |
| `strict` | Fail the command | High |

**Strict mode is high-risk because**:
- FNX may timeout on large graphs
- FNX may be unavailable in WASM builds
- Network or resource issues can cause transient failures

**Recommended migration**:
1. Start with `graceful` (default)
2. Use `warn` in CI to detect FNX issues without blocking
3. Only use `strict` when FNX is mandatory for your use case

**Example**:
```bash
# Development: graceful (never blocks)
fm-cli render diagram.mmd --fnx-fallback graceful

# CI: warn (log issues but don't fail build)
fm-cli render diagram.mmd --fnx-fallback warn 2>&1 | tee render.log
grep -q "FNX fallback" render.log && echo "FNX fell back to native"

# (future) Production with strict FNX requirement
# fm-cli render diagram.mmd --fnx-fallback strict
```

---

## Config Lint Warnings

The CLI automatically warns about risky configurations:

### Warning: FNX enabled with strict fallback

```
Warning: --fnx-mode=enabled with --fnx-fallback=strict may fail unexpectedly.
Recommendation: Use --fnx-fallback=graceful unless FNX is mandatory.
```

**Why**: Strict fallback fails the entire render if FNX analysis fails, which can happen for large graphs or resource constraints.

### Warning: Directed projection not yet supported

```
Warning: --fnx-projection=directed is not yet supported.
Using: undirected projection (direction information is preserved in layout).
```

**Why**: Directed graph algorithms are planned but not yet implemented.

### Warning: FNX unavailable in WASM

```
Warning: FNX integration is not available in WebAssembly builds.
Using: native layout engine only.
```

**Why**: FNX dependencies require native code features unavailable in WASM.

---

## Validation Checklist

Before enabling FNX in production:

### Functional Validation

- [ ] Run `docs/examples/fnx_compare.sh` on representative diagrams
- [ ] Compare output quality (node positions, edge crossings)
- [ ] Verify determinism: same input produces same output 5x

### Performance Validation

- [ ] Benchmark render time with FNX enabled vs disabled
- [ ] Test with largest expected diagram (node count, edge count)
- [ ] Verify timeout behavior for edge cases

### Rollback Plan

- [ ] Document current `--fnx-mode` setting
- [ ] Test that `--fnx-mode disabled` produces acceptable output
- [ ] Have CI detect FNX-related regressions

---

## Rollback Procedure

If FNX causes issues:

### Immediate Rollback (No Code Change)

```bash
# Add --fnx-mode disabled to all fm-cli commands
fm-cli render diagram.mmd --fnx-mode disabled --format svg
```

### CI/Script Rollback

```bash
# Set environment variable (if your wrapper respects it)
export FM_FNX_MODE=disabled

# Or update all commands in scripts
sed -i 's/--fnx-mode enabled/--fnx-mode disabled/g' scripts/*.sh
```

### Build-Time Rollback

```bash
# Rebuild without FNX feature
cargo build --release -p fm-cli
# (no --features fnx-integration)
```

---

## FAQ

### Will FNX break my existing diagrams?

No. FNX is advisory only. The native layout engine always makes final decisions. Output may differ slightly (better layout quality, additional CSS classes) but structural correctness is preserved.

### What if FNX makes a diagram worse?

Disable FNX for that specific diagram:

```bash
fm-cli render problematic.mmd --fnx-mode disabled --format svg
```

Report the issue with input diagram and both outputs.

### How do I know if FNX is being used?

Use `--json` output:

```bash
fm-cli render diagram.mmd --format svg --json -o out.svg 2>/dev/null
# Check fnx_witness field in stdout JSON
```

### What's the performance impact?

For typical diagrams (< 200 nodes):
- Parse: No change
- Layout: +5-20ms for FNX analysis
- Render: No change (FNX affects layout hints only)

For large diagrams (1000+ nodes), FNX may timeout (50ms budget) and fall back to native.

---

## Related Documentation

- [FNX User Guide](FNX_USER_GUIDE.md) - When to use FNX, command examples, troubleshooting
- [FNX Integration Architecture](FNX_INTEGRATION.md) - Technical contract, implementation details
