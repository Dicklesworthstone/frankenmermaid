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
pub use text::{TextAnchor, TextBuilder, TextMetrics};
pub use theme::{FontConfig, Theme, ThemeColors, ThemePreset, generate_palette};
pub use transform::{Transform, TransformBuilder};

use fm_core::{MermaidDiagramIr, MermaidTier};
use fm_layout::{DiagramLayout, LayoutEdgePath, LayoutNodeBox, layout_diagram};

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

/// Configuration for SVG rendering.
#[derive(Debug, Clone)]
pub struct SvgRenderConfig {
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
}

impl Default for SvgRenderConfig {
    fn default() -> Self {
        Self {
            responsive: true,
            accessible: true,
            font_family: String::from(
                "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif",
            ),
            font_size: 15.0,
            avg_char_width: 8.0,
            line_height: 1.5,
            padding: 48.0,
            shadows: true,
            shadow_offset_x: 2.0,
            shadow_offset_y: 2.0,
            shadow_blur: 4.0,
            shadow_opacity: 0.20,
            shadow_color: String::from("#000000"),
            node_gradients: true,
            node_gradient_style: NodeGradientStyle::LinearVertical,
            glow_enabled: true,
            glow_blur: 6.0,
            glow_opacity: 0.35,
            glow_color: String::from("#3b82f6"),
            cluster_fill_opacity: 0.08,
            inactive_opacity: 0.40,
            rounded_corners: 12.0,
            root_classes: Vec::new(),
            theme: ThemePreset::Default,
            embed_theme_css: true,
            detail_tier: MermaidTier::Auto,
            min_font_size: 8.0,
            print_optimized: true,
            a11y: A11yConfig::full(),
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
    let layout = layout_diagram(ir);
    render_layout_to_svg(&layout, ir, config)
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
        GradientStop::with_opacity(0.0, &theme.colors.node_fill, 0.95),
        GradientStop::with_opacity(1.0, &theme.colors.background, 0.88),
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

    let preset = ir
        .meta
        .theme_overrides
        .theme
        .as_deref()
        .and_then(|t| t.parse::<ThemePreset>().ok())
        .unwrap_or(config.theme);
    let mut theme = Theme::from_preset(preset);
    theme
        .colors
        .apply_overrides(&ir.meta.theme_overrides.theme_variables);
    let effects_enabled = config.node_gradients
        || config.glow_enabled
        || clamp_unit_interval(config.inactive_opacity) < 0.999
        || clamp_unit_interval(config.cluster_fill_opacity) < 0.999;

    // Build defs section
    let mut defs = DefsBuilder::new();

    // Add standard arrowhead markers
    defs = defs.marker(ArrowheadMarker::standard("arrow-end", "#333"));
    defs = defs.marker(ArrowheadMarker::filled("arrow-filled", "#333"));
    defs = defs.marker(ArrowheadMarker::open("arrow-open", "#333"));
    defs = defs.marker(ArrowheadMarker::circle_marker("arrow-circle", "#333"));
    defs = defs.marker(ArrowheadMarker::cross_marker("arrow-cross", "#333"));
    defs = defs.marker(ArrowheadMarker::diamond_marker("arrow-diamond", "#333"));

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

        // Detect cluster type from title for specialized styling
        let title_text = ir_cluster
            .and_then(|c| c.title)
            .and_then(|tid| ir.labels.get(tid.0))
            .map(|l| l.text.as_str())
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
                doc = doc.child(text);
            }
        }
    }

    // Render edges
    for edge_path in &layout.edges {
        let edge_elem = render_edge(edge_path, ir, offset_x, offset_y, config, detail);
        doc = doc.child(edge_elem);
    }

    // Render nodes
    for node_box in &layout.nodes {
        let node_elem = render_node(node_box, ir, offset_x, offset_y, config, detail);
        doc = doc.child(node_elem);
    }

    doc.to_string()
}

/// Render a single node to an SVG element.
fn render_node(
    node_box: &LayoutNodeBox,
    ir: &MermaidDiagramIr,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    detail: RenderDetailProfile,
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
        .or_else(|| ir_node.map(|n| n.id.as_str()))
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
            .fill("#fff")
            .stroke("#333")
            .stroke_width(1.5)
            .rx(config.rounded_corners * 0.55),

        NodeShape::Rounded => Element::rect()
            .x(x)
            .y(y)
            .width(w)
            .height(h)
            .fill("#fff")
            .stroke("#333")
            .stroke_width(1.5)
            .rx(config.rounded_corners),

        NodeShape::Stadium => Element::rect()
            .x(x)
            .y(y)
            .width(w)
            .height(h)
            .fill("#fff")
            .stroke("#333")
            .stroke_width(1.5)
            .rx(h / 2.0),

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
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5)
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
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5)
        }

        NodeShape::Circle | NodeShape::DoubleCircle => {
            let r = w.min(h) / 2.0;
            let mut elem = Element::circle()
                .cx(cx)
                .cy(cy)
                .r(r)
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5);

            if shape == NodeShape::DoubleCircle {
                // For double circle, we'll use a slightly smaller stroke
                elem = elem.stroke_width(2.0);
            }
            elem
        }

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
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5)
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
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5)
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
                        "#fff"
                    })
                    .stroke("#333")
                    .stroke_width(1.5)
                    .rx(config.rounded_corners * 0.45),
            );
            // Left vertical line
            g = g.child(
                Element::line()
                    .x1(x + inset)
                    .y1(y)
                    .x2(x + inset)
                    .y2(y + h)
                    .stroke("#333")
                    .stroke_width(1.0),
            );
            // Right vertical line
            g = g.child(
                Element::line()
                    .x1(x + w - inset)
                    .y1(y)
                    .x2(x + w - inset)
                    .y2(y + h)
                    .stroke("#333")
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
                        .fill("#333")
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
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5)
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
                .fill("#fffacd")
                .stroke("#333")
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
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5)
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
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5)
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
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5)
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
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5)
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
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5)
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
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5)
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
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5)
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
                .fill("#fff")
                .stroke("#333")
                .stroke_width(1.5)
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
                        "#fff"
                    })
                    .stroke("#333")
                    .stroke_width(1.5),
            );
            // Diagonal lines
            let offset = r * 0.707; // r * cos(45°)
            g = g.child(
                Element::line()
                    .x1(cx - offset)
                    .y1(cy - offset)
                    .x2(cx + offset)
                    .y2(cy + offset)
                    .stroke("#333")
                    .stroke_width(1.5),
            );
            g = g.child(
                Element::line()
                    .x1(cx + offset)
                    .y1(cy - offset)
                    .x2(cx - offset)
                    .y2(cy + offset)
                    .stroke("#333")
                    .stroke_width(1.5),
            );
            if detail.show_node_labels {
                return group.child(g).child(
                    TextBuilder::new(&label_text)
                        .x(cx)
                        .y(cy + node_font_size / 3.0)
                        .font_family(&config.font_family)
                        .font_size(node_font_size)
                        .anchor(TextAnchor::Middle)
                        .fill("#333")
                        .build(),
                );
            }
            return group.child(g);
        }
    };

    let shape_elem = if config.node_gradients && !matches!(shape, NodeShape::Note) {
        shape_elem.fill("url(#fm-node-gradient)")
    } else {
        shape_elem
    };

    // Apply shadow filter if enabled and this isn't a special composite shape.
    // Highlighted nodes prefer glow so the effects don't visually muddy each other.
    let shape_elem = if detail.enable_shadows
        && !(is_highlighted && config.glow_enabled)
        && !matches!(shape, NodeShape::Subroutine | NodeShape::CrossedCircle)
    {
        shape_elem.filter("url(#drop-shadow)")
    } else {
        shape_elem
    };

    group = group.child(shape_elem);
    if is_highlighted && config.glow_enabled {
        group = group.filter("url(#node-glow)");
    }

    // Add label text
    if detail.show_node_labels {
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
            .fill("#333")
            .build();
        group = group.child(text_elem);
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
        NodeShape::Asymmetric => "fm-node-shape-asymmetric",
        NodeShape::Cylinder => "fm-node-shape-cylinder",
        NodeShape::Trapezoid => "fm-node-shape-trapezoid",
        NodeShape::DoubleCircle => "fm-node-shape-double-circle",
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

/// Render a single edge to an SVG element.
fn render_edge(
    edge_path: &LayoutEdgePath,
    ir: &MermaidDiagramIr,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    detail: RenderDetailProfile,
) -> Element {
    use fm_core::ArrowType;

    let edge_index = edge_path.edge_index;
    let ir_edge = ir.edges.get(edge_index);
    let arrow = ir_edge.map_or(ArrowType::Arrow, |e| e.arrow);
    let is_back_edge = edge_path.reversed;

    // Build path from points
    let mut path_builder = PathBuilder::new();
    let mut first = true;
    for point in &edge_path.points {
        let px = point.x + offset_x;
        let py = point.y + offset_y;
        if first {
            path_builder = path_builder.move_to(px, py);
            first = false;
        } else {
            path_builder = path_builder.line_to(px, py);
        }
    }

    let path_str = path_builder.build();

    // Back-edges get special treatment: dashed + muted color
    let (base_dasharray, base_marker, base_color) = if is_back_edge {
        (Some("4,4"), Some("url(#arrow-open)"), "#999")
    } else {
        match arrow {
            ArrowType::Line => (None, None, "#333"),
            ArrowType::Arrow => (None, Some("url(#arrow-end)"), "#333"),
            ArrowType::ThickArrow => (None, Some("url(#arrow-filled)"), "#333"),
            ArrowType::DottedArrow => (Some("5,5"), Some("url(#arrow-end)"), "#666"),
            ArrowType::Circle => (None, Some("url(#arrow-circle)"), "#333"),
            ArrowType::Cross => (None, Some("url(#arrow-cross)"), "#333"),
        }
    };

    let stroke_width = match arrow {
        ArrowType::ThickArrow => 2.5,
        _ => 1.5,
    };

    // Determine edge style class
    let style_class = if is_back_edge {
        "fm-edge-back"
    } else {
        match arrow {
            ArrowType::DottedArrow => "fm-edge-dashed",
            ArrowType::ThickArrow => "fm-edge-thick",
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

    if let Some(dasharray) = base_dasharray {
        elem = elem.stroke_dasharray(dasharray);
    }

    if let Some(marker) = base_marker {
        elem = elem.marker_end(marker);
    }

    // If edge has a label, wrap in group with text
    if detail.show_edge_labels
        && let Some(label_id) = ir_edge.and_then(|e| e.label)
        && let Some(label) = ir.labels.get(label_id.0)
        && edge_path.points.len() >= 2
    {
        let label_text = truncate_label(&label.text, detail.edge_label_max_chars);

        // Position label at geometric midpoint of edge
        let (lx, ly) = if edge_path.points.len() == 4 {
            // For standard orthogonal paths, the center of the middle segment
            let p1 = &edge_path.points[1];
            let p2 = &edge_path.points[2];
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
                .fill("#fff")
                .stroke("#cbd5e1")
                .stroke_width(1.0)
                .rx(6.0),
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
                .fill("#666")
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
        ArrowType, DiagramType, IrCluster, IrClusterId, IrEdge, IrEndpoint, IrLabel, IrLabelId,
        IrNode, IrNodeId, MermaidDiagramIr, NodeShape, Span,
    };

    fn create_ir_with_cluster(title: &str) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let label_id = IrLabelId(0);
        ir.labels.push(IrLabel {
            text: title.to_string(),
            span: Span::default(),
        });
        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            title: Some(label_id),
            members: vec![],
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

    #[test]
    fn emits_svg_document() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let svg = render_svg(&ir);
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
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
    fn cluster_uses_translucent_fill() {
        let ir = create_ir_with_cluster("Regular Cluster");
        let svg = render_svg(&ir);
        // Standard clusters should have translucent fill
        assert!(svg.contains("rgba("));
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
        let ir = create_ir_with_single_node("tiny-node", NodeShape::Rect);
        let config = SvgRenderConfig {
            detail_tier: MermaidTier::Compact,
            padding: 0.0,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(!svg.contains("<text"));
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
}
