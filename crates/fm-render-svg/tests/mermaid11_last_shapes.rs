//! The last six mermaid 11 shape names that fell back to a plain rectangle (bd-7ls21).
//!
//! With these drawn, `UNIMPLEMENTED_UPSTREAM_SHAPES` is EMPTY: every author-facing name the pinned
//! mermaid 11.15.0 registry publishes now maps to a shape this renderer draws.
//!
//! REFERENCE BEHAVIOR, read off a Chromium 151 render of the pinned bundle and, where the outline
//! is unreadable, out of the bundle's own handler:
//!
//! ```text
//!   win-pane    a box with one horizontal rule below the top and one vertical rule right of the
//!               left, BOTH AT A FIXED 10 UNITS. The handler carries it as `on = 10`; sampling a
//!               58.67 x 64 render independently put the rules at 0.170 w and 0.156 h = 9.97, 9.98
//!   datastore   an ordinary <rect> with stroke-dasharray="{width} {height}" — measured as
//!               "78.671875 54" on a 78.67 x 54 node. The dash draws the top edge, skips the
//!               right, draws the bottom, skips the left: TOP AND BOTTOM ONLY
//!   text        <rect class="text"> with computed fill:none and stroke-width:0px — no outline
//!               and no fill at all, just the label
//!   brace       quarter-circle arcs of radius f = max(5, d * 0.1) around the label box: a point
//!               at the top, a straight spine one radius in, a MIDDLE SPUR at two radii, and a
//!               mirrored point at the bottom. A `comment` render measures a 10.00-wide brace
//!               against a 38.67-wide label hull — an arm of exactly 2f with f = 5
//!   brace-r     the same, mirrored; its own registry handler, not a flag on `brace`
//!   braces      both, as a third handler; three paths where a single brace renders two
//! ```
//!
//! ⚠️ TWO OF THESE ARE INVISIBLE TO A GEOMETRY PROBE. `datastore` reports as `rect.basic
//! label-container` with an unremarkable bounding box, and `text` reports as a `rect` sized to its
//! label — I recorded the first as "mermaid draws a plain rectangle here" before reading the
//! computed style, which is where both shapes actually live. Shape is not always in the `d`.
//!
//! ⚠️ THE NEGATIVE CASE, this bead's standing rule: each shape must render DIFFERENTLY from
//! `NodeShape::Rect`. The failure mode is a silent fallback to a plain box, which every "does it
//! parse?" assertion passes (bd-vfxu) — and for `text`, whose whole definition is that no box is
//! drawn, the fallback and the feature are the same picture unless the test looks at what is
//! PAINTED rather than at what exists.

fn render(shape: &str) -> String {
    let source = format!("flowchart TD\n  A@{{ shape: {shape}, label: \"X\" }}\n");
    fm_render_svg::render_svg(&fm_parser::parse(&source).ir)
}

/// The `d` of every `<path>` in the document, joined.
fn path_data(svg: &str) -> String {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find("<path ") {
        rest = &rest[at + 6..];
        let raw = &rest[..rest.find('>').unwrap_or(rest.len())];
        if let Some(start) = raw.find("d=\"") {
            let value_at = start + 3;
            if let Some(len) = raw[value_at..].find('"') {
                out.push(raw[value_at..value_at + len].to_string());
            }
        }
    }
    out.join(" || ")
}

/// The x coordinate of every point the document's path data actually VISITS.
///
/// ⚠️ NOT "every number in the `d`", which is what this was first written as and why the two mirror
/// tests below reported a brace reaching x = 0. An elliptical arc command is
/// `A rx ry rotation large-arc sweep x y`: five of its seven numbers are radii, an angle and two
/// boolean flags, so a naive scan reads the flags `0` and `1` as coordinates and the minimum x of
/// any brace becomes 0. Only the LAST PAIR of an arc is a point, and the FIRST of a line or move.
fn visited_xs(svg: &str) -> Vec<f32> {
    let data = path_data(svg);
    let mut out = Vec::new();
    let mut verb = ' ';
    let mut numbers: Vec<f32> = Vec::new();
    let mut token = String::new();

    let mut flush = |verb: char, numbers: &mut Vec<f32>| {
        match verb {
            'M' | 'L' if numbers.len() >= 2 => out.push(numbers[0]),
            'A' if numbers.len() >= 7 => out.push(numbers[numbers.len() - 2]),
            _ => {}
        }
        numbers.clear();
    };

    for ch in data.chars().chain(std::iter::once('Z')) {
        if ch.is_ascii_digit() || ch == '.' || (ch == '-' && token.is_empty()) {
            token.push(ch);
            continue;
        }
        if let Ok(value) = token.parse::<f32>() {
            numbers.push(value);
        }
        token.clear();
        if ch.is_ascii_alphabetic() {
            flush(verb, &mut numbers);
            verb = ch;
        }
    }
    flush(verb, &mut numbers);
    out
}

/// The six names, and the node box every one of them is drawn into.
///
/// A single-label flowchart node is 100 x 66.50 at (92, 92) — the same box every other shape test
/// in this crate works from, which is what lets the expected coordinates below be written out.
const SHAPES: [&str; 6] = [
    "win-pane",
    "datastore",
    "text",
    "brace",
    "brace-r",
    "braces",
];
const BOX_LEFT: f32 = 92.0;
const BOX_TOP: f32 = 92.0;
const BOX_W: f32 = 100.0;
const BOX_H: f32 = 66.5;

/// ⚠️ THE NEGATIVE CASE: none of the six renders as the plain rectangle it used to fall back to.
#[test]
fn each_new_shape_differs_from_a_plain_rectangle() {
    let rect = render("rect");
    for shape in SHAPES {
        let drawn = render(shape);
        assert_ne!(
            drawn, rect,
            "{shape} renders identically to a plain rectangle, so the name draws nothing new"
        );
    }
}

/// And none of them collapses onto another.
///
/// Six names mapped to one shape would pass the test above six times over. `brace` and `brace-r`
/// are the pair this actually guards: they are mirror images, so the cheap implementation draws one
/// for both and puts the comment on the wrong side of what it annotates.
#[test]
fn the_six_shapes_stay_distinct_from_each_other() {
    let mut seen: Vec<(&str, String)> = Vec::new();
    for shape in SHAPES {
        let drawn = render(shape);
        for (other, other_drawn) in &seen {
            assert_ne!(
                &drawn, other_drawn,
                "{shape} and {other} render identically — two published names, one picture"
            );
        }
        seen.push((shape, drawn));
    }
    assert_eq!(seen.len(), SHAPES.len());
}

/// ⚠️ THE WINDOW PANE'S RULES SIT AT A FIXED 10, NOT AT A FRACTION OF THE BOX.
///
/// The ratio is the natural guess and it is wrong in the direction that shows: this renderer's
/// boxes are wider than mermaid's, so a proportional inset grows the small corner pane into a
/// quadrant. The assertion is written against the box's own dimensions so it states the difference
/// rather than restating a constant — at 100 x 66.50, a ratio-derived inset would land at 17.0 and
/// 10.4 respectively, and neither is 10.
#[test]
fn a_window_pane_rules_sit_at_the_measured_fixed_inset() {
    let svg = render("win-pane");
    let inset = fm_core::WINDOW_PANE_INSET;

    let horizontal = format!(
        "M{:.0} {:.0} L{:.0} {:.0}",
        BOX_LEFT,
        BOX_TOP + inset,
        BOX_LEFT + BOX_W,
        BOX_TOP + inset
    );
    let vertical = format!(
        "M{:.0} {:.0} L{:.0} {:.2}",
        BOX_LEFT + inset,
        BOX_TOP,
        BOX_LEFT + inset,
        BOX_TOP + BOX_H
    );
    assert!(
        svg.contains(&horizontal),
        "no horizontal rule at the fixed inset ({horizontal}): {}",
        path_data(&svg)
    );
    assert!(
        svg.contains(&vertical),
        "no vertical rule at the fixed inset ({vertical}): {}",
        path_data(&svg)
    );

    // The ratio mermaid's own render happens to exhibit, applied to OUR box, is a different place.
    let ratio_x = BOX_LEFT + BOX_W * 0.170;
    assert!(
        (ratio_x - (BOX_LEFT + inset)).abs() > 1.0,
        "this box cannot tell a fixed inset from a proportional one, so the test is vacuous"
    );
    assert!(
        !svg.contains(&format!("M{ratio_x:.2} ")),
        "the vertical rule was placed proportionally instead of at the fixed inset"
    );
}

/// The data store draws its top and bottom edges and NEITHER side.
///
/// The missing sides are the entire shape. A four-edged box here is the fallback, and the fallback
/// is what mermaid's own output looks like to a probe that reads geometry and not style.
#[test]
fn a_data_store_draws_its_top_and_bottom_edges_and_no_sides() {
    let svg = render("datastore");
    let top = format!(
        "M{BOX_LEFT:.0} {BOX_TOP:.0} L{:.0} {BOX_TOP:.0}",
        BOX_LEFT + BOX_W
    );
    let bottom = format!(
        "M{BOX_LEFT:.0} {:.2} L{:.0} {:.2}",
        BOX_TOP + BOX_H,
        BOX_LEFT + BOX_W,
        BOX_TOP + BOX_H
    );
    assert!(
        svg.contains(&top) && svg.contains(&bottom),
        "no top/bottom edge pair ({top} / {bottom}): {}",
        path_data(&svg)
    );

    // A side run would appear as a segment holding x constant while y moves. The stroked path has
    // exactly two horizontal runs and nothing else.
    let stroked = path_data(&svg);
    assert_eq!(
        stroked.matches('L').count(),
        2,
        "the data store's stroked outline has more than its two edges: {stroked}"
    );

    // The fill is still there — the shape is an open-ended box, not an outline.
    assert!(
        svg.contains("<rect"),
        "the data store lost its fill along with its sides: {svg}"
    );
}

/// ⚠️ A TEXT BLOCK PAINTS NOTHING, AND THE ABSENCE IS THE ASSERTION.
///
/// This is the one shape whose fallback and whose implementation are the same picture to any test
/// that asks "did the node render?" — a rectangle is exactly what a text block would be if the
/// feature did nothing. So the test asks two questions that a fallback cannot both pass: is the
/// LABEL drawn, and is any box drawn around it.
#[test]
fn a_text_block_draws_its_label_and_no_box() {
    let svg = render("text");

    assert!(
        svg.contains(">X<"),
        "the text block lost its label, so drawing nothing went too far: {svg}"
    );
    assert!(
        svg.contains("fm-node-shape-text-block"),
        "the text block is not carrying its shape class: {svg}"
    );

    let node_start = svg
        .find("fm-node-shape-text-block")
        .expect("shape class present");
    let node = &svg[node_start..];
    let node_end = node.find("</g>").unwrap_or(node.len());
    let node = &node[..node_end];
    assert!(
        !node.contains("<path") && !node.contains("<rect") && !node.contains("<polygon"),
        "the text block painted an outline: {node}"
    );

    // The control: the same source as a plain rect DOES paint one, so "nothing drawn" is a property
    // of this shape and not of the fixture.
    let rect_svg = render("rect");
    assert!(
        rect_svg.contains("<rect") || rect_svg.contains("<path"),
        "the fixture draws nothing even as a rect, so this test proves nothing"
    );
}

/// ⚠️ A BRACE HAS A MIDDLE SPUR; A PARENTHESIS DOES NOT.
///
/// Both are "a curve down the side of the text", and the six segments are otherwise identical. The
/// spur reaches a full `2f` from the spine — so the brace's outline occupies THREE distinct x
/// columns (tip, spine, spur) where a parenthesis occupies two.
#[test]
fn a_brace_reaches_its_middle_spur() {
    for (shape, on_left) in [("brace", true), ("brace-r", false)] {
        let svg = render(shape);
        let f = (BOX_H * fm_core::BRACE_RADIUS_RATIO)
            .clamp(fm_core::BRACE_MIN_RADIUS, BOX_H * 0.25)
            .min(BOX_W * 0.25);

        let (tip, spine, spur) = if on_left {
            (BOX_LEFT + 2.0 * f, BOX_LEFT + f, BOX_LEFT)
        } else {
            (
                BOX_LEFT + BOX_W - 2.0 * f,
                BOX_LEFT + BOX_W - f,
                BOX_LEFT + BOX_W,
            )
        };
        let numbers = visited_xs(&svg);
        for (name, expected) in [("tip", tip), ("spine", spine), ("spur", spur)] {
            assert!(
                numbers.iter().any(|n| (n - expected).abs() < 0.05),
                "{shape}: no {name} column at {expected:.2}; the outline is {}",
                path_data(&svg)
            );
        }
        // Three DISTINCT columns — the property a parenthesis fails.
        assert!(
            (spur - spine).abs() > 0.5 && (spine - tip).abs() > 0.5,
            "{shape}: the spur and the spine coincide, so this brace is a parenthesis"
        );
    }
}

/// The two braces are mirror images, not translations of one another.
#[test]
fn the_left_and_right_braces_point_in_opposite_directions() {
    let left = visited_xs(&render("brace"));
    let right = visited_xs(&render("brace-r"));

    let left_min = left.iter().copied().fold(f32::MAX, f32::min);
    let right_max = right.iter().copied().fold(f32::MIN, f32::max);

    assert!(
        (left_min - BOX_LEFT).abs() < 0.05,
        "the left brace does not reach the box's left edge: {left_min}"
    );
    assert!(
        (right_max - (BOX_LEFT + BOX_W)).abs() < 0.05,
        "the right brace does not reach the box's right edge: {right_max}"
    );
}

/// `braces` draws both, and is not either one alone.
#[test]
fn braces_draws_both_arms() {
    let both = render("braces");
    let left = render("brace");
    let right = render("brace-r");

    assert_ne!(both, left, "`braces` rendered only the left arm");
    assert_ne!(both, right, "`braces` rendered only the right arm");

    let numbers = visited_xs(&both);
    let min = numbers.iter().copied().fold(f32::MAX, f32::min);
    let max = numbers.iter().copied().fold(f32::MIN, f32::max);
    assert!(
        (min - BOX_LEFT).abs() < 0.05 && (max - (BOX_LEFT + BOX_W)).abs() < 0.05,
        "`braces` does not reach both edges: {min}..{max}"
    );
}

/// Every alias the pinned registry publishes for these six shapes resolves to the same drawing.
///
/// A published name that resolves to nothing sends an author to fix a spelling that was already
/// right — the defect bd-3ra5y was filed for. Aliases are taken verbatim from the registry:
/// `win-pane` carries `window-pane` and `internal-storage`; `datastore` carries `data-store`;
/// `brace` carries `comment` and `brace-l`; `brace-r`, `braces` and `text` carry none.
#[test]
fn every_published_alias_draws_the_same_shape() {
    for group in [
        vec!["win-pane", "window-pane", "internal-storage"],
        vec!["datastore", "data-store"],
        vec!["brace", "comment", "brace-l"],
    ] {
        let canonical = render(group[0]);
        for alias in &group[1..] {
            assert_eq!(
                render(alias),
                canonical,
                "{alias} does not draw the same shape as {}",
                group[0]
            );
        }
    }
}

/// None of the six is still described as unimplemented, and no alias warns at all.
#[test]
fn none_of_the_six_still_warns() {
    for shape in SHAPES.iter().copied().chain([
        "window-pane",
        "internal-storage",
        "data-store",
        "comment",
        "brace-l",
    ]) {
        let source = format!("flowchart TD\n  A@{{ shape: {shape}, label: \"X\" }}\n");
        let result = fm_parser::parse(&source);
        assert!(
            result.warnings.is_empty(),
            "{shape} still warns: {:?}",
            result.warnings
        );
    }
}

/// ⚠️ EMPTYING THE LIST MUST NOT EMPTY THE MECHANISM.
///
/// `UNIMPLEMENTED_UPSTREAM_SHAPES` is now `[&str; 0]`, and the tempting cleanup is to delete it and
/// its warning arm along with it. That would trade a milestone for a regression: the next mermaid
/// release adds names, and without the arm an author writing one gets "check your spelling" instead
/// of "not built yet". An unknown name must still be named and still be warned about.
#[test]
fn an_unrecognised_shape_name_is_still_reported() {
    let result = fm_parser::parse("flowchart TD\n  A@{ shape: not-a-real-shape, label: \"X\" }\n");
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("not-a-real-shape")),
        "an unknown shape name was swallowed: {:?}",
        result.warnings
    );
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("kept its previous shape")),
        "the warning does not state the consequence: {:?}",
        result.warnings
    );
}
