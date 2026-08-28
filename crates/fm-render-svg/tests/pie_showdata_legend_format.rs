//! A `pie showData` legend row is `Dogs [30]`, and the value is the author's number — not a
//! rounded one.
//!
//! TWO DEFECTS, the second far worse than the first.
//!
//! **1. The row format diverged.** We drew `Dogs: 30 (60.0%)`; mermaid draws `Dogs [30]`. The
//! separator, the brackets and the absence of a percentage are all parity, not taste — the share is
//! still on screen as the wedge's own slice label, which both engines draw.
//!
//! **2. THE VALUE WAS ROUNDED TO AN INTEGER, which is data loss in a legend.** The old format
//! string used `{:.0}`, so a chart written `"A" : 0.25` published a legend row reading `A: 0` — an
//! assertion that the value is ZERO where the author wrote a quarter. Nothing about the picture
//! revealed the corruption: the wedge angle stayed correct, so only the printed number lied.
//!
//! MEASURED REFERENCE — pinned mermaid 11.15.0 rendered in Chromium 151, reading the drawn legend
//! text, with both engines read through the same DOM:
//!
//! ```text
//!   source value   reference     ours (before)
//!   30             A [30]        A: 30 (75.0%)
//!   30.0           A [30]        A: 30 (75.0%)
//!   1.5            A [1.5]       A: 2  (37.5%)     <-- rounded UP
//!   0.25           A [0.25]      A: 0  (25.0%)     <-- rounded to ZERO
//!   1.23456        A [1.23456]   A: 1  (55.2%)
//!   1000000        A [1000000]   A: 1000000 (100.0%)
//! ```
//!
//! Rust's `f32` `Display` is the shortest round-trip form and agrees with JavaScript's on every one
//! of those, so no bespoke number formatting is needed — which is itself the point: the old code
//! had bespoke formatting and that is what broke it.

/// The drawn text of an SVG: character data inside `<text>` elements, nested tags stripped.
///
/// ⚠️ A `>` OUTSIDE A TAG IS TEXT. The writer escapes `<` but leaves `>` literal (valid XML — only
/// `<` and `&` must be escaped in content), so a depth tracker that consumed every `>` would eat
/// real characters out of the drawn strings.
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

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

/// The legend row carries the label and the bracketed value, in the reference's spelling.
#[test]
fn a_showdata_legend_row_is_label_then_bracketed_value() {
    let texts = drawn_text(&render(
        "pie showData\n  title Pets\n  \"Dogs\" : 30\n  \"Cats\" : 20\n",
    ));
    for expected in ["Dogs [30]", "Cats [20]"] {
        assert!(
            texts.iter().any(|text| text == expected),
            "expected the reference's {expected:?} among the drawn text, got {texts:?}"
        );
    }
    // The old spelling must be gone, not merely joined by the new one.
    assert!(
        !texts.iter().any(|text| text.contains("Dogs:")),
        "the old `Label: value (pct%)` row is still drawn: {texts:?}"
    );
}

/// ⚠️ THE PLANTED NEGATIVE: the author's value is published exactly, never rounded.
///
/// This is the assertion the previous implementation fails hardest, and it is the one a careless
/// reimplementation fails too. `{:.0}` — or `as i64`, or `.round()`, or any "values are counts"
/// assumption — turns `0.25` into `0`, which is not a formatting difference but a legend stating a
/// number the author never wrote. A test that only checked `Dogs [30]` would pass with every one of
/// those, because 30 survives rounding. Fractional values are the only inputs that separate a
/// faithful implementation from a rounding one.
#[test]
fn a_fractional_value_is_published_exactly_not_rounded() {
    for (source_value, expected) in [
        ("0.25", "A [0.25]"),
        ("1.5", "A [1.5]"),
        ("1.23456", "A [1.23456]"),
    ] {
        let source = format!("pie showData\n  \"A\" : {source_value}\n  \"B\" : 4\n");
        let texts = drawn_text(&render(&source));
        assert!(
            texts.iter().any(|text| text == expected),
            "value {source_value} was not published exactly; expected {expected:?}, got {texts:?}"
        );
        // Name the corruption directly: a rounded row for these inputs reads `A [0]`, `A [2]`
        // or `A [1]`, all of which are a different number than the author wrote.
        for rounded in ["A [0]", "A [1]", "A [2]"] {
            assert!(
                !texts.iter().any(|text| text == rounded),
                "value {source_value} was ROUNDED to {rounded:?} — the legend publishes a number \
                 the author never wrote: {texts:?}"
            );
        }
    }
}

/// A whole number keeps no trailing `.0`, including when written as one.
///
/// The mirror of the case above: a fix that reaches for `{:.5}` or similar to stop rounding then
/// prints `30.00000`, and `30.0` in the source must still draw `[30]`. Both directions are pinned
/// so neither over-correction survives.
#[test]
fn a_whole_value_keeps_no_trailing_decimal() {
    for source_value in ["30", "30.0"] {
        let source = format!("pie showData\n  \"A\" : {source_value}\n  \"B\" : 10\n");
        let texts = drawn_text(&render(&source));
        assert!(
            texts.iter().any(|text| text == "A [30]"),
            "{source_value} did not draw as `A [30]`: {texts:?}"
        );
        assert!(
            !texts.iter().any(|text| text.starts_with("A [30.")),
            "{source_value} grew a trailing decimal: {texts:?}"
        );
    }
}

/// CONTROL: without `showData` no value is published at all.
///
/// The fix touches the branch that builds the row, so the other branch must be shown untouched —
/// and this is a standing contract of its own: `showData: false` means the author chose not to
/// publish the numbers, so a legend that leaks them is worse than one formatted wrongly.
#[test]
fn without_showdata_the_legend_publishes_no_value() {
    let texts = drawn_text(&render(
        "pie\n  title Pets\n  \"Dogs\" : 30\n  \"Cats\" : 20\n",
    ));
    assert!(
        texts.iter().any(|text| text == "Dogs"),
        "the bare label is missing: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains("[30]")),
        "a value leaked into a chart the author did not ask to publish numbers for: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains("Dogs [")),
        "the showData row shape leaked into a non-showData chart: {texts:?}"
    );
}

/// CONTROL: the wedge's own percentage label is unaffected.
///
/// The share moved OUT of the legend row, so the place it still belongs must be shown still there —
/// otherwise "the percentage is gone from the legend" and "the percentage is gone" look identical.
#[test]
fn the_slice_percentage_label_still_draws() {
    let texts = drawn_text(&render(
        "pie showData\n  title Pets\n  \"Dogs\" : 30\n  \"Cats\" : 20\n",
    ));
    assert!(
        texts.iter().any(|text| text == "60%"),
        "the wedge percentage label vanished with the legend's: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "40%"),
        "the wedge percentage label vanished with the legend's: {texts:?}"
    );
}
