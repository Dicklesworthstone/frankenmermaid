#![forbid(unsafe_code)]

use fm_core::MermaidDiagramIr;
use fm_layout::{LayoutAlgorithm, layout};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasRenderResult {
    pub draw_calls: usize,
}

#[must_use]
pub fn render_canvas(ir: &MermaidDiagramIr) -> CanvasRenderResult {
    let stats = layout(ir, LayoutAlgorithm::Auto);
    CanvasRenderResult {
        draw_calls: stats.node_count.saturating_add(stats.edge_count),
    }
}

#[cfg(test)]
mod tests {
    use super::render_canvas;
    use fm_core::{DiagramType, MermaidDiagramIr};

    #[test]
    fn canvas_stub_computes_draw_calls() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let result = render_canvas(&ir);
        assert_eq!(result.draw_calls, 0);
    }
}
