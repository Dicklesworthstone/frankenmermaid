//! A flowchart link's WEIGHT and its END MARKER are independent, and the SVG must carry both.
//!
//! mermaid's link grammar is a product: a head marker, a body run that sets the stroke weight, and
//! a tail marker. `A ==o B` is a thick line ending in a circle; `A -.-o B` is a dotted one. The
//! parser learned the whole product in bd-lrl48, and this is the renderer half — the half the
//! bead was originally filed for, "o==o / x==x render a solid stroke".
//!
//! Three things went wrong at once and each needs its own assertion:
//!
//! 1. The stroke-width match listed only `ThickArrow` / `DoubleThickArrow` / `ThickLine`, so every
//!    `==` body that ended in a circle or a cross fell through to the default 1.8. The `==` was
//!    parsed, stored, and then thrown away at the last step.
//! 2. The style-class match likewise fell through to `fm-edge-solid`, so `.fm-edge-thick` — the
//!    rule that thickens the edge on hover — never applied to those edges.
//! 3. The streaming path spelled the dotted class `fm-edge-dotted`, WHICH IS DECLARED NOWHERE. It
//!    was a class name with no rule behind it: the dashes came from the inline `stroke-dasharray`
//!    alone, and the `.fm-animations-enabled .fm-edge-dashed` flow animation skipped those edges.

use fm_render_svg::{SvgRenderConfig, render_svg_with_config};

fn render(source: &str) -> String {
    render_svg_with_config(&fm_parser::parse(source).ir, &SvgRenderConfig::default())
}

/// The `<path>` that IS the edge, as a string, so the assertions read against the markup that
/// actually shipped rather than against a whole document.
///
/// Anchored on `<path`, not on the first `class="fm-edge`: the edge is wrapped in a
/// `<g class="fm-edge">` that carries neither the width nor the style class, and matching that
/// wrapper made every assertion below fail against markup that was already correct. The CONTROL
/// assertions in each test are what caught it.
fn edge_element(svg: &str) -> String {
    let group = svg
        .find("id=\"fm-edge-0\"")
        .unwrap_or_else(|| panic!("no edge in:\n{svg}"));
    let start = group + svg[group..].find("<path").expect("the edge is a path");
    let end = svg[start..].find("/>").expect("the path closes");
    svg[start..start + end + 2].to_string()
}

#[test]
fn a_thick_body_stays_thick_whatever_marker_ends_it() {
    // The reference: `==>`, whose weight was never in question.
    let reference = edge_element(&render("flowchart LR\n  A ==> B\n"));
    assert!(
        reference.contains("stroke-width=\"2.50\""),
        "CONTROL: `==>` is the yardstick and must itself be 2.5: {reference}"
    );

    for source in [
        "flowchart LR\n  A ==o B\n",
        "flowchart LR\n  A ==x B\n",
        "flowchart LR\n  A o==o B\n",
        "flowchart LR\n  A x==x B\n",
    ] {
        let element = edge_element(&render(source));
        assert!(
            element.contains("stroke-width=\"2.50\""),
            "{source:?} lost its `==` weight: {element}"
        );
        assert!(
            element.contains("fm-edge-thick"),
            "{source:?} is not classed thick, so `.fm-edge-thick` cannot reach it: {element}"
        );
    }
}

#[test]
fn a_dotted_body_stays_dotted_whatever_marker_ends_it() {
    let reference = edge_element(&render("flowchart LR\n  A -.-> B\n"));
    assert!(
        reference.contains("fm-edge-dashed"),
        "CONTROL: `-.->` is the yardstick and must itself be dashed: {reference}"
    );

    for source in [
        "flowchart LR\n  A -.-o B\n",
        "flowchart LR\n  A o-.-o B\n",
        "flowchart LR\n  A x-.-x B\n",
    ] {
        let element = edge_element(&render(source));
        assert!(
            element.contains("fm-edge-dashed"),
            "{source:?} lost its dotted class: {element}"
        );
    }
}

/// A class the stylesheet never declares is dead markup, and a substring assertion on the edge
/// element alone would not catch it — the class would still be "present", just inert. Assert
/// against the WHOLE document, which contains the `<style>` too.
#[test]
fn no_edge_carries_a_class_the_stylesheet_never_declares() {
    for source in [
        "flowchart LR\n  A -.-o B\n",
        "flowchart LR\n  A o-.-o B\n",
        "flowchart LR\n  A x-.-x B\n",
        "flowchart LR\n  A ==o B\n",
    ] {
        let svg = render(source);
        assert!(
            !svg.contains("fm-edge-dotted"),
            "{source:?} emits `fm-edge-dotted`, which no stylesheet in this repo declares"
        );
        for class in ["fm-edge-dashed", "fm-edge-thick"] {
            if svg.contains(&format!("class=\"fm-edge {class}"))
                || svg.contains(&format!(" {class}\""))
            {
                assert!(
                    svg.contains(&format!(".{class}{{")),
                    "{source:?} uses `{class}` but the stylesheet block declaring it was stripped"
                );
            }
        }
    }
}
