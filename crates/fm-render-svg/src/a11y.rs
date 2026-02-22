//! Accessibility features for SVG diagrams.
//!
//! Provides ARIA attributes, text alternatives, and accessibility CSS utilities.

use fm_core::{IrNode, MermaidDiagramIr};

/// Generate an accessible description for a diagram.
#[must_use]
pub fn describe_diagram(ir: &MermaidDiagramIr) -> String {
    let mut parts = Vec::new();

    let type_desc = match ir.diagram_type.as_str() {
        "flowchart" => "flowchart diagram",
        "sequence" => "sequence diagram",
        "class" => "class diagram",
        "state" => "state diagram",
        "gantt" => "Gantt chart",
        "pie" => "pie chart",
        "er" | "erDiagram" => "entity-relationship diagram",
        "journey" => "user journey diagram",
        "mindmap" => "mindmap",
        "timeline" => "timeline",
        "quadrant" => "quadrant chart",
        _ => "diagram",
    };

    parts.push(format!(
        "A {} with {} nodes and {} edges",
        type_desc,
        ir.nodes.len(),
        ir.edges.len()
    ));

    if !ir.clusters.is_empty() {
        parts.push(format!(", organized in {} groups", ir.clusters.len()));
    }

    // Add direction hint
    let direction_desc = match ir.direction {
        fm_core::GraphDirection::LR => "flowing left to right",
        fm_core::GraphDirection::RL => "flowing right to left",
        fm_core::GraphDirection::TB | fm_core::GraphDirection::TD => "flowing top to bottom",
        fm_core::GraphDirection::BT => "flowing bottom to top",
    };
    parts.push(format!(", {}", direction_desc));

    parts.join("")
}

/// Generate a text alternative for a node.
#[must_use]
pub fn describe_node(node: &IrNode, ir: &MermaidDiagramIr) -> String {
    let label = node
        .label
        .and_then(|lid| ir.labels.get(lid.0))
        .map(|l| l.text.as_str())
        .unwrap_or(&node.id);

    let shape_desc = match node.shape {
        fm_core::NodeShape::Rect => "rectangle",
        fm_core::NodeShape::Rounded => "rounded rectangle",
        fm_core::NodeShape::Stadium => "stadium shape",
        fm_core::NodeShape::Diamond => "diamond",
        fm_core::NodeShape::Hexagon => "hexagon",
        fm_core::NodeShape::Circle => "circle",
        fm_core::NodeShape::DoubleCircle => "double circle",
        fm_core::NodeShape::Cylinder => "cylinder",
        fm_core::NodeShape::Trapezoid => "trapezoid",
        fm_core::NodeShape::Subroutine => "subroutine box",
        fm_core::NodeShape::Asymmetric => "flag shape",
        fm_core::NodeShape::Note => "note",
        fm_core::NodeShape::InvTrapezoid => "inverted trapezoid",
        fm_core::NodeShape::Triangle => "triangle",
        fm_core::NodeShape::Pentagon => "pentagon",
        fm_core::NodeShape::Star => "star",
        fm_core::NodeShape::Cloud => "cloud",
        fm_core::NodeShape::Tag => "tag",
        fm_core::NodeShape::CrossedCircle => "crossed circle",
        fm_core::NodeShape::Parallelogram => "parallelogram",
        fm_core::NodeShape::InvParallelogram => "inverted parallelogram",
    };

    format!("Node: {}, {}", label, shape_desc)
}

/// Generate a text alternative for an edge.
#[must_use]
pub fn describe_edge(
    from_node: Option<&IrNode>,
    to_node: Option<&IrNode>,
    arrow_type: fm_core::ArrowType,
    label: Option<&str>,
    ir: &MermaidDiagramIr,
) -> String {
    let from_label = from_node
        .and_then(|n| {
            n.label
                .and_then(|lid| ir.labels.get(lid.0))
                .map(|l| l.text.as_str())
        })
        .or_else(|| from_node.map(|n| n.id.as_str()))
        .unwrap_or("unknown");

    let to_label = to_node
        .and_then(|n| {
            n.label
                .and_then(|lid| ir.labels.get(lid.0))
                .map(|l| l.text.as_str())
        })
        .or_else(|| to_node.map(|n| n.id.as_str()))
        .unwrap_or("unknown");

    let arrow_desc = match arrow_type {
        fm_core::ArrowType::Line => "connects to",
        fm_core::ArrowType::Arrow => "points to",
        fm_core::ArrowType::ThickArrow => "strongly points to",
        fm_core::ArrowType::DottedArrow => "optionally points to",
        fm_core::ArrowType::Circle => "relates to",
        fm_core::ArrowType::Cross => "blocks",
    };

    if let Some(label_text) = label {
        format!(
            "{} {} {} with label: {}",
            from_label, arrow_desc, to_label, label_text
        )
    } else {
        format!("{} {} {}", from_label, arrow_desc, to_label)
    }
}

/// Generate accessibility CSS with media query support.
#[must_use]
pub fn accessibility_css() -> &'static str {
    r"
/* High contrast mode support */
@media (prefers-contrast: more) {
  :root {
    --fm-bg: #ffffff !important;
    --fm-text-color: #000000 !important;
    --fm-node-fill: #ffffff !important;
    --fm-node-stroke: #000000 !important;
    --fm-edge-color: #000000 !important;
  }
  .fm-node { stroke-width: 2px !important; }
  .fm-edge { stroke-width: 2px !important; }
}

/* Reduced motion support */
@media (prefers-reduced-motion: reduce) {
  .fm-edge, .fm-node {
    animation: none !important;
    transition: none !important;
  }
}

/* Focus indicators for keyboard navigation */
.fm-node:focus, .fm-edge:focus {
  outline: 3px solid #0066cc;
  outline-offset: 2px;
}

.fm-node:focus-visible, .fm-edge:focus-visible {
  outline: 3px solid #0066cc;
  outline-offset: 2px;
}

/* Screen reader only content */
.fm-sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}
"
}

/// Configuration for accessibility features.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct A11yConfig {
    /// Whether to add ARIA attributes to elements.
    pub aria_labels: bool,
    /// Whether to add title elements for text alternatives.
    pub text_alternatives: bool,
    /// Whether to make elements keyboard-focusable.
    pub keyboard_nav: bool,
    /// Whether to include accessibility CSS (high contrast, reduced motion).
    pub accessibility_css: bool,
}

impl A11yConfig {
    /// Full accessibility features enabled.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            aria_labels: true,
            text_alternatives: true,
            keyboard_nav: true,
            accessibility_css: true,
        }
    }

    /// Minimal accessibility (just ARIA labels).
    #[must_use]
    pub const fn minimal() -> Self {
        Self {
            aria_labels: true,
            text_alternatives: false,
            keyboard_nav: false,
            accessibility_css: false,
        }
    }

    /// No accessibility features.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            aria_labels: false,
            text_alternatives: false,
            keyboard_nav: false,
            accessibility_css: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{DiagramType, GraphDirection, MermaidDiagramIr, NodeShape};

    fn create_test_ir() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;

        // Add test nodes
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            label: None,
            shape: NodeShape::Rect,
            classes: vec![],
            span_primary: fm_core::Span::default(),
            span_all: vec![],
            implicit: false,
            members: vec![],
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            label: None,
            shape: NodeShape::Diamond,
            classes: vec![],
            span_primary: fm_core::Span::default(),
            span_all: vec![],
            implicit: false,
            members: vec![],
        });

        ir
    }

    #[test]
    fn describe_diagram_includes_counts() {
        let ir = create_test_ir();
        let desc = describe_diagram(&ir);
        assert!(desc.contains("2 nodes"));
        assert!(desc.contains("0 edges"));
        assert!(desc.contains("flowchart"));
        assert!(desc.contains("left to right"));
    }

    #[test]
    fn describe_node_includes_shape() {
        let ir = create_test_ir();
        let node = &ir.nodes[0];
        let desc = describe_node(node, &ir);
        assert!(desc.contains("A"));
        assert!(desc.contains("rectangle"));
    }

    #[test]
    fn describe_node_diamond_shape() {
        let ir = create_test_ir();
        let node = &ir.nodes[1];
        let desc = describe_node(node, &ir);
        assert!(desc.contains("B"));
        assert!(desc.contains("diamond"));
    }

    #[test]
    fn describe_edge_with_label() {
        let ir = create_test_ir();
        let desc = describe_edge(
            Some(&ir.nodes[0]),
            Some(&ir.nodes[1]),
            fm_core::ArrowType::Arrow,
            Some("Submit"),
            &ir,
        );
        assert!(desc.contains("A"));
        assert!(desc.contains("B"));
        assert!(desc.contains("points to"));
        assert!(desc.contains("Submit"));
    }

    #[test]
    fn describe_edge_without_label() {
        let ir = create_test_ir();
        let desc = describe_edge(
            Some(&ir.nodes[0]),
            Some(&ir.nodes[1]),
            fm_core::ArrowType::Line,
            None,
            &ir,
        );
        assert!(desc.contains("connects to"));
        assert!(!desc.contains("with label"));
    }

    #[test]
    fn accessibility_css_includes_media_queries() {
        let css = accessibility_css();
        assert!(css.contains("prefers-contrast"));
        assert!(css.contains("prefers-reduced-motion"));
    }

    #[test]
    fn accessibility_css_includes_focus_indicators() {
        let css = accessibility_css();
        assert!(css.contains(":focus"));
        assert!(css.contains("outline"));
    }

    #[test]
    fn a11y_config_full_enables_all() {
        let config = A11yConfig::full();
        assert!(config.aria_labels);
        assert!(config.text_alternatives);
        assert!(config.keyboard_nav);
        assert!(config.accessibility_css);
    }

    #[test]
    fn a11y_config_none_disables_all() {
        let config = A11yConfig::none();
        assert!(!config.aria_labels);
        assert!(!config.text_alternatives);
        assert!(!config.keyboard_nav);
        assert!(!config.accessibility_css);
    }
}
