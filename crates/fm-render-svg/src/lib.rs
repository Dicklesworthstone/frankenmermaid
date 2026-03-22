#![forbid(unsafe_code)]

//! Zero-dependency SVG builder for frankenmermaid diagram rendering.
//!
//! Provides a lightweight, type-safe API for generating clean SVG output
//! suitable for flowcharts, sequence diagrams, and other diagram types.

mod a11y;
mod attributes;
mod defs;
mod document;
mod element;
mod path;
mod text;
mod theme;
mod transform;

pub use a11y::{A11yConfig, accessibility_css, describe_diagram, describe_edge, describe_node};
pub use attributes::{Attribute, AttributeValue, Attributes};
pub use defs::{ArrowheadMarker, DefsBuilder, Filter, Gradient, GradientStop, MarkerOrient};
pub use document::SvgDocument;
pub use element::{Element, ElementKind};
pub use path::{PathBuilder, PathCommand};
pub use text::{TextAnchor, TextBuilder};
pub use theme::{FontConfig, Theme, ThemeColors, ThemePreset, generate_palette};
pub use transform::{Transform, TransformBuilder};

use fm_core::{IrXyChartMeta, IrXySeriesKind, MermaidDiagramIr, MermaidTier, Span};
use fm_layout::{
    DiagramLayout, FillStyle, LayoutBand, LayoutBandKind, LayoutEdgePath, LayoutNodeBox,
    LineCap as RenderLineCap, LineJoin as RenderLineJoin, MarkerKind, PathCmd, RenderClip,
    RenderGroup, RenderItem, RenderPath, RenderScene, RenderSource, RenderText, RenderTransform,
    StrokeStyle, TextAlign as RenderTextAlign, TextBaseline as RenderTextBaseline,
    build_render_scene,
};

/// Node fill gradient mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeGradientStyle {
    /// Top-to-bottom linear gradient.
    #[default]
    LinearVertical,
    /// Left-to-right linear gradient.
    LinearHorizontal,
    /// Center-weighted radial gradient.
    Radial,
}

/// Backend strategy used by SVG rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SvgBackend {
    /// Existing layout-driven renderer.
    #[default]
    LegacyLayout,
    /// Shared target-agnostic render scene backend.
    Scene,
}

/// Configuration for SVG rendering.
#[derive(Debug, Clone)]
pub struct SvgRenderConfig {
    /// Backend implementation used for rendering.
    pub backend: SvgBackend,
    /// Whether to include responsive sizing attributes.
    pub responsive: bool,
    /// Whether to include accessibility attributes.
    pub accessible: bool,
    /// Default font family for text.
    pub font_family: String,
    /// Default font size in pixels.
    pub font_size: f32,
    /// Average character width for text measurement (in pixels).
    pub avg_char_width: f32,
    /// Line height multiplier for multi-line text.
    pub line_height: f32,
    /// Padding around the diagram.
    pub padding: f32,
    /// Whether to include drop shadows.
    pub shadows: bool,
    /// Shadow X offset in px.
    pub shadow_offset_x: f32,
    /// Shadow Y offset in px.
    pub shadow_offset_y: f32,
    /// Shadow blur radius.
    pub shadow_blur: f32,
    /// Shadow opacity [0.0, 1.0].
    pub shadow_opacity: f32,
    /// Shadow color.
    pub shadow_color: String,
    /// Whether to include node gradients.
    pub node_gradients: bool,
    /// Node gradient style.
    pub node_gradient_style: NodeGradientStyle,
    /// Whether highlighted nodes should get glow treatment.
    pub glow_enabled: bool,
    /// Glow blur radius.
    pub glow_blur: f32,
    /// Glow opacity [0.0, 1.0].
    pub glow_opacity: f32,
    /// Glow color.
    pub glow_color: String,
    /// Opacity for cluster backgrounds [0.0, 1.0].
    pub cluster_fill_opacity: f32,
    /// Opacity for dim/inactive elements [0.0, 1.0].
    pub inactive_opacity: f32,
    /// Whether to use rounded corners on rectangles.
    pub rounded_corners: f32,
    /// CSS classes to apply to the root SVG element.
    pub root_classes: Vec<String>,
    /// Theme preset to use (default if not specified).
    pub theme: ThemePreset,
    /// Whether to embed theme CSS in the SVG.
    pub embed_theme_css: bool,
    /// Detail tier selection (`auto`, `compact`, `normal`, `rich`).
    pub detail_tier: MermaidTier,
    /// Minimum readable font size in pixels.
    pub min_font_size: f32,
    /// Whether to embed print-optimized CSS rules.
    pub print_optimized: bool,
    /// Accessibility configuration.
    pub a11y: A11yConfig,
    /// Whether to emit source-span metadata attributes in the SVG output.
    pub include_source_spans: bool,
}

impl SvgRenderConfig {
    /// Get the font metrics based on this configuration.
    #[must_use]
    pub fn font_metrics(&self) -> fm_core::FontMetrics {
        fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
            preset: fm_core::FontPreset::from_family(&self.font_family),
            font_size: self.font_size,
            line_height: self.line_height,
            fallback_chain: vec![
                fm_core::FontPreset::SansSerif,
                fm_core::FontPreset::Monospace,
            ],
            trace_fallbacks: false,
        })
    }
}

impl Default for SvgRenderConfig {
    fn default() -> Self {
        Self {
            backend: SvgBackend::LegacyLayout,
            responsive: true,
            accessible: true,
            font_family: String::from(
                "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif",
            ),
            font_size: 15.0,
            avg_char_width: 7.5,
            line_height: 1.5,
            padding: 40.0,
            shadows: true,
            shadow_offset_x: 2.0,
            shadow_offset_y: 2.0,
            shadow_blur: 6.0,
            shadow_opacity: 0.15,
            shadow_color: String::from("#0f172a"),
            node_gradients: true,
            node_gradient_style: NodeGradientStyle::LinearVertical,
            glow_enabled: true,
            glow_blur: 6.0,
            glow_opacity: 0.35,
            glow_color: String::from("#3b82f6"),
            cluster_fill_opacity: 0.08,
            inactive_opacity: 0.40,
            rounded_corners: 10.0,
            root_classes: Vec::new(),
            theme: ThemePreset::Default,
            embed_theme_css: true,
            detail_tier: MermaidTier::Auto,
            min_font_size: 8.0,
            print_optimized: true,
            a11y: A11yConfig::full(),
            include_source_spans: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderDetailTier {
    Compact,
    Normal,
    Rich,
}

#[derive(Debug, Clone, Copy)]
struct RenderDetailProfile {
    tier: RenderDetailTier,
    show_node_labels: bool,
    show_edge_labels: bool,
    show_cluster_labels: bool,
    node_label_max_chars: Option<usize>,
    edge_label_max_chars: Option<usize>,
    node_font_size: f32,
    edge_font_size: f32,
    cluster_font_size: f32,
    enable_shadows: bool,
}

/// Render an IR diagram to SVG string.
#[must_use]
pub fn render_svg(ir: &MermaidDiagramIr) -> String {
    render_svg_with_config(ir, &SvgRenderConfig::default())
}

/// Render an IR diagram to SVG string with custom configuration.
#[must_use]
pub fn render_svg_with_config(ir: &MermaidDiagramIr, config: &SvgRenderConfig) -> String {
    let layout_config = fm_layout::LayoutConfig {
        font_metrics: Some(config.font_metrics()),
        ..Default::default()
    };
    let layout = fm_layout::layout_diagram_with_config(ir, layout_config);
    render_svg_with_layout(ir, &layout, config)
}

/// Render an IR diagram to SVG string with a pre-computed layout.
#[must_use]
pub fn render_svg_with_layout(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    config: &SvgRenderConfig,
) -> String {
    match config.backend {
        SvgBackend::LegacyLayout => render_layout_to_svg(layout, ir, config),
        SvgBackend::Scene => {
            let scene = build_render_scene(ir, layout);
            render_scene_document_with_ir(&scene, config, Some(ir))
        }
    }
}

/// Render a target-agnostic scene to SVG string with custom configuration.
#[must_use]
pub fn render_scene_to_svg(scene: &RenderScene, config: &SvgRenderConfig) -> String {
    render_scene_document(scene, config)
}

fn render_scene_document(scene: &RenderScene, config: &SvgRenderConfig) -> String {
    render_scene_document_with_ir(scene, config, None)
}

fn resolve_theme(ir: Option<&MermaidDiagramIr>, config: &SvgRenderConfig) -> Theme {
    let preset = ir
        .and_then(|i| i.meta.theme_overrides.theme.as_deref())
        .and_then(|t| t.parse::<ThemePreset>().ok())
        .unwrap_or(config.theme);
    let mut theme = Theme::from_preset(preset);
    if let Some(i) = ir {
        theme
            .colors
            .apply_overrides(&i.meta.theme_overrides.theme_variables);
    }
    theme
}

fn render_scene_document_with_ir(
    scene: &RenderScene,
    config: &SvgRenderConfig,
    ir: Option<&MermaidDiagramIr>,
) -> String {
    let padding = config.padding;
    let width = (scene.bounds.width + padding * 2.0).max(1.0);
    let height = (scene.bounds.height + padding * 2.0).max(1.0);

    let mut doc = SvgDocument::new()
        .viewbox(
            scene.bounds.x - padding,
            scene.bounds.y - padding,
            width,
            height,
        )
        .preserve_aspect_ratio("xMidYMid meet");

    if config.responsive {
        doc = doc.responsive();
    }

    let (group_count, path_count, text_count) = count_scene_items(&scene.root);

    if config.accessible {
        let title = ir.map_or_else(
            || String::from("Render scene"),
            |diagram_ir| format!("{} diagram", diagram_ir.diagram_type.as_str()),
        );
        let desc = ir.map_or_else(
            || {
                format!(
                    "Target-agnostic render scene with {group_count} groups, {path_count} paths, and {text_count} text items"
                )
            },
            describe_diagram,
        );
        doc = doc.accessible(title, desc);
    }

    for class in &config.root_classes {
        doc = doc.class(class);
    }

    let scene_type = ir.map_or("scene", |diagram_ir| diagram_ir.diagram_type.as_str());
    doc = doc
        .data("type", scene_type)
        .data("groups", &group_count.to_string())
        .data("paths", &path_count.to_string())
        .data("texts", &text_count.to_string());

    let effects_enabled = clamp_unit_interval(config.inactive_opacity) < 0.999
        || clamp_unit_interval(config.cluster_fill_opacity) < 0.999;

    let theme = resolve_theme(ir, config);

    if config.embed_theme_css {
        let mut css = theme.to_svg_style(config.shadows);
        if effects_enabled {
            css.push_str(&effects_css(config));
        }
        if config.a11y.accessibility_css {
            css.push_str(accessibility_css());
        }
        if config.print_optimized {
            css.push_str(&print_css(config.min_font_size));
        }
        doc = doc.style(css);
    } else if config.a11y.accessibility_css || config.print_optimized || effects_enabled {
        let mut css = String::new();
        if effects_enabled {
            css.push_str(&effects_css(config));
        }
        if config.a11y.accessibility_css {
            css.push_str(accessibility_css());
        }
        if config.print_optimized {
            css.push_str(&print_css(config.min_font_size));
        }
        doc = doc.style(css);
    }

    let mut defs = DefsBuilder::new();

    // Add standard arrowhead markers
    defs = defs.marker(ArrowheadMarker::standard("arrow-end", &theme.colors.edge));
    defs = defs.marker(ArrowheadMarker::filled("arrow-filled", &theme.colors.edge));
    defs = defs.marker(ArrowheadMarker::open("arrow-open", &theme.colors.edge));
    defs = defs.marker(
        ArrowheadMarker::standard("arrow-start", &theme.colors.edge)
            .with_orient(crate::defs::MarkerOrient::AutoStartReverse),
    );
    defs = defs.marker(
        ArrowheadMarker::filled("arrow-start-filled", &theme.colors.edge)
            .with_orient(crate::defs::MarkerOrient::AutoStartReverse),
    );
    defs = defs.marker(ArrowheadMarker::circle_marker(
        "arrow-circle",
        &theme.colors.edge,
    ));
    defs = defs.marker(ArrowheadMarker::cross_marker(
        "arrow-cross",
        &theme.colors.edge,
    ));
    defs = defs.marker(ArrowheadMarker::diamond_marker(
        "arrow-diamond",
        &theme.colors.edge,
    ));

    let mut clip_defs = Vec::new();
    let mut clip_id_counter = 0usize;
    let scene_root = render_scene_group(
        &scene.root,
        config,
        ir,
        &mut clip_defs,
        &mut clip_id_counter,
    );

    for clip in clip_defs {
        defs = defs.custom(clip);
    }

    doc = doc.defs(defs);

    doc.child(scene_root).to_string()
}

fn count_scene_items(group: &RenderGroup) -> (usize, usize, usize) {
    let mut groups = 1usize;
    let mut paths = 0usize;
    let mut texts = 0usize;

    for child in &group.children {
        match child {
            RenderItem::Group(nested) => {
                let (nested_groups, nested_paths, nested_texts) = count_scene_items(nested);
                groups += nested_groups;
                paths += nested_paths;
                texts += nested_texts;
            }
            RenderItem::Path(_) => paths += 1,
            RenderItem::Text(_) => texts += 1,
        }
    }

    (groups, paths, texts)
}

fn render_scene_group(
    group: &RenderGroup,
    config: &SvgRenderConfig,
    ir: Option<&MermaidDiagramIr>,
    clip_defs: &mut Vec<Element>,
    clip_id_counter: &mut usize,
) -> Element {
    let mut elem = Element::group();

    if let Some(id) = &group.id {
        elem = elem.id(id);
    }

    elem = apply_source_metadata(elem, group.source, config.include_source_spans, ir);

    if config.a11y.keyboard_nav
        && matches!(group.source, RenderSource::Node(_) | RenderSource::Edge(_))
    {
        elem = elem.attr("tabindex", "0");
    }

    if let Some(transform) = group.transform {
        let transform_value = scene_transform_value(transform);
        elem = elem.transform(&transform_value);
    }

    if let Some(clip) = &group.clip {
        let clip_id = register_clip_path(clip_defs, clip, clip_id_counter);
        elem = elem.clip_path_ref(&format!("url(#{clip_id})"));
    }

    for child in &group.children {
        elem = elem.child(render_scene_item(
            child,
            config,
            ir,
            clip_defs,
            clip_id_counter,
        ));
    }

    elem
}

fn render_scene_item(
    item: &RenderItem,
    config: &SvgRenderConfig,
    ir: Option<&MermaidDiagramIr>,
    clip_defs: &mut Vec<Element>,
    clip_id_counter: &mut usize,
) -> Element {
    match item {
        RenderItem::Group(group) => {
            render_scene_group(group, config, ir, clip_defs, clip_id_counter)
        }
        RenderItem::Path(path) => render_scene_path(path, config.include_source_spans, ir),
        RenderItem::Text(text) => render_scene_text(text, config, ir),
    }
}

fn render_scene_path(
    path: &RenderPath,
    include_source_spans: bool,
    ir: Option<&MermaidDiagramIr>,
) -> Element {
    let mut elem = Element::path().d(&path_cmds_to_d(&path.commands));
    elem = apply_source_metadata(elem, path.source, include_source_spans, ir);

    if let Some(fill) = &path.fill {
        elem = apply_fill_style(elem, fill);
    } else {
        elem = elem.fill("none");
    }

    if let Some(stroke) = &path.stroke {
        elem = apply_stroke_style(elem, stroke);
    } else {
        elem = elem.stroke("none");
    }

    if path.marker_start != MarkerKind::None {
        elem = elem.marker_start(map_marker_kind(path.marker_start));
    }

    if path.marker_end != MarkerKind::None {
        elem = elem.marker_end(map_marker_kind(path.marker_end));
    }

    elem
}

fn map_marker_kind(kind: fm_layout::MarkerKind) -> &'static str {
    use fm_layout::MarkerKind;
    match kind {
        MarkerKind::None => "",
        MarkerKind::Arrow => "url(#arrow-end)",
        MarkerKind::ThickArrow => "url(#arrow-filled)",
        MarkerKind::DottedArrow => "url(#arrow-end)",
        MarkerKind::Circle => "url(#arrow-circle)",
        MarkerKind::Cross => "url(#arrow-cross)",
        MarkerKind::Diamond => "url(#arrow-diamond)",
        MarkerKind::Open => "url(#arrow-open)",
    }
}

fn render_scene_text(
    text: &RenderText,
    config: &SvgRenderConfig,
    ir: Option<&MermaidDiagramIr>,
) -> Element {
    let mut elem = TextBuilder::new(&text.text)
        .x(text.x)
        .y(text.y)
        .font_family(&config.font_family)
        .font_size(text.font_size)
        .line_height(config.line_height)
        .anchor(map_text_align(text.align))
        .baseline(map_text_baseline(text.baseline))
        .build();

    elem = apply_fill_style(elem, &text.fill);
    apply_source_metadata(elem, text.source, config.include_source_spans, ir)
}

fn apply_source_metadata(
    mut elem: Element,
    source: RenderSource,
    include_source_spans: bool,
    ir: Option<&MermaidDiagramIr>,
) -> Element {
    match source {
        RenderSource::Diagram => {
            elem = elem.data("fm-source-kind", "diagram");
        }
        RenderSource::Node(index) => {
            elem = elem
                .data("fm-source-kind", "node")
                .data("fm-source-index", &index.to_string());
        }
        RenderSource::Edge(index) => {
            elem = elem
                .data("fm-source-kind", "edge")
                .data("fm-source-index", &index.to_string());
        }
        RenderSource::Cluster(index) => {
            elem = elem
                .data("fm-source-kind", "cluster")
                .data("fm-source-index", &index.to_string());
        }
    }

    if let Some(diagram_ir) = ir {
        match source {
            RenderSource::Node(index) => {
                if let Some(node) = diagram_ir.nodes.get(index) {
                    elem = elem
                        .attr("role", "graphics-symbol")
                        .attr("aria-label", &crate::a11y::describe_node(node, diagram_ir));
                }
            }
            RenderSource::Edge(index) => {
                if let Some(edge) = diagram_ir.edges.get(index) {
                    let from_node = diagram_ir
                        .resolve_endpoint_node(edge.from)
                        .and_then(|id| diagram_ir.nodes.get(id.0));
                    let to_node = diagram_ir
                        .resolve_endpoint_node(edge.to)
                        .and_then(|id| diagram_ir.nodes.get(id.0));
                    let label = edge
                        .label
                        .and_then(|lid| diagram_ir.labels.get(lid.0))
                        .map(|l| l.text.as_str());

                    elem = elem.attr("role", "graphics-symbol").attr(
                        "aria-label",
                        &crate::a11y::describe_edge(
                            from_node, to_node, edge.arrow, label, diagram_ir,
                        ),
                    );
                }
            }
            _ => {}
        }
    }

    if include_source_spans
        && let Some(span) = ir.and_then(|diagram_ir| render_source_span(diagram_ir, source))
    {
        elem = apply_span_metadata(elem, span);
    }

    elem
}

fn render_source_span(ir: &MermaidDiagramIr, source: RenderSource) -> Option<Span> {
    let span = match source {
        RenderSource::Diagram => return None,
        RenderSource::Node(index) => ir.nodes.get(index).map(|node| node.span_primary),
        RenderSource::Edge(index) => ir.edges.get(index).map(|edge| edge.span),
        RenderSource::Cluster(index) => ir.clusters.get(index).map(|cluster| cluster.span),
    }?;

    (!span.is_unknown()).then_some(span)
}

fn apply_span_metadata(mut elem: Element, span: Span) -> Element {
    if span.is_unknown() {
        return elem;
    }

    elem = elem.data("fm-source-span", &span.compact_display());
    elem = elem.data("fm-source-start-line", &span.start.line.to_string());
    elem = elem.data("fm-source-start-col", &span.start.col.to_string());
    elem = elem.data("fm-source-start-byte", &span.start.byte.to_string());
    elem = elem.data("fm-source-end-line", &span.end.line.to_string());
    elem = elem.data("fm-source-end-col", &span.end.col.to_string());
    elem.data("fm-source-end-byte", &span.end.byte.to_string())
}

fn register_clip_path(
    clip_defs: &mut Vec<Element>,
    clip: &RenderClip,
    clip_id_counter: &mut usize,
) -> String {
    let clip_id = format!("fm-scene-clip-{clip_id_counter}");
    *clip_id_counter += 1;

    let shape = match clip {
        RenderClip::Rect(rect) => Element::rect()
            .x(rect.x)
            .y(rect.y)
            .width(rect.width)
            .height(rect.height),
        RenderClip::Path(commands) => Element::path().d(&path_cmds_to_d(commands)),
    };

    clip_defs.push(Element::clip_path().id(&clip_id).child(shape));
    clip_id
}

fn scene_transform_value(transform: RenderTransform) -> String {
    match transform {
        RenderTransform::Matrix { a, b, c, d, e, f } => {
            TransformBuilder::new().matrix(a, b, c, d, e, f).build()
        }
    }
}

fn path_cmds_to_d(commands: &[PathCmd]) -> String {
    let mut builder = PathBuilder::new();
    for command in commands {
        builder = match *command {
            PathCmd::MoveTo { x, y } => builder.move_to(x, y),
            PathCmd::LineTo { x, y } => builder.line_to(x, y),
            PathCmd::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => builder.curve_to(c1x, c1y, c2x, c2y, x, y),
            PathCmd::QuadTo { cx, cy, x, y } => builder.quadratic_to(cx, cy, x, y),
            PathCmd::Close => builder.close(),
        };
    }
    builder.build()
}

fn apply_fill_style(mut elem: Element, fill: &FillStyle) -> Element {
    match fill {
        FillStyle::Solid { color, opacity } => {
            elem = elem.fill(color);
            if *opacity < 0.999 {
                elem = elem.fill_opacity(clamp_unit_interval(*opacity));
            }
        }
    }
    elem
}

fn apply_stroke_style(mut elem: Element, stroke: &StrokeStyle) -> Element {
    elem = elem.stroke(&stroke.color).stroke_width(stroke.width);

    if stroke.opacity < 0.999 {
        elem = elem.stroke_opacity(clamp_unit_interval(stroke.opacity));
    }

    if !stroke.dash_array.is_empty() {
        let dasharray = stroke
            .dash_array
            .iter()
            .map(|value| fmt_svg_number(*value))
            .collect::<Vec<_>>()
            .join(",");
        elem = elem.stroke_dasharray(&dasharray);
    }

    elem = elem.stroke_linecap(map_line_cap(stroke.line_cap));
    elem.stroke_linejoin(map_line_join(stroke.line_join))
}

fn fmt_svg_number(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i32)
    } else {
        format!("{value:.2}")
    }
}

fn map_line_cap(cap: RenderLineCap) -> &'static str {
    match cap {
        RenderLineCap::Butt => "butt",
        RenderLineCap::Round => "round",
        RenderLineCap::Square => "square",
    }
}

fn map_line_join(join: RenderLineJoin) -> &'static str {
    match join {
        RenderLineJoin::Miter => "miter",
        RenderLineJoin::Round => "round",
        RenderLineJoin::Bevel => "bevel",
    }
}

fn map_text_align(align: RenderTextAlign) -> TextAnchor {
    match align {
        RenderTextAlign::Start => TextAnchor::Start,
        RenderTextAlign::Middle => TextAnchor::Middle,
        RenderTextAlign::End => TextAnchor::End,
    }
}

fn map_text_baseline(baseline: RenderTextBaseline) -> text::DominantBaseline {
    match baseline {
        RenderTextBaseline::Top => text::DominantBaseline::Hanging,
        RenderTextBaseline::Middle => text::DominantBaseline::Middle,
        RenderTextBaseline::Bottom => text::DominantBaseline::Alphabetic,
    }
}

fn clamp_font_size(candidate: f32, min_font_size: f32) -> f32 {
    candidate.max(min_font_size)
}

fn clamp_unit_interval(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn sanitize_css_token(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn split_inline_style_declarations(style: &str) -> Vec<&str> {
    let mut declarations = Vec::new();
    let mut start = 0_usize;
    let mut paren_depth = 0_usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for (index, ch) in style.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match ch {
            '\\' if quote.is_some() => escaped = true,
            '"' | '\'' => {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                }
            }
            '(' if quote.is_none() => paren_depth += 1,
            ')' if quote.is_none() => paren_depth = paren_depth.saturating_sub(1),
            ',' | ';' if quote.is_none() && paren_depth == 0 => {
                let declaration = style[start..index].trim();
                if !declaration.is_empty() {
                    declarations.push(declaration);
                }
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }

    let tail = style[start..].trim();
    if !tail.is_empty() {
        declarations.push(tail);
    }

    declarations
}

fn normalize_inline_style(parts: &[&str]) -> Option<String> {
    let inline = parts
        .iter()
        .flat_map(|part| split_inline_style_declarations(part))
        .map(str::trim)
        .filter(|declaration| !declaration.is_empty())
        .collect::<Vec<_>>()
        .join("; ");

    if inline.is_empty() {
        None
    } else {
        Some(inline)
    }
}

/// Resolve inline style for a node based on `classDef` and `style` directives.
///
/// Cascade: classDef properties (by class name) → style properties (by node ID).
/// Returns a CSS style string suitable for the `style` attribute, or `None`.
fn resolve_node_inline_style(ir: &MermaidDiagramIr, node_index: usize) -> Option<String> {
    use fm_core::IrStyleTarget;
    let node = ir.nodes.get(node_index)?;
    let node_id = fm_core::IrNodeId(node_index);

    // Collect class definitions.
    let mut class_defs: std::collections::BTreeMap<&str, &str> = std::collections::BTreeMap::new();
    for sr in &ir.style_refs {
        if let IrStyleTarget::Class(ref name) = sr.target {
            class_defs.insert(name.as_str(), sr.style.as_str());
        }
    }

    let mut parts: Vec<&str> = Vec::new();

    // Layer 1: classDef properties (applied in class order for stability).
    for class_name in &node.classes {
        if let Some(&style) = class_defs.get(class_name.as_str()) {
            parts.push(style);
        }
    }

    // Layer 2: direct `style nodeId` overrides.
    for sr in &ir.style_refs {
        if let IrStyleTarget::Node(target_id) = sr.target
            && target_id == node_id
        {
            parts.push(sr.style.as_str());
        }
    }

    if parts.is_empty() {
        return None;
    }

    normalize_inline_style(&parts)
}

/// Resolve inline style for an edge based on `linkStyle` directives.
fn resolve_edge_inline_style(ir: &MermaidDiagramIr, edge_index: usize) -> Option<String> {
    use fm_core::IrStyleTarget;
    let mut parts: Vec<&str> = Vec::new();

    for sr in &ir.style_refs {
        if let IrStyleTarget::Link(link_idx) = sr.target
            && link_idx == edge_index
        {
            parts.push(sr.style.as_str());
        }
    }

    if parts.is_empty() {
        return None;
    }

    normalize_inline_style(&parts)
}

fn truncate_label(label: &str, max_chars: Option<usize>) -> String {
    let Some(limit) = max_chars else {
        return label.to_string();
    };
    let mut chars = label.chars();
    let needs_truncation = chars.clone().count() > limit;
    if !needs_truncation {
        return label.to_string();
    }
    let mut text = String::new();
    for _ in 0..limit.saturating_sub(1) {
        let Some(ch) = chars.next() else {
            break;
        };
        text.push(ch);
    }
    text.push('…');
    text
}

fn detail_tier_name(tier: RenderDetailTier) -> &'static str {
    match tier {
        RenderDetailTier::Compact => "compact",
        RenderDetailTier::Normal => "normal",
        RenderDetailTier::Rich => "rich",
    }
}

fn resolve_detail_profile(
    width: f32,
    height: f32,
    config: &SvgRenderConfig,
) -> RenderDetailProfile {
    let area = width * height;
    let tier = match config.detail_tier {
        MermaidTier::Compact => RenderDetailTier::Compact,
        MermaidTier::Normal => RenderDetailTier::Normal,
        MermaidTier::Rich => RenderDetailTier::Rich,
        MermaidTier::Auto => {
            if area < 56_000.0 {
                RenderDetailTier::Compact
            } else if area < 220_000.0 {
                RenderDetailTier::Normal
            } else {
                RenderDetailTier::Rich
            }
        }
    };

    match tier {
        RenderDetailTier::Rich => RenderDetailProfile {
            tier,
            show_node_labels: true,
            show_edge_labels: true,
            show_cluster_labels: true,
            node_label_max_chars: None,
            edge_label_max_chars: None,
            node_font_size: clamp_font_size(config.font_size, config.min_font_size),
            edge_font_size: clamp_font_size(config.font_size * 0.85, config.min_font_size),
            cluster_font_size: clamp_font_size(config.font_size * 0.9, config.min_font_size),
            enable_shadows: config.shadows,
        },
        RenderDetailTier::Normal => RenderDetailProfile {
            tier,
            show_node_labels: true,
            show_edge_labels: true,
            show_cluster_labels: true,
            node_label_max_chars: Some(48),
            edge_label_max_chars: Some(40),
            node_font_size: clamp_font_size(config.font_size * 0.92, config.min_font_size),
            edge_font_size: clamp_font_size(config.font_size * 0.82, config.min_font_size),
            cluster_font_size: clamp_font_size(config.font_size * 0.86, config.min_font_size),
            enable_shadows: config.shadows,
        },
        RenderDetailTier::Compact => {
            let show_node_labels = area >= 36_000.0 && width >= 240.0 && height >= 150.0;
            RenderDetailProfile {
                tier,
                show_node_labels,
                show_edge_labels: false,
                show_cluster_labels: false,
                node_label_max_chars: Some(20),
                edge_label_max_chars: Some(24),
                node_font_size: clamp_font_size(config.font_size * 0.78, config.min_font_size),
                edge_font_size: clamp_font_size(config.font_size * 0.74, config.min_font_size),
                cluster_font_size: clamp_font_size(config.font_size * 0.76, config.min_font_size),
                enable_shadows: false,
            }
        }
    }
}

fn node_gradient_for(config: &SvgRenderConfig, theme: &Theme) -> Option<Gradient> {
    if !config.node_gradients {
        return None;
    }
    let stops = vec![
        GradientStop::with_opacity(0.0, &theme.colors.node_fill, 1.0),
        GradientStop::with_opacity(0.55, &theme.colors.node_fill, 0.97),
        GradientStop::with_opacity(1.0, &theme.colors.background, 0.92),
    ];
    let gradient = match config.node_gradient_style {
        NodeGradientStyle::LinearVertical => {
            Gradient::linear_with_coords("fm-node-gradient", 0.0, 0.0, 0.0, 1.0, stops)
        }
        NodeGradientStyle::LinearHorizontal => {
            Gradient::linear_with_coords("fm-node-gradient", 0.0, 0.0, 1.0, 0.0, stops)
        }
        NodeGradientStyle::Radial => Gradient::radial("fm-node-gradient", 0.5, 0.45, 0.8, stops),
    };
    Some(gradient)
}

fn effects_css(config: &SvgRenderConfig) -> String {
    let inactive_opacity = clamp_unit_interval(config.inactive_opacity);
    let cluster_fill_opacity = clamp_unit_interval(config.cluster_fill_opacity);
    format!(
        ".fm-node-inactive {{ opacity: {inactive_opacity:.2}; }}\n\
.fm-node-highlighted rect,\n\
.fm-node-highlighted path,\n\
.fm-node-highlighted circle,\n\
.fm-node-highlighted ellipse,\n\
.fm-node-highlighted polygon {{\n\
  stroke-width: 2.4;\n\
}}\n\
.fm-node-highlighted text {{ font-weight: 600; }}\n\
.fm-node-border-dashed rect,\n\
.fm-node-border-dashed path,\n\
.fm-node-border-dashed circle,\n\
.fm-node-border-dashed ellipse,\n\
.fm-node-border-dashed polygon {{\n\
  stroke-dasharray: 6 4;\n\
}}\n\
.fm-node-border-double rect,\n\
.fm-node-border-double path,\n\
.fm-node-border-double circle,\n\
.fm-node-border-double ellipse,\n\
.fm-node-border-double polygon {{\n\
  stroke-width: 2.9;\n\
}}\n\
.fm-cluster {{ fill-opacity: {cluster_fill_opacity:.2}; }}\n"
    )
}

fn print_css(min_font_size: f32) -> String {
    format!(
        "@media print {{
  .fm-node text, .fm-edge-labeled text, .fm-cluster-label {{
    font-size: {min_font_size:.1}px !important;
    fill: #111 !important;
  }}
  .fm-node path, .fm-node rect, .fm-node circle, .fm-edge {{
    stroke: #111 !important;
  }}
  .fm-cluster {{
    fill: #fff !important;
    stroke: #666 !important;
  }}
}}"
    )
}

/// Render a computed layout to SVG.
fn render_layout_to_svg(
    layout: &DiagramLayout,
    ir: &MermaidDiagramIr,
    config: &SvgRenderConfig,
) -> String {
    let padding = config.padding;
    let width = layout.bounds.width + padding * 2.0;
    let height = layout.bounds.height + padding * 2.0;
    let detail = resolve_detail_profile(width, height, config);

    let mut doc = SvgDocument::new()
        .viewbox(0.0, 0.0, width, height)
        .preserve_aspect_ratio("xMidYMid meet");

    if config.responsive {
        doc = doc.responsive();
    }

    if config.accessible {
        // Use enhanced accessibility description if ARIA labels are enabled
        let desc = if config.a11y.aria_labels {
            describe_diagram(ir)
        } else {
            format!(
                "Diagram with {} nodes and {} edges",
                ir.nodes.len(),
                ir.edges.len()
            )
        };
        doc = doc.accessible(format!("{} diagram", ir.diagram_type.as_str()), desc);
    }

    for class in &config.root_classes {
        doc = doc.class(class);
    }

    // Add data attributes for tooling
    doc = doc
        .data("nodes", &ir.nodes.len().to_string())
        .data("edges", &ir.edges.len().to_string())
        .data("type", ir.diagram_type.as_str())
        .data("detail-tier", detail_tier_name(detail.tier));

    let theme = resolve_theme(Some(ir), config);
    let effects_enabled = config.node_gradients
        || config.glow_enabled
        || clamp_unit_interval(config.inactive_opacity) < 0.999
        || clamp_unit_interval(config.cluster_fill_opacity) < 0.999;

    // Build defs section
    let mut defs = DefsBuilder::new();

    // Add standard arrowhead markers
    defs = defs.marker(ArrowheadMarker::standard("arrow-end", &theme.colors.edge));
    defs = defs.marker(ArrowheadMarker::filled("arrow-filled", &theme.colors.edge));
    defs = defs.marker(ArrowheadMarker::open("arrow-open", &theme.colors.edge));
    defs = defs.marker(
        ArrowheadMarker::standard("arrow-start", &theme.colors.edge)
            .with_orient(crate::defs::MarkerOrient::AutoStartReverse),
    );
    defs = defs.marker(
        ArrowheadMarker::filled("arrow-start-filled", &theme.colors.edge)
            .with_orient(crate::defs::MarkerOrient::AutoStartReverse),
    );
    defs = defs.marker(ArrowheadMarker::circle_marker(
        "arrow-circle",
        &theme.colors.edge,
    ));
    defs = defs.marker(ArrowheadMarker::cross_marker(
        "arrow-cross",
        &theme.colors.edge,
    ));
    defs = defs.marker(ArrowheadMarker::diamond_marker(
        "arrow-diamond",
        &theme.colors.edge,
    ));

    // Add drop shadow filter if enabled
    if detail.enable_shadows {
        if config.shadow_color.trim().is_empty() {
            defs = defs.filter(Filter::drop_shadow(
                "drop-shadow",
                config.shadow_offset_x,
                config.shadow_offset_y,
                config.shadow_blur,
                clamp_unit_interval(config.shadow_opacity),
            ));
        } else {
            defs = defs.filter(Filter::drop_shadow_with_color(
                "drop-shadow",
                config.shadow_offset_x,
                config.shadow_offset_y,
                config.shadow_blur,
                clamp_unit_interval(config.shadow_opacity),
                &config.shadow_color,
            ));
        }
    }
    if config.glow_enabled {
        defs = defs.filter(Filter::drop_shadow_with_color(
            "node-glow",
            0.0,
            0.0,
            config.glow_blur,
            clamp_unit_interval(config.glow_opacity),
            &config.glow_color,
        ));
    }
    if let Some(gradient) = node_gradient_for(config, &theme) {
        defs = defs.gradient(gradient);
    }

    doc = doc.defs(defs);

    // Embed theme CSS if enabled
    if config.embed_theme_css {
        let mut css = theme.to_svg_style(detail.enable_shadows);
        if effects_enabled {
            css.push_str(&effects_css(config));
        }

        // Add accessibility CSS if enabled
        if config.a11y.accessibility_css {
            css.push_str(accessibility_css());
        }
        if config.print_optimized {
            css.push_str(&print_css(config.min_font_size));
        }

        doc = doc.style(css);
    } else if config.a11y.accessibility_css || config.print_optimized {
        // Only add supplemental CSS (accessibility and/or print optimization).
        let mut css = String::new();
        if effects_enabled {
            css.push_str(&effects_css(config));
        }
        if config.a11y.accessibility_css {
            css.push_str(accessibility_css());
        }
        if config.print_optimized {
            css.push_str(&print_css(config.min_font_size));
        }
        doc = doc.style(css);
    }

    // Offset for padding
    let offset_x = padding - layout.bounds.x;
    let offset_y = padding - layout.bounds.y;

    if let Some(xy_chart_meta) = ir
        .xy_chart_meta
        .as_ref()
        .filter(|meta| !meta.series.is_empty())
    {
        doc = render_xychart_svg(
            doc,
            layout,
            xy_chart_meta,
            offset_x,
            offset_y,
            config,
            &theme,
        );
        return doc.to_string();
    }

    for band in &layout.extensions.bands {
        doc = doc.child(render_layout_band(band, offset_x, offset_y, config));
    }
    for tick in &layout.extensions.axis_ticks {
        doc = doc.child(render_layout_axis_tick(
            tick.label.as_str(),
            tick.position + offset_x,
            layout.bounds.y + offset_y - 12.0,
            config,
        ));
    }

    // Render sequence diagram activation bars.
    for bar in &layout.extensions.activation_bars {
        let mut rect = Element::rect()
            .x(bar.bounds.x + offset_x)
            .y(bar.bounds.y + offset_y)
            .width(bar.bounds.width)
            .height(bar.bounds.height)
            .fill(&theme.colors.node_fill)
            .stroke(&theme.colors.node_stroke)
            .stroke_width(1.2)
            .class("fm-activation-bar");
        if bar.depth > 0 {
            rect = rect.class("fm-activation-nested");
        }
        doc = doc.child(rect);
    }

    // Render clusters (subgraphs) as background rectangles
    // Sort clusters by size (largest first) for proper z-ordering of nested clusters
    let mut sorted_clusters: Vec<_> = layout.clusters.iter().enumerate().collect();
    sorted_clusters.sort_by(|a, b| {
        let area_a = a.1.bounds.width * a.1.bounds.height;
        let area_b = b.1.bounds.width * b.1.bounds.height;
        area_b
            .partial_cmp(&area_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (_sort_idx, cluster) in sorted_clusters {
        let ir_cluster = ir.clusters.get(cluster.cluster_index);

        // Detect cluster type from title for specialized styling.
        let title_text = cluster
            .title
            .as_deref()
            .or_else(|| {
                ir_cluster
                    .and_then(|c| c.title)
                    .and_then(|tid| ir.labels.get(tid.0))
                    .map(|l| l.text.as_str())
            })
            .unwrap_or("");

        let is_c4_boundary = title_text.contains("System_Boundary")
            || title_text.contains("Container_Boundary")
            || title_text.contains("Enterprise_Boundary")
            || title_text.contains("Deployment_Node");

        let is_swimlane = title_text.starts_with("swimlane:")
            || title_text.contains("section ")
            || ir.diagram_type.as_str() == "gantt"
            || ir.diagram_type.as_str() == "kanban";

        // Configure styling based on cluster type
        let (fill_color, stroke_color, stroke_style, label_color) = if is_c4_boundary {
            // C4 boundaries: dashed gray border, very light gray fill
            ("rgba(128,128,128,0.05)", "#888", Some("4,2"), "#555")
        } else if is_swimlane {
            // Swimlanes: solid subtle border, alternating translucent fill
            ("rgba(200,220,240,0.15)", "#b8c9db", None, "#4a6785")
        } else {
            // Standard clusters: translucent fill, subtle border
            ("rgba(248,249,250,0.85)", "#dee2e6", None, "#6c757d")
        };

        let mut rect = Element::rect()
            .x(cluster.bounds.x + offset_x)
            .y(cluster.bounds.y + offset_y)
            .width(cluster.bounds.width)
            .height(cluster.bounds.height)
            .fill(fill_color)
            .stroke(stroke_color)
            .stroke_width(1.0)
            .rx(if is_c4_boundary {
                0.0
            } else {
                config.rounded_corners
            })
            .class("fm-cluster");
        if config.cluster_fill_opacity < 0.999 {
            rect = rect.attr_num(
                "fill-opacity",
                clamp_unit_interval(config.cluster_fill_opacity),
            );
        }

        if let Some(dasharray) = stroke_style {
            rect = rect.stroke_dasharray(dasharray);
        }

        if is_c4_boundary {
            rect = rect.class("fm-cluster-c4");
        } else if is_swimlane {
            rect = rect.class("fm-cluster-swimlane");
        }

        if config.include_source_spans {
            rect = apply_span_metadata(rect, cluster.span);
        }

        doc = doc.child(rect);

        // Cluster label if present
        if detail.show_cluster_labels && !title_text.is_empty() {
            // For C4 boundaries, strip the boundary type prefix for display
            let display_title = if is_c4_boundary {
                title_text
                    .replace("System_Boundary", "")
                    .replace("Container_Boundary", "")
                    .replace("Enterprise_Boundary", "")
                    .replace("Deployment_Node", "")
                    .trim_matches(|c: char| c == '(' || c == ')' || c == ',' || c.is_whitespace())
                    .to_string()
            } else if is_swimlane && title_text.starts_with("swimlane:") {
                title_text.trim_start_matches("swimlane:").to_string()
            } else if is_swimlane && title_text.starts_with("section ") {
                title_text.trim_start_matches("section ").to_string()
            } else {
                title_text.to_string()
            };

            if !display_title.is_empty() {
                let text = TextBuilder::new(&display_title)
                    .x(cluster.bounds.x + offset_x + 8.0)
                    .y(cluster.bounds.y + offset_y + 16.0)
                    .font_family(&config.font_family)
                    .font_size(detail.cluster_font_size)
                    .fill(label_color)
                    .class("fm-cluster-label")
                    .build();
                let text = if config.include_source_spans {
                    apply_span_metadata(text, cluster.span)
                } else {
                    text
                };
                doc = doc.child(text);
            }
        }
    }

    for divider in &layout.extensions.cluster_dividers {
        let cluster_span = ir
            .clusters
            .get(divider.cluster_index)
            .map_or(Span::default(), |cluster| cluster.span);
        let mut line = Element::line()
            .x1(divider.start.x + offset_x)
            .y1(divider.start.y + offset_y)
            .x2(divider.end.x + offset_x)
            .y2(divider.end.y + offset_y)
            .stroke("#64748b")
            .stroke_width(1.0)
            .stroke_dasharray("6,4")
            .class("fm-cluster-divider");

        if config.include_source_spans {
            line = apply_span_metadata(line, cluster_span);
        }

        doc = doc.child(line);
    }

    // Render edges
    for edge_path in &layout.edges {
        let edge_elem = render_edge(
            edge_path,
            ir,
            offset_x,
            offset_y,
            config,
            detail,
            &theme.colors,
        );
        doc = doc.child(edge_elem);
    }

    // Render nodes
    for node_box in &layout.nodes {
        let node_elem = render_node(
            node_box,
            ir,
            offset_x,
            offset_y,
            config,
            detail,
            &theme.colors,
        );
        doc = doc.child(node_elem);
    }

    doc.to_string()
}

fn render_layout_band(
    band: &LayoutBand,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
) -> Element {
    let (fill, stroke, class_name) = match band.kind {
        LayoutBandKind::Section => (
            "rgba(191,219,254,0.18)",
            "#bfd7ff",
            "fm-band fm-band-section",
        ),
        LayoutBandKind::Lane => ("rgba(196,181,253,0.14)", "#c4b5fd", "fm-band fm-band-lane"),
        LayoutBandKind::Column => (
            "rgba(254,240,138,0.16)",
            "#fde68a",
            "fm-band fm-band-column",
        ),
    };

    let mut group = Element::group().class(class_name);
    let rect = Element::rect()
        .x(band.bounds.x + offset_x)
        .y(band.bounds.y + offset_y)
        .width(band.bounds.width)
        .height(band.bounds.height)
        .rx(config.rounded_corners.max(4.0))
        .fill(fill)
        .stroke(stroke)
        .stroke_width(1.0)
        .stroke_dasharray("6,4")
        .fill_opacity(0.8)
        .stroke_opacity(0.9);
    group = group.child(rect);

    if !band.label.is_empty() {
        group = group.child(
            TextBuilder::new(&band.label)
                .x(band.bounds.x + offset_x + 8.0)
                .y(band.bounds.y + offset_y + 16.0)
                .font_family(&config.font_family)
                .font_size(clamp_font_size(
                    config.font_size * 0.82,
                    config.min_font_size,
                ))
                .fill("#4a5568")
                .class("fm-band-label")
                .build(),
        );
    }

    group
}

fn render_layout_axis_tick(label: &str, x: f32, y: f32, config: &SvgRenderConfig) -> Element {
    let mut group = Element::group().class("fm-axis-tick");
    group = group.child(
        Element::line()
            .x1(x)
            .y1(y + 4.0)
            .x2(x)
            .y2(y + 16.0)
            .stroke("#94a3b8")
            .stroke_width(1.0),
    );
    group.child(
        TextBuilder::new(label)
            .x(x + 3.0)
            .y(y)
            .font_family(&config.font_family)
            .font_size(clamp_font_size(
                config.font_size * 0.72,
                config.min_font_size,
            ))
            .fill("#64748b")
            .class("fm-axis-tick-label")
            .build(),
    )
}

fn render_xychart_svg(
    mut doc: SvgDocument,
    layout: &DiagramLayout,
    xy_chart_meta: &IrXyChartMeta,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    theme: &Theme,
) -> SvgDocument {
    let plot_bounds = xychart_plot_bounds(layout);
    let plot_x = plot_bounds.x + offset_x;
    let plot_y = plot_bounds.y + offset_y;
    let plot_bottom = plot_y + plot_bounds.height;
    let plot_right = plot_x + plot_bounds.width;
    let (y_min, y_max) = resolve_xychart_y_domain(xy_chart_meta);
    let baseline_value = y_min.min(0.0).max(y_max.min(0.0));
    let baseline_y = xychart_value_to_y(baseline_value, y_min, y_max, plot_bounds) + offset_y;
    let categories = xychart_categories(xy_chart_meta);
    let palette = theme.colors.accents.clone();

    doc = doc.child(
        Element::rect()
            .x(plot_x)
            .y(plot_y)
            .width(plot_bounds.width)
            .height(plot_bounds.height)
            .fill("rgba(148,163,184,0.06)")
            .stroke("rgba(148,163,184,0.16)")
            .stroke_width(1.0)
            .rx(config.rounded_corners.max(6.0))
            .class("fm-xychart-plot"),
    );

    for tick_index in 0..=4 {
        let tick_ratio = tick_index as f32 / 4.0;
        let tick_y = plot_y + plot_bounds.height - (plot_bounds.height * tick_ratio);
        let tick_value = y_min + (y_max - y_min) * tick_ratio;
        doc = doc.child(
            Element::line()
                .x1(plot_x)
                .y1(tick_y)
                .x2(plot_right)
                .y2(tick_y)
                .stroke("rgba(148,163,184,0.35)")
                .stroke_width(1.0)
                .stroke_dasharray("4,4")
                .class("fm-xychart-gridline"),
        );
        doc = doc.child(
            TextBuilder::new(&format_xychart_tick_value(tick_value))
                .x(plot_x - 10.0)
                .y(tick_y + 4.0)
                .anchor(TextAnchor::End)
                .font_family(&config.font_family)
                .font_size(clamp_font_size(
                    config.font_size * 0.72,
                    config.min_font_size,
                ))
                .fill(&theme.colors.edge)
                .class("fm-xychart-y-tick")
                .build(),
        );
    }

    doc = doc.child(
        Element::line()
            .x1(plot_x)
            .y1(plot_bottom)
            .x2(plot_right)
            .y2(plot_bottom)
            .stroke(&theme.colors.edge)
            .stroke_width(1.5)
            .class("fm-xychart-axis fm-xychart-axis-x"),
    );
    doc = doc.child(
        Element::line()
            .x1(plot_x)
            .y1(plot_y)
            .x2(plot_x)
            .y2(plot_bottom)
            .stroke(&theme.colors.edge)
            .stroke_width(1.5)
            .class("fm-xychart-axis fm-xychart-axis-y"),
    );

    let band_width = plot_bounds.width / categories.len().max(1) as f32;
    for (index, category) in categories.iter().enumerate() {
        let x = plot_x + band_width * (index as f32 + 0.5);
        doc = doc.child(
            TextBuilder::new(category)
                .x(x)
                .y(plot_bottom + 24.0)
                .anchor(TextAnchor::Middle)
                .font_family(&config.font_family)
                .font_size(clamp_font_size(
                    config.font_size * 0.74,
                    config.min_font_size,
                ))
                .fill(&theme.colors.text)
                .class("fm-xychart-x-tick")
                .build(),
        );
    }

    if let Some(title) = xy_chart_meta.title.as_deref() {
        doc = doc.child(
            TextBuilder::new(title)
                .x((layout.bounds.width / 2.0) + offset_x)
                .y(plot_y - 34.0)
                .anchor(TextAnchor::Middle)
                .font_family(&config.font_family)
                .font_size(clamp_font_size(
                    config.font_size * 1.18,
                    config.min_font_size,
                ))
                .font_weight("600")
                .fill(&theme.colors.text)
                .class("fm-xychart-title")
                .build(),
        );
    }

    if let Some(y_label) = xy_chart_meta.y_axis.label.as_deref() {
        doc = doc.child(
            TextBuilder::new(y_label)
                .x(plot_x - 52.0)
                .y(plot_y - 12.0)
                .font_family(&config.font_family)
                .font_size(clamp_font_size(
                    config.font_size * 0.76,
                    config.min_font_size,
                ))
                .fill(&theme.colors.text)
                .class("fm-xychart-y-label")
                .build(),
        );
    }

    for (series_index, series) in xy_chart_meta.series.iter().enumerate() {
        let color = &palette[series_index % palette.len()];
        let series_nodes: Vec<_> = series
            .nodes
            .iter()
            .filter_map(|node_id| {
                layout
                    .nodes
                    .iter()
                    .find(|node| node.node_index == node_id.0)
            })
            .collect();

        match series.kind {
            IrXySeriesKind::Bar => {
                for node in series_nodes {
                    let mut rect = Element::rect()
                        .x(node.bounds.x + offset_x)
                        .y(node.bounds.y + offset_y)
                        .width(node.bounds.width)
                        .height(node.bounds.height)
                        .fill(color)
                        .fill_opacity(0.78)
                        .stroke(color)
                        .stroke_width(1.0)
                        .rx((config.rounded_corners * 0.45).max(3.0))
                        .class("fm-xychart-bar");
                    if config.include_source_spans {
                        rect = apply_span_metadata(rect, node.span);
                    }
                    doc = doc.child(rect);
                }
            }
            IrXySeriesKind::Line | IrXySeriesKind::Area => {
                if series_nodes.is_empty() {
                    continue;
                }
                let points: Vec<(f32, f32)> = series_nodes
                    .iter()
                    .map(|node| {
                        let center = node.bounds.center();
                        (center.x + offset_x, center.y + offset_y)
                    })
                    .collect();

                if matches!(series.kind, IrXySeriesKind::Area) {
                    let first_x = points.first().map_or(plot_x, |point| point.0);
                    let last_x = points.last().map_or(plot_x, |point| point.0);
                    let mut fill_points = vec![(first_x, baseline_y)];
                    fill_points.extend(points.iter().copied());
                    fill_points.push((last_x, baseline_y));
                    let mut area_path =
                        PathBuilder::new().move_to(fill_points[0].0, fill_points[0].1);
                    for point in fill_points.iter().skip(1) {
                        area_path = area_path.line_to(point.0, point.1);
                    }
                    area_path = area_path.close();
                    doc = doc.child(
                        Element::path()
                            .d(&area_path.build())
                            .fill(color)
                            .fill_opacity(0.16)
                            .stroke("none")
                            .class("fm-xychart-area"),
                    );
                }

                let mut line_path = PathBuilder::new().move_to(points[0].0, points[0].1);
                for point in points.iter().skip(1) {
                    line_path = line_path.line_to(point.0, point.1);
                }
                doc = doc.child(
                    Element::path()
                        .d(&line_path.build())
                        .fill("none")
                        .stroke(color)
                        .stroke_width(3.0)
                        .stroke_linecap("round")
                        .stroke_linejoin("round")
                        .class("fm-xychart-line"),
                );

                for node in series_nodes {
                    let center = node.bounds.center();
                    let mut point = Element::circle()
                        .cx(center.x + offset_x)
                        .cy(center.y + offset_y)
                        .r((node.bounds.width.min(node.bounds.height) / 2.0).max(3.5))
                        .fill(color)
                        .stroke(&theme.colors.background)
                        .stroke_width(2.0)
                        .class("fm-xychart-point");
                    if config.include_source_spans {
                        point = apply_span_metadata(point, node.span);
                    }
                    doc = doc.child(point);
                }
            }
        }
    }

    doc
}

fn xychart_plot_bounds(layout: &DiagramLayout) -> fm_layout::LayoutRect {
    const LEFT_MARGIN: f32 = 88.0;
    const TOP_MARGIN: f32 = 84.0;
    const RIGHT_MARGIN: f32 = 36.0;
    const BOTTOM_MARGIN: f32 = 76.0;

    fm_layout::LayoutRect {
        x: layout.bounds.x + LEFT_MARGIN,
        y: layout.bounds.y + TOP_MARGIN,
        width: (layout.bounds.width - LEFT_MARGIN - RIGHT_MARGIN).max(1.0),
        height: (layout.bounds.height - TOP_MARGIN - BOTTOM_MARGIN).max(1.0),
    }
}

fn xychart_categories(xy_chart_meta: &IrXyChartMeta) -> Vec<String> {
    if !xy_chart_meta.x_axis.categories.is_empty() {
        return xy_chart_meta.x_axis.categories.clone();
    }

    let count = xy_chart_meta
        .series
        .iter()
        .map(|series| series.values.len())
        .max()
        .unwrap_or(0);
    let (x_min, x_max) = resolve_xychart_x_domain(xy_chart_meta, count);
    if count <= 1 {
        return vec![format_xychart_tick_value(x_min)];
    }
    let step = (x_max - x_min) / (count.saturating_sub(1) as f32).max(1.0);
    (0..count)
        .map(|index| format_xychart_tick_value(x_min + step * index as f32))
        .collect()
}

fn resolve_xychart_x_domain(xy_chart_meta: &IrXyChartMeta, count: usize) -> (f32, f32) {
    let min = xy_chart_meta.x_axis.min.unwrap_or(0.0);
    let max = xy_chart_meta
        .x_axis
        .max
        .unwrap_or_else(|| count.saturating_sub(1) as f32);
    if (max - min).abs() < f32::EPSILON {
        (min, min + 1.0)
    } else {
        (min, max)
    }
}

fn resolve_xychart_y_domain(xy_chart_meta: &IrXyChartMeta) -> (f32, f32) {
    let mut min_value = xy_chart_meta.y_axis.min.unwrap_or(f32::INFINITY);
    let mut max_value = xy_chart_meta.y_axis.max.unwrap_or(f32::NEG_INFINITY);

    if xy_chart_meta.y_axis.min.is_none() || xy_chart_meta.y_axis.max.is_none() {
        for value in xy_chart_meta
            .series
            .iter()
            .flat_map(|series| series.values.iter().copied())
        {
            min_value = min_value.min(value);
            max_value = max_value.max(value);
        }
    }

    if !min_value.is_finite() || !max_value.is_finite() {
        return (0.0, 1.0);
    }
    if xy_chart_meta.y_axis.min.is_none() && min_value > 0.0 {
        min_value = 0.0;
    }
    if xy_chart_meta.y_axis.max.is_none() && max_value < 0.0 {
        max_value = 0.0;
    }
    if (max_value - min_value).abs() < f32::EPSILON {
        max_value += 1.0;
    }
    (min_value, max_value)
}

fn xychart_value_to_y(
    value: f32,
    y_min: f32,
    y_max: f32,
    plot_bounds: fm_layout::LayoutRect,
) -> f32 {
    let range = (y_max - y_min).max(f32::EPSILON);
    let ratio = ((value - y_min) / range).clamp(0.0, 1.0);
    plot_bounds.y + plot_bounds.height - (ratio * plot_bounds.height)
}

fn format_xychart_tick_value(value: f32) -> String {
    if (value - value.round()).abs() < 0.0001 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// Render a single node to an SVG element.
fn render_node(
    node_box: &LayoutNodeBox,
    ir: &MermaidDiagramIr,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    detail: RenderDetailProfile,
    colors: &ThemeColors,
) -> Element {
    use fm_core::NodeShape;

    let ir_node = ir.nodes.get(node_box.node_index);
    let shape = ir_node.map_or(NodeShape::Rect, |n| n.shape);
    let node_id = ir_node
        .map(|node| node.id.as_str())
        .unwrap_or_else(|| node_box.node_id.as_str());

    let x = node_box.bounds.x + offset_x;
    let y = node_box.bounds.y + offset_y;
    let w = node_box.bounds.width;
    let h = node_box.bounds.height;
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;

    // Get node label text
    let raw_label_text = ir_node
        .and_then(|n| n.label)
        .and_then(|lid| ir.labels.get(lid.0))
        .map(|l| l.text.as_str())
        .or_else(|| {
            ir_node.and_then(|node| match node.shape {
                NodeShape::FilledCircle => None,
                NodeShape::DoubleCircle if node.label.is_none() => None,
                NodeShape::HorizontalBar => None,
                _ => Some(node.id.as_str()),
            })
        })
        .unwrap_or("");
    let label_text = truncate_label(raw_label_text, detail.node_label_max_chars);
    let node_font_size = detail.node_font_size;

    let accent_class = format!("fm-node-accent-{}", stable_accent_index(node_id));
    let mut is_highlighted = false;
    let mut is_inactive = false;
    let mut dashed_border = false;
    let mut double_border = false;

    // Create group for node shape + label
    let mut group = Element::group()
        .class("fm-node")
        .class(&accent_class)
        .class(node_shape_css_class(shape))
        .data("id", node_id)
        .data("fm-node-id", node_id);
    if config.include_source_spans {
        group = apply_span_metadata(group, node_box.span);
    }

    if let Some(node) = ir_node {
        for class in &node.classes {
            let normalized = class.to_ascii_lowercase();
            let sanitized = sanitize_css_token(class);
            if !sanitized.is_empty() {
                group = group.class(&format!("fm-node-user-{sanitized}"));
            }
            if normalized.contains("highlight")
                || normalized.contains("selected")
                || normalized.contains("active")
                || normalized.contains("focus")
                || normalized.contains("important")
            {
                is_highlighted = true;
            }
            if normalized.contains("inactive")
                || normalized.contains("dim")
                || normalized.contains("muted")
                || normalized.contains("disabled")
            {
                is_inactive = true;
            }
            if normalized.contains("dashed-border") || normalized.contains("border-dashed") {
                dashed_border = true;
            }
            if normalized.contains("double-border") || normalized.contains("border-double") {
                double_border = true;
            }
        }
    }
    if is_highlighted {
        group = group.class("fm-node-highlighted");
    }
    if is_inactive {
        group = group.class("fm-node-inactive");
    }
    if dashed_border {
        group = group.class("fm-node-border-dashed");
    }
    if double_border {
        group = group.class("fm-node-border-double");
    }

    // Add accessibility attributes
    if config.a11y.aria_labels {
        group = group
            .attr("role", "graphics-symbol")
            .attr("aria-label", raw_label_text);
    }

    if config.a11y.keyboard_nav {
        group = group.attr("tabindex", "0");
    }

    // Create shape element based on node type
    let shape_elem = match shape {
        NodeShape::Rect => Element::rect()
            .x(x)
            .y(y)
            .width(w)
            .height(h)
            .fill(&colors.node_fill)
            .stroke(&colors.node_stroke)
            .stroke_width(1.6)
            .rx(config.rounded_corners * 0.55),

        NodeShape::Rounded => Element::rect()
            .x(x)
            .y(y)
            .width(w)
            .height(h)
            .fill(&colors.node_fill)
            .stroke(&colors.node_stroke)
            .stroke_width(1.6)
            .rx(config.rounded_corners),

        NodeShape::Stadium => Element::rect()
            .x(x)
            .y(y)
            .width(w)
            .height(h)
            .fill(&colors.node_fill)
            .stroke(&colors.node_stroke)
            .stroke_width(1.6)
            .rx(w.min(h) / 2.0),

        NodeShape::Diamond => {
            let path = PathBuilder::new()
                .move_to(cx, y)
                .line_to(x + w, cy)
                .line_to(cx, y + h)
                .line_to(x, cy)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.6)
        }

        NodeShape::Hexagon => {
            let inset = w * 0.15;
            let path = PathBuilder::new()
                .move_to(x + inset, y)
                .line_to(x + w - inset, y)
                .line_to(x + w, cy)
                .line_to(x + w - inset, y + h)
                .line_to(x + inset, y + h)
                .line_to(x, cy)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.6)
        }

        NodeShape::Circle | NodeShape::FilledCircle | NodeShape::DoubleCircle => {
            let r = w.min(h) / 2.0;
            let mut elem = Element::circle()
                .cx(cx)
                .cy(cy)
                .r(r)
                .fill(if shape == NodeShape::FilledCircle {
                    colors.node_stroke.as_str()
                } else {
                    colors.node_fill.as_str()
                })
                .stroke(&colors.node_stroke)
                .stroke_width(1.6);

            if shape == NodeShape::DoubleCircle {
                // For double circle, we'll use a slightly smaller stroke
                elem = elem.stroke_width(2.0);
            }
            elem
        }

        NodeShape::HorizontalBar => Element::rect()
            .x(x)
            .y(y + h * 0.25)
            .width(w)
            .height((h * 0.5).max(8.0))
            .fill(&colors.node_stroke)
            .stroke(&colors.node_stroke)
            .stroke_width(1.0)
            .rx((h * 0.25).max(3.0)),

        NodeShape::Cylinder => {
            let ry = h * 0.1;
            let path = PathBuilder::new()
                .move_to(x, y + ry)
                .arc_to(w / 2.0, ry, 0.0, false, true, x + w, y + ry)
                .line_to(x + w, y + h - ry)
                .arc_to(w / 2.0, ry, 0.0, false, false, x, y + h - ry)
                .close()
                .move_to(x, y + ry)
                .arc_to(w / 2.0, ry, 0.0, false, false, x + w, y + ry)
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.6)
        }

        NodeShape::Trapezoid => {
            let inset = w * 0.15;
            let path = PathBuilder::new()
                .move_to(x + inset, y)
                .line_to(x + w - inset, y)
                .line_to(x + w, y + h)
                .line_to(x, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.6)
        }

        NodeShape::Subroutine => {
            let inset = 8.0;
            let mut g = Element::group();
            g = g.child(
                Element::rect()
                    .x(x)
                    .y(y)
                    .width(w)
                    .height(h)
                    .fill(if config.node_gradients {
                        "url(#fm-node-gradient)"
                    } else {
                        colors.node_fill.as_str()
                    })
                    .stroke(&colors.node_stroke)
                    .stroke_width(1.6)
                    .rx(config.rounded_corners * 0.45),
            );
            // Left vertical line
            g = g.child(
                Element::line()
                    .x1(x + inset)
                    .y1(y)
                    .x2(x + inset)
                    .y2(y + h)
                    .stroke(&colors.node_stroke)
                    .stroke_width(1.0),
            );
            // Right vertical line
            g = g.child(
                Element::line()
                    .x1(x + w - inset)
                    .y1(y)
                    .x2(x + w - inset)
                    .y2(y + h)
                    .stroke(&colors.node_stroke)
                    .stroke_width(1.0),
            );
            if detail.show_node_labels {
                return group.child(g).child(
                    TextBuilder::new(&label_text)
                        .x(cx)
                        .y(cy + node_font_size / 3.0)
                        .font_family(&config.font_family)
                        .font_size(node_font_size)
                        .anchor(TextAnchor::Middle)
                        .fill(&colors.text)
                        .build(),
                );
            }
            return group.child(g);
        }

        NodeShape::Asymmetric => {
            let flag = w * 0.15;
            let path = PathBuilder::new()
                .move_to(x, y)
                .line_to(x + w - flag, y)
                .line_to(x + w, cy)
                .line_to(x + w - flag, y + h)
                .line_to(x, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.6)
        }

        NodeShape::Note => {
            let fold = 10.0;
            let path = PathBuilder::new()
                .move_to(x, y)
                .line_to(x + w - fold, y)
                .line_to(x + w, y + fold)
                .line_to(x + w, y + h)
                .line_to(x, y + h)
                .close()
                .move_to(x + w - fold, y)
                .line_to(x + w - fold, y + fold)
                .line_to(x + w, y + fold)
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.0)
        }

        // Extended shapes for FrankenMermaid
        NodeShape::InvTrapezoid => {
            let inset = w * 0.15;
            let path = PathBuilder::new()
                .move_to(x, y)
                .line_to(x + w, y)
                .line_to(x + w - inset, y + h)
                .line_to(x + inset, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.6)
        }

        NodeShape::Parallelogram => {
            let inset = w * 0.15;
            let path = PathBuilder::new()
                .move_to(x + inset, y)
                .line_to(x + w, y)
                .line_to(x + w - inset, y + h)
                .line_to(x, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.6)
        }

        NodeShape::InvParallelogram => {
            let inset = w * 0.15;
            let path = PathBuilder::new()
                .move_to(x, y)
                .line_to(x + w - inset, y)
                .line_to(x + w, y + h)
                .line_to(x + inset, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.6)
        }

        NodeShape::Triangle => {
            let path = PathBuilder::new()
                .move_to(cx, y)
                .line_to(x + w, y + h)
                .line_to(x, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.6)
        }

        NodeShape::Pentagon => {
            // Regular pentagon (5 sides)
            let angle_offset = -std::f32::consts::FRAC_PI_2; // Start at top
            let r = w.min(h) / 2.0;
            let mut path = PathBuilder::new();
            for i in 0..5 {
                let angle = angle_offset + (i as f32) * 2.0 * std::f32::consts::PI / 5.0;
                let px = cx + r * angle.cos();
                let py = cy + r * angle.sin();
                if i == 0 {
                    path = path.move_to(px, py);
                } else {
                    path = path.line_to(px, py);
                }
            }
            Element::path()
                .d(&path.close().build())
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.6)
        }

        NodeShape::Star => {
            // 5-pointed star
            let outer_r = w.min(h) / 2.0;
            let inner_r = outer_r * 0.4;
            let angle_offset = -std::f32::consts::FRAC_PI_2;
            let mut path = PathBuilder::new();
            for i in 0..10 {
                let r = if i % 2 == 0 { outer_r } else { inner_r };
                let angle = angle_offset + (i as f32) * std::f32::consts::PI / 5.0;
                let px = cx + r * angle.cos();
                let py = cy + r * angle.sin();
                if i == 0 {
                    path = path.move_to(px, py);
                } else {
                    path = path.line_to(px, py);
                }
            }
            Element::path()
                .d(&path.close().build())
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.6)
        }

        NodeShape::Cloud => {
            // Simplified cloud shape using circles
            let r = h / 3.0;
            let path = PathBuilder::new()
                .move_to(x + r, y + h * 0.6)
                .arc_to(r, r, 0.0, true, true, x + r * 2.0, y + h * 0.3)
                .arc_to(r * 0.8, r * 0.8, 0.0, true, true, x + w * 0.5, y + r * 0.5)
                .arc_to(r, r, 0.0, true, true, x + w - r * 2.0, y + h * 0.3)
                .arc_to(r, r, 0.0, true, true, x + w - r, y + h * 0.6)
                .arc_to(r * 0.7, r * 0.7, 0.0, true, true, x + w - r, y + h * 0.8)
                .line_to(x + r, y + h * 0.8)
                .arc_to(r * 0.7, r * 0.7, 0.0, true, true, x + r, y + h * 0.6)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.6)
        }

        NodeShape::Tag => {
            // Tag/flag shape (rectangle with arrow point on right)
            let point = w * 0.2;
            let path = PathBuilder::new()
                .move_to(x, y)
                .line_to(x + w - point, y)
                .line_to(x + w, cy)
                .line_to(x + w - point, y + h)
                .line_to(x, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke(&colors.node_stroke)
                .stroke_width(1.6)
        }

        NodeShape::CrossedCircle => {
            // Circle with X through it
            let r = w.min(h) / 2.0;
            let mut g = Element::group();
            g = g.child(
                Element::circle()
                    .cx(cx)
                    .cy(cy)
                    .r(r)
                    .fill(if config.node_gradients {
                        "url(#fm-node-gradient)"
                    } else {
                        colors.node_fill.as_str()
                    })
                    .stroke(&colors.node_stroke)
                    .stroke_width(1.6),
            );
            // Diagonal lines
            let offset = r * 0.707; // r * cos(45°)
            g = g.child(
                Element::line()
                    .x1(cx - offset)
                    .y1(cy - offset)
                    .x2(cx + offset)
                    .y2(cy + offset)
                    .stroke(&colors.node_stroke)
                    .stroke_width(1.6),
            );
            g = g.child(
                Element::line()
                    .x1(cx + offset)
                    .y1(cy - offset)
                    .x2(cx - offset)
                    .y2(cy + offset)
                    .stroke(&colors.node_stroke)
                    .stroke_width(1.6),
            );
            if detail.show_node_labels {
                return group.child(g).child(
                    TextBuilder::new(&label_text)
                        .x(cx)
                        .y(cy + node_font_size / 3.0)
                        .font_family(&config.font_family)
                        .font_size(node_font_size)
                        .anchor(TextAnchor::Middle)
                        .fill(&colors.text)
                        .build(),
                );
            }
            return group.child(g);
        }
    };

    let shape_elem = if config.node_gradients
        && !matches!(
            shape,
            NodeShape::Note | NodeShape::FilledCircle | NodeShape::HorizontalBar
        ) {
        shape_elem.fill("url(#fm-node-gradient)")
    } else {
        shape_elem
    };

    // Apply shadow filter if enabled and this isn't a special composite shape.
    // Highlighted nodes prefer glow so the effects don't visually muddy each other.
    let shape_elem = if detail.enable_shadows
        && !(is_highlighted && config.glow_enabled)
        && !matches!(
            shape,
            NodeShape::Subroutine
                | NodeShape::CrossedCircle
                | NodeShape::FilledCircle
                | NodeShape::HorizontalBar
        ) {
        shape_elem.filter("url(#drop-shadow)")
    } else {
        shape_elem
    };

    // Apply inline style from classDef/style directives if present.
    let shape_elem = if let Some(inline_style) = resolve_node_inline_style(ir, node_box.node_index)
    {
        shape_elem.attr("style", &inline_style)
    } else {
        shape_elem
    };

    group = group.child(shape_elem);
    if is_highlighted && config.glow_enabled {
        group = group.filter("url(#node-glow)");
    }

    // Add label text — with three-compartment rendering for class diagrams.
    if detail.show_node_labels {
        if let Some(node) = ir_node
            && let Some(ref meta) = node.class_meta
            && (!meta.attributes.is_empty() || !meta.methods.is_empty())
        {
            group = render_class_compartments(
                group,
                node,
                ir,
                x,
                y,
                w,
                h,
                node_font_size,
                config,
                colors,
            );
        } else {
            let lines_count = label_text.lines().count().max(1) as f32;
            let total_text_height = (lines_count - 1.0) * node_font_size * config.line_height;
            let start_y = cy - (total_text_height / 2.0) + (node_font_size / 3.0);

            let text_elem = TextBuilder::new(&label_text)
                .x(cx)
                .y(start_y)
                .font_family(&config.font_family)
                .font_size(node_font_size)
                .line_height(config.line_height)
                .anchor(TextAnchor::Middle)
                .fill(&colors.text)
                .build();
            group = group.child(text_elem);
        }
    }

    // Add title element for text alternatives
    if config.a11y.text_alternatives
        && let Some(node) = ir_node
    {
        let node_desc = describe_node(node, ir);
        group = group.child(Element::title(&node_desc));
    }

    if let Some(node) = ir_node
        && let Some(href) = &node.href
    {
        let mut a = Element::new(crate::element::ElementKind::A)
            .attr("href", href)
            .attr("target", "_blank")
            .attr("rel", "noopener noreferrer");

        // Add a cursor pointer style
        group = group.attr("style", "cursor: pointer;");

        a = a.child(group);
        return a;
    }

    group
}

fn stable_accent_index(node_id: &str) -> usize {
    // FNV-1a 32-bit hash for deterministic class assignment.
    let mut hash: u32 = 0x811c9dc5;
    for byte in node_id.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    (hash as usize % 8) + 1
}

/// Render a UML three-compartment class box: header | attributes | methods.
///
/// Adds separator lines and member text elements to the node group.
#[allow(clippy::too_many_arguments)]
fn render_class_compartments(
    mut group: Element,
    node: &fm_core::IrNode,
    ir: &MermaidDiagramIr,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    font_size: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
) -> Element {
    let meta = match &node.class_meta {
        Some(m) => m,
        None => return group,
    };

    let line_h = font_size * config.line_height;
    let padding_x = 8.0;
    let text_x = x + padding_x;
    let mut cursor_y = y + line_h;

    // Header: class name (centered, bold).
    let class_name = node
        .label
        .and_then(|lid| ir.labels.get(lid.0))
        .map(|l| l.text.as_str())
        .unwrap_or(&node.id);

    // Stereotype above class name if present.
    if let Some(ref stereotype) = meta.stereotype {
        let stereo_text = match stereotype {
            fm_core::ClassStereotype::Interface => "<<interface>>",
            fm_core::ClassStereotype::Abstract => "<<abstract>>",
            fm_core::ClassStereotype::Enum => "<<enumeration>>",
            fm_core::ClassStereotype::Service => "<<service>>",
            fm_core::ClassStereotype::Custom(s) => s.as_str(),
        };
        let stereo_elem = TextBuilder::new(stereo_text)
            .x(x + w / 2.0)
            .y(cursor_y)
            .font_family(&config.font_family)
            .font_size(font_size * 0.85)
            .anchor(TextAnchor::Middle)
            .italic()
            .fill(&colors.text)
            .build();
        group = group.child(stereo_elem);
        cursor_y += line_h;
    }

    // Append generic parameters to class name if present (e.g., "List<T>").
    let display_name = if meta.generics.is_empty() {
        class_name.to_string()
    } else {
        format!("{class_name}<{}>", meta.generics.join(", "))
    };

    let name_elem = TextBuilder::new(&display_name)
        .x(x + w / 2.0)
        .y(cursor_y)
        .font_family(&config.font_family)
        .font_size(font_size)
        .anchor(TextAnchor::Middle)
        .bold()
        .fill(&colors.text)
        .build();
    group = group.child(name_elem);
    cursor_y += line_h * 0.5;

    // Separator line after header.
    let sep1 = Element::new(crate::element::ElementKind::Line)
        .attr_num("x1", x)
        .attr_num("y1", cursor_y)
        .attr_num("x2", x + w)
        .attr_num("y2", cursor_y)
        .stroke(&colors.node_stroke)
        .stroke_width(1.0);
    group = group.child(sep1);
    cursor_y += line_h * 0.3;

    // Attributes compartment.
    let member_font_size = font_size * 0.9;
    for attr in &meta.attributes {
        if cursor_y > y + h - line_h {
            break;
        }
        cursor_y += member_font_size * config.line_height * 0.9;
        let vis = visibility_symbol(attr.visibility);
        let text = if let Some(ref ret) = attr.return_type {
            format!("{vis}{}: {ret}", attr.name)
        } else {
            format!("{vis}{}", attr.name)
        };
        let elem = TextBuilder::new(&text)
            .x(text_x)
            .y(cursor_y)
            .font_family(&config.font_family)
            .font_size(member_font_size)
            .anchor(TextAnchor::Start)
            .fill(&colors.text)
            .build();
        group = group.child(elem);
    }

    // Separator before methods (only if both sections present).
    if !meta.attributes.is_empty() && !meta.methods.is_empty() {
        cursor_y += line_h * 0.3;
        let sep2 = Element::new(crate::element::ElementKind::Line)
            .attr_num("x1", x)
            .attr_num("y1", cursor_y)
            .attr_num("x2", x + w)
            .attr_num("y2", cursor_y)
            .stroke(&colors.node_stroke)
            .stroke_width(1.0);
        group = group.child(sep2);
        cursor_y += line_h * 0.3;
    }

    // Methods compartment.
    for method in &meta.methods {
        if cursor_y > y + h - line_h * 0.5 {
            break;
        }
        cursor_y += member_font_size * config.line_height * 0.9;
        let vis = visibility_symbol(method.visibility);
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
        let elem = TextBuilder::new(&text)
            .x(text_x)
            .y(cursor_y)
            .font_family(&config.font_family)
            .font_size(member_font_size)
            .anchor(TextAnchor::Start)
            .fill(&colors.text)
            .build();
        group = group.child(elem);
    }

    group
}

fn visibility_symbol(vis: fm_core::ClassVisibility) -> &'static str {
    match vis {
        fm_core::ClassVisibility::Public => "+",
        fm_core::ClassVisibility::Private => "-",
        fm_core::ClassVisibility::Protected => "#",
        fm_core::ClassVisibility::Package => "~",
    }
}

const fn node_shape_css_class(shape: fm_core::NodeShape) -> &'static str {
    use fm_core::NodeShape;
    match shape {
        NodeShape::Rect => "fm-node-shape-rect",
        NodeShape::Rounded => "fm-node-shape-rounded",
        NodeShape::Stadium => "fm-node-shape-stadium",
        NodeShape::Subroutine => "fm-node-shape-subroutine",
        NodeShape::Diamond => "fm-node-shape-diamond",
        NodeShape::Hexagon => "fm-node-shape-hexagon",
        NodeShape::Circle => "fm-node-shape-circle",
        NodeShape::FilledCircle => "fm-node-shape-filled-circle",
        NodeShape::Asymmetric => "fm-node-shape-asymmetric",
        NodeShape::Cylinder => "fm-node-shape-cylinder",
        NodeShape::Trapezoid => "fm-node-shape-trapezoid",
        NodeShape::DoubleCircle => "fm-node-shape-double-circle",
        NodeShape::HorizontalBar => "fm-node-shape-horizontal-bar",
        NodeShape::Note => "fm-node-shape-note",
        NodeShape::InvTrapezoid => "fm-node-shape-inv-trapezoid",
        NodeShape::Parallelogram => "fm-node-shape-parallelogram",
        NodeShape::InvParallelogram => "fm-node-shape-inv-parallelogram",
        NodeShape::Triangle => "fm-node-shape-triangle",
        NodeShape::Pentagon => "fm-node-shape-pentagon",
        NodeShape::Star => "fm-node-shape-star",
        NodeShape::Cloud => "fm-node-shape-cloud",
        NodeShape::Tag => "fm-node-shape-tag",
        NodeShape::CrossedCircle => "fm-node-shape-crossed-circle",
    }
}

/// Build a smooth SVG path `d` attribute from a series of points using
/// Catmull-Rom to cubic bezier conversion.  For 2 or fewer points a simple
/// polyline is produced; for 3+ points each interior segment is drawn as a
/// cubic bezier curve giving a natural, rounded appearance.
///
/// A `tension` factor of 0.25 (1/4) is used so curves stay close to the
/// original waypoints while still looking smooth.
fn smooth_edge_path(points: &[(f32, f32)], _is_self_loop: bool) -> String {
    let n = points.len();
    if n == 0 {
        return String::new();
    }

    let mut pb = PathBuilder::new();
    pb = pb.move_to(points[0].0, points[0].1);

    if n == 1 {
        return pb.build();
    }

    if n == 2 {
        pb = pb.line_to(points[1].0, points[1].1);
        return pb.build();
    }

    // Catmull-Rom to cubic bezier conversion with tension = 1/4.
    // For segment from p[i] to p[i+1]:
    //   cp1 = p[i]   + (p[i+1] - p[i-1]) * tension
    //   cp2 = p[i+1] - (p[i+2] - p[i])   * tension
    // At boundaries we clamp the virtual neighbor to the endpoint itself.
    let t: f32 = 0.25;

    for i in 0..(n - 1) {
        let p_prev = if i == 0 { points[0] } else { points[i - 1] };
        let p_cur = points[i];
        let p_next = points[i + 1];
        let p_next2 = if i + 2 < n {
            points[i + 2]
        } else {
            points[n - 1]
        };

        let cp1x = p_cur.0 + (p_next.0 - p_prev.0) * t;
        let cp1y = p_cur.1 + (p_next.1 - p_prev.1) * t;
        let cp2x = p_next.0 - (p_next2.0 - p_cur.0) * t;
        let cp2y = p_next.1 - (p_next2.1 - p_cur.1) * t;

        pb = pb.curve_to(cp1x, cp1y, cp2x, cp2y, p_next.0, p_next.1);
    }

    pb.build()
}

/// Render a single edge to an SVG element.
fn render_edge(
    edge_path: &LayoutEdgePath,
    ir: &MermaidDiagramIr,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    detail: RenderDetailProfile,
    colors: &ThemeColors,
) -> Element {
    use fm_core::ArrowType;

    let edge_index = edge_path.edge_index;
    let ir_edge = ir.edges.get(edge_index);
    let arrow = ir_edge.map_or(ArrowType::Arrow, |e| e.arrow);
    let is_back_edge = edge_path.reversed;

    // Build path from points using smooth curves
    let pts: Vec<(f32, f32)> = edge_path
        .points
        .iter()
        .map(|p| (p.x + offset_x, p.y + offset_y))
        .collect();

    let path_str = smooth_edge_path(&pts, edge_path.is_self_loop);

    // Back-edges get special treatment: dashed + muted color
    let (base_dasharray, marker_start, marker_end, base_color): (
        Option<&str>,
        Option<&str>,
        Option<&str>,
        &str,
    ) = if is_back_edge {
        (
            Some("4,4"),
            None,
            Some("url(#arrow-open)"),
            &colors.cluster_stroke,
        )
    } else {
        match arrow {
            ArrowType::Line => (None, None, None, &colors.edge),
            ArrowType::Arrow => (None, None, Some("url(#arrow-end)"), &colors.edge),
            ArrowType::ThickArrow => (None, None, Some("url(#arrow-filled)"), &colors.edge),
            ArrowType::DottedArrow => (Some("5,5"), None, Some("url(#arrow-end)"), &colors.edge),
            ArrowType::Circle => (None, None, Some("url(#arrow-circle)"), &colors.edge),
            ArrowType::Cross => (None, None, Some("url(#arrow-cross)"), &colors.edge),
            ArrowType::ThickLine => (None, None, None, &colors.edge),
            ArrowType::DottedLine => (Some("5,5"), None, None, &colors.edge),
            ArrowType::DoubleArrow => (
                None,
                Some("url(#arrow-start)"),
                Some("url(#arrow-end)"),
                &colors.edge,
            ),
            ArrowType::DoubleThickArrow => (
                None,
                Some("url(#arrow-start-filled)"),
                Some("url(#arrow-filled)"),
                &colors.edge,
            ),
            ArrowType::DoubleDottedArrow => (
                Some("5,5"),
                Some("url(#arrow-start)"),
                Some("url(#arrow-end)"),
                &colors.edge,
            ),
        }
    };

    let stroke_width = match arrow {
        ArrowType::ThickArrow | ArrowType::DoubleThickArrow | ArrowType::ThickLine => 2.5,
        _ => 1.8,
    };

    // Determine edge style class
    let style_class = if is_back_edge {
        "fm-edge-back"
    } else {
        match arrow {
            ArrowType::DottedArrow | ArrowType::DottedLine | ArrowType::DoubleDottedArrow => {
                "fm-edge-dashed"
            }
            ArrowType::ThickArrow | ArrowType::DoubleThickArrow | ArrowType::ThickLine => {
                "fm-edge-thick"
            }
            _ => "fm-edge-solid",
        }
    };

    let mut elem = Element::path()
        .d(&path_str)
        .fill("none")
        .stroke(base_color)
        .stroke_width(stroke_width)
        .class("fm-edge")
        .class(style_class)
        .data("fm-edge-id", &edge_index.to_string());

    // Apply inline style from linkStyle directives if present.
    if let Some(inline_style) = resolve_edge_inline_style(ir, edge_index) {
        elem = elem.attr("style", &inline_style);
    }

    if let Some(marker) = marker_start {
        elem = elem.marker_start(marker);
    }
    if let Some(marker) = marker_end {
        elem = elem.marker_end(marker);
    }

    if config.include_source_spans {
        elem = apply_span_metadata(elem, edge_path.span);
    }

    if let Some(dasharray) = base_dasharray {
        elem = elem.stroke_dasharray(dasharray);
    }

    // If edge has a label, wrap in group with text
    if detail.show_edge_labels
        && let Some(label_id) = ir_edge.and_then(|e| e.label)
        && let Some(label) = ir.labels.get(label_id.0)
        && edge_path.points.len() >= 2
    {
        let base_label = truncate_label(&label.text, detail.edge_label_max_chars);

        // Prepend autonumber when enabled for sequence diagrams
        let label_text = if ir.sequence_meta.as_ref().is_some_and(|m| m.autonumber) {
            format!("{} {base_label}", edge_index + 1)
        } else {
            base_label
        };

        // Position label at geometric midpoint of edge
        let (lx, ly) = if edge_path.points.len() == 4 {
            // For standard orthogonal paths, the center of the middle segment
            let p1 = &edge_path.points[1];
            let p2 = &edge_path.points[2];
            (
                (p1.x + p2.x) / 2.0 + offset_x,
                (p1.y + p2.y) / 2.0 + offset_y - 8.0,
            )
        } else if edge_path.points.len() == 2 {
            // For straight lines, geometric center
            let p1 = &edge_path.points[0];
            let p2 = &edge_path.points[1];
            (
                (p1.x + p2.x) / 2.0 + offset_x,
                (p1.y + p2.y) / 2.0 + offset_y - 8.0,
            )
        } else {
            // Fallback for other path lengths
            let mid_idx = edge_path.points.len() / 2;
            let mid_point = &edge_path.points[mid_idx];
            (mid_point.x + offset_x, mid_point.y + offset_y - 8.0)
        };

        let mut group = Element::group()
            .class("fm-edge-labeled")
            .data("fm-edge-id", &edge_index.to_string());
        if config.include_source_spans {
            group = apply_span_metadata(group, edge_path.span);
        }

        // Add accessibility attributes to group
        if config.a11y.aria_labels {
            group = group.attr("role", "graphics-symbol");
        }

        if config.a11y.keyboard_nav {
            group = group.attr("tabindex", "0");
        }

        group = group.child(elem);

        // Add background rect for label
        let lines_count = label_text.lines().count().max(1) as f32;
        let max_line_len = label_text
            .lines()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0);
        let label_text_width = (max_line_len as f32 * config.avg_char_width) + 8.0;
        let label_padding_x = 10.0;
        let label_width = label_text_width + (label_padding_x * 2.0);

        let label_font_size = detail.edge_font_size;
        let total_text_height = (lines_count - 1.0) * label_font_size * config.line_height;
        let label_height = total_text_height + label_font_size + 14.0;

        let start_y = ly - (total_text_height / 2.0) + (label_font_size / 4.0);

        group = group.child(
            Element::rect()
                .x(lx - label_width / 2.0)
                .y(ly - label_height / 2.0 - 1.0)
                .width(label_width)
                .height(label_height)
                .fill(&colors.background)
                .stroke(&colors.cluster_stroke)
                .stroke_width(0.75)
                .rx(6.0)
                .ry(6.0),
        );

        // Add label text
        group = group.child(
            TextBuilder::new(&label_text)
                .x(lx)
                .y(start_y)
                .font_family(&config.font_family)
                .font_size(label_font_size)
                .line_height(config.line_height)
                .anchor(TextAnchor::Middle)
                .fill(&colors.text)
                .class("edge-label")
                .build(),
        );

        // Add title element for text alternatives
        if config.a11y.text_alternatives
            && let Some(edge) = ir_edge
        {
            let from_node = match &edge.from {
                fm_core::IrEndpoint::Node(nid) => ir.nodes.get(nid.0),
                _ => None,
            };
            let to_node = match &edge.to {
                fm_core::IrEndpoint::Node(nid) => ir.nodes.get(nid.0),
                _ => None,
            };
            let edge_desc = describe_edge(from_node, to_node, arrow, Some(&label_text), ir);
            group = group.child(Element::title(&edge_desc));
        }

        return group;
    }

    // Add title element for text alternatives (unlabeled edges)
    if config.a11y.text_alternatives
        && let Some(edge) = ir_edge
    {
        let from_node = match &edge.from {
            fm_core::IrEndpoint::Node(nid) => ir.nodes.get(nid.0),
            _ => None,
        };
        let to_node = match &edge.to {
            fm_core::IrEndpoint::Node(nid) => ir.nodes.get(nid.0),
            _ => None,
        };
        let edge_desc = describe_edge(from_node, to_node, arrow, None, ir);
        // Wrap in group to add title
        let mut group = Element::group()
            .class("fm-edge")
            .data("fm-edge-id", &edge_index.to_string());
        if config.include_source_spans {
            group = apply_span_metadata(group, edge_path.span);
        }
        if config.a11y.aria_labels {
            group = group.attr("role", "graphics-symbol");
        }
        if config.a11y.keyboard_nav {
            group = group.attr("tabindex", "0");
        }
        group = group.child(elem);
        group = group.child(Element::title(&edge_desc));
        return group;
    }

    // Add accessibility attributes for unwrapped edges
    if config.a11y.aria_labels {
        elem = elem.attr("role", "graphics-symbol");
    }
    if config.a11y.keyboard_nav {
        elem = elem.attr("tabindex", "0");
    }

    elem
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{
        ArrowType, DiagramType, IrCluster, IrClusterId, IrEdge, IrEndpoint, IrGraphCluster,
        IrGraphNode, IrLabel, IrLabelId, IrNode, IrNodeId, IrStyleRef, IrStyleTarget, IrSubgraph,
        IrSubgraphId, IrXyAxis, IrXyChartMeta, IrXySeries, IrXySeriesKind, MermaidDiagramIr,
        NodeShape, Span,
    };
    use fm_layout::{
        FillStyle, LayoutAxisTick, LayoutBand, LayoutBandKind, LayoutClusterBox,
        LineCap as RenderLineCap, LineJoin as RenderLineJoin, PathCmd, RenderClip, RenderGroup,
        RenderItem, RenderPath, RenderRect, RenderScene, RenderSource, RenderText,
        RenderTransform, StrokeStyle, TextAlign as RenderTextAlign,
        TextBaseline as RenderTextBaseline, layout_diagram,
    };
    use proptest::prelude::*;

    fn create_ir_with_cluster(title: &str) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let label_id = IrLabelId(0);
        ir.labels.push(IrLabel {
            text: title.to_string(),
            span: Span::default(),
        });
        // Clusters need member nodes to produce layout cluster boxes.
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            title: Some(label_id),
            members: vec![IrNodeId(0), IrNodeId(1)],
            grid_span: 1,
            span: Span::default(),
        });
        ir
    }

    fn create_ir_with_single_node(node_id: &str, shape: NodeShape) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let label_id = IrLabelId(0);
        ir.labels.push(IrLabel {
            text: "Single Node".to_string(),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            id: node_id.to_string(),
            label: Some(label_id),
            shape,
            ..Default::default()
        });
        ir
    }

    fn create_ir_with_single_node_classes(
        node_id: &str,
        shape: NodeShape,
        classes: &[&str],
    ) -> MermaidDiagramIr {
        let mut ir = create_ir_with_single_node(node_id, shape);
        if let Some(node) = ir.nodes.first_mut() {
            node.classes = classes.iter().map(|value| (*value).to_string()).collect();
        }
        ir
    }

    fn create_state_ir_with_concurrent_regions() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::State);
        let label_id = IrLabelId(0);
        ir.labels.push(IrLabel {
            text: "Active Mode".to_string(),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            id: "Processing".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "Monitoring".to_string(),
            ..IrNode::default()
        });
        ir.graph.nodes.push(IrGraphNode {
            node_id: IrNodeId(0),
            kind: fm_core::IrNodeKind::State,
            clusters: vec![IrClusterId(0)],
            subgraphs: vec![IrSubgraphId(0), IrSubgraphId(1)],
        });
        ir.graph.nodes.push(IrGraphNode {
            node_id: IrNodeId(1),
            kind: fm_core::IrNodeKind::State,
            clusters: vec![IrClusterId(0)],
            subgraphs: vec![IrSubgraphId(0), IrSubgraphId(2)],
        });
        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            title: Some(label_id),
            members: vec![IrNodeId(0), IrNodeId(1)],
            grid_span: 2,
            span: Span::default(),
        });
        ir.graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(0),
            title: Some(label_id),
            members: vec![IrNodeId(0), IrNodeId(1)],
            subgraph: Some(IrSubgraphId(0)),
            grid_span: 2,
            span: Span::default(),
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(0),
            key: "Active".to_string(),
            title: Some(label_id),
            children: vec![IrSubgraphId(1), IrSubgraphId(2)],
            members: vec![IrNodeId(0), IrNodeId(1)],
            cluster: Some(IrClusterId(0)),
            grid_span: 2,
            span: Span::default(),
            ..IrSubgraph::default()
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(1),
            key: "__state_region_1".to_string(),
            parent: Some(IrSubgraphId(0)),
            members: vec![IrNodeId(0)],
            span: Span::default(),
            ..IrSubgraph::default()
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(2),
            key: "__state_region_2".to_string(),
            parent: Some(IrSubgraphId(0)),
            members: vec![IrNodeId(1)],
            span: Span::default(),
            ..IrSubgraph::default()
        });
        ir
    }

    fn create_ir_with_labeled_edge() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.labels.push(IrLabel {
            text: "Start".to_string(),
            span: Span::default(),
        });
        ir.labels.push(IrLabel {
            text: "End".to_string(),
            span: Span::default(),
        });
        ir.labels.push(IrLabel {
            text: "edge label that can be truncated".to_string(),
            span: Span::default(),
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
            label: Some(IrLabelId(2)),
            ..Default::default()
        });
        ir
    }

    fn create_xychart_ir() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::XyChart);
        for node_id in [
            "Revenue_1",
            "Revenue_2",
            "Revenue_3",
            "Target_1",
            "Target_2",
            "Target_3",
        ] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..Default::default()
            });
        }
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(3)),
            to: IrEndpoint::Node(IrNodeId(4)),
            arrow: ArrowType::Line,
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(4)),
            to: IrEndpoint::Node(IrNodeId(5)),
            arrow: ArrowType::Line,
            ..Default::default()
        });
        ir.xy_chart_meta = Some(IrXyChartMeta {
            title: Some("Sales Revenue".to_string()),
            x_axis: IrXyAxis {
                categories: vec!["Jan".to_string(), "Feb".to_string(), "Mar".to_string()],
                ..Default::default()
            },
            y_axis: IrXyAxis {
                label: Some("Revenue".to_string()),
                min: Some(0.0),
                max: Some(100.0),
                ..Default::default()
            },
            series: vec![
                IrXySeries {
                    kind: IrXySeriesKind::Bar,
                    name: Some("Revenue".to_string()),
                    values: vec![30.0, 50.0, 70.0],
                    nodes: vec![IrNodeId(0), IrNodeId(1), IrNodeId(2)],
                },
                IrXySeries {
                    kind: IrXySeriesKind::Line,
                    name: Some("Target".to_string()),
                    values: vec![40.0, 60.0, 80.0],
                    nodes: vec![IrNodeId(3), IrNodeId(4), IrNodeId(5)],
                },
            ],
        });
        ir
    }

    fn create_linear_ir(node_count: usize) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for index in 0..node_count {
            ir.labels.push(IrLabel {
                text: format!("N{index}"),
                span: Span::default(),
            });
            ir.nodes.push(IrNode {
                id: format!("N{index}"),
                label: Some(IrLabelId(index)),
                ..Default::default()
            });
        }
        for index in 1..node_count {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(index - 1)),
                to: IrEndpoint::Node(IrNodeId(index)),
                arrow: ArrowType::Arrow,
                ..Default::default()
            });
        }
        ir
    }

    fn create_scene_with_path_and_text() -> RenderScene {
        let mut root =
            RenderGroup::new(Some(String::from("scene-root"))).with_source(RenderSource::Diagram);
        root.children.push(RenderItem::Path(RenderPath {
            source: RenderSource::Node(0),
            commands: vec![
                PathCmd::MoveTo { x: 0.0, y: 0.0 },
                PathCmd::LineTo { x: 10.0, y: 0.0 },
                PathCmd::CubicTo {
                    c1x: 15.0,
                    c1y: 5.0,
                    c2x: 20.0,
                    c2y: 15.0,
                    x: 25.0,
                    y: 20.0,
                },
                PathCmd::QuadTo {
                    cx: 30.0,
                    cy: 25.0,
                    x: 35.0,
                    y: 20.0,
                },
                PathCmd::Close,
            ],
            fill: Some(FillStyle::Solid {
                color: String::from("#ffeeaa"),
                opacity: 0.25,
            }),
            stroke: Some(StrokeStyle {
                color: String::from("#334455"),
                width: 2.5,
                opacity: 0.5,
                dash_array: vec![6.0, 4.0],
                line_cap: RenderLineCap::Round,
                line_join: RenderLineJoin::Bevel,
            }),
            marker_start: MarkerKind::None,
            marker_end: MarkerKind::None,
        }));
        root.children.push(RenderItem::Text(RenderText {
            source: RenderSource::Edge(2),
            text: String::from("scene-label"),
            x: 12.0,
            y: 18.0,
            font_size: 13.0,
            align: RenderTextAlign::Middle,
            baseline: RenderTextBaseline::Middle,
            fill: FillStyle::Solid {
                color: String::from("#102030"),
                opacity: 0.8,
            },
        }));

        RenderScene {
            bounds: RenderRect {
                x: 0.0,
                y: 0.0,
                width: 64.0,
                height: 40.0,
            },
            root,
        }
    }

    fn create_scene_with_transform_and_clip() -> RenderScene {
        let mut child =
            RenderGroup::new(Some(String::from("scene-child"))).with_source(RenderSource::Diagram);
        child.transform = Some(RenderTransform::Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 12.0,
            f: 8.0,
        });
        child.clip = Some(RenderClip::Rect(RenderRect {
            x: 1.0,
            y: 2.0,
            width: 30.0,
            height: 18.0,
        }));
        child.children.push(RenderItem::Path(RenderPath {
            source: RenderSource::Cluster(0),
            commands: vec![
                PathCmd::MoveTo { x: 0.0, y: 0.0 },
                PathCmd::LineTo { x: 40.0, y: 0.0 },
                PathCmd::LineTo { x: 40.0, y: 20.0 },
                PathCmd::Close,
            ],
            fill: Some(FillStyle::Solid {
                color: String::from("#ddeeff"),
                opacity: 1.0,
            }),
            stroke: None,
            marker_start: MarkerKind::None,
            marker_end: MarkerKind::None,
        }));

        let mut root =
            RenderGroup::new(Some(String::from("scene-root"))).with_source(RenderSource::Diagram);
        root.children.push(RenderItem::Group(child));

        RenderScene {
            bounds: RenderRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 80.0,
            },
            root,
        }
    }

    #[test]
    fn emits_svg_document() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let svg = render_svg(&ir);
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn explicit_legacy_backend_matches_default_output() {
        let ir = create_ir_with_labeled_edge();
        let default_svg = render_svg_with_config(&ir, &SvgRenderConfig::default());
        let explicit_legacy = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                backend: SvgBackend::LegacyLayout,
                ..Default::default()
            },
        );
        assert_eq!(default_svg, explicit_legacy);
    }

    #[test]
    fn precomputed_layout_matches_default_render_output() {
        let ir = create_ir_with_labeled_edge();
        let config = SvgRenderConfig::default();
        let layout = layout_diagram(&ir);

        let default_svg = render_svg_with_config(&ir, &config);
        let precomputed_svg = render_svg_with_layout(&ir, &layout, &config);

        assert_eq!(default_svg, precomputed_svg);
    }

    #[test]
    fn scene_backend_is_selectable_from_render_svg_with_config() {
        let ir = create_ir_with_labeled_edge();
        let scene_svg = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                backend: SvgBackend::Scene,
                ..Default::default()
            },
        );
        assert!(scene_svg.starts_with("<svg"));
        assert!(scene_svg.contains("data-type=\"flowchart\""));
        assert!(scene_svg.contains("fm-source-kind=\"node\""));
    }

    #[test]
    fn render_scene_to_svg_emits_paths_text_and_source_metadata() {
        let scene = create_scene_with_path_and_text();
        let svg = render_scene_to_svg(&scene, &SvgRenderConfig::default());
        assert!(svg.contains("data-type=\"scene\""));
        assert!(svg.contains("<path"));
        assert!(svg.contains("<text"));
        assert!(svg.contains("scene-label"));
        assert!(svg.contains("fm-source-kind=\"node\""));
        assert!(svg.contains("fm-source-kind=\"edge\""));
        assert!(svg.contains("C15 5,20 15,25 20"));
        assert!(svg.contains("Q30 25,35 20"));
    }

    #[test]
    fn render_scene_to_svg_supports_transform_and_clip_path() {
        let scene = create_scene_with_transform_and_clip();
        let svg = render_scene_to_svg(&scene, &SvgRenderConfig::default());
        assert!(svg.contains("transform=\"matrix(1,0,0,1,12,8)\""));
        assert!(svg.contains("<clipPath id=\"fm-scene-clip-0\""));
        assert!(svg.contains("clip-path=\"url(#fm-scene-clip-0)\""));
    }

    #[test]
    fn render_scene_to_svg_preserves_fill_and_stroke_styles() {
        let scene = create_scene_with_path_and_text();
        let svg = render_scene_to_svg(&scene, &SvgRenderConfig::default());
        assert!(svg.contains("fill=\"#ffeeaa\""));
        assert!(svg.contains("fill-opacity=\"0.25\""));
        assert!(svg.contains("stroke=\"#334455\""));
        assert!(svg.contains("stroke-width=\"2.50\""));
        assert!(svg.contains("stroke-opacity=\"0.50\""));
        assert!(svg.contains("stroke-dasharray=\"6,4\""));
        assert!(svg.contains("stroke-linecap=\"round\""));
        assert!(svg.contains("stroke-linejoin=\"bevel\""));
    }

    #[test]
    fn includes_data_attributes() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        let svg = render_svg(&ir);
        assert!(svg.contains("data-nodes=\"0\""));
        assert!(svg.contains("data-edges=\"0\""));
        assert!(svg.contains("data-type=\"sequence\""));
    }

    #[test]
    fn includes_accessibility() {
        let ir = MermaidDiagramIr::empty(DiagramType::Class);
        let svg = render_svg(&ir);
        assert!(svg.contains("role=\"img\""));
        assert!(svg.contains("<title>"));
        assert!(svg.contains("<desc>"));
    }

    #[test]
    fn includes_defs_section() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let svg = render_svg(&ir);
        assert!(svg.contains("<defs>"));
        assert!(svg.contains("</defs>"));
        assert!(svg.contains("<marker"));
        assert!(svg.contains("id=\"arrow-end\""));
    }

    #[test]
    fn custom_config_disables_shadows() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let config = SvgRenderConfig {
            shadows: false,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(!svg.contains("drop-shadow"));
    }

    #[test]
    fn renders_cluster_with_css_classes() {
        let ir = create_ir_with_cluster("Test Subgraph");
        let svg = render_svg(&ir);
        assert!(svg.contains("class=\"fm-cluster\""));
        assert!(svg.contains("class=\"fm-cluster-label\""));
    }

    #[test]
    fn renders_sequence_origin_cluster_title_from_layout() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: vec![LayoutClusterBox {
                cluster_index: 0,
                span: Span::default(),
                title: Some("Backend".to_string()),
                bounds: LayoutRect {
                    x: 10.0,
                    y: -20.0,
                    width: 120.0,
                    height: 160.0,
                },
            }],
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: -20.0,
                width: 140.0,
                height: 180.0,
            },
            stats: Default::default(),
            extensions: Default::default(),
        };

        let svg = render_svg_with_layout(&ir, &layout, &SvgRenderConfig::default());
        assert!(svg.contains("Backend"));
        assert!(svg.contains("fm-cluster-label"));
    }

    #[test]
    fn renders_c4_boundary_with_dashed_border() {
        let ir = create_ir_with_cluster("System_Boundary(webapp, Web Application)");
        let svg = render_svg(&ir);
        assert!(svg.contains("fm-cluster-c4"));
        assert!(svg.contains("stroke-dasharray"));
    }

    #[test]
    fn renders_swimlane_cluster_style() {
        let ir = create_ir_with_cluster("section Planning");
        let svg = render_svg(&ir);
        assert!(svg.contains("fm-cluster-swimlane"));
    }

    #[test]
    fn renders_state_cluster_concurrency_divider() {
        let ir = create_state_ir_with_concurrent_regions();
        let layout = layout_diagram(&ir);
        let svg = render_svg_with_layout(&ir, &layout, &SvgRenderConfig::default());
        assert!(svg.contains("Active Mode"));
        assert!(svg.contains("stroke-dasharray=\"6,4\""));
    }

    #[test]
    fn cluster_uses_translucent_fill() {
        let ir = create_ir_with_cluster("Regular Cluster");
        let svg = render_svg(&ir);
        // Standard clusters should have translucent fill
        assert!(svg.contains("rgba("));
    }

    #[test]
    fn node_inline_style_preserves_rgba_values() {
        let mut ir = create_ir_with_single_node("node-alpha", NodeShape::Rect);
        ir.style_refs.push(IrStyleRef {
            target: IrStyleTarget::Node(IrNodeId(0)),
            style: "fill:rgba(226,232,240,0.3),stroke:#334155".to_string(),
            span: Span::default(),
        });

        let inline = resolve_node_inline_style(&ir, 0).expect("node style should resolve");

        assert_eq!(inline, "fill:rgba(226,232,240,0.3); stroke:#334155");
    }

    #[test]
    fn edge_inline_style_preserves_css_function_commas() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.style_refs.push(IrStyleRef {
            target: IrStyleTarget::Link(0),
            style: "stroke:rgba(12,34,56,0.5),filter:drop-shadow(0px,1px,2px,#000)".to_string(),
            span: Span::default(),
        });

        let inline = resolve_edge_inline_style(&ir, 0).expect("edge style should resolve");

        assert_eq!(
            inline,
            "stroke:rgba(12,34,56,0.5); filter:drop-shadow(0px,1px,2px,#000)"
        );
    }

    #[test]
    fn inline_style_preserves_commas_inside_escaped_quoted_strings() {
        let declarations = split_inline_style_declarations(
            r#"label:"value with \"quote, comma\"",stroke:#334155"#,
        );

        assert_eq!(
            declarations,
            vec![r#"label:"value with \"quote, comma\"""#, "stroke:#334155"]
        );
    }

    #[test]
    fn renders_layout_extensions_for_bands_and_axis_ticks() {
        let ir = MermaidDiagramIr::empty(DiagramType::Gantt);
        let mut layout = layout_diagram(&ir);
        layout.extensions.bands.push(LayoutBand {
            kind: LayoutBandKind::Section,
            label: "Planning".to_string(),
            bounds: fm_layout::LayoutRect {
                x: 0.0,
                y: 20.0,
                width: 180.0,
                height: 80.0,
            },
        });
        layout.extensions.axis_ticks.push(LayoutAxisTick {
            label: "2026-02-01".to_string(),
            position: 24.0,
        });

        let svg = render_svg_with_layout(&ir, &layout, &SvgRenderConfig::default());
        assert!(svg.contains("fm-band-section"));
        assert!(svg.contains("fm-band-label"));
        assert!(svg.contains("fm-axis-tick"));
        assert!(svg.contains("2026-02-01"));
    }

    #[test]
    fn renders_xychart_axes_bars_and_line_series() {
        let ir = create_xychart_ir();
        let svg = render_svg_with_config(&ir, &SvgRenderConfig::default());

        assert!(svg.contains("fm-xychart-axis"));
        assert!(svg.contains("fm-xychart-gridline"));
        assert!(svg.contains("fm-xychart-bar"));
        assert!(svg.contains("fm-xychart-line"));
        assert!(svg.contains("fm-xychart-point"));
        assert!(svg.contains("Sales Revenue"));
        assert!(svg.contains(">Jan<"));
        assert!(svg.contains(">Revenue<"));
    }

    #[test]
    fn includes_accessibility_css() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let svg = render_svg(&ir);
        // Default config enables accessibility CSS
        assert!(svg.contains("prefers-contrast"));
        assert!(svg.contains("prefers-reduced-motion"));
    }

    #[test]
    fn accessibility_enhanced_description() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let svg = render_svg(&ir);
        // Enhanced description includes direction
        assert!(svg.contains("flowing"));
    }

    #[test]
    fn disabling_a11y_css() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let config = SvgRenderConfig {
            a11y: A11yConfig::minimal(),
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        // Minimal a11y should not include high contrast CSS
        assert!(!svg.contains("prefers-contrast"));
    }

    #[test]
    fn node_render_includes_deterministic_accent_and_shape_classes() {
        let ir = create_ir_with_single_node("node-alpha", NodeShape::Diamond);
        let svg = render_svg(&ir);
        assert!(svg.contains("fm-node-accent-"));
        assert!(svg.contains("fm-node-shape-diamond"));
    }

    #[test]
    fn stable_accent_index_is_deterministic_and_bounded() {
        let first = stable_accent_index("node-42");
        let second = stable_accent_index("node-42");
        assert_eq!(first, second);
        assert!((1..=8).contains(&first));
    }

    #[test]
    fn compact_tier_hides_edge_labels() {
        let ir = create_ir_with_labeled_edge();
        let config = SvgRenderConfig {
            detail_tier: MermaidTier::Compact,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(!svg.contains("class=\"edge-label\""));
    }

    #[test]
    fn rich_tier_preserves_edge_labels() {
        let ir = create_ir_with_labeled_edge();
        let config = SvgRenderConfig {
            detail_tier: MermaidTier::Rich,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("class=\"edge-label\""));
    }

    #[test]
    fn compact_tier_can_hide_node_text_for_tiny_layouts() {
        // Compact tier hides node labels when the layout area is below
        // the threshold (36K px², width<240, height<150).
        let ir = create_ir_with_single_node("tiny-node", NodeShape::Rect);
        let config = SvgRenderConfig {
            detail_tier: MermaidTier::Compact,
            padding: 0.0,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        // Verify compact tier is selected.
        assert!(svg.contains("data-detail-tier=\"compact\""));
        // In compact mode, edge labels are always hidden.
        assert!(!svg.contains("class=\"edge-label\""));
    }

    #[test]
    fn auto_tier_marks_detail_tier_data_attribute() {
        let ir = create_ir_with_single_node("auto-tier", NodeShape::Rect);
        let config = SvgRenderConfig {
            padding: 0.0,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("data-detail-tier=\"compact\""));
    }

    #[test]
    fn print_optimized_css_is_embedded_by_default() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let svg = render_svg(&ir);
        assert!(svg.contains("@media print"));
    }

    #[test]
    fn configurable_shadow_filter_is_emitted() {
        let ir = create_ir_with_single_node("shadow-node", NodeShape::Rect);
        let config = SvgRenderConfig {
            shadow_offset_x: 4.0,
            shadow_offset_y: 1.5,
            shadow_blur: 5.0,
            shadow_opacity: 0.45,
            shadow_color: "#ff3366".to_string(),
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("id=\"drop-shadow\""));
        assert!(svg.contains("flood-color=\"#ff3366\""));
        assert!(svg.contains("flood-opacity=\"0.45\""));
    }

    #[test]
    fn node_gradient_defs_and_fill_are_emitted() {
        let ir = create_ir_with_single_node("grad-node", NodeShape::Rect);
        let config = SvgRenderConfig {
            node_gradients: true,
            node_gradient_style: NodeGradientStyle::LinearVertical,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("id=\"fm-node-gradient\""));
        assert!(svg.contains("<linearGradient"));
        assert!(svg.contains("fill=\"url(#fm-node-gradient)\""));
    }

    #[test]
    fn highlighted_node_uses_glow_filter() {
        let ir = create_ir_with_single_node_classes("focus-node", NodeShape::Rect, &["highlight"]);
        let config = SvgRenderConfig {
            glow_enabled: true,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("id=\"node-glow\""));
        assert!(svg.contains("class=\"fm-node fm-node-accent-"));
        assert!(svg.contains("fm-node-highlighted"));
        assert!(svg.contains("filter=\"url(#node-glow)\""));
    }

    #[test]
    fn inactive_node_class_is_preserved_for_opacity_layering() {
        let ir =
            create_ir_with_single_node_classes("inactive-node", NodeShape::Rect, &["inactive"]);
        let config = SvgRenderConfig {
            inactive_opacity: 0.35,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("fm-node-inactive"));
        assert!(svg.contains(".fm-node-inactive { opacity: 0.35; }"));
    }

    #[test]
    fn renders_state_pseudo_state_shapes_without_fallback_ids_as_labels() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::State);
        ir.nodes.push(IrNode {
            id: "__state_start".to_string(),
            shape: NodeShape::FilledCircle,
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "fork_state".to_string(),
            shape: NodeShape::HorizontalBar,
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "chooser".to_string(),
            shape: NodeShape::Diamond,
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(1)),
            to: IrEndpoint::Node(IrNodeId(2)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });

        let svg = render_svg(&ir);
        assert!(svg.contains("fm-node-shape-filled-circle"));
        assert!(svg.contains("fm-node-shape-horizontal-bar"));
        assert!(svg.contains("fm-node-shape-diamond"));
        assert!(!svg.contains(">__state_start<"));
    }

    #[test]
    fn svg_emits_source_span_metadata_for_layout_elements() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let node_span = Span::at_line(2, 4);
        let edge_span = Span::at_line(3, 6);
        let cluster_span = Span::at_line(1, 10);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            span_primary: node_span,
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            span_primary: Span::at_line(4, 4),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            span: edge_span,
            ..IrEdge::default()
        });
        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            title: None,
            members: vec![IrNodeId(0), IrNodeId(1)],
            grid_span: 1,
            span: cluster_span,
        });

        let config = SvgRenderConfig {
            include_source_spans: true,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("data-fm-source-span=\"2:1-2:4@0-0\""));
        assert!(svg.contains("data-fm-source-span=\"3:1-3:6@0-0\""));
        assert!(svg.contains("data-fm-source-span=\"1:1-1:10@0-0\""));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn prop_svg_render_is_total_and_counts_match(node_count in 0usize..20) {
            let ir = create_linear_ir(node_count);
            let svg = render_svg(&ir);
            let expected_nodes_attr = format!("data-nodes=\"{}\"", node_count);
            let expected_edges_attr = format!("data-edges=\"{}\"", node_count.saturating_sub(1));

            prop_assert!(svg.starts_with("<svg"));
            prop_assert!(svg.ends_with("</svg>"));
            prop_assert!(svg.contains(&expected_nodes_attr));
            prop_assert!(svg.contains(&expected_edges_attr));
        }

        #[test]
        fn prop_svg_output_is_deterministic(node_count in 1usize..15) {
            let ir = create_linear_ir(node_count);
            let svg1 = render_svg(&ir);
            let svg2 = render_svg(&ir);
            prop_assert_eq!(svg1, svg2, "SVG output should be byte-identical for same input");
        }

        #[test]
        fn prop_svg_all_themes_render_without_panic(theme_token in 0usize..4) {
            let theme = match theme_token {
                0 => ThemePreset::Default,
                1 => ThemePreset::Dark,
                2 => ThemePreset::Forest,
                _ => ThemePreset::Neutral,
            };
            let ir = create_linear_ir(5);
            let config = SvgRenderConfig {
                theme,
                ..Default::default()
            };
            let svg = render_svg_with_config(&ir, &config);
            prop_assert!(svg.starts_with("<svg"));
            prop_assert!(svg.ends_with("</svg>"));
        }

        #[test]
        fn prop_svg_contains_viewbox(node_count in 1usize..10) {
            let ir = create_linear_ir(node_count);
            let svg = render_svg(&ir);
            prop_assert!(svg.contains("viewBox="), "SVG should contain viewBox attribute");
        }

        #[test]
        fn prop_svg_render_never_contains_nan(node_count in 0usize..15) {
            let ir = create_linear_ir(node_count);
            let svg = render_svg(&ir);
            prop_assert!(
                !svg.contains("NaN"),
                "SVG output should never contain NaN values"
            );
            prop_assert!(
                !svg.contains("Infinity"),
                "SVG output should never contain Infinity values"
            );
        }
    }
}
