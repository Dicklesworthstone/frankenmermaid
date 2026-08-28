//! `escape_xml_text` has THREE implementations of one rule, and only a comment said they agree.
//!
//! `write_escaped_text` dispatches on the input before escaping anything:
//!
//! ```text
//!   len >= 256 && no `]]>`   SIMD memchr2 over `&` and `<` only, bulk-copying the runs between
//!   no `&` `<` `>` at all    the string verbatim, one write
//!   otherwise                a per-byte loop escaping `&`, `<`, and `>` — the last ONLY after `]]`
//! ```
//!
//! Each is annotated "byte-identical" in the source, which is a claim, not a check. Nothing executed
//! the claim: a change to one path diverges from the others silently, and — because the split is on
//! LENGTH — the divergence appears only for long labels, which is exactly the shape of defect that
//! ships. This file executes the claim.
//!
//! THE CONTRACT, read off the implementation and confirmed against the rendered document:
//!
//! ```text
//!   &            ->  &amp;      always
//!   <            ->  &lt;       always
//!   >            ->  &gt;       ONLY when preceded by `]]`
//!   everything else             verbatim, including a lone `>` and a bogus `&notreal;`
//! ```
//!
//! ⚠️ THE `>` RULE IS THE INTERESTING ONE. A bare `>` in character data is legal XML and is left
//! alone; the sequence `]]>` is NOT legal in character data and must be broken up. That is why the
//! SIMD path — which never inspects `>` — is guarded by `!s.contains("]]>")`, and why removing that
//! guard is a silent correctness bug reachable only above 256 bytes.
//!
//! Verified against a real parse before these expectations were written: rendering a node whose
//! label contains `&`, `<`, `>`, `"` and `'` and re-parsing the SVG with `DOMParser` in Chromium 151
//! yields no `parsererror` and returns the author's text unchanged.

use fm_render_svg::escape_xml_text;

/// Padding that contains no character the escaper treats specially, used to push an input over the
/// 256-byte SIMD threshold without changing what there is to escape.
fn pad(len: usize) -> String {
    "a".repeat(len)
}

/// ⚠️ THE PLANTED NEGATIVE: the same content must escape identically either side of the 256-byte
/// dispatch boundary.
///
/// This is the assertion a naive test cannot make, because a naive test uses short strings only and
/// every short string takes the per-byte loop. Any edit to the SIMD path — adding a rule, dropping
/// one, reordering the replacements — passes a short-input suite completely while corrupting every
/// long label. The pairs below are byte-identical in their special characters and differ only in how
/// much inert padding surrounds them, so a divergence can only come from the dispatch.
#[test]
fn the_simd_and_byte_paths_escape_identically() {
    for fragment in ["a & b", "a < b", "x & y < z", "&amp;", "&notreal;", "a > b"] {
        let short = format!("{fragment}");
        assert!(
            short.len() < 256,
            "the SHORT case must stay under the dispatch threshold to exercise the other path"
        );

        // Same fragment, but long enough to take the SIMD path.
        let long = format!("{}{fragment}{}", pad(200), pad(200));
        assert!(long.len() >= 256, "the LONG case must cross the threshold");

        let escaped_short = escape_xml_text(&short);
        let escaped_long = escape_xml_text(&long);
        let expected_long = format!("{}{escaped_short}{}", pad(200), pad(200));
        assert_eq!(
            escaped_long, expected_long,
            "fragment {fragment:?} escapes differently above and below the 256-byte SIMD threshold"
        );
    }
}

/// ⚠️ THE SECOND PLANTED NEGATIVE: `]]>` must be broken up even in a LONG string.
///
/// The SIMD path never inspects `>`, so it is guarded by `!s.contains("]]>")`. Remove that guard —
/// a plausible "simplification", since the guard costs a substring search on every long write — and
/// this input silently emits a raw `]]>` into character data, which is not well-formed XML. No short
/// input can detect it, because short inputs never reach that path.
#[test]
fn a_cdata_terminator_is_escaped_at_any_length() {
    for (case, input) in [
        ("short", "a]]>b".to_string()),
        ("long", format!("{}a]]>b{}", pad(200), pad(200))),
    ] {
        let escaped = escape_xml_text(&input);
        assert!(
            escaped.contains("]]&gt;"),
            "{case}: the CDATA terminator was not broken up: {escaped:.80}"
        );
        assert!(
            !escaped.contains("]]>"),
            "{case}: a raw `]]>` survived into character data, which is not well-formed XML"
        );
    }
}

/// A lone `>` is legal character data and must NOT be escaped.
///
/// The mirror of the case above: over-escaping is as wrong as under-escaping, and it is what an
/// implementation written from "escape the XML specials" rather than from the spec produces. It also
/// pins the invariant this session's conformance tests rely on when parsing drawn text back out —
/// several of them treat a `>` outside a tag as text precisely because of this rule.
#[test]
fn a_lone_greater_than_is_left_alone() {
    for (input, expected) in [
        ("a > b", "a > b"),
        ("if x>y", "if x>y"),
        ("]x>y", "]x>y"),
        // Only TWO preceding brackets trigger it; one does not.
        ("]]>", "]]&gt;"),
    ] {
        assert_eq!(escape_xml_text(input), expected, "input {input:?}");
    }
}

/// `&` is always escaped, and never twice.
///
/// Double-escaping is the classic ordering bug: replace `<` with `&lt;` first, then replace `&` with
/// `&amp;`, and the just-written entity becomes `&amp;lt;`. The reader then sees the literal text
/// `&lt;` instead of `<`. Asserting the absence of `&amp;amp;` and `&amp;lt;` names that failure
/// directly rather than only checking the success spelling.
#[test]
fn ampersands_are_escaped_exactly_once() {
    assert_eq!(escape_xml_text("Tom & Jerry"), "Tom &amp; Jerry");
    assert_eq!(escape_xml_text("a < b"), "a &lt; b");

    let both = escape_xml_text("& <");
    assert_eq!(both, "&amp; &lt;");
    assert!(
        !both.contains("&amp;lt;") && !both.contains("&amp;amp;"),
        "the escaper ran `<` before `&` and double-escaped its own output: {both}"
    );

    // An author-written entity is TEXT, not markup: it is escaped like any other `&`, so the reader
    // sees the characters the author typed rather than a decoded symbol.
    assert_eq!(escape_xml_text("a &amp; b"), "a &amp;amp; b");
    assert_eq!(escape_xml_text("&notreal;"), "&amp;notreal;");
}

/// Every `&` in the output opens a recognised entity — the machine-checkable half of "well-formed".
///
/// A bare `&` is the single most common way to emit markup that fails to parse, and it is invisible
/// in a substring assertion. This walks the escaped output of a hostile input and requires each `&`
/// to begin one of the entities this escaper emits, at both lengths.
#[test]
fn no_bare_ampersand_survives_at_either_length() {
    let hostile = "& < > \" ' &amp; ]]> &#35; <<>>";
    for (case, input) in [
        ("short", hostile.to_string()),
        ("long", format!("{}{hostile}{}", pad(200), pad(200))),
    ] {
        let escaped = escape_xml_text(&input);
        let bytes = escaped.as_bytes();
        for (index, _) in escaped.match_indices('&') {
            let tail = &escaped[index..];
            assert!(
                tail.starts_with("&amp;") || tail.starts_with("&lt;") || tail.starts_with("&gt;"),
                "{case}: a bare `&` at byte {index} does not open a known entity: {rest:?}",
                rest = &escaped[index..(index + 12).min(bytes.len())]
            );
        }
    }
}

/// CONTROL: a string with nothing to escape is returned unchanged, at both lengths.
///
/// The third dispatch branch. Without this the two above could both pass while the "no specials"
/// fast path mangled ordinary text — the most common input of all, and the one no hostile-input test
/// ever exercises.
#[test]
fn ordinary_text_is_untouched_at_either_length() {
    for input in [
        "Plain label".to_string(),
        "日本語 ünïcødé 🎉".to_string(),
        pad(300),
        format!("{} plain {}", pad(150), pad(150)),
    ] {
        assert_eq!(
            escape_xml_text(&input),
            input,
            "ordinary text was altered by the escaper"
        );
    }
}
