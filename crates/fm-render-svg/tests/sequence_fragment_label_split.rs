//! Differential test: a sequence fragment's keyword and its condition are two drawn elements.
//!
//! THE DIVERGENCE. `alt Valid credentials` drew a single run, `alt [Valid credentials]`. mermaid
//! draws the keyword in the small tab at the top-left and the condition CENTRED across the fragment
//! below it — two elements, two classes, two positions.
//!
//! REFERENCE, from the pinned 11.15.0 bundle's `drawLoop`:
//!
//! ```text
//!   g.text = r;        g.x = t.startx;  g.y = t.starty;   g.class = "labelText"
//!   g.text = t.title;  g.x = t.startx + labelBoxWidth/2 + (t.stopx - t.startx)/2;
//!                      g.y = t.starty + boxMargin + boxTextMargin;
//!                      g.class = "loopText";  g.anchor = "middle"
//! ```
//!
//! A third call writes each `else`/`and`/`option` branch as `sectionTitle`. That third one we
//! already matched — the fusion of the first two was the whole of the divergence.
//!
//! ⚠️ FOUND BY THE CHROMIUM ORACLE, which reported it exactly:
//! `mermaid draws, we do not: ["alt","[Valid credentials]"]` against
//! `we draw, mermaid does not: ["alt [Valid credentials]"]`. `sequence_advanced` now reports
//! AGREE on all 22 runs.
//!
//! ⚠️ AND NOTE WHAT THE ORACLE COULD NOT SEE. It compares drawn STRINGS, so it would equally have
//! passed an implementation that emitted both runs stacked in the same place. The geometry control
//! below is what makes this a real fix rather than a string-shaped one.

fn runs_with_class(source: &str) -> Vec<(String, String)> {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let mut out = Vec::new();
    let mut rest = svg.as_str();
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open) = rest.find('>') else { break };
        let Some(close) = rest.find("</text>") else {
            break;
        };
        let attrs = &rest[..open];
        let class = attrs
            .find("class=\"")
            .map(|at| &attrs[at + 7..])
            .and_then(|tail| tail.find('"').map(|end| tail[..end].to_string()))
            .unwrap_or_default();
        // Strip markup before unescaping; `>` closes a tag only when one is open.
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
            out.push((text, class));
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

const ALT: &str = "sequenceDiagram\n  participant A\n  participant B\n  alt Valid credentials\n    A->>B: ok\n  else Invalid credentials\n    A->>B: no\n  end\n";

#[test]
fn the_keyword_and_the_condition_are_separate_runs() {
    let drawn = runs_with_class(ALT);
    let texts: Vec<&str> = drawn.iter().map(|(t, _)| t.as_str()).collect();
    assert!(
        texts.contains(&"alt"),
        "the fragment keyword must be its own run; drew {texts:?}"
    );
    assert!(
        texts.contains(&"[Valid credentials]"),
        "the condition must be its own run; drew {texts:?}"
    );
}

/// ⚠️ THE NEGATIVE CONTROL, and the defect exactly as it shipped. `alt [Valid credentials]`
/// contains both pieces, so any check phrased as "does the condition appear?" passes on the fused
/// output. Only the absence of the fused run tells them apart.
#[test]
fn the_keyword_and_condition_are_never_one_run() {
    let drawn = runs_with_class(ALT);
    assert!(
        !drawn.iter().any(|(text, _)| text.starts_with("alt [")),
        "the keyword and condition are still fused into one run: {drawn:?}"
    );
}

/// ⚠️ GEOMETRY CONTROL — the half the text-only oracle is blind to.
///
/// mermaid puts the keyword at the fragment's top-left corner and CENTRES the condition across it.
/// Two runs emitted at the same x would satisfy every string assertion above and still be wrong, so
/// the condition must be measurably further right and lower than the keyword.
#[test]
fn the_condition_sits_below_and_right_of_the_keyword_tab() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(ALT).ir);
    let coords = |needle: &str| -> (f32, f32) {
        let at = svg.find(needle).expect("run present");
        let open = svg[..at].rfind("<text").expect("enclosing text element");
        let attrs = &svg[open..at];
        let read = |name: &str| -> f32 {
            let at = attrs.find(name).expect("attribute present");
            let rest = &attrs[at + name.len()..];
            let end = rest.find('"').expect("terminated attribute");
            rest[..end].parse().expect("numeric")
        };
        (read(" x=\""), read(" y=\""))
    };
    let (keyword_x, keyword_y) = coords(">alt<");
    let (condition_x, condition_y) = coords(">[Valid credentials]<");
    assert!(
        condition_x > keyword_x,
        "the condition ({condition_x}) must be centred right of the keyword tab ({keyword_x})"
    );
    assert!(
        condition_y > keyword_y,
        "the condition ({condition_y}) must sit below the keyword ({keyword_y})"
    );
}

/// CONTROL: a fragment with no condition draws the keyword and nothing else.
///
/// `loop` and `opt` are commonly written bare. Emitting an empty `[]` beneath them would satisfy
/// every assertion above, and mermaid never does it — its condition call is skipped when the title
/// is empty.
#[test]
fn a_fragment_without_a_condition_draws_no_empty_brackets() {
    let bare =
        "sequenceDiagram\n  participant A\n  participant B\n  loop\n    A->>B: tick\n  end\n";
    let drawn = runs_with_class(bare);
    let texts: Vec<&str> = drawn.iter().map(|(t, _)| t.as_str()).collect();
    assert!(
        texts.contains(&"loop"),
        "the keyword vanished, so this test proves nothing: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t.contains("[]")),
        "an empty bracket pair was drawn for a fragment with no condition: {texts:?}"
    );
}

/// CONTROL: the two runs carry DIFFERENT classes, so a theme can style them apart the way mermaid
/// does (`labelText` bold in the tab, `loopText` plain).
#[test]
fn the_keyword_and_condition_are_separately_classed() {
    let drawn = runs_with_class(ALT);
    let class_of = |needle: &str| -> String {
        drawn
            .iter()
            .find(|(text, _)| text == needle)
            .map(|(_, class)| class.clone())
            .unwrap_or_default()
    };
    let keyword_class = class_of("alt");
    let condition_class = class_of("[Valid credentials]");
    assert!(!keyword_class.is_empty() && !condition_class.is_empty());
    assert_ne!(
        keyword_class, condition_class,
        "the keyword and the condition share a class, so no theme can tell them apart"
    );
}
