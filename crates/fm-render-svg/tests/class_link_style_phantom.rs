//! `linkStyle 0 stroke:#f00` in a class diagram must not be drawn as a class (bd-9gmvp).
//!
//! It was: the SVG contained a class box captioned with the author's own directive, and the
//! diagram gained a node nobody declared. `is_class_non_node_statement` lists `style`, `classDef`,
//! `cssClass`, `click`, `link`, `callback` and `note` — but not `linkStyle`. `keyword("link")` does
//! not cover it, because that helper requires the token to stand alone (which is what keeps a class
//! named `linkage` safe), and that is exactly why the omission stayed invisible: the list already
//! looked like it handled the `link` family.
//!
//! The SHARED sibling predicate, `is_non_node_directive_statement`, has covered `linkStyle` all
//! along — the same asymmetry bd-yfcfv found between two predicates meant to say the same thing.
//!
//! Latest of the phantom family: bd-871ka, bd-xfmm, bd-yrxu, bd-6r13, bd-t2fp, bd-0audg, bd-yfcfv,
//! bd-vc1zp.

fn text_runs(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        rest = &rest[open_end + 1..];
        let Some(close) = rest.find("</text>") else {
            break;
        };
        out.push(rest[..close].to_string());
        rest = &rest[close + "</text>".len()..];
    }
    out
}

fn ids(ir: &fm_core::MermaidDiagramIr) -> Vec<&str> {
    ir.nodes.iter().map(|node| node.id.as_str()).collect()
}

/// THE DEFECT: the directive is neither declared nor drawn, and the edge it styles survives.
#[test]
fn a_class_diagram_link_style_directive_is_not_drawn_as_a_class() {
    let source = "classDiagram\n  class A\n  class B\n  A --> B\n  linkStyle 0 stroke:#f00\n";
    let ir = fm_parser::parse(source).ir;

    assert_eq!(
        ids(&ir),
        vec!["A", "B"],
        "the directive was interned as a class"
    );
    // The RELATION must survive — a guard that ate the edge along with the directive would satisfy
    // the node assertion above while quietly deleting the thing being styled.
    assert_eq!(ir.edges.len(), 1, "the styled relation was lost");

    let runs = text_runs(&fm_render_svg::render_svg(&ir));
    assert!(
        !runs.iter().any(|run| run.contains("linkStyle")),
        "the SVG drew a box captioned with the author's own directive; text runs were {runs:?}"
    );
    // NON-VACUITY: the real classes must still be drawn.
    for expected in ["A", "B"] {
        assert!(
            runs.iter().any(|run| run == expected),
            "CONTROL FAILED: class {expected} was not drawn either; text runs were {runs:?}"
        );
    }
}

/// Every targets spelling mermaid accepts is recognised: an index, a comma list, and `default`.
#[test]
fn every_link_style_target_spelling_is_recognised() {
    for targets in ["0", "0,1", "default"] {
        let source = format!(
            "classDiagram\n  class A\n  class B\n  A --> B\n  linkStyle {targets} stroke:#f00\n"
        );
        let ir = fm_parser::parse(&source).ir;
        assert_eq!(
            ids(&ir),
            vec!["A", "B"],
            "`linkStyle {targets}` was interned as a class"
        );
    }
}

/// CONTROL: a CSS value containing `--` does not defeat the guard.
///
/// The obvious implementation — bail when the statement contains `--`, on the theory that only a
/// relation does — is wrong: `stroke:var(--x)` is a perfectly ordinary custom property. Keying on
/// the TARGETS position instead is what makes this case work, so it is pinned.
#[test]
fn a_link_style_whose_css_contains_a_double_dash_is_still_a_directive() {
    let ir = fm_parser::parse(
        "classDiagram\n  class A\n  class B\n  A --> B\n  linkStyle 0 stroke:var(--x)\n",
    )
    .ir;
    assert_eq!(
        ids(&ir),
        vec!["A", "B"],
        "a CSS custom property defeated the directive guard"
    );
}

/// CONTROL: a class legitimately NAMED `linkStyle` keeps its relations.
///
/// This is a regression I introduced and caught with this exact case before committing. A bare
/// `keyword("linkStyle")` swallowed `linkStyle --> B` and silently dropped the edge — valid input
/// eaten by a widened filter, the bd-ij0f shape. The targets check is what distinguishes a
/// directive from a relation, and this is the test that proves it.
#[test]
fn a_class_named_link_style_keeps_its_relations() {
    let ir = fm_parser::parse("classDiagram\n  class linkStyle\n  class B\n  linkStyle --> B\n").ir;
    assert!(
        ids(&ir).contains(&"linkStyle"),
        "a class named `linkStyle` was swallowed: {:?}",
        ids(&ir)
    );
    assert_eq!(
        ir.edges.len(),
        1,
        "the relation from a class named `linkStyle` was dropped"
    );
}

/// CONTROL: a class whose name merely BEGINS with the keyword is untouched.
#[test]
fn a_class_named_linkage_is_not_swallowed() {
    let ir = fm_parser::parse("classDiagram\n  class linkage\n").ir;
    assert_eq!(ids(&ir), vec!["linkage"], "`linkage` was swallowed");
}
