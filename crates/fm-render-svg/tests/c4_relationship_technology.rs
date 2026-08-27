//! Differential test: a C4 relationship's technology is its OWN drawn row, not part of the label.
//!
//! THE DIVERGENCE. `Rel(a, b, "Uses", "HTTPS")` drew a single run `Uses [HTTPS]`. mermaid draws two
//! text elements — the label, then the bracketed technology one row lower, in italic — so the run we
//! emitted is a string neither engine ever produces, and the technology inherited the label's
//! upright weight and position instead of its own.
//!
//! REFERENCE, from the pinned 11.15.0 bundle's `drawRels`:
//!
//! ```text
//!   Nu(r)("[" + s.techn.text + "]", n,
//!         … + Math.abs(…)/2 + h,
//!         … + r.messageFontSize + 5 + d,
//!         Math.max(s.label.width, s.techn.width), s.techn.height,
//!         { fill: l, "font-style": "italic" }, p)
//! ```
//!
//! The label is drawn by a separate call above it. Hence: separate element, `messageFontSize + 5`
//! lower, italic.
//!
//! ⚠️ FOUND BY THE CHROMIUM ORACLE, and it is the only instrument that could see it. C4 has no
//! head-to-head corpus item and will not render under jsdom.
//! `scripts/headtohead/chromium_text_diff.mjs` reported the split precisely —
//! `mermaid draws, we do not: ["Uses", "[HTTPS]"]` against
//! `we draw, mermaid does not: ["Uses [HTTPS]"]` — across c4_container, c4_component and
//! c4_deployment. All three now report content-equal (their residual difference is line wrapping,
//! which that instrument declares UNDECIDABLE rather than guessing).

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
        // Strip markup BEFORE unescaping, and treat `>` as a delimiter only inside a tag: the
        // renderer escapes `<` and leaves `>` literal, so doing either the other way round mangles
        // the very runs under test.
        let body = &rest[open + 1..close];
        let mut stripped = String::new();
        let mut in_tag = false;
        for ch in body.chars() {
            match ch {
                '<' => in_tag = true,
                '>' if in_tag => in_tag = false,
                _ if !in_tag => stripped.push(ch),
                _ => {}
            }
        }
        let text = stripped
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

const WITH_TECHNOLOGY: &str = "C4Context\n    title T\n    Person(a, \"A\")\n    System(b, \"B\")\n    Rel(a, b, \"Uses\", \"HTTPS\")\n";

#[test]
fn the_label_and_the_technology_are_separate_runs() {
    let drawn = runs(WITH_TECHNOLOGY);
    assert!(
        drawn.iter().any(|run| run == "Uses"),
        "the relationship label must be its own run; drew {drawn:?}"
    );
    assert!(
        drawn.iter().any(|run| run == "[HTTPS]"),
        "the technology must be its own bracketed run; drew {drawn:?}"
    );
}

/// ⚠️ THE NEGATIVE CONTROL, and the defect exactly as it shipped. `Uses [HTTPS]` contains both
/// substrings, so any check phrased as "does the technology appear somewhere?" passes on the broken
/// output. Only the absence of the fused run distinguishes them.
#[test]
fn the_technology_is_never_fused_into_the_label() {
    let drawn = runs(WITH_TECHNOLOGY);
    assert!(
        !drawn.iter().any(|run| run.contains("Uses [")),
        "the label and technology are still drawn as one run: {drawn:?}"
    );
}

/// ⚠️ CONTROL FOR THE ITALIC, which is half the reference and invisible to a text-only comparison.
///
/// The Chromium differ compares drawn STRINGS, so it would have called a technology row that
/// inherited the label's upright style a pass. mermaid passes `{"font-style":"italic"}` for this
/// element and not for the label.
#[test]
fn the_technology_row_is_italic_and_the_label_is_not() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(WITH_TECHNOLOGY).ir);
    let element_of = |needle: &str| -> String {
        let at = svg.find(needle).expect("run present in the document");
        let open = svg[..at].rfind("<text").expect("enclosing text element");
        svg[open..at].to_string()
    };
    assert!(
        element_of(">[HTTPS]<").contains("font-style=\"italic\""),
        "the technology row must be italic, as mermaid draws it"
    );
    assert!(
        !element_of(">Uses<").contains("font-style=\"italic\""),
        "the label must NOT be italic; only the technology is"
    );
}

/// CONTROL: a relationship with no technology draws no bracketed row at all.
///
/// Guards the opposite failure — an empty `[]` beneath every relationship — which every assertion
/// above tolerates, and which mermaid never produces because it guards on `s.techn.text !== ""`.
#[test]
fn a_relationship_without_technology_draws_no_bracketed_row() {
    let plain = "C4Context\n    title T\n    Person(a, \"A\")\n    System(b, \"B\")\n    Rel(a, b, \"Uses\")\n";
    let drawn = runs(plain);
    assert!(
        drawn.iter().any(|run| run == "Uses"),
        "the label vanished, so this test proves nothing: {drawn:?}"
    );
    assert!(
        !drawn
            .iter()
            .any(|run| run.starts_with('[') && run.ends_with(']')),
        "a relationship with no technology drew a bracketed row: {drawn:?}"
    );
}
