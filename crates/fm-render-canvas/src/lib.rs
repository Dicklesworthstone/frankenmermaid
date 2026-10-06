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
/// Glyph rasterisation into the text atlas (bd-2u0.2). Device-free — it produces a plain R8 coverage
/// bitmap — but gated with `webgpu` because that is its only consumer today.
#[cfg(feature = "webgpu")]
pub mod glyph_raster;
/// The wgpu device layer (bd-2u0.2). Behind the `webgpu` feature so the Canvas2D path — and the
/// size-optimised WASM bundle this crate ships in — never pays for a GPU backend it does not use.
#[cfg(feature = "webgpu")]
pub mod gpu_device;
mod gpu_layout;
pub mod gpu_pipeline;
mod gpu_plan;
/// Hit regions for `click` on a raster surface (bd-2u0.2, closing the gap bd-bk7h identified).
///
/// Always available: it needs no GPU and no `web` feature, because Canvas2D hosts need it as much
/// as the WebGPU path does.
pub mod interaction;
mod renderer;
mod shapes;
mod viewport;

pub use context::{
    Canvas2dContext, Color, DrawOperation, LineCap, LineJoin, MockCanvas2dContext, Point,
    TextAlign, TextBaseline, TextMetrics,
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
pub use interaction::{HitRegion, NavigationDirection, NodeSelection, hit_regions, hit_test};
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
///
/// The returned viewport maps ORIGINAL layout coordinates to canvas pixels, including the
/// renderer's origin normalization and padding. It can therefore be used directly with
/// [`NodeSelection::select_at_canvas`] and the result's layout-space hit regions.
pub fn render_to_canvas_with_layout<C: Canvas2dContext>(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    context: &mut C,
    config: &CanvasRenderConfig,
) -> CanvasRenderResult {
    let mut renderer = Canvas2dRenderer::new(config.clone());
    let mut result = renderer.render(layout, ir, context);
    // The low-level renderer applies this translation to each primitive before its viewport
    // transform. Fold it into the reported viewport as well, or pointers/overlays drift whenever
    // the layout has a nonzero origin (or auto-fit is off). Drawing itself is unchanged.
    let padding = if config.auto_fit { 0.0 } else { config.padding };
    result.viewport.offset_x += (padding - f64::from(layout.bounds.x)) * result.viewport.zoom;
    result.viewport.offset_y += (padding - f64::from(layout.bounds.y)) * result.viewport.zoom;
    result
}

/// Appearance of the node focus ring. Distances are in canvas pixels, not diagram units.
///
/// The wide halo and narrower foreground stroke keep the ring distinguishable on both light
/// and dark nodes. Padding is the gap between the node box and the INNER edge of the halo.
/// The ring never fills or obscures the node. Invalid numeric settings suppress the overlay,
/// without suppressing the diagram or changing the selection.
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasSelectionStyle {
    /// Foreground ring color, in the same CSS notation as [`CanvasRenderConfig::node_stroke`].
    pub stroke: String,
    /// Contrasting color behind the foreground stroke.
    pub halo: String,
    /// Foreground stroke width; must be finite and positive.
    pub line_width: f64,
    /// Extra halo width on EACH side of the foreground; finite and nonnegative.
    pub halo_width: f64,
    /// Gap between the node box and the halo; finite and nonnegative.
    pub padding: f64,
}

impl Default for CanvasSelectionStyle {
    fn default() -> Self {
        Self {
            stroke: String::from("#2563eb"),
            halo: String::from("#ffffff"),
            line_width: 2.0,
            halo_width: 2.0,
            padding: 3.0,
        }
    }
}

/// Render a diagram and its current node selection from ONE pre-computed layout.
///
/// This is the interactive counterpart of [`render_to_canvas_with_layout`]. It clears selection
/// when the selected node has disappeared, renders the full diagram, then draws a focus ring.
/// Node/edge/label counts and click metadata are unchanged; `draw_calls` includes the ring.
/// An empty selection produces the exact same drawing operations as the ordinary renderer.
///
/// Hosts own event dispatch: route pointer coordinates through [`NodeSelection::select_at_canvas`]
/// using this result's viewport, and arrow keys through [`NodeSelection::navigate`], then call
/// this function again to erase the previous ring and draw the new one. Selection does not
/// activate `click` directives. Use the SAME IR/layout for drawing and event handling.
pub fn render_to_canvas_with_selection<C: Canvas2dContext>(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    context: &mut C,
    config: &CanvasRenderConfig,
    selection: &mut NodeSelection,
    style: &CanvasSelectionStyle,
) -> CanvasRenderResult {
    selection.reconcile(ir, layout);
    let mut result = render_to_canvas_with_layout(ir, layout, context, config);
    result.draw_calls +=
        draw_selection_overlay(ir, layout, context, &result.viewport, selection, style);
    result
}

/// Draw a selection ring over an already-rendered diagram. Returns the number of draw calls.
///
/// The viewport must map raw layout coordinates to canvas pixels, as returned by
/// [`render_to_canvas_with_layout`]. Ring widths/gap stay constant under pan/zoom. This function
/// preserves drawing state and does NOT clear the canvas; redraw the base diagram before moving
/// selection, or use a separate overlay canvas that the host clears between frames.
pub fn draw_selection_overlay<C: Canvas2dContext>(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    context: &mut C,
    viewport: &Viewport,
    selection: &NodeSelection,
    style: &CanvasSelectionStyle,
) -> usize {
    let Some(bounds) = selection.selected_bounds(ir, layout) else {
        return 0;
    };
    if !viewport.zoom.is_finite()
        || viewport.zoom <= 0.0
        || !viewport.offset_x.is_finite()
        || !viewport.offset_y.is_finite()
        || !style.line_width.is_finite()
        || style.line_width <= 0.0
        || !style.halo_width.is_finite()
        || style.halo_width < 0.0
        || !style.padding.is_finite()
        || style.padding < 0.0
    {
        return 0;
    }
    let halo_width = style.line_width + 2.0 * style.halo_width;
    let outset = style.padding + halo_width / 2.0;
    let (x, y) = viewport.diagram_to_canvas(f64::from(bounds.x), f64::from(bounds.y));
    let x = x - outset;
    let y = y - outset;
    let width = f64::from(bounds.width) * viewport.zoom + 2.0 * outset;
    let height = f64::from(bounds.height) * viewport.zoom + 2.0 * outset;
    if ![x, y, width, height, halo_width, x + width, y + height]
        .iter()
        .all(|value| value.is_finite())
    {
        return 0;
    }

    context.save();
    context.reset_transform();
    context.set_global_alpha(1.0);
    context.set_line_dash(&[]);
    context.set_line_join(LineJoin::Round);
    context.set_shadow_blur(0.0);
    context.set_shadow_offset(0.0, 0.0);
    context.set_shadow_color("transparent");
    context.set_stroke_style(&style.halo);
    context.set_line_width(halo_width);
    context.stroke_rect(x, y, width, height);
    context.set_stroke_style(&style.stroke);
    context.set_line_width(style.line_width);
    context.stroke_rect(x, y, width, height);
    context.restore();
    2
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
        // From the layout this function just computed — the same one every other number here
        // describes. It draws nothing, so these regions are as hypothetical as its `draw_calls`,
        // and consistent with them.
        hit_regions: interaction::hit_regions(ir, &layout),
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

    fn selection_fixture() -> (MermaidDiagramIr, DiagramLayout) {
        let ir = fm_parser::parse("flowchart LR\n A[Alpha]\n").ir;
        assert_eq!(ir.nodes.len(), 1);
        let mut layout = layout_diagram(&ir);
        assert_eq!(layout.nodes.len(), 1);
        layout.bounds = fm_layout::LayoutRect {
            x: -100.0,
            y: -50.0,
            width: 400.0,
            height: 200.0,
        };
        layout.nodes[0].bounds = fm_layout::LayoutRect {
            x: -80.0,
            y: -20.0,
            width: 40.0,
            height: 30.0,
        };
        (ir, layout)
    }

    fn assert_counts_match(left: &CanvasRenderResult, right: &CanvasRenderResult) {
        assert_eq!(left.nodes_drawn, right.nodes_drawn);
        assert_eq!(left.edges_drawn, right.edges_drawn);
        assert_eq!(left.clusters_drawn, right.clusters_drawn);
        assert_eq!(left.labels_drawn, right.labels_drawn);
        assert_eq!(left.hit_regions, right.hit_regions);
    }

    #[test]
    fn empty_selection_preserves_complete_render_stream() {
        let (ir, layout) = selection_fixture();
        let config = CanvasRenderConfig::default();
        let mut base = MockCanvas2dContext::new(800.0, 600.0);
        let expected = render_to_canvas_with_layout(&ir, &layout, &mut base, &config);
        let mut selected = MockCanvas2dContext::new(800.0, 600.0);
        let actual = render_to_canvas_with_selection(
            &ir,
            &layout,
            &mut selected,
            &config,
            &mut NodeSelection::default(),
            &CanvasSelectionStyle::default(),
        );
        assert_eq!(base.operations(), selected.operations());
        assert_eq!(expected.draw_calls, actual.draw_calls);
        assert_eq!(expected.viewport, actual.viewport);
        assert_counts_match(&expected, &actual);
    }

    #[test]
    fn selected_render_appends_visible_ring_without_changing_diagram() {
        let (ir, layout) = selection_fixture();
        let config = CanvasRenderConfig {
            auto_fit: false,
            padding: 10.0,
            ..CanvasRenderConfig::default()
        };
        let style = CanvasSelectionStyle::default();
        let mut selection = NodeSelection::default();
        assert!(selection.select_node(&ir, &layout, "A"));
        let mut base = MockCanvas2dContext::new(300.0, 200.0);
        let expected = render_to_canvas_with_layout(&ir, &layout, &mut base, &config);
        let mut context = MockCanvas2dContext::new(300.0, 200.0);
        let actual = render_to_canvas_with_selection(
            &ir,
            &layout,
            &mut context,
            &config,
            &mut selection,
            &style,
        );
        assert_counts_match(&expected, &actual);
        assert_eq!(actual.draw_calls, expected.draw_calls + 2);
        assert_eq!(actual.viewport, expected.viewport);
        assert_eq!(
            &context.operations()[..base.operation_count()],
            base.operations()
        );
        let overlay = &context.operations()[base.operation_count()..];
        assert_eq!(overlay.first(), Some(&DrawOperation::Save));
        assert_eq!(overlay.last(), Some(&DrawOperation::Restore));
        assert!(overlay.contains(&DrawOperation::ResetTransform));
        assert!(overlay.contains(&DrawOperation::SetGlobalAlpha(1.0)));
        assert!(overlay.contains(&DrawOperation::SetLineDash(Vec::new())));
        assert!(overlay.contains(&DrawOperation::SetStrokeStyle(style.halo)));
        assert!(overlay.contains(&DrawOperation::SetStrokeStyle(style.stroke)));
        assert!(overlay.contains(&DrawOperation::SetLineWidth(6.0)));
        assert!(overlay.contains(&DrawOperation::SetLineWidth(2.0)));
        // The DRAWN node starts at (30,40): subtract (-100,-50), then add padding 10.
        // The ring centerline is 6 pixels outside: gap 3 plus half the 6-pixel halo stroke.
        let rectangles: Vec<_> = overlay
            .iter()
            .filter(|op| matches!(op, DrawOperation::StrokeRect(..)))
            .cloned()
            .collect();
        assert_eq!(
            rectangles,
            vec![
                DrawOperation::StrokeRect(24.0, 34.0, 52.0, 42.0),
                DrawOperation::StrokeRect(24.0, 34.0, 52.0, 42.0),
            ]
        );
    }

    // Read the actual emitted transform/text operations, not the returned viewport. Comparing
    // a viewport round-trip only with itself would miss the renderer's per-primitive translation.
    fn drawn_label_position(context: &MockCanvas2dContext, label: &str) -> (f64, f64) {
        let mut transform = ViewportTransform::identity();
        let mut stack = Vec::new();
        for op in context.operations() {
            match op {
                DrawOperation::Save => stack.push(transform),
                DrawOperation::Restore => transform = stack.pop().unwrap(),
                DrawOperation::SetTransform(a, b, c, d, e, f) => {
                    transform = ViewportTransform {
                        a: *a,
                        b: *b,
                        c: *c,
                        d: *d,
                        e: *e,
                        f: *f,
                    };
                }
                DrawOperation::ResetTransform => transform = ViewportTransform::identity(),
                DrawOperation::Translate(x, y) => {
                    transform = transform.multiply(&ViewportTransform::translate(*x, *y));
                }
                DrawOperation::Scale(x, y) => {
                    transform = transform.multiply(&ViewportTransform::scale(*x, *y));
                }
                DrawOperation::Rotate(_) => panic!("unexpected rotation in flowchart fixture"),
                DrawOperation::FillText(text, x, y) if text == label => {
                    return transform.apply(*x, *y);
                }
                _ => {}
            }
        }
        panic!("render never drew label {label}");
    }

    #[test]
    fn returned_viewport_picks_actual_drawn_label_with_origin_and_padding() {
        let (ir, layout) = selection_fixture();
        for auto_fit in [false, true] {
            let config = CanvasRenderConfig {
                auto_fit,
                padding: 10.0,
                ..CanvasRenderConfig::default()
            };
            let mut context = MockCanvas2dContext::new(200.0, 120.0);
            let result = render_to_canvas_with_layout(&ir, &layout, &mut context, &config);
            let (x, y) = drawn_label_position(&context, "Alpha");
            let mut selection = NodeSelection::default();
            assert!(selection.select_at_canvas(&ir, &layout, &result.viewport, x, y));
            assert_eq!(selection.selected_node_id(), Some("A"));
        }
    }

    #[test]
    fn viewport_correction_preserves_low_level_drawing_operations() {
        let (ir, layout) = selection_fixture();
        for auto_fit in [false, true] {
            let config = CanvasRenderConfig {
                auto_fit,
                ..CanvasRenderConfig::default()
            };
            let mut low_level = MockCanvas2dContext::new(300.0, 200.0);
            let before = Canvas2dRenderer::new(config.clone()).render(&layout, &ir, &mut low_level);
            let mut entry_point = MockCanvas2dContext::new(300.0, 200.0);
            let after = render_to_canvas_with_layout(&ir, &layout, &mut entry_point, &config);
            assert_eq!(low_level.operations(), entry_point.operations());
            assert_eq!(before.draw_calls, after.draw_calls);
            assert_counts_match(&before, &after);
        }
    }

    #[test]
    fn overlay_width_and_gap_stay_in_pixels_after_pan_and_zoom() {
        let (ir, layout) = selection_fixture();
        let mut selection = NodeSelection::default();
        selection.select_node(&ir, &layout, "A");
        for zoom in [0.25, 2.0, 8.0] {
            let viewport = Viewport {
                offset_x: 100.0,
                offset_y: 200.0,
                zoom,
                device_pixel_ratio: 2.0,
                ..Viewport::default()
            };
            let mut context = MockCanvas2dContext::new(800.0, 600.0);
            assert_eq!(
                draw_selection_overlay(
                    &ir,
                    &layout,
                    &mut context,
                    &viewport,
                    &selection,
                    &CanvasSelectionStyle::default(),
                ),
                2
            );
            assert!(context.operations().contains(&DrawOperation::StrokeRect(
                100.0 - 80.0 * zoom - 6.0,
                200.0 - 20.0 * zoom - 6.0,
                40.0 * zoom + 12.0,
                30.0 * zoom + 12.0,
            )));
            let widths: Vec<_> = context
                .operations()
                .iter()
                .filter_map(|op| match op {
                    DrawOperation::SetLineWidth(width) => Some(*width),
                    _ => None,
                })
                .collect();
            assert_eq!(widths, vec![6.0, 2.0]);
        }
    }

    #[test]
    fn rendering_reconciles_removed_selection_without_a_ghost_ring() {
        let (ir, layout) = selection_fixture();
        let mut selection = NodeSelection::default();
        selection.select_node(&ir, &layout, "A");
        let replacement = fm_parser::parse("flowchart LR\n B[Beta]\n").ir;
        let replacement_layout = layout_diagram(&replacement);
        let config = CanvasRenderConfig::default();
        let mut context = MockCanvas2dContext::new(800.0, 600.0);
        let actual = render_to_canvas_with_selection(
            &replacement,
            &replacement_layout,
            &mut context,
            &config,
            &mut selection,
            &CanvasSelectionStyle::default(),
        );
        let mut base = MockCanvas2dContext::new(800.0, 600.0);
        let expected =
            render_to_canvas_with_layout(&replacement, &replacement_layout, &mut base, &config);
        assert_eq!(selection.selected_node_id(), None);
        assert_eq!(context.operations(), base.operations());
        assert_eq!(actual.draw_calls, expected.draw_calls);
    }

    #[test]
    fn invalid_overlay_geometry_and_style_emit_no_operations() {
        let (ir, layout) = selection_fixture();
        let mut selection = NodeSelection::default();
        selection.select_node(&ir, &layout, "A");
        for invalid in [f64::NAN, f64::INFINITY, -1.0] {
            for field in 0..3 {
                let mut style = CanvasSelectionStyle::default();
                match field {
                    0 => style.line_width = invalid,
                    1 => style.halo_width = invalid,
                    _ => style.padding = invalid,
                }
                let mut context = MockCanvas2dContext::new(800.0, 600.0);
                assert_eq!(
                    draw_selection_overlay(
                        &ir,
                        &layout,
                        &mut context,
                        &Viewport::default(),
                        &selection,
                        &style,
                    ),
                    0
                );
                assert!(context.operations().is_empty());
            }
        }
        for zoom in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX] {
            let viewport = Viewport {
                zoom,
                ..Viewport::default()
            };
            let mut context = MockCanvas2dContext::new(800.0, 600.0);
            assert_eq!(
                draw_selection_overlay(
                    &ir,
                    &layout,
                    &mut context,
                    &viewport,
                    &selection,
                    &CanvasSelectionStyle::default(),
                ),
                0
            );
            assert!(context.operations().is_empty());
        }
        assert_eq!(selection.selected_node_id(), Some("A"));
    }

    #[test]
    fn selection_keeps_link_callback_and_tooltip_metadata() {
        let (mut ir, layout) = selection_fixture();
        // Interaction policy is the parser/host's concern. Supply a resolved interaction on the
        // parsed node so this test checks preservation, not default security-policy decisions.
        let interaction = ir.nodes[0].interaction_mut();
        interaction.href = Some(String::from("https://example.com/alpha"));
        interaction.callback = Some(String::from("inspectAlpha"));
        interaction.tooltip = Some(String::from("Read Alpha"));
        interaction.link_target = Some(String::from("_self"));
        let mut selection = NodeSelection::default();
        selection.select_node(&ir, &layout, "A");
        let mut context = MockCanvas2dContext::new(800.0, 600.0);
        let result = render_to_canvas_with_selection(
            &ir,
            &layout,
            &mut context,
            &CanvasRenderConfig::default(),
            &mut selection,
            &CanvasSelectionStyle::default(),
        );
        assert_eq!(result.hit_regions.len(), 1);
        let region = &result.hit_regions[0];
        assert_eq!(region.href.as_deref(), Some("https://example.com/alpha"));
        assert_eq!(region.callback.as_deref(), Some("inspectAlpha"));
        assert_eq!(region.tooltip.as_deref(), Some("Read Alpha"));
        assert_eq!(region.link_target.as_deref(), Some("_self"));
        assert_eq!(region.bounds, layout.nodes[0].bounds);
    }
}
