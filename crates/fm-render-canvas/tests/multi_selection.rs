//! Multi-selection uses the rendered IR/layout, not click metadata or parser-array positions.

use fm_core::MermaidDiagramIr;
use fm_layout::{DiagramLayout, LayoutRect, layout_diagram};
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
        placed.bounds = LayoutRect { x, y: -10.0, width: 20.0, height: 20.0 };
    }
    layout.bounds = LayoutRect { x: -50.0, y: -20.0, width: 120.0, height: 40.0 };
    (ir, layout)
}

fn ids(selection: &NodeSelectionSet) -> Vec<&str> {
    selection.selected_node_ids().collect()
}

fn all() -> SelectionBox {
    SelectionBox { start: (-50.0, -20.0), end: (70.0, 20.0) }
}

#[test]
fn marquee_selects_unlinked_nodes_and_sorts_author_ids() {
    let (ir, mut layout) = fixture();
    let mut selected = NodeSelectionSet::default();
    assert!(selected.select_box(&ir, &layout, all(), SelectionMode::Replace, SelectionContainment::Contains));
    assert_eq!(ids(&selected), ["A", "M", "Z"]);
    assert_eq!(selected.len(), 3);
    layout.nodes.reverse();
    assert!(!selected.select_box(&ir, &layout, all(), SelectionMode::Replace, SelectionContainment::Contains));
    assert_eq!(ids(&selected), ["A", "M", "Z"]);
    let reverse = SelectionBox { start: all().end, end: all().start };
    assert!(!selected.select_box(&ir, &layout, reverse, SelectionMode::Replace, SelectionContainment::Contains));
}

#[test]
fn partial_overlap_and_full_containment_are_distinct() {
    let (ir, layout) = fixture();
    let region = SelectionBox { start: (-5.0, -15.0), end: (5.0, 15.0) };
    let mut selected = NodeSelectionSet::default();
    assert!(selected.select_box(&ir, &layout, region, SelectionMode::Replace, SelectionContainment::Intersects));
    assert_eq!(ids(&selected), ["A"]);
    assert!(selected.select_box(&ir, &layout, region, SelectionMode::Replace, SelectionContainment::Contains));
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
    assert!(selected.select_box(&ir, &layout, all(), SelectionMode::Toggle, SelectionContainment::Contains));
    assert_eq!(ids(&selected), ["A", "Z"]);
    assert!(selected.select_box(&ir, &layout, all(), SelectionMode::Add, SelectionContainment::Contains));
    assert_eq!(ids(&selected), ["A", "M", "Z"]);
    assert!(selected.select_box(&ir, &layout, all(), SelectionMode::Remove, SelectionContainment::Contains));
    assert!(selected.is_empty());
    assert!(!selected.clear());
}

#[test]
fn clicks_choose_topmost_box_but_marquee_includes_overlaps() {
    let (ir, mut layout) = fixture();
    for placed in &mut layout.nodes {
        placed.bounds = LayoutRect { x: 0.0, y: 0.0, width: 20.0, height: 20.0 };
    }
    let top = ir.nodes[layout.nodes.last().unwrap().node_index].id.as_str();
    let mut selected = NodeSelectionSet::default();
    assert!(selected.select_at(&ir, &layout, (10.0, 10.0), SelectionMode::Replace));
    assert_eq!(ids(&selected), [top]);
    assert!(selected.select_at(&ir, &layout, (20.0, 10.0), SelectionMode::Replace));
    assert!(selected.is_empty());
    selected.select_box(&ir, &layout, all(), SelectionMode::Replace, SelectionContainment::Contains);
    assert_eq!(selected.len(), 3);
}

#[test]
fn bad_input_never_clears_selection_or_selects_invalid_geometry() {
    let (ir, mut layout) = fixture();
    let mut selected = NodeSelectionSet::default();
    selected.select_node(&ir, &layout, "A", SelectionMode::Replace);
    assert!(!selected.select_node(&ir, &layout, "missing", SelectionMode::Replace));
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let region = SelectionBox { start: (invalid, 0.0), end: (10.0, 10.0) };
        assert!(!selected.select_box(&ir, &layout, region, SelectionMode::Replace, SelectionContainment::Intersects));
        assert!(!selected.select_at(&ir, &layout, (invalid, 0.0), SelectionMode::Replace));
    }
    let line = SelectionBox { start: (0.0, 0.0), end: (0.0, 10.0) };
    assert!(!selected.select_box(&ir, &layout, line, SelectionMode::Replace, SelectionContainment::Intersects));
    assert_eq!(ids(&selected), ["A"]);
    for placed in &mut layout.nodes {
        match ir.nodes[placed.node_index].id.as_str() {
            "Z" => placed.node_index = usize::MAX,
            "M" => placed.bounds.width = f32::NAN,
            _ => {}
        }
    }
    assert!(!selected.select_box(&ir, &layout, all(), SelectionMode::Replace, SelectionContainment::Contains));
    assert_eq!(ids(&selected), ["A"]);
}

#[test]
fn selection_survives_node_reindexing_and_removes_deleted_ids() {
    let (ir, layout) = fixture();
    let mut selected = NodeSelectionSet::default();
    selected.select_box(&ir, &layout, all(), SelectionMode::Replace, SelectionContainment::Contains);
    let replacement = fm_parser::parse("flowchart LR\n M[Moved]\n A[Renamed label]\n B[New]\n").ir;
    let replacement_layout = layout_diagram(&replacement);
    assert!(selected.reconcile(&replacement, &replacement_layout));
    assert_eq!(ids(&selected), ["A", "M"]);
    assert!(!selected.reconcile(&replacement, &replacement_layout));
}

#[test]
fn backing_pixel_marquee_respects_zoom_translation_and_does_not_reapply_dpr() {
    let (ir, layout) = fixture();
    let viewport = Viewport { offset_x: 100.0, offset_y: 50.0, zoom: 2.0,
        device_pixel_ratio: 3.0, ..Viewport::default() };
    let region = SelectionBox { start: (99.0, 29.0), end: (141.0, 71.0) };
    let mut selected = NodeSelectionSet::default();
    assert!(selected.select_box_canvas(&ir, &layout, &viewport, region,
        SelectionMode::Replace, SelectionContainment::Contains));
    assert_eq!(ids(&selected), ["A"]);
    for zoom in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let invalid = Viewport { zoom, ..viewport };
        assert!(!selected.select_box_canvas(&ir, &layout, &invalid, region,
            SelectionMode::Replace, SelectionContainment::Contains));
    }
    assert_eq!(ids(&selected), ["A"]);
}

#[test]
fn selection_rings_append_to_the_real_draw_stream_and_keep_metadata() {
    let (mut ir, layout) = fixture();
    let node = ir.nodes.iter_mut().find(|node| node.id == "A").unwrap();
    node.interaction_mut().tooltip = Some(String::from("Inspect Alpha"));
    let config = CanvasRenderConfig { auto_fit: false, padding: 10.0, ..CanvasRenderConfig::default() };
    let style = CanvasSelectionStyle::default();
    let mut base = MockCanvas2dContext::new(400.0, 200.0);
    let expected = render_to_canvas_with_layout(&ir, &layout, &mut base, &config);
    let mut selected = NodeSelectionSet::default();
    let mut empty = MockCanvas2dContext::new(400.0, 200.0);
    let empty_result = render_to_canvas_with_multi_selection(&ir, &layout, &mut empty, &config, &mut selected, &style);
    assert_eq!(empty.operations(), base.operations());
    assert_eq!(empty_result.draw_calls, expected.draw_calls);
    selected.select_box(&ir, &layout, all(), SelectionMode::Replace, SelectionContainment::Contains);
    let mut context = MockCanvas2dContext::new(400.0, 200.0);
    let actual = render_to_canvas_with_multi_selection(&ir, &layout, &mut context, &config, &mut selected, &style);
    assert_eq!(&context.operations()[..base.operation_count()], base.operations());
    assert_eq!(actual.draw_calls, expected.draw_calls + 6);
    assert_eq!(actual.nodes_drawn, expected.nodes_drawn);
    assert_eq!(actual.edges_drawn, expected.edges_drawn);
    assert_eq!(actual.labels_drawn, expected.labels_drawn);
    assert_eq!(actual.hit_regions, expected.hit_regions);
    assert_eq!(actual.viewport, expected.viewport);
    let rings: Vec<_> = context.operations()[base.operation_count()..].iter()
        .filter(|op| matches!(op, DrawOperation::StrokeRect(..))).collect();
    assert_eq!(rings.len(), 6);
    // A starts at canvas (60,20), independently derived from raw bounds, origin and padding.
    assert!(rings.contains(&&DrawOperation::StrokeRect(54.0, 14.0, 32.0, 32.0)));
}
