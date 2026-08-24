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
mod gpu_layout;
mod gpu_plan;
/// The wgpu device layer (bd-2u0.2). Behind the `webgpu` feature so the Canvas2D path — and the
/// size-optimised WASM bundle this crate ships in — never pays for a GPU backend it does not use.
#[cfg(feature = "webgpu")]
pub mod gpu_device;
/// Glyph rasterisation into the text atlas (bd-2u0.2). Device-free — it produces a plain R8 coverage
/// bitmap — but gated with `webgpu` because that is its only consumer today.
#[cfg(feature = "webgpu")]
pub mod glyph_raster;
pub mod gpu_pipeline;
mod renderer;
mod shapes;
mod viewport;

pub use context::{
    Canvas2dContext, Color, LineCap, LineJoin, MockCanvas2dContext, Point, TextAlign, TextBaseline,
    TextMetrics,
};
pub use gpu_layout::{
    GpuBufferLayout, GpuVertexAttribute, GpuVertexFormat, arrowhead_buffer_layout,
    edge_buffer_layout, node_buffer_layout, text_buffer_layout,
};
pub use gpu_plan::{
    ARROWHEAD_WGSL, EDGE_WGSL, GlyphAtlasPlan, GlyphCell, GpuArrowheadInstance, GpuEdgeSegment,
    GpuNodeInstance, GpuNodeShape, GpuRenderPlan, GpuTextQuad, GpuTextRun, NODE_SDF_WGSL,
    TEXT_ATLAS_WGSL, parse_paint_rgba,
};
pub use renderer::{Canvas2dRenderer, CanvasRenderConfig, CanvasRenderResult};
pub use viewport::{Viewport, ViewportTransform};

use fm_core::MermaidDiagramIr;
use fm_layout::{DiagramLayout, RenderScene, layout_diagram};

/// Render a diagram to a Canvas2D context.
///
/// This is the main entry point for Canvas2D rendering. It computes
/// the layout and then draws the diagram using the provided context.
pub fn render_to_canvas<C: Canvas2dContext>(
    ir: &MermaidDiagramIr,
    context: &mut C,
    config: &CanvasRenderConfig,
) -> CanvasRenderResult {
    let layout_config = fm_layout::LayoutConfig {
        font_metrics: Some(config.font_metrics()),
        ..Default::default()
    };
    let layout = fm_layout::layout_diagram_with_config(ir, layout_config);
    render_to_canvas_with_layout(ir, &layout, context, config)
}

/// Render a diagram with a pre-computed layout to a Canvas2D context.
pub fn render_to_canvas_with_layout<C: Canvas2dContext>(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    context: &mut C,
    config: &CanvasRenderConfig,
) -> CanvasRenderResult {
    let mut renderer = Canvas2dRenderer::new(config.clone());
    renderer.render(layout, ir, context)
}

/// Render an explicit shared render scene to a Canvas2D context.
pub fn render_scene_to_canvas<C: Canvas2dContext>(
    scene: &RenderScene,
    context: &mut C,
    config: &CanvasRenderConfig,
) -> CanvasRenderResult {
    let mut renderer = Canvas2dRenderer::new(config.clone());
    renderer.render_scene(scene, context)
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
