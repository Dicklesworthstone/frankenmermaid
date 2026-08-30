//! The UML lollipop reaches the SVG as a declared, referenced, hollow marker (bd-lkm9i).
//!
//! The parser-side proof lives in `fm-parser/tests/class_lollipop_interface.rs`: `A ()-- B` is
//! `ArrowType::Lollipop` and not the bare `--` it used to collapse into. That says nothing about
//! whether anything is DRAWN, and the defect being fixed was precisely that a declared relationship
//! form rendered as an anonymous line — a parser-only fix would leave the picture unchanged.
//!
//! REFERENCE BEHAVIOUR, read from the pinned mermaid 11.15.0 bundle rather than guessed:
//!
//! ```text
//!   <marker id="…-lollipopStart" class="marker lollipop …"
//!           refX=13 refY=7 markerWidth=190 markerHeight=240 orient="auto">
//!     <circle fill="transparent" cx="7" cy="7" r="6"/>
//!   <marker id="…-lollipopEnd"   … refX=1 …>   (same circle)
//! ```
//!
//! ⚠️ TWO DEFS FOR ONE SHAPE, which looks like duplication and is not. A circle IS symmetric under
//! rotation, so unlike the ER crow's feet the two ends do not need mirrored geometry — they need
//! different ANCHORS. mermaid puts the socket outside the endpoint on whichever end provides the
//! interface: refX 13 at the source, refX 1 at the target. One def cannot serve both.
//!
//! ⚠️ AND IT IS NOT THE FLOWCHART CIRCLE. `--o` draws a FILLED r=5 circle anchored at its centre;
//! a lollipop is `fill="transparent"` r=6. Reusing the filled marker would draw a ball where the
//! socket belongs, which is the same "two declared forms, one picture" defect in a new place.

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

/// The `<marker id="…">` ids this document DECLARES.
fn declared_marker_ids(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in svg.split("<marker ").skip(1) {
        let tag = &chunk[..chunk.find('>').unwrap_or(chunk.len())];
        if let Some(at) = tag.find("id=\"") {
            let start = at + "id=\"".len();
            if let Some(end) = tag[start..].find('"') {
                out.push(tag[start..start + end].to_string());
            }
        }
    }
    out
}

/// The marker ids this document REFERENCES via `url(#…)`.
fn referenced_marker_ids(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in svg.split("url(#").skip(1) {
        if let Some(end) = chunk.find(')') {
            out.push(chunk[..end].to_string());
        }
    }
    out
}

/// ⚠️ REFERENCED **AND** DECLARED. Either half alone is a silent no-op.
///
/// A `marker-start="url(#start-arrow-lollipop)"` pointing at a def that was never emitted draws
/// NOTHING — the edge renders exactly as the plain line this bead exists to distinguish it from, and
/// every parser-level assertion still passes. The reverse (a def nothing points at) is dead weight
/// that ships in every diagram. This asserts both directions for the id actually used.
#[test]
fn the_source_socket_is_both_referenced_and_declared() {
    let svg = render("classDiagram\n  A ()-- B\n");
    let referenced = referenced_marker_ids(&svg);
    let declared = declared_marker_ids(&svg);

    assert!(
        referenced.iter().any(|id| id == "start-arrow-lollipop"),
        "no edge points at the lollipop marker; refs = {referenced:?}"
    );
    assert!(
        declared.iter().any(|id| id == "start-arrow-lollipop"),
        "the lollipop marker is referenced but never declared, so nothing is drawn; \
         declared = {declared:?}"
    );
}

/// The target spelling uses the OTHER def, not the same one.
#[test]
fn the_target_socket_uses_the_end_anchored_def() {
    let svg = render("classDiagram\n  A --() B\n");
    let referenced = referenced_marker_ids(&svg);

    assert!(
        referenced.iter().any(|id| id == "arrow-lollipop"),
        "`--()` does not reference the end-anchored lollipop; refs = {referenced:?}"
    );
    assert!(
        declared_marker_ids(&svg)
            .iter()
            .any(|id| id == "arrow-lollipop"),
        "the end-anchored lollipop is referenced but never declared"
    );
    assert!(
        !referenced.iter().any(|id| id == "start-arrow-lollipop"),
        "`--()` put the socket on the source end"
    );
}

/// ⚠️ THE NEGATIVE CASE: the socket must be HOLLOW, or it is the filled terminator wearing a new id.
///
/// Asserting only that a marker with the right id exists would pass on a def that draws a filled
/// disc, which is the specific wrong picture this marker was added to avoid.
#[test]
fn the_socket_is_hollow_and_not_the_filled_circle() {
    let svg = render("classDiagram\n  A ()-- B\n");
    let def = svg
        .split("<marker ")
        .find(|chunk| chunk.starts_with("id=\"start-arrow-lollipop\""))
        .unwrap_or_else(|| panic!("no start-arrow-lollipop def in:\n{svg}"));
    let def = &def[..def.find("</marker>").unwrap_or(def.len())];

    assert!(
        def.contains("fill=\"none\""),
        "the lollipop socket is filled, so it draws a ball rather than a socket: {def}"
    );
    assert!(
        def.contains("stroke="),
        "the lollipop socket has no stroke, so a hollow marker draws nothing at all: {def}"
    );
}

/// A lollipop edge does not render byte-identically to the plain link it used to collapse into.
///
/// This is the whole defect stated at the level that matters — the OUTPUT — and it is the assertion
/// a parser-only fix cannot satisfy.
#[test]
fn a_lollipop_edge_does_not_render_as_a_plain_link() {
    let socket = render("classDiagram\n  A ()-- B\n");
    let plain = render("classDiagram\n  A -- B\n");
    assert_ne!(
        socket, plain,
        "`A ()-- B` still renders exactly as `A -- B`"
    );
}

/// The dotted spelling keeps the socket AND gains the dash, so all four forms stay distinguishable.
#[test]
fn the_four_spellings_render_four_different_pictures() {
    let rendered: Vec<String> = ["()--", "--()", "()..", "..()"]
        .iter()
        .map(|op| render(&format!("classDiagram\n  A {op} B\n")))
        .collect();

    for (i, a) in rendered.iter().enumerate() {
        for (j, b) in rendered.iter().enumerate().skip(i + 1) {
            assert_ne!(a, b, "spellings {i} and {j} render identically");
        }
    }
}
