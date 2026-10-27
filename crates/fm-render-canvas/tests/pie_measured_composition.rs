//! A pie uses the same complete measured content on direct Canvas and shared scenes.

use fm_render_canvas::{Canvas2dRenderer, CanvasRenderConfig, DrawOperation, MockCanvas2dContext};

const SOURCE: &str = "pie showData\n  title Platform investment\n  \"Core infrastructure and reliability engineering\":90\n  \"Identity and access management\":1\n  \"Global customer analytics and reporting\":1\n  \"Developer productivity and delivery tooling\":1\n  \"Enterprise integrations and partner enablement\":1\n  \"Compliance evidence and audit automation\":1\n";

fn drawn_rows(context: &MockCanvas2dContext) -> Vec<(String, f64, f64)> {
    context
        .operations()
        .iter()
        .filter_map(|operation| match operation {
            DrawOperation::FillText(text, x, y) => Some((text.clone(), *x, *y)),
            _ => None,
        })
        .collect()
}

#[test]
fn canvas_and_scene_pies_keep_the_same_rows_and_coordinates_at_every_font_size() {
    let ir = fm_parser::parse(SOURCE).ir;
    let layout = fm_layout::layout_diagram(&ir);
    for font_size in [15.0, 24.0, 36.0] {
        let config = CanvasRenderConfig {
            font_size,
            padding: 0.0,
            auto_fit: false,
            ..Default::default()
        };
        let scene =
            fm_layout::build_render_scene_with_metrics(&ir, &layout, &config.font_metrics());
        let mut direct = MockCanvas2dContext::new(2400.0, 2400.0);
        let result = Canvas2dRenderer::new(config.clone()).render(&layout, &ir, &mut direct);
        let mut shared = MockCanvas2dContext::new(2400.0, 2400.0);
        Canvas2dRenderer::new(config).render_scene(&scene, &mut shared);
        let direct_rows = drawn_rows(&direct);
        let shared_rows = drawn_rows(&shared);
        assert_eq!(
            direct_rows, shared_rows,
            "pie consumers must not invent independent text positions"
        );
        assert_eq!(result.labels_drawn, direct_rows.len());
        assert!(
            direct_rows
                .iter()
                .any(|(text, _, _)| text == "Platform investment")
        );
        assert_eq!(
            direct_rows
                .iter()
                .filter(|(text, _, _)| text.ends_with('%'))
                .count(),
            6
        );
        assert!(direct_rows.iter().any(|(text, _, _)| text.contains("[90]")));
        assert!(
            direct_rows
                .iter()
                .any(|(text, _, _)| text.contains("automation"))
        );
        let arcs: Vec<_> = direct
            .operations()
            .iter()
            .filter_map(|operation| match operation {
                DrawOperation::Arc(x, y, radius, start, end) => {
                    Some((*x, *y, *radius, *start, *end))
                }
                _ => None,
            })
            .collect();
        assert_eq!(arcs.len(), 6);
        assert!(((arcs[0].4 - arcs[0].3) / (arcs[1].4 - arcs[1].3) - 90.0).abs() < 0.001);
        assert!(
            arcs.iter()
                .all(|arc| arc.0 == arcs[0].0 && arc.1 == arcs[0].1 && arc.2 == arcs[0].2)
        );
    }
}

#[test]
fn pie_layout_respects_configured_metrics_for_legend_hit_targets() {
    let ir = fm_parser::parse(SOURCE).ir;
    let metrics = fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
        font_size: 30.0,
        ..Default::default()
    });
    let layout = fm_layout::layout_diagram_with_config(
        &ir,
        fm_layout::LayoutConfig {
            font_metrics: Some(metrics.clone()),
            ..Default::default()
        },
    );
    let meta = ir.pie_meta.as_ref().unwrap();
    let pie = fm_layout::PieChartLayout::new(
        meta,
        meta.title.as_deref(),
        &metrics,
        8.0,
        metrics.avg_char_width(),
    );
    assert_eq!(layout.bounds, pie.bounds);
    for (node, entry) in layout.nodes.iter().zip(&pie.entries) {
        assert_eq!(node.bounds, entry.bounds());
    }
}
