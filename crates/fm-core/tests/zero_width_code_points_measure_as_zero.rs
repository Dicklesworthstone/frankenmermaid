//! Zero-width code points must not add width, or a label's box is sized for glyphs nobody draws.
//!
//! Text width drives node box geometry, so a measurement error here is a layout error everywhere the
//! character appears. `CharWidthClass::classify` assigns `Normal` (multiplier 1.0) to every code
//! point it does not name, and `FullWidth` (2.0) to everything in `0x1F300..=0x1F9FF`. Several
//! classes of code point render at ZERO advance width and fall into those buckets anyway:
//!
//! ```text
//!   U+0300..U+036F   combining marks       `e` + U+0301 draws as `é`, one glyph, one advance
//!   U+200D           zero-width joiner     glues emoji into one glyph; draws nothing itself
//!   U+FE0E / U+FE0F  variation selectors   select a presentation form; draw nothing
//!   U+1F3FB..U+1F3FF skin tone modifiers   modify the preceding emoji; inside the FullWidth range
//!   U+200B           zero-width space      a line-break opportunity, not a glyph
//! ```
//!
//! ⚠️ THE EXISTING SUITE ALREADY MEASURES ONE OF THESE AND CANNOT FAIL ON IT.
//! `mixed_script_width_matches_the_per_char_reference_bit_for_bit` in `font_metrics.rs` measures
//! `"Cafe\u{301}"` — the exact defect input — but asserts only that the ASCII-mixed fast path agrees
//! with the per-character reference. Both compute the same wrong number, so the assertion holds
//! while the width is wrong. That test is about a performance refactor being bit-identical, which is
//! a real thing to check; it is simply blind to whether the shared answer is correct. This file
//! asserts the answer.
//!
//! THE ORACLE IS CANONICAL EQUIVALENCE, not a number copied from mermaid-js. `Café` has two Unicode
//! spellings — NFC `caf\u{e9}` and NFD `cafe\u{301}` — that are the same string by definition and
//! draw the same four glyphs. Whatever width the engine assigns, it must assign the SAME width to
//! both, and that is decidable here without measuring a browser. mermaid-js gets this for free by
//! measuring a real DOM text node; this engine estimates, so it must be checked.

use fm_core::{CharWidthClass, FontMetrics};

/// The NFC and NFD spellings of a label are the same text and must measure the same.
///
/// ⚠️ PLANTED NEGATIVE: this is the assertion that fails today's `classify`, which gives a combining
/// mark `Normal` (1.0) and so measures the NFD spelling one full character wider than the NFC one.
/// A naive fix that special-cases only U+0301 passes the first pair and fails the rest.
#[test]
fn canonically_equivalent_spellings_measure_identically() {
    let metrics = FontMetrics::default_metrics();

    for (name, nfc, nfd) in [
        ("e-acute", "Café", "Cafe\u{301}"),
        ("a-umlaut", "Häuser", "Ha\u{308}user"),
        ("n-tilde", "mañana", "man\u{303}ana"),
        ("o-circumflex", "hôtel", "ho\u{302}tel"),
        ("c-cedilla", "façade", "fac\u{327}ade"),
        // Two marks on one base: a fix that handles a single mark still has to handle a stack.
        ("stacked marks", "ệ", "e\u{323}\u{302}"),
    ] {
        let width_nfc = metrics.estimate_width(nfc);
        let width_nfd = metrics.estimate_width(nfd);
        assert!(
            (width_nfc - width_nfd).abs() < 0.001,
            "{name}: the two Unicode spellings of the same label measure differently — \
             NFC {nfc:?} = {width_nfc}, NFD {nfd:?} = {width_nfd}. The box is sized for a glyph \
             that is never drawn, so the same label renders in two different-sized nodes \
             depending on how the author's editor normalised it."
        );
    }
}

/// A combining mark adds no advance width of its own.
///
/// The direct form of the property above, stated against the base character rather than against
/// another spelling — so a "fix" that made BOTH spellings equally wrong would still fail here.
#[test]
fn a_combining_mark_adds_no_width() {
    let metrics = FontMetrics::default_metrics();

    for (name, base, decomposed) in [
        ("acute", "e", "e\u{301}"),
        ("grave", "a", "a\u{300}"),
        ("diaeresis", "u", "u\u{308}"),
        ("ring above", "a", "a\u{30A}"),
        ("cedilla", "c", "c\u{327}"),
    ] {
        let plain = metrics.estimate_width(base);
        let marked = metrics.estimate_width(decomposed);
        assert!(
            (plain - marked).abs() < 0.001,
            "{name}: adding a combining mark to {base:?} changed its width from {plain} to \
             {marked}. A combining mark draws on top of its base and advances the pen by zero."
        );
    }
}

/// A ZWJ emoji sequence is one glyph, not one glyph per code point.
///
/// ⚠️ THE WORST CASE, and the reason this is a layout bug rather than a rounding quibble. A family
/// emoji is five code points — three emoji joined by two ZWJs — and every one of them is currently
/// billed: the emoji at `FullWidth` (2.0 each) and the joiners at `Normal` (1.0 each). The sequence
/// draws as ONE glyph roughly two units wide, so the estimate is off by a factor of four, and the
/// node box is sized for a label four times the width of the one the reader sees.
#[test]
fn a_zwj_sequence_measures_as_one_glyph() {
    let metrics = FontMetrics::default_metrics();
    let single = metrics.estimate_width("👩");

    for (name, sequence) in [
        ("family", "👨\u{200D}👩\u{200D}👧"),
        ("couple", "👩\u{200D}❤\u{FE0F}\u{200D}👨"),
        ("professional", "👩\u{200D}💻"),
    ] {
        let width = metrics.estimate_width(sequence);
        assert!(
            width <= single * 2.0 + 0.001,
            "{name}: the sequence {sequence:?} measured {width}, more than twice a single emoji \
             ({single}). Every code point in the sequence is being billed separately, but the \
             sequence renders as one glyph."
        );
    }
}

/// Zero-width formatting code points are free.
///
/// These carry no glyph at all. A variation selector picks a presentation form of the PRECEDING
/// character, a skin-tone modifier recolours it, and a zero-width space is a break opportunity —
/// none of them advance the pen.
///
/// ⚠️ The skin-tone case is the one a range-based fix gets wrong: U+1F3FB..U+1F3FF sits INSIDE the
/// `0x1F300..=0x1F9FF` block that `is_east_asian_wide` reports as full-width, so it is currently
/// billed 2.0 — the widest possible answer for a character that draws nothing.
#[test]
fn zero_width_formatting_code_points_add_no_width() {
    let metrics = FontMetrics::default_metrics();

    for (name, plain, decorated) in [
        ("variation selector 16", "☀", "☀\u{FE0F}"),
        ("variation selector 15", "☀", "☀\u{FE0E}"),
        ("skin tone", "👋", "👋\u{1F3FD}"),
        ("zero-width space", "ab", "a\u{200B}b"),
        ("zero-width joiner alone", "ab", "a\u{200D}b"),
    ] {
        let width_plain = metrics.estimate_width(plain);
        let width_decorated = metrics.estimate_width(decorated);
        assert!(
            (width_plain - width_decorated).abs() < 0.001,
            "{name}: {decorated:?} measured {width_decorated} against {width_plain} for {plain:?}. \
             This code point draws nothing and must not advance the pen."
        );
    }
}

/// CONTROL: the characters that SHOULD have width still do.
///
/// ⚠️ Without this, every assertion above is satisfiable by returning 0.0 for everything — the
/// cheapest wrong fix, and one that would silently collapse every node box in the engine to nothing.
/// This pins the other side: ordinary text, CJK, and a lone emoji all keep their widths, and the
/// full-width classes stay wider than the narrow ones.
#[test]
fn characters_that_draw_still_measure() {
    let metrics = FontMetrics::default_metrics();
    let avg = metrics.avg_char_width();

    assert!(
        metrics.estimate_width("Hello") > avg * 3.0,
        "ordinary ASCII text lost its width"
    );
    assert!(
        metrics.estimate_width("日本語") > metrics.estimate_width("abc"),
        "CJK must stay wider than the same count of ASCII characters"
    );
    assert!(
        metrics.estimate_width("👋") > avg,
        "a lone emoji must still have width — it draws a glyph"
    );
    assert!(
        metrics.estimate_width("W") > metrics.estimate_width("i"),
        "the narrow/wide classes collapsed into each other"
    );

    // The classification itself, not just the sums: a zero multiplier must be reachable ONLY for
    // the classes that draw nothing.
    assert_eq!(CharWidthClass::classify('a').multiplier(), 1.0);
    assert_eq!(CharWidthClass::classify('日').multiplier(), 2.0);
    assert!(
        CharWidthClass::classify('👋').multiplier() > 0.0,
        "a drawn emoji was classified as zero-width"
    );
}
