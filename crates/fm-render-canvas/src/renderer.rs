//! Canvas2D diagram renderer.
//!
//! Draws diagrams to Canvas2D contexts using computed layouts.

use crate::context::{Canvas2dContext, LineCap, LineJoin, TextAlign, TextBaseline};
use crate::shapes::{
    draw_arrowhead, draw_circle_marker, draw_cross_marker, draw_diamond_marker,
    draw_open_triangle_marker, draw_shape,
};
use crate::viewport::{Viewport, fit_to_viewport};
use fm_core::{ArrowType, DiagramType, MermaidDiagramIr, NodeShape};
use fm_layout::{
    DiagramLayout, FillStyle, LineCap as IrLineCap, LineJoin as IrLineJoin, MarkerKind, PathCmd,
    RenderClip, RenderGroup, RenderItem, RenderPath, RenderScene, RenderSource, RenderText,
    RenderTransform, StrokeStyle, TextAlign as IrTextAlign, TextBaseline as IrTextBaseline,
};
use std::collections::BTreeSet;

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

impl CanvasRenderConfig {
    /// Get the font metrics based on this configuration.
    #[must_use]
    pub fn font_metrics(&self) -> fm_core::FontMetrics {
        fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
            preset: fm_core::FontPreset::from_family(&self.font_family),
            font_size: self.font_size as f32,
            line_height: 1.4, // Matches CanvasRenderConfig default implicitly
            fallback_chain: vec![
                fm_core::FontPreset::SansSerif,
                fm_core::FontPreset::Monospace,
            ],
            trace_fallbacks: false,
        })
    }
}

impl Default for CanvasRenderConfig {
    fn default() -> Self {
        Self {
            font_family: String::from(
                "'Inter', 'Avenir Next', 'Segoe UI', 'Helvetica Neue', Arial, sans-serif",
            ),
            font_size: 14.0,
            padding: 28.0,
            node_fill: String::from("#ffffff"),
            node_stroke: String::from("#94a3b8"),
            node_stroke_width: 1.5,
            edge_stroke: String::from("#475569"),
            edge_stroke_width: 1.5,
            cluster_fill: String::from("rgba(226,232,240,0.44)"),
            cluster_stroke: String::from("rgba(148,163,184,0.78)"),
            label_color: String::from("#0f172a"),
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

const DENSE_SOURCE_INDEX_LIMIT: usize = 65_536;
const LEGACY_DOTTED_EDGE_DASH: [f64; 2] = [5.0, 5.0];

#[derive(Debug, Default)]
struct SourceIndexSet {
    words: Vec<u64>,
    sparse: BTreeSet<usize>,
    len: usize,
}

impl SourceIndexSet {
    fn insert(&mut self, index: usize) {
        if index < DENSE_SOURCE_INDEX_LIMIT {
            let word_index = index / u64::BITS as usize;
            if word_index >= self.words.len() {
                self.words.resize(word_index + 1, 0);
            }
            let mask = 1_u64 << (index % u64::BITS as usize);
            if self.words[word_index] & mask == 0 {
                self.words[word_index] |= mask;
                self.len += 1;
            }
        } else if self.sparse.insert(index) {
            self.len += 1;
        }
    }

    const fn len(&self) -> usize {
        self.len
    }
}

#[derive(Debug, Default)]
struct SceneRenderStats {
    node_sources: SourceIndexSet,
    edge_sources: SourceIndexSet,
    cluster_sources: SourceIndexSet,
    labels_drawn: usize,
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

        let mut labels_drawn = 0;

        // Draw clusters (background)
        let clusters_drawn = self.draw_clusters(layout, ir, ctx, offset_x, offset_y);

        // Draw layout bands (sequence lifelines, gantt sections, etc.)
        self.draw_bands(layout, ctx, offset_x, offset_y);

        // Draw the time/category axis (gantt dates, xychart categories).
        self.draw_axis_ticks(layout, ctx, offset_x, offset_y);

        // Draw stateDiagram notes.
        self.draw_state_notes(layout, ctx, offset_x, offset_y);

        // Draw the mirrored participant headers at the foot of a sequence diagram.
        self.draw_sequence_mirror_headers(layout, ir, ctx, offset_x, offset_y);

        // Draw the extra rows of packet fields that wrap across a 32-bit boundary.
        self.draw_packet_field_continuations(layout, ir, ctx, offset_x, offset_y);

        // Draw sequence activation bars.
        self.draw_activation_bars(layout, ctx, offset_x, offset_y);

        // Draw sequence lifecycle markers (destroy crosses).
        self.draw_sequence_lifecycle_markers(layout, ctx, offset_x, offset_y);

        // Draw sequence notes and fragments.
        self.draw_sequence_fragments(layout, ctx, offset_x, offset_y);
        self.draw_sequence_notes(layout, ctx, offset_x, offset_y, &mut labels_drawn);

        // Draw pie chart wedges if this is a pie diagram.
        if ir.diagram_type == DiagramType::Pie {
            self.draw_pie_wedges(layout, ir, ctx, offset_x, offset_y, &mut labels_drawn);
        }

        // Draw edges
        let edges_drawn = self.draw_edges(layout, ir, ctx, offset_x, offset_y, &mut labels_drawn);

        // Draw nodes
        let nodes_drawn = self.draw_nodes(layout, ir, ctx, offset_x, offset_y, &mut labels_drawn);

        ctx.restore();
        if self.draw_generic_diagram_title(ctx, ir, canvas_width) {
            labels_drawn += 1;
        }

        CanvasRenderResult {
            draw_calls: self.draw_calls,
            nodes_drawn,
            edges_drawn,
            clusters_drawn,
            labels_drawn,
            viewport,
        }
    }

    fn draw_generic_diagram_title<C: Canvas2dContext>(
        &mut self,
        ctx: &mut C,
        ir: &MermaidDiagramIr,
        canvas_width: f64,
    ) -> bool {
        let Some(title) = generic_canvas_diagram_title(ir) else {
            return false;
        };

        ctx.set_fill_style(&self.config.label_color);
        ctx.set_font(&format!(
            "{}px {}",
            self.config.font_size, self.config.font_family
        ));
        ctx.set_text_align(TextAlign::Center);
        ctx.set_text_baseline(TextBaseline::Top);
        ctx.fill_text(
            title,
            canvas_width / 2.0,
            (self.config.padding * 0.5).max(self.config.font_size * 0.5),
        );
        self.draw_calls += 1;
        true
    }

    /// Render a target-agnostic render scene to a Canvas2D context.
    pub fn render_scene<C: Canvas2dContext>(
        &mut self,
        scene: &RenderScene,
        ctx: &mut C,
    ) -> CanvasRenderResult {
        self.draw_calls = 0;

        let canvas_width = ctx.width();
        let canvas_height = ctx.height();

        let viewport = if self.config.auto_fit {
            fit_to_viewport(
                f64::from(scene.bounds.width),
                f64::from(scene.bounds.height),
                canvas_width,
                canvas_height,
                self.config.padding,
            )
        } else {
            Viewport::new(canvas_width, canvas_height)
        };

        ctx.clear_rect(0.0, 0.0, canvas_width, canvas_height);
        self.draw_calls += 1;

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

        let (offset_x, offset_y) = if self.config.auto_fit {
            (-f64::from(scene.bounds.x), -f64::from(scene.bounds.y))
        } else {
            (
                self.config.padding - f64::from(scene.bounds.x),
                self.config.padding - f64::from(scene.bounds.y),
            )
        };

        let mut stats = SceneRenderStats::default();
        self.render_group(&scene.root, ctx, offset_x, offset_y, &mut stats);

        ctx.restore();

        CanvasRenderResult {
            draw_calls: self.draw_calls,
            nodes_drawn: stats.node_sources.len(),
            edges_drawn: stats.edge_sources.len(),
            clusters_drawn: stats.cluster_sources.len(),
            labels_drawn: stats.labels_drawn,
            viewport,
        }
    }

    fn render_group<C: Canvas2dContext>(
        &mut self,
        group: &RenderGroup,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
        stats: &mut SceneRenderStats,
    ) {
        ctx.save();

        if let Some(transform) = group.transform {
            self.apply_render_transform(ctx, transform);
        }

        if let Some(clip) = &group.clip {
            self.apply_render_clip(ctx, clip, offset_x, offset_y);
        }

        for child in &group.children {
            match child {
                RenderItem::Group(nested) => {
                    self.render_group(nested, ctx, offset_x, offset_y, stats);
                }
                RenderItem::Path(path) => {
                    self.render_path_item(path, ctx, offset_x, offset_y, stats);
                }
                RenderItem::Text(text) => {
                    self.render_text_item(text, ctx, offset_x, offset_y, stats);
                }
            }
        }

        ctx.restore();
    }

    fn apply_render_transform<C: Canvas2dContext>(
        &mut self,
        ctx: &mut C,
        transform: RenderTransform,
    ) {
        match transform {
            RenderTransform::Matrix { a, b, c, d, e, f } => {
                if (a - 1.0).abs() < f32::EPSILON
                    && b.abs() < f32::EPSILON
                    && c.abs() < f32::EPSILON
                    && (d - 1.0).abs() < f32::EPSILON
                    && e.abs() < f32::EPSILON
                    && f.abs() < f32::EPSILON
                {
                    return;
                }

                if b.abs() < f32::EPSILON && c.abs() < f32::EPSILON {
                    ctx.translate(f64::from(e), f64::from(f));
                    ctx.scale(f64::from(a), f64::from(d));
                }

                // For arbitrary affine matrices, defer transformation for now.
                // Using `set_transform` here would replace the active viewport transform.
            }
        }
    }

    fn apply_render_clip<C: Canvas2dContext>(
        &mut self,
        ctx: &mut C,
        clip: &RenderClip,
        offset_x: f64,
        offset_y: f64,
    ) {
        ctx.begin_path();
        match clip {
            RenderClip::Rect(rect) => {
                ctx.rect(
                    f64::from(rect.x) + offset_x,
                    f64::from(rect.y) + offset_y,
                    f64::from(rect.width),
                    f64::from(rect.height),
                );
            }
            RenderClip::Path(commands) => {
                self.emit_path_commands(ctx, commands, offset_x, offset_y);
            }
        }
        ctx.clip();
        self.draw_calls += 1;
    }

    fn render_path_item<C: Canvas2dContext>(
        &mut self,
        path: &RenderPath,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
        stats: &mut SceneRenderStats,
    ) {
        ctx.begin_path();
        self.emit_path_commands(ctx, &path.commands, offset_x, offset_y);

        if let Some(fill) = &path.fill {
            self.apply_fill(ctx, fill);
            ctx.fill();
            self.draw_calls += 1;
            ctx.set_global_alpha(1.0);
        }

        if let Some(stroke) = &path.stroke {
            self.apply_stroke(ctx, stroke);
            ctx.stroke();
            self.draw_calls += 1;
            ctx.set_line_dash(&[]);
            ctx.set_global_alpha(1.0);
        }

        self.draw_path_markers(path, ctx, offset_x, offset_y);

        match path.source {
            RenderSource::Node(index) => {
                stats.node_sources.insert(index);
            }
            RenderSource::Edge(index) => {
                stats.edge_sources.insert(index);
            }
            RenderSource::Cluster(index) => {
                stats.cluster_sources.insert(index);
            }
            RenderSource::Diagram => {}
        }
    }

    fn draw_path_markers<C: Canvas2dContext>(
        &mut self,
        path: &RenderPath,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        // Most scene paths have no markers. Avoid cloning a color before discovering
        // there is nothing to draw, and borrow the path-owned color when markers exist.
        if path.marker_start == MarkerKind::None && path.marker_end == MarkerKind::None {
            return;
        }

        let fallback_color;
        let stroke_color = if let Some(stroke) = &path.stroke {
            stroke.color.as_str()
        } else {
            fallback_color = self.config.edge_stroke.clone();
            fallback_color.as_str()
        };

        if path.marker_start != MarkerKind::None
            && let Some((x, y, angle)) = path_marker_start_geometry(&path.commands)
        {
            self.draw_marker(
                ctx,
                path.marker_start,
                x + offset_x,
                y + offset_y,
                angle,
                stroke_color,
            );
        }

        if path.marker_end != MarkerKind::None
            && let Some((x, y, angle)) = path_marker_end_geometry(&path.commands)
        {
            self.draw_marker(
                ctx,
                path.marker_end,
                x + offset_x,
                y + offset_y,
                angle,
                stroke_color,
            );
        }
    }

    fn draw_marker<C: Canvas2dContext>(
        &mut self,
        ctx: &mut C,
        marker: MarkerKind,
        x: f64,
        y: f64,
        angle: f64,
        stroke_color: &str,
    ) {
        self.draw_calls += draw_marker_primitive(
            ctx,
            marker,
            x,
            y,
            angle,
            &self.config.node_fill,
            stroke_color,
        );
    }

    fn render_text_item<C: Canvas2dContext>(
        &mut self,
        text: &RenderText,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
        stats: &mut SceneRenderStats,
    ) {
        self.apply_fill(ctx, &text.fill);
        ctx.set_font(&format!("{}px {}", text.font_size, self.config.font_family));
        ctx.set_text_align(match text.align {
            IrTextAlign::Start => TextAlign::Left,
            IrTextAlign::Middle => TextAlign::Center,
            IrTextAlign::End => TextAlign::Right,
        });
        ctx.set_text_baseline(match text.baseline {
            IrTextBaseline::Top => TextBaseline::Top,
            IrTextBaseline::Middle => TextBaseline::Middle,
            IrTextBaseline::Bottom => TextBaseline::Bottom,
        });

        let lines: Vec<&str> = text.text.lines().collect();
        let line_height = f64::from(text.font_size) * 1.2;
        let total_height = line_height * lines.len() as f64;
        let mut current_y = f64::from(text.y) + offset_y;

        if lines.len() > 1 {
            match text.baseline {
                IrTextBaseline::Top => {}
                IrTextBaseline::Middle => {
                    current_y -= (total_height - line_height) / 2.0;
                }
                IrTextBaseline::Bottom => {
                    current_y -= total_height - line_height;
                }
            }
        }

        for line in lines {
            ctx.fill_text(line, f64::from(text.x) + offset_x, current_y);
            current_y += line_height;
            self.draw_calls += 1;
        }

        stats.labels_drawn += 1;
        ctx.set_global_alpha(1.0);
    }

    fn apply_fill<C: Canvas2dContext>(&self, ctx: &mut C, fill: &FillStyle) {
        match fill {
            FillStyle::Solid { color, opacity } => {
                ctx.set_fill_style(color);
                ctx.set_global_alpha(f64::from(*opacity));
            }
        }
    }

    fn apply_stroke<C: Canvas2dContext>(&self, ctx: &mut C, stroke: &StrokeStyle) {
        ctx.set_stroke_style(&stroke.color);
        ctx.set_line_width(f64::from(stroke.width));
        ctx.set_global_alpha(f64::from(stroke.opacity));
        if stroke.dash_array.is_empty() {
            ctx.set_line_dash(&[]);
        } else {
            with_canvas_dash_f64(&stroke.dash_array, |dash| ctx.set_line_dash(dash));
        }
        ctx.set_line_cap(match stroke.line_cap {
            IrLineCap::Butt => LineCap::Butt,
            IrLineCap::Round => LineCap::Round,
            IrLineCap::Square => LineCap::Square,
        });
        ctx.set_line_join(match stroke.line_join {
            IrLineJoin::Miter => LineJoin::Miter,
            IrLineJoin::Round => LineJoin::Round,
            IrLineJoin::Bevel => LineJoin::Bevel,
        });
    }

    fn emit_path_commands<C: Canvas2dContext>(
        &self,
        ctx: &mut C,
        commands: &[PathCmd],
        offset_x: f64,
        offset_y: f64,
    ) {
        for command in commands {
            match command {
                PathCmd::MoveTo { x, y } => {
                    ctx.move_to(f64::from(*x) + offset_x, f64::from(*y) + offset_y);
                }
                PathCmd::LineTo { x, y } => {
                    ctx.line_to(f64::from(*x) + offset_x, f64::from(*y) + offset_y);
                }
                PathCmd::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => {
                    ctx.bezier_curve_to(
                        f64::from(*c1x) + offset_x,
                        f64::from(*c1y) + offset_y,
                        f64::from(*c2x) + offset_x,
                        f64::from(*c2y) + offset_y,
                        f64::from(*x) + offset_x,
                        f64::from(*y) + offset_y,
                    );
                }
                PathCmd::QuadTo { cx, cy, x, y } => {
                    ctx.quadratic_curve_to(
                        f64::from(*cx) + offset_x,
                        f64::from(*cy) + offset_y,
                        f64::from(*x) + offset_x,
                        f64::from(*y) + offset_y,
                    );
                }
                PathCmd::Close => {
                    ctx.close_path();
                }
            }
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

        // The cluster label font is a pure function of `config` (invariant across clusters), so format it
        // once and reuse it — the prior canvas font-hoist campaign missed this site. Lazy (like the
        // `standard_label_font` site) so a cluster-free diagram never formats it. Byte-identical to the
        // per-cluster `format!("{}px {}", font_size*0.9, font_family)`.
        let mut cluster_label_font: Option<String> = None;

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
            let title_text = cluster_box.title.as_deref().or_else(|| {
                ir.clusters
                    .get(cluster_box.cluster_index)
                    .and_then(|ir_cluster| ir_cluster.title)
                    .and_then(|title_id| ir.labels.get(title_id.0))
                    .map(|label| label.text.as_str())
            });

            if let Some(title_text) = title_text {
                ctx.set_fill_style(&self.config.label_color);
                ctx.set_font(cluster_label_font.get_or_insert_with(|| {
                    format!(
                        "{}px {}",
                        self.config.font_size * 0.9,
                        self.config.font_family
                    )
                }));
                ctx.set_text_align(TextAlign::Left);
                ctx.set_text_baseline(TextBaseline::Top);
                ctx.fill_text(title_text, x + 8.0, y + 4.0);
                self.draw_calls += 1;
            }

            count += 1;
        }

        count
    }

    /// Draw layout extension bands (sequence lifelines, gantt sections, etc.).
    fn draw_bands<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        use fm_layout::LayoutBandKind;
        // Invariant section-label font, formatted once and reused across bands (lazy so a band-free
        // diagram never formats it). Byte-identical to the per-band
        // `format!("bold {}px {}", font_size*0.85, font_family)`.
        let mut section_label_font: Option<String> = None;
        for band in &layout.extensions.bands {
            let x = f64::from(band.bounds.x) + offset_x;
            let y = f64::from(band.bounds.y) + offset_y;
            let w = f64::from(band.bounds.width);
            let h = f64::from(band.bounds.height);

            match band.kind {
                LayoutBandKind::Lane => {
                    // Sequence lifeline: dashed vertical center line.
                    let cx = x + w / 2.0;
                    ctx.set_stroke_style(&self.config.node_stroke);
                    ctx.set_line_width(1.0);
                    ctx.set_line_dash(&[6.0, 4.0]);
                    ctx.begin_path();
                    ctx.move_to(cx, y);
                    ctx.line_to(cx, y + h);
                    ctx.stroke();
                    ctx.set_line_dash(&[]);
                    self.draw_calls += 1;
                }
                LayoutBandKind::Section => {
                    // Gantt section: light background band.
                    ctx.set_fill_style("rgba(226,232,240,0.3)");
                    ctx.fill_rect(x, y, w, h);
                    if !band.label.is_empty() {
                        ctx.set_fill_style(&self.config.label_color);
                        ctx.set_font(section_label_font.get_or_insert_with(|| {
                            format!(
                                "bold {}px {}",
                                self.config.font_size * 0.85,
                                self.config.font_family
                            )
                        }));
                        ctx.set_text_align(TextAlign::Left);
                        ctx.set_text_baseline(TextBaseline::Top);
                        ctx.fill_text(&band.label, x + 4.0, y + 2.0);
                    }
                    self.draw_calls += 1;
                }
                LayoutBandKind::Column => {
                    // Kanban column: subtle vertical separator.
                    ctx.set_stroke_style("rgba(148,163,184,0.4)");
                    ctx.set_line_width(1.0);
                    ctx.begin_path();
                    ctx.move_to(x + w, y);
                    ctx.line_to(x + w, y + h);
                    ctx.stroke();
                    self.draw_calls += 1;
                }
            }
        }
    }

    /// Draw sequence activation bars from layout extensions.
    /// Draw axis tick labels -- gantt dates and xychart categories.
    ///
    /// `extensions.axis_ticks` is filled by the gantt and xychart layout arms and drawn by
    /// fm-render-svg; this renderer referenced it nowhere (bd-t1jj). Canvas is the browser preview
    /// surface -- fm-wasm renders through `render_to_canvas_with_layout` -- so a gantt showed bars
    /// with nothing to measure them against, which is exactly the state bd-trsd fixed on the SVG side
    /// and I fixed on the terminal side in 27a6aadd.
    ///
    /// Ticks are drawn at the TOP of the layout bounds, above the bars, for the same reason the
    /// terminal draws them there: writing at the bar row would overwrite task names, trading one
    /// piece of dropped content for another.
    /// Draw the participant headers mirrored at the FOOT of a sequence diagram.
    ///
    /// `extensions.sequence_mirror_headers` is a `Vec<LayoutNodeBox>` filled by the sequence layout
    /// arm and rendered by fm-render-svg through the same node renderer it uses for the top row. This
    /// renderer referenced it nowhere (bd-t1jj), and canvas is the browser preview surface that
    /// fm-wasm renders through.
    ///
    /// mermaid draws that bottom row, and it is not ornamental: on a long sequence diagram the top
    /// headers scroll out of view, and without the mirrored row the reader has no way to tell which
    /// lifeline is which. Its absence is most costly exactly where the diagram is largest.
    fn draw_sequence_mirror_headers<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        if layout.extensions.sequence_mirror_headers.is_empty() {
            return;
        }
        let mut header_font: Option<String> = None;
        for node_box in &layout.extensions.sequence_mirror_headers {
            let x = f64::from(node_box.bounds.x) + offset_x;
            let y = f64::from(node_box.bounds.y) + offset_y;
            let w = f64::from(node_box.bounds.width);
            let h = f64::from(node_box.bounds.height);
            if w <= 0.0 || h <= 0.0 {
                continue;
            }

            ctx.set_fill_style(&self.config.node_fill);
            ctx.fill_rect(x, y, w, h);
            ctx.set_stroke_style(&self.config.node_stroke);
            ctx.set_line_width(self.config.node_stroke_width);
            ctx.stroke_rect(x, y, w, h);
            self.draw_calls += 2;

            // The label comes from the IR node this header mirrors, so the foot row cannot drift from
            // the head row: both name the same participant from the same source.
            let label = ir
                .nodes
                .get(node_box.node_index)
                .map(|node| {
                    node.label
                        .and_then(|label_id| ir.labels.get(label_id.0))
                        .map_or(node.id.as_str(), |label| label.text.as_str())
                })
                .unwrap_or(node_box.node_id.as_str());
            if label.is_empty() {
                continue;
            }
            ctx.set_fill_style(&self.config.label_color);
            ctx.set_font(header_font.get_or_insert_with(|| standard_node_font(&self.config)));
            ctx.set_text_align(TextAlign::Center);
            ctx.set_text_baseline(TextBaseline::Middle);
            ctx.fill_text(label, x + w / 2.0, y + h / 2.0);
            self.draw_calls += 1;
        }
    }

    /// Draw the extra rows of a packet field that crosses a 32-bit boundary.
    ///
    /// `extensions.packet_field_continuations` gives one box per additional row a field occupies, and
    /// fm-render-svg draws each with its label. This renderer drew only the primary box (bd-t1jj), so
    /// on the terminal side the same omission rendered a 24-bit field with the extent of an 8-bit one:
    /// primary at (768, 0, 256, 55), continuation at (0, 70, 512, 55), and only the 256-wide box drawn.
    ///
    /// That is not a missing decoration. A packet diagram exists to show how wide each field is, so
    /// dropping most of a field's extent misstates the one thing the diagram is for. The label is
    /// drawn on the continuation too, matching the SVG arm: a second box on a later row with no name
    /// in it does not say WHICH field wrapped, which is the only thing a reader needs from it.
    fn draw_packet_field_continuations<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        if layout.extensions.packet_field_continuations.is_empty() {
            return;
        }
        let mut field_font: Option<String> = None;
        for continuation in &layout.extensions.packet_field_continuations {
            let x = f64::from(continuation.bounds.x) + offset_x;
            let y = f64::from(continuation.bounds.y) + offset_y;
            let w = f64::from(continuation.bounds.width);
            let h = f64::from(continuation.bounds.height);
            if w <= 0.0 || h <= 0.0 {
                continue;
            }

            ctx.set_fill_style(&self.config.node_fill);
            ctx.fill_rect(x, y, w, h);
            ctx.set_stroke_style(&self.config.node_stroke);
            ctx.set_line_width(self.config.node_stroke_width);
            ctx.stroke_rect(x, y, w, h);
            self.draw_calls += 2;

            let Some(node) = ir.nodes.get(continuation.node_index) else {
                continue;
            };
            let label = node
                .label
                .and_then(|label_id| ir.labels.get(label_id.0))
                .map_or(node.id.as_str(), |label| label.text.as_str());
            if label.is_empty() {
                continue;
            }
            ctx.set_fill_style(&self.config.label_color);
            ctx.set_font(field_font.get_or_insert_with(|| standard_node_font(&self.config)));
            ctx.set_text_align(TextAlign::Center);
            ctx.set_text_baseline(TextBaseline::Middle);
            ctx.fill_text(label, x + w / 2.0, y + h / 2.0);
            self.draw_calls += 1;
        }
    }

    fn draw_axis_ticks<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        if layout.extensions.axis_ticks.is_empty() {
            return;
        }
        let mut tick_font: Option<String> = None;
        let y = f64::from(layout.bounds.y) + offset_y + 12.0;
        for tick in &layout.extensions.axis_ticks {
            if tick.label.is_empty() {
                continue;
            }
            ctx.set_fill_style(&self.config.label_color);
            ctx.set_font(tick_font.get_or_insert_with(|| {
                format!("{}px {}", self.config.font_size * 0.75, self.config.font_family)
            }));
            ctx.fill_text(&tick.label, f64::from(tick.position) + offset_x, y);
            self.draw_calls += 1;
        }
    }

    /// Draw stateDiagram notes -- box, leader and text.
    ///
    /// `extensions.state_notes` is filled by the state layout arm (bd-a6l4) and drawn by
    /// fm-render-svg; this renderer referenced it nowhere (bd-t1jj). Canvas is the browser preview
    /// surface -- fm-wasm renders through `render_to_canvas_with_layout` -- so `note right of X : ...`
    /// produced a note that existed in the layout, was hashed into the layout checksum, and appeared
    /// nowhere on screen.
    ///
    /// The LEADER is drawn, not just the box, for the same reason the terminal draws it (1d7324f7): a
    /// box beside a state with nothing connecting them reads as another node rather than as an
    /// annotation OF that state. That is a different wrong picture, not a smaller one.
    fn draw_state_notes<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        if layout.extensions.state_notes.is_empty() {
            return;
        }
        let mut note_font: Option<String> = None;
        for note in &layout.extensions.state_notes {
            let x = f64::from(note.bounds.x) + offset_x;
            let y = f64::from(note.bounds.y) + offset_y;
            let w = f64::from(note.bounds.width);
            let h = f64::from(note.bounds.height);
            if w <= 0.0 || h <= 0.0 {
                continue;
            }

            ctx.set_fill_style(&self.config.node_fill);
            ctx.fill_rect(x, y, w, h);
            ctx.set_stroke_style(&self.config.node_stroke);
            ctx.set_line_width(self.config.node_stroke_width);
            ctx.stroke_rect(x, y, w, h);

            // Leader from the annotated state to the note.
            ctx.set_stroke_style(&self.config.edge_stroke);
            ctx.begin_path();
            ctx.move_to(
                f64::from(note.leader_start.x) + offset_x,
                f64::from(note.leader_start.y) + offset_y,
            );
            ctx.line_to(
                f64::from(note.leader_end.x) + offset_x,
                f64::from(note.leader_end.y) + offset_y,
            );
            ctx.stroke();

            if !note.text.is_empty() {
                ctx.set_fill_style(&self.config.label_color);
                ctx.set_font(note_font.get_or_insert_with(|| {
                    format!("{}px {}", self.config.font_size * 0.8, self.config.font_family)
                }));
                // Multi-line aware: `note right of X … end note` is the form that carries more than a
                // sentence, and drawing only the first line would silently drop the rest.
                let line_height = self.config.font_size * 1.2;
                for (row, line) in note.text.lines().enumerate() {
                    let line_y = y + line_height * (row as f64 + 1.0);
                    if line_y > y + h {
                        break;
                    }
                    ctx.fill_text(line, x + 4.0, line_y);
                }
            }
            self.draw_calls += 3;
        }
    }

    fn draw_activation_bars<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        for bar in &layout.extensions.activation_bars {
            let x = f64::from(bar.bounds.x) + offset_x;
            let y = f64::from(bar.bounds.y) + offset_y;
            let w = f64::from(bar.bounds.width);
            let h = f64::from(bar.bounds.height);
            if w <= 0.0 || h <= 0.0 {
                continue;
            }

            ctx.set_fill_style(&self.config.node_fill);
            ctx.fill_rect(x, y, w, h);
            ctx.set_stroke_style(&self.config.node_stroke);
            ctx.set_line_width(self.config.node_stroke_width);
            ctx.stroke_rect(x, y, w, h);
            self.draw_calls += 2;
        }
    }

    /// Draw sequence lifecycle markers -- the X that terminates a destroyed participant's lifeline.
    ///
    /// `extensions.sequence_lifecycle_markers` is filled by the sequence layout arm and drawn by
    /// fm-render-svg; this renderer referenced it nowhere (bd-t1jj). Canvas is not a dead surface --
    /// fm-wasm renders the browser preview through `render_to_canvas_with_layout` -- so `destroy Bob`
    /// produced a lifeline that simply stopped, with nothing marking that the participant was
    /// destroyed rather than merely idle. Those are different diagrams.
    ///
    /// Geometry mirrors the SVG arm exactly: a cross of `size`, centred on `center`, so the two
    /// backends cannot disagree about where a destroyed lifeline ends.
    fn draw_sequence_lifecycle_markers<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        for marker in &layout.extensions.sequence_lifecycle_markers {
            match marker.kind {
                fm_layout::LayoutSequenceLifecycleMarkerKind::Destroy => {
                    let half = f64::from(marker.size) * 0.5;
                    if half <= 0.0 {
                        continue;
                    }
                    let cx = f64::from(marker.center.x) + offset_x;
                    let cy = f64::from(marker.center.y) + offset_y;
                    ctx.set_stroke_style(&self.config.edge_stroke);
                    ctx.set_line_width(1.5);
                    ctx.begin_path();
                    ctx.move_to(cx - half, cy - half);
                    ctx.line_to(cx + half, cy + half);
                    ctx.move_to(cx + half, cy - half);
                    ctx.line_to(cx - half, cy + half);
                    ctx.stroke();
                    self.draw_calls += 1;
                }
            }
        }
    }

    /// Draw sequence diagram interaction fragment boxes (loop, alt, par, etc.).
    fn draw_sequence_fragments<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        let mut fragment_font = None;

        for fragment in &layout.extensions.sequence_fragments {
            let x = f64::from(fragment.bounds.x) + offset_x;
            let y = f64::from(fragment.bounds.y) + offset_y;
            let w = f64::from(fragment.bounds.width);
            let h = f64::from(fragment.bounds.height);

            // Semi-transparent background.
            ctx.set_fill_style("rgba(226,232,240,0.2)");
            ctx.fill_rect(x, y, w, h);

            // Dashed border.
            ctx.set_stroke_style(&self.config.node_stroke);
            ctx.set_line_width(1.0);
            ctx.set_line_dash(&[4.0, 4.0]);
            ctx.stroke_rect(x, y, w, h);
            ctx.set_line_dash(&[]);

            // Kind label in top-left corner.
            if fragment.label.is_empty() {
                let label = fragment_kind_label(fragment.kind);
                ctx.set_fill_style(&self.config.label_color);
                let fragment_font =
                    fragment_font.get_or_insert_with(|| sequence_fragment_font_css(&self.config));
                ctx.set_font(fragment_font.as_str());
                ctx.set_text_align(TextAlign::Left);
                ctx.set_text_baseline(TextBaseline::Top);
                ctx.fill_text(label, x + 6.0, y + 4.0);
            } else {
                let label = format!(
                    "[{}] {}",
                    fragment_kind_label(fragment.kind),
                    fragment.label
                );
                ctx.set_fill_style(&self.config.label_color);
                let fragment_font =
                    fragment_font.get_or_insert_with(|| sequence_fragment_font_css(&self.config));
                ctx.set_font(fragment_font.as_str());
                ctx.set_text_align(TextAlign::Left);
                ctx.set_text_baseline(TextBaseline::Top);
                ctx.fill_text(&label, x + 6.0, y + 4.0);
            }
            self.draw_calls += 3;
        }
    }

    /// Draw sequence diagram notes.
    fn draw_sequence_notes<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
        labels_drawn: &mut usize,
    ) {
        let mut note_font = None;

        for note in &layout.extensions.sequence_notes {
            let x = f64::from(note.bounds.x) + offset_x;
            let y = f64::from(note.bounds.y) + offset_y;
            let w = f64::from(note.bounds.width);
            let h = f64::from(note.bounds.height);

            // Note background — uses config colors for theme awareness.
            ctx.set_fill_style(&self.config.node_fill);
            ctx.fill_rect(x, y, w, h);
            ctx.set_stroke_style(&self.config.node_stroke);
            ctx.set_line_width(1.0);
            ctx.stroke_rect(x, y, w, h);

            // Note text.
            if !note.text.is_empty() {
                ctx.set_fill_style(&self.config.label_color);
                let note_font =
                    note_font.get_or_insert_with(|| secondary_label_font_css(&self.config));
                ctx.set_font(note_font.as_str());
                ctx.set_text_align(TextAlign::Center);
                ctx.set_text_baseline(TextBaseline::Middle);
                ctx.fill_text(&note.text, x + w / 2.0, y + h / 2.0);
                *labels_drawn += 1;
            }
            self.draw_calls += 3;
        }
    }

    /// Draw all edges.
    fn draw_edges<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
        labels_drawn: &mut usize,
    ) -> usize {
        let mut count = 0;
        let mut edge_label_font = None;

        for edge_path in &layout.edges {
            let ir_edge = ir.edges.get(edge_path.edge_index);
            let arrow = ir_edge.map_or(ArrowType::Arrow, |e| e.arrow);

            // Deref the point `SmallVec` to a slice ONCE — the edge draw below indexes it ~12× (path
            // loop, arrowhead direction, label anchor), and each `edge_path.points[i]` / `.len()` was a
            // fresh `SmallVec` deref (inline-vs-spilled branch). Byte-identical: same points, same order.
            let points = edge_path.points.as_slice();
            if points.len() < 2 {
                continue;
            }

            // Set edge style
            let (stroke_width, dash_pattern) =
                legacy_edge_stroke(arrow, self.config.edge_stroke_width);

            ctx.set_stroke_style(&self.config.edge_stroke);
            ctx.set_line_width(stroke_width);
            ctx.set_line_dash(dash_pattern);

            // Draw edge path
            ctx.begin_path();
            let first = &points[0];
            ctx.move_to(f64::from(first.x) + offset_x, f64::from(first.y) + offset_y);

            for point in points.iter().skip(1) {
                ctx.line_to(f64::from(point.x) + offset_x, f64::from(point.y) + offset_y);
            }
            ctx.stroke();
            self.draw_calls += 1;

            let uml_markers = if edge_path.reversed {
                None
            } else {
                legacy_uml_markers(arrow)
            };
            if let Some((marker_start, marker_end)) = uml_markers {
                let start = &points[0];
                let next = &points[1];
                let end = &points[points.len() - 1];
                let prev = &points[points.len() - 2];

                if marker_start != MarkerKind::None {
                    let sx = f64::from(start.x) + offset_x;
                    let sy = f64::from(start.y) + offset_y;
                    let angle = f64::from(next.y - start.y).atan2(f64::from(next.x - start.x));
                    let marker_calls = draw_marker_primitive(
                        ctx,
                        marker_start,
                        sx,
                        sy,
                        angle,
                        &self.config.node_fill,
                        &self.config.edge_stroke,
                    );
                    self.draw_calls += marker_calls;
                }

                if marker_end != MarkerKind::None {
                    let ex = f64::from(end.x) + offset_x;
                    let ey = f64::from(end.y) + offset_y;
                    let angle = f64::from(end.y - prev.y).atan2(f64::from(end.x - prev.x));
                    let marker_calls = draw_marker_primitive(
                        ctx,
                        marker_end,
                        ex,
                        ey,
                        angle,
                        &self.config.node_fill,
                        &self.config.edge_stroke,
                    );
                    self.draw_calls += marker_calls;
                }
            } else {
                let end = &points[points.len() - 1];
                let prev = &points[points.len() - 2];
                let angle = f64::from(end.y - prev.y).atan2(f64::from(end.x - prev.x));

                let ex = f64::from(end.x) + offset_x;
                let ey = f64::from(end.y) + offset_y;

                match arrow {
                    ArrowType::Line => {}
                    ArrowType::Circle => {
                        draw_circle_marker(
                            ctx,
                            ex,
                            ey,
                            4.0,
                            &self.config.node_fill,
                            &self.config.edge_stroke,
                        );
                        self.draw_calls += 1;
                    }
                    ArrowType::Cross => {
                        draw_cross_marker(ctx, ex, ey, 8.0, &self.config.edge_stroke);
                        self.draw_calls += 1;
                    }
                    // All other arrow types (half arrows, stick arrows, etc.) — render as standard arrowhead.
                    _ => {
                        draw_arrowhead(ctx, ex, ey, angle, 10.0, &self.config.edge_stroke);
                        self.draw_calls += 1;
                    }
                }

                // Draw arrowhead at start for double arrows
                if matches!(
                    arrow,
                    ArrowType::DoubleArrow
                        | ArrowType::DoubleThickArrow
                        | ArrowType::DoubleDottedArrow
                ) {
                    let start = &points[0];
                    let next = &points[1];
                    let start_angle =
                        f64::from(start.y - next.y).atan2(f64::from(start.x - next.x));
                    let sx = f64::from(start.x) + offset_x;
                    let sy = f64::from(start.y) + offset_y;

                    draw_arrowhead(ctx, sx, sy, start_angle, 10.0, &self.config.edge_stroke);
                    self.draw_calls += 1;
                }
            }

            // Draw edge label if present
            if let Some(label_id) = ir_edge.and_then(|e| e.label)
                && let Some(label) = ir.labels.get(label_id.0)
                && points.len() >= 2
            {
                let label_offset = self.config.font_size * 0.8;
                let (lx, ly) = if points.len() == 4 {
                    let p1 = &points[1];
                    let p2 = &points[2];
                    (
                        f64::from(f32::midpoint(p1.x, p2.x)) + offset_x,
                        f64::from(f32::midpoint(p1.y, p2.y)) + offset_y - label_offset,
                    )
                } else if points.len() == 2 {
                    let p1 = &points[0];
                    let p2 = &points[1];
                    (
                        f64::from(f32::midpoint(p1.x, p2.x)) + offset_x,
                        f64::from(f32::midpoint(p1.y, p2.y)) + offset_y - label_offset,
                    )
                } else {
                    let mid_idx = points.len() / 2;
                    let mid = &points[mid_idx];
                    (
                        f64::from(mid.x) + offset_x,
                        f64::from(mid.y) + offset_y - label_offset,
                    )
                };

                let edge_label_font =
                    edge_label_font.get_or_insert_with(|| secondary_label_font_css(&self.config));
                ctx.set_font(edge_label_font.as_str());

                let line_height = self.config.font_size * 1.2;

                // The common single-line edge label (`:has`, `-->|x|`, …) draws exactly `label.text` at
                // `ly`, so measure it directly and skip the `Vec<&str>` collect. Only a genuinely
                // multi-line label (`\n`) needs the split (it's re-read for max-width + count + per-line
                // draw). Byte-identical: for a `\n`-free label the sole `lines()` item IS `label.text`,
                // `total_height == line_height`, and `start_y == ly`.
                if !label.text.contains('\n') {
                    let label_width = ctx.measure_text(&label.text).width + 8.0;
                    let label_height = line_height + 4.0;

                    ctx.set_fill_style(&self.config.node_fill);
                    ctx.fill_rect(
                        lx - label_width / 2.0,
                        ly - label_height / 2.0,
                        label_width,
                        label_height,
                    );
                    self.draw_calls += 1;

                    ctx.set_fill_style(&self.config.label_color);
                    ctx.set_font(edge_label_font.as_str());
                    ctx.set_text_align(TextAlign::Center);
                    ctx.set_text_baseline(TextBaseline::Middle);
                    ctx.fill_text(&label.text, lx, ly);
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                } else {
                    // Background for label
                    let lines: Vec<&str> = label.text.lines().collect();
                    let mut max_text_width = 0.0_f64;
                    for line in &lines {
                        let text_metrics = ctx.measure_text(line);
                        max_text_width = max_text_width.max(text_metrics.width);
                    }

                    let label_width = max_text_width + 8.0;
                    let total_height = lines.len() as f64 * line_height;
                    let label_height = total_height + 4.0;

                    ctx.set_fill_style(&self.config.node_fill);
                    ctx.fill_rect(
                        lx - label_width / 2.0,
                        ly - label_height / 2.0,
                        label_width,
                        label_height,
                    );
                    self.draw_calls += 1;

                    // Label text
                    ctx.set_fill_style(&self.config.label_color);
                    ctx.set_font(edge_label_font.as_str());
                    ctx.set_text_align(TextAlign::Center);
                    ctx.set_text_baseline(TextBaseline::Middle);

                    let start_y = ly - (total_height / 2.0) + (line_height / 2.0);
                    for (i, line) in lines.iter().enumerate() {
                        ctx.fill_text(line, lx, start_y + (i as f64) * line_height);
                        self.draw_calls += 1;
                        *labels_drawn += 1;
                    }
                }
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
        labels_drawn: &mut usize,
    ) -> usize {
        let mut count = 0;
        let mut class_compartment_fonts = None;
        let mut standard_label_font = None;
        // Resolved gantt name placements, indexed by node. Built once and only when the diagram has
        // any, so every other diagram type takes exactly the path it took before.
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

        for node_box in &layout.nodes {
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

            // Check for class diagram three-compartment rendering.
            if let Some(node) = ir_node
                && let Some(ref meta) = node.class_meta
                && (!meta.attributes.is_empty() || !meta.methods.is_empty())
            {
                let line_h = self.config.font_size * 1.3;
                let member_font = self.config.font_size * 0.9;
                let padding = 6.0;

                ctx.set_fill_style(&self.config.label_color);
                ctx.set_text_baseline(TextBaseline::Middle);

                // Header: class name centered + bold.
                let class_name = node
                    .label
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|l| l.text.as_str())
                    .unwrap_or(&node.id);
                let display_name = if meta.generics.is_empty() {
                    class_name.to_string()
                } else {
                    format!("{class_name}<{}>", meta.generics.join(", "))
                };

                let class_fonts = class_compartment_fonts
                    .get_or_insert_with(|| class_compartment_font_css(&self.config));
                ctx.set_font(class_fonts.0.as_str());
                ctx.set_text_align(TextAlign::Center);
                let mut cursor_y = y + line_h;

                // STEREOTYPE above the name, where fm-render-svg puts it (bd-rk14).
                //
                // Measured SVG vs canvas: an `interface` stereotype drew in the SVG and not on the
                // canvas. Same gap the terminal had (bd-039t) — the compartment path drew name,
                // attributes and methods and skipped `meta.stereotype`. Mapping mirrors
                // fm-render-svg, including Enum rendering as `enumeration` and a Custom stereotype
                // written verbatim.
                if let Some(stereotype) = &meta.stereotype {
                    let stereo_text = match stereotype {
                        fm_core::ClassStereotype::Interface => "<<interface>>",
                        fm_core::ClassStereotype::Abstract => "<<abstract>>",
                        fm_core::ClassStereotype::Enum => "<<enumeration>>",
                        fm_core::ClassStereotype::Service => "<<service>>",
                        fm_core::ClassStereotype::Custom(custom) => custom.as_str(),
                    };
                    ctx.fill_text(stereo_text, x + w / 2.0, cursor_y);
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                    cursor_y += line_h * 0.8;
                }

                ctx.fill_text(&display_name, x + w / 2.0, cursor_y);
                self.draw_calls += 1;
                *labels_drawn += 1;
                cursor_y += line_h * 0.5;

                // Separator line.
                ctx.begin_path();
                ctx.move_to(x, cursor_y);
                ctx.line_to(x + w, cursor_y);
                ctx.stroke();
                self.draw_calls += 1;
                cursor_y += member_font * 0.5;

                // Attributes.
                ctx.set_font(class_fonts.1.as_str());
                ctx.set_text_align(TextAlign::Left);
                for attr in &meta.attributes {
                    if cursor_y > y + h - line_h * 0.5 {
                        break;
                    }
                    let vis = class_vis_char(attr.visibility);
                    let text = format!("{vis}{}", attr.name);
                    ctx.fill_text(&text, x + padding, cursor_y);
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                    cursor_y += member_font * 1.2;
                }

                // Separator before methods.
                if !meta.attributes.is_empty() && !meta.methods.is_empty() {
                    ctx.begin_path();
                    ctx.move_to(x, cursor_y);
                    ctx.line_to(x + w, cursor_y);
                    ctx.stroke();
                    self.draw_calls += 1;
                    cursor_y += member_font * 0.5;
                }

                // Methods.
                for method in &meta.methods {
                    if cursor_y > y + h - member_font * 0.5 {
                        break;
                    }
                    let vis = class_vis_char(method.visibility);
                    let text = format!("{vis}{}", method.name);
                    ctx.fill_text(&text, x + padding, cursor_y);
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                    cursor_y += member_font * 1.2;
                }
            } else if let Some(node) = ir_node.filter(|n| !n.members.is_empty()) {
                // ER entities get the SAME compartment treatment as classes (bd-rk14).
                //
                // Measured SVG vs canvas: `A { string name PK }` drew `name` and `PK` in the SVG and
                // NEITHER on the canvas, while `class_member` passed in the same probe — so the
                // canvas could already draw compartments and ER simply never got them. This is the
                // canvas twin of bd-ekx2, which was the identical gap in the terminal.
                //
                // The row text mirrors fm-render-svg and the terminal: key prefix, type, name, then
                // the comment when present, so all three renderers say the same thing.
                let line_h = self.config.font_size * 1.3;
                let member_font = self.config.font_size * 0.9;
                let padding = 6.0;

                ctx.set_fill_style(&self.config.label_color);
                ctx.set_text_baseline(TextBaseline::Middle);

                let entity_name = node
                    .label
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|l| l.text.as_str())
                    .unwrap_or(&node.id);

                let class_fonts = class_compartment_fonts
                    .get_or_insert_with(|| class_compartment_font_css(&self.config));
                ctx.set_font(class_fonts.0.as_str());
                ctx.set_text_align(TextAlign::Center);
                let mut cursor_y = y + line_h;
                ctx.fill_text(entity_name, x + w / 2.0, cursor_y);
                self.draw_calls += 1;
                *labels_drawn += 1;
                cursor_y += line_h * 0.5;

                ctx.begin_path();
                ctx.move_to(x, cursor_y);
                ctx.line_to(x + w, cursor_y);
                ctx.stroke();
                self.draw_calls += 1;
                cursor_y += member_font * 0.5;

                ctx.set_font(class_fonts.1.as_str());
                ctx.set_text_align(TextAlign::Left);
                for attr in &node.members {
                    if cursor_y > y + h - member_font * 0.5 {
                        // Out of box: stop rather than draw rows past the entity.
                        break;
                    }
                    let key_prefix = match attr.key {
                        fm_core::IrAttributeKey::Pk => "PK ",
                        fm_core::IrAttributeKey::Fk => "FK ",
                        fm_core::IrAttributeKey::Uk => "UK ",
                        fm_core::IrAttributeKey::None => "",
                    };
                    let mut text = String::with_capacity(
                        key_prefix.len() + attr.data_type.len() + attr.name.len() + 1,
                    );
                    text.push_str(key_prefix);
                    text.push_str(&attr.data_type);
                    text.push(' ');
                    text.push_str(&attr.name);
                    if let Some(comment) = attr.comment.as_deref().filter(|c| !c.is_empty()) {
                        text.push(' ');
                        text.push_str(comment);
                    }
                    ctx.fill_text(&text, x + padding, cursor_y);
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                    cursor_y += member_font * 1.2;
                }
            } else if let Some((node, meta)) = ir_node
                .and_then(|n| n.requirement_meta.as_deref().map(|m| (n, m)))
            {
                // REQUIREMENT rows (bd-rk14) — canvas twin of the terminal fix in bd-039t.
                // Measured: `requirement R { id: 1 / text: hello }` drew `hello` in the SVG and not
                // on the canvas. Field order matches the SVG's row order.
                let line_h = self.config.font_size * 1.3;
                let member_font = self.config.font_size * 0.9;
                let padding = 6.0;

                ctx.set_fill_style(&self.config.label_color);
                ctx.set_text_baseline(TextBaseline::Middle);
                let name = node
                    .label
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|l| l.text.as_str())
                    .unwrap_or(&node.id);
                let fonts = class_compartment_fonts
                    .get_or_insert_with(|| class_compartment_font_css(&self.config));
                ctx.set_font(fonts.0.as_str());
                ctx.set_text_align(TextAlign::Center);
                let mut cursor_y = y + line_h;
                ctx.fill_text(name, x + w / 2.0, cursor_y);
                self.draw_calls += 1;
                *labels_drawn += 1;
                cursor_y += line_h * 0.5;

                ctx.begin_path();
                ctx.move_to(x, cursor_y);
                ctx.line_to(x + w, cursor_y);
                ctx.stroke();
                self.draw_calls += 1;
                cursor_y += member_font * 0.5;

                ctx.set_font(fonts.1.as_str());
                ctx.set_text_align(TextAlign::Left);
                for (prefix, value) in [
                    ("id: ", meta.req_id.as_deref()),
                    ("text: ", meta.text.as_deref()),
                    ("risk: ", meta.risk.as_deref()),
                    ("verify: ", meta.verify_method.as_deref()),
                ] {
                    let Some(value) = value.filter(|v| !v.is_empty()) else {
                        continue;
                    };
                    if cursor_y > y + h - member_font * 0.5 {
                        break;
                    }
                    ctx.fill_text(&format!("{prefix}{value}"), x + padding, cursor_y);
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                    cursor_y += member_font * 1.2;
                }
            } else if let Some((node, meta)) =
                ir_node.and_then(|n| n.c4_meta.as_deref().map(|m| (n, m)))
            {
                // C4 type / technology / description (bd-rk14) — canvas twin of the bd-039t fix.
                // Measured: `Person(a, "Alice", "A user")` drew `A user` in the SVG and not on the
                // canvas. Decorations match fm-render-svg.
                let line_h = self.config.font_size * 1.3;
                let member_font = self.config.font_size * 0.9;
                let padding = 6.0;

                ctx.set_fill_style(&self.config.label_color);
                ctx.set_text_baseline(TextBaseline::Middle);
                let name = node
                    .label
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|l| l.text.as_str())
                    .unwrap_or(&node.id);
                let fonts = class_compartment_fonts
                    .get_or_insert_with(|| class_compartment_font_css(&self.config));
                ctx.set_font(fonts.0.as_str());
                ctx.set_text_align(TextAlign::Center);
                let mut cursor_y = y + line_h;
                ctx.fill_text(name, x + w / 2.0, cursor_y);
                self.draw_calls += 1;
                *labels_drawn += 1;
                cursor_y += line_h * 0.5;

                ctx.set_font(fonts.1.as_str());
                ctx.set_text_align(TextAlign::Left);
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
                    if cursor_y > y + h - member_font * 0.5 {
                        break;
                    }
                    ctx.fill_text(&text, x + padding, cursor_y);
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                    cursor_y += member_font * 1.2;
                }
            } else {
                // Standard single-label rendering.
                let label_text = ir_node
                    .and_then(|n| n.label)
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|l| l.text.as_str())
                    .or_else(|| ir_node.map(|n| n.id.as_str()))
                    .unwrap_or("");

                if !label_text.is_empty() {
                    // GANTT TASK NAMES honour the placement layout resolved for them (bd-t1jj).
                    //
                    // Centring a name on its bar is wrong for a gantt task whose name is wider than
                    // the bar: it overflows, and when the bar sits near the right edge the overflow
                    // leaves the canvas and the name is lost. `extensions.gantt_task_labels` already
                    // solves this -- layout resolves each task to Inside / OutsideRight / OutsideLeft
                    // and hands back an anchor, choosing OutsideLeft precisely when there is no room
                    // to the right. fm-render-svg consumes it, the terminal consumes it since
                    // b0c1ff1d, and canvas did not.
                    //
                    // The anchors map ONE-TO-ONE onto canvas text alignment, which is why this needs
                    // no arithmetic of its own and cannot drift from the SVG arm: OutsideRight is
                    // "start", OutsideLeft is "end", Inside is "middle" -- the exact three values the
                    // SVG writer uses.
                    let gantt = gantt_label_by_node
                        .as_ref()
                        .and_then(|map| map.get(&node_box.node_index));
                    let (cx, align) = match gantt {
                        Some(entry) => (
                            f64::from(entry.x) + offset_x,
                            match entry.placement {
                                fm_layout::GanttLabelPlacement::OutsideRight => TextAlign::Left,
                                fm_layout::GanttLabelPlacement::OutsideLeft => TextAlign::Right,
                                fm_layout::GanttLabelPlacement::Inside => TextAlign::Center,
                            },
                        ),
                        None => (x + w / 2.0, TextAlign::Center),
                    };
                    let cy = y + h / 2.0;

                    ctx.set_fill_style(&self.config.label_color);
                    ctx.set_font(
                        standard_label_font.get_or_insert_with(|| standard_node_font(&self.config)),
                    );
                    ctx.set_text_align(align);
                    ctx.set_text_baseline(TextBaseline::Middle);

                    // The overwhelmingly common single-line label draws `label_text` directly and never
                    // touches the split lines, so only COUNT them (no `Vec<&str>` alloc) to pick the
                    // branch; materialise the `Vec` only on the rare multi-line path. Byte-identical:
                    // `lines().count()` equals the old `collect().len()`.
                    if label_text.lines().count() <= 1 {
                        ctx.fill_text(label_text, cx, cy);
                        self.draw_calls += 1;
                        *labels_drawn += 1;
                    } else {
                        let lines: Vec<&str> = label_text.lines().collect();
                        let line_height = self.config.font_size * 1.2;
                        let total_height = lines.len() as f64 * line_height;
                        let start_y = cy - (total_height / 2.0) + (line_height / 2.0);

                        for (i, line) in lines.iter().enumerate() {
                            ctx.fill_text(line, cx, start_y + (i as f64) * line_height);
                            self.draw_calls += 1;
                            *labels_drawn += 1;
                        }
                    }
                }
            }

            count += 1;
        }

        count
    }

    /// Draw pie chart wedges using canvas arc API.
    fn draw_pie_wedges<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
        labels_drawn: &mut usize,
    ) {
        let Some(pie_meta) = &ir.pie_meta else {
            return;
        };
        if pie_meta.slices.is_empty() {
            return;
        }

        let total: f64 = pie_meta
            .slices
            .iter()
            .map(|s| f64::from(s.value.max(0.0)))
            .sum::<f64>()
            .max(f64::EPSILON);

        let bounds = &layout.bounds;
        let chart_size = f64::from(bounds.width.min(bounds.height));
        let cx = f64::from(bounds.x) + f64::from(bounds.width) / 2.0 + offset_x;
        let cy = f64::from(bounds.y) + f64::from(bounds.height) / 2.0 + offset_y;
        let radius = (chart_size / 2.0 - 36.0).max(30.0);

        let accent_colors = [
            "#4c78a8", "#f58518", "#e45756", "#72b7b2", "#54a24b", "#eeca3b", "#b279a2", "#ff9da6",
            "#9d755d", "#bab0ac",
        ];

        let mut angle = -std::f64::consts::FRAC_PI_2;

        // Invariant slice-label font, hoisted out of the per-slice loop (byte-identical to the per-slice
        // `format!("{}px {}", font_size*0.8, font_family)`).
        let slice_label_font = format!(
            "{}px {}",
            self.config.font_size * 0.8,
            self.config.font_family
        );

        for (i, slice) in pie_meta.slices.iter().enumerate() {
            let value = f64::from(slice.value.max(0.0));
            let sweep = (value / total) * 2.0 * std::f64::consts::PI;
            let color = accent_colors[i % accent_colors.len()];

            ctx.begin_path();
            ctx.move_to(cx, cy);
            ctx.arc(cx, cy, radius, angle, angle + sweep);
            ctx.close_path();
            ctx.set_fill_style(color);
            ctx.fill();
            ctx.set_stroke_style(&self.config.node_stroke);
            ctx.set_line_width(1.5);
            ctx.stroke();
            self.draw_calls += 1;

            // Draw percentage label.
            let mid_angle = angle + sweep / 2.0;
            let label_r = radius + 20.0;
            let lx = cx + label_r * mid_angle.cos();
            let ly = cy + label_r * mid_angle.sin();
            let pct = (value / total) * 100.0;
            let label = format!("{}: {pct:.1}%", slice.label);

            ctx.set_fill_style(&self.config.label_color);
            ctx.set_font(&slice_label_font);
            ctx.set_text_align(TextAlign::Center);
            ctx.set_text_baseline(TextBaseline::Middle);
            ctx.fill_text(&label, lx, ly);
            self.draw_calls += 1;
            *labels_drawn += 1;

            angle += sweep;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_marker_primitive<C: Canvas2dContext>(
    ctx: &mut C,
    marker: MarkerKind,
    x: f64,
    y: f64,
    angle: f64,
    node_fill: &str,
    stroke_color: &str,
) -> usize {
    match marker {
        MarkerKind::None => 0,
        MarkerKind::Circle => {
            draw_circle_marker(ctx, x, y, 4.0, node_fill, stroke_color);
            1
        }
        MarkerKind::Cross => {
            draw_cross_marker(ctx, x, y, 8.0, stroke_color);
            2
        }
        MarkerKind::Diamond => {
            draw_diamond_marker(ctx, x, y, angle, 10.0, Some(stroke_color), stroke_color);
            1
        }
        MarkerKind::DiamondOpen => {
            draw_diamond_marker(ctx, x, y, angle, 10.0, None, stroke_color);
            1
        }
        MarkerKind::TriangleOpen => {
            draw_open_triangle_marker(ctx, x, y, angle, 10.0, stroke_color);
            1
        }
        MarkerKind::TriangleOpenStart => {
            draw_open_triangle_marker(ctx, x, y, angle + std::f64::consts::PI, 10.0, stroke_color);
            1
        }
        MarkerKind::Arrow
        | MarkerKind::ThickArrow
        | MarkerKind::DottedArrow
        | MarkerKind::Open
        | MarkerKind::HalfArrowTop
        | MarkerKind::HalfArrowBottom
        | MarkerKind::StickArrowTop
        | MarkerKind::StickArrowBottom => {
            draw_arrowhead(ctx, x, y, angle, 10.0, stroke_color);
            1
        }
    }
}

fn path_marker_start_geometry(commands: &[PathCmd]) -> Option<(f64, f64, f64)> {
    let mut current = None;
    let mut subpath_start = None;

    for command in commands {
        match *command {
            PathCmd::MoveTo { x, y } => {
                let point = (f64::from(x), f64::from(y));
                current = Some(point);
                subpath_start = Some(point);
            }
            PathCmd::LineTo { x, y } => {
                let start = current?;
                let end = (f64::from(x), f64::from(y));
                return Some((start.0, start.1, angle_between(start, end)));
            }
            PathCmd::QuadTo { cx, cy, x, y } => {
                let start = current?;
                let control = (f64::from(cx), f64::from(cy));
                let end = (f64::from(x), f64::from(y));
                return Some((
                    start.0,
                    start.1,
                    angle_from_start_tangent(start, control, end),
                ));
            }
            PathCmd::CubicTo {
                c1x,
                c1y,
                c2x: _,
                c2y: _,
                x,
                y,
            } => {
                let start = current?;
                let control = (f64::from(c1x), f64::from(c1y));
                let end = (f64::from(x), f64::from(y));
                return Some((
                    start.0,
                    start.1,
                    angle_from_start_tangent(start, control, end),
                ));
            }
            PathCmd::Close => {
                current = subpath_start;
            }
        }
    }

    None
}

fn path_marker_end_geometry(commands: &[PathCmd]) -> Option<(f64, f64, f64)> {
    if matches!(commands.first(), Some(PathCmd::MoveTo { .. }))
        && let Some((last_command, preceding_commands)) = commands.split_last()
    {
        match *last_command {
            PathCmd::LineTo { x, y } => {
                let start = match preceding_commands.last().copied() {
                    Some(
                        PathCmd::MoveTo { x, y }
                        | PathCmd::LineTo { x, y }
                        | PathCmd::QuadTo { x, y, .. }
                        | PathCmd::CubicTo { x, y, .. },
                    ) => Some((f64::from(x), f64::from(y))),
                    Some(PathCmd::Close) | None => None,
                };
                if let Some(start) = start {
                    let end = (f64::from(x), f64::from(y));
                    return Some((end.0, end.1, angle_between(start, end)));
                }
            }
            PathCmd::QuadTo { cx, cy, x, y } => {
                let control = (f64::from(cx), f64::from(cy));
                let end = (f64::from(x), f64::from(y));
                if points_are_distinct(control, end) {
                    return Some((end.0, end.1, angle_between(control, end)));
                }
            }
            PathCmd::CubicTo { c2x, c2y, x, y, .. } => {
                let control = (f64::from(c2x), f64::from(c2y));
                let end = (f64::from(x), f64::from(y));
                if points_are_distinct(control, end) {
                    return Some((end.0, end.1, angle_between(control, end)));
                }
            }
            PathCmd::MoveTo { .. } | PathCmd::Close => {}
        }
    }

    let mut current = None;
    let mut subpath_start = None;
    let mut last = None;

    for command in commands {
        match *command {
            PathCmd::MoveTo { x, y } => {
                let point = (f64::from(x), f64::from(y));
                current = Some(point);
                subpath_start = Some(point);
            }
            PathCmd::LineTo { x, y } => {
                let start = current?;
                let end = (f64::from(x), f64::from(y));
                last = Some((end.0, end.1, angle_between(start, end)));
                current = Some(end);
            }
            PathCmd::QuadTo { cx, cy, x, y } => {
                let start = current?;
                let control = (f64::from(cx), f64::from(cy));
                let end = (f64::from(x), f64::from(y));
                last = Some((end.0, end.1, angle_from_end_tangent(start, control, end)));
                current = Some(end);
            }
            PathCmd::CubicTo {
                c1x: _,
                c1y: _,
                c2x,
                c2y,
                x,
                y,
            } => {
                let start = current?;
                let control = (f64::from(c2x), f64::from(c2y));
                let end = (f64::from(x), f64::from(y));
                last = Some((end.0, end.1, angle_from_end_tangent(start, control, end)));
                current = Some(end);
            }
            PathCmd::Close => {
                if let (Some(start), Some(end)) = (subpath_start, current) {
                    last = Some((start.0, start.1, angle_between(end, start)));
                    current = Some(start);
                }
            }
        }
    }

    last
}

fn angle_between(start: (f64, f64), end: (f64, f64)) -> f64 {
    (end.1 - start.1).atan2(end.0 - start.0)
}

fn angle_from_start_tangent(start: (f64, f64), control: (f64, f64), end: (f64, f64)) -> f64 {
    if points_are_distinct(start, control) {
        angle_between(start, control)
    } else {
        angle_between(start, end)
    }
}

fn angle_from_end_tangent(start: (f64, f64), control: (f64, f64), end: (f64, f64)) -> f64 {
    if points_are_distinct(control, end) {
        angle_between(control, end)
    } else {
        angle_between(start, end)
    }
}

fn points_are_distinct(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() > f64::EPSILON || (a.1 - b.1).abs() > f64::EPSILON
}

fn fragment_kind_label(kind: fm_core::FragmentKind) -> &'static str {
    match kind {
        fm_core::FragmentKind::Loop => "loop",
        fm_core::FragmentKind::Alt => "alt",
        fm_core::FragmentKind::Opt => "opt",
        fm_core::FragmentKind::Par => "par",
        fm_core::FragmentKind::Critical => "critical",
        fm_core::FragmentKind::Break => "break",
        fm_core::FragmentKind::Rect => "rect",
    }
}

#[inline]
fn legacy_edge_stroke(arrow: ArrowType, default_width: f64) -> (f64, &'static [f64]) {
    match arrow {
        ArrowType::ThickArrow => (2.5, &[]),
        ArrowType::DottedArrow => (1.5, &LEGACY_DOTTED_EDGE_DASH),
        _ => (default_width, &[]),
    }
}

const fn legacy_uml_markers(arrow: ArrowType) -> Option<(MarkerKind, MarkerKind)> {
    match arrow {
        ArrowType::Aggregation => Some((MarkerKind::DiamondOpen, MarkerKind::None)),
        ArrowType::AggregationReverse => Some((MarkerKind::None, MarkerKind::DiamondOpen)),
        ArrowType::Composition => Some((MarkerKind::Diamond, MarkerKind::None)),
        ArrowType::CompositionReverse => Some((MarkerKind::None, MarkerKind::Diamond)),
        ArrowType::Inheritance => Some((MarkerKind::TriangleOpenStart, MarkerKind::None)),
        ArrowType::InheritanceReverse => Some((MarkerKind::None, MarkerKind::TriangleOpen)),
        _ => None,
    }
}

#[inline]
fn with_canvas_dash_f64<T>(dash: &[f32], use_dash: impl FnOnce(&[f64]) -> T) -> T {
    if let [first, second] = dash {
        use_dash(&[f64::from(*first), f64::from(*second)])
    } else {
        let converted: Vec<f64> = dash.iter().copied().map(f64::from).collect();
        use_dash(&converted)
    }
}

fn class_vis_char(vis: fm_core::ClassVisibility) -> char {
    match vis {
        fm_core::ClassVisibility::Public => '+',
        fm_core::ClassVisibility::Private => '-',
        fm_core::ClassVisibility::Protected => '#',
        fm_core::ClassVisibility::Package => '~',
    }
}

fn standard_node_font(config: &CanvasRenderConfig) -> String {
    format!("{}px {}", config.font_size, config.font_family)
}

fn secondary_label_font_css(config: &CanvasRenderConfig) -> String {
    format!("{}px {}", config.font_size * 0.85, config.font_family)
}

fn sequence_fragment_font_css(config: &CanvasRenderConfig) -> String {
    format!("bold {}px {}", config.font_size * 0.8, config.font_family)
}

fn class_compartment_font_css(config: &CanvasRenderConfig) -> (String, String) {
    (
        format!("bold {}px {}", config.font_size, config.font_family),
        format!("{}px {}", config.font_size * 0.9, config.font_family),
    )
}

fn generic_canvas_diagram_title(ir: &MermaidDiagramIr) -> Option<&str> {
    // The canvas backend has dedicated pie chart rendering. Gantt/xy/quadrant still use
    // the generic node/edge path, so the generic title fallback remains enabled.
    ir.meta.title.as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{DrawOperation, MockCanvas2dContext};
    use fm_core::{DiagramType, IrEdge, IrEndpoint, IrNode, IrNodeId};
    use fm_layout::{
        LayoutActivationBar, LayoutEdgePath, LayoutExtensions, LayoutRect, LayoutStats,
        build_render_scene, layout_diagram,
    };

    fn owned_legacy_edge_stroke_reference(
        arrow: ArrowType,
        default_width: f64,
    ) -> (f64, Option<Vec<f64>>) {
        match arrow {
            ArrowType::ThickArrow => (2.5, None),
            ArrowType::DottedArrow => (1.5, Some(vec![5.0, 5.0])),
            _ => (default_width, None),
        }
    }

    fn geometry_bits(geometry: (f64, f64, f64)) -> (u64, u64, u64) {
        (
            geometry.0.to_bits(),
            geometry.1.to_bits(),
            geometry.2.to_bits(),
        )
    }

    fn marker_operations(marker: MarkerKind) -> Vec<DrawOperation> {
        let mut ctx = MockCanvas2dContext::new(120.0, 40.0);
        assert_eq!(
            draw_marker_primitive(&mut ctx, marker, 50.0, 20.0, 0.0, "#ffffff", "#112233",),
            1
        );
        ctx.operations().to_vec()
    }

    fn legacy_uml_edge_operations(arrow: ArrowType) -> Vec<DrawOperation> {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Class);
        ir.nodes.push(IrNode {
            id: "Owner".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "Part".to_string(),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow,
            ..Default::default()
        });

        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: vec![LayoutEdgePath {
                edge_index: 0,
                span: Default::default(),
                points: [
                    fm_layout::LayoutPoint { x: 10.0, y: 20.0 },
                    fm_layout::LayoutPoint { x: 110.0, y: 20.0 },
                ]
                .into_iter()
                .collect(),
                reversed: false,
                is_self_loop: false,
                parallel_offset: 0.0,
                bundle_count: 1,
                bundled: false,
            }],
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 40.0,
            },
            stats: LayoutStats::default(),
            extensions: LayoutExtensions::default(),
            dirty_regions: Vec::new(),
        };

        let config = CanvasRenderConfig {
            auto_fit: false,
            padding: 0.0,
            ..Default::default()
        };
        let mut ctx = MockCanvas2dContext::new(120.0, 40.0);
        let mut renderer = Canvas2dRenderer::new(config);
        let mut labels_drawn = 0;
        assert_eq!(
            renderer.draw_edges(&layout, &ir, &mut ctx, 0.0, 0.0, &mut labels_drawn),
            1
        );
        ctx.operations().to_vec()
    }

    #[test]
    fn canvas_uml_marker_primitives_preserve_geometry_fill_and_orientation() {
        let composition = marker_operations(MarkerKind::Diamond);
        assert!(
            composition
                .iter()
                .any(|operation| matches!(operation, DrawOperation::Fill))
        );
        assert!(
            !composition
                .iter()
                .any(|operation| matches!(operation, DrawOperation::Stroke))
        );
        assert_eq!(
            composition
                .iter()
                .filter(|operation| matches!(operation, DrawOperation::LineTo(_, _)))
                .count(),
            3
        );
        assert!(
            composition
                .iter()
                .any(|operation| matches!(operation, DrawOperation::LineTo(x, y)
                if (*x + 10.0).abs() < 0.001 && y.abs() < 0.001))
        );

        let aggregation = marker_operations(MarkerKind::DiamondOpen);
        assert!(
            !aggregation
                .iter()
                .any(|operation| matches!(operation, DrawOperation::Fill))
        );
        assert!(
            aggregation
                .iter()
                .any(|operation| matches!(operation, DrawOperation::Stroke))
        );
        assert_eq!(
            aggregation
                .iter()
                .filter(|operation| matches!(operation, DrawOperation::LineTo(_, _)))
                .count(),
            3
        );

        let inheritance_end = marker_operations(MarkerKind::TriangleOpen);
        assert!(
            !inheritance_end
                .iter()
                .any(|operation| matches!(operation, DrawOperation::Fill))
        );
        assert!(
            inheritance_end
                .iter()
                .any(|operation| matches!(operation, DrawOperation::Stroke))
        );
        assert_eq!(
            inheritance_end
                .iter()
                .filter(|operation| matches!(operation, DrawOperation::LineTo(_, _)))
                .count(),
            2
        );
        assert!(
            inheritance_end
                .iter()
                .any(|operation| matches!(operation, DrawOperation::Rotate(angle)
                if angle.abs() < f64::EPSILON))
        );

        let inheritance_start = marker_operations(MarkerKind::TriangleOpenStart);
        assert!(
            inheritance_start
                .iter()
                .any(|operation| matches!(operation, DrawOperation::Rotate(angle)
                if (*angle - std::f64::consts::PI).abs() < 0.001))
        );
    }

    #[test]
    fn legacy_canvas_places_uml_markers_on_the_owning_endpoint() {
        for arrow in [
            ArrowType::Aggregation,
            ArrowType::Composition,
            ArrowType::Inheritance,
        ] {
            let operations = legacy_uml_edge_operations(arrow);
            assert!(operations.iter().any(
                |operation| matches!(operation, DrawOperation::Translate(x, y)
                    if (*x - 10.0).abs() < 0.001 && (*y - 20.0).abs() < 0.001)
            ));
            assert!(!operations.iter().any(
                |operation| matches!(operation, DrawOperation::Translate(x, y)
                    if (*x - 110.0).abs() < 0.001 && (*y - 20.0).abs() < 0.001)
            ));
        }

        for arrow in [
            ArrowType::AggregationReverse,
            ArrowType::CompositionReverse,
            ArrowType::InheritanceReverse,
        ] {
            let operations = legacy_uml_edge_operations(arrow);
            assert!(!operations.iter().any(
                |operation| matches!(operation, DrawOperation::Translate(x, y)
                    if (*x - 10.0).abs() < 0.001 && (*y - 20.0).abs() < 0.001)
            ));
            assert!(operations.iter().any(
                |operation| matches!(operation, DrawOperation::Translate(x, y)
                    if (*x - 110.0).abs() < 0.001 && (*y - 20.0).abs() < 0.001)
            ));
        }
    }

    #[test]
    fn marker_end_geometry_preserves_tail_and_fallback_contracts() {
        let cubic = [
            PathCmd::MoveTo { x: 1.0, y: 2.0 },
            PathCmd::CubicTo {
                c1x: 2.0,
                c1y: 3.0,
                c2x: 4.0,
                c2y: 5.0,
                x: 6.0,
                y: 8.0,
            },
            PathCmd::CubicTo {
                c1x: 7.0,
                c1y: 9.0,
                c2x: 10.0,
                c2y: 11.0,
                x: 13.0,
                y: 15.0,
            },
        ];
        assert_eq!(
            geometry_bits(path_marker_end_geometry(&cubic).expect("cubic geometry")),
            geometry_bits((13.0, 15.0, angle_between((10.0, 11.0), (13.0, 15.0))))
        );

        let quad = [
            PathCmd::MoveTo { x: 1.0, y: 2.0 },
            PathCmd::QuadTo {
                cx: 4.0,
                cy: 5.0,
                x: 7.0,
                y: 9.0,
            },
        ];
        assert_eq!(
            geometry_bits(path_marker_end_geometry(&quad).expect("quadratic geometry")),
            geometry_bits((7.0, 9.0, angle_between((4.0, 5.0), (7.0, 9.0))))
        );

        let line = [
            PathCmd::MoveTo { x: 1.0, y: 2.0 },
            PathCmd::LineTo { x: 7.0, y: 9.0 },
        ];
        assert_eq!(
            geometry_bits(path_marker_end_geometry(&line).expect("line geometry")),
            geometry_bits((7.0, 9.0, angle_between((1.0, 2.0), (7.0, 9.0))))
        );

        let degenerate_cubic = [
            PathCmd::MoveTo { x: 1.0, y: 2.0 },
            PathCmd::LineTo { x: 3.0, y: 4.0 },
            PathCmd::CubicTo {
                c1x: 5.0,
                c1y: 6.0,
                c2x: 8.0,
                c2y: 9.0,
                x: 8.0,
                y: 9.0,
            },
        ];
        assert_eq!(
            geometry_bits(
                path_marker_end_geometry(&degenerate_cubic).expect("degenerate cubic geometry")
            ),
            geometry_bits((8.0, 9.0, angle_between((3.0, 4.0), (8.0, 9.0))))
        );

        let closed = [
            PathCmd::MoveTo { x: 1.0, y: 2.0 },
            PathCmd::LineTo { x: 3.0, y: 4.0 },
            PathCmd::Close,
        ];
        assert_eq!(
            geometry_bits(path_marker_end_geometry(&closed).expect("closed geometry")),
            geometry_bits((1.0, 2.0, angle_between((3.0, 4.0), (1.0, 2.0))))
        );

        let trailing_move = [
            PathCmd::MoveTo { x: 1.0, y: 2.0 },
            PathCmd::LineTo { x: 3.0, y: 4.0 },
            PathCmd::MoveTo { x: 8.0, y: 9.0 },
        ];
        assert_eq!(
            geometry_bits(
                path_marker_end_geometry(&trailing_move).expect("trailing move geometry")
            ),
            geometry_bits((3.0, 4.0, angle_between((1.0, 2.0), (3.0, 4.0))))
        );

        let malformed = [
            PathCmd::LineTo { x: 1.0, y: 2.0 },
            PathCmd::MoveTo { x: 3.0, y: 4.0 },
            PathCmd::LineTo { x: 5.0, y: 6.0 },
        ];
        assert!(path_marker_end_geometry(&malformed).is_none());
    }

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
    fn standard_node_font_preserves_canvas_css_format() {
        for (font_size, font_family, expected) in [
            (0.0, "sans-serif", "0px sans-serif"),
            (12.5, "Test Sans, serif", "12.5px Test Sans, serif"),
            (14.0, "'Inter', Arial", "14px 'Inter', Arial"),
        ] {
            let config = CanvasRenderConfig {
                font_size,
                font_family: font_family.to_owned(),
                ..CanvasRenderConfig::default()
            };
            assert_eq!(standard_node_font(&config), expected);
        }
    }

    #[test]
    fn secondary_label_font_preserves_canvas_css_format() {
        for (font_size, font_family, expected) in [
            (0.0, "sans-serif", "0px sans-serif"),
            (10.0, "Test Sans", "8.5px Test Sans"),
            (20.0, "'Inter', Arial", "17px 'Inter', Arial"),
        ] {
            let config = CanvasRenderConfig {
                font_size,
                font_family: font_family.to_owned(),
                ..CanvasRenderConfig::default()
            };
            assert_eq!(secondary_label_font_css(&config), expected);
        }
    }

    #[test]
    fn sequence_fragment_font_preserves_canvas_css_format() {
        for (font_size, font_family, expected) in [
            (0.0, "sans-serif", "bold 0px sans-serif"),
            (10.0, "Test Sans", "bold 8px Test Sans"),
            (20.0, "'Inter', Arial", "bold 16px 'Inter', Arial"),
        ] {
            let config = CanvasRenderConfig {
                font_size,
                font_family: font_family.to_owned(),
                ..CanvasRenderConfig::default()
            };
            assert_eq!(sequence_fragment_font_css(&config), expected);
        }
    }

    #[test]
    fn class_compartment_fonts_preserve_canvas_css_format() {
        for (font_size, font_family, expected) in [
            (0.0, "sans-serif", ("bold 0px sans-serif", "0px sans-serif")),
            (
                12.5,
                "Test Sans, serif",
                ("bold 12.5px Test Sans, serif", "11.25px Test Sans, serif"),
            ),
            (
                20.0,
                "'Inter', Arial",
                ("bold 20px 'Inter', Arial", "18px 'Inter', Arial"),
            ),
        ] {
            let config = CanvasRenderConfig {
                font_size,
                font_family: font_family.to_owned(),
                ..CanvasRenderConfig::default()
            };
            assert_eq!(
                class_compartment_font_css(&config),
                (expected.0.to_owned(), expected.1.to_owned())
            );
        }
    }

    #[test]
    fn canvas_dash_conversion_preserves_order_and_float_bits() {
        let corpus: &[&[f32]] = &[&[], &[6.0, 4.0], &[3.25], &[1.0, 2.0, 3.0, 4.0, 5.0]];
        for &values in corpus {
            let expected: Vec<u64> = values
                .iter()
                .copied()
                .map(f64::from)
                .map(f64::to_bits)
                .collect();
            let actual = with_canvas_dash_f64(values, |converted| {
                converted
                    .iter()
                    .copied()
                    .map(f64::to_bits)
                    .collect::<Vec<_>>()
            });
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn legacy_edge_stroke_borrow_preserves_branch_mappings() {
        let default_width = 1.25;
        for arrow in [
            ArrowType::ThickArrow,
            ArrowType::DottedArrow,
            ArrowType::Arrow,
            ArrowType::DottedLine,
            ArrowType::DoubleDottedArrow,
        ] {
            let (expected_width, expected_dash) =
                owned_legacy_edge_stroke_reference(arrow, default_width);
            let (actual_width, actual_dash) = legacy_edge_stroke(arrow, default_width);
            assert_eq!(
                actual_width.to_bits(),
                expected_width.to_bits(),
                "{arrow:?}"
            );
            assert_eq!(
                actual_dash
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                expected_dash
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                "{arrow:?}"
            );
        }
    }

    #[test]
    #[ignore = "foreground release-only performance probe"]
    fn legacy_dotted_edge_dash_borrow_perf_ab() {
        use sha2::{Digest, Sha256};
        use std::hint::black_box;
        use std::time::Instant;

        const ROUNDS: usize = 41;
        const MIN_OF: u32 = 3;
        const MIN_SAMPLE_NS: u64 = 2_000_000;

        #[derive(Clone, Copy)]
        enum Arm {
            Owned,
            Borrowed,
        }

        struct Stats {
            a_p50_ns: f64,
            b_p50_ns: f64,
            ratio_p50: f64,
            ratio_ci: (f64, f64),
            cv_pct: f64,
            mad_pct: f64,
            checksum: u64,
        }

        fn self_identity() -> String {
            use std::fmt::Write as _;

            let Ok(path) = std::env::current_exe() else {
                return "unavailable".to_owned();
            };
            let Ok(bytes) = std::fs::read(&path) else {
                return "unavailable".to_owned();
            };
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let digest = hasher.finalize();
            let mut sha256 = String::with_capacity(digest.len() * 2);
            for byte in digest {
                write!(sha256, "{byte:02x}").expect("writing to String cannot fail");
            }
            format!("{} ({} bytes) {}", sha256, bytes.len(), path.display())
        }

        fn median(values: &mut [f64]) -> f64 {
            values.sort_by(f64::total_cmp);
            let middle = values.len() / 2;
            if values.len().is_multiple_of(2) {
                f64::midpoint(values[middle - 1], values[middle])
            } else {
                values[middle]
            }
        }

        fn bootstrap_median_ci(ratios: &[f64]) -> (f64, f64) {
            const RESAMPLES: usize = 2_000;
            let mut state = 0x2545_F491_4F6C_DD1D_u64;
            let mut medians = Vec::with_capacity(RESAMPLES);
            let mut sample = vec![0.0_f64; ratios.len()];
            for _ in 0..RESAMPLES {
                for slot in &mut sample {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    let index = usize::try_from(state >> 33).unwrap_or(0) % ratios.len();
                    *slot = ratios[index];
                }
                medians.push(median(&mut sample));
            }
            medians.sort_by(f64::total_cmp);
            (
                medians[RESAMPLES / 40],
                medians[RESAMPLES - 1 - RESAMPLES / 40],
            )
        }

        fn digest(width: f64, dash: &[f64]) -> u64 {
            dash.iter().fold(width.to_bits(), |state, value| {
                state.rotate_left(9) ^ value.to_bits()
            })
        }

        fn time_arm(arm: Arm, iterations: u32) -> (u64, u64) {
            let mut checksum = 0_u64;
            let start = Instant::now();
            for _ in 0..iterations.max(1) {
                match arm {
                    Arm::Owned => {
                        let (width, dash) = owned_legacy_edge_stroke_reference(
                            black_box(ArrowType::DottedArrow),
                            black_box(1.25),
                        );
                        let dash = black_box(dash);
                        checksum = checksum
                            .wrapping_add(digest(width, dash.as_deref().unwrap_or(black_box(&[]))));
                    }
                    Arm::Borrowed => {
                        let (width, dash) =
                            legacy_edge_stroke(black_box(ArrowType::DottedArrow), black_box(1.25));
                        checksum = checksum.wrapping_add(digest(width, black_box(dash)));
                    }
                }
            }
            (
                u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX),
                black_box(checksum),
            )
        }

        fn time_min(arm: Arm, iterations: u32) -> (u64, u64) {
            let mut best = u64::MAX;
            let mut checksum = 0_u64;
            for _ in 0..MIN_OF {
                let (elapsed, digest) = time_arm(arm, iterations);
                best = best.min(elapsed);
                checksum = checksum.wrapping_add(digest);
            }
            (best, checksum)
        }

        fn calibrate() -> u32 {
            let mut iterations = 1_024_u32;
            loop {
                let (elapsed, _) = time_arm(Arm::Borrowed, iterations);
                if elapsed >= MIN_SAMPLE_NS || iterations >= 1 << 28 {
                    return iterations;
                }
                iterations = iterations.saturating_mul(2);
            }
        }

        fn paired(arm_a: Arm, arm_b: Arm, iterations: u32) -> Stats {
            let mut a_samples = Vec::with_capacity(ROUNDS);
            let mut b_samples = Vec::with_capacity(ROUNDS);
            let mut ratios = Vec::with_capacity(ROUNDS);
            let mut checksum = 0_u64;
            for round in 0..ROUNDS {
                let (a_ns, b_ns, a_digest, b_digest) = if round.is_multiple_of(2) {
                    let (a_ns, a_digest) = time_min(arm_a, iterations);
                    let (b_ns, b_digest) = time_min(arm_b, iterations);
                    (a_ns, b_ns, a_digest, b_digest)
                } else {
                    let (b_ns, b_digest) = time_min(arm_b, iterations);
                    let (a_ns, a_digest) = time_min(arm_a, iterations);
                    (a_ns, b_ns, a_digest, b_digest)
                };
                checksum = checksum.wrapping_add(a_digest).wrapping_add(b_digest);
                a_samples.push(a_ns as f64);
                b_samples.push(b_ns as f64);
                ratios.push(a_ns as f64 / b_ns.max(1) as f64);
            }
            let a_p50_ns = median(&mut a_samples);
            let b_p50_ns = median(&mut b_samples);
            let ratio_p50 = median(&mut ratios.clone());
            let ratio_ci = bootstrap_median_ci(&ratios);
            let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
            let variance = ratios
                .iter()
                .map(|ratio| (ratio - mean).powi(2))
                .sum::<f64>()
                / ratios.len() as f64;
            let mut deviations = ratios
                .iter()
                .map(|ratio| (ratio - ratio_p50).abs())
                .collect::<Vec<_>>();
            Stats {
                a_p50_ns,
                b_p50_ns,
                ratio_p50,
                ratio_ci,
                cv_pct: variance.sqrt() / mean * 100.0,
                mad_pct: median(&mut deviations) / ratio_p50 * 100.0,
                checksum,
            }
        }

        println!("bench_elf_sha256={}", self_identity());
        legacy_edge_stroke_borrow_preserves_branch_mappings();

        let iterations = calibrate();
        let null = paired(Arm::Owned, Arm::Owned, iterations);
        let real = paired(Arm::Owned, Arm::Borrowed, iterations);
        let null_half_width = (null.ratio_ci.0 - 1.0)
            .abs()
            .max((null.ratio_ci.1 - 1.0).abs());
        let ci_margin = if null_half_width > 0.0 {
            (real.ratio_p50 - 1.0).abs() / null_half_width
        } else {
            f64::INFINITY
        };
        let decidable = ci_margin >= 2.0;
        let verdict = if !decidable {
            "INDETERMINATE"
        } else if real.ratio_p50 > 1.0 {
            "CAND_FASTER"
        } else {
            "CAND_SLOWER"
        };
        println!(
            "PERF legacy_canvas_dotted_dash null_ratio={:.6} \
             null_ci95=[{:.6},{:.6}] ab_ratio={:.6} ab_ci95=[{:.6},{:.6}] \
             ci_margin={ci_margin:.2}x verdict={verdict} baseline_p50_ns={:.0} \
             candidate_p50_ns={:.0} null_cv={:.2}% null_mad={:.2}% ab_cv={:.2}% \
             ab_mad={:.2}% parity=exact checksum={} iterations={iterations} min_of={MIN_OF} \
             rounds={ROUNDS}",
            null.ratio_p50,
            null.ratio_ci.0,
            null.ratio_ci.1,
            real.ratio_p50,
            real.ratio_ci.0,
            real.ratio_ci.1,
            real.a_p50_ns,
            real.b_p50_ns,
            null.cv_pct,
            null.mad_pct,
            real.cv_pct,
            real.mad_pct,
            null.checksum.wrapping_add(real.checksum),
        );
    }

    #[test]
    fn source_index_set_counts_dense_duplicates_and_sparse_indexes() {
        let mut indexes = SourceIndexSet::default();
        for index in [
            0,
            0,
            63,
            64,
            DENSE_SOURCE_INDEX_LIMIT - 1,
            DENSE_SOURCE_INDEX_LIMIT,
            usize::MAX,
            usize::MAX,
        ] {
            indexes.insert(index);
        }
        assert_eq!(indexes.len(), 6);
        assert_eq!(indexes.words.len(), DENSE_SOURCE_INDEX_LIMIT / 64);
        assert_eq!(indexes.sparse.len(), 2);
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

    #[test]
    fn render_scene_draws_expected_sources() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.labels.push(fm_core::IrLabel {
            text: "A".to_string(),
            ..Default::default()
        });
        ir.labels.push(fm_core::IrLabel {
            text: "B".to_string(),
            ..Default::default()
        });
        ir.nodes.push(fm_core::IrNode {
            id: "A".to_string(),
            label: Some(fm_core::IrLabelId(0)),
            ..Default::default()
        });
        ir.nodes.push(fm_core::IrNode {
            id: "B".to_string(),
            label: Some(fm_core::IrLabelId(1)),
            ..Default::default()
        });
        ir.edges.push(fm_core::IrEdge {
            from: fm_core::IrEndpoint::Node(fm_core::IrNodeId(0)),
            to: fm_core::IrEndpoint::Node(fm_core::IrNodeId(1)),
            arrow: fm_core::ArrowType::Arrow,
            ..Default::default()
        });

        let layout = layout_diagram(&ir);
        let scene = build_render_scene(&ir, &layout);
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());

        let result = renderer.render_scene(&scene, &mut ctx);
        assert_eq!(result.nodes_drawn, 2);
        assert_eq!(result.edges_drawn, 1);
        assert!(result.labels_drawn >= 2);
        assert!(ctx.operation_count() > 1);
        assert!(
            ctx.operations()
                .iter()
                .any(|operation| matches!(operation, DrawOperation::FillText(_, _, _)))
        );
    }

    #[test]
    fn render_scene_draws_path_markers() {
        let scene = RenderScene {
            bounds: fm_layout::RenderRect {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 40.0,
            },
            root: RenderGroup {
                id: None,
                source: RenderSource::Diagram,
                transform: None,
                clip: None,
                children: vec![RenderItem::Path(RenderPath {
                    source: RenderSource::Edge(0),
                    commands: vec![
                        PathCmd::MoveTo { x: 10.0, y: 20.0 },
                        PathCmd::LineTo { x: 110.0, y: 20.0 },
                    ],
                    fill: None,
                    stroke: Some(StrokeStyle::solid("#112233", 2.0)),
                    marker_start: MarkerKind::Circle,
                    marker_end: MarkerKind::Arrow,
                })],
            },
        };

        let config = CanvasRenderConfig {
            auto_fit: false,
            padding: 0.0,
            ..Default::default()
        };
        let mut ctx = MockCanvas2dContext::new(120.0, 40.0);
        let mut renderer = Canvas2dRenderer::new(config);

        let result = renderer.render_scene(&scene, &mut ctx);

        assert_eq!(result.edges_drawn, 1);
        assert!(
            ctx.operations()
                .iter()
                .any(|operation| matches!(operation, DrawOperation::Arc(x, y, radius, _, _)
                    if (*x - 10.0).abs() < 0.001 && (*y - 20.0).abs() < 0.001 && (*radius - 4.0).abs() < 0.001))
        );
        assert!(ctx.operations().iter().any(
            |operation| matches!(operation, DrawOperation::Translate(x, y)
                    if (*x - 110.0).abs() < 0.001 && (*y - 20.0).abs() < 0.001)
        ));
    }

    #[test]
    fn render_draws_generic_diagram_title() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.meta.title = Some("Shipping History".to_string());
        ir.nodes.push(fm_core::IrNode {
            id: "A".to_string(),
            ..Default::default()
        });

        let layout = layout_diagram(&ir);
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());

        let _result = renderer.render(&layout, &ir, &mut ctx);

        assert!(ctx.operations().iter().any(
            |operation| matches!(operation, DrawOperation::FillText(text, _, _)
                if text == "Shipping History")
        ));
    }

    #[test]
    fn render_draws_chart_title_via_generic_canvas_title_path() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Pie);
        ir.meta.title = Some("Revenue".to_string());
        ir.pie_meta = Some(fm_core::IrPieMeta {
            title: Some("Revenue".to_string()),
            slices: vec![fm_core::IrPieSlice {
                label: "A".to_string(),
                value: 1.0,
            }],
            ..Default::default()
        });
        ir.nodes.push(fm_core::IrNode {
            id: "slice_a".to_string(),
            ..Default::default()
        });

        let layout = layout_diagram(&ir);
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());

        let _result = renderer.render(&layout, &ir, &mut ctx);

        assert!(ctx.operations().iter().any(
            |operation| matches!(operation, DrawOperation::FillText(text, _, _)
                if text == "Revenue")
        ));
    }

    #[test]
    fn render_counts_generic_diagram_title_as_label() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.meta.title = Some("Shipping History".to_string());
        ir.nodes.push(fm_core::IrNode {
            id: "A".to_string(),
            ..Default::default()
        });

        let layout = layout_diagram(&ir);
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());

        let result = renderer.render(&layout, &ir, &mut ctx);

        assert!(result.labels_drawn >= 1);
    }

    #[test]
    fn edge_label_background_uses_edge_label_font_metrics() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.labels.push(fm_core::IrLabel {
            text: "A".to_string(),
            ..Default::default()
        });
        ir.labels.push(fm_core::IrLabel {
            text: "B".to_string(),
            ..Default::default()
        });
        ir.labels.push(fm_core::IrLabel {
            text: "Wide label text".to_string(),
            ..Default::default()
        });
        ir.nodes.push(fm_core::IrNode {
            id: "A".to_string(),
            label: Some(fm_core::IrLabelId(0)),
            ..Default::default()
        });
        ir.nodes.push(fm_core::IrNode {
            id: "B".to_string(),
            label: Some(fm_core::IrLabelId(1)),
            ..Default::default()
        });
        ir.edges.push(fm_core::IrEdge {
            from: fm_core::IrEndpoint::Node(fm_core::IrNodeId(0)),
            to: fm_core::IrEndpoint::Node(fm_core::IrNodeId(1)),
            label: Some(fm_core::IrLabelId(2)),
            ..Default::default()
        });

        let layout = layout_diagram(&ir);
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());

        let _result = renderer.render(&layout, &ir, &mut ctx);

        let label_background_width = ctx
            .operations()
            .iter()
            .find_map(|operation| match operation {
                DrawOperation::FillRect(_, _, width, _) if *width > 40.0 => Some(*width),
                _ => None,
            })
            .expect("expected edge label background rectangle");

        let expected_width = "Wide label text".len() as f64
            * (CanvasRenderConfig::default().font_size * 0.85 * 0.57)
            + 8.0;
        assert!((label_background_width - expected_width).abs() < 1.0);
    }

    #[test]
    fn render_draws_activation_bar_rectangles() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
            stats: LayoutStats::default(),
            extensions: LayoutExtensions {
                activation_bars: vec![LayoutActivationBar {
                    participant_index: 0,
                    depth: 0,
                    bounds: LayoutRect {
                        x: 10.0,
                        y: 20.0,
                        width: 8.0,
                        height: 30.0,
                    },
                }],
                ..Default::default()
            },
            dirty_regions: Vec::new(),
        };
        let config = CanvasRenderConfig {
            auto_fit: false,
            padding: 0.0,
            ..Default::default()
        };
        let mut ctx = MockCanvas2dContext::new(200.0, 200.0);
        let mut renderer = Canvas2dRenderer::new(config);

        let _result = renderer.render(&layout, &ir, &mut ctx);

        assert!(ctx.operations().iter().any(|operation| {
            matches!(operation, DrawOperation::FillRect(x, y, w, h)
                if (*x - 10.0).abs() < 0.001
                    && (*y - 20.0).abs() < 0.001
                    && (*w - 8.0).abs() < 0.001
                    && (*h - 30.0).abs() < 0.001)
        }));
        assert!(ctx.operations().iter().any(|operation| {
            matches!(operation, DrawOperation::StrokeRect(x, y, w, h)
                if (*x - 10.0).abs() < 0.001
                    && (*y - 20.0).abs() < 0.001
                    && (*w - 8.0).abs() < 0.001
                    && (*h - 30.0).abs() < 0.001)
        }));
    }

    #[test]
    fn render_draws_sequence_origin_cluster_titles() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: vec![fm_layout::LayoutClusterBox {
                cluster_index: 0,
                span: Default::default(),
                title: Some("Backend".to_string()),
                color: None,
                bounds: LayoutRect {
                    x: 5.0,
                    y: 10.0,
                    width: 100.0,
                    height: 60.0,
                },
            }],
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 100.0,
            },
            stats: LayoutStats::default(),
            extensions: LayoutExtensions::default(),
            dirty_regions: Vec::new(),
        };
        let config = CanvasRenderConfig {
            auto_fit: false,
            padding: 0.0,
            ..Default::default()
        };
        let mut ctx = MockCanvas2dContext::new(200.0, 200.0);
        let mut renderer = Canvas2dRenderer::new(config);

        let _result = renderer.render(&layout, &ir, &mut ctx);

        assert!(ctx.operations().iter().any(|operation| {
            matches!(operation, DrawOperation::FillText(text, x, y)
                if text == "Backend" && (*x - 13.0).abs() < 0.001 && (*y - 14.0).abs() < 0.001)
        }));
    }
    /// A destroyed participant's lifeline must be terminated with a CROSS on the canvas backend
    /// (bd-t1jj).
    ///
    /// `extensions.sequence_lifecycle_markers` is filled by the sequence layout arm and drawn by
    /// fm-render-svg; this renderer referenced it nowhere. Canvas is not a dead surface -- fm-wasm
    /// renders the browser preview through `render_to_canvas_with_layout` -- so `destroy Bob` gave a
    /// lifeline that simply stopped, with nothing distinguishing "destroyed" from "idle". Those are
    /// different diagrams.
    ///
    /// ASSERTED BY GEOMETRY, NOT BY OP COUNT. The cross is two diagonals of `marker.size`, so the op
    /// stream must contain a `MoveTo -> LineTo` with delta `(+size, +size)` and one with
    /// `(-size, +size)`. Those deltas are independent of the canvas offsets, so this pins the SHAPE
    /// the renderer draws rather than how many calls it happened to make -- an op-count assertion
    /// would pass on any two lines drawn anywhere.
    #[test]
    fn canvas_draws_a_destroy_cross_for_a_destroyed_participant() {
        let ir = fm_parser::parse(
            "sequenceDiagram\n  participant Alice\n  participant Bob\n  Alice->>Bob: Hi\n  destroy Bob\n  Bob->>Alice: Bye\n",
        )
        .ir;
        let layout = fm_layout::layout_diagram(&ir);

        // NON-VACUITY: the layout must actually publish a marker, or this test asserts nothing about
        // the renderer and would pass on a diagram with nothing to draw.
        let marker = layout
            .extensions
            .sequence_lifecycle_markers
            .first()
            .expect(
                "CONTROL FAILED: this source produced no lifecycle marker, so the renderer has \
                 nothing to draw and this test cannot detect the defect it was written for",
            );
        let size = f64::from(marker.size);
        assert!(size > 0.0, "a zero-size marker cannot be drawn or detected");

        let mut ctx = MockCanvas2dContext::new(1200.0, 800.0);
        let _ = crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, &CanvasRenderConfig::default());
        let ops = ctx.operations().to_vec();

        let has_diagonal = |dx: f64, dy: f64| {
            ops.windows(2).any(|pair| match (&pair[0], &pair[1]) {
                (DrawOperation::MoveTo(x0, y0), DrawOperation::LineTo(x1, y1)) => {
                    (x1 - x0 - dx).abs() < 0.01 && (y1 - y0 - dy).abs() < 0.01
                }
                _ => false,
            })
        };

        assert!(
            has_diagonal(size, size),
            "the destroy cross is missing its first diagonal (delta +{size}, +{size})"
        );
        assert!(
            has_diagonal(-size, size),
            "the destroy cross is missing its second diagonal (delta -{size}, +{size}); one line \
             alone is a slash, not a destroy marker"
        );
    }

    /// A sequence with NO destroy must draw no cross.
    ///
    /// Regression guard: without it, a renderer that drew crosses unconditionally would satisfy the
    /// test above. The same two diagonals are searched for, so this fails if the marker ever leaks
    /// into a diagram that declares no lifecycle event.
    #[test]
    fn canvas_draws_no_destroy_cross_without_a_destroy() {
        let ir = fm_parser::parse(
            "sequenceDiagram\n  participant Alice\n  participant Bob\n  Alice->>Bob: Hi\n  Bob->>Alice: Bye\n",
        )
        .ir;
        let layout = fm_layout::layout_diagram(&ir);
        assert!(
            layout.extensions.sequence_lifecycle_markers.is_empty(),
            "CONTROL FAILED: this source produced a lifecycle marker, so it cannot show the \
             renderer is inert without one"
        );

        let mut ctx = MockCanvas2dContext::new(1200.0, 800.0);
        let _ = crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, &CanvasRenderConfig::default());
        let ops = ctx.operations().to_vec();

        // Any perfectly diagonal MoveTo -> LineTo pair with equal-magnitude dx and dy would be the
        // shape the marker draws; there should be none.
        let diagonal_pairs = ops
            .windows(2)
            .filter(|pair| match (&pair[0], &pair[1]) {
                (DrawOperation::MoveTo(x0, y0), DrawOperation::LineTo(x1, y1)) => {
                    let dx = (x1 - x0).abs();
                    let dy = (y1 - y0).abs();
                    dx > 0.5 && (dx - dy).abs() < 0.01
                }
                _ => false,
            })
            .count();
        assert_eq!(
            diagonal_pairs, 0,
            "a destroy-cross-shaped diagonal was drawn for a diagram with no destroy"
        );
    }


    /// A gantt must show its time axis on the canvas backend (bd-t1jj).
    ///
    /// `extensions.axis_ticks` is filled by the gantt layout arm and drawn by fm-render-svg; this
    /// renderer referenced it nowhere. Canvas is the browser preview surface -- fm-wasm renders
    /// through `render_to_canvas_with_layout` -- so a gantt showed bars with nothing to measure them
    /// against, exactly the state bd-trsd fixed on the SVG side and 27a6aadd fixed on the terminal
    /// side.
    ///
    /// The expected text is taken from the labels the LAYOUT produced rather than a hardcoded date,
    /// so this pins the property and not the fixture's particular `axisFormat`.
    #[test]
    fn canvas_draws_gantt_axis_tick_labels() {
        let ir = fm_parser::parse(
            "gantt\n  title Roadmap\n  dateFormat  YYYY-MM-DD\n  section Core\n  Design :a1, 2026-01-01, 3d\n  Build :a2, after a1, 4d\n",
        )
        .ir;
        let layout = fm_layout::layout_diagram(&ir);

        // NON-VACUITY: the layout must actually publish ticks, or this asserts nothing about the
        // renderer and would pass on a diagram with no axis to draw.
        let expected = layout
            .extensions
            .axis_ticks
            .iter()
            .map(|tick| tick.label.clone())
            .find(|label| !label.is_empty())
            .expect(
                "CONTROL FAILED: this gantt produced no axis tick labels, so the renderer has \
                 nothing to draw and this test cannot detect the defect it was written for",
            );

        let mut ctx = MockCanvas2dContext::new(1400.0, 700.0);
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );
        let drew_tick = ctx.operations().iter().any(|op| match op {
            DrawOperation::FillText(text, _, _) => text == &expected,
            _ => false,
        });
        assert!(
            drew_tick,
            "no axis tick label was drawn; the chart shows bars with nothing to measure them against"
        );

        // The task names must still be drawn -- an axis pass that overwrote the bars would trade one
        // piece of dropped content for another.
        let drew_task = ctx.operations().iter().any(|op| match op {
            DrawOperation::FillText(text, _, _) => text.contains("Design"),
            _ => false,
        });
        assert!(drew_task, "a task name was displaced by the axis pass");
    }

    /// A diagram with no axis must draw no tick.
    ///
    /// Regression guard: without it, a renderer that drew ticks unconditionally would satisfy the
    /// claim above.
    #[test]
    fn canvas_draws_no_axis_for_a_diagram_without_one() {
        let ir = fm_parser::parse("flowchart LR\n  A[Alpha] --> B[Beta]\n").ir;
        let layout = fm_layout::layout_diagram(&ir);
        assert!(
            layout.extensions.axis_ticks.is_empty(),
            "CONTROL FAILED: a flowchart produced axis ticks, so it cannot show the pass is inert"
        );

        let mut ctx = MockCanvas2dContext::new(800.0, 400.0);
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );
        // Node labels are still drawn; what must not appear is a second copy of them from an axis
        // pass that ran when it should not have.
        let alpha_draws = ctx
            .operations()
            .iter()
            .filter(|op| matches!(op, DrawOperation::FillText(text, _, _) if text.contains("Alpha")))
            .count();
        assert_eq!(
            alpha_draws, 1,
            "a label was drawn more than once, suggesting the axis pass ran for a diagram with no axis"
        );
    }


    /// A stateDiagram note must appear in the browser preview (bd-t1jj).
    ///
    /// `extensions.state_notes` is filled by the state layout arm (bd-a6l4) and drawn by
    /// fm-render-svg; this renderer referenced it nowhere. Canvas is the browser preview surface --
    /// fm-wasm renders through `render_to_canvas_with_layout` -- so `note right of X : ...` produced a
    /// note that existed in the layout, was hashed into the layout checksum, and appeared nowhere on
    /// screen.
    #[test]
    fn canvas_draws_a_state_note() {
        let ir = fm_parser::parse(
            "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Running\n  note right of Idle : waiting for work\n",
        )
        .ir;
        let layout = fm_layout::layout_diagram(&ir);

        // NON-VACUITY: the layout must actually publish a note, or this asserts nothing about the
        // renderer and would pass on a diagram with nothing to draw.
        let note = layout
            .extensions
            .state_notes
            .first()
            .expect(
                "CONTROL FAILED: this source produced no state note, so the renderer has nothing to \
                 draw and this test cannot detect the defect it was written for",
            );
        assert!(!note.text.is_empty(), "an empty note cannot be detected");

        let mut ctx = MockCanvas2dContext::new(1200.0, 800.0);
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );
        let ops = ctx.operations().to_vec();

        let drew_text = ops.iter().any(|op| match op {
            DrawOperation::FillText(text, _, _) => note.text.lines().any(|line| line == text),
            _ => false,
        });
        assert!(drew_text, "the note text was never drawn");

        // The LEADER must be drawn too: a box beside a state with nothing connecting them reads as
        // another node rather than as an annotation of that state.
        let sx = f64::from(note.leader_start.x);
        let sy = f64::from(note.leader_start.y);
        let ex = f64::from(note.leader_end.x);
        let ey = f64::from(note.leader_end.y);
        let drew_leader = ops.windows(2).any(|pair| match (&pair[0], &pair[1]) {
            (DrawOperation::MoveTo(x0, y0), DrawOperation::LineTo(x1, y1)) => {
                // Offsets are applied uniformly, so match on the leader's DELTA, which is
                // offset-independent.
                ((x1 - x0) - (ex - sx)).abs() < 0.01 && ((y1 - y0) - (ey - sy)).abs() < 0.01
            }
            _ => false,
        });
        assert!(
            drew_leader,
            "the note leader was never drawn; the box does not say which state it annotates"
        );
    }

    /// A state diagram with no note draws none.
    ///
    /// Regression guard: without it, a renderer that drew a note unconditionally would satisfy the
    /// claim above.
    #[test]
    fn canvas_draws_no_state_note_without_one() {
        let ir = fm_parser::parse("stateDiagram-v2\n  [*] --> Idle\n  Idle --> Running\n").ir;
        let layout = fm_layout::layout_diagram(&ir);
        assert!(
            layout.extensions.state_notes.is_empty(),
            "CONTROL FAILED: this source produced a note, so it cannot show the pass is inert"
        );

        let mut ctx = MockCanvas2dContext::new(1200.0, 800.0);
        let before = ctx.operations().len();
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );
        let drew_note_text = ctx.operations().iter().skip(before).any(|op| match op {
            DrawOperation::FillText(text, _, _) => text.contains("waiting"),
            _ => false,
        });
        assert!(!drew_note_text, "note text was drawn for a diagram with no note");
    }


    /// A gantt task name must honour its resolved placement on canvas too (bd-t1jj).
    ///
    /// Centring a name on its bar overflows when the name is wider than the bar, and when the bar sits
    /// near the right edge the overflow leaves the canvas and the name is lost -- measured on the
    /// terminal side in b0c1ff1d, where at 80 columns the right-edge task vanished entirely.
    /// `extensions.gantt_task_labels` resolves each task to Inside / OutsideRight / OutsideLeft; SVG
    /// and the terminal consume it, canvas did not.
    ///
    /// ASSERTED ON THE ALIGNMENT AND THE ANCHOR TOGETHER. Either alone can be right by accident: a
    /// centred label at the correct x is still centred, and a right-aligned label at the wrong x is
    /// still misplaced. The pair is what pins the SVG convention (OutsideLeft anchors the text's RIGHT
    /// edge, hence Right alignment at the anchor x).
    #[test]
    fn canvas_honours_gantt_label_placement() {
        let src = "gantt\n  title Roadmap\n  dateFormat  YYYY-MM-DD\n  section Core\n  \
                   ReticulateTheSplinesThoroughly :a1, 2026-01-01, 1d\n  \
                   Build :a2, after a1, 6d\n  \
                   FinalIntegrationAndSignoffPhase :a3, after a2, 1d\n";
        let ir = fm_parser::parse(src).ir;
        let layout = fm_layout::layout_diagram(&ir);

        // NON-VACUITY: a task must actually resolve to OutsideLeft, or this fixture does not exercise
        // the placement path at all.
        let outside_left = layout
            .extensions
            .gantt_task_labels
            .iter()
            .find(|e| matches!(e.placement, fm_layout::GanttLabelPlacement::OutsideLeft))
            .expect("CONTROL FAILED: no task resolved to OutsideLeft, so this proves nothing");

        let mut ctx = MockCanvas2dContext::new(1400.0, 700.0);
        let _ = crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, &CanvasRenderConfig::default());
        let ops = ctx.operations().to_vec();

        // The OutsideLeft label must be drawn RIGHT-aligned, at the anchor layout resolved.
        // The Inside-placed task gives the canvas offset: its anchor and its drawn x differ by
        // exactly offset_x, which is then used to check the OutsideLeft one.
        let inside_anchor = layout
            .extensions
            .gantt_task_labels
            .iter()
            .find(|e| matches!(e.placement, fm_layout::GanttLabelPlacement::Inside))
            .expect("CONTROL FAILED: no Inside-placed task, so the offset cannot be derived")
            .x;
        let mut inside_drawn_x: Option<f64> = None;
        let mut align = TextAlign::Center;
        let mut found = false;
        for op in &ops {
            match op {
                DrawOperation::SetTextAlign(a) => align = *a,
                DrawOperation::FillText(text, x, _) => {
                    if text.contains("Build") {
                        inside_drawn_x = Some(*x);
                    }
                    if text.contains("FinalIntegrationAndSignoffPhase") {
                        assert_eq!(
                            align,
                            TextAlign::Right,
                            "an OutsideLeft gantt label must be right-aligned; centring it is what \
                             pushes it off the canvas"
                        );
                        // Compared DIFFERENTIALLY against another label's anchor, because the
                        // rendered x carries the canvas offset while the layout anchor does not.
                        // Comparing them directly fails by exactly offset_x -- which is the mistake
                        // this assertion made on its first draft, and the same one the terminal
                        // version of this test made before it.
                        let offset = inside_drawn_x
                            .expect("the Inside-placed task must be drawn before the OutsideLeft one")
                            - f64::from(inside_anchor);
                        assert!(
                            (*x - (f64::from(outside_left.x) + offset)).abs() < 2.0,
                            "label drawn at {x}; with offset {offset} the anchor {} implies {}",
                            outside_left.x,
                            f64::from(outside_left.x) + offset
                        );
                        found = true;
                    }
                }
                _ => {}
            }
        }
        assert!(found, "the right-edge task name was never drawn at all");
    }

    /// A flowchart keeps centred labels.
    ///
    /// Regression guard: the placement path must be inert for every diagram type that publishes no
    /// gantt labels.
    #[test]
    fn canvas_flowchart_labels_stay_centred() {
        let ir = fm_parser::parse("flowchart LR\n  A[Alpha] --> B[Beta]\n").ir;
        let layout = fm_layout::layout_diagram(&ir);
        assert!(
            layout.extensions.gantt_task_labels.is_empty(),
            "CONTROL FAILED: a flowchart produced gantt labels"
        );
        let mut ctx = MockCanvas2dContext::new(800.0, 400.0);
        let _ = crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, &CanvasRenderConfig::default());
        let mut align = TextAlign::Left;
        for op in ctx.operations() {
            match op {
                DrawOperation::SetTextAlign(a) => align = *a,
                DrawOperation::FillText(text, _, _) if text.contains("Alpha") => {
                    assert_eq!(align, TextAlign::Center, "a flowchart label lost its centring");
                }
                _ => {}
            }
        }
    }


    /// A packet field that wraps across a 32-bit boundary must be drawn on BOTH rows (bd-t1jj).
    ///
    /// `extensions.packet_field_continuations` gives one box per extra row; this renderer drew only
    /// the primary. On the terminal side the same omission rendered a 24-bit field with the extent of
    /// an 8-bit one. A packet diagram exists to show how wide each field is, so dropping most of a
    /// field's extent misstates the one thing the diagram is for.
    ///
    /// ASSERTED BY OCCURRENCE COUNT, which is what separates a drawn continuation from a drawn
    /// primary: the wrapped field's name must be drawn TWICE, once per segment, while fields that do
    /// not wrap are drawn once. A presence check would have PASSED before the fix, because the
    /// primary box always carried the name -- the defect was extent, not presence.
    #[test]
    fn canvas_draws_a_wrapped_packet_field_on_both_rows() {
        let src = "packet-beta\n  0-15: \"SourcePort\"\n  16-23: \"Flags\"\n  24-47: \"CrossingField\"\n  48-63: \"Checksum\"\n";
        let ir = fm_parser::parse(src).ir;
        let layout = fm_layout::layout_diagram(&ir);

        // NON-VACUITY: layout must actually emit a continuation for this fixture.
        assert_eq!(
            layout.extensions.packet_field_continuations.len(),
            1,
            "CONTROL FAILED: expected one continuation for a field crossing the 32-bit boundary"
        );

        let mut ctx = MockCanvas2dContext::new(1400.0, 700.0);
        let _ = crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, &CanvasRenderConfig::default());
        let count = |needle: &str| {
            ctx.operations()
                .iter()
                .filter(|op| matches!(op, DrawOperation::FillText(t, _, _) if t.contains(needle)))
                .count()
        };
        assert_eq!(
            count("CrossingField"),
            2,
            "the wrapped field was drawn on one row only, so its extent is understated"
        );
        // Fields that do NOT wrap must still be drawn once -- a continuation pass that labelled every
        // field twice would satisfy the assertion above while corrupting the rest of the packet.
        assert_eq!(count("SourcePort"), 1, "an unwrapped field was duplicated");
        assert_eq!(count("Checksum"), 1, "an unwrapped field was duplicated");
    }

    /// A packet whose fields all fit one row gains no continuation.
    #[test]
    fn canvas_unwrapped_packet_gains_no_continuation() {
        let src = "packet-beta\n  0-15: \"SourcePort\"\n  16-31: \"DestPort\"\n";
        let ir = fm_parser::parse(src).ir;
        let layout = fm_layout::layout_diagram(&ir);
        assert!(
            layout.extensions.packet_field_continuations.is_empty(),
            "CONTROL FAILED: this packet produced a continuation, so it cannot show the pass is inert"
        );
        let mut ctx = MockCanvas2dContext::new(1400.0, 700.0);
        let _ = crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, &CanvasRenderConfig::default());
        let drawn = ctx
            .operations()
            .iter()
            .filter(|op| matches!(op, DrawOperation::FillText(t, _, _) if t.contains("SourcePort")))
            .count();
        assert_eq!(drawn, 1, "a field was duplicated although nothing wrapped");
    }


    /// A sequence diagram's participant headers must be mirrored at the FOOT on canvas (bd-t1jj).
    ///
    /// `extensions.sequence_mirror_headers` is filled by the sequence layout arm and rendered by
    /// fm-render-svg through the same node renderer it uses for the top row; this renderer referenced
    /// it nowhere. mermaid draws that bottom row, and it is not ornamental: on a long diagram the top
    /// headers scroll out of view and the reader loses track of which lifeline is which.
    ///
    /// ASSERTED BY OCCURRENCE COUNT -- each participant must be drawn TWICE, head and foot. That is
    /// the only signal that separates a drawn mirror header from the ordinary node that was always
    /// there; a presence check passes on the unfixed renderer.
    #[test]
    fn canvas_mirrors_sequence_participant_headers() {
        let ir = fm_parser::parse(
            // `mirrorActors` must be enabled explicitly: this engine defaults it to FALSE, so a plain
            // sequence diagram publishes no mirror headers and the fixture would test nothing.
            // mermaid's own default is true -- a separate divergence, noted on bd-t1jj.
            "%%{init: {\"sequence\": {\"mirrorActors\": true}}}%%\nsequenceDiagram\n  participant Alice\n  participant Bob\n  Alice->>Bob: Hi\n  Bob->>Alice: Bye\n",
        )
        .ir;
        let layout = fm_layout::layout_diagram(&ir);

        // NON-VACUITY: the layout must actually publish mirror headers for this source.
        assert!(
            !layout.extensions.sequence_mirror_headers.is_empty(),
            "CONTROL FAILED: this sequence produced no mirror headers, so the renderer has nothing \
             to draw and this test cannot detect the defect it was written for"
        );

        let mut ctx = MockCanvas2dContext::new(1200.0, 800.0);
        let _ = crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, &CanvasRenderConfig::default());
        let count = |needle: &str| {
            ctx.operations()
                .iter()
                .filter(|op| matches!(op, DrawOperation::FillText(t, _, _) if t == needle))
                .count()
        };
        assert_eq!(count("Alice"), 2, "Alice was not drawn at both head and foot");
        assert_eq!(count("Bob"), 2, "Bob was not drawn at both head and foot");
    }

    /// A flowchart draws each node label once -- the mirror pass must be inert for it.
    #[test]
    fn canvas_flowchart_nodes_are_not_mirrored() {
        let ir = fm_parser::parse("flowchart LR\n  A[Alpha] --> B[Beta]\n").ir;
        let layout = fm_layout::layout_diagram(&ir);
        assert!(
            layout.extensions.sequence_mirror_headers.is_empty(),
            "CONTROL FAILED: a flowchart produced mirror headers"
        );
        let mut ctx = MockCanvas2dContext::new(800.0, 400.0);
        let _ = crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, &CanvasRenderConfig::default());
        let alpha = ctx
            .operations()
            .iter()
            .filter(|op| matches!(op, DrawOperation::FillText(t, _, _) if t == "Alpha"))
            .count();
        assert_eq!(alpha, 1, "a flowchart label was mirrored");
    }


}
