#![forbid(unsafe_code)]

mod dot_parser;
mod ir_builder;
mod mermaid_parser;

use fm_core::{DiagramType, MermaidDiagramIr};
use serde_json::json;

pub use dot_parser::{looks_like_dot, parse_dot};

#[derive(Debug, Clone, PartialEq)]
pub struct ParseResult {
    pub ir: MermaidDiagramIr,
    pub warnings: Vec<String>,
}

#[must_use]
pub fn detect_type(input: &str) -> DiagramType {
    mermaid_parser::detect_type(input)
}

#[must_use]
pub fn parse(input: &str) -> ParseResult {
    if input.trim().is_empty() {
        return ParseResult {
            ir: MermaidDiagramIr::empty(DiagramType::Unknown),
            warnings: vec!["Input was empty; returning empty IR".to_string()],
        };
    }

    if looks_like_dot(input) {
        return parse_dot(input);
    }

    mermaid_parser::parse_mermaid(input)
}

#[must_use]
pub fn parse_evidence_json(parsed: &ParseResult) -> String {
    json!({
        "diagram_type": parsed.ir.diagram_type.as_str(),
        "node_count": parsed.ir.nodes.len(),
        "edge_count": parsed.ir.edges.len(),
        "cluster_count": parsed.ir.clusters.len(),
        "label_count": parsed.ir.labels.len(),
        "warning_count": parsed.warnings.len(),
        "warnings": parsed.warnings.clone(),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{detect_type, parse};
    use fm_core::{ArrowType, DiagramType, GraphDirection, IrEndpoint};

    #[test]
    fn detects_flowchart_keyword() {
        assert_eq!(detect_type("flowchart LR\nA-->B"), DiagramType::Flowchart);
    }

    #[test]
    fn detects_sequence_keyword() {
        assert_eq!(
            detect_type("sequenceDiagram\nAlice->>Bob: Hello"),
            DiagramType::Sequence
        );
    }

    #[test]
    fn empty_input_returns_warning() {
        let result = parse("");
        assert_eq!(result.ir.diagram_type, DiagramType::Unknown);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn parse_flowchart_extracts_nodes_edges_and_direction() {
        let result = parse("flowchart LR\nA[Start] --> B(End)");
        assert_eq!(result.ir.diagram_type, DiagramType::Flowchart);
        assert_eq!(result.ir.direction, GraphDirection::LR);
        assert_eq!(result.ir.nodes.len(), 2);
        assert_eq!(result.ir.edges.len(), 1);
        assert!(result.warnings.is_empty());

        let edge = &result.ir.edges[0];
        assert_eq!(edge.arrow, ArrowType::Arrow);
        assert_eq!(edge.from, IrEndpoint::Node(fm_core::IrNodeId(0)));
        assert_eq!(edge.to, IrEndpoint::Node(fm_core::IrNodeId(1)));
    }

    #[test]
    fn parse_routes_dot_inputs_through_dot_parser() {
        let result = parse("digraph G { a -> b; }");
        assert_eq!(result.ir.diagram_type, DiagramType::Flowchart);
        assert_eq!(result.ir.nodes.len(), 2);
        assert_eq!(result.ir.edges.len(), 1);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn evidence_json_contains_counts_and_type() {
        let result = parse("flowchart LR\nA-->B");
        let evidence = super::parse_evidence_json(&result);
        assert!(evidence.contains("\"diagram_type\":\"flowchart\""));
        assert!(evidence.contains("\"node_count\":2"));
        assert!(evidence.contains("\"edge_count\":1"));
    }
}
