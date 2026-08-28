//! A classDiagram stereotype is drawn `«interface»`, not `<<interface>>` — and a custom one is
//! drawn at all.
//!
//! TWO DEFECTS, found by reading both engines through the SAME DOM. Reading our SVG with a regex
//! first gave a different (wrong) answer, because `<<interface>>` ships escaped as
//! `&lt;&lt;interface&gt;&gt;` and a naive scan does not see what a reader sees.
//!
//! ```text
//!   input                          reference          ours (before)
//!   <<interface>>  in a block      «interface»        <<interface>>
//!   <<enumeration>> in a block     «enumeration»      <<enumeration>>
//!   <<interface>> A  (statement)   «interface»        <<interface>>
//!   <<Frobnicator>> (custom)       «Frobnicator»      Frobnicator      <-- delimiters GONE
//! ```
//!
//! **1. The ASCII angles are input syntax, not output.** mermaid 11.15.0 draws the UML guillemets
//! the `<<…>>` spelling stands for. Measured in Chromium 151 against the pinned bundle, reading the
//! drawn text of each form.
//!
//! **2. A custom stereotype rendered BARE**, indistinguishable from a member row.
//! `ClassStereotype::label` returned `Custom`'s payload verbatim under the comment "the author
//! already wrote their own delimiters" — but `class_stereotype_from_annotation` builds `Custom`
//! from the annotation body with `<<` and `>>` already stripped, so there were no delimiters to
//! keep. The comment described an input that never reaches it.
//!
//! ONE FUNCTION, SIX CALLERS. `label()` is consumed by fm-render-svg (twice), fm-render-canvas,
//! fm-render-term, and TWICE by fm-layout — the second of which MEASURES the box the renderers draw
//! this string into. Its own docstring records why that matters: if the measured and drawn text
//! disagree, the class renderers drop a row that falls outside the box rather than growing it, so a
//! divergence deletes output instead of merely looking wrong. `«interface»` is 13 chars where
//! `<<interface>>` was 13 — but `«Frobnicator»` is 13 where the bare payload was 11, so the sizing
//! caller genuinely had to move with the drawing ones.

use fm_core::{ClassStereotype, DiagramType, MermaidDiagramIr};

/// The drawn text of an SVG: the character data inside `<text>` elements.
///
/// ⚠️ NOT a substring search over the whole document. The pre-fix spelling ships as
/// `&lt;&lt;interface&gt;&gt;`, and an attribute (the a11y name) can echo any string, so scanning
/// the raw markup answers a different question than "what does a reader see". Entity references are
/// resolved here for the same reason.
fn drawn_text(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<text") {
        let Some(open_end) = rest[start..].find('>') else {
            break;
        };
        let body_start = start + open_end + 1;
        let Some(close) = rest[body_start..].find("</text>") else {
            break;
        };
        let body = &rest[body_start..body_start + close];
        // Strip nested tags (tspan), then resolve the entities the writer escaped.
        // ⚠️ A `>` OUTSIDE A TAG IS TEXT, NOT A TAG CLOSE. The writer escapes `<` but leaves `>`
        // literal — valid XML, and only `<` and `&` must be escaped in content — so the pre-fix
        // stereotype ships as `&lt;&lt;interface>>`. A depth tracker that consumed every `>` would
        // silently eat the `>>` from `a << b >> c` and make the planted negative below assert
        // against text no reader ever sees. Verified against the emitted markup, not assumed.
        let mut text = String::new();
        let mut depth = 0usize;
        for ch in body.chars() {
            match ch {
                '<' => depth += 1,
                '>' if depth > 0 => depth -= 1,
                _ if depth == 0 => text.push(ch),
                _ => {}
            }
        }
        let text = text
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&");
        let trimmed = text.trim().to_string();
        if !trimmed.is_empty() {
            out.push(trimmed);
        }
        rest = &rest[body_start + close..];
    }
    out
}

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

/// Every stereotype spelling draws guillemets, custom words included.
#[test]
fn a_stereotype_is_drawn_with_guillemets() {
    for (case, source, expected) in [
        (
            "interface in a block",
            "classDiagram\n  class A {\n    <<interface>>\n    +run()\n  }\n",
            "«interface»",
        ),
        (
            "abstract in a block",
            "classDiagram\n  class A {\n    <<abstract>>\n  }\n",
            "«abstract»",
        ),
        (
            "enumeration in a block",
            "classDiagram\n  class Color {\n    <<enumeration>>\n    RED\n  }\n",
            "«enumeration»",
        ),
        (
            "the `<<interface>> Name` statement form",
            "classDiagram\n  class A\n  <<interface>> A\n",
            "«interface»",
        ),
        (
            "a CUSTOM stereotype, which used to draw bare",
            "classDiagram\n  class A {\n    <<Frobnicator>>\n  }\n",
            "«Frobnicator»",
        ),
    ] {
        let svg = render(source);
        let texts = drawn_text(&svg);
        assert!(
            texts.iter().any(|text| text == expected),
            "{case}: expected the reference's {expected:?} among the drawn text, got {texts:?}"
        );
        assert!(
            !texts.iter().any(|text| text.contains("<<")),
            "{case}: the ASCII input spelling reached the picture: {texts:?}"
        );
    }
}

/// ⚠️ THE PLANTED NEGATIVE: `<<` and `>>` inside a LABEL are ordinary text and must survive.
///
/// The cheap way to make every assertion above pass is to rewrite `<<` and `>>` wherever they
/// appear — in the label writer, in the escape pass, in a post-pass over the finished document.
/// Every one of those makes this case draw `«a » b « c»` or similar, and the reference draws
/// `a << b >> c` verbatim (measured in Chromium 151). The decoration belongs to the STEREOTYPE, not
/// to the character pair, and only a case where the pair is NOT a stereotype can tell the two
/// implementations apart.
#[test]
fn angle_pairs_inside_a_label_are_not_decorated() {
    let svg = render("classDiagram\n  class A[\"a << b >> c\"]\n");
    let texts = drawn_text(&svg);
    assert!(
        texts.iter().any(|text| text == "a << b >> c"),
        "a label containing `<<` and `>>` was rewritten; the reference draws it verbatim: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains('«') || text.contains('»')),
        "guillemets were applied to label text that is not a stereotype: {texts:?}"
    );
}

/// ⚠️ SECOND PLANTED NEGATIVE: the four known words must not be a hardcoded allowlist.
///
/// An implementation that decorates only `interface`/`abstract`/`enumeration`/`service` passes the
/// first test's four known rows and still leaves every other word bare — which is exactly the bug
/// being fixed, since `Custom` is the variant that was rendering without delimiters. Two unrelated
/// custom words are asserted so a fix cannot special-case the one used above.
#[test]
fn an_arbitrary_custom_stereotype_is_decorated() {
    for word in ["Frobnicator", "aggregate root", "Ünïcødé"] {
        let source = format!("classDiagram\n  class A {{\n    <<{word}>>\n  }}\n");
        let texts = drawn_text(&render(&source));
        let expected = format!("«{word}»");
        assert!(
            texts.iter().any(|text| text == &expected),
            "custom stereotype {word:?} did not draw as {expected:?}: {texts:?}"
        );
    }
}

/// ⚠️ PLANTED NEGATIVE 3: the guillemets are CLASS-DIAGRAM-SPECIFIC. C4 and requirement keep `<<>>`.
///
/// mermaid does NOT use guillemets everywhere it shows a stereotype. Measured in Chromium 151
/// against the pinned bundle:
///
/// ```text
///   C4Context   Person(...)     «person»  ✗   drawn `<<person>>`
///   requirement element/rel     drawn `<<Element>>`, `<<satisfies>>`, `<<Requirement>>`
///   classDiagram <<interface>>  drawn `«interface»`
/// ```
///
/// So a fix that reaches for the obvious shared place — decorating in a common text writer, or
/// rewriting `<<`/`>>` on the way out of the document — converts these families too and diverges
/// from the reference in three diagram types to fix one. They build their own strings on their own
/// paths today, and this test is what keeps that separation honest if someone later "centralises"
/// it.
#[test]
fn other_families_keep_the_ascii_angles() {
    let c4 = drawn_text(&render("C4Context\n  Person(p, \"Alice\", \"a user\")\n"));
    assert!(
        c4.iter().any(|text| text == "<<person>>"),
        "C4 must keep the ASCII angles the reference draws: {c4:?}"
    );
    assert!(
        !c4.iter().any(|text| text.contains('«')),
        "guillemets leaked into a C4 diagram: {c4:?}"
    );

    let req = drawn_text(&render(
        "requirementDiagram\n  requirement r {\n    id: 1\n    text: t\n    risk: high\n    verifymethod: test\n  }\n  element e {\n    type: sim\n  }\n  e - satisfies -> r\n",
    ));
    assert!(
        req.iter().any(|text| text == "<<Element>>"),
        "requirement must keep the ASCII angles the reference draws: {req:?}"
    );
    assert!(
        !req.iter().any(|text| text.contains('«')),
        "guillemets leaked into a requirement diagram: {req:?}"
    );
}

/// The layout that MEASURES the stereotype and the renderer that DRAWS it agree.
///
/// ⚠️ This is the coupling `label()`'s docstring warns about: the class renderers drop a row that
/// falls outside the measured box rather than growing the box, so a sizing path still measuring the
/// old string would silently delete the stereotype it was widened for. `«Frobnicator»` is two
/// characters wider than the bare `Frobnicator` it replaced, so the box must have grown; asserting
/// the row still DRAWS is what proves both halves moved together.
#[test]
fn the_measured_box_still_contains_the_drawn_stereotype() {
    let source = "classDiagram\n  class A {\n    <<Frobnicator>>\n    +run()\n  }\n";
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let svg = fm_render_svg::render_svg(&ir);
    let texts = drawn_text(&svg);

    assert!(
        texts.iter().any(|text| text == "«Frobnicator»"),
        "the widened stereotype was dropped instead of drawn: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "+run()"),
        "the member row below the stereotype was pushed out of the box: {texts:?}"
    );
    let node = layout.nodes.first().expect("one class node");
    assert!(
        node.bounds.width > 0.0 && node.bounds.height > 0.0,
        "the class box has no extent"
    );
}

/// CONTROL: the label of every variant is what `label()` promises, including the owned one.
///
/// Reads the type directly so a renderer change cannot make this pass by accident, and pins that
/// `Custom` builds its decoration rather than echoing a payload that never carried delimiters.
#[test]
fn every_stereotype_variant_reports_its_decorated_label() {
    assert_eq!(ClassStereotype::Interface.label(), "«interface»");
    assert_eq!(ClassStereotype::Abstract.label(), "«abstract»");
    assert_eq!(ClassStereotype::Enum.label(), "«enumeration»");
    assert_eq!(ClassStereotype::Service.label(), "«service»");
    assert_eq!(
        ClassStereotype::Custom("Frobnicator".to_string()).label(),
        "«Frobnicator»"
    );
    // An empty IR renders without a stereotype at all — the variant list above is the only place
    // the mapping is asserted, so this guards the enum staying exhaustively covered here.
    let ir = MermaidDiagramIr::empty(DiagramType::Class);
    assert!(!fm_render_svg::render_svg(&ir).contains('«'));
}
