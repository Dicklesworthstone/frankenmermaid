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
        // Use cell-based rendering for Compact tier or CellOnly mode.
        if matches!(self.config.tier, MermaidTier::Compact)
            || matches!(self.config.render_mode, MermaidRenderMode::CellOnly)
        {
            return self.render_cell_mode(ir, layout);
        }

        // Use sub-cell canvas rendering for higher fidelity.
        self.render_subcell_mode(ir, layout)
    }

    /// Render using character cells (Compact mode).
    fn render_cell_mode(&self, ir: &MermaidDiagramIr, layout: &DiagramLayout) -> TermRenderResult {
        // Calculate cell grid dimensions from layout bounds.
        let (cell_width, cell_height) =
            self.layout_to_cell_dimensions(&layout.bounds, ir.direction);

        // Create character buffer.
        let mut buffer = CellBuffer::new(cell_width, cell_height);

        // Render clusters first (background).
        if self.config.show_clusters {
            for cluster_box in &layout.clusters {
                self.render_cluster_cell(&mut buffer, ir, cluster_box);
            }
        }

        // Render edges.
        for edge_path in &layout.edges {
            self.render_edge_cell(&mut buffer, ir, edge_path);
        }

        // Render nodes (foreground).
        for node_box in &layout.nodes {
            self.render_node_cell(&mut buffer, ir, node_box);
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
    ) -> TermRenderResult {
        let (mult_x, mult_y) = self.config.subcell_multiplier();

        // Calculate cell grid dimensions and create canvas.
        let (cell_width, cell_height) =
            self.layout_to_cell_dimensions(&layout.bounds, ir.direction);
        let mut canvas = Canvas::new(cell_width, cell_height, self.config.render_mode);

        // Scale factor from layout coordinates to pixels.
        let scale_x = (cell_width * mult_x) as f32 / layout.bounds.width.max(1.0);
        let scale_y = (cell_height * mult_y) as f32 / layout.bounds.height.max(1.0);

        // Render clusters.
        if self.config.show_clusters {
            for cluster_box in &layout.clusters {
                self.render_cluster_canvas(&mut canvas, cluster_box, scale_x, scale_y);
            }
        }

        // Render edges.
        for edge_path in &layout.edges {
            self.render_edge_canvas(&mut canvas, edge_path, scale_x, scale_y);
        }

        // Render nodes.
        for node_box in &layout.nodes {
            self.render_node_canvas(&mut canvas, ir, node_box, scale_x, scale_y);
        }

        // Render canvas to string and overlay labels.
        let base_output = canvas.render();
        let output = self.overlay_labels(base_output, ir, layout, cell_width, cell_height);

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
    ) -> (usize, usize) {
        let padding = self.config.padding * 2;
        let max_width = self.config.cols.saturating_sub(padding).max(1);
        let max_height = self.config.rows.saturating_sub(padding).max(1);
        let scale = match self.config.tier {
            MermaidTier::Compact => 0.15,
            MermaidTier::Normal => 0.2,
            MermaidTier::Rich | MermaidTier::Auto => 0.25,
        };

        let base_width = (bounds.width * scale) as usize;
        let base_height = (bounds.height * scale) as usize;

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

        (
            width.saturating_add(padding),
            height.saturating_add(padding),
        )
    }

    fn render_cluster_cell(
        &self,
        buffer: &mut CellBuffer,
        ir: &MermaidDiagramIr,
        cluster_box: &LayoutClusterBox,
    ) {
        let (x, y, w, h) = self.bounds_to_cells(&cluster_box.bounds);
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
        if let Some(cluster) = ir.clusters.get(cluster_box.cluster_index)
            && let Some(label_id) = cluster.title
            && let Some(label) = ir.labels.get(label_id.0)
        {
            let title = self.truncate_label(&label.text);
            let title_x = x + 2;
            buffer.set_string(title_x, y, &title);
        }
    }

    fn render_edge_cell(
        &self,
        buffer: &mut CellBuffer,
        ir: &MermaidDiagramIr,
        edge_path: &LayoutEdgePath,
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
            let (x0, y0) = self.point_to_cells(&window[0]);
            let (x1, y1) = self.point_to_cells(&window[1]);
            self.draw_line_cell(buffer, x0, y0, x1, y1, glyphs, edge_path.reversed, arrow);
        }

        // Draw arrowhead at end.
        if let Some(last) = edge_path.points.last() {
            let (x, y) = self.point_to_cells(last);
            let arrow_char = if edge_path.points.len() >= 2 {
                let prev = &edge_path.points[edge_path.points.len() - 2];
                let (px, py) = self.point_to_cells(prev);
                self.arrowhead_for_direction(px, py, x, y, glyphs, arrow)
            } else {
                glyphs.arrow_right
            };
            if !matches!(arrow, ArrowType::Line) {
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
        let line_char = if reversed || matches!(arrow, ArrowType::DottedArrow) {
            if x0 == x1 {
                glyphs.dotted_v
            } else {
                glyphs.dotted_h
            }
        } else if x0 == x1 {
            glyphs.line_v
        } else if y0 == y1 {
            glyphs.line_h
        } else if (x1 > x0) == (y1 > y0) {
            glyphs.line_diag_nw
        } else {
            glyphs.line_diag_ne
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
            ArrowType::Cross => glyphs.cross_head,
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
    ) {
        let (x, y, w, h) = self.bounds_to_cells(&node_box.bounds);
        if w < 3 || h < 1 {
            return;
        }

        // Get node shape.
        let shape = ir
            .nodes
            .get(node_box.node_index)
            .map(|n| n.shape)
            .unwrap_or(NodeShape::Rect);

        // Draw shape border.
        self.draw_shape_border(buffer, x, y, w, h, shape);

        // Get label.
        let label = ir
            .nodes
            .get(node_box.node_index)
            .and_then(|n| n.label)
            .and_then(|lid| ir.labels.get(lid.0))
            .map(|l| self.truncate_label(&l.text))
            .unwrap_or_else(|| self.truncate_label(&node_box.node_id));

        // Center label in node.
        let lines: Vec<&str> = label.lines().collect();
        let start_y = y + (h.saturating_sub(lines.len())) / 2;

        for (i, line) in lines.iter().enumerate() {
            let label_len = line.chars().count();
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
                // Diamond shape: /\ on top, \/ on bottom
                let mid_x = x + w / 2;
                let mid_y = y + h / 2;
                buffer.set(mid_x, y, '/');
                buffer.set(mid_x + 1, y, '\\');
                buffer.set(x, mid_y, '<');
                buffer.set(x + w - 1, mid_y, '>');
                buffer.set(mid_x, y + h - 1, '\\');
                buffer.set(mid_x + 1, y + h - 1, '/');
            }
            NodeShape::Circle | NodeShape::DoubleCircle => {
                // Circle approximation.
                let mid_y = y + h / 2;
                buffer.set(x, mid_y, '(');
                buffer.set(x + w - 1, mid_y, ')');
                for dx in 1..w - 1 {
                    buffer.set(x + dx, y, glyphs.horizontal);
                    buffer.set(x + dx, y + h - 1, glyphs.horizontal);
                }
            }
            NodeShape::Rounded | NodeShape::Stadium => {
                // Rounded rectangle.
                buffer.set(x, y, '(');
                buffer.set(x + w - 1, y, ')');
                buffer.set(x, y + h - 1, '(');
                buffer.set(x + w - 1, y + h - 1, ')');
                for dx in 1..w - 1 {
                    buffer.set(x + dx, y, glyphs.horizontal);
                    buffer.set(x + dx, y + h - 1, glyphs.horizontal);
                }
                for dy in 1..h - 1 {
                    buffer.set(x, y + dy, glyphs.vertical);
                    buffer.set(x + w - 1, y + dy, glyphs.vertical);
                }
            }
            NodeShape::Hexagon => {
                // Hexagon.
                buffer.set(x, y + h / 2, '<');
                buffer.set(x + w - 1, y + h / 2, '>');
                for dx in 1..w - 1 {
                    buffer.set(x + dx, y, glyphs.horizontal);
                    buffer.set(x + dx, y + h - 1, glyphs.horizontal);
                }
            }
            _ => {
                // Standard rectangle.
                buffer.set(x, y, glyphs.top_left);
                buffer.set(x + w - 1, y, glyphs.top_right);
                buffer.set(x, y + h - 1, glyphs.bottom_left);
                buffer.set(x + w - 1, y + h - 1, glyphs.bottom_right);
                for dx in 1..w - 1 {
                    buffer.set(x + dx, y, glyphs.horizontal);
                    buffer.set(x + dx, y + h - 1, glyphs.horizontal);
                }
                for dy in 1..h - 1 {
                    buffer.set(x, y + dy, glyphs.vertical);
                    buffer.set(x + w - 1, y + dy, glyphs.vertical);
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
    ) {
        let x = (cluster_box.bounds.x * scale_x) as usize;
        let y = (cluster_box.bounds.y * scale_y) as usize;
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
    ) {
        for window in edge_path.points.windows(2) {
            let x0 = (window[0].x * scale_x) as isize;
            let y0 = (window[0].y * scale_y) as isize;
            let x1 = (window[1].x * scale_x) as isize;
            let y1 = (window[1].y * scale_y) as isize;
            canvas.draw_line(x0, y0, x1, y1);
        }
    }

    fn render_node_canvas(
        &self,
        canvas: &mut Canvas,
        ir: &MermaidDiagramIr,
        node_box: &LayoutNodeBox,
        scale_x: f32,
        scale_y: f32,
    ) {
        let x = (node_box.bounds.x * scale_x) as usize;
        let y = (node_box.bounds.y * scale_y) as usize;
        let w = (node_box.bounds.width * scale_x) as usize;
        let h = (node_box.bounds.height * scale_y) as usize;

        let shape = ir
            .nodes
            .get(node_box.node_index)
            .map(|n| n.shape)
            .unwrap_or(NodeShape::Rect);

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
            _ => {
                canvas.draw_rect(x, y, w.max(1), h.max(1));
            }
        }
    }

    fn overlay_labels(
        &self,
        base: String,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        cell_width: usize,
        cell_height: usize,
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
            let (x, y, w, h) = self.bounds_to_cells(&node_box.bounds);

            let label = ir
                .nodes
                .get(node_box.node_index)
                .and_then(|n| n.label)
                .and_then(|lid| ir.labels.get(lid.0))
                .map(|l| self.truncate_label(&l.text))
                .unwrap_or_else(|| self.truncate_label(&node_box.node_id));

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
                let truncated = self.truncate_label(&label.text);
                let label_lines: Vec<&str> = truncated.lines().collect();

                let (mid_x, mid_y) = if edge_path.points.len() == 4 {
                    let p1 = &edge_path.points[1];
                    let p2 = &edge_path.points[2];
                    let px = (p1.x + p2.x) / 2.0;
                    let py = (p1.y + p2.y) / 2.0;
                    self.point_to_cells(&fm_layout::LayoutPoint { x: px, y: py })
                } else {
                    let mid_idx = edge_path.points.len() / 2;
                    self.point_to_cells(&edge_path.points[mid_idx])
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

    fn bounds_to_cells(&self, bounds: &fm_layout::LayoutRect) -> (usize, usize, usize, usize) {
        let scale = match self.config.tier {
            MermaidTier::Compact => 0.15,
            MermaidTier::Normal => 0.2,
            MermaidTier::Rich | MermaidTier::Auto => 0.25,
        };

        let x = (bounds.x * scale) as usize + self.config.padding;
        let y = (bounds.y * scale) as usize + self.config.padding;
        let w = ((bounds.width * scale) as usize).max(3);
        let h = ((bounds.height * scale) as usize).max(2);

        (x, y, w, h)
    }

    fn point_to_cells(&self, point: &fm_layout::LayoutPoint) -> (usize, usize) {
        let scale = match self.config.tier {
            MermaidTier::Compact => 0.15,
            MermaidTier::Normal => 0.2,
            MermaidTier::Rich | MermaidTier::Auto => 0.25,
        };

        let x = (point.x * scale) as usize + self.config.padding;
        let y = (point.y * scale) as usize + self.config.padding;

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

        for line in source_lines.into_iter().take(max_lines) {
            let chars: Vec<char> = line.chars().collect();
            if chars.len() <= max_chars {
                lines.push(line.to_string());
            } else if max_chars == 1 {
                lines.push("…".to_string());
            } else {
                lines.push(format!(
                    "{}…",
                    chars[..max_chars - 1].iter().collect::<String>()
                ));
            }
        }

        lines.join("\n")
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{DiagramType, IrEdge, IrEndpoint, IrLabel, IrLabelId, IrNode, IrNodeId};

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
}
