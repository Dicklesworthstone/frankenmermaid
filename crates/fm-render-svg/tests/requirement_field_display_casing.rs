//! A requirement's `risk:` and `verifymethod:` are drawn as mermaid's enum names, not as the author
//! typed them.
//!
//! THE DEFECT. We echoed the authored keyword, so `risk: high` drew `Risk: high` where mermaid draws
//! `Risk: High`, and `verifymethod: analysis` drew `Verification: analysis` for `Analysis`.
//!
//! ⚠️ IT IS A TABLE, NOT A CAPITALISATION, and one measurement is what settles that. The obvious
//! repair — upper-case the first letter — is wrong:
//!
//! ```text
//!   source              reference       first-letter transform would give
//!   risk: high          Risk: High      High          ✓ agrees, and hides the bug below
//!   risk: HIGH          Risk: High      HIGH          ✗
//!   risk: hIgH          Risk: High      HIgH          ✗
//!   verifymethod: TEST  Verification: Test   TEST     ✗
//! ```
//!
//! mermaid parses the keyword into an enum and prints the enum's own name, discarding the author's
//! casing entirely. Measured in Chromium 151 against the pinned mermaid 11.15.0 bundle, reading the
//! drawn text.
//!
//! ⚠️ ONE DELIBERATE DIVERGENCE, PINNED RATHER THAN HIDDEN. An unrecognised keyword is a PARSE ERROR
//! upstream — `risk: bogus` yields "Parse error on line 5" from the reference, drawing nothing at
//! all. This parser's contract is best-effort recovery, so it keeps the author's own text instead of
//! erroring or inventing a value. That is a real difference in behaviour and the test below asserts
//! it, so the day someone decides we should reject the input instead, this is where it is written
//! down.

/// The drawn text of an SVG: character data inside `<text>` elements, nested tags stripped.
///
/// A `>` outside a tag is TEXT — the writer escapes `<` but leaves `>` literal (valid XML), so a
/// depth tracker that consumed every `>` would eat real characters out of the drawn strings.
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

fn render_requirement(risk: &str, verify: &str) -> Vec<String> {
    let source = format!(
        "requirementDiagram\n  requirement r {{\n    id: 1\n    text: t\n    risk: {risk}\n    verifymethod: {verify}\n  }}\n"
    );
    drawn_text(&fm_render_svg::render_svg(&fm_parser::parse(&source).ir))
}

/// Every keyword draws its enum name.
#[test]
fn risk_and_verification_draw_their_display_names() {
    for (risk, verify, risk_row, verify_row) in [
        ("high", "test", "Risk: High", "Verification: Test"),
        ("medium", "analysis", "Risk: Medium", "Verification: Analysis"),
        (
            "low",
            "inspection",
            "Risk: Low",
            "Verification: Inspection",
        ),
        (
            "low",
            "demonstration",
            "Risk: Low",
            "Verification: Demonstration",
        ),
    ] {
        let texts = render_requirement(risk, verify);
        for expected in [risk_row, verify_row] {
            assert!(
                texts.iter().any(|text| text == expected),
                "{risk}/{verify}: expected the reference's {expected:?}, got {texts:?}"
            );
        }
    }
}

/// ⚠️ THE PLANTED NEGATIVE: an UPPER-CASE keyword still draws the enum name.
///
/// This is what separates the table from a capitalisation, and it is the only kind of input that
/// does. A first-letter transform — `to_uppercase` on the head, or `char::to_ascii_uppercase` plus
/// the tail — draws `Risk: HIGH` here while satisfying every lower-case row above, so the happy path
/// alone cannot tell the two implementations apart. Mixed case is asserted for the same reason: it
/// fails an implementation that only touches the first character and leaves the rest alone.
#[test]
fn a_shouted_or_mixed_keyword_is_normalised_to_the_enum_name() {
    for (risk, verify) in [("HIGH", "TEST"), ("hIgH", "tEsT"), ("High", "Test")] {
        let texts = render_requirement(risk, verify);
        assert!(
            texts.iter().any(|text| text == "Risk: High"),
            "{risk}: expected `Risk: High`, got {texts:?}"
        );
        assert!(
            texts.iter().any(|text| text == "Verification: Test"),
            "{verify}: expected `Verification: Test`, got {texts:?}"
        );
        // Name the failure directly rather than only asserting the success.
        assert!(
            !texts.iter().any(|text| text == "Risk: HIGH" || text == "Risk: hIgH"),
            "{risk}: the author's casing survived into the picture: {texts:?}"
        );
    }
}

/// ⚠️ THE FIXTURE THAT CANNOT FAIL, kept as an explicit control.
///
/// `risk: High` renders `Risk: High` BOTH before and after this fix, because our old code echoed the
/// author's text and the author happened to capitalise it. A conformance test written with this
/// source alone would have gone green against the broken implementation and concluded the behaviour
/// was already correct. It is kept — with this note — so the next reader knows the already-correct
/// spelling proves nothing on its own, and that the lower- and upper-case rows above are the ones
/// carrying the weight.
#[test]
fn an_already_capitalised_keyword_is_unchanged_and_proves_nothing_alone() {
    let texts = render_requirement("High", "Test");
    assert!(texts.iter().any(|text| text == "Risk: High"));
    assert!(texts.iter().any(|text| text == "Verification: Test"));
}

/// The deliberate divergence: an unknown keyword keeps the author's text.
///
/// mermaid REFUSES this input (`risk: bogus` is a parse error and draws nothing). We recover and
/// draw what was written, which is this parser's stated best-effort contract. Asserted so the
/// difference is a recorded decision rather than an accident nobody noticed.
#[test]
fn an_unrecognised_keyword_keeps_the_authors_text() {
    let texts = render_requirement("bogus", "telepathy");
    assert!(
        texts.iter().any(|text| text == "Risk: bogus"),
        "an unknown risk was mangled instead of passed through: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "Verification: telepathy"),
        "an unknown verify method was mangled instead of passed through: {texts:?}"
    );
}

/// CONTROL: the display tables report exactly what the renderer draws.
///
/// Read off the functions directly so a renderer change cannot make the mapping pass by accident,
/// and so the case-insensitivity is pinned at the source rather than only through a rendered SVG.
#[test]
fn the_display_tables_normalise_case_insensitively() {
    for input in ["high", "HIGH", "hIgH", "High", "  high  "] {
        assert_eq!(fm_core::requirement_risk_display(input), "High", "{input:?}");
    }
    assert_eq!(fm_core::requirement_risk_display("medium"), "Medium");
    assert_eq!(fm_core::requirement_risk_display("low"), "Low");
    for input in ["test", "TEST", "tEsT"] {
        assert_eq!(
            fm_core::requirement_verify_method_display(input),
            "Test",
            "{input:?}"
        );
    }
    assert_eq!(
        fm_core::requirement_verify_method_display("demonstration"),
        "Demonstration"
    );
    // Pass-through, asserted at the source too.
    assert_eq!(fm_core::requirement_risk_display("bogus"), "bogus");
}
