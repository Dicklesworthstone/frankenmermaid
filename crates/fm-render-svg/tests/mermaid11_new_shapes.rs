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

/// The first shape element drawn for the node, with paint stripped.
fn silhouette(svg: &str) -> String {
    for tag in ["<path ", "<circle ", "<rect ", "<polygon "] {
        if let Some(at) = svg.find(tag) {
            let rest = &svg[at..];
            let end = rest.find('>').unwrap_or(rest.len());
            let raw = &rest[..end];
            if raw.contains("fm-node-gradient") || tag == "<path " || tag == "<polygon " {
                // Keep geometry attributes only; fills/strokes are theme, not shape.
                return raw
                    .split_whitespace()
                    .filter(|part| {
                        part.starts_with("d=")
                            || part.starts_with("points=")
                            || part.starts_with("r=")
                            || part.starts_with("width=")
                            || part.starts_with("height=")
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
            }
        }
    }
    String::new()
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
    for shape in ["notch-rect", "lin-rect", "sm-circ"] {
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
    let groups: [(&str, &[&str]); 3] = [
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
    ] {
        assert!(
            warnings(name).is_empty(),
            "`{name}` is implemented now and must not warn: {:?}",
            warnings(name)
        );
    }
    for name in ["hourglass", "brace", "bolt", "doc"] {
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
    ] {
        assert!(
            render(shape).contains(want),
            "`{shape}` has no accessible description mentioning {want:?}"
        );
    }
}
