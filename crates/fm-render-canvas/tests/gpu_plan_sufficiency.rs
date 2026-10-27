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
