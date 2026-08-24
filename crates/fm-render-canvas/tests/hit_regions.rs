//! bd-2u0.2: the canvas can finally express `click`, and it must agree with the SVG arm about which
//! nodes are interactive and what they carry.
//!
//! bd-bk7h established that `fm-render-canvas` referenced `href`, `callback` and `tooltip` NOWHERE,
//! so the whole `click` family was unreachable from any raster surface. These tests pin the export
//! that closes it, and they join to the SVG backend on `data-id` — the same node id the SVG puts on
//! its group — so a disagreement about which nodes are interactive fails rather than being invisible.

use fm_render_canvas::{hit_regions, hit_test};

fn plan(source: &str) -> (fm_core::MermaidDiagramIr, fm_layout::DiagramLayout) {
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    (ir, layout)
}

/// SAME-IR COMPARISON: every node the SVG decorates with a tooltip has a hit region carrying that
/// same text, joined on the node id both backends use.
#[test]
fn every_tooltip_the_svg_emits_has_a_hit_region_with_the_same_text() {
    let source = "flowchart LR\n  A[Alpha] --> B[Beta]\n  B --> C[Gamma]\n  \
                  click A \"https://example.com\" \"Alpha tip\"\n  \
                  click B call doThing() \"Beta tip\"\n";
    let (ir, layout) = plan(source);
    let regions = hit_regions(&ir, &layout);

    let svg = fm_render_svg::render_svg(&ir);

    // CONTROL ON THE REFERENCE ARM: if the SVG never emitted the tooltips, the `click` declarations
    // did not reach the IR and every assertion below would hold vacuously — which is precisely the
    // shape that let this field sit dead in three renderers until bd-bk7h measured it.
    for tip in ["Alpha tip", "Beta tip"] {
        assert!(
            svg.contains(&format!("title=\"{tip}\"")),
            "CONTROL FAILED: the SVG backend emitted no title={tip:?}, so `click` never reached the \
             IR and this fixture cannot compare the two backends"
        );
    }

    assert_eq!(
        regions.len(),
        2,
        "expected exactly the two interactive nodes, got {:?}",
        regions.iter().map(|r| &r.node_id).collect::<Vec<_>>()
    );

    let alpha = regions
        .iter()
        .find(|r| r.node_id == "A")
        .expect("node A is interactive in both backends");
    assert_eq!(alpha.tooltip.as_deref(), Some("Alpha tip"));
    assert_eq!(alpha.href.as_deref(), Some("https://example.com"));
    assert!(
        alpha.callback.is_none(),
        "A declared an href, not a callback"
    );

    let beta = regions
        .iter()
        .find(|r| r.node_id == "B")
        .expect("node B is interactive in both backends");
    assert_eq!(beta.tooltip.as_deref(), Some("Beta tip"));
    assert!(
        beta.callback.is_some(),
        "B declared `call doThing()` and the callback was dropped"
    );

    // C is NOT interactive, and must have no region. Without this the export could return a region
    // per node and every assertion above would still pass, while the host saw the whole diagram as
    // clickable.
    assert!(
        !regions.iter().any(|r| r.node_id == "C"),
        "a node with no `click` declaration was given a hit region"
    );
    assert!(
        !svg.contains("title=\"Gamma"),
        "CONTROL FAILED: the SVG gave C a tooltip, so C is not the negative case this assumes"
    );
}

/// THE RENDER RESULT CARRIES THE REGIONS, and they describe the layout that was actually drawn.
///
/// A host can call `hit_regions` itself — it is public — but nothing then stops it passing a layout
/// other than the one on screen, and the regions would sit where the nodes used to be. That is a
/// defect with no visible symptom until somebody clicks. Returning them from the render is what makes
/// the mismatch unrepresentable, so this asserts the coupling rather than the function.
#[test]
fn the_render_result_carries_regions_for_the_layout_it_drew() {
    let source =
        "flowchart LR\n  A[Alpha] --> B[Beta]\n  click A \"https://example.com\" \"tip\"\n";
    let (ir, layout) = plan(source);

    let mut ctx = fm_render_canvas::MockCanvas2dContext::new(1200.0, 800.0);
    let result = fm_render_canvas::render_to_canvas_with_layout(
        &ir,
        &layout,
        &mut ctx,
        &fm_render_canvas::CanvasRenderConfig::default(),
    );

    // NON-VACUITY: the render must actually have drawn something, or "the regions match what was
    // drawn" is a statement about an empty picture.
    assert!(
        result.nodes_drawn > 0,
        "CONTROL FAILED: the renderer drew no nodes"
    );

    assert_eq!(
        result.hit_regions,
        hit_regions(&ir, &layout),
        "the result's regions differ from the ones this exact layout produces"
    );
    assert_eq!(result.hit_regions.len(), 1, "only A declared a click");
    assert_eq!(result.hit_regions[0].node_id, "A");
    assert_eq!(result.hit_regions[0].tooltip.as_deref(), Some("tip"));
}

/// A diagram with no `click` at all must export NO regions.
#[test]
fn a_diagram_without_click_exports_no_regions() {
    let (ir, layout) = plan("flowchart LR\n  A[Alpha] --> B[Beta]\n");
    assert!(
        !ir.nodes.is_empty(),
        "CONTROL FAILED: no nodes, so an empty region list proves nothing"
    );
    assert!(
        hit_regions(&ir, &layout).is_empty(),
        "a diagram with no interactions exported hit regions"
    );
}

/// The region must land where the renderer draws the node, or the host's pointer maths is right and
/// the answer is still wrong.
#[test]
fn a_region_covers_the_node_centre_and_hit_test_finds_it() {
    let source =
        "flowchart LR\n  A[Alpha] --> B[Beta]\n  click A \"https://example.com\" \"tip\"\n";
    let (ir, layout) = plan(source);
    let regions = hit_regions(&ir, &layout);
    let region = regions.first().expect("A is interactive");

    // Taken from the LAYOUT, not from the region, or this would assert the region against itself.
    let placed = layout
        .nodes
        .iter()
        .find(|n| n.node_index == region.node_index)
        .expect("the region indexes a placed node");
    let centre_x = placed.bounds.x + placed.bounds.width * 0.5;
    let centre_y = placed.bounds.y + placed.bounds.height * 0.5;

    assert!(
        region.contains(centre_x, centre_y),
        "the region does not contain its own node's centre"
    );
    assert_eq!(
        hit_test(&regions, centre_x, centre_y).map(|r| r.node_id.as_str()),
        Some("A"),
        "hit_test missed a point inside the only interactive node"
    );

    // Far outside every node: the host must be told there is nothing there, not handed the nearest
    // thing.
    let far_x = placed.bounds.x + placed.bounds.width + 10_000.0;
    assert!(
        hit_test(&regions, far_x, centre_y).is_none(),
        "hit_test returned a region for a point far outside every node"
    );
}

/// Abutting regions must not both claim their shared edge.
///
/// `contains` is half-open on the far edges for this reason. Closed-closed would make the winner
/// depend on iteration order, which is the kind of ambiguity that shows up as a click landing on the
/// wrong node only sometimes.
#[test]
fn the_far_edge_belongs_to_the_next_region_not_this_one() {
    let source =
        "flowchart LR\n  A[Alpha] --> B[Beta]\n  click A \"https://example.com\" \"tip\"\n";
    let (ir, layout) = plan(source);
    let regions = hit_regions(&ir, &layout);
    let region = regions.first().expect("A is interactive");

    let right = region.bounds.x + region.bounds.width;
    let bottom = region.bounds.y + region.bounds.height;
    let inside_y = region.bounds.y + region.bounds.height * 0.5;
    let inside_x = region.bounds.x + region.bounds.width * 0.5;

    assert!(
        region.contains(region.bounds.x, region.bounds.y),
        "the near corner must be inside: the range is half-OPEN, not exclusive at both ends"
    );
    assert!(
        !region.contains(right, inside_y),
        "the right edge is claimed by this region as well as the next one"
    );
    assert!(
        !region.contains(inside_x, bottom),
        "the bottom edge is claimed by this region as well as the next one"
    );
}
