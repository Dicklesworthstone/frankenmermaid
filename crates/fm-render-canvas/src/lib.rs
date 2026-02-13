#![forbid(unsafe_code)]

//! Canvas2D rendering backend for frankenmermaid diagrams.
//!
//! This crate provides a Canvas2D-based renderer for drawing diagrams
//! to HTML Canvas elements. The actual web-sys implementation is behind
//! the `web` feature flag.
//!
//! # Architecture
//!
//! The renderer uses a trait-based abstraction (`Canvas2dContext`) that
//! allows testing without web-sys and provides a clean API for drawing.
//!
//! # Features
//!
//! - `web`: Enables actual Canvas2D rendering via web-sys (WASM target)

mod context;
mod renderer;
mod shapes;
mod viewport;

pub use context::{
    Canvas2dContext, Color, LineCap, LineJoin, MockCanvas2dContext, Point, TextAlign, TextBaseline,
    TextMetrics,
};
pub use renderer::{Canvas2dRenderer, CanvasRenderConfig, CanvasRenderResult};
pub use viewport::{Viewport, ViewportTransform};

use fm_core::MermaidDiagramIr;
use fm_layout::layout_diagram;

/// Render a diagram to a Canvas2D context.
///
/// This is the main entry point for Canvas2D rendering. It computes
/// the layout and then draws the diagram using the provided context.
pub fn render_to_canvas<C: Canvas2dContext>(
    ir: &MermaidDiagramIr,
    context: &mut C,
    config: &CanvasRenderConfig,
) -> CanvasRenderResult {
    let layout = layout_diagram(ir);
    let mut renderer = Canvas2dRenderer::new(config.clone());
    renderer.render(&layout, ir, context)
}

/// Legacy function for backwards compatibility.
#[must_use]
pub fn render_canvas(ir: &MermaidDiagramIr) -> CanvasRenderResult {
    let layout = layout_diagram(ir);
    CanvasRenderResult {
        draw_calls: layout.stats.node_count + layout.stats.edge_count,
        nodes_drawn: layout.stats.node_count,
        edges_drawn: layout.stats.edge_count,
        clusters_drawn: layout.clusters.len(),
        labels_drawn: 0,
        viewport: Viewport::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{DiagramType, MermaidDiagramIr};

    #[test]
    fn canvas_stub_computes_draw_calls() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let result = render_canvas(&ir);
        assert_eq!(result.draw_calls, 0);
    }

    #[test]
    fn render_with_mock_context() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let config = CanvasRenderConfig::default();
        let mut context = MockCanvas2dContext::new(800.0, 600.0);
        let result = render_to_canvas(&ir, &mut context, &config);
        // At minimum: clear_rect call
        assert!(result.draw_calls >= 1);
        assert_eq!(result.nodes_drawn, 0);
        assert_eq!(result.edges_drawn, 0);
    }
}
