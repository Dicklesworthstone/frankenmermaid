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

    // ⚠️ THIS USED TO CHECK FOR THE BARE `mindmap-no-border` AND PASSED ON A DEAD RULE. The
    // renderer emits a user class with an `fm-node-user-` PREFIX, so the markup carries
    // `fm-node-user-mindmap-no-border` while the stylesheet said `.fm-node.mindmap-no-border` — a
    // selector that can never match. A substring check for the class NAME is satisfied by both
    // spellings, so it certified a rule that changed nothing.
    //
    // Assert the SELECTOR AS THE MARKUP SPELLS IT, and prove the two agree by taking the token out
    // of the rendered class attribute rather than hard-coding it twice.
    let marker = svg
        .split("class=\"")
        .filter_map(|chunk| chunk.split('"').next())
        .flat_map(str::split_whitespace)
        .find(|token| token.ends_with("mindmap-no-border"))
        .expect("a default mindmap node should carry the marker class");
    assert!(
        stylesheet.contains(&format!(".fm-node.{marker} ")),
        "the stylesheet has no rule matching the class the markup actually carries ({marker:?}), \
         so the marker cannot change the picture:\n{stylesheet}"
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

        // ⚠️ THIS ASSERTION USED TO BE `count() == 1`, "one appearance is the CSS rule itself", and
        // it was WRONG THE MOMENT IT WAS FIRST COMPILED: the rule I added has THREE selectors
        // (`rect`, `path`, `polygon`), so the stylesheet alone spells the class three times while
        // zero nodes are marked. The renderer was right and the arithmetic was mine.
        //
        // Counting a magic number over the whole document couples this control to the CSS's
        // formatting, which is not what it is testing. Excise the stylesheet and assert the class is
        // ABSENT FROM THE MARKUP — that states the actual claim ("no node carries the marker") and
        // survives any future edit to the rule.
        let body = match (svg.find("<style"), svg.find("</style>")) {
            (Some(start), Some(end)) => format!("{}{}", &svg[..start], &svg[end..]),
            _ => svg.clone(),
        };
        assert!(
            !body.contains("mindmap-no-border"),
            "an explicitly shaped node was marked borderless by:\n{source}"
        );
        // NON-VACUITY: the excision must not have eaten the document. Without this, a change that
        // returned an empty body would pass the check above for the wrong reason.
        assert!(
            body.contains("<svg") && body.contains("fm-node"),
            "the stylesheet excision removed the markup, so the check above proves nothing"
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
