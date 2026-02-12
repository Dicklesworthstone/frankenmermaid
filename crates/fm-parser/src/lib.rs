#![forbid(unsafe_code)]

use fm_core::{DiagramType, MermaidDiagramIr};

#[derive(Debug, Clone, PartialEq)]
pub struct ParseResult {
    pub ir: MermaidDiagramIr,
    pub warnings: Vec<String>,
}

#[must_use]
pub fn detect_type(input: &str) -> DiagramType {
    let trimmed = input.trim_start();

    if trimmed.starts_with("flowchart") || trimmed.starts_with("graph") {
        DiagramType::Flowchart
    } else if trimmed.starts_with("sequenceDiagram") {
        DiagramType::Sequence
    } else if trimmed.starts_with("classDiagram") {
        DiagramType::Class
    } else {
        DiagramType::Unknown
    }
}

#[must_use]
pub fn parse(input: &str) -> ParseResult {
    let diagram_type = detect_type(input);
    let mut warnings = Vec::new();

    if input.trim().is_empty() {
        warnings.push("Input was empty; returning empty IR".to_string());
    }

    ParseResult {
        ir: MermaidDiagramIr::empty(diagram_type),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::{detect_type, parse};
    use fm_core::DiagramType;

    #[test]
    fn detects_flowchart_keyword() {
        assert_eq!(detect_type("flowchart LR\nA-->B"), DiagramType::Flowchart);
    }

    #[test]
    fn empty_input_returns_warning() {
        let result = parse("");
        assert_eq!(result.ir.diagram_type, DiagramType::Unknown);
        assert_eq!(result.warnings.len(), 1);
    }
}
