//! A sankey node's throughput is drawn to at most two decimals — and never with digits the author
//! did not write.
//!
//! THE DEFECT. `format_sankey_total` printed the raw stored value, so the picture showed whatever
//! `f32` happened to hold. A flow written `1234.5678` drew **`1234.5677`** — a last digit that
//! exists only because the total is accumulated in `f32`. mermaid draws `1234.57`.
//!
//! MEASURED REFERENCE — pinned mermaid 11.15.0 in Chromium 151, reading the drawn node labels, both
//! engines read through the same DOM:
//!
//! ```text
//!   flow value    reference    ours (before)
//!   10            10           10
//!   124.729       124.73       124.729
//!   1.23456789    1.23         1.2345679     <-- f32 noise
//!   1234.5678     1234.57      1234.5677     <-- f32 noise, wrong last digit
//!   0.3333333     0.33         0.3333333
//! ```
//!
//! ⚠️ WHAT LOOKED LIKE A THIRD DEFECT WAS AN ARTIFACT OF MY OWN PROBE. Reading both engines through
//! the DOM, the reference's label came back as `"A\n10"` and ours as `"A10"`, which reads as the
//! name and the value run together. They are not: our `<text>` carries `<tspan dy="0">A</tspan>`
//! and `<tspan dy="20.70">10</tspan>`, so the value sits on its own line exactly as intended.
//! `textContent` simply concatenates tspans without a separator. Checking the emitted markup is what
//! settled it; a "fix" for that non-defect would have broken working output.
//!
//! ⚠️ THE SAME NUMBER LIVES IN TWO PLACES, AND THIS TEST FOUND THE SECOND ONE. A sankey link also
//! carries a label, and that label IS the flow value stored as the author's raw text — so the node
//! read `124.73` while the link beside it still read `124.729`, the identical quantity spelled two
//! ways in one picture, with the `f32` noise still visible on the link. The "raw value must not
//! survive" assertion below is what caught it; both sites now share `format_sankey_total`.
//!
//! ⚠️ A DIVERGENCE THIS BEAD DOES NOT CLOSE, MEASURED AND RECORDED. mermaid draws NO text on a
//! sankey link at all: for `A,B,10` the reference's drawn text is exactly `["A\n10", "B\n10"]`,
//! with no standalone `10`. We draw a link label it does not. That is an element-level behavioural
//! decision — removing published information — rather than a formatting one, so it is left for its
//! own bead instead of being folded in here on the way past.
//!
//! ⚠️ ONE RESIDUAL DIVERGENCE, PINNED AS A FORMAT RATHER THAN A DIGIT. `A,B,0.005` draws `0.01`
//! upstream. mermaid parses into `f64`, whose nearest value to 0.005 sits just above the exact half
//! and rounds up; `sankey_flow_value` parses into `f32`, whose nearest value can fall on the other
//! side. That is the IR's numeric width, not this formatter, and closing it means widening the
//! stored flow — an IR change, not a print. The test below asserts the SHAPE for that input and
//! deliberately does not assert a digit this code cannot promise.

/// The drawn text of an SVG, `<text>` bodies with nested tags stripped.
///
/// A `>` outside a tag is TEXT: the writer escapes `<` but leaves `>` literal (valid XML), so a
/// depth tracker consuming every `>` would eat real characters.
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
        let trimmed = text.trim().to_string();
        if !trimmed.is_empty() {
            out.push(trimmed);
        }
        rest = &rest[body_start + close..];
    }
    out
}

/// The node-label texts of a one-flow sankey, as the tspans spell them.
///
/// Each node label is a `<text>` of two tspans (name, then total), so the body concatenates to
/// `A124.73`. The total is taken as the trailing numeric run — reading the value the way a reader
/// sees it on the second line, without asserting on tspan structure this test is not about.
fn totals_drawn(flow: &str) -> Vec<String> {
    let source = format!("sankey-beta\n\nA,B,{flow}\n");
    let svg = fm_render_svg::render_svg(&fm_parser::parse(&source).ir);
    drawn_text(&svg)
        .into_iter()
        .filter_map(|text| {
            let digits: String = text
                .chars()
                .rev()
                .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
                .collect();
            (!digits.is_empty()).then(|| digits.chars().rev().collect::<String>())
        })
        .collect()
}

/// ⚠️ THE PLANTED NEGATIVE: a value with more than two decimals is rounded, not echoed.
///
/// This is the only input class that separates the fix from the old behaviour, and it catches the
/// worse half of the bug at the same time. `1234.5678` printed raw comes out `1234.5677` — the last
/// digit is `f32` error, not the author's data — so a test asserting only whole numbers or short
/// decimals passes against code that publishes float noise. Both the rounding and the noise are
/// covered by the same case.
#[test]
fn a_long_decimal_is_rounded_to_two_places() {
    for (flow, expected) in [
        ("124.729", "124.73"),
        ("1.23456789", "1.23"),
        ("1234.5678", "1234.57"),
        ("0.3333333", "0.33"),
    ] {
        let totals = totals_drawn(flow);
        assert!(
            totals.iter().any(|total| total == expected),
            "flow {flow}: expected the reference's {expected:?}, got {totals:?}"
        );
        // Name the failure directly: the raw value must not survive into the picture.
        assert!(
            !totals.iter().any(|total| total == flow),
            "flow {flow}: the raw stored value was published verbatim: {totals:?}"
        );
    }
}

/// ⚠️ THE MIRROR OVER-CORRECTION: a whole number keeps no `.00`.
///
/// The obvious way to round is `format!("{:.2}")` and stop, which draws `10.00` where mermaid draws
/// `10`. Pinning both directions means neither the un-rounded nor the over-padded implementation
/// passes.
#[test]
fn a_whole_total_is_drawn_without_a_decimal_point() {
    for flow in ["10", "1200"] {
        let totals = totals_drawn(flow);
        assert!(
            totals.iter().any(|total| total == flow),
            "flow {flow}: expected the bare integer, got {totals:?}"
        );
        assert!(
            !totals.iter().any(|total| total.contains('.')),
            "flow {flow}: a whole total grew a decimal point: {totals:?}"
        );
    }
}

/// A trailing zero inside the fraction is trimmed, and only inside the fraction.
///
/// `1200` is the case that catches a careless trim: stripping trailing `0`s from `1200.00` without
/// stopping at the dot would draw `12`, silently changing the number by two orders of magnitude.
/// That input is asserted in the test above; here the fractional trim itself is pinned.
#[test]
fn a_trailing_fractional_zero_is_trimmed() {
    let totals = totals_drawn("2.50");
    assert!(
        totals.iter().any(|total| total == "2.5"),
        "expected `2.5`, got {totals:?}"
    );
    assert!(
        !totals.iter().any(|total| total == "2.50"),
        "the trailing fractional zero survived: {totals:?}"
    );
}

/// The residual `f32` case: pinned as a SHAPE, not a digit.
///
/// `A,B,0.005` draws `0.01` upstream. We cannot promise that from an `f32` (see this file's header),
/// so asserting `0.01` would fail and asserting our current output would bless whichever side of the
/// half the float lands on. What IS promised is the format: at most two decimals, no raw
/// `0.005` echoed through. That is what this asserts, so the day the flow value widens to `f64` the
/// remaining gap is a one-line change here rather than a rediscovery.
#[test]
fn an_exact_half_is_still_formatted_even_where_the_digit_diverges() {
    let totals = totals_drawn("0.005");
    assert!(!totals.is_empty(), "no total was drawn at all");
    for total in &totals {
        assert!(
            !total.contains("0.005"),
            "the raw value escaped the formatter: {totals:?}"
        );
        if let Some((_, fraction)) = total.split_once('.') {
            assert!(
                fraction.len() <= 2,
                "more than two decimals were drawn: {totals:?}"
            );
        }
    }
}

/// CONTROL: the node NAME still draws, on its own tspan line.
///
/// The formatter feeds a two-line label. A change that mangled the join would take the name with it,
/// and a test that only inspects numbers would not notice.
#[test]
fn the_node_name_still_draws_beside_its_total() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse("sankey-beta\n\nA,B,124.729\n").ir);
    assert!(
        svg.contains(">A</tspan>"),
        "the node name is no longer drawn on its own tspan"
    );
    assert!(
        svg.contains(">124.73</tspan>"),
        "the total is not on its own tspan beside the name"
    );
}
