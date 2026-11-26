//! Is the GPU plan SUFFICIENT to draw the diagram? (bd-2u0.2)
//!
//! `GpuRenderPlan` is the data a WebGPU backend would upload. The question that decides whether it
//! is finished is not "does it build" but "could a shader pass reproduce the picture from it alone".
//! Nothing asked that before: `from_layout` is called only from its own unit tests, so the plan has
//! never been compared against what the Canvas2D pass actually draws.
//!
//! These tests measure the plan against the raster pass on the same diagram. Where the plan covers
//! the geometry they assert it; where it CANNOT yet express something they record the size of the
//! gap as a number, so the bead's remaining work is quantified rather than described.

use fm_render_canvas::{CanvasRenderConfig, GpuRenderPlan, MockCanvas2dContext, render_to_canvas};

const DIAGRAM: &str = "flowchart TD\n  a[Alpha] --> b[Beta]\n  b --> c[Gamma]\n  c -.-> a\n";

const RADAR: &str = "radar-beta\n  axis a[\"Reliability\"], b[\"Scale\"], c[\"Coverage\"]\n  curve current[\"Current platform\"]{1,2,3}\n  curve target[\"Target platform\"]{3,3,3}\n";

fn treemap_gpu_rows(plan: &GpuRenderPlan) -> Vec<(fm_render_canvas::GpuTextSource, u32, String)> {
    plan.text_runs
        .iter()
        .map(|run| {
            let text = plan.text_quads
                [run.first_quad as usize..(run.first_quad + run.quad_count) as usize]
                .iter()
                .map(|quad| {
                    let cell = plan
                        .glyph_atlas
                        .cells
                        .iter()
                        .find(|cell| {
                            cell.uv_min[0] <= quad.uv_min[0] + 1e-6
                                && cell.uv_min[1] <= quad.uv_min[1] + 1e-6
                                && cell.uv_max[0] >= quad.uv_max[0] - 1e-6
                                && cell.uv_max[1] >= quad.uv_max[1] - 1e-6
                        })
                        .expect("even a clipped glyph must sample its original atlas cell");
                    assert_eq!(
                        cell.face_index, run.face_index,
                        "a glyph sampled another face"
                    );
                    cell.glyph
                })
                .collect();
            (run.source, run.node_index, text)
        })
        .collect()
}

#[test]
fn treemap_canvas_scene_and_gpu_keep_hierarchy_colors_labels_and_deep_totals() {
    use fm_render_canvas::{Canvas2dRenderer, DrawOperation, GpuTextSource};
    let source = "treemap\n\"Root\"\n    \"Group one\"\n        \"Alpha\": 10\n        \"Beta\": 20\n    \"Group two\"\n        \"Gamma\": 30\n";
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let config = CanvasRenderConfig {
        auto_fit: false,
        padding: 0.0,
        ..Default::default()
    };
    let scene = fm_layout::build_render_scene_with_metrics(&ir, &layout, &config.font_metrics());
    let mut direct = MockCanvas2dContext::new(1400.0, 900.0);
    let result = Canvas2dRenderer::new(config.clone()).render(&layout, &ir, &mut direct);
    let mut shared = MockCanvas2dContext::new(1400.0, 900.0);
    Canvas2dRenderer::new(config.clone()).render_scene(&scene, &mut shared);
    let painted_rows = |context: &MockCanvas2dContext| {
        context
            .operations()
            .iter()
            .filter_map(|operation| {
                if let DrawOperation::FillText(text, x, y) = operation {
                    Some((text.clone(), *x, *y))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(painted_rows(&direct), painted_rows(&shared));
    assert_eq!(result.labels_drawn, 12);
    assert_eq!(result.nodes_drawn, 6);
    let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
    assert_eq!(plan.treemap_tile_instances.len(), 6);
    assert!(plan.node_instances.is_empty());
    assert!(plan.edge_segments.is_empty());
    let rows = treemap_gpu_rows(&plan);
    for (node, label, value) in [
        (0, "Root", "60"),
        (1, "Group one", "30"),
        (2, "Alpha", "10"),
        (3, "Beta", "20"),
        (4, "Group two", "30"),
        (5, "Gamma", "30"),
    ] {
        assert_eq!(
            rows.iter()
                .filter(
                    |(source, index, _)| *source == GpuTextSource::TreemapLabel && *index == node
                )
                .map(|(_, _, text)| text.as_str())
                .collect::<String>(),
            label
        );
        assert_eq!(
            rows.iter()
                .filter(
                    |(source, index, _)| *source == GpuTextSource::TreemapValue && *index == node
                )
                .map(|(_, _, text)| text.as_str())
                .collect::<String>(),
            value
        );
    }
    let meta = ir.treemap_meta.as_ref().unwrap();
    for (instance, original) in plan
        .treemap_tile_instances
        .iter()
        .zip(&layout.extensions.treemap_tiles)
    {
        assert_eq!(instance.node_index as usize, original.node);
        assert_eq!(
            instance.center,
            [
                original.bounds.x + original.bounds.width * 0.5,
                original.bounds.y + original.bounds.height * 0.5
            ]
        );
        assert_eq!(
            instance.half_extent,
            [original.bounds.width * 0.5, original.bounds.height * 0.5]
        );
        assert!(
            instance.fill[3] > 0.0,
            "the GPU must paint the grouping color"
        );
        if original.is_leaf {
            let parent = meta.nodes[original.node].parent.unwrap();
            let section = plan
                .treemap_tile_instances
                .iter()
                .find(|tile| tile.node_index as usize == parent)
                .unwrap();
            assert_eq!(instance.fill[..3], section.fill[..3]);
        }
    }
}

#[test]
fn treemap_gpu_retains_requested_paints_opacity_fonts_and_tile_clipping() {
    use fm_render_canvas::{GpuTextSource, parse_paint_rgba};
    let source = "treemap\n\"Root\"\n    \"Alpha\": 10:::hot\n    \"Beta\": 20:::invisible\nclassDef hot fill:#123456,stroke:#abcdef,stroke-width:3px,opacity:0.5,fill-opacity:0.8,stroke-opacity:0.6,color:#654321,font-size:24px\nclassDef invisible fill:none,stroke:none\n";
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let config = CanvasRenderConfig::default();
    let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
    let alpha = plan
        .treemap_tile_instances
        .iter()
        .find(|tile| tile.node_index == 1)
        .unwrap();
    let mut fill = parse_paint_rgba("#123456").unwrap();
    fill[3] = 0.4;
    let mut stroke = parse_paint_rgba("#abcdef").unwrap();
    stroke[3] = 0.3;
    assert_eq!(alpha.fill, fill);
    assert_eq!(alpha.stroke, stroke);
    assert_eq!(alpha.stroke_width, 3.0);
    let invisible = plan
        .treemap_tile_instances
        .iter()
        .find(|tile| tile.node_index == 2)
        .unwrap();
    assert_eq!(invisible.fill[3], 0.0);
    assert_eq!(invisible.stroke[3], 0.0);
    let rows = treemap_gpu_rows(&plan);
    assert!(rows.contains(&(GpuTextSource::TreemapLabel, 1, "Alpha".to_owned())));
    assert!(rows.contains(&(GpuTextSource::TreemapValue, 1, "10".to_owned())));
    let tile = layout
        .extensions
        .treemap_tiles
        .iter()
        .find(|tile| tile.node == 1)
        .unwrap();
    let mut text_color = parse_paint_rgba("#654321").unwrap();
    text_color[3] = 0.5;
    for run in plan.text_runs.iter().filter(|run| run.node_index == 1) {
        for quad in
            &plan.text_quads[run.first_quad as usize..(run.first_quad + run.quad_count) as usize]
        {
            assert_eq!(quad.color, text_color);
            assert_eq!(quad.half_extent[1], 12.0);
            assert!(quad.center[0] - quad.half_extent[0] >= tile.bounds.x - 1e-4);
            assert!(
                quad.center[0] + quad.half_extent[0] <= tile.bounds.x + tile.bounds.width + 1e-4
            );
            assert!(quad.center[1] - quad.half_extent[1] >= tile.bounds.y - 1e-4);
            assert!(
                quad.center[1] + quad.half_extent[1] <= tile.bounds.y + tile.bounds.height + 1e-4
            );
        }
    }
}

#[test]
fn treemap_gpu_faces_preserve_styled_default_detail_and_title_rows() {
    use fm_render_canvas::{GlyphFace, GpuTextSource};

    let source = r#"treemap
title Shared
"Root"
    "Shared": 10:::styled
    "Plain": 10
    "Family": 10:::family
    "Small": 0:::styled
classDef styled font-family:monospace,font-weight:700,font-style:italic
classDef family font-family:Courier New
"#;
    let ir = fm_parser::parse(source).ir;
    let meta = ir.treemap_meta.as_ref().unwrap();
    assert_eq!(
        meta.nodes
            .iter()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>(),
        ["Root", "Shared", "Plain", "Family", "Small"]
    );
    let layout = fm_layout::layout_diagram(&ir);
    let config = CanvasRenderConfig {
        font_family: "serif".to_owned(),
        ..Default::default()
    };
    let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
    let default = GlyphFace::new("serif");
    let styled = GlyphFace {
        family: "monospace".to_owned(),
        weight: "700".to_owned(),
        style: "italic".to_owned(),
    };
    let family = GlyphFace::new("Courier New");
    let title = GlyphFace {
        weight: "600".to_owned(),
        ..default.clone()
    };

    for run in &plan.text_runs {
        let expected = match run.source {
            GpuTextSource::DiagramTitle => &title,
            GpuTextSource::TreemapLabel
            | GpuTextSource::TreemapValue
            | GpuTextSource::TreemapDetail
            | GpuTextSource::TreemapReference => match run.node_index {
                1 | 4 => &styled,
                3 => &family,
                _ => &default,
            },
            other => panic!("unexpected treemap text source: {other:?}"),
        };
        assert!(run.quad_count > 0);
        assert_eq!(&plan.glyph_atlas.faces[run.face_index as usize], expected);
    }

    // Reconstruct the visible text from the actual UVs, checking that every glyph samples its
    // run's face even after tile clipping. A descriptor alone cannot prove the quad uses it.
    let rows = treemap_gpu_rows(&plan);
    for (source, node, text) in [
        (GpuTextSource::TreemapLabel, 1, "Shared"),
        (GpuTextSource::TreemapValue, 1, "10"),
        (GpuTextSource::TreemapLabel, 2, "Plain"),
        (GpuTextSource::TreemapValue, 2, "10"),
        (GpuTextSource::TreemapLabel, 3, "Family"),
        (GpuTextSource::TreemapValue, 3, "10"),
        (GpuTextSource::TreemapValue, 4, "0"),
        (GpuTextSource::DiagramTitle, 0, "Shared"),
    ] {
        assert!(
            rows.contains(&(source, node, text.to_owned())),
            "missing {text:?}"
        );
    }
    assert!(rows.iter().any(|(source, node, text)| {
        *source == GpuTextSource::TreemapDetail && *node == 4 && text.contains("Root / Small")
    }));

    let faces = [&default, &styled, &family, &title];
    assert_eq!(plan.glyph_atlas.faces.len(), faces.len());
    let mut cells = Vec::new();
    for face in faces {
        let index = plan.glyph_atlas.face_index(face).unwrap();
        let cell = plan.glyph_atlas.cell_for_face(index, 'a').unwrap();
        assert!(cells.iter().all(|uv| *uv != (cell.uv_min, cell.uv_max)));
        cells.push((cell.uv_min, cell.uv_max));
    }
    assert_eq!(
        plan,
        GpuRenderPlan::from_layout_with_config(&ir, &layout, &config)
    );
}

#[test]
fn treemap_tiny_and_zero_entries_keep_full_text_values_and_title_outside_tile_area() {
    use fm_render_canvas::{Canvas2dRenderer, DrawOperation, GpuTextSource};
    let source = "treemap\ntitle Capacity\n\"Root\"\n    \"Tiny  区域  👩‍💻  café\": 0.00001\n    \"Large\": 100000\n    \"Zero\": 0\n";
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let original = layout.extensions.treemap_tiles.clone();
    let config = CanvasRenderConfig {
        font_size: 24.0,
        ..Default::default()
    };
    let chart = fm_layout::TreemapChartLayout::new(&ir, &layout, &config.font_metrics());
    assert!(chart.details.iter().any(|detail| detail.node == 1));
    assert!(chart.details.iter().any(|detail| detail.node == 3));
    let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
    let rows = treemap_gpu_rows(&plan);
    let details = rows
        .iter()
        .filter(|(source, node, _)| *source == GpuTextSource::TreemapDetail && *node == 1)
        .map(|(_, _, text)| text.as_str())
        .collect::<String>();
    assert!(
        details.contains("Root / Tiny  区域  👩‍💻  café"),
        "{details:?}"
    );
    assert!(rows.contains(&(GpuTextSource::TreemapValue, 1, "0.00001".to_owned())));
    assert!(rows.contains(&(GpuTextSource::TreemapValue, 3, "0".to_owned())));
    assert_eq!(
        rows.iter()
            .filter(|(source, _, _)| *source == GpuTextSource::DiagramTitle)
            .count(),
        1
    );
    assert!(rows.contains(&(GpuTextSource::DiagramTitle, 0, "Capacity".to_owned())));
    assert!(plan.bounds.width > layout.bounds.width);
    let mut canvas = MockCanvas2dContext::new(1800.0, 1000.0);
    let result = Canvas2dRenderer::new(config.clone()).render(&layout, &ir, &mut canvas);
    let texts: Vec<_> = canvas
        .operations()
        .iter()
        .filter_map(|operation| {
            if let DrawOperation::FillText(text, _, _) = operation {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(texts.iter().filter(|text| **text == "Capacity").count(), 1);
    assert!(texts.contains(&"0.00001"));
    assert!(texts.concat().contains("Tiny  区域  👩‍💻  café"));
    assert_eq!(result.labels_drawn, texts.len());
    assert_eq!(layout.extensions.treemap_tiles, original);
    assert_eq!(
        plan,
        GpuRenderPlan::from_layout_with_config(&ir, &layout, &config)
    );
    for quad in &plan.text_quads {
        for value in quad.center.into_iter().chain(quad.half_extent) {
            assert!(value.is_finite());
        }
        assert!(quad.center[0] - quad.half_extent[0] >= plan.bounds.x - 1e-4);
        assert!(quad.center[1] - quad.half_extent[1] >= plan.bounds.y - 1e-4);
        assert!(quad.center[0] + quad.half_extent[0] <= plan.bounds.x + plan.bounds.width + 1e-4);
        assert!(quad.center[1] + quad.half_extent[1] <= plan.bounds.y + plan.bounds.height + 1e-4);
    }
}

#[test]
fn treemap_authored_courier_and_wide_fallback_glyphs_retain_every_character() {
    use fm_render_canvas::GpuTextSource;
    let label = "W".repeat(24);
    let source = format!(
        "treemap\n\"Root\"\n    \"{label}\": 10:::mono\nclassDef mono font-family:Courier New,font-size:28px\n"
    );
    let ir = fm_parser::parse(&source).ir;
    for width in [180.0, 320.0, 640.0] {
        let mut layout = fm_layout::layout_diagram(&ir);
        let leaf = layout
            .extensions
            .treemap_tiles
            .iter_mut()
            .find(|tile| tile.node == 1)
            .unwrap();
        leaf.bounds.width = width;
        leaf.bounds.height = 180.0;
        let config = CanvasRenderConfig {
            font_family: "Arial, sans-serif".to_owned(),
            ..Default::default()
        };
        let chart = fm_layout::TreemapChartLayout::new(&ir, &layout, &config.font_metrics());
        let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
        let rows = treemap_gpu_rows(&plan);
        let caption = rows
            .iter()
            .filter(|(source, node, _)| {
                *node == 1
                    && matches!(
                        source,
                        GpuTextSource::TreemapLabel | GpuTextSource::TreemapDetail
                    )
            })
            .map(|(_, _, text)| text.as_str())
            .collect::<String>();
        assert!(
            caption.contains(&label),
            "width {width} lost glyphs: {caption:?}"
        );
        let block = chart
            .text
            .iter()
            .find(|block| {
                matches!(
                    block.role,
                    fm_layout::TreemapTextRole::Label(1)
                        | fm_layout::TreemapTextRole::DetailPath(1)
                )
            })
            .unwrap();
        for run in plan.text_runs.iter().filter(|run| {
            run.node_index == 1
                && matches!(
                    run.source,
                    GpuTextSource::TreemapLabel | GpuTextSource::TreemapDetail
                )
        }) {
            for quad in &plan.text_quads
                [run.first_quad as usize..(run.first_quad + run.quad_count) as usize]
            {
                if let Some(clip) = block.clip {
                    assert!(quad.center[0] - quad.half_extent[0] >= clip.x - 1e-4);
                    assert!(quad.center[0] + quad.half_extent[0] <= clip.x + clip.width + 1e-4);
                    assert!(quad.center[1] - quad.half_extent[1] >= clip.y - 1e-4);
                    assert!(quad.center[1] + quad.half_extent[1] <= clip.y + clip.height + 1e-4);
                } else {
                    assert!(
                        quad.center[0] + quad.half_extent[0]
                            <= plan.bounds.x + plan.bounds.width + 1e-4
                    );
                }
            }
        }
    }
}

fn radar_gpu_rows(plan: &GpuRenderPlan) -> Vec<String> {
    plan.text_runs
        .iter()
        .map(|run| {
            plan.text_quads[run.first_quad as usize..(run.first_quad + run.quad_count) as usize]
                .iter()
                .map(|quad| {
                    plan.glyph_atlas
                        .cells
                        .iter()
                        .find(|cell| cell.uv_min == quad.uv_min && cell.uv_max == quad.uv_max)
                        .expect("every planned glyph needs its atlas cell")
                        .glyph
                })
                .collect()
        })
        .collect()
}

#[test]
fn radar_canvas_scene_and_gpu_keep_axis_and_series_identity() {
    use fm_render_canvas::{Canvas2dRenderer, DrawOperation, GpuTextSource};
    let parsed = fm_parser::parse(RADAR);
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    let ir = parsed.ir;
    let layout = fm_layout::layout_diagram(&ir);
    let config = CanvasRenderConfig {
        auto_fit: false,
        padding: 0.0,
        ..Default::default()
    };
    let scene = fm_layout::build_render_scene_with_metrics(&ir, &layout, &config.font_metrics());
    let mut canvas = MockCanvas2dContext::new(1800.0, 1200.0);
    let result = Canvas2dRenderer::new(config.clone()).render(&layout, &ir, &mut canvas);
    let mut shared = MockCanvas2dContext::new(1800.0, 1200.0);
    Canvas2dRenderer::new(config.clone()).render_scene(&scene, &mut shared);
    let rows = |context: &MockCanvas2dContext| {
        context
            .operations()
            .iter()
            .filter_map(|op| {
                if let DrawOperation::FillText(text, x, y) = op {
                    Some((text.clone(), *x, *y))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    let actual = rows(&canvas);
    assert_eq!(actual, rows(&shared));
    assert_eq!(actual.len(), result.labels_drawn);
    let expected = [
        "Reliability",
        "Scale",
        "Coverage",
        "Current platform",
        "Target platform",
    ];
    assert_eq!(
        actual
            .iter()
            .map(|(text, _, _)| text.as_str())
            .collect::<Vec<_>>(),
        expected
    );
    let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
    assert_eq!(radar_gpu_rows(&plan), expected);
    assert_eq!(
        plan.text_runs
            .iter()
            .filter(|run| run.source == GpuTextSource::RadarAxis)
            .count(),
        3
    );
    assert_eq!(
        plan.text_runs
            .iter()
            .filter(|run| run.source == GpuTextSource::RadarLegend)
            .count(),
        2
    );
    for color in ["#8686ff", "#ffff78"] {
        let rgba = fm_render_canvas::parse_paint_rgba(color).unwrap();
        assert!(
            plan.radar_segments
                .iter()
                .any(|segment| segment.color == rgba)
        );
        assert!(
            plan.xy_area_triangles
                .iter()
                .any(|triangle| triangle.color[..3] == rgba[..3] && triangle.color[3] == 0.5)
        );
    }
    assert!(
        plan.node_instances.is_empty(),
        "radar samples are not graph nodes"
    );
    assert!(plan.arrowheads.is_empty());
}

#[test]
fn radar_polygon_mode_and_hidden_legend_reach_every_shared_surface() {
    use fm_render_canvas::{Canvas2dRenderer, DrawOperation, GpuTextSource};
    let ir = fm_parser::parse(&format!("{RADAR}  graticule polygon\n  showLegend false\n")).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let config = CanvasRenderConfig::default();
    let mut canvas = MockCanvas2dContext::new(1400.0, 900.0);
    Canvas2dRenderer::new(config.clone()).render(&layout, &ir, &mut canvas);
    assert!(
        !canvas
            .operations()
            .iter()
            .any(|op| matches!(op, DrawOperation::BezierCurveTo(..)))
    );
    assert!(
        !canvas.operations().iter().any(
            |op| matches!(op, DrawOperation::FillText(text, _, _) if text.contains("platform"))
        )
    );
    let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
    assert!(
        !plan
            .text_runs
            .iter()
            .any(|run| run.source == GpuTextSource::RadarLegend)
    );
    // Five triangular rings, three spokes and two closed three-edge series.
    assert_eq!(
        plan.radar_segments
            .iter()
            .filter(|segment| segment.from != segment.to)
            .count(),
        24
    );
    let svg = fm_render_svg::render_svg_with_layout(
        &ir,
        &layout,
        &fm_render_svg::SvgRenderConfig {
            backend: fm_render_svg::SvgBackend::Scene,
            ..Default::default()
        },
    );
    for axis in ["Reliability", "Scale", "Coverage"] {
        assert!(svg.contains(axis));
    }
    assert!(!svg.contains("Current platform"));
    assert!(!svg.contains("Target platform"));
}

#[test]
fn radar_large_unicode_captions_fit_without_changing_data_positions() {
    let mut ir = fm_parser::parse(RADAR).ir;
    let meta = ir.radar_meta.as_mut().unwrap();
    meta.axes[1].label =
        Some("地域別  可用性 👩‍💻 and reliable customer operations across all regions".to_owned());
    meta.curves[1].label = Some(
        "Target  service quality with complete coverage and operational resilience".to_owned(),
    );
    let layout = fm_layout::layout_diagram(&ir);
    let original = layout.extensions.radar.clone();
    for font_size in [15.0, 30.0, 48.0] {
        let config = CanvasRenderConfig {
            font_size,
            ..Default::default()
        };
        let chart = fm_layout::RadarChartLayout::new(
            ir.radar_meta.as_ref().unwrap(),
            original.as_ref().unwrap(),
            None,
            &config.font_metrics(),
        );
        for block in &chart.text {
            assert!(block.bounds.x >= chart.bounds.x);
            assert!(block.bounds.y >= chart.bounds.y);
            assert!(
                block.bounds.x + block.bounds.width <= chart.bounds.x + chart.bounds.width + 0.001
            );
            assert!(
                block.bounds.y + block.bounds.height
                    <= chart.bounds.y + chart.bounds.height + 0.001
            );
        }
        let name = chart
            .text
            .iter()
            .find(|block| block.role == fm_layout::RadarTextRole::Axis(1))
            .unwrap();
        assert_eq!(
            name.lines.concat(),
            ir.radar_meta.as_ref().unwrap().axes[1].display()
        );
        let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
        assert!(radar_gpu_rows(&plan).concat().contains("地域別  可用性"));
        for quad in &plan.text_quads {
            assert!(quad.center[0] - quad.half_extent[0] >= plan.bounds.x - 0.001);
            assert!(
                quad.center[0] + quad.half_extent[0] <= plan.bounds.x + plan.bounds.width + 0.001
            );
            assert!(quad.center[1] - quad.half_extent[1] >= plan.bounds.y - 0.001);
            assert!(
                quad.center[1] + quad.half_extent[1] <= plan.bounds.y + plan.bounds.height + 0.001
            );
        }
        assert_eq!(layout.extensions.radar, original);
    }
}

#[test]
fn radar_crowded_polar_captions_do_not_overlap_across_the_wheel() {
    let axes = (0..20)
        .map(|index| format!("a{index}[\"Axis {index} with complete operational coverage\"]"))
        .collect::<Vec<_>>()
        .join(", ");
    let source =
        format!("radar-beta\n title Dense capabilities\n axis {axes}\n curve current{{1}}\n");
    let ir = fm_parser::parse(&source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let radar = layout.extensions.radar.as_ref().unwrap();
    for font_size in [15.0, 48.0] {
        let config = CanvasRenderConfig {
            font_size,
            ..Default::default()
        };
        let chart = fm_layout::RadarChartLayout::new(
            ir.radar_meta.as_ref().unwrap(),
            radar,
            ir.declared_title_if_drawn(),
            &config.font_metrics(),
        );
        assert_eq!(
            chart
                .text
                .iter()
                .filter(|block| matches!(block.role, fm_layout::RadarTextRole::Axis(_)))
                .count(),
            20
        );
        assert!(
            chart
                .paths
                .iter()
                .any(|path| path.role == fm_layout::RadarPathRole::Leader)
        );
        for (index, first) in chart.text.iter().enumerate() {
            for second in &chart.text[index + 1..] {
                let (a, b) = (first.bounds, second.bounds);
                assert!(
                    a.x + a.width <= b.x
                        || b.x + b.width <= a.x
                        || a.y + a.height <= b.y
                        || b.y + b.height <= a.y,
                    "overlapping {:?} and {:?} at {font_size}px: {a:?}, {b:?}",
                    first.role,
                    second.role
                );
            }
        }
    }
}

#[test]
fn radar_five_hundred_axis_cusps_keep_all_data_with_finite_bounded_primitives() {
    use fm_render_canvas::GpuTextSource;

    let axes = (0..500)
        .map(|index| format!("a{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let values = (0..500)
        .map(|index| if index % 2 == 0 { "0" } else { "10" })
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "radar-beta\n axis {axes}\n curve cusps{{{values}}}\n curve zero{{0}}\n min 0\n max 10\n showLegend false\n"
    );
    let parsed = fm_parser::parse(&source);
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    let layout = fm_layout::layout_diagram(&parsed.ir);
    let radar = layout.extensions.radar.as_ref().unwrap();
    assert_eq!(radar.axes.len(), 500);
    assert!(radar.curves.iter().all(|curve| curve.points.len() == 500));
    let plan =
        GpuRenderPlan::from_layout_with_config(&parsed.ir, &layout, &CanvasRenderConfig::default());
    assert_eq!(
        plan.text_runs
            .iter()
            .filter(|run| run.source == GpuTextSource::RadarAxis)
            .count(),
        500
    );
    let curve_color = fm_render_canvas::parse_paint_rgba("#8686ff").unwrap();
    let curve_segments: Vec<_> = plan
        .radar_segments
        .iter()
        .filter(|segment| segment.color == curve_color)
        .collect();
    for point in &radar.curves[0].points {
        assert!(
            curve_segments.iter().any(|segment| {
                segment.from == [point.x, point.y] || segment.to == [point.x, point.y]
            }),
            "a cusp's authored sample vanished from the GPU path: {point:?}"
        );
    }
    // Dense cusps exercise subdivision and self-intersections; the collapsed series
    // exercises degenerate fill edges. Neither may amplify into unbounded buffers.
    assert!(!plan.xy_area_triangles.is_empty());
    assert!(
        plan.radar_segments.len() < 500 * 256,
        "{} segments",
        plan.radar_segments.len()
    );
    assert!(
        plan.xy_area_triangles.len() < 500 * 512,
        "{} triangles",
        plan.xy_area_triangles.len()
    );
    for triangle in &plan.xy_area_triangles {
        assert!(
            triangle
                .a
                .into_iter()
                .chain(triangle.b)
                .chain(triangle.c)
                .all(f32::is_finite)
        );
    }
    for segment in &plan.radar_segments {
        assert!(
            segment
                .from
                .into_iter()
                .chain(segment.to)
                .all(f32::is_finite)
        );
    }
}

/// Count `FillText` operations in a recorded canvas run.
fn fill_text_count(ops_debug: &str) -> usize {
    ops_debug.matches("FillText(").count()
}

/// The plan must carry ONE instance per laid-out node.
///
/// A missing instance is a node the GPU pass would simply not draw, which no amount of shader work
/// recovers.
#[test]
fn the_plan_carries_every_laid_out_node() {
    let ir = fm_parser::parse(DIAGRAM).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.25);

    assert_eq!(
        plan.node_instances.len(),
        layout.nodes.len(),
        "the plan lost nodes: {} instances for {} laid-out nodes",
        plan.node_instances.len(),
        layout.nodes.len()
    );
    assert!(
        !layout.nodes.is_empty(),
        "the fixture produced no nodes, so the equality above proves nothing"
    );
}

/// Every non-bundled edge must contribute its complete path and arrowhead.
///
/// Curved connectors need more GPU chords than routed waypoints. Compare the actual shared path
/// boundaries and continuity rather than treating one route leg as one rendered segment. The
/// independent curve-sampling unit test checks the approximation's 0.05-unit flatness bound.
#[test]
fn the_plan_carries_every_edge_segment_and_head() {
    use fm_layout::PathCmd;

    let ir = fm_parser::parse(DIAGRAM).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.25);

    let mut covered_segments = 0;
    let mut directed = 0;
    for edge in layout.edges.iter().filter(|edge| !edge.bundled) {
        let mut commands = Vec::new();
        let endpoints = fm_layout::edge_paths::emit_diagram_edge_path(
            &ir,
            edge.edge_index,
            &edge.points,
            |command| commands.push(command),
        )
        .expect("fixture edges must have drawable paths");
        let segments: Vec<_> = plan
            .edge_segments
            .iter()
            .filter(|segment| segment.edge_index as usize == edge.edge_index)
            .collect();
        assert!(
            !segments.is_empty(),
            "edge {} lost its path",
            edge.edge_index
        );
        assert_eq!(
            segments[0].from,
            [endpoints.start.point.x, endpoints.start.point.y]
        );
        assert_eq!(
            segments.last().unwrap().to,
            [endpoints.end.point.x, endpoints.end.point.y]
        );
        assert!(segments.windows(2).all(|pair| pair[0].to == pair[1].from));
        assert!(segments.iter().all(|segment| segment.from != segment.to));

        // Every line/curve boundary the Canvas and Scene emit must occur in the
        // GPU chain: matching only its outer endpoints would permit missing legs.
        for command in commands {
            let end = match command {
                PathCmd::MoveTo { x, y }
                | PathCmd::LineTo { x, y }
                | PathCmd::QuadTo { x, y, .. }
                | PathCmd::CubicTo { x, y, .. } => [x, y],
                PathCmd::Close => panic!("connector fixture must remain open"),
            };
            assert!(
                segments[0].from == end || segments.iter().any(|segment| segment.to == end),
                "edge {} omitted a rendered path boundary at {end:?}",
                edge.edge_index
            );
        }
        let mut distance = 0.0;
        for segment in &segments {
            assert!((segment.dash_phase - distance).abs() < 0.001);
            distance += (segment.to[0] - segment.from[0]).hypot(segment.to[1] - segment.from[1]);
        }
        covered_segments += segments.len();

        // `-.->` is directed, so every fixture edge takes exactly one end head.
        let heads: Vec<_> = plan
            .arrowheads
            .iter()
            .filter(|head| head.edge_index as usize == edge.edge_index)
            .collect();
        assert_eq!(heads.len(), 1, "edge {} lost its head", edge.edge_index);
        assert_eq!(
            heads[0].position,
            [endpoints.end.point.x, endpoints.end.point.y]
        );
        assert_eq!(
            heads[0].angle,
            endpoints.end.tangent.y.atan2(endpoints.end.tangent.x)
        );
        directed += 1;
    }
    assert!(covered_segments > 0, "the fixture produced no segments");
    assert_eq!(
        covered_segments,
        plan.edge_segments.len(),
        "unaccounted edge geometry"
    );

    assert_eq!(
        plan.arrowheads.len(),
        directed,
        "expected one head per directed edge, got {:?}",
        plan.arrowheads.len()
    );
}

/// The plan now carries text, and this asserts the RUN COUNT matches the raster pass.
///
/// This test previously asserted the ABSENCE of a text buffer and instructed its own replacement
/// once the glyph atlas landed (bd-2u0.2 component 3). That has happened, so the shortfall
/// assertion is gone and the comparison it named is here instead.
///
/// RUNS, not glyphs, is the comparable unit: the raster pass issues one `fill_text` per label, so a
/// glyph count would compare 24 against 3 and never agree. Each run carries its own quad range, so
/// the glyph detail is still checked — by `a_node_label_becomes_one_run_of_glyph_quads` in the
/// module's unit tests, where the fixture is small enough for an exact count to mean something.
#[test]
fn the_plan_carries_one_text_run_per_drawn_label() {
    let ir = fm_parser::parse(DIAGRAM).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.25);

    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let drawn_text = fill_text_count(&format!("{:?}", context.operations()));

    assert!(
        drawn_text > 0,
        "the raster pass drew no text, so this comparison would be vacuous"
    );

    assert_eq!(
        plan.text_runs.len(),
        drawn_text,
        "the plan carries {} text runs against {drawn_text} the raster pass draws",
        plan.text_runs.len()
    );

    // Every run must actually carry glyphs, or the count above is satisfied by empty runs.
    for run in &plan.text_runs {
        assert!(
            run.quad_count > 0,
            "a text run carries no glyph quads: {run:?}"
        );
        let end = usize::try_from(run.first_quad + run.quad_count).unwrap_or(usize::MAX);
        assert!(
            end <= plan.text_quads.len(),
            "run range {run:?} overruns the {} quads in the buffer",
            plan.text_quads.len()
        );
    }

    // And the atlas must cover them: a quad pointing at a cell the atlas does not have would sample
    // whatever else happens to live at those UVs.
    assert!(
        !plan.glyph_atlas.cells.is_empty(),
        "text quads exist but the atlas is empty"
    );
}

mod annotations {
    use super::*;
    use fm_render_canvas::gpu_pipeline::{PrimitiveFamily, draw_batches};
    use fm_render_canvas::{DrawOperation, GpuTextSource, render_to_canvas_with_layout};

    fn rows(plan: &GpuRenderPlan, source: GpuTextSource) -> Vec<String> {
        treemap_gpu_rows(plan)
            .into_iter()
            .filter_map(|(kind, _, text)| (kind == source).then_some(text))
            .collect()
    }

    fn assert_text_fits(plan: &GpuRenderPlan) {
        assert!(!plan.text_quads.is_empty());
        for quad in &plan.text_quads {
            assert!(quad.center.iter().all(|value| value.is_finite()));
            assert!(quad.center[0] - quad.half_extent[0] >= plan.bounds.x - 0.001);
            assert!(quad.center[1] - quad.half_extent[1] >= plan.bounds.y - 0.001);
            assert!(
                quad.center[0] + quad.half_extent[0] <= plan.bounds.x + plan.bounds.width + 0.001
            );
            assert!(
                quad.center[1] + quad.half_extent[1] <= plan.bounds.y + plan.bounds.height + 0.001
            );
        }
    }

    #[test]
    fn generic_titles_reach_gpu_and_expand_the_camera_above_content() {
        for source in [
            "---\ntitle: Release η\n---\nflowchart LR\nA --> B\n",
            "sequenceDiagram\ntitle Release η\nAlice->>Bob: Hello\n",
            "stateDiagram-v2\ntitle Release η\nIdle --> Ready\n",
            "C4Context\ntitle Release η\nPerson(user, \"Customer\")\n",
        ] {
            let ir = fm_parser::parse(source).ir;
            assert_eq!(ir.meta.title.as_deref(), Some("Release η"));
            let config = CanvasRenderConfig {
                font_size: 48.0,
                font_family: "monospace".to_owned(),
                label_color: "#123456".to_owned(),
                ..Default::default()
            };
            let layout = fm_layout::layout_diagram(&ir);
            let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
            assert_eq!(rows(&plan, GpuTextSource::DiagramTitle), ["Release η"]);
            let run = plan
                .text_runs
                .iter()
                .find(|run| run.source == GpuTextSource::DiagramTitle)
                .unwrap();
            assert_eq!(
                plan.glyph_atlas.faces[run.face_index as usize].family,
                "monospace"
            );
            for quad in &plan.text_quads
                [run.first_quad as usize..(run.first_quad + run.quad_count) as usize]
            {
                assert_eq!(quad.half_extent[1], 24.0);
                assert_eq!(
                    quad.color,
                    fm_render_canvas::parse_paint_rgba("#123456").unwrap()
                );
                assert!(quad.center[1] + quad.half_extent[1] < layout.bounds.y);
            }
            assert!(plan.bounds.y < layout.bounds.y);
            assert_text_fits(&plan);
            let scene = fm_layout::build_render_scene(&ir, &layout);
            let scene_plan =
                GpuRenderPlan::from_layout_and_scene_with_config(&ir, &layout, &scene, &config);
            assert_eq!(
                rows(&scene_plan, GpuTextSource::DiagramTitle),
                ["Release η"]
            );
            assert_eq!(scene_plan.bounds, plan.bounds);
            let mut canvas = MockCanvas2dContext::new(1200.0, 900.0);
            render_to_canvas_with_layout(&ir, &layout, &mut canvas, &config);
            assert!(canvas.operations().iter().any(|op| {
                matches!(op, DrawOperation::FillText(text, _, _) if text == "Release η")
            }));
        }
    }

    #[test]
    fn title_family_rules_and_specialized_title_ownership_are_preserved() {
        for source in [
            "flowchart LR\ntitle Hidden\nA --> B\n",
            "---\ntitle: Hidden\n---\nC4Context\nPerson(user, \"Customer\")\n",
        ] {
            let ir = fm_parser::parse(source).ir;
            assert_eq!(ir.meta.title.as_deref(), Some("Hidden"));
            let layout = fm_layout::layout_diagram(&ir);
            let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
            assert!(rows(&plan, GpuTextSource::DiagramTitle).is_empty());
        }
        for source in [
            "pie title Exactly once\n\"A\": 1\n",
            "radar-beta\ntitle Exactly once\naxis a[\"A\"], b[\"B\"], c[\"C\"]\ncurve current{1,2,3}\n",
            "treemap\ntitle Exactly once\n\"Root\"\n  \"A\": 1\n",
            "journey\ntitle Exactly once\nsection Morning\nWake: 5: Me\n",
            "xychart-beta\ntitle Exactly once\nx-axis [A, B]\nbar [1,2]\n",
            "quadrantChart\ntitle Exactly once\nA: [0.3, 0.6]\n",
        ] {
            let ir = fm_parser::parse(source).ir;
            let layout = fm_layout::layout_diagram(&ir);
            let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
            let titles: Vec<_> = treemap_gpu_rows(&plan)
                .into_iter()
                .filter(|(kind, _, _)| {
                    matches!(
                        kind,
                        GpuTextSource::DiagramTitle | GpuTextSource::JourneyTitle
                    )
                })
                .map(|(_, _, text)| text)
                .collect();
            assert_eq!(titles, ["Exactly once"], "{source}");
        }
    }

    #[test]
    fn c4_gpu_legend_keeps_roles_boundaries_fonts_and_themed_panel() {
        let source = "C4Context\nSHOW_LEGEND()\nPerson(person, \"Customer\")\nSystem(store, \"Store\")\nContainer(web, \"Web\", \"Rust\")\nComponent(core, \"Core\", \"Rust\")\nContainerDb(db, \"Data\", \"SQL\")\nContainerQueue(queue, \"Jobs\", \"AMQP\")\nSystem_Ext(bank, \"Bank\")\nSystem_Boundary(payments, \"Payments\") {\nSystem(api, \"API\")\n}\n";
        let ir = fm_parser::parse(source).ir;
        let expected = [
            "C4 Legend",
            "◉ Person",
            "▭ System",
            "▣ Container",
            "◫ Component",
            "◌ Database",
            "▱ Queue",
            "╌ External",
            "⬚ Boundary",
        ];
        assert_eq!(ir.clusters[0].c4_boundary_type.as_deref(), Some("SYSTEM"));
        for font_size in [14.0, 64.0] {
            let config = CanvasRenderConfig {
                font_size,
                font_family: "monospace".to_owned(),
                node_fill: "#0f172a".to_owned(),
                label_color: "#f8fafc".to_owned(),
                cluster_stroke: "#64748b".to_owned(),
                ..Default::default()
            };
            let layout = fm_layout::layout_diagram(&ir);
            let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
            assert_eq!(rows(&plan, GpuTextSource::C4Legend), expected);
            let panel = plan
                .node_instances
                .iter()
                .find(|node| {
                    node.node_index == u32::MAX
                        && node.center[1] - node.half_extent[1]
                            > layout.bounds.y + layout.bounds.height
                })
                .expect("the complete legend needs a background below the diagram");
            let mut fill = fm_render_canvas::parse_paint_rgba(&config.node_fill).unwrap();
            fill[3] *= 0.96;
            assert_eq!(panel.fill, fill);
            assert_eq!(
                panel.stroke,
                fm_render_canvas::parse_paint_rgba(&config.cluster_stroke).unwrap()
            );
            for run in plan
                .text_runs
                .iter()
                .filter(|run| run.source == GpuTextSource::C4Legend)
            {
                let face = &plan.glyph_atlas.faces[run.face_index as usize];
                assert_eq!(face.family, "monospace");
                assert_eq!(
                    face.weight,
                    if run.node_index == 0 { "600" } else { "normal" }
                );
                for quad in &plan.text_quads
                    [run.first_quad as usize..(run.first_quad + run.quad_count) as usize]
                {
                    assert!(
                        quad.center[0] - quad.half_extent[0]
                            >= panel.center[0] - panel.half_extent[0] - 0.001
                    );
                    assert!(
                        quad.center[0] + quad.half_extent[0]
                            <= panel.center[0] + panel.half_extent[0] + 0.001
                    );
                    assert!(
                        quad.center[1] - quad.half_extent[1]
                            >= panel.center[1] - panel.half_extent[1] - 0.001
                    );
                    assert!(
                        quad.center[1] + quad.half_extent[1]
                            <= panel.center[1] + panel.half_extent[1] + 0.001
                    );
                }
            }
            assert_text_fits(&plan);
            let mut canvas = MockCanvas2dContext::new(1400.0, 1000.0);
            render_to_canvas_with_layout(&ir, &layout, &mut canvas, &config);
            for expected in expected {
                assert!(
                    canvas.operations().iter().any(|operation| {
                        matches!(operation, DrawOperation::FillText(text, _, _) if text == expected)
                    }),
                    "missing Canvas legend row {expected}"
                );
            }
        }

        let hidden = fm_parser::parse(&source.replace("SHOW_LEGEND()", "HIDE_LEGEND()")).ir;
        let layout = fm_layout::layout_diagram(&hidden);
        let plan = GpuRenderPlan::from_layout(&hidden, &layout, 1.5);
        assert!(rows(&plan, GpuTextSource::C4Legend).is_empty());
        assert!(
            plan.node_instances
                .iter()
                .all(|node| node.node_index != u32::MAX)
        );
    }

    #[test]
    fn info_gpu_banner_matches_canvas_and_submits_real_text() {
        let ir = fm_parser::parse("info\n").ir;
        let layout = fm_layout::layout_diagram(&ir);
        let config = CanvasRenderConfig {
            font_family: "monospace".to_owned(),
            label_color: "#228844".to_owned(),
            ..Default::default()
        };
        let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
        let expected = concat!("v", env!("CARGO_PKG_VERSION"));
        assert_eq!(rows(&plan, GpuTextSource::InfoVersion), [expected]);
        assert!(plan.node_instances.is_empty());
        assert!(plan.edge_segments.is_empty());
        assert!(
            plan.text_quads
                .iter()
                .all(|quad| quad.center[1] == 40.0 && quad.half_extent[1] == 16.0)
        );
        let batches = draw_batches(&plan);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].family, PrimitiveFamily::Text);
        assert_eq!(batches[0].instance_count as usize, expected.chars().count());
        assert_text_fits(&plan);
        let mut canvas = MockCanvas2dContext::new(800.0, 600.0);
        render_to_canvas_with_layout(&ir, &layout, &mut canvas, &config);
        assert!(canvas.operations().iter().any(|operation| {
            matches!(operation, DrawOperation::FillText(text, _, _) if text == expected)
        }));
    }

    #[test]
    fn gantt_gpu_today_marker_uses_the_calendar_axis_and_under_bar_pass() {
        let source = "gantt\ntitle Roadmap\ndateFormat YYYY-MM-DD\nsection Core\nDesign :a1, 2026-01-01, 3d\nBuild :a2, after a1, 4d\n";
        let ir = fm_parser::parse(source).ir;
        let layout = fm_layout::layout_diagram(&ir);
        let axis = layout.extensions.gantt_day_axis.unwrap();
        let baseline = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
        for date in ["2026-01-02", "2026-01-03"] {
            let config = CanvasRenderConfig {
                gantt_today: Some(date.to_owned()),
                auto_fit: false,
                padding: 0.0,
                ..Default::default()
            };
            let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
            assert_eq!(
                plan.axis_tick_segments.len(),
                baseline.axis_tick_segments.len() + 1
            );
            let marker = plan.axis_tick_segments.last().unwrap();
            let x = axis
                .x_for_day(fm_layout::parse_iso_day_number(date).unwrap())
                .unwrap();
            assert_eq!(marker.from, [x, layout.bounds.y + 12.0]);
            assert_eq!(marker.to, [x, layout.bounds.y + layout.bounds.height]);
            assert_eq!(marker.color, [1.0, 0.0, 0.0, 1.0]);
            assert_eq!(marker.width, 2.0);
            assert_eq!(marker.dash, [0.0; 2]);
            assert_eq!(marker.edge_index, u32::MAX);
            let batches = draw_batches(&plan);
            let axis_pass = batches
                .iter()
                .position(|batch| batch.family == PrimitiveFamily::AxisTick)
                .unwrap();
            // Task bars are path-filled (rounded bars, milestones, progress overlays), so they
            // draw in the NodeFill pass rather than as SDF node boxes.
            let task_pass = batches
                .iter()
                .position(|batch| batch.family == PrimitiveFamily::NodeFill)
                .expect("Gantt task bars must reach the GPU plan as filled paths");
            assert!(
                !batches
                    .iter()
                    .any(|batch| batch.family == PrimitiveFamily::Node),
                "Gantt task bars must not fall back to SDF node boxes"
            );
            assert!(axis_pass < task_pass);
            assert_eq!(
                batches[axis_pass].instance_count as usize,
                plan.axis_tick_segments.len()
            );

            let mut canvas = MockCanvas2dContext::new(1400.0, 800.0);
            let result = render_to_canvas_with_layout(&ir, &layout, &mut canvas, &config);
            assert!(canvas.operations().windows(2).any(|pair| {
                match (&pair[0], &pair[1]) {
                    (DrawOperation::MoveTo(x0, y0), DrawOperation::LineTo(x1, y1)) => {
                        (*x0 - f64::from(marker.from[0]) - result.viewport.offset_x).abs() < 0.001
                            && (*x1 - f64::from(marker.to[0]) - result.viewport.offset_x).abs()
                                < 0.001
                            && (*y0 - f64::from(marker.from[1]) - result.viewport.offset_y).abs()
                                < 0.001
                            && (*y1 - f64::from(marker.to[1]) - result.viewport.offset_y).abs()
                                < 0.001
                    }
                    _ => false,
                }
            }));
        }
        for date in ["invalid", "2026-02-30", "2025-12-31", "2027-01-01"] {
            let config = CanvasRenderConfig {
                gantt_today: Some(date.to_owned()),
                ..Default::default()
            };
            let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
            assert_eq!(
                plan.axis_tick_segments, baseline.axis_tick_segments,
                "{date}"
            );
        }
        let suppressed =
            fm_parser::parse(&source.replace("gantt\n", "gantt\ntodayMarker off\n")).ir;
        let layout = fm_layout::layout_diagram(&suppressed);
        let config = CanvasRenderConfig {
            gantt_today: Some("2026-01-03".to_owned()),
            ..Default::default()
        };
        let plan = GpuRenderPlan::from_layout_with_config(&suppressed, &layout, &config);
        assert_eq!(
            plan.axis_tick_segments,
            GpuRenderPlan::from_layout(&suppressed, &layout, 1.5).axis_tick_segments
        );
    }
}

mod xychart {
    use super::*;
    use fm_core::IrXySeriesKind;
    use fm_render_canvas::{DrawOperation, GpuNodeShape, GpuTextSource, GpuTriangleInstance};

    fn run_text(plan: &GpuRenderPlan, run: &fm_render_canvas::GpuTextRun) -> String {
        plan.text_quads[run.first_quad as usize..(run.first_quad + run.quad_count) as usize]
            .iter()
            .map(|quad| {
                plan.glyph_atlas
                    .cells
                    .iter()
                    .find(|cell| cell.uv_min == quad.uv_min && cell.uv_max == quad.uv_max)
                    .expect("every drawn glyph must have an atlas cell")
                    .glyph
            })
            .collect()
    }

    fn triangle_area(triangle: &GpuTriangleInstance) -> f32 {
        let (a, b, c) = (triangle.a, triangle.b, triangle.c);
        ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
    }

    fn three_series_source(orientation: &str) -> String {
        format!(
            "xychart-beta{orientation}\n title Revenue & Costs\n x-axis \"Month\" [Jan, Feb, Mar]\n y-axis \"Net\" -20 --> 30\n bar \"Sales\" [20, -10, 30]\n line \"Trend\" [10, 0, -10]\n area \"Cash\" [-10, 20, -5]\n"
        )
    }

    fn configured_palette() -> CanvasRenderConfig {
        CanvasRenderConfig {
            label_color: "rgba(12,34,56,0.6)".to_owned(),
            edge_stroke: "rgba(78,90,123,0.7)".to_owned(),
            node_fill: "rgba(145,167,189,0.8)".to_owned(),
            accents: [
                "rgba(200,10,20,0.5)",
                "rgba(30,210,40,0.4)",
                "rgba(50,60,220,0.3)",
                "#441122",
                "#552233",
                "#663344",
                "#774455",
                "#885566",
            ]
            .map(String::from),
            ..Default::default()
        }
    }

    fn assert_three_series_palette(plan: &GpuRenderPlan, expected: &CanvasRenderConfig) {
        let rgba = |paint: &str| fm_render_canvas::parse_paint_rgba(paint).unwrap();
        let label = rgba(&expected.label_color);
        let edge = rgba(&expected.edge_stroke);
        let outline = rgba(&expected.node_fill);
        let accents = expected.accents.each_ref().map(|paint| rgba(paint));
        assert!(
            !plan.text_quads.is_empty(),
            "the chart must retain its labels"
        );
        assert!(plan.text_quads.iter().all(|quad| quad.color == label));
        let mut main_axes = 0;
        let mut ticks = 0;
        for segment in &plan.xy_axis_segments {
            if segment.dash == [4.0, 4.0] {
                assert_eq!(
                    segment.color,
                    [148.0 / 255.0, 163.0 / 255.0, 184.0 / 255.0, 0.35]
                );
            } else if segment.width == 1.5 {
                main_axes += 1;
                assert_eq!(segment.color, edge);
            } else {
                ticks += 1;
                assert_eq!(segment.color, label);
            }
        }
        assert_eq!(main_axes, 2);
        assert!(ticks > 3, "both value and category ticks must be tested");
        assert_eq!(plan.xy_bar_instances.len(), 3);
        let mut bar_fill = accents[0];
        bar_fill[3] *= 0.78;
        for bar in &plan.xy_bar_instances {
            assert_eq!(bar.fill, bar_fill);
            assert_eq!(bar.stroke, accents[0]);
        }
        assert_eq!(plan.node_instances.len(), 6);
        for (index, point) in plan.node_instances.iter().enumerate() {
            assert_eq!(point.fill, accents[if index < 3 { 1 } else { 2 }]);
            assert_eq!(point.stroke, outline);
        }
        assert_eq!(plan.edge_segments.len(), 4);
        for (index, segment) in plan.edge_segments.iter().enumerate() {
            assert_eq!(segment.color, accents[if index < 2 { 1 } else { 2 }]);
        }
        assert!(!plan.xy_area_triangles.is_empty());
        let mut area_fill = accents[2];
        area_fill[3] *= 0.16;
        assert!(
            plan.xy_area_triangles
                .iter()
                .all(|triangle| triangle.color == area_fill)
        );
        assert!(
            plan.arrowheads.is_empty(),
            "series are not directed graph edges"
        );
    }

    #[test]
    fn configured_chart_paints_and_alpha_reach_every_gpu_channel() {
        let config = configured_palette();
        for orientation in ["", " horizontal"] {
            let ir = fm_parser::parse(&three_series_source(orientation)).ir;
            let layout = fm_layout::layout_diagram(&ir);
            let scene = fm_layout::build_render_scene(&ir, &layout);
            let plan =
                GpuRenderPlan::from_layout_and_scene_with_config(&ir, &layout, &scene, &config);
            assert_three_series_palette(&plan, &config);
            assert_eq!(
                plan,
                GpuRenderPlan::from_layout_with_config(&ir, &layout, &config)
            );

            // Compare real text and bar paint operations to the planned colors, including
            // Canvas2D's global alpha rather than just checking that a style was assigned.
            let mut canvas = MockCanvas2dContext::new(1200.0, 900.0);
            fm_render_canvas::render_to_canvas_with_layout(&ir, &layout, &mut canvas, &config);
            let mut fill = [0.0, 0.0, 0.0, 1.0];
            let mut alpha = 1.0;
            let mut stack = Vec::new();
            let mut rect_fills = Vec::new();
            let mut text_fills = Vec::new();
            for operation in canvas.operations() {
                match operation {
                    DrawOperation::Save => stack.push((fill, alpha)),
                    DrawOperation::Restore => (fill, alpha) = stack.pop().unwrap(),
                    DrawOperation::SetFillStyle(paint) => {
                        fill = fm_render_canvas::parse_paint_rgba(paint).unwrap();
                    }
                    DrawOperation::SetGlobalAlpha(value) => alpha = *value as f32,
                    DrawOperation::FillRect(..) | DrawOperation::FillText(..) => {
                        let mut effective = fill;
                        effective[3] *= alpha;
                        if matches!(operation, DrawOperation::FillRect(..)) {
                            rect_fills.push(effective);
                        } else {
                            text_fills.push(effective);
                        }
                    }
                    _ => {}
                }
            }
            assert_eq!(
                rect_fills.len(),
                4,
                "plot background followed by three bars"
            );
            assert_eq!(rect_fills[0], plan.xy_plot_instances[0].fill);
            assert_eq!(
                rect_fills[1..],
                plan.xy_bar_instances
                    .iter()
                    .map(|bar| bar.fill)
                    .collect::<Vec<_>>()
            );
            assert_eq!(text_fills.len(), plan.text_runs.len());
            assert!(
                text_fills
                    .iter()
                    .all(|fill| *fill == plan.text_quads[0].color)
            );
        }
    }

    #[test]
    fn source_chart_palette_overrides_config_without_leaking_into_the_next_plan() {
        let config = configured_palette();
        let directive = "%%{init: {'themeVariables': {'primaryTextColor': '#aabbcc80', 'textColor': '#ff00ff', 'lineColor': '#11223366', 'primaryColor': '#44556640', 'pie1': '#cc112280', 'pie2': '#33dd4440', 'pie3': '#5566ee20'}}}%%\n";
        let mut expected = config.clone();
        expected.label_color = "#aabbcc80".to_owned();
        expected.edge_stroke = "#11223366".to_owned();
        expected.node_fill = "#44556640".to_owned();
        expected.accents[0] = "#cc112280".to_owned();
        expected.accents[1] = "#33dd4440".to_owned();
        expected.accents[2] = "#5566ee20".to_owned();
        for orientation in ["", " horizontal"] {
            let source = three_series_source(orientation);
            let ir = fm_parser::parse(&format!("{directive}{source}")).ir;
            assert_eq!(ir.meta.theme_overrides.theme_variables.len(), 7);
            let layout = fm_layout::layout_diagram(&ir);
            let plan = GpuRenderPlan::from_layout_with_config(&ir, &layout, &config);
            assert_three_series_palette(&plan, &expected);

            let plain_ir = fm_parser::parse(&source).ir;
            let plain_layout = fm_layout::layout_diagram(&plain_ir);
            let plain_plan =
                GpuRenderPlan::from_layout_with_config(&plain_ir, &plain_layout, &config);
            assert_three_series_palette(&plain_plan, &config);
        }
        assert_eq!(config.label_color, "rgba(12,34,56,0.6)");
        assert_eq!(config.accents[0], "rgba(200,10,20,0.5)");
    }

    #[test]
    fn transparent_chart_paints_stay_disabled_after_unsafe_theme_overrides() {
        let config = CanvasRenderConfig {
            label_color: "transparent".to_owned(),
            edge_stroke: "transparent".to_owned(),
            node_fill: "transparent".to_owned(),
            accents: std::array::from_fn(|_| "transparent".to_owned()),
            ..Default::default()
        };
        let directive = "%%{init: {'themeVariables': {'primaryTextColor': 'url(#text)', 'lineColor': 'url(#axis)', 'primaryColor': 'url(#outline)', 'pie1': 'url(#bar)', 'pie2': 'url(#line)', 'pie3': 'url(#area)'}}}%%\n";
        let ir = fm_parser::parse(&format!("{directive}{}", three_series_source(""))).ir;
        let layout = fm_layout::layout_diagram(&ir);
        let scene = fm_layout::build_render_scene(&ir, &layout);
        let plan = GpuRenderPlan::from_layout_and_scene_with_config(&ir, &layout, &scene, &config);
        assert_three_series_palette(&plan, &config);
    }

    #[test]
    fn both_orientations_preserve_canvas_axes_text_and_measured_marks() {
        for orientation in ["", " horizontal"] {
            let source = format!(
                "xychart-beta{orientation}\n title Revenue & Costs\n x-axis \"Month\" [Jan, Feb, Mar]\n y-axis \"Net\" -20 --> 30\n bar \"Sales\" [20, -10, 30]\n line \"Trend\" [10, 0, -10]\n area \"Cash\" [-10, 20, -5]\n"
            );
            let ir = fm_parser::parse(&source).ir;
            let meta = ir.xy_chart_meta.as_ref().expect("XY metadata");
            assert_eq!(meta.series.len(), 3);
            let layout = fm_layout::layout_diagram(&ir);
            let scene = fm_layout::build_render_scene(&ir, &layout);
            let plan = GpuRenderPlan::from_layout_and_scene(&ir, &layout, &scene, 1.5);
            assert_eq!(plan, GpuRenderPlan::from_layout(&ir, &layout, 1.5));
            assert!(
                plan.arrowheads.is_empty(),
                "chart lines must never acquire arrows"
            );
            assert_eq!(plan.node_instances.len(), 6);
            assert_eq!(plan.xy_bar_instances.len(), 3);
            assert_eq!(plan.edge_segments.len(), 4);
            assert_eq!(plan.xy_plot_instances.len(), 1);
            assert!(!plan.xy_area_triangles.is_empty());

            let mut canvas = MockCanvas2dContext::new(1200.0, 900.0);
            let config = CanvasRenderConfig::default();
            assert_three_series_palette(&plan, &config);
            fm_render_canvas::render_to_canvas_with_layout(&ir, &layout, &mut canvas, &config);
            let mut drawn: Vec<_> = canvas
                .operations()
                .iter()
                .filter_map(|op| match op {
                    DrawOperation::FillText(text, _, _) => Some(text.clone()),
                    _ => None,
                })
                .collect();
            let mut planned: Vec<_> = plan
                .text_runs
                .iter()
                .map(|run| run_text(&plan, run))
                .collect();
            drawn.sort();
            planned.sort();
            assert_eq!(
                planned, drawn,
                "XY axes/title must match the actual Canvas text stream"
            );
            assert!(
                plan.text_runs
                    .iter()
                    .all(|run| !matches!(run.source, GpuTextSource::Node | GpuTextSource::Edge)),
                "generic graph labels do not belong to data marks"
            );

            for (series_index, series) in meta.series.iter().enumerate() {
                let color =
                    fm_render_canvas::parse_paint_rgba(&config.accents[series_index]).unwrap();
                for id in &series.nodes {
                    let measured = layout
                        .nodes
                        .iter()
                        .find(|node| node.node_index == id.0)
                        .unwrap();
                    let mark = plan
                        .node_instances
                        .iter()
                        .chain(plan.xy_bar_instances.iter())
                        .find(|mark| mark.node_index as usize == id.0)
                        .unwrap();
                    let center = measured.bounds.center();
                    assert_eq!(mark.center, [center.x, center.y]);
                    assert_eq!(
                        mark.half_extent,
                        [measured.bounds.width / 2.0, measured.bounds.height / 2.0]
                    );
                    assert_eq!(mark.fill[..3], color[..3]);
                    assert_eq!(
                        mark.shape,
                        if series.kind == IrXySeriesKind::Bar {
                            GpuNodeShape::Rect
                        } else {
                            GpuNodeShape::Circle
                        } as u32
                    );
                }
            }

            let ticks = fm_layout::xychart_y_ticks(-20.0, 30.0);
            let plot = fm_layout::xychart_plot_bounds(meta, layout.bounds);
            let grid: Vec<_> = plan
                .xy_axis_segments
                .iter()
                .filter(|segment| segment.dash == [4.0, 4.0])
                .collect();
            assert_eq!(grid.len(), ticks.len());
            for (segment, value) in grid.into_iter().zip(ticks) {
                if meta.horizontal {
                    let x = fm_layout::xychart_value_to_x(value, -20.0, 30.0, plot);
                    assert_eq!(segment.from, [x, plot.y]);
                    assert_eq!(segment.to, [x, plot.y + plot.height]);
                } else {
                    let y = fm_layout::xychart_value_to_y(value, -20.0, 30.0, plot);
                    assert_eq!(segment.from, [plot.x, y]);
                    assert_eq!(segment.to, [plot.x + plot.width, y]);
                }
            }
        }
    }

    #[test]
    fn area_zero_crossings_have_the_correct_non_overlapping_extent() {
        for orientation in ["", " horizontal"] {
            let ir = fm_parser::parse(&format!("xychart-beta{orientation}\n x-axis [A, B, C]\n y-axis -10 --> 10\n area [-8, 8, -8]\n")).ir;
            let layout = fm_layout::layout_diagram(&ir);
            let meta = ir.xy_chart_meta.as_ref().unwrap();
            let plot = fm_layout::xychart_plot_bounds(meta, layout.bounds);
            let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
            let value_axis = usize::from(!meta.horizontal);
            let category_axis = 1 - value_axis;
            let baseline = if meta.horizontal {
                plot.x + plot.width / 2.0
            } else {
                plot.y + plot.height / 2.0
            };
            assert_eq!(
                plan.xy_area_triangles.len(),
                4,
                "each sign-changing strip has two separate lobes"
            );
            let first = layout.nodes[0].bounds.center();
            let second = layout.nodes[1].bounds.center();
            let first = [first.x, first.y];
            let second = [second.x, second.y];
            // Two equal-magnitude crossings: the absolute integral is half the rectangle
            // for each strip, so two strips sum to one rectangle.
            let expected_area = (second[category_axis] - first[category_axis]).abs()
                * (first[value_axis] - baseline).abs();
            let area: f32 = plan.xy_area_triangles.iter().map(triangle_area).sum();
            assert!(
                (area - expected_area).abs() < 0.05,
                "overlapping or missing area: {area} versus {expected_area}"
            );
            for triangle in &plan.xy_area_triangles {
                let deltas =
                    [triangle.a, triangle.b, triangle.c].map(|point| point[value_axis] - baseline);
                assert!(
                    deltas.iter().all(|delta| *delta >= -0.001)
                        || deltas.iter().all(|delta| *delta <= 0.001),
                    "a zero-crossing triangle leaked into the opposite lobe"
                );
                assert!(triangle_area(triangle) > 0.0);
            }
        }
    }

    #[test]
    fn empty_singleton_numeric_and_zero_charts_are_finite_and_deterministic() {
        for source in [
            "xychart-beta\n title Empty\n",
            "xychart-beta\n x-axis [only]\n line [4]\n",
            "xychart-beta horizontal\n x-axis 10 --> 30\n y-axis 0 --> 0\n area [0, 0, 0]\n",
            "xychart-beta\n x-axis [A, B]\n y-axis 10 --> 20\n area [12, 18]\n",
        ] {
            let ir = fm_parser::parse(source).ir;
            let layout = fm_layout::layout_diagram(&ir);
            let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
            assert_eq!(plan, GpuRenderPlan::from_layout(&ir, &layout, 1.5));
            assert!(plan.node_instances.iter().all(|mark| {
                mark.center
                    .iter()
                    .chain(&mark.half_extent)
                    .all(|value| value.is_finite())
            }));
            assert!(
                plan.xy_axis_segments
                    .iter()
                    .chain(&plan.edge_segments)
                    .all(|segment| segment
                        .from
                        .iter()
                        .chain(&segment.to)
                        .all(|value| value.is_finite()))
            );
            assert!(plan.xy_area_triangles.iter().all(
                |triangle| triangle_area(triangle).is_finite() && triangle_area(triangle) > 0.0
            ));
        }
    }

    #[test]
    fn long_categories_and_titles_extend_gpu_camera_bounds() {
        // Horizontal layout already reserves a measured category-label margin.
        // An overflowing title exercises the GPU's additional camera expansion.
        let title = "Very long chart title with useful context that must remain visible ".repeat(8);
        let ir = fm_parser::parse(&format!("xychart-beta horizontal\n title {title}\n x-axis [A deliberately long category label well beyond the plot margin, B]\n bar [1, 2]\n")).ir;
        let layout = fm_layout::layout_diagram(&ir);
        let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
        let title_run = plan
            .text_runs
            .iter()
            .find(|run| run.source == GpuTextSource::DiagramTitle)
            .expect("the title must contribute actual glyphs");
        assert_eq!(run_text(&plan, title_run), title.trim());
        let mut extents = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for quad in &plan.text_quads {
            extents[0] = extents[0].min(quad.center[0] - quad.half_extent[0]);
            extents[1] = extents[1].min(quad.center[1] - quad.half_extent[1]);
            extents[2] = extents[2].max(quad.center[0] + quad.half_extent[0]);
            extents[3] = extents[3].max(quad.center[1] + quad.half_extent[1]);
        }
        assert!(
            extents[0] < layout.bounds.x && extents[2] > layout.bounds.x + layout.bounds.width,
            "fixture title must actually overflow both layout edges: {extents:?} versus {:?}",
            layout.bounds
        );
        let expected = [
            layout.bounds.x.min(extents[0]),
            layout.bounds.y.min(extents[1]),
            (layout.bounds.x + layout.bounds.width).max(extents[2]),
            (layout.bounds.y + layout.bounds.height).max(extents[3]),
        ];
        let actual = [
            plan.bounds.x,
            plan.bounds.y,
            plan.bounds.x + plan.bounds.width,
            plan.bounds.y + plan.bounds.height,
        ];
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.001,
                "camera edge {actual} must match measured extent {expected}"
            );
        }
        for quad in &plan.text_quads {
            assert!(quad.center[0] - quad.half_extent[0] >= plan.bounds.x - 0.001);
            assert!(quad.center[1] - quad.half_extent[1] >= plan.bounds.y - 0.001);
            assert!(
                quad.center[0] + quad.half_extent[0] <= plan.bounds.x + plan.bounds.width + 0.001
            );
            assert!(
                quad.center[1] + quad.half_extent[1] <= plan.bounds.y + plan.bounds.height + 0.001
            );
        }
    }
}
