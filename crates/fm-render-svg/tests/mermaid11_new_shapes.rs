//! Three mermaid 11 shapes this renderer now draws instead of falling back to a box (bd-7ls21).
//!
//! REFERENCE BEHAVIOR, silhouettes read off a Chromium 151 render of the pinned mermaid 11.15.0
//! bundle:
//!
//! ```text
//!   notch-rect  polygon "12,-39 37.67,-39 37.67,0 0,0 0,-27 12,-39"
//!               a box whose TOP-LEFT corner is cut away at 45°, 12 units on each axis
//!   lin-rect    "M-20.34 -27 L28.34 -27 L28.34 27 L-28.34 27 L-28.34 -27 L-20.34 -27 L-20.34 27"
//!               a box x -28.34..28.34 plus a VERTICAL RULE at x = -20.34, 8 in from the left
//!   sm-circ     <circle class="state-start" r="7">   a FIXED-radius marker, not label-sized
//!   div-rect    "M-12.84 -15.6 L12.84 -15.6 L12.84 23.4 L-12.84 23.4 L-12.84 -23.4
//!                L12.84 -23.4 L12.84 -15.6"
//!               a box y -23.4..23.4 with a HORIZONTAL rule at y=-15.6 — 7.8 of 46.8,
//!               one SIXTH of the height, so it is held as a ratio not a constant
//!   fr-circ     concentric paths from "M7 0" and "M2.5 0", both carrying the theme
//!               node fill and stroke — two RINGS, not a ring around a filled dot
//!   flip-tri    vertices (0,-49.671875) (49.671875,-49.671875) (24.8359375,0)
//!               a full-width TOP edge with the apex pointing DOWN
//!   notch-pent  vertices (-16.26875,-27) (16.26875,-27) (20.3359375,-16.2)
//!               (20.3359375,27) (-20.3359375,27) (-20.3359375,-16.2)
//!               a 40.67x54 box with BOTH top corners cut, 4.07 by 10.8 each —
//!               a tenth of the width by a fifth of the height
//!   sl-rect     vertices (-20.3359375,-27) (-20.3359375,27) (20.3359375,27)
//!               (20.3359375,-54): the top edge SLOPES UP to the right, the
//!               top-left corner sitting 27 of that 81-unit span below the top
//!   h-cyl       arcs "a5.031948881789138,15.75 0,0,1" capping a body of 23.2 —
//!               `Cylinder` rotated a quarter turn, inner arc on the LEFT end
//!   tag-rect    a FULL box plus a second path drawing the triangle
//!               (14.9359375,27) (25.7359375,27) (25.7359375,16.2) over its
//!               bottom-right corner — 10.8 on each axis, a fifth of the height
//!   lin-cyl     the `cyl` path plus a trailing "M0,12.151 a20.3359375,6.137
//!               0,0,0 40.671875,0" — a SECOND rim at twice the ellipse radius
//! ```
//!
//! ⚠️ MERMAID'S SECOND PATH IS NOT GEOMETRY. These go through rough.js, whose sketch overlay
//! re-traces the same outline as dozens of jittered cubics — `doc` reaches 5 KB of `d`. The first
//! path is the shape; the hand-drawn effect is deliberately not reproduced, so the silhouette is
//! what gets matched and never the byte string.
//!
//! ⚠️ AND THE ALIASES ARE VERBATIM FROM THE REGISTRY, not guessed: `card`/`notched-rectangle`,
//! `lined-rectangle`/`lined-process`/`lin-proc`/`shaded-process`, `small-circle`/`start`. A name the
//! registry publishes that resolves to nothing sends an author to fix a spelling that was already
//! right — the defect bd-3ra5y was filed for.

fn render(shape: &str) -> String {
    let source = format!("flowchart TD\n  A@{{ shape: {shape}, label: \"X\" }}\n");
    fm_render_svg::render_svg(&fm_parser::parse(&source).ir)
}

/// EVERY shape element in the document, reduced to its GEOMETRY attributes.
///
/// ⚠️ ATTRIBUTE VALUES CONTAIN SPACES, so this cannot be a `split_whitespace` filter. The first
/// version of this helper was exactly that, and `d="M92 92 L192 …"` reduced to the single token
/// `d="M92` — which made every path-based shape compare EQUAL and turned the distinctness test below
/// into a check on the first coordinate. It passed only because the shapes happened to start at
/// different points. Values are parsed to their closing quote instead.
///
/// ⚠️ AND IT MUST COLLECT EVERY ELEMENT, NOT THE FIRST. `fr-circ`'s outer ring is identical to
/// `sm-circ` — the whole difference is its SECOND circle, so a first-element helper reported the two
/// as the same shape. Anything that distinguishes shapes by one element cannot see a shape whose
/// identity is in its second.
fn silhouette(svg: &str) -> String {
    let mut out = Vec::new();
    for tag in ["<path ", "<circle ", "<rect ", "<polygon "] {
        let mut rest = svg;
        while let Some(at) = rest.find(tag) {
            rest = &rest[at + tag.len()..];
            let raw = &rest[..rest.find('>').unwrap_or(rest.len())];
            for name in ["d=", "points=", "r=", "width=", "height="] {
                let needle = format!("{name}\"");
                if let Some(start) = raw.find(&needle) {
                    let value_at = start + needle.len();
                    if let Some(len) = raw[value_at..].find('"') {
                        out.push(format!("{name}{}", &raw[value_at..value_at + len]));
                    }
                }
            }
        }
    }
    out.join("|")
}

#[test]
fn each_new_shape_draws_its_measured_silhouette() {
    let notch = render("notch-rect");
    assert!(
        notch.contains("L92 112.61 Z") && notch.contains("M112.61 92"),
        "notch-rect did not cut its top-left corner: {}",
        silhouette(&notch)
    );

    let lined = render("lin-rect");
    assert!(
        lined.contains("M106 92 L106 158.50"),
        "lin-rect drew no vertical rule: {}",
        silhouette(&lined)
    );

    let small = render("sm-circ");
    assert!(
        small.contains("r=\"7\""),
        "sm-circ is not a fixed radius-7 circle: {}",
        silhouette(&small)
    );

    // The rule sits one sixth down: 66.50 * 1/6 = 11.08 below the box top at y=92.
    let divided = render("div-rect");
    assert!(
        divided.contains("M92 103.08 L192 103.08"),
        "div-rect drew no horizontal header rule: {}",
        silhouette(&divided)
    );

    let framed = render("fr-circ");
    assert!(
        framed.contains("r=\"7\"") && framed.contains("r=\"2.50\""),
        "fr-circ is not two concentric fixed rings: {framed}"
    );

    // Full-width top edge (92..192 at y=92), apex at the bottom centre (142, 158.50).
    let flipped = render("flip-tri");
    assert!(
        flipped.contains("M92 92 L192 92 L142 158.50 Z"),
        "flip-tri does not point downward: {}",
        silhouette(&flipped)
    );

    // Cuts of 100*0.1 = 10 horizontally and 66.50*0.2 = 13.30 vertically, on BOTH top corners.
    let pentagon = render("notch-pent");
    assert!(
        pentagon.contains("M102 92 L182 92 L192 105.30 L192 158.50 L92 158.50 L92 105.30 Z"),
        "notch-pent did not cut both top corners: {}",
        silhouette(&pentagon)
    );

    // Top-left drops 66.50/3 = 22.17 below the box top at y=92.
    let sloped = render("sl-rect");
    assert!(
        sloped.contains("M92 114.17 L192 92 L192 158.50 L92 158.50 Z"),
        "sl-rect has no sloping top edge: {}",
        silhouette(&sloped)
    );

    // Caps on the LEFT and RIGHT ends: rx = 100*0.1 = 10, ry = 66.50/2 = 33.25.
    let horizontal = render("h-cyl");
    assert!(
        horizontal.contains("A10 33.25"),
        "h-cyl is not capped on its ends: {}",
        silhouette(&horizontal)
    );

    // Full box, then the fold: 66.50 * 0.2 = 13.30 on each axis at the bottom-right.
    let tagged = render("tag-rect");
    assert!(
        tagged.contains("M92 92 L192 92 L192 158.50 L92 158.50 Z")
            && tagged.contains("M178.70 158.50 L192 158.50 L192 145.20 Z"),
        "tag-rect is not a whole box with a corner fold: {}",
        silhouette(&tagged)
    );

    // The extra rim sits at twice the ellipse radius: 92 + 2*6.65 = 105.30.
    let lined_cylinder = render("lin-cyl");
    assert!(
        lined_cylinder.contains("M92 105.30 A50 6.65"),
        "lin-cyl has no second rim: {}",
        silhouette(&lined_cylinder)
    );

    // Straight top and sides, then two quadratics: 0.80h at the right edge, control at 1.06h.
    let document = render("doc");
    assert!(
        document
            .contains("M92 92 L192 92 L192 145.20 Q167 147.86,142 155.18 Q117 162.49,92 151.85 Z"),
        "doc has no wavy bottom edge: {}",
        silhouette(&document)
    );

    // The document body plus a rule 0.045 w in from the left (96.50), down to 0.94 h (154.51).
    let lined_document = render("lin-doc");
    assert!(
        lined_document.contains("M96.50 92 L96.50 154.51"),
        "lin-doc has no vertical rule: {}",
        silhouette(&lined_document)
    );

    // Six vertices, the two waists at 0.55h (128.57) and 0.45h (121.93).
    let lightning = render("bolt");
    assert!(
        lightning.contains(
            "M192 92 L94.30 128.57 L150.60 128.57 L92 158.50 L189.70 121.93 L133.40 121.93 Z"
        ),
        "bolt is not the measured six-vertex zigzag: {}",
        silhouette(&lightning)
    );

    // Both edges waved, in opposite phase: bottom control 1.09h (164.49), top control -0.09h (86.01).
    let banner = render("flag");
    assert!(
        banner.contains("Q117 164.49,142 152.51") && banner.contains("Q167 86.01,142 97.99"),
        "flag does not wave both edges: {}",
        silhouette(&banner)
    );
}

/// The flag's two waves must run in OPPOSITE phase, which is what makes it a banner.
///
/// Wave only the bottom and it is a document. Wave both in the SAME phase and it is a ribbon. Either
/// mistake keeps the curve commands, the closed path and the difference from a rect element — so
/// "has curves" and this bead's differs-from-Rect rule both pass while the shape is wrong. The phase
/// shows up as controls on OPPOSITE sides of the box: 164.49 is below its bottom edge (158.50) and
/// 86.01 is above its top (92).
#[test]
fn the_flag_waves_both_edges_in_opposite_phase() {
    let banner = render("flag");
    assert!(
        banner.contains("164.49"),
        "the bottom wave does not bulge below the box: {}",
        silhouette(&banner)
    );
    assert!(
        banner.contains("86.01"),
        "the top wave does not bulge above the box, so both edges wave the same way: {}",
        silhouette(&banner)
    );
    // A document waves ONE edge; the flag must not collapse onto it.
    assert_ne!(
        silhouette(&banner),
        silhouette(&render("doc")),
        "the flag renders identically to a document"
    );
    assert_ne!(
        silhouette(&banner),
        silhouette(&render("rect")),
        "the flag renders identically to a rectangle"
    );
}

/// The bolt's two waists sit at DIFFERENT heights, and that asymmetry is the whole shape.
///
/// Equalise them and the six vertices collapse into a wedge — still six points, still a closed
/// polygon, still different from a rect element, so both "is it a polygon?" and this bead's
/// differs-from-Rect rule pass while the picture stops being a lightning bolt. The same gap the
/// flattened document exposed, in a straight-edged shape.
#[test]
fn the_bolt_waists_are_at_different_heights() {
    let lightning = render("bolt");
    assert!(
        lightning.contains("128.57") && lightning.contains("121.93"),
        "the bolt's two waists collapsed onto one height, making it a wedge: {}",
        silhouette(&lightning)
    );
    assert_ne!(
        silhouette(&lightning),
        silhouette(&render("rect")),
        "the bolt renders identically to a rectangle"
    );
}

/// The lined document's rule sits where MERMAID puts it, not where the lined RECTANGLE's does.
///
/// mermaid widens the document by 2.03 on each side instead of insetting the rule, so the rule lands
/// 0.045 of the width in — against `lin-rect`'s 0.14. Porting the rectangle's ratio by analogy would
/// have placed it three times too far in, and nothing about the resulting picture would look wrong
/// enough to notice; only the measurement says so.
#[test]
fn the_lined_document_rule_is_not_the_lined_rectangle_rule() {
    let lined_document = render("lin-doc");
    let lined_rect = render("lin-rect");
    assert!(
        lined_document.contains("M96.50 92"),
        "the document rule moved off its measured inset: {}",
        silhouette(&lined_document)
    );
    assert!(
        lined_rect.contains("M106 92"),
        "the rectangle rule moved off its measured inset: {}",
        silhouette(&lined_rect)
    );
    assert_ne!(
        silhouette(&lined_document),
        silhouette(&lined_rect),
        "the lined document and lined rectangle render identically"
    );
}

/// And the rule is ADDITIVE: strip it and a `lin-doc` is a `doc`.
#[test]
fn the_lined_document_is_the_plain_one_plus_a_rule() {
    let plain = render("doc");
    let lined = render("lin-doc");
    assert!(
        !plain.contains("M96.50 92 L96.50 154.51"),
        "the plain document grew a rule: {}",
        silhouette(&plain)
    );
    assert_ne!(
        silhouette(&plain),
        silhouette(&lined),
        "the plain and lined documents render identically"
    );
}

/// The document's wave must actually BE a wave, not a flattened bottom.
///
/// This guards something subtler than falling back to `Rect`: a document whose two quadratics were
/// flattened onto the baseline is still a closed path, still contains curve commands, and still
/// differs from a rect element — so "is it a path?" and "does it differ from rect?" both pass. What
/// makes it a document is that the bottom dips BELOW the box and rises ABOVE it.
#[test]
fn the_document_bottom_leaves_the_baseline_in_both_directions() {
    let document = render("doc");
    // The box is y = 92..158.50. The trough control reaches 1.06h (162.49), below the bottom edge;
    // the right corner sits at 0.80h (145.20), above it.
    assert!(
        document.contains("162.49"),
        "the wave never dips below the box, so it is not a document: {}",
        silhouette(&document)
    );
    assert!(
        document.contains("192 145.20"),
        "the right edge does not rise above the bottom, so the wave has no crest: {}",
        silhouette(&document)
    );
    assert_ne!(
        silhouette(&document),
        silhouette(&render("rect")),
        "the document renders identically to a rectangle"
    );
}

/// ⚠️ A FOLD IS NOT A CUT. `tag-rect` keeps its whole rectangle and draws a triangle OVER the
/// bottom-right corner; `notch-rect` REMOVES the top-left corner. Both are "a rectangle with a
/// triangle involved", and a test asserting "there is a diagonal" or counting path commands passes
/// on either — but one loses area mermaid keeps.
#[test]
fn the_tagged_rectangle_folds_where_the_notched_one_cuts() {
    let tagged = render("tag-rect");
    let notched = render("notch-rect");
    assert!(
        tagged.contains("M92 92 L192 92 L192 158.50 L92 158.50 Z"),
        "tag-rect lost its complete box, so the fold became a cut: {}",
        silhouette(&tagged)
    );
    assert!(
        !notched.contains("M92 92 L192 92 L192 158.50 L92 158.50 Z"),
        "notch-rect drew a complete box, so its corner is no longer cut: {}",
        silhouette(&notched)
    );
    assert_ne!(
        silhouette(&tagged),
        silhouette(&notched),
        "the folded and cut rectangles render identically"
    );
}

/// ⚠️ AND THE LINED CYLINDER IS THE PLAIN ONE PLUS EXACTLY ONE ARC. Dropping that trailing rim
/// leaves a `cyl` — same element, same command sequence, one subpath shorter. A test asserting "it
/// is a cylinder" cannot see the difference.
#[test]
fn the_lined_cylinder_is_the_plain_one_plus_a_rim() {
    let plain = render("cyl");
    let lined = render("lin-cyl");
    assert!(
        !plain.contains("M92 105.30 A50 6.65"),
        "the plain cylinder grew a second rim: {}",
        silhouette(&plain)
    );
    assert!(
        lined.contains("M92 105.30 A50 6.65"),
        "the lined cylinder lost its second rim: {}",
        silhouette(&lined)
    );
    assert_ne!(
        silhouette(&plain),
        silhouette(&lined),
        "the plain and lined cylinders render identically"
    );
}

/// ⚠️ THE CYLINDER MUST NOT BE THE VERTICAL ONE ROTATED BY ACCIDENT — or worse, reused outright.
///
/// `cyl` caps TOP and BOTTOM (`A50 6.65`, radii wide-and-flat); `h-cyl` caps LEFT and RIGHT
/// (`A10 33.25`, narrow-and-tall). Both are capsules with the same bounding box and the same command
/// sequence, so a test asserting "it has arcs" or "it is a closed path" passes on either. The radii
/// are what distinguish them.
#[test]
fn the_horizontal_cylinder_is_not_the_vertical_one() {
    let vertical = render("cyl");
    let horizontal = render("h-cyl");
    assert!(
        vertical.contains("A50 6.65"),
        "the vertical cylinder lost its wide flat caps: {}",
        silhouette(&vertical)
    );
    assert!(
        horizontal.contains("A10 33.25"),
        "the horizontal cylinder lost its tall narrow caps: {}",
        silhouette(&horizontal)
    );
    assert_ne!(
        silhouette(&vertical),
        silhouette(&horizontal),
        "the two cylinder orientations render identically"
    );
}

/// ⚠️ THE TWO TRIANGLES POINT OPPOSITE WAYS, and this is the assertion that catches a mirror bug.
///
/// `tri` points UP, `flip-tri` points DOWN. Drawing one for the other keeps the same three vertices
/// and the same bounding box, so a test comparing vertex COUNT, area, or "is it a triangle?" cannot
/// see it — the diagram just renders a `manual-file` marker upside down.
#[test]
fn the_flipped_triangle_is_not_the_upward_one() {
    let up = render("tri");
    let down = render("flip-tri");
    assert!(
        up.contains("M142 92") || up.contains("142 92"),
        "the upward triangle no longer starts at its top apex: {}",
        silhouette(&up)
    );
    assert!(
        down.contains("M92 92 L192 92"),
        "the flipped triangle no longer starts along its top edge: {}",
        silhouette(&down)
    );
    assert_ne!(
        silhouette(&up),
        silhouette(&down),
        "the upward and downward triangles render identically"
    );
}

/// ⚠️ AND THE TWO NOTCHED SHAPES CUT DIFFERENT CORNERS. `notch-rect` cuts ONE (top-left, at 45°);
/// `notch-pent` cuts BOTH top corners at a shallower angle. Two published names, two silhouettes.
#[test]
fn the_notched_rectangle_and_pentagon_are_different_shapes() {
    let rect_notch = silhouette(&render("notch-rect"));
    let pent = silhouette(&render("notch-pent"));
    assert_ne!(
        rect_notch, pent,
        "the one-corner and two-corner notches render identically"
    );
}

/// ⚠️ THE TWO RULED RECTANGLES MUST NOT COLLAPSE INTO EACH OTHER. `lin-rect` rules VERTICALLY near
/// the left; `div-rect` rules HORIZONTALLY near the top. mermaid publishes both and they read
/// differently, so an implementation that drew one rule for both would satisfy every "is there a
/// rule?" assertion while making two published names draw one picture.
#[test]
fn the_vertical_and_horizontal_rules_are_different_shapes() {
    let lined = render("lin-rect");
    let divided = render("div-rect");
    assert!(
        lined.contains("M106 92 L106 158.50"),
        "lin-rect lost its VERTICAL rule"
    );
    assert!(
        divided.contains("M92 103.08 L192 103.08"),
        "div-rect lost its HORIZONTAL rule"
    );
    assert_ne!(
        silhouette(&lined),
        silhouette(&divided),
        "the vertically and horizontally ruled rectangles render identically"
    );
}

/// ⚠️ AND THE FRAMED CIRCLE IS NOT THE DOUBLE CIRCLE. `fr-circ` is a fixed-radius terminal marker
/// (7 and 2.5); `(((x)))` sizes both rings to its label. Collapsing them would make a marker grow
/// with its text, which is the bd-vfxu failure in the other direction.
#[test]
fn the_framed_circle_is_not_the_label_sized_double_circle() {
    let framed = render("fr-circ");
    let double = fm_render_svg::render_svg(&fm_parser::parse("flowchart TD\n  A(((Double)))\n").ir);
    assert!(
        framed.contains("r=\"7\"") && framed.contains("r=\"2.50\""),
        "fr-circ lost a fixed ring"
    );
    assert!(
        !double.contains("r=\"2.50\""),
        "the label-sized double circle picked up the framed circle's fixed inner radius"
    );
}

/// ⚠️ THE NEGATIVE CASE A WRONG IMPLEMENTATION FAILS.
///
/// The bug being fixed is not "no shape" — it is a SILENT FALLBACK to the default rectangle, which
/// renders a perfectly ordinary box. Every "does it parse?", "did we get a shape?" and "is there an
/// element?" assertion passes on that. Each new shape must differ from `rect` AND from the others,
/// or three names collapse onto one picture (bd-vfxu, where two declared shapes rendered
/// byte-identical geometry and the test asserting "a circle exists" saw nothing wrong).
#[test]
fn the_new_shapes_differ_from_a_rectangle_and_from_each_other() {
    let rect = silhouette(&render("rect"));
    assert!(!rect.is_empty(), "the control drew nothing");

    let mut seen = vec![("rect", rect)];
    for shape in [
        "notch-rect",
        "lin-rect",
        "sm-circ",
        "div-rect",
        "fr-circ",
        "flip-tri",
        "notch-pent",
        "sl-rect",
        "h-cyl",
        "tag-rect",
        "lin-cyl",
        "doc",
        "lin-doc",
        "bolt",
        "flag",
    ] {
        let sig = silhouette(&render(shape));
        assert!(!sig.is_empty(), "{shape} drew no geometry at all");
        for (other, other_sig) in &seen {
            assert_ne!(
                &sig, other_sig,
                "`{shape}` renders identically to `{other}`, so the two names draw one picture"
            );
        }
        seen.push((shape, sig));
    }
}

/// Every alias the registry publishes resolves to the same drawing as its short name. A registry
/// name that resolves to nothing tells an author to fix a spelling that was already correct.
#[test]
fn every_published_alias_draws_the_same_shape() {
    let groups: [(&str, &[&str]); 15] = [
        ("notch-rect", &["card", "notched-rectangle"]),
        (
            "lin-rect",
            &[
                "lined-rectangle",
                "lined-process",
                "lin-proc",
                "shaded-process",
            ],
        ),
        ("sm-circ", &["small-circle", "start"]),
        (
            "div-rect",
            &["div-proc", "divided-rectangle", "divided-process"],
        ),
        ("fr-circ", &["framed-circle", "stop"]),
        ("flip-tri", &["flipped-triangle", "manual-file"]),
        ("notch-pent", &["notched-pentagon", "loop-limit"]),
        ("sl-rect", &["sloped-rectangle", "manual-input"]),
        ("h-cyl", &["horizontal-cylinder", "das"]),
        (
            "tag-rect",
            &["tagged-rectangle", "tag-proc", "tagged-process"],
        ),
        ("lin-cyl", &["lined-cylinder", "disk"]),
        ("doc", &["document"]),
        ("lin-doc", &["lined-document"]),
        ("bolt", &["lightning-bolt", "com-link"]),
        ("flag", &["paper-tape"]),
    ];
    for (short, aliases) in groups {
        let expected = silhouette(&render(short));
        for alias in aliases {
            assert_eq!(
                silhouette(&render(alias)),
                expected,
                "alias `{alias}` does not draw the same shape as `{short}`"
            );
        }
    }
}

/// ⚠️ AND THE WARNING MUST STOP FOR THESE NAMES ONLY.
///
/// Eleven names left `UNIMPLEMENTED_UPSTREAM_SHAPES`. That edit could as easily have emptied the
/// list, trading a wrong shape for a SILENT one — which is worse, and is the property bd-xfmm spent
/// a bead establishing. A still-unimplemented name must still warn, and a typo must still be called
/// a typo.
#[test]
fn implemented_names_stop_warning_and_others_do_not() {
    let warnings = |shape: &str| {
        fm_parser::parse(&format!("flowchart TD\n  A@{{ shape: {shape} }}\n")).warnings
    };
    for name in [
        "notch-rect",
        "card",
        "lin-rect",
        "shaded-process",
        "sm-circ",
        "start",
        "div-rect",
        "divided-process",
        "fr-circ",
        "stop",
        "flip-tri",
        "manual-file",
        "notch-pent",
        "loop-limit",
        "sl-rect",
        "manual-input",
        "h-cyl",
        "das",
        "tag-rect",
        "tagged-process",
        "lin-cyl",
        "disk",
        "doc",
        "document",
        "lin-doc",
        "lined-document",
        "bolt",
        "com-link",
        "flag",
        "paper-tape",
    ] {
        assert!(
            warnings(name).is_empty(),
            "`{name}` is implemented now and must not warn: {:?}",
            warnings(name)
        );
    }
    // `doc` left this list by being implemented; `bang` replaces it so the list keeps its size.
    // ⚠️ ANCHORED ON THE TWO CONFIRMED NON-SHAPES, deliberately. `win-pane` and `datastore` are
    // names mermaid 11.15.0 publishes and draws as a PLAIN RECTANGLE — measured, recorded on
    // bd-7ls21, and therefore never going to be implemented here. Every other name in this list has
    // had to be swapped out the moment someone implemented it, three lists at a time; these two
    // cannot be. The third entry rotates and is expected to churn.
    for name in ["win-pane", "datastore", "hourglass", "bang"] {
        assert!(
            !warnings(name).is_empty(),
            "`{name}` is still unimplemented and must still warn"
        );
    }
    assert!(
        !warnings("definitely-not-a-shape").is_empty(),
        "a nonsense name produced no diagnostic at all"
    );
}

/// CONTROL: the small circle's radius does NOT grow with its label. It is a start marker; one that
/// scaled with text would stop reading as a marker, which is why the radius is a constant rather
/// than derived from the node box.
#[test]
fn the_small_circle_ignores_its_label_width() {
    let short = render("sm-circ");
    let long = fm_render_svg::render_svg(
        &fm_parser::parse(
            "flowchart TD\n  A@{ shape: sm-circ, label: \"a considerably longer label\" }\n",
        )
        .ir,
    );
    assert!(
        short.contains("r=\"7\"") && long.contains("r=\"7\""),
        "the small circle scaled with its label"
    );
}

/// CONTROL: the accessible description names the shape a reader sees. `notch-rect` spoken aloud
/// means nothing; a screen-reader user gets the shape's identity from this string alone.
#[test]
fn each_new_shape_has_an_accessible_description() {
    for (shape, want) in [
        ("notch-rect", "notched rectangle"),
        ("lin-rect", "lined rectangle"),
        ("sm-circ", "small circle"),
        ("div-rect", "divided rectangle"),
        ("fr-circ", "framed circle"),
        ("flip-tri", "downward triangle"),
        ("notch-pent", "notched pentagon"),
        ("sl-rect", "sloped rectangle"),
        ("h-cyl", "horizontal cylinder"),
        ("tag-rect", "tagged rectangle"),
        ("lin-cyl", "lined cylinder"),
        ("doc", "document"),
        ("lin-doc", "lined document"),
        ("bolt", "lightning bolt"),
        ("flag", "flag"),
    ] {
        assert!(
            render(shape).contains(want),
            "`{shape}` has no accessible description mentioning {want:?}"
        );
    }
}
