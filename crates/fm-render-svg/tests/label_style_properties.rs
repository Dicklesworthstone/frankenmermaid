//! Every property mermaid treats as a LABEL style must reach the label (bd-jyg4s).
//!
//! REFERENCE, transcribed from the pinned mermaid 11.15.0 bundle. `styles2String` splits a `style`
//! declaration list with `isLabelStyle`, applying what it accepts to the label and everything else
//! to the shape:
//!
//! ```text
//! isLabelStyle = e => e === "color" || e === "font-size" || e === "font-family"
//!   || e === "font-weight" || e === "font-style" || e === "text-decoration"
//!   || e === "text-align" || e === "text-transform" || e === "line-height"
//!   || e === "letter-spacing" || e === "word-spacing" || e === "text-shadow"
//!   || e === "text-overflow" || e === "white-space" || e === "word-wrap"
//!   || e === "word-break" || e === "overflow-wrap" || e === "hyphens"
//! ```
//!
//! EIGHTEEN properties. Our security allowlist accepted SIX of them, so `style a
//! letter-spacing:3px` was discarded before the splitter ever ran and produced no styling at all —
//! not on the label, not on the shape, nowhere. bd-xfmm had just fixed the splitter itself; this is
//! the half that gives it something to split.
//!
//! ⚠️ TWO COPIES OF ONE LIST IS WHAT CAUSED THIS. fm-render-svg carried its own six-entry
//! `TEXT_STYLE_PROPERTIES` beside fm-core's allowlist, and neither knew the other existed. They are
//! one list now, and `the_allowlist_admits_every_label_property` fails if they diverge again.

/// The opening tag of the first `tag` element whose attributes contain `needle`.
fn element_containing<'a>(svg: &'a str, tag: &str, needle: &str) -> &'a str {
    let mut rest = svg;
    while let Some(start) = rest.find(tag) {
        rest = &rest[start..];
        let end = rest.find('>').expect("unterminated element") + 1;
        let element = &rest[..end];
        if element.contains(needle) {
            return element;
        }
        rest = &rest[end..];
    }
    panic!("no {tag} element containing {needle} in:\n{svg}");
}

/// The opening tag of the `<text>` whose CONTENT is `content`.
///
/// Separate from `element_containing` because the label is identified by what it DRAWS, not by an
/// attribute — searching the opening tag for `>Alpha<` finds nothing, since that text lives after
/// the tag closes.
fn text_element_drawing<'a>(svg: &'a str, content: &str) -> &'a str {
    let mut rest = svg;
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let open_end = rest.find('>').expect("unterminated <text>") + 1;
        let close = rest.find("</text>").expect("unterminated <text>");
        if rest[open_end..close].trim() == content {
            return &rest[..open_end];
        }
        rest = &rest[close + "</text>".len()..];
    }
    panic!("no <text> drawing {content:?} in:\n{svg}");
}

/// The value of an element's `style` attribute, or `""` when it has none.
fn style_attribute(element: &str) -> &str {
    let Some(start) = element.find(" style=\"") else {
        return "";
    };
    let rest = &element[start + " style=\"".len()..];
    let end = rest.find('"').unwrap_or(rest.len());
    &rest[..end]
}

/// A one-node flowchart with `property:value` declared on the node.
fn render_with(property: &str, value: &str) -> String {
    let source = format!("flowchart TD\n  a[Alpha]-->b[Beta]\n  style a {property}:{value}\n");
    fm_render_svg::render_svg(&fm_parser::parse(&source).ir)
}

/// A representative value per property — every one legal CSS for it, so a rejection is about the
/// PROPERTY and never about the value being malformed.
const CASES: &[(&str, &str)] = &[
    ("color", "#123456"),
    ("font-size", "20px"),
    ("font-family", "serif"),
    ("font-weight", "bold"),
    ("font-style", "italic"),
    ("text-decoration", "underline"),
    ("text-align", "center"),
    ("text-transform", "uppercase"),
    ("line-height", "2"),
    ("letter-spacing", "3px"),
    ("word-spacing", "4px"),
    ("text-shadow", "1px 1px red"),
    ("text-overflow", "ellipsis"),
    ("white-space", "nowrap"),
    ("word-wrap", "break-word"),
    ("word-break", "break-all"),
    ("overflow-wrap", "anywhere"),
    ("hyphens", "auto"),
];

/// THE LIST-CONSISTENCY GUARD. A label property the allowlist refuses is dropped before the split,
/// which is the defect this bead is about — and it is invisible from either list alone.
#[test]
fn the_allowlist_admits_every_label_property() {
    for property in fm_core::MERMAID_LABEL_STYLE_PROPERTIES {
        assert!(
            fm_core::is_allowed_style_property(property),
            "{property:?} is a mermaid label style but the security allowlist refuses it, so it is \
             discarded before the label/shape split ever sees it"
        );
    }
    assert_eq!(
        fm_core::MERMAID_LABEL_STYLE_PROPERTIES.len(),
        18,
        "mermaid's isLabelStyle names eighteen properties; this list has drifted from the bundle"
    );
    assert_eq!(
        CASES.len(),
        fm_core::MERMAID_LABEL_STYLE_PROPERTIES.len(),
        "every label property needs a rendering case, or this file certifies only some of them"
    );
}

#[test]
fn every_label_property_reaches_the_label() {
    for (property, value) in CASES {
        let svg = render_with(property, value);
        let label = text_element_drawing(&svg, "Alpha");
        // `color` is the one that changes name: SVG text has no `color` presentation attribute, so
        // the splitter maps it to `fill`.
        let expected = if *property == "color" {
            format!("fill:{value}")
        } else {
            format!("{property}:{value}")
        };
        assert!(
            label.contains(&expected),
            "{property}:{value} never reached the label; drew {label}"
        );
    }
}

/// ⚠️ THE NEGATIVE HALF, and the one a wrong implementation fails.
///
/// Adding these to the security allowlist WITHOUT adding them to the label list admits them and
/// then puts them on the `<rect>`, where a text-layout property does nothing — the same dead
/// declaration bd-xfmm removed for `color`. Every assertion in `every_label_property_reaches_the_label`
/// is about the label and none of them would notice.
#[test]
fn no_label_property_lands_on_the_shape() {
    for (property, value) in CASES {
        let svg = render_with(property, value);
        let shape = element_containing(&svg, "<rect", "fm-node");
        assert!(
            !shape.contains(&format!("{property}:")),
            "{property} is a LABEL style and does nothing on a shape, but the rect carries it: \
             {shape}"
        );
    }
}

/// CONTROL: a shape property must still go to the shape. Without this, an implementation that
/// routed EVERY declaration to the label would pass both tests above.
#[test]
fn a_shape_property_still_reaches_the_shape() {
    let svg = render_with("stroke-width", "4px");
    let shape = element_containing(&svg, "<rect", "fm-node");
    assert!(
        shape.contains("stroke-width:4px"),
        "a shape property stopped reaching the shape: {shape}"
    );
    let label = text_element_drawing(&svg, "Alpha");
    assert!(
        !label.contains("stroke-width:"),
        "a shape property leaked onto the label: {label}"
    );
}

/// CONTROL ON THE SANITIZER: widening the property allowlist must not widen VALUE screening.
///
/// `sanitize_style_value` is property-independent — it rejects `url(`, CSS comment markers,
/// `javascript:`, event-handler names, `<`/`>`, `{`/`}`, backslashes, control characters and
/// `expression(` whatever property carries them. This proves that held for the newly-admitted
/// properties rather than assuming it.
///
/// ⚠️ ASSERTED ON THE STYLED ELEMENTS, NOT ON THE DOCUMENT. `url(` appears in every diagram this
/// renderer produces — `fill="url(#fm-node-gradient)"` is its own gradient reference — so a
/// document-wide search for it fails on correct output and proves nothing about the injected value.
/// The question is whether the ATTACKER'S value survived, so the attacker's value is what is looked
/// for, on the two elements it could have reached.
#[test]
fn the_new_properties_do_not_admit_dangerous_values() {
    for property in ["letter-spacing", "text-shadow", "white-space", "hyphens"] {
        for value in [
            "url(javascript:alert(1))",
            "red;} body{display:none",
            "expression(alert(1))",
            "<script>",
        ] {
            let source =
                format!("flowchart TD\n  a[Alpha]-->b[Beta]\n  style a {property}:{value}\n");
            let svg = fm_render_svg::render_svg(&fm_parser::parse(&source).ir);
            let label = text_element_drawing(&svg, "Alpha");
            let shape = element_containing(&svg, "<rect", "fm-node");
            for element in [label, shape] {
                // The `style` attribute is the only thing a `style` directive can reach, and it is
                // the only place these markers would be an injection. `fill="url(#fm-node-gradient)"`
                // is the renderer's own gradient reference and lives on the same element.
                let styled = style_attribute(element);
                assert!(
                    !styled.contains(value),
                    "{property}:{value} survived sanitisation into style={styled:?}"
                );
                for marker in ["javascript:", "expression(", "<script", "url("] {
                    assert!(
                        !styled.contains(marker),
                        "{property}:{value} let {marker:?} through into style={styled:?}"
                    );
                }
            }
        }
    }
}
