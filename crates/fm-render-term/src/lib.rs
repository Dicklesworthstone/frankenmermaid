#![forbid(unsafe_code)]

use fm_core::MermaidDiagramIr;
use fm_layout::{LayoutAlgorithm, layout};

#[must_use]
pub fn render_term(ir: &MermaidDiagramIr) -> String {
    let stats = layout(ir, LayoutAlgorithm::Auto);
    format!(
        "TERM_DIAGRAM nodes={} edges={}",
        stats.node_count, stats.edge_count
    )
}

#[cfg(test)]
mod tests {
    use super::render_term;
    use fm_core::{DiagramType, MermaidDiagramIr};

    #[test]
    fn emits_terminal_stub_string() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let output = render_term(&ir);
        assert!(output.contains("TERM_DIAGRAM"));
    }
}
