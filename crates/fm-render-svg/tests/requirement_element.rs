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
//!
//! ⚠️ AN INTEGRATION TEST DOES NOT COMPILE THE CRATE'S OWN `#[cfg(test)]` MODULE, and that cost a
//! red main. Adding two fields to `IrRequirementNodeMeta` broke two EXHAUSTIVE struct literals that
//! live in unit-test modules — `fm-layout/src/lib.rs` and `fm-render-svg/src/lib.rs` — while
//! `cargo test -p fm-render-svg --test requirement_element` reported 5 passed, because `--test
//! <name>` builds the lib for the INTEGRATION target and never the lib-test target that holds those
//! literals. The green was real and it covered less than it looked like it did.
//!
//! Adding a field to a shared IR struct is a WORKSPACE-WIDE change: run `cargo test --workspace`,
//! or at minimum `cargo check --workspace --all-targets`, before believing a targeted pass.

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

    assert!(
        svg.contains("hello"),
        "the requirement's text row is missing:\n{svg}"
    );
    assert!(
        svg.contains("high"),
        "the requirement's risk row is missing:\n{svg}"
    );
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

/// ACCEPTANCE GATE for bd-qdmn. Was `#[ignore]`d while the field had no IR home at all.
#[test]
fn a_requirement_element_draws_its_declared_type() {
    let svg = fm_render_svg::render_svg(
        &fm_parser::parse("requirementDiagram\n  element E {\n  type: simulation\n  }\n").ir,
    );

    assert!(
        svg.contains("simulation"),
        "the element's declared type never reached the SVG:\n{svg}"
    );
}

/// `docRef:` is spelled camelCase in mermaid's grammar, and this parser matches the RAW field text.
/// A lowercase-only arm would compile, pass every other test here, and silently keep dropping the
/// spelling authors actually write — so both casings are asserted.
#[test]
fn a_requirement_element_draws_its_doc_ref_in_either_casing() {
    for source in [
        "requirementDiagram\n  element E {\n  type: simulation\n  docRef: ./spec.md\n  }\n",
        "requirementDiagram\n  element E {\n  type: simulation\n  docref: ./spec.md\n  }\n",
    ] {
        let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
        assert!(
            svg.contains("./spec.md"),
            "the element's docRef never reached the SVG for:\n{source}\n{svg}"
        );
    }
}

/// NEGATIVE CASE, and it is the one a naive fix fails. `requirement_type` is the KEYWORD a
/// requirement was declared with (`requirement`, `functionalRequirement`, …); `element_type` is a
/// value the author writes inside an `element` block. Wiring `type:` into the existing
/// `requirement_type` field would satisfy the gate above and CORRUPT every requirement's declared
/// category — so a plain requirement must still report its keyword, and must not gain a Type row it
/// never declared.
#[test]
fn a_requirement_keyword_is_not_overwritten_by_the_element_type_field() {
    let ir = fm_parser::parse(
        "requirementDiagram\n  functionalRequirement R {\n  id: 1\n  text: hello\n  }\n",
    )
    .ir;

    let meta = ir
        .nodes
        .iter()
        .find_map(|node| node.requirement_meta.as_deref())
        .expect("the requirement must carry meta");

    assert_eq!(
        meta.requirement_type.as_deref(),
        Some("functionalRequirement"),
        "the declared keyword was lost or overwritten"
    );
    assert!(
        meta.element_type.is_none(),
        "a requirement that declared no `type:` gained one: {:?}",
        meta.element_type
    );
}
