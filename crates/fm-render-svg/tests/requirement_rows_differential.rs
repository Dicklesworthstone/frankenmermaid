//! Differential test: the field rows and relationship labels mermaid draws in a requirement diagram.
//!
//! FOUR DIVERGENCES, all found the same way and all measured in a real browser.
//!
//! ```text
//!   risk + verification   we drew ONE row `Risk: High | Verify: Test`; mermaid draws TWO
//!   the verification label  we wrote `Verify:`; mermaid writes `Verification:`
//!   the doc-ref label       we wrote `Doc:`;    mermaid writes `Doc Ref:`
//!   an element's header     we drew `<<element>>`; mermaid draws the literal `<<Element>>`
//!   a relationship label    we drew `satisfies`;  mermaid draws `<<satisfies>>`
//! ```
//!
//! REFERENCE, from the pinned 11.15.0 bundle's requirement renderer. Each field is its own line and
//! nothing is ever joined:
//!
//! ```text
//!   u ? $u(m, `<<${n.type}>>`, 0, …) : $u(m, "<<Element>>", 0, …)
//!   $u(m, n.name, y, … "; font-weight: bold;")
//!   `ID: ${n.requirementId}`   `Text: ${n.text}`
//!   `Risk: ${n.risk}`          `Verification: ${n.verifyMethod}`
//!   `Type: ${a.type}`          `Doc Ref: ${a.docRef}`
//! ```
//!
//! and its relationship edges carry `` label: `<<${n.type}>>`, classes: "relationshipLine" ``.
//!
//! ⚠️ HOW THESE WERE FOUND, because it is the reusable part. The requirement family has NO
//! head-to-head corpus item and its renderer will not run under jsdom, so `equivalence.mjs` has
//! nothing to compare and `drawn_text_diff.mjs` reports INCUMBENT-DNF. Both cheap oracles are blind
//! here. `scripts/headtohead/chromium_text_diff.mjs` renders the pinned bundle in real Chromium over
//! CDP and diffs the drawn text as a multiset; it took `requirement_basic` from four divergences to
//! `AGREE 17 runs`.

fn runs(source: &str) -> Vec<String> {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let mut out = Vec::new();
    let mut rest = svg.as_str();
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open) = rest.find('>') else { break };
        let Some(close) = rest.find("</text>") else {
            break;
        };
        // ⚠️ STRIP MARKUP FIRST, UNESCAPE SECOND. Unescaping `&lt;&lt;` into `<<` before removing
        // tags hands the tag stripper a `<<Requirement>>` that looks exactly like markup, and it
        // deletes the very runs this file exists to check — every `<<…>>` assertion then fails
        // against a renderer that is drawing them correctly. I wrote it the wrong way round first
        // and it accused two working fixes of being broken.
        let body = rest[open + 1..close].replace("</tspan><tspan", "\u{1}<tspan");
        let mut stripped = String::new();
        let mut in_tag = false;
        for ch in body.chars() {
            // ⚠️ `>` IS ONLY A DELIMITER INSIDE A TAG. The renderer escapes `<` (to `&lt;`) but emits
            // `>` literally, so a drawn `<<Element>>` reaches here as `&lt;&lt;Element>>`. Treating
            // every `>` as a tag close swallows the closing angles and yields `<<Element` — which
            // reads as a truncation bug in the renderer and is a bug in this reader.
            match ch {
                '<' => in_tag = true,
                '>' if in_tag => in_tag = false,
                _ if !in_tag => stripped.push(ch),
                _ => {}
            }
        }
        let text = stripped
            .replace('\u{1}', " ")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        let text = text.trim().to_string();
        if !text.is_empty() {
            out.push(text);
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

const DIAGRAM: &str = "requirementDiagram\n    requirement AuthReq {\n        id: \"REQ-001\"\n        text: Users must authenticate\n        risk: High\n        verifymethod: Test\n    }\n    element LoginModule {\n        type: Module\n        docref: spec.md\n    }\n    LoginModule - satisfies -> AuthReq\n";

#[test]
fn every_field_is_its_own_row_with_mermaids_label() {
    let drawn = runs(DIAGRAM);
    for expected in [
        "ID: REQ-001",
        "Text: Users must authenticate",
        "Risk: High",
        "Verification: Test",
        "Type: Module",
        "Doc Ref: spec.md",
    ] {
        assert!(
            drawn.iter().any(|run| run == expected),
            "{expected:?} is not drawn as its own row; drew {drawn:?}"
        );
    }
}

/// ⚠️ THE NEGATIVE CONTROL FOR THE FUSED ROW, and the defect exactly as it shipped.
///
/// `Risk: High | Verify: Test` satisfies "the risk reached the drawing" and "the verify method
/// reached the drawing" — a substring check for either would pass on it. What it never satisfies is
/// mermaid drawing them as two independent lines with no separator between them.
#[test]
fn risk_and_verification_are_never_joined_into_one_row() {
    let drawn = runs(DIAGRAM);
    assert!(
        !drawn.iter().any(|run| run.contains('|')),
        "a row still carries the `|` separator mermaid never draws: {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|run| run.contains("Verify:")),
        "the label is `Verification:`, not the abbreviation we invented: {drawn:?}"
    );
    assert!(
        !drawn
            .iter()
            .any(|run| run.starts_with("Doc: ") || run.contains("| Verify")),
        "the label is `Doc Ref:`, not `Doc:`: {drawn:?}"
    );
}

/// ⚠️ CONTROL FOR THE ELEMENT HEADER. mermaid hardcodes a capitalised literal here rather than
/// echoing the `element` keyword the author typed — the same machine-token-reaching-a-reader defect
/// the requirement TYPE header had.
#[test]
fn an_element_draws_mermaids_capitalised_literal() {
    let drawn = runs(DIAGRAM);
    assert!(
        drawn.iter().any(|run| run == "<<Element>>"),
        "an element must draw the literal <<Element>>; drew {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|run| run == "<<element>>"),
        "the authored keyword reached the drawing: {drawn:?}"
    );
}

/// ⚠️ CONTROL FOR THE RELATIONSHIP LABEL. mermaid stores the WRAPPED string on the edge, so a bare
/// `satisfies` is a divergence even though the word itself is right.
#[test]
fn a_relationship_label_is_wrapped_in_angles() {
    let drawn = runs(DIAGRAM);
    assert!(
        drawn.iter().any(|run| run == "<<satisfies>>"),
        "the relationship must draw <<satisfies>>; drew {drawn:?}"
    );
    assert!(
        !drawn.iter().any(|run| run == "satisfies"),
        "the bare relationship keyword reached the drawing: {drawn:?}"
    );
}

/// CONTROL ON ABSENCE: a field the author did not declare must produce NO row.
///
/// Guards the opposite failure — emitting `Risk: ` or `Doc Ref: ` with an empty value — which every
/// assertion above would tolerate. mermaid guards each row on its own field
/// (`${n.risk ? ... : ""}`), and an empty label is worse than a missing one because it reads as a
/// declared-but-blank field.
#[test]
fn an_undeclared_field_draws_no_row() {
    let sparse =
        "requirementDiagram\n    requirement Bare {\n        id: \"R\"\n        text: t\n    }\n";
    let drawn = runs(sparse);
    for absent in ["Risk:", "Verification:", "Type:", "Doc Ref:"] {
        assert!(
            !drawn.iter().any(|run| run.starts_with(absent)),
            "{absent:?} was drawn for a requirement that never declared it: {drawn:?}"
        );
    }
    // Non-vacuity: the rows that WERE declared are still there, so the check above is not passing
    // because nothing renders at all.
    assert!(
        drawn.iter().any(|run| run == "ID: R"),
        "the declared rows vanished too, so this test proves nothing: {drawn:?}"
    );
}
