//! Core terminal diagram renderer.

use fm_core::{
    ArrowType, GanttTaskType, GraphDirection, MermaidDiagramIr, MermaidRenderMode, MermaidTier,
    NodeShape,
};
use fm_layout::{DiagramLayout, LayoutClusterBox, LayoutEdgePath, LayoutNodeBox, layout_diagram};

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

const fn edge_marker_ends(arrow: ArrowType) -> (bool, bool) {
    match arrow {
        ArrowType::Line | ArrowType::ThickLine | ArrowType::DottedLine => (false, false),
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
        | ArrowType::Inheritance => (true, false),
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
        | ArrowType::InheritanceReverse => (false, true),
    }
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

        // Use cell-based rendering for Compact tier or CellOnly mode.
        if matches!(self.config.tier, MermaidTier::Compact)
            || matches!(self.config.render_mode, MermaidRenderMode::CellOnly)
        {
            return self.render_cell_mode(ir, layout, cell_width, cell_height, scale_x, scale_y);
        }

        // Use sub-cell canvas rendering for higher fidelity.
        self.render_subcell_mode(ir, layout, cell_width, cell_height, scale_x, scale_y)
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

        // Render edges.
        for edge_path in &layout.edges {
            self.render_edge_cell(&mut buffer, ir, edge_path, scale_x, scale_y);
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
        if ir.diagram_type == fm_core::DiagramType::Pie
            && let Some(pie_meta) = &ir.pie_meta
            && !pie_meta.slices.is_empty()
        {
            self::render_pie_cell(&mut buffer, pie_meta, cell_width, cell_height);
        } else if ir.diagram_type == fm_core::DiagramType::Gantt && ir.gantt_meta.is_some() {
            render_gantt_cell(&mut buffer, ir, layout, cell_width, cell_height);
        } else if ir.diagram_type == fm_core::DiagramType::XyChart && ir.xy_chart_meta.is_some() {
            render_xychart_cell(&mut buffer, ir, cell_width, cell_height);
        } else if ir.diagram_type == fm_core::DiagramType::QuadrantChart
            && ir.quadrant_meta.is_some()
        {
            render_quadrant_cell(
                &mut buffer,
                ir,
                layout,
                cell_width,
                cell_height,
                scale_x,
                scale_y,
            );
        } else {
            // Render nodes (foreground).
            for node_box in &layout.nodes {
                self.render_node_cell(&mut buffer, ir, node_box, scale_x, scale_y);
            }
            for node_box in &layout.extensions.sequence_mirror_headers {
                self.render_node_cell(&mut buffer, ir, node_box, scale_x, scale_y);
            }
        }

        self.render_generic_diagram_title(&mut buffer.cells, cell_width, ir);
        let output = buffer.to_output_string();

        TermRenderResult {
            output,
            width: cell_width,
            height: cell_height,
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

        // Render clusters.
        if self.config.show_clusters {
            for cluster_box in &layout.clusters {
                self.render_cluster_canvas(
                    &mut canvas,
                    cluster_box,
                    pixel_scale_x,
                    pixel_scale_y,
                    padding_x,
                    padding_y,
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
            // guard would only drop leaders that are partly on-canvas.
            canvas.draw_line(lx0, ly0, lx1, ly1);
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
            );
        }
        for node_box in &layout.extensions.sequence_mirror_headers {
            self.render_node_canvas(
                &mut canvas,
                ir,
                node_box,
                pixel_scale_x,
                pixel_scale_y,
                padding_x,
                padding_y,
            );
        }

        // Render the canvas straight to its char grid (skipping the String the overlay would re-parse)
        // and overlay labels.
        let base_grid = canvas.render_char_grid();
        let output = self.overlay_labels(
            base_grid,
            ir,
            layout,
            cell_width,
            cell_height,
            scale_x,
            scale_y,
        );

        TermRenderResult {
            output,
            width: cell_width,
            height: cell_height,
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

        let (marker_at_start, marker_at_end) = edge_marker_ends(arrow);

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

        let (x, y, w, h) = self.bounds_to_cells(&node_box.bounds, scale_x, scale_y);
        if w < 3 || h < 1 {
            return;
        }

        // Get node shape.
        let shape = ir_node.map(|n| n.shape).unwrap_or(NodeShape::Rect);

        // Draw shape border.
        self.draw_shape_border(buffer, x, y, w, h, shape);

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

    fn render_cluster_canvas(
        &self,
        canvas: &mut Canvas,
        cluster_box: &LayoutClusterBox,
        scale_x: f32,
        scale_y: f32,
        padding_x: usize,
        padding_y: usize,
    ) {
        let x = (cluster_box.bounds.x * scale_x) as usize + padding_x;
        let y = (cluster_box.bounds.y * scale_y) as usize + padding_y;
        let w = (cluster_box.bounds.width * scale_x) as usize;
        let h = (cluster_box.bounds.height * scale_y) as usize;

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
    ) {
        let ir_node = ir.nodes.get(node_box.node_index);
        if ir_node.is_some_and(is_block_beta_space_node) {
            return;
        }

        let x = (node_box.bounds.x * scale_x) as usize + padding_x;
        let y = (node_box.bounds.y * scale_y) as usize + padding_y;
        let w = (node_box.bounds.width * scale_x) as usize;
        let h = (node_box.bounds.height * scale_y) as usize;

        let shape = ir_node.map(|n| n.shape).unwrap_or(NodeShape::Rect);

        match shape {
            NodeShape::Circle | NodeShape::DoubleCircle => {
                let radius = w.min(h) / 2;
                let cx = x + w / 2;
                let cy = y + h / 2;
                canvas.draw_circle(cx as isize, cy as isize, radius as isize);
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
    ) -> String {
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
            for cluster_box in &layout.clusters {
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

                // ⚠️ NO C4 BOUNDARY TYPE ROW HERE, and that is a MEASURED decision recorded on
                // bd-c23yq rather than an oversight.
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
        for node_box in &layout.nodes {
            let (x, y, w, h) = self.bounds_to_cells(&node_box.bounds, scale_x, scale_y);
            let ir_node = ir.nodes.get(node_box.node_index);

            if ir_node.is_some_and(is_block_beta_space_node) {
                continue;
            }

            // Class diagram nodes with class_meta get three-compartment rendering.
            if let Some(node) = ir_node
                && let Some(ref meta) = node.class_meta
                && (!meta.attributes.is_empty()
                    || !meta.methods.is_empty()
                    || meta.stereotype.is_some())
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
            let start_y = y + (h.saturating_sub(label_lines.len())) / 2;

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
        for edge_path in &layout.edges {
            if edge_path.points.len() < 2 {
                continue;
            }
            if let Some(label_id) = ir.edges.get(edge_path.edge_index).and_then(|e| e.label)
                && let Some(label) = ir.labels.get(label_id.0)
            {
                let base_label = self.truncate_label(&label.text);
                let truncated = if let Some(number) = ir
                    .sequence_meta
                    .as_ref()
                    .and_then(|meta| meta.autonumber_value(edge_path.edge_index))
                {
                    format!("{number} {base_label}")
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

                let start_y = mid_y.saturating_sub(label_lines.len() / 2);

                for (i, line) in label_lines.iter().enumerate() {
                    // Iterate chars directly (see the node-label loops above): `chars().count()` for the
                    // centering width, `chars().enumerate()` for placement — no per-line Vec allocation.
                    let label_len = line.chars().count();
                    let label_x = mid_x.saturating_sub(label_len / 2);
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

        // Single-pass serialization: push every cell char into one pre-sized `String`, with a `'\n'`
        // between rows. The previous `map(collect::<String>).collect::<Vec>().join("\n")` built one
        // `String` per row (cell_height allocations) and then RE-COPIED every byte in `join`. Byte-
        // identical: same chars in row-major order, `'\n'` between rows, no trailing newline. Reserve for
        // the worst case (every cell a 3-byte U+2800.. braille/box glyph) so no push ever reallocates.
        let total_chars: usize = lines.iter().map(Vec::len).sum();
        let mut out = String::with_capacity(total_chars * 3 + lines.len());
        for (row_index, row) in lines.into_iter().enumerate() {
            if row_index > 0 {
                out.push('\n');
            }
            for ch in row {
                out.push(ch);
            }
        }
        out
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

    fn node_display_label(
        &self,
        ir: &MermaidDiagramIr,
        ir_node: Option<&fm_core::IrNode>,
        fallback_id: &str,
    ) -> Option<String> {
        let node = ir_node?;
        if is_block_beta_space_node(node) {
            return None;
        }

        Some(
            node.label
                .and_then(|lid| ir.labels.get(lid.0))
                .map(|label| self.truncate_label(&label.text))
                .unwrap_or_else(|| self.truncate_label(fallback_id)),
        )
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
            let stereo = self.truncate_label(stereo_text);
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
        let name_text = self.truncate_label(class_name);
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
    ir.meta.title.as_deref()
}

/// Render a pie chart as an ASCII ellipse with wedge detection and a side legend.
fn render_pie_cell(
    buffer: &mut CellBuffer,
    pie_meta: &fm_core::IrPieMeta,
    cell_width: usize,
    cell_height: usize,
) {
    use std::f32::consts::PI;

    let slices = &pie_meta.slices;
    let total: f32 = slices
        .iter()
        .map(|s| s.value.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);

    // Reserve space for legend on the right.
    let legend_width = slices
        .iter()
        .map(|s| s.label.len() + 8) // " X Label  "
        .max()
        .unwrap_or(10)
        .min(cell_width / 3);
    let chart_width = cell_width.saturating_sub(legend_width + 2);
    let chart_height = cell_height.saturating_sub(2);

    if chart_width < 4 || chart_height < 4 {
        return;
    }

    // Title
    if let Some(title) = &pie_meta.title {
        let tx = chart_width.saturating_sub(title.len()) / 2;
        buffer.set_string(tx, 0, title);
    }

    let cx = chart_width / 2;
    let cy = chart_height / 2 + 1;
    let rx = (chart_width / 2).saturating_sub(1).max(2);
    let ry = (chart_height / 2).saturating_sub(1).max(2);

    let slice_chars: &[char] = &['#', '*', '@', '+', '=', '~', '%', '&'];

    // Build cumulative angle boundaries.
    let mut boundaries = Vec::with_capacity(slices.len() + 1);
    boundaries.push(-PI / 2.0);
    let mut angle = -PI / 2.0;
    for slice in slices {
        angle += (slice.value.max(0.0) / total) * 2.0 * PI;
        boundaries.push(angle);
    }

    // Render pie ellipse pixel-by-pixel.
    for row in 0..chart_height {
        for col in 0..chart_width {
            let dx = (col as f32 - cx as f32) / rx as f32;
            let dy = (row as f32 - cy as f32) / ry as f32;
            if dx * dx + dy * dy > 1.0 {
                continue;
            }
            let cell_angle = (-dy).atan2(dx);
            // Find which slice this angle belongs to.
            let slice_idx = boundaries
                .windows(2)
                .position(|w| cell_angle >= w[0] && cell_angle < w[1])
                .unwrap_or(0);
            let ch = slice_chars[slice_idx % slice_chars.len()];
            buffer.set(col, row + 1, ch);
        }
    }

    // Render legend on the right side.
    let legend_x = chart_width + 2;
    for (i, slice) in slices.iter().enumerate() {
        let row = i + 2;
        if row >= cell_height {
            break;
        }
        let ch = slice_chars[i % slice_chars.len()];
        let pct = (slice.value.max(0.0) / total) * 100.0;
        let entry = format!("{ch} {:.0}% {}", pct, slice.label);
        // Truncate by character count (not byte count) to avoid UTF-8 boundary panics.
        let truncated: String = entry.chars().take(legend_width).collect();
        buffer.set_string(legend_x, row, &truncated);
    }
}

/// Render gantt task bars in the terminal as horizontal block characters.
fn render_gantt_cell(
    buffer: &mut CellBuffer,
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    cell_width: usize,
    cell_height: usize,
) {
    let Some(gantt_meta) = &ir.gantt_meta else {
        return;
    };

    let label_width = gantt_meta
        .sections
        .iter()
        .map(|s| s.name.len())
        .max()
        .unwrap_or(0)
        .min(cell_width / 3)
        .max(8);
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
    if let Some(title) = &gantt_meta.title {
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
        let header: String = section.name.chars().take(label_width).collect();
        buffer.set_string(0, row, &header);
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
            let task_label: String = task_name.chars().take(label_width).collect();
            buffer.set_string(0, row, &task_label);

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

/// Render an XY chart with ASCII axes, category labels, and data bars.
fn render_xychart_cell(
    buffer: &mut CellBuffer,
    ir: &MermaidDiagramIr,
    cell_width: usize,
    cell_height: usize,
) {
    let Some(xy_meta) = &ir.xy_chart_meta else {
        return;
    };

    let label_margin = 8_usize;
    let chart_left = label_margin + 1;
    let chart_right = cell_width.saturating_sub(2);
    let chart_top = 2_usize;
    let chart_bottom = cell_height.saturating_sub(3);
    let chart_w = chart_right.saturating_sub(chart_left);
    let chart_h = chart_bottom.saturating_sub(chart_top);

    if chart_w < 4 || chart_h < 4 {
        return;
    }

    // Title
    if let Some(title) = &xy_meta.title {
        let tx = cell_width.saturating_sub(title.len()) / 2;
        buffer.set_string(tx, 0, title);
    }

    // Y axis (vertical line).
    for row in chart_top..=chart_bottom {
        buffer.set(chart_left, row, '\u{2502}'); // │
    }

    // X axis (horizontal line).
    for col in chart_left..=chart_right {
        buffer.set(col, chart_bottom, '\u{2500}'); // ─
    }
    buffer.set(chart_left, chart_bottom, '\u{2514}'); // └ corner

    // Category labels along x axis.
    let categories = &xy_meta.x_axis.categories;
    if !categories.is_empty() {
        let cat_spacing = chart_w / categories.len().max(1);
        for (i, cat) in categories.iter().enumerate() {
            let x = chart_left + 1 + i * cat_spacing;
            let label: String = cat.chars().take(cat_spacing.saturating_sub(1)).collect();
            buffer.set_string(x, chart_bottom + 1, &label);
        }
    }

    // Render data series as vertical bar characters.
    let bar_chars: &[char] = &['\u{2588}', '\u{2593}', '\u{2592}', '\u{2591}']; // █ ▓ ▒ ░
    for (series_idx, series) in xy_meta.series.iter().enumerate() {
        let bar_ch = bar_chars[series_idx % bar_chars.len()];
        let max_val = series
            .values
            .iter()
            .copied()
            .fold(0.0_f32, f32::max)
            .max(f32::EPSILON);
        let val_count = series.values.len().max(1);
        let bar_spacing = chart_w / val_count;

        for (i, &val) in series.values.iter().enumerate() {
            let bar_height = ((val / max_val) * chart_h as f32) as usize;
            let x = chart_left + 1 + i * bar_spacing;
            for h in 0..bar_height.min(chart_h) {
                let y = chart_bottom.saturating_sub(1 + h);
                if y >= chart_top && x < chart_right {
                    buffer.set(x, y, bar_ch);
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
    cell_width: usize,
    cell_height: usize,
    scale_x: f32,
    scale_y: f32,
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
    if let Some(title) = &quad_meta.title {
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
        let stereotype = format!("{}interface{}", "<<", ">>");
        assert!(
            out.contains(&stereotype),
            "the terminal dropped the stereotype for a memberless class:\n{out}"
        );
        // The name must survive alongside it, not be replaced by it.
        assert!(
            out.contains("Shape"),
            "the class name was lost when the stereotype was drawn:\n{out}"
        );
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
}
