//! `info`: the last family from the registry sweep (bd-a3tmn).
//!
//! Reading the pinned mermaid 11.15.0 bundle's own diagram registry against our `DiagramType`
//! turned up three families it ships and we did not: `treemap` (bd-9ghyo), `radar` (bd-sk4dv) and
//! `info`. This is the third and smallest.
//!
//! REFERENCE, measured in Chromium 151: the whole diagram is ONE text run — `v11.15.0`, the
//! renderer's own version, at (100, 40) in 32px centred type with class `version`. The legacy
//! `showInfo` keyword renders identically.
//!
//! ⚠️ THE VERSION IS OURS, NOT MERMAID'S. The diagram's entire purpose is "tell me what rendered
//! this", so echoing `v11.15.0` would make every frankenmermaid render claim to be mermaid — the
//! one answer this diagram must not give. That is why the assertions below are about OUR crate
//! version rather than about a literal.
//!
//! THE NEGATIVE CASE, in this bead's shape: the diagram must render DIFFERENTLY from the fallback
//! it used to collapse into. For a node shape that fallback is `Rect`; here it is the low-confidence
//! flowchart `info` was detected as before.

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

/// THE NEGATIVE CASE: an `info` diagram no longer renders as the flowchart it used to be.
#[test]
fn an_info_diagram_does_not_render_as_the_flowchart_it_used_to_be() {
    let info = render("info\n");
    let as_flowchart = render("flowchart TD\n  info\n");
    assert_ne!(
        info, as_flowchart,
        "an info diagram renders identically to the flowchart fallback"
    );
    assert!(
        !info.contains("fm-node-shape-"),
        "an info diagram drew a graph node shape: its header became a node"
    );
    assert!(
        info.contains("fm-info-version"),
        "no version text was drawn:\n{info}"
    );
}

/// It is detected as its own type, at high confidence, and warns about nothing.
#[test]
fn info_is_detected_as_its_own_type() {
    for source in ["info\n", "info\nshowInfo\n"] {
        let result = fm_parser::parse(source);
        assert_eq!(
            result.ir.diagram_type,
            fm_core::DiagramType::Info,
            "not detected as info: {source:?}"
        );
        assert!(
            result.warnings.is_empty(),
            "a valid info diagram warned: {:?}",
            result.warnings
        );
    }
}

/// ⚠️ THE VERSION IS RESOLVED, NOT HARDCODED.
///
/// Comparing against `env!("CARGO_PKG_VERSION")` — the same crate the renderer lives in — is what
/// makes this an assertion rather than a restatement: a literal baked into the renderer passes a
/// `contains("v0.2.0")` check today and silently lies at the next release.
#[test]
fn the_reported_version_tracks_the_crate_version() {
    let info = render("info\n");
    let expected = format!(">v{}<", env!("CARGO_PKG_VERSION"));
    assert!(
        info.contains(&expected),
        "expected the drawn text to be {expected:?}:\n{info}"
    );
}

/// ⚠️ AND IT IS NOT MERMAID'S VERSION.
///
/// The half that matters most. `info` exists to identify the renderer, so reporting the number of
/// the engine we are compared against would make every render of this diagram a false claim —
/// and it is exactly the value a copy-the-reference implementation would produce.
#[test]
fn the_reported_version_is_not_the_incumbents() {
    let info = render("info\n");
    assert!(
        !info.contains("11.15.0"),
        "the info diagram reports mermaid's version as its own:\n{info}"
    );
}

/// A stray line inside an `info` document is named rather than silently swallowed.
#[test]
fn an_unexpected_line_is_reported() {
    let result = fm_parser::parse("info\n  wat\n");
    assert_eq!(result.ir.diagram_type, fm_core::DiagramType::Info);
    assert!(
        result.warnings.iter().any(|w| w.contains("wat")),
        "an unrecognised line was swallowed: {:?}",
        result.warnings
    );
}

/// `info` is no longer named as an unimplemented upstream type.
///
/// The converse guard the treemap work introduced, applied to this family: a list meaning "your
/// syntax is right, we have not built this" must not still hold a type we render.
#[test]
fn info_is_no_longer_reported_as_unimplemented() {
    let detected = fm_parser::detect_type_with_confidence("info\n");
    assert!(
        !detected
            .warnings
            .iter()
            .any(|w| w.contains("does not implement")),
        "info is still named as unimplemented: {:?}",
        detected.warnings
    );
}

/// Other diagram types are untouched.
#[test]
fn other_diagram_types_draw_no_version_text() {
    for source in ["flowchart LR\n  A --> B\n", "classDiagram\n  class A\n"] {
        let svg = render(source);
        assert!(
            !svg.contains("fm-info-version"),
            "a non-info diagram drew the version text: {source:?}"
        );
    }
}
