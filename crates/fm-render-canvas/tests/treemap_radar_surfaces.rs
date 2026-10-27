//! Treemap and radar reach the canvas and WebGPU surfaces at all (bd-dw450).
//!
//! Sibling of `fm-render-term/tests/treemap_radar_surfaces.rs`. Both families keep their whole
//! diagram in a layout extension (`treemap_tiles`, `radar`) that only fm-render-svg read, so a
//! valid `treemap` or `radar-beta` document produced an EMPTY SCENE here — zero draw calls on the
//! Canvas2D path and zero instances on the GPU path.
//!
//! THE NEGATIVE CASE, in this bead's shape: the output must DIFFER from the empty scene it used to
//! be. Asserted against a real render of a diagram with nothing in it rather than against a
//! hand-written expectation, so the assertion cannot pass by agreeing with a wrong constant.

use fm_render_canvas::{Canvas2dRenderer, CanvasRenderConfig, GpuRenderPlan, MockCanvas2dContext};

const TREEMAP: &str = "treemap\n\"R\"\n    \"G1\"\n        \"a\": 10\n        \"b\": 20\n    \"G2\"\n        \"c\": 30\n";
const RADAR: &str = "radar-beta\n  axis a, b, c\n  curve x{1,2,3}\n";

fn plan(source: &str) -> GpuRenderPlan {
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    GpuRenderPlan::from_layout(&ir, &layout, 1.5)
}

/// THE NEGATIVE CASE on the GPU path: a treemap emits instances where it used to emit none.
#[test]
fn a_treemap_is_not_an_empty_gpu_scene() {
    let treemap = plan(TREEMAP);
    assert!(
        !treemap.treemap_tile_instances.is_empty(),
        "a treemap still plans zero GPU instances"
    );
    // One instance per tile: R, G1, a, b, G2, c.
    assert_eq!(
        treemap.treemap_tile_instances.len(),
        6,
        "expected one instance per tile"
    );
    // Every tile has real extent — an instance with zero size is present but invisible, which
    // would satisfy a count-only assertion while drawing nothing.
    for instance in &treemap.treemap_tile_instances {
        assert!(
            instance.half_extent[0] > 0.0 && instance.half_extent[1] > 0.0,
            "a tile instance has no extent: {instance:?}"
        );
    }
}

/// THE NEGATIVE CASE on the GPU path for radar.
#[test]
fn a_radar_is_not_an_empty_gpu_scene() {
    use fm_render_canvas::GpuTextSource;

    let ir = fm_parser::parse(RADAR).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let wheel = layout.extensions.radar.as_ref().unwrap();
    let radar = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
    assert!(
        !radar.radar_segments.is_empty(),
        "a radar still plans zero GPU segments"
    );
    // Adaptive subdivision follows the shared cubic geometry. Verify each authored
    // component reaches the GPU without depending on a fixed sampling count.
    for axis in &wheel.axes {
        assert!(
            radar.radar_segments.iter().any(|segment| {
                segment.from == [wheel.center.x, wheel.center.y]
                    && segment.to == [axis.tip.x, axis.tip.y]
            }),
            "a spoke is missing"
        );
    }
    let grid_color = fm_render_canvas::parse_paint_rgba("#dedede").unwrap();
    for &radius in &wheel.rings {
        for point in [
            [wheel.center.x + radius, wheel.center.y],
            [wheel.center.x, wheel.center.y + radius],
            [wheel.center.x - radius, wheel.center.y],
            [wheel.center.x, wheel.center.y - radius],
        ] {
            assert!(
                radar.radar_segments.iter().any(|segment| {
                    segment.color == grid_color && (segment.from == point || segment.to == point)
                }),
                "a graticule ring lost its cardinal point: {point:?}"
            );
        }
    }
    let curve_color = fm_render_canvas::parse_paint_rgba("#8686ff").unwrap();
    for point in &wheel.curves[0].points {
        let point = [point.x, point.y];
        assert!(
            radar.radar_segments.iter().any(|segment| {
                segment.color == curve_color && (segment.from == point || segment.to == point)
            }),
            "an authored series value vanished: {point:?}"
        );
    }
    assert!(
        radar.xy_area_triangles.iter().any(|triangle| {
            let (a, b, c) = (triangle.a, triangle.b, triangle.c);
            let area = ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs();
            let x = (a[0] + b[0] + c[0]) / 3.0 - wheel.center.x;
            let y = (a[1] + b[1] + c[1]) / 3.0 - wheel.center.y;
            triangle.color[..3] == curve_color[..3]
                && triangle.color[3] > 0.0
                && area > 0.0
                && x * x + y * y < wheel.outer_radius * wheel.outer_radius
        }),
        "the plotted series needs a real fill inside the wheel, beyond grid and legend paint"
    );
    assert_eq!(
        radar
            .text_runs
            .iter()
            .filter(|run| run.source == GpuTextSource::RadarAxis)
            .count(),
        wheel.axes.len()
    );
    assert_eq!(
        radar
            .text_runs
            .iter()
            .filter(|run| run.source == GpuTextSource::RadarLegend)
            .count(),
        wheel.curves.len()
    );
    for segment in &radar.radar_segments {
        assert!(
            segment.from != segment.to,
            "a degenerate zero-length segment was planned"
        );
    }
}

/// The control: a family that never used these extensions plans none of this furniture.
#[test]
fn other_diagram_types_plan_no_treemap_or_radar_geometry() {
    let flowchart = plan("flowchart LR\n  A --> B\n  B --> C\n");
    assert!(
        flowchart.treemap_tile_instances.is_empty(),
        "a flowchart planned treemap instances"
    );
    assert!(
        flowchart.radar_segments.is_empty(),
        "a flowchart planned radar segments"
    );
    // And it still plans its own content, so this is not passing because planning is broken.
    assert!(
        !flowchart.node_instances.is_empty(),
        "the flowchart planned no nodes at all, so this control proves nothing"
    );
}

/// THE NEGATIVE CASE on the Canvas2D path: both families now issue draw calls.
///
/// Counted against a document that genuinely has nothing to draw, so the comparison is against the
/// real floor rather than against zero — a renderer that emits chrome for every diagram would make
/// a bare `> 0` assertion pass while still drawing no diagram.
#[test]
fn both_families_issue_canvas_draw_calls() {
    let empty_floor = draw_call_count("flowchart LR\n");
    for (name, source) in [("treemap", TREEMAP), ("radar", RADAR)] {
        let count = draw_call_count(source);
        assert!(
            count > empty_floor,
            "{name} issued {count} canvas draw calls against an empty-diagram floor of \
             {empty_floor}: nothing of it is being drawn"
        );
    }
}

fn draw_call_count(source: &str) -> usize {
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig::default());
    let mut ctx = MockCanvas2dContext::new(1200.0, 900.0);
    renderer.render(&layout, &ir, &mut ctx).draw_calls
}
