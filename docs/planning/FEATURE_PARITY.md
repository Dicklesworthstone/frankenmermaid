# FEATURE_PARITY

## Meaning Of This Document

This file tracks actual parity status against the FrankenTUI Mermaid extraction
reference, not marketing claims and not aspirational support.

Status meanings:

- `Complete`: implemented and verified in current Rust code
- `Partial`: implemented for a meaningful subset, but not yet parity-complete
- `Fallback`: detected or acknowledged, but routed through generic/fallback
  behavior rather than a real implementation
- `Missing`: no meaningful implementation yet

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

| Diagram family | Detection | Dedicated parser | Dedicated layout | SVG render | Current status | Notes |
|---|---|---|---|---|---|---|
| Flowchart | Yes | Yes | Sugiyama | Yes | Partial | Most advanced path; recursive document AST, edge bundling, layout constraints |
| Sequence | Yes | Yes | Sequence | Yes | Partial | Participants, messages, notes, fragments (loop/alt/par/opt/critical/break), activations, lifecycle events, participant groups |
| Class | Yes | Yes | Sugiyama | Yes | Partial | Members, inheritance, stereotypes, generics, compartment rendering |
| State | Yes | Yes | Sugiyama | Yes | Partial | Transitions, composites, fork/join, history states, choice |
| ER | Yes | Yes | Sugiyama | Yes | Partial | Entity attributes with PK/FK/UK, cardinality labels on edges |
| Requirement | Yes | Yes | Sugiyama | Yes | Partial | Requirement types, id/text/risk/verifyMethod metadata extraction |
| Mindmap | Yes | Yes | Radial | Yes | Partial | Indentation-based hierarchy, node shapes |
| Journey | Yes | Yes | Kanban | Yes | Partial | Steps, sections |
| Timeline | Yes | Yes | Timeline | Yes | Partial | Periods with events |
| Packet Beta | Yes | Yes | Packet (Grid) | Yes | Partial | Field parsing, grid-based layout |
| Gantt | Yes | Yes | Gantt | Yes | Partial | Tasks, sections, durations, task types, date metadata |
| Pie | Yes | Yes | Pie | Yes | Partial | Slice values, title, showData, wedge SVG rendering with accent colors |
| Quadrant Chart | Yes | Yes | Quadrant | Yes | Partial | Axis labels, quadrant labels, data points with [0,1] coords, scatter SVG |
| Git Graph | Yes | Yes | GitGraph | Yes | Partial | Commits, branches, merges, cherry-pick, lane-based layout |
| Sankey | Yes | Yes | Sankey | Yes | Partial | Dedicated parser and flow-preserving layout; fixture-backed FrankenTUI conformance for link rows |
| XY Chart | Yes | Yes | XyChart | Yes | Partial | Axis/series metadata, bar/line/area rendering; fixture-backed FrankenTUI conformance for axes + named series |
| Block Beta | Yes | Yes | Grid | Yes | Partial | Column spanning, space blocks, group nesting; fixture-backed FrankenTUI conformance for nested structure |
| Architecture Beta | Yes | Yes | Sugiyama | Yes | Partial | Groups, services, junctions, icon classes |
| C4 family | Yes | Yes | Sugiyama | Yes | Partial | Boundary detection, C4 node metadata |
| Kanban | Yes | Yes | Kanban | Yes | Partial | Columns and cards via clusters |
| DOT bridge | Yes | Yes | Sugiyama | Yes | Partial | Graphviz DOT format to shared IR |

### Layout Algorithms

| Algorithm | Diagram types | Status | Notes |
|---|---|---|---|
| Sugiyama (hierarchical) | Flowchart, Class, State, ER, C4, Requirement, Architecture | Complete | Cycle breaking (4 strategies), crossing minimization, Brandes-Kopf coordinate assignment, edge bundling |
| Force-directed | General (fallback for dense/cyclic) | Complete | Fruchterman-Reingold with Barnes-Hut, cluster cohesion |
| Tree | Tree-like graphs | Complete | Reingold-Tilford variant with direction support |
| Radial | Mindmap | Complete | Leaf-weighted angle allocation |
| Sequence | Sequence diagrams | Complete | Participant columns, message stacking, activation bars, notes, fragments |
| Timeline | Timeline | Complete | Horizontal periods with vertical events |
| Gantt | Gantt charts | Complete | Time-axis bar layout with sections |
| Sankey | Sankey diagrams | Complete | Flow-preserving column layout |
| Kanban | Journey, Kanban | Complete | Fixed-column card stacking |
| Grid | Block-beta | Complete | CSS-grid-like positioning with column spans |
| Pie | Pie charts | Complete | Wedge angle computation, perimeter label positioning |
| Quadrant | Quadrant charts | Complete | 2D scatter on [0,1] axes |
| GitGraph | Git graphs | Complete | Lane-based commit positioning |
| Packet | Packet-beta | Complete | Grid-based field layout |
| XyChart | XY charts | Complete | Cartesian coordinate mapping |
| Auto | All types | Complete | Intelligent selection based on diagram type and graph topology |

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
| Deterministic layout | Complete | BTreeMap everywhere, stable tie-breaking, 16 algorithms |
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
- 16 dedicated layout algorithms (vs ~6 in FrankenTUI)
- SVG gradients, shadows, glow effects
- Canvas2D rendering backend
- WASM/JavaScript API
- 10 theme presets (vs 6 in FrankenTUI)
- Accessibility (ARIA labels, accTitle/accDescr, keyboard nav)
- Click callback hooks rendered into SVG via `data-callback` attributes for
  host-side JS integration
- Terminal pie chart rendering, gantt chart rendering, diff rendering, and
  minimap rendering are all present as dedicated `fm-render-term` surfaces
