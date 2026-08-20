//! The `--` concurrency separator reaches the canvas (bd-dgnm4).
//!
//! `state Big { A --> B  --  C --> D }` declares two regions running in parallel. The layout records
//! each boundary in `extensions.cluster_dividers`, and fm-render-svg drew a dashed line per divider
//! while this surface referenced the extension nowhere — so the two regions ran together into one
//! box and a reader could not tell there were two.
//!
//! The canvas half of the fix landed under the build freeze with "NO TEST YET - unbuilt", and the
//! bead named the probe it wanted: a composite state with a `--` separator must produce a stroked
//! path that a state WITHOUT the separator does not. That probe is here.
//!
//! ISOLATED BY A MECHANISM CONTROL. Deleting the `--` from the source changes the region structure
//! and hence the whole placement, so an op-stream diff between two sources would be dominated by
//! relayout. These tests render the SAME ir and the SAME layout twice, differing only in whether
//! `extensions.cluster_dividers` is populated.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas_with_layout};

/// Two concurrent regions inside one composite state.
const CONCURRENT: &str = "stateDiagram-v2\n    state Big {\n        [*] --> A\n        A --> B\n        --\n        [*] --> C\n        C --> D\n    }\n";

/// The same composite state with NO separator: one region, so no boundary exists to draw.
const SEQUENTIAL: &str = "stateDiagram-v2\n    state Big {\n        [*] --> A\n        A --> B\n        C --> D\n    }\n";

/// The dash the SVG uses for a region boundary. It is what distinguishes a divider from an ordinary
/// cluster edge, so the two backends have to agree on it or they draw different pictures.
const DIVIDER_DASH: &str = "SetLineDash([6.0, 4.0])";

fn ops_with_and_without_dividers(source: &str) -> (String, String) {
    let ir = fm_parser::parse(source).ir;
    let config = CanvasRenderConfig::default();
    let layout_config = fm_layout::LayoutConfig {
        font_metrics: Some(config.font_metrics()),
        ..Default::default()
    };
    let layout = fm_layout::layout_diagram_with_config(&ir, layout_config);

    let mut without_layout = layout.clone();
    without_layout.extensions.cluster_dividers.clear();

    let mut with_context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas_with_layout(&ir, &layout, &mut with_context, &config);

    let mut without_context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas_with_layout(&ir, &without_layout, &mut without_context, &config);

    (
        format!("{:?}", with_context.operations()),
        format!("{:?}", without_context.operations()),
    )
}

/// The separator produces a divider in the layout at all. Asserted on its own because every other
/// test here would pass vacuously if it did not.
#[test]
fn the_separator_reaches_the_layout_as_a_divider() {
    let ir = fm_parser::parse(CONCURRENT).ir;
    let config = CanvasRenderConfig::default();
    let layout_config = fm_layout::LayoutConfig {
        font_metrics: Some(config.font_metrics()),
        ..Default::default()
    };
    let layout = fm_layout::layout_diagram_with_config(&ir, layout_config);
    assert!(
        !layout.extensions.cluster_dividers.is_empty(),
        "the `--` separator produced no cluster divider, so nothing downstream can draw one"
    );
}

/// THE BEAD'S NAMED PROBE: the divider is drawn, and it is drawn DASHED.
#[test]
fn a_concurrency_divider_draws_a_dashed_stroke() {
    let (with_dividers, without_dividers) = ops_with_and_without_dividers(CONCURRENT);

    assert_ne!(
        with_dividers, without_dividers,
        "populating extensions.cluster_dividers changed no canvas operation"
    );
    assert!(
        with_dividers.contains(DIVIDER_DASH),
        "the divider was not drawn with the region-boundary dash the SVG uses"
    );
    assert!(
        !without_dividers.contains(DIVIDER_DASH),
        "CONTROL FAILED: the region-boundary dash appears with no divider present, so its presence \
         above proves nothing"
    );
}

/// THE NEGATIVE CASE a naive implementation fails: a composite state WITHOUT a separator must not
/// grow a rule. An implementation that drew a line per CLUSTER rather than per DIVIDER would pass
/// the test above and fail this one.
#[test]
fn a_composite_state_without_a_separator_draws_no_divider() {
    let ir = fm_parser::parse(SEQUENTIAL).ir;
    let config = CanvasRenderConfig::default();
    let layout_config = fm_layout::LayoutConfig {
        font_metrics: Some(config.font_metrics()),
        ..Default::default()
    };
    let layout = fm_layout::layout_diagram_with_config(&ir, layout_config);
    assert!(
        layout.extensions.cluster_dividers.is_empty(),
        "a single-region composite state must publish no divider"
    );
    // Non-vacuity: this source must still produce a real composite state, or the assertion above
    // holds for the boring reason that there is no cluster at all.
    assert!(
        !layout.clusters.is_empty(),
        "CONTROL FAILED: this source produced no cluster, so it is not the case being tested"
    );

    let (with_dividers, without_dividers) = ops_with_and_without_dividers(SEQUENTIAL);
    assert_eq!(
        with_dividers, without_dividers,
        "a composite state with no `--` still took the divider path"
    );
}

/// The dash is RESET after the dividers are drawn. Canvas2D dash state is sticky: leaving it set
/// would silently dash whatever the renderer draws next, which is the class of bug that makes one
/// missing `set_line_dash(&[])` show up as an unrelated element rendering wrong.
#[test]
fn the_divider_dash_does_not_leak_to_later_drawing() {
    let (with_dividers, _) = ops_with_and_without_dividers(CONCURRENT);

    let last_dash = with_dividers
        .rfind(DIVIDER_DASH)
        .expect("the divider dash must be present for this test to mean anything");
    let tail = &with_dividers[last_dash..];
    assert!(
        tail.contains("SetLineDash([])"),
        "the region-boundary dash was never reset, so it leaks onto everything drawn afterwards"
    );
}
