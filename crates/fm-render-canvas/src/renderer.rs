//! Canvas2D diagram renderer.
//!
//! Draws diagrams to Canvas2D contexts using computed layouts.

use crate::context::{Canvas2dContext, TextAlign, TextBaseline};
use crate::shapes::{draw_arrowhead, draw_circle_marker, draw_cross_marker, draw_shape};
use crate::viewport::{Viewport, fit_to_viewport};
use fm_core::{ArrowType, MermaidDiagramIr, NodeShape};
use fm_layout::DiagramLayout;

/// Configuration for Canvas2D rendering.
#[derive(Debug, Clone)]
pub struct CanvasRenderConfig {
    /// Font family for labels.
    pub font_family: String,
    /// Font size in pixels.
    pub font_size: f64,
    /// Padding around the diagram.
    pub padding: f64,
    /// Node fill color.
    pub node_fill: String,
    /// Node stroke color.
    pub node_stroke: String,
    /// Node stroke width.
    pub node_stroke_width: f64,
    /// Edge stroke color.
    pub edge_stroke: String,
    /// Edge stroke width.
    pub edge_stroke_width: f64,
    /// Cluster background color.
    pub cluster_fill: String,
    /// Cluster stroke color.
    pub cluster_stroke: String,
    /// Label text color.
    pub label_color: String,
    /// Whether to auto-fit the diagram to the canvas.
    pub auto_fit: bool,
}

impl Default for CanvasRenderConfig {
    fn default() -> Self {
        Self {
            font_family: String::from("system-ui, -apple-system, sans-serif"),
            font_size: 14.0,
            padding: 20.0,
            node_fill: String::from("#ffffff"),
            node_stroke: String::from("#333333"),
            node_stroke_width: 1.5,
            edge_stroke: String::from("#333333"),
            edge_stroke_width: 1.5,
            cluster_fill: String::from("#f8f9fa"),
            cluster_stroke: String::from("#dee2e6"),
            label_color: String::from("#333333"),
            auto_fit: true,
        }
    }
}

/// Result of a canvas render operation.
#[derive(Debug, Clone)]
pub struct CanvasRenderResult {
    /// Total number of draw calls made.
    pub draw_calls: usize,
    /// Number of nodes drawn.
    pub nodes_drawn: usize,
    /// Number of edges drawn.
    pub edges_drawn: usize,
    /// Number of clusters drawn.
    pub clusters_drawn: usize,
    /// Number of labels drawn.
    pub labels_drawn: usize,
    /// The viewport used for rendering.
    pub viewport: Viewport,
}

/// Canvas2D diagram renderer.
#[derive(Debug, Clone)]
pub struct Canvas2dRenderer {
    config: CanvasRenderConfig,
    draw_calls: usize,
}

impl Canvas2dRenderer {
    /// Create a new renderer with the given configuration.
    #[must_use]
    pub fn new(config: CanvasRenderConfig) -> Self {
        Self {
            config,
            draw_calls: 0,
        }
    }

    /// Render a diagram layout to a Canvas2D context.
    pub fn render<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
    ) -> CanvasRenderResult {
        self.draw_calls = 0;

        let canvas_width = ctx.width();
        let canvas_height = ctx.height();

        // Compute viewport to fit diagram
        let viewport = if self.config.auto_fit {
            fit_to_viewport(
                f64::from(layout.bounds.width),
                f64::from(layout.bounds.height),
                canvas_width,
                canvas_height,
                self.config.padding,
            )
        } else {
            Viewport::new(canvas_width, canvas_height)
        };

        // Clear canvas
        ctx.clear_rect(0.0, 0.0, canvas_width, canvas_height);
        self.draw_calls += 1;

        // Apply viewport transform
        ctx.save();
        let transform = viewport.transform();
        ctx.set_transform(
            transform.a,
            transform.b,
            transform.c,
            transform.d,
            transform.e,
            transform.f,
        );

        // Offset for diagram bounds (convert f32 layout coords to f64).
        //
        // When `auto_fit` is enabled we already account for `padding` in the viewport
        // (screen space). Adding `padding` again here (diagram space) causes the diagram
        // to be mis-centered and margins to become asymmetric, especially when zoom != 1.
        let (offset_x, offset_y) = if self.config.auto_fit {
            (-f64::from(layout.bounds.x), -f64::from(layout.bounds.y))
        } else {
            (
                self.config.padding - f64::from(layout.bounds.x),
                self.config.padding - f64::from(layout.bounds.y),
            )
        };

        // Draw clusters (background)
        let clusters_drawn = self.draw_clusters(layout, ir, ctx, offset_x, offset_y);

        // Draw edges
        let edges_drawn = self.draw_edges(layout, ir, ctx, offset_x, offset_y);

        // Draw nodes
        let nodes_drawn = self.draw_nodes(layout, ir, ctx, offset_x, offset_y);

        ctx.restore();

        CanvasRenderResult {
            draw_calls: self.draw_calls,
            nodes_drawn,
            edges_drawn,
            clusters_drawn,
            labels_drawn: nodes_drawn + edges_drawn, // Each node/edge may have a label
            viewport,
        }
    }

    /// Draw all cluster backgrounds.
    fn draw_clusters<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) -> usize {
        let mut count = 0;

        for cluster_box in &layout.clusters {
            let x = f64::from(cluster_box.bounds.x) + offset_x;
            let y = f64::from(cluster_box.bounds.y) + offset_y;
            let w = f64::from(cluster_box.bounds.width);
            let h = f64::from(cluster_box.bounds.height);

            // Draw cluster background
            ctx.set_fill_style(&self.config.cluster_fill);
            ctx.set_stroke_style(&self.config.cluster_stroke);
            ctx.set_line_width(1.0);

            ctx.begin_path();
            // Rounded rectangle for cluster
            let r = 4.0;
            ctx.move_to(x + r, y);
            ctx.line_to(x + w - r, y);
            ctx.arc_to(x + w, y, x + w, y + r, r);
            ctx.line_to(x + w, y + h - r);
            ctx.arc_to(x + w, y + h, x + w - r, y + h, r);
            ctx.line_to(x + r, y + h);
            ctx.arc_to(x, y + h, x, y + h - r, r);
            ctx.line_to(x, y + r);
            ctx.arc_to(x, y, x + r, y, r);
            ctx.close_path();
            ctx.fill();
            ctx.stroke();
            self.draw_calls += 2;

            // Draw cluster label if present
            if let Some(ir_cluster) = ir.clusters.get(cluster_box.cluster_index)
                && let Some(title_id) = ir_cluster.title
                && let Some(label) = ir.labels.get(title_id.0)
            {
                ctx.set_fill_style("#6c757d");
                ctx.set_font(&format!(
                    "{}px {}",
                    self.config.font_size * 0.9,
                    self.config.font_family
                ));
                ctx.set_text_align(TextAlign::Left);
                ctx.set_text_baseline(TextBaseline::Top);
                ctx.fill_text(&label.text, x + 8.0, y + 4.0);
                self.draw_calls += 1;
            }

            count += 1;
        }

        count
    }

    /// Draw all edges.
    fn draw_edges<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) -> usize {
        let mut count = 0;

        for edge_path in layout.edges.iter() {
            let ir_edge = ir.edges.get(edge_path.edge_index);
            let arrow = ir_edge.map_or(ArrowType::Arrow, |e| e.arrow);

            if edge_path.points.len() < 2 {
                continue;
            }

            // Set edge style
            let (stroke_width, dash_pattern) = match arrow {
                ArrowType::ThickArrow => (2.5, None),
                ArrowType::DottedArrow => (1.5, Some(vec![5.0, 5.0])),
                _ => (self.config.edge_stroke_width, None),
            };

            ctx.set_stroke_style(&self.config.edge_stroke);
            ctx.set_line_width(stroke_width);
            if let Some(pattern) = dash_pattern {
                ctx.set_line_dash(&pattern);
            } else {
                ctx.set_line_dash(&[]);
            }

            // Draw edge path
            ctx.begin_path();
            let first = &edge_path.points[0];
            ctx.move_to(f64::from(first.x) + offset_x, f64::from(first.y) + offset_y);

            for point in edge_path.points.iter().skip(1) {
                ctx.line_to(f64::from(point.x) + offset_x, f64::from(point.y) + offset_y);
            }
            ctx.stroke();
            self.draw_calls += 1;

            // Draw arrowhead at end
            if edge_path.points.len() >= 2 {
                let end = &edge_path.points[edge_path.points.len() - 1];
                let prev = &edge_path.points[edge_path.points.len() - 2];
                let angle = f64::from(end.y - prev.y).atan2(f64::from(end.x - prev.x));

                let ex = f64::from(end.x) + offset_x;
                let ey = f64::from(end.y) + offset_y;

                match arrow {
                    ArrowType::Line => {}
                    ArrowType::Arrow | ArrowType::ThickArrow | ArrowType::DottedArrow => {
                        draw_arrowhead(ctx, ex, ey, angle, 10.0, &self.config.edge_stroke);
                        self.draw_calls += 1;
                    }
                    ArrowType::Circle => {
                        draw_circle_marker(ctx, ex, ey, 4.0, "#fff", &self.config.edge_stroke);
                        self.draw_calls += 1;
                    }
                    ArrowType::Cross => {
                        draw_cross_marker(ctx, ex, ey, 8.0, &self.config.edge_stroke);
                        self.draw_calls += 1;
                    }
                }
            }

            // Draw edge label if present
            if let Some(label_id) = ir_edge.and_then(|e| e.label)
                && let Some(label) = ir.labels.get(label_id.0)
                && edge_path.points.len() >= 2
            {
                let mid_idx = edge_path.points.len() / 2;
                let mid = &edge_path.points[mid_idx];
                let lx = f64::from(mid.x) + offset_x;
                let ly = f64::from(mid.y) + offset_y - 12.0;

                // Background for label
                let text_metrics = ctx.measure_text(&label.text);
                let label_width = text_metrics.width + 8.0;
                let label_height = self.config.font_size + 4.0;

                ctx.set_fill_style("#ffffff");
                ctx.fill_rect(
                    lx - label_width / 2.0,
                    ly - label_height / 2.0,
                    label_width,
                    label_height,
                );
                self.draw_calls += 1;

                // Label text
                ctx.set_fill_style("#666666");
                ctx.set_font(&format!(
                    "{}px {}",
                    self.config.font_size * 0.85,
                    self.config.font_family
                ));
                ctx.set_text_align(TextAlign::Center);
                ctx.set_text_baseline(TextBaseline::Middle);
                ctx.fill_text(&label.text, lx, ly);
                self.draw_calls += 1;
            }

            // Reset dash pattern
            ctx.set_line_dash(&[]);
            count += 1;
        }

        count
    }

    /// Draw all nodes.
    fn draw_nodes<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) -> usize {
        let mut count = 0;

        for node_box in layout.nodes.iter() {
            let ir_node = ir.nodes.get(node_box.node_index);
            let shape = ir_node.map_or(NodeShape::Rect, |n| n.shape);

            let x = f64::from(node_box.bounds.x) + offset_x;
            let y = f64::from(node_box.bounds.y) + offset_y;
            let w = f64::from(node_box.bounds.width);
            let h = f64::from(node_box.bounds.height);

            // Draw shape
            draw_shape(
                ctx,
                shape,
                x,
                y,
                w,
                h,
                &self.config.node_fill,
                &self.config.node_stroke,
                self.config.node_stroke_width,
            );
            self.draw_calls += 1;

            // Get label text
            let label_text = ir_node
                .and_then(|n| n.label)
                .and_then(|lid| ir.labels.get(lid.0))
                .map(|l| l.text.as_str())
                .or_else(|| ir_node.map(|n| n.id.as_str()))
                .unwrap_or("");

            if !label_text.is_empty() {
                let cx = x + w / 2.0;
                let cy = y + h / 2.0;

                ctx.set_fill_style(&self.config.label_color);
                ctx.set_font(&format!(
                    "{}px {}",
                    self.config.font_size, self.config.font_family
                ));
                ctx.set_text_align(TextAlign::Center);
                ctx.set_text_baseline(TextBaseline::Middle);
                ctx.fill_text(label_text, cx, cy);
                self.draw_calls += 1;
            }

            count += 1;
        }

        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{DrawOperation, MockCanvas2dContext};
    use fm_core::DiagramType;
    use fm_layout::layout_diagram;

    #[test]
    fn renderer_handles_empty_diagram() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let layout = layout_diagram(&ir);
        let config = CanvasRenderConfig::default();
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut renderer = Canvas2dRenderer::new(config);

        let result = renderer.render(&layout, &ir, &mut ctx);
        assert_eq!(result.nodes_drawn, 0);
        assert_eq!(result.edges_drawn, 0);
    }

    #[test]
    fn render_result_tracks_draw_calls() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let layout = layout_diagram(&ir);
        let config = CanvasRenderConfig::default();
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut renderer = Canvas2dRenderer::new(config);

        let result = renderer.render(&layout, &ir, &mut ctx);
        // At minimum: clear_rect
        assert!(result.draw_calls >= 1);
    }

    #[test]
    fn default_config_has_sensible_values() {
        let config = CanvasRenderConfig::default();
        assert!(!config.font_family.is_empty());
        assert!(config.font_size > 0.0);
        assert!(config.padding > 0.0);
    }

    #[test]
    fn auto_fit_does_not_apply_padding_in_diagram_space() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(fm_core::IrNode {
            id: "A".to_string(),
            ..Default::default()
        });
        let layout = layout_diagram(&ir);

        let config = CanvasRenderConfig {
            auto_fit: true,
            padding: 20.0,
            ..Default::default()
        };

        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut renderer = Canvas2dRenderer::new(config);
        let _result = renderer.render(&layout, &ir, &mut ctx);

        let node_box = layout
            .nodes
            .iter()
            .find(|node| node.node_index == 0)
            .expect("expected node 0 to be present in layout");

        let (rect_x, rect_y) = ctx
            .operations()
            .iter()
            .find_map(|op| match op {
                DrawOperation::Rect(x, y, _w, _h) => Some((*x, *y)),
                _ => None,
            })
            .expect("expected a Rect operation for node box");

        let expected_x = f64::from(node_box.bounds.x - layout.bounds.x);
        let expected_y = f64::from(node_box.bounds.y - layout.bounds.y);
        assert!((rect_x - expected_x).abs() < 0.001);
        assert!((rect_y - expected_y).abs() < 0.001);
    }

    #[test]
    fn non_auto_fit_applies_padding_in_diagram_space() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(fm_core::IrNode {
            id: "A".to_string(),
            ..Default::default()
        });
        let layout = layout_diagram(&ir);

        let config = CanvasRenderConfig {
            auto_fit: false,
            padding: 20.0,
            ..Default::default()
        };

        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut renderer = Canvas2dRenderer::new(config.clone());
        let _result = renderer.render(&layout, &ir, &mut ctx);

        let node_box = layout
            .nodes
            .iter()
            .find(|node| node.node_index == 0)
            .expect("expected node 0 to be present in layout");

        let (rect_x, rect_y) = ctx
            .operations()
            .iter()
            .find_map(|op| match op {
                DrawOperation::Rect(x, y, _w, _h) => Some((*x, *y)),
                _ => None,
            })
            .expect("expected a Rect operation for node box");

        let expected_x = f64::from(node_box.bounds.x - layout.bounds.x) + config.padding;
        let expected_y = f64::from(node_box.bounds.y - layout.bounds.y) + config.padding;
        assert!((rect_x - expected_x).abs() < 0.001);
        assert!((rect_y - expected_y).abs() < 0.001);
    }
}
