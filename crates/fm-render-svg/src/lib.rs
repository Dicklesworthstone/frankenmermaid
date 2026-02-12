#![forbid(unsafe_code)]

use fm_core::MermaidDiagramIr;
use fm_layout::{LayoutAlgorithm, layout};

#[must_use]
pub fn render_svg(ir: &MermaidDiagramIr) -> String {
    let stats = layout(ir, LayoutAlgorithm::Auto);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" data-nodes=\"{}\" data-edges=\"{}\"></svg>",
        stats.node_count, stats.edge_count
    )
}

#[cfg(test)]
mod tests {
    use super::render_svg;
    use fm_core::{DiagramType, MermaidDiagramIr};

    #[test]
    fn emits_svg_document() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let svg = render_svg(&ir);
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
    }
}
