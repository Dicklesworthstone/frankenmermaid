
<!-- BEGIN GENERATED: runtime-capability-metadata -->
| Surface | Status | Evidence |
|---------|--------|----------|
| CLI detect command | Implemented | 2 evidence refs |
| CLI parse command with IR JSON evidence | Implemented | 1 evidence refs |
| CLI SVG rendering | Implemented | 1 evidence refs |
| CLI terminal rendering | Implemented | 1 evidence refs |
| CLI validate command with structured diagnostics | Implemented | 1 evidence refs |
| CLI capability matrix command | Implemented | 2 evidence refs |
| WASM API renders SVG | Implemented | 1 evidence refs |
| WASM API exposes capability matrix metadata | Implemented | 1 evidence refs |
| Canvas rendering backend | Implemented | 1 evidence refs |
<!-- END GENERATED: runtime-capability-metadata -->
erminal/web output from a single pipeline.

<div align="center">

**Live Demo:** <https://dicklesworthstone.github.io/frankenmermaid/>

```bash
curl -fsSL "https://raw.githubusercontent.com/Dicklesworthstone/frankenmermaid/main/install.sh" | bash
```

The installer builds `fm-cli` from the GitHub repo with Cargo and installs it into `~/.local/bin`.

</div>

---

## TL;DR

**The Problem**: Mermaid syntax is great for documentation-as-code, but real-world diagrams hit walls fast. Cycles produce tangled layouts. Malformed input crashes the parser. Large graphs slow to a crawl. Styling control is limited. And there is no terminal output at all.

**The Solution**: `frankenmermaid` is a ground-up Rust implementation with a shared intermediate representation that feeds 15 layout algorithms and 3 render backends. It recovers from bad input instead of failing, picks cycle-aware layout strategies automatically, and produces deterministic output suitable for CI snapshot testing.

### Why Use frankenmermaid?

| Feature | What It Does |
|---------|--------------|
| **24 Diagram Types** | Flowchart, sequence, class, state, ER, gantt, pie, gitGraph, journey, mindmap, timeline, sankey, quadrant, xyChart, block-beta, packet-beta, architecture-beta, 5 C4 variants, requirement, kanban |
| **Intent-Aware Parsing** | Recovers from malformed syntax and infers likely intent instead of failing. Fuzzy keyword matching catches typos like `flowchar` or `seqeunceDiagram` |
| **15 Layout Algorithms** | Sugiyama, force, tree, radial, timeline, gantt, xychart, sankey, kanban, grid, sequence, pie, quadrant, gitgraph, and packet, with auto-selection per diagram type |
| **4 Cycle Strategies** | Greedy, DFS back-edge, MFAS approximation, and cycle-aware layout with cluster collapse and quality metrics |
| **High-Fidelity SVG** | Responsive viewBox, 20+ node shapes, gradient fills, drop shadows, glow effects, cluster backgrounds, accessible markup, 4 theme presets |
| **Terminal Rendering** | Braille (2x4), block (2x2), half-block, and cell-only sub-pixel modes with Unicode box-drawing and ASCII fallback |
| **Web / WASM** | `@frankenmermaid/core` npm package with Canvas2D rendering backend and full parse/layout/render API |
| **Deterministic Output** | Same input + same config = byte-identical SVG. Stable tie-breaking at every pipeline stage |
| **Zero Unsafe Code** | `#![forbid(unsafe_code)]` in every crate. No panics on malformed input |
| **DOT Bridge** | Parses Graphviz DOT format and converts to the shared IR for rendering |

## Quick Example

```bash
# Detect diagram type with confidence score
echo 'flowchart LR; A-->B-->C' | fm-cli detect -
# → Flowchart (confidence: 1.0, method: ExactKeyword)

# Render to SVG
echo 'flowchart LR; A-->B-->C' | fm-cli render - --format svg --output demo.svg

# Render to terminal (great for CI logs)
echo 'flowchart LR; A-->B-->C' | fm-cli render - --format term

# Validate with diagnostics
echo 'flowchrt LR; A-->B' | fm-cli validate -
# → Warning: fuzzy match "flowchrt" → "flowchart" (confidence: 0.85)

# Parse to IR JSON for tooling integration
echo 'sequenceDiagram; Alice->>Bob: hello' | fm-cli parse - --format json

# Emit capability matrix
fm-cli capabilities --pretty

# File-based workflow
fm-cli render diagrams/process.mmd --format svg --theme dark --output out/process.svg
```

## Design Philosophy

1. **Never Waste User Intent**
   Malformed input degrades gracefully into best-effort IR plus actionable diagnostics, not dead-end errors. If the parser can figure out what you probably meant, it will.

2. **Determinism Is a Feature**
   Every layout phase uses stable tie-breaking. Node ordering, rank assignment, coordinate computation, and edge routing all produce identical results for identical input. CI snapshot tests rely on this.

3. **Layout Quality Beats Minimal Correctness**
   Four cycle-breaking strategies. Barycenter + transpose crossing minimization. Orthogonal edge routing with bend minimization. Specialized algorithms for sequence, gantt, timeline, sankey, radial, and grid diagrams.

4. **One IR, Many Outputs**
   A shared `MermaidDiagramIr` feeds SVG, terminal, Canvas, and WASM APIs. Parse once, render everywhere. Layout statistics and diagnostics travel through the entire pipeline.

5. **Polish Is Core Product Surface**
   Typography, spacing, theming, accessibility, node gradients, drop shadows, and responsive sizing are all part of correctness.

## How frankenmermaid Compares

| Capability | frankenmermaid | mermaid-js | mermaid-cli (mmdc) |
|------------|----------------|------------|--------------------|
| Language / runtime | Rust + WASM | JavaScript | Node.js wrapper |
| Parser recovery on malformed input | Best-effort with diagnostics | Often strict failure | Upstream behavior |
| Fuzzy keyword detection | Levenshtein + heuristics | No | No |
| Cycle-aware layout strategies | 4 strategies + cluster collapse | Basic | Upstream |
| Specialized layout algorithms | 10 (auto-selected per type) | Varies by type | Upstream |
| Terminal rendering | Built-in (4 fidelity modes) | No | No |
| Canvas2D web rendering | Built-in | No | No |
| DOT format bridge | Built-in | No | No |
| Deterministic output guarantee | Explicit design goal | Not guaranteed | Not guaranteed |
| SVG accessibility (ARIA) | Built-in | Limited | Upstream |
| WASM JS API | `@frankenmermaid/core` | Yes | No |
| Unsafe code | Forbidden | N/A (JS) | N/A |

## Supported Diagram Types

<!-- BEGIN GENERATED: supported-diagram-types -->
| Diagram Type | Runtime Status |
|--------------|----------------|
| `flowchart` | Implemented |
| `sequence` | Partial |
| `class` | Implemented |
| `state` | Implemented |
| `er` | Implemented |
| `C4Context` | Implemented |
| `C4Container` | Implemented |
| `C4Component` | Implemented |
| `C4Dynamic` | Implemented |
| `C4Deployment` | Implemented |
| `architecture-beta` | Implemented |
| `block-beta` | Implemented |
| `gantt` | Implemented |
| `timeline` | Implemented |
| `journey` | Implemented |
| `gitGraph` | Implemented |
| `sankey` | Implemented |
| `mindmap` | Implemented |
| `pie` | Implemented |
| `quadrantChart` | Implemented |
| `xyChart` | Implemented |
| `requirementDiagram` | Implemented |
| `packet-beta` | Implemented |
| `kanban` | Implemented |
<!-- END GENERATED: supported-diagram-types -->

**Key:** Full = complete syntax coverage. Basic = core syntax works, advanced features in progress. Minimal = parsed but rendering needs dedicated work.

## Installation

### Quick Install (CLI)

```bash
curl -fsSL "https://raw.githubusercontent.com/Dicklesworthstone/frankenmermaid/main/install.sh" | bash
```

### JavaScript / WASM

> **Note:** The `@frankenmermaid/core` package is not yet published to npm. Use one of the methods below:

**Option 1: Build from source (recommended)**
```bash
git clone https://github.com/Dicklesworthstone/frankenmermaid.git
cd frankenmermaid
./build-wasm.sh
# Copy pkg/ to your project
cp -r pkg/ your-project/frankenmermaid/
```

**Option 2: Use the GitHub Pages bundle**

The [live demo](https://dicklesworthstone.github.io/frankenmermaid/) uses a pre-built WASM bundle. You can reference it directly or download from the `gh-pages` branch.

**Coming soon:** npm publishing is planned. Track progress in the [issues](https://github.com/Dicklesworthstone/frankenmermaid/issues).

### From Source

```bash
git clone https://github.com/Dicklesworthstone/frankenmermaid.git
cd frankenmermaid
cargo build --release --workspace
# Binary at target/release/fm-cli
```

**Note:** Requires Rust nightly (see `rust-toolchain.toml`). The project uses Rust 2024 edition features.

`frankenmermaid` is not published on crates.io yet, so `cargo install frankenmermaid` is not a working install path today. Until crates.io publishing is prepared across the workspace crates, use the installer above or build `fm-cli` from source.

### Optional FNX Integration

> **Status: Early Development** — The FNX integration is currently under active development. The feature flags compile but the integration surface is not yet functional. Enabling `--features fnx-integration` will compile additional dependencies but won't change runtime behavior. Track progress via the `bd-ml2r` epic in the issue tracker.

**Roadmap:**
- **Phase 1** (in progress): Undirected graph intelligence + diagnostics — cycle detection, connectivity analysis, centrality metrics for layout heuristics
- **Phase 2** (planned): Directed graph analysis + ordering enrichment — topological awareness, critical path analysis

The canonical FNX dependency model is a pinned Git dependency on
[`franken_networkx`](https://github.com/Dicklesworthstone/franken_networkx),
with the integration disabled by default.

- `fnx-integration`: enables the Phase 1 undirected/advisory integration surface (dependencies compile, runtime integration WIP)
- `fnx-experimental-directed`: extends `fnx-integration` for future directed-only work; keep this off outside explicit experiments

Default builds remain FNX-free, which is also the safe path when FNX is
unavailable (offline or missing git access):

```bash
cargo build --workspace
```

Build with FNX dependencies (for development/testing only):

```bash
cargo build -p fm-cli --features fnx-integration
cargo build -p fm-wasm --features fnx-integration
```

**Why this dependency model:**

- Local path dependencies would tightly couple frankenmermaid releases to one workstation layout.
- Published crates do not exist yet, so crates.io is not an option.
- A Git dependency pinned to a specific commit is reproducible in CI and release packaging while keeping the default build independent.
- When FNX is unavailable, keeping `fnx-integration` disabled guarantees the build stays isolated from the FNX toolchain.

See [docs/FNX_INTEGRATION.md](docs/FNX_INTEGRATION.md) for the full architecture decision record.

If you are developing `frankenmermaid` and `franken_networkx` together, prefer a
local developer-only Cargo patch override instead of committing path
dependencies into this repo.

Example (developer-only, do not commit):

```toml
# .cargo/config.toml
[patch."https://github.com/Dicklesworthstone/franken_networkx.git"]
fnx-runtime = { path = "/abs/path/to/franken_networkx/crates/fnx-runtime" }
fnx-classes = { path = "/abs/path/to/franken_networkx/crates/fnx-classes" }
fnx-algorithms = { path = "/abs/path/to/franken_networkx/crates/fnx-algorithms" }
fnx-views = { path = "/abs/path/to/franken_networkx/crates/fnx-views" }
```

## Quick Start

1. **Create** a Mermaid file:
   ```bash
   echo 'flowchart LR
     A[Start] --> B{Decision}
     B -->|Yes| C[Action]
     B -->|No| D[Skip]
     C --> E[End]
     D --> E' > demo.mmd
   ```

2. **Detect** the diagram type:
   ```bash
   fm-cli detect demo.mmd
   ```

3. **Render** to SVG:
   ```bash
   fm-cli render demo.mmd --format svg --output demo.svg
   ```

4. **Preview** in terminal:
   ```bash
   fm-cli render demo.mmd --format term
   ```

5. **Validate** for issues:
   ```bash
   fm-cli validate demo.mmd
   ```

6. **Edit interactively** with live preview:
   ```bash
   fm-cli interactive demo.mmd
   ```

7. **Use from JavaScript**:
   ```ts
   import { init, renderSvg } from '@frankenmermaid/core';
   await init();
   const svg = renderSvg('flowchart LR\nA-->B');
   document.getElementById('diagram').innerHTML = svg;
   ```

## Command Reference

### Global Flags

```
--config <path>        Config file (TOML/JSON)
--theme <name>         Theme preset (default, dark, forest, neutral)
--format <fmt>         Output format (svg, png, term, json)
-v, --verbose          Structured debug logging (repeatable: -vv for trace)
-q, --quiet            Errors only
--json                 Machine-readable JSON output
```

### `fm-cli render`

Parse, layout, and render a diagram.

```bash
# SVG output to file
fm-cli render input.mmd --format svg --output diagram.svg

# PNG rasterization (requires --features png)
fm-cli render input.mmd --format png --output diagram.png

# Terminal preview
fm-cli render input.mmd --format term

# With theme and layout override
fm-cli render input.mmd --format svg --theme dark

# From stdin
echo 'flowchart TD; A-->B' | fm-cli render - --format svg
```

### `fm-cli parse`

Emit the intermediate representation as JSON.

```bash
fm-cli parse input.mmd --format json
fm-cli parse input.mmd --format json --pretty
```

### `fm-cli detect`

Detect diagram type, confidence score, and detection method.

```bash
fm-cli detect input.mmd
fm-cli detect input.mmd --json
# Output: { "type": "flowchart", "confidence": 1.0, "method": "ExactKeyword" }
```

### `fm-cli validate`

Check syntax and semantics, print diagnostics with source spans.

```bash
fm-cli validate input.mmd
fm-cli validate input.mmd --verbose
```

### `fm-cli capabilities`

Emit the runtime capability matrix as JSON.

```bash
fm-cli capabilities --pretty
```

### `fm-cli diff`

Compare two diagrams and show structural differences.

```bash
fm-cli diff before.mmd after.mmd --format term
fm-cli diff before.mmd after.mmd --format json
```

### `fm-cli watch` (requires `--features watch`)

Watch files and re-render on change.

```bash
fm-cli watch diagrams/ --format svg --output out/
```

### `fm-cli serve` (requires `--features serve`)

Local playground with live reload.

```bash
fm-cli serve --host 127.0.0.1 --port 4173 --open
```

### `fm-cli interactive`

Launch a split-pane terminal editor with live diagram preview.

```bash
fm-cli interactive input.mmd
fm-cli interactive input.mmd --theme dark
```

Keyboard shortcuts: **Ctrl+Q** to quit, **Ctrl+S** to save. Requires a terminal (not available in CI or piped contexts).

## JavaScript / WASM API

```ts
import {
  init,
  renderSvg,
  detectType,
  parse,
  capabilityMatrix,
  Diagram
} from '@frankenmermaid/core';

// Initialize with defaults
await init({ theme: 'corporate' });

// Render SVG string
const svg = renderSvg('flowchart LR\nA-->B', { theme: 'dark' });

// Detect diagram type
const type = detectType('sequenceDiagram\nAlice->>Bob: hi');
// → { type: "sequence", confidence: 1.0 }

// Parse to IR
const ir = parse('classDiagram\nA <|-- B');

// Query capabilities
const caps = capabilityMatrix();

// Canvas rendering
const diagram = new Diagram(
  document.getElementById('canvas-root')!,
  { renderer: 'canvas2d' }
);
diagram.render('flowchart TD\nStart-->End');
```

## Configuration

Example `frankenmermaid.toml`:

```toml
# Global behavior
[core]
deterministic = true          # Reserved: output remains deterministic regardless
max_input_bytes = 5_000_000   # Input size limit
fallback_on_error = true      # Best-effort on parse failure

# Parser settings
[parser]
intent_inference = true       # Fuzzy keyword matching
fuzzy_keyword_distance = 2    # Max Levenshtein distance
auto_close_delimiters = true  # Auto-close unclosed brackets
create_placeholder_nodes = true # Create nodes for dangling edges

# Layout defaults
[layout]
algorithm = "auto"            # auto | sugiyama | force | tree | radial | sequence | timeline | gantt | sankey | kanban | grid
cycle_strategy = "cycle-aware" # greedy | dfs-back | mfas | cycle-aware
node_spacing = 48
rank_spacing = 72
edge_routing = "orthogonal"   # orthogonal | spline

# Render defaults
[render]
default_format = "svg"
show_back_edges = true
reduced_motion = "auto"

# SVG visual system
[svg]
theme = "corporate"
rounded_corners = 8
shadows = true
gradients = true
accessibility = true          # ARIA labels, semantic markup

# Terminal renderer
[term]
tier = "rich"                 # compact | normal | rich
unicode = true                # Unicode box-drawing vs ASCII
minimap = true                # Scaled overview of large diagrams
```

Mermaid inline directives are also supported:

```mermaid
%%{init: {"theme":"dark","flowchart":{"curve":"basis"}} }%%
flowchart LR
A --> B
```

## Technical Architecture

### Crate Map

| Crate | Lines | Responsibility |
|-------|-------|----------------|
| `fm-core` | ~12,300 | Shared IR types, config, errors, diagnostics, 20+ node shapes |
| `fm-parser` | ~16,400 | 24-type detection + parsing + error recovery + DOT bridge |
| `fm-layout` | ~24,400 | 15 layout algorithms, 4 cycle strategies, crossing minimization |
| `fm-render-svg` | ~13,100 | Accessible, themeable SVG with gradients/shadows/glows |
| `fm-render-term` | ~6,400 | Terminal renderer + diff engine + minimap + 4 fidelity modes |
| `fm-render-canvas` | ~3,500 | Canvas2D web rendering with trait-based abstraction |
| `fm-wasm` | ~1,500 | wasm-bindgen API and TypeScript bindings |
| `fm-cli` | ~7,300 | CLI surface: render, parse, detect, validate, diff, watch, serve, interactive |
| **Total** | **~84,900** | |

### Pipeline

```
            Mermaid / DOT text
                    |
                    v
      +----------------------------+
      | fm-parser                  |
      |  - type detection          |  25 diagram types
      |  - fuzzy matching          |  Levenshtein + heuristics
      |  - recovery + warnings     |  best-effort, never crashes
      +----------------------------+
                    |
                    v
      +----------------------------+
      | fm-core                    |
      |  MermaidDiagramIr          |  nodes, edges, clusters,
      |                            |  labels, subgraphs, ports
      +----------------------------+
                    |
                    v
      +----------------------------+
      | fm-layout                  |
      |  - auto algorithm select   |  15 algorithms available
      |  - cycle strategy          |  4 cycle-breaking modes
      |  - crossing minimization   |  barycenter + transpose
      +----------------------------+
                    |
                    v
      +----------------------------+
      | DiagramLayout + stats      |
      |  nodes, edges, clusters    |
      |  bounds, cycle info        |
      +---------+---------+--------+
               |         |        |
               v         v        v
         +---------+ +------+ +--------+
         |   SVG   | | Term | | Canvas |
         +---------+ +------+ +--------+
              |                    |
              v                    v
         SVG / PNG          WASM + browser
```

### Feature Flags

```toml
[features]
default = []
watch = ["dep:notify"]        # File watching for live reload
serve = ["dep:tiny_http"]     # Local preview server
png = ["dep:resvg", "dep:usvg"] # PNG rasterization from SVG
fnx-integration = ["fm-layout/fnx-integration"] # Optional FNX advisory path
fnx-experimental-directed = ["fm-layout/fnx-experimental-directed"] # Experimental directed path
```

## How the Parser Works

The parser uses a **five-tier detection pipeline** to identify diagram types, then dispatches to a type-specific parser that produces a shared intermediate representation.

### Type Detection Pipeline

```
Input text
    |
    v
+------------------------------------------------+
| 1. DOT Format Detection          conf: 0.95    |
|    digraph/graph keyword + braces              |
+------------------------------------------------+
| 2. Exact Keyword Match            conf: 1.0    |
|    "flowchart", "sequenceDiagram",             |
|    "classDiagram", "gantt", etc.               |
+------------------------------------------------+
| 3. Fuzzy Keyword Match          conf: 0.70+    |
|    Levenshtein distance 1-2                    |
|    "flowchrt" -> "flowchart"                   |
+------------------------------------------------+
| 4. Content Heuristics           conf: 0.60+    |
|    Arrow patterns: -->  ->>  ||--o{            |
|    Keywords: participant, state                |
+------------------------------------------------+
| 5. Fallback                      conf: 0.30    |
|    Default to Flowchart + warning              |
+------------------------------------------------+
```

Each tier is tried in order; the first match wins. The confidence score tells downstream consumers how certain the detection was, so tooling can surface low-confidence detections as warnings.

### Fuzzy Matching

The fuzzy matcher uses a two-row dynamic-programming Levenshtein distance computation (O(mn) time, O(n) space) against 14 base keywords. Only distances of 1 or 2 are accepted:

| Distance | Confidence | Example |
|----------|------------|---------|
| 0 | 1.0 (exact match, handled by tier 2) | `flowchart` |
| 1 | 0.85 | `flowchrt` → `flowchart` |
| 2 | 0.70 | `flwchart` → `flowchart` |
| 3+ | Rejected | Too ambiguous |

### Content Heuristics

When no keyword matches, the parser examines the input for characteristic symbols:

| Pattern | Detected Type | Confidence |
|---------|---------------|------------|
| `\|\|--o{`, `}|--\|\|`, `\|o--o\|` | ER diagram | 0.80 |
| `->>`, `participant`, `actor` | Sequence | 0.75 |
| `<\|--`, `--\|>`, `class {` | Class | 0.75 |
| `[*] -->`, `--> [*]`, `state` | State | 0.70 |
| `-->`, `---`, `==>` | Flowchart | 0.60 |

### Error Recovery

The parser never panics on malformed input. Instead, it uses several recovery strategies:

1. **Dangling edge recovery**: If an edge references a node that was never declared, the parser auto-creates an implicit placeholder node and emits a diagnostic suggesting the user define it explicitly.

2. **Node deduplication**: If the same node ID appears multiple times with different labels or shapes, the parser keeps the most specific variant rather than creating duplicates.

3. **Label normalization**: Quotes, backticks, and surrounding whitespace are stripped from labels. Empty labels after cleaning are silently dropped.

4. **Graceful unknown syntax**: Lines that don't match any known pattern produce a warning-level diagnostic but don't abort parsing. The rest of the diagram continues to parse normally.

The result is that even heavily malformed input produces a best-effort IR with diagnostics explaining what was recovered, rather than a cryptic error message and no output.

## How the Layout Engine Works

The layout engine takes a parsed `MermaidDiagramIr` and produces a `DiagramLayout`: positioned node boxes, routed edge paths, and cluster boundaries. Different diagram types get different algorithms, but the output shape is always the same.

### Algorithm Auto-Selection

When `algorithm = "auto"` (the default), the engine maps diagram types to their best algorithm:

| Algorithm | Used For | Strategy |
|-----------|----------|----------|
| **Sugiyama** | Flowchart, class, state, ER, C4, requirement | Hierarchical layered layout with rank assignment and crossing minimization |
| **Force** | Available for all graph types | Spring-electrical simulation with Barnes-Hut optimization |
| **Tree** | Available for all graph types | Reingold-Tilford tidy tree with Knuth-style spacing |
| **Radial** | Mindmap | Concentric rings with angle allocation proportional to subtree leaf count |
| **Timeline** | Timeline diagrams | Linear horizontal periods with vertically stacked events |
| **Gantt** | Gantt charts | Time-axis bar layout with section swimlanes |
| **XyChart** | XY chart diagrams | Cartesian axis layout with spaced categories and plotted series anchors |
| **Sankey** | Sankey diagrams | Flow-conserving column layout with iterative relaxation |
| **Kanban** | Journey, kanban | Fixed-column card stacking |
| **Grid** | Block-beta | CSS-grid-like positioning with column/row spans |
| **Sequence** | Sequence diagrams | Horizontal participant columns, vertical message stacking, self-message loops |
| **Pie** | Pie diagrams | Radial slice layout with centered slice geometry and label anchoring |
| **Quadrant** | Quadrant charts | Four-quadrant scatter layout around central axes |
| **GitGraph** | GitGraph diagrams | Branch-lane layout with chronological commit stacking |
| **Packet** | Packet-beta diagrams | Grid-derived packet lane layout for packet-beta structures |

### The Sugiyama Algorithm (Hierarchical Layout)

The Sugiyama algorithm is the workhorse for most graph diagram types. It transforms an arbitrary directed graph into a clean layered layout through seven phases:

**Phase 1: Cycle Removal**

Directed graphs with cycles can't be laid out in layers (every edge must point "downward"). The engine breaks cycles by temporarily reversing selected edges. Four strategies are available:

| Strategy | How It Works | When to Use |
|----------|--------------|-------------|
| **Greedy** | Repeatedly remove sink/source nodes, reverse remaining edges | Fast default. Good enough for most graphs |
| **DFS back-edge** | Run DFS, reverse back-edges found during traversal | Predictable; the same DFS order gives the same result |
| **MFAS approximation** | Approximate minimum feedback arc set via heuristic ordering | Minimizes the number of reversed edges |
| **Cycle-aware** | Full SCC (strongly connected component) detection with optional cluster collapse | Best visual quality. Cycle clusters rendered as grouped boxes |

The cycle-aware strategy additionally computes `cycle_count`, `cycle_node_count`, `max_cycle_size`, and `reversed_edge_total_length` metrics that are available in the layout stats.

Under the hood, cycle detection uses **Tarjan's strongly connected components** algorithm with index/lowlink tracking and on-stack bit flags to distinguish back-edges from cross-edges. The individual strategies then work differently:

- **Greedy**: Repeatedly removes sinks (out-degree 0) and sources (in-degree 0). Remaining nodes are ordered by `max(out_degree - in_degree)`. Edges that violate the resulting ordering are reversed.
- **DFS back-edge**: Standard DFS with three-color marking (unvisited → visiting → visited). Edges to nodes in the "visiting" state are back-edges and get reversed. Linear O(V+E).
- **MFAS**: Operates per SCC. Sorts nodes by `(out_degree - in_degree)` descending, then reverses edges that violate the resulting position order. Falls back to DFS if no improvement found.

**Phase 2: Rank Assignment**

Each node is assigned an integer rank (layer) using a longest-path heuristic. Ranks are computed in topological order so that every non-reversed edge goes from a lower rank to a higher rank. This determines the vertical (or horizontal, depending on direction) position of each node.

**Phase 3: Crossing Minimization (Barycenter)**

The ordering of nodes within each rank is optimized to minimize edge crossings. The algorithm performs 4 bidirectional sweeps:

1. For each rank, compute each node's **barycenter**, the weighted average position of its connected neighbors in the adjacent rank.
2. Sort the rank's nodes by barycenter value, breaking ties by stable node index.
3. Sweep top-to-bottom, then bottom-to-top (bidirectional).

**Phase 4: Crossing Refinement (Transpose + Sift)**

After barycenter ordering, two local-search refinements further reduce crossings:

- **Transpose**: Try swapping every adjacent pair of nodes within each rank. Accept swaps that reduce the total crossing count. Run up to 10 passes, early-exit if crossings reach zero.
- **Sifting**: For each node, evaluate all possible positions within its rank and move it to the position that minimizes crossings.

The layout stats record `crossing_count_before_refinement` and final `crossing_count` so you can see how much the refinement improved things.

The crossing count itself is computed using a **merge-sort inversion counting** algorithm. For each pair of adjacent ranks, edges are sorted by source position and their target positions are extracted. The number of inversions in the target sequence equals the number of crossings. This runs in O(m log m) per rank pair, where m is the number of edges between the two ranks.

**Phase 5: Coordinate Assignment**

Nodes are positioned in 2D space using their rank (vertical position) and order (horizontal position within rank), plus configurable spacing (`node_spacing` default 80px, `rank_spacing` default 120px).

**Phase 6: Edge Routing**

Edges are routed as orthogonal (Manhattan-style) paths with horizontal and vertical segments. Special cases:

- **Self-loops**: When source equals target, the edge routes as a rectangular loop extending to the right and back.
- **Parallel edges**: When multiple edges connect the same pair of nodes, each gets an incremental lateral offset so they're visually distinguishable.
- **Reversed edges**: Edges that were reversed for cycle-breaking are flagged (`reversed: true`) so renderers can draw them with dashed or highlighted styling.

**Phase 7: Post-Processing**

Cluster boundaries are computed to enclose their member nodes with configurable padding (default 52px). All coordinates are normalized to non-negative values. Edge length metrics (`total_edge_length`, `reversed_edge_total_length`) are computed for quality analysis.

### Layout Guardrails

For very large diagrams, the layout engine enforces time, iteration, and routing operation budgets:

| Budget | Default | When Exceeded |
|--------|---------|---------------|
| **Time** | 250 ms | Falls back to a faster algorithm (e.g., Tree instead of Sugiyama) |
| **Iterations** | ~1000 | Skips refinement phases (transpose/sifting) |
| **Route operations** | ~10,000 | Simplifies edge routing |

The guardrail system estimates costs *before* running layout and proactively selects a cheaper algorithm if needed. The `LayoutGuardDecision` struct in the trace records what happened: `initial_algorithm`, `selected_algorithm`, whether `fallback_applied`, and the `reason`.

The fallback chain tries alternatives in order of preference:

```
Sugiyama → Tree → Grid (cheapest)
Force → Tree → Grid
Radial → Tree → Sugiyama
```

This ensures that even 10,000-node graphs produce output in bounded time.

## How SVG Rendering Works

The SVG renderer turns a `DiagramLayout` into a complete SVG document with visual polish features that go well beyond basic rectangles and lines.

### Node Shape Library

The renderer supports 21 distinct node shapes, each implemented as a pure-geometry SVG path builder:

| Shape | Syntax | Visual |
|-------|--------|--------|
| Rectangle | `A[text]` | Standard box |
| Rounded | `A(text)` | Rounded corners |
| Stadium | `A([text])` | Pill shape (fully rounded ends) |
| Subroutine | `A[[text]]` | Double-bordered box |
| Diamond | `A{text}` | Rotated square |
| Hexagon | `A{{text}}` | Six-sided polygon |
| Circle | `A((text))` | Circular |
| Double Circle | `A(((text)))` | Concentric circles |
| Asymmetric | `A>text]` | Flag shape |
| Cylinder | `A[(text)]` | Database icon |
| Trapezoid | `A[/text\]` | Wider top |
| Inv. Trapezoid | `A[\text/]` | Wider bottom |
| Parallelogram | `A[/text/]` | Slanted |
| Inv. Parallelogram | `A[\text\]` | Reverse slant |
| Triangle | | Three-sided |
| Pentagon | | Five-sided |
| Star | | Five-pointed star |
| Cloud | `)text(` | Mindmap cloud |
| Tag | | Bookmark shape |
| Crossed Circle | | Circle with X |
| Note | | Folded-corner rectangle |

### Visual Effects System

**Gradients** come in three styles, all defined as reusable SVG `<defs>`:
- **Linear Vertical**: Top-to-bottom gradient with 3 stops (full opacity → 97% → 92% background blend)
- **Linear Horizontal**: Left-to-right with the same stops
- **Radial**: Center-weighted with a 0.8 radius, creating a subtle inner glow

**Drop Shadows** use an SVG `<filter>` with configurable offset (default 2px), blur radius (default 6px), and opacity (default 0.15). The shadow color defaults to a dark slate (`#0f172a`) but adapts to the active theme.

**Glow Effects** add a colored blur behind highlighted elements (blur radius 6px, opacity 0.35, default color `#3b82f6`). Used for interactive highlighting or emphasis.

**Cluster Backgrounds** are drawn as semi-transparent filled rectangles (default opacity 0.08) behind their member nodes, with a 10px rounded corner radius and the cluster title above.

### Theme System

The renderer ships with 10 theme presets:

| Theme | Character |
|-------|-----------|
| Default | Clean light background with blue accents |
| Dark | Dark background with bright node fills |
| Forest | Green-tinted organic palette |
| Neutral | Grayscale with minimal color |
| Corporate | Professional blue/gray tones |
| Neon | Dark background with vivid accent colors |
| Pastel | Soft muted colors |
| High Contrast | Maximum readability, WCAG compliant |
| Monochrome | Pure black and white |
| Blueprint | Technical drawing style on blue background |

Themes define 13 CSS custom properties (`--fm-bg`, `--fm-text-color`, `--fm-node-fill`, `--fm-node-stroke`, `--fm-edge-color`, `--fm-cluster-fill`, `--fm-cluster-stroke`, plus 8 accent colors). Mermaid-style `%%{init}%%` theme variable overrides (`primaryColor`, `lineColor`, `clusterBkg`, etc.) are mapped to these properties automatically.

### Accessibility

The SVG renderer includes built-in accessibility features:

- `<title>` and `<desc>` elements on the root `<svg>` for screen readers
- ARIA labels on node and edge groups
- `describe_diagram()`, `describe_node()`, and `describe_edge()` functions that generate human-readable descriptions
- Print-optimized CSS rules (accessible via `accessibility_css()`)
- Source span tracking: optional `data-fm-source-span` attributes linking SVG elements back to their source line/column

## How Terminal Rendering Works

The terminal renderer produces diagrams as text using Unicode box-drawing characters and sub-cell pixel rendering. It's designed for CI logs, SSH sessions, and quick previews without leaving the terminal.

### Sub-Cell Rendering

The key insight is that Unicode characters can represent more than one "pixel" per terminal cell. The renderer offers four fidelity modes:

| Mode | Resolution | Characters Used | Best For |
|------|-----------|-----------------|----------|
| **Braille** | 2×4 per cell | Unicode braille U+2800–U+28FF (256 patterns) | Highest resolution, smooth curves |
| **Block** | 2×2 per cell | Quarter blocks U+2596–U+259F (16 patterns: ▘ ▝ ▀ ▖ ▌ ▞ ▛ etc.) | Good balance of detail and compatibility |
| **HalfBlock** | 1×2 per cell | Half blocks ▀ ▄ █ (4 patterns) | Wide terminal compatibility |
| **CellOnly** | 1×1 per cell | Full block █ or space (2 patterns) | Maximum compatibility, lowest resolution |

In **Braille mode**, each terminal cell represents an 8-dot braille pattern where each dot maps to a sub-pixel. The 8 dots are arranged in a 2-wide × 4-tall grid, giving 8 sub-pixels per cell, the highest resolution achievable in a standard terminal. The renderer draws into a boolean pixel buffer using Bresenham's line algorithm and midpoint circle algorithm, then encodes the buffer into braille code points starting from U+2800.

### Rendering Tiers

Three detail tiers control how much visual information is shown:

| Tier | Node Style | Edge Style | Labels |
|------|-----------|------------|--------|
| **Compact** | Single character or small box | Minimal line segments | Abbreviated |
| **Normal** | Box-drawn rectangles with labels | Box-drawing line characters (─ │ ┌ ┐ └ ┘ ├ ┤) | Full text |
| **Rich** | Decorated boxes with shape hints | Styled edges with arrowheads (→ ← ↑ ↓) | Full text with wrapping |

### Diff Engine

The terminal renderer includes a structural diff engine for comparing two diagrams:

```bash
fm-cli diff before.mmd after.mmd --format term
```

The diff engine tracks changes at the element level:

- **Nodes**: Added, Removed, Changed (label, shape, classes, members), Unchanged
- **Edges**: Added, Removed, Changed (arrow type, label), Unchanged

Output shows a side-by-side comparison with color-coded change markers, plus aggregate counts (`3 added, 1 removed, 2 changed, 15 unchanged`).

### Minimap

For large diagrams that exceed the terminal viewport, the renderer can produce a scaled minimap, a compressed overview showing the overall structure:

```
┌──────────────────┐
│ ▄▀▄    ▄▀▄      │  ← Minimap (each braille cell = many nodes)
│ █▀█────█▀█──▄▀▄ │
│        ▀▀▀  █▀█ │
│    ┌──────┐      │  ← Viewport indicator
│    │      │      │
│    └──────┘      │
└──────────────────┘
```

The minimap auto-selects detail level based on density classification:
- **Sparse** (< 0.5 elements/pixel): Show every node and edge
- **Medium** (0.5–2.0 elements/pixel): Simplify dense areas
- **Dense** (> 2.0 elements/pixel): Coarse overview, edges as direct lines

## The Intermediate Representation

The `MermaidDiagramIr` is the central data structure that connects parsing to layout to rendering. Understanding it helps when debugging unexpected output or building tooling on top of frankenmermaid.

### Structure

```rust
MermaidDiagramIr {
    diagram_type: DiagramType,          // One of 24 types
    direction: GraphDirection,          // TB, LR, RL, BT
    nodes: Vec<IrNode>,                 // Each with shape, label, classes, href, span
    edges: Vec<IrEdge>,                 // Each with arrow type, label, span
    ports: Vec<IrPort>,                 // For ER diagram entity attributes
    clusters: Vec<IrCluster>,           // Visual grouping containers
    graph: MermaidGraphIr,              // Indexed graph view (adjacency)
    labels: Vec<IrLabel>,               // Interned text (shared by nodes/edges)
    subgraphs: Vec<IrSubgraph>,         // Hierarchical nesting
    constraints: Vec<IrConstraint>,     // Layout hints (same-rank, min-length)
    meta: MermaidDiagramMeta,           // Config, parse mode, theme overrides
    diagnostics: Vec<Diagnostic>,       // Warnings/errors with source spans
}
```

### Key Design Decisions

**Label interning**: Instead of storing label text directly on nodes and edges, labels are stored in a shared `Vec<IrLabel>` and referenced by `IrLabelId`. This avoids string duplication when the same label appears on multiple elements and makes label manipulation (normalization, wrapping) a single-point concern.

**Span tracking**: Every node, edge, label, and cluster carries a `Span` with byte offset, line, and column positions pointing back to the original input. This enables precise error reporting ("line 7, column 12: unknown node shape") and powers the source-span attributes in SVG output for click-to-source tooling.

**Implicit nodes**: Nodes referenced only in edges (never explicitly declared) are auto-created with `implicit: true`. This lets the parser accept terse input like `A --> B` without requiring `A[A]` and `B[B]` declarations first, matching mermaid-js behavior.

**Semantic edge kinds**: Edges carry an `IrEdgeKind` that encodes diagram-specific semantics beyond just the arrow type:

| Kind | Meaning | Used By |
|------|---------|---------|
| Generic | Standard directed/undirected connection | Flowchart, class, state |
| Relationship | ER relationship with cardinality | ER diagrams |
| Message | Sequence message with timing semantics | Sequence diagrams |
| Timeline | Temporal connection between events | Timeline, journey |
| Dependency | Task dependency with ordering | Gantt |
| Commit | Git commit parent/child link | GitGraph |

### Diagnostics

Diagnostics are rich structured objects:

```rust
Diagnostic {
    severity: DiagnosticSeverity,    // Hint, Info, Warning, Error
    category: DiagnosticCategory,    // Parse, Semantic, Recovery, Compatibility
    message: String,                 // Human-readable description
    span: Option<Span>,              // Source location
    suggestion: Option<String>,      // "Did you mean..."
}
```

Categories help tooling filter diagnostics:
- **Parse**: Syntax errors in the input
- **Semantic**: Valid syntax but questionable intent (e.g., duplicate node definitions)
- **Recovery**: Actions the parser took to recover from errors
- **Compatibility**: Features that work differently from mermaid-js

## Release Profile and Binary Size

The workspace is optimized for WASM deployment with a dual-profile release configuration:

```toml
[profile.release]
opt-level = "z"       # Optimize for binary size (WASM target)
lto = true            # Link-time optimization across all crates
codegen-units = 1     # Single codegen unit for maximum optimization
panic = "abort"       # No unwinding overhead
strip = true          # Remove debug symbols

[profile.release.package.fm-layout]
opt-level = 3         # Maximum performance for the layout engine
```

The layout crate gets `opt-level = 3` (maximum speed) instead of `opt-level = "z"` (minimum size) because layout is the computational bottleneck; crossing minimization and coordinate assignment dominate pipeline latency. Every other crate prioritizes small binary size for fast WASM delivery.

## Force-Directed Layout

For graphs where hierarchical layering isn't appropriate, the force-directed layout simulates a physical system where nodes repel each other and edges act as springs.

### Physics Model (Fruchterman-Reingold)

The simulation applies two forces per iteration:

- **Repulsive force** between all node pairs: `F = k² / distance` (inverse-distance, like electrical charge). Prevents node overlap.
- **Attractive force** along edges: `F = distance² / k` (Hooke's law). Pulls connected nodes together.

Where `k` is the ideal edge length, computed as `sqrt(area / node_count)`.

### Cooling Schedule

The simulation uses linear cooling: `temperature = t₀ × (1.0 - progress)` where `t₀ = k × 10.0`. The temperature limits how far nodes can move per iteration, preventing oscillation as the system converges.

### Iteration Budget

The number of iterations scales with graph size: `min(50 + n×2, 500)`. A 10-node graph runs 70 iterations; a 200-node graph runs the maximum 500.

### Cluster Cohesion

For graphs with clusters (subgraphs), an additional cohesion force pulls nodes toward their cluster centroid with strength 0.3. This keeps visually grouped nodes together without hard containment constraints.

### Barnes-Hut Optimization

For graphs with more than 100 nodes, the engine switches from O(n²) all-pairs force computation to a grid-based Barnes-Hut approximation:

- Grid size: `√n` cells per side
- Opening angle threshold: 1.5
- Within-cell interactions: computed exactly (direct summation)
- Cross-cell interactions: approximated using cell centroid

This reduces force computation from O(n²) to roughly O(n log n).

### Deterministic Initial Placement

Initial positions are computed from FNV-1a hashes of node IDs (prime: `0x0100_0000_01b3`, offset: `0xcbf2_9ce4_8422_2325`), laid out in a `⌈√n⌉`-column grid with ±30% jitter derived from hash bits. This ensures the same input always starts from the same initial state, which combined with IEEE 754 deterministic arithmetic, guarantees identical final positions.

## Tree and Radial Layouts

### Tree Layout (Reingold-Tilford Variant)

The tree layout uses a modified Reingold-Tilford algorithm:

1. **Root selection**: All nodes with in-degree 0. If there are multiple roots, they're treated as siblings of a virtual root.
2. **Depth assignment**: BFS from roots assigns each node to a level.
3. **Subtree span computation**: Bottom-up recursive calculation. Each node's span is `max(own_width, sum_of_children_spans)`.
4. **Coordinate assignment**: Children are centered under their parent. Siblings are spaced by `node_spacing`.
5. **Direction support**: TB (top-to-bottom, default), LR, RL, BT. The depth axis and breadth axis swap roles depending on direction.

### Radial Layout (Leaf-Weighted Angle Allocation)

For mindmaps and hierarchical structures that benefit from a radial arrangement:

1. **Leaf counting**: Memoized bottom-up count of leaf descendants per subtree.
2. **Angle allocation**: Each child receives an angular range proportional to its leaf count relative to its siblings' total leaf count.
3. **Ring radius**: Each depth level gets its own radius, growing outward. The radius increment accounts for the widest node at that level plus `rank_spacing`.
4. **Positioning**: Polar coordinates (angle, radius) are converted to Cartesian (x, y) for the final layout.
5. **Floating-point drift correction**: The last child's angle span is adjusted to exactly fill the remaining range, preventing gaps from accumulated rounding errors.

## Sankey and Specialized Chart Layouts

### Sankey Layout

The sankey layout arranges nodes in columns with flow bands proportional to edge values:

1. **Column assignment**: Nodes are layered by reachability from sources (nodes with no incoming edges).
2. **Height scaling**: Node heights are proportional to their total flow: `30 + max(in_degree, out_degree) × 14.0` pixels.
3. **Column spacing**: `rank_spacing + 136px` (extra margin for flow band rendering).
4. **Vertical ordering**: Within each column, nodes are ordered to minimize flow band crossings.

### Grid Layout (Block-Beta)

The grid layout provides CSS-grid-like positioning:

1. **Column count**: Read from the `columns N` directive, or defaults to `⌈√n⌉`.
2. **Cell sizing**: Each cell is `max_node_width + node_spacing` wide by `max_node_height + rank_spacing × 0.6` tall.
3. **Column spanning**: Blocks with `:N` suffix span N columns, getting width `base_width × N + spacing × (N-1)`.
4. **Space blocks**: `space` or `space:N` creates empty cells for visual gaps.
5. **Group nesting**: `block:id ... end` creates sub-grids within the parent grid.

## Canvas2D Web Rendering

The Canvas2D renderer provides an alternative to SVG for browser-based rendering, particularly suited for large diagrams and interactive use.

### Trait-Based Abstraction

The renderer is built around a `Canvas2dContext` trait with 35 methods covering:

| Category | Methods |
|----------|---------|
| **Path operations** | `begin_path`, `close_path`, `move_to`, `line_to`, `bezier_curve_to`, `arc` |
| **Drawing** | `fill`, `stroke`, `fill_rect`, `stroke_rect`, `clear_rect` |
| **Text** | `fill_text`, `stroke_text`, `measure_text` |
| **Style** | `set_fill_style`, `set_stroke_style`, `set_line_width`, `set_line_cap`, `set_line_join` |
| **Transform** | `save`, `restore`, `translate`, `scale`, `rotate`, `set_transform` |
| **Shadows** | `set_shadow_blur`, `set_shadow_color`, `set_shadow_offset_x/y` |

In WASM builds, this trait is implemented against `web_sys::CanvasRenderingContext2d`. For testing, a `MockCanvas2dContext` records all draw operations in a `Vec<DrawOperation>` without requiring a browser, enabling full render pipeline testing in CI.

### Viewport Transform

The viewport system provides automatic fit-to-container scaling:

- **Scale**: `min(container_width / diagram_width, container_height / diagram_height)`, clamped to never zoom beyond 100% for small diagrams
- **Centering**: The diagram is centered within the available space
- **Pan/zoom**: Point-preserving zoom (zooming toward the cursor position rather than the origin)

## DOT Format Bridge

The DOT parser (`dot_parser.rs`) enables Graphviz interop by converting DOT syntax to the shared Mermaid IR:

### Supported DOT Features

| Feature | DOT Syntax | IR Mapping |
|---------|-----------|------------|
| Directed graph | `digraph G { ... }` | `DiagramType::Flowchart` with `ArrowType::Arrow` |
| Undirected graph | `graph G { ... }` | `DiagramType::Flowchart` with `ArrowType::Line` |
| Node declaration | `node_id [label="text"]` | `IrNode` with label |
| Edge declaration | `A -> B -> C` | Two `IrEdge` entries (chaining supported) |
| Subgraph | `subgraph cluster_X { ... }` | `IrSubgraph` + `IrCluster` |
| Anonymous subgraph | `{ A B }` | Cluster with auto-generated ID |
| Attribute lists | `[label="...", shape=box]` | Label extracted, other attributes as classes |
| HTML labels | `[label=<b>bold</b>]` | HTML stripped, text preserved |
| Comments | `// line` and `/* block */` | Stripped during pre-processing |
| Escape sequences | `\n`, `\t`, `\"`, `\\` | Decoded in string values |

### Identifier Rules

DOT identifiers are normalized: only alphanumeric characters, `_`, `-`, `.`, and `/` are kept. Leading quotes are stripped. This ensures DOT node IDs map cleanly to the Mermaid IR's string-based node identity system.

## Font Metrics and Text Measurement

Since the layout and rendering engines need to know how wide text will be (for node sizing, label placement, and wrapping) but don't have access to a real font renderer, they use a heuristic character-width model.

### Character Width Classes

Each character is classified into one of six width classes:

| Class | Multiplier | Characters |
|-------|-----------|------------|
| Very Narrow | 0.4× | `i l \| ! ' . , : ;` |
| Narrow | 0.6× | `I j t f r ( ) [ ]` |
| Half | 0.5× | space |
| Normal | 1.0× | Most characters (a-z, 0-9, etc.) |
| Wide | 1.2× | `w m` |
| Very Wide | 1.5× | `W M @ % &` |

### Font Family Presets

Different font families have different average character-to-pixel ratios:

| Family | Avg Char Ratio | Used When |
|--------|---------------|-----------|
| System UI / Sans-Serif | 0.55 | Default (Inter, -apple-system) |
| Monospace | 0.60 | Code labels |
| Serif | 0.52 | Document-style diagrams |
| Condensed | 0.45 | Dense layouts |

### Measurement Algorithm

Width estimation: `Σ(char_width × class_multiplier × avg_char_ratio × font_size)` for each character in the string. For multi-line text, the width is the maximum line width.

Height estimation: `line_count × font_size × line_height` (default line_height: 1.5).

### Text Wrapping

The engine uses greedy word-fit wrapping: words are placed on the current line until the next word would exceed the target width. If a single word is wider than the target, it's placed on its own line (overflow allowed on line start). This is used for node labels and edge labels that exceed their container width.

### Truncation

When text must fit a fixed width, characters are removed from the end and replaced with "..." (ellipsis). The truncation point is found by character-by-character measurement until the remaining text plus ellipsis fits the target width.

## Diagram-Specific Parser Deep Dives

### ER Diagram: 14 Cardinality Operators

The ER parser recognizes 14 distinct cardinality operators, each encoding a specific relationship type:

| Operator | Meaning | Line Style |
|----------|---------|------------|
| `\|\|--o{` | One-to-many (optional many) | Solid |
| `\|\|--\|{` | One-to-many (required many) | Solid |
| `}\|--\|\|` | Many-to-one (required) | Solid |
| `}o--\|\|` | Many-to-one (optional many) | Solid |
| `\|o--o\|` | One-to-one (both optional) | Solid |
| `\|\|--\|\|` | One-to-one (both required) | Solid |
| `o\|--\|{` | Optional-one to many | Solid |
| `}\|--\|{` | Many-to-many | Solid |
| `o\|--\|\|` | Optional-one to one | Solid |
| `}\|..\|{` | Many-to-many | Dotted |
| `\|\|..\|\|` | One-to-one | Dotted |
| `o\|..\|{` | Optional-one to many | Dotted |
| `\|o..\|{` | One-optional to many | Dotted |
| `}o--o{` | Many-optional to many-optional | Solid |

The parser finds the operator position in the relationship string, splits into left entity and right entity, and maps the operator to an `ArrowType`. Dotted operators (containing `..`) produce `ArrowType::DottedArrow`; solid operators produce `ArrowType::Arrow`.

### GitGraph: Stateful Branch Tracking

The gitGraph parser maintains a `GitGraphState` struct that tracks:

- **Branch heads**: `BTreeMap<String, IrNodeId>` mapping branch names to their current head commit
- **Current branch**: Defaults to `"main"`, changes with `checkout`/`switch`
- **Commit counter**: Auto-increments to generate IDs (`commit_1`, `commit_2`, ...)

Each `commit` statement creates a node on the current branch and an edge from the previous commit. `branch` creates a new branch pointing at the current HEAD. `merge` creates a commit with two parent edges. `cherry-pick` creates a commit with an edge from the specified source commit.

### Mindmap: Indentation-Based Hierarchy

The mindmap parser uses indentation depth to determine parent-child relationships:

1. Count leading spaces for each line to determine depth.
2. Maintain an ancestry stack indexed by depth.
3. Each node's parent is the nearest ancestor at depth-1.
4. Shape is determined by bracket syntax: `[text]` = rect, `(text)` = rounded, `((text))` = circle, `{{text}}` = hexagon, `)text(` = cloud, `))text((` = bang.
5. `::icon(name)` directives attach icon metadata to the preceding node.

### Block-Beta: Column Span Parsing

The block-beta parser supports CSS-grid-like column spanning:

```
block-beta
  columns 3
  A["Wide Block"]:2    %% spans 2 columns
  B["Normal"]          %% spans 1 column
  space                %% empty cell
  C["Full Width"]:3    %% spans all 3 columns
```

The `:N` suffix after a block declaration sets `grid_span = N` on the node. The grid layout then computes the block's width as `base_width × N + spacing × (N-1)`, effectively merging N adjacent cells.

## Node Sizing Model

The layout engine needs to know how much space each node occupies before it can position anything. Since frankenmermaid doesn't have access to a browser font renderer at layout time, it uses a heuristic model.

### Sizing Formula

```
node_width  = max(label_width + 72.0, 100.0)
node_height = max(label_height + 44.0, 52.0)
```

The 72px horizontal padding (36px per side) and 44px vertical padding (22px per side) give labels breathing room inside node shapes. The minimums (100px wide, 52px tall) ensure even empty or single-character nodes are large enough to be visually meaningful and clickable.

Label dimensions come from the font metrics system (see "Font Metrics and Text Measurement" above). Shape does not affect the bounding box; a diamond and a rectangle with the same label get the same allocated space. The shape is drawn within the bounding box at render time.

### Spacing Constants

| Constant | Default | Purpose |
|----------|---------|---------|
| `node_spacing` | 80px | Horizontal gap between adjacent nodes in the same rank |
| `rank_spacing` | 120px | Vertical gap between ranks (layers) |
| `cluster_padding` | 52px | Padding inside cluster/subgraph boundaries (all 4 sides) |

These are configurable via `LayoutSpacing` but the defaults are tuned for readable diagrams at typical screen sizes.

## Security Model

frankenmermaid processes untrusted input (user-provided diagram text) and produces output that may be embedded in web pages (SVG). The security model addresses injection attacks at multiple layers.

### XML/SVG Injection Prevention

All text content passes through escape functions before being embedded in SVG:

| Context | Escapes | Why |
|---------|---------|-----|
| XML attributes | `& < > " '` → `&amp; &lt; &gt; &quot; &#39;` | Prevents attribute breakout |
| XML text content | `& <` → `&amp; &lt;` | Prevents element injection. `>` is intentionally NOT escaped to preserve CSS child combinators in embedded stylesheets |
| CSS tokens | Strip everything except `[a-z0-9_-]` | Prevents CSS injection via class names |

SVG elements are constructed programmatically (not string-concatenated), so there is no path for injecting arbitrary SVG elements through diagram input.

### Link Sanitization

Links are disabled by default (`enable_links: false` in `MermaidConfig`). When enabled, the `MermaidSanitizeMode` controls behavior:

| Mode | Behavior |
|------|----------|
| **Strict** (default) | Links must pass URL scheme validation. `javascript:`, `vbscript:`, `data:`, `file:`, `blob:` schemes are blocked. Only `http:`, `https:`, and relative URLs are allowed. |
| **Lenient** | All URL schemes are permitted. Use only in trusted environments. |

The `MermaidLinkMode` further restricts which links are rendered:

| Mode | Effect |
|------|--------|
| `Off` | No links rendered regardless of `enable_links` |
| `Local` | Only relative URLs and same-origin links |
| `External` | Only absolute URLs |
| `All` | Both local and external |

### Input Limits

The `MermaidConfig` enforces input size limits to prevent denial-of-service via pathological diagrams:

| Limit | Default | Effect When Exceeded |
|-------|---------|---------------------|
| `max_nodes` | 200 | Degradation warning, reduced visual fidelity |
| `max_edges` | 400 | Degradation warning, simplified edge routing |
| `max_label_chars` | 48 | Labels truncated with "..." |
| `max_label_lines` | 3 | Multi-line labels capped |
| `max_input_bytes` | 5,000,000 | Parse refused |

## Determinism Guarantees

Deterministic output is an explicit design goal. Here are the concrete engineering choices that make it work.

### Ordered Data Structures

The codebase uses `BTreeMap` and `BTreeSet` everywhere, never `HashMap` or `HashSet`. Hash maps iterate in arbitrary (seed-dependent) order, which would make layout output depend on the hash seed. B-tree maps iterate in key order, which is deterministic.

### Stable Node Ordering

Before any layout phase that depends on node order, nodes are sorted by a stable priority function:

```
Primary: node ID (string comparison)
Secondary: node index (declaration order)
```

This means two diagrams with the same nodes and edges always produce the same layout, regardless of the order nodes were declared.

### Floating-Point Discipline

IEEE 754 arithmetic is deterministic for identical inputs on the same platform. The codebase avoids operations that could introduce platform-dependent results:

- No use of `f32::sin`/`cos` in layout-critical paths (these can differ across libm implementations)
- Explicit drift correction: after allocating angular spans in radial layout, the last span is adjusted to exactly fill the remaining range, preventing accumulated rounding errors from producing gaps
- Epsilon-based comparisons (`0.001`) for collinearity tests, avoiding exact float equality

### Verification

The test suite includes explicit determinism checks:

```rust
#[test]
fn traced_layout_is_deterministic() {
    let ir = sample_ir();
    let first = layout_diagram_traced(&ir);
    let second = layout_diagram_traced(&ir);
    assert_eq!(first, second); // Bit-for-bit equality
}
```

Property-based tests verify determinism across random graph shapes (up to 20 nodes × 5 directions = 100 combinations per test run).

## The Diff Engine

The terminal renderer includes a structural diff engine for comparing two diagram versions.

### How Diffing Works

1. **Parse both diagrams** independently into `MermaidDiagramIr`
2. **Match nodes** by ID using a `BTreeMap<&str, (usize, &IrNode)>` lookup
3. **Classify each node** as Added (only in new), Removed (only in old), Changed (both but different), or Unchanged
4. **Match edges** by `(from_id, to_id)` key pair using `BTreeMap`
5. **Classify each edge** similarly

### Change Detection

For nodes classified as "Changed", the engine identifies exactly what changed:

| Change Type | Detected When |
|-------------|---------------|
| `LabelChanged` | Node text differs between old and new |
| `ShapeChanged` | Node shape (rect → diamond, etc.) differs |
| `ClassesChanged` | Applied CSS classes differ |
| `MembersChanged` | ER entity attributes differ |

For edges:

| Change Type | Detected When |
|-------------|---------------|
| `ArrowChanged` | Arrow type (solid → dashed, etc.) differs |
| `LabelChanged` | Edge label text differs |

### Output Formats

```bash
# Side-by-side terminal diff with ANSI colors
fm-cli diff old.mmd new.mmd --format terminal

# Machine-readable JSON
fm-cli diff old.mmd new.mmd --format json

# Summary counts only
fm-cli diff old.mmd new.mmd --format summary

# Plain text (no colors)
fm-cli diff old.mmd new.mmd --format plain
```

The terminal format uses color coding: green for added, red for removed, yellow for changed, gray for unchanged.

## The Validate Pipeline

The `fm-cli validate` command runs 4 diagnostic collection stages and produces a sorted, deduplicated report.

### Validation Stages

| Stage | What It Checks |
|-------|----------------|
| **Parse** | Parser warnings, init directive errors, structured diagnostics from IR, unstructured recovery warnings |
| **Structural** | Unknown diagram type, empty diagram (no nodes and no edges) |
| **Layout** | Algorithm capability unavailable for diagram type, guardrail fallback applied, cycles detected and edges reversed |
| **Render** | SVG envelope validation (output starts with `<svg` and ends with `</svg>`) |

### Fail Threshold

The `--fail-on` flag controls which severity level causes a non-zero exit code:

```bash
fm-cli validate input.mmd --fail-on warning   # Exit 1 if any warnings
fm-cli validate input.mmd --fail-on error      # Exit 1 only on errors (default)
fm-cli validate input.mmd --fail-on hint       # Exit 1 on anything
fm-cli validate input.mmd --fail-on none       # Always exit 0
```

### Diagnostic Sorting

Diagnostics are sorted by 6 keys for consistent output:

1. Severity (errors first, then warnings, info, hints)
2. Source line number
3. Source column number
4. Validation stage name
5. Error code
6. Message text

## Property-Based Testing

frankenmermaid uses [proptest](https://github.com/proptest-rs/proptest) to verify invariants that must hold for ALL inputs, not just hand-picked test cases. Each test run generates 48-64 random inputs and checks that invariants are never violated.

### Layout Invariants

**Determinism**: For any random chain graph (1-20 nodes, any of 5 directions), `layout(graph) == layout(graph)`. Running layout twice on the same input produces bit-identical output.

**Non-overlapping**: No two nodes in the output have overlapping bounding boxes (within floating-point tolerance).

**Non-negative stats**: `total_edge_length >= 0.0` and `reversed_edge_total_length >= 0.0` and `bounds.width >= 0.0` and `bounds.height >= 0.0` for any input.

### SVG Render Invariants

**Totality**: `render_svg(ir)` always produces valid SVG (starts with `<svg`, ends with `</svg>`) for any IR, including empty diagrams.

**Count accuracy**: The SVG output contains `data-nodes="N"` and `data-edges="M"` attributes matching the actual node and edge counts in the IR.

### Terminal Render Invariants

**Bounds enforcement**: `render_term(ir, cols, rows)` always produces output where `output.width <= cols` and `output.height <= rows`, regardless of diagram size. The renderer scales down rather than overflow.

### Parser Invariants

**Totality**: `parse(input)` never panics for any input string (tested with random strings up to 256 characters including non-ASCII, control characters, and adversarial patterns).

**Confidence bounds**: The confidence score is always in `[0.0, 1.0]`.

**Serde roundtrip**: `deserialize(serialize(ir)) == ir`. The IR survives JSON serialization and deserialization without data loss.

## Scaling Characteristics

The engine is designed for diagrams in the 1-500 node range (typical documentation diagrams). Here's how the major phases scale:

| Phase | Complexity | 10 nodes | 100 nodes | 1000 nodes |
|-------|-----------|----------|-----------|------------|
| Parsing | O(n) | <1ms | <1ms | ~5ms |
| Cycle removal | O(V+E) | <1ms | <1ms | ~2ms |
| Rank assignment | O(V+E) | <1ms | <1ms | ~3ms |
| Crossing minimization | O(E × sweeps) | <1ms | ~5ms | ~200ms |
| Coordinate assignment | O(V) | <1ms | <1ms | ~1ms |
| Edge routing | O(E) | <1ms | <1ms | ~5ms |
| SVG rendering | O(V+E) | <1ms | ~2ms | ~15ms |
| **Total pipeline** | | **<5ms** | **~10ms** | **~230ms** |

The crossing minimization phase dominates for large graphs because it performs multiple sweeps over all edges. The layout guardrails (250ms budget) automatically fall back to simpler algorithms when this phase threatens to exceed the budget.

For very large diagrams (1000+ nodes), the force-directed layout with Barnes-Hut optimization may be a better choice than Sugiyama, since its O(n log n) force computation avoids the quadratic crossing count.

## The Render Scene (Target-Agnostic IR)

Between layout and the final render backends (SVG, terminal, Canvas), there is an intermediate **render scene** that abstracts away backend specifics. This allows new render targets to be added without touching layout code.

### Scene Structure

```
RenderScene
├── bounds: RenderRect
└── root: RenderGroup (id="diagram-root")
    ├── transform: identity matrix
    ├── clip: RenderClip::Rect(bounds)
    └── children: [RenderItem]
        ├── Cluster layer (backgrounds + titles)
        ├── Edge layer (paths with arrowheads)
        ├── Node layer (shapes with fills/strokes)
        └── Label layer (text elements)
```

Each `RenderItem` is one of:
- **Group**: Container with optional transform (6-component affine matrix `[a,b,c,d,e,f]`) and clip region
- **Path**: SVG-style path commands (`MoveTo`, `LineTo`, `BezierTo`, `ArcTo`, `Close`) with fill/stroke
- **Text**: Positioned text with font metrics, alignment, and optional rotation

Every render item carries a `RenderSource` tag indicating what it represents (Node, Edge, Cluster, Label), enabling backends to apply type-specific styling without parsing the geometry.

## Init Directives and Configuration Merging

frankenmermaid supports Mermaid-compatible inline configuration via `%%{init: {...}}%%` directives at the start of a diagram.

### Supported Init Variables

| Variable | Type | Effect |
|----------|------|--------|
| `theme` | string | Selects theme preset (default, dark, forest, neutral, etc.) |
| `themeVariables.primaryColor` | color | Overrides primary node fill color |
| `themeVariables.lineColor` | color | Overrides edge/line color |
| `themeVariables.clusterBkg` | color | Overrides cluster background |
| `flowchart.rankDir` / `flowchart.direction` | LR/TB/RL/BT | Sets graph direction |
| `flowchart.curve` | basis/linear/step | Edge curve interpolation style |
| `sequence.mirrorActors` | bool | Show actors at bottom too |
| `securityLevel` | strict/loose | Controls link/script sanitization |

### Config Merge Order

Configuration is resolved in priority order (highest wins):

1. **Per-call overrides** (CLI flags like `--theme dark`)
2. **Inline `%%{init}%%` directive** in the diagram text
3. **Config file** (`frankenmermaid.toml` via `--config`)
4. **Built-in defaults**

This means a diagram with `%%{init: {"theme":"dark"}}%%` will use the dark theme even if the config file says `theme = "corporate"`, but a CLI flag `--theme forest` overrides both.

## The Parser IR Builder

The parser doesn't construct the `MermaidDiagramIr` directly. It uses an `IrBuilder` that provides deduplication, normalization, and recovery services.

### Node Interning

When the parser encounters a node reference (e.g., `A` in `A --> B`), it calls `intern_node("A", ...)`. The builder:

1. Checks `node_index_by_id` (a `BTreeMap<String, IrNodeId>`) for an existing node with that ID.
2. **If found**: Returns the existing `IrNodeId`. If the new reference provides a label or shape that the existing node lacks, updates the existing node in-place. This is how `A --> B` followed by `A[Start]` correctly assigns the label "Start" to node A.
3. **If not found**: Creates a new `IrNode`, appends it to `ir.nodes`, registers in the dedup map, and returns the new ID.

This interning approach means parsers don't need to worry about whether a node was already declared. References and declarations are unified automatically.

### Cluster and Subgraph Construction

Clusters (visual grouping boxes) and subgraphs (hierarchical nesting) are created via `ensure_cluster()` and `ensure_subgraph()`, which maintain bidirectional consistency: every update to `ir.clusters` is mirrored in `ir.graph.clusters`, and subgraph parent/child relationships are maintained on both sides.

### Label Interning

Labels are stored in a shared `Vec<IrLabel>` and referenced by `IrLabelId`. The `intern_label()` method creates the label entry and returns the ID. This avoids string duplication when the same text appears on multiple elements.

### Dangling Edge Recovery

When an edge references a node that was never declared (common in terse input), the builder auto-creates a placeholder node with `implicit: true`. After parsing completes, `apply_semantic_recovery()` emits diagnostic warnings for each auto-created node:

```
Warning [recovery]: Node "UndeclaredNode" was referenced in an edge but never
declared. A placeholder node was created automatically. Consider adding an
explicit declaration.
```

## The Diagnostic System

Diagnostics are first-class structured objects designed to be consumed by both humans and tooling.

### Diagnostic Fields

```rust
Diagnostic {
    severity: Hint | Info | Warning | Error,
    category: Lexer | Parser | Semantic | Recovery | Inference | Compatibility,
    message: "Human-readable description",
    span: Some(Span { start: Position { line, col, byte }, end: ... }),
    suggestion: Some("Did you mean 'flowchart'?"),
    expected: vec!["-->", "->>", "---"],     // For parse errors
    found: Some("==>"),                       // What was actually found
    related: vec![RelatedDiagnostic { ... }], // Multi-location context
}
```

### Structured Output Format

For automation, diagnostics can be serialized as `StructuredDiagnostic`:

```json
{
  "error_code": "mermaid/diag/recovery",
  "severity": "warning",
  "message": "Node 'X' auto-created as placeholder",
  "source_line": 7,
  "source_column": 12,
  "rule_id": "implicit-node",
  "confidence": 0.85,
  "remediation_hint": "Add explicit node declaration: X[Label]"
}
```

### Diagnostic Categories

| Category | When Emitted | Example |
|----------|-------------|---------|
| **Lexer** | Tokenization problems | Invalid character in identifier |
| **Parser** | Syntax errors | Expected `-->` but found `==>` |
| **Semantic** | Valid syntax, questionable intent | Duplicate node definition with conflicting labels |
| **Recovery** | Parser took corrective action | Auto-created placeholder node for dangling edge |
| **Inference** | Intent was inferred from ambiguous input | Fuzzy-matched `flowchrt` to `flowchart` |
| **Compatibility** | Behavior differs from mermaid-js | Feature works but produces different visual output |

## MermaidConfig: Runtime Behavior Controls

The `MermaidConfig` struct controls parser, layout, and rendering behavior. Key fields and their defaults:

### Input Limits

| Field | Default | Purpose |
|-------|---------|---------|
| `max_nodes` | 200 | Maximum node count before degradation warnings |
| `max_edges` | 400 | Maximum edge count before degradation warnings |
| `max_label_chars` | 48 | Truncate labels beyond this length |
| `max_label_lines` | 3 | Maximum lines in wrapped labels |

### Layout Budgets

| Field | Default | Purpose |
|-------|---------|---------|
| `layout_iteration_budget` | 200 | Maximum crossing-minimization iterations |
| `route_budget` | 4,000 | Maximum edge routing operations |

### Security

| Field | Default | Purpose |
|-------|---------|---------|
| `sanitize_mode` | Strict | Controls URL/script sanitization. Strict blocks `javascript:`, `vbscript:`, `data:`, `file:`, `blob:` schemes |
| `enable_links` | false | Whether `click` directives produce clickable elements |
| `link_mode` | Off | Off / Local / External / All; controls which URLs are allowed |

### Rendering

| Field | Default | Purpose |
|-------|---------|---------|
| `glyph_mode` | Unicode | Unicode / Ascii / Block; character set for terminal rendering |
| `wrap_mode` | WordChar | WordChar / Word / Char; text wrapping strategy |
| `edge_bundling` | false | Merge parallel edges into bundles (min count: 3) |
| `enable_styles` | true | Whether style/classDef directives are processed |

### Degradation Control

When input exceeds `max_nodes` or `max_edges`, the engine produces a `MermaidGuardReport` describing the degradation plan: which visual features to reduce (e.g., disable shadows, simplify edges, use compact tier) to maintain performance. This is a graceful degradation path, not a hard failure.

## The Golden Test System

Golden tests verify rendering stability: the same input must produce byte-identical SVG output across commits. This catches unintentional visual regressions.

### How It Works

1. **Input files** (`tests/golden/*.mmd`) contain representative Mermaid diagrams
2. **Golden snapshots** (`tests/golden/*.svg`) contain the expected SVG output
3. The test harness parses, lays out, and renders each input
4. The rendered SVG is normalized (line endings, trailing newline)
5. An **FNV-1a hash** of the rendered output is compared to the hash of the golden file
6. If hashes differ, the test fails with a diff

### Test Cases

| Case | What It Validates |
|------|-------------------|
| `flowchart_simple` | Basic 4-node graph with edges |
| `flowchart_cycle` | Graph with cycles (tests cycle-breaking stability) |
| `sequence_basic` | Sequence diagram with participants and messages |
| `class_basic` | Class diagram with inheritance |
| `state_basic` | State diagram with transitions |
| `gantt_basic` | Gantt chart with sections and tasks |
| `pie_basic` | Pie chart with slices |
| `malformed_recovery` | Intentionally broken input (tests graceful degradation) |

### Blessing New Snapshots

When you intentionally change rendering (e.g., improve layout quality), update the golden files:

```bash
BLESS=1 cargo test -p fm-cli --test golden_svg_test
```

This overwrites the `.svg` files with the new output. The test then passes because the hashes match. Always review the diff before committing blessed snapshots.

### Evidence Logging

Each golden test emits structured JSON evidence:

```json
{
  "scenario_id": "flowchart_simple",
  "input_hash": "660b25ea28c56e64",
  "surface": "cli-integration",
  "renderer": "svg",
  "parse_ms": 0, "layout_ms": 0, "render_ms": 0,
  "node_count": 4, "edge_count": 4,
  "layout_width": 672.2, "layout_height": 317.0,
  "output_artifact_hash": "fa91f6d45178a254",
  "degradation_tier": "full",
  "pass_fail_reason": "matched-golden"
}
```

This makes test results auditable and debuggable without re-running.

## Quality and Testing

- **200+ unit tests** across parser, core, layout, and render crates
- **Integration tests** for full parse → layout → render pipeline round-trips
- **Golden SVG snapshots** for regression safety (8 diagram types, blessed with `BLESS=1`)
- **Property-based tests** (proptest) for parser and layout invariants
- **Determinism checks**: same input verified to produce identical output across runs
- **Clippy pedantic + nursery** lints enabled workspace-wide with `-D warnings`
- **Zero unsafe code** enforced via `#![forbid(unsafe_code)]`

```bash
# Run the full quality gate
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo test --workspace
```

## Deployment and Rollback

### Demo Surfaces

The project has two deployment targets:

1. **GitHub Pages** (`dicklesworthstone.github.io/frankenmermaid/`): Deployed automatically on push to `main` via `.github/workflows/pages.yml`. Includes the showcase HTML, WASM binary, and web/web_react host pages.

2. **Cloudflare Pages**: Deployed via the CI pipeline with staged smoke checks in `.github/workflows/ci.yml`.

### Post-Deploy Verification

Both deployment paths include automated smoke checks:
- GitHub Pages: verifies HTTP 200 for root page and WASM artifact after deploy
- Cloudflare: runs `scripts/cloudflare_pages_ops.py smoke-check` before promoting

### Rollback Procedures

**GitHub Pages rollback:**
```bash
# Revert to the previous deployment by re-running the workflow on a known-good commit
gh workflow run "Deploy GitHub Pages" --ref <known-good-commit-sha>

# Or revert the commit and push
git revert HEAD
git push origin main
```

**Cloudflare Pages rollback:**
```bash
# Roll back to previous deployment via Cloudflare dashboard or CLI
# Cloudflare Pages keeps all deployment versions; select the previous one in the dashboard
```

**WASM package rollback:**
```bash
# Rebuild from a known-good commit
git checkout <known-good-sha>
bash build-wasm.sh
# Re-publish to npm if needed
```

## Troubleshooting

### `fm-cli: command not found`

```bash
# Check if installed
which fm-cli

# If installed via cargo, ensure cargo bin is in PATH
export PATH="$HOME/.cargo/bin:$PATH"

# If installed via curl script, check ~/.local/bin
export PATH="$HOME/.local/bin:$PATH"
```

### WASM package builds but browser demo is blank

```bash
# Rebuild WASM artifacts
./build-wasm.sh

# Serve over HTTP (file:// won't work for WASM)
python3 -m http.server 4173
```

### Labels overlap on dense graphs

Increase spacing or switch layout algorithm:

```bash
fm-cli render dense.mmd --format svg --config frankenmermaid.toml
```

In your config, try `layout.algorithm = "force"` and increase `node_spacing` / `rank_spacing`.

### Large diagrams feel slow in browser

Switch to Canvas backend and disable visual effects:

```toml
[svg]
shadows = false
gradients = false
```

For 1000+ node graphs, use the Canvas2D backend via the WASM API rather than SVG.

### Output differs from mermaid-js screenshot

frankenmermaid is not a pixel-for-pixel clone of mermaid-js. It uses its own layout algorithms that often produce better results, but will differ from upstream. Check diagnostics:

```bash
fm-cli validate input.mmd --verbose
fm-cli detect input.mmd --json
```

### Diagram type detected wrong

Check with explicit detection:

```bash
fm-cli detect input.mmd --json
```

If fuzzy matching picked the wrong type, add the explicit keyword header (e.g., `flowchart LR` instead of just starting with `A --> B`).

## Limitations

- **XyChart** is the only diagram type marked unsupported. It parses but lacks dedicated layout and rendering. Tracked for implementation.
- **Sequence diagram advanced features** (activation boxes, interaction fragments, notes) are not yet implemented. Basic participant/message flow works.
- **classDef / style directives** are applied in SVG output; terminal/canvas renderers still use theme defaults.
- **Very large SVGs** (10k+ nodes) can be heavy for browsers. Use the Canvas2D backend via WASM for interactive exploration of large graphs.
- **PNG export** rasterizes the SVG output. CSS animations and hover effects are not preserved in static PNGs.
- **WebGPU backend** is planned but not yet available. Canvas2D is the current web rendering path.
- Some niche Mermaid syntax may parse with warnings rather than producing identical output to mermaid-js.

## FAQ

### Is this a fork of mermaid-js?

No. It is a clean Rust implementation with its own parser, layout engine, and render pipeline. It reads the same Mermaid syntax but shares no code with mermaid-js.

### Can I migrate from `mermaid.initialize(...)` configs?

Yes. `frankenmermaid` accepts Mermaid-style `%%{init: {...}}%%` directives and maps them to native config keys.

### Does it handle malformed diagrams?

Yes. The parser is explicitly designed to recover and produce best-effort output with diagnostics. It never panics on bad input.

### Which output format should I use?

| Use Case | Format |
|----------|--------|
| Documentation / web embedding | `svg` |
| Static image sharing | `png` (requires `--features png`) |
| CI logs / terminal preview | `term` |
| Large interactive browser views | Canvas2D via WASM API |
| Tooling integration | `json` (IR output from `parse`) |

### Is output deterministic for CI snapshots?

Yes. Deterministic tie-breaking and stable pipeline behavior are explicit design goals. The golden test suite verifies this.

### What is `legacy_mermaid_code/` in this repo?

A syntax and behavior reference corpus (including mermaid-js source/docs). Not a port target; used only for edge-case validation.

### How does the layout algorithm get chosen?

When `algorithm = "auto"` (the default), the engine selects based on diagram type:

| Diagram Type | Algorithm |
|---|---|
| flowchart, class, state, ER, requirement | Sugiyama (hierarchical) |
| mindmap | Radial tree |
| timeline | Timeline (linear horizontal) |
| gantt | Gantt (time-axis bar chart) |
| sankey | Sankey (flow-conserving columns) |
| journey, kanban | Kanban (column-based) |
| block-beta | Grid |
| sequence | Sequence (participants + messages) |
| All others | Sugiyama (default) |

You can override with `--layout <algorithm>` or in config.

### What cycle strategy should I use?

| Strategy | Best For |
|----------|----------|
| `greedy` | Fast, good enough for most graphs |
| `dfs-back` | Predictable back-edge selection |
| `mfas` | Minimum reversed edges (better visual quality) |
| `cycle-aware` | Full SCC detection with cluster collapse (best quality, slightly slower) |

### How does the Sugiyama layout handle cycles?

Directed graphs with cycles can't be drawn in layers. The engine temporarily reverses selected edges to break cycles, runs the full layout, then marks those edges as `reversed: true` in the output. Renderers can draw reversed edges with dashed lines or special styling to indicate back-edges. The `cycle-aware` strategy additionally detects strongly connected components and can collapse them into visual clusters.

### What happens with very large diagrams?

The layout guardrails kick in automatically. Before running layout, the engine estimates the computational cost based on node count, edge count, and the selected algorithm. If the estimate exceeds the time budget (default 250ms), it falls back to a cheaper algorithm (for example, Tree instead of Sugiyama). The fallback chain ensures that even 10,000-node graphs produce output in bounded time, at the cost of potentially lower visual quality.

### Can I use the IR directly for tooling?

Yes. `fm-cli parse --format json` emits the full intermediate representation as JSON, including nodes, edges, clusters, labels, diagnostics, and metadata. This is designed for editor integrations, diagram linters, and downstream tooling that wants to consume diagram structure without reimplementing parsing.

### How does the braille terminal rendering work?

Each terminal cell represents a 2x4 grid of sub-pixels using Unicode braille characters (U+2800-U+28FF). The renderer draws into a boolean pixel buffer using Bresenham's line algorithm, then encodes 8-pixel blocks into single braille code points. This gives an effective resolution of 2x the terminal width and 4x the terminal height, enough for smooth diagonal lines and curves.

### Why Rust instead of JavaScript?

Three reasons. (1) Determinism: Rust's lack of garbage collection pauses and its deterministic floating-point behavior make output stability achievable. (2) Performance: the layout engine does O(n^2 log n) work for crossing minimization; Rust runs this 10-50x faster than equivalent JS. (3) WASM: Rust compiles to compact WASM with no runtime dependencies, so the same code runs natively for CLI and in-browser via npm.

### How does DOT format support work?

The DOT bridge parser (`dot_parser.rs`) recognizes Graphviz `digraph` and `graph` declarations, extracts nodes and edges with their attributes, and converts them to `MermaidDiagramIr` with `DiagramType::Flowchart`. DOT files get the same layout algorithms, SVG themes, and terminal rendering as native Mermaid input. This covers the structural subset (nodes, edges, subgraphs, labels) rather than full Graphviz visual attribute passthrough.

## About Contributions

> *About Contributions:* Please don't take this the wrong way, but I do not accept outside contributions for any of my projects. I simply don't have the mental bandwidth to review anything, and it's my name on the thing, so I'm responsible for any problems it causes; thus, the risk-reward is highly asymmetric from my perspective. I'd also have to worry about other "stakeholders," which seems unwise for tools I mostly make for myself for free. Feel free to submit issues, and even PRs if you want to illustrate a proposed fix, but know I won't merge them directly. Instead, I'll have Claude or Codex review submissions via `gh` and independently decide whether and how to address them. Bug reports in particular are welcome. Sorry if this offends, but I want to avoid wasted time and hurt feelings. I understand this isn't in sync with the prevailing open-source ethos that seeks community contributions, but it's the only way I can move at this velocity and keep my sanity.

## License

MIT License (with OpenAI/Anthropic Rider). See [LICENSE](LICENSE).

<!-- BEGIN GENERATED: runtime-capability-metadata -->
| Surface | Status | Evidence |
|---------|--------|----------|
| CLI detect command | Implemented | 2 evidence refs |
| CLI parse command with IR JSON evidence | Implemented | 1 evidence refs |
| CLI SVG rendering | Implemented | 1 evidence refs |
| CLI terminal rendering | Implemented | 1 evidence refs |
| CLI validate command with structured diagnostics | Implemented | 1 evidence refs |
| CLI capability matrix command | Implemented | 2 evidence refs |
| WASM API renders SVG | Implemented | 1 evidence refs |
| WASM API exposes capability matrix metadata | Implemented | 1 evidence refs |
| Canvas rendering backend | Implemented | 1 evidence refs |
<!-- END GENERATED: runtime-capability-metadata -->
s |
<!-- END GENERATED: runtime-capability-metadata -->
