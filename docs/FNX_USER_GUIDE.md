# FNX User Guide

> User-focused guide for frankenmermaid's graph intelligence integration.

## What is FNX?

FNX (franken_networkx) is an optional graph analysis engine that provides structural intelligence to improve layout quality and surface actionable diagnostics. When enabled, FNX:

- Identifies high-centrality hub nodes for semantic styling
- Detects cycles and suggests mitigation strategies
- Finds disconnected components that may indicate diagram issues
- Computes connectivity metrics for layout algorithm selection

**Key principle**: FNX is **advisory only**. The native layout engine always has final authority. FNX provides hints that can improve layout quality, but it never overrides deterministic output guarantees.

---

## Quick Start

```bash
# Default: FNX runs in auto mode (enabled when available)
echo 'flowchart TD; A-->B-->C-->D' | fm-cli render - --format svg

# Explicitly enable FNX analysis
echo 'flowchart TD; A-->B-->C-->D' | fm-cli render - --format svg --fnx-mode enabled

# Disable FNX (use native engine only)
echo 'flowchart TD; A-->B-->C-->D' | fm-cli render - --format svg --fnx-mode disabled

# Check FNX diagnostics in validate output
echo 'flowchart TD; A-->B; C-->D' | fm-cli validate - --format json --fnx-mode enabled
```

---

## When FNX Helps

### 1. Hub-and-Spoke Diagrams

FNX identifies central nodes that connect many others, enabling semantic styling:

```bash
# Hub detection highlights the central node
cat << 'EOF' | fm-cli render - --format svg --fnx-mode enabled
flowchart TD
    Hub[Central Service]
    A --> Hub
    B --> Hub
    C --> Hub
    Hub --> D
    Hub --> E
    Hub --> F
EOF
```

**Result**: The `Hub` node receives a `fm-node-centrality-high` CSS class, allowing visual emphasis.

### 2. Complex Cyclic Graphs

FNX detects cycles and recommends appropriate layout strategies:

```bash
cat << 'EOF' | fm-cli validate - --format json --fnx-mode enabled
flowchart TD
    A --> B
    B --> C
    C --> A
    A --> D
    D --> E
    E --> B
EOF
```

**Output includes**:
```json
{
  "diagnostics": [
    {
      "category": "cycle_analysis",
      "message": "Graph contains 2 cycles affecting layout",
      "recommendation": "Consider cycle_aware layout strategy"
    }
  ]
}
```

### 3. Disconnected Component Detection

FNX warns when subgraphs have no connections:

```bash
cat << 'EOF' | fm-cli validate - --format json --fnx-mode enabled
flowchart LR
    subgraph Main
        A --> B --> C
    end
    subgraph Orphan
        X --> Y
    end
EOF
```

**Diagnostic**: "2 disconnected components detected. Consider adding edges or merging subgraphs."

### 4. Large Graphs with Complex Structure

FNX computes metrics that help select the optimal layout algorithm:

```bash
# FNX helps choose between sugiyama, force-directed, or tree layouts
fm-cli render large_graph.mmd --format svg --fnx-mode enabled --layout-algorithm auto
```

---

## When to Disable FNX

### 1. Simple Linear Diagrams

For straightforward A→B→C chains, FNX analysis adds no value:

```bash
# Faster without FNX for simple graphs
echo 'flowchart LR; A-->B-->C-->D' | fm-cli render - --fnx-mode disabled
```

### 2. Non-Graph Diagram Types

These diagram types don't benefit from graph analysis:
- Pie charts
- Gantt charts
- XY charts
- Timeline diagrams
- Sequence diagrams (use specialized layout)

```bash
# Pie charts ignore FNX automatically
echo 'pie; "A": 40; "B": 60' | fm-cli render - --format svg
```

### 3. Performance-Critical Batch Processing

When processing thousands of diagrams:

```bash
# Disable FNX for maximum throughput
for f in diagrams/*.mmd; do
    fm-cli render "$f" --fnx-mode disabled --format svg --output "out/$(basename "$f" .mmd).svg"
done
```

### 4. WASM Environments

FNX is automatically disabled in WebAssembly builds due to dependency constraints. No action needed.

---

## CLI Options Reference

### `--fnx-mode`

| Value | Description |
|-------|-------------|
| `auto` | Enable FNX when available and beneficial (default) |
| `enabled` | Always use FNX analysis (error if unavailable) |
| `disabled` | Never use FNX, use native engine only |

### `--fnx-projection`

| Value | Description |
|-------|-------------|
| `undirected` | Project directed edges to undirected graph (default) |
| `directed` | Use directed graph analysis (future, not yet supported) |

### `--fnx-fallback`

| Value | Description |
|-------|-------------|
| `graceful` | Continue with native engine if FNX fails (default) |
| `strict` | Fail the command if FNX analysis fails |

---

## Troubleshooting

### "Graph analysis skipped: undirected projection"

**Cause**: Phase 1 (undirected) analysis projects the graph to an undirected form. For directed diagrams like flowcharts, edge directions are temporarily ignored during the undirected pass.

**Impact**: Minimal. Most structural metrics (centrality, connectivity, cycles) are meaningful even without direction.

**Workaround**: None needed. This is informational. Directed analysis (SCC, WCC, reachability) ships behind the `fnx-experimental-directed` feature flag, currently in canary rollout.

### "FNX analysis timed out"

**Cause**: The graph is very large (1000+ nodes) or highly connected, exceeding the 50ms analysis budget.

**Impact**: Layout proceeds normally using native heuristics. Output quality is unchanged.

**Workaround**:
```bash
# Increase timeout (not recommended for interactive use)
fm-cli render large.mmd --fnx-timeout-ms 200
```

### "fnx-integration feature not available"

**Cause**: The binary was built without FNX support, or you're using WASM.

**Impact**: `--fnx-mode enabled` will fail. `auto` and `disabled` work normally.

**Workaround**:
```bash
# Rebuild with FNX support
cargo build --release --features fnx-integration

# Or use disabled mode
fm-cli render input.mmd --fnx-mode disabled
```

### Centrality classes not appearing in SVG

**Cause**: FNX analysis may have been skipped (too few nodes, unsupported diagram type, or disabled).

**Check**:
```bash
# Verify FNX witness in JSON output
fm-cli render input.mmd --format svg --json --fnx-mode enabled | jq '.fnx_witness'
```

**Expected output** (when FNX runs):
```json
{
  "enabled": true,
  "used": true,
  "projection_mode": "undirected",
  "algorithms_invoked": ["degree_centrality", "cycle_detection"]
}
```

### Layout quality seems worse with FNX enabled

**Cause**: Rare, but FNX hints may occasionally conflict with optimal layout for specific graph structures.

**Workaround**:
```bash
# Disable FNX for this specific diagram
fm-cli render problematic.mmd --fnx-mode disabled
```

If you find a case where FNX produces inferior results, please report it with:
1. The input diagram
2. Both FNX-on and FNX-off SVG outputs
3. Description of the quality difference

---

## Performance Expectations

| Graph Size | FNX Overhead | Recommendation |
|------------|--------------|----------------|
| < 50 nodes | < 5ms | Always use `auto` |
| 50-200 nodes | 5-20ms | `auto` is fine |
| 200-500 nodes | 20-50ms | Consider `disabled` for batch |
| > 500 nodes | May timeout | Use `disabled` or increase timeout |

FNX analysis is single-threaded and bounded by the timeout budget. Layout quality impact is typically neutral to positive for graphs with complex structure.

---

## Example Scripts

### Compare FNX-on vs FNX-off

```bash
#!/bin/bash
# compare_fnx.sh - Compare layout with and without FNX
INPUT="${1:-input.mmd}"
BASE=$(basename "$INPUT" .mmd)

fm-cli render "$INPUT" --fnx-mode disabled --format svg --output "${BASE}_fnx_off.svg"
fm-cli render "$INPUT" --fnx-mode enabled --format svg --output "${BASE}_fnx_on.svg"

echo "Generated: ${BASE}_fnx_off.svg, ${BASE}_fnx_on.svg"
```

### Batch Process with FNX Disabled

```bash
#!/bin/bash
# batch_render.sh - Fast batch rendering without FNX
for f in diagrams/*.mmd; do
    out="output/$(basename "$f" .mmd).svg"
    fm-cli render "$f" --fnx-mode disabled --format svg --output "$out"
    echo "Rendered: $out"
done
```

### Extract FNX Diagnostics

```bash
#!/bin/bash
# extract_diagnostics.sh - Get FNX recommendations
INPUT="${1:-input.mmd}"

fm-cli validate "$INPUT" --format json --fnx-mode enabled 2>/dev/null | \
    jq '.diagnostics[] | select(.category | startswith("fnx_"))'
```

---

## Related Documentation

- [FNX Integration Architecture](FNX_INTEGRATION.md) - Technical contract and implementation details
- [CLI Reference](../README.md#quick-example) - Full command-line interface
- [Layout Algorithms](../README.md#design-philosophy) - How layouts are selected

---

## Feedback

If FNX produces unexpected results or you have suggestions for improvement:

1. Report issues at https://github.com/Dicklesworthstone/frankenmermaid/issues
2. Include the input diagram and `--json` output
3. Describe expected vs actual behavior
