//! Core terminal diagram renderer.

use fm_core::{
    ArrowType, GraphDirection, MermaidDiagramIr, MermaidRenderMode, MermaidTier, NodeShape,
};
use fm_layout::{DiagramLayout, LayoutClusterBox, LayoutEdgePath, LayoutNodeBox, layout_diagram};

use crate::canvas::Canvas;
use crate::config::{ResolvedConfig, TermRenderConfig};
use crate::glyphs::{BoxGlyphs, ClusterGlyphs, EdgeGlyphs};

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
}

/// Terminal diagram renderer.
pub struct TermRenderer {
    config: ResolvedConfig,
    box_glyphs: BoxGlyphs,
    edge_glyphs: EdgeGlyphs,
    cluster_glyphs: ClusterGlyphs,
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
            self.layout_to_cell_dimensions(&layout.bounds, ir.direction);

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
            render_gantt_cell(&mut buffer, ir, cell_width, cell_height);
        } else if ir.diagram_type == fm_core::DiagramType::XyChart && ir.xy_chart_meta.is_some() {
            render_xychart_cell(&mut buffer, ir, cell_width, cell_height);
        } else {
            // Render nodes (foreground).
            for node_box in &layout.nodes {
                self.render_node_cell(&mut buffer, ir, node_box, scale_x, scale_y);
            }
            for node_box in &layout.extensions.sequence_mirror_headers {
                self.render_node_cell(&mut buffer, ir, node_box, scale_x, scale_y);
            }
        }

        let output = buffer.to_string();

        TermRenderResult {
            output,
            width: cell_width,
            height: cell_height,
            tier: self.config.tier,
            render_mode: self.config.render_mode,
            node_count: layout.nodes.len(),
            edge_count: layout.edges.len(),
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

        // Render canvas to string and overlay labels.
        let base_output = canvas.render();
        let output = self.overlay_labels(
            base_output,
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
        }
    }

    fn layout_to_cell_dimensions(
        &self,
        bounds: &fm_layout::LayoutRect,
        direction: GraphDirection,
    ) -> (usize, usize, f32, f32) {
        let padding_total = self.config.padding * 2;
        let max_width = self.config.cols.saturating_sub(padding_total).max(1);
        let max_height = self.config.rows.saturating_sub(padding_total).max(1);
        let base_scale = match self.config.tier {
            MermaidTier::Compact => 0.15,
            MermaidTier::Normal => 0.2,
            MermaidTier::Rich | MermaidTier::Auto => 0.25,
        };

        let base_width = (bounds.width * base_scale) as usize;
        let base_height = (bounds.height * base_scale) as usize;

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

        // Draw arrowhead at start for double arrows.
        if matches!(
            arrow,
            ArrowType::DoubleArrow | ArrowType::DoubleThickArrow | ArrowType::DoubleDottedArrow
        ) && let Some(first) = edge_path.points.first()
        {
            let (x, y) = self.point_to_cells(first, scale_x, scale_y);
            if edge_path.points.len() >= 2 {
                let next = &edge_path.points[1];
                let (nx, ny) = self.point_to_cells(next, scale_x, scale_y);
                let arrow_char = self.arrowhead_for_direction(nx, ny, x, y, glyphs, arrow);
                buffer.set(x, y, arrow_char);
            }
        }

        // Draw arrowhead at end.
        if let Some(last) = edge_path.points.last() {
            let (x, y) = self.point_to_cells(last, scale_x, scale_y);
            let arrow_char = if edge_path.points.len() >= 2 {
                let prev = &edge_path.points[edge_path.points.len() - 2];
                let (px, py) = self.point_to_cells(prev, scale_x, scale_y);
                self.arrowhead_for_direction(px, py, x, y, glyphs, arrow)
            } else {
                glyphs.arrow_right
            };
            if !matches!(
                arrow,
                ArrowType::Line | ArrowType::ThickLine | ArrowType::DottedLine
            ) {
                buffer.set(x, y, arrow_char);
            }
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
            let label_chars: Vec<char> = line.chars().collect();
            let label_len = label_chars.len();
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
                let inset = (w as f32 * 0.15) as isize;
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
                let inset = (w as f32 * 0.15) as isize;
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
                let inset = (w as f32 * 0.15) as isize;
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
                let inset = (w as f32 * 0.15) as isize;
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
        base: String,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        cell_width: usize,
        cell_height: usize,
        scale_x: f32,
        scale_y: f32,
    ) -> String {
        let mut lines: Vec<Vec<char>> = base.lines().map(|l| l.chars().collect()).collect();

        // Pad lines to consistent width.
        for line in &mut lines {
            while line.len() < cell_width {
                line.push(' ');
            }
        }
        while lines.len() < cell_height {
            lines.push(vec![' '; cell_width]);
        }

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
                && (!meta.attributes.is_empty() || !meta.methods.is_empty())
            {
                self.overlay_class_compartments(&mut lines, x, y, w, h, ir, node, meta, cell_width);
                continue;
            }

            let Some(label) = self.node_display_label(ir, ir_node, &node_box.node_id) else {
                continue;
            };

            let label_lines: Vec<&str> = label.lines().collect();
            let start_y = y + (h.saturating_sub(label_lines.len())) / 2;

            for (i, line) in label_lines.iter().enumerate() {
                let label_chars: Vec<char> = line.chars().collect();
                let label_len = label_chars.len();
                let label_x = x + (w.saturating_sub(label_len)) / 2;
                let label_y = start_y + i;

                if label_y < lines.len() {
                    for (j, ch) in label_chars.into_iter().enumerate() {
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
                let label_chars: Vec<char> = line.chars().collect();
                let label_len = label_chars.len();
                let label_x = x + (w.saturating_sub(label_len)) / 2;
                let label_y = start_y + i;

                if label_y < lines.len() {
                    for (j, ch) in label_chars.into_iter().enumerate() {
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
                    let px = (p1.x + p2.x) / 2.0;
                    let py = (p1.y + p2.y) / 2.0;
                    self.point_to_cells(&fm_layout::LayoutPoint { x: px, y: py }, scale_x, scale_y)
                } else if edge_path.points.len() == 2 {
                    let p1 = &edge_path.points[0];
                    let p2 = &edge_path.points[1];
                    let px = (p1.x + p2.x) / 2.0;
                    let py = (p1.y + p2.y) / 2.0;
                    self.point_to_cells(&fm_layout::LayoutPoint { x: px, y: py }, scale_x, scale_y)
                } else {
                    let mid_idx = edge_path.points.len() / 2;
                    self.point_to_cells(&edge_path.points[mid_idx], scale_x, scale_y)
                };

                let start_y = mid_y.saturating_sub(label_lines.len() / 2);

                for (i, line) in label_lines.iter().enumerate() {
                    let label_chars: Vec<char> = line.chars().collect();
                    let label_len = label_chars.len();
                    let label_x = mid_x.saturating_sub(label_len / 2);
                    let label_y = start_y + i;

                    if label_y < lines.len() {
                        for (j, ch) in label_chars.into_iter().enumerate() {
                            let col = label_x + j;
                            if col < cell_width && col < lines[label_y].len() {
                                lines[label_y][col] = ch;
                            }
                        }
                    }
                }
            }
        }

        lines
            .into_iter()
            .map(|l| l.into_iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn bounds_to_cells(
        &self,
        bounds: &fm_layout::LayoutRect,
        scale_x: f32,
        scale_y: f32,
    ) -> (usize, usize, usize, usize) {
        let x = (bounds.x * scale_x) as usize + self.config.padding;
        let y = (bounds.y * scale_y) as usize + self.config.padding;
        let w = ((bounds.width * scale_x) as usize).max(3);
        let h = ((bounds.height * scale_y) as usize).max(2);

        (x, y, w, h)
    }

    fn point_to_cells(
        &self,
        point: &fm_layout::LayoutPoint,
        scale_x: f32,
        scale_y: f32,
    ) -> (usize, usize) {
        let x = (point.x * scale_x) as usize + self.config.padding;
        let y = (point.y * scale_y) as usize + self.config.padding;

        (x, y)
    }

    fn truncate_label(&self, text: &str) -> String {
        let max_chars = self.config.max_label_chars.max(1);
        let max_lines = self.config.max_label_lines.max(1);
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
            let vis = visibility_char(attr.visibility);
            let text = format!("{vis}{}", attr.name);
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
            let vis = visibility_char(method.visibility);
            let suffix = if method.is_abstract {
                "*"
            } else if method.is_static {
                "$"
            } else {
                ""
            };
            let ret = method
                .return_type
                .as_deref()
                .map(|t| format!(": {t}"))
                .unwrap_or_default();
            let text = format!("{vis}{}{suffix}{ret}", method.name);
            write_text(grid, row, x + 1, &text, inner_w);
            row += 1;
        }
    }
}

/// Map ClassVisibility to its UML symbol.
fn visibility_char(vis: fm_core::ClassVisibility) -> char {
    match vis {
        fm_core::ClassVisibility::Public => '+',
        fm_core::ClassVisibility::Private => '-',
        fm_core::ClassVisibility::Protected => '#',
        fm_core::ClassVisibility::Package => '~',
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

    // Title
    if let Some(title) = &gantt_meta.title {
        let tx = cell_width.saturating_sub(title.len()) / 2;
        buffer.set_string(tx, 0, title);
    }

    let task_count = gantt_meta.tasks.len().max(1);
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
        for (task_idx, task) in gantt_meta.tasks.iter().enumerate() {
            if task.section_idx != section_idx {
                continue;
            }
            if row >= cell_height {
                break;
            }
            // Task label from the associated node.
            let task_name = ir
                .nodes
                .get(task.node.0)
                .map(|n| n.id.as_str())
                .unwrap_or("task");
            let task_label: String = task_name.chars().take(label_width).collect();
            buffer.set_string(0, row, &task_label);

            // Task bar — position proportionally in the bar area.
            let bar_start = label_width + 2 + (task_idx * bar_area_width) / task_count;
            let bar_end = label_width + 2 + ((task_idx + 1) * bar_area_width) / task_count;
            let bar_char = if task.critical {
                '\u{2593}' // ▓
            } else if task.done {
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

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{DiagramType, IrEdge, IrEndpoint, IrLabel, IrLabelId, IrNode, IrNodeId};
    use fm_layout::{
        LayoutActivationBar, LayoutClusterBox, LayoutExtensions, LayoutNodeBox, LayoutRect,
        LayoutStats,
    };

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
                    points: vec![
                        fm_layout::LayoutPoint { x: 5.0, y: 6.0 },
                        fm_layout::LayoutPoint { x: 30.0, y: 6.0 },
                    ],
                    reversed: false,
                    is_self_loop: false,
                    parallel_offset: 0.0,
                    bundle_count: 1,
                    bundled: false,
                },
                LayoutEdgePath {
                    edge_index: 1,
                    span: Default::default(),
                    points: vec![
                        fm_layout::LayoutPoint { x: 30.0, y: 12.0 },
                        fm_layout::LayoutPoint { x: 5.0, y: 12.0 },
                    ],
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
