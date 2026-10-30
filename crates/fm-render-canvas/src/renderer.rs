//! Canvas2D diagram renderer.
//!
//! Draws diagrams to Canvas2D contexts using computed layouts.

use crate::context::{Canvas2dContext, LineCap, LineJoin, TextAlign, TextBaseline};
use crate::shapes::{
    draw_arrowhead, draw_circle_marker, draw_cross_marker, draw_diamond_marker,
    draw_er_cardinality_marker, draw_open_triangle_marker, draw_shape, trace_rounded_rect,
};
use crate::viewport::{Viewport, fit_to_viewport};
use fm_core::{ArrowType, DiagramType, IrXyChartMeta, IrXySeriesKind, MermaidDiagramIr, NodeShape};
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
    /// Line height for node labels and layout measurement, as a multiple of the font size.
    pub line_height: f64,
    /// Padding around the diagram.
    pub padding: f64,
    /// Corner radius for rounded nodes. Ordinary cards use 55% of this radius and
    /// subgraph panels use 150%, matching the SVG renderer's visual hierarchy.
    pub rounded_corners: f64,
    /// Optional document background. `None` keeps a standalone canvas transparent.
    /// Browser theme adapters supply the theme's background so text stays readable on any host.
    pub background: Option<String>,
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
    /// Semantic branch accents, in the same order as the SVG theme palette.
    pub accents: [String; 8],
    /// Use restrained shape-role colors in flowcharts. Disable for exact named-theme palettes.
    pub semantic_colors: bool,
    /// Whether to auto-fit the diagram to the canvas.
    pub auto_fit: bool,
    /// Today's date, as `YYYY-MM-DD`, for the gantt today marker. `None` draws no marker.
    ///
    /// SUPPLIED, NEVER READ FROM THE CLOCK, exactly as `SvgRenderConfig::gantt_today` is. Output
    /// that is a function of wall time is a defect class this repo has already been bitten by three
    /// times; a renderer that called `now()` would make every canvas op-stream test depend on the
    /// day it ran. The default is `None`, so nothing about an existing render changes.
    pub gantt_today: Option<String>,
    /// Whether to draw diagnostic debug overlays (dashed node bounding boxes and HUD stats)
    /// onto the Canvas2D context (`bd-0l1x2`).
    pub debug_overlay: bool,
}

impl CanvasRenderConfig {
    /// Apply authored Mermaid palette variables without changing typography or fit settings.
    /// Node, edge and cluster style declarations still take precedence when they are drawn.
    pub fn apply_theme_variables(
        &mut self,
        variables: &std::collections::BTreeMap<String, String>,
    ) {
        let paint = |key: &str| {
            variables
                .get(key)
                .and_then(|value| sanitize_canvas_paint(value))
        };
        if let Some(color) = paint("background") {
            self.background = Some(color);
        }
        if let Some(color) = variables
            .get("primaryTextColor")
            .or_else(|| variables.get("textColor"))
            .and_then(|value| sanitize_canvas_paint(value))
        {
            self.label_color = color;
        }
        for (key, target) in [
            ("primaryColor", &mut self.node_fill),
            ("primaryBorderColor", &mut self.node_stroke),
            ("lineColor", &mut self.edge_stroke),
            ("clusterBkg", &mut self.cluster_fill),
            ("clusterBorder", &mut self.cluster_stroke),
        ] {
            if let Some(color) = paint(key) {
                *target = color;
            }
        }
        for (index, accent) in self.accents.iter_mut().enumerate() {
            if let Some(color) = paint(&format!("pie{}", index + 1)) {
                *accent = color;
            }
        }
    }

    /// Get the font metrics based on this configuration.
    #[must_use]
    pub fn font_metrics(&self) -> fm_core::FontMetrics {
        fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
            preset: fm_core::FontPreset::from_family(&self.font_family),
            font_size: self.font_size as f32,
            line_height: self.resolved_line_height(),
            fallback_chain: vec![
                fm_core::FontPreset::SansSerif,
                fm_core::FontPreset::Monospace,
            ],
            trace_fallbacks: false,
        })
    }

    fn resolved_line_height(&self) -> f32 {
        let line_height = self.line_height as f32;
        if line_height.is_finite() && line_height > 0.0 {
            line_height
        } else {
            1.5
        }
    }
}

impl Default for CanvasRenderConfig {
    fn default() -> Self {
        Self {
            font_family: String::from(
                "system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif",
            ),
            font_size: 15.0,
            line_height: 1.5,
            padding: 24.0,
            rounded_corners: 8.0,
            background: None,
            node_fill: String::from("#f8fafc"),
            node_stroke: String::from("#7c8da5"),
            node_stroke_width: 1.5,
            edge_stroke: String::from("#64748b"),
            edge_stroke_width: 1.5,
            cluster_fill: String::from("#f1f5f9"),
            cluster_stroke: String::from("#cbd5e1"),
            label_color: String::from("#1e293b"),
            accents: fm_core::mindmap::DEFAULT_BRANCH_ACCENTS.map(String::from),
            semantic_colors: true,
            auto_fit: true,
            gantt_today: None,
            debug_overlay: false,
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
    edge_label_layout: Option<CanvasEdgeLabelLayout>,
}

#[derive(Debug, Clone, Copy)]
struct CanvasEdgeLabel {
    position: fm_layout::LayoutPoint,
    width: f64,
    height: f64,
    line_height: f64,
}

#[derive(Debug, Clone)]
struct CanvasEdgeLabelLayout {
    labels: Vec<Option<CanvasEdgeLabel>>,
    bounds: fm_layout::LayoutRect,
}

/// Measure the labels the browser will actually draw before fitting the diagram. Placement is
/// shared with SVG, while font measurement belongs to the Canvas context and honours linkStyle.
fn measure_graph_edge_labels<C: Canvas2dContext>(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    ctx: &mut C,
    config: &CanvasRenderConfig,
) -> Option<CanvasEdgeLabelLayout> {
    if !fm_layout::edge_labels::uses_graph_edge_labels(ir.diagram_type)
        || !layout.edges.iter().any(|path| {
            !path.bundled
                && path.points.len() >= 2
                && ir.edges.get(path.edge_index).is_some_and(|edge| {
                    edge.arrow != ArrowType::Invisible
                        && ir.graph_edge_caption(path.edge_index).is_some()
                })
        })
    {
        return None;
    }

    let mut result = CanvasEdgeLabelLayout {
        labels: vec![None; ir.edges.len()],
        bounds: layout.bounds,
    };
    let mut placer = fm_layout::edge_labels::EdgeLabelPlacer::with_font_metrics(
        ir,
        layout,
        config.padding as f32,
        &config.font_metrics(),
    );
    let default_font = secondary_label_font_css(config);
    ctx.save();
    for path in &layout.edges {
        let Some(edge) = ir.edges.get(path.edge_index) else {
            continue;
        };
        if path.bundled || path.points.len() < 2 || edge.arrow == ArrowType::Invisible {
            continue;
        }
        let Some(caption) = ir.graph_edge_caption(path.edge_index) else {
            continue;
        };
        let declared_font = resolve_edge_label_font(ir, path.edge_index, config);
        let (width, height, line_height) = measure_graph_edge_caption(
            &caption,
            ctx,
            config,
            declared_font.as_deref().unwrap_or(&default_font),
        );
        if !width.is_finite() || !height.is_finite() {
            continue;
        }
        let position = placer.place(path, width as f32, height as f32);
        result.labels[path.edge_index] = Some(CanvasEdgeLabel {
            position,
            width,
            height,
            line_height,
        });
    }
    result.bounds = placer.bounds();
    ctx.restore();
    Some(result)
}

/// Measure every row under the same font used for painting. C4 technology can be wider than
/// the relationship text, and dynamic ordinals belong to the measured label too.
fn measure_graph_edge_caption<C: Canvas2dContext>(
    caption: &fm_core::GraphEdgeCaption<'_>,
    ctx: &mut C,
    config: &CanvasRenderConfig,
    font: &str,
) -> (f64, f64, f64) {
    let mut width = 0.0_f64;
    // A browser adapter may expose only ink bounds or a conservative fallback. Keep at
    // least the resolved font size even for a caption containing only italic technology.
    let mut text_height =
        parse_declared_font_size(font.split_whitespace().next()).unwrap_or(config.font_size * 0.85);
    ctx.set_font(font);
    for line in caption.label.lines() {
        let metrics = ctx.measure_text(line);
        width = width.max(metrics.width);
        text_height = text_height.max(metrics.height);
    }
    if let Some(technology) = &caption.technology {
        ctx.set_font(&format!("italic {font}"));
        for line in technology.lines() {
            let metrics = ctx.measure_text(line);
            width = width.max(metrics.width);
            text_height = text_height.max(metrics.height);
        }
    }
    // Authored large fonts need both a taller plate and enough separation between rows.
    let line_height = (config.font_size * 1.2).max(text_height * 1.2);
    let height = caption.line_count().max(1) as f64 * line_height + 4.0;
    (width + 8.0, height, line_height)
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
            edge_label_layout: None,
        }
    }

    /// Render a diagram layout to a Canvas2D context.
    pub fn render<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
    ) -> CanvasRenderResult {
        let variables = &ir.meta.theme_overrides.theme_variables;
        if variables.is_empty() {
            return self.render_diagram(layout, ir, ctx);
        }
        // Source palettes belong to this frame, not to the reusable renderer's defaults.
        let mut effective = self.config.clone();
        effective.apply_theme_variables(variables);
        let base = std::mem::replace(&mut self.config, effective);
        let result = self.render_diagram(layout, ir, ctx);
        self.config = base;
        result
    }

    fn clear_canvas<C: Canvas2dContext>(&mut self, ctx: &mut C, width: f64, height: f64) {
        ctx.clear_rect(0.0, 0.0, width, height);
        self.draw_calls += 1;
        if let Some(background) = self
            .config
            .background
            .as_deref()
            .and_then(sanitize_canvas_paint)
        {
            // The background covers the backing store, independently of diagram zoom. Isolate
            // its paint and alpha so it cannot change the caller's state or any later primitive.
            ctx.save();
            ctx.reset_transform();
            ctx.set_global_alpha(1.0);
            ctx.set_fill_style(&background);
            ctx.fill_rect(0.0, 0.0, width, height);
            ctx.restore();
            self.draw_calls += 1;
        }
    }

    fn render_diagram<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
    ) -> CanvasRenderResult {
        self.draw_calls = 0;

        let canvas_width = ctx.width();
        let canvas_height = ctx.height();
        self.edge_label_layout = measure_graph_edge_labels(ir, layout, ctx, &self.config);
        let pie = ir
            .pie_meta
            .as_ref()
            .filter(|_| ir.diagram_type == DiagramType::Pie)
            .map(|meta| {
                let metrics = self.config.font_metrics();
                fm_layout::PieChartLayout::new(
                    meta,
                    ir.meta.title.as_deref().or(meta.title.as_deref()),
                    &metrics,
                    8.0,
                    metrics.avg_char_width(),
                )
            });
        let radar = ir
            .radar_meta
            .as_ref()
            .zip(layout.extensions.radar.as_ref())
            .map(|(meta, radar)| {
                fm_layout::RadarChartLayout::new(
                    meta,
                    radar,
                    ir.declared_title_if_drawn(),
                    &self.config.font_metrics(),
                )
            });
        let treemap = (ir.diagram_type == DiagramType::Treemap && ir.treemap_meta.is_some())
            .then(|| fm_layout::TreemapChartLayout::new(ir, layout, &self.config.font_metrics()));
        let quadrant = ir
            .quadrant_meta
            .as_ref()
            .filter(|_| ir.diagram_type == DiagramType::QuadrantChart)
            .map(|meta| {
                fm_layout::QuadrantChartLayout::new(
                    layout,
                    meta,
                    ir.meta.title.as_deref().or(meta.title.as_deref()),
                    (0.0, 0.0),
                    &self.config.font_metrics(),
                    8.0,
                    self.config.font_size as f32 * 0.5,
                )
            });
        let render_bounds = quadrant
            .as_ref()
            .map(|chart| chart.bounds)
            .or_else(|| treemap.as_ref().map(|chart| chart.bounds))
            .or_else(|| radar.as_ref().map(|chart| chart.bounds))
            .unwrap_or_else(|| {
                pie.as_ref().map_or_else(
                    || {
                        self.edge_label_layout
                            .as_ref()
                            .map_or(layout.bounds, |labels| labels.bounds)
                    },
                    |pie| pie.bounds,
                )
            });

        // Compute viewport to fit diagram
        let mut viewport = if self.config.auto_fit {
            fit_to_viewport(
                f64::from(render_bounds.width),
                f64::from(render_bounds.height),
                canvas_width,
                canvas_height,
                self.config.padding,
            )
        } else {
            Viewport::new(canvas_width, canvas_height)
        };

        // Clear canvas
        self.clear_canvas(ctx, canvas_width, canvas_height);

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
            (-f64::from(render_bounds.x), -f64::from(render_bounds.y))
        } else {
            (
                self.config.padding - f64::from(render_bounds.x),
                self.config.padding - f64::from(render_bounds.y),
            )
        };

        if let Some(pie) = &pie {
            let mut labels_drawn = 0;
            self.draw_pie_wedges(pie, ir, ctx, offset_x, offset_y, &mut labels_drawn);
            let mut hit_regions = crate::interaction::hit_regions(ir, layout);
            for region in &mut hit_regions {
                if let Some(entry) = pie.entries.get(region.node_index) {
                    region.bounds = entry.bounds();
                }
            }
            ctx.restore();
            return CanvasRenderResult {
                draw_calls: self.draw_calls,
                nodes_drawn: pie.slices.len(),
                edges_drawn: 0,
                clusters_drawn: 0,
                labels_drawn,
                viewport,
                hit_regions,
            };
        }

        if let Some(treemap) = &treemap {
            let labels_drawn = self.draw_treemap(treemap, ctx, offset_x, offset_y);
            ctx.restore();
            return CanvasRenderResult {
                draw_calls: self.draw_calls,
                nodes_drawn: treemap.tiles.len(),
                edges_drawn: 0,
                clusters_drawn: 0,
                labels_drawn,
                viewport,
                hit_regions: crate::interaction::hit_regions(ir, layout),
            };
        }

        if let Some(chart) = &quadrant {
            let meta = ir.quadrant_meta.as_ref().expect("quadrant metadata");
            let labels_drawn = self.draw_quadrant_chart(chart, meta, ctx, offset_x, offset_y);
            ctx.restore();
            // The interaction wrapper accounts for the original layout origin. Include the
            // extra measured chart margin so fitting long axis captions keeps picking aligned.
            viewport.offset_x += f64::from(layout.bounds.x - chart.bounds.x) * viewport.zoom;
            viewport.offset_y += f64::from(layout.bounds.y - chart.bounds.y) * viewport.zoom;
            return CanvasRenderResult {
                draw_calls: self.draw_calls,
                nodes_drawn: chart.points.len(),
                edges_drawn: 0,
                clusters_drawn: 0,
                labels_drawn,
                viewport,
                hit_regions: crate::interaction::hit_regions(ir, layout),
            };
        }

        // XY marks are data, not labelled graph nodes. The generic path loses the numeric
        // scale and axis titles, paints every series alike, and never fills area series.
        if let Some(meta) = ir.xy_chart_meta.as_ref()
            && ir.diagram_type == DiagramType::XyChart
        {
            let labels_drawn = self.draw_xychart_axes(ir, meta, layout, ctx, offset_x, offset_y);
            let (nodes_drawn, edges_drawn) =
                self.draw_xychart_series(meta, layout, ctx, offset_x, offset_y);
            ctx.restore();
            return CanvasRenderResult {
                draw_calls: self.draw_calls,
                nodes_drawn,
                edges_drawn,
                clusters_drawn: 0,
                labels_drawn,
                viewport,
                hit_regions: crate::interaction::hit_regions(ir, layout),
            };
        }

        let mut labels_drawn = 0;

        // Draw clusters (background)
        let clusters_drawn = self.draw_clusters(layout, ir, ctx, offset_x, offset_y);

        // Draw layout bands (sequence lifelines, gantt sections, etc.)
        self.draw_bands(layout, ir, ctx, offset_x, offset_y);
        // Paint each timeline section caption above every background, in the same order as GPU.
        if ir.diagram_type == DiagramType::Timeline {
            let mut cluster_label_font = None;
            for cluster_box in &layout.clusters {
                let opacity = resolve_cluster_opacity(ir, cluster_box.cluster_index);
                if let Some(alpha) = opacity {
                    ctx.set_global_alpha(alpha);
                }
                self.paint_cluster_caption(
                    cluster_box,
                    ir,
                    ctx,
                    (offset_x, offset_y),
                    &mut cluster_label_font,
                );
                if opacity.is_some() {
                    ctx.set_global_alpha(1.0);
                }
            }
        }
        // Radar uses the same measured wheel, legend and captions as the shared scene.
        labels_drawn += self.draw_radar(radar.as_ref(), ctx, offset_x, offset_y);
        if ir.diagram_type == DiagramType::Info {
            self.draw_info_version(ctx, offset_x, offset_y, &mut labels_drawn);
        }

        // Draw extension-provided axis labels, such as gantt dates.
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

        // Draw edges
        let edges_drawn = self.draw_edges(layout, ir, ctx, offset_x, offset_y, &mut labels_drawn);

        // Draw nodes
        let nodes_drawn = self.draw_nodes(layout, ir, ctx, offset_x, offset_y, &mut labels_drawn);

        if let Some(journey) = &layout.extensions.journey {
            ctx.set_fill_style(&self.config.label_color);
            ctx.set_text_align(TextAlign::Center);
            ctx.set_text_baseline(TextBaseline::Middle);
            for row in journey.title.iter().chain(&journey.legend) {
                let center = row.bounds.center();
                ctx.set_font(&format!("{}px {}", row.font_size, self.config.font_family));
                ctx.fill_text(
                    &row.text,
                    f64::from(center.x) + offset_x,
                    f64::from(center.y) + offset_y,
                );
                self.draw_calls += 1;
                labels_drawn += 1;
            }
        }

        if self.config.debug_overlay {
            self.draw_debug_overlay_world(layout, ctx, offset_x, offset_y);
        }

        ctx.restore();
        if layout.extensions.journey.is_none()
            && radar.is_none()
            && self.draw_generic_diagram_title(ctx, ir, canvas_width)
        {
            labels_drawn += 1;
        }
        labels_drawn += self.draw_c4_legend(ctx, ir, canvas_width, canvas_height);
        if self.config.debug_overlay {
            self.draw_debug_overlay_hud(layout, ctx, canvas_height);
            labels_drawn += 1;
        }

        // The public entrypoint folds the original layout origin into this viewport. Account
        // here only for the extra origin shift introduced by labels outside that layout, so
        // pointer hit regions and selection overlays still follow the nodes that were drawn.
        viewport.offset_x += f64::from(layout.bounds.x - render_bounds.x) * viewport.zoom;
        viewport.offset_y += f64::from(layout.bounds.y - render_bounds.y) * viewport.zoom;

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

    fn draw_debug_overlay_world<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        ctx.save();
        ctx.set_stroke_style("#ef4444");
        ctx.set_line_width(1.0);
        ctx.set_line_dash(&[3.0, 2.0]);
        for node in &layout.nodes {
            ctx.stroke_rect(
                f64::from(node.bounds.x) + offset_x,
                f64::from(node.bounds.y) + offset_y,
                f64::from(node.bounds.width),
                f64::from(node.bounds.height),
            );
            self.draw_calls += 1;
        }
        ctx.set_line_dash(&[]);
        ctx.restore();
    }

    fn draw_debug_overlay_hud<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ctx: &mut C,
        canvas_height: f64,
    ) {
        let rank_count = layout.nodes.iter().map(|n| n.rank + 1).max().unwrap_or(0);
        let hud = format!(
            "debug: nodes={} edges={} ranks={} crossings={} cycles={}",
            layout.nodes.len(),
            layout.edges.len(),
            rank_count,
            layout.stats.crossing_count,
            layout.stats.cycle_count,
        );
        ctx.save();
        ctx.set_fill_style("#ef4444");
        ctx.set_font("10px monospace");
        ctx.set_text_align(TextAlign::Left);
        ctx.set_text_baseline(TextBaseline::Bottom);
        ctx.fill_text(&hud, 8.0, (canvas_height - 6.0).max(12.0));
        self.draw_calls += 1;
        ctx.restore();
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

        self.clear_canvas(ctx, canvas_width, canvas_height);

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
                start_marker_angle(path.marker_start, angle),
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
            IrTextBaseline::Alphabetic => TextBaseline::Alphabetic,
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
                IrTextBaseline::Bottom | IrTextBaseline::Alphabetic => {
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

        // Parent panels must be behind their descendants even when parser or layout order
        // puts an inner subgraph first. Stable ordering preserves equal-size sibling order.
        let mut clusters: Vec<_> = layout.clusters.iter().collect();
        clusters.sort_by(|left, right| {
            (right.bounds.width * right.bounds.height)
                .total_cmp(&(left.bounds.width * left.bounds.height))
        });
        for cluster_box in clusters {
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

            let is_c4_boundary = ir
                .clusters
                .get(cluster_box.cluster_index)
                .is_some_and(|cluster| cluster.c4_boundary_type.is_some());
            trace_rounded_rect(
                ctx,
                x,
                y,
                w,
                h,
                if is_c4_boundary {
                    0.0
                } else {
                    self.config.rounded_corners * 1.5
                },
            );
            ctx.fill();
            ctx.stroke();
            if cluster_dash.is_some() {
                ctx.set_line_dash(&[]);
            }
            self.draw_calls += 2;

            if ir.diagram_type != DiagramType::Timeline {
                self.paint_cluster_caption(
                    cluster_box,
                    ir,
                    ctx,
                    (offset_x, offset_y),
                    &mut cluster_label_font,
                );
            }

            if cluster_opacity.is_some() {
                ctx.set_global_alpha(1.0);
            }

            count += 1;
        }

        count
    }

    /// Paint the caption independently so a later section tint cannot cover its lettering.
    fn paint_cluster_caption<C: Canvas2dContext>(
        &mut self,
        cluster_box: &fm_layout::LayoutClusterBox,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset: (f64, f64),
        cluster_label_font: &mut Option<String>,
    ) {
        let title_text = cluster_box.title.as_deref().or_else(|| {
            ir.clusters
                .get(cluster_box.cluster_index)
                .and_then(|ir_cluster| ir_cluster.title)
                .and_then(|title_id| ir.labels.get(title_id.0))
                .map(|label| label.text.as_str())
        });
        let Some(title_text) = title_text else {
            return;
        };
        let x = f64::from(cluster_box.bounds.x) + offset.0;
        let y = f64::from(cluster_box.bounds.y) + offset.1;
        let declared_title_color = resolve_cluster_text_color(ir, cluster_box.cluster_index);
        let title_color = declared_title_color
            .as_deref()
            .and_then(sanitize_canvas_paint);
        ctx.set_fill_style(title_color.as_deref().unwrap_or(&self.config.label_color));
        ctx.set_font(cluster_label_font.get_or_insert_with(|| {
            format!(
                "600 {}px {}",
                self.config.font_size * 0.9,
                self.config.font_family
            )
        }));
        ctx.set_text_align(TextAlign::Left);
        ctx.set_text_baseline(TextBaseline::Top);
        ctx.fill_text(title_text, x + 8.0, y + 4.0);
        self.draw_calls += 1;

        // C4 boundary types are the second row of a captioned boundary, so a boundary
        // without a title must not gain a bracketed type floating on its own.
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

    /// Use the shared chart composition so data markers, readable labels, and axes occupy the
    /// same coordinates in raster, SVG, and GPU output. Generic graph nodes obscure this chart.
    fn draw_quadrant_chart<C: Canvas2dContext>(
        &mut self,
        chart: &fm_layout::QuadrantChartLayout,
        meta: &fm_core::IrQuadrantMeta,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) -> usize {
        let plot = chart.plot;
        let left = f64::from(plot.x) + offset_x;
        let top = f64::from(plot.y) + offset_y;
        let width = f64::from(plot.width);
        let height = f64::from(plot.height);
        let mid_x = left + width / 2.0;
        let mid_y = top + height / 2.0;
        let mut labels_drawn = 0;
        ctx.save();
        ctx.set_line_dash(&[]);
        ctx.set_global_alpha(0.10);
        for (index, (x, y)) in [(mid_x, top), (left, top), (left, mid_y), (mid_x, mid_y)]
            .into_iter()
            .enumerate()
        {
            ctx.set_fill_style(&self.config.accents[index]);
            ctx.fill_rect(x, y, width / 2.0, height / 2.0);
            self.draw_calls += 1;
        }
        ctx.set_global_alpha(1.0);
        ctx.set_fill_style(&self.config.label_color);
        ctx.set_text_baseline(TextBaseline::Middle);
        ctx.set_text_align(TextAlign::Left);
        for heading in &chart.headings {
            ctx.set_font(&format!(
                "600 {}px {}",
                heading.font_size, self.config.font_family
            ));
            for (line, text) in heading.lines.iter().enumerate() {
                ctx.fill_text(
                    text,
                    f64::from(heading.bounds.x) + offset_x,
                    f64::from(heading.bounds.y + heading.font_size * (0.65 + line as f32 * 1.3))
                        + offset_y,
                );
                self.draw_calls += 1;
                labels_drawn += 1;
            }
        }

        ctx.set_stroke_style(&self.config.edge_stroke);
        ctx.set_line_width(1.0);
        for ((x1, y1), (x2, y2)) in [
            ((mid_x, top), (mid_x, top + height)),
            ((left, mid_y), (left + width, mid_y)),
        ] {
            ctx.begin_path();
            ctx.move_to(x1, y1);
            ctx.line_to(x2, y2);
            ctx.stroke();
            self.draw_calls += 1;
        }
        ctx.set_line_width(0.5);
        ctx.set_line_dash(&[4.0, 4.0]);
        ctx.set_global_alpha(0.3);
        for fraction in [0.25, 0.75] {
            for ((x1, y1), (x2, y2)) in [
                (
                    (left + width * fraction, top),
                    (left + width * fraction, top + height),
                ),
                (
                    (left, top + height * fraction),
                    (left + width, top + height * fraction),
                ),
            ] {
                ctx.begin_path();
                ctx.move_to(x1, y1);
                ctx.line_to(x2, y2);
                ctx.stroke();
                self.draw_calls += 1;
            }
        }
        ctx.set_line_dash(&[]);
        ctx.set_global_alpha(1.0);
        ctx.set_text_baseline(TextBaseline::Alphabetic);
        for annotation in &chart.annotations {
            let weight = if annotation.role == fm_layout::QuadrantTextRole::Title {
                600
            } else {
                400
            };
            ctx.set_font(&format!(
                "{weight} {}px {}",
                annotation.font_size, self.config.font_family
            ));
            ctx.set_text_align(match annotation.align {
                IrTextAlign::Start => TextAlign::Left,
                IrTextAlign::Middle => TextAlign::Center,
                IrTextAlign::End => TextAlign::Right,
            });
            for (text, position) in annotation.rows() {
                ctx.fill_text(
                    text,
                    f64::from(position.x) + offset_x,
                    f64::from(position.y) + offset_y,
                );
                self.draw_calls += 1;
                labels_drawn += 1;
            }
        }

        ctx.set_global_alpha(0.65);
        ctx.set_line_width(0.75);
        for leader in &chart.leaders {
            ctx.begin_path();
            ctx.move_to(
                f64::from(leader.start.x) + offset_x,
                f64::from(leader.start.y) + offset_y,
            );
            ctx.line_to(
                f64::from(leader.end.x) + offset_x,
                f64::from(leader.end.y) + offset_y,
            );
            ctx.stroke();
            self.draw_calls += 1;
        }
        let background = self
            .config
            .background
            .as_deref()
            .and_then(sanitize_canvas_paint)
            .unwrap_or_else(|| "#ffffff".to_owned());
        ctx.set_fill_style(&background);
        ctx.set_global_alpha(0.92);
        for label in &chart.labels {
            ctx.begin_path();
            self.emit_path_commands(
                ctx,
                &fm_layout::shapes::rounded_rect_path(label.bounds, 3.0),
                offset_x,
                offset_y,
            );
            ctx.fill();
            self.draw_calls += 1;
        }
        ctx.set_global_alpha(1.0);
        ctx.set_fill_style(&self.config.label_color);
        ctx.set_font(&format!(
            "{}px {}",
            chart.label_font_size, self.config.font_family
        ));
        ctx.set_text_baseline(TextBaseline::Middle);
        ctx.set_text_align(TextAlign::Left);
        for label in &chart.labels {
            for (line, text) in label.lines.iter().enumerate() {
                ctx.fill_text(
                    text,
                    f64::from(label.bounds.x + 3.0) + offset_x,
                    f64::from(
                        label.bounds.y + 2.0 + chart.label_font_size * (0.65 + line as f32 * 1.3),
                    ) + offset_y,
                );
                self.draw_calls += 1;
                labels_drawn += 1;
            }
        }
        for (index, point) in chart.points.iter().enumerate() {
            let (fill, stroke) = quadrant_point_colors(meta.points.get(index), index, &self.config);
            ctx.set_fill_style(&fill);
            ctx.set_stroke_style(&stroke);
            ctx.begin_path();
            ctx.arc(
                f64::from(point.center.x) + offset_x,
                f64::from(point.center.y) + offset_y,
                f64::from(point.radius),
                0.0,
                std::f64::consts::TAU,
            );
            ctx.fill();
            self.draw_calls += 1;
            if point.stroke_width > 0.0 {
                ctx.set_line_width(f64::from(point.stroke_width));
                ctx.stroke();
                self.draw_calls += 1;
            }
        }
        ctx.restore();
        labels_drawn
    }
}

impl Canvas2dRenderer {
    /// Draw every tile's color, measured caption and value, retaining overflow in details.
    fn draw_treemap<C: Canvas2dContext>(
        &mut self,
        chart: &fm_layout::TreemapChartLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) -> usize {
        let scene = chart.to_scene(&self.config.label_color, &self.config.node_stroke);
        let mut stats = SceneRenderStats::default();
        self.render_group(&scene.root, ctx, offset_x, offset_y, &mut stats);
        stats.labels_drawn
    }

    /// Consume the shared wheel, measured captions and series legend in layout coordinates.
    fn draw_radar<C: Canvas2dContext>(
        &mut self,
        chart: Option<&fm_layout::RadarChartLayout>,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) -> usize {
        let Some(chart) = chart else {
            return 0;
        };
        let scene = chart.to_scene(&self.config.label_color, &self.config.node_stroke);
        let mut stats = SceneRenderStats::default();
        self.render_group(&scene.root, ctx, offset_x, offset_y, &mut stats);
        stats.labels_drawn
    }

    /// Draw an `info` diagram: a single centred version string matching `fm-render-svg` (bd-cfigc).
    fn draw_info_version<C: Canvas2dContext>(
        &mut self,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
        labels_drawn: &mut usize,
    ) {
        let font = format!("32px {}", self.config.font_family);
        ctx.set_font(&font);
        ctx.set_fill_style(&self.config.label_color);
        ctx.set_text_align(TextAlign::Center);
        ctx.set_text_baseline(TextBaseline::Middle);
        ctx.fill_text(
            concat!("v", env!("CARGO_PKG_VERSION")),
            100.0 + offset_x,
            40.0 + offset_y,
        );
        self.draw_calls += 1;
        *labels_drawn += 1;
    }
}

impl Canvas2dRenderer {
    /// Draw the same value scale, category slots and titles used by the SVG chart.
    #[allow(clippy::too_many_arguments)]
    fn draw_xychart_axes<C: Canvas2dContext>(
        &mut self,
        ir: &MermaidDiagramIr,
        meta: &IrXyChartMeta,
        layout: &DiagramLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) -> usize {
        let plot = fm_layout::xychart_plot_bounds(meta, layout.bounds);
        let (left, top) = (f64::from(plot.x) + offset_x, f64::from(plot.y) + offset_y);
        let (width, height) = (f64::from(plot.width), f64::from(plot.height));
        let (right, bottom) = (left + width, top + height);
        let (min, max) = fm_layout::resolve_xychart_y_domain(meta);
        let categories = fm_layout::xychart_categories(meta);
        let mut labels = 0;

        ctx.set_fill_style("rgba(148,163,184,0.06)");
        ctx.fill_rect(left, top, width, height);
        ctx.set_stroke_style("rgba(148,163,184,0.16)");
        ctx.set_line_width(1.0);
        ctx.set_line_dash(&[]);
        ctx.stroke_rect(left, top, width, height);
        self.draw_calls += 2;

        ctx.set_font(&format!(
            "{}px {}",
            (self.config.font_size * 0.72).max(8.0),
            self.config.font_family
        ));
        ctx.set_fill_style(&self.config.label_color);
        ctx.set_text_baseline(TextBaseline::Alphabetic);
        for value in fm_layout::xychart_y_ticks(min, max) {
            let (x, y) = if meta.horizontal {
                (
                    f64::from(fm_layout::xychart_value_to_x(value, min, max, plot)) + offset_x,
                    top,
                )
            } else {
                (
                    left,
                    f64::from(fm_layout::xychart_value_to_y(value, min, max, plot)) + offset_y,
                )
            };
            ctx.set_stroke_style("rgba(148,163,184,0.35)");
            ctx.set_line_dash(&[4.0, 4.0]);
            ctx.begin_path();
            ctx.move_to(x, y);
            if meta.horizontal {
                ctx.line_to(x, bottom);
            } else {
                ctx.line_to(right, y);
            }
            ctx.stroke();

            ctx.set_stroke_style(&self.config.label_color);
            ctx.set_line_dash(&[]);
            ctx.begin_path();
            ctx.move_to(x, y);
            if meta.horizontal {
                ctx.line_to(x, top - 5.0);
                ctx.set_text_align(TextAlign::Center);
                ctx.fill_text(&fm_layout::format_xychart_tick_value(value), x, top - 11.0);
            } else {
                ctx.line_to(left - 5.0, y);
                ctx.set_text_align(TextAlign::Right);
                ctx.fill_text(
                    &fm_layout::format_xychart_tick_value(value),
                    left - 10.0,
                    y + 4.0,
                );
            }
            ctx.stroke();
            self.draw_calls += 3;
            labels += 1;
        }

        ctx.set_stroke_style(&self.config.edge_stroke);
        ctx.set_line_width(1.5);
        ctx.begin_path();
        ctx.move_to(left, top);
        ctx.line_to(left, bottom);
        if meta.horizontal {
            ctx.move_to(left, top);
            ctx.line_to(right, top);
        } else {
            ctx.move_to(left, bottom);
            ctx.line_to(right, bottom);
        }
        ctx.stroke();
        self.draw_calls += 1;

        let category_font = (self.config.font_size * 0.74).max(8.0);
        ctx.set_font(&format!("{category_font}px {}", self.config.font_family));
        ctx.set_stroke_style(&self.config.label_color);
        ctx.set_line_width(1.0);
        let band = if meta.horizontal { height } else { width } / categories.len().max(1) as f64;
        for (index, category) in categories.iter().enumerate() {
            let position = band * (index as f64 + 0.5);
            ctx.begin_path();
            if meta.horizontal {
                let y = top + position;
                ctx.move_to(left - 5.0, y);
                ctx.line_to(left, y);
                ctx.set_text_align(TextAlign::Right);
                ctx.fill_text(category, left - 11.0, y + category_font * 0.35);
            } else {
                let x = left + position;
                ctx.move_to(x, bottom);
                ctx.line_to(x, bottom + 5.0);
                ctx.set_text_align(TextAlign::Center);
                ctx.fill_text(category, x, bottom + 24.0);
            }
            ctx.stroke();
            self.draw_calls += 2;
            labels += 1;
        }

        ctx.set_font(&format!(
            "{}px {}",
            (self.config.font_size * 0.76).max(8.0),
            self.config.font_family
        ));
        if let Some(label) = meta.y_axis.label.as_deref() {
            if meta.horizontal {
                ctx.set_text_align(TextAlign::Center);
                ctx.fill_text(label, left + width / 2.0, top - 34.0);
            } else {
                ctx.set_text_align(TextAlign::Left);
                ctx.fill_text(label, left - 52.0, top - 12.0);
            }
            self.draw_calls += 1;
            labels += 1;
        }
        if let Some(label) = meta.x_axis.label.as_deref() {
            if meta.horizontal {
                ctx.set_text_align(TextAlign::Right);
                ctx.fill_text(label, left - 11.0, top - 34.0);
            } else {
                ctx.set_text_align(TextAlign::Center);
                ctx.fill_text(label, left + width / 2.0, bottom + 48.0);
            }
            self.draw_calls += 1;
            labels += 1;
        }
        if let Some(title) = ir.declared_title_if_drawn() {
            ctx.set_font(&format!(
                "600 {}px {}",
                (self.config.font_size * 1.18).max(8.0),
                self.config.font_family
            ));
            ctx.set_text_align(TextAlign::Center);
            ctx.fill_text(
                title,
                f64::from(layout.bounds.x + layout.bounds.width / 2.0) + offset_x,
                top - if meta.horizontal { 70.0 } else { 34.0 },
            );
            self.draw_calls += 1;
            labels += 1;
        }
        labels
    }

    /// Paint data from the layout's exact mark boxes; never reconstruct their values or ordering.
    fn draw_xychart_series<C: Canvas2dContext>(
        &mut self,
        meta: &IrXyChartMeta,
        layout: &DiagramLayout,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) -> (usize, usize) {
        let plot = fm_layout::xychart_plot_bounds(meta, layout.bounds);
        let (min, max) = fm_layout::resolve_xychart_y_domain(meta);
        let baseline = min.min(0.0).max(max.min(0.0));
        let baseline_x =
            f64::from(fm_layout::xychart_value_to_x(baseline, min, max, plot)) + offset_x;
        let baseline_y =
            f64::from(fm_layout::xychart_value_to_y(baseline, min, max, plot)) + offset_y;
        let by_index: std::collections::BTreeMap<_, _> = layout
            .nodes
            .iter()
            .map(|node| (node.node_index, node))
            .collect();
        let (mut nodes_drawn, mut edges_drawn) = (0, 0);
        ctx.set_line_dash(&[]);

        for (series_index, series) in meta.series.iter().enumerate() {
            let color = &self.config.accents[series_index % self.config.accents.len()];
            let nodes: Vec<_> = series
                .nodes
                .iter()
                .filter_map(|id| by_index.get(&id.0).copied())
                .collect();
            let Some(first) = nodes.first() else {
                continue;
            };
            ctx.set_fill_style(color);
            ctx.set_stroke_style(color);
            nodes_drawn += nodes.len();
            if series.kind == IrXySeriesKind::Bar {
                ctx.set_line_width(1.0);
                for node in nodes {
                    let bounds = node.bounds;
                    let (x, y) = (
                        f64::from(bounds.x) + offset_x,
                        f64::from(bounds.y) + offset_y,
                    );
                    let (w, h) = (f64::from(bounds.width), f64::from(bounds.height));
                    ctx.set_global_alpha(0.78);
                    ctx.fill_rect(x, y, w, h);
                    ctx.set_global_alpha(1.0);
                    ctx.stroke_rect(x, y, w, h);
                    self.draw_calls += 2;
                }
                continue;
            }

            let center = |node: &fm_layout::LayoutNodeBox| {
                let center = node.bounds.center();
                (
                    f64::from(center.x) + offset_x,
                    f64::from(center.y) + offset_y,
                )
            };
            let (first_x, first_y) = center(first);
            if series.kind == IrXySeriesKind::Area {
                ctx.begin_path();
                if meta.horizontal {
                    ctx.move_to(baseline_x, first_y);
                } else {
                    ctx.move_to(first_x, baseline_y);
                }
                let (mut last_x, mut last_y) = (first_x, first_y);
                for node in &nodes {
                    (last_x, last_y) = center(node);
                    ctx.line_to(last_x, last_y);
                }
                if meta.horizontal {
                    ctx.line_to(baseline_x, last_y);
                } else {
                    ctx.line_to(last_x, baseline_y);
                }
                ctx.close_path();
                ctx.set_global_alpha(0.16);
                ctx.fill();
                ctx.set_global_alpha(1.0);
                self.draw_calls += 1;
            }
            ctx.set_line_width(3.0);
            ctx.set_line_cap(LineCap::Round);
            ctx.set_line_join(LineJoin::Round);
            ctx.begin_path();
            ctx.move_to(first_x, first_y);
            for node in nodes.iter().skip(1) {
                let (x, y) = center(node);
                ctx.line_to(x, y);
            }
            ctx.stroke();
            self.draw_calls += 1;
            edges_drawn += nodes.len().saturating_sub(1);

            ctx.set_stroke_style(&self.config.node_fill);
            ctx.set_line_width(2.0);
            for node in nodes {
                let (x, y) = center(node);
                let radius = f64::from((node.bounds.width.min(node.bounds.height) / 2.0).max(3.5));
                ctx.begin_path();
                ctx.arc(x, y, radius, 0.0, std::f64::consts::TAU);
                ctx.fill();
                ctx.stroke();
                self.draw_calls += 2;
            }
        }
        (nodes_drawn, edges_drawn)
    }

    fn draw_bands<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
    ) {
        use fm_layout::LayoutBandKind;
        let diagram_type = ir.diagram_type;
        let gantt = diagram_type == DiagramType::Gantt;
        let mut section_label_font: Option<String> = None;
        for (index, band) in layout.extensions.bands.iter().enumerate() {
            // The timeline's cluster already supplies this exact background, including all
            // authored style channels. An opaque duplicate would cover those declarations.
            if band_has_cluster_background(diagram_type, layout, band) {
                continue;
            }
            let x = f64::from(band.bounds.x) + offset_x;
            let y = f64::from(band.bounds.y) + offset_y;
            let w = f64::from(band.bounds.width);
            let h = f64::from(band.bounds.height);

            if diagram_type == DiagramType::Sequence && band.kind == LayoutBandKind::Lane {
                // Only sequence lanes represent lifelines; journey and branch lanes are panels.
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
                continue;
            }

            ctx.set_fill_style(if gantt && !index.is_multiple_of(2) {
                &self.config.node_fill
            } else {
                &self.config.cluster_fill
            });
            ctx.set_stroke_style(&self.config.cluster_stroke);
            ctx.set_line_width(if gantt { 0.75 } else { 1.0 });
            ctx.set_line_dash(&[]);
            trace_rounded_rect(
                ctx,
                x,
                y,
                w,
                h,
                if gantt {
                    4.0
                } else {
                    self.config.rounded_corners.max(4.0)
                },
            );
            ctx.fill();
            ctx.stroke();
            self.draw_calls += 2;

            let repeated_caption = (diagram_type == DiagramType::Journey
                && band.kind == LayoutBandKind::Lane)
                || (diagram_type == DiagramType::Timeline && band.kind == LayoutBandKind::Section);
            if !band.label.is_empty() && !repeated_caption {
                ctx.set_fill_style(&self.config.label_color);
                ctx.set_font(section_label_font.get_or_insert_with(|| {
                    format!(
                        "600 {}px {}",
                        if gantt {
                            self.config.font_size * 0.85
                        } else {
                            (self.config.font_size * 0.82).max(8.0)
                        },
                        self.config.font_family
                    )
                }));
                ctx.set_text_align(TextAlign::Left);
                ctx.set_text_baseline(TextBaseline::Alphabetic);
                ctx.fill_text(
                    &band.label,
                    x + if gantt { 6.0 } else { 8.0 },
                    y + if gantt {
                        self.config.font_size * 0.9
                    } else {
                        16.0
                    },
                );
                self.draw_calls += 1;
            }
        }
    }

    /// Draw sequence activation bars from layout extensions.
    /// Draw axis tick labels supplied by layout extensions, such as gantt dates.
    ///
    /// `extensions.axis_ticks` is filled by the gantt layout arm and drawn by
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
        let metrics = self.config.font_metrics();
        let label_rows = fm_layout::gantt_axis_label_rows(layout, &metrics);
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
        for (axis_index, y) in axis_rows.into_iter().enumerate() {
            for (tick_index, tick) in layout.extensions.axis_ticks.iter().enumerate() {
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
                        (self.config.font_size * 0.72).max(8.0),
                        self.config.font_family
                    )
                }));
                // `x + 3.0`, as the SVG writes it: the label sits just right of its tick mark rather
                // than centred on it.
                let label_y = layout
                    .extensions
                    .gantt_axis_rows
                    .get(axis_index)
                    .map_or(y, |axis| {
                        f64::from(fm_layout::gantt_axis_label_baseline(
                            *axis,
                            label_rows[tick_index],
                            &metrics,
                        )) + offset_y
                    });
                ctx.fill_text(&tick.label, tick_x + 3.0, label_y);
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

            // Leader from the annotated state to the note; a note on the whole diagram has none.
            if note.leader_start != note.leader_end {
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
            }

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
                // Layout reserves each row at the note font size and the configured line height.
                // Keep that spacing when the caller chooses a non-default line-height multiplier.
                let line_height =
                    self.config.font_size * 0.8 * f64::from(self.config.resolved_line_height());
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
        let font_size =
            f64::from(self.config.font_size as f32 * fm_layout::LayoutSequenceNote::FONT_SCALE);
        let line_height = font_size * f64::from(self.config.resolved_line_height());

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
            self.draw_calls += 2;

            // Note text.
            if !note.text.is_empty() {
                ctx.set_fill_style(&self.config.label_color);
                let note_font = note_font
                    .get_or_insert_with(|| format!("{font_size}px {}", self.config.font_family));
                ctx.set_font(note_font.as_str());
                ctx.set_text_align(TextAlign::Left);
                ctx.set_text_baseline(TextBaseline::Top);
                for (row, line) in note.text.split('\n').enumerate() {
                    ctx.fill_text(
                        line,
                        x + f64::from(fm_layout::LayoutSequenceNote::PADDING_X),
                        y + f64::from(fm_layout::LayoutSequenceNote::PADDING_Y)
                            + row as f64 * line_height,
                    );
                    self.draw_calls += 1;
                }
                *labels_drawn += 1;
            }
        }
    }

    /// Draw all edges.
    /// Mask the connector with a quiet, rounded plate matching the SVG edge-label surface.
    /// A standalone transparent canvas keeps its configured node surface as the safe fallback.
    fn fill_edge_label_background<C: Canvas2dContext>(
        &self,
        ctx: &mut C,
        center_x: f64,
        center_y: f64,
        width: f64,
        height: f64,
    ) {
        let background = self
            .config
            .background
            .as_deref()
            .and_then(sanitize_canvas_paint);
        ctx.set_fill_style(background.as_deref().unwrap_or(&self.config.node_fill));
        trace_rounded_rect(
            ctx,
            center_x - width / 2.0,
            center_y - height / 2.0,
            width,
            height,
            4.0,
        );
        ctx.fill();
    }

    fn draw_edges<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
        labels_drawn: &mut usize,
    ) -> usize {
        let count = self.draw_edge_pass(layout, ir, ctx, offset_x, offset_y, labels_drawn, false);
        if self.edge_label_layout.is_some() {
            // Shared corridors can carry several labels. Paint every connector first so a
            // later route cannot cross the opaque plate and glyphs of an earlier label.
            self.draw_edge_pass(layout, ir, ctx, offset_x, offset_y, labels_drawn, true);
        }
        count
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_edge_pass<C: Canvas2dContext>(
        &mut self,
        layout: &DiagramLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
        labels_drawn: &mut usize,
        labels_only: bool,
    ) -> usize {
        let mut count = 0;
        let mut edge_label_font = None;
        let sankey_widest = fm_layout::sankey_widest_flow(ir);

        for edge_path in &layout.edges {
            let ir_edge = ir.edges.get(edge_path.edge_index);
            let arrow = ir_edge.map_or(ArrowType::Arrow, |e| e.arrow);
            // A `~~~` link has placed its nodes; it is not drawn, nor is its label.
            if edge_path.bundled || arrow == ArrowType::Invisible {
                continue;
            }

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
            let (declared_stroke, declared_width) =
                resolve_edge_style_with_defaults(ir, edge_path.edge_index, &self.config);
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
            let quantity_width = fm_layout::sankey_flow_stroke_width(ir, ir_edge, sankey_widest)
                .map(f64::from)
                .unwrap_or(legacy_width);
            let stroke_width = declared_width.unwrap_or(quantity_width);
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

            if !labels_only {
                ctx.set_stroke_style(stroke);
                ctx.set_line_width(stroke_width);
                ctx.set_line_dash(dash);

                // Draw the same bounded connector geometry as SVG and the shared scene. Native
                // quadratic/cubic commands keep elbows smooth at any zoom; specialized messages and
                // chart paths retain their polylines through the shared diagram-family policy.
                ctx.begin_path();
                let path_endpoints = fm_layout::edge_paths::emit_diagram_edge_path(
                    ir,
                    edge_path.edge_index,
                    points,
                    |command| {
                        self.emit_path_commands(
                            ctx,
                            std::slice::from_ref(&command),
                            offset_x,
                            offset_y,
                        );
                    },
                );
                ctx.stroke();
                self.draw_calls += 1;

                if let Some(endpoints) = path_endpoints {
                    let (marker_start, marker_end) = ir_edge
                        .map_or((MarkerKind::None, MarkerKind::Arrow), |edge| {
                            fm_layout::edge_marker_kinds(edge, edge_path.reversed)
                        });
                    // A dashed connector still has solid endpoint glyphs. Retaining the path's
                    // dash here can erase an entire short stick or cardinality bar.
                    ctx.set_line_dash(&[]);
                    for (marker, endpoint, at_start) in [
                        (marker_start, endpoints.start, true),
                        (marker_end, endpoints.end, false),
                    ] {
                        if marker == MarkerKind::None {
                            continue;
                        }
                        let angle =
                            f64::from(endpoint.tangent.y).atan2(f64::from(endpoint.tangent.x));
                        self.draw_calls += draw_marker_primitive(
                            ctx,
                            marker,
                            f64::from(endpoint.point.x) + offset_x,
                            f64::from(endpoint.point.y) + offset_y,
                            if at_start {
                                start_marker_angle(marker, angle)
                            } else {
                                angle
                            },
                            &self.config.node_fill,
                            stroke,
                        );
                    }
                }
            }
            if !labels_only && self.edge_label_layout.is_some() {
                ctx.set_line_dash(&[]);
                if edge_opacity.is_some() {
                    ctx.set_global_alpha(1.0);
                }
                count += 1;
                continue;
            }

            // CLASS cardinalities reach the canvas (bd-rk14).
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
            //
            // ⚠️ ER cardinality is NOT placed here (bd-b1sy2): the crow's-foot markers drawn above
            // carry it, matching the incumbent, which renders no cardinality text at all. The
            // terminal keeps the text because a character grid has no marker carrier.
            if let Some(edge) = ir_edge
                .filter(|e| e.source_cardinality().is_some() || e.target_cardinality().is_some())
                && points.len() >= 2
            {
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
                place(edge.source_cardinality(), &first, &second);
                place(edge.target_cardinality(), &last, &penultimate);
            }

            // A Sankey edge label stores its quantity. The ribbon width and node totals
            // communicate that value; SVG likewise omits a separate number on each link.
            if let Some(message) = layout.extensions.sequence_message(edge_path.edge_index) {
                let bounds = message.bounds;
                ctx.set_fill_style(&self.config.node_fill);
                ctx.fill_rect(
                    f64::from(bounds.x) + offset_x,
                    f64::from(bounds.y) + offset_y,
                    f64::from(bounds.width),
                    f64::from(bounds.height),
                );
                self.draw_calls += 1;
                ctx.set_fill_style(edge_label_fill);
                ctx.set_font(&format!(
                    "{}px {}",
                    message.font_size, self.config.font_family
                ));
                ctx.set_text_align(TextAlign::Center);
                ctx.set_text_baseline(TextBaseline::Middle);
                for (row, text) in message.text.split('\n').enumerate() {
                    let center = message.line_center(row);
                    ctx.fill_text(
                        text,
                        f64::from(center.x) + offset_x,
                        f64::from(center.y) + offset_y,
                    );
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                }
            } else if ir.diagram_type != DiagramType::Sankey
                && let Some(caption) = ir.graph_edge_caption(edge_path.edge_index)
                && points.len() >= 2
            {
                let label_text = &caption.label;
                let label_offset = self.config.font_size * 0.8;
                let planned = self
                    .edge_label_layout
                    .as_ref()
                    .and_then(|plan| plan.labels.get(edge_path.edge_index))
                    .copied()
                    .flatten();
                // Generic graph labels use the same collision solver as SVG. Sequence messages
                // and chart annotations keep their existing specialized anchor and offset.
                let (lx, ly) = if let Some(label) = planned {
                    (
                        f64::from(label.position.x) + offset_x,
                        f64::from(label.position.y) + offset_y,
                    )
                } else {
                    let anchor = fm_layout::edge_label_position(edge_path);
                    (
                        f64::from(anchor.x) + offset_x,
                        f64::from(anchor.y) + offset_y - label_offset,
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

                let line_height =
                    planned.map_or(self.config.font_size * 1.2, |label| label.line_height);

                // The common single-line edge label (`:has`, `-->|x|`, …) draws exactly `label.text` at
                // `ly`, so measure it directly and skip the `Vec<&str>` collect. Only a genuinely
                // multi-line label (`\n`) needs the split (it's re-read for max-width + count + per-line
                // draw). Byte-identical: for a `\n`-free label the sole `lines()` item IS `label.text`,
                // `total_height == line_height`, and `start_y == ly`.
                if let Some(technology) = &caption.technology {
                    let font = match declared_label_font.as_deref() {
                        Some(font) => font,
                        None => edge_label_font
                            .get_or_insert_with(|| secondary_label_font_css(&self.config))
                            .as_str(),
                    };
                    let (width, height, line_height) = planned.map_or_else(
                        || measure_graph_edge_caption(&caption, ctx, &self.config, font),
                        |label| (label.width, label.height, label.line_height),
                    );
                    self.fill_edge_label_background(ctx, lx, ly, width, height);
                    self.draw_calls += 1;
                    ctx.set_fill_style(edge_label_fill);
                    ctx.set_text_align(TextAlign::Center);
                    ctx.set_text_baseline(TextBaseline::Middle);
                    let start_y = ly - (caption.line_count().max(1) - 1) as f64 * line_height / 2.0;
                    ctx.set_font(font);
                    for (row, line) in caption.label.lines().enumerate() {
                        ctx.fill_text(line, lx, start_y + row as f64 * line_height);
                        self.draw_calls += 1;
                        *labels_drawn += 1;
                    }
                    ctx.set_font(&format!("italic {font}"));
                    let label_rows = caption.label.lines().count();
                    for (row, line) in technology.lines().enumerate() {
                        ctx.fill_text(line, lx, start_y + (label_rows + row) as f64 * line_height);
                        self.draw_calls += 1;
                        *labels_drawn += 1;
                    }
                } else if !label_text.contains('\n') {
                    let label_width = planned.map_or_else(
                        || ctx.measure_text(label_text).width + 8.0,
                        |label| label.width,
                    );
                    let label_height = planned.map_or(line_height + 4.0, |label| label.height);

                    self.fill_edge_label_background(ctx, lx, ly, label_width, label_height);
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
                    ctx.fill_text(label_text, lx, ly);
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                } else {
                    // Background for label
                    let lines: Vec<&str> = label_text.lines().collect();
                    let label_width = planned.map_or_else(
                        || {
                            lines
                                .iter()
                                .map(|line| ctx.measure_text(line).width)
                                .fold(0.0_f64, f64::max)
                                + 8.0
                        },
                        |label| label.width,
                    );
                    let total_height = lines.len() as f64 * line_height;
                    let label_height = planned.map_or(total_height + 4.0, |label| label.height);

                    self.fill_edge_label_background(ctx, lx, ly, label_width, label_height);
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
        // Pie parser nodes retain slice labels for layout; their circles are not chart marks.
        // The specialized wedge pass already paints each sector and its percentage label.
        if ir.diagram_type == DiagramType::Pie {
            return 0;
        }
        let mut count = 0;
        let mut standard_label_font = None;
        let label_metrics = self.config.font_metrics();
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
        let gantt_task_by_node: Option<std::collections::HashMap<usize, &fm_core::IrGanttTask>> =
            ir.gantt_meta
                .as_ref()
                .filter(|_| ir.diagram_type == DiagramType::Gantt)
                .map(|meta| meta.tasks.iter().map(|task| (task.node.0, task)).collect());

        for node_box in &layout.nodes {
            // Referenced composite states share their cluster's box. The cluster was already
            // painted behind its children; drawing the duplicate node here would cover them.
            if fm_layout::is_composite_state_node(ir, node_box) {
                continue;
            }
            let ir_node = ir.nodes.get(node_box.node_index);
            let shape = ir_node.map_or(NodeShape::Rect, |n| n.shape);
            let gantt_task = gantt_task_by_node
                .as_ref()
                .and_then(|tasks| tasks.get(&node_box.node_index))
                .copied();
            let gantt_label = gantt_label_by_node
                .as_ref()
                .and_then(|placements| placements.get(&node_box.node_index))
                .copied();

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
            let (fill, stroke) =
                resolve_node_colors_with_defaults(ir, node_box.node_index, &self.config);
            let (fill, stroke) = if let Some(task) = gantt_task {
                (
                    fill.or_else(|| Some(fm_layout::gantt_task_fill(task.flags).to_owned())),
                    stroke.or_else(|| task.flags.crit.then(|| "#dc2626".to_owned())),
                )
            } else {
                (fill, stroke)
            };
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
            let label_fill = text_color.as_deref().unwrap_or_else(|| {
                if ir.diagram_type == DiagramType::Journey
                    || ir_node.is_some_and(|node| node.requirement_meta.is_some())
                    || (gantt_task.is_some()
                        && gantt_label.is_none_or(|entry| {
                            entry.placement == fm_layout::GanttLabelPlacement::Inside
                        }))
                {
                    fm_core::readable_label_color(
                        fill.as_deref().unwrap_or(&self.config.node_fill),
                        &self.config.label_color,
                        &self.config.node_fill,
                    )
                } else {
                    &self.config.label_color
                }
            });
            // The border WIDTH is the third channel of the same declaration and was the last one
            // still discarded here: the edge resolver has read `stroke-width` since bd-lvj3's edge
            // half landed, while every node border was drawn at the config width whatever the
            // author wrote. `unwrap_or` keeps the theme default when nothing was declared.
            let default_stroke_width = if gantt_task.is_some_and(|task| task.flags.crit) {
                2.0
            } else {
                self.config.node_stroke_width
            };
            let stroke_width =
                resolve_node_stroke_width(ir, node_box.node_index).unwrap_or(default_stroke_width);
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
            let declared_parts = resolve_declared_node_font(ir, node_box.node_index);
            let dash = resolve_node_stroke_dasharray(ir, node_box.node_index);
            if let Some(ref pattern) = dash {
                ctx.set_line_dash(pattern);
            }
            let is_state_terminal =
                ir.diagram_type == DiagramType::State && shape == NodeShape::DoubleCircle;
            if let Some(task) = gantt_task {
                // Task bounds encode calendar time. A milestone is an instant centered on that
                // date, so its glyph must not turn the layout's hit box into a duration bar.
                let commands = if task.flags.milestone {
                    let center = node_box.bounds.center();
                    let radius = node_box.bounds.width.min(node_box.bounds.height) * 0.4;
                    fm_layout::shapes::diamond_path(fm_layout::LayoutRect {
                        x: center.x - radius,
                        y: center.y - radius,
                        width: radius * 2.0,
                        height: radius * 2.0,
                    })
                } else {
                    fm_layout::shapes::rounded_rect_path(node_box.bounds, 3.0)
                };
                ctx.set_fill_style(fill.as_deref().unwrap_or(&self.config.node_fill));
                ctx.set_stroke_style(stroke.as_deref().unwrap_or(&self.config.node_stroke));
                ctx.set_line_width(stroke_width);
                ctx.begin_path();
                self.emit_path_commands(ctx, &commands, offset_x, offset_y);
                ctx.fill();
                ctx.stroke();
            } else {
                draw_shape(
                    ctx,
                    if is_state_terminal {
                        NodeShape::Circle
                    } else {
                        shape
                    },
                    x,
                    y,
                    w,
                    h,
                    fill.as_deref().unwrap_or(&self.config.node_fill),
                    stroke.as_deref().unwrap_or(&self.config.node_stroke),
                    stroke_width,
                    self.config.rounded_corners,
                );
            }
            if is_state_terminal {
                // Draw the solid terminal disc once; ordinary double circles keep a hollow ring.
                let radius = w.min(h) / 2.0;
                ctx.set_fill_style(fill.as_deref().unwrap_or(&self.config.edge_stroke));
                ctx.begin_path();
                ctx.arc(
                    x + w / 2.0,
                    y + h / 2.0,
                    (radius - 4.0).max(radius * 0.5),
                    0.0,
                    std::f64::consts::TAU,
                );
                ctx.fill();
                ctx.stroke();
            }
            if dash.is_some() {
                ctx.set_line_dash(&[]);
            }
            self.draw_calls += 1;

            if let Some(task) = gantt_task
                && !task.flags.milestone
                && let Some(progress) = task
                    .progress
                    .filter(|value| value.is_finite() && *value > 0.0)
            {
                // The current path is the task bar. Clip the quantitative overlay to its rounded
                // outline, multiply the node opacity, then restore every paint state for its label.
                ctx.save();
                ctx.clip();
                ctx.set_fill_style("#0f172a");
                ctx.set_global_alpha(declared_opacity.unwrap_or(1.0) * 0.22);
                ctx.fill_rect(x, y, w * f64::from(progress.min(1.0)), h);
                ctx.restore();
                self.draw_calls += 1;
            }

            if let Some(step) = layout
                .extensions
                .journey
                .as_ref()
                .and_then(|journey| journey.step(node_box.node_index))
            {
                ctx.set_fill_style(label_fill);
                ctx.set_text_align(TextAlign::Center);
                ctx.set_text_baseline(TextBaseline::Middle);
                for row in &step.rows {
                    let center = row.bounds.center();
                    ctx.set_font(&declared_parts.compose(
                        f64::from(row.font_size),
                        None,
                        &self.config.font_family,
                    ));
                    ctx.fill_text(
                        &row.text,
                        f64::from(center.x) + offset_x,
                        f64::from(center.y) + offset_y,
                    );
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                }
            // Check for class diagram three-compartment rendering.
            } else if let Some(node) = ir_node
                && let Some(ref meta) = node.class_meta
                && meta.has_compartments()
            {
                let class_name = node
                    .label
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|l| l.text.as_str())
                    .unwrap_or(&node.id);
                let declared = ir.resolved_node_style(node_box.node_index);
                let compartments = fm_core::ClassCompartmentLayout::with_font_size_override(
                    meta,
                    class_name,
                    self.config.font_size as f32,
                    self.config.resolved_line_height(),
                    declared.get("font-size").map(String::as_str),
                );
                let border = stroke.as_deref().unwrap_or(&self.config.node_stroke);
                if fill.is_none() && w > 2.0 && h > 2.0 {
                    ctx.set_fill_style(&fm_core::mindmap::branch_fill(
                        &self.config.node_fill,
                        border,
                        false,
                    ));
                    ctx.fill_rect(
                        x + 0.75,
                        y + 0.75,
                        w - 1.5,
                        f64::from(compartments.header_height).min(h) - 0.75,
                    );
                    self.draw_calls += 1;
                }
                ctx.set_stroke_style(border);
                ctx.set_line_width(0.75);
                ctx.set_global_alpha(declared_opacity.unwrap_or(1.0) * 0.65);
                for separator_y in compartments.separator_ys {
                    ctx.begin_path();
                    ctx.move_to(x + 0.75, y + f64::from(separator_y));
                    ctx.line_to(x + w - 0.75, y + f64::from(separator_y));
                    ctx.stroke();
                    self.draw_calls += 1;
                }
                ctx.set_global_alpha(declared_opacity.unwrap_or(1.0));
                ctx.set_fill_style(label_fill);
                ctx.set_text_baseline(TextBaseline::Middle);
                for row in compartments.rows {
                    let font = declared_parts.compose(
                        f64::from(row.font_size),
                        (row.kind == fm_core::ClassCompartmentRowKind::Name).then_some("600"),
                        &self.config.font_family,
                    );
                    ctx.set_font(&if row.italic && declared_parts.style.is_none() {
                        format!("italic {font}")
                    } else {
                        font
                    });
                    ctx.set_text_align(if row.centered() {
                        TextAlign::Center
                    } else {
                        TextAlign::Left
                    });
                    let text_x = if row.centered() {
                        x + w * 0.5
                    } else {
                        x + f64::from(fm_core::ClassCompartmentLayout::PADDING_X)
                    };
                    let text_y = y + f64::from(row.center_y);
                    ctx.fill_text(&row.text, text_x, text_y);
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                    if row.underline {
                        let width = ctx.measure_text(&row.text).width;
                        let baseline = text_y + f64::from(row.font_size) * 0.5;
                        ctx.set_stroke_style(label_fill);
                        ctx.begin_path();
                        ctx.move_to(text_x, baseline);
                        ctx.line_to(text_x + width, baseline);
                        ctx.stroke();
                        self.draw_calls += 1;
                    }
                }
            } else if let Some(node) = ir_node.filter(|n| !n.members.is_empty()) {
                let entity_name = node
                    .label
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|l| l.text.as_str())
                    .unwrap_or(&node.id);
                let declared = ir.resolved_node_style(node_box.node_index);
                let metrics = fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
                    preset: fm_core::FontPreset::from_family(
                        declared_parts
                            .family
                            .as_deref()
                            .unwrap_or(&self.config.font_family),
                    ),
                    ..self.config.font_metrics().config().clone()
                });
                let card = fm_core::ErCardLayout::new(
                    &node.members,
                    entity_name,
                    &metrics,
                    declared.get("font-size").map(String::as_str),
                    fm_core::ErCardLayout::MIN_ATTRIBUTE_FONT_SIZE,
                );
                ctx.set_stroke_style(stroke.as_deref().unwrap_or(&self.config.node_stroke));
                ctx.set_line_width(0.8);
                ctx.begin_path();
                ctx.move_to(x + 2.0, y + f64::from(card.header_height));
                ctx.line_to(x + w - 2.0, y + f64::from(card.header_height));
                ctx.stroke();
                self.draw_calls += 1;
                ctx.set_fill_style(label_fill);
                ctx.set_text_baseline(TextBaseline::Middle);
                // Every run was measured into the box. A separate cursor/cutoff previously
                // discarded the twelfth and subsequent attributes from ordinary default tables.
                for run in card.text {
                    ctx.set_font(&declared_parts.compose(
                        f64::from(run.font_size),
                        run.emphasized.then_some("bold"),
                        &self.config.font_family,
                    ));
                    ctx.set_text_align(if run.column.is_some() {
                        TextAlign::Left
                    } else {
                        TextAlign::Center
                    });
                    let text_x = run.column.map_or(w * 0.5, |column| {
                        f64::from(fm_core::ErCardLayout::PADDING_X + card.column_offsets[column])
                    });
                    ctx.fill_text(&run.text, x + text_x, y + f64::from(run.center_y));
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                }
            } else if let Some((node, meta)) =
                ir_node.and_then(|n| n.requirement_meta.as_deref().map(|m| (n, m)))
            {
                ctx.set_fill_style(label_fill);
                ctx.set_text_baseline(TextBaseline::Middle);
                ctx.set_text_align(TextAlign::Center);
                let name = node
                    .label
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|l| l.text.as_str())
                    .unwrap_or(&node.id);
                let declared = ir.resolved_node_style(node_box.node_index);
                let metrics = fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
                    preset: fm_core::FontPreset::from_family(
                        declared_parts
                            .family
                            .as_deref()
                            .unwrap_or(&self.config.font_family),
                    ),
                    ..self.config.font_metrics().config().clone()
                });
                let card = fm_core::RequirementCardLayout::new(
                    meta,
                    name,
                    &metrics,
                    Some(node_box.bounds.width),
                    declared.get("font-size").map(String::as_str),
                )
                .fit_to_height(node_box.bounds.height);
                let top = y + ((h - f64::from(card.height)) * 0.5).max(0.0);
                if card.rows.iter().any(|row| {
                    !matches!(
                        row.kind,
                        fm_core::RequirementCardRowKind::Name
                            | fm_core::RequirementCardRowKind::Stereotype
                    )
                }) {
                    ctx.set_stroke_style(stroke.as_deref().unwrap_or(&self.config.node_stroke));
                    ctx.set_line_width(0.8);
                    ctx.begin_path();
                    ctx.move_to(x + 2.0, top + f64::from(card.header_height));
                    ctx.line_to(x + w - 2.0, top + f64::from(card.header_height));
                    ctx.stroke();
                    self.draw_calls += 1;
                }
                for row in card.rows {
                    let font = declared_parts.compose(
                        f64::from(row.font_size),
                        (row.kind == fm_core::RequirementCardRowKind::Name).then_some("bold"),
                        &self.config.font_family,
                    );
                    ctx.set_font(&if row.kind == fm_core::RequirementCardRowKind::Stereotype
                        && declared_parts.style.is_none()
                    {
                        format!("italic {font}")
                    } else {
                        font
                    });
                    ctx.fill_text(&row.text, x + w * 0.5, top + f64::from(row.center_y));
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                }
            } else if let Some((node, meta)) =
                ir_node.and_then(|n| n.c4_meta.as_deref().map(|m| (n, m)))
            {
                // Layout and SVG consume these same wrapped rows. Reconstructing a separate
                // cursor here previously made descriptions single, unbounded lines and dropped
                // metadata when its independently guessed height did not fit.
                ctx.set_fill_style(label_fill);
                ctx.set_text_baseline(TextBaseline::Middle);
                ctx.set_text_align(TextAlign::Center);
                let name = node
                    .label
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|l| l.text.as_str())
                    .unwrap_or(&node.id);
                let declared = ir.resolved_node_style(node_box.node_index);
                let metrics = fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
                    preset: fm_core::FontPreset::from_family(
                        declared_parts
                            .family
                            .as_deref()
                            .unwrap_or(&self.config.font_family),
                    ),
                    ..self.config.font_metrics().config().clone()
                });
                let card = fm_core::C4CardLayout::new(
                    meta,
                    name,
                    node.shape,
                    &metrics,
                    Some(w as f32),
                    declared.get("font-size").map(String::as_str),
                );
                let top = y + ((h - f64::from(card.height)) * 0.5).max(0.0);
                for row in card.rows {
                    ctx.set_font(&declared_parts.compose(
                        f64::from(row.font_size),
                        (row.kind == fm_core::C4CardRowKind::Name).then_some("600"),
                        &self.config.font_family,
                    ));
                    ctx.fill_text(&row.text, x + w / 2.0, top + f64::from(row.center_y));
                    self.draw_calls += 1;
                    *labels_drawn += 1;
                }
            } else {
                // Standard single-label rendering.
                // Layout and SVG use the same rule: state ornaments and junctions draw no text,
                // while gitGraph commits keep their labels even when they use those shapes.
                let sankey_label = fm_layout::sankey_node_label(ir, node_box.node_index);
                let node_metrics =
                    ir.effective_node_font_metrics(node_box.node_index, &label_metrics);
                let node_metrics = node_metrics.as_ref();
                let paragraph = ir_node.map(|node| ir.node_layout_text(node, node_metrics));
                let label_text = sankey_label
                    .as_deref()
                    .or(paragraph.as_deref())
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
                    let label_region =
                        shape.label_region(node_box.bounds.width, node_box.bounds.height);
                    let (label_cx, label_cy) =
                        label_region.map_or((x + w / 2.0, y + h / 2.0), |region| {
                            (
                                x + f64::from(region.x + region.width / 2.0),
                                y + f64::from(region.y + region.height / 2.0),
                            )
                        });
                    let (cx, align) = match gantt_label {
                        Some(entry) => (
                            f64::from(entry.x) + offset_x,
                            match entry.placement {
                                fm_layout::GanttLabelPlacement::OutsideRight => TextAlign::Left,
                                fm_layout::GanttLabelPlacement::OutsideLeft => TextAlign::Right,
                                fm_layout::GanttLabelPlacement::Inside => TextAlign::Center,
                            },
                        ),
                        None => (label_cx, TextAlign::Center),
                    };
                    let cy = label_cy;

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
                        let line_height = declared_parts.size.unwrap_or(self.config.font_size)
                            * f64::from(self.config.resolved_line_height());
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
        pie: &fm_layout::PieChartLayout,
        ir: &MermaidDiagramIr,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
        labels_drawn: &mut usize,
    ) {
        let cx = f64::from(pie.center.x) + offset_x;
        let cy = f64::from(pie.center.y) + offset_y;
        let radius = f64::from(pie.radius);
        if let Some(title) = &pie.title {
            *labels_drawn += self.draw_pie_text_block(title, ctx, offset_x, offset_y, true);
        }
        ctx.set_fill_style(&self.config.node_fill);
        ctx.fill_rect(
            f64::from(pie.legend.x) + offset_x,
            f64::from(pie.legend.y) + offset_y,
            f64::from(pie.legend.width),
            f64::from(pie.legend.height),
        );
        ctx.set_stroke_style(&self.config.node_stroke);
        ctx.set_line_width(1.0);
        ctx.stroke_rect(
            f64::from(pie.legend.x) + offset_x,
            f64::from(pie.legend.y) + offset_y,
            f64::from(pie.legend.width),
            f64::from(pie.legend.height),
        );
        self.draw_calls += 2;
        for (index, slice) in pie.slices.iter().enumerate() {
            let color = ir
                .meta
                .theme_overrides
                .theme_variables
                .get(&format!("pie{}", index % self.config.accents.len() + 1))
                .and_then(|paint| sanitize_canvas_paint(paint))
                .unwrap_or_else(|| self.config.accents[index % self.config.accents.len()].clone());
            if slice.sweep_angle > 0.0 {
                let angle = f64::from(slice.start_angle);
                let sweep = f64::from(slice.sweep_angle);
                ctx.begin_path();
                if slice.sweep_angle < std::f32::consts::TAU {
                    ctx.move_to(cx, cy);
                }
                ctx.arc(cx, cy, radius, angle, angle + sweep);
                ctx.close_path();
                ctx.set_fill_style(&color);
                ctx.fill();
                ctx.set_stroke_style(&self.config.node_fill);
                ctx.set_line_width(2.0);
                ctx.stroke();
                self.draw_calls += 1;
            }
            if let Some(leader) = slice.leader {
                ctx.begin_path();
                ctx.move_to(
                    f64::from(leader[0].x) + offset_x,
                    f64::from(leader[0].y) + offset_y,
                );
                for point in &leader[1..] {
                    ctx.line_to(f64::from(point.x) + offset_x, f64::from(point.y) + offset_y);
                }
                ctx.set_stroke_style(&self.config.label_color);
                ctx.set_line_width(1.0);
                ctx.set_global_alpha(0.45);
                ctx.stroke();
                ctx.set_global_alpha(1.0);
                self.draw_calls += 1;
            }
            *labels_drawn += self.draw_pie_text_block(&slice.label, ctx, offset_x, offset_y, false);
            let entry = &pie.entries[index];
            ctx.set_fill_style(&color);
            ctx.fill_rect(
                f64::from(entry.swatch.x) + offset_x,
                f64::from(entry.swatch.y) + offset_y,
                f64::from(entry.swatch.width),
                f64::from(entry.swatch.height),
            );
            self.draw_calls += 1;
            *labels_drawn += self.draw_pie_text_block(&entry.label, ctx, offset_x, offset_y, false);
        }
    }

    fn draw_pie_text_block<C: Canvas2dContext>(
        &mut self,
        block: &fm_layout::PieTextBlock,
        ctx: &mut C,
        offset_x: f64,
        offset_y: f64,
        emphasized: bool,
    ) -> usize {
        ctx.set_fill_style(&self.config.label_color);
        ctx.set_font(&format!(
            "{}{}px {}",
            if emphasized { "600 " } else { "" },
            block.font_size,
            self.config.font_family
        ));
        ctx.set_text_align(match block.align {
            IrTextAlign::Start => TextAlign::Left,
            IrTextAlign::Middle => TextAlign::Center,
            IrTextAlign::End => TextAlign::Right,
        });
        ctx.set_text_baseline(TextBaseline::Middle);
        for (text, point) in block.rows() {
            ctx.fill_text(
                text,
                f64::from(point.x) + offset_x,
                f64::from(point.y) + offset_y,
            );
            self.draw_calls += 1;
        }
        block.lines.len()
    }
}

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
            draw_circle_marker(ctx, x, y, 4.0, Some(node_fill), stroke_color);
            1
        }
        // The socket sits beyond the path endpoint, so the shaft meets its circumference rather
        // than crossing its hollow interior. SVG encodes the same offsets with refX 13/1.
        MarkerKind::Lollipop | MarkerKind::LollipopStart => {
            let offset = if marker == MarkerKind::LollipopStart {
                -6.0
            } else {
                6.0
            };
            draw_circle_marker(
                ctx,
                x + offset * angle.cos(),
                y + offset * angle.sin(),
                6.0,
                None,
                stroke_color,
            );
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
        MarkerKind::Arrow | MarkerKind::ThickArrow | MarkerKind::DottedArrow => {
            draw_arrowhead(ctx, x, y, angle, 10.0, stroke_color);
            1
        }
        MarkerKind::Open
        | MarkerKind::HalfArrowTop
        | MarkerKind::HalfArrowBottom
        | MarkerKind::StickArrowTop
        | MarkerKind::StickArrowBottom => {
            // Half arrows occupy only their authored side of the shaft. A stick or open arrow
            // remains an unfilled stroke; filling it changes the sequence-message semantics.
            let bottom = matches!(
                marker,
                MarkerKind::HalfArrowBottom | MarkerKind::StickArrowBottom
            );
            let filled = matches!(
                marker,
                MarkerKind::HalfArrowTop | MarkerKind::HalfArrowBottom
            );
            ctx.save();
            ctx.translate(x, y);
            ctx.rotate(angle);
            ctx.set_line_dash(&[]);
            ctx.set_stroke_style(stroke_color);
            ctx.set_fill_style(stroke_color);
            ctx.set_line_width(1.2);
            ctx.begin_path();
            ctx.move_to(-10.0, if bottom { 5.0 } else { -5.0 });
            ctx.line_to(0.0, 0.0);
            if marker == MarkerKind::Open {
                ctx.line_to(-10.0, 5.0);
            } else if filled {
                ctx.line_to(-10.0, 0.0);
                ctx.close_path();
            }
            if filled {
                ctx.fill();
            } else {
                ctx.stroke();
            }
            ctx.restore();
            1
        }
    }
}

/// Source arrow tips face against the path's forward tangent. Diamonds also extend behind
/// their tip and must turn out of the owning node. `TriangleOpenStart` turns in its primitive;
/// ER glyphs already encode which endpoint they belong to.
pub(crate) fn start_marker_angle(marker: MarkerKind, angle: f64) -> f64 {
    if matches!(
        marker,
        MarkerKind::Diamond
            | MarkerKind::DiamondOpen
            | MarkerKind::Arrow
            | MarkerKind::ThickArrow
            | MarkerKind::DottedArrow
            | MarkerKind::Open
            | MarkerKind::HalfArrowTop
            | MarkerKind::HalfArrowBottom
            | MarkerKind::StickArrowTop
            | MarkerKind::StickArrowBottom
    ) {
        let reversed = angle + std::f64::consts::PI;
        if reversed > std::f64::consts::PI {
            reversed - std::f64::consts::TAU
        } else {
            reversed
        }
    } else {
        angle
    }
}

fn path_marker_start_geometry(commands: &[PathCmd]) -> Option<(f64, f64, f64)> {
    let endpoint = fm_layout::edge_paths::path_endpoints(commands)?.start;
    Some((
        f64::from(endpoint.point.x),
        f64::from(endpoint.point.y),
        f64::from(endpoint.tangent.y).atan2(f64::from(endpoint.tangent.x)),
    ))
}

fn path_marker_end_geometry(commands: &[PathCmd]) -> Option<(f64, f64, f64)> {
    let endpoint = fm_layout::edge_paths::path_endpoints(commands)?.end;
    Some((
        f64::from(endpoint.point.x),
        f64::from(endpoint.point.y),
        f64::from(endpoint.tangent.y).atan2(f64::from(endpoint.tangent.x)),
    ))
}

#[cfg(test)]
fn angle_between(start: (f64, f64), end: (f64, f64)) -> f64 {
    (end.1 - start.1).atan2(end.0 - start.0)
}

/// Resolve the authored quadrant marker palette once for the Canvas and GPU consumers.
pub(crate) fn quadrant_point_colors(
    point: Option<&fm_core::IrQuadrantPoint>,
    index: usize,
    config: &CanvasRenderConfig,
) -> (String, String) {
    let style = point.and_then(|point| point.style.as_ref());
    let fill = style
        .and_then(|style| style.color.as_deref())
        .and_then(sanitize_canvas_paint)
        .unwrap_or_else(|| config.accents[index % config.accents.len()].clone());
    let stroke = style
        .and_then(|style| style.stroke_color.as_deref())
        .and_then(sanitize_canvas_paint)
        .or_else(|| config.background.as_deref().and_then(sanitize_canvas_paint))
        .unwrap_or_else(|| "#ffffff".to_owned());
    (fill, stroke)
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
pub(crate) fn band_has_cluster_background(
    diagram_type: DiagramType,
    layout: &DiagramLayout,
    band: &fm_layout::LayoutBand,
) -> bool {
    diagram_type == DiagramType::Timeline
        && band.kind == fm_layout::LayoutBandKind::Section
        && layout
            .clusters
            .iter()
            .any(|cluster| cluster.bounds == band.bounds)
}

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
    fm_layout::edge_labels::edge_label_style(ir, edge_index)
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

/// Authored strokes win over theme variables and category colors. Sharing this resolution
/// keeps a raster preview and a GPU plan faithful to the same Sankey source category.
pub(crate) fn resolve_edge_style_with_defaults(
    ir: &MermaidDiagramIr,
    edge_index: usize,
    config: &CanvasRenderConfig,
) -> (Option<String>, Option<f64>) {
    let (stroke, width) = resolve_edge_style(ir, edge_index);
    let stroke = stroke
        .or_else(|| {
            ir.meta
                .theme_overrides
                .theme_variables
                .get("lineColor")
                .and_then(|value| sanitize_canvas_paint(value))
        })
        .or_else(|| fm_core::sankey_edge_color(ir, edge_index, &config.accents).map(str::to_owned));
    (stroke, width)
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
/// Four channels, merged in the order mermaid applies them — later wins:
///   1. the implicit `classDef default`, when defined
///   2. `classDef` definitions named by the node's `classes`
///   3. `style <id> ...` directives targeting this node (`ir.style_refs`)
///   4. the node's own `inline_style`
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
    ir.resolved_node_style(node_index)
}

pub(crate) fn resolve_node_colors(
    ir: &MermaidDiagramIr,
    node_index: usize,
) -> (Option<String>, Option<String>) {
    let merged = merged_node_style(ir, node_index);
    // `fill-opacity` fades the FILL alone — the outline and label keep full strength, which is
    // what separates it from `opacity`. A canvas has one `fillStyle` per shape, so the fade has to
    // travel inside the colour itself; folding it into an `rgba()` here gives the Canvas2D pass and
    // the GPU plan (which reads this same function) one answer. Overlapping translucent shapes —
    // venn-beta's set circles are the reason this exists — would otherwise paint opaque and bury
    // each other.
    let fill = merged.get("fill").map(|fill| {
        parse_opacity(merged.get("fill-opacity").map(String::as_str))
            .and_then(|opacity| hex_fill_with_opacity(fill, opacity))
            .unwrap_or_else(|| fill.clone())
    });
    (fill, merged.get("stroke").cloned())
}

/// Semantic defaults are fallbacks for each paint channel, never author declarations.
/// A fill-only class retains its role border, and a stroke-only class retains its role fill.
pub(crate) fn resolve_node_colors_with_defaults(
    ir: &MermaidDiagramIr,
    node_index: usize,
    config: &CanvasRenderConfig,
) -> (Option<String>, Option<String>) {
    let (fill, stroke) = resolve_node_colors(ir, node_index);
    if let Some(meta) = ir
        .nodes
        .get(node_index)
        .and_then(|node| node.requirement_meta.as_deref())
    {
        return (
            fill.or_else(|| fm_core::requirement_risk_fill(meta).map(str::to_owned)),
            stroke,
        );
    }
    if ir.diagram_type == DiagramType::Journey {
        return (
            fill.or_else(|| fm_layout::journey_score_fill(ir, node_index).map(str::to_owned)),
            stroke,
        );
    }
    if ir.diagram_type == DiagramType::Sankey {
        let role = fm_core::sankey_node_style(
            ir,
            node_index,
            &config.node_fill,
            &config.node_stroke,
            &config.label_color,
            &config.accents,
        );
        let variables = &ir.meta.theme_overrides.theme_variables;
        let theme_fill = variables
            .get("primaryColor")
            .and_then(|value| sanitize_canvas_paint(value));
        let theme_stroke = variables
            .get("primaryBorderColor")
            .and_then(|value| sanitize_canvas_paint(value));
        return (
            fill.or(theme_fill)
                .or_else(|| role.as_ref().map(|role| role.fill.clone())),
            stroke
                .or(theme_stroke)
                .or_else(|| role.map(|role| role.stroke)),
        );
    }
    if config.semantic_colors
        && ir.diagram_type == DiagramType::Flowchart
        && ir.meta.theme_overrides.theme.as_deref().is_none_or(|name| {
            name.eq_ignore_ascii_case("default") || name.eq_ignore_ascii_case("dark")
        })
        && let Some(node) = ir.nodes.get(node_index)
        && let Some(role) = fm_core::semantic_shape_style(
            ir.diagram_type,
            node.shape,
            &config.node_fill,
            &config.node_stroke,
            &config.label_color,
            &config.accents,
        )
    {
        let variables = &ir.meta.theme_overrides.theme_variables;
        let theme_fill = variables
            .get("primaryColor")
            .and_then(|value| sanitize_canvas_paint(value));
        let theme_stroke = variables
            .get("primaryBorderColor")
            .and_then(|value| sanitize_canvas_paint(value));
        return (
            fill.or(theme_fill).or(Some(role.fill)),
            stroke.or(theme_stroke).or(Some(role.stroke)),
        );
    }
    let Some(role) = fm_core::mindmap::node_role(ir, node_index) else {
        let solid_marker = ir.nodes.get(node_index).is_some_and(|node| {
            matches!(
                node.shape,
                NodeShape::FilledCircle | NodeShape::HorizontalBar
            )
        });
        return (
            fill.or_else(|| solid_marker.then(|| config.edge_stroke.clone())),
            stroke,
        );
    };
    let accent = &config.accents[role.branch];
    (
        fill.or_else(|| {
            Some(fm_core::mindmap::branch_fill(
                &config.node_fill,
                accent,
                role.is_root,
            ))
        }),
        stroke.or_else(|| {
            Some(if role.borderless {
                // Canvas rejects CSS `none`; transparent paint removes the border reliably.
                String::from("transparent")
            } else {
                accent.clone()
            })
        }),
    )
}

/// A `#rgb`/`#rrggbb` (with optional alpha) colour as `rgba()` with `opacity` multiplied into its
/// alpha; `None` for anything that is not a hex colour, so the caller keeps the declaration as is.
fn hex_fill_with_opacity(fill: &str, opacity: f64) -> Option<String> {
    let hex = fill.trim().strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |text: &str| u8::from_str_radix(text, 16).ok();
    let doubled = |c: char| channel(&format!("{c}{c}"));
    let chars: Vec<char> = hex.chars().collect();
    let (r, g, b, a) = match chars.len() {
        3 | 4 => (
            doubled(chars[0])?,
            doubled(chars[1])?,
            doubled(chars[2])?,
            chars.get(3).map_or(Some(255), |c| doubled(*c))?,
        ),
        6 | 8 => (
            channel(&hex[0..2])?,
            channel(&hex[2..4])?,
            channel(&hex[4..6])?,
            hex.get(6..8).map_or(Some(255), channel)?,
        ),
        _ => return None,
    };
    // Thousandths as an INTEGER: formatting the f64 would link core's float formatter into the
    // WASM bundle for this one string.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let milli = (f64::from(a) / 255.0 * opacity.clamp(0.0, 1.0) * 1000.0).round() as u32;
    let alpha = match milli {
        0 => "0".to_string(),
        1000.. => "1".to_string(),
        _ => format!("0.{}", format!("{milli:03}").trim_end_matches('0')),
    };
    Some(format!("rgba({r}, {g}, {b}, {alpha})"))
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
    resolve_edge_label_font_size(ir, edge_index)
        .map(|size| format!("{size}px {}", config.font_family))
}

pub(crate) fn resolve_edge_label_font_size(
    ir: &MermaidDiagramIr,
    edge_index: usize,
) -> Option<f64> {
    parse_declared_font_size(
        merged_edge_style(ir, edge_index)
            .get("font-size")
            .map(String::as_str),
    )
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
            .and_then(sanitize_font_weight)
            .or_else(|| {
                fm_core::mindmap::node_role(ir, node_index)
                    .filter(|role| role.is_root)
                    .map(|_| "600")
            }),
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
        ArrowType::ThickArrow
        | ArrowType::ThickLine
        | ArrowType::DoubleThickArrow
        | ArrowType::ThickCircle
        | ArrowType::ThickCross
        | ArrowType::ThickCircleBoth
        | ArrowType::ThickCrossBoth => (2.5, &[]),
        ArrowType::DottedArrow
        | ArrowType::DottedLine
        | ArrowType::DottedOpenArrow
        | ArrowType::DottedCross
        | ArrowType::DoubleDottedArrow
        | ArrowType::DottedCircle
        | ArrowType::DottedCircleBoth
        | ArrowType::DottedCrossBoth
        | ArrowType::HalfArrowTopDotted
        | ArrowType::HalfArrowBottomDotted
        | ArrowType::HalfArrowTopReverseDotted
        | ArrowType::HalfArrowBottomReverseDotted
        | ArrowType::StickArrowTopDotted
        | ArrowType::StickArrowBottomDotted
        | ArrowType::StickArrowTopReverseDotted
        | ArrowType::StickArrowBottomReverseDotted => (1.5, &LEGACY_DOTTED_EDGE_DASH),
        _ => (default_width, &[]),
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

fn standard_node_font(config: &CanvasRenderConfig) -> String {
    format!("{}px {}", config.font_size, config.font_family)
}

fn secondary_label_font_css(config: &CanvasRenderConfig) -> String {
    format!("{}px {}", config.font_size * 0.85, config.font_family)
}

fn sequence_fragment_font_css(config: &CanvasRenderConfig) -> String {
    format!("bold {}px {}", config.font_size * 0.8, config.font_family)
}

fn generic_canvas_diagram_title(ir: &MermaidDiagramIr) -> Option<&str> {
    // XY charts draw their title in the plot's reserved margin and return before this
    // screen-space overlay. Other families keep the shared title fallback.
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
            ArrowType::ThickArrow
            | ArrowType::ThickLine
            | ArrowType::DoubleThickArrow
            | ArrowType::ThickCircle
            | ArrowType::ThickCross
            | ArrowType::ThickCircleBoth
            | ArrowType::ThickCrossBoth => (2.5, None),
            ArrowType::DottedArrow
            | ArrowType::DottedLine
            | ArrowType::DottedOpenArrow
            | ArrowType::DottedCross
            | ArrowType::DoubleDottedArrow
            | ArrowType::DottedCircle
            | ArrowType::DottedCircleBoth
            | ArrowType::DottedCrossBoth
            | ArrowType::HalfArrowTopDotted
            | ArrowType::HalfArrowBottomDotted
            | ArrowType::HalfArrowTopReverseDotted
            | ArrowType::HalfArrowBottomReverseDotted
            | ArrowType::StickArrowTopDotted
            | ArrowType::StickArrowBottomDotted
            | ArrowType::StickArrowTopReverseDotted
            | ArrowType::StickArrowBottomReverseDotted => (1.5, Some(vec![5.0, 5.0])),
            _ => (default_width, None),
        }
    }

    #[test]
    fn fill_opacity_fades_only_the_fill_colour() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "a".to_string(),
            inline_style: Some(Box::new(fm_core::IrInlineStyle::from_pairs([
                ("fill".to_string(), "#4f6bed".to_string()),
                ("fill-opacity".to_string(), "0.1".to_string()),
                ("stroke".to_string(), "#4f6bed".to_string()),
            ]))),
            ..IrNode::default()
        });
        let (fill, stroke) = resolve_node_colors(&ir, 0);
        assert_eq!(fill.as_deref(), Some("rgba(79, 107, 237, 0.1)"));
        assert_eq!(
            stroke.as_deref(),
            Some("#4f6bed"),
            "the outline keeps full strength"
        );
        // Short hex, an existing alpha channel, and a non-hex colour left untouched.
        assert_eq!(
            hex_fill_with_opacity("#f00", 0.5).as_deref(),
            Some("rgba(255, 0, 0, 0.5)")
        );
        assert_eq!(
            hex_fill_with_opacity("#ff000080", 0.5).as_deref(),
            Some("rgba(255, 0, 0, 0.251)")
        );
        assert_eq!(hex_fill_with_opacity("red", 0.5), None);
        assert_eq!(hex_fill_with_opacity("#ggg", 0.5), None);
    }

    #[test]
    fn canvas_half_and_open_markers_keep_their_authored_geometry() {
        for (marker, filled, top, bottom) in [
            (MarkerKind::HalfArrowTop, true, true, false),
            (MarkerKind::HalfArrowBottom, true, false, true),
            (MarkerKind::StickArrowTop, false, true, false),
            (MarkerKind::StickArrowBottom, false, false, true),
            (MarkerKind::Open, false, true, true),
        ] {
            let operations = marker_operations(marker);
            let fills = operations
                .iter()
                .filter(|op| matches!(op, DrawOperation::Fill))
                .count();
            assert_eq!(
                fills,
                usize::from(filled),
                "{marker:?} lost its fill semantics"
            );
            let strokes = operations
                .iter()
                .filter(|op| matches!(op, DrawOperation::Stroke))
                .count();
            assert_eq!(
                strokes,
                usize::from(!filled),
                "{marker:?} lost its outline semantics"
            );
            let vertices: Vec<_> = operations
                .iter()
                .filter_map(|op| match op {
                    DrawOperation::MoveTo(x, y) | DrawOperation::LineTo(x, y) => Some((*x, *y)),
                    _ => None,
                })
                .collect();
            assert!(
                vertices.contains(&(0.0, 0.0)),
                "the tip must meet the connector"
            );
            assert_eq!(vertices.iter().any(|(_, y)| *y < 0.0), top, "{marker:?}");
            assert_eq!(vertices.iter().any(|(_, y)| *y > 0.0), bottom, "{marker:?}");
        }
    }

    #[test]
    fn canvas_parsed_edge_markers_preserve_plain_lines_both_ends_and_stroke_weight() {
        for (link, fills, arcs, dashed, thick) in [
            ("---", 0, 0, false, false),
            ("-.-", 0, 0, true, false),
            ("===", 0, 0, false, true),
            ("<-->", 2, 0, false, false),
            ("<-.->", 2, 0, true, false),
            ("<==>", 2, 0, false, true),
            ("o--o", 2, 2, false, false),
            ("o-.-o", 2, 2, true, false),
            ("o==o", 2, 2, false, true),
            ("x--x", 0, 0, false, false),
        ] {
            let source = format!("flowchart LR\nA {link} B");
            let ir = fm_parser::parse(&source).ir;
            assert_eq!(ir.edges.len(), 1, "{source}");
            let mut layout = layout_diagram(&ir);
            layout.edges[0].points = [(10.0, 20.0), (110.0, 20.0)]
                .into_iter()
                .map(|(x, y)| fm_layout::LayoutPoint { x, y })
                .collect();
            let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
            let mut ctx = MockCanvas2dContext::new(150.0, 60.0);
            renderer.draw_edges(&layout, &ir, &mut ctx, 0.0, 0.0, &mut 0);
            let operations = ctx.operations();
            assert_eq!(
                operations
                    .iter()
                    .filter(|op| matches!(op, DrawOperation::Fill))
                    .count(),
                fills,
                "{link}"
            );
            assert_eq!(
                operations
                    .iter()
                    .filter(|op| matches!(op, DrawOperation::Arc(..)))
                    .count(),
                arcs,
                "{link}"
            );
            assert_eq!(
                operations.iter().any(
                    |op| matches!(op, DrawOperation::SetLineDash(pattern) if !pattern.is_empty())
                ),
                dashed,
                "{link}"
            );
            assert_eq!(operations.iter().any(|op| matches!(op, DrawOperation::SetLineWidth(width) if (*width - 2.5).abs() < 1e-6)), thick, "{link}");
            if arcs > 0 {
                let centers: Vec<_> = operations
                    .iter()
                    .filter_map(|op| match op {
                        DrawOperation::Arc(x, y, ..) => Some((*x, *y)),
                        _ => None,
                    })
                    .collect();
                assert_eq!(centers, [(10.0, 20.0), (110.0, 20.0)], "{link}");
            }
            if link == "x--x" {
                assert_eq!(
                    operations
                        .iter()
                        .filter(|op| matches!(op, DrawOperation::Stroke))
                        .count(),
                    5,
                    "both crosses need two arms in addition to the connector"
                );
            }
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

    fn first_stroked_path(operations: &[DrawOperation]) -> Vec<PathCmd> {
        operations
            .iter()
            .skip_while(|operation| !matches!(operation, DrawOperation::BeginPath))
            .take_while(|operation| !matches!(operation, DrawOperation::Stroke))
            .filter_map(|operation| match *operation {
                DrawOperation::MoveTo(x, y) => Some(PathCmd::MoveTo {
                    x: x as f32,
                    y: y as f32,
                }),
                DrawOperation::LineTo(x, y) => Some(PathCmd::LineTo {
                    x: x as f32,
                    y: y as f32,
                }),
                DrawOperation::QuadraticCurveTo(cx, cy, x, y) => Some(PathCmd::QuadTo {
                    cx: cx as f32,
                    cy: cy as f32,
                    x: x as f32,
                    y: y as f32,
                }),
                DrawOperation::BezierCurveTo(c1x, c1y, c2x, c2y, x, y) => Some(PathCmd::CubicTo {
                    c1x: c1x as f32,
                    c1y: c1y as f32,
                    c2x: c2x as f32,
                    c2y: c2y as f32,
                    x: x as f32,
                    y: y as f32,
                }),
                _ => None,
            })
            .collect()
    }

    fn scene_edge_path(group: &RenderGroup, edge_index: usize) -> Option<&[PathCmd]> {
        group.children.iter().find_map(|item| match item {
            RenderItem::Group(group) => scene_edge_path(group, edge_index),
            RenderItem::Path(path) if path.source == RenderSource::Edge(edge_index) => {
                Some(path.commands.as_slice())
            }
            _ => None,
        })
    }

    #[test]
    fn graph_connectors_draw_real_rounded_elbows_matching_the_shared_scene() {
        let expected = vec![
            PathCmd::MoveTo { x: 0.0, y: 0.0 },
            PathCmd::LineTo { x: 34.0, y: 0.0 },
            PathCmd::QuadTo {
                cx: 40.0,
                cy: 0.0,
                x: 40.0,
                y: 6.0,
            },
            PathCmd::LineTo { x: 40.0, y: 74.0 },
            PathCmd::QuadTo {
                cx: 40.0,
                cy: 80.0,
                x: 46.0,
                y: 80.0,
            },
            PathCmd::LineTo { x: 120.0, y: 80.0 },
        ];
        for source in [
            "flowchart LR\nA --> B",
            "classDiagram\nA --> B",
            "stateDiagram-v2\nA --> B",
            "erDiagram\nA ||--o{ B : owns",
        ] {
            let ir = fm_parser::parse(source).ir;
            let mut layout = layout_diagram(&ir);
            assert_eq!(
                layout.edges.len(),
                1,
                "fixture must contain one real parsed edge"
            );
            layout.edges[0].points = [(0.0, 0.0), (40.0, 0.0), (40.0, 80.0), (120.0, 80.0)]
                .into_iter()
                .map(|(x, y)| fm_layout::LayoutPoint { x, y })
                .collect();
            let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
            let mut ctx = MockCanvas2dContext::new(300.0, 200.0);
            renderer.draw_edges(&layout, &ir, &mut ctx, 0.0, 0.0, &mut 0);
            let actual = first_stroked_path(ctx.operations());
            assert_eq!(actual, expected, "{source}");
            let scene = build_render_scene(&ir, &layout);
            assert_eq!(
                scene_edge_path(&scene.root, 0).unwrap(),
                actual,
                "scene diverged for {source}"
            );
        }
    }

    #[test]
    fn per_edge_curve_hints_preserve_native_scene_and_gpu_geometry() {
        for (source, smoothed) in [
            ("flowchart LR\nA e1@--> B\ne1@{ curve: linear }", false),
            ("flowchart LR\nA e1@--> B\ne1@{ curve: stepAfter }", false),
            (
                "flowchart LR\nA e1@--> B\nlinkStyle default interpolate basis\ne1@{ curve: step }",
                false,
            ),
            (
                "flowchart LR\nA e1@--> B\nlinkStyle default interpolate linear\ne1@{ curve: basis }",
                true,
            ),
            (
                "flowchart LR\nA --> B\nlinkStyle default interpolate linear",
                false,
            ),
            ("flowchart LR\nA --> B", true),
        ] {
            let ir = fm_parser::parse(source).ir;
            let mut layout = layout_diagram(&ir);
            layout.edges[0].points = [(0.0, 0.0), (60.0, 0.0), (60.0, 80.0), (120.0, 80.0)]
                .into_iter()
                .map(|(x, y)| fm_layout::LayoutPoint { x, y })
                .collect();
            let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
            let mut ctx = MockCanvas2dContext::new(300.0, 200.0);
            renderer.draw_edges(&layout, &ir, &mut ctx, 0.0, 0.0, &mut 0);
            let native = first_stroked_path(ctx.operations());
            assert_eq!(
                native.iter().any(|command| matches!(
                    command,
                    PathCmd::QuadTo { .. } | PathCmd::CubicTo { .. }
                )),
                smoothed,
                "{source}: {native:?}"
            );
            let scene = build_render_scene(&ir, &layout);
            assert_eq!(scene_edge_path(&scene.root, 0).unwrap(), native, "{source}");

            let plan = crate::GpuRenderPlan::from_layout(&ir, &layout, 1.5);
            let segments: Vec<_> = plan
                .edge_segments
                .iter()
                .filter(|segment| segment.edge_index == 0)
                .collect();
            assert_eq!(segments.first().unwrap().from, [0.0, 0.0]);
            assert_eq!(segments.last().unwrap().to, [120.0, 80.0]);
            if smoothed {
                assert!(
                    segments.len() > 3,
                    "GPU flattened away the requested bends: {source}"
                );
            } else {
                assert_eq!(segments.len(), 3, "GPU resmoothed a linear edge: {source}");
                for (segment, pair) in segments.iter().zip(layout.edges[0].points.windows(2)) {
                    assert_eq!(segment.from, [pair[0].x, pair[0].y]);
                    assert_eq!(segment.to, [pair[1].x, pair[1].y]);
                }
            }
        }
    }

    #[test]
    fn connector_markers_follow_collapsed_geometry_and_skip_zero_length_routes() {
        let ir = fm_parser::parse("flowchart LR\nA <--> B").ir;
        let mut layout = layout_diagram(&ir);
        layout.edges[0].points = [
            (0.0, 0.0),
            (0.0, 0.0),
            (40.0, 0.0),
            (40.0, 1.0),
            (120.0, 1.0),
            (120.0, 1.0),
        ]
        .into_iter()
        .map(|(x, y)| fm_layout::LayoutPoint { x, y })
        .collect();
        let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
        let mut ctx = MockCanvas2dContext::new(300.0, 200.0);
        renderer.draw_edges(&layout, &ir, &mut ctx, 7.0, -3.0, &mut 0);
        assert_eq!(
            first_stroked_path(ctx.operations()),
            vec![
                PathCmd::MoveTo { x: 7.0, y: -3.0 },
                PathCmd::LineTo { x: 127.0, y: -2.0 },
            ]
        );
        let rotations: Vec<_> = ctx
            .operations()
            .iter()
            .filter_map(|operation| {
                if let DrawOperation::Rotate(angle) = operation {
                    Some(*angle)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(rotations.len(), 2);
        for expected in [1.0_f64.atan2(120.0), (-1.0_f64).atan2(-120.0)] {
            assert!(
                rotations
                    .iter()
                    .any(|angle| (*angle - expected).abs() < 1e-10),
                "marker ignored the drawn tangent: {rotations:?}"
            );
        }

        let scene = build_render_scene(&ir, &layout);
        ctx.clear();
        renderer.render_scene(&scene, &mut ctx);
        let scene_rotations: Vec<_> = ctx
            .operations()
            .iter()
            .filter_map(|operation| match operation {
                DrawOperation::Rotate(angle) => Some(*angle),
                _ => None,
            })
            .collect();
        assert_eq!(scene_rotations.len(), rotations.len());
        for angle in &rotations {
            assert!(
                scene_rotations.iter().any(|scene_angle| {
                    (scene_angle - angle).sin().abs() < 1e-10
                        && (scene_angle - angle).cos() > 0.99999
                }),
                "scene arrow direction differs from Canvas: {scene_rotations:?}"
            );
        }

        layout.edges[0].points = [fm_layout::LayoutPoint { x: 5.0, y: 9.0 }; 4]
            .into_iter()
            .collect();
        ctx.clear();
        renderer.draw_edges(&layout, &ir, &mut ctx, 0.0, 0.0, &mut 0);
        assert!(
            !ctx.operations()
                .iter()
                .any(|operation| matches!(operation, DrawOperation::Rotate(_))),
            "a point has no arrow direction"
        );
        let scene = build_render_scene(&ir, &layout);
        ctx.clear();
        renderer.render_scene(&scene, &mut ctx);
        assert!(
            !ctx.operations()
                .iter()
                .any(|operation| matches!(operation, DrawOperation::Rotate(_))),
            "a scene must not invent an arrow direction for a point"
        );
    }

    #[test]
    fn curved_connector_scene_markers_ignore_repeated_terminal_points() {
        let ir = fm_parser::parse("flowchart LR\nA <--> B").ir;
        let mut layout = layout_diagram(&ir);
        layout.edges[0].points = [
            (0.0, 0.0),
            (0.0, 0.0),
            (40.0, 80.0),
            (120.0, 80.0),
            (120.0, 80.0),
        ]
        .into_iter()
        .map(|(x, y)| fm_layout::LayoutPoint { x, y })
        .collect();
        let scene = build_render_scene(&ir, &layout);
        assert!(
            scene_edge_path(&scene.root, 0)
                .unwrap()
                .iter()
                .any(|command| matches!(command, PathCmd::CubicTo { .. }))
        );
        for use_scene in [false, true] {
            let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
            let mut ctx = MockCanvas2dContext::new(300.0, 200.0);
            if use_scene {
                renderer.render_scene(&scene, &mut ctx);
            } else {
                renderer.draw_edges(&layout, &ir, &mut ctx, 0.0, 0.0, &mut 0);
            }
            let rotations: Vec<_> = ctx
                .operations()
                .iter()
                .filter_map(|operation| match operation {
                    DrawOperation::Rotate(angle) => Some(*angle),
                    _ => None,
                })
                .collect();
            assert_eq!(rotations.len(), 2);
            for expected in [(-2.0_f64).atan2(-1.0), 0.0] {
                assert!(
                    rotations.iter().any(|angle| {
                        (angle - expected).sin().abs() < 1e-10 && (angle - expected).cos() > 0.99999
                    }),
                    "scene={use_scene} used a degenerate segment's direction: {rotations:?}"
                );
            }
        }
    }

    #[test]
    fn sequence_self_message_keeps_its_rectangular_route() {
        let ir = fm_parser::parse("sequenceDiagram\nA->>A: retry").ir;
        let layout = layout_diagram(&ir);
        assert!(
            layout.edges[0].points.len() > 2,
            "fixture must exercise a self-message bend"
        );
        let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
        let mut ctx = MockCanvas2dContext::new(300.0, 200.0);
        renderer.draw_edges(&layout, &ir, &mut ctx, 0.0, 0.0, &mut 0);
        let commands = first_stroked_path(ctx.operations());
        assert_eq!(commands.len(), layout.edges[0].points.len());
        assert!(
            commands[1..]
                .iter()
                .all(|command| matches!(command, PathCmd::LineTo { .. }))
        );
        for (command, point) in commands.iter().zip(&layout.edges[0].points) {
            let (PathCmd::MoveTo { x, y } | PathCmd::LineTo { x, y }) = command else {
                unreachable!()
            };
            assert_eq!((*x, *y), (point.x, point.y));
        }
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

        // A start diamond is turned to face back along the path (the edge runs +x from the
        // owner), so its body lies on the edge rather than inside the owning node.
        for arrow in [ArrowType::Aggregation, ArrowType::Composition] {
            let operations = legacy_uml_edge_operations(arrow);
            assert!(
                operations.iter().any(|operation| matches!(operation,
                    DrawOperation::Rotate(angle) if (*angle - std::f64::consts::PI).abs() < 0.001)),
                "{arrow:?}: {operations:?}"
            );
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
        assert!(config.line_height >= 1.0);
        assert!(config.padding > 0.0);
    }

    #[test]
    fn default_canvas_palette_and_typography_match_svg() {
        let canvas = CanvasRenderConfig::default();
        let svg = fm_render_svg::SvgRenderConfig::default();
        let colors = fm_render_svg::ThemeColors::default();
        assert_eq!(canvas.font_family, svg.font_family);
        assert_eq!(canvas.font_size, f64::from(svg.font_size));
        assert_eq!(canvas.line_height, f64::from(svg.line_height));
        assert_eq!(canvas.node_fill, colors.node_fill);
        assert_eq!(canvas.node_stroke, colors.node_stroke);
        assert_eq!(canvas.edge_stroke, colors.edge);
        assert_eq!(canvas.cluster_fill, colors.cluster_fill);
        assert_eq!(canvas.cluster_stroke, colors.cluster_stroke);
        assert_eq!(canvas.label_color, colors.text);
        assert_eq!(canvas.accents, colors.accents);
        assert_eq!(
            canvas.font_metrics().estimate_dimensions("First\nSecond"),
            svg.font_metrics().estimate_dimensions("First\nSecond")
        );
    }

    #[test]
    fn authored_palette_reaches_canvas_without_overriding_styles_or_leaking_between_frames() {
        let ir = fm_parser::parse(
            "%%{init: {'themeVariables': {'background':'#101820','primaryColor':'#dbeafe',\
             'primaryBorderColor':'#1d4ed8','primaryTextColor':'#0f172a','lineColor':'#a855f7',\
             'clusterBkg':'#e0e7ff','clusterBorder':'#818cf8'}}}%%\n\
             flowchart LR\n subgraph group[Cluster]\n A[Palette] -->|Link| B[Custom]\n end\n\
             style B fill:#fee2e2,color:#991b1b\n linkStyle 0 stroke:#22c55e,color:#065f46\n",
        )
        .ir;
        assert_eq!(ir.nodes.len(), 2);
        assert_eq!(ir.meta.theme_overrides.theme_variables.len(), 7);
        let config = CanvasRenderConfig::default();
        let mut renderer = Canvas2dRenderer::new(config.clone());
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        renderer.render(&layout_diagram(&ir), &ir, &mut ctx);
        let (fills, text) = record_gantt_canvas_paint(ctx.operations());
        assert_eq!(fills[0].bounds, [0.0, 0.0, 800.0, 600.0]);
        assert_eq!(fills[0].paint.fill, "#101820");
        for color in ["#dbeafe", "#fee2e2", "#e0e7ff"] {
            assert!(
                fills.iter().any(|fill| fill.paint.fill == color),
                "missing {color}"
            );
        }
        for (label, color) in [
            ("Palette", "#0f172a"),
            ("Custom", "#991b1b"),
            ("Link", "#065f46"),
        ] {
            assert_eq!(
                text.iter()
                    .find(|run| run.text == label)
                    .unwrap()
                    .paint
                    .fill,
                color
            );
        }
        assert!(ctx.operations().iter().any(|operation| {
            matches!(operation, DrawOperation::SetStrokeStyle(color) if color == "#22c55e")
        }));

        let plain = fm_parser::parse("flowchart LR\n A[Plain] --> B[Next]").ir;
        let layout = layout_diagram(&plain);
        let mut reused = MockCanvas2dContext::new(800.0, 600.0);
        renderer.render(&layout, &plain, &mut reused);
        let mut fresh = MockCanvas2dContext::new(800.0, 600.0);
        Canvas2dRenderer::new(config).render(&layout, &plain, &mut fresh);
        assert_eq!(
            reused.operations(),
            fresh.operations(),
            "source palette leaked to another diagram"
        );
    }

    #[test]
    fn canvas_background_is_optional_safe_and_isolated_from_scene_paint() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let scene = build_render_scene(&ir, &layout_diagram(&ir));
        for background in [
            None,
            Some("#0f172a"),
            Some("transparent"),
            Some("url(https://example.com/image.svg)"),
        ] {
            let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
            let config = CanvasRenderConfig {
                background: background.map(str::to_owned),
                ..Default::default()
            };
            let result = Canvas2dRenderer::new(config).render_scene(&scene, &mut ctx);
            let (fills, _) = record_gantt_canvas_paint(ctx.operations());
            let expected = background.filter(|paint| !paint.starts_with("url("));
            assert_eq!(fills.len(), usize::from(expected.is_some()));
            // The empty scene still applies its root clip in addition to clearing the canvas.
            assert_eq!(result.draw_calls, 2 + usize::from(expected.is_some()));
            if let Some(color) = expected {
                assert_eq!(fills[0].bounds, [0.0, 0.0, 800.0, 600.0]);
                assert_eq!(fills[0].paint.fill, color);
                assert_eq!(fills[0].paint.alpha, 1.0);
                assert!(ctx.operations().windows(5).any(|ops| matches!(
                    ops,
                    [
                        DrawOperation::ResetTransform,
                        DrawOperation::SetGlobalAlpha(1.0),
                        DrawOperation::SetFillStyle(_),
                        DrawOperation::FillRect(..),
                        DrawOperation::Restore
                    ]
                )));
            }
        }
    }

    #[test]
    fn canvas_theme_variables_match_svg_palette_and_reject_resource_paints() {
        let variables = std::collections::BTreeMap::from([
            ("background".to_owned(), "#112233".to_owned()),
            ("primaryColor".to_owned(), "#aabbcc".to_owned()),
            ("primaryBorderColor".to_owned(), "#334455".to_owned()),
            ("primaryTextColor".to_owned(), "#010203".to_owned()),
            ("textColor".to_owned(), "#ffffff".to_owned()),
            ("lineColor".to_owned(), "#445566".to_owned()),
            ("clusterBkg".to_owned(), "#ddeeff".to_owned()),
            ("clusterBorder".to_owned(), "#667788".to_owned()),
            ("pie1".to_owned(), "#123456".to_owned()),
            (
                "pie2".to_owned(),
                "url(https://example.com/image.svg)".to_owned(),
            ),
        ]);
        let mut canvas = CanvasRenderConfig::default();
        canvas.apply_theme_variables(&variables);
        let mut svg = fm_render_svg::ThemeColors::default();
        svg.apply_overrides(&variables);
        assert_eq!(canvas.background.as_deref(), Some(svg.background.as_str()));
        assert_eq!(canvas.node_fill, svg.node_fill);
        assert_eq!(canvas.node_stroke, svg.node_stroke);
        assert_eq!(canvas.label_color, svg.text);
        assert_eq!(canvas.edge_stroke, svg.edge);
        assert_eq!(canvas.cluster_fill, svg.cluster_fill);
        assert_eq!(canvas.cluster_stroke, svg.cluster_stroke);
        assert_eq!(canvas.accents, svg.accents);
    }

    #[test]
    fn canvas_cards_use_configured_corners_without_changing_authored_shape_semantics() {
        let ir = fm_parser::parse("flowchart LR\nA[Entry]\nB(Worker)\nC([Done])\nD{Ready?}").ir;
        let layout = layout_diagram(&ir);
        for (radius, card_radius, rounded_radius) in [(8.0, 4.4, 8.0), (12.0, 6.6, 12.0)] {
            let mut context = MockCanvas2dContext::new(800.0, 600.0);
            let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig {
                rounded_corners: radius,
                ..Default::default()
            });
            assert_eq!(
                renderer.draw_nodes(&layout, &ir, &mut context, 0.0, 0.0, &mut 0),
                4
            );
            let radii: Vec<_> = context
                .operations()
                .iter()
                .filter_map(|operation| match operation {
                    DrawOperation::ArcTo(_, _, _, _, radius) => Some(*radius),
                    _ => None,
                })
                .collect();
            assert_eq!(
                radii.len(),
                12,
                "two cards and one stadium have four corners each"
            );
            assert!(
                radii[..4]
                    .iter()
                    .all(|value| (*value - card_radius).abs() < 0.001)
            );
            assert!(
                radii[4..8]
                    .iter()
                    .all(|value| (*value - rounded_radius).abs() < 0.001)
            );
            let stadium = layout
                .nodes
                .iter()
                .find(|node| node.node_id == "C")
                .unwrap();
            assert!(
                radii[8..].iter().all(|value| {
                    (*value - f64::from(stadium.bounds.height) / 2.0).abs() < 0.001
                })
            );
            let (fills, text) = record_gantt_canvas_paint(context.operations());
            assert_eq!(fills.len(), 4);
            assert_eq!(fills[3].vertices, 4, "decision diamonds remain four-sided");
            assert_eq!(text.len(), 4, "rounding must not remove labels");
        }
    }

    #[test]
    fn nested_canvas_panels_paint_outer_first_and_share_configured_corner_geometry() {
        let ir = fm_parser::parse(
            "flowchart TB\nsubgraph outer[Platform]\nsubgraph inner[Workers]\nA --> B\nend\nend\n\
             style outer fill:#f1f5f9\nstyle inner fill:#e0e7ff",
        )
        .ir;
        let mut layout = layout_diagram(&ir);
        assert_eq!(layout.clusters.len(), 2);
        layout.clusters.sort_by(|left, right| {
            (left.bounds.width * left.bounds.height)
                .total_cmp(&(right.bounds.width * right.bounds.height))
        });
        for (radius, expected_radius) in [(8.0, 12.0), (0.0, 0.0), (12.0, 18.0)] {
            let mut context = MockCanvas2dContext::new(800.0, 600.0);
            let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig {
                rounded_corners: radius,
                ..Default::default()
            });
            assert_eq!(
                renderer.draw_clusters(&layout, &ir, &mut context, 0.0, 0.0),
                2
            );
            let (fills, labels) = record_gantt_canvas_paint(context.operations());
            assert_eq!(
                fills[0].paint.fill, "#f1f5f9",
                "outer panel must be behind its child"
            );
            assert_eq!(fills[1].paint.fill, "#e0e7ff");
            assert!(
                fills[0].bounds[2] * fills[0].bounds[3] > fills[1].bounds[2] * fills[1].bounds[3]
            );
            assert!(
                labels
                    .iter()
                    .all(|label| label.paint.font.starts_with("600 "))
            );
            let radii: Vec<_> = context
                .operations()
                .iter()
                .filter_map(|operation| {
                    if let DrawOperation::ArcTo(_, _, _, _, radius) = operation {
                        Some(*radius)
                    } else {
                        None
                    }
                })
                .collect();
            if radius == 0.0 {
                assert!(
                    radii.is_empty(),
                    "square panel override must reach the geometry"
                );
            } else {
                assert_eq!(radii.len(), 8);
                assert!(
                    radii
                        .iter()
                        .all(|value| (*value - expected_radius).abs() < 0.001)
                );
            }
        }
    }

    #[test]
    fn canvas_edge_plates_use_the_document_surface_with_soft_corners() {
        let ir = fm_parser::parse("flowchart LR\nA -->|Approved| B").ir;
        let layout = layout_diagram(&ir);
        let mut context = MockCanvas2dContext::new(800.0, 600.0);
        let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig {
            background: Some("#ffffff".to_owned()),
            node_fill: "#e0e7ff".to_owned(),
            ..Default::default()
        });
        renderer.draw_edges(&layout, &ir, &mut context, 0.0, 0.0, &mut 0);
        let (fills, labels) = record_gantt_canvas_paint(context.operations());
        let plate = fills.last().expect("edge caption needs a backing plate");
        assert_eq!(plate.paint.fill, "#ffffff");
        let label = labels
            .iter()
            .find(|label| label.text == "Approved")
            .unwrap();
        assert!(label.x > plate.bounds[0] && label.x < plate.bounds[0] + plate.bounds[2]);
        assert!(label.y > plate.bounds[1] && label.y < plate.bounds[1] + plate.bounds[3]);
        assert_eq!(context.operations().iter().filter(|operation| {
            matches!(operation, DrawOperation::ArcTo(_, _, _, _, radius) if (*radius - 4.0).abs() < 0.001)
        }).count(), 4);
    }

    #[test]
    fn sankey_source_colors_reach_raster_ribbons_nodes_and_gpu_markers() {
        let ir =
            fm_parser::parse("sankey-beta\nSource,Review,100\nSource,Queue,50\nReview,Done,80").ir;
        let layout = layout_diagram(&ir);
        let scene = build_render_scene(&ir, &layout);
        for preset in [
            fm_render_svg::ThemePreset::Default,
            fm_render_svg::ThemePreset::Dark,
        ] {
            let colors = fm_render_svg::ThemeColors::from_preset(preset);
            let config = CanvasRenderConfig {
                background: Some(colors.background),
                node_fill: colors.node_fill,
                node_stroke: colors.node_stroke,
                edge_stroke: colors.edge,
                label_color: colors.text,
                accents: colors.accents,
                ..Default::default()
            };
            let mut context = MockCanvas2dContext::new(1000.0, 700.0);
            Canvas2dRenderer::new(config.clone()).render(&layout, &ir, &mut context);
            let mut stroke = String::new();
            let mut width = 0.0;
            let mut ribbon_colors = Vec::new();
            for operation in context.operations() {
                match operation {
                    DrawOperation::SetStrokeStyle(value) => stroke.clone_from(value),
                    DrawOperation::SetLineWidth(value) => width = *value,
                    DrawOperation::Stroke if width > 2.0 => ribbon_colors.push(stroke.clone()),
                    _ => {}
                }
            }
            assert_eq!(
                ribbon_colors,
                [
                    config.accents[0].clone(),
                    config.accents[0].clone(),
                    config.accents[1].clone()
                ]
            );
            let (fills, labels) = record_gantt_canvas_paint(context.operations());
            assert!(
                labels.iter().any(|label| label.text == "150"),
                "flow totals remain visible"
            );
            for plan in [
                crate::GpuRenderPlan::from_layout_with_config(&ir, &layout, &config),
                crate::GpuRenderPlan::from_layout_and_scene_with_config(
                    &ir, &layout, &scene, &config,
                ),
            ] {
                for segment in &plan.edge_segments {
                    let color =
                        crate::parse_paint_rgba(&ribbon_colors[segment.edge_index as usize])
                            .unwrap();
                    assert_eq!(segment.color, color);
                    assert!(
                        plan.arrowheads
                            .iter()
                            .filter(|marker| marker.edge_index == segment.edge_index)
                            .all(|marker| { marker.color == color && marker.fill == color }),
                        "a ribbon tip must keep its source hue"
                    );
                }
                assert_eq!(plan.arrowheads.len(), ir.edges.len());
                assert!(!plan.text_quads.is_empty());
                assert!(
                    plan.text_quads.iter().all(|quad| {
                        quad.color == crate::parse_paint_rgba(&config.label_color).unwrap()
                    }),
                    "category names and totals must remain readable on dark surfaces"
                );
                for node in &plan.node_instances {
                    let role = fm_core::sankey_node_style(
                        &ir,
                        node.node_index as usize,
                        &config.node_fill,
                        &config.node_stroke,
                        &config.label_color,
                        &config.accents,
                    )
                    .unwrap();
                    assert_eq!(node.fill, crate::parse_paint_rgba(&role.fill).unwrap());
                    assert!(
                        fills.iter().any(|fill| fill.paint.fill == role.fill),
                        "raster node lost its category surface"
                    );
                }
            }
        }
    }

    #[test]
    fn sankey_palette_preserves_author_paint_and_source_theme_precedence() {
        let mut ir = fm_parser::parse("sankey-beta\nA,B,100\nA,C,50").ir;
        ir.meta.theme_overrides.theme_variables.extend([
            ("primaryColor".to_owned(), "#ddeeff".to_owned()),
            ("primaryBorderColor".to_owned(), "#335577".to_owned()),
            ("lineColor".to_owned(), "#2468ac".to_owned()),
        ]);
        ir.nodes[0].inline_style = Some(Box::new(fm_core::IrInlineStyle::from_pairs([(
            "fill".to_owned(),
            "#cc8844".to_owned(),
        )])));
        ir.edges[1].inline_style = Some(Box::new(fm_core::IrInlineStyle::from_pairs([
            ("stroke".to_owned(), "#992244".to_owned()),
            ("stroke-width".to_owned(), "7px".to_owned()),
        ])));
        let config = CanvasRenderConfig::default();
        assert_eq!(
            resolve_node_colors_with_defaults(&ir, 0, &config),
            (Some("#cc8844".to_owned()), Some("#335577".to_owned()))
        );
        assert_eq!(
            resolve_node_colors_with_defaults(&ir, 1, &config),
            (Some("#ddeeff".to_owned()), Some("#335577".to_owned()))
        );
        assert_eq!(
            resolve_edge_style_with_defaults(&ir, 0, &config),
            (Some("#2468ac".to_owned()), None)
        );
        assert_eq!(
            resolve_edge_style_with_defaults(&ir, 1, &config),
            (Some("#992244".to_owned()), Some(7.0))
        );
        let layout = layout_diagram(&ir);
        let scene = build_render_scene(&ir, &layout);
        let plan =
            crate::GpuRenderPlan::from_layout_and_scene_with_config(&ir, &layout, &scene, &config);
        for segment in &plan.edge_segments {
            let expected = if segment.edge_index == 0 {
                "#2468ac"
            } else {
                "#992244"
            };
            assert_eq!(segment.color, crate::parse_paint_rgba(expected).unwrap());
            if segment.edge_index == 1 {
                assert_eq!(segment.width, 7.0);
            }
        }
        for node in &plan.node_instances {
            let expected = if node.node_index == 0 {
                "#cc8844"
            } else {
                "#ddeeff"
            };
            assert_eq!(node.fill, crate::parse_paint_rgba(expected).unwrap());
            assert_eq!(node.stroke, crate::parse_paint_rgba("#335577").unwrap());
        }
    }

    #[test]
    fn mindmap_defaults_follow_branches_and_preserve_author_channels() {
        let mut ir = fm_parser::parse(
            "mindmap\n  root((Root))\n    A[Research]\n      Child[Child]\n    B[Design]\n    Plain\n",
        )
        .ir;
        let index = |id: &str| ir.nodes.iter().position(|node| node.id == id).unwrap();
        let (root, a, child, b, plain) = (
            index("root"),
            index("A"),
            index("Child"),
            index("B"),
            index("Plain"),
        );
        for preset in [
            fm_render_svg::ThemePreset::Default,
            fm_render_svg::ThemePreset::Dark,
        ] {
            let colors = fm_render_svg::ThemeColors::from_preset(preset);
            let config = CanvasRenderConfig {
                node_fill: colors.node_fill,
                accents: colors.accents,
                ..Default::default()
            };
            let branch = resolve_node_colors_with_defaults(&ir, a, &config);
            assert_eq!(
                branch,
                resolve_node_colors_with_defaults(&ir, child, &config)
            );
            assert_ne!(branch, resolve_node_colors_with_defaults(&ir, b, &config));
            assert_eq!(branch.1.as_deref(), Some(config.accents[1].as_str()));
            assert_eq!(resolve_declared_node_font(&ir, root).weight, Some("600"));
            assert_eq!(resolve_declared_node_font(&ir, a).weight, None);
            assert_eq!(
                resolve_node_colors_with_defaults(&ir, plain, &config)
                    .1
                    .as_deref(),
                Some("transparent")
            );
            ir.nodes[a].inline_style = Some(Box::new(fm_core::parse_style_string("fill:#ffcc00")));
            let styled = resolve_node_colors_with_defaults(&ir, a, &config);
            assert_eq!(styled.0.as_deref(), Some("#ffcc00"));
            assert_eq!(styled.1, branch.1);
            ir.nodes[a].inline_style = None;
        }
        ir.nodes[root].inline_style =
            Some(Box::new(fm_core::parse_style_string("font-weight:400")));
        assert_eq!(resolve_declared_node_font(&ir, root).weight, Some("400"));
        let styled = fm_parser::parse(
            "mindmap\n  root((Root))\n    Plain:::custom\nclassDef custom stroke:#b45309\n",
        )
        .ir;
        let plain = styled
            .nodes
            .iter()
            .position(|node| node.id == "Plain")
            .unwrap();
        assert_eq!(
            resolve_node_colors_with_defaults(&styled, plain, &CanvasRenderConfig::default())
                .1
                .as_deref(),
            Some("#b45309")
        );
    }

    #[test]
    fn state_markers_have_solid_fills_and_terminal_rings() {
        let ir = fm_parser::parse(
            "stateDiagram-v2\n  state fork_point <<fork>>\n  [*] --> fork_point\n  fork_point --> [*]\n",
        )
        .ir;
        assert_eq!(ir.nodes.len(), 3);
        for preset in [
            fm_render_svg::ThemePreset::Default,
            fm_render_svg::ThemePreset::Dark,
        ] {
            let colors = fm_render_svg::ThemeColors::from_preset(preset);
            let config = CanvasRenderConfig {
                node_fill: colors.node_fill,
                node_stroke: colors.node_stroke,
                edge_stroke: colors.edge,
                ..Default::default()
            };
            let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
            let result = crate::render_to_canvas(&ir, &mut ctx, &config);
            assert_eq!(result.nodes_drawn, 3);
            let mut active_fill = "";
            let mut curved_path = false;
            let mut curved_fills = Vec::new();
            for operation in ctx.operations() {
                match operation {
                    DrawOperation::SetFillStyle(fill) => active_fill = fill,
                    DrawOperation::BeginPath => curved_path = false,
                    DrawOperation::Arc(..) | DrawOperation::ArcTo(..) => curved_path = true,
                    DrawOperation::Fill if curved_path => curved_fills.push(active_fill),
                    _ => {}
                }
            }
            assert_eq!(
                curved_fills
                    .iter()
                    .filter(|fill| **fill == config.edge_stroke)
                    .count(),
                3,
                "start, fork, and terminal disc must have contrasting solid fills"
            );
            assert_eq!(
                curved_fills
                    .iter()
                    .filter(|fill| **fill == config.node_fill)
                    .count(),
                1,
                "the terminal outer ring must retain its background"
            );
            assert_eq!(
                ctx.operations()
                    .iter()
                    .filter(|op| matches!(op, DrawOperation::Arc(..)))
                    .count(),
                3,
                "the terminal disc must be drawn only once"
            );
        }
    }

    #[test]
    fn solid_marker_defaults_preserve_class_and_inline_fill_overrides() {
        for declaration in [
            "style A fill:#f97316,stroke:#020617",
            "classDef marker fill:#f97316,stroke:#020617\n  class A marker",
        ] {
            let ir = fm_parser::parse(&format!(
                "flowchart LR\n  A@{{ shape: f-circ }}\n  {declaration}\n"
            ))
            .ir;
            assert_eq!(ir.nodes.len(), 1);
            assert_eq!(ir.nodes[0].shape, NodeShape::FilledCircle);
            let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
            crate::render_to_canvas(&ir, &mut ctx, &CanvasRenderConfig::default());
            let mut active_fill = "";
            let mut fills = Vec::new();
            for operation in ctx.operations() {
                match operation {
                    DrawOperation::SetFillStyle(fill) => active_fill = fill,
                    DrawOperation::Fill => fills.push(active_fill),
                    _ => {}
                }
            }
            assert_eq!(fills, ["#f97316"]);
        }
    }

    #[test]
    fn canvas_ornaments_do_not_draw_internal_ids() {
        for (source, minimum_ornaments, visible_labels) in [
            (
                "stateDiagram-v2\n  [*] --> Idle\n  state fork_point <<fork>>\n  \
                 state join_point <<join>>\n  state decision <<choice>>\n  \
                 Idle --> fork_point\n  fork_point --> Working\n  Working --> join_point\n  \
                 join_point --> decision\n  decision --> Done\n  Done --> [*]\n",
                5,
                ["Idle", "Working", "Done"].as_slice(),
            ),
            (
                "flowchart LR\n  A[Receive] --> J@{ shape: f-circ, label: \"Junction plumbing\" }\n  \
                 J --> B[Continue]\n",
                1,
                ["Receive", "Continue"].as_slice(),
            ),
        ] {
            let ir = fm_parser::parse(source).ir;
            let ornaments: Vec<_> = ir
                .nodes
                .iter()
                .filter(|node| ir.is_textless_ornament_node(node))
                .collect();
            assert!(
                ornaments.len() >= minimum_ornaments,
                "the fixture must contain the ornament shapes being tested"
            );

            let mut ctx = MockCanvas2dContext::new(1200.0, 900.0);
            let result = crate::render_to_canvas(&ir, &mut ctx, &CanvasRenderConfig::default());
            assert_eq!(result.nodes_drawn, ir.nodes.len());
            let texts: Vec<_> = ctx
                .operations()
                .iter()
                .filter_map(|operation| match operation {
                    DrawOperation::FillText(text, _, _) => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            for label in visible_labels {
                assert!(texts.contains(label), "visible label {label:?} was lost");
            }
            for node in ornaments {
                assert!(
                    !texts.contains(&node.id.as_str()),
                    "ornament id {:?} was drawn over its glyph",
                    node.id
                );
                if let Some(label) = node.label.and_then(|id| ir.labels.get(id.0)) {
                    assert!(
                        !texts.contains(&label.text.as_str()),
                        "an ornament label was drawn despite the shared display rule"
                    );
                }
            }
        }
    }

    #[test]
    fn canvas_keeps_labels_on_reversed_and_highlighted_git_commits() {
        let ir = fm_parser::parse(
            "gitGraph\n  commit id: \"reverted\" type: REVERSE\n  \
             commit id: \"highlighted\" type: HIGHLIGHT\n",
        )
        .ir;
        assert!(
            ir.nodes
                .iter()
                .any(|node| node.shape == NodeShape::FilledCircle)
        );
        assert!(
            ir.nodes
                .iter()
                .any(|node| node.shape == NodeShape::DoubleCircle)
        );

        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        crate::render_to_canvas(&ir, &mut ctx, &CanvasRenderConfig::default());
        for label in ["reverted", "highlighted"] {
            assert!(ctx.operations().iter().any(|operation| {
                matches!(operation, DrawOperation::FillText(text, _, _) if text == label)
            }));
        }
    }

    #[test]
    fn multiline_node_labels_use_the_resolved_font_and_keep_their_center() {
        let ir = fm_parser::parse(
            "flowchart TB\n  A[\"First<br/>Second\"] --> B[\"Other<br/>Lines\"]\n  \
             style A font-size:32px\n",
        )
        .ir;
        assert!(ir.labels.iter().any(|label| label.text == "First\nSecond"));
        assert!(ir.labels.iter().any(|label| label.text == "Other\nLines"));
        let config = CanvasRenderConfig {
            font_size: 18.0,
            line_height: 1.75,
            padding: 12.0,
            auto_fit: false,
            ..CanvasRenderConfig::default()
        };
        let layout = fm_layout::layout_diagram_with_config(
            &ir,
            fm_layout::LayoutConfig {
                font_metrics: Some(config.font_metrics()),
                ..Default::default()
            },
        );
        let mut ctx = MockCanvas2dContext::new(1200.0, 900.0);
        Canvas2dRenderer::new(config.clone()).render(&layout, &ir, &mut ctx);
        let mut font = String::new();
        let mut text_runs = Vec::new();
        for operation in ctx.operations() {
            match operation {
                DrawOperation::SetFont(value) => font.clone_from(value),
                DrawOperation::FillText(text, _, y) => {
                    text_runs.push((text.as_str(), *y, font.clone()));
                }
                _ => {}
            }
        }

        for (node_id, first, second, font_size) in [
            ("A", "First", "Second", 32.0),
            ("B", "Other", "Lines", 18.0),
        ] {
            let first = text_runs.iter().find(|run| run.0 == first).unwrap();
            let second = text_runs.iter().find(|run| run.0 == second).unwrap();
            assert!(first.2.starts_with(&format!("{font_size}px ")));
            assert_eq!(first.2, second.2);
            assert!((second.1 - first.1 - font_size * config.line_height).abs() < 1e-6);
            let node = layout
                .nodes
                .iter()
                .find(|node| node.node_id == node_id)
                .unwrap();
            let center = config.padding - f64::from(layout.bounds.y)
                + f64::from(node.bounds.y)
                + f64::from(node.bounds.height) * 0.5;
            assert!(((first.1 + second.1) * 0.5 - center).abs() < 1e-6);
        }
    }

    #[test]
    fn shaped_labels_fit_the_drawn_outline_and_gpu_uses_the_same_baselines() {
        let inside = |x: f64, y: f64, outline: &[(f64, f64)]| {
            let mut result = false;
            let mut previous = outline[outline.len() - 1];
            for &(px, py) in outline {
                if (py > y) != (previous.1 > y)
                    && x < (previous.0 - px) * (y - py) / (previous.1 - py) + px
                {
                    result = !result;
                }
                previous = (px, py);
            }
            result
        };
        for shape in [
            NodeShape::Triangle,
            NodeShape::FlippedTriangle,
            NodeShape::Pentagon,
            NodeShape::Star,
        ] {
            let mut ir = fm_parser::parse("flowchart TB\n A[\"Project<br/>approved\"]").ir;
            ir.nodes[0].shape = shape;
            let config = CanvasRenderConfig {
                auto_fit: false,
                padding: 0.0,
                ..Default::default()
            };
            let layout = fm_layout::layout_diagram_with_config(
                &ir,
                fm_layout::LayoutConfig {
                    font_metrics: Some(config.font_metrics()),
                    ..Default::default()
                },
            );
            let mut ctx = MockCanvas2dContext::new(1200.0, 900.0);
            Canvas2dRenderer::new(config.clone()).render(&layout, &ir, &mut ctx);
            // Read the polygon actually submitted by Canvas, independently of label_region.
            let mut outline = Vec::new();
            for operation in ctx.operations() {
                match operation {
                    DrawOperation::MoveTo(x, y) | DrawOperation::LineTo(x, y) => {
                        outline.push((*x, *y))
                    }
                    DrawOperation::Fill => break,
                    _ => {}
                }
            }
            assert!(outline.len() >= 3);
            let (_, text) = record_gantt_canvas_paint(ctx.operations());
            assert_eq!(text.len(), 2);
            let plan = crate::GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
            let runs: Vec<_> = plan
                .text_runs
                .iter()
                .filter(|run| run.source == crate::GpuTextSource::Node)
                .collect();
            assert_eq!(runs.len(), text.len());
            for (run, glyphs) in text.iter().zip(runs) {
                assert!(
                    run.paint.font.starts_with("15px "),
                    "type must stay readable for {shape:?}"
                );
                let half_width = f64::from(config.font_metrics().estimate_width(&run.text)) * 0.5;
                for x in [run.x - half_width, run.x + half_width] {
                    for y in [
                        run.y - config.font_size * 0.5,
                        run.y + config.font_size * 0.5,
                    ] {
                        assert!(
                            inside(x, y, &outline),
                            "{shape:?} label {:?} leaves its drawn outline at ({x}, {y})",
                            run.text
                        );
                    }
                }
                let first = glyphs.first_quad as usize;
                let quads = &plan.text_quads[first..first + glyphs.quad_count as usize];
                assert!(!quads.is_empty());
                for quad in quads {
                    assert!(
                        (f64::from(quad.center[1] - layout.bounds.y) - run.y).abs() < 0.001,
                        "GPU and Canvas disagree on {shape:?} label placement"
                    );
                    assert!(
                        (f64::from(quad.half_extent[1]) - config.font_size * 0.5).abs() < 0.001
                    );
                }
            }
        }
    }

    #[test]
    fn sequence_notes_draw_each_measured_line_inside_the_note() {
        let ir = fm_parser::parse(
            "sequenceDiagram\n  participant A\n  participant B\n  \
             Note left of A: Customer checks the current account balance before approving a payment<br/><br/>Finish\n  \
             A->>B: Continue\n",
        )
        .ir;
        let config = CanvasRenderConfig {
            font_size: 20.0,
            line_height: 1.75,
            padding: 12.0,
            auto_fit: false,
            ..Default::default()
        };
        let layout = fm_layout::layout_diagram_with_config(
            &ir,
            fm_layout::LayoutConfig {
                font_metrics: Some(config.font_metrics()),
                ..Default::default()
            },
        );
        let note = &layout.extensions.sequence_notes[0];
        let lines: Vec<_> = note.text.split('\n').collect();
        assert!(
            lines.len() >= 4,
            "wrapping and the authored blank line both survive"
        );
        assert!(lines.contains(&""));
        let mut ctx = MockCanvas2dContext::new(1600.0, 1200.0);
        Canvas2dRenderer::new(config.clone()).render(&layout, &ir, &mut ctx);

        let mut font = String::new();
        let mut align = TextAlign::Center;
        let mut baseline = TextBaseline::Alphabetic;
        let mut runs = Vec::new();
        for operation in ctx.operations() {
            match operation {
                DrawOperation::SetFont(value) => font.clone_from(value),
                DrawOperation::SetTextAlign(value) => align = *value,
                DrawOperation::SetTextBaseline(value) => baseline = *value,
                DrawOperation::FillText(text, x, y) if font.starts_with("16px ") => {
                    assert_eq!(align, TextAlign::Left);
                    assert_eq!(baseline, TextBaseline::Top);
                    runs.push((text.as_str(), *x, *y));
                }
                _ => {}
            }
        }
        assert_eq!(runs.iter().map(|run| run.0).collect::<Vec<_>>(), lines);
        let x = config.padding + f64::from(note.bounds.x - layout.bounds.x) + 12.0;
        let top = config.padding + f64::from(note.bounds.y - layout.bounds.y);
        for (row, (_, actual_x, actual_y)) in runs.iter().enumerate() {
            assert!((actual_x - x).abs() < 0.01);
            assert!((actual_y - (top + 10.0 + row as f64 * 28.0)).abs() < 0.01);
            assert!(*actual_y + 16.0 <= top + f64::from(note.bounds.height) - 10.0 + 0.01);
        }
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

        let (fills, _) = record_gantt_canvas_paint(ctx.operations());
        let [rect_x, rect_y, _, _] = fills.first().expect("node box must be painted").bounds;

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

        let (fills, _) = record_gantt_canvas_paint(ctx.operations());
        let [rect_x, rect_y, _, _] = fills.first().expect("node box must be painted").bounds;

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
    fn quadrant_canvas_draws_small_styled_markers_and_fits_all_captions() {
        let mut ir = fm_parser::parse(
            "quadrantChart\n\
             title Priorities across the product roadmap\n\
             x-axis Low effort --> High effort\n\
             y-axis Limited customer benefit --> Significant lasting customer benefit\n\
             quadrant-1 Invest next\n\
             quadrant-2 Quick wins\n\
             quadrant-3 Reconsider\n\
             quadrant-4 Research first\n\
             Reliable exports: [0.52, 0.52] radius: 12, color: #ef4444, stroke-color: #f8fafc, stroke-width: 2px\n\
             Fast previews: [0.52, 0.52]\n\
             Accessible labels: [0.52, 0.52]\n\
             Search across projects: [0.52, 0.52]\n\
             Shared editing: [0.52, 0.52]\n\
             Offline workspace: [0.52, 0.52]",
        ).ir;
        ir.meta.theme_overrides.theme_variables.extend([
            ("background".to_owned(), "#0f172a".to_owned()),
            ("primaryTextColor".to_owned(), "#e2e8f0".to_owned()),
            ("lineColor".to_owned(), "#94a3b8".to_owned()),
        ]);
        let layout = layout_diagram(&ir);
        assert_eq!(layout.nodes.len(), 6, "all styled data points must parse");
        let config = CanvasRenderConfig::default();
        let meta = ir.quadrant_meta.as_ref().unwrap();
        let chart = fm_layout::QuadrantChartLayout::new(
            &layout,
            meta,
            ir.meta.title.as_deref().or(meta.title.as_deref()),
            (0.0, 0.0),
            &config.font_metrics(),
            8.0,
            config.font_size as f32 * 0.5,
        );
        assert!(
            !chart.leaders.is_empty(),
            "crowded captions need leader lines"
        );
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        let result = crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, &config);
        assert_eq!(result.nodes_drawn, 6);
        assert_eq!(result.edges_drawn, 0);
        let markers: Vec<_> = ctx
            .operations()
            .iter()
            .filter_map(|operation| {
                if let DrawOperation::Arc(x, y, radius, _, _) = operation {
                    Some((*x, *y, *radius))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            markers.len(),
            6,
            "generic label-sized graph circles must not be drawn"
        );
        assert_eq!(markers[0].2, 12.0, "the authored marker radius wins");
        assert!(markers[1..].iter().all(|marker| marker.2 == 6.0));
        for (marker, node) in markers.iter().zip(&layout.nodes) {
            let center = node.bounds.center();
            assert!((marker.0 - f64::from(center.x - chart.bounds.x)).abs() < 0.001);
            assert!((marker.1 - f64::from(center.y - chart.bounds.y)).abs() < 0.001);
        }
        let (fills, texts) = record_gantt_canvas_paint(ctx.operations());
        assert_eq!(
            fills
                .iter()
                .filter(|fill| (fill.paint.alpha - 0.10).abs() < 1e-6)
                .count(),
            4
        );
        assert_eq!(
            fills
                .iter()
                .filter(|fill| (fill.paint.alpha - 0.92).abs() < 1e-6)
                .count(),
            6
        );
        assert!(texts.iter().all(|text| text.paint.fill == "#e2e8f0"));
        assert_eq!(
            texts
                .iter()
                .filter(|text| text.text == "Reliable exports")
                .count(),
            1
        );
        assert!(
            ctx.operations()
                .contains(&DrawOperation::SetFillStyle("#ef4444".to_owned()))
        );
        assert!(
            ctx.operations()
                .contains(&DrawOperation::SetStrokeStyle("#f8fafc".to_owned()))
        );
        assert!(ctx.operations().contains(&DrawOperation::SetLineWidth(2.0)));
        for bounds in chart
            .labels
            .iter()
            .map(|label| label.bounds)
            .chain(chart.annotations.iter().map(|text| text.bounds))
        {
            let left = f64::from(bounds.x) * result.viewport.zoom + result.viewport.offset_x;
            let top = f64::from(bounds.y) * result.viewport.zoom + result.viewport.offset_y;
            let right = left + f64::from(bounds.width) * result.viewport.zoom;
            let bottom = top + f64::from(bounds.height) * result.viewport.zoom;
            assert!(
                left >= config.padding - 0.01 && right <= 800.0 - config.padding + 0.01,
                "caption horizontally clipped: {bounds:?}"
            );
            assert!(
                top >= config.padding - 0.01 && bottom <= 600.0 - config.padding + 0.01,
                "caption vertically clipped: {bounds:?}"
            );
        }
        let mut second = MockCanvas2dContext::new(800.0, 600.0);
        crate::render_to_canvas_with_layout(&ir, &layout, &mut second, &config);
        assert_eq!(
            ctx.operations(),
            second.operations(),
            "chart placement must be deterministic"
        );
    }

    #[derive(Clone, Debug)]
    struct GanttCanvasPaint {
        fill: String,
        stroke: String,
        line_width: f64,
        alpha: f64,
        dash: Vec<f64>,
        font: String,
        align: TextAlign,
        baseline: TextBaseline,
        clipped: bool,
    }

    #[derive(Debug)]
    struct GanttCanvasFill {
        paint: GanttCanvasPaint,
        bounds: [f64; 4],
        vertices: usize,
    }

    #[derive(Debug)]
    struct GanttCanvasText {
        paint: GanttCanvasPaint,
        text: String,
        x: f64,
        y: f64,
    }

    fn record_gantt_canvas_paint(
        operations: &[DrawOperation],
    ) -> (Vec<GanttCanvasFill>, Vec<GanttCanvasText>) {
        let mut paint = GanttCanvasPaint {
            fill: String::new(),
            stroke: String::new(),
            line_width: 1.0,
            alpha: 1.0,
            dash: Vec::new(),
            font: String::new(),
            align: TextAlign::Left,
            baseline: TextBaseline::Alphabetic,
            clipped: false,
        };
        let mut stack = Vec::new();
        let mut points = Vec::new();
        let mut fills = Vec::new();
        let mut text = Vec::new();
        for operation in operations {
            match operation {
                DrawOperation::Save => stack.push(paint.clone()),
                DrawOperation::Restore => paint = stack.pop().expect("balanced canvas state"),
                DrawOperation::SetFillStyle(value) => paint.fill.clone_from(value),
                DrawOperation::SetStrokeStyle(value) => paint.stroke.clone_from(value),
                DrawOperation::SetLineWidth(value) => paint.line_width = *value,
                DrawOperation::SetGlobalAlpha(value) => paint.alpha = *value,
                DrawOperation::SetLineDash(value) => paint.dash.clone_from(value),
                DrawOperation::SetFont(value) => paint.font.clone_from(value),
                DrawOperation::SetTextAlign(value) => paint.align = *value,
                DrawOperation::SetTextBaseline(value) => paint.baseline = *value,
                DrawOperation::Clip => paint.clipped = true,
                DrawOperation::BeginPath => points.clear(),
                DrawOperation::MoveTo(x, y) | DrawOperation::LineTo(x, y) => points.push((*x, *y)),
                DrawOperation::QuadraticCurveTo(cx, cy, x, y) => {
                    points.extend([(*cx, *cy), (*x, *y)])
                }
                DrawOperation::Rect(x, y, w, h) => points.extend([(*x, *y), (*x + *w, *y + *h)]),
                DrawOperation::Fill if !points.is_empty() => {
                    let min_x = points
                        .iter()
                        .map(|point| point.0)
                        .fold(f64::INFINITY, f64::min);
                    let min_y = points
                        .iter()
                        .map(|point| point.1)
                        .fold(f64::INFINITY, f64::min);
                    let max_x = points
                        .iter()
                        .map(|point| point.0)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let max_y = points
                        .iter()
                        .map(|point| point.1)
                        .fold(f64::NEG_INFINITY, f64::max);
                    fills.push(GanttCanvasFill {
                        paint: paint.clone(),
                        bounds: [min_x, min_y, max_x - min_x, max_y - min_y],
                        vertices: points.len(),
                    });
                }
                DrawOperation::FillRect(x, y, w, h) => fills.push(GanttCanvasFill {
                    paint: paint.clone(),
                    bounds: [*x, *y, *w, *h],
                    vertices: 4,
                }),
                DrawOperation::FillText(value, x, y) => text.push(GanttCanvasText {
                    paint: paint.clone(),
                    text: value.clone(),
                    x: *x,
                    y: *y,
                }),
                _ => {}
            }
        }
        assert!(stack.is_empty());
        (fills, text)
    }

    #[test]
    fn canvas_gantt_tasks_preserve_status_milestones_progress_and_labels() {
        let ir = fm_parser::parse(
            "%%{init: {\"gantt\": {\"topAxis\": true}}}%%\n\
             gantt\n title Release schedule\n dateFormat YYYY-MM-DD\n excludes weekends\n\
             section Delivery\n Design :crit, done, design, 2026-02-06, 2d, 50%\n\
             Build :active, build, after design, 3d, 25%\n\
             Release :milestone, release, after build, 0d, 80%\n\
             Critical review :crit, review, after build, 2d\n\
             Verification with a deliberately long name :verify, after build, 1d\n",
        )
        .ir;
        let mut layout = layout_diagram(&ir);
        assert_eq!(layout.nodes.len(), 5);
        assert_eq!(layout.edges.len(), 4);
        layout.nodes.reverse();
        for dark in [false, true] {
            let mut config = CanvasRenderConfig {
                auto_fit: false,
                padding: 0.0,
                ..Default::default()
            };
            if dark {
                config.node_fill = "#111827".to_owned();
                config.label_color = "#f8fafc".to_owned();
            }
            let mut ctx = MockCanvas2dContext::new(1600.0, 1000.0);
            let result = crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, &config);
            assert_eq!((result.nodes_drawn, result.edges_drawn), (5, 4));
            let mut repeated = MockCanvas2dContext::new(1600.0, 1000.0);
            crate::render_to_canvas_with_layout(&ir, &layout, &mut repeated, &config);
            assert_eq!(ctx.operations(), repeated.operations());
            let (fills, text) = record_gantt_canvas_paint(ctx.operations());
            for (index, color) in ["#86efac", "#94a3b8", "#c4b5fd", "#fca5a5", "#93c5fd"]
                .into_iter()
                .enumerate()
            {
                let fill = fills
                    .iter()
                    .find(|fill| fill.paint.fill == color)
                    .expect("status-colored task");
                let node = layout
                    .nodes
                    .iter()
                    .find(|node| node.node_index == index)
                    .unwrap();
                let [x, y, w, h] = fill.bounds;
                assert_eq!(fill.paint.alpha, 1.0);
                assert!(
                    (x + w / 2.0 + f64::from(layout.bounds.x) - f64::from(node.bounds.center().x))
                        .abs()
                        < 0.002
                );
                assert!(
                    (y + h / 2.0 + f64::from(layout.bounds.y) - f64::from(node.bounds.center().y))
                        .abs()
                        < 0.002
                );
                if index == 2 {
                    assert_eq!(
                        fill.vertices, 4,
                        "a milestone is a diamond, not a duration bar"
                    );
                    assert!((w - h).abs() < 0.002);
                    assert!(
                        (w - f64::from(node.bounds.width.min(node.bounds.height) * 0.8)).abs()
                            < 0.002
                    );
                } else {
                    assert!((w - f64::from(node.bounds.width)).abs() < 0.002);
                    assert!((h - f64::from(node.bounds.height)).abs() < 0.002);
                }
                if index == 0 || index == 3 {
                    assert_eq!(fill.paint.stroke, "#dc2626");
                    assert_eq!(fill.paint.line_width, 2.0);
                }
            }
            let progress: Vec<_> = fills
                .iter()
                .filter(|fill| fill.paint.fill == "#0f172a")
                .collect();
            assert_eq!(progress.len(), 2, "milestones have no duration to fill");
            for (index, fraction) in [(0, 0.5), (1, 0.25)] {
                let node = layout
                    .nodes
                    .iter()
                    .find(|node| node.node_index == index)
                    .unwrap();
                let overlay = progress
                    .iter()
                    .find(|fill| {
                        (fill.bounds[1] + f64::from(layout.bounds.y) - f64::from(node.bounds.y))
                            .abs()
                            < 0.002
                    })
                    .unwrap();
                assert!(
                    (overlay.bounds[2] / f64::from(node.bounds.width) - fraction).abs() < 0.002
                );
                assert_eq!(overlay.paint.alpha, 0.22);
                assert!(
                    overlay.paint.clipped,
                    "progress must stay inside the task outline"
                );
            }
            assert_eq!(
                text.iter()
                    .filter(|entry| entry.text == "Release schedule")
                    .count(),
                1
            );
            for placement in &layout.extensions.gantt_task_labels {
                let label = ir.node_display_text(&ir.nodes[placement.node_index]);
                let entry = text
                    .iter()
                    .find(|entry| entry.text == label)
                    .expect("complete task name");
                let expected_align = match placement.placement {
                    fm_layout::GanttLabelPlacement::Inside => TextAlign::Center,
                    fm_layout::GanttLabelPlacement::OutsideLeft => TextAlign::Right,
                    fm_layout::GanttLabelPlacement::OutsideRight => TextAlign::Left,
                };
                assert_eq!(entry.paint.align, expected_align);
                assert_eq!(entry.paint.baseline, TextBaseline::Middle);
                assert!(
                    (entry.x + f64::from(layout.bounds.x) - f64::from(placement.x)).abs() < 0.002
                );
                let expected_color =
                    if dark && placement.placement == fm_layout::GanttLabelPlacement::Inside {
                        &config.node_fill
                    } else {
                        &config.label_color
                    };
                assert_eq!(&entry.paint.fill, expected_color);
            }
            for axis in &layout.extensions.gantt_axis_rows {
                for tick in &layout.extensions.axis_ticks {
                    assert!(text.iter().any(|entry| {
                        entry.text == tick.label
                            && (entry.x + f64::from(layout.bounds.x)
                                - f64::from(tick.position + 3.0))
                            .abs()
                                < 0.002
                            && (entry.y + f64::from(layout.bounds.y) - f64::from(axis.y)).abs()
                                < 0.002
                            && entry.paint.baseline == TextBaseline::Alphabetic
                    }));
                }
            }
        }
    }

    #[test]
    fn canvas_gantt_tasks_preserve_declared_styles_and_isolate_progress_opacity() {
        let mut ir = fm_parser::parse(
            "gantt\n dateFormat YYYY-MM-DD\n section Delivery\n\
             Styled :crit, done, styled, 2026-01-01, 4d, 50%\n\
             Untouched :plain, after styled, 2d\n\
             Marker :milestone, marker, after plain, 0d, 75%\n",
        )
        .ir;
        let styled_index = ir.gantt_meta.as_ref().unwrap().tasks[0].node.0;
        ir.nodes[styled_index].inline_style = Some(Box::new(fm_core::parse_style_string(
            "fill:#123456;fill-opacity:0.6;stroke:#abcdef;stroke-width:4px;stroke-dasharray:7 3;opacity:0.4;color:#ff00ff;font-size:19px",
        )));
        let layout = layout_diagram(&ir);
        let config = CanvasRenderConfig {
            auto_fit: false,
            padding: 0.0,
            node_stroke: "#fedcba".to_owned(),
            node_stroke_width: 3.25,
            ..Default::default()
        };
        let mut ctx = MockCanvas2dContext::new(1400.0, 700.0);
        crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, &config);
        let (fills, text) = record_gantt_canvas_paint(ctx.operations());
        let styled = fills
            .iter()
            .find(|fill| fill.paint.fill == "rgba(18, 52, 86, 0.6)")
            .expect("authored fill and fill opacity");
        assert_eq!(styled.paint.stroke, "#abcdef");
        assert_eq!(styled.paint.line_width, 4.0);
        assert_eq!(styled.paint.dash, [7.0, 3.0]);
        assert_eq!(styled.paint.alpha, 0.4);
        let overlay = fills
            .iter()
            .find(|fill| fill.paint.fill == "#0f172a")
            .expect("visible progress on an authored fill");
        assert!((overlay.paint.alpha - 0.088).abs() < 0.0001);
        assert!(overlay.paint.clipped);
        assert!((overlay.bounds[2] / styled.bounds[2] - 0.5).abs() < 0.002);
        let label = text.iter().find(|entry| entry.text == "Styled").unwrap();
        assert_eq!(label.paint.fill, "#ff00ff");
        assert_eq!(label.paint.alpha, 0.4);
        assert!(label.paint.font.contains("19px"));
        assert!(
            !label.paint.clipped,
            "the task's clip must not trim an outside name"
        );
        let untouched = fills
            .iter()
            .find(|fill| fill.paint.fill == "#93c5fd")
            .unwrap();
        assert_eq!(untouched.paint.alpha, 1.0);
        assert_eq!(untouched.paint.stroke, "#fedcba");
        assert_eq!(untouched.paint.line_width, 3.25);
        assert!(untouched.paint.dash.is_empty());
        let milestone = fills
            .iter()
            .find(|fill| fill.paint.fill == "#c4b5fd")
            .unwrap();
        assert_eq!(milestone.paint.stroke, "#fedcba");
        assert_eq!(milestone.paint.line_width, 3.25);
        let following_label = text.iter().find(|entry| entry.text == "Untouched").unwrap();
        assert_eq!(following_label.paint.alpha, 1.0);
        assert!(!following_label.paint.font.contains("19px"));

        for value in [f32::NAN, f32::INFINITY, -0.2, 0.0, 2.0] {
            ir.gantt_meta.as_mut().unwrap().tasks[0].progress = Some(value);
            let mut ctx = MockCanvas2dContext::new(1400.0, 700.0);
            crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, &config);
            let (fills, _) = record_gantt_canvas_paint(ctx.operations());
            let overlays: Vec<_> = fills
                .iter()
                .filter(|fill| fill.paint.fill == "#0f172a")
                .collect();
            if value == 2.0 {
                assert_eq!(overlays.len(), 1);
                assert!((overlays[0].bounds[2] - styled.bounds[2]).abs() < 0.002);
            } else {
                assert!(
                    overlays.is_empty(),
                    "invalid/empty progress painted: {value}"
                );
            }
        }

        let mut graph = fm_parser::parse("flowchart LR\nA[Ordinary]-->B[Unchanged]\n").ir;
        let graph_layout = layout_diagram(&graph);
        let mut before = MockCanvas2dContext::new(800.0, 600.0);
        crate::render_to_canvas_with_layout(&graph, &graph_layout, &mut before, &config);
        graph.gantt_meta = ir.gantt_meta;
        let mut after = MockCanvas2dContext::new(800.0, 600.0);
        crate::render_to_canvas_with_layout(&graph, &graph_layout, &mut after, &config);
        assert_eq!(
            before.operations(),
            after.operations(),
            "a foreign chart's metadata must not restyle a flowchart"
        );
    }

    #[test]
    fn canvas_scene_preserves_gantt_schedule_status_and_date_axes() {
        let ir = fm_parser::parse(
            "%%{init: {\"gantt\": {\"topAxis\": true}}}%%\n\
             gantt\n title Release schedule\n dateFormat YYYY-MM-DD\n excludes weekends\n\
             section Delivery\n Design :crit, done, design, 2026-02-06, 2d, 50%\n\
             Build :active, build, after design, 3d, 25%\n section Delivery\n\
             Release :milestone, release, after build, 0d\n\
             Verification with a deliberately long name :verify, after build, 1d\n",
        )
        .ir;
        let layout = layout_diagram(&ir);
        assert_eq!(layout.nodes.len(), 4);
        assert_eq!(layout.edges.len(), 3);
        assert_eq!(layout.extensions.gantt_axis_rows.len(), 2);
        let scene = build_render_scene(&ir, &layout);
        let config = CanvasRenderConfig {
            auto_fit: false,
            padding: 0.0,
            ..Default::default()
        };
        let mut ctx = MockCanvas2dContext::new(1600.0, 1000.0);
        let result = crate::render_scene_to_canvas(&scene, &mut ctx, &config);
        assert_eq!((result.nodes_drawn, result.edges_drawn), (4, 3));
        let mut repeated = MockCanvas2dContext::new(1600.0, 1000.0);
        crate::render_scene_to_canvas(&scene, &mut repeated, &config);
        assert_eq!(ctx.operations(), repeated.operations());

        let mut baseline = TextBaseline::Alphabetic;
        let mut align = TextAlign::Left;
        let mut text = Vec::new();
        let mut color = String::new();
        let mut alpha = 1.0;
        let mut points = Vec::new();
        let mut fills = Vec::new();
        for operation in ctx.operations() {
            match operation {
                DrawOperation::SetTextBaseline(value) => baseline = *value,
                DrawOperation::SetTextAlign(value) => align = *value,
                DrawOperation::FillText(label, x, y) => {
                    text.push((label.as_str(), *x, *y, align, baseline));
                }
                DrawOperation::SetFillStyle(value) => color.clone_from(value),
                DrawOperation::SetGlobalAlpha(value) => alpha = *value,
                DrawOperation::BeginPath => points.clear(),
                DrawOperation::MoveTo(x, y) | DrawOperation::LineTo(x, y) => points.push((*x, *y)),
                DrawOperation::QuadraticCurveTo(cx, cy, x, y) => {
                    points.extend([(*cx, *cy), (*x, *y)])
                }
                DrawOperation::BezierCurveTo(cx1, cy1, cx2, cy2, x, y) => {
                    points.extend([(*cx1, *cy1), (*cx2, *cy2), (*x, *y)]);
                }
                DrawOperation::Fill => fills.push((color.clone(), alpha, points.clone())),
                _ => {}
            }
        }
        for label in [
            "Release schedule",
            "Design",
            "Build",
            "Release",
            "Verification with a deliberately long name",
        ] {
            assert_eq!(
                text.iter().filter(|entry| entry.0 == label).count(),
                1,
                "{label}"
            );
        }
        assert_eq!(text.iter().filter(|entry| entry.0 == "Delivery").count(), 2);
        for axis in &layout.extensions.gantt_axis_rows {
            for tick in &layout.extensions.axis_ticks {
                assert!(
                    text.iter().any(|(label, x, y, align, baseline)| {
                        *label == tick.label
                            && (*x + f64::from(scene.bounds.x) - f64::from(tick.position + 3.0))
                                .abs()
                                < 0.002
                            && (*y + f64::from(scene.bounds.y) - f64::from(axis.y)).abs() < 0.002
                            && *align == TextAlign::Left
                            && *baseline == TextBaseline::Alphabetic
                    }),
                    "date tick must share the layout's baseline and day position"
                );
            }
        }
        for placement in &layout.extensions.gantt_task_labels {
            let label = ir.node_display_text(&ir.nodes[placement.node_index]);
            let expected = match placement.placement {
                fm_layout::GanttLabelPlacement::Inside => TextAlign::Center,
                fm_layout::GanttLabelPlacement::OutsideLeft => TextAlign::Right,
                fm_layout::GanttLabelPlacement::OutsideRight => TextAlign::Left,
            };
            assert!(text.iter().any(|entry| entry.0 == label
                && entry.3 == expected
                && (entry.1 + f64::from(scene.bounds.x) - f64::from(placement.x)).abs() < 0.002));
        }
        let extent = |points: &[(f64, f64)]| {
            let left = points
                .iter()
                .map(|point| point.0)
                .fold(f64::INFINITY, f64::min);
            let right = points
                .iter()
                .map(|point| point.0)
                .fold(f64::NEG_INFINITY, f64::max);
            let top = points
                .iter()
                .map(|point| point.1)
                .fold(f64::INFINITY, f64::min);
            let bottom = points
                .iter()
                .map(|point| point.1)
                .fold(f64::NEG_INFINITY, f64::max);
            (left, top, right - left, bottom - top)
        };
        for (index, color) in ["#86efac", "#94a3b8", "#c4b5fd", "#93c5fd"]
            .into_iter()
            .enumerate()
        {
            let (_, alpha, points) = fills
                .iter()
                .find(|fill| fill.0 == color)
                .expect("task status fill");
            assert_eq!(*alpha, 1.0);
            let actual = extent(points);
            let node = layout
                .nodes
                .iter()
                .find(|node| node.node_index == index)
                .unwrap();
            if index == 2 {
                assert_eq!(points.len(), 4, "milestone requires four diamond vertices");
                assert!((actual.2 - actual.3).abs() < 0.002);
                assert!(
                    (actual.0 + actual.2 / 2.0 + f64::from(scene.bounds.x)
                        - f64::from(node.bounds.center().x))
                    .abs()
                        < 0.002
                );
            } else {
                assert!((actual.2 - f64::from(node.bounds.width)).abs() < 0.002);
                assert!((actual.3 - f64::from(node.bounds.height)).abs() < 0.002);
            }
        }
        let progress: Vec<_> = fills.iter().filter(|fill| fill.0 == "#0f172a").collect();
        assert_eq!(progress.len(), 2);
        for (fill, fraction) in progress.iter().zip([0.5, 0.25]) {
            assert!((fill.1 - 0.22).abs() < 0.001);
            let bounds = extent(&fill.2);
            let node = layout
                .nodes
                .iter()
                .find(|node| {
                    (f64::from(node.bounds.y) - bounds.1 - f64::from(scene.bounds.y)).abs() < 0.002
                })
                .expect("progress must cover its own bar");
            assert!((bounds.2 / f64::from(node.bounds.width) - fraction).abs() < 0.002);
        }
        assert!(
            ctx.operations().iter().any(|operation| {
                matches!(operation, DrawOperation::SetStrokeStyle(color) if color == "#dc2626")
            }),
            "completed critical tasks retain their critical border"
        );
    }

    #[test]
    fn canvas_scene_draws_xychart_data_and_matches_direct_axis_labels() {
        fn text_operations(ctx: &MockCanvas2dContext) -> Vec<(String, f64, f64, TextBaseline)> {
            let mut baseline = TextBaseline::Alphabetic;
            let mut labels = Vec::new();
            for operation in ctx.operations() {
                match operation {
                    DrawOperation::SetTextBaseline(value) => baseline = *value,
                    DrawOperation::FillText(text, x, y) => {
                        labels.push((text.clone(), *x, *y, baseline))
                    }
                    _ => {}
                }
            }
            labels
        }

        for horizontal in [false, true] {
            let source = format!(
                "xychart-beta{}\n title Quarterly movement\n\
                 x-axis Quarter [Q1, Q2, Q3]\n y-axis Dollars -10 --> 20\n\
                 bar [-10, 0, 20]\n line [0, 10, -10]\n area [-5, 15, -5]\n",
                if horizontal { " horizontal" } else { "" },
            );
            let ir = fm_parser::parse(&source).ir;
            let layout = layout_diagram(&ir);
            let scene = build_render_scene(&ir, &layout);
            let config = CanvasRenderConfig {
                auto_fit: false,
                padding: 0.0,
                ..Default::default()
            };
            let mut direct_ctx = MockCanvas2dContext::new(1000.0, 800.0);
            let direct =
                crate::render_to_canvas_with_layout(&ir, &layout, &mut direct_ctx, &config);
            let mut scene_ctx = MockCanvas2dContext::new(1000.0, 800.0);
            let result = crate::render_scene_to_canvas(&scene, &mut scene_ctx, &config);
            assert_eq!(result.nodes_drawn, 9);
            assert_eq!(result.edges_drawn, 4);
            assert_eq!(result.nodes_drawn, direct.nodes_drawn);
            assert_eq!(result.edges_drawn, direct.edges_drawn);
            let direct_labels = text_operations(&direct_ctx);
            let scene_labels = text_operations(&scene_ctx);
            assert_eq!(scene_labels.len(), direct_labels.len());
            for ((direct_text, dx, dy, direct_baseline), (scene_text, sx, sy, scene_baseline)) in
                direct_labels.iter().zip(&scene_labels)
            {
                assert_eq!(scene_text, direct_text);
                // The scene can expand its clip for long labels. Compare in original layout
                // coordinates so this test also catches a second origin translation.
                assert!(
                    (sx + f64::from(scene.bounds.x) - dx - f64::from(layout.bounds.x)).abs()
                        < 0.002,
                    "x for {scene_text}"
                );
                assert!(
                    (sy + f64::from(scene.bounds.y) - dy - f64::from(layout.bounds.y)).abs()
                        < 0.002,
                    "y for {scene_text}"
                );
                assert_eq!(scene_baseline, direct_baseline);
            }
            let mut alpha = 1.0_f64;
            let mut area_fills = 0;
            let mut bar_fills = 0;
            for operation in scene_ctx.operations() {
                match operation {
                    DrawOperation::SetGlobalAlpha(value) => alpha = *value,
                    DrawOperation::Fill if (alpha - 0.16).abs() < 0.0001 => area_fills += 1,
                    DrawOperation::Fill if (alpha - 0.78).abs() < 0.0001 => bar_fills += 1,
                    _ => {}
                }
            }
            assert_eq!(
                area_fills, 1,
                "area series must paint its quantitative extent"
            );
            assert_eq!(
                bar_fills, 3,
                "each bar must retain its own filled rectangle"
            );
            assert!(scene_ctx.operations().iter().any(|operation| {
                matches!(operation, DrawOperation::SetLineDash(dash) if dash == &[4.0, 4.0])
            }));
        }
    }

    #[test]
    fn canvas_scene_draws_sequence_geometry_and_notes_in_layout_coordinates() {
        let ir = fm_parser::parse(
            "%%{init: {\"sequence\": {\"mirrorActors\": true}}}%%\n\
             sequenceDiagram\n\
             box rgb(230,240,250) Workers\n\
             participant Alice\n\
             participant Bob\n\
             end\n\
             rect rgb(250,240,220)\n\
             Alice->>+Bob: request\n\
             alt allowed\n\
             Note right of Bob: Check<br/>Audit\n\
             Bob-->>Alice: accept\n\
             else denied\n\
             Note over Alice,Bob: wait\n\
             end\n\
             Bob-->>-Alice: done\n\
             end\n\
             destroy Bob\n\
             Bob->>Alice: bye\n",
        )
        .ir;
        let layout = layout_diagram(&ir);
        assert_eq!(layout.extensions.sequence_notes.len(), 2);
        assert_eq!(layout.extensions.activation_bars.len(), 1);
        assert_eq!(layout.extensions.sequence_lifecycle_markers.len(), 1);
        assert_eq!(
            layout.extensions.sequence_mirror_headers.len(),
            ir.nodes.len(),
            "the fixture must reserve one footer header per participant"
        );
        let scene = build_render_scene(&ir, &layout);
        let config = CanvasRenderConfig {
            auto_fit: false,
            padding: 0.0,
            ..Default::default()
        };
        let mut ctx = MockCanvas2dContext::new(1200.0, 1600.0);
        let result = crate::render_scene_to_canvas(&scene, &mut ctx, &config);
        let operations = ctx.operations();
        let has_segment = |x0: f32, y0: f32, x1: f32, y1: f32| {
            let expected = [
                f64::from(x0 - layout.bounds.x),
                f64::from(y0 - layout.bounds.y),
                f64::from(x1 - layout.bounds.x),
                f64::from(y1 - layout.bounds.y),
            ];
            operations
                .windows(2)
                .any(|pair| match (&pair[0], &pair[1]) {
                    (DrawOperation::MoveTo(a, b), DrawOperation::LineTo(c, d))
                    | (DrawOperation::LineTo(a, b), DrawOperation::LineTo(c, d)) => {
                        [*a, *b, *c, *d]
                            .iter()
                            .zip(expected)
                            .all(|(actual, expected)| (actual - expected).abs() < 0.001)
                    }
                    _ => false,
                })
        };
        for band in &layout.extensions.bands {
            let x = band.bounds.center().x;
            assert!(has_segment(
                x,
                band.bounds.y,
                x,
                band.bounds.y + band.bounds.height
            ));
        }
        let activation = &layout.extensions.activation_bars[0].bounds;
        assert!(has_segment(
            activation.x,
            activation.y,
            activation.x + activation.width,
            activation.y,
        ));
        assert!(has_segment(
            activation.x + activation.width,
            activation.y,
            activation.x + activation.width,
            activation.y + activation.height,
        ));
        let expected_corners = [
            (activation.x, activation.y),
            (activation.x + activation.width, activation.y),
            (
                activation.x + activation.width,
                activation.y + activation.height,
            ),
            (activation.x, activation.y + activation.height),
        ]
        .map(|(x, y)| {
            (
                f64::from(x - layout.bounds.x),
                f64::from(y - layout.bounds.y),
            )
        });
        assert!(
            operations.windows(5).any(|window| {
                let corners = match window {
                    [
                        DrawOperation::MoveTo(x0, y0),
                        DrawOperation::LineTo(x1, y1),
                        DrawOperation::LineTo(x2, y2),
                        DrawOperation::LineTo(x3, y3),
                        DrawOperation::ClosePath,
                    ] => [(*x0, *y0), (*x1, *y1), (*x2, *y2), (*x3, *y3)],
                    _ => return false,
                };
                corners
                    .iter()
                    .zip(expected_corners)
                    .all(|(&(x, y), (expected_x, expected_y))| {
                        (x - expected_x).abs() < 0.001 && (y - expected_y).abs() < 0.001
                    })
            }),
            "the activation must trace all four square corners and close its left edge"
        );
        let alternative = layout
            .extensions
            .sequence_fragments
            .iter()
            .find(|fragment| fragment.kind == fm_core::FragmentKind::Alt)
            .expect("alt geometry");
        let divider_y = alternative.branch_tops[0];
        assert!(
            has_segment(
                alternative.bounds.x,
                divider_y,
                alternative.bounds.x + alternative.bounds.width,
                divider_y,
            ),
            "the note-only branch retains its own divider row"
        );
        let destroyed = &layout.extensions.sequence_lifecycle_markers[0];
        let half = destroyed.size / 2.0;
        assert!(has_segment(
            destroyed.center.x - half,
            destroyed.center.y - half,
            destroyed.center.x + half,
            destroyed.center.y + half,
        ));
        assert!(has_segment(
            destroyed.center.x + half,
            destroyed.center.y - half,
            destroyed.center.x - half,
            destroyed.center.y + half,
        ));

        let labels: Vec<&str> = operations
            .iter()
            .filter_map(|operation| match operation {
                DrawOperation::FillText(text, _, _) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        for label in [
            "Workers",
            "alt",
            "[allowed]",
            "[denied]",
            "Check",
            "Audit",
            "wait",
        ] {
            assert_eq!(
                labels.iter().filter(|drawn| **drawn == label).count(),
                1,
                "{label}"
            );
        }
        for participant in ["Alice", "Bob"] {
            assert_eq!(
                labels.iter().filter(|drawn| **drawn == participant).count(),
                2
            );
        }
        let background = operations.iter().position(|operation| {
            matches!(operation, DrawOperation::SetFillStyle(color) if color == "rgb(250,240,220)")
        }).expect("author's background color");
        let lifeline = operations.iter().position(|operation| {
            matches!(operation, DrawOperation::SetLineDash(dash) if dash == &[6.0, 4.0])
        }).expect("dashed lifeline");
        let note = operations.iter().position(|operation| {
            matches!(operation, DrawOperation::FillText(text, _, _) if text == "Check")
        }).expect("note body");
        assert!(background < lifeline && lifeline < note);
        assert_eq!(result.nodes_drawn, ir.nodes.len());
        assert_eq!(result.edges_drawn, ir.edges.len());
        assert_eq!(result.clusters_drawn, 1);
    }

    #[test]
    fn canvas_scene_keeps_single_headers_when_sequence_mirroring_is_disabled() {
        for directive in [
            "",
            "%%{init: {\"sequence\": {\"mirrorActors\": false}}}%%\n",
        ] {
            let ir = fm_parser::parse(&format!(
                "{directive}sequenceDiagram\nparticipant Alice\nparticipant Bob\nAlice->>Bob: hello\n"
            ))
            .ir;
            let layout = layout_diagram(&ir);
            assert!(layout.extensions.sequence_mirror_headers.is_empty());
            let scene = build_render_scene(&ir, &layout);
            let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
            let result =
                crate::render_scene_to_canvas(&scene, &mut ctx, &CanvasRenderConfig::default());
            for participant in ["Alice", "Bob"] {
                assert_eq!(
                    ctx.operations()
                        .iter()
                        .filter(|operation| {
                            matches!(operation, DrawOperation::FillText(text, _, _) if text == participant)
                        })
                        .count(),
                    1,
                    "{participant} must have one header with directive {directive:?}"
                );
            }
            assert_eq!(result.nodes_drawn, ir.nodes.len());
        }
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
    fn generic_class_headers_reach_canvas_with_and_without_members() {
        for members in ["", "  A : +go()\n"] {
            let source = format!("classDiagram\n  class A~T~[\"Repository\"]\n{members}");
            let ir = fm_parser::parse(&source).ir;
            let meta = ir.nodes[0]
                .class_meta
                .as_deref()
                .expect("generic class metadata");
            assert_eq!(meta.generics, ["T"]);
            assert_eq!(meta.methods.len(), usize::from(!members.is_empty()));
            let layout = layout_diagram(&ir);

            for use_scene in [false, true] {
                let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
                let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
                if use_scene {
                    renderer.render_scene(&build_render_scene(&ir, &layout), &mut ctx);
                } else {
                    renderer.render(&layout, &ir, &mut ctx);
                }
                assert!(
                    ctx.operations().iter().any(
                        |operation| matches!(operation, DrawOperation::FillText(text, _, _)
                            if text == "Repository<T>")
                    ),
                    "canvas (scene: {use_scene}) lost the complete generic heading"
                );
                if !members.is_empty() {
                    assert!(ctx.operations().iter().any(
                        |operation| matches!(operation, DrawOperation::FillText(text, _, _)
                            if text == "+go()")
                    ));
                }
            }
        }
    }

    #[test]
    fn measured_sequence_captions_share_geometry_across_graphical_backends() {
        let source = "sequenceDiagram\nparticipant Client\nparticipant Server\nClient->>Server: Authenticate the incoming request with a signed identity token that includes the resource owner and permission scope before continuing\nServer->>Server: persist session<br/>write audit record<br/>return result\nServer-->>Client: complete\n";
        let ir = fm_parser::parse(source).ir;
        for font_size in [15.0, 28.0] {
            let config = CanvasRenderConfig {
                font_size,
                line_height: 1.5,
                padding: 0.0,
                auto_fit: false,
                ..Default::default()
            };
            let metrics = config.font_metrics();
            let layout = fm_layout::layout_diagram_with_config(
                &ir,
                fm_layout::LayoutConfig {
                    font_metrics: Some(metrics.clone()),
                    ..Default::default()
                },
            );
            let messages = &layout.extensions.sequence_messages;
            assert_eq!(messages.len(), 3);
            assert!(messages[0].text.contains('\n'), "long captions must wrap");
            let scene = fm_layout::build_render_scene_with_metrics(&ir, &layout, &metrics);
            for use_scene in [false, true] {
                let mut ctx = MockCanvas2dContext::new(2000.0, 2000.0);
                let mut renderer = Canvas2dRenderer::new(config.clone());
                if use_scene {
                    renderer.render_scene(&scene, &mut ctx);
                } else {
                    renderer.render(&layout, &ir, &mut ctx);
                }
                for message in messages {
                    for (row, text) in message.text.split('\n').enumerate() {
                        let center = message.line_center(row);
                        let (offset_x, offset_y) = if use_scene {
                            (-scene.bounds.x, -scene.bounds.y)
                        } else {
                            (-layout.bounds.x, -layout.bounds.y)
                        };
                        let mut font = String::new();
                        let drawn: Vec<_> = ctx
                            .operations()
                            .iter()
                            .filter_map(|op| match op {
                                DrawOperation::SetFont(value) => {
                                    font.clone_from(value);
                                    None
                                }
                                DrawOperation::FillText(value, x, y) if value == text => {
                                    Some((*x, *y, font.clone()))
                                }
                                _ => None,
                            })
                            .collect();
                        assert_eq!(drawn.len(), 1, "scene={use_scene}, row={text}");
                        assert!((drawn[0].0 - f64::from(center.x + offset_x)).abs() < 0.01);
                        assert!((drawn[0].1 - f64::from(center.y + offset_y)).abs() < 0.01);
                        let size = drawn[0]
                            .2
                            .split_whitespace()
                            .find_map(|part| {
                                part.strip_suffix("px")
                                    .and_then(|size| size.parse::<f32>().ok())
                            })
                            .unwrap();
                        assert!((size - message.font_size).abs() < 0.01);
                    }
                }
            }
            let plan =
                crate::gpu_plan::GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
            for message in messages {
                let runs: Vec<_> = plan
                    .text_runs
                    .iter()
                    .filter(|run| {
                        run.source == crate::gpu_plan::GpuTextSource::Edge
                            && run.node_index as usize == message.edge_index
                    })
                    .collect();
                let rows: Vec<_> = message
                    .text
                    .split('\n')
                    .enumerate()
                    .filter(|(_, line)| !line.is_empty())
                    .collect();
                assert_eq!(
                    runs.len(),
                    rows.len(),
                    "every visible message row needs a GPU run"
                );
                for (run, (row, text)) in runs.iter().zip(rows) {
                    let quads = &plan.text_quads
                        [run.first_quad as usize..(run.first_quad + run.quad_count) as usize];
                    assert_eq!(
                        quads.len(),
                        text.chars().filter(|ch| !ch.is_control()).count()
                    );
                    assert!(quads.iter().all(|quad| {
                        (quad.half_extent[1] * 2.0 - message.font_size).abs() < 0.01
                            && (quad.center[1] - message.line_center(row).y).abs() < 0.01
                    }));
                }
            }
            for backend in [
                fm_render_svg::SvgBackend::LegacyLayout,
                fm_render_svg::SvgBackend::Scene,
            ] {
                let svg_config = fm_render_svg::SvgRenderConfig {
                    backend,
                    font_size: font_size as f32,
                    line_height: 1.5,
                    ..Default::default()
                };
                let svg = fm_render_svg::render_svg_with_layout(&ir, &layout, &svg_config);
                for message in messages {
                    for line in message.text.split('\n') {
                        assert!(
                            svg.contains(&format!(">{line}<")),
                            "{backend:?}: lost caption row {line}"
                        );
                    }
                }
                assert_eq!(
                    svg,
                    fm_render_svg::render_svg_with_layout(&ir, &layout, &svg_config)
                );
            }
        }
    }

    #[test]
    fn c4_card_rows_share_svg_canvas_scene_and_gpu_geometry() {
        let description = "Stores customer profiles, billing preferences, and a complete audit trail for every account change.";
        for constructor in ["Container", "ContainerDb", "ContainerQueue"] {
            for (font_size, declared_font_size) in [
                (15.0, None),
                (26.0, None),
                (15.0, Some("30px")),
                (15.0, Some("900px")),
                (15.0, Some("24pt")),
            ] {
                let source = format!(
                    "C4Container\n{constructor}(db, \"Customer records\", \"Rust and PostgreSQL\", \"{description}\")"
                );
                let mut ir = fm_parser::parse(&source).ir;
                assert_eq!(ir.nodes.len(), 1);
                if let Some(size) = declared_font_size {
                    ir.nodes[0].inline_style = Some(Box::new(fm_core::IrInlineStyle::from_pairs(
                        [("font-size".to_owned(), size.to_owned())],
                    )));
                }
                let node = &ir.nodes[0];
                let config = CanvasRenderConfig {
                    font_size,
                    padding: 0.0,
                    auto_fit: false,
                    ..Default::default()
                };
                let metrics = config.font_metrics();
                let layout = fm_layout::layout_diagram_with_config(
                    &ir,
                    fm_layout::LayoutConfig {
                        font_metrics: Some(metrics.clone()),
                        ..Default::default()
                    },
                );
                let bounds = layout.nodes[0].bounds;
                let card = fm_core::C4CardLayout::new(
                    node.c4_meta.as_deref().unwrap(),
                    "Customer records",
                    node.shape,
                    &metrics,
                    Some(bounds.width),
                    declared_font_size,
                );
                let name_font = card
                    .rows
                    .iter()
                    .find(|row| row.kind == fm_core::C4CardRowKind::Name)
                    .unwrap()
                    .font_size;
                assert!(bounds.width <= name_font * 21.0);
                assert!(card.rows.len() >= 6, "a real wrapped paragraph is required");
                let scene = fm_layout::build_render_scene_with_metrics(&ir, &layout, &metrics);
                for use_scene in [false, true] {
                    let mut ctx = MockCanvas2dContext::new(2000.0, 2000.0);
                    let mut renderer = Canvas2dRenderer::new(config.clone());
                    let (offset_x, offset_y) = if use_scene {
                        renderer.render_scene(&scene, &mut ctx);
                        (-scene.bounds.x, -scene.bounds.y)
                    } else {
                        renderer.render(&layout, &ir, &mut ctx);
                        (-layout.bounds.x, -layout.bounds.y)
                    };
                    let mut font = String::new();
                    let drawn: Vec<_> = ctx
                        .operations()
                        .iter()
                        .filter_map(|op| match op {
                            DrawOperation::SetFont(value) => {
                                font.clone_from(value);
                                None
                            }
                            DrawOperation::FillText(text, x, y) => {
                                Some((text.as_str(), *x, *y, font.clone()))
                            }
                            _ => None,
                        })
                        .collect();
                    assert_eq!(
                        drawn.len(),
                        card.rows.len(),
                        "scene={use_scene}: dropped C4 metadata"
                    );
                    for ((text, x, y, font), row) in drawn.iter().zip(&card.rows) {
                        assert_eq!(*text, row.text);
                        assert!(
                            (*x - f64::from(bounds.x + bounds.width * 0.5 + offset_x)).abs() < 0.01
                        );
                        assert!((*y - f64::from(bounds.y + row.center_y + offset_y)).abs() < 0.01);
                        let size = font
                            .split_whitespace()
                            .find_map(|part| part.strip_suffix("px")?.parse::<f32>().ok())
                            .unwrap();
                        assert!((size - row.font_size).abs() < 0.01);
                        let local_y = *y as f32 - bounds.y - offset_y;
                        let (top, bottom) = if constructor == "ContainerDb" {
                            (bounds.height * 0.2, bounds.height * 0.9)
                        } else {
                            (0.0, bounds.height)
                        };
                        assert!(local_y - size * 0.5 > top && local_y + size * 0.5 < bottom);
                    }
                    let paragraph = drawn
                        .iter()
                        .zip(&card.rows)
                        .filter(|(_, row)| row.kind == fm_core::C4CardRowKind::Description)
                        .map(|(drawn, _)| drawn.0)
                        .collect::<Vec<_>>()
                        .join(" ");
                    assert_eq!(paragraph, description, "every authored word must survive");
                }
                let plan =
                    crate::gpu_plan::GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
                let runs: Vec<_> = plan
                    .text_runs
                    .iter()
                    .filter(|run| run.source == crate::gpu_plan::GpuTextSource::Node)
                    .collect();
                assert_eq!(runs.len(), card.rows.len(), "GPU omitted C4 metadata");
                for (run, row) in runs.iter().zip(&card.rows) {
                    let quads = &plan.text_quads
                        [run.first_quad as usize..(run.first_quad + run.quad_count) as usize];
                    assert_eq!(
                        quads.len(),
                        row.text.chars().filter(|ch| !ch.is_control()).count()
                    );
                    assert!(
                        quads
                            .iter()
                            .all(|quad| (quad.center[1] - bounds.y - row.center_y).abs() < 0.01)
                    );
                }
                for backend in [
                    fm_render_svg::SvgBackend::LegacyLayout,
                    fm_render_svg::SvgBackend::Scene,
                ] {
                    let svg = fm_render_svg::render_svg_with_layout(
                        &ir,
                        &layout,
                        &fm_render_svg::SvgRenderConfig {
                            font_size: font_size as f32,
                            backend,
                            ..Default::default()
                        },
                    );
                    for row in &card.rows {
                        let escaped = row.text.replace('&', "&amp;").replace('<', "&lt;");
                        assert!(
                            svg.contains(&format!(">{escaped}<")),
                            "{backend:?}: missing C4 row {}",
                            row.text
                        );
                        if declared_font_size.is_some() {
                            assert!(
                                svg.contains(&format!("font-size:{}px!important", row.font_size)),
                                "{backend:?}: CSS must use the measured row size"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn er_card_cells_share_svg_canvas_scene_and_gpu_geometry() {
        for (count, font_size, line_height, declared_font_size) in [
            (12, 15.0, 1.5, None),
            (40, 15.0, 1.5, None),
            (12, 24.0, 1.8, None),
            (12, 15.0, 1.5, Some("30px")),
            (12, 15.0, 1.5, Some("900px")),
            (12, 15.0, 1.5, Some("24pt")),
        ] {
            let mut source =
                "erDiagram\nACCOUNT {\n uuid account_id PK,FK \"Ledger key\"\n".to_owned();
            for index in 1..count {
                source.push_str(&format!(" string field_{index}"));
                if index == count - 1 {
                    source.push_str(" \"Final attribute must remain visible\"");
                }
                source.push('\n');
            }
            source.push_str("}\n");
            let mut ir = fm_parser::parse(&source).ir;
            assert_eq!(ir.nodes.len(), 1);
            assert_eq!(ir.nodes[0].members.len(), count);
            assert_eq!(ir.nodes[0].members[0].key_cell(), "PK,FK");
            if count == 40 {
                ir.nodes[0].members[5].comment =
                    Some("First note line\n\nLast note line".to_owned());
            }
            if let Some(size) = declared_font_size {
                ir.nodes[0].inline_style = Some(Box::new(fm_core::IrInlineStyle::from_pairs([(
                    "font-size".to_owned(),
                    size.to_owned(),
                )])));
            }
            let node = &ir.nodes[0];
            // Derive the content oracle from parsed fields, independently of the shared card model.
            let mut expected = vec!["ACCOUNT".to_owned()];
            for attribute in &node.members {
                let key = attribute.key_cell();
                for cell in [
                    attribute.data_type.as_str(),
                    attribute.name.as_str(),
                    key.as_ref(),
                    attribute.comment.as_deref().unwrap_or(""),
                ] {
                    expected.extend(
                        cell.lines()
                            .filter(|line| !line.is_empty())
                            .map(str::to_owned),
                    );
                }
            }
            expected.sort();
            let config = CanvasRenderConfig {
                font_size,
                line_height,
                padding: 0.0,
                auto_fit: false,
                ..Default::default()
            };
            let metrics = config.font_metrics();
            let layout = fm_layout::layout_diagram_with_config(
                &ir,
                fm_layout::LayoutConfig {
                    font_metrics: Some(metrics.clone()),
                    ..Default::default()
                },
            );
            let bounds = layout.nodes[0].bounds;
            let card = fm_core::ErCardLayout::new(
                &node.members,
                "ACCOUNT",
                &metrics,
                declared_font_size,
                fm_core::ErCardLayout::MIN_ATTRIBUTE_FONT_SIZE,
            );
            let mut measured: Vec<_> = card.text.iter().map(|run| run.text.clone()).collect();
            measured.sort();
            assert_eq!(
                measured, expected,
                "measurement omitted authored ER content"
            );
            let scene = fm_layout::build_render_scene_with_metrics(&ir, &layout, &metrics);
            for use_scene in [false, true] {
                let mut ctx = MockCanvas2dContext::new(3000.0, 3000.0);
                let mut renderer = Canvas2dRenderer::new(config.clone());
                let (offset_x, offset_y) = if use_scene {
                    renderer.render_scene(&scene, &mut ctx);
                    (-scene.bounds.x, -scene.bounds.y)
                } else {
                    renderer.render(&layout, &ir, &mut ctx);
                    (-layout.bounds.x, -layout.bounds.y)
                };
                let mut font = String::new();
                let drawn: Vec<_> = ctx
                    .operations()
                    .iter()
                    .filter_map(|operation| match operation {
                        DrawOperation::SetFont(value) => {
                            font.clone_from(value);
                            None
                        }
                        DrawOperation::FillText(text, x, y) => Some((text, *x, *y, font.clone())),
                        _ => None,
                    })
                    .collect();
                let mut content: Vec<_> = drawn
                    .iter()
                    .map(|(text, _, _, _)| (*text).clone())
                    .collect();
                content.sort();
                assert_eq!(
                    content, expected,
                    "scene={use_scene}, {count} attributes: lost table cells"
                );
                for ((text, x, y, font), run) in drawn.iter().zip(&card.text) {
                    assert_eq!(*text, &run.text);
                    let expected_x = bounds.x
                        + run.column.map_or(bounds.width * 0.5, |column| {
                            fm_core::ErCardLayout::PADDING_X + card.column_offsets[column]
                        });
                    assert!((*x - f64::from(expected_x + offset_x)).abs() < 0.01);
                    assert!((*y - f64::from(bounds.y + run.center_y + offset_y)).abs() < 0.01);
                    let size = font
                        .split_whitespace()
                        .find_map(|part| part.strip_suffix("px")?.parse::<f32>().ok())
                        .unwrap();
                    assert!((size - run.font_size).abs() < 0.01);
                    let local_y = *y as f32 - bounds.y - offset_y;
                    assert!(local_y - size * 0.5 > 0.0 && local_y + size * 0.5 < bounds.height);
                    if run.column.is_some() {
                        assert!(local_y - size * 0.5 > card.header_height);
                    } else {
                        assert!(local_y + size * 0.5 < card.header_height);
                    }
                }
            }
            let plan =
                crate::gpu_plan::GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
            let runs: Vec<_> = plan
                .text_runs
                .iter()
                .filter(|run| run.source == crate::gpu_plan::GpuTextSource::Node)
                .collect();
            assert_eq!(runs.len(), expected.len(), "GPU omitted ER cells");
            for (run, cell) in runs.iter().zip(&card.text) {
                let quads = &plan.text_quads
                    [run.first_quad as usize..(run.first_quad + run.quad_count) as usize];
                let decoded: String = quads
                    .iter()
                    .map(|quad| {
                        plan.glyph_atlas
                            .cells
                            .iter()
                            .find(|glyph| {
                                glyph.uv_min == quad.uv_min && glyph.uv_max == quad.uv_max
                            })
                            .expect("every emitted glyph must have an atlas entry")
                            .glyph
                    })
                    .collect();
                assert_eq!(
                    decoded, cell.text,
                    "GPU lost or substituted an attribute glyph"
                );
                assert!(quads.iter().all(|quad| {
                    (quad.center[1] - bounds.y - cell.center_y).abs() < 0.01
                        && (quad.half_extent[1] * 2.0 - cell.font_size).abs() < 0.01
                }));
            }
            assert!(
                plan.node_border_segments.iter().any(|segment| {
                    (segment.from[1] - bounds.y - card.header_height).abs() < 0.01
                        && (segment.to[1] - segment.from[1]).abs() < 0.01
                }),
                "GPU omitted the entity header separator"
            );
            for backend in [
                fm_render_svg::SvgBackend::LegacyLayout,
                fm_render_svg::SvgBackend::Scene,
            ] {
                let svg_config = fm_render_svg::SvgRenderConfig {
                    backend,
                    font_size: font_size as f32,
                    line_height: line_height as f32,
                    ..Default::default()
                };
                let svg = fm_render_svg::render_svg_with_layout(&ir, &layout, &svg_config);
                // Legacy SVG translates into a padded zero-origin document. Scene SVG keeps
                // layout coordinates and expresses the same padding in its viewBox instead.
                let (svg_offset_x, svg_offset_y) = match backend {
                    fm_render_svg::SvgBackend::LegacyLayout => (
                        svg_config.padding - layout.bounds.x,
                        svg_config.padding - layout.bounds.y,
                    ),
                    fm_render_svg::SvgBackend::Scene => (0.0, 0.0),
                };
                let drawn: Vec<_> = svg
                    .split("<text ")
                    .skip(1)
                    .map(|chunk| {
                        let (attributes, tail) = chunk.split_once('>').unwrap();
                        let text = tail.split_once("</text>").unwrap().0;
                        (attributes, text)
                    })
                    .collect();
                let mut content: Vec<_> =
                    drawn.iter().map(|(_, text)| (*text).to_owned()).collect();
                content.sort();
                assert_eq!(content, expected, "{backend:?}: SVG lost ER cells");
                let number = |attributes: &str, name: &str| -> f32 {
                    attributes
                        .split(&format!("{name}=\""))
                        .nth(1)
                        .and_then(|value| value.split('"').next())
                        .unwrap()
                        .parse()
                        .unwrap()
                };
                for ((attributes, text), run) in drawn.iter().zip(&card.text) {
                    assert_eq!(*text, run.text);
                    let expected_x = bounds.x
                        + svg_offset_x
                        + run.column.map_or(bounds.width * 0.5, |column| {
                            fm_core::ErCardLayout::PADDING_X + card.column_offsets[column]
                        });
                    let expected_y = bounds.y + run.center_y + svg_offset_y;
                    let actual_x = number(attributes, "x");
                    let actual_y = number(attributes, "y");
                    let actual_size = number(attributes, "font-size");
                    assert!(
                        (actual_x - expected_x).abs() < 0.1,
                        "{backend:?}, {count} rows, font={font_size}, declared={declared_font_size:?}, {text:?}: x={actual_x}, expected {expected_x}"
                    );
                    assert!(
                        (actual_y - expected_y).abs() < 0.1,
                        "{backend:?}, {count} rows, font={font_size}, declared={declared_font_size:?}, {text:?}: y={actual_y}, expected {expected_y}"
                    );
                    assert!(
                        (actual_size - run.font_size).abs() < 0.01,
                        "{backend:?}, {count} rows, {text:?}: font-size={actual_size}, expected {}",
                        run.font_size
                    );
                    if declared_font_size.is_some() {
                        assert!(
                            attributes
                                .contains(&format!("font-size:{}px!important", run.font_size)),
                            "{backend:?}: CSS must retain the measured font size"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn class_card_rows_share_canvas_scene_and_gpu_geometry() {
        let source = "classDiagram\nclass Repository~T~ {\n<<interface>>\n+String identifier\n+fetch() List~T~\n+save()$\n}\n";
        for (font_size, line_height, style) in [
            (15.0, 1.5, ""),
            (24.0, 1.8, ""),
            (
                15.0,
                1.5,
                "classDef default font-size:24px\nstyle Repository font-size:40px\n",
            ),
        ] {
            let ir = fm_parser::parse(&format!("{source}{style}")).ir;
            let node = &ir.nodes[0];
            let meta = node.class_meta.as_deref().expect("class metadata");
            assert_eq!(meta.attributes.len(), 1);
            assert_eq!(meta.methods.len(), 2);
            let config = CanvasRenderConfig {
                font_size,
                line_height,
                padding: 0.0,
                auto_fit: false,
                ..Default::default()
            };
            let metrics = config.font_metrics();
            let layout = fm_layout::layout_diagram_with_config(
                &ir,
                fm_layout::LayoutConfig {
                    font_metrics: Some(metrics.clone()),
                    ..Default::default()
                },
            );
            let bounds = layout.nodes[0].bounds;
            let name = node
                .label
                .and_then(|id| ir.labels.get(id.0))
                .map_or(node.id.as_str(), |label| label.text.as_str());
            let declared = ir.resolved_node_style(0);
            let card = fm_core::ClassCompartmentLayout::with_font_size_override(
                meta,
                name,
                font_size as f32,
                line_height as f32,
                declared.get("font-size").map(String::as_str),
            );
            if !style.is_empty() {
                assert!(card.rows.iter().all(|row| row.font_size == 40.0));
                for rows in card.rows.windows(2) {
                    assert!(rows[0].center_y + 20.0 < rows[1].center_y - 20.0);
                }
            }
            let scene = fm_layout::build_render_scene_with_metrics(&ir, &layout, &metrics);
            for use_scene in [false, true] {
                let mut ctx = MockCanvas2dContext::new(1600.0, 1200.0);
                let mut renderer = Canvas2dRenderer::new(config.clone());
                if use_scene {
                    renderer.render_scene(&scene, &mut ctx);
                } else {
                    renderer.render(&layout, &ir, &mut ctx);
                }
                for row in &card.rows {
                    let mut font = String::new();
                    let positions: Vec<_> = ctx
                        .operations()
                        .iter()
                        .filter_map(|operation| match operation {
                            DrawOperation::SetFont(value) => {
                                font.clone_from(value);
                                None
                            }
                            DrawOperation::FillText(text, x, y) if text == &row.text => {
                                Some((*x, *y, font.clone()))
                            }
                            _ => None,
                        })
                        .collect();
                    assert_eq!(
                        positions.len(),
                        1,
                        "scene={use_scene}: missing or duplicated {}",
                        row.text
                    );
                    let expected_x = if row.centered() {
                        bounds.x + bounds.width * 0.5
                    } else {
                        bounds.x + fm_core::ClassCompartmentLayout::PADDING_X
                    };
                    let offset_x = if use_scene {
                        -scene.bounds.x
                    } else {
                        -layout.bounds.x
                    };
                    let offset_y = if use_scene {
                        -scene.bounds.y
                    } else {
                        -layout.bounds.y
                    };
                    assert!((positions[0].0 - f64::from(expected_x + offset_x)).abs() < 0.01);
                    assert!(
                        (positions[0].1 - f64::from(bounds.y + row.center_y + offset_y)).abs()
                            < 0.01
                    );
                    let rendered_font = positions[0]
                        .2
                        .split_whitespace()
                        .find_map(|part| {
                            part.strip_suffix("px")
                                .and_then(|size| size.parse::<f64>().ok())
                        })
                        .expect("canvas row font size");
                    assert!(
                        (rendered_font - f64::from(row.font_size)).abs() < 0.01,
                        "scene={use_scene}: {} uses the wrong font: {}",
                        row.text,
                        positions[0].2
                    );
                }
            }
            let plan =
                crate::gpu_plan::GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
            let runs: Vec<_> = plan
                .text_runs
                .iter()
                .filter(|run| run.source == crate::gpu_plan::GpuTextSource::Node)
                .collect();
            assert_eq!(runs.len(), card.rows.len());
            for (run, row) in runs.into_iter().zip(&card.rows) {
                let quads = &plan.text_quads
                    [run.first_quad as usize..(run.first_quad + run.quad_count) as usize];
                assert_eq!(quads.len(), row.text.chars().count());
                assert!(
                    quads
                        .iter()
                        .all(|quad| (quad.center[1] - bounds.y - row.center_y).abs() < 0.01)
                );
                assert!(
                    quads
                        .iter()
                        .all(|quad| (quad.half_extent[1] * 2.0 - row.font_size).abs() < 0.01)
                );
                assert!(
                    quads
                        .iter()
                        .all(|quad| quad.center[1] - quad.half_extent[1] >= bounds.y
                            && quad.center[1] + quad.half_extent[1] <= bounds.y + bounds.height)
                );
            }
            assert!(
                plan.node_border_segments.len() >= 3,
                "two dividers and static underline must be drawn"
            );
            assert_eq!(
                plan.node_instances.len(),
                2,
                "class body and header must both be drawn"
            );
        }
    }

    #[test]
    fn render_draws_generic_diagram_title() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.meta.title = Some("Shipping History".to_string());
        // A flowchart draws its title only with front-matter provenance -- mermaid ignores a
        // `title` statement there -- so this is the state the parser actually produces for a titled
        // flowchart. See `MermaidDiagramIr::declared_title_if_drawn`.
        ir.meta.title_from_front_matter = true;
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

        let label_background_width = drawn_edge_plate(&ctx, "Wide label text").2;

        let expected_width = "Wide label text".len() as f64
            * (CanvasRenderConfig::default().font_size * 0.85 * 0.57)
            + 8.0;
        assert!((label_background_width - expected_width).abs() < 1.0);
    }

    fn crossing_label_fixture(diagram_type: DiagramType) -> (MermaidDiagramIr, DiagramLayout) {
        let source = match diagram_type {
            DiagramType::Flowchart => {
                "flowchart LR\nA[Alpha] -->|crossing| B[Beta]\nC[Blocker]\nD[Top] -->|secondary| E[Bottom]"
            }
            DiagramType::Class => {
                "classDiagram\nclass A\nclass B\nclass C\nclass D\nclass E\nA --> B : crossing\nD --> E : secondary"
            }
            DiagramType::State => {
                "stateDiagram-v2\nA --> B : crossing\nstate \"Blocker\" as C\nD --> E : secondary"
            }
            DiagramType::Er => {
                "erDiagram\nA ||--|| B : crossing\nD ||--|| E : secondary\nC {\n string id\n}"
            }
            _ => unreachable!(),
        };
        let ir = fm_parser::parse(source).ir;
        assert_eq!(
            ir.edges.len(),
            2,
            "fixture parse failed for {diagram_type:?}"
        );
        let layout = crossing_label_layout(&ir);
        (ir, layout)
    }

    fn crossing_label_layout(ir: &MermaidDiagramIr) -> DiagramLayout {
        let mut layout = layout_diagram(ir);
        for node in &mut layout.nodes {
            let (x, y, width, height) = match ir.nodes[node.node_index].id.as_str() {
                "A" => (20.0, 120.0, 60.0, 40.0),
                "B" => (300.0, 120.0, 60.0, 40.0),
                "C" => (150.0, 100.0, 80.0, 80.0),
                "D" => (180.0, 20.0, 60.0, 40.0),
                "E" => (180.0, 240.0, 60.0, 40.0),
                id => panic!("unexpected fixture node {id}"),
            };
            node.bounds = LayoutRect {
                x,
                y,
                width,
                height,
            };
        }
        for edge in &mut layout.edges {
            let points = if edge.edge_index == 0 {
                [(80.0, 140.0), (300.0, 140.0)]
            } else {
                [(210.0, 60.0), (210.0, 240.0)]
            };
            edge.points = points
                .into_iter()
                .map(|(x, y)| fm_layout::LayoutPoint { x, y })
                .collect();
        }
        layout.bounds = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 300.0,
        };
        layout
    }

    fn drawn_edge_plate(ctx: &MockCanvas2dContext, label: &str) -> (f64, f64, f64, f64) {
        let text_index = ctx
            .operations()
            .iter()
            .position(|op| matches!(op, DrawOperation::FillText(text, _, _) if text == label))
            .unwrap_or_else(|| panic!("edge label {label} was not drawn"));
        painted_plate_before(ctx.operations(), text_index)
    }

    fn painted_plate_before(
        operations: &[DrawOperation],
        text_index: usize,
    ) -> (f64, f64, f64, f64) {
        let fill_index = operations[..text_index]
            .iter()
            .rposition(|operation| {
                matches!(operation, DrawOperation::Fill | DrawOperation::FillRect(..))
            })
            .expect("edge text needs its backing plate");
        if let DrawOperation::FillRect(x, y, width, height) = operations[fill_index] {
            return (x, y, width, height);
        }
        let path_start = operations[..fill_index]
            .iter()
            .rposition(|operation| matches!(operation, DrawOperation::BeginPath))
            .expect("filled plate must start a path");
        let mut points = Vec::new();
        for operation in &operations[path_start..fill_index] {
            match operation {
                DrawOperation::MoveTo(x, y) | DrawOperation::LineTo(x, y) => points.push((*x, *y)),
                DrawOperation::ArcTo(x1, y1, x2, y2, _) => points.extend([(*x1, *y1), (*x2, *y2)]),
                DrawOperation::Rect(x, y, width, height) => {
                    points.extend([(*x, *y), (*x + *width, *y + *height)])
                }
                _ => {}
            }
        }
        assert!(!points.is_empty(), "plate must have measurable geometry");
        let min_x = points
            .iter()
            .map(|point| point.0)
            .fold(f64::INFINITY, f64::min);
        let min_y = points
            .iter()
            .map(|point| point.1)
            .fold(f64::INFINITY, f64::min);
        let max_x = points
            .iter()
            .map(|point| point.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_y = points
            .iter()
            .map(|point| point.1)
            .fold(f64::NEG_INFINITY, f64::max);
        (min_x, min_y, max_x - min_x, max_y - min_y)
    }

    const CYCLIC_STATE_LABEL_REPRO: &str = concat!(
        "stateDiagram-v2\n[*] --> Idle\n",
        "Idle --> Starting: Space down / tap\nStarting --> Idle: closed\n",
        "Starting --> Recording: opened\nStarting --> Releasing: Space up, focus lost\n",
        "Recording --> Idle: closed\nRecording --> Stopping: Space up, focus lost / tap\n",
        "Releasing --> Idle: closed\nReleasing --> Stopping: opened / tap\n",
        "Stopping --> Settling: closed\nSettling --> Idle: quiet 1 s\n",
        "Settling --> Stopping: opened / tap\n",
    );

    #[test]
    fn reported_cyclic_state_labels_are_clear_across_graphical_renderers() {
        use crate::gpu_plan::rendered_label_gap;
        fn clear(a: LayoutRect, b: LayoutRect) -> bool {
            a.x + a.width <= b.x + 0.02
                || b.x + b.width <= a.x + 0.02
                || a.y + a.height <= b.y + 0.02
                || b.y + b.height <= a.y + 0.02
        }
        fn number(element: &str, attr: &str) -> f32 {
            element
                .split_once(&format!("{attr}=\""))
                .unwrap()
                .1
                .split_once('"')
                .unwrap()
                .0
                .parse()
                .unwrap()
        }
        fn svg_path(tag: &str) -> Vec<PathCmd> {
            let data = tag.split_once("d=\"").unwrap().1.split_once('"').unwrap().0;
            let mut separated = String::new();
            for character in data.chars() {
                if matches!(character, 'M' | 'L' | 'Q' | 'C' | 'Z') {
                    separated.push(' ');
                    separated.push(character);
                    separated.push(' ');
                } else {
                    separated.push(if character == ',' { ' ' } else { character });
                }
            }
            let mut tokens = separated.split_whitespace();
            let mut commands = Vec::new();
            while let Some(operation) = tokens.next() {
                let mut n = || tokens.next().unwrap().parse::<f32>().unwrap();
                commands.push(match operation {
                    "M" => PathCmd::MoveTo { x: n(), y: n() },
                    "L" => PathCmd::LineTo { x: n(), y: n() },
                    "Q" => PathCmd::QuadTo {
                        cx: n(),
                        cy: n(),
                        x: n(),
                        y: n(),
                    },
                    "C" => PathCmd::CubicTo {
                        c1x: n(),
                        c1y: n(),
                        c2x: n(),
                        c2y: n(),
                        x: n(),
                        y: n(),
                    },
                    "Z" => PathCmd::Close,
                    other => panic!("unexpected connector command {other}"),
                });
            }
            commands
        }
        fn scene_route(group: &RenderGroup, index: usize) -> Option<&fm_layout::RenderPath> {
            group.children.iter().find_map(|item| match item {
                RenderItem::Group(child) => scene_route(child, index),
                RenderItem::Path(path)
                    if path.source == RenderSource::Edge(index) && path.stroke.is_some() =>
                {
                    Some(path)
                }
                _ => None,
            })
        }
        fn scene_plates(group: &RenderGroup, plates: &mut Vec<LayoutRect>) {
            for item in &group.children {
                match item {
                    RenderItem::Group(child) => scene_plates(child, plates),
                    RenderItem::Path(path)
                        if matches!(path.source, RenderSource::Edge(_))
                            && path.stroke.is_none()
                            && path.fill.is_some() =>
                    {
                        let mut left = f32::INFINITY;
                        let mut top = f32::INFINITY;
                        let mut right = f32::NEG_INFINITY;
                        let mut bottom = f32::NEG_INFINITY;
                        for command in &path.commands {
                            let (PathCmd::MoveTo { x, y }
                            | PathCmd::LineTo { x, y }
                            | PathCmd::QuadTo { x, y, .. }
                            | PathCmd::CubicTo { x, y, .. }) = *command
                            else {
                                continue;
                            };
                            left = left.min(x);
                            right = right.max(x);
                            top = top.min(y);
                            bottom = bottom.max(y);
                        }
                        plates.push(LayoutRect {
                            x: left,
                            y: top,
                            width: right - left,
                            height: bottom - top,
                        });
                    }
                    _ => {}
                }
            }
        }
        for direction in [fm_core::GraphDirection::TB, fm_core::GraphDirection::LR] {
            for routing in [
                fm_layout::EdgeRouting::Orthogonal,
                fm_layout::EdgeRouting::Spline,
            ] {
                let mut ir = fm_parser::parse(CYCLIC_STATE_LABEL_REPRO).ir;
                ir.direction = direction;
                let layout = fm_layout::layout_diagram_with_config(
                    &ir,
                    fm_layout::LayoutConfig {
                        edge_routing: routing,
                        ..Default::default()
                    },
                );
                let check = |surface: &str,
                             plates: &[LayoutRect],
                             nodes: &[fm_layout::LayoutNodeBox]| {
                    assert_eq!(
                        plates.len(),
                        11,
                        "{surface}: all 55 label pairs must be checked"
                    );
                    for (index, plate) in plates.iter().enumerate() {
                        assert!(plate.width > 0.0 && plate.height > 0.0);
                        for previous in &plates[..index] {
                            assert!(
                                clear(*plate, *previous),
                                "{surface} {direction:?}/{routing:?}: overlapping {plate:?}, {previous:?}"
                            );
                        }
                        for node in nodes {
                            assert!(
                                clear(*plate, node.bounds),
                                "{surface}: label {index} covers {}",
                                node.node_id
                            );
                        }
                    }
                };
                let config = CanvasRenderConfig {
                    auto_fit: false,
                    padding: 0.0,
                    ..Default::default()
                };
                let mut context = MockCanvas2dContext::new(1600.0, 1000.0);
                let mut renderer = Canvas2dRenderer::new(config.clone());
                assert_eq!(renderer.render(&layout, &ir, &mut context).edges_drawn, 12);
                let frame = renderer.edge_label_layout.as_ref().unwrap().bounds;
                let mut canvas = Vec::new();
                for (op_index, op) in context.operations().iter().enumerate() {
                    match op {
                        DrawOperation::FillText(text, _, _)
                            if ir.edges.iter().any(|edge| {
                                edge.label.is_some_and(|id| ir.labels[id.0].text == *text)
                            }) =>
                        {
                            let (x, y, width, height) =
                                painted_plate_before(context.operations(), op_index);
                            canvas.push(LayoutRect {
                                x: x as f32 + frame.x,
                                y: y as f32 + frame.y,
                                width: width as f32,
                                height: height as f32,
                            });
                        }
                        _ => {}
                    }
                }
                check("Canvas", &canvas, &layout.nodes);
                let mut strokes = Vec::new();
                let mut start = 0;
                let mut stroke_width = 1.0;
                let mut saved_widths = Vec::new();
                for (index, operation) in context.operations().iter().enumerate() {
                    match operation {
                        DrawOperation::Save => saved_widths.push(stroke_width),
                        DrawOperation::Restore => {
                            stroke_width = saved_widths.pop().expect("balanced canvas state");
                        }
                        DrawOperation::SetLineWidth(width) => stroke_width = *width as f32,
                        DrawOperation::BeginPath => start = index,
                        // Connector paths use only the outer viewport save. Hollow feedback
                        // markers add their own save/transform and stroke; they are not routes.
                        DrawOperation::Stroke if saved_widths.len() == 1 => strokes.push((
                            first_stroked_path(&context.operations()[start..=index]),
                            stroke_width,
                        )),
                        DrawOperation::FillText(_, _, _) => break,
                        _ => {}
                    }
                }
                assert_eq!(
                    strokes.len(),
                    12,
                    "all transitions precede caption painting"
                );
                let mut label_index = 0;
                for (edge, (path, stroke)) in layout.edges.iter().zip(&strokes) {
                    if ir.edges[edge.edge_index].label.is_none() {
                        continue;
                    }
                    let mut plate = canvas[label_index];
                    label_index += 1;
                    plate.x -= frame.x;
                    plate.y -= frame.y;
                    let gap = rendered_label_gap(path, plate, 0.0, *stroke);
                    assert!(
                        gap <= 1.5,
                        "Canvas {direction:?}/{routing:?} edge {} is {gap}px detached; plate={plate:?}, frame={frame:?}, path={path:?}, route={:?}, planned={:?}, font={}",
                        edge.edge_index,
                        edge.points,
                        renderer.edge_label_layout.as_ref().unwrap().labels[edge.edge_index],
                        config.font_size,
                    );
                }
                let mut repeated = MockCanvas2dContext::new(1600.0, 1000.0);
                Canvas2dRenderer::new(config).render(&layout, &ir, &mut repeated);
                assert_eq!(context.operations(), repeated.operations());

                let svg_config = fm_render_svg::SvgRenderConfig::default();
                let svg = fm_render_svg::render_svg_with_layout(&ir, &layout, &svg_config);
                assert_eq!(
                    svg,
                    fm_render_svg::render_svg_with_layout(&ir, &layout, &svg_config)
                );
                let mut svg_plates = Vec::new();
                for (index, edge) in ir
                    .edges
                    .iter()
                    .enumerate()
                    .filter(|(_, edge)| edge.label.is_some())
                {
                    let group = svg
                        .split_once(&format!("id=\"fm-edge-{index}\""))
                        .unwrap()
                        .1
                        .split_once("</g>")
                        .unwrap()
                        .0;
                    let rect = group
                        .split_once("<rect ")
                        .unwrap()
                        .1
                        .split_once("/>")
                        .unwrap()
                        .0;
                    let plate = LayoutRect {
                        x: number(rect, "x"),
                        y: number(rect, "y"),
                        width: number(rect, "width"),
                        height: number(rect, "height"),
                    };
                    let path = group
                        .split_once("<path ")
                        .unwrap()
                        .1
                        .split_once("/>")
                        .unwrap()
                        .0;
                    let gap = rendered_label_gap(
                        &svg_path(path),
                        plate,
                        number(rect, "rx"),
                        number(path, "stroke-width"),
                    );
                    assert!(
                        gap <= 1.5,
                        "SVG {direction:?}/{routing:?} edge {index} is {gap}px detached"
                    );
                    svg_plates.push(plate);
                    assert!(group.contains(&ir.labels[edge.label.unwrap().0].text));
                }
                check("SVG", &svg_plates, &[]);
                let scene = fm_layout::build_render_scene(&ir, &layout);
                let mut plates = Vec::new();
                scene_plates(&scene.root, &mut plates);
                check("Scene", &plates, &layout.nodes);
                for (edge, plate) in layout
                    .edges
                    .iter()
                    .filter(|edge| ir.edges[edge.edge_index].label.is_some())
                    .zip(&plates)
                {
                    let path = scene_route(&scene.root, edge.edge_index).unwrap();
                    let gap = rendered_label_gap(
                        &path.commands,
                        *plate,
                        3.0,
                        path.stroke.as_ref().unwrap().width,
                    );
                    assert!(
                        gap <= 1.5,
                        "Scene {direction:?}/{routing:?} edge {} is {gap}px detached",
                        edge.edge_index
                    );
                }

                let gpu = crate::gpu_plan::GpuRenderPlan::from_layout(&ir, &layout, 1.5);
                assert_eq!(
                    gpu,
                    crate::gpu_plan::GpuRenderPlan::from_layout(&ir, &layout, 1.5)
                );
                let mut gpu_plates = Vec::new();
                for run in gpu
                    .text_runs
                    .iter()
                    .filter(|run| run.source == crate::gpu_plan::GpuTextSource::Edge)
                {
                    let text = &ir.labels[ir.edges[run.node_index as usize].label.unwrap().0].text;
                    assert_eq!(run.quad_count as usize, text.chars().count());
                    let first = gpu.text_quads[run.first_quad as usize];
                    let font = first.half_extent[1] * 2.0;
                    let advance = crate::gpu_plan::run_advance(text, font);
                    let center = first.center[0]
                        - crate::gpu_plan::glyph_advance(text.chars().next().unwrap(), font) / 2.0
                        + advance / 2.0;
                    let width = advance + font + 8.0;
                    let height = (font * 1.2).max(18.0) + 4.0;
                    let plate = LayoutRect {
                        x: center - width / 2.0,
                        y: first.center[1] - height / 2.0,
                        width,
                        height,
                    };
                    assert!(
                        plate.x >= gpu.bounds.x - 0.02
                            && plate.x + width <= gpu.bounds.x + gpu.bounds.width + 0.02
                    );
                    assert!(
                        plate.y >= gpu.bounds.y - 0.02
                            && plate.y + height <= gpu.bounds.y + gpu.bounds.height + 0.02
                    );
                    let gap = gpu
                        .edge_segments
                        .iter()
                        .filter(|segment| segment.edge_index == run.node_index)
                        .map(|segment| {
                            rendered_label_gap(
                                &[
                                    PathCmd::MoveTo {
                                        x: segment.from[0],
                                        y: segment.from[1],
                                    },
                                    PathCmd::LineTo {
                                        x: segment.to[0],
                                        y: segment.to[1],
                                    },
                                ],
                                plate,
                                0.0,
                                segment.width,
                            )
                        })
                        .fold(f32::INFINITY, f32::min);
                    assert!(
                        gap <= 1.5,
                        "GPU {direction:?}/{routing:?} edge {} is {gap}px detached",
                        run.node_index
                    );
                    gpu_plates.push(plate);
                }
                check("GPU", &gpu_plates, &layout.nodes);
            }
        }
    }

    #[test]
    fn shared_corridor_connectors_are_painted_before_every_label() {
        let ir = fm_parser::parse("flowchart LR\nA -->|first| B\nA -->|second| B\n").ir;
        let mut layout = layout_diagram(&ir);
        layout.nodes.clear();
        layout.bounds = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 500.0,
            height: 160.0,
        };
        for edge in &mut layout.edges {
            edge.points = [
                fm_layout::LayoutPoint { x: 20.0, y: 80.0 },
                fm_layout::LayoutPoint { x: 480.0, y: 80.0 },
            ]
            .into_iter()
            .collect();
        }
        let mut context = MockCanvas2dContext::new(500.0, 160.0);
        Canvas2dRenderer::new(CanvasRenderConfig::default()).render(&layout, &ir, &mut context);
        let first_label = context
            .operations()
            .iter()
            .position(|op| matches!(op, DrawOperation::FillText(text, _, _) if text == "first"))
            .unwrap();
        assert!(
            !context.operations()[first_label..]
                .iter()
                .any(|op| matches!(op, DrawOperation::Stroke))
        );
        let a = drawn_edge_plate(&context, "first");
        let b = drawn_edge_plate(&context, "second");
        assert!(a.0 + a.2 <= b.0 || b.0 + b.2 <= a.0 || a.1 + a.3 <= b.1 || b.1 + b.3 <= a.1);
        assert_eq!(
            a.1 + a.3 / 2.0,
            b.1 + b.3 / 2.0,
            "captions should slide along their shared route"
        );
    }

    #[test]
    fn graph_edge_label_plates_avoid_nodes_foreign_routes_and_each_other() {
        for diagram_type in [
            DiagramType::Flowchart,
            DiagramType::Class,
            DiagramType::State,
            DiagramType::Er,
        ] {
            let (ir, layout) = crossing_label_fixture(diagram_type);
            let mut ctx = MockCanvas2dContext::new(400.0, 300.0);
            let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig {
                auto_fit: false,
                padding: 0.0,
                ..Default::default()
            });
            renderer.render(&layout, &ir, &mut ctx);
            let plan = renderer.edge_label_layout.as_ref().unwrap();
            let plates = [
                drawn_edge_plate(&ctx, "crossing"),
                drawn_edge_plate(&ctx, "secondary"),
            ];
            for (edge_index, &(x, y, width, height)) in plates.iter().enumerate() {
                // Convert the actual draw commands back from the expanded render frame.
                let left = x + f64::from(plan.bounds.x);
                let top = y + f64::from(plan.bounds.y);
                let right = left + width;
                let bottom = top + height;
                for node in &layout.nodes {
                    let rect = node.bounds;
                    assert!(
                        right <= f64::from(rect.x)
                            || left >= f64::from(rect.x + rect.width)
                            || bottom <= f64::from(rect.y)
                            || top >= f64::from(rect.y + rect.height),
                        "{diagram_type:?} label {edge_index} overlaps node {}",
                        node.node_id,
                    );
                }
                let foreign_path = &layout.edges[1 - edge_index];
                let a = foreign_path.points[0];
                let b = foreign_path.points[1];
                let crosses = if a.x == b.x {
                    left < f64::from(a.x)
                        && f64::from(a.x) < right
                        && top < f64::from(a.y.max(b.y))
                        && f64::from(a.y.min(b.y)) < bottom
                } else {
                    top < f64::from(a.y)
                        && f64::from(a.y) < bottom
                        && left < f64::from(a.x.max(b.x))
                        && f64::from(a.x.min(b.x)) < right
                };
                assert!(!crosses, "{diagram_type:?} label covers an unrelated route");
            }
            let (a, b) = (plates[0], plates[1]);
            assert!(
                a.0 + a.2 <= b.0 || b.0 + b.2 <= a.0 || a.1 + a.3 <= b.1 || b.1 + b.3 <= a.1,
                "{diagram_type:?} label plates overlap",
            );
        }
    }

    #[test]
    fn graph_edge_label_plate_measures_authored_multiline_font() {
        let ir =
            fm_parser::parse("flowchart LR\nA -->|first<br/>second| B\nlinkStyle 0 font-size:36px")
                .ir;
        assert_eq!(
            ir.labels[ir.edges[0].label.unwrap().0].text,
            "first\nsecond"
        );
        let layout = layout_diagram(&ir);
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
        renderer.render(&layout, &ir, &mut ctx);
        let (_, _, width, height) = drawn_edge_plate(&ctx, "first");
        assert!(width >= 6.0 * 36.0 * 0.57 + 8.0 - 0.001);
        assert!(height >= 2.0 * 36.0 * 1.2 + 4.0 - 0.001);
        let rows: Vec<_> = ctx
            .operations()
            .iter()
            .filter_map(|op| match op {
                DrawOperation::FillText(text, _, y) if text == "first" || text == "second" => {
                    Some(*y)
                }
                _ => None,
            })
            .collect();
        assert_eq!(rows.len(), 2);
        assert!(rows[1] - rows[0] >= 36.0 * 1.2 - 0.001);
    }

    #[test]
    fn c4_relationships_keep_every_row_in_one_clear_measured_plate() {
        for header in [
            "C4Context",
            "C4Container",
            "C4Component",
            "C4Dynamic",
            "C4Deployment",
        ] {
            let source = format!(
                "{header}\nPerson(A, \"Alpha\")\nSystem(B, \"Beta\")\n\
                 System(C, \"Blocker\")\nSystem(D, \"Top\")\nSystem(E, \"Bottom\")\n\
                 Rel(A, B, \"short<br/>confirmation\", \"HTTPS<br/>TLS with certificate pinning\")\n\
                 Rel(D, E, \"other\", \"gRPC\")"
            );
            let mut ir = fm_parser::parse(&source).ir;
            assert_eq!(ir.edges.len(), 2, "{header}");
            ir.edges[0].inline_style = Some(Box::new(fm_core::IrInlineStyle::from_pairs([(
                "font-size".to_owned(),
                "32px".to_owned(),
            )])));
            let layout = crossing_label_layout(&ir);
            let mut ctx = MockCanvas2dContext::new(400.0, 300.0);
            let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig {
                auto_fit: false,
                padding: 0.0,
                ..Default::default()
            });
            renderer.render(&layout, &ir, &mut ctx);
            let plan = renderer.edge_label_layout.as_ref().unwrap();
            let mut plates = Vec::new();
            for edge_index in 0..2 {
                let first = if edge_index == 0 { "short" } else { "other" };
                let first = if header == "C4Dynamic" {
                    format!("{}: {first}", edge_index + 1)
                } else {
                    first.to_owned()
                };
                let rows = if edge_index == 0 {
                    vec![
                        first.as_str(),
                        "confirmation",
                        "[HTTPS",
                        "TLS with certificate pinning]",
                    ]
                } else {
                    vec![first.as_str(), "[gRPC]"]
                };
                let plate = drawn_edge_plate(&ctx, &first);
                let font_size = if edge_index == 0 {
                    32.0
                } else {
                    renderer.config.font_size * 0.85
                };
                let line_height = (renderer.config.font_size * 1.2).max(font_size * 1.2);
                let expected_width =
                    rows.iter().map(|line| line.len()).max().unwrap() as f64 * font_size * 0.57
                        + 8.0;
                assert!(
                    (plate.2 - expected_width).abs() < 0.001,
                    "{header}: complete caption width"
                );
                assert!((plate.3 - rows.len() as f64 * line_height - 4.0).abs() < 0.001);
                let mut font = String::new();
                let drawn: Vec<_> = ctx
                    .operations()
                    .iter()
                    .filter_map(|op| match op {
                        DrawOperation::SetFont(value) => {
                            font.clone_from(value);
                            None
                        }
                        DrawOperation::FillText(text, x, y) if rows.contains(&text.as_str()) => {
                            Some((text.as_str(), *x, *y, font.clone()))
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    drawn.len(),
                    rows.len(),
                    "{header}: every caption row survives"
                );
                let label_rows = if edge_index == 0 { 2 } else { 1 };
                for (row, ((text, x, y, font), expected)) in drawn.iter().zip(&rows).enumerate() {
                    assert_eq!(text, expected);
                    assert_eq!(
                        drawn_edge_plate(&ctx, text),
                        plate,
                        "one backing plate for all rows"
                    );
                    assert!((*x - plate.0 - plate.2 * 0.5).abs() < 0.001);
                    let expected_y = plate.1
                        + plate.3 * 0.5
                        + (row as f64 - (rows.len() - 1) as f64 * 0.5) * line_height;
                    assert!((*y - expected_y).abs() < 0.001);
                    assert_eq!(
                        font.starts_with("italic "),
                        row >= label_rows,
                        "{header}: technology style"
                    );
                    assert!(*y - font_size * 0.5 >= plate.1);
                    assert!(*y + font_size * 0.5 <= plate.1 + plate.3);
                }
                let left = plate.0 + f64::from(plan.bounds.x);
                let top = plate.1 + f64::from(plan.bounds.y);
                let right = left + plate.2;
                let bottom = top + plate.3;
                assert!(left >= f64::from(plan.bounds.x) - 0.001);
                assert!(top >= f64::from(plan.bounds.y) - 0.001);
                assert!(right <= f64::from(plan.bounds.x + plan.bounds.width) + 0.001);
                assert!(bottom <= f64::from(plan.bounds.y + plan.bounds.height) + 0.001);
                for node in &layout.nodes {
                    let bounds = node.bounds;
                    assert!(
                        right <= f64::from(bounds.x)
                            || left >= f64::from(bounds.x + bounds.width)
                            || bottom <= f64::from(bounds.y)
                            || top >= f64::from(bounds.y + bounds.height),
                        "{header}: full caption overlaps {}",
                        node.node_id,
                    );
                }
                let foreign_path = &layout.edges[1 - edge_index];
                for step in 0..=32 {
                    let a = foreign_path.points[0];
                    let b = foreign_path.points[1];
                    let t = f64::from(step) / 32.0;
                    let x = f64::from(a.x) + f64::from(b.x - a.x) * t;
                    let y = f64::from(a.y) + f64::from(b.y - a.y) * t;
                    assert!(
                        x <= left || x >= right || y <= top || y >= bottom,
                        "{header}: technology covers a foreign route"
                    );
                }
                plates.push(plate);
            }
            let (a, b) = (plates[0], plates[1]);
            assert!(a.0 + a.2 <= b.0 || b.0 + b.2 <= a.0 || a.1 + a.3 <= b.1 || b.1 + b.3 <= a.1);
        }
    }

    #[test]
    fn c4_technology_only_relationship_is_measured_and_hidden_edges_stay_hidden() {
        let mut ir = fm_parser::parse(
            "C4Context\nPerson(a, \"A\")\nSystem(b, \"B\")\nRel(a, b, \"\", \"gRPC\")",
        )
        .ir;
        ir.edges[0].label = None;
        ir.edges[0].inline_style = Some(Box::new(fm_core::IrInlineStyle::from_pairs([(
            "font-size".to_owned(),
            "64px".to_owned(),
        )])));
        let mut layout = layout_diagram(&ir);
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
        renderer.render(&layout, &ir, &mut ctx);
        assert!(renderer.edge_label_layout.as_ref().unwrap().labels[0].is_some());
        let (_, _, width, height) = drawn_edge_plate(&ctx, "[gRPC]");
        assert!((width - 6.0 * 64.0 * 0.57 - 8.0).abs() < 0.001);
        let planned = renderer.edge_label_layout.as_ref().unwrap().labels[0]
            .as_ref()
            .unwrap();
        assert!(planned.height >= 64.0 * 1.2 + 4.0);
        assert!(
            (height - planned.height).abs() < 1e-6,
            "painted height {height} differs from planned height {}",
            planned.height
        );

        for bundled in [true, false] {
            layout.edges[0].bundled = bundled;
            ir.edges[0].arrow = if bundled {
                ArrowType::Arrow
            } else {
                ArrowType::Invisible
            };
            ctx.clear();
            renderer.render(&layout, &ir, &mut ctx);
            assert!(renderer.edge_label_layout.is_none());
            assert!(!ctx.operations().iter().any(|op| matches!(
                op, DrawOperation::FillText(text, _, _) if text == "[gRPC]"
            )));
        }
    }

    #[test]
    fn bundled_and_invisible_edges_do_not_reserve_or_draw_label_plates() {
        let (mut ir, mut layout) = crossing_label_fixture(DiagramType::Flowchart);
        ir.edges[1].arrow = ArrowType::Invisible;
        layout.edges[0].bundled = true;
        let mut ctx = MockCanvas2dContext::new(400.0, 300.0);
        let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
        let result = renderer.render(&layout, &ir, &mut ctx);
        assert_eq!(result.edges_drawn, 0);
        assert!(renderer.edge_label_layout.is_none());
        assert!(!ctx.operations().iter().any(|op| matches!(
            op,
            DrawOperation::FillText(text, _, _) if text == "crossing" || text == "secondary"
        )));
    }

    #[test]
    fn nested_composite_states_keep_internal_labels_and_do_not_overpaint_children() {
        let ir = fm_parser::parse(
            "stateDiagram-v2\n[*] --> Outer\nstate Outer {\n [*] --> Inner\n state Inner {\n  A --> B : ready\n }\n Inner --> Finished : complete\n}\nOuter --> [*]",
        )
        .ir;
        let layout = layout_diagram(&ir);
        let composite_count = layout
            .nodes
            .iter()
            .filter(|node| fm_layout::is_composite_state_node(&ir, node))
            .count();
        assert_eq!(composite_count, 2, "fixture must reference both containers");
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
        let result = renderer.render(&layout, &ir, &mut ctx);
        assert_eq!(result.nodes_drawn, layout.nodes.len() - composite_count);
        for title in ["Outer", "Inner"] {
            assert_eq!(
                ctx.operations()
                    .iter()
                    .filter(|op| matches!(op, DrawOperation::FillText(text, _, _) if text == title))
                    .count(),
                1,
                "a composite title belongs to its cluster, with no overlaid node",
            );
        }
        let inner = layout
            .clusters
            .iter()
            .find(|cluster| cluster.title.as_deref() == Some("Inner"))
            .expect("nested cluster exists")
            .bounds;
        let plan = renderer.edge_label_layout.as_ref().unwrap();
        let (x, y, width, height) = drawn_edge_plate(&ctx, "ready");
        let left = x + f64::from(plan.bounds.x);
        let top = y + f64::from(plan.bounds.y);
        assert!(left >= f64::from(inner.x));
        assert!(left + width <= f64::from(inner.x + inner.width));
        assert!(top >= f64::from(inner.y));
        assert!(top + height <= f64::from(inner.y + inner.height));
    }

    #[test]
    fn state_scene_canvas_keeps_composite_contents_notes_and_dividers() {
        let ir = fm_parser::parse(
            "stateDiagram-v2\n[*] --> Outer\nstate Outer {\n\
             state Inner {\nA --> B : ready\n--\nC --> D : parallel\n}\n\
             Inner --> Finished\n}\nOuter --> [*]\n\
             note right of Outer\nFirst note row\nSecond note row\nend note\n",
        )
        .ir;
        let config = CanvasRenderConfig {
            font_size: 20.0,
            line_height: 1.75,
            ..Default::default()
        };
        let metrics = config.font_metrics();
        let layout = fm_layout::layout_diagram_with_config(
            &ir,
            fm_layout::LayoutConfig {
                font_metrics: Some(metrics.clone()),
                ..Default::default()
            },
        );
        assert_eq!(layout.extensions.state_notes.len(), 1);
        assert_eq!(layout.extensions.cluster_dividers.len(), 1);
        let composite_count = layout
            .nodes
            .iter()
            .filter(|node| fm_layout::is_composite_state_node(&ir, node))
            .count();
        assert_eq!(composite_count, 2);
        let scene = fm_layout::build_render_scene_with_metrics(&ir, &layout, &metrics);
        let mut ctx = MockCanvas2dContext::new(1200.0, 900.0);
        let result = crate::render_scene_to_canvas(&scene, &mut ctx, &config);
        assert_eq!(result.nodes_drawn, layout.nodes.len() - composite_count);
        for label in [
            "Outer",
            "Inner",
            "A",
            "B",
            "C",
            "D",
            "ready",
            "parallel",
            "First note row",
            "Second note row",
        ] {
            assert_eq!(
                ctx.operations()
                    .iter()
                    .filter(|op| {
                        matches!(op, DrawOperation::FillText(text, _, _) if text == label)
                    })
                    .count(),
                1,
                "missing or duplicated {label}"
            );
        }
        for pattern in [vec![6.0, 4.0], vec![4.0, 3.0]] {
            assert!(
                ctx.operations().iter().any(|op| {
                    matches!(op, DrawOperation::SetLineDash(dash) if *dash == pattern)
                }),
                "missing concurrency divider or annotation leader"
            );
        }
        let row_positions: Vec<_> = ctx
            .operations()
            .iter()
            .filter_map(|op| match op {
                DrawOperation::FillText(text, _, y) if text.ends_with("note row") => Some(*y),
                _ => None,
            })
            .collect();
        assert_eq!(row_positions.len(), 2);
        assert!((row_positions[1] - row_positions[0] - 28.0).abs() < 0.001);
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
        assert_state_note_text_fits_reserved_rows(&CanvasRenderConfig::default());
    }

    #[test]
    fn state_note_spacing_tracks_configured_line_height() {
        for (font_size, line_height) in [(24.0, 2.0), (18.0, 1.25), (18.0, f64::NAN)] {
            assert_state_note_text_fits_reserved_rows(&CanvasRenderConfig {
                font_size,
                line_height,
                ..Default::default()
            });
        }
    }

    fn assert_state_note_text_fits_reserved_rows(config: &CanvasRenderConfig) {
        let src = "stateDiagram-v2\n  A --| B\n  note right of A\n    alpha line\n                       beta line\n  end note\n"
            .replace("--|", "-->");
        let ir = fm_parser::parse(&src).ir;
        let layout = fm_layout::layout_diagram_with_config(
            &ir,
            fm_layout::LayoutConfig {
                font_metrics: Some(config.font_metrics()),
                ..Default::default()
            },
        );

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
        let _ = crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, config);

        let mut lines: Vec<(f64, f64)> = Vec::new();
        for op in ctx.operations() {
            if let DrawOperation::FillText(text, x, y) = op
                && note.text.lines().any(|line| line == text)
            {
                lines.push((*x, *y));
            }
        }
        assert_eq!(
            lines.len(),
            note.text.lines().count(),
            "every note line must be drawn"
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
        let expected_line_height = (f64::from(note.bounds.height) - 2.0 * STATE_NOTE_PAD_Y)
            / note.text.lines().count() as f64;
        assert!(
            (second_y - first_y - expected_line_height).abs() < 0.01,
            "line spacing must use the {expected_line_height}px rows reserved by layout: got {}",
            second_y - first_y
        );
        let svg = fm_render_svg::render_svg_with_layout(
            &ir,
            &layout,
            &fm_render_svg::SvgRenderConfig {
                font_family: config.font_family.clone(),
                font_size: config.font_size as f32,
                line_height: config.resolved_line_height(),
                ..Default::default()
            },
        );
        let note_text = svg
            .split_once("class=\"fm-state-note-text\"")
            .unwrap()
            .1
            .split_once("</text>")
            .unwrap()
            .0;
        let row_offsets: Vec<f64> = note_text
            .split("<tspan ")
            .skip(1)
            .map(|row| {
                row.split_once("dy=\"")
                    .unwrap()
                    .1
                    .split_once('"')
                    .unwrap()
                    .0
                    .parse()
                    .unwrap()
            })
            .collect();
        assert_eq!(row_offsets.len(), lines.len());
        assert_eq!(row_offsets[0], 0.0);
        for (pair, offset) in lines.windows(2).zip(row_offsets.iter().skip(1)) {
            assert!(
                (pair[1].1 - pair[0].1 - offset).abs() < 0.01,
                "Canvas note rows must match the font and spacing actually emitted by SVG"
            );
        }
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

    #[test]
    fn dense_gantt_axis_dates_stay_readable_in_canvas_and_gpu() {
        for source in [
            "%%{init: {'gantt': {'topAxis': true}}}%%\ngantt\n tickInterval 1day\n dateFormat YYYY-MM-DD\n Task :a,2026-01-01,14d\n",
            "%%{init: {'gantt': {'topAxis': true}}}%%\ngantt\n dateFormat YYYY-MM-DD\n Task :a,2026-01-01,1d\n",
        ] {
            let ir = fm_parser::parse(source).ir;
            for font_size in [9.0, 15.0, 24.0] {
                let mut config = CanvasRenderConfig {
                    font_size,
                    line_height: 1.8,
                    auto_fit: false,
                    padding: 0.0,
                    ..Default::default()
                };
                if font_size == 24.0 {
                    config.font_family = "monospace".to_owned();
                    config.label_color = "#f8fafc".to_owned();
                    config.edge_stroke = "#94a3b8".to_owned();
                }
                let metrics = config.font_metrics();
                let layout = fm_layout::layout_diagram_with_config(
                    &ir,
                    fm_layout::LayoutConfig {
                        font_metrics: Some(metrics.clone()),
                        ..Default::default()
                    },
                );
                let ticks = &layout.extensions.axis_ticks;
                let rows = fm_layout::gantt_axis_label_rows(&layout, &metrics);
                assert!(ticks.len() >= 2);
                assert!(
                    rows.iter().any(|&row| row > 0),
                    "fixture needs crowded dates"
                );
                assert_eq!(layout.extensions.gantt_axis_rows.len(), 2);

                let mut ctx = MockCanvas2dContext::new(2000.0, 1200.0);
                crate::render_to_canvas_with_layout(&ir, &layout, &mut ctx, &config);
                let (_, painted) = record_gantt_canvas_paint(ctx.operations());
                let labels: Vec<_> = painted
                    .iter()
                    .filter(|label| ticks.iter().any(|tick| tick.label == label.text))
                    .collect();
                let plan = crate::GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
                let runs: Vec<_> = plan
                    .text_runs
                    .iter()
                    .filter(|run| run.source == crate::GpuTextSource::AxisTick)
                    .collect();
                assert_eq!(labels.len(), ticks.len() * 2);
                assert_eq!(runs.len(), labels.len());
                assert_eq!(plan.axis_tick_segments.len(), labels.len());
                let tick_font_size = (font_size * 0.72).max(8.0);
                let text_color = crate::gpu_plan::parse_paint_rgba(&config.label_color).unwrap();
                let stroke_color = crate::gpu_plan::parse_paint_rgba(&config.edge_stroke).unwrap();

                for (axis_index, axis) in layout.extensions.gantt_axis_rows.iter().enumerate() {
                    let mut ink_bounds = Vec::new();
                    for (tick_index, tick) in ticks.iter().enumerate() {
                        let index = axis_index * ticks.len() + tick_index;
                        let label = labels[index];
                        let label_x = label.x + f64::from(layout.bounds.x);
                        let label_y = label.y + f64::from(layout.bounds.y);
                        let expected_y =
                            fm_layout::gantt_axis_label_baseline(*axis, rows[tick_index], &metrics);
                        assert_eq!(label.text, tick.label);
                        assert_eq!(label.paint.baseline, TextBaseline::Alphabetic);
                        assert_eq!(label.paint.align, TextAlign::Left);
                        assert_eq!(label.paint.fill, config.label_color);
                        assert!(
                            label
                                .paint
                                .font
                                .starts_with(&format!("{tick_font_size}px "))
                        );
                        assert!((label_x - f64::from(tick.position) - 3.0).abs() < 0.01);
                        assert!((label_y - f64::from(expected_y)).abs() < 0.01);

                        let width = f64::from(metrics.estimate_preserved_width(
                            &tick.label,
                            tick_font_size as f32,
                            0.0,
                        ));
                        let bounds = [
                            label_x,
                            label_y - tick_font_size,
                            width,
                            tick_font_size * 1.3,
                        ];
                        assert!(bounds[0] >= f64::from(layout.bounds.x) - 0.01);
                        assert!(bounds[1] >= f64::from(layout.bounds.y) - 0.01);
                        assert!(
                            bounds[0] + bounds[2]
                                <= f64::from(layout.bounds.x + layout.bounds.width) + 0.01
                        );
                        assert!(
                            bounds[1] + bounds[3]
                                <= f64::from(layout.bounds.y + layout.bounds.height) + 0.01
                        );
                        ink_bounds.push(bounds);

                        let run = runs[index];
                        assert_eq!(run.node_index as usize, tick_index);
                        assert!(run.quad_count > 0);
                        for quad in
                            &plan.text_quads[run.first_quad as usize..][..run.quad_count as usize]
                        {
                            let baseline = f64::from(quad.center[1])
                                + f64::from(crate::gpu_plan::GLYPH_BASELINE_RATIO - 0.5)
                                    * tick_font_size;
                            assert!((baseline - label_y).abs() < 0.01);
                            assert_eq!(quad.color, text_color);
                        }
                        let mark = &plan.axis_tick_segments[index];
                        assert_eq!(mark.from, [tick.position, axis.y + 4.0]);
                        assert_eq!(mark.to, [tick.position, axis.y + 16.0]);
                        assert_eq!(mark.color, stroke_color);
                    }
                    for (index, a) in ink_bounds.iter().enumerate() {
                        for b in &ink_bounds[index + 1..] {
                            assert!(
                                a[0] + a[2] <= b[0] + 0.01
                                    || b[0] + b[2] <= a[0] + 0.01
                                    || a[1] + a[3] <= b[1] + 0.01
                                    || b[1] + b[3] <= a[1] + 0.01,
                                "painted date labels overlap: {a:?} and {b:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn canvas_and_gpu_bands_use_authored_dark_surfaces_and_keep_true_lifelines() {
        for source in [
            "gantt\n dateFormat YYYY-MM-DD\n section Planning\n Design :a,2026-01-01,3d\n section Delivery\n Build :b,after a,4d\n",
            "journey\n section Discover\n Research: 5: Ada\n Review: 4: Ada\n",
            "gitGraph\n commit\n branch develop\n checkout develop\n commit\n",
            "sankey-beta\nSource,Process,5\nProcess,Output,5\n",
            "sequenceDiagram\n Alice->>Bob: Hello\n",
        ] {
            let mut authored = String::from(
                "%%{init: {'themeVariables': {'clusterBkg': '#172554', 'clusterBorder': '#64748b', 'primaryColor': '#0f172a', 'primaryBorderColor': '#94a3b8', 'primaryTextColor': '#f8fafc'}}}%%\n",
            );
            authored.push_str(source);
            let ir = fm_parser::parse(&authored).ir;
            let config = CanvasRenderConfig {
                auto_fit: false,
                padding: 0.0,
                ..Default::default()
            };
            let layout = fm_layout::layout_diagram(&ir);
            assert!(!layout.extensions.bands.is_empty());
            let mut context = MockCanvas2dContext::new(1600.0, 1000.0);
            crate::render_to_canvas_with_layout(&ir, &layout, &mut context, &config);
            let (fills, labels) = record_gantt_canvas_paint(context.operations());
            let plan = crate::GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
            let color = |paint| crate::gpu_plan::parse_paint_rgba(paint).unwrap();

            if ir.diagram_type == DiagramType::Sequence {
                assert!(plan.band_section_instances.is_empty());
                assert_eq!(plan.band_lane_segments.len(), layout.extensions.bands.len());
                for (band, line) in layout.extensions.bands.iter().zip(&plan.band_lane_segments) {
                    assert_eq!(
                        line.from,
                        [band.bounds.x + band.bounds.width / 2.0, band.bounds.y]
                    );
                    assert_eq!(line.to, [line.from[0], band.bounds.y + band.bounds.height]);
                    assert_eq!(line.color, color("#94a3b8"));
                    assert_eq!(line.dash, [6.0, 4.0]);
                    assert_eq!(line.width, 1.0);
                }
                assert_eq!(
                    labels.iter().filter(|label| label.text == "Alice").count(),
                    1,
                    "an unmirrored actor is drawn once without a duplicate lifeline caption"
                );
                continue;
            }

            assert!(plan.band_lane_segments.is_empty());
            assert!(plan.band_column_segments.is_empty());
            assert_eq!(
                plan.band_section_instances.len(),
                layout.extensions.bands.len()
            );
            for (index, band) in layout.extensions.bands.iter().enumerate() {
                let panel = &plan.band_section_instances[index];
                let expected_fill =
                    if ir.diagram_type == DiagramType::Gantt && !index.is_multiple_of(2) {
                        "#0f172a"
                    } else {
                        "#172554"
                    };
                assert_eq!(panel.fill, color(expected_fill));
                assert_eq!(panel.stroke, color("#64748b"));
                assert_eq!(
                    panel.center,
                    [band.bounds.center().x, band.bounds.center().y]
                );
                assert_eq!(
                    panel.half_extent,
                    [band.bounds.width / 2.0, band.bounds.height / 2.0]
                );
                let painted = fills
                    .iter()
                    .find(|fill| {
                        fill.paint.fill == expected_fill
                            && (fill.bounds[0] + f64::from(layout.bounds.x - band.bounds.x)).abs()
                                < 0.01
                            && (fill.bounds[1] + f64::from(layout.bounds.y - band.bounds.y)).abs()
                                < 0.01
                            && (fill.bounds[2] - f64::from(band.bounds.width)).abs() < 0.01
                            && (fill.bounds[3] - f64::from(band.bounds.height)).abs() < 0.01
                    })
                    .expect("the complete panel rectangle must reach the raster surface");
                assert_eq!(painted.paint.stroke, "#64748b");
                assert!(painted.paint.dash.is_empty());
                assert_eq!(painted.paint.alpha, 1.0);
                assert_eq!(painted.paint.line_width, f64::from(panel.stroke_width));

                if !band.label.is_empty() && ir.diagram_type != DiagramType::Journey {
                    let caption = labels
                        .iter()
                        .find(|label| label.text == band.label)
                        .unwrap();
                    assert!(caption.paint.font.starts_with("600 "));
                    assert_eq!(caption.paint.fill, "#f8fafc");
                    assert_eq!(caption.paint.baseline, TextBaseline::Alphabetic);
                    let run = plan
                        .text_runs
                        .iter()
                        .find(|run| {
                            run.source == crate::GpuTextSource::Band
                                && run.node_index as usize == index
                        })
                        .unwrap();
                    assert!(run.quad_count > 0);
                    assert_eq!(
                        plan.text_quads[run.first_quad as usize].color,
                        color("#f8fafc")
                    );
                }
            }
        }
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

    #[test]
    fn debug_overlay_draws_node_bboxes_and_hud_text_when_enabled() {
        let parsed = fm_parser::parse("flowchart TD\n  a[Alpha] --> b[Beta]\n");
        let layout = fm_layout::layout_diagram(&parsed.ir);

        let mut base_ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut base_renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
        let base_res = base_renderer.render(&layout, &parsed.ir, &mut base_ctx);

        let mut debug_ctx = MockCanvas2dContext::new(800.0, 600.0);
        let mut debug_renderer = Canvas2dRenderer::new(CanvasRenderConfig {
            debug_overlay: true,
            ..CanvasRenderConfig::default()
        });
        let debug_res = debug_renderer.render(&layout, &parsed.ir, &mut debug_ctx);

        assert!(debug_res.draw_calls > base_res.draw_calls);
        assert_eq!(debug_res.labels_drawn, base_res.labels_drawn + 1);
        assert!(
            debug_ctx
                .operations()
                .iter()
                .any(|op| matches!(op, DrawOperation::FillText(t, _, _) if t.starts_with("debug: nodes=2 edges=1")))
        );
    }
}
