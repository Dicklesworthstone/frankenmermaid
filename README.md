# frankenmermaid

<div align="center">
  <img src="frankenmermaid_illustration.webp" alt="frankenmermaid - resilient Mermaid-compatible diagram engine in Rust" />
</div>

<div align="center">

![Rust Nightly 2024](https://img.shields.io/badge/Rust-nightly%202024-orange)
![WASM](https://img.shields.io/badge/WASM-web%20target-blue)
![Deterministic](https://img.shields.io/badge/layout-deterministic-success)
![Diagram Types](https://img.shields.io/badge/diagram%20types-25-purple)
![Tests](https://img.shields.io/badge/tests-200%2B-brightgreen)
![No Unsafe](https://img.shields.io/badge/unsafe-forbidden-red)

</div>

Rust-first Mermaid-compatible diagram engine. Parses messy input without crashing, picks smarter layouts automatically, and renders polished SVG/terminal/web output from a single pipeline.

<div align="center">

**Live Demo:** <https://dicklesworthstone.github.io/frankenmermaid/>

```bash
curl -fsSL "https://raw.githubusercontent.com/Dicklesworthstone/frankenmermaid/main/install.sh" | bash
```

</div>

---

## TL;DR

**The Problem**: Mermaid syntax is great for documentation-as-code, but real-world diagrams hit walls fast. Cycles produce tangled layouts. Malformed input crashes the parser. Large graphs slow to a crawl. Styling control is limited. And there is no terminal output at all.

**The Solution**: `frankenmermaid` is a ground-up Rust implementation with a shared intermediate representation that feeds 10+ layout algorithms and 3 render backends. It recovers from bad input instead of failing, picks cycle-aware layout strategies automatically, and produces deterministic output suitable for CI snapshot testing.

### Why Use frankenmermaid?

| Feature | What It Does |
|---------|--------------|
| **25 Diagram Types** | Flowchart, sequence, class, state, ER, gantt, pie, gitGraph, journey, mindmap, timeline, sankey, quadrant, xyChart, block-beta, packet-beta, architecture-beta, 5 C4 variants, requirement, kanban |
| **Intent-Aware Parsing** | Recovers from malformed syntax and infers likely intent instead of failing. Fuzzy keyword matching catches typos like `flowchar` or `seqeunceDiagram` |
| **10 Layout Algorithms** | Sugiyama (hierarchical), force-directed, tree, radial, timeline, gantt, sankey, kanban/grid, and sequence with auto-selection per diagram type |
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
   Four cycle-breaking strategies. Barycenter + transpose crossing minimization. Orthogonal edge routing with bend minimization. Specialized algorithms for sequence, gantt, timeline, sankey, radial, and grid diagrams. The layout engine doesn't just place nodes — it optimizes.

4. **One IR, Many Outputs**
   A shared `MermaidDiagramIr` feeds SVG, terminal, Canvas, and WASM APIs. Parse once, render everywhere. Layout statistics and diagnostics travel through the entire pipeline.

5. **Polish Is Core Product Surface**
   Typography, spacing, theming, accessibility, node gradients, drop shadows, responsive sizing — these aren't extras. They're part of correctness.

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
| Diagram Type | Parse | Layout | SVG Render | Status |
|--------------|-------|--------|------------|--------|
| `flowchart` | Full | Sugiyama + Force + Tree | Full | Implemented |
| `sequence` | Basic | Sequence-specific | Basic | Partial |
| `class` | Basic | Sugiyama | Basic | Partial |
| `state` | Basic | Sugiyama | Basic | Partial |
| `er` | Full | Sugiyama | Full | Partial |
| `gantt` | Basic | Gantt-specific | Basic | Partial |
| `pie` | Full | Pie-specific | Full | Partial |
| `gitGraph` | Full | Tree | Basic | Partial |
| `journey` | Full | Kanban | Basic | Partial |
| `mindmap` | Full | Radial | Basic | Partial |
| `timeline` | Full | Timeline-specific | Basic | Partial |
| `sankey` | Full | Sankey-specific | Basic | Partial |
| `quadrantChart` | Basic | Grid | Basic | Partial |
| `xyChart` | Basic | Sugiyama (fallback) | Minimal | Unsupported |
| `block-beta` | Full | Grid | Basic | Partial |
| `packet-beta` | Basic | Sugiyama | Basic | Partial |
| `architecture-beta` | Basic | Sugiyama | Basic | Partial |
| `C4Context` | Full | Sugiyama | Basic | Partial |
| `C4Container` | Full | Sugiyama | Basic | Partial |
| `C4Component` | Full | Sugiyama | Basic | Partial |
| `C4Dynamic` | Full | Sugiyama | Basic | Partial |
| `C4Deployment` | Full | Sugiyama | Basic | Partial |
| `requirementDiagram` | Basic | Sugiyama | Basic | Partial |
| `kanban` | Full | Kanban | Basic | Partial |
<!-- END GENERATED: supported-diagram-types -->

**Key:** Full = complete syntax coverage. Basic = core syntax works, advanced features in progress. Minimal = parsed but rendering needs dedicated work.

## Installation

### Quick Install (CLI)

```bash
curl -fsSL "https://raw.githubusercontent.com/Dicklesworthstone/frankenmermaid/main/install.sh" | bash
```

### JavaScript / WASM

```bash
npm install @frankenmermaid/core
```

### Rust (Cargo)

```bash
cargo install frankenmermaid
```

### From Source

```bash
git clone https://github.com/Dicklesworthstone/frankenmermaid.git
cd frankenmermaid
cargo build --release --workspace
# Binary at target/release/fm-cli
```

**Note:** Requires Rust nightly (see `rust-toolchain.toml`). The project uses Rust 2024 edition features.

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

6. **Use from JavaScript**:
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
deterministic = true          # Enforce deterministic output
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
| `fm-core` | ~4,000 | Shared IR types, config, errors, diagnostics, 20+ node shapes |
| `fm-parser` | ~8,700 | 25-type detection + parsing + error recovery + DOT bridge |
| `fm-layout` | ~8,400 | 10 layout algorithms, 4 cycle strategies, crossing minimization |
| `fm-render-svg` | ~7,000 | Accessible, themeable SVG with gradients/shadows/glows |
| `fm-render-term` | ~4,400 | Terminal renderer + diff engine + minimap + 4 fidelity modes |
| `fm-render-canvas` | ~2,500 | Canvas2D web rendering with trait-based abstraction |
| `fm-wasm` | ~850 | wasm-bindgen API and TypeScript bindings |
| `fm-cli` | ~1,800 | CLI surface: render, parse, detect, validate, diff, watch, serve |
| **Total** | **~37,900** | |

### Pipeline

```
     Mermaid / DOT text
              |
              v
  +-----------------------+
  | fm-parser             |
  | - type detection      |     25 diagram types recognized
  | - fuzzy matching      |     Levenshtein + content heuristics
  | - recovery + warnings |     Best-effort parse, never crashes
  +-----------------------+
              |
              v
  +-----------------------+
  | fm-core               |
  | MermaidDiagramIr      |     Nodes, edges, clusters, labels,
  |                       |     subgraphs, ports, diagnostics
  +-----------------------+
              |
              v
  +-----------------------+
  | fm-layout             |
  | - auto algorithm      |     Picks best of 10 algorithms
  | - cycle strategy      |     4 strategies for cycle-breaking
  | - crossing minimize   |     Barycenter + transpose refinement
  +-----------------------+
              |
              v
  +--DiagramLayout + stats--+
  |  nodes, edges, clusters |
  |  bounds, cycle info     |
  +---------+-------+-------+
            |       |       |
            v       v       v
  +-------+ +-----+ +---------+
  |  SVG  | | Term| | Canvas  |
  +-------+ +-----+ +---------+
      |                  |
      v                  v
  SVG / PNG        WASM + browser
```

### Feature Flags

```toml
[features]
default = []
watch = ["dep:notify"]        # File watching for live reload
serve = ["dep:tiny_http"]     # Local preview server
png = ["dep:resvg", "dep:usvg"] # PNG rasterization from SVG
```

## How the Parser Works

The parser uses a **five-tier detection pipeline** to identify diagram types, then dispatches to a type-specific parser that produces a shared intermediate representation.

### Type Detection Pipeline

```
Input text
    │
    ▼
┌─────────────────────────────────────┐
│ 1. DOT Format Detection             │  confidence: 0.95
│    digraph/graph keyword + braces   │  Graphviz interop
├─────────────────────────────────────┤
│ 2. Exact Keyword Match              │  confidence: 1.0
│    "flowchart", "sequenceDiagram",  │  25 keywords recognized
│    "classDiagram", "gantt", etc.    │
├─────────────────────────────────────┤
│ 3. Fuzzy Keyword Match              │  confidence: 0.70–0.85
│    Levenshtein distance 1–2         │  Catches typos like
│    "flowchrt" → "flowchart"         │  "seqeunceDiagram"
├─────────────────────────────────────┤
│ 4. Content Heuristics               │  confidence: 0.60–0.80
│    Arrow patterns: -->  ->>  ||--o{ │  Symbol fingerprinting
│    Keywords: participant, state      │
├─────────────────────────────────────┤
│ 5. Fallback                         │  confidence: 0.30
│    Default to Flowchart + warning   │  Never returns "unknown"
└─────────────────────────────────────┘
```

Each tier is tried in order. The first match wins. The confidence score tells downstream consumers how certain the detection was — tooling can surface low-confidence detections as warnings.

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

The layout engine takes a parsed `MermaidDiagramIr` and produces a `DiagramLayout` — a collection of positioned node boxes, routed edge paths, and cluster boundaries. Different diagram types get different algorithms, but the output shape is always the same.

### Algorithm Auto-Selection

When `algorithm = "auto"` (the default), the engine maps diagram types to their best algorithm:

| Algorithm | Used For | Strategy |
|-----------|----------|----------|
| **Sugiyama** | Flowchart, class, state, ER, C4, requirement | Hierarchical layered layout with rank assignment and crossing minimization |
| **Force-directed** | Available for all graph types | Spring-electrical simulation with Barnes-Hut optimization |
| **Tree** | Available for all graph types | Reingold-Tilford tidy tree with Knuth-style spacing |
| **Radial** | Mindmap | Concentric rings with angle allocation proportional to subtree leaf count |
| **Sequence** | Sequence diagrams | Horizontal participant columns, vertical message stacking, self-message loops |
| **Timeline** | Timeline diagrams | Linear horizontal periods with vertically stacked events |
| **Gantt** | Gantt charts | Time-axis bar layout with section swimlanes |
| **Sankey** | Sankey diagrams | Flow-conserving column layout with iterative relaxation |
| **Kanban** | Journey, kanban | Fixed-column card stacking |
| **Grid** | Block-beta | CSS-grid-like positioning with column/row spans |

### The Sugiyama Algorithm (Hierarchical Layout)

The Sugiyama algorithm is the workhorse for most graph diagram types. It transforms an arbitrary directed graph into a clean layered layout through seven phases:

**Phase 1 — Cycle Removal**

Directed graphs with cycles can't be laid out in layers (every edge must point "downward"). The engine breaks cycles by temporarily reversing selected edges. Four strategies are available:

| Strategy | How It Works | When to Use |
|----------|--------------|-------------|
| **Greedy** | Repeatedly remove sink/source nodes, reverse remaining edges | Fast default. Good enough for most graphs |
| **DFS back-edge** | Run DFS, reverse back-edges found during traversal | Predictable — the same DFS order gives the same result |
| **MFAS approximation** | Approximate minimum feedback arc set via heuristic ordering | Minimizes the number of reversed edges |
| **Cycle-aware** | Full SCC (strongly connected component) detection with optional cluster collapse | Best visual quality. Cycle clusters rendered as grouped boxes |

The cycle-aware strategy additionally computes `cycle_count`, `cycle_node_count`, `max_cycle_size`, and `reversed_edge_total_length` metrics that are available in the layout stats.

**Phase 2 — Rank Assignment**

Each node is assigned an integer rank (layer) using a longest-path heuristic. Ranks are computed in topological order so that every non-reversed edge goes from a lower rank to a higher rank. This determines the vertical (or horizontal, depending on direction) position of each node.

**Phase 3 — Crossing Minimization (Barycenter)**

The ordering of nodes within each rank is optimized to minimize edge crossings. The algorithm performs 4 bidirectional sweeps:

1. For each rank, compute each node's **barycenter** — the weighted average position of its connected neighbors in the adjacent rank.
2. Sort the rank's nodes by barycenter value, breaking ties by stable node index.
3. Sweep top-to-bottom, then bottom-to-top (bidirectional).

**Phase 4 — Crossing Refinement (Transpose + Sift)**

After barycenter ordering, two local-search refinements further reduce crossings:

- **Transpose**: Try swapping every adjacent pair of nodes within each rank. Accept swaps that reduce the total crossing count. Run up to 10 passes, early-exit if crossings reach zero.
- **Sifting**: For each node, evaluate all possible positions within its rank and move it to the position that minimizes crossings.

The layout stats record `crossing_count_before_refinement` and final `crossing_count` so you can see how much the refinement improved things.

**Phase 5 — Coordinate Assignment**

Nodes are positioned in 2D space using their rank (vertical position) and order (horizontal position within rank), plus configurable spacing (`node_spacing` default 80px, `rank_spacing` default 120px).

**Phase 6 — Edge Routing**

Edges are routed as orthogonal (Manhattan-style) paths with horizontal and vertical segments. Special cases:

- **Self-loops**: When source equals target, the edge routes as a rectangular loop extending to the right and back.
- **Parallel edges**: When multiple edges connect the same pair of nodes, each gets an incremental lateral offset so they're visually distinguishable.
- **Reversed edges**: Edges that were reversed for cycle-breaking are flagged (`reversed: true`) so renderers can draw them with dashed or highlighted styling.

**Phase 7 — Post-Processing**

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

**Glow Effects** add a colored blur behind highlighted elements — blur radius 6px, opacity 0.35, default color `#3b82f6` (blue). Used for interactive highlighting or emphasis.

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
- Source span tracking — optional `data-fm-source-span` attributes linking SVG elements back to their source line/column

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

In **Braille mode**, each terminal cell represents an 8-dot braille pattern where each dot maps to a sub-pixel. The 8 dots are arranged in a 2-wide × 4-tall grid, giving 8 sub-pixels per cell — the highest resolution achievable in a standard terminal. The renderer draws into a boolean pixel buffer using Bresenham's line algorithm and midpoint circle algorithm, then encodes the buffer into braille code points starting from U+2800.

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

For large diagrams that exceed the terminal viewport, the renderer can produce a scaled minimap — a compressed overview showing the overall structure:

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
    diagram_type: DiagramType,          // One of 25 types
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

**Implicit nodes**: Nodes referenced only in edges (never explicitly declared) are auto-created with `implicit: true`. This lets the parser accept terse input like `A --> B` without requiring `A[A]` and `B[B]` declarations first — matching mermaid-js behavior.

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

The layout crate gets `opt-level = 3` (maximum speed) instead of `opt-level = "z"` (minimum size) because layout is the computational bottleneck — the crossing minimization and coordinate assignment phases dominate pipeline latency. Every other crate prioritizes small binary size for fast WASM delivery.

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
3. **Subtree span computation**: Bottom-up recursive calculation — each node's span is `max(own_width, sum_of_children_spans)`.
4. **Coordinate assignment**: Children are centered under their parent. Siblings are spaced by `node_spacing`.
5. **Direction support**: TB (top-to-bottom, default), LR, RL, BT — the depth axis and breadth axis swap roles.

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

## Quality and Testing

- **200+ unit tests** across parser, core, layout, and render crates
- **Integration tests** for full parse → layout → render pipeline round-trips
- **Golden SVG snapshots** for regression safety (8 diagram types, blessed with `BLESS=1`)
- **Property-based tests** (proptest) for parser and layout invariants
- **Determinism checks** — same input verified to produce identical output across runs
- **Clippy pedantic + nursery** lints enabled workspace-wide with `-D warnings`
- **Zero unsafe code** enforced via `#![forbid(unsafe_code)]`

```bash
# Run the full quality gate
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo test --workspace
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

- **XyChart** is the only diagram type marked unsupported — it parses but lacks dedicated layout and rendering. Tracked for implementation.
- **Sequence diagram advanced features** (activation boxes, interaction fragments, notes) are not yet implemented. Basic participant/message flow works.
- **classDef / style directives** are parsed but not yet applied to rendered output. Styling support is in progress.
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

A syntax and behavior reference corpus (including mermaid-js source/docs). It is not a port target — used only for edge-case validation.

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

The layout guardrails kick in automatically. Before running layout, the engine estimates the computational cost based on node count, edge count, and the selected algorithm. If the estimate exceeds the time budget (default 250ms), it falls back to a cheaper algorithm — for example, Tree instead of Sugiyama. The fallback chain ensures that even 10,000-node graphs produce output in bounded time, at the cost of potentially lower visual quality.

### Can I use the IR directly for tooling?

Yes. `fm-cli parse --format json` emits the full intermediate representation as JSON, including nodes, edges, clusters, labels, diagnostics, and metadata. This is designed for editor integrations, diagram linters, and downstream tooling that wants to consume diagram structure without reimplementing parsing.

### How does the braille terminal rendering work?

Each terminal cell represents a 2x4 grid of sub-pixels using Unicode braille characters (U+2800–U+28FF). The renderer draws into a boolean pixel buffer using Bresenham's line algorithm, then encodes 8-pixel blocks into single braille code points. This gives an effective resolution of 2× the terminal width and 4× the terminal height — enough to render smooth diagonal lines and curves in a standard terminal.

### Why Rust instead of JavaScript?

Three reasons: (1) Determinism — Rust's lack of garbage collection pauses and its deterministic floating-point behavior make output stability achievable. (2) Performance — the layout engine does O(n² log n) work for crossing minimization; Rust runs this 10-50x faster than equivalent JS. (3) WASM — Rust compiles to compact WASM with no runtime dependencies, so the same code runs natively for CLI and in-browser via npm.

### How does DOT format support work?

The DOT bridge parser (`dot_parser.rs`) recognizes Graphviz `digraph` and `graph` declarations, extracts nodes and edges with their attributes, and converts them to `MermaidDiagramIr` with `DiagramType::Flowchart`. This means DOT files get the same layout algorithms, SVG themes, and terminal rendering as native Mermaid input. It's not a complete Graphviz reimplementation — it covers the structural subset (nodes, edges, subgraphs, labels) rather than visual attribute passthrough.

## About Contributions

> *About Contributions:* Please don't take this the wrong way, but I do not accept outside contributions for any of my projects. I simply don't have the mental bandwidth to review anything, and it's my name on the thing, so I'm responsible for any problems it causes; thus, the risk-reward is highly asymmetric from my perspective. I'd also have to worry about other "stakeholders," which seems unwise for tools I mostly make for myself for free. Feel free to submit issues, and even PRs if you want to illustrate a proposed fix, but know I won't merge them directly. Instead, I'll have Claude or Codex review submissions via `gh` and independently decide whether and how to address them. Bug reports in particular are welcome. Sorry if this offends, but I want to avoid wasted time and hurt feelings. I understand this isn't in sync with the prevailing open-source ethos that seeks community contributions, but it's the only way I can move at this velocity and keep my sanity.

## License

MIT License (with OpenAI/Anthropic Rider). See [LICENSE](LICENSE).
