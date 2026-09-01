//! A relation marked at BOTH ends must say so, not just draw it (bd-f9t0r).
//!
//! `6cc9dd43` made `Alpha o--* Beta` draw both the aggregation diamond and the composition diamond.
//! The accessible text still described one `ArrowType`, so the far diamond was drawn and never
//! spoken: a screen-reader user was told "Alpha aggregates Beta" about a picture that also states a
//! composition. The two accessibility surfaces and the drawing have to agree.
//!
//! ⚠️ THIS DRIVES ITSELF FROM THE PARSER, NOT FROM A LIST OF SPELLINGS. Which relations carry a far
//! marker is `class_relation_co_arrow`'s answer, and restating it here would let the two drift --
//! the test would keep passing against a table that no longer matches. Each case below asks the IR
//! whether the edge HAS a `co_arrow` and only then requires the rendered text to name two ends, so
//! a spelling that stops carrying one fails as a missing marker rather than silently passing.
//!
//! The exact WORDS are pinned by `a_far_end_marker_is_named_with_the_word_its_own_end_would_use` in
//! `a11y.rs`, which can read the phrase table directly. What this file adds is the wiring: that the
//! phrase actually reaches the emitted SVG, which a unit test on the formatter cannot show.

use fm_render_svg::{A11yConfig, SvgRenderConfig, render_svg_with_config};

/// Both-ends spellings, one per distinct co-arrow shape, with the two words each must speak.
///
/// `<--`/`<..` are deliberately absent: a start-side dependency maps onto the forward arrow and
/// relies on the endpoints being swapped, so it has no far marker to name. That exception is
/// `class_relation_co_arrow`'s, and it is stated here rather than left to be inferred from a gap.
const BOTH_ENDS: &[(&str, &str, &str)] = &[
    ("o--o", "aggregates", "is aggregated by"),
    ("o--*", "aggregates", "composes"),
    ("*--o", "is composed of", "is aggregated by"),
    ("<|--|>", "is inherited by", "inherits"),
    ("o--|>", "aggregates", "inherits"),
    ("*..*", "is composed of", "composes"),
];

fn render(source: &str) -> String {
    render_svg_with_config(
        &fm_parser::parse(source).ir,
        &SvgRenderConfig {
            a11y: A11yConfig::full(),
            ..SvgRenderConfig::default()
        },
    )
}

#[test]
fn a_relation_marked_at_both_ends_names_both_ends_in_its_accessible_text() {
    let mut wrong = Vec::new();
    for (op, near, far) in BOTH_ENDS {
        let source = format!("classDiagram\n  Alpha {op} Beta\n");

        // The premise: this spelling really does carry a far marker. If the parser stops producing
        // one, that is the failure to report -- not a missing phrase downstream of it.
        let ir = fm_parser::parse(&source).ir;
        let Some(edge) = ir.edges.first() else {
            wrong.push(format!("  {op:<7} produced NO EDGE"));
            continue;
        };
        if edge.co_arrow().is_none() {
            wrong.push(format!("  {op:<7} carries no co_arrow, so it cannot name a far end"));
            continue;
        }

        let svg = render(&source);
        if !svg.contains(near) || !svg.contains(far) {
            wrong.push(format!(
                "  {op:<7} missing {}: expected both {near:?} and {far:?}",
                if svg.contains(near) { "the FAR phrase" } else { "the NEAR phrase" },
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} both-ends spelling(s) do not speak both ends:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// A single-ended relation must NOT gain a second phrase.
///
/// The negative half: a change that made every relation say two things would pass the test above
/// while making every ordinary diagram's accessible text wrong.
#[test]
fn a_single_ended_relation_still_names_exactly_one_end() {
    for (op, phrase, absent) in [
        ("o--", "aggregates", "is aggregated by"),
        ("*--", "is composed of", "composes"),
        ("<|--", "is inherited by", "inherits"),
    ] {
        let source = format!("classDiagram\n  Alpha {op} Beta\n");
        let ir = fm_parser::parse(&source).ir;
        let edge = ir
            .edges
            .first()
            .unwrap_or_else(|| panic!("`{op}` produced no edge"));
        assert!(
            edge.co_arrow().is_none(),
            "`{op}` is single-ended but carries a co_arrow"
        );
        let svg = render(&source);
        assert!(svg.contains(phrase), "`{op}` lost its own phrase {phrase:?}");
        assert!(
            !svg.contains(absent),
            "`{op}` gained a far-end phrase {absent:?} it has no marker for"
        );
    }
}
