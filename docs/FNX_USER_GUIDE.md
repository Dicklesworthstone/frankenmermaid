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
| `directed` | Directed structural analysis for `validate`, requiring the `fnx-experimental-directed` build feature |
| `auto` | Reserved; rejected while FNX is active (use `undirected`) |

Directed validation preserves edge direction when finding strongly connected components
and feedback loops. For example, the diamond `A --> B --> D` and `A --> C --> D` has an
undirected cycle but no directed cycle. Two-way arrowheads (`<-->`, `<==>`, `<-.->`)
contribute both directions; invisible `~~~` positioning links do not contribute structural
relationships. Cyclic edge indices always refer to the original input IR, including when
earlier links were invisible or one authored link contributes two arcs.

```bash
cargo build -p frankenmermaid-cli --features fnx-experimental-directed
fm-cli validate dependencies.mmd --fnx-projection directed --format json
```

The JSON `fnx_directed` object reports strong and weak component counts, strong component
membership, cyclic nodes and edge indices, and source/sink nodes. Source and sink sets
describe connections to other nodes, so an isolated self-loop remains both a source and a
sink. Missing metric fields mean their stage did not complete; they do not imply zero.
The witness records actual FNX SCC execution and timing. Native weak-connectivity and
edge-summary passes appear separately in `native_algorithms_invoked`.

This explicit validation mode uses native layout and does not change rollout state or
enable directed rendering. `render` and `render-batch` still reject directed projection
while FNX is active. `--fnx-mode disabled` suppresses directed analysis as well.

### `--fnx-time-budget-ms` and `--fnx-work-budget`

Both options are available on `render`, `render-batch`, and `validate`.

| Option | Default | Meaning |
|--------|---------|---------|
| `--fnx-time-budget-ms <u64>` | `100` | Cooperative elapsed-time limit for each FNX analysis stage |
| `--fnx-work-budget <u64>` | `10000` | Deterministic work-unit allowance for each FNX analysis stage |

The configured limits cover automatic layout selection, centrality ordering and styling,
cycle-criticality scoring, and structural validation. Each analysis stage has its own
allowance. In a batch, each diagram receives its own limits; the values do not cap the
whole batch or the native layout/rendering pipeline.

Directed validation shares one allowance across SCC projection, backend execution,
verification, native weak connectivity, and native edge summaries. Each reserves
`nodes + original edges + added reverse arcs` work units before starting, so a complete
nonempty analysis uses five such reservations. Invisible links remain charged for scanning;
reverse arcs are admitted before constructing the expanded graph. It does not compute
all-pairs reachability. Graceful and warn modes
retain completed directed metrics when a later stage exhausts the budget; strict mode
fails while still producing the validation report.

Work is admitted before graph projection and backend passes. A work budget of `0`
prevents applicable FNX analysis and follows the selected fallback policy. Empty inputs
and diagrams with no applicable layout analysis do not fail merely because the allowance
is zero. Use `--fnx-mode disabled` to skip FNX without requesting analysis or triggering
budget fallback.

Time limits are cooperative checks between operations, not hard deadlines that interrupt
a running backend call. A time budget of `0` is accepted, but it does not guarantee that
no work runs before elapsed time is observed. Use a zero **work** budget for that behavior.
Both arguments accept nonnegative integers, including zero; a disabled or unavailable
automatic FNX mode ignores the analysis limits.

```bash
# Give a larger diagram more analysis capacity while requiring completion
fm-cli render large.mmd --fnx-mode enabled --fnx-time-budget-ms 500 \
    --fnx-work-budget 50000 --fnx-fallback strict --output large.svg

# Apply a smaller independent allowance to every diagram in a batch
fm-cli render-batch diagrams/*.mmd --out-dir rendered --fnx-time-budget-ms 50 \
    --fnx-work-budget 5000 --fnx-fallback warn

# Inspect cancellation and unknown metrics in the validation report
fm-cli validate input.mmd --format json --fnx-work-budget 0 \
    --fnx-fallback warn --fail-on none
```

Changing either budget invalidates the corresponding layout and batch cache key, so
a result computed under a larger allowance cannot satisfy a later strict request with
a smaller allowance.

### `--fnx-fallback`

| Value | Description |
|-------|-------------|
| `graceful` | Continue with available analysis and native heuristics after a budget cancellation (default) |
| `warn` | Report budget cancellation on stderr and in JSON warnings, then continue with available results |
| `strict` | Fail the affected render or validation when an FNX analysis exceeds its budget |

A diagram with no applicable FNX pass can render successfully under all three policies.
Use `--fnx-mode enabled` to require a build with FNX support. During validation,
`strict` preserves the diagnostic report and returns a failing exit status even with
`--fail-on none`.

These policies apply to `render`, `render-batch`, and `validate`. A successful layout cache hit or `--fnx-mode disabled` does not trigger strict failure. Layouts with cancelled FNX analysis are not retained as successful full-layout cache entries, so a later strict request must still evaluate its analysis.

JSON validation output and `--diagnostics-out` retain completed structural metrics. The validation witness combines actual layout and structural executions and preserves the first cancellation reason if a later stage succeeds. Warn mode adds a `fnx.fallback.warn` diagnostic with remediation guidance; the usual `--fail-on` threshold still applies. Render warnings are also retained in JSON metadata.

---

## Troubleshooting

### "Graph analysis skipped: undirected projection"

**Cause**: Phase 1 (undirected) analysis projects the graph to an undirected form. For directed diagrams like flowcharts, edge directions are temporarily ignored during the undirected pass.

**Impact**: Minimal. Most structural metrics (centrality, connectivity, cycles) are meaningful even without direction.

**Workaround**: Build with `fnx-experimental-directed` and use `validate --fnx-projection directed`
to inspect directed connectivity and feedback loops. Rendering continues to use `undirected`;
the `auto` projection remains reserved.

### "FNX analysis timed out"

**Cause**: Automatic selection, centrality, cycle-criticality scoring, or structural validation exceeded its configured budget. The defaults are 100 ms and 10,000 work units per analysis stage. Checks occur before backend passes and between scoring steps; an individual running backend call is not preempted at the deadline.

**Impact**: Graceful and warn policies continue with completed findings and native heuristics. Strict policy fails the command. A missing metric is unknown, not a zero result. Layout quality may differ when advisory analysis is incomplete.

**Workaround**:
```bash
# Skip FNX for this diagram
fm-cli render large.mmd --fnx-mode disabled

# Allow fallback while making cancellation visible
fm-cli validate large.mmd --fnx-fallback warn --format json

# Raise both limits when complete analysis is required
fm-cli validate large.mmd --fnx-time-budget-ms 500 --fnx-work-budget 50000 \
    --fnx-fallback strict --format json
```

The same limits apply to layout assistance and structural validation. Completed findings
remain in the report after cancellation; unperformed checks remain unknown.

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
fm-cli render input.mmd --format svg --output out.svg --json --fnx-mode enabled | jq '.fnx_witness'
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

The render witness's `results_hash` covers the selected layout, node and edge
geometry, graph bounds, crossing counts, and per-node centrality results.
Validation hashes the completed structural passes, their counts, and the
fallback outcome. Replaying the same result under another fallback policy
preserves the hash. The v2 hash encoding changes earlier recorded digest
values, so regenerate reference digests when comparing across this change.

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
| > 500 nodes | May exceed a budget | Use `disabled` or an explicit fallback policy |

FNX analysis stages use cooperative cancellation, which does not impose a hard wall-clock deadline on a running backend call. Automatic selection and centrality ordering use the configured budgets too. Tune `--fnx-time-budget-ms` and `--fnx-work-budget` to control the allowance; FNX remains advisory and incomplete analysis follows the selected fallback policy.

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
