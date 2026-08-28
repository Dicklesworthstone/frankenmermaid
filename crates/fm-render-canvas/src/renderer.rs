//! Canvas2D diagram renderer.
//!
//! Draws diagrams to Canvas2D contexts using computed layouts.

use crate::context::{Canvas2dContext, LineCap, LineJoin, TextAlign, TextBaseline};
use crate::shapes::{
    draw_arrowhead, draw_circle_marker, draw_cross_marker, draw_diamond_marker,
    draw_er_cardinality_marker, draw_open_triangle_marker, draw_shape,
};
use crate::viewport::{Viewport, fit_to_viewport};
use fm_core::{ArrowType, DiagramType, MermaidDiagramIr, NodeShape};
use fm_layout::{
    DiagramLayout, FillStyle, LineCap as IrLineCap, LineJoin as IrLineJoin, MarkerKind, PathCmd,
    RenderClip, RenderGroup, RenderItem, RenderPath, RenderScene, RenderSource, RenderText,
    RenderTransform, StrokeStyle, TextAlign as IrTextAlign, TextBaseline as IrTextBaseline,
};
use std::{borrow::Cow, collections::BTreeSet};

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
    /// Today's date, as `YYYY-MM-DD`, for the gantt today marker. `None` draws no marker.
    ///
    /// SUPPLIED, NEVER READ FROM THE CLOCK, exactly as `SvgRenderConfig::gantt_today` is. Output
    /// that is a function of wall time is a defect class this repo has already been bitten by three
    /// times; a renderer that called `now()` would make every canvas op-stream test depend on the
    /// day it ran. The default is `None`, so nothing about an existing render changes.
    pub gantt_today: Option<String>,
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
            gantt_today: None,
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
    /// Clickable areas for the interactive nodes IN THE LAYOUT THAT WAS JUST DRAWN (bd-2u0.2).
    ///
    /// ⚠️ CARRIED ON THE RESULT RATHER THAN LEFT TO A SECOND CALL, and that is the whole point.
    /// `interaction::hit_regions(ir, layout)` is public and a host could call it itself — but then
    /// nothing stops it passing a DIFFERENT layout than the one on screen, and the regions would sit
    /// where the nodes used to be. A pointer landing on the wrong node after a re-layout is a defect
    /// with no visible symptom until a user clicks. Returning them from the render makes that
    /// mismatch unrepresentable.
    ///
    /// Empty for a diagram with no `click`, and empty from [`Canvas2dRenderer::render_scene`] —
    /// see the note there.
    pub hit_regions: Vec<crate::interaction::HitRegion>,
}

/// Canvas2D diagram renderer.
#[derive(Debug, Clone)]
pub struct Canvas2dRenderer {
    config: CanvasRenderConfig,
    draw_calls: usize,
}

const DENSE_SOURCE_INDEX_LIMIT: usize = 65_536;
const LEGACY_DOTTED_EDGE_DASH: [f64; 2] = [5.0, 5.0];

/// Horizontal inset of a state note's text, mirroring fm-layout's `STATE_NOTE_PAD_X`.
///
/// The layout sizes the note box as `text * 0.8 + 2 * PAD`, so the text must start AT the padding
/// the box reserved. fm-render-svg writes `nx + 10.0` for the same reason. These are duplicated
/// rather than imported because the fm-layout constants are private; if they move, the state-note
/// agreement test is what catches the drift.
const STATE_NOTE_PAD_X: f64 = 10.0;

/// Vertical inset of a state note's first line, mirroring fm-layout's `STATE_NOTE_PAD_Y`.
const STATE_NOTE_PAD_Y: f64 = 8.0;

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
        self.draw_bands(layout, ir.diagram_type, ctx, offset_x, offset_y);
        // Treemap tiles and the radar wheel (bd-dw450): both families keep their whole
        // diagram in a layout extension only fm-render-svg read, so the canvas drew nothing
        // at all for them. Both calls return immediately for every other diagram type.
        self.draw_treemap(ir, layout, ctx, offset_x, offset_y);
        self.draw_radar(ir, layout, ctx, offset_x, offset_y);

        // Draw the time/category axis (gantt dates, xychart categories).
        self.draw_axis_ticks(layout, ctx, offset_x, offset_y);

        // Draw the gantt today marker, on top of the bands and under the bars.
        self.draw_gantt_today_marker(layout, ir, ctx, offset_x, offset_y);

        // Draw the concurrency-region dividers inside a composite state.
        self.draw_cluster_dividers(layout, ctx, offset_x, offset_y);

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
        self.draw_sequence_fragments(layout, ir, ctx, offset_x, offset_y);
        self.draw_sequence_notes(layout, ctx, offset_x, offset_y, &mut labels_drawn);

        // Draw pie chart wedges if this is a pie diagram.
        if ir.diagram_type == DiagramType::Pie {
            self.draw_pie_wedges(layout, ir, ctx, offset_x, offset_y, &mut labels_drawn);
        }

        // Draw quadrant axis labels (bd-59o4).
        if ir.diagram_type == DiagramType::QuadrantChart {
            self.draw_quadrant_axis_labels(layout, ir, ctx, offset_x, offset_y, &mut labels_drawn);
        }

        // Draw edges
        let edges_drawn = self.draw_edges(layout, ir, ctx, offset_x, offset_y, &mut labels_drawn);

        // Draw nodes
        let nodes_drawn = self.draw_nodes(layout, ir, ctx, offset_x, offset_y, &mut labels_drawn);

        ctx.restore();
        if self.draw_generic_diagram_title(ctx, ir, canvas_width) {
            labels_drawn += 1;
        }
        labels_drawn += self.draw_c4_legend(ctx, ir, canvas_width, canvas_height);

        CanvasRenderResult {
            draw_calls: self.draw_calls,
            nodes_drawn,
            edges_drawn,
            clusters_drawn,
            labels_drawn,
            viewport,
            // From the SAME `ir` and `layout` this call just drew, so the regions cannot
            // describe a different picture than the one on screen.
            hit_regions: crate::interaction::hit_regions(ir, layout),
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

    /// Draw the C4 legend requested by `SHOW_LEGEND()`.
    ///
    /// SVG reserves extra diagram-space height for this panel. Canvas owns a fixed viewport, so
    /// placing it after restoring the viewport transform makes it a screen-space overlay instead:
    /// it stays visible when auto-fit changes scale and it does not alter node/edge coordinates.
    /// The entries intentionally use the same role classes and glyphs as the SVG legend.
    fn draw_c4_legend<C: Canvas2dContext>(
        &mut self,
        ctx: &mut C,
        ir: &MermaidDiagramIr,
        canvas_width: f64,
        canvas_height: f64,
    ) -> usize {
        if !canvas_c4_legend_enabled(ir) {
            return 0;
        }

        let entries = canvas_c4_legend_entries(ir);
        let padding = self.config.padding.max(8.0);
        let box_width = (canvas_width - padding * 2.0).clamp(0.0, 320.0);
        let box_height = (canvas_height - padding * 2.0).clamp(0.0, 128.0);
        if box_width == 0.0 || box_height == 0.0 {
            return 0;
        }
        let x = padding;
        let y = canvas_height - padding - box_height;

        ctx.set_fill_style("rgba(248,249,250,0.96)");
        ctx.fill_rect(x, y, box_width, box_height);
        ctx.set_stroke_style(&self.config.cluster_stroke);
        ctx.set_line_width(1.0);
        ctx.stroke_rect(x, y, box_width, box_height);
        self.draw_calls += 2;

        ctx.set_fill_style(&self.config.label_color);
        ctx.set_font(&format!(
            "600 {}px {}",
            self.config.font_size * 0.82,
            self.config.font_family
        ));
        ctx.set_text_align(TextAlign::Left);
        ctx.set_text_baseline(TextBaseline::Top);
        ctx.fill_text("C4 Legend", x + 14.0, y + 12.0);
        self.draw_calls += 1;

        let left_x = x + 14.0;
        let right_x = x + box_width / 2.0 + 8.0;
        let mut left_y = y + 36.0;
        let mut right_y = y + 36.0;
        ctx.set_font(&format!(
            "{}px {}",
            self.config.font_size * 0.72,
            self.config.font_family
        ));
        for (index, entry) in entries.iter().enumerate() {
            let (entry_x, entry_y) = if index % 2 == 0 {
                let position = (left_x, left_y);
                left_y += 18.0;
                position
            } else {
                let position = (right_x, right_y);
                right_y += 18.0;
                position
            };
            ctx.fill_text(entry, entry_x, entry_y);
            self.draw_calls += 1;
        }

        entries.len() + 1
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
            // EMPTY, and not an oversight. A `RenderScene` carries drawable geometry and no
            // `MermaidDiagramIr`, so there is no `click` declaration here to export — the
            // interaction data was dropped upstream when the scene was built. Synthesising
            // regions from the geometry alone would invent clickable areas the author never
            // declared; a caller that needs them must render through `render`, which has the IR.
            hit_regions: Vec::new(),
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

            // Draw cluster background, honouring a declared colour (bd-lvj3).
            //
            // The last row of that bead's measured table: `rect rgb(255,0,0)` in a sequence diagram
            // rendered in SVG and NOT on the canvas. The colour was never far away -- it rides on the
            // very `LayoutClusterBox` this loop already holds, and the fill was hardcoded to the
            // theme colour anyway.
            //
            // `transparent` is mapped the way fm-render-svg maps it (lib.rs:4365): the FILL goes
            // transparent but the border falls back to a visible default, because a subgraph that
            // asked for a transparent body still needs an edge or it stops being a grouping.
            // The author's own `style mySubgraph fill:#f00` (bd-xfmm), resolved from the style
            // refs rather than from the layout box. It takes precedence over `cluster_box.color`
            // because the two come from DIFFERENT syntaxes: `color` carries a sequence `rect
            // rgb(...)`, which is a property of the frame, while a `style` directive names this
            // cluster explicitly. When an author does both, the one that named the target wins.
            let (styled_fill, styled_stroke) =
                resolve_cluster_colors(ir, cluster_box.cluster_index);
            let styled_fill = styled_fill.as_deref().and_then(sanitize_canvas_paint);
            let styled_stroke = styled_stroke.as_deref().and_then(sanitize_canvas_paint);

            let declared = cluster_box.color.as_deref().and_then(sanitize_canvas_paint);
            let (fill, stroke) = if let Some(styled) = styled_fill.as_deref() {
                // A declared stroke is honoured; without one the fill doubles as the border, which
                // is what the `rect`/cluster path above already does for a declared colour.
                (styled, styled_stroke.as_deref().unwrap_or(styled))
            } else if let Some(styled) = styled_stroke.as_deref() {
                // Stroke alone: keep the theme fill rather than painting the body a border colour.
                (self.config.cluster_fill.as_str(), styled)
            } else {
                match declared.as_deref() {
                    Some("transparent") => ("transparent", self.config.cluster_stroke.as_str()),
                    Some(color) => (color, color),
                    None => (
                        self.config.cluster_fill.as_str(),
                        self.config.cluster_stroke.as_str(),
                    ),
                }
            };
            // OPACITY wraps the whole cluster — box and title — as `opacity` does on an SVG
            // element. `globalAlpha` is canvas STATE, so it is restored at the end of the
            // iteration; left set, it would fade every node drawn inside this subgraph, which are
            // drawn after their container.
            let cluster_opacity = resolve_cluster_opacity(ir, cluster_box.cluster_index);
            if let Some(alpha) = cluster_opacity {
                ctx.set_global_alpha(alpha);
            }
            ctx.set_fill_style(fill);
            ctx.set_stroke_style(stroke);
            // The border WIDTH and DASH were the two channels this surface still discarded: the
            // width was hardcoded to 1.0 and no dash was ever set, while the SVG arm emits both
            // (`stroke-width:5px`, `stroke-dasharray:9 4`). `unwrap_or(1.0)` keeps the previous
            // constant when nothing was declared, so an unstyled cluster is drawn exactly as before.
            ctx.set_line_width(
                resolve_cluster_stroke_width(ir, cluster_box.cluster_index).unwrap_or(1.0),
            );
            // Set only when declared and cleared after the stroke, because `lineDash` is canvas
            // STATE — a dashed cluster would otherwise dash every shape drawn after it.
            let cluster_dash = resolve_cluster_dash_array(ir, cluster_box.cluster_index);
            if let Some(ref pattern) = cluster_dash {
                ctx.set_line_dash(pattern);
            }

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
            if cluster_dash.is_some() {
                ctx.set_line_dash(&[]);
            }
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
                let declared_title_color =
                    resolve_cluster_text_color(ir, cluster_box.cluster_index);
                let title_color = declared_title_color
                    .as_deref()
                    .and_then(sanitize_canvas_paint);
                ctx.set_fill_style(title_color.as_deref().unwrap_or(&self.config.label_color));
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

                // THE C4 BOUNDARY TYPE ROW, which mermaid draws beneath the label and the SVG arm
                // already draws. Nested inside the title branch on purpose: mermaid emits the type
                // only as the second row of a captioned boundary, so a boundary with no label to
                // sit under should not show a bracketed type floating on its own.
                //
                // The IR stores mermaid's bare token and each renderer adds the brackets, exactly
                // as mermaid does — its `drawInsideBoundary` rewrites `l.type.text` to
                // `"[" + l.type.text + "]"` before `drawBoundary` draws it.
                if let Some(boundary_type) = ir
                    .clusters
                    .get(cluster_box.cluster_index)
                    .and_then(|ir_cluster| ir_cluster.c4_boundary_type.as_deref())
                    .filter(|value| !value.is_empty())
                {
                    ctx.fill_text(
                        &format!("[{boundary_type}]"),
                        x + 8.0,
                        y + 4.0 + self.config.font_size * 1.25,
                    );
                    self.draw_calls += 1;
                }
            }

            if cluster_opacity.is_some() {
                ctx.set_global_alpha(1.0);
            }

            count += 1;
        }

        count
    }

    /// Draw layout extension bands (sequence lifelines, gantt sections, etc.).
    /// Draw a quadrant chart's four axis labels (bd-59o4).
    ///
    /// Measured: `x_axis_left` had ZERO references anywhere in this crate — against one in
    /// fm-render-term and two in fm-render-svg — so the canvas never read the field at all. The
    /// chart's TITLE and its data POINTS still appeared, because both come from the generic title
    /// and node paths, which is why only the axes were missing and the chart looked almost right.
    ///
    /// That is a DIFFERENT cause from the terminal's, which drew these labels correctly but only on
    /// a render path `TermRenderConfig::rich()` never takes. Keeping the two halves of bd-59o4
    /// apart is what kept this one visible after the terminal was fixed.
    ///
    /// Placed at the layout bounds rather than at chart-relative margins: unlike fm-render-svg,
    /// which computes `margin_left` from the label's own width, this renderer has no quadrant
    /// geometry pass to borrow margins from, and an invented margin would drift from the axis it
    /// labels.
    fn draw_quadrant_axis_labels<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
        labels_drawn: &mut usize,
    ) {
        let Some(quad) = ir.quadrant_meta.as_ref() else {
            return;
        };

        let left = f64::from(layout.bounds.x) + offset_x;
        let right = f64::from(layout.bounds.x + layout.bounds.width) + offset_x;
        let top = f64::from(layout.bounds.y) + offset_y;
        let bottom = f64::from(layout.bounds.y + layout.bounds.height) + offset_y;
        let pad = self.config.font_size;

        ctx.set_fill_style(&self.config.label_color);
        ctx.set_font(&secondary_label_font_css(&self.config));
        ctx.set_text_baseline(TextBaseline::Middle);

        let mut draw = |text: Option<&String>, x: f64, y: f64, align: TextAlign| {
            let Some(text) = text.map(String::as_str).filter(|t| !t.is_empty()) else {
                return;
            };
            ctx.set_text_align(align);
            ctx.fill_text(text, x, y);
            self.draw_calls += 1;
            *labels_drawn += 1;
        };

        draw(
            quad.x_axis_left.as_ref(),
            left + pad,
            bottom + pad,
            TextAlign::Left,
        );
        draw(
            quad.x_axis_right.as_ref(),
            right - pad,
            bottom + pad,
            TextAlign::Right,
        );
        draw(
            quad.y_axis_top.as_ref(),
            left - pad,
            top + pad,
            TextAlign::Right,
        );
        draw(
            quad.y_axis_bottom.as_ref(),
            left - pad,
            bottom - pad,
            TextAlign::Right,
        );

        // The quadrant NAMES, which this drew none of (bd-039t, third widening of
        // `renderer_agreement.rs`). bd-59o4 fixed the four AXIS labels in both this renderer and
        // the terminal and stopped there, so `quadrant-1 Do it` reached the SVG and neither of the
        // other two. The chart still looked almost right — title, points and axes all present —
        // which is why a three-renderer gate caught it and looking at it did not.
        //
        // `quadrant_labels` is documented index-ordered [Q1 top-right, Q2 top-left, Q3 bottom-left,
        // Q4 bottom-right], so placement follows the INDEX rather than a guess. `.get()` rather
        // than indexing: a chart may declare fewer than four.
        //
        // Centred in each quadrant, INSIDE the plot area — unlike the axis labels above, which sit
        // outside it by design. A quadrant name belongs to the region it names, so anchoring it to
        // the region's own centre is what makes it readable as that region's label.
        let mid_x = f64::midpoint(left, right);
        let mid_y = f64::midpoint(top, bottom);
        let quarter_x_left = f64::midpoint(left, mid_x);
        let quarter_x_right = f64::midpoint(mid_x, right);
        let quarter_y_top = f64::midpoint(top, mid_y);
        let quarter_y_bottom = f64::midpoint(mid_y, bottom);

        draw(
            quad.quadrant_labels.first(),
            quarter_x_right,
            quarter_y_top,
            TextAlign::Center,
        );
        draw(
            quad.quadrant_labels.get(1),
            quarter_x_left,
            quarter_y_top,
            TextAlign::Center,
        );
        draw(
            quad.quadrant_labels.get(2),
            quarter_x_left,
            quarter_y_bottom,
            TextAlign::Center,
        );
        draw(
            quad.quadrant_labels.get(3),
            quarter_x_right,
            quarter_y_bottom,
            TextAlign::Center,
        );
    }
}

/// The radar series palette, matching the SVG surface so one diagram is not two colour schemes.
const RADAR_CANVAS_PALETTE: [&str; 6] = [
    "#8686ff", "#ffff78", "#d7ff86", "#ff86c8", "#86e0ff", "#ffc386",
];

/// Render a treemap value for a canvas caption: no trailing zeros on a whole number.
///
/// Deliberately the same rule the SVG and terminal surfaces apply, so the three never disagree
/// about what a value IS — a `30` on one surface and `30.0` on another reads as two numbers.
fn format_canvas_treemap_value(value: f64) -> String {
    if (value - value.round()).abs() < 1e-9 {
        format!("{}", value.round() as i64)
    } else {
        let text = format!("{value:.4}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

impl Canvas2dRenderer {
    /// Draw `treemap` tiles: a nested outline per tile with its label and value (bd-dw450).
    ///
    /// Outlines rather than fills, because a treemap nests: filling a section would bury every
    /// child it contains under its own parent. The SVG surface can layer translucent fills to say
    /// the same thing; here the outline carries it unambiguously.
    fn draw_treemap<C: Canvas2dContext>(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        let Some(meta) = ir.treemap_meta.as_ref() else {
            return;
        };
        if layout.extensions.treemap_tiles.is_empty() {
            return;
        }
        let label_font = format!("{}px {}", self.config.font_size, self.config.font_family);
        for tile in &layout.extensions.treemap_tiles {
            let Some(item) = meta.nodes.get(tile.node) else {
                continue;
            };
            let x = f64::from(tile.bounds.x) + offset_x;
            let y = f64::from(tile.bounds.y) + offset_y;
            let w = f64::from(tile.bounds.width);
            let h = f64::from(tile.bounds.height);
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            ctx.set_stroke_style(&self.config.node_stroke);
            ctx.set_line_width(1.0);
            ctx.stroke_rect(x, y, w, h);
            self.draw_calls += 1;

            let caption = format!("{} {}", item.label, format_canvas_treemap_value(tile.value));
            ctx.set_font(&label_font);
            ctx.set_fill_style(&self.config.label_color);
            // A leaf is captioned at its centre; a section just inside its top edge, where a
            // container has room that its middle does not.
            if tile.is_leaf {
                ctx.set_text_align(TextAlign::Center);
                ctx.set_text_baseline(TextBaseline::Middle);
                ctx.fill_text(&caption, x + w / 2.0, y + h / 2.0);
            } else {
                ctx.set_text_align(TextAlign::Left);
                ctx.set_text_baseline(TextBaseline::Top);
                ctx.fill_text(&caption, x + 4.0, y + 2.0);
            }
            self.draw_calls += 1;
        }
    }

    /// Draw a `radar-beta` wheel: graticule, spokes, axis labels and one closed curve per series.
    fn draw_radar<C: Canvas2dContext>(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        let (Some(meta), Some(radar)) = (ir.radar_meta.as_ref(), layout.extensions.radar.as_ref())
        else {
            return;
        };
        let cx = f64::from(radar.center.x) + offset_x;
        let cy = f64::from(radar.center.y) + offset_y;

        // Graticule. `arc` draws the ring form directly; the polygon form walks the axis directions,
        // because `graticule polygon` measurably changes the grid AND the curve upstream.
        ctx.set_stroke_style(&self.config.node_stroke);
        ctx.set_line_width(1.0);
        for &ring in &radar.rings {
            let ring = f64::from(ring);
            ctx.begin_path();
            match meta.graticule {
                fm_core::RadarGraticule::Circle => {
                    ctx.arc(cx, cy, ring, 0.0, std::f64::consts::TAU);
                }
                fm_core::RadarGraticule::Polygon => {
                    for (index, axis) in radar.axes.iter().enumerate() {
                        let angle = f64::from(axis.angle);
                        let (x, y) = (ring.mul_add(angle.cos(), cx), ring.mul_add(angle.sin(), cy));
                        if index == 0 {
                            ctx.move_to(x, y);
                        } else {
                            ctx.line_to(x, y);
                        }
                    }
                    ctx.close_path();
                }
            }
            ctx.stroke();
            self.draw_calls += 1;
        }

        // Spokes, then their labels.
        let label_font = format!("{}px {}", self.config.font_size, self.config.font_family);
        for (index, axis) in radar.axes.iter().enumerate() {
            ctx.begin_path();
            ctx.move_to(cx, cy);
            ctx.line_to(
                f64::from(axis.tip.x) + offset_x,
                f64::from(axis.tip.y) + offset_y,
            );
            ctx.stroke();
            self.draw_calls += 1;

            if let Some(declared) = meta.axes.get(index) {
                ctx.set_font(&label_font);
                ctx.set_fill_style(&self.config.label_color);
                ctx.set_text_align(TextAlign::Center);
                ctx.set_text_baseline(TextBaseline::Middle);
                ctx.fill_text(
                    declared.display(),
                    f64::from(axis.label_anchor.x) + offset_x,
                    f64::from(axis.label_anchor.y) + offset_y,
                );
                self.draw_calls += 1;
            }
        }

        // One closed outline per series.
        for laid in &radar.curves {
            if laid.points.is_empty() {
                continue;
            }
            let fill = RADAR_CANVAS_PALETTE[laid.curve % RADAR_CANVAS_PALETTE.len()];
            ctx.set_stroke_style(fill);
            ctx.set_line_width(2.0);
            ctx.begin_path();
            for (index, point) in laid.points.iter().enumerate() {
                let x = f64::from(point.x) + offset_x;
                let y = f64::from(point.y) + offset_y;
                if index == 0 {
                    ctx.move_to(x, y);
                } else {
                    ctx.line_to(x, y);
                }
            }
            ctx.close_path();
            ctx.stroke();
            self.draw_calls += 1;
        }
        ctx.set_line_width(1.0);
    }
}

impl Canvas2dRenderer {
    fn draw_bands<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        diagram_type: fm_core::DiagramType,
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

                    // A NAMED lane also draws its name (bd-rk14).
                    //
                    // This arm drew geometry and no text, while the Section arm below drew its
                    // label — so a gitGraph branch band reached the canvas as a bare dashed line
                    // and the branch name appeared nowhere. Measured: the layout carries
                    // [(Lane,"main"), (Lane,"dev")] and the canvas drew only "commit_1"/"commit_2".
                    //
                    // NOT the same cause as the box-content drops in this bead, which is why it was
                    // held back rather than lumped in with them: those needed new node arms, this is
                    // a missing label inside a band arm that already existed. It is the canvas twin
                    // of bd-u3fo.
                    //
                    // ⚠️ A NON-EMPTY LABEL IS NOT ENOUGH TO TELL THESE APART. I first gated on that
                    // alone, assuming sequence lifelines were unlabelled — they are NOT: a lifeline
                    // band carries its participant's name, so that version drew `Alice` a THIRD
                    // time and `canvas_mirrors_sequence_participant_headers` failed on
                    // `count("Alice") == 2`. The existing control caught the wrong assumption.
                    //
                    // `LayoutBandKind::Lane` is overloaded — sequence lifelines AND named lanes
                    // (gitGraph branches, journey lanes) share it — so the discriminator has to come
                    // from outside the band. A distinct kind in fm-layout would be the better fix;
                    // that file is under another agent's exclusive lease, so this gates on the
                    // diagram type instead, which is what actually decides the meaning: in a
                    // sequence diagram a Lane IS a lifeline and its name is already drawn as a
                    // head/foot header.
                    if !band.label.is_empty() && diagram_type != fm_core::DiagramType::Sequence {
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
                        self.draw_calls += 1;
                    }
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
        // Gantt publishes its axis baselines from layout: the bottom row is always present and
        // `topAxis` appends a second row. Other diagram types retain their existing generic axis.
        let axis_rows: Vec<f64> = if layout.extensions.gantt_axis_rows.is_empty() {
            vec![f64::from(layout.bounds.y) + offset_y - 12.0]
        } else {
            layout
                .extensions
                .gantt_axis_rows
                .iter()
                .map(|axis| f64::from(axis.y) + offset_y)
                .collect()
        };
        for y in axis_rows {
            for tick in &layout.extensions.axis_ticks {
                if tick.label.is_empty() {
                    continue;
                }

                // THE TICK MARK ITSELF, which this surface never drew (bd-4n5j2). fm-render-svg emits
                // `<line y1=y+4 y2=y+16>` per tick; the canvas drew the label alone, so a gantt axis
                // rendered as floating dates with nothing tying them to a column. That is the
                // bd-039t/bd-t1jj family: declared content one renderer draws and another omits.
                let tick_x = f64::from(tick.position) + offset_x;
                ctx.set_stroke_style(&self.config.edge_stroke);
                ctx.set_line_width(1.0);
                ctx.begin_path();
                ctx.move_to(tick_x, y + 4.0);
                ctx.line_to(tick_x, y + 16.0);
                ctx.stroke();

                ctx.set_fill_style(&self.config.label_color);
                // Set explicitly, not inherited (bd-4n5j2): this runs after draw_clusters and
                // draw_bands, either of which may or may not have set these depending on whether the
                // diagram had a titled cluster or a labelled band.
                //
                // ALPHABETIC, not Top. The SVG tick text carries `text-anchor="start"` and NO
                // `dominant-baseline`, so its y IS the alphabetic baseline -- which is what makes y=89
                // in the golden the baseline rather than the top of the glyphs. An earlier pass here
                // set Top, which was deterministic but disagreed with the reference arm by an ascent.
                ctx.set_text_align(TextAlign::Left);
                ctx.set_text_baseline(TextBaseline::Alphabetic);
                ctx.set_font(tick_font.get_or_insert_with(|| {
                    format!(
                        "{}px {}",
                        self.config.font_size * 0.72,
                        self.config.font_family
                    )
                }));
                // `x + 3.0`, as the SVG writes it: the label sits just right of its tick mark rather
                // than centred on it.
                ctx.fill_text(&tick.label, tick_x + 3.0, y);
                self.draw_calls += 2;
            }
        }
    }

    /// Draw the gantt today marker: a vertical line across the chart at the supplied date.
    ///
    /// `extensions.gantt_day_axis` was the last field this renderer referenced NOWHERE (bd-t1jj),
    /// and it is the only thing that answers "where is a given DATE on this chart". So a canvas
    /// gantt had no today line at all, while the same diagram rendered to SVG had one — and
    /// `todayMarker off`, which a user writes precisely to turn the line off, was equally invisible
    /// because there was nothing to turn off. Canvas is a shipping surface: fm-wasm renders the
    /// browser preview through `render_to_canvas_with_layout`.
    ///
    /// FOUR CONDITIONS, mirroring the SVG arm one for one so the two backends cannot disagree about
    /// whether a marker belongs:
    ///  1. `config.gantt_today` is supplied. Never the clock — see the field's own doc.
    ///  2. It parses as a real calendar date via `fm_layout::parse_iso_day_number`, the SAME
    ///     function the layout used to place the bars, not a second copy of that arithmetic.
    ///  3. Today falls INSIDE the charted span. `x_for_day` returns `None` otherwise, and drawing
    ///     nothing is the correct response to "today is not in this chart".
    ///  4. `todayMarker off` suppresses it.
    ///
    /// The x comes from `axis.x_for_day`, never re-derived here. `LayoutGanttDayAxis`'s own doc
    /// warns that a marker computing its own x is how a today line and its axis come to disagree
    /// about where a day is.
    fn draw_gantt_today_marker<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        let (Some(today), Some(axis)) = (
            self.config.gantt_today.as_deref(),
            layout.extensions.gantt_day_axis,
        ) else {
            return;
        };
        let style = ir
            .gantt_meta
            .as_ref()
            .and_then(|meta| meta.today_marker_style.as_deref())
            .unwrap_or("");
        if style.trim().eq_ignore_ascii_case("off") {
            return;
        }
        let Some(day) = fm_layout::parse_iso_day_number(today) else {
            return;
        };
        let Some(x) = axis.x_for_day(day) else {
            return;
        };

        let x = f64::from(x) + offset_x;
        let top = f64::from(layout.bounds.y) + offset_y + 12.0;
        let bottom = f64::from(layout.bounds.y + layout.bounds.height) + offset_y;

        // mermaid's own red today line, matching the SVG arm's fallback. A declared style string is
        // CSS and has no canvas equivalent, so it is not half-applied here: honouring `off` and
        // ignoring the colouring is a smaller divergence than inventing a CSS parser in this crate.
        ctx.set_stroke_style("#ff0000");
        ctx.set_line_width(2.0);
        ctx.begin_path();
        ctx.move_to(x, top);
        ctx.line_to(x, bottom);
        ctx.stroke();
        self.draw_calls += 1;
    }

    /// Draw stateDiagram notes -- box, leader and text.
    ///
    /// The `--` CONCURRENCY SEPARATOR inside a composite state, which only the SVG drew (bd-dgnm4).
    ///
    /// `state Big { A --> B  --  C --> D }` declares two regions running in parallel. The layout
    /// records the boundary between them in `extensions.cluster_dividers` — built by
    /// `build_state_cluster_dividers`, keyed on the `__state_region_` subgraphs the `--` creates —
    /// and fm-render-svg draws a dashed line for each. This surface referenced that extension
    /// nowhere, so the two regions ran together into one box and a reader could not tell there were
    /// two. The separator is SYNTAX the author wrote, not decoration.
    ///
    /// Found by diffing which LAYOUT EXTENSIONS each renderer consumes, which is the comparison that
    /// works: an earlier pass compared the CONFIG FIELD `sequence_mirror_actors` and wrongly flagged
    /// the canvas, because layout had already applied the flag and the canvas consumes the derived
    /// `sequence_mirror_headers` instead. Compare what a renderer READS, not what it is configured by.
    ///
    /// Dashed to match the SVG, whose `stroke-dasharray("6,4")` is the thing distinguishing a region
    /// boundary from an ordinary cluster edge. The dash is reset afterwards so nothing drawn later
    /// inherits it.
    fn draw_cluster_dividers<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        if layout.extensions.cluster_dividers.is_empty() {
            return;
        }

        ctx.set_stroke_style(&self.config.cluster_stroke);
        ctx.set_line_width(1.0);
        ctx.set_line_dash(&[6.0, 4.0]);
        for divider in &layout.extensions.cluster_dividers {
            ctx.begin_path();
            ctx.move_to(
                f64::from(divider.start.x) + offset_x,
                f64::from(divider.start.y) + offset_y,
            );
            ctx.line_to(
                f64::from(divider.end.x) + offset_x,
                f64::from(divider.end.y) + offset_y,
            );
            ctx.stroke();
            self.draw_calls += 1;
        }
        ctx.set_line_dash(&[]);
    }

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
                // ⚠️ SET EXPLICITLY, NOT INHERITED (bd-4n5j2). Every draw source runs inside one
                // save()/restore(), and this one used to call fill_text without touching either,
                // so it took whatever the last source to set them left behind. draw_clusters sets
                // Left/Top only when it draws a cluster TITLE -- so the same note landed at a
                // different vertical position depending on whether the diagram also happened to
                // contain a composite state. Two state diagrams with identical notes rendered them
                // differently.
                //
                // Start/Hanging is what fm-render-svg uses for this text (TextAnchor::Start,
                // DominantBaseline::Hanging), so pinning it here also moves the canvas toward the
                // fuller arm instead of merely making the accident deterministic.
                ctx.set_text_align(TextAlign::Left);
                ctx.set_text_baseline(TextBaseline::Top);
                ctx.set_font(note_font.get_or_insert_with(|| {
                    format!(
                        "{}px {}",
                        self.config.font_size * 0.8,
                        self.config.font_family
                    )
                }));
                // Multi-line aware: `note right of X … end note` is the form that carries more than a
                // sentence, and drawing only the first line would silently drop the rest.
                //
                // ⚠️ THE INSETS ARE THE LAYOUT'S OWN PADDING, NOT A LOCAL GUESS (bd-4n5j2).
                // fm-layout sizes this box as `text * 0.8 + 2 * PAD`, with STATE_NOTE_PAD_X = 10.0
                // and STATE_NOTE_PAD_Y = 8.0, and fm-render-svg draws the text at exactly
                // `nx + 10, ny + 8` to match. This surface used `x + 4` and started the first line a
                // whole line-height down, so it ignored the padding the box was measured for: text
                // began 6 left of its reserved margin and 8.8 below it, pushing the last line toward
                // an overflow the box was never sized to hold.
                //
                // Line SPACING was already right, by coincidence of two different formulas: the SVG
                // uses `font_size * 0.8 * line_height(1.5)` and this uses `font_size * 1.2`, both
                // 16.8 at the default 14. Left alone rather than "unified" -- they agree, and the
                // two configs are not shared.
                let line_height = self.config.font_size * 1.2;
                for (row, line) in note.text.lines().enumerate() {
                    let line_y = y + STATE_NOTE_PAD_Y + line_height * row as f64;
                    // The layout sized the box to fit, so this guard is unreachable for a note the
                    // layout measured; it stays as a bound against drawing outside the box, which is
                    // the one case where matching the SVG would be worse than differing from it.
                    if line_y > y + h {
                        break;
                    }
                    ctx.fill_text(line, x + STATE_NOTE_PAD_X, line_y);
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
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        let mut fragment_font = None;

        for (fragment_index, fragment) in layout.extensions.sequence_fragments.iter().enumerate() {
            let x = f64::from(fragment.bounds.x) + offset_x;
            let y = f64::from(fragment.bounds.y) + offset_y;
            let w = f64::from(fragment.bounds.width);
            let h = f64::from(fragment.bounds.height);

            // Semi-transparent background, or the author's own (bd-lvj3).
            //
            // `rect rgb(255,0,0)` in a sequence diagram becomes a FRAGMENT, not a cluster, and the
            // fill here was a hardcoded literal while `fragment.color` sat unread on the very struct
            // this loop iterates. This is the last row of that bead's measured table
            // (seq_rect_color svg=true canvas=FALSE).
            let declared = fragment.color.as_deref().and_then(sanitize_canvas_paint);
            ctx.set_fill_style(declared.as_deref().unwrap_or("rgba(226,232,240,0.2)"));
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

            // An `else`/`and` branch is part of the sequence's meaning, not decoration. Layout
            // preserves the branch's first message edge, so place its divider at the same lead from
            // that message that the frame has from its own first message. A fixed frame fraction
            // drifts when branches contain different numbers of messages.
            let Some(ir_fragment) = ir
                .sequence_meta
                .as_ref()
                .and_then(|meta| meta.fragments.get(fragment_index))
            else {
                continue;
            };
            let message_y = |edge_index: usize| {
                layout
                    .edges
                    .iter()
                    .find(|edge| edge.edge_index == edge_index)
                    .and_then(|edge| edge.points.first())
                    .map(|point| f64::from(point.y) + offset_y)
            };
            let lead = message_y(ir_fragment.start_edge).map_or(0.0, |first_y| first_y - y);
            for alternative in &ir_fragment.alternatives {
                let Some(branch_y) = message_y(alternative.start_edge) else {
                    continue;
                };
                let divider_y = branch_y - lead;
                ctx.set_stroke_style(&self.config.node_stroke);
                ctx.set_line_width(1.0);
                ctx.set_line_dash(&[4.0, 4.0]);
                ctx.begin_path();
                ctx.move_to(x, divider_y);
                ctx.line_to(x + w, divider_y);
                ctx.stroke();
                ctx.set_line_dash(&[]);
                self.draw_calls += 1;

                if !alternative.label.is_empty() {
                    let fragment_font = fragment_font
                        .get_or_insert_with(|| sequence_fragment_font_css(&self.config));
                    ctx.set_fill_style(&self.config.label_color);
                    ctx.set_font(fragment_font.as_str());
                    ctx.set_text_align(TextAlign::Left);
                    ctx.set_text_baseline(TextBaseline::Bottom);
                    ctx.fill_text(
                        &format!("[{}]", alternative.label),
                        x + 6.0,
                        divider_y - 3.0,
                    );
                    self.draw_calls += 1;
                }
            }
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

            // Set edge style, honouring `linkStyle` (bd-lvj3).
            //
            // Every site in this loop passed `config.edge_stroke` unconditionally, so `linkStyle`
            // was discarded exactly the way `style`/`classDef` was on nodes before that half of the
            // bead landed. The colour is resolved ONCE here and threaded through the path, the UML
            // markers and the arrowheads below — an edge whose line obeyed the author while its
            // arrowhead kept the theme colour would be a worse bug than the one being fixed.
            let (declared_stroke, declared_width) = resolve_edge_style(ir, edge_path.edge_index);
            // The LABEL colour of this edge, resolved once for the same reason the stroke is: the
            // three label sites in this loop (the `|text|` label, its wrapped continuation lines,
            // and the `"1" --> "many"` secondary labels) all draw text belonging to THIS edge, so
            // they must agree about its colour. Resolving per site is how one branch silently
            // keeps the theme default.
            let declared_label_color = resolve_edge_label_color(ir, edge_path.edge_index);
            let sanitised_label_color = declared_label_color
                .as_deref()
                .and_then(sanitize_canvas_paint);
            let edge_label_fill = sanitised_label_color
                .as_deref()
                .unwrap_or(&self.config.label_color);
            let (legacy_width, dash_pattern) =
                legacy_edge_stroke(arrow, self.config.edge_stroke_width);
            let stroke = declared_stroke
                .as_deref()
                .unwrap_or(&self.config.edge_stroke);
            let stroke_width = declared_width.unwrap_or(legacy_width);
            // A declared dash OVERRIDES the arrow-derived one: `legacy_edge_stroke` infers a
            // pattern from the arrow glyph (`-.->` is dotted), and an explicit `stroke-dasharray`
            // is the author answering the same question more specifically.
            let declared_dash = resolve_edge_dash_array(ir, edge_path.edge_index);
            let dash: &[f64] = declared_dash.as_deref().unwrap_or(dash_pattern);

            // OPACITY wraps the whole edge — line, markers and label — as it does on an SVG
            // element. Restored beside the dash reset at the end of this iteration for the same
            // reason: `globalAlpha` is canvas STATE and would otherwise fade the rest of the
            // diagram.
            let edge_opacity = resolve_edge_opacity(ir, edge_path.edge_index);
            if let Some(alpha) = edge_opacity {
                ctx.set_global_alpha(alpha);
            }

            ctx.set_stroke_style(stroke);
            ctx.set_line_width(stroke_width);
            ctx.set_line_dash(dash);

            // Draw edge path
            ctx.begin_path();
            let first = &points[0];
            ctx.move_to(f64::from(first.x) + offset_x, f64::from(first.y) + offset_y);

            for point in points.iter().skip(1) {
                ctx.line_to(f64::from(point.x) + offset_x, f64::from(point.y) + offset_y);
            }
            ctx.stroke();
            self.draw_calls += 1;

            // ER crow's-foot cardinality (bd-hh0o7), BEFORE the UML marker path and not through it.
            //
            // ⚠️ `legacy_uml_markers` IS KEYED ON `ArrowType`, and ER cardinality is not an
            // ArrowType — it lives in `edge.er_notation()`. Adding the eight glyphs to
            // `draw_marker_primitive` alone therefore left them as DEAD CODE: nothing on this
            // surface ever asked for an `Er*` variant, and a draw-call test still passed because an
            // ER diagram has plenty of other things to draw. Caught by disarming the arm and
            // watching nothing fail.
            //
            // The start/end kinds come from `parse_er_cardinality_forms` + `MarkerKind::er_pair` —
            // the same pair fm-render-svg and the GPU plan select with, not a second table.
            if let Some(notation) = ir_edge.and_then(fm_core::IrEdge::er_notation) {
                let (left, right) = fm_core::parse_er_cardinality_forms(notation);
                let start = &points[0];
                let next = &points[1];
                let end = &points[points.len() - 1];
                let prev = &points[points.len() - 2];
                let placements = [
                    (
                        left.map(|form| MarkerKind::er_pair(form).0),
                        f64::from(start.x) + offset_x,
                        f64::from(start.y) + offset_y,
                        f64::from(next.y - start.y).atan2(f64::from(next.x - start.x)),
                    ),
                    (
                        right.map(|form| MarkerKind::er_pair(form).1),
                        f64::from(end.x) + offset_x,
                        f64::from(end.y) + offset_y,
                        f64::from(end.y - prev.y).atan2(f64::from(end.x - prev.x)),
                    ),
                ];
                for (kind, mx, my, angle) in placements {
                    let Some(kind) = kind else {
                        continue;
                    };
                    self.draw_calls += draw_er_cardinality_marker(
                        ctx,
                        kind,
                        mx,
                        my,
                        angle,
                        &self.config.node_fill,
                        stroke,
                    );
                }
            }

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
                        stroke,
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
                        stroke,
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
                        draw_circle_marker(ctx, ex, ey, 4.0, &self.config.node_fill, stroke);
                        self.draw_calls += 1;
                    }
                    ArrowType::Cross => {
                        draw_cross_marker(ctx, ex, ey, 8.0, stroke);
                        self.draw_calls += 1;
                    }
                    // All other arrow types (half arrows, stick arrows, etc.) — render as standard arrowhead.
                    _ => {
                        draw_arrowhead(ctx, ex, ey, angle, 10.0, stroke);
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

                    draw_arrowhead(ctx, sx, sy, start_angle, 10.0, stroke);
                    self.draw_calls += 1;
                }
            }

            // CARDINALITIES reach the canvas (bd-rk14).
            //
            // `"1" --> "many"` lives in `IrEdgeExtras`, not in `edge.label`, and this path drew the
            // label and nothing else — measured drawing in the SVG and absent from the canvas. Last
            // of the eight drops in that bead, and the canvas twin of bd-o2wf.
            //
            // Placed OUTSIDE the label block on purpose: an edge may carry cardinality and no label.
            // Each number goes by ITS OWN endpoint, since which end carries `1` and which carries
            // `many` is the entire content.
            //
            // Unlike the terminal, this needs no blank-cell search: the canvas has real coordinates
            // rather than a character grid, so the numbers are simply inset along the edge — the
            // same approach fm-render-svg takes.
            // ⚠️ ER CARDINALITY WAS DRAWN BY THE SVG ALONE. `}o--o|` declares "0..*" and "0..1",
            // fm-render-svg writes both, and this surface wrote neither — the relationship line
            // appeared with no cardinality at all, which is the bd-039t family (declared content one
            // renderer draws and another silently omits).
            //
            // Nothing new is placed: the class-diagram cardinality block below already insets a
            // label along the edge at each end, so ER only has to supply the text. The class values
            // take precedence where both somehow exist; in practice an edge carries one or the
            // other, since `er_notation` is set by the ER path and `*_cardinality` by the class one.
            if let Some(edge) = ir_edge.filter(|e| {
                e.source_cardinality().is_some()
                    || e.target_cardinality().is_some()
                    || e.er_cardinality_labels().is_some()
            }) && points.len() >= 2
            {
                // Resolved ONCE for the edge: both ends read it, and it parses the notation string.
                let er_labels = edge.er_cardinality_labels();
                let inset = self.config.font_size * 1.2;
                let font =
                    edge_label_font.get_or_insert_with(|| secondary_label_font_css(&self.config));
                ctx.set_font(font.as_str());
                ctx.set_fill_style(edge_label_fill);
                ctx.set_text_align(TextAlign::Center);
                ctx.set_text_baseline(TextBaseline::Middle);

                let mut place = |text: Option<&str>,
                                 from: &fm_layout::LayoutPoint,
                                 toward: &fm_layout::LayoutPoint| {
                    let Some(text) = text.filter(|t| !t.is_empty()) else {
                        return;
                    };
                    let (fx, fy) = (f64::from(from.x), f64::from(from.y));
                    let (tx, ty) = (f64::from(toward.x), f64::from(toward.y));
                    let (dx, dy) = (tx - fx, ty - fy);
                    let len = dx.hypot(dy);
                    // A zero-length segment has no direction to inset along; draw at the point.
                    let (ux, uy) = if len > 0.0 {
                        (dx / len, dy / len)
                    } else {
                        (0.0, 0.0)
                    };
                    ctx.fill_text(text, fx + ux * inset + offset_x, fy + uy * inset + offset_y);
                    self.draw_calls += 1;
                };

                let first = points[0];
                let second = points[1];
                let last = points[points.len() - 1];
                let penultimate = points[points.len() - 2];
                place(
                    edge.source_cardinality()
                        .or_else(|| er_labels.map(|(source, _)| source)),
                    &first,
                    &second,
                );
                place(
                    edge.target_cardinality()
                        .or_else(|| er_labels.map(|(_, target)| target)),
                    &last,
                    &penultimate,
                );
            }

            // Draw edge label if present
            if let Some(label_id) = ir_edge.and_then(|e| e.label)
                && let Some(label) = ir.labels.get(label_id.0)
                && points.len() >= 2
            {
                // C4Dynamic relationship labels are numbered at render time, matching Mermaid's
                // `drawRels` behavior and the SVG renderer. The IR keeps the author's label
                // untouched, so the same graph can still render unnumbered in C4Context.
                let label_text: Cow<'_, str> = if ir.diagram_type == DiagramType::C4Dynamic {
                    Cow::Owned(format!("{}: {}", edge_path.edge_index + 1, label.text))
                } else {
                    Cow::Borrowed(&label.text)
                };
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

                // A declared `font-size` takes a side path so the hoisted secondary-label font
                // is still what every undeclared edge draws under — same reasoning as the node
                // label, where the invariant `format!` is a landed lever.
                let declared_label_font =
                    resolve_edge_label_font(ir, edge_path.edge_index, &self.config);
                match declared_label_font.as_deref() {
                    Some(font) => ctx.set_font(font),
                    None => {
                        let edge_label_font = edge_label_font
                            .get_or_insert_with(|| secondary_label_font_css(&self.config));
                        ctx.set_font(edge_label_font.as_str());
                    }
                }

                let line_height = self.config.font_size * 1.2;

                // The common single-line edge label (`:has`, `-->|x|`, …) draws exactly `label.text` at
                // `ly`, so measure it directly and skip the `Vec<&str>` collect. Only a genuinely
                // multi-line label (`\n`) needs the split (it's re-read for max-width + count + per-line
                // draw). Byte-identical: for a `\n`-free label the sole `lines()` item IS `label.text`,
                // `total_height == line_height`, and `start_y == ly`.
                if !label_text.contains('\n') {
                    let label_width = ctx.measure_text(&label_text).width + 8.0;
                    let label_height = line_height + 4.0;

                    ctx.set_fill_style(&self.config.node_fill);
                    ctx.fill_rect(
                        lx - label_width / 2.0,
                        ly - label_height / 2.0,
                        label_width,
                        label_height,
                    );
                    self.draw_calls += 1;

                    ctx.set_fill_style(edge_label_fill);
                    // Same choice as the site above: declared font, else the hoisted one. All
                    // three label sites in this branch belong to ONE edge and must agree.
                    match declared_label_font.as_deref() {
                        Some(font) => ctx.set_font(font),
                        None => {
                            let hoisted = edge_label_font
                                .get_or_insert_with(|| secondary_label_font_css(&self.config));
                            ctx.set_font(hoisted.as_str());
                        }
                    }
                    ctx.set_text_align(TextAlign::Center);
                    ctx.set_text_baseline(TextBaseline::Middle);
                    ctx.fill_text(&label_text, lx, ly);
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                } else {
                    // Background for label
                    let lines: Vec<&str> = label_text.lines().collect();
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
                    ctx.set_fill_style(edge_label_fill);
                    // Same choice as the site above: declared font, else the hoisted one. All
                    // three label sites in this branch belong to ONE edge and must agree.
                    match declared_label_font.as_deref() {
                        Some(font) => ctx.set_font(font),
                        None => {
                            let hoisted = edge_label_font
                                .get_or_insert_with(|| secondary_label_font_css(&self.config));
                            ctx.set_font(hoisted.as_str());
                        }
                    }
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
            if edge_opacity.is_some() {
                ctx.set_global_alpha(1.0);
            }
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

            // Draw shape, honouring the author's own styling (bd-lvj3).
            //
            // This passed `config.node_fill`/`node_stroke` unconditionally, so `style a fill:#f00`,
            // `classDef` and every other declared colour was discarded: measured, the canvas emitted
            // only 3-4 distinct fills for an ENTIRE diagram no matter what the source said. Counted
            // at the time — fm-render-svg reads `inline_style` 30 times, `classes` 19 and
            // `style_refs` 11; this crate read all three ZERO times.
            let (fill, stroke) = resolve_node_colors(ir, node_box.node_index);
            // Resolved ONCE for the whole node: the four label sites below (plain label, class
            // compartments, ER entity rows, requirement rows) all draw text belonging to THIS
            // node, so they must agree about its colour. Resolving per-site would let a later
            // branch silently keep the theme default.
            // SANITISED, like every other paint this renderer forwards. A canvas silently IGNORES
            // an unparsable `fillStyle` and keeps the PREVIOUS colour, so forwarding junk paints
            // the text in whatever was drawn last — a position-dependent wrong colour rather than
            // a visible failure. The cluster SHAPE path has sanitised since it landed; the label
            // paths did not, which is the same asymmetric-sibling shape as the properties above.
            let declared_text_color = resolve_node_text_color(ir, node_box.node_index);
            let text_color = declared_text_color
                .as_deref()
                .and_then(sanitize_canvas_paint);
            let label_fill = text_color.as_deref().unwrap_or(&self.config.label_color);
            // The border WIDTH is the third channel of the same declaration and was the last one
            // still discarded here: the edge resolver has read `stroke-width` since bd-lvj3's edge
            // half landed, while every node border was drawn at the config width whatever the
            // author wrote. `unwrap_or` keeps the theme default when nothing was declared.
            let stroke_width = resolve_node_stroke_width(ir, node_box.node_index)
                .unwrap_or(self.config.node_stroke_width);
            // The BORDER DASH is the fourth channel, and the only one that has to be UNDONE.
            // `lineDash` is canvas STATE, not a draw argument: it persists until something sets it
            // again, so a dashed node would leave every later shape dashed too. Set only when the
            // author declared one, and cleared immediately after, so an undeclared node is drawn
            // under exactly the state it was drawn under before this existed.
            // OPACITY wraps the WHOLE node — shape and every label — because that is what the
            // SVG arm does: `opacity` on an element fades the element, not just its outline. So
            // this is set BEFORE the shape and restored at the END of the iteration, after the
            // compartment and label branches below have drawn.
            //
            // `globalAlpha` is canvas STATE, like `lineDash`. Left set, it fades every subsequent
            // node, edge and label in the diagram — so the restore is not optional, and the
            // control that matters asserts a later node is NOT faded.
            let declared_opacity = resolve_node_opacity(ir, node_box.node_index);
            if let Some(alpha) = declared_opacity {
                ctx.set_global_alpha(alpha);
            }
            let declared_font = resolve_node_font(ir, node_box.node_index, &self.config);
            // The COMPARTMENT fonts derive different defaults from the plain label — the heading is
            // bold at the theme size, the member rows are 0.9x — so they are composed separately
            // from the same declared components. Only built when the author declared something, so
            // an undeclared node still uses the hoisted pair and costs nothing.
            let declared_parts = resolve_declared_node_font(ir, node_box.node_index);
            let (declared_heading_font, declared_member_font) =
                if declared_parts.declares_anything() {
                    (
                        Some(declared_parts.compose(
                            self.config.font_size,
                            Some("bold"),
                            &self.config.font_family,
                        )),
                        Some(declared_parts.compose(
                            self.config.font_size * 0.9,
                            None,
                            &self.config.font_family,
                        )),
                    )
                } else {
                    (None, None)
                };
            let dash = resolve_node_stroke_dasharray(ir, node_box.node_index);
            if let Some(ref pattern) = dash {
                ctx.set_line_dash(pattern);
            }
            draw_shape(
                ctx,
                shape,
                x,
                y,
                w,
                h,
                fill.as_deref().unwrap_or(&self.config.node_fill),
                stroke.as_deref().unwrap_or(&self.config.node_stroke),
                stroke_width,
            );
            if dash.is_some() {
                ctx.set_line_dash(&[]);
            }
            self.draw_calls += 1;

            // Check for class diagram three-compartment rendering.
            if let Some(node) = ir_node
                && let Some(ref meta) = node.class_meta
                && (!meta.attributes.is_empty()
                    || !meta.methods.is_empty()
                    || meta.stereotype.is_some())
            {
                let line_h = self.config.font_size * 1.3;
                let member_font = self.config.font_size * 0.9;
                let padding = 6.0;

                ctx.set_fill_style(label_fill);
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
                match declared_heading_font.as_deref() {
                    Some(font) => ctx.set_font(font),
                    None => ctx.set_font(class_fonts.0.as_str()),
                }
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
                    let stereo_text = stereotype.label();
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
                match declared_member_font.as_deref() {
                    Some(font) => ctx.set_font(font),
                    None => ctx.set_font(class_fonts.1.as_str()),
                }
                ctx.set_text_align(TextAlign::Left);
                for attr in &meta.attributes {
                    if cursor_y > y + h - line_h * 0.5 {
                        break;
                    }
                    let text = class_member_row(attr, false);
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
                let base_member_font = match declared_member_font.as_deref() {
                    Some(font) => font.to_string(),
                    None => class_fonts.1.clone(),
                };
                for method in &meta.methods {
                    if cursor_y > y + h - member_font * 0.5 {
                        break;
                    }
                    let text = class_member_row(method, true);
                    // ⚠️ THE CLASSIFIER IS A STYLE HERE TOO (bd-r2gll). `class_member_row` no longer
                    // puts `$`/`*` in the text, so this backend has to draw the marker or the
                    // static/abstract distinction is simply LOST — which is worse than the literal
                    // character it replaced. Canvas2D has no `text-decoration`, so the underline is
                    // a measured rule; italic goes through the font string.
                    let classifier =
                        fm_core::class_member_classifier_css(method.is_static, method.is_abstract);
                    if classifier == Some("font-style:italic") {
                        ctx.set_font(&format!("italic {base_member_font}"));
                    }
                    ctx.fill_text(&text, x + padding, cursor_y);
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                    if classifier == Some("font-style:italic") {
                        ctx.set_font(&base_member_font);
                    } else if classifier == Some("text-decoration:underline") {
                        let width = ctx.measure_text(&text).width;
                        let baseline = cursor_y + member_font * 0.15;
                        ctx.begin_path();
                        ctx.move_to(x + padding, baseline);
                        ctx.line_to(x + padding + width, baseline);
                        ctx.stroke();
                        self.draw_calls += 1;
                    }
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

                ctx.set_fill_style(label_fill);
                ctx.set_text_baseline(TextBaseline::Middle);

                let entity_name = node
                    .label
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|l| l.text.as_str())
                    .unwrap_or(&node.id);

                let class_fonts = class_compartment_fonts
                    .get_or_insert_with(|| class_compartment_font_css(&self.config));
                match declared_heading_font.as_deref() {
                    Some(font) => ctx.set_font(font),
                    None => ctx.set_font(class_fonts.0.as_str()),
                }
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

                ctx.set_text_align(TextAlign::Left);
                // ER attribute cells at shared COLUMN offsets (bd-jbrzc), not one fused run.
                //
                // ⚠️ THE BOX IS SIZED FROM THE COLUMN GEOMETRY. fm-layout measures an entity with
                // `fm_core::er_cell_columns`, so a surface drawing fused rows is measured by a rule
                // it does not follow: the reserved width goes unused and the fields do not line up,
                // while the SVG surface — sized by the same rule — aligns them. Measured on the
                // skew fixture `T { verylongtypename a / t verylongattributename PK }`: SVG puts
                // both second cells at x=195.39, the canvas put them wherever the first cell
                // happened to end.
                //
                // ⚠️ THE OFFSETS COME FROM `fm_core::er_cell_columns`, NOT FROM LOCAL ARITHMETIC.
                // That helper exists precisely so layout and every renderer agree; a hand-rolled
                // copy here is how a cell ends up drawn outside the box that was sized for it.
                // ⚠️ THE ATTRIBUTE FONT IS `* 0.8`, NOT THIS SURFACE'S USUAL `* 0.9`, and the
                // difference is not cosmetic. fm-layout sizes the entity box with
                // `attr_font_size = node_font_size * 0.8` (floored at 8.0) and fm-render-svg draws
                // at the same, so a canvas measuring its columns at 0.9 builds a table ~12% wider
                // than the box it must fit in. Measured on the skew fixture the moment the columns
                // went in: `PK` landed at x=297.9 against a box ending at 257.68 — the same
                // sixty-pixel spill fm-layout's own comment records from the SVG side, reproduced
                // here because the fused row had been hiding the font disagreement all along (a
                // fused row is never wider than the columns, so it fits a box it does not match).
                let metrics = self.config.font_metrics();
                let metrics_size = metrics.font_size();
                // ⚠️ DERIVED FROM `metrics.font_size()`, NOT FROM `self.config.font_size`. fm-layout
                // sizes the box with `node_font_size = metrics.font_size()`, so measuring from a
                // different base produces a different scale and therefore different columns — which
                // is what put `PK` 13px outside the box on the first attempt at this fix.
                let attr_font = (f64::from(metrics_size) * 0.8).max(f64::from(ER_ATTR_FONT_FLOOR));
                let scale = if metrics_size > 0.0 {
                    (attr_font / f64::from(metrics_size)) as f32
                } else {
                    1.0
                };
                let (cell_offsets, _right_edge) = fm_core::er_cell_columns(
                    &node.members,
                    &metrics,
                    scale,
                    fm_core::er_cell_gutter(attr_font as f32),
                );
                // ⚠️ THE TEXT IS DRAWN AT THE FONT THE COLUMNS WERE MEASURED AT. This block used to
                // set the class-compartment member font (`config.font_size * 0.9` = 12.6) while the
                // columns are built at the ER attribute font (`* 0.8` = 11.2) — so every glyph was
                // ~12% wider than the column reserved for it and `verylongtypename` overran its own
                // gutter into the name column. Invisible while the row was one fused run, because a
                // fused run has no columns to overrun. A declared font override still wins, as it
                // does everywhere else on this surface.
                match declared_member_font.as_deref() {
                    Some(font) => ctx.set_font(font),
                    None => ctx.set_font(&format!("{attr_font}px {}", self.config.font_family)),
                }
                for attr in &node.members {
                    if cursor_y > y + h - member_font * 0.5 {
                        // Out of box: stop rather than draw rows past the entity.
                        break;
                    }
                    // Same four cells, in the same order, as both SVG writers use.
                    let key = attr.key_cell();
                    let cells: [&str; 4] = [
                        attr.data_type.as_str(),
                        attr.name.as_str(),
                        key.as_ref(),
                        attr.comment.as_deref().unwrap_or(""),
                    ];
                    let mut drew_any = false;
                    for (index, cell) in cells.iter().enumerate() {
                        if cell.is_empty() {
                            continue;
                        }
                        // ⚠️ ANCHORED AT `x + 8.0`, NOT `x + padding`. fm-layout reserves exactly
                        // eight pixels on each side (`row_width + 16.0`) and both SVG writers draw
                        // from `x + 8.0`; starting two pixels earlier would shift every column and
                        // leave ten on the right, which is the kind of slack that hides a real
                        // overflow until the widest entity comes along.
                        ctx.fill_text(
                            cell,
                            x + ER_ROW_PADDING + f64::from(cell_offsets[index]),
                            cursor_y,
                        );
                        self.draw_calls += 1;
                        drew_any = true;
                    }
                    if drew_any {
                        *labels_drawn += 1;
                    }
                    cursor_y += member_font * 1.2;
                }
            } else if let Some((node, meta)) =
                ir_node.and_then(|n| n.requirement_meta.as_deref().map(|m| (n, m)))
            {
                // REQUIREMENT rows (bd-rk14) — canvas twin of the terminal fix in bd-039t.
                // Measured: `requirement R { id: 1 / text: hello }` drew `hello` in the SVG and not
                // on the canvas. Field order matches the SVG's row order.
                let line_h = self.config.font_size * 1.3;
                let member_font = self.config.font_size * 0.9;
                let padding = 6.0;

                ctx.set_fill_style(label_fill);
                ctx.set_text_baseline(TextBaseline::Middle);
                let name = node
                    .label
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|l| l.text.as_str())
                    .unwrap_or(&node.id);
                let fonts = class_compartment_fonts
                    .get_or_insert_with(|| class_compartment_font_css(&self.config));
                match declared_heading_font.as_deref() {
                    Some(font) => ctx.set_font(font),
                    None => ctx.set_font(fonts.0.as_str()),
                }
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

                match declared_member_font.as_deref() {
                    Some(font) => ctx.set_font(font),
                    None => ctx.set_font(fonts.1.as_str()),
                }
                ctx.set_text_align(TextAlign::Left);
                // `type`/`doc` are an ELEMENT's fields (bd-qdmn), in the SVG's row order so all
                // three backends read alike — ID, Text, Type, Doc, then Risk and Verify.
                for (prefix, value) in [
                    ("id: ", meta.req_id.as_deref()),
                    ("text: ", meta.text.as_deref()),
                    ("type: ", meta.element_type.as_deref()),
                    ("doc: ", meta.doc_ref.as_deref()),
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

                ctx.set_fill_style(label_fill);
                ctx.set_text_baseline(TextBaseline::Middle);
                let name = node
                    .label
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|l| l.text.as_str())
                    .unwrap_or(&node.id);
                let fonts = class_compartment_fonts
                    .get_or_insert_with(|| class_compartment_font_css(&self.config));
                match declared_heading_font.as_deref() {
                    Some(font) => ctx.set_font(font),
                    None => ctx.set_font(fonts.0.as_str()),
                }
                ctx.set_text_align(TextAlign::Center);
                let mut cursor_y = y + line_h;
                ctx.fill_text(name, x + w / 2.0, cursor_y);
                self.draw_calls += 1;
                *labels_drawn += 1;
                cursor_y += line_h * 0.5;

                match declared_member_font.as_deref() {
                    Some(font) => ctx.set_font(font),
                    None => ctx.set_font(fonts.1.as_str()),
                }
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

                    // ⚠️ THE FIFTH SITE, and the one that mattered most. When bd-lvj3 bound
                    // `label_fill` it converted the four COMPARTMENT label sites (class, ER,
                    // requirement, C4) and missed this one — the PLAIN node label, i.e. every
                    // ordinary flowchart node. The bounded edit asserted an exact count of four and
                    // the count was honest; the SCOPE was wrong, so `style a color:#ff0000` and
                    // `classDef … color:` resolved correctly and were then thrown away one line
                    // before the text was drawn. Only compiling the tests found it.
                    ctx.set_fill_style(label_fill);
                    // A declared `font-size` is applied HERE ONLY, and the hoist is preserved for
                    // everyone else. `standard_label_font` is formatted once for the whole diagram
                    // (a landed lever — the invariant `format!` used to run per node), and a
                    // per-node font string would undo that for every diagram to serve the rare one
                    // that declares a size. So the declaration takes a side path and an undeclared
                    // node still draws under the identical hoisted string.
                    //
                    // ⚠️ SCOPE, stated because the last bounded edit on this very line got it
                    // wrong: this is the PLAIN node label only. The class/ER/requirement/C4
                    // COMPARTMENT labels derive their own smaller fonts and are NOT rescaled, so a
                    // `font-size` on a class node still disagrees with the SVG arm, which cascades
                    // it to the whole element. That is recorded as open rather than half-fixed.
                    match declared_font.as_deref() {
                        Some(font) => ctx.set_font(font),
                        None => ctx.set_font(
                            standard_label_font
                                .get_or_insert_with(|| standard_node_font(&self.config)),
                        ),
                    }
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

            // Restore before the next node. Only when this node set it, so an undeclared diagram
            // emits no alpha operations at all and is drawn under exactly the state it was drawn
            // under before this feature existed.
            if declared_opacity.is_some() {
                ctx.set_global_alpha(1.0);
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
/// Left inset of an ER attribute row, mirroring the `row_width + 16.0` fm-layout reserves and the
/// `x + 8.0` both SVG writers draw from.
const ER_ROW_PADDING: f64 = 8.0;

/// Lower bound on the ER attribute font size, mirroring `fm-layout`'s `ER_ATTR_FONT_FLOOR`.
///
/// Duplicated as a literal rather than imported because fm-layout keeps it private; the value is
/// load-bearing in one direction only — it raises the size, which widens the box, so a disagreement
/// here over-sizes rather than spills.
const ER_ATTR_FONT_FLOOR: f32 = 8.0;

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
        // ER crow's-foot cardinality (bd-dun16 for the SVG half, bd-hh0o7 for this one). These
        // eight arms used to return 0 DELIBERATELY: `GpuMarkerKind`'s fallback is `Arrow`, and an
        // arrowhead where a crow's foot belongs does not read as "cardinality unavailable", it
        // reads as a DIFFERENT cardinality — the diagram states something false rather than
        // something incomplete. Now that the eight real glyphs exist, that reasoning is satisfied
        // by drawing the right shape rather than by drawing none.
        MarkerKind::ErOnlyOneStart
        | MarkerKind::ErOnlyOneEnd
        | MarkerKind::ErZeroOrOneStart
        | MarkerKind::ErZeroOrOneEnd
        | MarkerKind::ErOneOrMoreStart
        | MarkerKind::ErOneOrMoreEnd
        | MarkerKind::ErZeroOrMoreStart
        | MarkerKind::ErZeroOrMoreEnd => {
            draw_er_cardinality_marker(ctx, marker, x, y, angle, node_fill, stroke_color)
        }
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
/// `pub(crate)` so `gpu_plan` can carry the SAME width the Canvas2D pass strokes with. Forking this
/// mapping was the alternative and a worse one: a duplicated helper drifts silently, and then the
/// GPU plan claims a width the raster path never used.
/// Accept a colour only if it is one this renderer can safely hand to a canvas context.
///
/// fm-render-svg has `sanitize_svg_paint`, but it is `pub(crate)` there and answers a different
/// question -- what is safe inside an SVG attribute. A canvas `fillStyle` is not markup, so the risk
/// is not injection but a malformed value silently blanking a shape: browsers ignore an unparsable
/// fillStyle and keep the PREVIOUS colour, which would paint this cluster with whatever was drawn
/// last. Refusing early and falling back to the theme colour is the visible failure, not the silent
/// one.
///
/// Deliberately conservative: hex, the four functional notations, and bare keywords. Anything
/// carrying a quote, semicolon, backslash, or control character is refused outright.
fn sanitize_canvas_paint(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 64 {
        return None;
    }
    if value
        .bytes()
        .any(|b| b < 0x20 || matches!(b, b'"' | b'\'' | b';' | b'\\' | b'<' | b'>'))
    {
        return None;
    }

    if let Some(hex) = value.strip_prefix('#') {
        let ok = matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit());
        return ok.then(|| value.to_ascii_lowercase());
    }

    for prefix in ["rgb(", "rgba(", "hsl(", "hsla("] {
        if let Some(rest) = value.to_ascii_lowercase().strip_prefix(prefix) {
            let args = rest.strip_suffix(')')?;
            let ok = !args.is_empty()
                && args
                    .bytes()
                    .all(|b| b.is_ascii_digit() || matches!(b, b',' | b'.' | b'%' | b' ' | b'-'));
            return ok.then(|| value.to_ascii_lowercase());
        }
    }

    value
        .bytes()
        .all(|b| b.is_ascii_alphabetic())
        .then(|| value.to_ascii_lowercase())
}

/// Resolve an edge's stroke colour and width from the author's own styling (bd-lvj3).
///
/// The edge twin of [`resolve_node_colors`], and a free function for the same reason: the
/// WebGPU plan needs the identical answer for its segment buffer.
///
/// The edge twin of [`resolve_node_colors`]. Three channels, merged in the order mermaid
/// applies them — later wins:
///   1. `linkStyle default stroke:#f00`  (`IrStyleTarget::LinkDefault`)
///   2. `linkStyle 3 stroke:#f00`        (`IrStyleTarget::Link(index)`)
///   3. the edge's own `inline_style`
///
/// `LinkDefault` must be merged FIRST so a per-index `linkStyle` overrides it rather than the
/// other way round — that ordering is the whole point of having a default.
///
/// Returns `None` per channel when nothing was declared, so the caller keeps the theme colour
/// and the arrow-derived width from `legacy_edge_stroke` instead of a value invented here.
/// A cluster's declared fill and stroke from `style mySubgraph fill:#f00` (bd-xfmm).
///
/// bd-xfmm could only WARN about a subgraph style, because `IrStyleTarget` had no `Cluster`
/// variant. The variant landed with the SVG consumer; this is the canvas half, so the two backends
/// stop disagreeing about a document the author styled.
///
/// Only `IrStyleTarget::Cluster` is consulted. A cluster has no `classes` and no `inline_style` of
/// its own - the node resolver's other two channels do not exist here - so merging them would be
/// inventing a cascade the IR does not have.
///
/// Returns `None` per channel when nothing was declared, so the caller keeps whatever it would
/// otherwise have used rather than a colour resolved from an empty map.
/// The merged `style` declaration for one cluster.
///
/// Factored out of [`resolve_cluster_colors`] so a second property reads off the SAME chain, the
/// shape `merged_node_style` and `merged_edge_style` already have. Only `IrStyleTarget::Cluster`
/// is consulted: a cluster has no `classes` and no `inline_style`, so merging those would invent a
/// cascade the IR does not have.
fn merged_cluster_style(
    ir: &MermaidDiagramIr,
    cluster_index: usize,
) -> std::collections::BTreeMap<String, String> {
    let mut merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();

    for style_ref in &ir.style_refs {
        if let fm_core::IrStyleTarget::Cluster(target) = style_ref.target
            && target == cluster_index
        {
            merged.extend(fm_core::parse_style_string(&style_ref.style).properties);
        }
    }

    merged
}

/// The author's declared cluster LABEL colour, if any (bd-lvj3).
///
/// The third surface to learn `color`, after nodes and edges. Measured against the SVG arm, which
/// emits the declaration for `style one color:#ff00ff`, this renderer drew every subgraph title in
/// `config.label_color` regardless.
pub(crate) fn resolve_cluster_text_color(
    ir: &MermaidDiagramIr,
    cluster_index: usize,
) -> Option<String> {
    merged_cluster_style(ir, cluster_index)
        .get("color")
        .cloned()
}

/// The author's declared cluster BORDER WIDTH, if any (bd-lvj3).
///
/// The cluster draw hardcoded `set_line_width(1.0)`, so `style one stroke-width:5px` was discarded
/// while the SVG arm emits it. Reads the same merge the cluster's colours already use.
pub(crate) fn resolve_cluster_stroke_width(
    ir: &MermaidDiagramIr,
    cluster_index: usize,
) -> Option<f64> {
    parse_stroke_width(
        merged_cluster_style(ir, cluster_index)
            .get("stroke-width")
            .map(String::as_str),
    )
}

/// The author's declared cluster BORDER DASH, if any (bd-lvj3).
///
/// Third surface to learn `stroke-dasharray`, after nodes and edges, and it reuses their parser so
/// all three share one set of refusals rather than drifting.
pub(crate) fn resolve_cluster_dash_array(
    ir: &MermaidDiagramIr,
    cluster_index: usize,
) -> Option<Vec<f64>> {
    parse_dash_array(
        merged_cluster_style(ir, cluster_index)
            .get("stroke-dasharray")
            .map(String::as_str),
    )
}

pub(crate) fn resolve_cluster_colors(
    ir: &MermaidDiagramIr,
    cluster_index: usize,
) -> (Option<String>, Option<String>) {
    let merged = merged_cluster_style(ir, cluster_index);

    (merged.get("fill").cloned(), merged.get("stroke").cloned())
}

/// The merged `linkStyle`/inline declaration for one edge.
///
/// Factored out of [`resolve_edge_style`] so a second property reads off the SAME chain rather
/// than a second copy of it — the shape `merged_node_style` already has on the node side.
/// [`resolve_edge_style`] keeps its exact signature, which matters because `gpu_plan.rs` calls it
/// and must get the identical answer for its segment buffer.
fn merged_edge_style(
    ir: &MermaidDiagramIr,
    edge_index: usize,
) -> std::collections::BTreeMap<String, String> {
    let mut merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();

    for style_ref in &ir.style_refs {
        if matches!(style_ref.target, fm_core::IrStyleTarget::LinkDefault) {
            merged.extend(fm_core::parse_style_string(&style_ref.style).properties);
        }
    }

    for style_ref in &ir.style_refs {
        if let fm_core::IrStyleTarget::Link(target) = style_ref.target
            && target == edge_index
        {
            merged.extend(fm_core::parse_style_string(&style_ref.style).properties);
        }
    }

    if let Some(edge) = ir.edges.get(edge_index)
        && let Some(inline) = edge.inline_style.as_ref()
    {
        merged.extend(inline.properties.clone());
    }

    merged
}

/// The author's declared EDGE LABEL colour, if any (bd-lvj3).
///
/// `linkStyle 0 color:#f00` sets the colour of the edge's LABEL, not of its line — `stroke` is the
/// line. Measured against the SVG arm, which emits the declaration and lets the browser cascade
/// it, this renderer drew every edge label in `config.label_color` regardless.
///
/// The node twin is `resolve_node_text_color`, and `color` is the same property there: in
/// fm-render-svg's `split_style_properties` it is the single TEXT_STYLE_PROPERTIES entry that gets
/// RENAMED onto the text element's `fill` rather than passed through. A canvas has no cascade, so
/// it has to be resolved here and handed to `set_fill_style` before the text is drawn.
///
/// Returns `None` when nothing was declared, so the caller keeps the theme's `label_color`.
pub(crate) fn resolve_edge_label_color(ir: &MermaidDiagramIr, edge_index: usize) -> Option<String> {
    merged_edge_style(ir, edge_index).get("color").cloned()
}

/// The author's declared EDGE DASH, if any (bd-lvj3).
///
/// The edge twin of `resolve_node_stroke_dasharray`, and the last dash channel. Measured against
/// the SVG arm, which emits `stroke-dasharray:7 3` for `linkStyle 0 stroke-dasharray:7 3`, this
/// renderer used the ARROW-DERIVED pattern from `legacy_edge_stroke` regardless — so a solid
/// `-->` the author asked to be dashed stayed solid, and a `-.->` the author asked to be solid
/// stayed dotted.
///
/// The declared pattern OVERRIDES the arrow-derived one rather than merging with it: they are two
/// answers to the same question, and an explicit `stroke-dasharray` is the author being more
/// specific than the arrow glyph. Same precedence `stroke` and `stroke-width` already use here.
pub(crate) fn resolve_edge_dash_array(
    ir: &MermaidDiagramIr,
    edge_index: usize,
) -> Option<Vec<f64>> {
    parse_dash_array(
        merged_edge_style(ir, edge_index)
            .get("stroke-dasharray")
            .map(String::as_str),
    )
}

pub(crate) fn resolve_edge_style(
    ir: &MermaidDiagramIr,
    edge_index: usize,
) -> (Option<String>, Option<f64>) {
    let merged = merged_edge_style(ir, edge_index);
    let width = parse_stroke_width(merged.get("stroke-width").map(String::as_str));

    (merged.get("stroke").cloned(), width)
}

/// Parse a CSS `stroke-width` declaration into a device width.
///
/// `2px` and a bare `2` must both parse, because both are valid in a mermaid `style` directive.
///
/// A malformed, non-finite or non-positive value yields `None` so the CALLER'S default stands. That
/// is deliberate and load-bearing in both directions: a declared style must never be able to make a
/// border vanish, and `NaN` or a negative reaching `set_line_width` is a draw call a canvas ignores
/// silently — the element would simply not be there, with nothing in the output to say why.
///
/// Shared by the edge and node resolvers rather than written twice. The two differ only in which
/// merge chain they read, and a forked copy is the duplicated-helper trap this repo has paid for
/// before: the first one to learn about a new unit would leave the other behind, and nobody would
/// see it until an edge and a node disagreed about the same declaration.
fn parse_stroke_width(raw: Option<&str>) -> Option<f64> {
    raw.and_then(|raw| {
        raw.trim()
            .trim_end_matches("px")
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|w| w.is_finite() && *w > 0.0)
    })
}

/// Resolve a node's fill and stroke from the author's own styling (bd-lvj3).
///
/// Three channels, merged in the order mermaid applies them — later wins:
///   1. `classDef` definitions named by the node's `classes`
///   2. `style <id> ...` directives targeting this node (`ir.style_refs`)
///   3. the node's own `inline_style`
///
/// fm-render-svg gets `classDef` support for free by emitting a CSS class and letting the BROWSER
/// cascade it. A canvas has no cascade, so the class must be resolved here — which is why porting
/// the SVG helper would not have worked: it returns CSS strings, not colours.
///
/// Returns `None` per channel when the author declared nothing, so the caller keeps its own theme
/// default rather than being handed a colour this function invented.
///
/// A FREE function, not a `Canvas2dRenderer` method, because the WebGPU plan needs the identical
/// answer (bd-2u0.2 requires fill and stroke ON each node instance). It never used `self`, and a
/// forked second copy is the duplicated-helper trap this repo has paid for before: the two
/// renderers would drift silently, and the disagreement would look like a GPU bug.
/// Every style declaration applying to this node, merged in the order mermaid applies them.
///
/// Factored out of `resolve_node_colors` so a second property can be read off the SAME chain
/// without a second copy of the merge. Two copies drift the moment one learns about a new channel,
/// and the ORDER is the part that carries meaning: classDef first, then `style` directives, then
/// the node's own inline style, so the later declaration wins.
fn merged_node_style(
    ir: &MermaidDiagramIr,
    node_index: usize,
) -> std::collections::BTreeMap<String, String> {
    let mut merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let Some(node) = ir.nodes.get(node_index) else {
        return merged;
    };

    for class_name in &node.classes {
        if let Some(def) = ir.style_defs.iter().find(|d| &d.name == class_name) {
            merged.extend(def.properties.clone());
        }
    }

    for style_ref in &ir.style_refs {
        if let fm_core::IrStyleTarget::Node(target) = style_ref.target
            && target == fm_core::IrNodeId(node_index)
        {
            merged.extend(fm_core::parse_style_string(&style_ref.style).properties);
        }
    }

    if let Some(inline) = node.inline_style.as_ref() {
        merged.extend(inline.properties.clone());
    }

    merged
}

pub(crate) fn resolve_node_colors(
    ir: &MermaidDiagramIr,
    node_index: usize,
) -> (Option<String>, Option<String>) {
    let merged = merged_node_style(ir, node_index);
    (merged.get("fill").cloned(), merged.get("stroke").cloned())
}

/// The author's declared TEXT colour for this node, if any (bd-lvj3).
///
/// `style a color:#f00`, a `classDef` carrying `color:`, or an inline style all set the LABEL
/// colour, and this renderer drew every label in `config.label_color` regardless - the same shape
/// as the fill/stroke half of bd-lvj3, one channel later.
///
/// The property is `color`, and fm-render-svg maps it onto the text element's `fill`: in
/// `split_style_properties`, `color` is the single TEXT_STYLE_PROPERTIES entry that gets RENAMED
/// rather than passed through. A canvas has no cascade, so the same declaration has to be resolved
/// here and handed to `set_fill_style` before the text is drawn.
///
/// Returns `None` when the author declared nothing, so the caller keeps the theme's `label_color`
/// instead of a colour this function invented.
pub(crate) fn resolve_node_text_color(ir: &MermaidDiagramIr, node_index: usize) -> Option<String> {
    merged_node_style(ir, node_index).get("color").cloned()
}

/// The author's declared node BORDER WIDTH, if any (bd-lvj3).
///
/// The edge half of this bead already reads `stroke-width` off its merge chain; the node half did
/// not, so `style a stroke-width:4px` and a `classDef` carrying one were both discarded and every
/// node border was drawn at `config.node_stroke_width` regardless. Same three channels, same merge
/// order, one property later — the asymmetric-sibling shape, where two halves of one feature were
/// written at different times and only one learned the property.
///
/// fm-render-svg needs no equivalent: it emits the declaration as CSS and the browser applies it.
/// A canvas has no cascade, which is why every one of these channels has to be resolved here.
/// Parse an SVG `stroke-dasharray` into a canvas dash pattern.
///
/// SVG accepts commas, whitespace, or both as separators (`5 5`, `5,5`, `5, 5`), so both are
/// treated as separators rather than guessing one. `none` is SVG's explicit "solid" and returns
/// `None`, which is the same answer as "nothing declared" and correctly means "keep the default".
///
/// Rejected rather than forwarded: any component that does not parse, is negative, or is
/// non-finite, and an ALL-ZERO pattern. The last one matters because `[0, 0]` is not a no-op on a
/// canvas — the specification makes a zero-length dash pattern render as though no dash were set
/// on some paths and can stall rasterisation on others, so a junk declaration would be strictly
/// worse than ignoring it. This mirrors `parse_stroke_width`, which refuses for the same reason.
fn parse_dash_array(raw: Option<&str>) -> Option<Vec<f64>> {
    let raw = raw?.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("none") {
        return None;
    }

    let mut pattern = Vec::new();
    for part in raw.split([',', ' ', '\t']).filter(|part| !part.is_empty()) {
        let value = part
            .trim()
            .trim_end_matches("px")
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value >= 0.0)?;
        pattern.push(value);
    }

    if pattern.is_empty() || pattern.iter().all(|value| *value == 0.0) {
        return None;
    }
    Some(pattern)
}

/// The author's declared node BORDER DASH, if any (bd-lvj3).
///
/// The fourth channel of the same declaration. `style a stroke-dasharray:5 5` and a `classDef`
/// carrying one reached fm-render-svg — measured, it emits `stroke-dasharray:5 5` — and were
/// dropped here, so a node the author asked to be dashed drew a solid border.
///
/// Same asymmetric-sibling shape as `stroke-width`: the EDGE path has drawn dashes since the edge
/// half landed (`with_canvas_dash_f64` beside `legacy_edge_stroke`), and the node path never
/// learned the property.
/// The author's declared node FONT SIZE in px, if any (bd-lvj3).
///
/// Measured against the SVG arm, which emits `font-size:32px` for `style a font-size:32px` and
/// lets the browser apply it, while this renderer drew every label at `config.font_size`.
///
/// Refuses what it cannot use, for the same reason `parse_stroke_width` does: a canvas given a
/// non-finite or absurd font size draws nothing at all, which is worse than ignoring the
/// declaration. The upper bound is deliberately generous — it exists to stop a typo'd `font-size:
/// 100000` from producing an invisible diagram, not to second-guess a legitimately large heading.
fn parse_declared_font_size(raw: Option<&str>) -> Option<f64> {
    raw.and_then(|raw| {
        raw.trim()
            .trim_end_matches("px")
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|size| size.is_finite() && *size > 0.0 && *size <= 512.0)
    })
}

/// The author's declared node OPACITY, if any (bd-lvj3).
///
/// Measured against the SVG arm, which emits `opacity:0.5` for `style a opacity:0.5` and lets the
/// browser apply it to the whole element, while this renderer drew every node fully opaque.
///
/// `0.0` is ACCEPTED rather than treated as junk: a fully transparent node is a thing an author
/// can legitimately ask for, and SVG honours it. Anything outside `0..=1` or non-finite is
/// refused, because `globalAlpha` outside that range is ignored by a canvas — which would leave
/// the PREVIOUS alpha in force and fade whatever came next instead.
/// Parse a CSS `opacity` into a canvas `globalAlpha`.
///
/// Shared by the node, edge and cluster resolvers so all three refuse the same things rather than
/// drifting apart — the same reason `parse_dash_array` is shared.
///
/// `0.0` is ACCEPTED: a fully transparent element is something an author can legitimately ask for,
/// and SVG honours it. Anything outside `0..=1` or non-finite is refused, because `globalAlpha`
/// outside that range is IGNORED by a canvas — which leaves the PREVIOUS alpha in force and fades
/// whatever comes next instead.
fn parse_opacity(raw: Option<&str>) -> Option<f64> {
    raw.and_then(|raw| {
        raw.trim()
            .parse::<f64>()
            .ok()
            .filter(|alpha| alpha.is_finite() && (0.0..=1.0).contains(alpha))
    })
}

pub(crate) fn resolve_node_opacity(ir: &MermaidDiagramIr, node_index: usize) -> Option<f64> {
    parse_opacity(
        merged_node_style(ir, node_index)
            .get("opacity")
            .map(String::as_str),
    )
}

/// The author's declared EDGE opacity, if any (bd-lvj3).
pub(crate) fn resolve_edge_opacity(ir: &MermaidDiagramIr, edge_index: usize) -> Option<f64> {
    parse_opacity(
        merged_edge_style(ir, edge_index)
            .get("opacity")
            .map(String::as_str),
    )
}

/// The author's declared CLUSTER opacity, if any (bd-lvj3).
pub(crate) fn resolve_cluster_opacity(ir: &MermaidDiagramIr, cluster_index: usize) -> Option<f64> {
    parse_opacity(
        merged_cluster_style(ir, cluster_index)
            .get("opacity")
            .map(String::as_str),
    )
}

/// The author's declared EDGE LABEL font, if any (bd-lvj3).
///
/// The edge twin of `resolve_node_font`, but only `font-size` is read: the edge label has no
/// weight/style/family channel in the SVG arm either, so reading more here would invent a
/// disagreement rather than close one.
///
/// The declared size is used AS DECLARED, not scaled by the 0.85 the theme applies to secondary
/// labels — the author asking for 22px means 22px, exactly as the SVG arm passes it through.
fn resolve_edge_label_font(
    ir: &MermaidDiagramIr,
    edge_index: usize,
    config: &CanvasRenderConfig,
) -> Option<String> {
    parse_declared_font_size(
        merged_edge_style(ir, edge_index)
            .get("font-size")
            .map(String::as_str),
    )
    .map(|size| format!("{size}px {}", config.font_family))
}

/// A CSS `font-weight` this renderer is willing to put in a canvas font string.
///
/// ⚠️ VALIDATED AGAINST A CLOSED SET RATHER THAN PASSED THROUGH, and the reason is the same one
/// that makes an unchecked `fillStyle` dangerous, one property over: a canvas given an UNPARSABLE
/// FONT STRING ignores the whole assignment and keeps the PREVIOUS font. One junk weight would
/// therefore silently discard the SIZE beside it too, and draw the label in whatever font the last
/// draw happened to leave behind. The failure is position-dependent and invisible in the output.
fn sanitize_font_weight(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Some("normal"),
        "bold" => Some("bold"),
        "bolder" => Some("bolder"),
        "lighter" => Some("lighter"),
        "100" => Some("100"),
        "200" => Some("200"),
        "300" => Some("300"),
        "400" => Some("400"),
        "500" => Some("500"),
        "600" => Some("600"),
        "700" => Some("700"),
        "800" => Some("800"),
        "900" => Some("900"),
        _ => None,
    }
}

/// A CSS `font-style` this renderer is willing to put in a canvas font string.
///
/// Closed set, for the reason `sanitize_font_weight` documents: an unparsable font string makes a
/// canvas ignore the WHOLE assignment and keep the previous font, so one junk component discards
/// the valid ones beside it.
fn sanitize_font_style(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" => Some("normal"),
        "italic" => Some("italic"),
        "oblique" => Some("oblique"),
        _ => None,
    }
}

/// A CSS `font-family` this renderer is willing to put in a canvas font string.
///
/// Cannot be a closed set — a family name is arbitrary — so this is a CHARACTER allowlist plus a
/// length bound. Permitted: letters, digits, spaces, `,`, `-`, `_`, `.` and quotes, which covers
/// every real declaration including quoted multi-word stacks like `'Inter', Arial`.
///
/// ⚠️ THE REJECTED CHARACTERS ARE THE POINT. The family lands in a font string that a canvas
/// PARSES, so a stray `;` or brace does not merely look wrong — it makes the whole assignment
/// unparsable and the label is drawn in whatever font the last draw left behind, silently. The
/// same reasoning as `sanitize_canvas_paint`, one grammar over.
fn sanitize_font_family(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 {
        return None;
    }
    let permitted = value.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, ' ' | ',' | '-' | '_' | '.' | '\'' | '"')
    });
    permitted.then(|| value.to_string())
}

/// The font components an author declared on a node, before any fallback is applied.
///
/// ⚠️ COMPONENTS, NOT A FINISHED STRING, and that distinction is the whole reason this type exists.
/// The plain label, the compartment HEADING and the compartment MEMBER rows each derive a
/// DIFFERENT default size (`font_size`, `font_size`, `font_size * 0.9`) and the heading is bold.
/// Handing all three one pre-composed string would force the label's size onto the compartments
/// whenever the author declared only, say, a weight.
///
/// That is not what the reference does. Measured on the SVG arm for
/// `class A { +int member }` + `style A font-size:30px`, BOTH texts carry `style="font-size:30px"`
/// beside their own `font-size="13.80"` / `font-size="12.42"` presentation attributes — and an
/// inline style beats a presentation attribute, so both render at 30px. Declare a weight instead
/// and only the weight is emitted; the derived sizes stand. So each declared property applies
/// FLAT and independently, and each undeclared one keeps that site's own default. Composing per
/// site from components is the only way to reproduce that.
#[derive(Default)]
pub(crate) struct DeclaredNodeFont {
    pub(crate) size: Option<f64>,
    weight: Option<&'static str>,
    style: Option<&'static str>,
    family: Option<String>,
}

impl DeclaredNodeFont {
    const fn declares_anything(&self) -> bool {
        self.size.is_some()
            || self.weight.is_some()
            || self.style.is_some()
            || self.family.is_some()
    }

    /// Compose a canvas font string, taking each undeclared component from this site's default.
    ///
    /// CSS shorthand order is not free — style, weight, size, family. A canvas parses this string,
    /// and a wrong arrangement makes it unparsable, which is silently equivalent to setting no
    /// font at all.
    fn compose(&self, size: f64, weight: Option<&str>, family: &str) -> String {
        let mut font = String::new();
        if let Some(style) = self.style {
            font.push_str(style);
            font.push(' ');
        }
        if let Some(weight) = self.weight.or(weight) {
            font.push_str(weight);
            font.push(' ');
        }
        font.push_str(&format!(
            "{}px {}",
            self.size.unwrap_or(size),
            self.family.as_deref().unwrap_or(family)
        ));
        font
    }
}

pub(crate) fn resolve_declared_node_font(
    ir: &MermaidDiagramIr,
    node_index: usize,
) -> DeclaredNodeFont {
    let merged = merged_node_style(ir, node_index);
    DeclaredNodeFont {
        size: parse_declared_font_size(merged.get("font-size").map(String::as_str)),
        weight: merged
            .get("font-weight")
            .map(String::as_str)
            .and_then(sanitize_font_weight),
        style: merged
            .get("font-style")
            .map(String::as_str)
            .and_then(sanitize_font_style),
        family: merged
            .get("font-family")
            .map(String::as_str)
            .and_then(sanitize_font_family),
    }
}

/// The author's declared node FONT, as a canvas font string, if anything was declared (bd-lvj3).
///
/// Returns `None` when neither `font-size` nor `font-weight` was declared, which is what keeps the
/// hoisted `standard_label_font` on the common path. When only one of the two is present the other
/// comes from the theme, because a canvas font string has no way to say "inherit this component".
fn resolve_node_font(
    ir: &MermaidDiagramIr,
    node_index: usize,
    config: &CanvasRenderConfig,
) -> Option<String> {
    let declared = resolve_declared_node_font(ir, node_index);
    declared
        .declares_anything()
        .then(|| declared.compose(config.font_size, None, &config.font_family))
}

pub(crate) fn resolve_node_stroke_dasharray(
    ir: &MermaidDiagramIr,
    node_index: usize,
) -> Option<Vec<f64>> {
    parse_dash_array(
        merged_node_style(ir, node_index)
            .get("stroke-dasharray")
            .map(String::as_str),
    )
}

pub(crate) fn resolve_node_stroke_width(ir: &MermaidDiagramIr, node_index: usize) -> Option<f64> {
    parse_stroke_width(
        merged_node_style(ir, node_index)
            .get("stroke-width")
            .map(String::as_str),
    )
}

pub(crate) fn legacy_edge_stroke(arrow: ArrowType, default_width: f64) -> (f64, &'static [f64]) {
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

fn class_vis_symbol(vis: fm_core::ClassVisibility) -> &'static str {
    match vis {
        fm_core::ClassVisibility::Unmarked => "",
        fm_core::ClassVisibility::Public => "+",
        fm_core::ClassVisibility::Private => "-",
        fm_core::ClassVisibility::Protected => "#",
        fm_core::ClassVisibility::Package => "~",
    }
}

/// The text of one class compartment row: `{vis}{name}{*|$}{": " return_type}` (bd-9wdra).
///
/// ONE function for attributes and methods because the row is one contract, not two. The canvas used
/// to build the two lines separately and inline, and both had quietly diverged from the other
/// backends: neither drew a member's type, and the methods line drew no abstract/static classifier
/// at all — `is_abstract` and `is_static` were referenced ZERO times in this crate, against two in
/// fm-render-svg and one in fm-render-term.
///
/// This mirrors `fm_layout::class_member_row_width`, which builds the SAME string to MEASURE the box
/// this text is drawn into, and whose doc comment already claimed to mirror "the renderer's row
/// text". The box was therefore always sized for the fuller row; the canvas simply declined to draw
/// it. That is why this fix cannot move any geometry.
///
/// `is_method` gates the classifier, not the type: mermaid writes `*`/`$` on methods only, and SVG's
/// attribute path likewise appends a type but never a suffix.
fn class_member_row(member: &fm_core::IrClassMember, is_method: bool) -> String {
    let mut row = String::with_capacity(member.name.len() + 8);
    row.push_str(class_vis_symbol(member.visibility));
    row.push_str(&fm_core::class_member_display_name(&member.name, is_method));
    // ⚠️ NO CLASSIFIER CHARACTER (bd-r2gll); it is a STYLE, and the caller applies it. Keeping the
    // byte here would also disagree with `fm_layout::class_member_row_width`, which this function's
    // own doc comment promises to mirror and which sizes the box this text is drawn into.
    if let Some(ref return_type) = member.return_type {
        // ` : `, as mermaid draws it and as `fm_layout::class_member_row_width` measures it
        // (bd-ci658).
        row.push_str(" : ");
        row.push_str(&fm_core::parse_generic_types(return_type));
    }
    row
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
    //
    // ⚠️ WHICH FAMILIES TAKE A TITLE IS NOT THIS BACKEND'S DECISION. This used to return
    // `ir.meta.title` outright, so the canvas drew a title on every family — including the ones
    // mermaid leaves bare — and so did the SVG and terminal backends, each from its own copy of the
    // same wrong rule. The measured (family, spelling) table now lives in
    // `MermaidDiagramIr::declared_title_if_drawn`; only the pie exclusion above is genuinely local.
    ir.declared_title_if_drawn()
}

fn canvas_c4_legend_enabled(ir: &MermaidDiagramIr) -> bool {
    matches!(
        ir.diagram_type,
        DiagramType::C4Context
            | DiagramType::C4Container
            | DiagramType::C4Component
            | DiagramType::C4Dynamic
            | DiagramType::C4Deployment
    ) && ir.meta.c4_show_legend
}

fn canvas_c4_legend_entries(ir: &MermaidDiagramIr) -> Vec<&'static str> {
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
        let marker = layout.extensions.sequence_lifecycle_markers.first().expect(
            "CONTROL FAILED: this source produced no lifecycle marker, so the renderer has \
                 nothing to draw and this test cannot detect the defect it was written for",
        );
        let size = f64::from(marker.size);
        assert!(size > 0.0, "a zero-size marker cannot be drawn or detected");

        let mut ctx = MockCanvas2dContext::new(1200.0, 800.0);
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );
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
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );
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
            .filter(
                |op| matches!(op, DrawOperation::FillText(text, _, _) if text.contains("Alpha")),
            )
            .count();
        assert_eq!(
            alpha_draws, 1,
            "a label was drawn more than once, suggesting the axis pass ran for a diagram with no axis"
        );
    }

    /// A Canvas sequence `alt` must show both branches, rather than presenting one undivided frame.
    ///
    /// The alternative's `start_edge` is intentionally not at the midpoint of this frame: the first
    /// branch has two messages and the second has one. This rejects an implementation that guesses a
    /// divider from the frame height instead of deriving it from the message geometry layout published.
    #[test]
    fn canvas_draws_sequence_alt_branch_divider_and_condition() {
        let ir = fm_parser::parse(
            "sequenceDiagram\n    A->>B: start\n    alt is ok\n        A->>B: yes\n        A->>B: yes2\n    else is bad\n        A->>B: no\n    end",
        )
        .ir;
        let layout = fm_layout::layout_diagram(&ir);
        let fragment = layout.extensions.sequence_fragments.first().expect(
            "CONTROL FAILED: this source produced no sequence fragment, so it cannot exercise the canvas branch renderer",
        );
        let ir_fragment = ir
            .sequence_meta
            .as_ref()
            .and_then(|meta| meta.fragments.first())
            .expect("CONTROL FAILED: the parser did not preserve the sequence fragment");
        let alternative = ir_fragment
            .alternatives
            .first()
            .expect("CONTROL FAILED: this alt has no else branch to render");
        assert_eq!(alternative.label, "is bad");

        let message_y = |edge_index: usize| {
            layout
                .edges
                .iter()
                .find(|edge| edge.edge_index == edge_index)
                .and_then(|edge| edge.points.first())
                .map(|point| f64::from(point.y) - f64::from(layout.bounds.y))
                .unwrap_or_else(|| panic!("layout published no message edge {edge_index}"))
        };
        let frame_y = f64::from(fragment.bounds.y) - f64::from(layout.bounds.y);
        let divider_y =
            message_y(alternative.start_edge) - (message_y(ir_fragment.start_edge) - frame_y);
        let frame_x = f64::from(fragment.bounds.x) - f64::from(layout.bounds.x);
        let frame_right = frame_x + f64::from(fragment.bounds.width);

        let config = CanvasRenderConfig {
            auto_fit: false,
            padding: 0.0,
            ..Default::default()
        };
        let mut ctx = MockCanvas2dContext::new(1200.0, 800.0);
        let mut renderer = Canvas2dRenderer::new(config);
        let _ = renderer.render(&layout, &ir, &mut ctx);
        let ops = ctx.operations();

        assert!(
            ops.iter()
                .any(|op| matches!(op, DrawOperation::FillText(text, _, _) if text == "[is bad]")),
            "the else condition never reached the Canvas operation stream"
        );
        assert!(
            ops.windows(2).any(|pair| match (&pair[0], &pair[1]) {
                (DrawOperation::MoveTo(x0, y0), DrawOperation::LineTo(x1, y1)) => {
                    (*x0 - frame_x).abs() < 0.01
                        && (*x1 - frame_right).abs() < 0.01
                        && (*y0 - divider_y).abs() < 0.01
                        && (*y1 - divider_y).abs() < 0.01
                }
                _ => false,
            }),
            "the branch divider was not drawn at the geometry-derived location"
        );
    }

    /// A fragment without an alternative must not acquire a meaningless branch divider.
    #[test]
    fn canvas_draws_no_sequence_branch_divider_without_an_alternative() {
        let ir = fm_parser::parse(
            "sequenceDiagram\n    A->>B: start\n    alt is ok\n        A->>B: yes\n    end",
        )
        .ir;
        let layout = fm_layout::layout_diagram(&ir);
        let fragment = layout.extensions.sequence_fragments.first().expect(
            "CONTROL FAILED: this source produced no sequence fragment, so the absence assertion is vacuous",
        );
        assert!(
            ir.sequence_meta.as_ref().is_some_and(|meta| meta
                .fragments
                .first()
                .is_some_and(|item| item.alternatives.is_empty())),
            "CONTROL FAILED: this source unexpectedly has an alternative"
        );

        let config = CanvasRenderConfig {
            auto_fit: false,
            padding: 0.0,
            ..Default::default()
        };
        let frame_x = f64::from(fragment.bounds.x) - f64::from(layout.bounds.x);
        let frame_right = frame_x + f64::from(fragment.bounds.width);
        let mut ctx = MockCanvas2dContext::new(1200.0, 800.0);
        let mut renderer = Canvas2dRenderer::new(config);
        let _ = renderer.render(&layout, &ir, &mut ctx);

        assert!(
            !ctx.operations()
                .windows(2)
                .any(|pair| match (&pair[0], &pair[1]) {
                    (DrawOperation::MoveTo(x0, _), DrawOperation::LineTo(x1, _)) => {
                        (*x0 - frame_x).abs() < 0.01 && (*x1 - frame_right).abs() < 0.01
                    }
                    _ => false,
                }),
            "a fragment without an else branch drew a full-width divider"
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
        let note = layout.extensions.state_notes.first().expect(
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
        assert!(
            !drew_note_text,
            "note text was drawn for a diagram with no note"
        );
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
    /// A state note's text starts at the padding the LAYOUT reserved (bd-4n5j2).
    ///
    /// Not "matches the SVG" as a matter of taste: fm-layout sizes the note box as
    /// `text * 0.8 + 2 * PAD` with STATE_NOTE_PAD_X = 10 and STATE_NOTE_PAD_Y = 8, so those pads
    /// are the box's own definition of where its text goes. fm-render-svg writes `nx + 10, ny + 8`.
    /// This surface used `x + 4` and started the first line a full line-height down, ignoring the
    /// margins the box was measured for.
    ///
    /// Asserted against the note's OWN bounds rather than absolute coordinates, so the test tracks
    /// the relationship rather than a snapshot of today's layout.
    #[test]
    fn a_state_note_draws_its_text_at_the_reserved_padding() {
        let src = "stateDiagram-v2\n  A --| B\n  note right of A\n    alpha line\n                       beta line\n  end note\n"
            .replace("--|", "-->");
        let ir = fm_parser::parse(&src).ir;
        let layout = fm_layout::layout_diagram(&ir);

        let note = layout
            .extensions
            .state_notes
            .first()
            .expect("CONTROL FAILED: the fixture laid out no state note");
        // NON-VACUITY: a single-line note would leave the spacing assertion untested.
        assert!(
            note.text.contains('\n'),
            "CONTROL FAILED: the note is not multi-line, so line spacing proves nothing: {:?}",
            note.text
        );

        let mut ctx = MockCanvas2dContext::new(1200.0, 900.0);
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );

        let mut lines: Vec<(f64, f64)> = Vec::new();
        for op in ctx.operations() {
            if let DrawOperation::FillText(text, x, y) = op
                && note.text.lines().any(|line| line == text)
            {
                lines.push((*x, *y));
            }
        }
        assert!(
            lines.len() >= 2,
            "expected every note line to be drawn, got {}",
            lines.len()
        );

        // THE AUTO-FIT OFFSET COMES FROM THE BOX, NOT FROM AN INSET UNDER TEST. Deriving it from
        // the text's own x made an x error surface as a y failure: the bad inset was absorbed into
        // the offset and the vertical assertion took the blame. The note's rect is drawn at
        // `bounds + offset`, so it yields both offsets independently of anything asserted below.
        let note_rect = ctx
            .operations()
            .iter()
            .find_map(|op| match op {
                DrawOperation::FillRect(rx, ry, rw, rh)
                    if (*rw - f64::from(note.bounds.width)).abs() < 0.001
                        && (*rh - f64::from(note.bounds.height)).abs() < 0.001 =>
                {
                    Some((*rx, *ry))
                }
                _ => None,
            })
            .expect("the note's own box was never filled, so no offset can be derived");
        let (box_x, box_y) = note_rect;

        let (first_x, first_y) = lines[0];
        let (second_x, second_y) = lines[1];
        assert!(
            (second_x - first_x).abs() < 0.001,
            "note lines must share one left margin: {first_x} then {second_x}"
        );
        assert!(
            (first_x - box_x - STATE_NOTE_PAD_X).abs() < 0.001,
            "the text must start at the reserved HORIZONTAL padding: box at {box_x}, text at \
             {first_x}, expected {}",
            box_x + STATE_NOTE_PAD_X
        );
        assert!(
            (first_y - box_y - STATE_NOTE_PAD_Y).abs() < 0.001,
            "the first line must start at the reserved VERTICAL padding, not a line-height down: \
             box at {box_y}, text at {first_y}, expected {}",
            box_y + STATE_NOTE_PAD_Y
        );
        assert!(
            (second_y - first_y - 16.8).abs() < 0.01,
            "line spacing must stay at the SVG's 16.8 (font_size * 0.8 * 1.5): got {}",
            second_y - first_y
        );
    }

    /// The canvas gantt axis agrees with the SVG arm, tick marks included (bd-c7ijh).
    ///
    /// Pinned against the CHECKED-IN GOLDEN rather than against the canvas's own arithmetic. The
    /// Mermaid 11.15 always renders the bottom grid, while `topAxis` APPENDS the top grid. Both
    /// rows are published by layout; canvas must consume those coordinates rather than recreate a
    /// `bounds.y +/- 12` rule that can disagree with SVG.
    ///
    /// The two layout-owned baselines, rather than a fixed fixture coordinate, keep this honest if
    /// Gantt task geometry moves.
    #[test]
    fn the_canvas_gantt_axis_matches_the_svg_arm() {
        let src = "%%{init: {'gantt': {'topAxis': true}} }%%\ngantt\n  title Roadmap\n  \
                   dateFormat  YYYY-MM-DD\n  section Core\n  \
                   Design :a1, 2026-01-01, 3d\n  Build :a2, after a1, 4d\n";
        let ir = fm_parser::parse(src).ir;
        let layout = fm_layout::layout_diagram(&ir);

        // NON-VACUITY: no ticks means every assertion below is skipped silently.
        assert!(
            !layout.extensions.axis_ticks.is_empty(),
            "CONTROL FAILED: the gantt fixture produced no axis ticks"
        );
        assert_eq!(
            layout.extensions.gantt_axis_rows.len(),
            2,
            "topAxis must append a top row to the unconditional bottom row"
        );

        let mut ctx = MockCanvas2dContext::new(1200.0, 800.0);
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );

        let first = &layout.extensions.axis_ticks[0];
        let mut label_positions = Vec::new();
        let mut baseline = TextBaseline::Alphabetic;
        let mut label_baselines = Vec::new();
        for op in ctx.operations() {
            match op {
                DrawOperation::SetTextBaseline(value) => baseline = *value,
                DrawOperation::FillText(text, x, y) if *text == first.label => {
                    label_positions.push((*x, *y));
                    label_baselines.push(baseline);
                }
                _ => {}
            }
        }
        assert_eq!(
            label_positions.len(),
            2,
            "each published axis row needs the first label"
        );
        assert!(
            label_baselines
                .iter()
                .all(|baseline| *baseline == TextBaseline::Alphabetic),
            "the SVG tick text carries no dominant-baseline, so its y is the alphabetic baseline"
        );
        let offset_y = label_positions[0].1 - f64::from(layout.extensions.gantt_axis_rows[0].y);
        for (axis, (_, label_y)) in layout
            .extensions
            .gantt_axis_rows
            .iter()
            .zip(&label_positions)
        {
            assert!(
                (*label_y - (f64::from(axis.y) + offset_y)).abs() < 0.5,
                "label y={label_y} did not consume the layout-owned axis row {}",
                axis.y
            );
        }
        let label_x = label_positions[0].0;
        // The label sits just RIGHT of its mark, as the SVG writes it (x + 3).
        let offset_x = label_x - (f64::from(first.position) + 3.0);

        // THE TICK MARK, which this surface used to omit entirely: one 12-unit vertical line per
        // tick, starting 4 below the axis line the label sits on.
        let mut marks = 0;
        let mut from = None;
        for op in ctx.operations() {
            match op {
                DrawOperation::MoveTo(x, y) => from = Some((*x, *y)),
                DrawOperation::LineTo(x, y) => {
                    if let Some((fx, fy)) = from
                        && (fx - x).abs() < 0.001
                        && (*y - fy - 12.0).abs() < 0.001
                        && layout
                            .extensions
                            .gantt_axis_rows
                            .iter()
                            .any(|axis| (fy - (f64::from(axis.y) + offset_y + 4.0)).abs() < 0.5)
                    {
                        marks += 1;
                    }
                }
                _ => {}
            }
        }
        assert_eq!(
            marks,
            layout.extensions.axis_ticks.len() * layout.extensions.gantt_axis_rows.len(),
            "every tick must be drawn on both Mermaid grid rows"
        );

        // Every tick label shares one x offset with its mark, so a single tick agreeing is not
        // enough to call the axis right.
        assert!(
            offset_x.abs() < 40.0,
            "label x offset {offset_x} is not a plausible auto-fit offset"
        );
    }

    /// A state note's text state is SET, not inherited (bd-4n5j2).
    ///
    /// Every draw source shares one save()/restore(), and draw_state_notes used to call fill_text
    /// without setting align or baseline -- so it took whatever the previous source left. The
    /// previous source that sets them is draw_clusters, and only when it draws a cluster TITLE, so
    /// the note's vertical anchor depended on whether the diagram ALSO contained a composite state.
    ///
    /// TWO FIXTURES, and that is the whole test: the same note is rendered in a diagram with a
    /// composite state and in one without. Asserting Top in a single fixture would have passed
    /// before the fix, because that fixture's cluster title had already set it.
    #[test]
    fn a_state_note_does_not_inherit_its_text_state_from_the_rest_of_the_diagram() {
        // With a composite state: draw_clusters draws a title and sets Left/Top before the note.
        let with_cluster = "stateDiagram-v2\n  state Outer {\n    A --> B\n  }\n                              note right of Outer : watched\n";
        // Without: nothing before the note sets either, so it used the fresh-canvas default.
        let without_cluster = "stateDiagram-v2\n  A --> B\n  note right of A : watched\n";

        for (label, src) in [("with composite", with_cluster), ("bare", without_cluster)] {
            let ir = fm_parser::parse(src).ir;
            let layout = fm_layout::layout_diagram(&ir);

            // NON-VACUITY: no note laid out means the loop below never sees the text at all.
            assert!(
                !layout.extensions.state_notes.is_empty(),
                "CONTROL FAILED ({label}): the fixture laid out no state note"
            );

            let mut ctx = MockCanvas2dContext::new(1200.0, 800.0);
            let _ = crate::render_to_canvas_with_layout(
                &ir,
                &layout,
                &mut ctx,
                &CanvasRenderConfig::default(),
            );

            // SEEDED WITH THE MOCK'S OWN INITIAL STATE, so what this tracks is genuinely what the
            // context would be in, not an arbitrary starting guess that makes the failure message
            // claim an inherited value nobody set.
            let mut align = TextAlign::Left;
            let mut baseline = TextBaseline::Alphabetic;
            let mut seen = false;
            for op in ctx.operations() {
                match op {
                    DrawOperation::SetTextAlign(value) => align = *value,
                    DrawOperation::SetTextBaseline(value) => baseline = *value,
                    DrawOperation::FillText(text, _, _) if text.contains("watched") => {
                        assert_eq!(
                            align,
                            TextAlign::Left,
                            "({label}) note text must be left-aligned, like the SVG arm's \
                             TextAnchor::Start"
                        );
                        assert_eq!(
                            baseline,
                            TextBaseline::Top,
                            "({label}) note text must hang from the top, like the SVG arm's \
                             DominantBaseline::Hanging -- inheriting the alphabetic default drops \
                             it by an ascent and can cross the note's own border"
                        );
                        seen = true;
                    }
                    _ => {}
                }
            }
            assert!(seen, "({label}) the note text was never drawn");
        }
    }

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
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );
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
                        let offset = inside_drawn_x.expect(
                            "the Inside-placed task must be drawn before the OutsideLeft one",
                        ) - f64::from(inside_anchor);
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
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );
        let mut align = TextAlign::Left;
        for op in ctx.operations() {
            match op {
                DrawOperation::SetTextAlign(a) => align = *a,
                DrawOperation::FillText(text, _, _) if text.contains("Alpha") => {
                    assert_eq!(
                        align,
                        TextAlign::Center,
                        "a flowchart label lost its centring"
                    );
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
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );
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
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );
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
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );
        let count = |needle: &str| {
            ctx.operations()
                .iter()
                .filter(|op| matches!(op, DrawOperation::FillText(t, _, _) if t == needle))
                .count()
        };
        assert_eq!(
            count("Alice"),
            2,
            "Alice was not drawn at both head and foot"
        );
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
        let _ = crate::render_to_canvas_with_layout(
            &ir,
            &layout,
            &mut ctx,
            &CanvasRenderConfig::default(),
        );
        let alpha = ctx
            .operations()
            .iter()
            .filter(|op| matches!(op, DrawOperation::FillText(t, _, _) if t == "Alpha"))
            .count();
        assert_eq!(alpha, 1, "a flowchart label was mirrored");
    }

    const GANTT_CHART: &str = "gantt\n  title Roadmap\n  dateFormat  YYYY-MM-DD\n  section Core\n  Design :a1, 2026-01-01, 3d\n  Build :a2, after a1, 4d\n";

    /// Every vertical `MoveTo -> LineTo` segment in the op stream, as `(x, top, bottom)`.
    fn vertical_segments(source: &str, today: Option<&str>) -> Vec<(f64, f64, f64)> {
        let ir = fm_parser::parse(source).ir;
        let config = CanvasRenderConfig {
            gantt_today: today.map(str::to_string),
            ..CanvasRenderConfig::default()
        };
        let mut ctx = MockCanvas2dContext::new(1200.0, 600.0);
        let _ = crate::render_to_canvas(&ir, &mut ctx, &config);

        ctx.operations()
            .windows(2)
            .filter_map(|pair| match (&pair[0], &pair[1]) {
                (DrawOperation::MoveTo(x0, y0), DrawOperation::LineTo(x1, y1))
                    if (x0 - x1).abs() < 0.001 && (y0 - y1).abs() > 0.001 =>
                {
                    Some((*x0, y0.min(*y1), y0.max(*y1)))
                }
                _ => None,
            })
            .collect()
    }

    /// The vertical segments a marked render has that the unmarked one does not.
    ///
    /// DIFFED AGAINST A BASELINE rather than counted: a gantt render emits many segments, so an
    /// op-count assertion would pass on any extra line drawn anywhere. What is asserted here is the
    /// segment the marker itself added.
    fn segments_added_by_marker(source: &str, today: &str) -> Vec<(f64, f64, f64)> {
        let baseline = vertical_segments(source, None);
        vertical_segments(source, Some(today))
            .into_iter()
            .filter(|segment| {
                !baseline.iter().any(|other| {
                    (other.0 - segment.0).abs() < 0.001
                        && (other.1 - segment.1).abs() < 0.001
                        && (other.2 - segment.2).abs() < 0.001
                })
            })
            .collect()
    }

    /// The gantt today marker reaches the canvas (bd-t1jj).
    ///
    /// `extensions.gantt_day_axis` was the last `LayoutExtensions` field this renderer referenced
    /// nowhere, and it is the only thing that answers "where is a given DATE on this chart". So a
    /// canvas gantt drew no today line while the same source rendered to SVG drew one.
    #[test]
    fn canvas_draws_the_gantt_today_marker() {
        let ir = fm_parser::parse(GANTT_CHART).ir;
        let layout = fm_layout::layout_diagram(&ir);
        let axis = layout
            .extensions
            .gantt_day_axis
            .expect("a gantt layout must publish its day axis");

        // NON-VACUITY: the date must be inside the charted span, or `x_for_day` returns None, the
        // renderer correctly draws nothing, and this test asserts the absence it exists to rule out.
        let day = fm_layout::parse_iso_day_number("2026-01-03").expect("a real calendar date");
        assert!(
            axis.x_for_day(day).is_some(),
            "CONTROL FAILED: 2026-01-03 is outside this chart, so this test proves nothing"
        );

        let added = segments_added_by_marker(GANTT_CHART, "2026-01-03");
        assert_eq!(
            added.len(),
            1,
            "expected exactly one new vertical segment for the today marker, got {added:?}"
        );

        // It must cross the chart, not be a stub. The layout's own height is the yardstick.
        let (_, top, bottom) = added[0];
        assert!(
            bottom - top > f64::from(layout.bounds.height) * 0.5,
            "the today marker spans {} of a {} chart, which is not a line across it",
            bottom - top,
            layout.bounds.height
        );
    }

    /// The marker's x comes from the AXIS, not from arithmetic of its own.
    ///
    /// Asserted DIFFERENTIALLY across two dates one day apart rather than against an absolute
    /// coordinate: the rendered x carries the canvas offset, so comparing it against the layout's
    /// raw `x_for_day` compares a rendered coordinate to a layout one. The difference cancels the
    /// offset and pins what matters — the marker advances by the axis's own `day_width` per day, so
    /// the line and the ticks cannot disagree about where a day is. `LayoutGanttDayAxis`'s own doc
    /// warns that re-deriving day positions is exactly how they come to disagree.
    #[test]
    fn the_canvas_today_marker_advances_with_the_axis_day_width() {
        let ir = fm_parser::parse(GANTT_CHART).ir;
        let axis = fm_layout::layout_diagram(&ir)
            .extensions
            .gantt_day_axis
            .expect("a gantt layout must publish its day axis");

        for date in ["2026-01-02", "2026-01-03"] {
            let day = fm_layout::parse_iso_day_number(date).expect("a real calendar date");
            assert!(
                axis.x_for_day(day).is_some(),
                "CONTROL FAILED: {date} is outside this chart's span"
            );
        }

        let x_at = |today: &str| -> f64 {
            let added = segments_added_by_marker(GANTT_CHART, today);
            assert_eq!(added.len(), 1, "expected one marker for {today}: {added:?}");
            added[0].0
        };

        let advance = x_at("2026-01-03") - x_at("2026-01-02");
        assert!(
            (advance - f64::from(axis.day_width)).abs() < 0.01,
            "one day of marker movement was {advance}, but the axis places days {} apart -- the \
             marker and the axis disagree about where a day is",
            axis.day_width
        );
    }

    /// SUPPRESSION, four distinct routes to "no marker". A renderer that drew the line
    /// unconditionally would satisfy the positive tests above and fail every one of these.
    #[test]
    fn the_canvas_today_marker_is_suppressed_when_it_should_be() {
        // 1. No date supplied -- the library default, so this is also the proof that no existing
        //    canvas render changed.
        let unmarked = vertical_segments(GANTT_CHART, None);
        let marked = vertical_segments(GANTT_CHART, Some("2026-01-03"));
        assert_eq!(
            marked.len(),
            unmarked.len() + 1,
            "supplying no date must draw strictly one segment fewer than supplying one"
        );

        // 2. `todayMarker off`. The directive exists to turn the line off, so it must -- and before
        //    this bead it was equally invisible, because there was nothing to turn off.
        let off = format!("{GANTT_CHART}  todayMarker off\n");
        assert!(
            segments_added_by_marker(&off, "2026-01-03").is_empty(),
            "`todayMarker off` did not suppress the marker"
        );

        // 3. A date outside the charted span. Drawing nothing is the correct answer to "today is
        //    not in this chart"; an off-canvas x invites drawing it at the edge, where it reads as
        //    a real date that happens to sit there.
        assert!(
            segments_added_by_marker(GANTT_CHART, "2031-06-01").is_empty(),
            "a date outside the chart drew a marker anyway"
        );

        // 4. A string that is not a date at all.
        assert!(
            segments_added_by_marker(GANTT_CHART, "not-a-date").is_empty(),
            "an unparseable date drew a marker anyway"
        );
    }

    /// INERT CASE: a non-gantt diagram publishes no day axis, so supplying a date changes nothing.
    /// Without this, a marker drawn on every diagram type would pass everything above.
    #[test]
    fn a_non_gantt_canvas_render_is_untouched_by_a_supplied_date() {
        let flowchart = "flowchart TD\n  a[Alpha] --> b[Beta]\n";
        assert_eq!(
            vertical_segments(flowchart, Some("2026-01-03")),
            vertical_segments(flowchart, None),
            "supplying a today date altered a flowchart render"
        );
    }
}
