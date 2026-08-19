# EXISTING_FRANKENTUI_MERMAID_STRUCTURE

## Purpose

This document is the behavior/spec extraction baseline for the Mermaid-related
FrankenTUI surfaces referenced by [`AGENTS.md`](/data/projects/frankenmermaid/AGENTS.md).

It exists so frankenmermaid implementation work can be driven from a written
spec instead of repeatedly spelunking the legacy/reference code.

This is not a line-by-line translation plan.

## Source Files

Primary reference files:

- `/dp/frankentui/crates/ftui-extras/src/mermaid.rs`
- `/dp/frankentui/crates/ftui-extras/src/mermaid_layout.rs`
- `/dp/frankentui/crates/ftui-extras/src/mermaid_render.rs`
- `/dp/frankentui/crates/ftui-extras/src/mermaid_diff.rs`
- `/dp/frankentui/crates/ftui-extras/src/mermaid_minimap.rs`
- `/dp/frankentui/crates/ftui-extras/src/dot_parser.rs`
- `/dp/frankentui/crates/ftui-extras/src/canvas.rs`

## 1. Canonical Reference Metadata

The FrankenTUI reference includes two explicit parity registries in
`mermaid.rs`:

- `FEATURE_MATRIX`
- `DIAGRAM_FAMILY_REGISTRY`

Important extracted constants and meanings:

- `MERMAID_BASELINE_VERSION = "11.4.0"`
- diagram-family pipeline stages are tracked as:
  `parser`, `ir`, `layout`, `render`, `fixtures`, `snapshots`, `pty_e2e`,
  `demo_picker`
- stage status values are:
  `Done`, `Partial`, `NotStarted`, `NotApplicable`

This matters because the legacy/reference implementation does not treat parity
as a single boolean. It tracks parity per family and per pipeline stage.

## 2. Top-Level Parse / Prepare Pipeline

### Public entry points

Extracted public parser/preparation APIs from `mermaid.rs`:

- `tokenize(input: &str) -> Vec<Token<'_>>`
- `parse(input: &str) -> Result<MermaidAst, MermaidError>`
- `parse_with_diagnostics(input: &str) -> MermaidParse`
- `validate_ast(...) -> MermaidValidation`
- `validate_ast_with_policy(...) -> MermaidValidation`
- `validate_ast_with_policy_and_init(...) -> MermaidValidation`
- `prepare_with_policy(...) -> MermaidPrepared`
- `prepare(...) -> MermaidPrepared`
- `sanitize_url(url: &str, mode: MermaidSanitizeMode) -> LinkSanitizeOutcome`
- `resolve_links(...) -> LinkResolution`
- `resolve_styles(ir: &MermaidDiagramIr) -> MermaidResolvedStyles`
- `layout_diagram_cached(...)`
- `compatibility_report(...)`

### Parse flow

`parse_with_diagnostics` is line-oriented and performs these top-level steps:

1. Initialize `diagram_type`, `direction`, `pie_show_data`, `directives`,
   `statements`, `errors`, and per-family temporary state.
2. Strip inline comments per line.
3. Parse directive blocks starting with `%%{...}%%`.
4. Parse comment lines starting with `%%`.
5. Parse the first diagram header with `parse_header`.
6. Dispatch each subsequent line by detected diagram family:
   `State`, `Graph`, `Class`, `Er`, `Sequence`, `Gantt`, `Mindmap`, `Pie`,
   `GitGraph`, `Journey`, `Requirement`, `Timeline`, `XyChart`, `Sankey`,
   `BlockBeta`, `QuadrantChart`, `PacketBeta`, `ArchitectureBeta`, `C4*`,
   or `Unknown`.
7. Preserve unsupported/unparsed content as `Statement::Raw`.
8. Return `MermaidParse { ast, errors }`.

### Validation flow

Validation is separate from parsing. `validate_ast_with_policy_and_init`:

- checks unsupported diagram families against the compatibility matrix
- checks disabled init/style/link features against config and fallback policy
- converts `Statement::Raw` into warnings/errors except for known pie metadata
- merges init-directive warnings and errors into the validation result

### Prepare flow

`prepare_with_policy` performs:

1. parse
2. init directive application
3. theme override extraction
4. init-config hashing
5. compatibility report generation
6. validation
7. structured JSONL logging

That means the reference behavior is explicitly split into:

- syntax parsing
- init/theme processing
- compatibility assessment
- policy-driven validation

frankenmermaid should preserve that conceptual split even if the Rust crate
boundaries differ.

## 3. Diagram Family Coverage In The Reference

### Families with dedicated family-specific parse functions

The reference contains explicit parser functions for all of these families:

- Graph / Flowchart
- Sequence
- State
- Class
- ER
- Gantt
- Mindmap
- Pie
- GitGraph
- Journey
- Requirement
- Timeline
- QuadrantChart
- Sankey
- BlockBeta
- XyChart
- PacketBeta
- ArchitectureBeta
- `C4Context`
- `C4Container`
- `C4Component`
- `C4Dynamic`
- `C4Deployment`

Key extracted parser helpers:

- `parse_header`
- `parse_edge`
- `parse_node`
- `parse_subgraph_line`
- `parse_direction_line`
- `parse_class_def_line`
- `parse_class_line`
- `parse_style_line`
- `parse_link_style_line`
- `parse_link_directive`
- `parse_state_decl_line`
- `parse_state_note_start`
- `parse_state_edge`
- `parse_sequence`
- `parse_sequence_participant`
- `parse_sequence_note`
- `parse_sequence_activation`
- `parse_sequence_control`
- `parse_gantt`
- `parse_pie`
- `parse_packet_line`
- `parse_architecture_line`
- `parse_quadrant_line`
- `parse_mindmap`
- `parse_gitgraph_line`
- `parse_journey_line`
- `parse_sankey_line`
- `parse_block_line` / `parse_block_line_multi`
- `parse_xychart_line`
- `parse_timeline_line`
- `parse_requirement_line`
- `parse_c4_line`

### Important implication

The reference already contains dedicated parsers for families that
frankenmermaid currently still detects but fallback-routes, especially:

- `Sankey`
- `XyChart`
- `BlockBeta`
- `ArchitectureBeta`
- `C4*`

Those are concrete parity gaps, not hypothetical future features.

## 4. Core IR Types In The Reference

### Identity wrappers

The reference IR uses typed wrapper IDs:

- `IrNodeId(pub usize)`
- `IrPortId(pub usize)`
- `IrLabelId(pub usize)`
- `IrClusterId(pub usize)`
- `IrStyleRefId(pub usize)`
- `IrLinkId(pub usize)`

### Core graph/content structures

Exact extracted structures from `mermaid.rs`:

```rust
pub struct IrLabel {
    pub text: String,
    pub span: Span,
}

pub struct IrNode {
    pub id: String,
    pub label: Option<IrLabelId>,
    pub shape: NodeShape,
    pub classes: Vec<String>,
    pub style_ref: Option<IrStyleRefId>,
    pub span_primary: Span,
    pub span_all: Vec<Span>,
    pub implicit: bool,
    pub members: Vec<String>,
    pub annotation: Option<String>,
}

pub struct IrPort {
    pub node: IrNodeId,
    pub name: String,
    pub side_hint: IrPortSideHint,
    pub span: Span,
}

pub enum IrEndpoint {
    Node(IrNodeId),
    Port(IrPortId),
}

pub struct IrEdge {
    pub from: IrEndpoint,
    pub to: IrEndpoint,
    pub arrow: String,
    pub label: Option<IrLabelId>,
    pub style_ref: Option<IrStyleRefId>,
    pub span: Span,
}

pub struct IrCluster {
    pub id: IrClusterId,
    pub title: Option<IrLabelId>,
    pub members: Vec<IrNodeId>,
    pub span: Span,
}
```

### Specialized IR collections

The reference IR is not just nodes/edges/clusters. `MermaidDiagramIr` also
contains specialized collections for:

- `pie_entries`
- `quadrant_points`
- `quadrant_title`
- `quadrant_x_axis`
- `quadrant_y_axis`
- `quadrant_labels`
- `packet_fields`
- `packet_title`
- `packet_bits_per_row`
- `sequence_participants`
- `sequence_controls`
- `sequence_notes`
- `sequence_activations`
- `sequence_autonumber`
- `gantt_title`
- `gantt_sections`
- `gantt_tasks`
- `constraints`
- `links`
- `style_refs`

That means parity is not just "family detected" or "family parses to generic
nodes"; several families rely on dedicated IR fields.

### Exact specialized structs extracted

```rust
pub struct IrPieEntry {
    pub label: IrLabelId,
    pub value: f64,
    pub value_text: String,
    pub span: Span,
}

pub struct IrQuadrantPoint {
    pub label: IrLabelId,
    pub x: f64,
    pub y: f64,
}

pub struct IrQuadrantAxis {
    pub label_start: IrLabelId,
    pub label_end: IrLabelId,
}

pub struct IrPacketField {
    pub label: IrLabelId,
    pub bit_start: u32,
    pub bit_end: u32,
}

pub struct IrGanttSection {
    pub name: IrLabelId,
}

pub struct IrGanttTask {
    pub title: IrLabelId,
    pub meta: String,
    pub section_idx: usize,
    pub span: Span,
}
```

Sequence-specific extracted structures:

```rust
pub struct IrSeqControlBlock {
    pub kind: SeqControlKind,
    pub label: Option<IrLabelId>,
    pub start_edge_idx: usize,
    pub end_edge_idx: usize,
    pub depth: usize,
}

pub struct IrSeqNote {
    pub position: SeqNotePosition,
    pub over_nodes: Vec<usize>,
    pub text: IrLabelId,
    pub after_edge_idx: usize,
}

pub struct IrSeqActivation {
    pub node_idx: usize,
    pub start_edge_idx: usize,
    pub end_edge_idx: usize,
}
```

## 5. Shapes, Styling, Links, And Constraints

### Node shapes

Extracted node-shape variants:

- `Rect`
- `Rounded`
- `Stadium`
- `Subroutine`
- `Diamond`
- `Hexagon`
- `Circle`
- `Asymmetric`

These are explicitly tied to bracket syntax in the reference implementation.

### Style parsing

The reference parses Mermaid style strings into structured properties:

```rust
pub struct MermaidStyleProperties {
    pub fill: Option<MermaidColor>,
    pub stroke: Option<MermaidColor>,
    pub stroke_width: Option<u8>,
    pub stroke_dash: Option<MermaidStrokeDash>,
    pub color: Option<MermaidColor>,
    pub font_weight: Option<MermaidFontWeight>,
    pub unsupported: Vec<(String, String)>,
}
```

Resolution precedence is explicitly documented in code:

- theme defaults
- class styles
- node-specific styles

Then WCAG contrast clamping is applied when both foreground and background are
known.

### Links

The reference has explicit link IR and sanitization policy:

```rust
pub enum LinkSanitizeOutcome {
    Allowed,
    Blocked,
}

pub struct IrLink {
    pub kind: LinkKind,
    pub target: IrNodeId,
    pub url: String,
    pub tooltip: Option<String>,
    pub sanitize_outcome: LinkSanitizeOutcome,
    pub span: Span,
}
```

Important extracted behavior:

- blocked protocols always include:
  `javascript:`, `vbscript:`, `data:`, `file:`, `blob:`
- strict-allowed protocols are:
  `http:`, `https:`, `mailto:`, `tel:`
- relative paths without a protocol are allowed in strict mode

### Layout constraints

The reference parses explicit layout constraints from directives:

- `SameRank`
- `MinLength`
- `Pin`
- `OrderInRank`

These are not generic warnings; they are dedicated IR-level layout hints.

## 6. Layout Pipeline In The Reference

### Public layout output types

Extracted from `mermaid_layout.rs`:

```rust
pub struct LayoutPoint {
    pub x: f64,
    pub y: f64,
}

pub struct LayoutRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub struct LayoutNodeBox {
    pub node_idx: usize,
    pub rect: LayoutRect,
    pub label_rect: Option<LayoutRect>,
    pub rank: usize,
    pub order: usize,
}

pub struct LayoutClusterBox {
    pub cluster_idx: usize,
    pub rect: LayoutRect,
    pub title_rect: Option<LayoutRect>,
}

pub struct LayoutEdgePath {
    pub edge_idx: usize,
    pub waypoints: Vec<LayoutPoint>,
    pub bundle_count: usize,
    pub bundle_members: Vec<usize>,
}

pub struct LayoutStats {
    pub iterations_used: usize,
    pub max_iterations: usize,
    pub budget_exceeded: bool,
    pub crossings: usize,
    pub ranks: usize,
    pub max_rank_width: usize,
    pub total_bends: usize,
    pub position_variance: f64,
}

pub struct DiagramLayout {
    pub nodes: Vec<LayoutNodeBox>,
    pub clusters: Vec<LayoutClusterBox>,
    pub edges: Vec<LayoutEdgePath>,
    pub bounding_box: LayoutRect,
    pub stats: LayoutStats,
    pub degradation: Option<MermaidDegradationPlan>,
}
```

### Pipeline stages

The reference layout engine explicitly models intermediate debug stages through:

- `LayoutStageSnapshot`
- `LayoutTrace`

That means parity also includes debuggability and stage-trace behavior, not
just final node positions.

### Algorithm responsibilities

The reference layout module documents a deterministic Sugiyama-style pipeline:

1. rank assignment
2. ordering within ranks
3. coordinate assignment
4. cluster boundary computation
5. edge routing

It also contains specialized layout entry points for families such as:

- timeline
- gantt
- sankey
- journey
- grid/block

## 7. Rendering Surface In The Reference

### Terminal renderer and render-plan selection

Extracted public render-side entry points from `mermaid_render.rs`:

- `build_adjacency`
- `navigate_direction`
- `select_render_plan`
- `select_fidelity`
- `render_diagram`
- `render_diagram_adaptive`
- `render_mermaid_error_panel`
- `render_mermaid_error_overlay`

Important public/supporting types:

- `SelectionState`
- `RenderPlan`
- `DiagramPalette`
- `MermaidRenderer`
- `DebugOverlayInfo`
- `MermaidErrorRenderReport`

### Render-plan behavior

The renderer explicitly chooses fidelity/adaptation via:

- glyph policy
- render mode
- canvas mode
- render plan selection

This is not a single renderer path. It adapts across compact/normal/rich
surfaces and error/degradation modes.

### Family-specific rendering helpers

The reference renderer contains dedicated helpers for:

- gantt
- packet
- quadrant
- pie
- ER cardinality
- journey score fill
- timeline era fill
- xychart series fill
- sankey flow fill
- block-beta fill

That is a concrete signal that several families need family-specific rendering
parity, not just generic graph rendering.

## 8. Diff Surface In The Reference

The reference includes a full diagram diff surface in `mermaid_diff.rs`.

### Extracted public diff types

```rust
pub enum DiffStatus {
    Added,
    Removed,
    Changed,
    Unchanged,
}

pub struct DiffNode {
    pub id: String,
    pub status: DiffStatus,
    pub node_idx: usize,
    pub old_node_idx: Option<usize>,
}

pub struct DiffEdge {
    pub from_id: String,
    pub to_id: String,
    pub status: DiffStatus,
    pub edge_idx: usize,
    pub old_edge_idx: Option<usize>,
}

pub struct DiagramDiff {
    pub nodes: Vec<DiffNode>,
    pub edges: Vec<DiffEdge>,
    pub new_ir: MermaidDiagramIr,
    pub old_ir: MermaidDiagramIr,
    pub added_nodes: usize,
    pub removed_nodes: usize,
    pub changed_nodes: usize,
    pub added_edges: usize,
    pub removed_edges: usize,
    pub changed_edges: usize,
}
```

### Diff entry points

- `diff_diagrams(old, new) -> DiagramDiff`
- `render_diff(...)`

This is a parity surface frankenmermaid currently does not track in its new
ledger and should be treated as an explicit gap.

## 9. Minimap Surface In The Reference

The reference minimap is a first-class precomputed overlay surface.

### Extracted public minimap types

```rust
pub enum MinimapCorner {
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
}

pub struct MinimapConfig {
    pub max_width: u16,
    pub max_height: u16,
    pub corner: MinimapCorner,
    pub margin: u16,
    pub node_color: PackedRgba,
    pub edge_color: PackedRgba,
    pub viewport_color: PackedRgba,
    pub highlight_color: PackedRgba,
    pub bg_color: PackedRgba,
    pub border_color: PackedRgba,
}
```

### Minimap behavior

`Minimap::new(layout, config)` pre-renders nodes and edges into a Braille
painter, then `render(...)` overlays the current viewport and selection state.

Important extracted behavior:

- aspect ratio is preserved
- border space is accounted for in size fitting
- nodes are rendered after edges
- viewport/highlight overlays are dynamic on top of cached painter content

## 10. DOT Bridge Surface In The Reference

The reference DOT bridge is not a trivial import shim. It is a dedicated parser.

Extracted public API:

- `looks_like_dot(input: &str) -> bool`
- `parse_dot(input: &str) -> Result<MermaidIrParse, DotParseError>`

Extracted public error type:

```rust
pub struct DotParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}
```

Key stated support in code comments:

- `graph` / `digraph`
- node declarations with attributes
- edge declarations
- subgraph / cluster declarations
- DOT shape to `NodeShape` mapping

## 11. Canvas Surface In The Reference

The canvas subsystem is a reusable raster-like drawing substrate, not Mermaid-
specific glue.

Extracted public types:

- `Mode`
- `Painter`
- `Canvas`
- `CanvasRef<'a>`

Representative public methods on `Painter`:

- `new`
- `for_area`
- `ensure_size`
- `clear`
- `point`
- `point_colored`
- `line`
- `line_colored`
- `rect`
- `rect_filled`
- `polygon_filled`
- `circle`
- `render_metaball_field`
- `render_to_buffer`

Parity work that touches canvas-backed render paths should use this as the
behavioral reference surface.

## 12. Immediate Spec-Driven Parity Targets For frankenmermaid

The first implementation targets should come directly from this extracted
reference, not from README marketing text.

Highest-value targets:

1. Families with dedicated reference parsers but current fallback routing in
   frankenmermaid:
   `Sankey`, `XyChart`, `BlockBeta`, `ArchitectureBeta`, `C4*`
2. Dedicated-family IR surfaces that still collapse into generic graph-only
   handling
3. Missing parity surfaces entirely:
   diff, minimap, and conformance-led pipeline registry/coverage reporting

## 13. Next Documents Needed

After this extraction doc, the next required document is:

- `PROPOSED_ARCHITECTURE.md`

That document should map these extracted reference behaviors into
frankenmermaid’s current crate boundaries and decide which legacy surfaces
should be preserved directly versus redesigned idiomatically in Rust.
