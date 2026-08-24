//! `class A["Pretty Label"]` must reach the SVG as `Pretty Label` (bd-lfrlx).
//!
//! A bracket label whose text contained a SPACE did not merely lose its label — the class vanished
//! entirely, and a one-class diagram parsed to ZERO nodes. `parse_class_assignment_ast` splits the
//! statement on whitespace to read `class <nodes> <cssClass>`, so it saw `A["Pretty` as the node
//! list and `Label"]` as a CSS class to apply, claimed the statement, and the declaration branch
//! below it never ran.
//!
//! Two independent authorities say this is wrong, which is why it is a bug and not a dialect choice:
//!
//!   - THE PINNED INCUMBENT. mermaid 11.15.0's class grammar has a `setClassLabel` production whose
//!     body is `this.classes.get(n).label = r` — the bracket string becomes the class's label, and
//!     its `text` is rebuilt from it. `parse_probe.mjs` confirms the grammar accepts the syntax.
//!   - THIS REPO'S OWN PARSER. The BRACE form, `class A["Pretty Label"] { +go() }`, already set the
//!     label correctly. Only the bare form was broken, so the two spellings of one construct
//!     disagreed with each other.
//!
//! These tests join the parser to the SVG backend on the same IR: the point is not that some
//! internal field is populated but that the label a user typed is the text that gets drawn.

/// Every `<text>` run in the document, unescaped-free and matched structurally.
///
/// Deliberately NOT a `svg.contains("Pretty Label")`: the label also appears in the accessibility
/// `<desc>` and in a `<title>`, so a bare substring check passes even when the class BOX draws the
/// bare id. Reading the text runs makes the assertion about what a reader sees.
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

/// SAME-IR COMPARISON: the declared label is what the SVG draws, and the class exists at all.
#[test]
fn a_bracket_label_with_a_space_reaches_the_svg_as_the_class_heading() {
    let source = "classDiagram\n  class A[\"Pretty Label\"]\n  A : +go()\n";
    let ir = fm_parser::parse(source).ir;

    // CONTROL ON THE PARSE, and it is the assertion that would have failed hardest before the fix:
    // the class did not merely lose its label, it was never declared.
    assert_eq!(
        ir.nodes.len(),
        1,
        "expected exactly the one declared class, got {:?}",
        ir.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>()
    );

    let svg = fm_render_svg::render_svg(&ir);
    let runs = text_runs(&svg);

    assert!(
        runs.iter().any(|run| run == "Pretty Label"),
        "the SVG never drew the declared label; text runs were {runs:?}"
    );
    assert!(
        !runs.iter().any(|run| run == "A"),
        "the SVG drew the raw id instead of the declared label; text runs were {runs:?}"
    );
    // The member must survive alongside the label — a fix that recovered the heading by losing the
    // compartment would satisfy the two assertions above.
    assert!(
        runs.iter().any(|run| run == "+go()"),
        "the member was lost when the label was recovered; text runs were {runs:?}"
    );
}

/// CONTROL: `class <node> <cssClass>` is STILL a CSS-class assignment, not a declaration.
///
/// This is the form the fix had to keep working. The guard fires only on an UNBALANCED `[` in the
/// first token, so a real assignment — which contains no bracket at all — is untouched; without
/// this test, widening the guard to any `[` would still pass everything above while silently
/// turning every `class A someClass` in the corpus into a node declaration.
#[test]
fn a_css_class_assignment_is_not_mistaken_for_a_labelled_declaration() {
    let source = "classDiagram\n  class A\n  classDef bad fill:#f00\n  class A bad\n";
    let ir = fm_parser::parse(source).ir;

    let node = ir
        .nodes
        .iter()
        .find(|node| node.id.as_str() == "A")
        .expect("CONTROL FAILED: the class was not declared at all");
    assert!(
        node.classes.iter().any(|applied| applied == "bad"),
        "the CSS class stopped being applied; node carries {:?}",
        node.classes
    );
    assert_eq!(
        ir.nodes.len(),
        1,
        "the assignment statement declared a spurious extra node: {:?}",
        ir.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>()
    );
}

/// CONTROL: a bracket label with NO space keeps working, and agrees with the spaced one.
///
/// `class A["Pretty"]` already parsed correctly before the fix — `class_list_raw` came out empty so
/// the assignment parser declined on its own. Pinning it makes the two spellings one contract
/// rather than two accidents, and proves the guard did not regress the path that worked.
#[test]
fn a_single_word_bracket_label_still_reaches_the_svg() {
    let ir = fm_parser::parse("classDiagram\n  class A[\"Pretty\"]\n").ir;
    let runs = text_runs(&fm_render_svg::render_svg(&ir));
    assert!(
        runs.iter().any(|run| run == "Pretty"),
        "the single-word bracket label stopped reaching the SVG; text runs were {runs:?}"
    );
}

/// Mermaid accepts generics and bracket labels on the same class declaration. The generic is
/// metadata rendered after the declared heading; it must not consume the bracket suffix before the
/// ordinary label parser sees it (bd-lfrlx).
#[test]
fn a_generic_bracket_label_reaches_the_svg_as_the_class_heading() {
    let source = "classDiagram\n  class A~T~[\"Pretty Label\"]\n  A : +go()\n";
    let ir = fm_parser::parse(source).ir;
    let node = ir
        .nodes
        .first()
        .expect("the generic class must be declared");
    let label = node
        .label
        .and_then(|label_id| ir.labels.get(label_id.0))
        .map(|label| label.text.as_str());
    assert_eq!(label, Some("Pretty Label"));

    let runs = text_runs(&fm_render_svg::render_svg(&ir));
    assert!(
        runs.iter()
            .any(|run| run.contains("Pretty Label") && run.contains('T')),
        "the SVG did not draw the declared generic heading: {runs:?}"
    );
    assert!(
        !runs.iter().any(|run| run.contains("A&lt;T>")),
        "the SVG drew the raw id instead of the declared heading: {runs:?}"
    );
    assert!(
        runs.iter().any(|run| run == "+go()"),
        "the class member was lost while preserving the labelled generic: {runs:?}"
    );
}

/// The brace spelling must retain the same label and generic metadata as the bare declaration.
#[test]
fn a_generic_bracket_label_inside_a_class_block_reaches_the_svg() {
    let source = "classDiagram\n  class A~T~[\"Pretty Label\"] {\n    +go()\n  }\n";
    let ir = fm_parser::parse(source).ir;
    let node = ir
        .nodes
        .first()
        .expect("the generic class block must be declared");
    let label = node
        .label
        .and_then(|label_id| ir.labels.get(label_id.0))
        .map(|label| label.text.as_str());
    assert_eq!(label, Some("Pretty Label"));

    let runs = text_runs(&fm_render_svg::render_svg(&ir));
    assert!(
        runs.iter()
            .any(|run| run.contains("Pretty Label") && run.contains('T')),
        "the SVG did not draw the labelled generic class block: {runs:?}"
    );
    assert!(runs.iter().any(|run| run == "+go()"));
}
