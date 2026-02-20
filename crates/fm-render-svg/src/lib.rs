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

use fm_core::MermaidDiagramIr;
use fm_layout::{DiagramLayout, LayoutEdgePath, LayoutNodeBox, layout_diagram};

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
    /// Whether to use rounded corners on rectangles.
    pub rounded_corners: f32,
    /// CSS classes to apply to the root SVG element.
    pub root_classes: Vec<String>,
    /// Theme preset to use (default if not specified).
    pub theme: ThemePreset,
    /// Whether to embed theme CSS in the SVG.
    pub embed_theme_css: bool,
    /// Accessibility configuration.
    pub a11y: A11yConfig,
}

impl Default for SvgRenderConfig {
    fn default() -> Self {
        Self {
            responsive: true,
            accessible: true,
            font_family: String::from("system-ui, -apple-system, sans-serif"),
            font_size: 14.0,
            avg_char_width: 8.0,
            line_height: 1.4,
            padding: 20.0,
            shadows: true,
            rounded_corners: 4.0,
            root_classes: Vec::new(),
            theme: ThemePreset::Default,
            embed_theme_css: true,
            a11y: A11yConfig::full(),
        }
    }
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

/// Render a computed layout to SVG.
fn render_layout_to_svg(
    layout: &DiagramLayout,
    ir: &MermaidDiagramIr,
    config: &SvgRenderConfig,
) -> String {
    let padding = config.padding;
    let width = layout.bounds.width + padding * 2.0;
    let height = layout.bounds.height + padding * 2.0;

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
        .data("type", ir.diagram_type.as_str());

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
    if config.shadows {
        defs = defs.filter(Filter::drop_shadow("drop-shadow", 2.0, 2.0, 3.0, 0.2));
    }

    doc = doc.defs(defs);

    // Embed theme CSS if enabled
    if config.embed_theme_css {
        let theme = Theme::from_preset(config.theme);
        let mut css = theme.to_svg_style();

        // Add accessibility CSS if enabled
        if config.a11y.accessibility_css {
            css.push_str(accessibility_css());
        }

        doc = doc.style(css);
    } else if config.a11y.accessibility_css {
        // Only add accessibility CSS
        doc = doc.style(accessibility_css());
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
        if !title_text.is_empty() {
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
                    .font_size(config.font_size * 0.9)
                    .fill(label_color)
                    .class("fm-cluster-label")
                    .build();
                doc = doc.child(text);
            }
        }
    }

    // Render edges
    for edge_path in &layout.edges {
        let edge_elem = render_edge(edge_path, ir, offset_x, offset_y, config);
        doc = doc.child(edge_elem);
    }

    // Render nodes
    for node_box in &layout.nodes {
        let node_elem = render_node(node_box, ir, offset_x, offset_y, config);
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
    let label_text = ir_node
        .and_then(|n| n.label)
        .and_then(|lid| ir.labels.get(lid.0))
        .map(|l| l.text.as_str())
        .or_else(|| ir_node.map(|n| n.id.as_str()))
        .unwrap_or("");

    // Create group for node shape + label
    let mut group = Element::group()
        .class("fm-node")
        .data("id", node_id)
        .data("fm-node-id", node_id);

    // Add accessibility attributes
    if config.a11y.aria_labels {
        group = group
            .attr("role", "graphics-symbol")
            .attr("aria-label", label_text);
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
            .rx(0.0),

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
                .arc_to(w / 2.0, ry, 0.0, false, true, x, y + h - ry)
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
                    .fill("#fff")
                    .stroke("#333")
                    .stroke_width(1.5),
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
            return group.child(g).child(
                TextBuilder::new(label_text)
                    .x(cx)
                    .y(cy + config.font_size / 3.0)
                    .font_family(&config.font_family)
                    .font_size(config.font_size)
                    .anchor(TextAnchor::Middle)
                    .fill("#333")
                    .build(),
            );
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
                    .fill("#fff")
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
            return group.child(g).child(
                TextBuilder::new(label_text)
                    .x(cx)
                    .y(cy + config.font_size / 3.0)
                    .font_family(&config.font_family)
                    .font_size(config.font_size)
                    .anchor(TextAnchor::Middle)
                    .fill("#333")
                    .build(),
            );
        }
    };

    // Apply shadow filter if enabled and this isn't a special composite shape
    let shape_elem =
        if config.shadows && !matches!(shape, NodeShape::Subroutine | NodeShape::CrossedCircle) {
            shape_elem.filter("url(#drop-shadow)")
        } else {
            shape_elem
        };

    group = group.child(shape_elem);

    // Add label text
    let text_elem = TextBuilder::new(label_text)
        .x(cx)
        .y(cy + config.font_size / 3.0)
        .font_family(&config.font_family)
        .font_size(config.font_size)
        .anchor(TextAnchor::Middle)
        .fill("#333")
        .build();
    group = group.child(text_elem);

    // Add title element for text alternatives
    if config.a11y.text_alternatives
        && let Some(node) = ir_node
    {
        let node_desc = describe_node(node, ir);
        group = group.child(Element::title(&node_desc));
    }

    group
}

/// Render a single edge to an SVG element.
fn render_edge(
    edge_path: &LayoutEdgePath,
    ir: &MermaidDiagramIr,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
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
    if let Some(label_id) = ir_edge.and_then(|e| e.label)
        && let Some(label) = ir.labels.get(label_id.0)
        && edge_path.points.len() >= 2
    {
        // Position label at midpoint of edge
        let mid_idx = edge_path.points.len() / 2;
        let mid_point = &edge_path.points[mid_idx];
        let lx = mid_point.x + offset_x;
        let ly = mid_point.y + offset_y - 8.0; // Offset above the line

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
        let label_width = label.text.len() as f32 * config.avg_char_width + 8.0;
        let label_height = config.font_size + 4.0;
        group = group.child(
            Element::rect()
                .x(lx - label_width / 2.0)
                .y(ly - label_height / 2.0 - 2.0)
                .width(label_width)
                .height(label_height)
                .fill("#fff")
                .stroke("none")
                .rx(2.0),
        );

        // Add label text
        group = group.child(
            TextBuilder::new(&label.text)
                .x(lx)
                .y(ly + config.font_size / 4.0)
                .font_family(&config.font_family)
                .font_size(config.font_size * 0.85)
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
            let edge_desc = describe_edge(from_node, to_node, arrow, Some(&label.text), ir);
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
        DiagramType, IrCluster, IrClusterId, IrLabel, IrLabelId, MermaidDiagramIr, Span,
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
}
