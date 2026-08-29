# FEATURE_PARITY

## Meaning Of This Document

This file tracks actual parity status against the FrankenTUI Mermaid extraction
reference, not marketing claims and not aspirational support.

TWO AXES, NEVER COLLAPSED. A family has an independent answer for each:

- **Runtime** (`fm_core::DiagramType::support_level`): does the family parse,
  lay out, and render here? `Full` / `Partial` / `Unsupported`. This is the axis
  the README capability tables and the capability matrix publish.
- **Parity** (`fm_core::DiagramType::parity_level`): how completely does it
  reproduce the FrankenTUI reference's documented behavior?
  `Complete` (reference behavior reproduced) / `Partial` (meaningful subset,
  documented gaps remain) / `Fallback` (detected, routed through generic
  behavior) / `Missing` (no meaningful implementation) /
  `N/A` (new family with no reference counterpart).

`Runtime: Full` and `Parity: Partial` is a normal, honest combination — a family
can render end to end while still missing documented reference behavior. Both
tables below are GENERATED from the pinned Rust sources and byte-checked by
tests; hand-editing the generated blocks is a drift the build will reject.

## Evidence Sources

Current status in this file is grounded in:

- parser dispatch in [`crates/fm-parser/src/mermaid_parser.rs`](/data/projects/frankenmermaid/crates/fm-parser/src/mermaid_parser.rs)
- type detection in [`crates/fm-parser/src/lib.rs`](/data/projects/frankenmermaid/crates/fm-parser/src/lib.rs)
- CLI support reporting in [`crates/fm-cli/src/main.rs`](/data/projects/frankenmermaid/crates/fm-cli/src/main.rs)
- layout specialization in [`crates/fm-layout/src/lib.rs`](/data/projects/frankenmermaid/crates/fm-layout/src/lib.rs)
- fixture-backed FrankenTUI conformance coverage in [`crates/fm-cli/tests/frankentui_conformance_test.rs`](/data/projects/frankenmermaid/crates/fm-cli/tests/frankentui_conformance_test.rs)
- hosted mermaid.js differential evidence in [`scripts/run_static_web_e2e.py`](/data/projects/frankenmermaid/scripts/run_static_web_e2e.py) and [`scripts/showcase_harness.py`](/data/projects/frankenmermaid/scripts/showcase_harness.py)
- behavioral reference paths listed in [`AGENTS.md`](/data/projects/frankenmermaid/AGENTS.md)

## Current Baseline

### Parser Families

<!-- BEGIN GENERATED: feature-parity-families -->
| Diagram family | Detection | Dedicated parser | Dedicated layout | SVG render | Runtime | Parity | Notes |
|---|---|---|---|---|---|---|---|
| flowchart | Yes | Yes | `auto` | Yes | Full | Partial | Most advanced path; recursive document AST, edge bundling, layout constraints |
| sequence | Yes | Yes | `sequence` | Yes | Partial | Partial | Participants, messages, notes, fragments (loop/alt/par/opt/critical/break), activations, lifecycle events, participant groups |
| class | Yes | Yes | `auto` | Yes | Full | Partial | Members, inheritance, stereotypes, generics, compartment rendering |
| state | Yes | Yes | `auto` | Yes | Full | Partial | Transitions, composites, fork/join, history states, choice |
| er | Yes | Yes | `auto` | Yes | Full | Partial | Entity attributes with PK/FK/UK, crow's-foot cardinality markers (bd-b1sy2) |
| requirementDiagram | Yes | Yes | `auto` | Yes | Full | Partial | Requirement types, id/text/risk/verifyMethod metadata extraction |
| mindmap | Yes | Yes | `radial` | Yes | Full | Partial | Indentation-based hierarchy, node shapes |
| journey | Yes | Yes | `kanban` | Yes | Full | Partial | Steps, sections |
| timeline | Yes | Yes | `timeline` | Yes | Full | Partial | Periods with events |
| packet-beta | Yes | Yes | `packet` | Yes | Full | Partial | Field parsing, grid-based layout |
| gantt | Yes | Yes | `gantt` | Yes | Full | Partial | Tasks, sections, durations, task types, date metadata |
| pie | Yes | Yes | `pie` | Yes | Full | Partial | Slice values, title, showData, wedge SVG rendering with accent colors |
| quadrantChart | Yes | Yes | `quadrant` | Yes | Full | Partial | Axis labels, quadrant labels, data points with [0,1] coords, scatter SVG |
| gitGraph | Yes | Yes | `gitgraph` | Yes | Full | Partial | Commits, branches, merges, cherry-pick, lane-based layout |
| sankey | Yes | Yes | `sankey` | Yes | Full | Partial | Dedicated parser and flow-preserving layout; fixture-backed FrankenTUI conformance for link rows |
| xyChart | Yes | Yes | `xychart` | Yes | Full | Partial | Axis/series metadata, bar/line/area rendering; fixture-backed FrankenTUI conformance for axes + named series |
| block-beta | Yes | Yes | `grid` | Yes | Full | Partial | Column spanning, space blocks, group nesting; fixture-backed FrankenTUI conformance for nested structure |
| architecture-beta | Yes | Yes | `architecture` | Yes | Full | Partial | Groups, services, junctions, icon classes; the dedicated architecture algorithm engages when an edge declares a side, otherwise the general selector runs (bd-zce4) |
| C4 (all five variants) | Yes | Yes | `architecture` | Yes | Full | Partial | Boundary detection, C4 node metadata; one notation at five zoom levels; direction-aware algorithm engages on a declared relationship side |
| kanban | Yes | Yes | `kanban` | Yes | Full | Partial | Columns and cards via clusters |
| DOT bridge | Yes | Yes | `auto` | Yes | Full | Partial | Graphviz DOT format to shared IR |
| treemap | Yes | Yes | `treemap` | Yes | Full | N/A | Squarified treemap; new family with no FrankenTUI reference counterpart (bd-dw450 certified terminal/canvas/WebGPU draw) |
| radar-beta | Yes | Yes | `radar` | Yes | Full | N/A | Polar layout with cardinal-spline wedges; new family with no FrankenTUI reference counterpart (bd-sk4dv) |
| info | Yes | Yes | `auto` | Yes | Full | N/A | Title banner; routes through the general graph selector |
<!-- END GENERATED: feature-parity-families -->

### Layout Algorithms

<!-- BEGIN GENERATED: feature-parity-layouts -->
| Algorithm | Serves | Notes |
|---|---|---|
| `auto` | All types | Selection by diagram type and graph topology; sugiyama default in the general selector |
| `sugiyama` | General graphs (flowchart, class, state, ER, requirement, DOT) | Cycle breaking (4 strategies), crossing minimization, Brandes-Kopf coordinate assignment, edge bundling |
| `force` | General (fallback for dense/cyclic) | Fruchterman-Reingold with Barnes-Hut, cluster cohesion |
| `tree` | Tree-like graphs | Reingold-Tilford variant with direction support |
| `radial` | Mindmap | Leaf-weighted angle allocation |
| `sequence` | Sequence diagrams | Participant columns, message stacking, activation bars, notes, fragments |
| `timeline` | Timeline | Horizontal periods with vertical events |
| `gantt` | Gantt charts | Time-axis bar layout with sections |
| `xychart` | XY charts | Cartesian coordinate mapping |
| `sankey` | Sankey diagrams | Flow-preserving column layout |
| `kanban` | Journey, kanban | Fixed-column card stacking |
| `grid` | Block-beta | CSS-grid-like positioning with column spans |
| `pie` | Pie charts | Wedge angle computation, perimeter label positioning |
| `quadrant` | Quadrant charts | 2D scatter on [0,1] axes |
| `gitgraph` | Git graphs | Lane-based commit positioning |
| `packet` | Packet-beta | Grid-based field layout |
| `architecture` | Architecture-beta, C4 (conditional) | Direction-aware placement; engages when the input declares a side (bd-zce4), otherwise the general selector runs |
| `treemap` | Treemap | Squarified tile allocation |
| `radar` | Radar-beta | Polar wedges with cardinal-spline rendering |
<!-- END GENERATED: feature-parity-layouts -->

### Cross-Cutting Features

| Feature | Status | Notes |
|---|---|---|
| Edge bundling | Complete | Groups parallel edges, collapses to representative with count label |
| Layout constraints (SameRank, MinLength) | Complete | Applied after rank assignment in Sugiyama |
| accTitle/accDescr directives | Complete | Parsed and propagated to SVG title/desc |
| Subgraph direction override | Complete | `direction LR` inside subgraph blocks |
| linkStyle default | Complete | Default style for all unindexed edges |
| Click/callback directives with tooltips | Complete | `click nodeId "url" "tooltip"` plus callback hooks; fixture-backed FrankenTUI conformance coverage exists |
| ER cardinality labels | Complete | Notation parsed and rendered as endpoint labels |
| Theme variable overrides | Complete | primaryColor, lineColor, clusterBkg, etc. mapped to palette |
| Sequence notes SVG | Complete | Rounded-corner boxes near lifelines |
| Sequence fragments SVG | Complete | Dashed-border rectangles with kind/label tabs |

### Rendering Surfaces

| Surface | Current status | Notes |
|---|---|---|
| Shared IR pipeline | Complete | `MermaidDiagramIr` feeds all renderers |
| Deterministic layout | Complete | BTreeMap everywhere, stable tie-breaking, 18 algorithms + Auto |
| SVG renderer | Partial | 21 node shapes, gradients, shadows, themes, accessibility, pie/quadrant/xychart specializations |
| Terminal renderer | Partial | 4 sub-cell modes, diff engine, minimap, glyphs |
| Canvas/WASM | Partial | Canvas2D with mock context, viewport transforms |
| Diff engine | Complete | Structural node/edge diffing with change classification |
| Minimap | Complete | Density-aware scaling with viewport indicator |

## Remaining Gaps vs FrankenTUI

### Parser-Level

- `classDef default` — neither FrankenTUI nor frankenmermaid supports this
- The first fixture-backed FrankenTUI conformance slice now exists in
  [`crates/fm-cli/tests/frankentui_conformance_test.rs`](/data/projects/frankenmermaid/crates/fm-cli/tests/frankentui_conformance_test.rs)
  and
  [`crates/fm-cli/tests/frankentui_conformance_cases.json`](/data/projects/frankenmermaid/crates/fm-cli/tests/frankentui_conformance_cases.json),
  covering click/callback directives, block-beta structure, sankey links, and
  xychart axes/series against explicit reference-surface expectations
- Several `Partial` rows still remain implementation-backed rather than fully
  reference-proved because the fixture corpus is intentionally narrow in this
  first pass
- The showcase E2E harness now emits machine-checked differential summaries for
  the compare/export path so mermaid.js shadow rendering is tracked as evidence,
  but this is still browser-surface evidence rather than full per-family parser
  parity proof
- Hosted E2E summaries and replay manifests now preserve per-run `trace_id`
  lineage so deterministic replay evidence can be tied back to the same
  observability IDs emitted by the Rust runtime

### Rendering-Level

- Terminal fidelity-tier selection and overlay behavior still need an
  evidence-backed parity audit against FrankenTUI's `Outline` / `Compact` /
  `Normal` / `Rich` model
- Debug overlay panel (crossings, bends, symmetry metrics) — TUI-specific and
  not yet mapped into frankenmermaid surfaces
- Interactive selection state (node highlight, directional navigation) —
  TUI-specific and not yet mapped into frankenmermaid surfaces

### Areas Where frankenmermaid Is Ahead of FrankenTUI

- Participant groups with color support
- Lifecycle events (create/destroy)
- Fragment alternatives with labeled sections
- Fragment nesting (children tracking)
- 18 dedicated layout algorithms plus Auto (vs ~6 in FrankenTUI)
- SVG gradients, shadows, glow effects
- Canvas2D rendering backend
- WASM/JavaScript API
- 10 theme presets (vs 6 in FrankenTUI)
- Accessibility (ARIA labels, accTitle/accDescr, keyboard nav)
- Click callback hooks rendered into SVG via `data-callback` attributes for
  host-side JS integration
- Terminal pie chart rendering, gantt chart rendering, diff rendering, and
  minimap rendering are all present as dedicated `fm-render-term` surfaces
