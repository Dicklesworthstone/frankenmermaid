#![forbid(unsafe_code)]

//! Zero-dependency SVG builder for frankenmermaid diagram rendering.
//!
//! Provides a lightweight, type-safe API for generating clean SVG output
//! suitable for flowcharts, sequence diagrams, and other diagram types.

mod attributes;
mod defs;
mod document;
mod element;
mod path;
mod text;
mod transform;

pub use attributes::{Attribute, AttributeValue, Attributes};
pub use defs::{ArrowheadMarker, DefsBuilder, Filter, Gradient, GradientStop, MarkerOrient};
pub use document::SvgDocument;
pub use element::{Element, ElementKind};
pub use path::{PathBuilder, PathCommand};
pub use text::{TextAnchor, TextBuilder, TextMetrics};
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
        doc = doc.accessible(
            format!("{} diagram", ir.diagram_type.as_str()),
            format!(
                "Diagram with {} nodes and {} edges",
                ir.nodes.len(),
                ir.edges.len()
            ),
        );
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

    // Add drop shadow filter if enabled
    if config.shadows {
        defs = defs.filter(Filter::drop_shadow("drop-shadow", 2.0, 2.0, 3.0, 0.2));
    }

    doc = doc.defs(defs);

    // Offset for padding
    let offset_x = padding - layout.bounds.x;
    let offset_y = padding - layout.bounds.y;

    // Render clusters (subgraphs) as background rectangles
    for cluster in &layout.clusters {
        let rect = Element::rect()
            .x(cluster.bounds.x + offset_x)
            .y(cluster.bounds.y + offset_y)
            .width(cluster.bounds.width)
            .height(cluster.bounds.height)
            .fill("#f8f9fa")
            .stroke("#dee2e6")
            .stroke_width(1.0)
            .rx(config.rounded_corners)
            .class("cluster");
        doc = doc.child(rect);

        // Cluster label if present - get title from IR cluster
        if let Some(ir_cluster) = ir.clusters.get(cluster.cluster_index) {
            if let Some(title_id) = ir_cluster.title {
                if let Some(label) = ir.labels.get(title_id.0) {
                    let text = TextBuilder::new(&label.text)
                        .x(cluster.bounds.x + offset_x + 8.0)
                        .y(cluster.bounds.y + offset_y + 16.0)
                        .font_family(&config.font_family)
                        .font_size(config.font_size * 0.9)
                        .fill("#6c757d")
                        .class("cluster-label")
                        .build();
                    doc = doc.child(text);
                }
            }
        }
    }

    // Render edges
    for (idx, edge_path) in layout.edges.iter().enumerate() {
        let edge_elem = render_edge(edge_path, ir, idx, offset_x, offset_y, config);
        doc = doc.child(edge_elem);
    }

    // Render nodes
    for (idx, node_box) in layout.nodes.iter().enumerate() {
        let node_elem = render_node(node_box, ir, idx, offset_x, offset_y, config);
        doc = doc.child(node_elem);
    }

    doc.to_string()
}

/// Render a single node to an SVG element.
fn render_node(
    node_box: &LayoutNodeBox,
    ir: &MermaidDiagramIr,
    idx: usize,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
) -> Element {
    use fm_core::NodeShape;

    let ir_node = ir.nodes.get(idx);
    let shape = ir_node.map_or(NodeShape::Rect, |n| n.shape);

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
        .class("node")
        .data("id", ir_node.map_or("", |n| &n.id));

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
    };

    // Apply shadow filter if enabled and this isn't a special composite shape
    let shape_elem = if config.shadows && !matches!(shape, NodeShape::Subroutine) {
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

    group
}

/// Render a single edge to an SVG element.
fn render_edge(
    edge_path: &LayoutEdgePath,
    ir: &MermaidDiagramIr,
    idx: usize,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
) -> Element {
    use fm_core::ArrowType;

    let ir_edge = ir.edges.get(idx);
    let arrow = ir_edge.map_or(ArrowType::Arrow, |e| e.arrow);

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

    // Determine stroke style and markers based on arrow type
    let (stroke_dasharray, marker_end, stroke_color) = match arrow {
        ArrowType::Line => (None, None, "#333"),
        ArrowType::Arrow => (None, Some("url(#arrow-end)"), "#333"),
        ArrowType::ThickArrow => (None, Some("url(#arrow-filled)"), "#333"),
        ArrowType::DottedArrow => (Some("5,5"), Some("url(#arrow-end)"), "#666"),
        ArrowType::Circle => (None, Some("url(#arrow-circle)"), "#333"),
        ArrowType::Cross => (None, Some("url(#arrow-cross)"), "#333"),
    };

    let stroke_width = match arrow {
        ArrowType::ThickArrow => 2.5,
        _ => 1.5,
    };

    let mut elem = Element::path()
        .d(&path_str)
        .fill("none")
        .stroke(stroke_color)
        .stroke_width(stroke_width)
        .class("edge");

    if let Some(dasharray) = stroke_dasharray {
        elem = elem.stroke_dasharray(dasharray);
    }

    if let Some(marker) = marker_end {
        elem = elem.marker_end(marker);
    }

    // If edge has a label, wrap in group with text
    if let Some(label_id) = ir_edge.and_then(|e| e.label) {
        if let Some(label) = ir.labels.get(label_id.0) {
            // Position label at midpoint of edge
            if edge_path.points.len() >= 2 {
                let mid_idx = edge_path.points.len() / 2;
                let mid_point = &edge_path.points[mid_idx];
                let lx = mid_point.x + offset_x;
                let ly = mid_point.y + offset_y - 8.0; // Offset above the line

                let mut group = Element::group().class("edge-labeled");
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

                return group;
            }
        }
    }

    elem
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{DiagramType, MermaidDiagramIr};

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
}
