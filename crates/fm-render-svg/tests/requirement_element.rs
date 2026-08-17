//! A requirement-diagram ELEMENT's declared fields must reach the output (bd-qdmn).
//!
//! Found by the third widening of `renderer_agreement.rs`, and it is not a backend disagreement:
//! the SVG is that gate's REFERENCE renderer and it does not draw the text either, because the
//! content never reaches the IR. `IrRequirementNodeMeta` carries exactly five fields —
//! `requirement_type`, `req_id`, `text`, `risk`, `verify_method` — and an `element` block declares
//! `type:` and `docRef:`, neither of which has anywhere to go.
//!
//! Same family as bd-jerh and bd-ekx2: declared content with no home in the IR. A renderer-vs-
//! renderer gate is blind to it by construction, since all three agree — on drawing nothing.

/// CONTROL, and it must pass TODAY. A requirement's own rows DO render, so the reproducer below
/// fails because the ELEMENT path drops its fields and not because requirement diagrams are broken
/// or the fixture is malformed. Without this, a later "fix" would be validated against nothing.
#[test]
fn a_requirement_still_draws_its_declared_rows() {
    let svg = fm_render_svg::render_svg(
        &fm_parser::parse(
            "requirementDiagram\n  requirement R {\n  id: 1\n  text: hello\n  risk: high\n  }\n",
        )
        .ir,
    );

    assert!(svg.contains("hello"), "the requirement's text row is missing:\n{svg}");
    assert!(svg.contains("high"), "the requirement's risk row is missing:\n{svg}");
}

/// NON-VACUITY for the reproducer: the element must at least become a NODE, so that what the
/// reproducer detects is a dropped FIELD and not a dropped element.
#[test]
fn a_requirement_element_reaches_the_diagram_as_a_node() {
    let ir = fm_parser::parse("requirementDiagram\n  element E {\n  type: simulation\n  }\n").ir;

    assert!(
        ir.nodes.iter().any(|node| node.id.contains('E')),
        "the element itself never became a node, so the reproducer would be testing the wrong \
         thing: {:?}",
        ir.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
    );
}

/// ACCEPTANCE GATE for bd-qdmn.
///
/// ⚠️ `#[ignore]` BECAUSE IT REPRODUCES A LIVE DEFECT, not because it is unfinished. Un-ignoring it
/// is how bd-qdmn closes. Measured today: the SVG does not contain `simulation` at all.
///
/// Fixing it means adding the two fields to `IrRequirementNodeMeta`, populating them on the element
/// parse path, and drawing them in the three renderers as the requirement rows already are. Note
/// that several SVG fast paths gate on `requirement_meta.is_none()`, so an element that suddenly
/// HAS meta may take a different path — check those rather than assuming.
#[test]
#[ignore = "bd-qdmn: a requirement element's type:/docRef: have no IR field, so nothing can draw them"]
fn a_requirement_element_draws_its_declared_type() {
    let svg = fm_render_svg::render_svg(
        &fm_parser::parse("requirementDiagram\n  element E {\n  type: simulation\n  }\n").ir,
    );

    assert!(
        svg.contains("simulation"),
        "the element's declared type never reached the SVG:\n{svg}"
    );
}
