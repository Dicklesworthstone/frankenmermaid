//! Differential test: how mermaid labels a `packet-beta` field's bit range.
//!
//! THE DIVERGENCE THIS PINS. mermaid draws each field as THREE text runs — the name, the first bit
//! and the last bit — so the numbers read as a scale beside the field. We baked the range into the
//! name and drew one run. Found by `scripts/headtohead/drawn_text_diff.mjs`:
//!
//! ```text
//!   mermaid draws, we do NOT: ["Source Port","0","15","Destination Port","16","31", …]
//!   we draw, mermaid does NOT: ["Source Port\n[0-15]","Destination Port\n[16-31]", …]
//! ```
//!
//! REFERENCE BEHAVIOUR, probed against the pinned 11.15.0 bundle:
//!
//! ```text
//!   0-3: "A"      ->  ["A","0","3"]     name, then start, then end
//!   0: "Flag"     ->  ["Flag","0"]      ONE number when the field is a single bit
//! ```
//!
//! ⚠️ THE SINGLE-BIT CASE IS THE DISCRIMINATING ONE. Emitting both ends unconditionally prints `0`
//! twice on every one-bit flag, and every multi-bit fixture agrees with that wrong rule — including
//! the project's own `packet_basic`, whose eight fields all span more than one bit.
//!
//! NOT FIXED HERE, and filed separately: a single-bit field's box is too narrow for its name, so
//! `0: "Flag"` draws `Fl…` at a shrunken font size. That is a pre-existing eliding defect on the
//! narrow-box path — the label was already the bare name before this change — and it is why
//! `drawn_text_diff` still reports a divergence for a one-bit fixture.

/// Every `<text>` leaf, in document order.
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
        let text = rest[open + 1..close].trim();
        if !text.is_empty() && !text.contains('<') {
            out.push(text.to_string());
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

#[test]
fn a_field_draws_its_name_and_both_bit_ends() {
    let drawn = runs("packet-beta\n0-3: \"A\"\n4-7: \"B\"\n");
    for expected in ["A", "0", "3", "B", "4", "7"] {
        assert!(
            drawn.iter().any(|run| run == expected),
            "{expected:?} is missing from {drawn:?}"
        );
    }
}

/// ⚠️ NEGATIVE CONTROL for the OLD shape: the range must not be baked into the name.
#[test]
fn the_range_is_not_part_of_the_name() {
    let drawn = runs("packet-beta\n0-3: \"A\"\n4-7: \"B\"\n");
    assert!(
        !drawn.iter().any(|run| run.contains('[')),
        "a field name still carries its bracketed range: {drawn:?}"
    );
    assert!(
        drawn.iter().any(|run| run == "A"),
        "the bare field name is gone entirely: {drawn:?}"
    );
}

/// ⚠️ THE DISCRIMINATING CASE. A single-bit field gets ONE number. An implementation that always
/// emits both ends passes every multi-bit fixture — including `packet_basic`, whose eight fields all
/// span more than one bit — and prints `0` twice here.
#[test]
fn a_single_bit_field_is_marked_once() {
    let drawn = runs("packet-beta\n0: \"Flag\"\n1-7: \"Rest\"\n");
    let zeroes = drawn.iter().filter(|run| *run == "0").count();
    assert_eq!(
        zeroes, 1,
        "bit 0 is a single-bit field and must be marked once, not at both ends: {drawn:?}"
    );
    // ...and the neighbouring multi-bit field still gets both of its ends.
    for expected in ["1", "7"] {
        assert!(
            drawn.iter().any(|run| run == expected),
            "the multi-bit field lost its {expected:?} marking: {drawn:?}"
        );
    }
}

/// CONTROL: no other diagram type gains bit markings. A pass that is not gated on the packet meta
/// would stamp stray numbers on every diagram.
#[test]
fn only_packet_diagrams_gain_bit_markings() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse("flowchart LR\n  A-->B\n").ir);
    assert!(
        !svg.contains("fm-packet-bit"),
        "a flowchart emitted packet bit markings"
    );
}
