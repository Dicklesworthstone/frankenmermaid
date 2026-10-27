//! Core terminal diagram renderer.

use fm_core::{
    ArrowType, GanttTaskType, GraphDirection, MermaidDiagramIr, MermaidRenderMode, MermaidTier,
    NodeShape,
};
use fm_layout::{DiagramLayout, LayoutClusterBox, LayoutEdgePath, LayoutNodeBox, layout_diagram};
use unicode_width::UnicodeWidthStr;

use crate::canvas::Canvas;
use crate::config::{ResolvedConfig, TermRenderConfig};
use crate::glyphs::{BoxGlyphs, ClusterGlyphs, EdgeGlyphs};
use crate::transform::TermTransform;

/// Result of terminal rendering.
#[derive(Debug, Clone)]
pub struct TermRenderResult {
    /// Rendered string output.
    pub output: String,
    /// Number of cells wide.
    pub width: usize,
    /// Number of cells tall.
    pub height: usize,
    /// Effective tier used.
    pub tier: MermaidTier,
    /// Render mode used.
    pub render_mode: MermaidRenderMode,
    /// Node count.
    pub node_count: usize,
    /// Edge count.
    pub edge_count: usize,
    /// How many of those nodes left no box on the canvas at all.
    ///
    /// A terminal is a fixed grid, so a diagram taller or wider than the viewport cannot draw every
    /// node: boxes land on the same cells and each one overwrites the one before it. That is a
    /// legitimate response to an impossible viewport — what is not legitimate is doing it silently.
    /// `node_count` keeps meaning "nodes the layout produced"; this is the number of them the reader
    /// cannot see, so a caller can warn instead of presenting a clean, plausible, incomplete
    /// diagram. Zero whenever the diagram fits.
    pub occluded_node_count: usize,
    /// Requested graph-edge captions that could not fit without hiding protected content.
    /// A paired route callout counts as one visible caption. Intentionally hidden labels do not
    /// contribute to this count.
    pub occluded_edge_label_count: usize,
    /// Layout-quality metrics shown by the optional debug overlay.
    ///
    /// `None` is the default so existing callers and byte snapshots do not pay for diagnostics.
    pub debug_metrics: Option<TermDebugMetrics>,
}

/// Deterministic layout-quality metrics for the terminal debug panel.
///
/// `symmetry_permille` measures horizontal center-of-mass balance: 1000 is perfectly balanced
/// around the layout center and 0 is maximally one-sided. It is intentionally dimensionless so the
/// value is stable across terminal sizes and render modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermDebugMetrics {
    pub crossings: usize,
    pub bends: usize,
    pub symmetry_permille: u16,
    pub nodes: usize,
    pub edges: usize,
    pub ranks: usize,
    pub cycles: usize,
}

impl TermDebugMetrics {
    fn from_layout(ir: &MermaidDiagramIr, layout: &DiagramLayout) -> Self {
        let bends = layout
            .edges
            .iter()
            .filter(|edge| !edge.bundled && !is_invisible_edge(ir, edge))
            .map(|edge| edge.points.len().saturating_sub(2))
            .sum();
        let ranks = layout
            .nodes
            .iter()
            .map(|node| node.rank + 1)
            .max()
            .unwrap_or(0);
        Self {
            crossings: layout.stats.crossing_count,
            bends,
            symmetry_permille: horizontal_symmetry_permille(layout),
            nodes: layout.nodes.len(),
            edges: layout.edges.len(),
            ranks,
            cycles: layout.stats.cycle_count,
        }
    }
}

fn horizontal_symmetry_permille(layout: &DiagramLayout) -> u16 {
    let center = f64::from(layout.bounds.x) + f64::from(layout.bounds.width) / 2.0;
    if !center.is_finite() || !layout.bounds.width.is_finite() || layout.bounds.width <= 0.0 {
        return 1000;
    }

    let mut left = 0.0_f64;
    let mut right = 0.0_f64;
    for node in &layout.nodes {
        let node_center = f64::from(node.bounds.x) + f64::from(node.bounds.width) / 2.0;
        if !node_center.is_finite() {
            continue;
        }
        let offset = node_center - center;
        if offset < 0.0 {
            left += -offset;
        } else {
            right += offset;
        }
    }
    let total = left + right;
    if !total.is_finite() || total <= f64::EPSILON {
        return 1000;
    }
    let score = (1.0 - (left - right).abs() / total).clamp(0.0, 1.0);
    (score * 1000.0).round() as u16
}

fn overlay_debug_panel(
    output: &mut String,
    width: usize,
    height: usize,
    metrics: TermDebugMetrics,
) {
    if width == 0 || height == 0 {
        return;
    }
    let mut lines: Vec<Vec<char>> = output
        .split('\n')
        .map(|line| {
            let mut chars: Vec<char> = line.chars().take(width).collect();
            chars.resize(width, ' ');
            chars
        })
        .collect();
    lines.resize_with(height, || vec![' '; width]);
    lines.truncate(height);

    let labels = [
        "[layout debug]".to_string(),
        format!(
            "crossings={} bends={} symmetry={}/1000",
            metrics.crossings, metrics.bends, metrics.symmetry_permille
        ),
        format!(
            "nodes={} edges={} ranks={} cycles={}",
            metrics.nodes, metrics.edges, metrics.ranks, metrics.cycles
        ),
    ];
    for (row, label) in labels.iter().take(height).enumerate() {
        for (column, ch) in label.chars().take(width).enumerate() {
            lines[row][column] = ch;
        }
    }

    output.clear();
    for (row, line) in lines.iter().enumerate() {
        if row > 0 {
            output.push('\n');
        }
        let end = line
            .iter()
            .rposition(|ch| *ch != ' ')
            .map_or(0, |index| index + 1);
        output.extend(line[..end].iter().copied());
    }
}

/// Count nodes whose box is completely covered by nodes drawn after them.
///
/// Works in CELL space, so it is independent of which render mode produced the output: both modes
/// map layout coordinates onto the same `cell_width` x `cell_height` grid via `scale_x`/`scale_y`.
///
/// Draw order is significant and runs FORWARD, so a later node overwrites an earlier one. This walks
/// the nodes in REVERSE and marks the cells each one claims; a node all of whose cells are already
/// claimed by a later node has been painted over entirely and is invisible to the reader. A node
/// that is merely clipped, or partly overlapped, still shows and is not counted — the count is
/// deliberately conservative, because an over-count would cry wolf on a diagram that renders fine.
fn count_occluded_nodes(
    nodes: &[fm_layout::LayoutNodeBox],
    scale_x: f32,
    scale_y: f32,
    cell_width: usize,
    cell_height: usize,
) -> usize {
    if cell_width == 0 || cell_height == 0 {
        return 0;
    }
    let mut claimed = vec![false; cell_width * cell_height];
    let mut occluded = 0;

    for node in nodes.iter().rev() {
        let x0 = (node.bounds.x * scale_x).floor().max(0.0) as usize;
        let y0 = (node.bounds.y * scale_y).floor().max(0.0) as usize;
        let x1 = (((node.bounds.x + node.bounds.width) * scale_x)
            .ceil()
            .max(0.0) as usize)
            .min(cell_width);
        let y1 = (((node.bounds.y + node.bounds.height) * scale_y)
            .ceil()
            .max(0.0) as usize)
            .min(cell_height);

        // Entirely off-canvas: nothing was drawn, so the reader cannot see it either.
        if x0 >= cell_width || y0 >= cell_height || x0 >= x1 || y0 >= y1 {
            occluded += 1;
            continue;
        }

        let mut all_claimed = true;
        for y in y0..y1 {
            for x in x0..x1 {
                if !claimed[y * cell_width + x] {
                    all_claimed = false;
                    claimed[y * cell_width + x] = true;
                }
            }
        }
        if all_claimed {
            occluded += 1;
        }
    }
    occluded
}

/// Terminal diagram renderer.
pub struct TermRenderer {
    config: ResolvedConfig,
    box_glyphs: BoxGlyphs,
    edge_glyphs: EdgeGlyphs,
    cluster_glyphs: ClusterGlyphs,
}

#[inline]
fn compact_label_width(line: &str) -> usize {
    line.chars().count()
}

/// A `~~~` link shapes the layout and is never drawn, nor is its label.
fn is_invisible_edge(ir: &MermaidDiagramIr, edge_path: &fm_layout::LayoutEdgePath) -> bool {
    ir.edges
        .get(edge_path.edge_index)
        .is_some_and(|edge| edge.arrow == ArrowType::Invisible)
}

const fn edge_marker_ends(arrow: ArrowType) -> (bool, bool) {
    match arrow {
        ArrowType::Line | ArrowType::ThickLine | ArrowType::DottedLine | ArrowType::Invisible => {
            (false, false)
        }
        ArrowType::DoubleArrow
        | ArrowType::DoubleThickArrow
        | ArrowType::DoubleDottedArrow
        // `o--o` / `x--x` are double-ended too (bd-zdpwd).
        | ArrowType::CircleBoth
        | ArrowType::CrossBoth
        | ArrowType::ThickCircleBoth
        | ArrowType::ThickCrossBoth
        | ArrowType::DottedCircleBoth
        | ArrowType::DottedCrossBoth => (true, true),
        ArrowType::HalfArrowTopReverse
        | ArrowType::HalfArrowBottomReverse
        | ArrowType::StickArrowTopReverse
        | ArrowType::StickArrowBottomReverse
        | ArrowType::HalfArrowTopReverseDotted
        | ArrowType::HalfArrowBottomReverseDotted
        | ArrowType::StickArrowTopReverseDotted
        | ArrowType::StickArrowBottomReverseDotted
        | ArrowType::Aggregation
        | ArrowType::Composition
        | ArrowType::Inheritance
        | ArrowType::Lollipop => (true, false),
        ArrowType::Arrow
        | ArrowType::OpenArrow
        | ArrowType::HalfArrowTop
        | ArrowType::HalfArrowBottom
        | ArrowType::StickArrowTop
        | ArrowType::StickArrowBottom
        | ArrowType::ThickArrow
        | ArrowType::DottedArrow
        | ArrowType::DottedOpenArrow
        | ArrowType::DottedCross
        | ArrowType::HalfArrowTopDotted
        | ArrowType::HalfArrowBottomDotted
        | ArrowType::StickArrowTopDotted
        | ArrowType::StickArrowBottomDotted
        | ArrowType::Circle
        | ArrowType::Cross
        | ArrowType::ThickCircle
        | ArrowType::ThickCross
        | ArrowType::DottedCircle
        | ArrowType::AggregationReverse
        | ArrowType::CompositionReverse
        | ArrowType::InheritanceReverse
        | ArrowType::LollipopReverse => (false, true),
    }
}

/// Cell rows a C4 boundary's caption needs beyond its border: the label row and the bracketed
/// type row (bd-c23yq).
///
/// TWO, and only for a boundary that HAS a type. mermaid draws a boundary as label-then-type and
/// both the SVG and canvas arms do; the terminal drew only the label because there was nowhere to
/// put the second row.
const C4_CAPTION_ROWS: usize = 2;

/// One braille cell is two sub-pixels wide and FOUR tall, so a cell row is four canvas rows.
const CANVAS_ROWS_PER_CELL: usize = 4;

/// How far, in CELL rows, a node must be pushed down to clear the caption of the C4 boundary it
/// sits in — and how much taller that boundary must be drawn so the push costs it no content.
///
/// ⚠️ CELL SPACE, NOT PIXELS, and that is the whole reason two earlier attempts failed. This
/// surface scales the diagram to fit its row count, so growing a box upstream is cancelled by the
/// scale it induces. The reservation therefore has to be applied where the rows actually exist.
///
/// ⚠️ THE PUSH IS `1 + CAPTION_ROWS`, NOT `CAPTION_ROWS`. A contained node's box top currently
/// lands on the SAME cell row as its boundary's, so the caption has to clear the node's own top
/// BORDER as well as the rows it needs. Measured: a two-row push put `[SYSTEM]` exactly on that
/// border and the blank-cell guard refused it, which is the same symptom the first attempt at this
/// bead recorded and attributed to the overlay instead.
fn c4_caption_push(ir: &MermaidDiagramIr, cluster: &fm_core::IrCluster) -> usize {
    let _ = ir;
    if cluster.c4_boundary_type.is_some() {
        1 + C4_CAPTION_ROWS
    } else {
        0
    }
}

/// The push that applies to each node index: the SUM over every captioned boundary containing it.
///
/// ⚠️ SUM, NOT MAX. `IrCluster::members` is transitive, so a node two boundaries deep appears in
/// both member lists — and both captions are drawn above it, so it must clear both. Taking the max
/// was tried and measured: with nested boundaries the inner `Core` and `[SYSTEM]` had nowhere to go
/// and only the outer `[ENTERPRISE]` appeared.
fn c4_node_pushes(ir: &MermaidDiagramIr) -> std::collections::HashMap<usize, usize> {
    let mut pushes: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for cluster in &ir.clusters {
        let push = c4_caption_push(ir, cluster);
        if push == 0 {
            continue;
        }
        for member in &cluster.members {
            *pushes.entry(member.0).or_insert(0) += push;
        }
    }
    pushes
}

/// The push that applies to each laid-out cluster box: the sum over its captioned ANCESTORS.
///
/// ⚠️ ANCESTRY IS INFERRED FROM GEOMETRY, because `IrCluster` carries no parent. A boundary drawn
/// strictly inside another is nested in it, which is exactly what the layout has already decided by
/// placing it there. Strict containment on both axes, so a boundary is never treated as its own
/// ancestor and two side-by-side boundaries never nest.
fn c4_cluster_pushes(
    ir: &MermaidDiagramIr,
    clusters: &[fm_layout::LayoutClusterBox],
) -> std::collections::HashMap<usize, usize> {
    let mut pushes: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (index, inner) in clusters.iter().enumerate() {
        let mut total = 0_usize;
        for (other, outer) in clusters.iter().enumerate() {
            if other == index {
                continue;
            }
            let contains = outer.bounds.x <= inner.bounds.x
                && outer.bounds.y <= inner.bounds.y
                && outer.bounds.x + outer.bounds.width >= inner.bounds.x + inner.bounds.width
                && outer.bounds.y + outer.bounds.height >= inner.bounds.y + inner.bounds.height
                && (outer.bounds.width > inner.bounds.width
                    || outer.bounds.height > inner.bounds.height);
            if !contains {
                continue;
            }
            total += ir
                .clusters
                .get(outer.cluster_index)
                .map_or(0, |cluster| c4_caption_push(ir, cluster));
        }
        if total > 0 {
            pushes.insert(index, total);
        }
    }
    pushes
}

impl TermRenderer {
    /// Create a new renderer with resolved configuration.
    #[must_use]
    pub fn new(config: ResolvedConfig) -> Self {
        Self {
            box_glyphs: BoxGlyphs::for_mode(config.glyph_mode),
            edge_glyphs: EdgeGlyphs::for_mode(config.glyph_mode),
            cluster_glyphs: ClusterGlyphs::for_mode(config.glyph_mode),
            config,
        }
    }

    /// Render an IR diagram to terminal output.
    #[must_use]
    pub fn render(&self, ir: &MermaidDiagramIr) -> TermRenderResult {
        let layout = layout_diagram(ir);
        self.render_layout(ir, &layout)
    }

    /// Render a pre-computed layout to terminal output.
    #[must_use]
    pub fn render_layout(&self, ir: &MermaidDiagramIr, layout: &DiagramLayout) -> TermRenderResult {
        let (cell_width, cell_height, scale_x, scale_y) =
            self.fit_cell_dimensions(&layout.bounds, ir.direction, &layout.nodes);

        // Chart marks are data, not graph nodes. Every fidelity uses the same laid-out bars,
        // points and value scale; only the raster resolution and text budget may change.
        if ir.diagram_type == fm_core::DiagramType::XyChart
            && let Some(meta) = &ir.xy_chart_meta
        {
            return self.render_xychart(
                ir,
                meta,
                layout,
                (cell_width, cell_height, scale_x, scale_y),
            );
        }

        if ir.diagram_type == fm_core::DiagramType::Pie
            && let Some(meta) = &ir.pie_meta
        {
            return self.render_pie(ir, meta, layout, (cell_width, cell_height));
        }

        // Outline fidelity stays on the compact cell renderer so specialized geometry (pie
        // wedges, gantt bars, quadrant marks) survives while labels are suppressed. The legacy
        // id-marker skeleton is retained only for direct unit-test coverage.

        // Use cell-based rendering for Compact tier or CellOnly mode.
        if matches!(self.config.tier, MermaidTier::Compact)
            || matches!(self.config.render_mode, MermaidRenderMode::CellOnly)
        {
            return self.render_cell_mode(ir, layout, cell_width, cell_height, scale_x, scale_y);
        }

        // Use sub-cell canvas rendering for higher fidelity.
        self.render_subcell_mode(ir, layout, cell_width, cell_height, scale_x, scale_y)
    }

    /// Return the CGA-backed [`TermTransform`] that maps layout coordinates to terminal cells
    /// for `layout` (`bd-z8971`).
    #[must_use]
    pub fn layout_transform(
        &self,
        layout: &DiagramLayout,
        direction: GraphDirection,
    ) -> Option<TermTransform> {
        let (_cell_width, _cell_height, scale_x, scale_y) =
            self.fit_cell_dimensions(&layout.bounds, direction, &layout.nodes);
        TermTransform::new(scale_x, scale_y)
    }

    /// Map a layout-space point into padded terminal cell coordinates `(col, row)` (`bd-z8971`).
    #[must_use]
    pub fn map_layout_point_to_cells(
        &self,
        layout: &DiagramLayout,
        direction: GraphDirection,
        point: fm_layout::LayoutPoint,
    ) -> (usize, usize) {
        let (_cell_width, _cell_height, scale_x, scale_y) =
            self.fit_cell_dimensions(&layout.bounds, direction, &layout.nodes);
        self.point_to_cells(&point, scale_x, scale_y)
    }

    /// Render skeleton outline (Outline fidelity mode) for tight viewports or dense graphs.
    #[cfg(test)]
    fn render_outline_mode(
        &self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        cell_width: usize,
        cell_height: usize,
        scale_x: f32,
        scale_y: f32,
    ) -> TermRenderResult {
        let mut buffer = CellBuffer::new(cell_width, cell_height);

        for edge_path in &layout.edges {
            self.render_edge_cell(&mut buffer, ir, edge_path, scale_x, scale_y);
        }

        for node_box in &layout.nodes {
            let ir_node = ir.nodes.get(node_box.node_index);
            if ir_node.is_some_and(is_block_beta_space_node) {
                continue;
            }
            let (x, y, w, h) = self.bounds_to_cells(&node_box.bounds, scale_x, scale_y);
            let cy = y + h / 2;
            let raw_id = self.truncate_label(&node_box.node_id);
            let marker = if self.is_selected_node(node_box.node_index) {
                if matches!(self.config.glyph_mode, fm_core::MermaidGlyphMode::Ascii) {
                    format!(">[{raw_id}]")
                } else {
                    format!("▶[{raw_id}]")
                }
            } else if w >= 3 {
                format!("[{raw_id}]")
            } else if matches!(self.config.glyph_mode, fm_core::MermaidGlyphMode::Ascii) {
                String::from("*")
            } else {
                String::from("▪")
            };
            let marker_len = compact_label_width(&marker);
            let start_x = x + w.saturating_sub(marker_len) / 2;
            buffer.set_string(start_x, cy, &marker);
        }

        self.render_generic_diagram_title(&mut buffer.cells, cell_width, ir);
        let mut output = buffer.to_output_string();
        let (legend_rows, legend_width) = append_c4_legend(&mut output, ir);
        let (links_rows, links_width) =
            append_footnote_links(&mut output, ir, self.config.link_mode);
        let (debug_rows, debug_width) = if self.config.debug_overlay {
            append_term_debug_overlay(&mut output, layout)
        } else {
            (0, 0)
        };

        let debug_metrics = self
            .config
            .debug_overlay
            .then(|| TermDebugMetrics::from_layout(ir, layout));
        TermRenderResult {
            output,
            width: cell_width
                .max(legend_width)
                .max(links_width)
                .max(debug_width),
            height: cell_height + legend_rows + links_rows + debug_rows,
            tier: MermaidTier::Compact,
            render_mode: MermaidRenderMode::CellOnly,
            node_count: layout.nodes.len(),
            edge_count: layout.edges.len(),
            occluded_node_count: count_occluded_nodes(
                &layout.nodes,
                scale_x,
                scale_y,
                cell_width,
                cell_height,
            ),
            occluded_edge_label_count: 0,
            debug_metrics,
        }
    }

    /// Render using character cells (Compact mode).
    fn render_cell_mode(
        &self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        cell_width: usize,
        cell_height: usize,
        scale_x: f32,
        scale_y: f32,
    ) -> TermRenderResult {
        // Create character buffer.
        let mut buffer = CellBuffer::new(cell_width, cell_height);

        // Render clusters first (background).
        if self.config.show_clusters {
            for cluster_box in &layout.clusters {
                self.render_cluster_cell(&mut buffer, ir, cluster_box, scale_x, scale_y);
            }
        }

        // Compact state markers occupy one character at their actual layout centre. Route
        // their incident segments to that character rather than leaving an empty circle-sized
        // gap between the connector's geometric port and the displayed dot.
        let state_marker_centers: std::collections::BTreeMap<_, _> = layout
            .nodes
            .iter()
            .filter(|node| self.state_marker_cell_glyph(ir, node.node_index).is_some())
            .map(|node| (node.node_index, node.bounds.center()))
            .collect();

        // Render edges.
        for edge_path in &layout.edges {
            if is_invisible_edge(ir, edge_path) {
                continue;
            }
            let marker_path = ir.edges.get(edge_path.edge_index).and_then(|edge| {
                let from = ir
                    .resolve_endpoint_node(edge.from)
                    .and_then(|id| state_marker_centers.get(&id.0));
                let to = ir
                    .resolve_endpoint_node(edge.to)
                    .and_then(|id| state_marker_centers.get(&id.0));
                if from.is_none() && to.is_none() {
                    return None;
                }
                let mut path = edge_path.clone();
                if let (Some(center), Some(point)) = (from, path.points.first_mut()) {
                    *point = *center;
                }
                if let (Some(center), Some(point)) = (to, path.points.last_mut()) {
                    *point = *center;
                }
                Some(path)
            });
            self.render_edge_cell(
                &mut buffer,
                ir,
                marker_path.as_ref().unwrap_or(edge_path),
                scale_x,
                scale_y,
            );
        }

        for marker in &layout.extensions.sequence_lifecycle_markers {
            match marker.kind {
                fm_layout::LayoutSequenceLifecycleMarkerKind::Destroy => {
                    let x = (marker.center.x * scale_x) as usize;
                    let y = (marker.center.y * scale_y) as usize;
                    if x < cell_width && y < cell_height {
                        buffer.set(x, y, 'X');
                    }
                }
            }
        }

        // Chart-specific terminal rendering.
        if ir.diagram_type == fm_core::DiagramType::Gantt && ir.gantt_meta.is_some() {
            render_gantt_cell(
                &mut buffer,
                ir,
                layout,
                cell_width,
                cell_height,
                self.config.show_labels,
            );
        } else if ir.diagram_type == fm_core::DiagramType::QuadrantChart
            && ir.quadrant_meta.is_some()
        {
            render_quadrant_cell(
                &mut buffer,
                ir,
                layout,
                (cell_width, cell_height),
                scale_x,
                scale_y,
                self.config.show_labels,
            );
        } else if ir.diagram_type == fm_core::DiagramType::Treemap {
            if let Some(meta) = ir.treemap_meta.as_ref() {
                for tile in &layout.extensions.treemap_tiles {
                    let (x, y, w, h) = self.bounds_to_cells(&tile.bounds, scale_x, scale_y);
                    if w >= 3 && h >= 2 {
                        self.draw_shape_border(&mut buffer, x, y, w, h, NodeShape::Rect);
                    }
                    if let Some(item) = meta.nodes.get(tile.node) {
                        let caption = self.truncate_label(&format!(
                            "{} {}",
                            item.label,
                            format_terminal_treemap_value(tile.value)
                        ));
                        let width = caption.chars().count();
                        if width + 2 <= w {
                            let (row, col) = if tile.is_leaf {
                                (y + h / 2, x + (w.saturating_sub(width)) / 2)
                            } else {
                                (y + 1, x + 1)
                            };
                            buffer.set_string(col, row, &caption);
                        }
                    }
                }
            }
        } else if ir.diagram_type == fm_core::DiagramType::Radar {
            if let Some((meta, radar)) =
                ir.radar_meta.as_ref().zip(layout.extensions.radar.as_ref())
            {
                let (cx, cy) = self.point_to_cells(&radar.center, scale_x, scale_y);
                for axis in &radar.axes {
                    let (tx, ty) = self.point_to_cells(&axis.tip, scale_x, scale_y);
                    self.draw_line_cell(
                        &mut buffer,
                        cx,
                        cy,
                        tx,
                        ty,
                        &self.edge_glyphs,
                        false,
                        ArrowType::Line,
                    );
                }
                for curve in &radar.curves {
                    let pts: Vec<(usize, usize)> = curve
                        .points
                        .iter()
                        .map(|pt| self.point_to_cells(pt, scale_x, scale_y))
                        .collect();
                    for idx in 0..pts.len() {
                        let (x0, y0) = pts[idx];
                        let (x1, y1) = pts[(idx + 1) % pts.len()];
                        self.draw_line_cell(
                            &mut buffer,
                            x0,
                            y0,
                            x1,
                            y1,
                            &self.edge_glyphs,
                            false,
                            ArrowType::Line,
                        );
                    }
                }
                for (index, axis) in radar.axes.iter().enumerate() {
                    if let Some(declared) = meta.axes.get(index) {
                        let (ax, ay) = self.point_to_cells(&axis.label_anchor, scale_x, scale_y);
                        let caption = self.truncate_label(declared.display());
                        let width = caption.chars().count();
                        let col = ax
                            .saturating_sub(width / 2)
                            .min(cell_width.saturating_sub(width));
                        let row = ay.min(cell_height.saturating_sub(1));
                        buffer.set_string(col, row, &caption);
                    }
                }
                if meta.show_legend {
                    for (index, curve) in meta.curves.iter().enumerate() {
                        let caption = self.truncate_label(curve.display());
                        buffer.set_string(1, 1 + index, &caption);
                    }
                }
            }
        } else if ir.diagram_type == fm_core::DiagramType::Info {
            let version = concat!("v", env!("CARGO_PKG_VERSION"));
            let row = cell_height / 2;
            let col = cell_width.saturating_sub(version.chars().count()) / 2;
            for (idx, ch) in version.chars().enumerate() {
                if col + idx < cell_width && row < cell_height {
                    buffer.set(col + idx, row, ch);
                }
            }
        } else {
            // Render nodes (foreground).
            for node_box in &layout.nodes {
                self.render_node_cell(&mut buffer, ir, node_box, scale_x, scale_y);
            }
            for node_box in &layout.extensions.sequence_mirror_headers {
                self.render_node_cell(&mut buffer, ir, node_box, scale_x, scale_y);
            }
            for continuation in &layout.extensions.packet_field_continuations {
                let (x, y, w, h) = self.bounds_to_cells(&continuation.bounds, scale_x, scale_y);
                if w >= 3 && h >= 1 {
                    self.draw_shape_border(&mut buffer, x, y, w, h, NodeShape::Rect);
                    if let Some(node) = ir.nodes.get(continuation.node_index)
                        && let Some(label) = self.node_display_label(ir, Some(node), &node.id)
                    {
                        let label = self.truncate_label(&label);
                        let label_len = label.chars().count();
                        let label_x = x + (w.saturating_sub(label_len)) / 2;
                        buffer.set_string(label_x, y + h / 2, &label);
                    }
                }
            }
            for note in &layout.extensions.state_notes {
                let (x, y, w, h) = self.bounds_to_cells(&note.bounds, scale_x, scale_y);
                if w >= 3 && h >= 2 {
                    self.draw_shape_border(&mut buffer, x, y, w, h, NodeShape::Note);
                    let budget = w.saturating_sub(2);
                    for (row, line) in note.text.lines().enumerate() {
                        if row >= h {
                            break;
                        }
                        let clipped: String =
                            self.truncate_label(line).chars().take(budget).collect();
                        buffer.set_string(x + 1, y + row, &clipped);
                    }
                }
            }
            for fragment in &layout.extensions.sequence_fragments {
                let (fx, fy, fw, fh) = self.bounds_to_cells(&fragment.bounds, scale_x, scale_y);
                if fw >= 3 && fh >= 3 {
                    self.draw_shape_border(&mut buffer, fx, fy, fw, fh, NodeShape::Rect);
                    let mut text = format!("{:?}", fragment.kind).to_lowercase();
                    if !fragment.label.is_empty() {
                        text.push(' ');
                        text.push_str(&fragment.label);
                    }
                    let text = self.truncate_label(&text);
                    buffer.set_string(fx + 1, fy + 1, &text);
                }
            }
            for note in &layout.extensions.sequence_notes {
                let (nx, ny, nw, nh) = self.bounds_to_cells(&note.bounds, scale_x, scale_y);
                if nw >= 3 && nh >= 2 {
                    self.draw_shape_border(&mut buffer, nx, ny, nw, nh, NodeShape::Note);
                    let text = self.truncate_label(&note.text);
                    buffer.set_string(nx + 1, ny + 1, &text);
                }
            }
        }

        if self.config.show_labels {
            self.render_generic_diagram_title(&mut buffer.cells, cell_width, ir);
        }
        let mut grid: Vec<Vec<char>> = buffer
            .cells
            .chunks(cell_width)
            .map(<[char]>::to_vec)
            .collect();
        let (edge_labels, occluded_edge_label_count) =
            self.overlay_graph_edge_labels(&mut grid, ir, layout, scale_x, scale_y, &[]);
        let mut output = serialize_cell_labels(&grid, &edge_labels, true);
        let debug_metrics = self
            .config
            .debug_overlay
            .then(|| TermDebugMetrics::from_layout(ir, layout));
        if let Some(metrics) = debug_metrics {
            overlay_debug_panel(&mut output, cell_width, cell_height, metrics);
        }
        let (legend_rows, legend_width) = if self.config.show_labels {
            append_c4_legend(&mut output, ir)
        } else {
            (0, 0)
        };
        let (links_rows, links_width) =
            append_footnote_links(&mut output, ir, self.config.link_mode);

        TermRenderResult {
            output,
            width: cell_width.max(legend_width).max(links_width),
            height: cell_height + legend_rows + links_rows,
            tier: self.config.tier,
            render_mode: self.config.render_mode,
            node_count: layout.nodes.len(),
            edge_count: layout.edges.len(),
            occluded_node_count: count_occluded_nodes(
                &layout.nodes,
                scale_x,
                scale_y,
                cell_width,
                cell_height,
            ),
            occluded_edge_label_count,
            debug_metrics,
        }
    }

    /// Render using sub-cell canvas (Normal/Rich mode).
    fn render_subcell_mode(
        &self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        cell_width: usize,
        cell_height: usize,
        scale_x: f32,
        scale_y: f32,
    ) -> TermRenderResult {
        let (mult_x, mult_y) = self.config.subcell_multiplier();
        let mut canvas = Canvas::new(cell_width, cell_height, self.config.render_mode);

        // Scale factors from layout coordinates to pixels.
        // We scale into the padded area of the cell grid.
        let pixel_scale_x = scale_x * mult_x as f32;
        let pixel_scale_y = scale_y * mult_y as f32;
        let padding_x = self.config.padding * mult_x;
        let padding_y = self.config.padding * mult_y;

        // C4 boundary caption reservations, in cell rows (bd-c23yq). Empty for every diagram
        // that has no captioned boundary, so every other family draws exactly as before.
        let caption_pushes = c4_node_pushes(ir);

        // Render clusters.
        if self.config.show_clusters {
            let ancestor_pushes = c4_cluster_pushes(ir, &layout.clusters);
            for (index, cluster_box) in layout.clusters.iter().enumerate() {
                let own = ir
                    .clusters
                    .get(cluster_box.cluster_index)
                    .map_or(0, |cluster| c4_caption_push(ir, cluster));
                let inherited = ancestor_pushes.get(&index).copied().unwrap_or(0);
                self.render_cluster_canvas(
                    &mut canvas,
                    cluster_box,
                    pixel_scale_x,
                    pixel_scale_y,
                    padding_x,
                    padding_y + inherited * CANVAS_ROWS_PER_CELL,
                    own,
                );
            }
        }

        // PACKET FIELD CONTINUATIONS (bd-t1jj). A packet-beta field that crosses a 32-bit row
        // boundary is laid out as a primary box plus one continuation box per extra row, and the
        // terminal drew only the primary. Measured on `24-47: "CrossingField"`: layout emits the
        // primary at (768, 0, 256, 55) and a continuation at (0, 70, 512, 55), and the terminal drew
        // the 256-wide box alone -- so a 24-bit field was rendered with the extent of an 8-bit one.
        //
        // That is not a missing decoration. A packet diagram exists to show how wide each field is,
        // so dropping two thirds of a field's extent misstates the one thing the diagram is for.
        for continuation in &layout.extensions.packet_field_continuations {
            let x = (continuation.bounds.x * pixel_scale_x) as isize + padding_x as isize;
            let y = (continuation.bounds.y * pixel_scale_y) as isize + padding_y as isize;
            let w = (continuation.bounds.width * pixel_scale_x) as isize;
            let h = (continuation.bounds.height * pixel_scale_y) as isize;
            if w > 2 && h > 2 && x >= 0 && y >= 0 {
                canvas.draw_rect(
                    usize::try_from(x).unwrap_or(0),
                    usize::try_from(y).unwrap_or(0),
                    usize::try_from(w).unwrap_or(0),
                    usize::try_from(h).unwrap_or(0),
                );
            }
        }

        // stateDiagram NOTES (bd-t1jj). `extensions.state_notes` is filled by the state layout arm
        // and drawn by fm-render-svg; the terminal referenced it nowhere, so `note right of X : ...`
        // produced a note that existed in the layout and appeared in no terminal output.
        //
        // Both the box AND the leader are drawn here, unlike the band and cluster overlays which only
        // needed text: those had geometry already on the canvas to attach to, and a note has none. A
        // bare string floating beside a state would read as another node's label rather than as an
        // annotation of that state, which is a different wrong picture, not a smaller one.
        for note in &layout.extensions.state_notes {
            let x = (note.bounds.x * pixel_scale_x) as isize + padding_x as isize;
            let y = (note.bounds.y * pixel_scale_y) as isize + padding_y as isize;
            let w = (note.bounds.width * pixel_scale_x) as isize;
            let h = (note.bounds.height * pixel_scale_y) as isize;
            if w > 2 && h > 2 && x >= 0 && y >= 0 {
                canvas.draw_rect(
                    usize::try_from(x).unwrap_or(0),
                    usize::try_from(y).unwrap_or(0),
                    usize::try_from(w).unwrap_or(0),
                    usize::try_from(h).unwrap_or(0),
                );
            }
            // The leader is what makes the box an annotation OF something rather than a second node.
            let lx0 = (note.leader_start.x * pixel_scale_x) as isize + padding_x as isize;
            let ly0 = (note.leader_start.y * pixel_scale_y) as isize + padding_y as isize;
            let lx1 = (note.leader_end.x * pixel_scale_x) as isize + padding_x as isize;
            let ly1 = (note.leader_end.y * pixel_scale_y) as isize + padding_y as isize;
            // `draw_line` takes isize and clips internally, so negatives are safe to pass and the
            // guard would only drop leaders that are partly on-canvas. A note on the whole diagram
            // has no leader.
            if note.leader_start != note.leader_end {
                canvas.draw_line(lx0, ly0, lx1, ly1);
            }
        }

        // STATE CONCURRENCY-REGION DIVIDERS (bd-dgnm4). `state Big { A --> B  --  C --> D }`
        // declares two regions running in parallel, and the `--` separator is SYNTAX the author
        // wrote, not decoration. The layout records each boundary in
        // `extensions.cluster_dividers` (built by `build_state_cluster_dividers`, keyed on the
        // `__state_region_` subgraphs the `--` creates); fm-render-svg drew one dashed line per
        // divider and fm-render-canvas now does too, but this surface referenced that extension
        // NOWHERE — so the two regions ran together into one box and a terminal reader could not
        // tell there were two.
        //
        // Drawn on the CANVAS layer rather than the text overlay for the reason the gantt today
        // marker gives above: the overlay carries state names, and a rule written there would erase
        // one. A divider is worth less than the label it would cover.
        //
        // DASHED, at the same 3-pixel cadence the sequence lifeline already uses on this canvas,
        // because a SOLID rule here is indistinguishable from the cluster's own border —
        // `render_cluster_canvas` draws that border with `draw_rect`, which is solid. The dash is
        // what carries "region boundary" rather than "another box", exactly as the SVG's
        // `stroke-dasharray("6,4")` does there. It survives sub-cell quantisation: braille packs 2
        // pixels per column, so a 3-on/3-off run alternates set and clear cells rather than filling
        // every one.
        for divider in &layout.extensions.cluster_dividers {
            let x0 = (divider.start.x * pixel_scale_x) as isize + padding_x as isize;
            let y0 = (divider.start.y * pixel_scale_y) as isize + padding_y as isize;
            let x1 = (divider.end.x * pixel_scale_x) as isize + padding_x as isize;
            let y1 = (divider.end.y * pixel_scale_y) as isize + padding_y as isize;
            Self::draw_dashed_segment(&mut canvas, x0, y0, x1, y1);
        }

        // Render layout bands based on their kind.
        for band in &layout.extensions.bands {
            use fm_layout::LayoutBandKind;
            let bx = (band.bounds.x * pixel_scale_x) as isize + padding_x as isize;
            let by = (band.bounds.y * pixel_scale_y) as isize + padding_y as isize;
            let bw = (band.bounds.width * pixel_scale_x) as isize;
            let bh = (band.bounds.height * pixel_scale_y) as isize;

            match band.kind {
                LayoutBandKind::Lane => {
                    // Sequence lifeline: dashed vertical line at band center.
                    let cx = bx + bw / 2;
                    let dash = 3_isize;
                    let mut y_pos = by;
                    while y_pos < by + bh {
                        let end = (y_pos + dash).min(by + bh);
                        canvas.draw_line(cx, y_pos, cx, end);
                        y_pos += dash * 2;
                    }
                }
                LayoutBandKind::Section => {
                    // Gantt section: horizontal top/bottom border lines.
                    canvas.draw_line(bx, by, bx + bw, by);
                    canvas.draw_line(bx, by + bh, bx + bw, by + bh);
                }
                LayoutBandKind::Column => {
                    // Kanban column: vertical separator on right edge.
                    canvas.draw_line(bx + bw, by, bx + bw, by + bh);
                }
            }
        }

        // TREEMAP tiles and the RADAR wheel (bd-dw450). Both families put their whole diagram in a
        // layout extension that only fm-render-svg read, so a `treemap` or `radar-beta` document
        // rendered to the terminal as a completely BLANK canvas — no error, no warning, nothing to
        // suggest the diagram had been understood perfectly one layer earlier.
        //
        // Drawn here, in the geometry pass, for the same reason bands are: this is where layout
        // coordinates become canvas pixels. The labels go on in `overlay_labels` with the rest of
        // the text.
        for tile in &layout.extensions.treemap_tiles {
            let tx = (tile.bounds.x * pixel_scale_x) as isize + padding_x as isize;
            let ty = (tile.bounds.y * pixel_scale_y) as isize + padding_y as isize;
            let tw = (tile.bounds.width * pixel_scale_x) as isize;
            let th = (tile.bounds.height * pixel_scale_y) as isize;
            if tw <= 0 || th <= 0 {
                continue;
            }
            // Outline only, never a fill: a treemap nests, so filling a section would bury every
            // child it contains under its own parent.
            canvas.draw_line(tx, ty, tx + tw, ty);
            canvas.draw_line(tx, ty + th, tx + tw, ty + th);
            canvas.draw_line(tx, ty, tx, ty + th);
            canvas.draw_line(tx + tw, ty, tx + tw, ty + th);
        }

        if let Some(radar) = layout.extensions.radar.as_ref() {
            let to_cell = |x: f32, y: f32| {
                (
                    (x * pixel_scale_x) as isize + padding_x as isize,
                    (y * pixel_scale_y) as isize + padding_y as isize,
                )
            };
            let (cx, cy) = to_cell(radar.center.x, radar.center.y);

            // Graticule. Sampled as a closed polyline rather than drawn as a shape, because the
            // canvas has straight segments and nothing else — 48 samples is well past the point
            // where a braille cell can tell a circle from a polygon.
            let ring_samples = 48_usize;
            for &ring in &radar.rings {
                let mut previous: Option<(isize, isize)> = None;
                let mut first: Option<(isize, isize)> = None;
                for step in 0..ring_samples {
                    let angle = std::f32::consts::TAU * step as f32 / ring_samples as f32;
                    let point = to_cell(
                        ring.mul_add(angle.cos(), radar.center.x),
                        ring.mul_add(angle.sin(), radar.center.y),
                    );
                    if let Some((px, py)) = previous {
                        canvas.draw_line(px, py, point.0, point.1);
                    } else {
                        first = Some(point);
                    }
                    previous = Some(point);
                }
                if let (Some((px, py)), Some((fx, fy))) = (previous, first) {
                    canvas.draw_line(px, py, fx, fy);
                }
            }

            // Spokes.
            for axis in &radar.axes {
                let (tx, ty) = to_cell(axis.tip.x, axis.tip.y);
                canvas.draw_line(cx, cy, tx, ty);
            }

            // Each series, closed. Straight segments here even though SVG smooths the default
            // graticule: a cubic through three braille cells is the same three cells, so spending
            // the arithmetic would buy nothing and the vertices are what carry the data.
            for curve in &radar.curves {
                let points: Vec<(isize, isize)> = curve
                    .points
                    .iter()
                    .map(|point| to_cell(point.x, point.y))
                    .collect();
                for index in 0..points.len() {
                    let (x0, y0) = points[index];
                    let (x1, y1) = points[(index + 1) % points.len()];
                    canvas.draw_line(x0, y0, x1, y1);
                }
            }
        }

        // The gantt TODAY MARKER: a vertical line across the chart at the supplied date (bd-t1jj).
        //
        // `extensions.gantt_day_axis` is the only thing that answers "where is a given DATE on this
        // chart", and this renderer referenced it nowhere — so a terminal gantt drew no today line
        // while the same source exported to SVG drew one, and `todayMarker off`, which a user writes
        // precisely to turn the line off, was equally invisible because there was nothing to turn
        // off.
        //
        // Drawn on the CANVAS layer, not the text overlay below, deliberately: the overlay writes
        // task names and axis dates, and a marker written there would erase one of them. That is the
        // trade bd-u3fo's kanban case warned about — one piece of dropped content swapped for
        // another — and a marker is worth less than the name it would cover.
        //
        // Four conditions, mirroring the SVG and canvas arms so no two backends disagree about
        // whether a marker belongs: the date is supplied (never the clock, see the config field),
        // it parses as a calendar date via the SAME `parse_iso_day_number` the layout used to place
        // the bars (done once in `ResolvedConfig::resolve`, so this file holds no second copy of
        // that arithmetic), it falls inside the charted span, and `todayMarker off` suppresses it.
        // The x comes from `axis.x_for_day` and is never re-derived here — `LayoutGanttDayAxis`'s
        // own doc warns that re-deriving day positions is how a marker and its axis come to
        // disagree about where a day is.
        if let (Some(day), Some(axis)) = (
            self.config.gantt_today_day,
            layout.extensions.gantt_day_axis,
        ) {
            let disabled = ir
                .gantt_meta
                .as_ref()
                .and_then(|meta| meta.today_marker_style.as_deref())
                .is_some_and(|style| style.trim().eq_ignore_ascii_case("off"));
            if !disabled && let Some(marker_x) = axis.x_for_day(day) {
                // `draw_line` takes `isize` and clips internally. Casting to `usize` here would wrap
                // a negative coordinate into an enormous positive one and lose the marker entirely —
                // the same mistake the state-note leader made in this file (1d7324f7).
                let mx = (marker_x * pixel_scale_x) as isize + padding_x as isize;
                let top = (layout.bounds.y * pixel_scale_y) as isize + padding_y as isize;
                let bottom = ((layout.bounds.y + layout.bounds.height) * pixel_scale_y) as isize
                    + padding_y as isize;
                canvas.draw_line(mx, top, mx, bottom);
            }
        }

        // Render activation bars on sequence lifelines.
        for bar in &layout.extensions.activation_bars {
            let bx = (bar.bounds.x * pixel_scale_x) as usize + padding_x;
            let by = (bar.bounds.y * pixel_scale_y) as usize + padding_y;
            let bw = (bar.bounds.width * pixel_scale_x) as usize;
            let bh = (bar.bounds.height * pixel_scale_y) as usize;
            canvas.draw_rect(bx, by, bw.max(1), bh.max(1));
        }

        // Render sequence fragment boxes (loop/alt/par, etc.).
        for fragment in &layout.extensions.sequence_fragments {
            let fx = (fragment.bounds.x * pixel_scale_x) as usize + padding_x;
            let fy = (fragment.bounds.y * pixel_scale_y) as usize + padding_y;
            let fw = (fragment.bounds.width * pixel_scale_x) as usize;
            let fh = (fragment.bounds.height * pixel_scale_y) as usize;
            if fw > 2 && fh > 2 {
                canvas.draw_rect(fx, fy, fw, fh);
            }
        }

        // Render sequence notes as small rectangles.
        for note in &layout.extensions.sequence_notes {
            let nx = (note.bounds.x * pixel_scale_x) as usize + padding_x;
            let ny = (note.bounds.y * pixel_scale_y) as usize + padding_y;
            let nw = (note.bounds.width * pixel_scale_x) as usize;
            let nh = (note.bounds.height * pixel_scale_y) as usize;
            if nw > 1 && nh > 1 {
                canvas.draw_rect(nx, ny, nw.max(1), nh.max(1));
            }
        }

        for marker in &layout.extensions.sequence_lifecycle_markers {
            match marker.kind {
                fm_layout::LayoutSequenceLifecycleMarkerKind::Destroy => {
                    let half = ((marker.size * pixel_scale_x.max(pixel_scale_y)) * 0.5) as isize;
                    let cx = (marker.center.x * pixel_scale_x) as isize + padding_x as isize;
                    let cy = (marker.center.y * pixel_scale_y) as isize + padding_y as isize;
                    let reach = half.max(1);
                    canvas.draw_line(cx - reach, cy - reach, cx + reach, cy + reach);
                    canvas.draw_line(cx - reach, cy + reach, cx + reach, cy - reach);
                }
            }
        }

        // Render edges.
        for edge_path in &layout.edges {
            if is_invisible_edge(ir, edge_path) {
                continue;
            }
            self.render_edge_canvas(
                &mut canvas,
                edge_path,
                pixel_scale_x,
                pixel_scale_y,
                padding_x,
                padding_y,
            );
        }

        // Render nodes.
        for node_box in &layout.nodes {
            self.render_node_canvas(
                &mut canvas,
                ir,
                node_box,
                pixel_scale_x,
                pixel_scale_y,
                padding_x,
                padding_y,
                caption_pushes
                    .get(&node_box.node_index)
                    .copied()
                    .unwrap_or(0),
            );
        }
        for node_box in &layout.extensions.sequence_mirror_headers {
            // Sequence mirror headers are never inside a C4 boundary, so no push applies.
            self.render_node_canvas(
                &mut canvas,
                ir,
                node_box,
                pixel_scale_x,
                pixel_scale_y,
                padding_x,
                padding_y,
                0,
            );
        }

        // Render the canvas straight to its char grid (skipping the String the overlay would re-parse)
        // and overlay labels.
        let base_grid = canvas.render_char_grid();
        let (mut output, occluded_edge_label_count) = self.overlay_labels(
            base_grid,
            ir,
            layout,
            cell_width,
            cell_height,
            scale_x,
            scale_y,
        );
        let debug_metrics = self
            .config
            .debug_overlay
            .then(|| TermDebugMetrics::from_layout(ir, layout));
        if let Some(metrics) = debug_metrics {
            overlay_debug_panel(&mut output, cell_width, cell_height, metrics);
        }
        let (legend_rows, legend_width) = if self.config.show_labels {
            append_c4_legend(&mut output, ir)
        } else {
            (0, 0)
        };
        let (links_rows, links_width) =
            append_footnote_links(&mut output, ir, self.config.link_mode);

        TermRenderResult {
            output,
            width: cell_width.max(legend_width).max(links_width),
            height: cell_height + legend_rows + links_rows,
            tier: self.config.tier,
            render_mode: self.config.render_mode,
            node_count: layout.nodes.len(),
            edge_count: layout.edges.len(),
            occluded_node_count: count_occluded_nodes(
                &layout.nodes,
                scale_x,
                scale_y,
                cell_width,
                cell_height,
            ),
            occluded_edge_label_count,
            debug_metrics,
        }
    }

    /// Canvas size, growing toward the viewport only while nodes are still being lost.
    ///
    /// `base_scale` used to act as an absolute CEILING: the canvas was `bounds * base_scale` and the
    /// terminal could only clamp that DOWN, so a 400x400 terminal produced the same 47x40 canvas —
    /// and lost the same 12 nodes — as an 80x400 one. Enlarging the terminal bought nothing, which
    /// left a user with a clipped diagram no remedy at all.
    ///
    /// The growth is gated on `count_occluded_nodes`, the same measure the result reports, so a
    /// diagram that already fits is left EXACTLY as it was: the loop exits before the first step
    /// and the returned tuple is the old one, byte for byte. Only a diagram that was actually
    /// losing nodes moves, and only until the loss stops or the viewport runs out.
    fn fit_cell_dimensions(
        &self,
        bounds: &fm_layout::LayoutRect,
        direction: GraphDirection,
        nodes: &[fm_layout::LayoutNodeBox],
    ) -> (usize, usize, f32, f32) {
        let base = self.cell_dimensions_scaled(bounds, direction, 1.0);
        let occluded =
            |d: (usize, usize, f32, f32)| count_occluded_nodes(nodes, d.2, d.3, d.0, d.1);
        if occluded(base) == 0 {
            return base;
        }

        let mut best = base;
        let mut mult = 1.0_f32;
        // Doubling reaches any usable terminal in a handful of steps; the loop also stops as soon
        // as growing stops changing the size, which is what "the viewport is exhausted" looks like.
        for _ in 0..8 {
            mult *= 2.0;
            let candidate = self.cell_dimensions_scaled(bounds, direction, mult);
            if candidate.0 == best.0 && candidate.1 == best.1 {
                break;
            }
            best = candidate;
            if occluded(candidate) == 0 {
                break;
            }
        }
        best
    }

    fn cell_dimensions_scaled(
        &self,
        bounds: &fm_layout::LayoutRect,
        direction: GraphDirection,
        mult: f32,
    ) -> (usize, usize, f32, f32) {
        let padding_total = self.config.padding * 2;
        let max_width = self.config.cols.saturating_sub(padding_total).max(1);
        let max_height = self.config.rows.saturating_sub(padding_total).max(1);
        let base_scale = match self.config.tier {
            MermaidTier::Compact => 0.15,
            MermaidTier::Normal => 0.2,
            MermaidTier::Rich | MermaidTier::Auto => 0.25,
        };

        let base_width = (bounds.width * base_scale * mult) as usize;
        let base_height = (bounds.height * base_scale * mult) as usize;

        // Adjust for direction (LR/RL diagrams are wider).
        let (width, height) = match direction {
            GraphDirection::LR | GraphDirection::RL => (
                base_width.max(20).min(max_width),
                base_height.max(10).min(max_height),
            ),
            _ => (
                base_width.max(15).min(max_width),
                base_height.max(15).min(max_height),
            ),
        };

        // Calculate fitted scale factors for the diagram content.
        let scale_x = if bounds.width > 0.0 {
            width as f32 / bounds.width
        } else {
            1.0
        };
        let scale_y = if bounds.height > 0.0 {
            height as f32 / bounds.height
        } else {
            1.0
        };

        (
            width.saturating_add(padding_total),
            height.saturating_add(padding_total),
            scale_x,
            scale_y,
        )
    }

    fn render_cluster_cell(
        &self,
        buffer: &mut CellBuffer,
        ir: &MermaidDiagramIr,
        cluster_box: &LayoutClusterBox,
        scale_x: f32,
        scale_y: f32,
    ) {
        let (x, y, w, h) = self.bounds_to_cells(&cluster_box.bounds, scale_x, scale_y);
        if w < 3 || h < 3 {
            return;
        }

        let glyphs = &self.cluster_glyphs;

        // Top border.
        buffer.set(x, y, glyphs.corner_tl);
        for dx in 1..w - 1 {
            buffer.set(x + dx, y, glyphs.border_h);
        }
        buffer.set(x + w - 1, y, glyphs.corner_tr);

        // Side borders.
        for dy in 1..h - 1 {
            buffer.set(x, y + dy, glyphs.border_v);
            buffer.set(x + w - 1, y + dy, glyphs.border_v);
        }

        // Bottom border.
        buffer.set(x, y + h - 1, glyphs.corner_bl);
        for dx in 1..w - 1 {
            buffer.set(x + dx, y + h - 1, glyphs.border_h);
        }
        buffer.set(x + w - 1, y + h - 1, glyphs.corner_br);

        // Cluster title if available.
        let title_text = cluster_box.title.as_deref().or_else(|| {
            ir.clusters
                .get(cluster_box.cluster_index)
                .and_then(|cluster| cluster.title)
                .and_then(|label_id| ir.labels.get(label_id.0))
                .map(|label| label.text.as_str())
        });

        if let Some(title_text) = title_text {
            let title = self.truncate_label(title_text);
            let title_x = x + 2;
            buffer.set_string(title_x, y, &title);
        }
    }

    fn render_edge_cell(
        &self,
        buffer: &mut CellBuffer,
        ir: &MermaidDiagramIr,
        edge_path: &LayoutEdgePath,
        scale_x: f32,
        scale_y: f32,
    ) {
        if edge_path.points.len() < 2 {
            return;
        }

        let glyphs = &self.edge_glyphs;

        // Get arrow type for this edge.
        let arrow = ir
            .edges
            .get(edge_path.edge_index)
            .map(|e| e.arrow)
            .unwrap_or(ArrowType::Arrow);

        // Draw line segments.
        for window in edge_path.points.windows(2) {
            let (x0, y0) = self.point_to_cells(&window[0], scale_x, scale_y);
            let (x1, y1) = self.point_to_cells(&window[1], scale_x, scale_y);
            self.draw_line_cell(buffer, x0, y0, x1, y1, glyphs, edge_path.reversed, arrow);
        }

        // A RELATION MARKED AT BOTH ENDS carries its target-end marker as a second `ArrowType`
        // (bd-f9t0r), so the SAME `edge_marker_ends` mapping decides that end too and the two
        // answers are OR-ed. Reusing the mapping rather than restating it is what keeps the
        // terminal's idea of "which end is marked" from drifting away from SVG's and canvas's.
        let co_arrow = ir
            .edges
            .get(edge_path.edge_index)
            .and_then(fm_core::IrEdge::co_arrow);
        let (marker_at_start, marker_at_end) = {
            let (start, end) = edge_marker_ends(arrow);
            let (co_start, co_end) = co_arrow.map_or((false, false), edge_marker_ends);
            (start || co_start, end || co_end)
        };

        // The operator determines the semantic endpoint. In particular, UML ownership and
        // inheritance operators place their marker at the source for `o--`, `*--`, and `<|--`,
        // while their reverse spellings place it at the target.
        if marker_at_start && let Some(first) = edge_path.points.first() {
            let (x, y) = self.point_to_cells(first, scale_x, scale_y);
            if edge_path.points.len() >= 2 {
                let next = &edge_path.points[1];
                let (nx, ny) = self.point_to_cells(next, scale_x, scale_y);
                let arrow_char = self.arrowhead_for_direction(nx, ny, x, y, glyphs, arrow);
                buffer.set(x, y, arrow_char);
            }
        }

        if marker_at_end && let Some(last) = edge_path.points.last() {
            let (x, y) = self.point_to_cells(last, scale_x, scale_y);
            let arrow_char = if edge_path.points.len() >= 2 {
                let prev = &edge_path.points[edge_path.points.len() - 2];
                let (px, py) = self.point_to_cells(prev, scale_x, scale_y);
                self.arrowhead_for_direction(px, py, x, y, glyphs, arrow)
            } else {
                glyphs.arrow_right
            };
            buffer.set(x, y, arrow_char);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_line_cell(
        &self,
        buffer: &mut CellBuffer,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
        glyphs: &EdgeGlyphs,
        reversed: bool,
        arrow: ArrowType,
    ) {
        let line_char = if reversed
            || matches!(
                arrow,
                ArrowType::DottedArrow
                    | ArrowType::DottedOpenArrow
                    | ArrowType::HalfArrowTopDotted
                    | ArrowType::HalfArrowBottomDotted
                    | ArrowType::HalfArrowTopReverseDotted
                    | ArrowType::HalfArrowBottomReverseDotted
                    | ArrowType::StickArrowTopDotted
                    | ArrowType::StickArrowBottomDotted
                    | ArrowType::StickArrowTopReverseDotted
                    | ArrowType::StickArrowBottomReverseDotted
                    | ArrowType::DottedLine
                    | ArrowType::DoubleDottedArrow
            ) {
            if x0 == x1 {
                glyphs.dotted_v
            } else {
                glyphs.dotted_h
            }
        } else if x0 == x1 {
            glyphs.line_v
        } else if y0 == y1 {
            glyphs.line_h
        } else if (x1 as isize - x0 as isize).abs() == (y1 as isize - y0 as isize).abs() {
            // Check for perfect diagonal
            if (x1 > x0) == (y1 > y0) {
                glyphs.line_diag_nw
            } else {
                glyphs.line_diag_ne
            }
        } else {
            // Default to horizontal for mixed diagonal segments in cell mode
            glyphs.line_h
        };

        // Bresenham line drawing.
        let dx = (x1 as isize - x0 as isize).abs();
        let dy = -(y1 as isize - y0 as isize).abs();
        let sx = if x0 < x1 { 1_isize } else { -1 };
        let sy = if y0 < y1 { 1_isize } else { -1 };
        let mut err = dx + dy;
        let mut x = x0 as isize;
        let mut y = y0 as isize;

        loop {
            if x >= 0 && y >= 0 {
                buffer.set(x as usize, y as usize, line_char);
            }

            if x == x1 as isize && y == y1 as isize {
                break;
            }

            let e2 = 2 * err;
            if e2 >= dy {
                if x == x1 as isize {
                    break;
                }
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                if y == y1 as isize {
                    break;
                }
                err += dx;
                y += sy;
            }
        }
    }

    fn arrowhead_for_direction(
        &self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        glyphs: &EdgeGlyphs,
        arrow: ArrowType,
    ) -> char {
        let dx = to_x as isize - from_x as isize;
        let dy = to_y as isize - from_y as isize;

        match arrow {
            ArrowType::Circle => glyphs.circle_head,
            ArrowType::Cross | ArrowType::DottedCross => glyphs.cross_head,
            _ => {
                if dx.abs() > dy.abs() {
                    if dx > 0 {
                        glyphs.arrow_right
                    } else {
                        glyphs.arrow_left
                    }
                } else if dy > 0 {
                    glyphs.arrow_down
                } else {
                    glyphs.arrow_up
                }
            }
        }
    }

    fn state_marker_cell_glyph(&self, ir: &MermaidDiagramIr, node_index: usize) -> Option<char> {
        if ir.diagram_type != fm_core::DiagramType::State {
            return None;
        }
        let node = ir.nodes.get(node_index)?;
        if !ir.is_textless_ornament_node(node) {
            return None;
        }
        let ascii = matches!(self.config.glyph_mode, fm_core::MermaidGlyphMode::Ascii);
        match node.shape {
            NodeShape::FilledCircle => Some(if ascii { '*' } else { '●' }),
            NodeShape::DoubleCircle => Some(if ascii { '@' } else { '◉' }),
            _ => None,
        }
    }

    fn render_node_cell(
        &self,
        buffer: &mut CellBuffer,
        ir: &MermaidDiagramIr,
        node_box: &LayoutNodeBox,
        scale_x: f32,
        scale_y: f32,
    ) {
        let ir_node = ir.nodes.get(node_box.node_index);
        if ir_node.is_some_and(is_block_beta_space_node) {
            return;
        }

        if let Some(glyph) = self.state_marker_cell_glyph(ir, node_box.node_index) {
            let (cx, cy) = self.point_to_cells(&node_box.bounds.center(), scale_x, scale_y);
            buffer.set(cx, cy, glyph);
            return;
        }

        let (x, y, w, h) = self.bounds_to_cells(&node_box.bounds, scale_x, scale_y);
        if w < 3 || h < 1 {
            return;
        }

        // Get node shape.
        let shape = ir_node.map(|n| n.shape).unwrap_or(NodeShape::Rect);

        if shape == NodeShape::HorizontalBar {
            for row in y..y + h {
                for col in x..x + w {
                    buffer.set(col, row, self.box_glyphs.block_full);
                }
            }
            return;
        }

        // Draw shape border.
        self.draw_shape_border(buffer, x, y, w, h, shape);
        if self.is_selected_node(node_box.node_index) && w >= 2 && h >= 2 {
            let (tl, tr, bl, br) =
                if matches!(self.config.glyph_mode, fm_core::MermaidGlyphMode::Ascii) {
                    ('#', '#', '#', '#')
                } else {
                    ('╔', '╗', '╚', '╝')
                };
            buffer.set(x, y, tl);
            buffer.set(x + w - 1, y, tr);
            buffer.set(x, y + h - 1, bl);
            buffer.set(x + w - 1, y + h - 1, br);
        }

        // Get label.
        let Some(label) = self.node_display_label(ir, ir_node, &node_box.node_id) else {
            return;
        };

        // Center label in node.
        let lines: Vec<&str> = label.lines().collect();
        let start_y = y + (h.saturating_sub(lines.len())) / 2;

        for (i, line) in lines.iter().enumerate() {
            let label_len = compact_label_width(line);
            let label_x = x + (w.saturating_sub(label_len)) / 2;
            buffer.set_string(label_x, start_y + i, line);
        }
    }

    fn draw_shape_border(
        &self,
        buffer: &mut CellBuffer,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        shape: NodeShape,
    ) {
        let glyphs = &self.box_glyphs;

        match shape {
            NodeShape::Diamond => {
                let mid_x = x + w / 2;
                let mid_y = y + h / 2;
                buffer.set(mid_x, y, '/');
                buffer.set(mid_x + 1, y, '\\');
                buffer.set(x, mid_y, '<');
                buffer.set(x + w - 1, mid_y, '>');
                buffer.set(mid_x, y + h - 1, '\\');
                buffer.set(mid_x + 1, y + h - 1, '/');
            }
            NodeShape::Circle | NodeShape::DoubleCircle | NodeShape::CrossedCircle => {
                let mid_y = y + h / 2;
                buffer.set(x, mid_y, '(');
                buffer.set(x + w - 1, mid_y, ')');
                for dx in 1..w.saturating_sub(1) {
                    buffer.set(x + dx, y, glyphs.horizontal);
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
            }
            NodeShape::Rounded | NodeShape::Stadium | NodeShape::Cloud => {
                buffer.set(x, y, '(');
                buffer.set(x + w.saturating_sub(1), y, ')');
                buffer.set(x, y + h.saturating_sub(1), '(');
                buffer.set(x + w.saturating_sub(1), y + h.saturating_sub(1), ')');
                for dx in 1..w.saturating_sub(1) {
                    buffer.set(x + dx, y, glyphs.horizontal);
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
                for dy in 1..h.saturating_sub(1) {
                    buffer.set(x, y + dy, glyphs.vertical);
                    buffer.set(x + w.saturating_sub(1), y + dy, glyphs.vertical);
                }
            }
            NodeShape::Hexagon => {
                buffer.set(x, y + h / 2, '<');
                buffer.set(x + w.saturating_sub(1), y + h / 2, '>');
                for dx in 1..w.saturating_sub(1) {
                    buffer.set(x + dx, y, glyphs.horizontal);
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
            }
            NodeShape::Subroutine => {
                // Double vertical borders on left and right.
                buffer.set(x, y, glyphs.top_left);
                buffer.set(x + w.saturating_sub(1), y, glyphs.top_right);
                buffer.set(x, y + h.saturating_sub(1), glyphs.bottom_left);
                buffer.set(
                    x + w.saturating_sub(1),
                    y + h.saturating_sub(1),
                    glyphs.bottom_right,
                );
                for dx in 1..w.saturating_sub(1) {
                    buffer.set(x + dx, y, glyphs.horizontal);
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
                for dy in 1..h.saturating_sub(1) {
                    buffer.set(x, y + dy, glyphs.vertical);
                    buffer.set(x + w.saturating_sub(1), y + dy, glyphs.vertical);
                    // Inner vertical lines for subroutine double-border.
                    if w > 3 {
                        buffer.set(x + 1, y + dy, glyphs.vertical);
                        buffer.set(x + w.saturating_sub(2), y + dy, glyphs.vertical);
                    }
                }
            }
            NodeShape::Asymmetric | NodeShape::Tag => {
                // Flag/tag shape: rectangle with pointed right side.
                buffer.set(x, y, glyphs.top_left);
                buffer.set(x, y + h.saturating_sub(1), glyphs.bottom_left);
                buffer.set(x + w.saturating_sub(1), y + h / 2, '>');
                for dx in 1..w.saturating_sub(1) {
                    buffer.set(x + dx, y, glyphs.horizontal);
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
                for dy in 1..h.saturating_sub(1) {
                    buffer.set(x, y + dy, glyphs.vertical);
                }
            }
            NodeShape::Cylinder => {
                // Database cylinder: curved top/bottom, straight sides.
                buffer.set(x, y, '(');
                buffer.set(x + w.saturating_sub(1), y, ')');
                buffer.set(x, y + h.saturating_sub(1), '(');
                buffer.set(x + w.saturating_sub(1), y + h.saturating_sub(1), ')');
                for dx in 1..w.saturating_sub(1) {
                    buffer.set(x + dx, y, glyphs.horizontal);
                    // Double line at top to suggest cylinder cap.
                    if h > 2 {
                        buffer.set(x + dx, y + 1, glyphs.horizontal);
                    }
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
                for dy in 2..h.saturating_sub(1) {
                    buffer.set(x, y + dy, glyphs.vertical);
                    buffer.set(x + w.saturating_sub(1), y + dy, glyphs.vertical);
                }
            }
            NodeShape::Trapezoid => {
                // Wider top, narrower bottom.
                let inset = w / 6;
                buffer.set(x, y, '/');
                buffer.set(x + w.saturating_sub(1), y, '\\');
                buffer.set(x + inset, y + h.saturating_sub(1), '\\');
                buffer.set(
                    x + w.saturating_sub(1).saturating_sub(inset),
                    y + h.saturating_sub(1),
                    '/',
                );
                for dx in 1..w.saturating_sub(1) {
                    buffer.set(x + dx, y, glyphs.horizontal);
                }
                for dx in (inset + 1)..w.saturating_sub(1).saturating_sub(inset) {
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
            }
            NodeShape::InvTrapezoid => {
                // Narrower top, wider bottom.
                let inset = w / 6;
                buffer.set(x + inset, y, '\\');
                buffer.set(x + w.saturating_sub(1).saturating_sub(inset), y, '/');
                buffer.set(x, y + h.saturating_sub(1), '\\');
                buffer.set(x + w.saturating_sub(1), y + h.saturating_sub(1), '/');
                for dx in (inset + 1)..w.saturating_sub(1).saturating_sub(inset) {
                    buffer.set(x + dx, y, glyphs.horizontal);
                }
                for dx in 1..w.saturating_sub(1) {
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
            }
            NodeShape::Parallelogram => {
                let inset = w / 5;
                for dx in inset..w.saturating_sub(1) {
                    buffer.set(x + dx, y, glyphs.horizontal);
                }
                for dx in 0..w.saturating_sub(inset) {
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
                buffer.set(x + inset, y, '/');
                buffer.set(x, y + h.saturating_sub(1), '/');
            }
            NodeShape::InvParallelogram => {
                let inset = w / 5;
                for dx in 0..w.saturating_sub(inset) {
                    buffer.set(x + dx, y, glyphs.horizontal);
                }
                for dx in inset..w.saturating_sub(1) {
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
                buffer.set(x + w.saturating_sub(1).saturating_sub(inset), y, '\\');
                buffer.set(x + w.saturating_sub(1), y + h.saturating_sub(1), '\\');
            }
            NodeShape::Triangle => {
                let mid_x = x + w / 2;
                buffer.set(mid_x, y, '^');
                for dx in 0..w {
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
                buffer.set(x, y + h.saturating_sub(1), '/');
                buffer.set(x + w.saturating_sub(1), y + h.saturating_sub(1), '\\');
            }
            NodeShape::Pentagon | NodeShape::Star => {
                // Pentagon/star approximation: use hexagon-like shape.
                buffer.set(x, y + h / 2, '<');
                buffer.set(x + w.saturating_sub(1), y + h / 2, '>');
                for dx in 1..w.saturating_sub(1) {
                    buffer.set(x + dx, y, glyphs.horizontal);
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
                for dy in 1..h.saturating_sub(1) {
                    buffer.set(x, y + dy, glyphs.vertical);
                    buffer.set(x + w.saturating_sub(1), y + dy, glyphs.vertical);
                }
            }
            NodeShape::Note => {
                // Note shape: rectangle with folded corner.
                buffer.set(x, y, glyphs.top_left);
                buffer.set(x + w.saturating_sub(1), y, '+');
                buffer.set(x, y + h.saturating_sub(1), glyphs.bottom_left);
                buffer.set(
                    x + w.saturating_sub(1),
                    y + h.saturating_sub(1),
                    glyphs.bottom_right,
                );
                for dx in 1..w.saturating_sub(1) {
                    buffer.set(x + dx, y, glyphs.horizontal);
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
                for dy in 1..h.saturating_sub(1) {
                    buffer.set(x, y + dy, glyphs.vertical);
                    buffer.set(x + w.saturating_sub(1), y + dy, glyphs.vertical);
                }
            }
            // No border: a text block is drawn as its label alone on every surface.
            NodeShape::TextBlock => {}
            _ => {
                // Standard rectangle (Rect and any unhandled shapes).
                buffer.set(x, y, glyphs.top_left);
                buffer.set(x + w.saturating_sub(1), y, glyphs.top_right);
                buffer.set(x, y + h.saturating_sub(1), glyphs.bottom_left);
                buffer.set(
                    x + w.saturating_sub(1),
                    y + h.saturating_sub(1),
                    glyphs.bottom_right,
                );
                for dx in 1..w.saturating_sub(1) {
                    buffer.set(x + dx, y, glyphs.horizontal);
                    buffer.set(x + dx, y + h.saturating_sub(1), glyphs.horizontal);
                }
                for dy in 1..h.saturating_sub(1) {
                    buffer.set(x, y + dy, glyphs.vertical);
                    buffer.set(x + w.saturating_sub(1), y + dy, glyphs.vertical);
                }
            }
        }
    }

    /// Draw a DASHED segment on the pixel canvas, in the 3-on/3-off cadence the sequence lifeline
    /// already uses here (bd-dgnm4).
    ///
    /// Written parametrically rather than as a horizontal special case even though every divider
    /// `build_state_cluster_dividers` emits today is horizontal (it sets `start.y == end.y`). The
    /// geometry it consumes is a general `LayoutPoint` pair, and a horizontal-only implementation
    /// would silently draw a future vertical or diagonal divider in the wrong place instead of
    /// failing — the same shape as the state-note leader bug that cast a negative coordinate to
    /// `usize` and lost the line entirely.
    ///
    /// `draw_line` takes `isize` and clips internally, so off-canvas dashes are dropped rather than
    /// wrapped.
    fn draw_dashed_segment(canvas: &mut Canvas, x0: isize, y0: isize, x1: isize, y1: isize) {
        /// Pixels drawn, then pixels skipped. Matches the lifeline dash on this canvas.
        const DASH: isize = 3;

        let dx = x1 - x0;
        let dy = y1 - y0;
        let steps = dx.abs().max(dy.abs());
        if steps == 0 {
            canvas.draw_line(x0, y0, x0, y0);
            return;
        }

        let mut step = 0_isize;
        while step <= steps {
            let end = (step + DASH - 1).min(steps);
            // Endpoints of this dash, interpolated along the segment. Integer arithmetic keeps the
            // result deterministic across platforms, which the terminal output determinism test
            // depends on.
            let ax = x0 + dx * step / steps;
            let ay = y0 + dy * step / steps;
            let bx = x0 + dx * end / steps;
            let by = y0 + dy * end / steps;
            canvas.draw_line(ax, ay, bx, by);
            step += DASH * 2;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_cluster_canvas(
        &self,
        canvas: &mut Canvas,
        cluster_box: &LayoutClusterBox,
        scale_x: f32,
        scale_y: f32,
        padding_x: usize,
        padding_y: usize,
        // Cell rows this boundary's caption reserves, which it must also GROW BY so the push
        // applied to its members costs it no content (bd-c23yq).
        caption_push: usize,
    ) {
        let x = (cluster_box.bounds.x * scale_x) as usize + padding_x;
        let y = (cluster_box.bounds.y * scale_y) as usize + padding_y;
        let w = (cluster_box.bounds.width * scale_x) as usize;
        let h =
            (cluster_box.bounds.height * scale_y) as usize + caption_push * CANVAS_ROWS_PER_CELL;

        if w > 2 && h > 2 {
            canvas.draw_rect(x, y, w, h);
        }
    }

    fn render_edge_canvas(
        &self,
        canvas: &mut Canvas,
        edge_path: &LayoutEdgePath,
        scale_x: f32,
        scale_y: f32,
        padding_x: usize,
        padding_y: usize,
    ) {
        for window in edge_path.points.windows(2) {
            let x0 = (window[0].x * scale_x) as isize + padding_x as isize;
            let y0 = (window[0].y * scale_y) as isize + padding_y as isize;
            let x1 = (window[1].x * scale_x) as isize + padding_x as isize;
            let y1 = (window[1].y * scale_y) as isize + padding_y as isize;
            canvas.draw_line(x0, y0, x1, y1);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_node_canvas(
        &self,
        canvas: &mut Canvas,
        ir: &MermaidDiagramIr,
        node_box: &LayoutNodeBox,
        scale_x: f32,
        scale_y: f32,
        padding_x: usize,
        padding_y: usize,
        // Cell rows this node is pushed down to clear its C4 boundary's caption (bd-c23yq). Zero
        // for every node that is not inside a captioned boundary, which is every node in every
        // other diagram type — so nothing else moves.
        caption_push: usize,
    ) {
        let ir_node = ir.nodes.get(node_box.node_index);
        if ir_node.is_some_and(is_block_beta_space_node) {
            return;
        }

        let x = (node_box.bounds.x * scale_x) as usize + padding_x;
        // Pushed in CANVAS rows, four per cell row, so the shift lands on a cell boundary and the
        // node's border does not straddle two rows.
        let y = (node_box.bounds.y * scale_y) as usize
            + padding_y
            + caption_push * CANVAS_ROWS_PER_CELL;
        let w = (node_box.bounds.width * scale_x) as usize;
        // Height is NOT reduced by the push. Shrinking it was tried and measured: the node's own
        // content rows stop fitting `max_content_row` and `Alice` / `<<person>>` vanished from the
        // output entirely — a caption bought with the contained content is not a fix. The boundary
        // grows instead, below.
        let h = (node_box.bounds.height * scale_y) as usize;

        let shape = ir_node.map(|n| n.shape).unwrap_or(NodeShape::Rect);

        match shape {
            NodeShape::Circle | NodeShape::FilledCircle | NodeShape::DoubleCircle => {
                // Scale the layout circle along both axes, just like its connector ports.
                // Taking min(w, h) after scaling shrank the outline away from those ports.
                let center = node_box.bounds.center();
                let radius = node_box.bounds.width.min(node_box.bounds.height) * 0.5;
                let cx = (center.x * scale_x) as isize + padding_x as isize;
                let cy = (center.y * scale_y) as isize
                    + padding_y as isize
                    + (caption_push * CANVAS_ROWS_PER_CELL) as isize;
                let rx = (radius * scale_x) as isize;
                let ry = (radius * scale_y) as isize;
                if shape == NodeShape::FilledCircle {
                    canvas.fill_ellipse(cx, cy, rx, ry);
                } else {
                    canvas.draw_ellipse(cx, cy, rx, ry);
                }
                if shape == NodeShape::DoubleCircle {
                    let inner_radius = (radius - 4.0).max(radius * 0.5);
                    let inner_x = (inner_radius * scale_x) as isize;
                    let inner_y = (inner_radius * scale_y) as isize;
                    if ir.diagram_type == fm_core::DiagramType::State {
                        canvas.fill_ellipse(cx, cy, inner_x, inner_y);
                    } else {
                        canvas.draw_ellipse(cx, cy, inner_x, inner_y);
                    }
                }
            }
            NodeShape::HorizontalBar => {
                canvas.fill_rect(x, y, w.max(1), h.max(1));
            }
            NodeShape::Diamond => {
                // Draw diamond as four lines.
                let mid_x = (x + w / 2) as isize;
                let mid_y = (y + h / 2) as isize;
                let top = y as isize;
                let bottom = (y + h) as isize;
                let left = x as isize;
                let right = (x + w) as isize;
                canvas.draw_line(mid_x, top, right, mid_y);
                canvas.draw_line(right, mid_y, mid_x, bottom);
                canvas.draw_line(mid_x, bottom, left, mid_y);
                canvas.draw_line(left, mid_y, mid_x, top);
            }
            NodeShape::Parallelogram => {
                let inset = (w as f32 * fm_core::SLANTED_SHAPE_INSET_RATIO) as isize;
                let top = y as isize;
                let bottom = (y + h) as isize;
                let left = x as isize;
                let right = (x + w) as isize;
                canvas.draw_line(left + inset, top, right, top);
                canvas.draw_line(right, top, right - inset, bottom);
                canvas.draw_line(right - inset, bottom, left, bottom);
                canvas.draw_line(left, bottom, left + inset, top);
            }
            NodeShape::InvParallelogram => {
                let inset = (w as f32 * fm_core::SLANTED_SHAPE_INSET_RATIO) as isize;
                let top = y as isize;
                let bottom = (y + h) as isize;
                let left = x as isize;
                let right = (x + w) as isize;
                canvas.draw_line(left, top, right - inset, top);
                canvas.draw_line(right - inset, top, right, bottom);
                canvas.draw_line(right, bottom, left + inset, bottom);
                canvas.draw_line(left + inset, bottom, left, top);
            }
            NodeShape::Trapezoid => {
                let inset = (w as f32 * fm_core::SLANTED_SHAPE_INSET_RATIO) as isize;
                let top = y as isize;
                let bottom = (y + h) as isize;
                let left = x as isize;
                let right = (x + w) as isize;
                canvas.draw_line(left + inset, top, right - inset, top);
                canvas.draw_line(right - inset, top, right, bottom);
                canvas.draw_line(right, bottom, left, bottom);
                canvas.draw_line(left, bottom, left + inset, top);
            }
            NodeShape::InvTrapezoid => {
                let inset = (w as f32 * fm_core::SLANTED_SHAPE_INSET_RATIO) as isize;
                let top = y as isize;
                let bottom = (y + h) as isize;
                let left = x as isize;
                let right = (x + w) as isize;
                canvas.draw_line(left, top, right, top);
                canvas.draw_line(right, top, right - inset, bottom);
                canvas.draw_line(right - inset, bottom, left + inset, bottom);
                canvas.draw_line(left + inset, bottom, left, top);
            }
            NodeShape::Hexagon => {
                let inset = (w as f32 * 0.15) as isize;
                let top = y as isize;
                let bottom = (y + h) as isize;
                let left = x as isize;
                let right = (x + w) as isize;
                let mid_y = (y + h / 2) as isize;
                canvas.draw_line(left + inset, top, right - inset, top);
                canvas.draw_line(right - inset, top, right, mid_y);
                canvas.draw_line(right, mid_y, right - inset, bottom);
                canvas.draw_line(right - inset, bottom, left + inset, bottom);
                canvas.draw_line(left + inset, bottom, left, mid_y);
                canvas.draw_line(left, mid_y, left + inset, top);
            }
            NodeShape::Rounded | NodeShape::Stadium | NodeShape::Cloud => {
                // Rounded rectangle: draw rect + round the corners with arcs.
                canvas.draw_rect(x, y, w.max(1), h.max(1));
            }
            NodeShape::Subroutine => {
                // Double-bordered rectangle.
                canvas.draw_rect(x, y, w.max(1), h.max(1));
                if w > 4 {
                    let inner_x = x + 2;
                    canvas.draw_line(
                        inner_x as isize,
                        y as isize,
                        inner_x as isize,
                        (y + h) as isize,
                    );
                    let inner_right = x + w - 2;
                    canvas.draw_line(
                        inner_right as isize,
                        y as isize,
                        inner_right as isize,
                        (y + h) as isize,
                    );
                }
            }
            NodeShape::Asymmetric | NodeShape::Tag => {
                // Flag shape: rect with pointed right side.
                let top = y as isize;
                let bottom = (y + h) as isize;
                let left = x as isize;
                let right = (x + w) as isize;
                let mid_y = (y + h / 2) as isize;
                let point = (w as f32 * 0.2) as isize;
                canvas.draw_line(left, top, right - point, top);
                canvas.draw_line(right - point, top, right, mid_y);
                canvas.draw_line(right, mid_y, right - point, bottom);
                canvas.draw_line(right - point, bottom, left, bottom);
                canvas.draw_line(left, bottom, left, top);
            }
            NodeShape::Cylinder => {
                // Database shape: rect with elliptical top.
                canvas.draw_rect(x, y, w.max(1), h.max(1));
                // Draw second horizontal line near top to suggest cylinder cap.
                if h > 3 {
                    canvas.draw_line(
                        x as isize,
                        (y + 2) as isize,
                        (x + w) as isize,
                        (y + 2) as isize,
                    );
                }
            }
            NodeShape::Triangle => {
                let mid_x = (x + w / 2) as isize;
                let top = y as isize;
                let bottom = (y + h) as isize;
                let left = x as isize;
                let right = (x + w) as isize;
                canvas.draw_line(mid_x, top, right, bottom);
                canvas.draw_line(right, bottom, left, bottom);
                canvas.draw_line(left, bottom, mid_x, top);
            }
            NodeShape::Note => {
                // Rectangle with folded corner.
                let fold = (w.min(h) as f32 * 0.2) as isize;
                let top = y as isize;
                let bottom = (y + h) as isize;
                let left = x as isize;
                let right = (x + w) as isize;
                canvas.draw_line(left, top, right - fold, top);
                canvas.draw_line(right - fold, top, right, top + fold);
                canvas.draw_line(right, top + fold, right, bottom);
                canvas.draw_line(right, bottom, left, bottom);
                canvas.draw_line(left, bottom, left, top);
            }
            // A text block is its label alone — no outline — exactly as fm-render-svg draws it
            // (an empty group) and the canvas backend skips it. Outlining it here boxed every
            // flowchart `text` node, every fishbone sub-cause and every tree-view entry.
            NodeShape::TextBlock => {}
            _ => {
                canvas.draw_rect(x, y, w.max(1), h.max(1));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn overlay_labels(
        &self,
        mut lines: Vec<Vec<char>>,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        cell_width: usize,
        cell_height: usize,
        scale_x: f32,
        scale_y: f32,
    ) -> (String, usize) {
        // `lines` arrives as the canvas's char grid (`Canvas::render_char_grid`) — one row per cell
        // row — instead of a rendered `String` this fn used to re-parse with `lines().chars().collect()`.
        // Skips a full encode+decode of the whole raster.

        // Pad lines to consistent width.
        for line in &mut lines {
            while line.len() < cell_width {
                line.push(' ');
            }
        }
        while lines.len() < cell_height {
            lines.push(vec![' '; cell_width]);
        }

        // Overlay BAND labels (kanban columns, gantt sections).
        //
        // The band loop in `render_subcell_mode` draws each kind's GEOMETRY -- a dashed lifeline, a
        // section's rules, a column's separator -- and no text for any kind, while `LayoutBand`
        // carries a `label` that fm-render-svg does draw.
        //
        // ⚠️ CORRECTION to what this comment used to say: it cited a kanban column named `Alpha` as
        // the motivating case. That was wrong. A parsed kanban reaches the renderer with ZERO bands
        // — `layout_diagram_kanban_traced` returns before the band block when the columns are
        // declared lanes — so its columns are CLUSTERS and are handled by the cluster-title overlay
        // below. This loop's live users are journey lanes and gantt sections.
        //
        // Drawing this was only made safe by the layout half of bd-u3fo. Before it, the Column
        // band's label was `format!("column {}", rank + 1)`, so an overlay here would have printed
        // "column 1" over the user's own name -- a placeholder rendered confidently, which is worse
        // than a blank.
        for band in &layout.extensions.bands {
            if band.label.is_empty() {
                continue;
            }
            let (x, y, w, h) = self.bounds_to_cells(&band.bounds, scale_x, scale_y);
            if w < 3 || y >= lines.len() {
                continue;
            }
            // FIT the label to the band rather than dropping it. A gantt section band spans the
            // whole chart and has room to spare; a kanban column band is one card wide, so an
            // all-or-nothing guard silently discarded exactly the labels this fix exists to draw.
            // The band's own width is the budget, so a label can still never spill across a
            // neighbouring band's geometry.
            // FIT the label to the band rather than dropping it. A gantt section band spans the
            // whole chart and has room to spare; a kanban column band is one card wide, so an
            // all-or-nothing guard silently discarded exactly the labels this fix exists to draw.
            // The band's own width is the budget, so a label can still never spill across a
            // neighbouring band's geometry.
            //
            // ⚠️ THE CAP IS LOAD-BEARING AND TRUNCATES (bd-039t). Measured: a gantt section band is
            // SIX cells wide here, so `Build` draws as `Buil` and `Engineering` as `Engi` — the
            // surviving prefix is 4 characters regardless of name length. Removing the cap was
            // TRIED and reverted: writing along the row displaces content and breaks both
            // `band_label_overlay_does_not_invent_or_displace` and
            // `a_sequence_diagram_is_unaffected_by_the_axis_overlay`, and it did not even fix the
            // gantt case. Pinned by `a_gantt_section_name_is_drawn_in_full`.
            //
            // ⚠️ WHAT IS ACTUALLY IN THE WAY — corrected after reading the grid the pinned
            // reproducer dumps, because the earlier note here named the wrong obstacle and would
            // send the next attempt at the wrong thing. It said the cells past the band hold "the
            // band's own horizontal rule". They do not. Row 2 of that dump is:
            //
            //     ⠀⠀Engi2026-01-01⠀⠀2026-01-02⠀⠀2026-01-03⠀⠀…
            //
            // `Engi` sits at columns 2-5 and the first AXIS TICK begins at column 6: the section
            // label and the gantt date axis compete for THE SAME ROW. That is why lifting the cap
            // broke the controls — it overwrote real content, not decoration — and why widening the
            // budget alone cannot work, since it only moves the collision.
            //
            // A fix should DECONFLICT THE ROWS rather than widen the budget. In the same dump rows
            // 3-5 are blank across the full width, between the tick row and the chart box top, and
            // columns 0-5 are blank on every body row, so there is somewhere to go.
            // ⚠️ AND IT IS AN ORDERING COLLISION, not merely a spatial one. This band loop runs
            // BEFORE the axis-tick overlay below, so a label written along the tick row is
            // OVERWRITTEN by the ticks a few dozen lines later. That is the missing half of why the
            // earlier attempt "did not even fix the gantt case": widening the budget wrote more
            // characters that were then clobbered.
            //
            // So a SECTION band's label moves off the tick row entirely, onto an interior row of its
            // own band — which is also where the incumbent puts it. mermaid draws gantt section
            // titles from `vertLabels` at a fixed `x=10` inside its reserved `leftPadding: 75`
            // gutter, vertically CENTRED on the section's rows and never truncated; centring on the
            // band is the part of that we can reproduce without a layout gutter we do not have.
            //
            // Everything is gated on `Section` so lane and column bands keep byte-identical
            // behaviour, and the wide write happens only when every target cell is blank — so this
            // cannot displace content, which is what got the previous two attempts reverted. When
            // the blank run is not there, it falls back to exactly the old truncated write, so the
            // worst case is today's behaviour rather than a regression.
            let is_section = matches!(band.kind, fm_layout::LayoutBandKind::Section);
            // ⚠️ ONE CANDIDATE ROW WAS NOT ENOUGH, and betting on `y + h/2` was the bug. Measured on
            // the failing reproducer: the Section band is 288 x 114.5 layout units — most of the
            // chart, not the six-cell gutter I had assumed — so its middle row lands INSIDE the
            // chart box, where the box's own left border occupies a cell within the label's span.
            // The blank-run guard then correctly refused, and the fallback wrote on the tick row
            // where the axis overlay later overwrote everything past the first four characters.
            // That is why `Engineering` kept surfacing as `Engi`: never a budget truncation at all,
            // but a CLOBBER, and the truncation story sent two earlier attempts at the wrong thing.
            //
            // So SCAN the band's rows instead of picking one. Ascending from just below the band's
            // top, which is where a section header belongs and where the dump shows a full-width
            // blank run between the tick row and the top of the chart box. The first row whose span
            // is entirely blank wins; if none is, the old truncated write still runs, so the worst
            // case remains today's behaviour rather than a regression.
            let placed_wide = if is_section {
                let start = x + 1;
                let full: Vec<char> = self.truncate_label(&band.label).chars().collect();
                // The canvas fills empty cells with the BLANK BRAILLE PATTERN, not a space, so a
                // space-only check reads every empty cell as occupied — the same trap the cluster
                // overlay documents.
                // The canvas fills empty cells with the BLANK BRAILLE PATTERN, not a space, so a
                // space-only check reads every empty cell as occupied — the same trap the cluster
                // overlay documents.
                let row_is_clear = |row: usize, lines: &[Vec<char>]| {
                    !full.is_empty()
                        && full.iter().enumerate().all(|(offset, _)| {
                            let col = start + offset;
                            col < cell_width
                                && lines[row]
                                    .get(col)
                                    .is_some_and(|cell| *cell == ' ' || *cell == '\u{2800}')
                        })
                };
                // `y + 1` skips the band's own top rule, and the tick row when they coincide.
                let last_row = (y + h).min(lines.len());
                let target = (y + 1..last_row).find(|row| row_is_clear(*row, &lines));
                if let Some(row) = target {
                    for (offset, ch) in full.iter().enumerate() {
                        lines[row][start + offset] = *ch;
                    }
                }
                target.is_some()
            } else {
                false
            };

            if !placed_wide {
                let budget = w - 2;
                let label: String = self
                    .truncate_label(&band.label)
                    .chars()
                    .take(budget)
                    .collect();
                for (offset, ch) in label.chars().enumerate() {
                    let col = x + 1 + offset;
                    if col < cell_width {
                        lines[y][col] = ch;
                    }
                }
            }
        }

        // Overlay AXIS TICK labels (gantt dates, xychart categories).
        //
        // `layout.extensions.axis_ticks` is populated by the gantt and xychart layout arms and drawn
        // by fm-render-svg, and NOTHING in the terminal renderer referenced it — not the geometry and
        // not the labels. bd-trsd established what that costs: before it, the complete text content of
        // the shipped `gantt_basic.svg` was "Roadmap | Design | Build", two bars whose lengths encode
        // durations no reader could name. That is exactly what `-f term` still rendered for a gantt.
        //
        // Same one-cell placement discipline as the band and cluster overlays above: a tick is
        // written at its own x, and a tick whose label would run past the canvas is CLIPPED rather
        // than dropped, so a dense axis degrades to fewer readable dates instead of none.
        //
        // Ticks are drawn on the row ABOVE the diagram body where one exists, because the tick's own
        // y in layout space is the top of the chart and the bars start below it; writing at the bar
        // row would overwrite task names, which is the trade bd-u3fo's kanban case warned about —
        // one piece of dropped content swapped for another.
        if !layout.extensions.axis_ticks.is_empty() {
            let axis_rows: Vec<f32> = if layout.extensions.gantt_axis_rows.is_empty() {
                vec![layout.bounds.y]
            } else {
                layout
                    .extensions
                    .gantt_axis_rows
                    .iter()
                    .map(|axis| axis.y)
                    .collect()
            };
            for axis_y in axis_rows {
                let mut last_end: Option<usize> = None;
                for tick in &layout.extensions.axis_ticks {
                    if tick.label.is_empty() {
                        continue;
                    }
                    let (x, y, _w, _h) = self.bounds_to_cells(
                        &fm_layout::LayoutRect {
                            x: tick.position,
                            y: axis_y,
                            width: 0.0,
                            height: 0.0,
                        },
                        scale_x,
                        scale_y,
                    );
                    if y >= lines.len() {
                        continue;
                    }
                    // Never let two dates collide into an unreadable run: a tick that would start before
                    // the previous one ended is skipped, which thins a dense axis rather than corrupting
                    // it. The FIRST tick always survives, so an axis is never emptied by this rule.
                    if let Some(end) = last_end
                        && x <= end
                    {
                        continue;
                    }
                    let label: String = self.truncate_label(&tick.label).chars().collect();
                    let mut written = 0usize;
                    for (offset, ch) in label.chars().enumerate() {
                        let col = x + offset;
                        if col >= cell_width || col >= lines[y].len() {
                            break;
                        }
                        lines[y][col] = ch;
                        written = offset + 1;
                    }
                    if written > 0 {
                        last_end = Some(x + written);
                    }
                }
            }
        }

        // PACKET CONTINUATION LABELS (bd-t1jj). fm-render-svg labels each continuation, and it is
        // right to: a second box on a later row with no name in it does not say WHICH field wrapped,
        // which is the only thing a reader needs from it. The name therefore appears on both
        // segments, exactly as the SVG arm renders it.
        for continuation in &layout.extensions.packet_field_continuations {
            let Some(node) = ir.nodes.get(continuation.node_index) else {
                continue;
            };
            let Some(label) = self.node_display_label(ir, Some(node), &node.id) else {
                continue;
            };
            let (x, y, w, h) = self.bounds_to_cells(&continuation.bounds, scale_x, scale_y);
            if w < 3 || h < 1 || y >= lines.len() {
                continue;
            }
            let label_len = label.chars().count();
            let label_x = x + (w.saturating_sub(label_len)) / 2;
            let label_y = y + h / 2;
            if label_y >= lines.len() {
                continue;
            }
            for (offset, ch) in label.chars().enumerate() {
                let col = label_x + offset;
                if col < cell_width && col < lines[label_y].len() {
                    lines[label_y][col] = ch;
                }
            }
        }

        // stateDiagram NOTE TEXT (bd-t1jj). The box above is empty without it, and an empty box
        // beside a state is arguably worse than nothing: it asserts an annotation exists and withholds
        // it.
        //
        // Multi-line aware, because `note right of X … end note` is the form that carries more than a
        // sentence and truncating it to one line would silently drop the rest. Lines that do not fit
        // the box height are dropped rather than spilling onto the diagram below, and each line is
        // clipped to the box width for the same reason.
        for note in &layout.extensions.state_notes {
            if note.text.is_empty() {
                continue;
            }
            let (x, y, w, h) = self.bounds_to_cells(&note.bounds, scale_x, scale_y);
            if w < 3 || h < 1 || y >= lines.len() {
                continue;
            }
            let budget = w - 2;
            for (row, line) in note.text.lines().enumerate() {
                if row >= h {
                    break;
                }
                let label_y = y + row;
                if label_y >= lines.len() {
                    break;
                }
                let clipped: String = self.truncate_label(line).chars().take(budget).collect();
                for (offset, ch) in clipped.chars().enumerate() {
                    let col = x + 1 + offset;
                    if col < cell_width && col < lines[label_y].len() {
                        lines[label_y][col] = ch;
                    }
                }
            }
        }

        // Overlay QUADRANT AXIS labels (bd-59o4).
        //
        // ROOT CAUSE, measured rather than guessed: `render_quadrant_cell` DOES draw these — but it
        // is called only from `render_cell_mode`, and `TermRenderConfig::rich()` selects
        // `MermaidRenderMode::Braille`, which routes to `render_subcell_mode`. So the axis labels
        // live exclusively on a path this mode never takes. The quadrant TITLE and POINTS still
        // appear because they come from the generic title overlay and the generic node loop, which
        // is why only the axis labels went missing and the diagram looked almost right.
        //
        // The same shape explains bd-039t's gantt_section: `render_gantt_cell` is likewise
        // cell-mode-only. Recorded here because the next person to find a missing chart label in
        // the terminal should suspect the MODE before the drawing code.
        //
        // Drawn along the canvas edges rather than at chart-relative offsets: this overlay works in
        // cell space and has no access to the chart margins `render_quadrant_cell` computes, and an
        // invented margin would drift from the axis it labels.
        if let Some(quad) = ir.quadrant_meta.as_ref() {
            let bottom = lines.len().saturating_sub(1);
            // Computed HERE, beside `bottom`, and not at the call sites below: `place` borrows
            // `lines` mutably for the rest of the block, so reading `lines.len()` after it exists
            // is a borrow error. Quarter heights keep the quadrant names clear of the axis labels,
            // which occupy rows 1, `bottom - 1` and `bottom`.
            let upper_quarter = (lines.len() / 4).max(1);
            let lower_quarter = lines.len().saturating_mul(3) / 4;
            let mut place = |text: Option<&String>, row: usize, right_aligned: bool| {
                let Some(text) = text.map(String::as_str).filter(|t| !t.is_empty()) else {
                    return;
                };
                if row >= lines.len() {
                    return;
                }
                let text = self.truncate_label(text);
                let width = text.chars().count();
                let start = if right_aligned {
                    cell_width.saturating_sub(width + 1)
                } else {
                    1
                };
                // Blank cells preferred; written anyway if none, because an all-or-nothing guard
                // drops the label entirely — the failure that cost three builds on bd-039t.
                let free = text.chars().enumerate().all(|(offset, _)| {
                    let col = start + offset;
                    col < cell_width
                        && lines[row]
                            .get(col)
                            .is_some_and(|cell| *cell == ' ' || *cell == '\u{2800}')
                });
                let _ = free;
                for (offset, ch) in text.chars().enumerate() {
                    let col = start + offset;
                    if col >= cell_width {
                        break;
                    }
                    lines[row][col] = ch;
                }
            };
            place(quad.x_axis_left.as_ref(), bottom, false);
            place(quad.x_axis_right.as_ref(), bottom, true);
            place(quad.y_axis_top.as_ref(), 1, false);
            place(quad.y_axis_bottom.as_ref(), bottom.saturating_sub(1), false);

            // The quadrant NAMES, which this overlay drew none of (bd-039t, third widening of
            // `renderer_agreement.rs`). bd-59o4 fixed the four AXIS labels here and stopped there,
            // so `quadrant-1 Do it` reached the SVG and neither the terminal nor the canvas — and
            // the chart still looked almost right, because the title, the points and the axes were
            // all present. A quadrant chart whose quadrants are unnamed has lost the thing the
            // four regions MEAN.
            //
            // `quadrant_labels` is documented index-ordered [Q1 top-right, Q2 top-left,
            // Q3 bottom-left, Q4 bottom-right], so the placement follows the index rather than a
            // guess; `.get()` rather than indexing, because a chart may declare fewer than four.
            // Rows come from `upper_quarter`/`lower_quarter`, computed above beside `bottom`.
            place(quad.quadrant_labels.first(), upper_quarter, true);
            place(quad.quadrant_labels.get(1), upper_quarter, false);
            place(quad.quadrant_labels.get(2), lower_quarter, false);
            place(quad.quadrant_labels.get(3), lower_quarter, true);
        }

        // Overlay SEQUENCE NOTE text (bd-59o4).
        //
        // `render_subcell_mode` draws each note as a bare RECTANGLE via `canvas.draw_rect` and no
        // text, so `Note over Alice: Ponder` came out as an empty box. Measured SVG vs terminal vs
        // canvas: `Ponder` appears in the SVG and on the CANVAS, and not in the terminal — so this
        // is terminal-only, and the same geometry-without-text shape as bd-u3fo and the fragment
        // labels below.
        //
        // Same placement discipline as those: interior rows only (the note's top row is its own
        // border), blank cells preferred, and a fallback that writes anyway when no blank run
        // exists — because an all-or-nothing guard silently drew NOTHING for the longer of two
        // otherwise identical fragment labels, which is the bug that cost three builds on bd-039t.
        for note in &layout.extensions.sequence_notes {
            if note.text.is_empty() {
                continue;
            }
            let (nx, ny, nw, nh) = self.bounds_to_cells(&note.bounds, scale_x, scale_y);
            if nw < 3 || nh < 2 {
                continue;
            }
            let text = self.truncate_label(&note.text);
            let start = nx + 1;
            let last_row = (ny + nh.saturating_sub(1)).min(lines.len());
            let mut placed = false;
            for line in lines.iter_mut().take(last_row).skip(ny + 1) {
                let fits = text.chars().enumerate().all(|(offset, _)| {
                    let col = start + offset;
                    col < cell_width
                        && line
                            .get(col)
                            .is_some_and(|cell| *cell == ' ' || *cell == '\u{2800}')
                });
                if !fits {
                    continue;
                }
                for (offset, ch) in text.chars().enumerate() {
                    line[start + offset] = ch;
                }
                placed = true;
                break;
            }
            if !placed {
                let row = ny + 1;
                if row < lines.len() {
                    for (offset, ch) in text.chars().enumerate() {
                        let col = start + offset;
                        if col >= cell_width {
                            break;
                        }
                        lines[row][col] = ch;
                    }
                }
            }
        }

        // Overlay SEQUENCE FRAGMENT labels (bd-039t).
        //
        // `render_subcell_mode` draws each fragment's RECTANGLE via `canvas.draw_rect` and no text,
        // so `loop Every day` and `alt is ok` came out as bare boxes while fm-render-svg drew the
        // frame AND its label. Measured SVG vs terminal: `Every day` and `is ok` appeared in the
        // SVG and in neither terminal render.
        //
        // Drawn at the first INTERIOR row, not the frame's top row: that row is the rectangle's own
        // border, and writing there would trade the frame edge for the text. Only blank cells are
        // written, the same guard the cluster-title overlay uses, so a label can never overwrite a
        // message arrow or a participant box that shares the frame's interior.
        for fragment in &layout.extensions.sequence_fragments {
            let (fx, fy, fw, fh) = self.bounds_to_cells(&fragment.bounds, scale_x, scale_y);
            if fw < 3 || fh < 3 {
                continue;
            }
            // TRY SUCCESSIVE INTERIOR ROWS, because one occupied cell must not drop the whole
            // label. Measured: `alt is ok` (9 chars) drew while `loop Every day` (14 chars) did
            // not, with IDENTICAL frame bounds — the longer run met a lifeline glyph and the
            // all-or-nothing guard discarded it entirely. Rows are tried top-down so the label
            // still reads as belonging to the frame's head.
            // `kind` is the frame tag mermaid shows (loop / alt / opt / par), and the label is the
            // condition. Both matter: `alt` with no condition and a bare condition read differently.
            let mut text = format!("{:?}", fragment.kind).to_lowercase();
            if !fragment.label.is_empty() {
                text.push(' ');
                text.push_str(&fragment.label);
            }
            let text = self.truncate_label(&text);
            let start = fx + 1;
            let last_row = (fy + fh.saturating_sub(1)).min(lines.len());
            let mut placed = false;
            for line in lines.iter_mut().take(last_row).skip(fy + 1) {
                let fits = text.chars().enumerate().all(|(offset, _)| {
                    let col = start + offset;
                    col < cell_width
                        && line
                            .get(col)
                            .is_some_and(|cell| *cell == ' ' || *cell == '\u{2800}')
                });
                if !fits {
                    continue;
                }
                for (offset, ch) in text.chars().enumerate() {
                    line[start + offset] = ch;
                }
                placed = true;
                // Placed; a second copy further down would be worse than none.
                break;
            }

            // FALLBACK: draw it anyway, in the frame's first interior row.
            //
            // A blank span is not always available, and the all-or-nothing guard then discarded the
            // label completely. MEASURED: `alt is ok` (9 chars) found a span and drew, while `loop
            // Every day` (14 chars) did not — with IDENTICAL frame bounds, so it is the run length
            // meeting a lifeline column, not the frame's size. Verified by disarming this overlay:
            // both labels then vanish, so this code is what draws them.
            //
            // mermaid draws the frame label in a tab at the top-left, over whatever lies beneath, so
            // overwriting here is faithful rather than a compromise. It is confined to the frame's
            // OWN first interior row at its left edge, where messages between lifelines do not sit,
            // and `sequence_fragment_label_reaches_terminal_output` asserts the frame's contents
            // survive.
            if !placed {
                let row = fy + 1;
                if row < lines.len() {
                    for (offset, ch) in text.chars().enumerate() {
                        let col = start + offset;
                        if col >= cell_width {
                            break;
                        }
                        lines[row][col] = ch;
                    }
                }
            }
        }

        // Overlay CLUSTER titles (bd-u3fo).
        //
        // `render_cluster_canvas` draws each cluster's RECTANGLE and nothing else, while
        // fm-render-svg draws the box AND its title. So every subgraph name — a flowchart
        // `subgraph Backend`, a kanban column — was a nameless box in `-f term`.
        //
        // This is the path a real kanban column actually takes, which is NOT the band path the
        // comment above describes. `layout_diagram_kanban_traced` returns early when the columns
        // are declared lanes (fm-layout `columns_are_declared_lanes`), so a parsed kanban reaches
        // the renderer with ZERO bands and its columns as clusters; measured on `kanban / Alpha /
        // t1[Beta]`: 1 node, 0 bands, and the terminal drew the card `Beta` and the column box but
        // never `Alpha`. The band overlay stays for journey, whose lanes are bands.
        //
        // Placed at the first INTERIOR row rather than over the top border: the border row is the
        // box's own geometry and overwriting it would trade a drawn edge for the text. Skipped
        // entirely unless every target cell is blank, so a title can never displace a card label or
        // a nested cluster's border — a name that overwrote the content it names is a worse outcome
        // than the missing name this fixes.
        if self.config.show_clusters {
            // The same ancestor reservation the canvas pass applied, so a nested boundary's caption
            // is written at the row its box was actually drawn at (bd-c23yq). Without it the inner
            // `Core` and `[SYSTEM]` were written at the un-pushed row, where the blank-cell guard
            // correctly refused them — the outer boundary's own caption was already there.
            let overlay_ancestor_pushes = c4_cluster_pushes(ir, &layout.clusters);
            for (cluster_order, cluster_box) in layout.clusters.iter().enumerate() {
                let title = cluster_box
                    .title
                    .as_deref()
                    .or_else(|| {
                        ir.clusters
                            .get(cluster_box.cluster_index)
                            .and_then(|cluster| cluster.title)
                            .and_then(|label_id| ir.labels.get(label_id.0))
                            .map(|label| label.text.as_str())
                    })
                    .unwrap_or("");
                if title.is_empty() {
                    continue;
                }
                let (x, y, w, h) = self.bounds_to_cells(&cluster_box.bounds, scale_x, scale_y);
                let y = y + overlay_ancestor_pushes
                    .get(&cluster_order)
                    .copied()
                    .unwrap_or(0);
                // `h < 3` has no interior row at all, and `w < 3` no room between the side borders.
                if w < 3 || h < 3 {
                    continue;
                }
                let row = y + 1;
                if row >= lines.len() {
                    continue;
                }
                let text: String = self.truncate_label(title).chars().take(w - 2).collect();
                let start = x + 1;
                // The canvas fills empty cells with the BLANK BRAILLE PATTERN, not a space, so a
                // space-only check would read every empty cell as occupied and drop every title.
                // `chars().enumerate()`, NOT `char_indices()`: the write loop below advances one
                // CELL per char, so a byte offset would check the wrong columns for any title with
                // a multi-byte character in it.
                let is_free = text.chars().enumerate().all(|(offset, _)| {
                    let col = start + offset;
                    col < cell_width
                        && lines[row]
                            .get(col)
                            .is_some_and(|cell| *cell == ' ' || *cell == '\u{2800}')
                });
                if !is_free {
                    continue;
                }
                for (offset, ch) in text.chars().enumerate() {
                    lines[row][start + offset] = ch;
                }

                // THE C4 BOUNDARY TYPE, on its own row beneath the label (bd-c23yq).
                //
                // mermaid draws a boundary as label-then-type and both the SVG and canvas arms do.
                // The terminal could not, because a contained node's box began on the very row the
                // type needed. It now begins `C4_CAPTION_ROWS + 1` rows lower — the `+ 1` clears the
                // node's own top border, which a two-row reservation measurably did not and which
                // is why the first attempt at this concluded the row was unreachable.
                //
                // Still written under the blank-cell guard: if anything did reach this row the type
                // yields, because a type that overwrote a node border would trade one piece of
                // dropped content for another.
                if let Some(kind) = ir
                    .clusters
                    .get(cluster_box.cluster_index)
                    .and_then(|cluster| cluster.c4_boundary_type.as_deref())
                {
                    let type_row = row + 1;
                    let type_text: String = format!("[{kind}]")
                        .chars()
                        .take(w.saturating_sub(2))
                        .collect();
                    if type_row < lines.len() && !type_text.is_empty() {
                        let free = type_text.chars().enumerate().all(|(offset, _)| {
                            let col = start + offset;
                            col < cell_width
                                && lines[type_row]
                                    .get(col)
                                    .is_some_and(|cell| *cell == ' ' || *cell == '\u{2800}')
                        });
                        if free {
                            for (offset, ch) in type_text.chars().enumerate() {
                                lines[type_row][start + offset] = ch;
                            }
                        }
                    }
                }

                // ⚠️ HISTORY, kept because it records two measured dead ends and their causes.
                //
                // mermaid draws a boundary as two rows (label, then its bracketed type) and both the
                // SVG and canvas arms do the same. TWO attempts at the terminal twin were written,
                // measured, and reverted:
                //
                //   1. Write `[SYSTEM]` into `row + 1` under the blank-cell guard above. Never drew
                //      anything at width 80, 160 or 240 -- the guard refused every time.
                //   2. Additionally reserve a caption row in fm-layout, which is what the incumbent
                //      does (`drawInsideBoundary` advances its cursor past the type before placing
                //      any child). That made the row REACH `lines` -- instrumented and confirmed
                //      written -- and it still did not survive to the output.
                //
                // THE REASON, measured rather than reasoned: the node overlay runs AFTER this one
                // and does not check what it paints over. A C4 node draws a name header, a divider
                // and its type rows, and that divider lands exactly on the caption band, so the
                // written `[SYSTEM]` is overwritten before the frame is emitted. Where a boundary
                // contains only other BOUNDARIES the row does survive -- `[ENTERPRISE]` and
                // `[custom]` appeared at widths 160 and 240 -- which is how partial, inconsistent
                // coverage would have shipped had this not been checked per-fixture.
                //
                // A correct fix has to give the caption its own cell rows in the TERMINAL's own
                // coordinate space and offset node drawing accordingly; reserving pixels upstream
                // cannot do it, because this surface scales the whole diagram to fit its row count.
            }
        }

        // Resolved gantt name placements, indexed by node. Built once rather than scanned per node,
        // and only when the diagram actually has any -- every other diagram type leaves this `None`
        // and takes exactly the path it took before.
        let gantt_label_by_node: Option<
            std::collections::HashMap<usize, &fm_layout::LayoutGanttTaskLabel>,
        > = if layout.extensions.gantt_task_labels.is_empty() {
            None
        } else {
            Some(
                layout
                    .extensions
                    .gantt_task_labels
                    .iter()
                    .map(|entry| (entry.node_index, entry))
                    .collect(),
            )
        };

        // Overlay node labels.
        // Same reservation the canvas pass applied, so the text follows the box it belongs to
        // rather than staying where the box used to be (bd-c23yq).
        let overlay_caption_pushes = c4_node_pushes(ir);
        for node_box in &layout.nodes {
            let (x, y, w, h) = self.bounds_to_cells(&node_box.bounds, scale_x, scale_y);
            let y = y + overlay_caption_pushes
                .get(&node_box.node_index)
                .copied()
                .unwrap_or(0);
            let ir_node = ir.nodes.get(node_box.node_index);

            if ir_node.is_some_and(is_block_beta_space_node) {
                continue;
            }

            // Class diagram nodes with class_meta get three-compartment rendering.
            if let Some(node) = ir_node
                && let Some(ref meta) = node.class_meta
                && meta.has_compartments()
            {
                self.overlay_class_compartments(&mut lines, x, y, w, h, ir, node, meta, cell_width);
                continue;
            }

            // ER entities get the SAME treatment (bd-ekx2).
            //
            // `IrNode::members` is populated only for ER entities, and the identifier `members` did
            // not appear anywhere in this file: the terminal drew the box and the entity name and
            // stopped. Measured on `CUSTOMER { string name PK / int age }`, the IR carried 2 members
            // while `name`, `age` and `PK` were absent at 100x40, 200x60 AND 400x120 —
            // size-independent, so not the viewport ceiling of bd-8tsw. The class branch directly
            // above proves the terminal can draw compartments; ER simply never got them.
            //
            // No layout change is needed: `er_compartment_dimensions` already sizes the entity box
            // to hold its rows, so the box has been big enough all along and only the text was
            // missing.
            if let Some(node) = ir_node
                && !node.members.is_empty()
            {
                self.overlay_er_compartments(&mut lines, x, y, w, h, ir, node, cell_width);
                continue;
            }

            // REQUIREMENT rows reach the terminal too (bd-039t).
            //
            // Measured SVG vs terminal on the same IR: `requirement R { id: 1 / text: hello /
            // risk: high }` drew `hello` and `high` in the SVG and NEITHER in the terminal. Same
            // shape as bd-ekx2 — content attached to the node that the terminal never learned to
            // draw, while it drew the node's label happily. `requirement_row_dimensions` already
            // sizes the box for these rows (that is what bd-jnc1 and bd-f3tc are about), so the
            // box has room and only the text was missing.
            if let Some(node) = ir_node
                && let Some(meta) = node.requirement_meta.as_deref()
            {
                self.overlay_requirement_rows(&mut lines, x, y, w, h, ir, node, meta, cell_width);
                continue;
            }

            // C4 element type, technology and description reach the terminal too (bd-039t).
            //
            // Measured SVG vs terminal: `Person(a, "Alice", "A user")` drew `A user` in the SVG and
            // not in the terminal. Third member of the same class as bd-ekx2 and the requirement
            // rows above — node-attached content the terminal never learned to draw.
            if let Some(node) = ir_node
                && let Some(meta) = node.c4_meta.as_deref()
            {
                self.overlay_c4_rows(&mut lines, x, y, w, h, ir, node, meta, cell_width);
                continue;
            }

            let Some(label) = self.node_display_label(ir, ir_node, &node_box.node_id) else {
                continue;
            };

            // GANTT TASK NAMES honour the placement layout resolved for them (bd-t1jj).
            //
            // The generic path below centres a label on its node and lets an oversized one overflow
            // to the RIGHT. For a gantt bar that is wrong in one specific, measured way: a task whose
            // name is wider than its bar AND whose bar sits near the right edge has nowhere to
            // overflow, so the name is clipped at the canvas edge and LOST. Measured on a three-task
            // chart at 80 columns, `FinalIntegrationAndSignoffPhase` did not appear at all, while the
            // same chart at 120 columns showed it -- a name that survives only if the terminal is
            // wide enough is not a name the reader can rely on.
            //
            // `extensions.gantt_task_labels` already solved this: layout resolves each task to
            // Inside / OutsideRight / OutsideLeft and hands back the anchor x, choosing OutsideLeft
            // precisely when there is no room to the right. fm-render-svg consumes it; the terminal
            // did not.
            //
            // ANCHOR CONVENTION matches the SVG arm exactly, which is why the two backends cannot
            // disagree about where a name sits: OutsideRight anchors the text's LEFT edge ("start"),
            // OutsideLeft its RIGHT edge ("end"), Inside its CENTRE ("middle"). In cells the start
            // column is therefore the anchor, the anchor minus the length, or the anchor minus half
            // the length.
            let gantt_anchor = gantt_label_by_node
                .as_ref()
                .and_then(|map| map.get(&node_box.node_index));

            let label_lines: Vec<&str> = label.lines().collect();
            let start_y = if fm_layout::edge_labels::uses_graph_edge_labels(ir.diagram_type) {
                // A 2.8-cell-high box has an interior row. Flooring its height before centering
                // puts the text on its top border instead of that actual geometric center.
                self.point_to_cells(&node_box.bounds.center(), scale_x, scale_y)
                    .1
                    .saturating_sub(label_lines.len() / 2)
            } else {
                y + (h.saturating_sub(label_lines.len())) / 2
            };

            for (i, line) in label_lines.iter().enumerate() {
                // Iterate the label line's chars directly for both the centering width and the placement —
                // the previous `chars().collect::<Vec<char>>()` allocated a Vec per label line only to read
                // its `.len()` and then consume it sequentially. `chars().count()` gives the same width and
                // `chars().enumerate()` the same `(j, ch)`. Byte-identical, no per-line allocation.
                let label_len = line.chars().count();
                let label_x = match gantt_anchor {
                    Some(entry) => {
                        // Mirror `bounds_to_cells`' x arithmetic so the anchor lands in the same cell
                        // space as every other overlay.
                        let anchor = (entry.x * scale_x) as isize + self.config.padding as isize;
                        let start = match entry.placement {
                            fm_layout::GanttLabelPlacement::OutsideRight => anchor,
                            fm_layout::GanttLabelPlacement::OutsideLeft => {
                                anchor - isize::try_from(label_len).unwrap_or(0)
                            }
                            fm_layout::GanttLabelPlacement::Inside => {
                                anchor - isize::try_from(label_len).unwrap_or(0) / 2
                            }
                        };
                        usize::try_from(start.max(0)).unwrap_or(0)
                    }
                    None => x + (w.saturating_sub(label_len)) / 2,
                };
                let label_y = start_y + i;

                if label_y < lines.len() {
                    for (j, ch) in line.chars().enumerate() {
                        let col = label_x + j;
                        if col < cell_width && col < lines[label_y].len() {
                            lines[label_y][col] = ch;
                        }
                    }
                }
            }
        }

        for node_box in &layout.extensions.sequence_mirror_headers {
            let (x, y, w, h) = self.bounds_to_cells(&node_box.bounds, scale_x, scale_y);
            let ir_node = ir.nodes.get(node_box.node_index);

            if ir_node.is_some_and(is_block_beta_space_node) {
                continue;
            }

            let Some(label) = self.node_display_label(ir, ir_node, &node_box.node_id) else {
                continue;
            };

            let label_lines: Vec<&str> = label.lines().collect();
            let start_y = y + (h.saturating_sub(label_lines.len())) / 2;

            for (i, line) in label_lines.iter().enumerate() {
                // Iterate the label line's chars directly for both the centering width and the placement —
                // the previous `chars().collect::<Vec<char>>()` allocated a Vec per label line only to read
                // its `.len()` and then consume it sequentially. `chars().count()` gives the same width and
                // `chars().enumerate()` the same `(j, ch)`. Byte-identical, no per-line allocation.
                let label_len = line.chars().count();
                let label_x = x + (w.saturating_sub(label_len)) / 2;
                let label_y = start_y + i;

                if label_y < lines.len() {
                    for (j, ch) in line.chars().enumerate() {
                        let col = label_x + j;
                        if col < cell_width && col < lines[label_y].len() {
                            lines[label_y][col] = ch;
                        }
                    }
                }
            }
        }

        // Overlay edge labels.
        //
        // Edges in opposite directions often share a geometric midpoint. Keep a separate record
        // of label rectangles so the later relationship does not overwrite an earlier one (for
        // example, turning `1: Place order` and `2: Confirm order` into `1:2: Confirm order`).
        let mut edge_label_rects = Vec::new();
        // Protect the actual node and annotation boxes, not just text already stamped into them.
        // A label in a blank node interior reads as content of that node. Composite-state boxes
        // are containers: only their border is protected, so internal transitions can be named.
        for node_box in &layout.nodes {
            let (x, y, w, h) = self.bounds_to_cells(&node_box.bounds, scale_x, scale_y);
            let y = y + overlay_caption_pushes
                .get(&node_box.node_index)
                .copied()
                .unwrap_or(0);
            if fm_layout::is_composite_state_node(ir, node_box) {
                edge_label_rects.extend([
                    (x, y, x + w, y + 1),
                    (x, y + h, x + w + 1, y + h + 1),
                    (x, y, x + 1, y + h + 1),
                    (x + w, y, x + w + 1, y + h + 1),
                ]);
            } else if !ir
                .nodes
                .get(node_box.node_index)
                .is_some_and(is_block_beta_space_node)
            {
                edge_label_rects.push((x, y, x + w + 1, y + h + 1));
            }
        }
        for bounds in layout
            .extensions
            .state_notes
            .iter()
            .map(|note| &note.bounds)
            .chain(
                layout
                    .extensions
                    .sequence_notes
                    .iter()
                    .map(|note| &note.bounds),
            )
            .chain(
                layout
                    .extensions
                    .sequence_mirror_headers
                    .iter()
                    .map(|node| &node.bounds),
            )
        {
            let (x, y, w, h) = self.bounds_to_cells(bounds, scale_x, scale_y);
            edge_label_rects.push((x, y, x + w + 1, y + h + 1));
        }
        // Keep all prior overlay text, including text wider than its node's scaled box. The
        // sub-cell raster only emits block/braille glyphs; everything else is authored content
        // or an annotation border that must survive the next label.
        for (y, row) in lines.iter().enumerate() {
            for (x, ch) in row.iter().enumerate() {
                if !matches!(*ch, ' ' | '\u{2580}'..='\u{259f}' | '\u{2800}'..='\u{28ff}') {
                    edge_label_rects.push((x, y, x + 1, y + 1));
                }
            }
        }
        if generic_terminal_diagram_title(ir).is_some() {
            edge_label_rects.push((0, 0, cell_width, 1));
        }
        for edge_path in &layout.edges {
            if edge_path.points.len() < 2 || is_invisible_edge(ir, edge_path) {
                continue;
            }
            if !fm_layout::edge_labels::uses_graph_edge_labels(ir.diagram_type)
                && let Some(label_id) = ir.edges.get(edge_path.edge_index).and_then(|e| e.label)
                && let Some(label) = ir.labels.get(label_id.0)
            {
                let base_label = self.truncate_label(&label.text);
                let truncated = if let Some(number) = ir
                    .sequence_meta
                    .as_ref()
                    .and_then(|meta| meta.autonumber_value(edge_path.edge_index))
                {
                    format!("{number} {base_label}")
                } else if ir.diagram_type == fm_core::DiagramType::C4Dynamic {
                    // C4Dynamic diagrams use the declaration order as part of their meaning:
                    // Mermaid numbers each relationship while drawing it, leaving the parsed label
                    // unchanged so the same relationship syntax remains bare in C4Context.
                    format!("{}: {base_label}", edge_path.edge_index + 1)
                } else {
                    base_label
                };
                let label_lines: Vec<&str> = truncated.lines().collect();

                let (mid_x, mid_y) = if edge_path.points.len() == 4 {
                    let p1 = &edge_path.points[1];
                    let p2 = &edge_path.points[2];
                    let px = f32::midpoint(p1.x, p2.x);
                    let py = f32::midpoint(p1.y, p2.y);
                    self.point_to_cells(&fm_layout::LayoutPoint { x: px, y: py }, scale_x, scale_y)
                } else if edge_path.points.len() == 2 {
                    let p1 = &edge_path.points[0];
                    let p2 = &edge_path.points[1];
                    let px = f32::midpoint(p1.x, p2.x);
                    let py = f32::midpoint(p1.y, p2.y);
                    self.point_to_cells(&fm_layout::LayoutPoint { x: px, y: py }, scale_x, scale_y)
                } else {
                    let mid_idx = edge_path.points.len() / 2;
                    self.point_to_cells(&edge_path.points[mid_idx], scale_x, scale_y)
                };

                let label_width = label_lines
                    .iter()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(0);
                let preferred_y = mid_y.saturating_sub(label_lines.len() / 2);
                let (label_x, start_y) = place_edge_label(
                    &edge_label_rects,
                    mid_x,
                    preferred_y,
                    label_width,
                    label_lines.len(),
                    cell_width,
                    lines.len(),
                );

                // Clear one whole backing plate, including a one-cell horizontal gutter. This
                // keeps short continuation rows from being threaded through connector glyphs.
                for row in lines.iter_mut().skip(start_y).take(label_lines.len()) {
                    let left = label_x.saturating_sub(1);
                    let right = (label_x + label_width + 1).min(cell_width).min(row.len());
                    if left < right {
                        row[left..right].fill(' ');
                    }
                }
                for (i, line) in label_lines.iter().enumerate() {
                    // Iterate chars directly (see the node-label loops above): `chars().count()` for the
                    // centering width, `chars().enumerate()` for placement — no per-line Vec allocation.
                    let line_x = label_x + label_width.saturating_sub(line.chars().count()) / 2;
                    let label_y = start_y + i;

                    if label_y < lines.len() {
                        for (j, ch) in line.chars().enumerate() {
                            let col = line_x + j;
                            if col < cell_width && col < lines[label_y].len() {
                                lines[label_y][col] = ch;
                            }
                        }
                    }
                }
                edge_label_rects.push((
                    label_x.saturating_sub(1),
                    start_y,
                    label_x.saturating_add(label_width + 1).min(cell_width),
                    start_y.saturating_add(label_lines.len()).min(lines.len()),
                ));
            }

            // CARDINALITIES reach the canvas too (bd-o2wf).
            //
            // `"1" --> "many"` lives in `IrEdgeExtras`, not in `edge.label`, and this overlay drew
            // the label and nothing else — so a class diagram rendered BYTE-IDENTICAL in the
            // terminal with and without its cardinalities while fm-render-svg drew both. Sits
            // outside the label block on purpose: an edge may carry cardinality and no label.
            //
            // Each number goes by ITS OWN endpoint, since which end carries `1` and which carries
            // `many` is the entire content. Written only into BLANK cells, the same guard the
            // cluster-title overlay uses: a number that overwrote a node border or an existing
            // label would trade one piece of dropped content for another.
            if let Some(edge) = ir.edges.get(edge_path.edge_index) {
                // ER notation feeds the SAME placement (bd-2h3pp). `}o--o|` declares "0..*" and
                // "0..1" exactly as `"1" --> "many"` declares a class cardinality, and fm-render-svg
                // drew both while this surface drew only the class one — so an ER diagram lost its
                // cardinality here for the same reason a class diagram used to.
                //
                // Reusing this block rather than adding an overlay is the whole point: the candidate
                // search below is the MEASURED part, and the two attempts that failed on the gantt
                // band label (bd-039t) both failed by guessing at placement instead of reusing
                // something already proven on this grid.
                //
                // Resolved once per edge because both ends read it and it parses the notation.
                let er_labels = edge.er_cardinality_labels();
                // Placed a short way ALONG the edge rather than exactly at the endpoint. The
                // endpoint sits ON the node border, whose box glyph is never blank, so the
                // blank-cell guard rejected every write and the first version of this fix drew
                // NOTHING — caught by `class_cardinalities_reach_terminal_output` rather than
                // shipped. `t` is small so the number still reads as belonging to its own end.
                let mut place =
                    |text: Option<&str>,
                     point: Option<&fm_layout::LayoutPoint>,
                     toward: Option<&fm_layout::LayoutPoint>| {
                        let (Some(text), Some(point)) = (text.filter(|t| !t.is_empty()), point)
                        else {
                            return;
                        };
                        const T: f32 = 0.25;
                        let anchor = toward.map_or(
                            fm_layout::LayoutPoint {
                                x: point.x,
                                y: point.y,
                            },
                            |other| fm_layout::LayoutPoint {
                                x: (other.x - point.x).mul_add(T, point.x),
                                y: (other.y - point.y).mul_add(T, point.y),
                            },
                        );
                        let (bx, by) = self.point_to_cells(&anchor, scale_x, scale_y);
                        let text = self.truncate_label(text);
                        let width = text.chars().count();

                        // SEARCH BESIDE THE LINE. The anchor cell lies ON the edge itself, whose glyph
                        // is never blank, so writing there is always rejected — measured: the first two
                        // versions of this fix drew nothing at all, and only dumping the canvas showed
                        // the run sitting on a column of `⢸`. The cells flanking the line are free, so
                        // step perpendicular (then vertically) and take the first run that fits.
                        const CANDIDATES: [(i32, i32); 6] =
                            [(1, 0), (-1, 0), (2, 0), (-2, 0), (0, 1), (0, -1)];
                        for (dx, dy) in CANDIDATES {
                            let Some(cx) = bx.checked_add_signed(dx as isize) else {
                                continue;
                            };
                            let Some(cy) = by.checked_add_signed(dy as isize) else {
                                continue;
                            };
                            if cy >= lines.len() || cx + width > cell_width {
                                continue;
                            }
                            let free = (0..width).all(|offset| {
                                lines[cy]
                                    .get(cx + offset)
                                    .is_some_and(|cell| *cell == ' ' || *cell == '\u{2800}')
                            });
                            if !free {
                                continue;
                            }
                            for (offset, ch) in text.chars().enumerate() {
                                lines[cy][cx + offset] = ch;
                            }
                            edge_label_rects.push((
                                cx.saturating_sub(1),
                                cy,
                                (cx + width + 1).min(cell_width),
                                cy + 1,
                            ));
                            // Placed; a second copy at another offset would be worse than none.
                            break;
                        }
                    };
                // `get(1)` / second-to-last give the direction to step away from each endpoint.
                let after_first = edge_path.points.get(1);
                let before_last = edge_path
                    .points
                    .len()
                    .checked_sub(2)
                    .and_then(|i| edge_path.points.get(i));
                place(
                    edge.source_cardinality()
                        .or_else(|| er_labels.map(|(source, _)| source)),
                    edge_path.points.first(),
                    after_first,
                );
                place(
                    edge.target_cardinality()
                        .or_else(|| er_labels.map(|(_, target)| target)),
                    edge_path.points.last(),
                    before_last,
                );
            }
        }

        // TREEMAP and RADAR text (bd-dw450). The geometry pass drew their boxes, rings and spokes;
        // without this the terminal shows a diagram whose every label is missing, which is only a
        // little better than the blank canvas it used to show.
        //
        // Every write goes through `write_if_blank`, which refuses to overwrite an occupied cell.
        // That is the rule the band and cluster overlays already work under, and for the same
        // reason: an overlay that displaces content trades a missing label for a corrupted diagram.
        let write_if_blank = |lines: &mut Vec<Vec<char>>, row: usize, col: usize, text: &str| {
            let chars: Vec<char> = text.chars().collect();
            if chars.is_empty() || row >= lines.len() || col + chars.len() > cell_width {
                return false;
            }
            // The canvas fills empty cells with the BLANK BRAILLE PATTERN, not a space, so a
            // space-only check reads every empty cell as occupied.
            let clear = (0..chars.len()).all(|offset| {
                lines[row]
                    .get(col + offset)
                    .is_some_and(|cell| *cell == ' ' || *cell == '\u{2800}')
            });
            if !clear {
                return false;
            }
            for (offset, ch) in chars.into_iter().enumerate() {
                lines[row][col + offset] = ch;
            }
            true
        };

        if let Some(meta) = ir.treemap_meta.as_ref() {
            for tile in &layout.extensions.treemap_tiles {
                let Some(item) = meta.nodes.get(tile.node) else {
                    continue;
                };
                let (x, y, w, h) = self.bounds_to_cells(&tile.bounds, scale_x, scale_y);
                let caption = self.truncate_label(&format!(
                    "{} {}",
                    item.label,
                    format_terminal_treemap_value(tile.value)
                ));
                let width = caption.chars().count();
                if width + 2 > w {
                    continue;
                }
                // A LEAF is captioned at its centre and a SECTION just inside its top edge, which
                // is where each one has room: a section's middle is full of its children.
                let (row, col) = if tile.is_leaf {
                    (y + h / 2, x + (w.saturating_sub(width)) / 2)
                } else {
                    (y + 1, x + 1)
                };
                write_if_blank(&mut lines, row, col, &caption);
            }
        }

        if let Some((meta, radar)) = ir.radar_meta.as_ref().zip(layout.extensions.radar.as_ref()) {
            for (index, axis) in radar.axes.iter().enumerate() {
                let Some(declared) = meta.axes.get(index) else {
                    continue;
                };
                let (ax, ay) = self.point_to_cells(&axis.label_anchor, scale_x, scale_y);
                let caption = self.truncate_label(declared.display());
                let width = caption.chars().count();
                // Centre the caption on the anchor, then pull it back inside the grid rather than
                // dropping it: an axis label sits at the extreme of the wheel by construction, so
                // "off the edge" is the normal case, not an error case.
                let col = ax
                    .saturating_sub(width / 2)
                    .min(cell_width.saturating_sub(width));

                // ⚠️ ONE CANDIDATE ROW IS NOT ENOUGH, and the anchor row is the WORST single bet.
                // Measured: the label sits 15 layout units beyond the spoke's tip, which at terminal
                // resolution is less than one cell — so it lands on the spoke's own last cell, the
                // blank guard correctly refuses, and the label is dropped. The topmost axis of a
                // three-axis wheel went missing entirely that way while the other two, whose spokes
                // end mid-cell, were fine.
                //
                // So STEP AWAY FROM THE CENTRE until a clear row is found: away is where there is
                // nothing left to collide with, and it is the direction the label already points.
                // Same shape as the band overlay's row scan, and for the same reason.
                let outward: isize = if axis.label_anchor.y < radar.center.y {
                    -1
                } else {
                    1
                };
                for attempt in 0..3_isize {
                    let Ok(row) = usize::try_from(ay as isize + outward * attempt) else {
                        break;
                    };
                    if write_if_blank(&mut lines, row, col, &caption) {
                        break;
                    }
                }
            }
            if meta.show_legend {
                for (index, curve) in meta.curves.iter().enumerate() {
                    let caption = self.truncate_label(curve.display());
                    let row = 1 + index;
                    write_if_blank(&mut lines, row, 1, &caption);
                }
            }
        }

        if ir.diagram_type == fm_core::DiagramType::Info {
            let version = concat!("v", env!("CARGO_PKG_VERSION"));
            let row = cell_height / 2;
            let col = cell_width.saturating_sub(version.chars().count()) / 2;
            write_if_blank(&mut lines, row, col, version);
        }

        if let Some(title) = generic_terminal_diagram_title(ir)
            && let Some(first_line) = lines.first_mut()
        {
            let title = self.truncate_label(title);
            let title_len = title.chars().count().min(cell_width);
            let start_x = cell_width.saturating_sub(title_len) / 2;
            for (index, ch) in title.chars().take(title_len).enumerate() {
                let col = start_x + index;
                if col < first_line.len() {
                    first_line[col] = ch;
                }
            }
        }

        let (labels, occluded) = self.overlay_graph_edge_labels(
            &mut lines,
            ir,
            layout,
            scale_x,
            scale_y,
            &edge_label_rects,
        );
        (serialize_cell_labels(&lines, &labels, false), occluded)
    }

    /// Use shared geometric placement with terminal-sized measurements, then validate the
    /// quantized painted cells. A pixel-space clearance smaller than one column is insufficient
    /// here: it can merge captions or erase a neighbouring connector after rasterization.
    #[allow(clippy::too_many_arguments)]
    fn overlay_graph_edge_labels(
        &self,
        grid: &mut [Vec<char>],
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        scale_x: f32,
        scale_y: f32,
        protected: &[(usize, usize, usize, usize)],
    ) -> (Vec<CellLabel>, usize) {
        let width = grid.first().map_or(0, Vec::len);
        let height = grid.len();
        if !self.config.show_labels
            || !fm_layout::edge_labels::uses_graph_edge_labels(ir.diagram_type)
        {
            return (Vec::new(), 0);
        }
        if width < 3 || height == 0 {
            let occluded = layout
                .edges
                .iter()
                .filter(|path| {
                    !path.bundled
                        && path.points.len() >= 2
                        && !is_invisible_edge(ir, path)
                        && ir
                            .edges
                            .get(path.edge_index)
                            .and_then(|edge| edge.label)
                            .and_then(|id| ir.labels.get(id.0))
                            .is_some_and(|label| {
                                label
                                    .text
                                    .chars()
                                    .any(|ch| !ch.is_control() && !ch.is_whitespace())
                            })
                })
                .count();
            return (Vec::new(), occluded);
        }
        let mut labels = Vec::new();
        let mut occluded = 0;
        let mut placer = fm_layout::edge_labels::EdgeLabelPlacer::new(ir, layout, 0.0);
        let mut node_cells = vec![false; width * height];
        let mut protect = |left: usize, top: usize, right: usize, bottom: usize| {
            for row in top.min(height)..bottom.min(height) {
                node_cells[row * width + left.min(width)..row * width + right.min(width)]
                    .fill(true);
            }
        };
        for node in &layout.nodes {
            if fm_layout::is_composite_state_node(ir, node) {
                continue;
            }
            let (x, y, w, h) = self.bounds_to_cells(&node.bounds, scale_x, scale_y);
            protect(x, y, x.saturating_add(w + 1), y.saturating_add(h + 1));
        }
        for note in &layout.extensions.state_notes {
            let (x, y, w, h) = self.bounds_to_cells(&note.bounds, scale_x, scale_y);
            protect(x, y, x.saturating_add(w + 1), y.saturating_add(h + 1));
        }
        for &(left, top, right, bottom) in protected {
            protect(left, top, right, bottom);
        }

        for path in &layout.edges {
            if path.bundled || path.points.len() < 2 || is_invisible_edge(ir, path) {
                continue;
            }
            let Some(label) = ir
                .edges
                .get(path.edge_index)
                .and_then(|edge| edge.label)
                .and_then(|id| ir.labels.get(id.0))
            else {
                continue;
            };
            // Preserve the configured total text budget, while fitting line lengths to the
            // available columns. The default 24-character soft wrap must not cut a 25-column
            // caption (or identifier) that fits intact in this terminal.
            let text = edge_caption_text(
                &label.text,
                self.config
                    .max_label_chars
                    .max(1)
                    .saturating_mul(self.config.max_label_lines.max(1)),
            );
            if text.trim().is_empty() {
                continue;
            }
            let route: Vec<_> = path
                .points
                .iter()
                .map(|point| {
                    (
                        point.x * scale_x + self.config.padding as f32,
                        point.y * scale_y + self.config.padding as f32,
                    )
                })
                .collect();
            let own_route = self
                .edge_label_route_grid(ir, layout, path, false, width, height, scale_x, scale_y);
            let foreign_routes =
                self.edge_label_route_grid(ir, layout, path, true, width, height, scale_x, scale_y);
            let mut blocked_nodes = node_cells.clone();
            for node in layout
                .nodes
                .iter()
                .filter(|node| fm_layout::is_composite_state_node(ir, node))
            {
                let contains_endpoint = ir.clusters.iter().any(|cluster| {
                    fm_layout::composite_state_cluster(ir, &node.node_id) == Some(cluster.id)
                        && ir.edges.get(path.edge_index).is_some_and(|edge| {
                            [edge.from, edge.to].iter().any(|endpoint| {
                                ir.resolve_endpoint_node(*endpoint)
                                    .is_some_and(|id| cluster.members.contains(&id))
                            })
                        })
                });
                if !contains_endpoint {
                    let (x, y, w, h) = self.bounds_to_cells(&node.bounds, scale_x, scale_y);
                    for row in y..y.saturating_add(h + 1).min(height) {
                        for col in x..x.saturating_add(w + 1).min(width) {
                            blocked_nodes[row * width + col] = true;
                        }
                    }
                }
            }
            let occupied: Vec<bool> = grid
                .iter()
                .enumerate()
                .flat_map(|(row, cells)| {
                    let own_route = &own_route;
                    let foreign_routes = &foreign_routes;
                    let blocked_nodes = &blocked_nodes;
                    cells.iter().enumerate().map(move |(col, &cell)| {
                        blocked_nodes[row * width + col]
                            || !blank_terminal_cell(foreign_routes[row][col])
                            || (!blank_terminal_cell(cell) && cell != own_route[row][col])
                    })
                })
                .collect();

            let mut caption = CellLabel::new(
                path.edge_index,
                false,
                wrap_cell_label(&text, width - 2, self.config.max_label_lines.max(1)),
            );
            let preferred = placer.place(
                path,
                (caption.width + 2) as f32 / scale_x,
                caption.lines.len() as f32 / scale_y,
            );
            let preferred = (
                preferred.x * scale_x + self.config.padding as f32,
                preferred.y * scale_y + self.config.padding as f32,
            );
            if let Some((x, y)) = fit_edge_label_cells(
                &occupied,
                &labels,
                preferred,
                &route,
                caption.width,
                caption.lines.len(),
                width,
                height,
                Some(2.0),
            ) {
                caption.x = x;
                caption.y = y;
                placer.update_last_placement(Some(caption.layout_bounds(
                    scale_x,
                    scale_y,
                    self.config.padding,
                )));
                clear_cell_label(grid, &caption);
                labels.push(caption);
                continue;
            }

            // A finite grid can have enough area for the words but no sufficiently wide slot
            // beside their route. Pair that caption with a stable, compact route badge. Both
            // use the same fit checks, and neither is painted unless the complete pair fits.
            let badge_text = format!("[{}]", path.edge_index + 1);
            caption = CellLabel::new(
                path.edge_index,
                false,
                wrap_cell_label(
                    &format!("{badge_text} {text}"),
                    width - 2,
                    self.config.max_label_lines.max(1),
                ),
            );
            placer.update_last_placement(None);
            let preferred = placer.place(
                path,
                (caption.width + 2) as f32 / scale_x,
                caption.lines.len() as f32 / scale_y,
            );
            let preferred = (
                preferred.x * scale_x + self.config.padding as f32,
                preferred.y * scale_y + self.config.padding as f32,
            );
            let Some((x, y)) = fit_edge_label_cells(
                &occupied,
                &labels,
                preferred,
                &route,
                caption.width,
                caption.lines.len(),
                width,
                height,
                None,
            ) else {
                placer.update_last_placement(None);
                occluded += 1;
                continue;
            };
            caption.x = x;
            caption.y = y;
            labels.push(caption);
            let mut badge = CellLabel::new(path.edge_index, true, vec![badge_text]);
            let anchor = fm_layout::edge_label_position(path);
            let anchor = (
                anchor.x * scale_x + self.config.padding as f32,
                anchor.y * scale_y + self.config.padding as f32,
            );
            let Some((x, y)) = fit_edge_label_cells(
                &occupied,
                &labels,
                anchor,
                &route,
                badge.width,
                1,
                width,
                height,
                Some(2.0),
            ) else {
                labels.pop();
                placer.update_last_placement(None);
                occluded += 1;
                continue;
            };
            badge.x = x;
            badge.y = y;
            let caption = labels.last().expect("paired caption was reserved");
            debug_assert!(
                caption.edge_index == badge.edge_index && !caption.is_callout && badge.is_callout
            );
            placer.update_last_placement(Some(caption.layout_bounds(
                scale_x,
                scale_y,
                self.config.padding,
            )));
            clear_cell_label(grid, caption);
            placer.place(path, (badge.width + 2) as f32 / scale_x, 1.0 / scale_y);
            placer.update_last_placement(Some(badge.layout_bounds(
                scale_x,
                scale_y,
                self.config.padding,
            )));
            clear_cell_label(grid, &badge);
            labels.push(badge);
        }
        (labels, occluded)
    }

    /// Rasterize the connectors using the same cell/subcell geometry as the drawing pass.
    /// Collinear shared portions are usable by either edge; distinct crossings stay protected.
    #[allow(clippy::too_many_arguments)]
    fn edge_label_route_grid(
        &self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        own: &LayoutEdgePath,
        foreign: bool,
        width: usize,
        height: usize,
        scale_x: f32,
        scale_y: f32,
    ) -> Vec<Vec<char>> {
        let cell_mode = self.config.tier == MermaidTier::Compact
            || self.config.render_mode == MermaidRenderMode::CellOnly;
        let (mult_x, mult_y) = self.config.subcell_multiplier();
        let mut canvas = Canvas::new(width, height, self.config.render_mode);
        let mut cells = CellBuffer::new(width, height);
        for path in &layout.edges {
            if is_invisible_edge(ir, path) || (path.edge_index == own.edge_index) == foreign {
                continue;
            }
            let mut segments = Vec::new();
            for pair in path.points.windows(2) {
                if foreign {
                    fm_layout::edge_labels::append_foreign_route_obstacles(
                        own,
                        pair[0],
                        pair[1],
                        &mut segments,
                    );
                } else {
                    segments.push(fm_layout::edge_labels::EdgeLabelObstacle::Segment(
                        pair[0], pair[1],
                    ));
                }
            }
            for obstacle in segments {
                let fm_layout::edge_labels::EdgeLabelObstacle::Segment(from, to) = obstacle else {
                    continue;
                };
                if cell_mode {
                    let (x0, y0) = self.point_to_cells(&from, scale_x, scale_y);
                    let (x1, y1) = self.point_to_cells(&to, scale_x, scale_y);
                    let arrow = ir
                        .edges
                        .get(path.edge_index)
                        .map_or(ArrowType::Arrow, |edge| edge.arrow);
                    self.draw_line_cell(
                        &mut cells,
                        x0,
                        y0,
                        x1,
                        y1,
                        &self.edge_glyphs,
                        path.reversed,
                        arrow,
                    );
                } else {
                    let x0 = (from.x * scale_x * mult_x as f32) as isize
                        + (self.config.padding * mult_x) as isize;
                    let y0 = (from.y * scale_y * mult_y as f32) as isize
                        + (self.config.padding * mult_y) as isize;
                    let x1 = (to.x * scale_x * mult_x as f32) as isize
                        + (self.config.padding * mult_x) as isize;
                    let y1 = (to.y * scale_y * mult_y as f32) as isize
                        + (self.config.padding * mult_y) as isize;
                    canvas.draw_line(x0, y0, x1, y1);
                }
            }
        }
        if cell_mode {
            cells.cells.chunks(width).map(<[char]>::to_vec).collect()
        } else {
            canvas.render_char_grid()
        }
    }

    fn render_generic_diagram_title(
        &self,
        cells: &mut [char],
        row_width: usize,
        ir: &MermaidDiagramIr,
    ) {
        let Some(title) = generic_terminal_diagram_title(ir) else {
            return;
        };
        if row_width == 0 || cells.len() < row_width {
            return;
        }

        let title = self.truncate_label(title);
        // Iterate the title chars directly (matches the other title path above): `chars().count()` for the
        // width, `chars().take().enumerate()` for placement — no per-title Vec<char> allocation.
        let title_len = title.chars().count().min(row_width);
        let start_x = row_width.saturating_sub(title_len) / 2;

        for (index, ch) in title.chars().take(title_len).enumerate() {
            cells[start_x + index] = ch;
        }
    }

    fn bounds_to_cells(
        &self,
        bounds: &fm_layout::LayoutRect,
        scale_x: f32,
        scale_y: f32,
    ) -> (usize, usize, usize, usize) {
        let (x, y) = self.layout_point_to_cells(bounds.x, bounds.y, scale_x, scale_y);
        let w = ((bounds.width * scale_x) as usize).max(3);
        let h = ((bounds.height * scale_y) as usize).max(2);

        (x + self.config.padding, y + self.config.padding, w, h)
    }

    fn point_to_cells(
        &self,
        point: &fm_layout::LayoutPoint,
        scale_x: f32,
        scale_y: f32,
    ) -> (usize, usize) {
        let (x, y) = self.layout_point_to_cells(point.x, point.y, scale_x, scale_y);

        (x + self.config.padding, y + self.config.padding)
    }

    /// Convert one layout-space point into terminal cell space through the CGA-backed transform.
    ///
    /// The direct arithmetic fallback preserves the established degenerate-scale behavior. Normal
    /// rendering obtains positive finite scales from `fit_cell_dimensions`, so it takes the rotor
    /// path; keeping the fallback makes this low-level conversion total for direct unit callers.
    fn layout_point_to_cells(&self, x: f32, y: f32, scale_x: f32, scale_y: f32) -> (usize, usize) {
        let (x, y) = TermTransform::new(scale_x, scale_y)
            .map(|transform| transform.apply(x, y))
            .unwrap_or((x * scale_x, y * scale_y));
        (x as usize, y as usize)
    }

    fn truncate_label(&self, text: &str) -> String {
        if !self.config.show_labels {
            return String::new();
        }
        let max_chars = self.config.max_label_chars.max(1);
        let max_lines = self.config.max_label_lines.max(1);
        let bytes = text.as_bytes();
        let mut previous_space = false;
        let unchanged_short_ascii = bytes.len() <= max_chars
            && bytes.first() != Some(&b' ')
            && bytes.iter().copied().all(|byte| {
                let is_space = byte == b' ';
                let valid = byte.is_ascii_graphic() || (is_space && !previous_space);
                previous_space = is_space;
                valid
            })
            && !previous_space;
        if unchanged_short_ascii {
            return text.to_owned();
        }

        let sanitized: String = text
            .chars()
            .map(|ch| match ch {
                '\n' => '\n',
                '\r' | '\t' => ' ',
                other if other.is_control() => ' ',
                other => other,
            })
            .collect();

        let mut lines: Vec<String> = Vec::new();
        let mut source_lines: Vec<&str> = sanitized.lines().collect();
        if source_lines.is_empty() {
            source_lines.push(sanitized.as_str());
        }

        for line in source_lines {
            if lines.len() >= max_lines {
                break;
            }
            // Word-wrap long lines at word boundaries.
            let wrapped = wrap_text(line, max_chars);
            for wrapped_line in wrapped {
                if lines.len() >= max_lines {
                    // Truncate the last line with ellipsis if there's more content.
                    if let Some(last) = lines.last_mut() {
                        let chars: Vec<char> = last.chars().collect();
                        if chars.len() >= max_chars {
                            *last = format!(
                                "{}…",
                                chars[..max_chars.saturating_sub(1)]
                                    .iter()
                                    .collect::<String>()
                            );
                        }
                    }
                    break;
                }
                lines.push(wrapped_line);
            }
        }

        lines.join("\n")
    }

    const fn is_selected_node(&self, node_index: usize) -> bool {
        self.config.show_selection
            && matches!(self.config.selected_node, Some(idx) if idx == node_index)
    }

    fn is_highlighted_node(&self, ir: &MermaidDiagramIr, node_index: usize) -> bool {
        let Some(selected) = self
            .config
            .selected_node
            .filter(|_| self.config.show_selection)
        else {
            return false;
        };
        if selected == node_index {
            return false;
        }
        let resolve = |ep: &fm_core::IrEndpoint| -> Option<usize> {
            match ep {
                fm_core::IrEndpoint::Node(id) => Some(id.0),
                fm_core::IrEndpoint::Port(port_id) => ir.ports.get(port_id.0).map(|p| p.node.0),
                fm_core::IrEndpoint::Unresolved => None,
            }
        };
        ir.edges.iter().any(|edge| {
            matches!(
                (resolve(&edge.from), resolve(&edge.to)),
                (Some(u), Some(v))
                    if (u == selected && v == node_index) || (v == selected && u == node_index)
            )
        })
    }

    fn node_link_url(node: &fm_core::IrNode) -> Option<&str> {
        node.href()
            .or_else(|| node.menu_links.first().map(|link| link.url.as_str()))
    }

    fn footnote_index(ir: &MermaidDiagramIr, target_node_id: &str) -> Option<usize> {
        let mut idx = 0usize;
        for node in &ir.nodes {
            if Self::node_link_url(node).is_some() {
                idx += 1;
                if node.id == target_node_id {
                    return Some(idx);
                }
            }
        }
        None
    }

    fn node_display_label(
        &self,
        ir: &MermaidDiagramIr,
        ir_node: Option<&fm_core::IrNode>,
        fallback_id: &str,
    ) -> Option<String> {
        let node = ir_node?;
        // A pure mark (state `[*]`, a fork bar, a Wardley dot) carries a generated id that the
        // shared rule says is never displayed; falling back to it printed `__state_start`.
        if is_block_beta_space_node(node) || ir.is_textless_ornament_node(node) {
            return None;
        }

        let mut text = node
            .label
            .and_then(|lid| ir.labels.get(lid.0))
            .map(|label| self.truncate_label(&label.text))
            .unwrap_or_else(|| self.truncate_label(fallback_id));

        if matches!(self.config.link_mode, fm_core::MermaidLinkMode::Footnote)
            && let Some(idx) = Self::footnote_index(ir, &node.id)
        {
            text.push_str(&format!(" [{idx}]"));
        }

        if self.config.show_selection
            && let Some(node_idx) = ir
                .nodes
                .iter()
                .position(|candidate| std::ptr::eq(candidate, node))
        {
            if self.is_selected_node(node_idx) {
                let prefix = if matches!(self.config.glyph_mode, fm_core::MermaidGlyphMode::Ascii) {
                    "> "
                } else {
                    "▶ "
                };
                text = format!("{prefix}{text}");
            } else if self.is_highlighted_node(ir, node_idx) {
                let prefix = if matches!(self.config.glyph_mode, fm_core::MermaidGlyphMode::Ascii) {
                    "* "
                } else {
                    "• "
                };
                text = format!("{prefix}{text}");
            }
        }

        Some(text)
    }

    /// Render a UML-style three-compartment class box into the character grid.
    ///
    /// Layout:
    /// ```text
    /// ┌──────────┐
    /// │ ClassName │  ← header (centered)
    /// ├──────────┤
    /// │ +name    │  ← attributes with visibility
    /// │ -age     │
    /// ├──────────┤
    /// │ +eat()   │  ← methods with visibility
    /// └──────────┘
    /// ```
    /// Draw a C4 node as a name header, a divider, then its type, technology and description
    /// (bd-039t).
    ///
    /// Decorations match fm-render-svg — `<<Person>>` and `[technology]` — so the two renderers say
    /// the same thing about the same element.
    #[allow(clippy::too_many_arguments)]
    fn overlay_c4_rows(
        &self,
        grid: &mut [Vec<char>],
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        ir: &MermaidDiagramIr,
        node: &fm_core::IrNode,
        meta: &fm_core::IrC4NodeMeta,
        grid_width: usize,
    ) {
        let inner_w = w.saturating_sub(2);
        let glyphs = &self.box_glyphs;

        let write_text =
            |grid: &mut [Vec<char>], row: usize, col: usize, text: &str, max_w: usize| {
                if row >= grid.len() {
                    return;
                }
                for (i, ch) in text.chars().take(max_w).enumerate() {
                    let c = col + i;
                    if c < grid_width && c < grid[row].len() {
                        grid[row][c] = ch;
                    }
                }
            };

        let draw_separator = |grid: &mut [Vec<char>], row: usize| {
            if row >= grid.len() {
                return;
            }
            if x < grid_width && x < grid[row].len() {
                grid[row][x] = glyphs.t_right;
            }
            for dx in 1..w.saturating_sub(1) {
                let c = x + dx;
                if c < grid_width && c < grid[row].len() {
                    grid[row][c] = glyphs.horizontal;
                }
            }
            let right = x + w.saturating_sub(1);
            if right < grid_width && right < grid[row].len() {
                grid[row][right] = glyphs.t_left;
            }
        };

        let mut row = y + 1;
        let max_content_row = if h >= 2 { y + h - 1 } else { y + h };

        let name = node
            .label
            .and_then(|lid| ir.labels.get(lid.0))
            .map(|l| l.text.as_str())
            .unwrap_or(&node.id);
        let name_text = self.truncate_label(name);
        let name_chars = name_text.chars().count();
        let name_x = x + 1 + inner_w.saturating_sub(name_chars) / 2;
        if row < max_content_row {
            write_text(grid, row, name_x, &name_text, inner_w);
            row += 1;
        }
        if row < max_content_row {
            draw_separator(grid, row);
            row += 1;
        }

        let mut rows: Vec<String> = Vec::with_capacity(3);
        if !meta.element_type.is_empty() {
            rows.push(format!("<<{}>>", meta.element_type));
        }
        if let Some(technology) = meta.technology.as_deref().filter(|t| !t.is_empty()) {
            rows.push(format!("[{technology}]"));
        }
        if let Some(description) = meta.description.as_deref().filter(|d| !d.is_empty()) {
            rows.push(description.to_string());
        }
        for text in rows {
            if row >= max_content_row {
                // Out of box: stop rather than spill rows into whatever is laid out below.
                break;
            }
            write_text(grid, row, x + 1, &self.truncate_label(&text), inner_w);
            row += 1;
        }
    }

    /// Draw a requirement node as a name header, a divider and one row per declared field (bd-039t).
    ///
    /// Mirrors `overlay_er_compartments`; the field set matches the rows fm-render-svg draws, so the
    /// two renderers say the same thing about the same requirement.
    #[allow(clippy::too_many_arguments)]
    fn overlay_requirement_rows(
        &self,
        grid: &mut [Vec<char>],
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        ir: &MermaidDiagramIr,
        node: &fm_core::IrNode,
        meta: &fm_core::IrRequirementNodeMeta,
        grid_width: usize,
    ) {
        let inner_w = w.saturating_sub(2);
        let glyphs = &self.box_glyphs;

        let write_text =
            |grid: &mut [Vec<char>], row: usize, col: usize, text: &str, max_w: usize| {
                if row >= grid.len() {
                    return;
                }
                for (i, ch) in text.chars().take(max_w).enumerate() {
                    let c = col + i;
                    if c < grid_width && c < grid[row].len() {
                        grid[row][c] = ch;
                    }
                }
            };

        let draw_separator = |grid: &mut [Vec<char>], row: usize| {
            if row >= grid.len() {
                return;
            }
            if x < grid_width && x < grid[row].len() {
                grid[row][x] = glyphs.t_right;
            }
            for dx in 1..w.saturating_sub(1) {
                let c = x + dx;
                if c < grid_width && c < grid[row].len() {
                    grid[row][c] = glyphs.horizontal;
                }
            }
            let right = x + w.saturating_sub(1);
            if right < grid_width && right < grid[row].len() {
                grid[row][right] = glyphs.t_left;
            }
        };

        let mut row = y + 1;
        let max_content_row = if h >= 2 { y + h - 1 } else { y + h };

        let name = node
            .label
            .and_then(|lid| ir.labels.get(lid.0))
            .map(|l| l.text.as_str())
            .unwrap_or(&node.id);
        let name_text = self.truncate_label(name);
        let name_chars = name_text.chars().count();
        let name_x = x + 1 + inner_w.saturating_sub(name_chars) / 2;
        if row < max_content_row {
            write_text(grid, row, name_x, &name_text, inner_w);
            row += 1;
        }
        if row < max_content_row {
            draw_separator(grid, row);
            row += 1;
        }

        // Field order matches the SVG's row order so the two renderers read alike. `type`/`doc` are
        // an ELEMENT's fields and join the same table for the same reason (bd-qdmn); the SVG orders
        // them ID, Text, Type, Doc, then Risk/Verify, so the risk and verify rows stay last here to
        // preserve that reading order rather than merely to append.
        let fields: [(&str, Option<&str>); 6] = [
            ("id: ", meta.req_id.as_deref()),
            ("text: ", meta.text.as_deref()),
            ("type: ", meta.element_type.as_deref()),
            ("doc: ", meta.doc_ref.as_deref()),
            ("risk: ", meta.risk.as_deref()),
            ("verify: ", meta.verify_method.as_deref()),
        ];
        for (prefix, value) in fields {
            let Some(value) = value.filter(|v| !v.is_empty()) else {
                continue;
            };
            if row >= max_content_row {
                // Out of box: stop rather than spill rows into whatever is laid out below.
                break;
            }
            let mut text = String::with_capacity(prefix.len() + value.len());
            text.push_str(prefix);
            text.push_str(value);
            write_text(grid, row, x + 1, &self.truncate_label(&text), inner_w);
            row += 1;
        }
    }

    /// Draw an ER entity as a name header, a divider and one row per attribute (bd-ekx2).
    ///
    /// The row text mirrors fm-render-svg EXACTLY — `{key_prefix}{data_type} {name}`, plus the
    /// comment when present — so the two renderers say the same thing about the same entity. It is
    /// also the concatenation `er_attribute_row_width` measures in layout, which is why the box
    /// already has room for it.
    #[allow(clippy::too_many_arguments)]
    fn overlay_er_compartments(
        &self,
        grid: &mut [Vec<char>],
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        ir: &MermaidDiagramIr,
        node: &fm_core::IrNode,
        grid_width: usize,
    ) {
        let inner_w = w.saturating_sub(2);
        let glyphs = &self.box_glyphs;

        let write_text =
            |grid: &mut [Vec<char>], row: usize, col: usize, text: &str, max_w: usize| {
                if row >= grid.len() {
                    return;
                }
                for (i, ch) in text.chars().take(max_w).enumerate() {
                    let c = col + i;
                    if c < grid_width && c < grid[row].len() {
                        grid[row][c] = ch;
                    }
                }
            };

        let draw_separator = |grid: &mut [Vec<char>], row: usize| {
            if row >= grid.len() {
                return;
            }
            if x < grid_width && x < grid[row].len() {
                grid[row][x] = glyphs.t_right;
            }
            for dx in 1..w.saturating_sub(1) {
                let c = x + dx;
                if c < grid_width && c < grid[row].len() {
                    grid[row][c] = glyphs.horizontal;
                }
            }
            let right = x + w.saturating_sub(1);
            if right < grid_width && right < grid[row].len() {
                grid[row][right] = glyphs.t_left;
            }
        };

        let mut row = y + 1;
        // Content must stay above the bottom border row, exactly as the class overlay does: a box
        // too short for even the header draws nothing rather than writing over its own border.
        let max_content_row = if h >= 2 { y + h - 1 } else { y + h };

        let entity_name = node
            .label
            .and_then(|lid| ir.labels.get(lid.0))
            .map(|l| l.text.as_str())
            .unwrap_or(&node.id);
        let name_text = self.truncate_label(entity_name);
        let name_chars = name_text.chars().count();
        let name_x = x + 1 + inner_w.saturating_sub(name_chars) / 2;
        if row < max_content_row {
            write_text(grid, row, name_x, &name_text, inner_w);
            row += 1;
        }

        if row < max_content_row {
            draw_separator(grid, row);
            row += 1;
        }

        // ER attribute cells at shared CHARACTER columns when they fit, fused otherwise (bd-jbrzc).
        //
        // ⚠️ CHARACTER COLUMNS, NOT `fm_core::er_cell_columns`. That helper returns PIXEL offsets
        // measured from proportional font metrics, which is the right answer for SVG and canvas and
        // a meaningless one on a grid where every glyph occupies exactly one cell. Reusing it here
        // would be sharing a name rather than a rule.
        //
        // ⚠️ AND THE TWO MEASURES HAVE NO GUARANTEED RELATIONSHIP. The box's cell width comes from
        // scaling a PIXEL width that was itself sized from pixel columns; character columns count
        // characters. Text made of wide glyphs measures large in pixels and small in characters,
        // and narrow glyphs the reverse — so a document exists for which aligned columns do not fit
        // a box that the pixel columns fit fine. Hence the guard: align when there is room, and
        // otherwise draw exactly what this surface drew before, which shows the start of every
        // field rather than a truncated first column. Measured on the bead's skew fixture
        // `T { verylongtypename a / t verylongattributename PK }`: 41 columns needed against 54
        // available, so that one aligns.
        let (offsets, columns_width) = Self::er_character_columns(&node.members);
        let columns_fit = columns_width <= inner_w;

        for attr in &node.members {
            if row >= max_content_row {
                // Out of box: stop rather than spill rows into whatever is laid out below.
                break;
            }
            if columns_fit {
                let key = attr.key_cell();
                let cells: [&str; 4] = [
                    attr.data_type.as_str(),
                    attr.name.as_str(),
                    key.as_ref(),
                    attr.comment.as_deref().unwrap_or(""),
                ];
                for (index, cell) in cells.iter().enumerate() {
                    if cell.is_empty() {
                        continue;
                    }
                    write_text(
                        grid,
                        row,
                        x + 1 + offsets[index],
                        cell,
                        inner_w.saturating_sub(offsets[index]),
                    );
                }
            } else {
                // Shared composition — see `IrEntityAttribute::display_row`.
                let text = attr.display_row();
                write_text(grid, row, x + 1, &self.truncate_label(&text), inner_w);
            }
            row += 1;
        }
    }

    /// Character-cell column offsets for an ER entity's attributes, and the total width they need.
    ///
    /// ⚠️ CHARACTERS, NOT `fm_core::er_cell_columns`. That helper returns PIXEL offsets from
    /// proportional font metrics — the right answer for SVG and canvas, and a meaningless one on a grid
    /// where every glyph occupies exactly one cell. Reusing it here would share a name rather than a
    /// rule.
    ///
    /// Extracted from the drawing loop so the fits-or-fuse decision can be tested directly. The
    /// fallback it feeds could not be reached from any rendered fixture tried — narrow glyphs, wide
    /// glyphs, long comments, and six attributes all fit — so a rendering test cannot cover it, and an
    /// untestable branch is exactly what this bead warns against shipping.
    pub(crate) fn er_character_columns(
        members: &[fm_core::IrEntityAttribute],
    ) -> ([usize; 4], usize) {
        let mut widths = [0_usize; 4];
        for attr in members {
            let key = attr.key_cell();
            let cells: [&str; 4] = [
                attr.data_type.as_str(),
                attr.name.as_str(),
                key.as_ref(),
                attr.comment.as_deref().unwrap_or(""),
            ];
            for (index, cell) in cells.iter().enumerate() {
                widths[index] = widths[index].max(cell.chars().count());
            }
        }
        let mut offsets = [0_usize; 4];
        let mut cursor = 0_usize;
        for index in 0..4 {
            offsets[index] = cursor;
            if widths[index] > 0 {
                cursor += widths[index] + 1;
            }
        }
        // `cursor` overshoots by one trailing gutter, which no cell occupies.
        (offsets, cursor.saturating_sub(1))
    }

    #[allow(clippy::too_many_arguments)]
    fn overlay_class_compartments(
        &self,
        grid: &mut [Vec<char>],
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        ir: &MermaidDiagramIr,
        node: &fm_core::IrNode,
        meta: &fm_core::IrClassNodeMeta,
        grid_width: usize,
    ) {
        let inner_w = w.saturating_sub(2); // Width inside borders
        let glyphs = &self.box_glyphs;

        // Helper to write a left-aligned string into the grid at (row, col).
        let write_text =
            |grid: &mut [Vec<char>], row: usize, col: usize, text: &str, max_w: usize| {
                if row >= grid.len() {
                    return;
                }
                for (i, ch) in text.chars().take(max_w).enumerate() {
                    let c = col + i;
                    if c < grid_width && c < grid[row].len() {
                        grid[row][c] = ch;
                    }
                }
            };

        // Helper to draw a horizontal separator.
        let draw_separator = |grid: &mut [Vec<char>], row: usize| {
            if row >= grid.len() {
                return;
            }
            if x < grid_width && x < grid[row].len() {
                grid[row][x] = glyphs.t_right;
            }
            for dx in 1..w.saturating_sub(1) {
                let c = x + dx;
                if c < grid_width && c < grid[row].len() {
                    grid[row][c] = glyphs.horizontal;
                }
            }
            let right = x + w.saturating_sub(1);
            if right < grid_width && right < grid[row].len() {
                grid[row][right] = glyphs.t_left;
            }
        };

        let mut row = y + 1; // Start inside the top border.
        // Content must stay above the bottom border row.
        let max_content_row = if h >= 2 { y + h - 1 } else { y + h };

        // STEREOTYPE, above the name and centred, exactly where fm-render-svg puts it (bd-039t).
        //
        // Measured SVG vs terminal: `<<interface>> Alpha` drew the stereotype in the SVG and not in
        // the terminal. Unlike the ER, requirement and C4 cases in this bead — each of which needed
        // a whole new overlay — this one is a gap INSIDE an overlay that already existed: the class
        // compartments drew name, attributes and methods and simply skipped `meta.stereotype`.
        //
        // The variant-to-text mapping mirrors `write_class_stereotype_into` in fm-render-svg,
        // including that `Enum` renders as `<<enumeration>>` and a `Custom` stereotype is written
        // verbatim (the author already supplied its own brackets).
        if let Some(stereotype) = &meta.stereotype
            && row < max_content_row
        {
            let stereo_text = stereotype.label();
            let stereo = self.truncate_label(&stereo_text);
            let stereo_chars = stereo.chars().count();
            let stereo_x = x + 1 + inner_w.saturating_sub(stereo_chars) / 2;
            write_text(grid, row, stereo_x, &stereo, inner_w);
            row += 1;
        }

        // Header: class name (centered).
        let class_name = node
            .label
            .and_then(|lid| ir.labels.get(lid.0))
            .map(|l| l.text.as_str())
            .unwrap_or(&node.id);
        let display_name = meta.display_header(class_name);
        let name_text = self.truncate_label(&display_name);
        let name_chars = name_text.chars().count();
        let name_x = x + 1 + inner_w.saturating_sub(name_chars) / 2;
        write_text(grid, row, name_x, &name_text, inner_w);
        row += 1;

        // Separator after header.
        if row < max_content_row {
            draw_separator(grid, row);
            row += 1;
        }

        // Attributes compartment.
        for attr in &meta.attributes {
            if row >= max_content_row {
                break;
            }
            let vis = visibility_symbol(attr.visibility);
            let text = format!(
                "{vis}{}",
                fm_core::class_member_display_name(&attr.name, false)
            );
            write_text(grid, row, x + 1, &text, inner_w);
            row += 1;
        }

        // Separator before methods (only if we have both attributes and methods).
        if !meta.attributes.is_empty() && !meta.methods.is_empty() && row < max_content_row {
            draw_separator(grid, row);
            row += 1;
        }

        // Methods compartment.
        for method in &meta.methods {
            if row >= max_content_row {
                break;
            }
            let vis = visibility_symbol(method.visibility);
            // ⚠️ THIS BACKEND DELIBERATELY KEEPS THE LITERAL CHARACTER (bd-r2gll), and it is the one
            // arm of the five that does. Everywhere else the classifier became a STYLE, matching
            // mermaid: `text-decoration:underline` for static, `font-style:italic` for abstract,
            // with no `$`/`*` in the drawn text.
            //
            // A terminal grid cannot express either. `TermRenderResult.output` is a plain `String`
            // of cells with no ANSI in it anywhere in this renderer, so calling
            // `class_member_classifier_css` here would mean discarding its answer and drawing
            // nothing — silently losing the static/abstract distinction rather than rendering it
            // differently. Keeping the marker preserves the information in the only channel this
            // backend has. mermaid has no terminal target, so nothing is diverging FROM anything.
            //
            // This is why the row text is built here rather than shared: the string genuinely
            // differs between backends, and `fm_layout::class_member_row_width` measures the
            // classifier-free form the SVG and canvas draw.
            let suffix = if method.is_abstract {
                "*"
            } else if method.is_static {
                "$"
            } else {
                ""
            };
            // ` : `, matching the SVG and canvas backends and the width the layout computed
            // for this row (bd-ci658).
            let ret = method
                .return_type
                .as_deref()
                .map(|t| format!(" : {}", fm_core::parse_generic_types(t)))
                .unwrap_or_default();
            let text = format!(
                "{vis}{}{suffix}{ret}",
                fm_core::class_member_display_name(&method.name, true)
            );
            write_text(grid, row, x + 1, &text, inner_w);
            row += 1;
        }
    }
}

/// Map ClassVisibility to its UML symbol.
fn visibility_symbol(vis: fm_core::ClassVisibility) -> &'static str {
    match vis {
        fm_core::ClassVisibility::Unmarked => "",
        fm_core::ClassVisibility::Public => "+",
        fm_core::ClassVisibility::Private => "-",
        fm_core::ClassVisibility::Protected => "#",
        fm_core::ClassVisibility::Package => "~",
    }
}

/// Wrap text at word boundaries to fit within `max_width` characters per line.
///
/// Uses greedy word-fit: words are placed on the current line until the next
/// word would exceed the width. A single word wider than the target is placed
/// on its own line and truncated with ellipsis.
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let max_width = max_width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        let word_len = word.chars().count();

        if current_line.is_empty() {
            // First word on line — always place it, truncate if needed.
            if word_len <= max_width {
                current_line.push_str(word);
            } else {
                let truncated: String = word.chars().take(max_width.saturating_sub(1)).collect();
                current_line = format!("{truncated}…");
            }
        } else {
            let current_len = current_line.chars().count();
            // Check if word fits on current line (+ 1 for space).
            if current_len + 1 + word_len <= max_width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                // Word doesn't fit — push current line and start new one.
                lines.push(current_line);
                if word_len <= max_width {
                    current_line = word.to_string();
                } else {
                    let truncated: String =
                        word.chars().take(max_width.saturating_sub(1)).collect();
                    current_line = format!("{truncated}…");
                }
            }
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Simple character cell buffer for cell-mode rendering.
struct CellBuffer {
    cells: Vec<char>,
    width: usize,
    height: usize,
}

impl CellBuffer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            cells: vec![' '; width * height],
            width,
            height,
        }
    }

    fn set(&mut self, x: usize, y: usize, ch: char) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = ch;
        }
    }

    fn set_string(&mut self, x: usize, y: usize, s: &str) {
        for (i, ch) in s.chars().enumerate() {
            self.set(x + i, y, ch);
        }
    }

    fn to_output_string(&self) -> String {
        let mut output = String::with_capacity(
            self.cells
                .len()
                .saturating_add(self.height.saturating_sub(1)),
        );
        for y in 0..self.height {
            if y > 0 {
                output.push('\n');
            }
            let start = y * self.width;
            let row = &self.cells[start..start + self.width];
            let retained_len = row
                .iter()
                .rposition(|ch| !ch.is_whitespace())
                .map_or(0, |index| index + 1);
            output.extend(row[..retained_len].iter().copied());
        }
        output
    }
}

impl std::fmt::Display for CellBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for y in 0..self.height {
            if y > 0 {
                writeln!(f)?;
            }
            let start = y * self.width;
            let end = start + self.width;
            let line: String = self.cells[start..end].iter().collect();
            write!(f, "{}", line.trim_end())?;
        }
        Ok(())
    }
}

/// Render an IR diagram to terminal output with default configuration.
#[must_use]
pub fn render_diagram(ir: &MermaidDiagramIr) -> TermRenderResult {
    render_diagram_with_config(ir, &TermRenderConfig::default(), 80, 24)
}

/// Render an IR diagram to terminal output with custom configuration.
#[must_use]
pub fn render_diagram_with_config(
    ir: &MermaidDiagramIr,
    config: &TermRenderConfig,
    cols: usize,
    rows: usize,
) -> TermRenderResult {
    let resolved = ResolvedConfig::resolve(config, cols, rows);
    let renderer = TermRenderer::new(resolved);
    renderer.render(ir)
}

/// Render an IR diagram to terminal output using a pre-computed layout.
#[must_use]
pub fn render_diagram_with_layout_and_config(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    config: &TermRenderConfig,
    cols: usize,
    rows: usize,
) -> TermRenderResult {
    let resolved = ResolvedConfig::resolve(config, cols, rows);
    let renderer = TermRenderer::new(resolved);
    renderer.render_layout(ir, layout)
}

fn is_block_beta_space_node(node: &fm_core::IrNode) -> bool {
    node.id.starts_with("__space_")
        || node
            .classes
            .iter()
            .any(|class_name| class_name.eq_ignore_ascii_case("block-beta-space"))
}

/// Render a treemap value for a terminal caption: no trailing zeros on a whole number.
///
/// Deliberately the same rule the SVG renderer applies, so the two surfaces never disagree about
/// what a value IS — a `30` here and a `30.0` there would read as two different numbers.
fn format_terminal_treemap_value(value: f64) -> String {
    if (value - value.round()).abs() < 1e-9 {
        format!("{}", value.round() as i64)
    } else {
        let text = format!("{value:.4}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Append the C4 legend below the fixed terminal grid.
///
/// Unlike SVG, a terminal renderer cannot enlarge a completed diagram grid without changing its
/// scale and overwriting content. A footer preserves the grid exactly, remains visible at any
/// viewport size, and reports its extra rows through [`TermRenderResult`].
fn append_c4_legend(output: &mut String, ir: &MermaidDiagramIr) -> (usize, usize) {
    if !terminal_c4_legend_enabled(ir) {
        return (0, 0);
    }

    let entries = terminal_c4_legend_entries(ir);
    let rows = entries.len() + 2;
    let mut width = "C4 Legend".chars().count();
    output.push_str("\n\nC4 Legend");
    for entry in &entries {
        width = width.max(entry.chars().count());
        output.push('\n');
        output.push_str(entry);
    }
    (rows, width)
}

fn append_footnote_links(
    output: &mut String,
    ir: &MermaidDiagramIr,
    link_mode: fm_core::MermaidLinkMode,
) -> (usize, usize) {
    if !matches!(link_mode, fm_core::MermaidLinkMode::Footnote) {
        return (0, 0);
    }

    let mut entries = Vec::new();
    for node in &ir.nodes {
        if let Some(url) = TermRenderer::node_link_url(node) {
            let idx = entries.len() + 1;
            entries.push(format!("[{idx}] {}: {url}", node.id));
        }
    }
    if entries.is_empty() {
        return (0, 0);
    }

    let mut width = "Links:".chars().count();
    output.push_str("\nLinks:");
    for entry in &entries {
        width = width.max(entry.chars().count());
        output.push('\n');
        output.push_str(entry);
    }
    (entries.len() + 1, width)
}

#[cfg(test)]
fn append_term_debug_overlay(output: &mut String, layout: &DiagramLayout) -> (usize, usize) {
    let rank_count = layout.nodes.iter().map(|n| n.rank + 1).max().unwrap_or(0);
    let banner = format!(
        "[debug: nodes={} edges={} ranks={} crossings={} cycles={}]",
        layout.nodes.len(),
        layout.edges.len(),
        rank_count,
        layout.stats.crossing_count,
        layout.stats.cycle_count,
    );
    let width = banner.chars().count();
    output.push('\n');
    output.push_str(&banner);
    (1, width)
}

#[derive(Debug)]
struct CellLabel {
    edge_index: usize,
    is_callout: bool,
    x: usize,
    y: usize,
    width: usize,
    lines: Vec<String>,
}

impl CellLabel {
    fn new(edge_index: usize, is_callout: bool, lines: Vec<String>) -> Self {
        let width = lines.iter().map(|line| line.width()).max().unwrap_or(0);
        Self {
            edge_index,
            is_callout,
            x: 0,
            y: 0,
            width,
            lines,
        }
    }

    fn layout_bounds(&self, scale_x: f32, scale_y: f32, padding: usize) -> fm_layout::LayoutRect {
        fm_layout::LayoutRect {
            x: (self.x as f32 - 1.0 - padding as f32) / scale_x,
            y: (self.y as f32 - padding as f32) / scale_y,
            width: (self.width + 2) as f32 / scale_x,
            height: self.lines.len() as f32 / scale_y,
        }
    }
}

fn blank_terminal_cell(cell: char) -> bool {
    cell == ' ' || cell == '\u{2800}'
}

fn clear_cell_label(grid: &mut [Vec<char>], label: &CellLabel) {
    for row in &mut grid[label.y..label.y + label.lines.len()] {
        row[label.x - 1..label.x + label.width + 1].fill(' ');
    }
}

/// Fit the measured shared proposal to actual character cells. The prefix sum checks the full
/// painted backing rectangle, including separating blanks, without ever damaging occupied cells.
#[allow(clippy::too_many_arguments)]
fn fit_edge_label_cells(
    occupied: &[bool],
    labels: &[CellLabel],
    preferred: (f32, f32),
    route: &[(f32, f32)],
    label_width: usize,
    label_height: usize,
    cell_width: usize,
    cell_height: usize,
    max_route_gap: Option<f32>,
) -> Option<(usize, usize)> {
    if label_width + 2 > cell_width || label_height > cell_height || label_height == 0 {
        return None;
    }
    let mut occupied = occupied.to_vec();
    for label in labels {
        for row in label.y..label.y + label.lines.len() {
            for col in label.x..label.x + label.width {
                occupied[row * cell_width + col] = true;
            }
        }
    }
    let stride = cell_width + 1;
    let mut prefix = vec![0_usize; stride * (cell_height + 1)];
    for row in 0..cell_height {
        let mut count = 0;
        for col in 0..cell_width {
            count += usize::from(occupied[row * cell_width + col]);
            prefix[(row + 1) * stride + col + 1] = prefix[row * stride + col + 1] + count;
        }
    }
    let mut best = None;
    let mut best_score = f32::INFINITY;
    for row in 0..=cell_height - label_height {
        for col in 1..=cell_width - label_width - 1 {
            let (left, right, bottom) = (col - 1, col + label_width + 1, row + label_height);
            let count = prefix[bottom * stride + right] + prefix[row * stride + left]
                - prefix[row * stride + right]
                - prefix[bottom * stride + left];
            if count != 0 {
                continue;
            }
            let gap = cell_label_route_gap(route, col, row, label_width, label_height);
            if max_route_gap.is_some_and(|limit| gap > limit) {
                continue;
            }
            let center = (
                col as f32 + label_width as f32 / 2.0,
                row as f32 + label_height as f32 / 2.0,
            );
            let score =
                (center.0 - preferred.0).abs() + (center.1 - preferred.1).abs() * 2.0 + gap * 4.0;
            if score < best_score {
                best = Some((col, row));
                best_score = score;
            }
        }
    }
    best
}

/// Distance from the actual caption rectangle to its own polyline, with terminal rows weighted
/// twice as high as columns. Measuring from the text's center wrongly detaches long side labels.
fn cell_label_route_gap(
    route: &[(f32, f32)],
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> f32 {
    let (left, right) = (x as f32, (x + width) as f32);
    let (top, bottom) = (y as f32 * 2.0, (y + height) as f32 * 2.0);
    let mut best = f32::INFINITY;
    for pair in route.windows(2) {
        let a = (pair[0].0, pair[0].1 * 2.0);
        let b = (pair[1].0, pair[1].1 * 2.0);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let (mut enter, mut leave) = (0.0_f32, 1.0_f32);
        let mut intersects = true;
        for (origin, delta, lo, hi) in [(a.0, dx, left, right), (a.1, dy, top, bottom)] {
            if delta.abs() <= f32::EPSILON {
                intersects &= origin >= lo && origin <= hi;
            } else {
                let (p, q) = ((lo - origin) / delta, (hi - origin) / delta);
                enter = enter.max(p.min(q));
                leave = leave.min(p.max(q));
            }
        }
        if intersects && enter <= leave {
            return 0.0;
        }
        for point in [a, b] {
            let gx = (left - point.0).max(0.0).max(point.0 - right);
            let gy = (top - point.1).max(0.0).max(point.1 - bottom);
            best = best.min(gx * gx + gy * gy);
        }
        let length = dx * dx + dy * dy;
        for corner in [(left, top), (left, bottom), (right, top), (right, bottom)] {
            let t = if length > f32::EPSILON {
                (((corner.0 - a.0) * dx + (corner.1 - a.1) * dy) / length).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let gx = corner.0 - (a.0 + dx * t);
            let gy = corner.1 - (a.1 + dy * t);
            best = best.min(gx * gx + gy * gy);
        }
    }
    best.sqrt()
}

fn edge_caption_text(text: &str, budget: usize) -> String {
    let mut chars = text.chars().map(|ch| match ch {
        '\n' => '\n',
        other if other.is_control() => ' ',
        other => other,
    });
    let mut result: String = chars.by_ref().take(budget).collect();
    if chars.next().is_some() {
        result.pop();
        result.push('…');
    }
    result
}

fn wrap_cell_label(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for source_line in text.lines() {
        let mut line = String::new();
        for word in source_line.split_whitespace() {
            let needed = line.width() + usize::from(!line.is_empty()) + word.width();
            if !line.is_empty() && needed > width {
                lines.push(line);
                line = String::new();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            if word.width() <= width {
                line.push_str(word);
            } else {
                for ch in word.chars() {
                    line.push(ch);
                    if line.width() >= width {
                        line.pop();
                        break;
                    }
                }
                line.push('…');
            }
        }
        lines.push(line);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            while last.width() >= width {
                last.pop();
            }
            last.push('…');
        }
    }
    lines
}

/// Emit complete caption strings while skipping their measured columns. A Unicode scalar is
/// not a terminal cell: CJK glyphs occupy two columns and combining marks occupy none.
fn serialize_cell_labels(grid: &[Vec<char>], labels: &[CellLabel], trim: bool) -> String {
    let mut output = String::new();
    for (row_index, row) in grid.iter().enumerate() {
        if row_index > 0 {
            output.push('\n');
        }
        let mut runs: Vec<_> = labels
            .iter()
            .filter_map(|label| {
                let offset = row_index.checked_sub(label.y)?;
                let text = label.lines.get(offset)?;
                Some((label.x + (label.width - text.width()) / 2, text))
            })
            .collect();
        runs.sort_by_key(|(col, _)| *col);
        let mut line = String::new();
        let mut cursor = 0;
        for (col, text) in runs {
            line.extend(row[cursor..col].iter());
            line.push_str(text);
            cursor = col + text.width();
        }
        line.extend(row[cursor..].iter());
        output.push_str(if trim { line.trim_end() } else { &line });
    }
    output
}

/// Place the complete label and a horizontal gutter near its route, within the cell frame.
/// The distance ordering is deterministic, preferring a vertical slide when distances tie.
/// Search both axes: fixing the column can make a clear half of the viewport unreachable.
fn place_edge_label(
    occupied: &[(usize, usize, usize, usize)],
    midpoint_x: usize,
    preferred_y: usize,
    label_width: usize,
    label_height: usize,
    cell_width: usize,
    cell_height: usize,
) -> (usize, usize) {
    let max_x = cell_width.saturating_sub(label_width);
    let max_y = cell_height.saturating_sub(label_height);
    let preferred_x = midpoint_x.saturating_sub(label_width / 2).min(max_x);
    let preferred_y = preferred_y.min(max_y);
    let mut closest = (preferred_x, preferred_y);
    let mut best = (usize::MAX, usize::MAX, usize::MAX, usize::MAX);
    for y in 0..=max_y {
        for x in 0..=max_x {
            let left = x.saturating_sub(1);
            let right = x.saturating_add(label_width + 1).min(cell_width);
            let bottom = y.saturating_add(label_height).min(cell_height);
            // In an exhausted viewport minimize the damaged area, while a free placement always
            // beats every overlapping candidate. Do not silently drop the transition text.
            let overlap = occupied
                .iter()
                .map(|&(ox, oy, oright, obottom)| {
                    right.min(oright).saturating_sub(left.max(ox))
                        * bottom.min(obottom).saturating_sub(y.max(oy))
                })
                .sum::<usize>();
            let dx = x.abs_diff(preferred_x);
            let dy = y.abs_diff(preferred_y);
            let score = (overlap, dx + dy, dx, y);
            if score < best {
                best = score;
                closest = (x, y);
            }
        }
    }
    closest
}

fn terminal_c4_legend_enabled(ir: &MermaidDiagramIr) -> bool {
    matches!(
        ir.diagram_type,
        fm_core::DiagramType::C4Context
            | fm_core::DiagramType::C4Container
            | fm_core::DiagramType::C4Component
            | fm_core::DiagramType::C4Dynamic
            | fm_core::DiagramType::C4Deployment
    ) && ir.meta.c4_show_legend
}

fn terminal_c4_legend_entries(ir: &MermaidDiagramIr) -> Vec<&'static str> {
    let has_class = |needle: &str| {
        ir.nodes
            .iter()
            .flat_map(|node| node.classes.iter())
            .any(|class_name| class_name == needle)
    };
    let has_boundary = ir.clusters.iter().any(|cluster| {
        cluster
            .title
            .and_then(|label_id| ir.labels.get(label_id.0))
            .is_some_and(|label| {
                label.text.contains("Boundary") || label.text.contains("Deployment_Node")
            })
    });

    let mut entries = Vec::new();
    if has_class("c4-person") {
        entries.push("◉ Person");
    }
    if has_class("c4-system") {
        entries.push("▭ System");
    }
    if has_class("c4-container") {
        entries.push("▣ Container");
    }
    if has_class("c4-component") {
        entries.push("◫ Component");
    }
    if has_class("c4-database") {
        entries.push("◌ Database");
    }
    if has_class("c4-queue") {
        entries.push("▱ Queue");
    }
    if has_class("c4-external") {
        entries.push("╌ External");
    }
    if has_boundary {
        entries.push("⬚ Boundary");
    }
    entries
}

fn generic_terminal_diagram_title(ir: &MermaidDiagramIr) -> Option<&str> {
    // The title used to be SUPPRESSED for pie, gantt, xychart and quadrant, on the ground that each
    // "has a specialized title renderer" that would draw it instead. None of them does. Measured
    // with the shipping binary, a `title ZZTITLE` on each of the four appears in the SVG and is
    // absent from `-f term`, while the generic types (flowchart, journey) show it in both. The
    // guard was preventing a double-draw that could not happen, and the cost was the title
    // vanishing entirely on exactly the four chart types whose title carries the most meaning.
    //
    // If a specialized renderer ever does start drawing its own title, the fix is for THAT renderer
    // to stop, or for this to test what it actually drew -- not to suppress unconditionally.
    // `every_chart_type_draws_its_title_exactly_once` fails on a double-draw.
    //
    // ⚠️ WHICH FAMILIES TAKE A TITLE IS NOT THIS BACKEND'S DECISION EITHER. Returning
    // `ir.meta.title` outright drew one on families mermaid leaves bare -- the same rule, copied
    // into three backends and wrong in all three. The measured (family, spelling) table now lives in
    // `MermaidDiagramIr::declared_title_if_drawn`; the reasoning above about specialized renderers
    // is what remains local to the terminal.
    ir.declared_title_if_drawn()
}

const PIE_SLICE_CHARS: [char; 8] = ['#', '*', '@', '+', '=', '~', '%', '&'];

/// Cumulative fractions in source order. f64 accumulation keeps finite f32 values from
/// overflowing their total, and tiny positive values retain their actual proportions.
fn pie_slice_boundaries(meta: &fm_core::IrPieMeta) -> Vec<f64> {
    let value = |slice: &fm_core::IrPieSlice| {
        if slice.value.is_finite() && slice.value > 0.0 {
            f64::from(slice.value)
        } else {
            0.0
        }
    };
    let total = meta.slices.iter().map(value).sum::<f64>();
    let mut cumulative = 0.0;
    meta.slices
        .iter()
        .map(|slice| {
            cumulative += value(slice);
            if total > 0.0 { cumulative / total } else { 0.0 }
        })
        .collect()
}

/// Screen coordinates grow downwards: start at noon and advance clockwise through [0, 1).
/// Repeated boundaries are zero-area slices and must never acquire a fallback wedge.
fn pie_slice_at(boundaries: &[f64], dx: f64, dy: f64) -> Option<usize> {
    use std::f64::consts::{FRAC_PI_2, TAU};
    if dx.mul_add(dx, dy * dy) > 1.0 {
        return None;
    }
    let fraction = (dy.atan2(dx) + FRAC_PI_2).rem_euclid(TAU) / TAU;
    let index = boundaries.partition_point(|&end| end <= fraction);
    (index < boundaries.len()).then_some(index)
}

/// Render gantt task bars in the terminal as horizontal block characters.
fn render_gantt_cell(
    buffer: &mut CellBuffer,
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    cell_width: usize,
    cell_height: usize,
    show_labels: bool,
) {
    let Some(gantt_meta) = &ir.gantt_meta else {
        return;
    };

    let label_width = if show_labels {
        gantt_meta
            .sections
            .iter()
            .map(|s| s.name.len())
            .max()
            .unwrap_or(0)
            .min(cell_width / 3)
            .max(8)
    } else {
        0
    };
    let bar_area_width = cell_width.saturating_sub(label_width + 3);
    if bar_area_width < 4 {
        return;
    }

    // Large Gantt charts previously scanned every layout node for every task, making terminal rendering
    // O(tasks * nodes). Build an order-preserving dense index once when it can amortize the allocation.
    // Only fill an empty slot so duplicate node indexes retain `.find()`'s first-match behavior; sparse
    // external layouts keep the allocation-free linear path rather than sizing from an arbitrary index.
    let layout_nodes_by_index = (gantt_meta.tasks.len() >= 16)
        .then(|| {
            layout
                .nodes
                .iter()
                .map(|node| node.node_index)
                .max()
                .map_or(0, |max_index| max_index.saturating_add(1))
        })
        .filter(|lookup_len| *lookup_len <= layout.nodes.len().saturating_mul(2))
        .map(|lookup_len| {
            let mut nodes_by_index = vec![None; lookup_len];
            for node in &layout.nodes {
                if let Some(slot) = nodes_by_index.get_mut(node.node_index)
                    && slot.is_none()
                {
                    *slot = Some(node);
                }
            }
            nodes_by_index
        });

    // Title
    if show_labels && let Some(title) = &gantt_meta.title {
        let tx = cell_width.saturating_sub(title.len()) / 2;
        buffer.set_string(tx, 0, title);
    }

    let schedule_width = layout.bounds.width.max(1.0);
    let mut row = 2_usize;

    for (section_idx, section) in gantt_meta.sections.iter().enumerate() {
        if row >= cell_height {
            break;
        }
        // Section header (truncated by char count for UTF-8 safety).
        if show_labels {
            let header: String = section.name.chars().take(label_width).collect();
            buffer.set_string(0, row, &header);
        }
        // Separator line
        for col in label_width + 1..cell_width {
            buffer.set(col, row, '\u{2500}'); // ─
        }
        row += 1;

        // Tasks belonging to this section (matched by section_idx).
        for task in &gantt_meta.tasks {
            if task.section_idx != section_idx {
                continue;
            }
            if row >= cell_height {
                break;
            }
            let task_name = ir
                .nodes
                .get(task.node.0)
                .and_then(|node| {
                    node.label
                        .and_then(|label_id| ir.labels.get(label_id.0))
                        .map(|label| label.text.as_str())
                        .or(Some(node.id.as_str()))
                })
                .unwrap_or("task");
            if show_labels {
                let task_label: String = task_name.chars().take(label_width).collect();
                buffer.set_string(0, row, &task_label);
            }

            let bar_origin = label_width + 2;
            let node_box = match &layout_nodes_by_index {
                Some(nodes_by_index) => nodes_by_index.get(task.node.0).and_then(|node| *node),
                None => layout
                    .nodes
                    .iter()
                    .find(|node| node.node_index == task.node.0),
            };
            let (bar_start, bar_end) = node_box
                .map(|node_box| {
                    let start_ratio =
                        ((node_box.bounds.x - layout.bounds.x) / schedule_width).clamp(0.0, 1.0);
                    let end_ratio = ((node_box.bounds.x + node_box.bounds.width - layout.bounds.x)
                        / schedule_width)
                        .clamp(start_ratio, 1.0);
                    let start = bar_origin + (start_ratio * bar_area_width as f32).floor() as usize;
                    let end = bar_origin + (end_ratio * bar_area_width as f32).ceil() as usize;
                    (start, end.max(start + 1))
                })
                .unwrap_or((
                    bar_origin,
                    (bar_origin + bar_area_width / 2).max(bar_origin + 1),
                ));
            let bar_char = if matches!(task.flags.primary_type(), GanttTaskType::Critical) {
                '\u{2593}' // ▓
            } else if matches!(task.flags.primary_type(), GanttTaskType::Done) {
                '\u{2591}' // ░
            } else {
                '\u{2588}' // █
            };
            for col in bar_start..bar_end.min(cell_width) {
                buffer.set(col, row, bar_char);
            }
            row += 1;
        }
    }
}

impl TermRenderer {
    /// Render pie sectors in every fidelity, with one shared angular partition for cell fills,
    /// sub-cell hatching, and the legend. Generic graph nodes are only label anchors for a pie.
    fn render_pie(
        &self,
        ir: &MermaidDiagramIr,
        meta: &fm_core::IrPieMeta,
        layout: &DiagramLayout,
        (width, height): (usize, usize),
    ) -> TermRenderResult {
        use std::f64::consts::{FRAC_PI_2, TAU};

        let width = width.min(self.config.cols);
        let height = height.min(self.config.rows);
        let mode = if self.config.tier == MermaidTier::Compact
            || self.config.glyph_mode == fm_core::MermaidGlyphMode::Ascii
        {
            MermaidRenderMode::CellOnly
        } else {
            self.config.render_mode
        };
        let title = self
            .config
            .show_labels
            .then(|| ir.declared_title_if_drawn())
            .flatten();
        let boundaries = pie_slice_boundaries(meta);
        let selected = self
            .config
            .selected_node
            .filter(|&index| self.config.show_selection && index < meta.slices.len());
        let mut previous = 0.0;
        let legends: Vec<_> = meta
            .slices
            .iter()
            .zip(&boundaries)
            .enumerate()
            .map(|(index, (slice, &end))| {
                let percentage = (end - previous) * 100.0;
                previous = end;
                let value = if meta.show_data {
                    format!(" [{}]", slice.value)
                } else {
                    String::new()
                };
                let label = self
                    .truncate_label(&slice.label)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                format!(
                    "{}{} {percentage:.0}%{value} {label}",
                    if selected == Some(index) { ">" } else { "" },
                    PIE_SLICE_CHARS[index % PIE_SLICE_CHARS.len()]
                )
            })
            .collect();
        let padding = self.config.padding.min(width / 2).min(height / 2);
        let top = (padding + usize::from(title.is_some()) * 2).min(height);
        let bottom = height.saturating_sub(padding).max(top);
        let left = padding;
        let mut right = width.saturating_sub(padding);
        let mut chart_bottom = bottom;
        let mut legend = None;
        if self.config.show_labels && !legends.is_empty() {
            if width >= 28 {
                let legend_width = legends
                    .iter()
                    .map(|entry| entry.width())
                    .max()
                    .unwrap_or(0)
                    .min(width / 2);
                right = right.saturating_sub(legend_width + 2);
                legend = Some((right + 2, top, legend_width, bottom - top));
            } else if bottom.saturating_sub(top) >= 8 {
                let legend_rows = legends.len().min((bottom - top) / 2);
                chart_bottom = bottom.saturating_sub(legend_rows + 1);
                legend = Some((left, chart_bottom + 1, right - left, legend_rows));
            }
        }
        let chart_width = right.saturating_sub(left);
        let chart_height = chart_bottom.saturating_sub(top);
        let cx = left as f64 + chart_width as f64 / 2.0;
        let cy = top as f64 + chart_height as f64 / 2.0;
        // A terminal cell is roughly twice as tall as it is wide.
        let rx = (chart_width as f64 / 2.0).min(chart_height as f64);
        let ry = rx / 2.0;
        let mut buffer = CellBuffer::new(width, height);
        let mut painted = vec![false; meta.slices.len()];
        if rx > 0.0 && ry > 0.0 {
            if mode == MermaidRenderMode::CellOnly {
                for row in top..chart_bottom {
                    for col in left..right {
                        let dx = (col as f64 + 0.5 - cx) / rx;
                        let dy = (row as f64 + 0.5 - cy) / ry;
                        if let Some(index) = pie_slice_at(&boundaries, dx, dy) {
                            buffer.set(col, row, PIE_SLICE_CHARS[index % PIE_SLICE_CHARS.len()]);
                            painted[index] = true;
                        }
                    }
                }
            } else {
                let (mx, my) = crate::canvas::subcell_multiplier(mode);
                let mut canvas = Canvas::new(width, height, mode);
                let outline = (1.0 - 1.0 / (rx * mx as f64).min(ry * my as f64).max(1.0)).powi(2);
                for y in top * my..chart_bottom * my {
                    for x in left * mx..right * mx {
                        let dx = ((x as f64 + 0.5) / mx as f64 - cx) / rx;
                        let dy = ((y as f64 + 0.5) / my as f64 - cy) / ry;
                        let Some(index) = pie_slice_at(&boundaries, dx, dy) else {
                            continue;
                        };
                        let hatch = match index % 8 {
                            0 => (x + y).is_multiple_of(4),
                            1 => (x + 4 - y % 4).is_multiple_of(4),
                            2 => x.is_multiple_of(3),
                            3 => y.is_multiple_of(3),
                            4 => (x + y).is_multiple_of(2),
                            5 => x.is_multiple_of(4) && y.is_multiple_of(4),
                            6 => x.is_multiple_of(4) || y.is_multiple_of(4),
                            _ => true,
                        };
                        if hatch || dx.mul_add(dx, dy * dy) >= outline {
                            canvas.set_pixel(x, y);
                            painted[index] = true;
                        }
                    }
                }
                let mut start = 0.0;
                let positive_slices = boundaries
                    .iter()
                    .filter(|&&end| {
                        let positive = end > start;
                        start = end;
                        positive
                    })
                    .count();
                start = 0.0;
                for &end in &boundaries {
                    if end > start && positive_slices > 1 {
                        let angle = start.mul_add(TAU, -FRAC_PI_2);
                        canvas.draw_line(
                            (cx * mx as f64) as isize,
                            (cy * my as f64) as isize,
                            ((cx + rx * angle.cos()) * mx as f64) as isize,
                            ((cy + ry * angle.sin()) * my as f64) as isize,
                        );
                    }
                    start = end;
                }
                buffer.cells = canvas
                    .render_char_grid()
                    .into_iter()
                    .flatten()
                    .map(|ch| if ch == '\u{2800}' { ' ' } else { ch })
                    .collect();
                if self.config.show_labels {
                    let mut occupied = std::collections::BTreeSet::new();
                    start = 0.0;
                    for (index, &end) in boundaries.iter().enumerate() {
                        if end > start {
                            let angle = ((start + end) / 2.0).mul_add(TAU, -FRAC_PI_2);
                            let col = (cx + rx * 0.65 * angle.cos()) as usize;
                            let row = (cy + ry * 0.65 * angle.sin()) as usize;
                            if occupied.insert((col, row)) {
                                buffer.set(
                                    col,
                                    row,
                                    PIE_SLICE_CHARS[index % PIE_SLICE_CHARS.len()],
                                );
                            }
                        }
                        start = end;
                    }
                }
            }
        }
        if let Some(index) = selected {
            let start = index
                .checked_sub(1)
                .map_or(0.0, |previous| boundaries[previous]);
            let end = boundaries[index];
            if end > start && rx > 0.0 && ry > 0.0 {
                let angle = ((start + end) / 2.0).mul_add(TAU, -FRAC_PI_2);
                buffer.set(
                    (cx + rx * 0.65 * angle.cos()) as usize,
                    (cy + ry * 0.65 * angle.sin()) as usize,
                    '>',
                );
            }
        }
        if let Some((column, row, budget, available)) = legend {
            let shown = if legends.len() > available {
                available.saturating_sub(1)
            } else {
                legends.len()
            };
            for (offset, entry) in legends.iter().take(shown).enumerate() {
                buffer.set_string(column, row + offset, &self.pie_text_to_width(entry, budget));
            }
            if shown < legends.len() && available > 0 {
                buffer.set_string(
                    column,
                    row + shown,
                    &self.pie_text_to_width(
                        &format!("+{} more slices", legends.len() - shown),
                        budget,
                    ),
                );
            }
        }
        if let Some(title) = title {
            let title = self.pie_text_to_width(&self.truncate_label(title), width);
            buffer.set_string(width.saturating_sub(title.width()) / 2, 0, &title);
        }
        let mut output = buffer.to_output_string();
        let debug_metrics = self
            .config
            .debug_overlay
            .then(|| TermDebugMetrics::from_layout(ir, layout));
        if let Some(metrics) = debug_metrics {
            overlay_debug_panel(&mut output, width, height, metrics);
        }
        TermRenderResult {
            output,
            width,
            height,
            tier: self.config.tier,
            render_mode: mode,
            node_count: layout.nodes.len(),
            edge_count: layout.edges.len(),
            occluded_node_count: meta
                .slices
                .iter()
                .zip(painted)
                .filter(|(slice, painted)| slice.value.is_finite() && slice.value > 0.0 && !painted)
                .count(),
            occluded_edge_label_count: 0,
            debug_metrics,
        }
    }

    /// Pie captions terminate their row; measure display columns so wide text stays in the
    /// legend's reserved space without displacing chart pixels or exceeding the viewport.
    fn pie_text_to_width(&self, text: &str, budget: usize) -> String {
        use unicode_width::UnicodeWidthChar;
        if budget == 0 {
            return String::new();
        }
        let mut text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if self.config.glyph_mode == fm_core::MermaidGlyphMode::Ascii {
            text = text.replace('…', "~");
        }
        if text.width() <= budget {
            return text;
        }
        let mut used = 0;
        let mut clipped: String = text
            .chars()
            .take_while(|ch| {
                used += ch.width().unwrap_or(0);
                used < budget
            })
            .collect();
        clipped.push(
            if self.config.glyph_mode == fm_core::MermaidGlyphMode::Ascii {
                '~'
            } else {
                '…'
            },
        );
        clipped
    }

    /// Project XY marks onto a terminal raster without changing their common numeric scale.
    fn render_xychart(
        &self,
        ir: &MermaidDiagramIr,
        meta: &fm_core::IrXyChartMeta,
        layout: &DiagramLayout,
        dimensions: (usize, usize, f32, f32),
    ) -> TermRenderResult {
        let (width, height, scale_x, scale_y) = dimensions;
        let width = width.min(self.config.cols);
        let height = height.min(self.config.rows);
        let mode = if self.config.tier == MermaidTier::Compact {
            MermaidRenderMode::CellOnly
        } else {
            self.config.render_mode
        };
        let (mult_x, mult_y) = crate::canvas::subcell_multiplier(mode);
        let mut canvas = Canvas::new(width, height, mode);
        let project = |x: f32, y: f32| {
            (
                (x * scale_x * mult_x as f32) as isize + (self.config.padding * mult_x) as isize,
                (y * scale_y * mult_y as f32) as isize + (self.config.padding * mult_y) as isize,
            )
        };
        let plot = fm_layout::xychart_plot_bounds(meta, layout.bounds);
        let (min, max) = fm_layout::resolve_xychart_y_domain(meta);
        let baseline = min.min(0.0).max(max.min(0.0));
        let baseline_x = fm_layout::xychart_value_to_x(baseline, min, max, plot);
        let baseline_y = fm_layout::xychart_value_to_y(baseline, min, max, plot);
        let (left, top) = project(plot.x, plot.y);
        let (right, bottom) = project(plot.x + plot.width, plot.y + plot.height);
        let (zero_x, zero_y) = project(baseline_x, baseline_y);
        let value_axis = if meta.horizontal {
            ((left, top), (right, top))
        } else {
            ((left, bottom), (right, bottom))
        };
        let zero_axis = if meta.horizontal {
            ((zero_x, top), (zero_x, bottom))
        } else {
            ((left, zero_y), (right, zero_y))
        };
        let axes = [((left, top), (left, bottom)), value_axis, zero_axis];
        for &((x0, y0), (x1, y1)) in &axes {
            canvas.draw_line(x0, y0, x1, y1);
        }
        for value in fm_layout::xychart_y_ticks(min, max) {
            if meta.horizontal {
                let (x, _) = project(fm_layout::xychart_value_to_x(value, min, max, plot), plot.y);
                canvas.draw_line(x, top - 1, x, top);
            } else {
                let (_, y) = project(plot.x, fm_layout::xychart_value_to_y(value, min, max, plot));
                canvas.draw_line(left - 1, y, left, y);
            }
        }

        let by_index: std::collections::BTreeMap<_, _> = layout
            .nodes
            .iter()
            .map(|node| (node.node_index, node))
            .collect();
        let points = |series: &fm_core::IrXySeries| {
            series
                .nodes
                .iter()
                .filter_map(|id| by_index.get(&id.0))
                .map(|node| {
                    let center = node.bounds.center();
                    project(center.x, center.y)
                })
                .collect::<Vec<_>>()
        };

        // Hatching separates an area from its line boundary without introducing ANSI color.
        // Filling along the category axis also preserves both lobes at a zero crossing.
        for series in &meta.series {
            if series.kind == fm_core::IrXySeriesKind::Area {
                fill_xychart_area(
                    &mut canvas,
                    &points(series),
                    if meta.horizontal { zero_x } else { zero_y },
                    meta.horizontal,
                );
            }
        }
        // Outlined bars leave overlapping line series readable on a monochrome surface.
        // Use the layout's exact grouped slots and value endpoints, including negative bars.
        for series in &meta.series {
            if series.kind != fm_core::IrXySeriesKind::Bar {
                continue;
            }
            for id in &series.nodes {
                let Some(node) = by_index.get(&id.0) else {
                    continue;
                };
                let bounds = node.bounds;
                let (x0, y0) = project(bounds.x, bounds.y);
                let (x1, y1) = project(bounds.x + bounds.width, bounds.y + bounds.height);
                let (x0, x1) = (x0.clamp(left, right), x1.clamp(left, right));
                let (y0, y1) = (y0.clamp(top, bottom), y1.clamp(top, bottom));
                canvas.draw_line(x0, y0, x1, y0);
                canvas.draw_line(x0, y1, x1, y1);
                canvas.draw_line(x0, y0, x0, y1);
                canvas.draw_line(x1, y0, x1, y1);
            }
        }
        for series in &meta.series {
            if series.kind == fm_core::IrXySeriesKind::Bar {
                continue;
            }
            let points = points(series);
            for pair in points.windows(2) {
                canvas.draw_line(pair[0].0, pair[0].1, pair[1].0, pair[1].1);
            }
            for (x, y) in points {
                canvas.fill_circle(
                    x,
                    y,
                    if mode == MermaidRenderMode::CellOnly {
                        0
                    } else {
                        1
                    },
                );
            }
        }

        let mut buffer = CellBuffer {
            cells: canvas
                .render_char_grid()
                .into_iter()
                .flatten()
                .map(|ch| {
                    if ch == '\u{2800}' {
                        ' '
                    } else if mode == MermaidRenderMode::CellOnly && ch == '█' {
                        self.box_glyphs.block_full
                    } else {
                        ch
                    }
                })
                .collect(),
            width,
            height,
        };
        if mode == MermaidRenderMode::CellOnly {
            for &((x0, y0), (x1, y1)) in &axes {
                self.draw_line_cell(
                    &mut buffer,
                    x0.max(0) as usize,
                    y0.max(0) as usize,
                    x1.max(0) as usize,
                    y1.max(0) as usize,
                    &self.edge_glyphs,
                    false,
                    ArrowType::Line,
                );
            }
            for series in &meta.series {
                if series.kind != fm_core::IrXySeriesKind::Bar {
                    for (x, y) in points(series) {
                        buffer.set(x.max(0) as usize, y.max(0) as usize, self.box_glyphs.circle);
                    }
                }
            }
        }
        if self.config.show_labels {
            self.overlay_xychart_labels(&mut buffer, ir, meta, layout, (scale_x, scale_y));
        }
        if let Some(node) = self.config.selected_node.and_then(|id| by_index.get(&id))
            && self.config.show_selection
        {
            let center = node.bounds.center();
            let (x, y) = project(center.x, center.y);
            buffer.set(
                x.max(0) as usize / mult_x,
                y.max(0) as usize / mult_y,
                self.box_glyphs.diamond,
            );
        }

        let mut output = buffer.to_output_string();
        let debug_metrics = self
            .config
            .debug_overlay
            .then(|| TermDebugMetrics::from_layout(ir, layout));
        if let Some(metrics) = debug_metrics {
            overlay_debug_panel(&mut output, width, height, metrics);
        }
        let (links_rows, links_width) =
            append_footnote_links(&mut output, ir, self.config.link_mode);
        TermRenderResult {
            output,
            width: width.max(links_width),
            height: height + links_rows,
            tier: self.config.tier,
            render_mode: mode,
            node_count: layout.nodes.len(),
            edge_count: layout.edges.len(),
            occluded_node_count: count_occluded_nodes(
                &layout.nodes,
                scale_x,
                scale_y,
                width,
                height,
            ),
            occluded_edge_label_count: 0,
            debug_metrics,
        }
    }

    fn overlay_xychart_labels(
        &self,
        buffer: &mut CellBuffer,
        ir: &MermaidDiagramIr,
        meta: &fm_core::IrXyChartMeta,
        layout: &DiagramLayout,
        (scale_x, scale_y): (f32, f32),
    ) {
        use fm_layout::TextAlign;
        let plot = fm_layout::xychart_plot_bounds(meta, layout.bounds);
        let to_cell = |x: f32, y: f32| {
            (
                (x * scale_x) as usize + self.config.padding,
                (y * scale_y) as usize + self.config.padding,
            )
        };
        let (left, top) = to_cell(plot.x, plot.y);
        let (right, bottom) = to_cell(plot.x + plot.width, plot.y + plot.height);
        let (min, max) = fm_layout::resolve_xychart_y_domain(meta);
        let ticks = fm_layout::xychart_y_ticks(min, max);
        let mut occupied = Vec::<(usize, usize)>::new();
        // Retain zero and the extreme ticks first when terminal resolution merges nearby ticks.
        for &value in ticks
            .iter()
            .filter(|&&value| value == 0.0)
            .chain(ticks.first())
            .chain(ticks.last())
            .chain(ticks.iter())
        {
            let label = fm_layout::format_xychart_tick_value(value);
            let length = label.chars().count();
            if meta.horizontal {
                let (column, _) =
                    to_cell(fm_layout::xychart_value_to_x(value, min, max, plot), plot.y);
                if top == 0 || length > buffer.width {
                    continue;
                }
                let column = column
                    .saturating_sub(length / 2)
                    .min(buffer.width.saturating_sub(length));
                let end = column + length;
                if occupied
                    .iter()
                    .any(|&(start, stop)| column <= stop + 1 && end + 1 >= start)
                {
                    continue;
                }
                buffer.set_string(column, top - 1, &label);
                occupied.push((column, end));
            } else {
                let (_, row) =
                    to_cell(plot.x, fm_layout::xychart_value_to_y(value, min, max, plot));
                if row >= buffer.height
                    || length + 1 > left
                    || occupied.iter().any(|&(other, _)| row.abs_diff(other) < 2)
                {
                    continue;
                }
                buffer.set_string(left - length - 1, row, &label);
                occupied.push((row, row));
            }
        }

        let categories = fm_layout::xychart_categories(meta);
        let count = categories.len().max(1) as f32;
        let band = if meta.horizontal {
            plot.height
        } else {
            plot.width
        } / count;
        let mut previous_row = None;
        for (index, category) in categories.iter().enumerate() {
            let position = (index as f32 + 0.5) * band;
            if meta.horizontal {
                let (_, row) = to_cell(plot.x, plot.y + position);
                if previous_row == Some(row) {
                    continue;
                }
                self.write_xychart_text(
                    buffer,
                    category,
                    row,
                    (0, left.saturating_sub(1)),
                    TextAlign::End,
                );
                previous_row = Some(row);
            } else {
                let (start, _) = to_cell(plot.x + index as f32 * band, plot.y);
                let (end, _) = to_cell(plot.x + (index + 1) as f32 * band, plot.y);
                self.write_xychart_text(
                    buffer,
                    category,
                    bottom + 1,
                    (start, end.saturating_sub(1)),
                    TextAlign::Middle,
                );
            }
        }
        if let Some(label) = &meta.y_axis.label {
            self.write_xychart_text(
                buffer,
                label,
                top.saturating_sub(if meta.horizontal { 3 } else { 2 }),
                if meta.horizontal {
                    (left, right)
                } else {
                    (0, right)
                },
                if meta.horizontal {
                    TextAlign::Middle
                } else {
                    TextAlign::Start
                },
            );
        }
        if let Some(label) = &meta.x_axis.label {
            self.write_xychart_text(
                buffer,
                label,
                if meta.horizontal {
                    top.saturating_sub(3)
                } else {
                    bottom + 3
                },
                if meta.horizontal {
                    (0, left.saturating_sub(1))
                } else {
                    (left, right)
                },
                if meta.horizontal {
                    TextAlign::End
                } else {
                    TextAlign::Middle
                },
            );
        }
        if let Some(title) = ir.declared_title_if_drawn() {
            self.write_xychart_text(buffer, title, 0, (0, buffer.width), TextAlign::Middle);
        }
    }

    fn write_xychart_text(
        &self,
        buffer: &mut CellBuffer,
        text: &str,
        row: usize,
        (left, right): (usize, usize),
        align: fm_layout::TextAlign,
    ) {
        let right = right.min(buffer.width);
        let budget = right.saturating_sub(left);
        if budget == 0 || row >= buffer.height {
            return;
        }
        let text = self
            .truncate_label(text)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let ellipsis = if self.config.glyph_mode == fm_core::MermaidGlyphMode::Ascii {
            '~'
        } else {
            '…'
        };
        let mut chars: Vec<_> = text
            .chars()
            .map(|ch| if ch == '…' { ellipsis } else { ch })
            .collect();
        if chars.len() > budget {
            chars.truncate(budget);
            chars[budget - 1] = ellipsis;
        }
        let column = match align {
            fm_layout::TextAlign::Start => left,
            fm_layout::TextAlign::Middle => left + budget.saturating_sub(chars.len()) / 2,
            fm_layout::TextAlign::End => right.saturating_sub(chars.len()),
        };
        for (offset, ch) in chars.into_iter().enumerate() {
            buffer.set(column + offset, row, ch);
        }
    }
}

/// Hatch each strip between an area series and its zero baseline. Working along the category
/// axis avoids self-intersecting fill polygons when successive values cross zero.
fn fill_xychart_area(
    canvas: &mut Canvas,
    points: &[(isize, isize)],
    baseline: isize,
    horizontal: bool,
) {
    let (width, height) = canvas.pixel_dimensions();
    let (category_extent, value_extent) = if horizontal {
        (height, width)
    } else {
        (width, height)
    };
    if category_extent == 0 || value_extent == 0 {
        return;
    }
    for pair in points.windows(2) {
        let (a, b) = if horizontal {
            ((pair[0].1, pair[0].0), (pair[1].1, pair[1].0))
        } else {
            (pair[0], pair[1])
        };
        let start = a.0.min(b.0).max(0);
        let end = a.0.max(b.0).min(category_extent as isize - 1);
        for category in start..=end {
            let ratio = if a.0 == b.0 {
                0.5
            } else {
                (category - a.0) as f64 / (b.0 - a.0) as f64
            };
            let value = ((b.1 - a.1) as f64).mul_add(ratio, a.1 as f64).round() as isize;
            let low = value.min(baseline).max(0);
            let high = value.max(baseline).min(value_extent as isize - 1);
            for value in low..=high {
                if (category + value) % 4 == 0 {
                    let (x, y) = if horizontal {
                        (value, category)
                    } else {
                        (category, value)
                    };
                    canvas.set_pixel(x as usize, y as usize);
                }
            }
        }
    }
}

/// Render a quadrant chart with ASCII axes, quadrant labels, and data points.
fn render_quadrant_cell(
    buffer: &mut CellBuffer,
    ir: &MermaidDiagramIr,
    layout: &fm_layout::DiagramLayout,
    (cell_width, cell_height): (usize, usize),
    scale_x: f32,
    scale_y: f32,
    show_labels: bool,
) {
    let Some(quad_meta) = &ir.quadrant_meta else {
        return;
    };

    let margin_left = 2_usize;
    let margin_top = 2_usize;
    let chart_w = cell_width.saturating_sub(margin_left + 2);
    let chart_h = cell_height.saturating_sub(margin_top + 3);

    if chart_w < 6 || chart_h < 6 {
        return;
    }

    let mid_x = margin_left + chart_w / 2;
    let mid_y = margin_top + chart_h / 2;

    // Title.
    if show_labels && let Some(title) = &quad_meta.title {
        let tx = cell_width.saturating_sub(title.len()) / 2;
        buffer.set_string(tx, 0, title);
    }

    // Vertical center axis.
    for row in margin_top..margin_top + chart_h {
        buffer.set(mid_x, row, '\u{2502}'); // │
    }

    // Horizontal center axis.
    for col in margin_left..margin_left + chart_w {
        buffer.set(col, mid_y, '\u{2500}'); // ─
    }

    // Center cross.
    buffer.set(mid_x, mid_y, '\u{253c}'); // ┼

    // Quadrant labels in the four corners.
    let labels = &quad_meta.quadrant_labels;
    if show_labels {
        if let Some(q1) = labels.first() {
            // Q1: top-right
            let x = mid_x + 2;
            let y = margin_top + 1;
            let label: String = q1.chars().take(chart_w / 2 - 3).collect();
            buffer.set_string(x.min(cell_width.saturating_sub(1)), y, &label);
        }
        if let Some(q2) = labels.get(1) {
            // Q2: top-left
            let label: String = q2.chars().take(chart_w / 2 - 3).collect();
            let x = mid_x.saturating_sub(2 + label.len());
            buffer.set_string(x, margin_top + 1, &label);
        }
        if let Some(q3) = labels.get(2) {
            // Q3: bottom-left
            let label: String = q3.chars().take(chart_w / 2 - 3).collect();
            let x = mid_x.saturating_sub(2 + label.len());
            buffer.set_string(x, mid_y + 2, &label);
        }
        if let Some(q4) = labels.get(3) {
            // Q4: bottom-right
            let x = mid_x + 2;
            let label: String = q4.chars().take(chart_w / 2 - 3).collect();
            buffer.set_string(x.min(cell_width.saturating_sub(1)), mid_y + 2, &label);
        }

        // X-axis labels.
        if let Some(left) = &quad_meta.x_axis_left {
            let label: String = left.chars().take(chart_w / 3).collect();
            buffer.set_string(margin_left, margin_top + chart_h + 1, &label);
        }
        if let Some(right) = &quad_meta.x_axis_right {
            let label: String = right.chars().take(chart_w / 3).collect();
            let x = (margin_left + chart_w).saturating_sub(label.len());
            buffer.set_string(x, margin_top + chart_h + 1, &label);
        }
    }
    // Data points: render from layout node positions.
    let point_chars: &[char] = &['\u{25cf}', '\u{25cb}', '\u{25c6}', '\u{25a0}']; // ● ○ ◆ ■
    for (i, node_box) in layout.nodes.iter().enumerate() {
        let center = node_box.bounds.center();
        let x = (center.x * scale_x) as usize;
        let y = (center.y * scale_y) as usize;
        if x < cell_width && y < cell_height {
            buffer.set(x, y, point_chars[i % point_chars.len()]);
        }
    }
}

#[cfg(test)]
mod er_character_column_tests {
    use super::TermRenderer;

    /// `pk` selects the two-character `PK` cell; anything else leaves the key column empty, which
    /// is what `key_cell` returns for `IrAttributeKey::None`.
    fn attr(
        data_type: &str,
        name: &str,
        pk: bool,
        comment: Option<&str>,
    ) -> fm_core::IrEntityAttribute {
        fm_core::IrEntityAttribute {
            data_type: data_type.to_string(),
            name: name.to_string(),
            keys: if pk {
                vec![fm_core::IrAttributeKey::Pk]
            } else {
                Vec::new()
            },
            comment: comment.map(str::to_string),
        }
    }

    /// The widths come from the WIDEST cell in each column across every attribute, which is the
    /// whole reason a skewed entity lays out wider than any single row measures.
    #[test]
    fn columns_take_the_widest_cell_in_each_column() {
        let members = [
            attr("verylongtypename", "a", false, None),
            attr("t", "verylongattributename", true, None),
        ];
        let (offsets, width) = TermRenderer::er_character_columns(&members);
        // type 16, gutter 1, name 21, gutter 1, key 2 = 41 columns.
        assert_eq!(offsets, [0, 17, 39, 42]);
        assert_eq!(width, 41);
    }

    /// An empty column takes no space and no gutter — otherwise every entity without comments
    /// would reserve a comment column and the fits-or-fuse decision would be made on phantom width.
    #[test]
    fn an_empty_column_costs_nothing() {
        let members = [attr("string", "name", false, None)];
        let (offsets, width) = TermRenderer::er_character_columns(&members);
        assert_eq!(offsets[0], 0);
        assert_eq!(
            offsets[1], 7,
            "the name column should follow `string` plus one gutter"
        );
        // No key and no comment, so both later columns sit at the end and add nothing.
        assert_eq!(width, 6 + 1 + 4, "an absent key or comment widened the row");
    }

    /// THE FALLBACK'S ARITHMETIC, which no rendered fixture reaches.
    ///
    /// Narrow glyphs, wide glyphs, long comments and six attributes all FIT the box on this
    /// surface, so the fused branch cannot be exercised through a render — and an untested branch
    /// is exactly what this bead warns against shipping. What the branch keys on is this width
    /// against the box's interior, so the width is what gets tested directly.
    #[test]
    fn a_wide_entity_reports_a_width_that_can_exceed_a_box() {
        let members = [attr(
            &"t".repeat(40),
            &"n".repeat(40),
            true,
            Some(&"c".repeat(40)),
        )];
        let (_offsets, width) = TermRenderer::er_character_columns(&members);
        assert_eq!(width, 40 + 1 + 40 + 1 + 2 + 1 + 40);
        assert!(
            width > 80,
            "a 123-column entity must report more than any ordinary terminal interior"
        );
    }

    /// An entity with no attributes reports no width, so the decision is not made on a stray gutter.
    #[test]
    fn an_entity_with_no_attributes_has_no_width() {
        let (offsets, width) = TermRenderer::er_character_columns(&[]);
        assert_eq!(offsets, [0, 0, 0, 0]);
        assert_eq!(width, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{
        DiagramType, GanttDate, IrEdge, IrEndpoint, IrGanttMeta, IrGanttSection, IrGanttTask,
        IrLabel, IrLabelId, IrNode, IrNodeId,
    };
    use fm_layout::{
        LayoutActivationBar, LayoutClusterBox, LayoutExtensions, LayoutNodeBox, LayoutRect,
        LayoutStats,
    };

    /// A class declaring a stereotype and NO members must still show it here (bd-d48wi).
    ///
    /// The terminal shared the defect with the SVG and canvas backends: all three gated the whole
    /// compartment stack on having at least one member, so a marker interface — idiomatic mermaid,
    /// and gated in the pinned incumbent on `annotations.length > 0` with no member requirement —
    /// printed as a bare box with its annotation silently dropped.
    ///
    /// Asserted on this backend separately because agreement between SVG and canvas cannot speak
    /// for it: the terminal draws into a character grid and clips on its own rules, so a fix that
    /// works in two vector backends can still be invisible in the third.
    #[test]
    fn a_class_with_only_a_stereotype_still_shows_it_in_the_terminal() {
        let source = format!(
            "classDiagram\n  class Shape {{\n    {}interface{}\n  }}\n",
            "<<", ">>"
        );
        let ir = fm_parser::parse(&source).ir;

        // CONTROL ON THE PARSE, or a dropped stereotype and an unparsed one look identical below.
        let meta = ir
            .nodes
            .iter()
            .find_map(|node| node.class_meta.as_deref())
            .expect("CONTROL FAILED: no class metadata parsed");
        assert!(
            meta.stereotype.is_some() && meta.attributes.is_empty() && meta.methods.is_empty(),
            "CONTROL FAILED: fixture is not a memberless class carrying a stereotype"
        );

        let out = crate::render_term(&ir);
        // ⚠️ THE EXPECTED SPELLING MOVED, ON THE INCUMBENT'S AUTHORITY. This asserted the ASCII
        // `<<interface>>` the author types. mermaid 11.15.0 draws the UML guillemets it stands for
        // — `«interface»` — measured in Chromium 151 against the pinned bundle. The assertion's
        // INTENT is unchanged and is the reason it exists: the terminal must not DROP the
        // stereotype. Only the string it is drawn as is corrected, and it is still a literal so
        // this backend keeps pinning the spelling rather than deferring to the mapping under test.
        //
        // Note this is class-diagram-specific: C4 (`<<person>>`) and requirement (`<<Element>>`)
        // keep the ASCII angles upstream, and they build those strings on their own paths.
        assert!(
            out.contains("«interface»"),
            "the terminal dropped the stereotype for a memberless class:\n{out}"
        );
        // The name must survive alongside it, not be replaced by it.
        assert!(
            out.contains("Shape"),
            "the class name was lost when the stereotype was drawn:\n{out}"
        );
    }

    #[test]
    fn generic_class_headers_reach_the_terminal_with_and_without_members() {
        for members in ["", "  A : +go()\n"] {
            let source = format!("classDiagram\n  class A~T~[\"Repository\"]\n{members}");
            let ir = fm_parser::parse(&source).ir;
            let meta = ir.nodes[0]
                .class_meta
                .as_deref()
                .expect("generic class metadata");
            assert_eq!(meta.generics, ["T"]);
            assert_eq!(meta.methods.len(), usize::from(!members.is_empty()));

            let out = crate::render_term(&ir);
            assert!(
                out.contains("Repository<T>"),
                "the terminal lost the declared generic heading:\n{out}"
            );
            if !members.is_empty() {
                assert!(
                    out.contains("+go()"),
                    "the class member was lost when its generic heading was drawn:\n{out}"
                );
            }
        }
    }

    #[test]
    fn pie_cell_raster_preserves_clockwise_quadrants_and_proportions() {
        let config = TermRenderConfig {
            tier: MermaidTier::Compact,
            render_mode: MermaidRenderMode::CellOnly,
            show_labels: false,
            padding: 0,
            max_width: 160,
            max_height: 80,
            ..TermRenderConfig::default()
        };
        let ir = fm_parser::parse("pie\n\"A\":1\n\"B\":1\n\"C\":1\n\"D\":1").ir;
        let result = crate::render_term_with_config(&ir, &config, 160, 80);
        let mut counts = [0_usize; 4];
        let mut centers = [(0_usize, 0_usize); 4];
        for (row, line) in result.output.lines().enumerate() {
            for (column, ch) in line.chars().enumerate() {
                if let Some(index) = ['#', '*', '@', '+'].iter().position(|&mark| mark == ch) {
                    counts[index] += 1;
                    centers[index].0 += column;
                    centers[index].1 += row;
                }
            }
        }
        assert!(counts.iter().all(|&count| count > 100), "{counts:?}");
        assert!(
            counts.iter().max().unwrap() - counts.iter().min().unwrap() <= 2,
            "{counts:?}"
        );
        for (index, &(x, y)) in centers.iter().enumerate() {
            let right = x / counts[index] >= result.width / 2;
            let bottom = y / counts[index] >= result.height / 2;
            assert_eq!(
                (right, bottom),
                [(true, false), (true, true), (false, true), (false, false)][index]
            );
        }

        let ir = fm_parser::parse("pie\n\"A\":1\n\"B\":2\n\"C\":3").ir;
        let result = crate::render_term_with_config(&ir, &config, 160, 80);
        let counts = ['#', '*', '@'].map(|mark| result.output.matches(mark).count());
        let total = counts.iter().sum::<usize>() as f64;
        for (count, fraction) in counts.into_iter().zip([1.0 / 6.0, 2.0 / 6.0, 3.0 / 6.0]) {
            assert!(
                (count as f64 / total - fraction).abs() < 0.02,
                "pie lost its authored ratios"
            );
        }
    }

    #[test]
    fn pie_all_modes_draw_sectors_legend_values_and_one_title() {
        let ir = fm_parser::parse(
            "pie showData title Allocation\n\"Alpha\":2\n\"Beta\":2\n\"Gamma\":2\n\"Delta\":2",
        )
        .ir;
        for tier in [MermaidTier::Compact, MermaidTier::Normal, MermaidTier::Rich] {
            for mode in [
                MermaidRenderMode::CellOnly,
                MermaidRenderMode::Braille,
                MermaidRenderMode::Block,
                MermaidRenderMode::HalfBlock,
            ] {
                let config = TermRenderConfig {
                    tier,
                    render_mode: mode,
                    max_width: 160,
                    max_height: 60,
                    ..TermRenderConfig::rich()
                };
                let result = crate::render_term_with_config(&ir, &config, 160, 60);
                assert_eq!(
                    result.output.matches("Allocation").count(),
                    1,
                    "{tier:?}/{mode:?}"
                );
                for label in ["Alpha", "Beta", "Gamma", "Delta"] {
                    assert!(
                        result.output.contains(label),
                        "{tier:?}/{mode:?}: {label}\n{}",
                        result.output
                    );
                }
                assert_eq!(result.output.matches("25% [2]").count(), 4);
                for mark in ['#', '*', '@', '+'] {
                    assert!(
                        result.output.matches(mark).count() >= 2,
                        "slice {mark} has a legend but no sector: {tier:?}/{mode:?}\n{}",
                        result.output
                    );
                }
                let effective = if tier == MermaidTier::Compact {
                    MermaidRenderMode::CellOnly
                } else {
                    mode
                };
                assert_eq!(result.render_mode, effective);
                if effective == MermaidRenderMode::Braille {
                    assert!(
                        result
                            .output
                            .chars()
                            .any(|ch| ('\u{2801}'..='\u{28ff}').contains(&ch))
                    );
                }
                assert_eq!(result.occluded_node_count, 0);
                assert_eq!(
                    result.output,
                    crate::render_term_with_config(&ir, &config, 160, 60).output
                );
            }
        }
    }

    #[test]
    fn pie_zero_invalid_and_extreme_values_do_not_invent_or_lose_area() {
        let mut ir = fm_parser::parse("pie\n\"A\":1\n\"B\":1\n\"C\":1\n\"D\":1").ir;
        let layout = layout_diagram(&ir);
        let config = TermRenderConfig {
            show_labels: false,
            padding: 0,
            ..TermRenderConfig::compact()
        };
        let renderer = TermRenderer::new(ResolvedConfig::resolve(&config, 80, 24));
        for values in [
            [0.0, 1.0, 0.0, 0.0],
            [f32::NAN, f32::INFINITY, -2.0, 0.0],
            [f32::MAX, f32::MAX, f32::MAX, f32::MAX],
            [1.0e-30, 1.0e-30, 1.0e-30, 1.0e-30],
        ] {
            let meta = ir.pie_meta.as_mut().unwrap();
            for (slice, value) in meta.slices.iter_mut().zip(values) {
                slice.value = value;
            }
            let result = renderer.render_pie(&ir, ir.pie_meta.as_ref().unwrap(), &layout, (80, 24));
            let counts = ['#', '*', '@', '+'].map(|mark| result.output.matches(mark).count());
            if values[0] == 0.0 {
                assert_eq!((counts[0], counts[2], counts[3]), (0, 0, 0));
                assert!(counts[1] > 0);
            } else if values[0].is_nan() {
                assert!(
                    result.output.trim().is_empty(),
                    "invalid values created a positive wedge"
                );
            } else {
                assert!(counts.iter().all(|&count| count > 0));
                assert!(
                    counts.iter().max().unwrap() - counts.iter().min().unwrap() <= 2,
                    "{counts:?}"
                );
            }
        }
    }

    #[test]
    fn pie_caps_unicode_captions_and_empty_charts_to_the_viewport() {
        let ir =
            fm_parser::parse("pie showData title 販売\u{1b}[31m\n\"国内🚀\":20\n\"国外\":30").ir;
        let layout = layout_diagram(&ir);
        for (width, height) in [(0, 0), (1, 1), (8, 4), (19, 10), (40, 12), (100, 40)] {
            let result = crate::render_term_with_layout_and_config(
                &ir,
                &layout,
                &TermRenderConfig::rich(),
                width,
                height,
            );
            assert!(result.width <= width && result.height <= height);
            assert!(result.output.lines().count() <= height);
            assert!(
                result.output.lines().all(|line| line.width() <= width),
                "{width}x{height}: {}",
                result.output
            );
            assert!(!result.output.contains('\u{1b}'));
        }
        let empty = fm_parser::parse("pie title Empty").ir;
        let result = crate::render_term_with_config(&empty, &TermRenderConfig::rich(), 80, 24);
        assert_eq!(result.output.matches("Empty").count(), 1);
        assert_eq!(result.occluded_node_count, 0);

        let mut config = TermRenderConfig::rich();
        config.glyph_mode = fm_core::MermaidGlyphMode::Ascii;
        config.max_label_chars = 8;
        let ir = fm_parser::parse("pie title LongTitleForClipping\n\"LongSliceLabel\":1").ir;
        let result = crate::render_term_with_config(&ir, &config, 40, 12);
        assert!(result.output.is_ascii(), "{}", result.output);
        assert_eq!(result.render_mode, MermaidRenderMode::CellOnly);
    }

    #[test]
    fn pie_selection_remains_visible_in_chart_and_legend() {
        let ir = fm_parser::parse("pie\n\"Alpha\":1\n\"Beta\":1").ir;
        for mode in [MermaidRenderMode::CellOnly, MermaidRenderMode::Braille] {
            for show_labels in [false, true] {
                let config = TermRenderConfig {
                    render_mode: mode,
                    show_labels,
                    ..TermRenderConfig::rich()
                };
                let plain = crate::render_term_with_config(&ir, &config, 100, 40);
                let selected = crate::render_term_with_config(
                    &ir,
                    &TermRenderConfig {
                        selected_node: Some(1),
                        ..config
                    },
                    100,
                    40,
                );
                assert_ne!(plain.output, selected.output);
                assert_eq!(
                    selected.output.matches('>').count(),
                    if show_labels { 2 } else { 1 }
                );
                if show_labels {
                    assert!(selected.output.contains(">* 50% Beta"));
                }
                assert_eq!(
                    (plain.width, plain.height),
                    (selected.width, selected.height)
                );
            }
        }
    }

    fn sample_ir() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;
        ir.labels.push(IrLabel {
            text: "Start".to_string(),
            ..Default::default()
        });
        ir.labels.push(IrLabel {
            text: "End".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            label: Some(IrLabelId(0)),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            label: Some(IrLabelId(1)),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..Default::default()
        });
        ir
    }

    #[test]
    fn renders_simple_diagram() {
        let ir = sample_ir();
        let result = render_diagram(&ir);
        assert_eq!(result.node_count, 2);
        assert_eq!(result.edge_count, 1);
        assert!(!result.output.is_empty());
    }

    #[test]
    fn cell_geometry_uses_the_cga_transform_without_moving_cells() {
        let config = TermRenderConfig {
            padding: 2,
            ..Default::default()
        };
        let renderer = TermRenderer::new(ResolvedConfig::resolve(&config, 80, 24));
        let point = fm_layout::LayoutPoint { x: 2.5, y: 1.5 };
        let bounds = LayoutRect {
            x: point.x,
            y: point.y,
            width: 3.0,
            height: 2.0,
        };

        // Baseline contract: the cell renderer has always mapped points with x * scale_x and
        // y * scale_y, then added its padding. The CGA rotor plus explicit aspect must preserve
        // that exact cell placement for the anisotropic terminal grid.
        assert_eq!(renderer.point_to_cells(&point, 2.0, 4.0), (7, 8));
        assert_eq!(renderer.bounds_to_cells(&bounds, 2.0, 4.0), (7, 8, 6, 8));
    }

    #[test]
    fn cell_mode_places_uml_markers_on_the_semantic_endpoint() {
        let config = TermRenderConfig {
            tier: MermaidTier::Compact,
            render_mode: MermaidRenderMode::CellOnly,
            padding: 0,
            ..Default::default()
        };
        let renderer = TermRenderer::new(ResolvedConfig::resolve(&config, 8, 1));
        let edge_path = LayoutEdgePath {
            edge_index: 0,
            span: Default::default(),
            points: [
                fm_layout::LayoutPoint { x: 1.0, y: 0.0 },
                fm_layout::LayoutPoint { x: 6.0, y: 0.0 },
            ]
            .into_iter()
            .collect(),
            reversed: false,
            is_self_loop: false,
            parallel_offset: 0.0,
            bundle_count: 1,
            bundled: false,
        };

        for arrow in [
            ArrowType::Aggregation,
            ArrowType::Composition,
            ArrowType::Inheritance,
        ] {
            let mut ir = sample_ir();
            ir.edges[0].arrow = arrow;
            let mut buffer = CellBuffer::new(8, 1);
            renderer.render_edge_cell(&mut buffer, &ir, &edge_path, 1.0, 1.0);
            assert_eq!(
                buffer.cells[1], renderer.edge_glyphs.arrow_left,
                "{arrow:?} must mark the source"
            );
            assert_eq!(
                buffer.cells[6], renderer.edge_glyphs.line_h,
                "{arrow:?} must not mark the target"
            );
        }

        for arrow in [
            ArrowType::AggregationReverse,
            ArrowType::CompositionReverse,
            ArrowType::InheritanceReverse,
        ] {
            let mut ir = sample_ir();
            ir.edges[0].arrow = arrow;
            let mut buffer = CellBuffer::new(8, 1);
            renderer.render_edge_cell(&mut buffer, &ir, &edge_path, 1.0, 1.0);
            assert_eq!(
                buffer.cells[1], renderer.edge_glyphs.line_h,
                "{arrow:?} must not mark the source"
            );
            assert_eq!(
                buffer.cells[6], renderer.edge_glyphs.arrow_right,
                "{arrow:?} must mark the target"
            );
        }
    }

    #[test]
    fn compact_mode_produces_smaller_output() {
        let ir = sample_ir();
        let config = TermRenderConfig::compact();
        let compact = render_diagram_with_config(&ir, &config, 80, 24);
        let normal = render_diagram(&ir);
        assert!(compact.width <= normal.width);
    }

    #[test]
    fn output_contains_node_labels() {
        let ir = sample_ir();
        let result = render_diagram(&ir);
        // Should contain the labels or node IDs.
        assert!(result.output.contains("Start") || result.output.contains('A'));
    }

    #[test]
    fn tiny_terminal_dimensions_do_not_underflow() {
        let ir = sample_ir();
        let config = TermRenderConfig::default();
        let result = render_diagram_with_config(&ir, &config, 1, 1);
        assert!(result.width >= 1);
        assert!(result.height >= 1);
    }

    #[test]
    fn zero_max_label_chars_is_clamped_and_safe() {
        let mut ir = sample_ir();
        if let Some(label) = ir.labels.get_mut(0) {
            label.text = "VeryLongLabel".to_string();
        }
        let config = TermRenderConfig {
            max_label_chars: 0,
            max_label_lines: 1,
            ..Default::default()
        };
        let result = render_diagram_with_config(&ir, &config, 80, 24);
        assert!(!result.output.is_empty());
    }

    #[test]
    fn strips_terminal_control_characters_from_labels() {
        let mut ir = sample_ir();
        if let Some(label) = ir.labels.get_mut(0) {
            label.text = "Safe\u{1b}[31mText".to_string();
        }
        let result = render_diagram(&ir);
        assert!(!result.output.contains('\u{1b}'));
    }

    #[test]
    fn short_ascii_label_fast_path_preserves_wrapping_contract() {
        let renderer = TermRenderer::new(ResolvedConfig::resolve(
            &TermRenderConfig::default(),
            120,
            40,
        ));
        assert_eq!(renderer.truncate_label("Node 42"), "Node 42");
        assert_eq!(renderer.truncate_label(""), "");
        assert_eq!(renderer.truncate_label(" leading"), "leading");
        assert_eq!(renderer.truncate_label("trailing "), "trailing");
        assert_eq!(renderer.truncate_label("two  spaces"), "two spaces");
        assert_eq!(renderer.truncate_label("tab\there"), "tab here");
        assert_eq!(renderer.truncate_label("界面 42"), "界面 42");
    }

    #[test]
    fn edge_labels_search_the_whole_frame_and_leave_a_text_gutter() {
        assert_eq!(
            place_edge_label(&[(0, 0, 10, 8)], 5, 0, 4, 1, 10, 12),
            (3, 8),
            "free rows beyond half of the viewport must be considered"
        );
        assert_eq!(
            place_edge_label(&[(3, 0, 7, 12)], 5, 4, 4, 1, 14, 12),
            (8, 4),
            "a full-height obstruction requires a horizontal slide"
        );
        assert_eq!(
            place_edge_label(&[], 79, 23, 22, 2, 80, 24),
            (58, 22),
            "the whole measured plate must remain inside the frame"
        );
        let (x, y) = place_edge_label(&[(0, 0, 4, 1)], 4, 0, 4, 1, 12, 1);
        assert_eq!((x, y), (5, 0));
        assert!(x > 4, "adjacent labels need a separating blank cell");
    }

    fn cyclic_state_ir() -> MermaidDiagramIr {
        fm_parser::parse(concat!(
            "stateDiagram-v2\n",
            "[*] --> Idle\n",
            "Idle --> Starting: Space down / tap\n",
            "Starting --> Idle: closed\n",
            "Starting --> Recording: opened\n",
            "Starting --> Releasing: Space up, focus lost\n",
            "Recording --> Idle: closed\n",
            "Recording --> Stopping: Space up, focus lost / tap\n",
            "Releasing --> Idle: closed\n",
            "Releasing --> Stopping: opened / tap\n",
            "Stopping --> Settling: closed\n",
            "Settling --> Idle: quiet 1 s\n",
            "Settling --> Stopping: opened / tap\n",
        ))
        .ir
    }

    #[test]
    fn state_markers_keep_their_fill_rings_and_scaled_connection_ports() {
        let ir = fm_parser::parse(
            "stateDiagram-v2\nstate Fork <<fork>>\n[*] --> Fork\nFork --> Active\nActive --> [*]\n",
        )
        .ir;
        let layout = layout_diagram(&ir);
        let renderer =
            TermRenderer::new(ResolvedConfig::resolve(&TermRenderConfig::rich(), 100, 40));
        for shape in [
            NodeShape::FilledCircle,
            NodeShape::DoubleCircle,
            NodeShape::HorizontalBar,
        ] {
            let mut node = layout
                .nodes
                .iter()
                .find(|node| ir.nodes[node.node_index].shape == shape)
                .expect("parsed state marker")
                .clone();
            node.bounds.x = 16.0;
            node.bounds.y = 16.0;
            for (scale_x, scale_y) in [(2.0, 0.5), (0.5, 2.0), (1.0, 1.0)] {
                let mut canvas = Canvas::new(100, 40, MermaidRenderMode::Braille);
                renderer.render_node_canvas(&mut canvas, &ir, &node, scale_x, scale_y, 0, 0, 0);
                let center = node.bounds.center();
                let cx = (center.x * scale_x) as usize;
                let cy = (center.y * scale_y) as usize;
                assert!(
                    canvas.get_pixel(cx, cy),
                    "{shape:?} must have a filled centre"
                );
                if shape == NodeShape::HorizontalBar {
                    continue;
                }
                let radius = node.bounds.width.min(node.bounds.height) * 0.5;
                let rx = (radius * scale_x) as usize;
                let ry = (radius * scale_y) as usize;
                for (x, y) in [(cx - rx, cy), (cx + rx, cy), (cx, cy - ry), (cx, cy + ry)] {
                    assert!(
                        canvas.get_pixel(x, y),
                        "{shape:?} lost a connection port under scale ({scale_x}, {scale_y})"
                    );
                }
                assert!(
                    !canvas.get_pixel(cx + rx, cy + ry),
                    "a circular mark must not become a rectangle"
                );
                if shape == NodeShape::DoubleCircle {
                    let inner_x = ((radius - 4.0).max(radius * 0.5) * scale_x) as usize;
                    if rx > inner_x + 1 {
                        assert!(
                            !canvas.get_pixel(cx + inner_x + 1, cy),
                            "the final marker needs a clear annulus"
                        );
                    }
                    let mut flowchart = ir.clone();
                    flowchart.diagram_type = DiagramType::Flowchart;
                    canvas.clear();
                    renderer.render_node_canvas(
                        &mut canvas,
                        &flowchart,
                        &node,
                        scale_x,
                        scale_y,
                        0,
                        0,
                        0,
                    );
                    assert!(
                        !canvas.get_pixel(cx, cy),
                        "a flowchart double circle stays hollow"
                    );
                    assert!(
                        canvas.get_pixel(cx + inner_x, cy),
                        "a flowchart double circle needs its second ring"
                    );
                }
            }
        }
    }

    #[test]
    fn compact_state_markers_are_distinct_and_connected_in_both_glyph_modes() {
        let ir = fm_parser::parse("stateDiagram-v2\n[*] --> Idle\nIdle --> [*]\n").ir;
        let layout = layout_diagram(&ir);
        for (mode, start, end) in [
            (fm_core::MermaidGlyphMode::Unicode, '●', '◉'),
            (fm_core::MermaidGlyphMode::Ascii, '*', '@'),
        ] {
            let config = TermRenderConfig {
                glyph_mode: mode,
                ..TermRenderConfig::compact()
            };
            let renderer = TermRenderer::new(ResolvedConfig::resolve(&config, 80, 24));
            let result = renderer.render_layout(&ir, &layout);
            assert!(result.output.contains("Idle"));
            assert!(!result.output.contains("__state"));
            let grid: Vec<Vec<char>> = result
                .output
                .lines()
                .map(|line| line.chars().collect())
                .collect();
            for glyph in [start, end] {
                assert_eq!(result.output.matches(glyph).count(), 1, "{}", result.output);
                let (y, x) = grid
                    .iter()
                    .enumerate()
                    .find_map(|(y, row)| row.iter().position(|ch| *ch == glyph).map(|x| (y, x)))
                    .unwrap();
                let connected = (y.saturating_sub(1)..=(y + 1).min(grid.len() - 1)).any(|ny| {
                    (x.saturating_sub(1)..=(x + 1).min(grid[ny].len().saturating_sub(1))).any(
                        |nx| (nx, ny) != (x, y) && grid[ny].get(nx).is_some_and(|ch| *ch != ' '),
                    )
                });
                assert!(
                    connected,
                    "state marker {glyph} is detached:\n{}",
                    result.output
                );
            }
            if mode == fm_core::MermaidGlyphMode::Ascii {
                assert!(result.output.is_ascii());
            }
        }
    }

    #[test]
    fn the_reported_cyclic_state_keeps_a_visible_start_transition() {
        let ir = cyclic_state_ir();
        let layout = layout_diagram(&ir);
        let start = layout
            .nodes
            .iter()
            .find(|node| ir.nodes[node.node_index].shape == NodeShape::FilledCircle)
            .unwrap();
        let path = layout
            .edges
            .iter()
            .find(|path| {
                ir.resolve_endpoint_node(ir.edges[path.edge_index].from)
                    == Some(IrNodeId(start.node_index))
            })
            .expect("start transition reaches layout");
        let mut without_entry = layout.clone();
        without_entry
            .edges
            .retain(|edge| edge.edge_index != path.edge_index);
        for mode in [
            MermaidRenderMode::Braille,
            MermaidRenderMode::HalfBlock,
            MermaidRenderMode::Block,
        ] {
            let config = TermRenderConfig {
                render_mode: mode,
                ..TermRenderConfig::default()
            };
            let renderer = TermRenderer::new(ResolvedConfig::resolve(&config, 80, 24));
            let (cols, rows, sx, sy) =
                renderer.fit_cell_dimensions(&layout.bounds, ir.direction, &layout.nodes);
            let (mx, my) = renderer.config.subcell_multiplier();
            let mut marker = Canvas::new(cols, rows, mode);
            renderer.render_node_canvas(
                &mut marker,
                &ir,
                start,
                sx * mx as f32,
                sy * my as f32,
                renderer.config.padding * mx,
                renderer.config.padding * my,
                0,
            );
            let port = path.points[0];
            let px = (port.x * sx * mx as f32) as usize + renderer.config.padding * mx;
            let py = (port.y * sy * my as f32) as usize + renderer.config.padding * my;
            assert!(
                (py.saturating_sub(1)..=py + 1)
                    .any(|y| (px.saturating_sub(1)..=px + 1).any(|x| marker.get_pixel(x, y))),
                "the start transition's port is detached from its {mode:?} marker"
            );
            let result = renderer.render_layout(&ir, &layout);
            let control = renderer.render_layout(&ir, &without_entry);
            let control_grid: Vec<Vec<char>> = control
                .output
                .lines()
                .map(|line| line.chars().collect())
                .collect();
            let (x, y, w, h) = renderer.bounds_to_cells(&start.bounds, sx, sy);
            let visible_route_cells = result
                .output
                .lines()
                .enumerate()
                .map(|(row, drawn)| {
                    drawn
                        .chars()
                        .enumerate()
                        .filter(|(col, ch)| {
                            *ch != control_grid
                                .get(row)
                                .and_then(|line| line.get(*col))
                                .copied()
                                .unwrap_or(' ')
                                && !(row >= y && row <= y + h && *col >= x && *col <= x + w)
                        })
                        .count()
                })
                .sum::<usize>();
            assert!(
                visible_route_cells >= 2,
                "start transition vanished beneath the overlays in {mode:?}:\n{}",
                result.output
            );
            assert!(result.width <= 80 && result.height <= 24);
        }
    }

    #[test]
    fn cyclic_state_labels_remain_separate_and_complete_in_terminal_cells() {
        // GitHub #16/#17: duplicate transition text and shared return corridors used to fuse
        // labels, and a long transition's second row was painted through other routes.
        let ir = cyclic_state_ir();
        assert_eq!(ir.edges.len(), 12);
        let cases = [
            (TermRenderConfig::default(), 80, 24),
            (
                TermRenderConfig {
                    render_mode: MermaidRenderMode::HalfBlock,
                    ..TermRenderConfig::rich()
                },
                110,
                40,
            ),
            (
                TermRenderConfig {
                    glyph_mode: fm_core::MermaidGlyphMode::Ascii,
                    ..TermRenderConfig::rich()
                },
                110,
                40,
            ),
        ];
        for (config, cols, rows) in cases {
            let result = render_diagram_with_config(&ir, &config, cols, rows);
            assert!(result.width <= cols && result.height <= rows);
            assert_eq!(
                result.output,
                render_diagram_with_config(&ir, &config, cols, rows).output,
                "placement must be deterministic"
            );
            for name in [
                "Idle",
                "Starting",
                "Recording",
                "Releasing",
                "Stopping",
                "Settling",
            ] {
                assert!(
                    result.output.contains(name),
                    "lost state {name}:\n{}",
                    result.output
                );
            }
            assert_eq!(
                result.output.matches("closed").count(),
                4,
                "{}",
                result.output
            );
            assert_eq!(
                result.output.matches("opened / tap").count(),
                2,
                "{}",
                result.output
            );
            for text in ["Space down / tap", "Space up, focus lost", "quiet 1 s"] {
                assert!(
                    result.output.contains(text),
                    "lost transition {text}:\n{}",
                    result.output
                );
            }
            let grid: Vec<Vec<char>> = result
                .output
                .lines()
                .map(|row| row.chars().collect())
                .collect();
            for text in ["closed", "opened / tap", "Space down / tap", "quiet 1 s"] {
                let chars: Vec<char> = text.chars().collect();
                for row in &grid {
                    for (x, segment) in row.windows(chars.len()).enumerate() {
                        if segment == chars.as_slice() {
                            assert!(
                                x == 0 || row[x - 1] == ' ',
                                "no left gutter for {text}:\n{}",
                                result.output
                            );
                            assert!(
                                x + chars.len() == row.len() || row[x + chars.len()] == ' ',
                                "no right gutter for {text}:\n{}",
                                result.output
                            );
                        }
                    }
                }
            }
            assert_eq!(result.occluded_edge_label_count, 0, "{}", result.output);
            assert_eq!(
                result.output.matches("Space up, focus lost / tap").count(),
                1,
                "a complete 25-column caption fits on one row at these widths:\n{}",
                result.output,
            );
            assert!(result.output.lines().all(|line| line.width() <= cols));
        }
    }

    #[test]
    fn terminal_label_blocks_are_bounded_and_separated_after_quantization() {
        let ir = cyclic_state_ir();
        let layout = layout_diagram(&ir);
        for cols in [40, 60, 80, 110] {
            for mode in [
                MermaidRenderMode::Braille,
                MermaidRenderMode::HalfBlock,
                MermaidRenderMode::CellOnly,
            ] {
                let config = TermRenderConfig {
                    render_mode: mode,
                    ..Default::default()
                };
                let renderer = TermRenderer::new(ResolvedConfig::resolve(&config, cols, 24));
                let (width, height, sx, sy) =
                    renderer.fit_cell_dimensions(&layout.bounds, ir.direction, &layout.nodes);
                let mut grid = vec![vec![' '; width]; height];
                let (labels, occluded) =
                    renderer.overlay_graph_edge_labels(&mut grid, &ir, &layout, sx, sy, &[]);
                assert_eq!(
                    labels.iter().filter(|label| !label.is_callout).count() + occluded,
                    11
                );
                for (index, label) in labels.iter().enumerate() {
                    assert!(label.x + label.width < width && label.y + label.lines.len() <= height);
                    for other in &labels[..index] {
                        let horizontal =
                            label.x <= other.x + other.width && other.x <= label.x + label.width;
                        let vertical = label.y < other.y + other.lines.len()
                            && other.y < label.y + label.lines.len();
                        assert!(
                            !(horizontal && vertical),
                            "labels merge at {cols} columns in {mode:?}"
                        );
                    }
                    if label.is_callout {
                        let caption = labels
                            .iter()
                            .find(|caption| {
                                !caption.is_callout && caption.edge_index == label.edge_index
                            })
                            .expect("every badge has a visible caption");
                        assert!(caption.lines[0].starts_with(&label.lines[0]));
                        let path = layout
                            .edges
                            .iter()
                            .find(|path| path.edge_index == label.edge_index)
                            .unwrap();
                        let route = renderer.edge_label_route_grid(
                            &ir, &layout, path, false, width, height, sx, sy,
                        );
                        let adjacent = route
                            .iter()
                            .take((label.y + 2).min(height))
                            .skip(label.y.saturating_sub(1))
                            .any(|row| {
                                row.iter()
                                    .take((label.x + label.width + 2).min(width))
                                    .skip(label.x.saturating_sub(2))
                                    .any(|&cell| !blank_terminal_cell(cell))
                            });
                        assert!(
                            adjacent,
                            "badge must touch or sit beside its own rasterized route"
                        );
                    }
                }
                let result = renderer.render_layout(&ir, &layout);
                assert!(
                    result.output.lines().all(|line| line.width() <= cols),
                    "{}",
                    result.output
                );
                assert_eq!(result.output, renderer.render_layout(&ir, &layout).output);
            }
        }
    }

    #[test]
    fn detached_caption_has_a_matching_local_callout_and_never_an_orphan() {
        let text = "the complete transition caption remains readable";
        let ir = fm_parser::parse(&format!("flowchart LR\nA -->|{text}| B")).ir;
        let mut layout = layout_diagram(&ir);
        layout.nodes[0].bounds = fm_layout::LayoutRect {
            x: 0.0,
            y: 3.0,
            width: 20.0,
            height: 4.0,
        };
        layout.nodes[1].bounds = fm_layout::LayoutRect {
            x: 40.0,
            y: 3.0,
            width: 20.0,
            height: 4.0,
        };
        layout.edges[0].points = [
            fm_layout::LayoutPoint { x: 20.0, y: 5.0 },
            fm_layout::LayoutPoint { x: 40.0, y: 5.0 },
        ]
        .into_iter()
        .collect();
        let config = TermRenderConfig {
            tier: MermaidTier::Normal,
            ..Default::default()
        };
        let renderer = TermRenderer::new(ResolvedConfig::resolve(&config, 60, 12));
        let mut grid = vec![vec![' '; 60]; 12];
        let (labels, occluded) =
            renderer.overlay_graph_edge_labels(&mut grid, &ir, &layout, 1.0, 1.0, &[]);
        assert_eq!(occluded, 0);
        let caption = labels
            .iter()
            .find(|label| !label.is_callout)
            .expect("complete caption");
        let badge = labels
            .iter()
            .find(|label| label.is_callout)
            .expect("local route badge");
        assert_eq!(labels.len(), 2);
        assert_eq!(caption.lines, [format!("[1] {text}")]);
        assert_eq!(badge.lines, ["[1]"]);
        assert!(badge.x >= 21 && badge.x + badge.width < 40);
        assert!(
            (4..=6).contains(&badge.y),
            "badge must stay beside the horizontal route"
        );
        let output = serialize_cell_labels(&grid, &labels, false);
        assert_eq!(output.matches("[1]").count(), 2);
        assert_eq!(output.matches(text).count(), 1);

        let mut impossible = vec![vec![' '; 6]; 1];
        let (labels, occluded) =
            renderer.overlay_graph_edge_labels(&mut impossible, &ir, &layout, 1.0, 1.0, &[]);
        assert_eq!(occluded, 1);
        assert!(
            labels.is_empty(),
            "a badge without its caption would misidentify the route"
        );
        let mut too_narrow = vec![vec![' '; 2]; 1];
        let (labels, occluded) =
            renderer.overlay_graph_edge_labels(&mut too_narrow, &ir, &layout, 1.0, 1.0, &[]);
        assert!(labels.is_empty());
        assert_eq!(
            occluded, 1,
            "an impossible grid must report the missing caption"
        );
    }

    #[test]
    fn cell_only_uses_shared_placement_and_keeps_both_cycle_captions() {
        let ir = fm_parser::parse("flowchart LR\nA -->|sent| B\nB -->|acknowledged| A").ir;
        let config = TermRenderConfig {
            render_mode: MermaidRenderMode::CellOnly,
            ..Default::default()
        };
        let result = render_diagram_with_config(&ir, &config, 80, 24);
        assert_eq!(result.occluded_edge_label_count, 0, "{}", result.output);
        for text in ["sent", "acknowledged"] {
            assert_eq!(result.output.matches(text).count(), 1, "{}", result.output);
        }
    }

    #[test]
    fn unicode_edge_labels_use_display_columns_and_keep_fitting_long_words() {
        let text = "界面 e\u{301} ready";
        let mut label = CellLabel::new(0, false, vec![text.to_owned()]);
        label.x = 2;
        assert_eq!(label.width, 12);
        let output = serialize_cell_labels(&[vec![' '; 30]], &[label], false);
        assert_eq!(output.width(), 30);
        assert!(output.contains(text));
        let long = "aLongTransitionIdentifier";
        assert!(long.width() > 24);
        assert_eq!(wrap_cell_label(&edge_caption_text(long, 48), 78, 2), [long]);
        let ir = fm_parser::parse(&format!("flowchart LR\nA -->|{text}| B\nB -->|{long}| A")).ir;
        let rendered = render_diagram(&ir);
        assert_eq!(rendered.occluded_edge_label_count, 0, "{}", rendered.output);
        assert!(rendered.output.contains(text), "{}", rendered.output);
        assert!(rendered.output.contains(long), "{}", rendered.output);
        assert!(rendered.output.lines().all(|line| line.width() <= 80));
        let wrapped = wrap_cell_label("界面 文本 界面 文本", 8, 2);
        assert!(wrapped.iter().all(|line| line.width() <= 8));
    }

    #[test]
    fn graph_node_name_uses_the_interior_row_at_fractional_cell_height() {
        let ir = fm_parser::parse("flowchart TB\nA[Alpha]").ir;
        let mut layout = layout_diagram(&ir);
        layout.nodes[0].bounds = fm_layout::LayoutRect {
            x: 10.0,
            y: 0.0,
            width: 100.0,
            height: 28.0,
        };
        layout.bounds = fm_layout::LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 220.0,
        };
        let result = render_diagram_with_layout_and_config(
            &ir,
            &layout,
            &TermRenderConfig::default(),
            80,
            24,
        );
        let rows: Vec<_> = result.output.lines().collect();
        assert!(rows[2].contains("Alpha"), "{}", result.output);
        assert!(!rows[1].contains("Alpha"), "name erased the top border");
        assert!(!rows[3].contains("Alpha"), "name erased the bottom border");
    }

    #[inline]
    fn owned_compact_label_width_reference(line: &str) -> usize {
        let chars: Vec<char> = line.chars().collect();
        chars.len()
    }

    #[test]
    fn compact_label_width_preserves_unicode_scalar_centering() {
        for line in [
            "",
            "Node 42",
            "界面 → 缓存",
            "e\u{301}quipe",
            "🦀 worker",
            "مرحبا",
            "🏳️‍🌈",
        ] {
            assert_eq!(
                compact_label_width(line),
                owned_compact_label_width_reference(line),
                "Unicode-scalar width changed for {line:?}"
            );
        }
    }

    #[test]
    #[ignore = "release-only same-binary performance probe"]
    fn compact_label_width_streaming_perf_ab() {
        use std::hint::black_box;
        use std::time::Instant;

        const SAMPLE_COUNT: usize = 9;
        const SWEEPS: usize = 16;
        const BOX_WIDTH: usize = 48;
        const BOX_X: usize = 7;

        let patterns = [
            "Node 0042",
            "API gateway",
            "界面 → 缓存",
            "e\u{301}quipe worker",
            "Δ-state_17",
            "🦀 render worker",
            "مرحبا service",
            "request\nresponse",
        ];
        let labels: Vec<String> = (0..4_096)
            .map(|index| format!("{}-{index}", patterns[index % patterns.len()]))
            .collect();

        fn centered_cell_stream(
            labels: &[String],
            width: impl Fn(&str) -> usize,
        ) -> Vec<(usize, usize, usize, char)> {
            let mut cells = Vec::new();
            for (label_index, label) in labels.iter().enumerate() {
                for (line_index, line) in label.lines().enumerate() {
                    let label_x = BOX_X + (BOX_WIDTH.saturating_sub(width(line))) / 2;
                    cells.extend(
                        line.chars()
                            .enumerate()
                            .map(|(offset, ch)| (label_index, line_index, label_x + offset, ch)),
                    );
                }
            }
            cells
        }

        fn measure(labels: &[String], width: impl Fn(&str) -> usize) -> (u128, u64) {
            let started = Instant::now();
            let mut digest = 0xcbf2_9ce4_8422_2325_u64;
            for _ in 0..SWEEPS {
                for label in labels {
                    for line in black_box(label.as_str()).lines() {
                        let label_width = black_box(width(black_box(line)));
                        let label_x = BOX_X + (BOX_WIDTH.saturating_sub(label_width)) / 2;
                        digest ^= label_width as u64;
                        digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
                        digest ^= label_x as u64;
                        digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
                        for (offset, ch) in line.chars().enumerate() {
                            digest ^= ((label_x + offset) as u64).rotate_left(32)
                                ^ u64::from(u32::from(ch));
                            digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
                        }
                    }
                }
            }
            (started.elapsed().as_nanos(), black_box(digest))
        }

        assert_eq!(
            centered_cell_stream(&labels, owned_compact_label_width_reference),
            centered_cell_stream(&labels, compact_label_width),
        );

        let mut baseline_samples = Vec::with_capacity(SAMPLE_COUNT);
        let mut candidate_samples = Vec::with_capacity(SAMPLE_COUNT);
        let mut expected_digest = None;
        for sample in 0..SAMPLE_COUNT {
            let (baseline, candidate) = if sample % 2 == 0 {
                (
                    measure(&labels, owned_compact_label_width_reference),
                    measure(&labels, compact_label_width),
                )
            } else {
                let candidate = measure(&labels, compact_label_width);
                let baseline = measure(&labels, owned_compact_label_width_reference);
                (baseline, candidate)
            };
            assert_eq!(candidate.1, baseline.1);
            assert_eq!(*expected_digest.get_or_insert(baseline.1), baseline.1);
            baseline_samples.push(baseline.0);
            candidate_samples.push(candidate.0);
        }

        baseline_samples.sort_unstable();
        candidate_samples.sort_unstable();
        let baseline_median = baseline_samples[SAMPLE_COUNT / 2];
        let candidate_median = candidate_samples[SAMPLE_COUNT / 2];
        let improvement = (1.0 - candidate_median as f64 / baseline_median as f64) * 100.0;

        eprintln!("baseline_ns={baseline_samples:?}");
        eprintln!("candidate_ns={candidate_samples:?}");
        eprintln!(
            "PERF compact_terminal_label_width baseline_median_ns={baseline_median} candidate_median_ns={candidate_median} improvement_pct={improvement:.3} parity=exact rounds={SAMPLE_COUNT} labels={} sweeps={SWEEPS} digest={:016x}",
            labels.len(),
            expected_digest.expect("at least one performance sample")
        );
    }

    #[test]
    fn cell_buffer_direct_output_matches_display() {
        for (width, height) in [(0, 0), (0, 3), (1, 1), (12, 4)] {
            let mut buffer = CellBuffer::new(width, height);
            if width >= 12 && height >= 4 {
                buffer.set_string(1, 0, "Node A");
                buffer.set_string(1, 1, "界面 → 缓存");
                buffer.set_string(1, 2, "a\tb\u{00a0}\u{2003}");
                buffer.set_string(0, 3, "full-row-1234");
            }
            assert_eq!(
                buffer.to_output_string(),
                buffer.to_string(),
                "direct output changed Display semantics for {width}x{height} buffer"
            );
        }
    }

    #[test]
    #[ignore = "release-only same-binary performance probe"]
    fn cell_buffer_direct_output_perf_ab() {
        use std::hint::black_box;
        use std::time::Instant;

        const SAMPLE_COUNT: usize = 9;
        const ITERATIONS: usize = 512;
        const WIDTH: usize = 160;
        const HEIGHT: usize = 72;

        let mut buffer = CellBuffer::new(WIDTH, HEIGHT);
        for row in 0..HEIGHT {
            match row % 7 {
                0 => buffer.set_string(2, row, "service_0042 --> cache"),
                1 => buffer.set_string(3, row, "界面 → 缓存 🦀"),
                2 => buffer.set_string(4, row, "internal   spaces remain"),
                3 => buffer.set_string(5, row, "a\tb\u{00a0}c\u{2003}"),
                4 => buffer.set_string(1, row, "request\u{2003}\u{00a0}"),
                5 => {}
                _ => {
                    for column in 0..WIDTH {
                        buffer.set(column, row, if column % 17 == 0 { '┼' } else { '─' });
                    }
                }
            }
        }

        let baseline_output = buffer.to_string();
        let candidate_output = buffer.to_output_string();
        assert_eq!(candidate_output, baseline_output);

        fn measure(buffer: &CellBuffer, serializer: impl Fn(&CellBuffer) -> String) -> (u128, u64) {
            let started = Instant::now();
            let mut digest = 0xcbf2_9ce4_8422_2325_u64;
            for iteration in 0..ITERATIONS {
                let output = black_box(serializer(black_box(buffer)));
                let bytes = black_box(output.as_bytes());
                digest ^= bytes.len() as u64;
                if !bytes.is_empty() {
                    digest ^= u64::from(bytes[(iteration * 131) % bytes.len()]);
                }
                digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
                black_box(output);
            }
            (started.elapsed().as_nanos(), black_box(digest))
        }

        let mut baseline_samples = Vec::with_capacity(SAMPLE_COUNT);
        let mut candidate_samples = Vec::with_capacity(SAMPLE_COUNT);
        let mut expected_digest = None;
        for sample in 0..SAMPLE_COUNT {
            let (baseline, candidate) = if sample % 2 == 0 {
                (
                    measure(&buffer, CellBuffer::to_string),
                    measure(&buffer, CellBuffer::to_output_string),
                )
            } else {
                let candidate = measure(&buffer, CellBuffer::to_output_string);
                let baseline = measure(&buffer, CellBuffer::to_string);
                (baseline, candidate)
            };
            assert_eq!(candidate.1, baseline.1);
            assert_eq!(*expected_digest.get_or_insert(baseline.1), baseline.1);
            baseline_samples.push(baseline.0);
            candidate_samples.push(candidate.0);
        }

        baseline_samples.sort_unstable();
        candidate_samples.sort_unstable();
        let baseline_median = baseline_samples[SAMPLE_COUNT / 2];
        let candidate_median = candidate_samples[SAMPLE_COUNT / 2];
        let improvement = (1.0 - candidate_median as f64 / baseline_median as f64) * 100.0;

        eprintln!("baseline_ns={baseline_samples:?}");
        eprintln!("candidate_ns={candidate_samples:?}");
        eprintln!(
            "PERF terminal_cell_buffer_direct_output baseline_median_ns={baseline_median} candidate_median_ns={candidate_median} improvement_pct={improvement:.3} parity=exact rounds={SAMPLE_COUNT} iterations={ITERATIONS} dimensions={WIDTH}x{HEIGHT} digest={:016x}",
            expected_digest.expect("at least one performance sample")
        );
    }

    #[test]
    fn renders_sequence_origin_cluster_titles_in_cell_mode() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        let config = TermRenderConfig {
            tier: MermaidTier::Normal,
            render_mode: MermaidRenderMode::CellOnly,
            ..Default::default()
        };
        let renderer = TermRenderer::new(ResolvedConfig::resolve(&config, 40, 12));
        let mut buffer = CellBuffer::new(40, 12);
        let cluster = LayoutClusterBox {
            cluster_index: 0,
            span: Default::default(),
            title: Some("Ops".to_string()),
            color: None,
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 8.0,
            },
        };

        renderer.render_cluster_cell(&mut buffer, &ir, &cluster, 1.0, 1.0);

        assert!(buffer.to_string().contains("Ops"));
    }

    #[test]
    fn tiny_scaled_activation_bars_still_render() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 1_000.0,
                height: 1_000.0,
            },
            stats: LayoutStats::default(),
            extensions: LayoutExtensions {
                activation_bars: vec![LayoutActivationBar {
                    participant_index: 0,
                    depth: 0,
                    bounds: LayoutRect {
                        x: 100.0,
                        y: 100.0,
                        width: 10.0,
                        height: 10.0,
                    },
                }],
                ..Default::default()
            },
            dirty_regions: Vec::new(),
        };
        let config = TermRenderConfig {
            tier: MermaidTier::Normal,
            render_mode: MermaidRenderMode::Block,
            ..Default::default()
        };

        let result = render_diagram_with_layout_and_config(&ir, &layout, &config, 10, 10);

        assert!(result.output.chars().any(|ch| !ch.is_whitespace()));
    }

    #[test]
    fn renders_sequence_destroy_marker_in_cell_mode() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 20.0,
            },
            stats: LayoutStats::default(),
            extensions: LayoutExtensions {
                sequence_lifecycle_markers: vec![fm_layout::LayoutSequenceLifecycleMarker {
                    participant_index: 0,
                    kind: fm_layout::LayoutSequenceLifecycleMarkerKind::Destroy,
                    center: fm_layout::LayoutPoint { x: 12.0, y: 8.0 },
                    size: 6.0,
                }],
                ..Default::default()
            },
            dirty_regions: Vec::new(),
        };
        let config = TermRenderConfig {
            tier: MermaidTier::Normal,
            render_mode: MermaidRenderMode::CellOnly,
            ..Default::default()
        };

        let result = render_diagram_with_layout_and_config(&ir, &layout, &config, 40, 20);

        assert!(result.output.contains('X'));
    }

    #[test]
    fn renders_sequence_mirror_headers_in_cell_mode() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.labels.push(fm_core::IrLabel {
            text: "Alice".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "Alice".to_string(),
            label: Some(fm_core::IrLabelId(0)),
            ..Default::default()
        });

        let layout = DiagramLayout {
            nodes: vec![LayoutNodeBox {
                node_index: 0,
                node_id: "Alice".to_string(),
                rank: 0,
                order: 0,
                span: Default::default(),
                bounds: LayoutRect {
                    x: 2.0,
                    y: 0.0,
                    width: 12.0,
                    height: 3.0,
                },
            }],
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 12.0,
            },
            stats: LayoutStats::default(),
            extensions: LayoutExtensions {
                sequence_mirror_headers: vec![LayoutNodeBox {
                    node_index: 0,
                    node_id: "Alice".to_string(),
                    rank: 1,
                    order: 0,
                    span: Default::default(),
                    bounds: LayoutRect {
                        x: 2.0,
                        y: 8.0,
                        width: 12.0,
                        height: 3.0,
                    },
                }],
                ..Default::default()
            },
            dirty_regions: Vec::new(),
        };
        let config = TermRenderConfig {
            tier: MermaidTier::Normal,
            render_mode: MermaidRenderMode::CellOnly,
            ..Default::default()
        };

        let result = render_diagram_with_layout_and_config(&ir, &layout, &config, 40, 20);

        assert!(result.output.matches("Alice").count() >= 2);
    }

    #[test]
    fn hide_footbox_suppresses_sequence_mirror_headers_in_cell_mode() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.meta.init.config.sequence_mirror_actors = Some(true);
        ir.sequence_meta = Some(fm_core::IrSequenceMeta {
            hide_footbox: true,
            ..Default::default()
        });
        ir.labels.push(fm_core::IrLabel {
            text: "Alice".to_string(),
            ..Default::default()
        });
        ir.labels.push(fm_core::IrLabel {
            text: "Bob".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "Alice".to_string(),
            label: Some(fm_core::IrLabelId(0)),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "Bob".to_string(),
            label: Some(fm_core::IrLabelId(1)),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..Default::default()
        });

        let config = TermRenderConfig {
            tier: MermaidTier::Normal,
            render_mode: MermaidRenderMode::CellOnly,
            ..Default::default()
        };
        let result = render_diagram_with_config(&ir, &config, 60, 20);

        assert_eq!(result.output.matches("Alice").count(), 1);
        assert_eq!(result.output.matches("Bob").count(), 1);
    }

    #[test]
    fn sequence_autonumber_uses_configured_start_and_increment_in_block_mode() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.sequence_meta = Some(fm_core::IrSequenceMeta {
            autonumber: true,
            autonumber_start: 10,
            autonumber_increment: 5,
            ..Default::default()
        });
        ir.labels.push(fm_core::IrLabel {
            text: "Ping".to_string(),
            ..Default::default()
        });
        ir.labels.push(fm_core::IrLabel {
            text: "Pong".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "Alice".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "Bob".to_string(),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            label: Some(fm_core::IrLabelId(0)),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(1)),
            to: IrEndpoint::Node(IrNodeId(0)),
            arrow: ArrowType::Arrow,
            label: Some(fm_core::IrLabelId(1)),
            ..Default::default()
        });

        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: vec![
                LayoutEdgePath {
                    edge_index: 0,
                    span: Default::default(),
                    points: [
                        fm_layout::LayoutPoint { x: 5.0, y: 6.0 },
                        fm_layout::LayoutPoint { x: 30.0, y: 6.0 },
                    ]
                    .into_iter()
                    .collect(),
                    reversed: false,
                    is_self_loop: false,
                    parallel_offset: 0.0,
                    bundle_count: 1,
                    bundled: false,
                },
                LayoutEdgePath {
                    edge_index: 1,
                    span: Default::default(),
                    points: [
                        fm_layout::LayoutPoint { x: 30.0, y: 12.0 },
                        fm_layout::LayoutPoint { x: 5.0, y: 12.0 },
                    ]
                    .into_iter()
                    .collect(),
                    reversed: false,
                    is_self_loop: false,
                    parallel_offset: 0.0,
                    bundle_count: 1,
                    bundled: false,
                },
            ],
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 18.0,
            },
            stats: LayoutStats::default(),
            extensions: LayoutExtensions::default(),
            dirty_regions: Vec::new(),
        };
        let config = TermRenderConfig {
            tier: MermaidTier::Normal,
            render_mode: MermaidRenderMode::Block,
            ..Default::default()
        };
        let result = render_diagram_with_layout_and_config(&ir, &layout, &config, 80, 24);

        assert!(result.output.contains("10 Ping"), "{}", result.output);
        assert!(result.output.contains("15 Pong"), "{}", result.output);
    }

    #[test]
    fn gantt_cell_mode_uses_task_labels_and_layout_positions() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Gantt);
        ir.labels.push(IrLabel {
            text: "Build UI".to_string(),
            ..Default::default()
        });
        ir.labels.push(IrLabel {
            text: "Verify".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "build_1".to_string(),
            label: Some(IrLabelId(0)),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "verify_1".to_string(),
            label: Some(IrLabelId(1)),
            ..Default::default()
        });
        let mut tasks = vec![
            IrGanttTask {
                node: IrNodeId(0),
                section_idx: 0,
                task_id: Some("build_1".to_string()),
                start: Some(GanttDate::Absolute("2026-02-01".to_string())),
                end: Some(GanttDate::DurationDays(2)),
                flags: fm_core::GanttTaskFlags {
                    done: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            IrGanttTask {
                node: IrNodeId(1),
                section_idx: 0,
                task_id: Some("verify_1".to_string()),
                start: Some(GanttDate::Absolute("2026-02-05".to_string())),
                end: Some(GanttDate::DurationDays(2)),
                ..Default::default()
            },
        ];
        // Exercise the dense node-box lookup used by larger Gantt diagrams.
        for index in 2..16 {
            tasks.push(IrGanttTask {
                node: IrNodeId(index % 2),
                section_idx: 0,
                flags: fm_core::GanttTaskFlags::default(),
                ..Default::default()
            });
        }
        ir.gantt_meta = Some(IrGanttMeta {
            title: Some("Roadmap".to_string()),
            sections: vec![IrGanttSection {
                name: "Alpha".to_string(),
            }],
            tasks,
            ..Default::default()
        });

        let layout = DiagramLayout {
            nodes: vec![
                LayoutNodeBox {
                    node_index: 0,
                    node_id: "build_1".to_string(),
                    rank: 0,
                    order: 0,
                    span: Default::default(),
                    bounds: LayoutRect {
                        x: 0.0,
                        y: 0.0,
                        width: 20.0,
                        height: 6.0,
                    },
                },
                LayoutNodeBox {
                    node_index: 1,
                    node_id: "verify_1".to_string(),
                    rank: 1,
                    order: 1,
                    span: Default::default(),
                    bounds: LayoutRect {
                        x: 60.0,
                        y: 10.0,
                        width: 20.0,
                        height: 6.0,
                    },
                },
                // A duplicate index proves the dense lookup retains the old linear search's first match.
                LayoutNodeBox {
                    node_index: 0,
                    node_id: "duplicate_build".to_string(),
                    rank: 2,
                    order: 2,
                    span: Default::default(),
                    bounds: LayoutRect {
                        x: 90.0,
                        y: 20.0,
                        width: 10.0,
                        height: 6.0,
                    },
                },
            ],
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 24.0,
            },
            stats: LayoutStats::default(),
            extensions: LayoutExtensions::default(),
            dirty_regions: Vec::new(),
        };
        let config = TermRenderConfig {
            tier: MermaidTier::Compact,
            render_mode: MermaidRenderMode::CellOnly,
            ..Default::default()
        };

        let result = render_diagram_with_layout_and_config(&ir, &layout, &config, 60, 12);
        let lines = result.output.lines().collect::<Vec<_>>();
        let build_line = lines
            .iter()
            .find(|line| line.contains("Build UI"))
            .expect("Build UI line");
        let verify_line = lines
            .iter()
            .find(|line| line.contains("Verify"))
            .expect("Verify line");

        assert!(!build_line.contains("build_1"));
        assert!(verify_line.find('█').unwrap_or(0) > build_line.find('░').unwrap_or(0));
    }

    #[test]
    fn renders_generic_diagram_title_in_compact_mode() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.meta.title = Some("Shipping History".to_string());
        // A flowchart draws its title only with front-matter provenance -- mermaid ignores a
        // `title` statement there -- so this is the state the parser actually produces for a
        // titled flowchart. See `MermaidDiagramIr::declared_title_if_drawn`.
        ir.meta.title_from_front_matter = true;
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });

        let config = TermRenderConfig::compact();
        let result = render_diagram_with_config(&ir, &config, 40, 12);
        let first_line = result.output.lines().next().unwrap_or("");

        assert!(first_line.contains("Shipping"));
    }

    #[test]
    fn renders_generic_diagram_title_in_rich_mode() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.meta.title = Some("Shipping History".to_string());
        // A flowchart draws its title only with front-matter provenance -- mermaid ignores a
        // `title` statement there -- so this is the state the parser actually produces for a
        // titled flowchart. See `MermaidDiagramIr::declared_title_if_drawn`.
        ir.meta.title_from_front_matter = true;
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });

        let config = TermRenderConfig::rich();
        let result = render_diagram_with_config(&ir, &config, 40, 12);
        let first_line = result.output.lines().next().unwrap_or("");

        assert!(first_line.contains("Shipping History"));
    }

    #[test]
    fn block_beta_space_nodes_are_hidden_in_compact_term_output() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::BlockBeta);
        ir.nodes.push(IrNode {
            id: "__space_12".to_string(),
            classes: vec!["block-beta".to_string(), "block-beta-space".to_string()],
            ..IrNode::default()
        });

        let config = TermRenderConfig::compact();
        let result = render_diagram_with_config(&ir, &config, 40, 12);
        assert!(!result.output.contains("__space_12"));
        assert!(!result.output.chars().any(|ch| !ch.is_whitespace()));
    }

    #[test]
    fn block_beta_space_nodes_are_hidden_in_rich_term_output() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::BlockBeta);
        ir.nodes.push(IrNode {
            id: "__space_34".to_string(),
            classes: vec!["block-beta".to_string(), "block-beta-space".to_string()],
            ..IrNode::default()
        });

        let config = TermRenderConfig::rich();
        let result = render_diagram_with_config(&ir, &config, 40, 12);
        assert!(!result.output.contains("__space_34"));
        assert!(
            result
                .output
                .chars()
                .all(|ch| ch.is_whitespace() || ch == '⠀')
        );
    }

    #[test]
    fn debug_overlay_appends_stats_banner_when_enabled() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            ..IrNode::default()
        });

        let default_res = render_diagram_with_config(&ir, &TermRenderConfig::default(), 60, 20);
        assert!(!default_res.output.contains("[layout debug]"));
        assert!(default_res.debug_metrics.is_none());

        let debug_cfg = TermRenderConfig {
            debug_overlay: true,
            ..TermRenderConfig::default()
        };
        let debug_res = render_diagram_with_config(&ir, &debug_cfg, 60, 20);
        assert!(debug_res.output.contains("[layout debug]"));
        assert!(debug_res.output.contains("nodes=2"));
        assert!(debug_res.output.contains("edges=0"));
        assert_eq!(debug_res.height, default_res.height);
        assert_eq!(debug_res.width, default_res.width);

        let mut banner = String::new();
        let (rows, _width) = append_term_debug_overlay(&mut banner, &layout_diagram(&ir));
        assert_eq!(rows, 1);
        assert!(banner.contains("[debug: nodes=2 edges=0"));
    }

    #[test]
    fn cell_mode_renders_treemap_radar_and_notes() {
        let compact = TermRenderConfig::compact();

        // 1. Treemap in Compact / CellOnly mode
        let mut treemap_ir = MermaidDiagramIr::empty(DiagramType::Treemap);
        treemap_ir.treemap_meta = Some(fm_core::IrTreemapMeta {
            nodes: vec![
                fm_core::IrTreemapItem {
                    label: "Root".to_string(),
                    value: None,
                    parent: None,
                    children: vec![1],
                    depth: 0,
                    ..Default::default()
                },
                fm_core::IrTreemapItem {
                    label: "LeafA".to_string(),
                    value: Some(40.0),
                    parent: Some(0),
                    children: Vec::new(),
                    depth: 1,
                    ..Default::default()
                },
            ],
            roots: vec![0],
        });
        let treemap_res = render_diagram_with_config(&treemap_ir, &compact, 80, 24);
        assert!(
            treemap_res.output.contains("LeafA"),
            "Compact cell mode must render treemap leaf labels: {}",
            treemap_res.output
        );

        // 2. Radar in Compact / CellOnly mode
        let mut radar_ir = MermaidDiagramIr::empty(DiagramType::Radar);
        radar_ir.radar_meta = Some(fm_core::IrRadarMeta {
            axes: vec![
                fm_core::IrRadarAxis {
                    id: "A".to_string(),
                    label: Some("Speed".to_string()),
                    span: Default::default(),
                },
                fm_core::IrRadarAxis {
                    id: "B".to_string(),
                    label: Some("Power".to_string()),
                    span: Default::default(),
                },
                fm_core::IrRadarAxis {
                    id: "C".to_string(),
                    label: Some("Range".to_string()),
                    span: Default::default(),
                },
            ],
            curves: vec![fm_core::IrRadarCurve {
                id: "c1".to_string(),
                label: Some("Bot".to_string()),
                values: vec![80.0, 60.0, 90.0],
                span: Default::default(),
            }],
            show_legend: true,
            ..Default::default()
        });
        let radar_res = render_diagram_with_config(&radar_ir, &compact, 80, 24);
        assert!(
            radar_res.output.contains("Speed") && radar_res.output.contains("Bot"),
            "Compact cell mode must render radar axis labels and legend: {}",
            radar_res.output
        );
    }

    #[test]
    fn outline_fidelity_and_selection_and_footnote_links_render_end_to_end() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let mut node_a = IrNode {
            id: "Alpha".to_string(),
            ..IrNode::default()
        };
        node_a.interaction_mut().href = Some("https://example.com/alpha".to_string());
        let node_b = IrNode {
            id: "Beta".to_string(),
            ..IrNode::default()
        };
        ir.nodes.push(node_a);
        ir.nodes.push(node_b);
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..Default::default()
        });

        assert_eq!(
            crate::select_fidelity(0.2, 0.2, 0.1, 2, 14, 5, fm_core::MermaidTier::Auto),
            fm_core::MermaidFidelity::Outline
        );

        let selection = crate::SelectionState::from_selected(0, &ir);
        assert_eq!(selection.selected_node, Some(0));
        assert_eq!(selection.highlighted_nodes, vec![1]);

        let adj = crate::build_adjacency(&ir);
        assert_eq!(adj[0], vec![1]);
        let layout = layout_diagram(&ir);
        let next = crate::navigate_direction(&layout, 0, 1.0, 0.0)
            .or_else(|| crate::navigate_direction(&layout, 0, 0.0, 1.0));
        assert_eq!(next, Some(1));

        let mut cfg = TermRenderConfig::compact().with_selection(&selection);
        cfg.link_mode = fm_core::MermaidLinkMode::Footnote;
        let res = render_diagram_with_config(&ir, &cfg, 80, 24);
        assert!(res.output.contains("▶ Alpha [1]"), "{}", res.output);
        assert!(res.output.contains("• Beta"), "{}", res.output);
        assert!(
            res.output.contains("[1] Alpha: https://example.com/alpha"),
            "{}",
            res.output
        );

        let outline_cfg = TermRenderConfig::outline().with_selection(&selection);
        let outline_res = render_diagram_with_config(&ir, &outline_cfg, 80, 24);
        assert!(!outline_res.output.is_empty(), "{}", outline_res.output);
        assert!(
            !outline_res.output.contains("Alpha"),
            "outline cell renderer leaked a label: {}",
            outline_res.output
        );
        assert!(
            !outline_res.output.contains("Beta"),
            "outline cell renderer leaked a label: {}",
            outline_res.output
        );

        // The low-level marker renderer takes fitted cell scales, not layout-pixel scales.
        // Its IDs obey the same label visibility setting as the public renderer.
        let render_skeleton = |config: &TermRenderConfig| {
            let renderer = TermRenderer::new(ResolvedConfig::resolve(config, 80, 24));
            let (cols, rows, scale_x, scale_y) =
                renderer.fit_cell_dimensions(&layout.bounds, ir.direction, &layout.nodes);
            renderer.render_outline_mode(&ir, &layout, cols, rows, scale_x, scale_y)
        };
        let skeleton = render_skeleton(&TermRenderConfig::compact().with_selection(&selection));
        assert!(skeleton.output.contains("▶[Alpha]"), "{}", skeleton.output);
        assert!(skeleton.output.contains("[Beta]"), "{}", skeleton.output);

        let textless = render_skeleton(&outline_cfg);
        assert!(textless.output.contains("▶[]"), "{}", textless.output);
        assert_eq!(
            textless.output.matches("[]").count(),
            2,
            "{}",
            textless.output
        );
        assert!(!textless.output.contains("Alpha"));
        assert!(!textless.output.contains("Beta"));
        assert!(textless.width <= outline_cfg.max_width);
        assert!(textless.height <= outline_cfg.max_height);
    }
}
