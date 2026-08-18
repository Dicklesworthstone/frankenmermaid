//! A mindmap's DEFAULT node is borderless; `[square]` is a rectangle (bd-d9mw).
//!
//! MEASURED BY CONTENT HASH before the fix: in a mindmap, `A` and `A[A]` produced BYTE-IDENTICAL
//! SVG, because both parse to `NodeShape::Rect`. Every other mindmap shape was already distinct, so
//! this was a single collision rather than a broken feature.
//!
//! THE BEAD WAS BLOCKED ON NOT KNOWING WHAT DEFAULT SHOULD LOOK LIKE, and would not guess. Read out
//! of the pinned mermaid 11.15.0 bundle, its mindmap switch maps node types to shape NAMES:
//!
//!     DEFAULT -> "no-border"   RECT -> "rect"   ROUNDED_RECT -> "rounded-rect"
//!     CIRCLE  -> "circle"      CLOUD -> "cloud"  BANG -> "bang"   HEXAGON -> "hexgon" (sic)
//!
//! with `DEFAULT:0` and `NO_BORDER:0` sharing an enum value. So the incumbent's default node is
//! BORDERLESS, and it draws that difference in CSS rather than in geometry — `defaultBkg` emits
//! `class="node-bkg node-no-border"`. That is why the Rust side needs a marker class and NOT a new
//! `NodeShape` variant, which would have meant auditing 172 `NodeShape::` sites in this crate alone.

/// The two sources must stop rendering identically. This is the bead's own measurement, inverted.
#[test]
fn a_default_mindmap_node_no_longer_renders_identically_to_a_square_one() {
    let default_svg =
        fm_render_svg::render_svg(&fm_parser::parse("mindmap\n  root((R))\n    A\n").ir);
    let square_svg =
        fm_render_svg::render_svg(&fm_parser::parse("mindmap\n  root((R))\n    A[A]\n").ir);

    assert_ne!(
        default_svg, square_svg,
        "a default mindmap node and a [square] one still render identically"
    );
}

/// NON-VACUITY: the marker class must actually reach the output for the DEFAULT node, and must NOT
/// be attached to the explicitly-square one. Without this, the inequality above could be satisfied
/// by any incidental difference between the two renders.
#[test]
fn the_marker_class_marks_the_default_node_and_only_it() {
    let default_svg =
        fm_render_svg::render_svg(&fm_parser::parse("mindmap\n  root((R))\n    A\n").ir);
    let square_svg =
        fm_render_svg::render_svg(&fm_parser::parse("mindmap\n  root((R))\n    A[A]\n").ir);

    assert!(
        default_svg.contains("mindmap-no-border"),
        "the default node carries no marker class:\n{default_svg}"
    );
    // The CSS RULE is emitted for every mindmap render, so count the marker's appearances rather
    // than merely asserting absence: the square render should carry the rule but no marked node.
    assert!(
        default_svg.matches("mindmap-no-border").count()
            > square_svg.matches("mindmap-no-border").count(),
        "the square node is marked as often as the default one, so the marker does not \
         discriminate them"
    );
}

/// THE MARKER MUST HAVE A CSS RULE, not merely be present as a class.
///
/// A class with no rule behind it changes the bytes and not the picture — this project has already
/// shipped semantic CSS behind a cosmetic gate once (bd-w0f0) and learned to assert the rule, not
/// the class. `embed_theme_css` defaults to true, so the shipping render carries the stylesheet.
#[test]
fn the_marker_class_has_a_css_rule_behind_it() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse("mindmap\n  root((R))\n    A\n").ir);

    let style_start = svg
        .find("<style")
        .expect("the shipping config embeds a stylesheet");
    let style_end = svg[style_start..]
        .find("</style>")
        .map(|offset| style_start + offset)
        .expect("the stylesheet is closed");
    let stylesheet = &svg[style_start..style_end];

    assert!(
        stylesheet.contains("mindmap-no-border"),
        "the marker class has no rule in the embedded stylesheet, so it cannot change the picture:\n{stylesheet}"
    );
    assert!(
        stylesheet.contains("stroke: none"),
        "the marker's rule does not remove the border:\n{stylesheet}"
    );
}

/// CONTROL: the shapes that were ALREADY distinct must stay distinct and stay unmarked. The fix
/// touches the default branch only, so a change that marked every mindmap node would pass the tests
/// above and quietly strip the border from circles and hexagons too.
#[test]
fn explicitly_shaped_mindmap_nodes_are_untouched() {
    for source in [
        "mindmap\n  root((R))\n    A(A)\n",
        "mindmap\n  root((R))\n    A((A))\n",
        "mindmap\n  root((R))\n    A{{A}}\n",
        "mindmap\n  root((R))\n    A))A((\n",
        "mindmap\n  root((R))\n    A)A(\n",
    ] {
        let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
        // One appearance is the CSS rule itself; a MARKED node would add more.
        assert_eq!(
            svg.matches("mindmap-no-border").count(),
            1,
            "an explicitly shaped node was marked borderless by:\n{source}"
        );
    }
}

/// CONTROL FOR THE CLASS-SUFFIX PATH: `A:::cls` declares no shape, so it is still DEFAULT — and the
/// suffix must not be mistaken for one. The discriminator strips `:::` through the same helper the
/// shape parser uses, so this pins that they agree.
#[test]
fn a_class_suffix_does_not_change_the_default_shape() {
    let svg =
        fm_render_svg::render_svg(&fm_parser::parse("mindmap\n  root((R))\n    A:::mycls\n").ir);

    assert!(
        svg.matches("mindmap-no-border").count() > 1,
        "a node with a class suffix lost its default-shape marker:\n{svg}"
    );
}
