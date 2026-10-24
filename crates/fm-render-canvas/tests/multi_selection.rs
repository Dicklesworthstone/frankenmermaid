//! Multi-selection uses the rendered IR/layout, not click metadata or parser-array positions.

use fm_core::MermaidDiagramIr;
use fm_layout::{DiagramLayout, LayoutRect, layout_diagram};
use fm_render_canvas::selection::{MAX_LASSO_POINTS, SelectionRegion};
use fm_render_canvas::{
    CanvasRenderConfig, CanvasSelectionStyle, DrawOperation, MockCanvas2dContext,
    NodeSelectionSet, SelectionBox, SelectionContainment, SelectionMode, Viewport,
    render_to_canvas_with_layout, render_to_canvas_with_multi_selection,
};

fn fixture() -> (MermaidDiagramIr, DiagramLayout) {
    let ir = fm_parser::parse("flowchart LR\n Z[Zulu]\n A[Alpha]\n M[Mike]\n").ir;
    assert_eq!(ir.nodes.len(), 3);
    let mut layout = layout_diagram(&ir);
    for placed in &mut layout.nodes {
        let x = match ir.nodes[placed.node_index].id.as_str() {
            "Z" => -40.0,
            "A" => 0.0,
            "M" => 40.0,
            _ => unreachable!(),
        };
        placed.bounds = LayoutRect {
            x,
            y: -10.0,
            width: 20.0,
            height: 20.0,
        };
    }
    layout.bounds = LayoutRect {
        x: -50.0,
        y: -20.0,
        width: 120.0,
        height: 40.0,
    };
    (ir, layout)
}

fn ids(selection: &NodeSelectionSet) -> Vec<&str> {
    selection.selected_node_ids().collect()
}

fn all() -> SelectionBox {
    SelectionBox {
        start: (-50.0, -20.0),
        end: (70.0, 20.0),
    }
}

#[test]
fn marquee_selects_unlinked_nodes_and_sorts_author_ids() {
    let (ir, mut layout) = fixture();
    let mut selected = NodeSelectionSet::default();
    assert!(selected.select_box(
        &ir,
        &layout,
        all(),
        SelectionMode::Replace,
        SelectionContainment::Contains,
    ));
    assert_eq!(ids(&selected), ["A", "M", "Z"]);
    assert_eq!(selected.len(), 3);
    layout.nodes.reverse();
    assert!(!selected.select_box(
        &ir,
        &layout,
        all(),
        SelectionMode::Replace,
        SelectionContainment::Contains,
    ));
    assert_eq!(ids(&selected), ["A", "M", "Z"]);
    let reverse = SelectionBox {
        start: all().end,
        end: all().start,
    };
    assert!(!selected.select_box(
        &ir,
        &layout,
        reverse,
        SelectionMode::Replace,
        SelectionContainment::Contains,
    ));
}

#[test]
fn partial_overlap_and_full_containment_are_distinct() {
    let (ir, layout) = fixture();
    let region = SelectionBox {
        start: (-5.0, -15.0),
        end: (5.0, 15.0),
    };
    let mut selected = NodeSelectionSet::default();
    assert!(selected.select_box(
        &ir,
        &layout,
        region,
        SelectionMode::Replace,
        SelectionContainment::Intersects,
    ));
    assert_eq!(ids(&selected), ["A"]);
    assert!(selected.select_box(
        &ir,
        &layout,
        region,
        SelectionMode::Replace,
        SelectionContainment::Contains,
    ));
    assert!(selected.is_empty());
}

#[test]
fn add_remove_and_toggle_apply_to_every_match_once() {
    let (ir, mut layout) = fixture();
    let duplicate = layout.nodes[0].clone();
    layout.nodes.push(duplicate);
    let mut selected = NodeSelectionSet::default();
    assert!(selected.select_node(&ir, &layout, "M", SelectionMode::Add));
    assert!(!selected.select_node(&ir, &layout, "M", SelectionMode::Add));
    assert!(selected.select_box(
        &ir,
        &layout,
        all(),
        SelectionMode::Toggle,
        SelectionContainment::Contains,
    ));
    assert_eq!(ids(&selected), ["A", "Z"]);
    assert!(selected.select_box(
        &ir,
        &layout,
        all(),
        SelectionMode::Add,
        SelectionContainment::Contains,
    ));
    assert_eq!(ids(&selected), ["A", "M", "Z"]);
    assert!(selected.select_box(
        &ir,
        &layout,
        all(),
        SelectionMode::Remove,
        SelectionContainment::Contains,
    ));
    assert!(selected.is_empty());
    assert!(!selected.clear());
}

#[test]
fn clicks_choose_topmost_box_but_marquee_includes_overlaps() {
    let (ir, mut layout) = fixture();
    for placed in &mut layout.nodes {
        placed.bounds = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        };
    }
    let top = ir.nodes[layout.nodes.last().unwrap().node_index].id.as_str();
    let mut selected = NodeSelectionSet::default();
    assert!(selected.select_at(&ir, &layout, (10.0, 10.0), SelectionMode::Replace));
    assert_eq!(ids(&selected), [top]);
    assert!(selected.select_at(&ir, &layout, (20.0, 10.0), SelectionMode::Replace));
    assert!(selected.is_empty());
    selected.select_box(
        &ir,
        &layout,
        all(),
        SelectionMode::Replace,
        SelectionContainment::Contains,
    );
    assert_eq!(selected.len(), 3);
}

#[test]
fn bad_input_never_clears_selection_or_selects_invalid_geometry() {
    let (ir, mut layout) = fixture();
    let mut selected = NodeSelectionSet::default();
    selected.select_node(&ir, &layout, "A", SelectionMode::Replace);
    assert!(!selected.select_node(&ir, &layout, "missing", SelectionMode::Replace));
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let region = SelectionBox {
            start: (invalid, 0.0),
            end: (10.0, 10.0),
        };
        assert!(!selected.select_box(
            &ir,
            &layout,
            region,
            SelectionMode::Replace,
            SelectionContainment::Intersects,
        ));
        assert!(!selected.select_at(&ir, &layout, (invalid, 0.0), SelectionMode::Replace));
    }
    let line = SelectionBox {
        start: (0.0, 0.0),
        end: (0.0, 10.0),
    };
    assert!(!selected.select_box(
        &ir,
        &layout,
        line,
        SelectionMode::Replace,
        SelectionContainment::Intersects,
    ));
    assert_eq!(ids(&selected), ["A"]);
    for placed in &mut layout.nodes {
        match ir.nodes[placed.node_index].id.as_str() {
            "Z" => placed.node_index = usize::MAX,
            "M" => placed.bounds.width = f32::NAN,
            _ => {}
        }
    }
    assert!(!selected.select_box(
        &ir,
        &layout,
        all(),
        SelectionMode::Replace,
        SelectionContainment::Contains,
    ));
    assert_eq!(ids(&selected), ["A"]);
}

#[test]
fn selection_survives_node_reindexing_and_removes_deleted_ids() {
    let (ir, layout) = fixture();
    let mut selected = NodeSelectionSet::default();
    selected.select_box(
        &ir,
        &layout,
        all(),
        SelectionMode::Replace,
        SelectionContainment::Contains,
    );
    let replacement =
        fm_parser::parse("flowchart LR\n M[Moved]\n A[Renamed label]\n B[New]\n").ir;
    let replacement_layout = layout_diagram(&replacement);
    assert!(selected.reconcile(&replacement, &replacement_layout));
    assert_eq!(ids(&selected), ["A", "M"]);
    assert!(!selected.reconcile(&replacement, &replacement_layout));
}

#[test]
fn backing_pixel_marquee_respects_zoom_translation_and_does_not_reapply_dpr() {
    let (ir, layout) = fixture();
    let viewport = Viewport {
        offset_x: 100.0,
        offset_y: 50.0,
        zoom: 2.0,
        device_pixel_ratio: 3.0,
        ..Viewport::default()
    };
    let region = SelectionBox {
        start: (99.0, 29.0),
        end: (141.0, 71.0),
    };
    let mut selected = NodeSelectionSet::default();
    assert!(selected.select_box_canvas(
        &ir,
        &layout,
        &viewport,
        region,
        SelectionMode::Replace,
        SelectionContainment::Contains,
    ));
    assert_eq!(ids(&selected), ["A"]);
    for zoom in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let invalid = Viewport { zoom, ..viewport };
        assert!(!selected.select_box_canvas(
            &ir,
            &layout,
            &invalid,
            region,
            SelectionMode::Replace,
            SelectionContainment::Contains,
        ));
    }
    assert_eq!(ids(&selected), ["A"]);
}

#[test]
fn selection_rings_append_to_the_real_draw_stream_and_keep_metadata() {
    let (mut ir, layout) = fixture();
    let node = ir.nodes.iter_mut().find(|node| node.id.as_str() == "A").unwrap();
    node.interaction_mut().tooltip = Some(String::from("Inspect Alpha"));
    let config = CanvasRenderConfig {
        auto_fit: false,
        padding: 10.0,
        ..CanvasRenderConfig::default()
    };
    let style = CanvasSelectionStyle::default();
    let mut base = MockCanvas2dContext::new(400.0, 200.0);
    let expected = render_to_canvas_with_layout(&ir, &layout, &mut base, &config);
    let mut selected = NodeSelectionSet::default();
    let mut empty = MockCanvas2dContext::new(400.0, 200.0);
    let empty_result = render_to_canvas_with_multi_selection(
        &ir,
        &layout,
        &mut empty,
        &config,
        &mut selected,
        &style,
    );
    assert_eq!(empty.operations(), base.operations());
    assert_eq!(empty_result.draw_calls, expected.draw_calls);
    selected.select_box(
        &ir,
        &layout,
        all(),
        SelectionMode::Replace,
        SelectionContainment::Contains,
    );
    let mut context = MockCanvas2dContext::new(400.0, 200.0);
    let actual = render_to_canvas_with_multi_selection(
        &ir,
        &layout,
        &mut context,
        &config,
        &mut selected,
        &style,
    );
    assert_eq!(
        &context.operations()[..base.operation_count()],
        base.operations()
    );
    assert_eq!(actual.draw_calls, expected.draw_calls + 6);
    assert_eq!(actual.nodes_drawn, expected.nodes_drawn);
    assert_eq!(actual.edges_drawn, expected.edges_drawn);
    assert_eq!(actual.labels_drawn, expected.labels_drawn);
    assert_eq!(actual.hit_regions, expected.hit_regions);
    assert_eq!(actual.viewport, expected.viewport);
    let rings: Vec<_> = context.operations()[base.operation_count()..]
        .iter()
        .filter(|op| matches!(op, DrawOperation::StrokeRect(..)))
        .collect();
    assert_eq!(rings.len(), 6);
    // A starts at canvas (60,20), independently derived from raw bounds, origin and padding.
    assert!(rings.contains(&&DrawOperation::StrokeRect(54.0, 14.0, 32.0, 32.0)));
}

fn polygon_box(left: f64, top: f64, right: f64, bottom: f64) -> [(f64, f64); 4] {
    [(left, top), (right, top), (right, bottom), (left, bottom)]
}

#[test]
fn lasso_accepts_boundary_nodes_closing_points_and_either_winding() {
    let (ir, layout) = fixture();
    let mut points = polygon_box(0.0, -10.0, 20.0, 10.0).to_vec();
    points.push(points[0]);
    points.push(points[0]);
    let mut selected = NodeSelectionSet::default();
    assert!(selected.select_lasso(
        &ir, &layout, &points, SelectionMode::Replace, SelectionContainment::Contains,
    ));
    assert_eq!(ids(&selected), ["A"]);
    points.reverse();
    assert!(!selected.select_lasso(
        &ir, &layout, &points, SelectionMode::Replace, SelectionContainment::Contains,
    ));
    assert_eq!(ids(&selected), ["A"]);
}

#[test]
fn lasso_finds_crossing_strips_and_polygons_wholly_inside_a_node() {
    let (ir, layout) = fixture();
    let strip = polygon_box(-5.0, -1.0, 25.0, 1.0);
    let mut selected = NodeSelectionSet::default();
    // Neither shape has a corner inside the other: only an edge-crossing test finds this.
    assert!(selected.select_lasso(
        &ir, &layout, &strip, SelectionMode::Replace, SelectionContainment::Intersects,
    ));
    assert_eq!(ids(&selected), ["A"]);
    assert!(selected.select_lasso(
        &ir, &layout, &strip, SelectionMode::Replace, SelectionContainment::Contains,
    ));
    assert!(selected.is_empty());
    let triangle = [(5.0, -1.0), (10.0, 1.0), (15.0, -1.0)];
    assert!(selected.select_lasso(
        &ir, &layout, &triangle, SelectionMode::Replace, SelectionContainment::Intersects,
    ));
    assert_eq!(ids(&selected), ["A"]);
}

#[test]
fn lasso_contains_rejects_a_concave_notch_despite_all_four_corners_being_inside() {
    let (ir, layout) = fixture();
    let notch = [
        (-5.0, -15.0), (25.0, -15.0), (25.0, 15.0), (15.0, 15.0),
        (15.0, 0.0), (5.0, 0.0), (5.0, 15.0), (-5.0, 15.0),
    ];
    let mut selected = NodeSelectionSet::default();
    selected.select_node(&ir, &layout, "A", SelectionMode::Replace);
    assert!(selected.select_lasso(
        &ir, &layout, &notch, SelectionMode::Replace, SelectionContainment::Contains,
    ));
    assert!(selected.is_empty());
    assert!(selected.select_lasso(
        &ir, &layout, &notch, SelectionMode::Replace, SelectionContainment::Intersects,
    ));
    assert_eq!(ids(&selected), ["A"]);
}

#[test]
fn self_crossing_lasso_uses_even_odd_fill_instead_of_signed_area() {
    let (ir, layout) = fixture();
    let bow_tie = [(-20.0, -20.0), (40.0, 20.0), (-20.0, 20.0), (40.0, -20.0)];
    let mut selected = NodeSelectionSet::default();
    selected.select_node(&ir, &layout, "Z", SelectionMode::Replace);
    assert!(selected.select_lasso(
        &ir, &layout, &bow_tie, SelectionMode::Replace, SelectionContainment::Intersects,
    ));
    assert_eq!(ids(&selected), ["A"]);
    assert!(selected.select_lasso(
        &ir, &layout, &bow_tie, SelectionMode::Replace, SelectionContainment::Contains,
    ));
    assert!(selected.is_empty());
    // All corners lie on the stroke, but tracing the same boundary twice cancels its fill.
    let boundary = polygon_box(0.0, -10.0, 20.0, 10.0);
    let twice: Vec<_> = boundary.into_iter().chain(boundary).collect();
    assert!(!selected.select_lasso(
        &ir, &layout, &twice, SelectionMode::Replace, SelectionContainment::Contains,
    ));
    assert!(selected.is_empty());
}

#[test]
fn lasso_does_not_lose_a_small_node_on_a_very_long_segment() {
    let (ir, layout) = fixture();
    let strip = polygon_box(-1.0e30, -1.0, 1.0e30, 1.0);
    let mut selected = NodeSelectionSet::default();
    assert!(selected.select_lasso(
        &ir, &layout, &strip, SelectionMode::Replace, SelectionContainment::Intersects,
    ));
    assert_eq!(ids(&selected), ["A", "M", "Z"]);
}

#[test]
fn invalid_or_oversized_lasso_does_not_mutate_selection() {
    let (ir, layout) = fixture();
    let mut selected = NodeSelectionSet::default();
    selected.select_node(&ir, &layout, "M", SelectionMode::Replace);
    let invalid = [
        Vec::new(),
        vec![(0.0, 0.0), (10.0, 10.0)],
        vec![(0.0, 0.0); 3],
        vec![(0.0, 0.0), (10.0, 10.0), (20.0, 20.0)],
        vec![(f64::NAN, 0.0), (20.0, 0.0), (10.0, 20.0)],
        vec![(f64::INFINITY, 0.0), (20.0, 0.0), (10.0, 20.0)],
        vec![(1.0e101, 0.0), (20.0, 0.0), (10.0, 20.0)],
        vec![(0.0, 0.0); MAX_LASSO_POINTS + 1],
    ];
    for points in invalid {
        assert!(!selected.select_lasso(
            &ir, &layout, &points, SelectionMode::Replace, SelectionContainment::Intersects,
        ));
        assert_eq!(ids(&selected), ["M"]);
    }
    let empty = polygon_box(100.0, 100.0, 120.0, 120.0);
    assert!(selected.select_lasso(
        &ir, &layout, &empty, SelectionMode::Replace, SelectionContainment::Contains,
    ));
    assert!(selected.is_empty());
}

#[test]
fn lasso_toggle_deduplicates_layout_instances_and_ignores_draw_order() {
    let (ir, mut layout) = fixture();
    layout.nodes.push(layout.nodes[0].clone());
    let points = polygon_box(-50.0, -20.0, 70.0, 20.0);
    let mut selected = NodeSelectionSet::default();
    assert!(selected.select_lasso(
        &ir, &layout, &points, SelectionMode::Toggle, SelectionContainment::Contains,
    ));
    assert_eq!(ids(&selected), ["A", "M", "Z"]);
    layout.nodes.reverse();
    assert!(selected.select_lasso(
        &ir, &layout, &points, SelectionMode::Toggle, SelectionContainment::Contains,
    ));
    assert!(selected.is_empty());
}

#[test]
fn toggle_drag_previews_are_idempotent_and_recomputed_from_pointer_down() {
    let (ir, layout) = fixture();
    let mut selected = NodeSelectionSet::default();
    selected.select_node(&ir, &layout, "A", SelectionMode::Replace);
    let gesture = selected.begin_gesture(SelectionMode::Toggle, SelectionContainment::Contains);
    let broad = SelectionRegion::Box(all());
    assert!(gesture.preview(&ir, &layout, &mut selected, broad));
    assert_eq!(ids(&selected), ["M", "Z"]);
    assert!(!gesture.preview(&ir, &layout, &mut selected, broad));
    assert_eq!(ids(&selected), ["M", "Z"]);
    let points = polygon_box(40.0, -10.0, 60.0, 10.0);
    assert!(gesture.preview(&ir, &layout, &mut selected, SelectionRegion::Lasso(&points)));
    assert_eq!(ids(&selected), ["A", "M"]);
    assert!(!gesture.preview(&ir, &layout, &mut selected, SelectionRegion::Lasso(&points)));
    assert!(gesture.cancel(&ir, &layout, &mut selected));
    assert_eq!(ids(&selected), ["A"]);
}

#[test]
fn add_and_remove_drags_restore_nodes_outside_the_current_preview() {
    let (ir, layout) = fixture();
    for mode in [SelectionMode::Add, SelectionMode::Remove] {
        let mut selected = NodeSelectionSet::default();
        if mode == SelectionMode::Add {
            selected.select_node(&ir, &layout, "Z", SelectionMode::Replace);
        } else {
            selected.select_box(
                &ir, &layout, all(), SelectionMode::Replace, SelectionContainment::Contains,
            );
        }
        let gesture = selected.begin_gesture(mode, SelectionContainment::Contains);
        gesture.preview(&ir, &layout, &mut selected, SelectionRegion::Box(all()));
        let points = polygon_box(0.0, -10.0, 20.0, 10.0);
        assert!(gesture.preview(&ir, &layout, &mut selected, SelectionRegion::Lasso(&points)));
        if mode == SelectionMode::Add {
            assert_eq!(ids(&selected), ["A", "Z"]);
        } else {
            assert_eq!(ids(&selected), ["M", "Z"]);
        }
    }
}

#[test]
fn invalid_preview_is_noop_and_cancel_does_not_resurrect_deleted_nodes() {
    let (ir, layout) = fixture();
    let mut selected = NodeSelectionSet::default();
    selected.select_node(&ir, &layout, "Z", SelectionMode::Replace);
    let gesture = selected.begin_gesture(SelectionMode::Replace, SelectionContainment::Contains);
    gesture.preview(&ir, &layout, &mut selected, SelectionRegion::Box(all()));
    let invalid = [(f64::NAN, 0.0), (20.0, 0.0), (10.0, 20.0)];
    assert!(!gesture.preview(&ir, &layout, &mut selected, SelectionRegion::Lasso(&invalid)));
    assert_eq!(ids(&selected), ["A", "M", "Z"]);
    let replacement = fm_parser::parse("flowchart LR\n A[Alpha]\n M[Mike]\n").ir;
    let replacement_layout = layout_diagram(&replacement);
    assert!(gesture.cancel(&replacement, &replacement_layout, &mut selected));
    assert!(selected.is_empty());
}

#[test]
fn lasso_and_live_preview_use_the_actual_renderer_viewport() {
    let (ir, layout) = fixture();
    let config = CanvasRenderConfig {
        auto_fit: false,
        padding: 10.0,
        ..CanvasRenderConfig::default()
    };
    let mut context = MockCanvas2dContext::new(400.0, 200.0);
    let result = render_to_canvas_with_layout(&ir, &layout, &mut context, &config);
    // Independently derived from A's raw box, layout origin and padding, not a round trip.
    let points = polygon_box(59.0, 19.0, 81.0, 41.0);
    let mut selected = NodeSelectionSet::default();
    assert!(selected.select_lasso_canvas(
        &ir, &layout, &result.viewport, &points,
        SelectionMode::Replace, SelectionContainment::Contains,
    ));
    assert_eq!(ids(&selected), ["A"]);
    selected.clear();
    let gesture = selected.begin_gesture(SelectionMode::Replace, SelectionContainment::Contains);
    assert!(gesture.preview_canvas(
        &ir, &layout, &result.viewport, &mut selected, SelectionRegion::Lasso(&points),
    ));
    assert_eq!(ids(&selected), ["A"]);
    for zoom in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let invalid = Viewport { zoom, ..result.viewport };
        assert!(!selected.select_lasso_canvas(
            &ir, &layout, &invalid, &points,
            SelectionMode::Replace, SelectionContainment::Contains,
        ));
        assert!(!gesture.preview_canvas(
            &ir, &layout, &invalid, &mut selected, SelectionRegion::Lasso(&points),
        ));
    }
    assert_eq!(ids(&selected), ["A"]);
}
