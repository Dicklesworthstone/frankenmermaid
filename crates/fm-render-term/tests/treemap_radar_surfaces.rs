//! Treemap and radar reach the terminal surface at all (bd-dw450).
//!
//! THE DEFECT. bd-9ghyo and bd-sk4dv built `treemap` and `radar-beta` end to end — parser, IR,
//! layout, SVG — and both put their whole diagram in a LAYOUT EXTENSION (`treemap_tiles`,
//! `radar`). Only fm-render-svg ever read those, so on the terminal, canvas and WebGPU surfaces a
//! valid document of either type rendered as a COMPLETELY BLANK CANVAS: no error, no warning, no
//! diagnostic — nothing to suggest the diagram had been parsed and laid out perfectly one layer
//! earlier. A blank page is the worst possible failure here precisely because it looks like a
//! diagram that legitimately has nothing in it.
//!
//! THE NEGATIVE CASE, in this bead's shape: the output must DIFFER from the empty canvas it used
//! to be. Every assertion below is therefore a comparison against a real blank render rather than
//! a search for some expected glyph — because "contains a box-drawing character" is satisfied by
//! any diagram at all, while "differs from what an empty document produces" is satisfied only by
//! actually drawing something.

use fm_render_term::render_term;

const TREEMAP: &str = "treemap\n\"R\"\n    \"G1\"\n        \"a\": 10\n        \"b\": 20\n    \"G2\"\n        \"c\": 30\n";
const RADAR: &str = "radar-beta\n  axis a, b, c\n  curve x{1,2,3}\n";

fn render(source: &str) -> String {
    render_term(&fm_parser::parse(source).ir)
}

/// A canvas holding nothing but the blank braille pattern and whitespace.
fn is_blank(rendered: &str) -> bool {
    rendered
        .chars()
        .all(|ch| ch == '\u{2800}' || ch.is_whitespace())
}

#[test]
fn the_blank_detector_actually_detects_blankness() {
    assert!(is_blank("\u{2800}\u{2800}\n \n"));
    assert!(!is_blank("\u{2800}x\u{2800}"));
    // A diagram that has always worked is not blank — the control that says this file's
    // assertions are about treemap and radar, not about the terminal renderer being broken.
    assert!(!is_blank(&render("flowchart LR\n  A --> B\n")));
}

/// THE NEGATIVE CASE for treemap: the terminal no longer renders it as an empty canvas.
#[test]
fn a_treemap_is_not_a_blank_terminal_canvas() {
    let rendered = render(TREEMAP);
    assert!(
        !is_blank(&rendered),
        "a treemap still renders as a blank terminal canvas"
    );
    // Its geometry, not merely its text: an implementation that drew only labels would pass a
    // blankness check while leaving the treemap's actual shape invisible.
    let geometry: usize = rendered
        .chars()
        .filter(|&ch| ('\u{2801}'..='\u{28ff}').contains(&ch))
        .count();
    assert!(
        geometry > 50,
        "only {geometry} drawn cells: the tiles are not being outlined"
    );
}

/// THE NEGATIVE CASE for radar.
#[test]
fn a_radar_is_not_a_blank_terminal_canvas() {
    let rendered = render(RADAR);
    assert!(
        !is_blank(&rendered),
        "a radar still renders as a blank terminal canvas"
    );
    let geometry: usize = rendered
        .chars()
        .filter(|&ch| ('\u{2801}'..='\u{28ff}').contains(&ch))
        .count();
    assert!(
        geometry > 50,
        "only {geometry} drawn cells: the wheel is not being drawn"
    );
}

/// Every treemap tile is captioned with its label and its value.
///
/// The values are the discriminating half: `G1` showing `30` rather than nothing proves the
/// terminal resolves the DEEP descendant sum, not just the labels it could read straight off the
/// parse tree.
#[test]
fn treemap_tiles_are_captioned_with_label_and_value() {
    let rendered = render(TREEMAP);
    for expected in ["R 60", "G1 30", "G2 30", "a 10", "b 20", "c 30"] {
        assert!(
            rendered.contains(expected),
            "missing caption {expected:?} in:\n{rendered}"
        );
    }
}

/// Every radar axis is labelled, including the topmost one.
///
/// ⚠️ THE TOP AXIS IS THE ONE THAT BREAKS. Its label sits 15 layout units beyond the spoke's tip,
/// which at terminal resolution is under one cell — so it lands on the spoke's own last cell, the
/// overlay's blank guard correctly refuses to overwrite, and the label vanishes. It did exactly
/// that until the overlay learned to step outward. The other two axes, whose spokes end mid-cell,
/// were fine throughout and would have made this look solved.
#[test]
fn every_radar_axis_is_labelled_including_the_topmost() {
    let rendered = render("radar-beta\n  axis north, east, west\n  curve x{1,2,3}\n");
    for axis in ["north", "east", "west"] {
        assert!(
            rendered.contains(axis),
            "axis {axis:?} is unlabelled in:\n{rendered}"
        );
    }
}

/// The series legend names each curve, and `showLegend false` suppresses it.
#[test]
fn the_radar_legend_is_drawn_unless_suppressed() {
    let shown = render("radar-beta\n  axis a, b, c\n  curve alpha{1,2,3}\n");
    assert!(shown.contains("alpha"), "the series is unnamed:\n{shown}");
    let hidden = render("radar-beta\n  axis a, b, c\n  curve alpha{1,2,3}\n  showLegend false\n");
    assert!(
        !hidden.contains("alpha"),
        "`showLegend false` was ignored on the terminal surface"
    );
}

/// Values render by the same rule the SVG surface uses: no invented trailing zeros.
///
/// ⚠️ PINNED ON EACH SIDE SEPARATELY, not by comparing the two renderers here. fm-render-term does
/// not depend on fm-render-svg and should not start to for a test — that would couple two surfaces
/// that are deliberately independent. So this asserts the terminal half and
/// `values_display_without_trailing_zeros` in fm-render-svg asserts the other, against the same
/// literals. The reason both exist: `30` and `30.0` are two different numbers to a reader, and a
/// value formatter is exactly the kind of thing that gets reimplemented per surface and drifts.
#[test]
fn treemap_values_carry_no_invented_trailing_zeros() {
    let terminal = render("treemap\n\"Root\"\n    \"A\": 10.5\n    \"B\": 4.25\n");
    for expected in ["10.5", "4.25", "14.75"] {
        assert!(
            terminal.contains(expected),
            "terminal is missing {expected:?} in:\n{terminal}"
        );
    }
    assert!(
        !terminal.contains("10.5000") && !terminal.contains("14.7500"),
        "the terminal padded a value with trailing zeros"
    );
    let whole = render("treemap\n\"Root\"\n    \"A\": 10\n    \"B\": 20\n");
    assert!(
        whole.contains("Root 30"),
        "the whole-number sum is not 'Root 30' in:\n{whole}"
    );
    assert!(
        !whole.contains("30.0"),
        "the whole-number sum gained a decimal"
    );
}

/// A diagram type that never used these extensions is untouched.
///
/// The control for the whole file: the new drawing runs off `treemap_tiles` and `radar`, both of
/// which are empty for every other family, so a flowchart must render exactly as it did before.
#[test]
fn other_diagram_types_are_unaffected() {
    let flowchart = "flowchart LR\n  A[Start] --> B{Choice}\n  B -->|yes| C[Do]\n";
    let before = render(flowchart);
    assert!(!before.is_empty());
    assert!(
        !before.contains("fm-treemap") && !before.contains("fm-radar"),
        "a flowchart picked up treemap or radar furniture"
    );
    // Rendering twice is byte-stable, which is the property the whole surface is built on.
    assert_eq!(before, render(flowchart), "terminal render is not stable");
}
