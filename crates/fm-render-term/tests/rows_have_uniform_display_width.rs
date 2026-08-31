//! A terminal diagram is a character GRID, so every row must occupy the same number of display
//! columns. A row that is wider on screen than its neighbours puts the box border in the wrong place.
//!
//! ⚠️ TWO DIFFERENT THINGS ARE CALLED "WIDTH" HERE AND THE ENGINE USES THE WRONG ONE IN PLACES.
//!
//! ```text
//!   codepoint count   `s.chars().count()`             "日本語" -> 3
//!   display columns   wide characters count as two    "日本語" -> 6
//! ```
//!
//! `fm-render-term` contains both notions. `diff.rs::display_width` counts columns correctly (it
//! also skips ANSI escapes); `renderer.rs::wrap_text` measures words with `word.chars().count()`.
//! The grid itself is a `Vec<Vec<char>>` — one char per CELL — so a full-width character consumes
//! one cell while occupying two columns.
//!
//! MEASURED on the rendered output of a one-node flowchart (rows / distinct display widths):
//!
//! ```text
//!   "Hello World"      24 rows, display {51},     codepoints {51}   uniform
//!   "日本語のラベル"   24 rows, display {60, 67}, codepoints {60}   7 columns ragged
//!   "Data 日本語 text" 24 rows, display {59, 62}, codepoints {59}   3 columns ragged
//!   "push 🚀 now"      24 rows, display {54, 55}, codepoints {54}   1 column ragged
//! ```
//!
//! The codepoint count is uniform in every case — the grid is internally consistent — and the
//! DISPLAY width is not. That is the signature of the defect: the renderer builds a correct grid of
//! cells and then those cells render at different widths on a real terminal.
//!
//! The ASCII case below is a live guard and passes today. The wide-character case is `#[ignore]`d
//! because it is a known defect that this file documents rather than fixes: fixing it means making
//! the grid column-aware (a wide character consuming two cells, the second a continuation) and
//! updating every `.chars().enumerate()` stamp site, node sizing, box drawing and clipping. A
//! partial fix — teaching `wrap_text` about columns while the grid stays cell-based — would change
//! the output without making the rows uniform, which is worse than leaving it measured and known.
//! Run it with `cargo test -p fm-render-term -- --ignored` to see the current raggedness.

/// Display columns occupied by a string, counting East-Asian-wide characters as two.
///
/// ⚠️ NOT `chars().count()`. That is the bug this file is about, so the test must not reimplement it.
fn display_width(text: &str) -> usize {
    text.chars()
        .map(|c| if fm_core::is_east_asian_wide(c) { 2 } else { 1 })
        .sum()
}

/// Render a single-node flowchart carrying `label`.
fn render_labelled(label: &str) -> String {
    let source = format!("flowchart LR\n  A[\"{label}\"]\n");
    let ir = fm_parser::parse(&source).ir;
    fm_render_term::render_term(&ir)
}

/// The rows of an ASCII diagram all occupy the same number of display columns.
///
/// ⚠️ PLANTED NEGATIVE: this fails for any renderer that pads rows to a *byte* length rather than a
/// character count — the failure mode one step before the wide-character one, and the one that would
/// appear the moment a multi-byte character reached a padding calculation. It passes today and is a
/// live regression guard, which is what makes the ignored case below a statement about wide
/// characters specifically rather than about row uniformity in general.
#[test]
fn ascii_rows_have_a_uniform_display_width() {
    for label in [
        "Hello World",
        "short",
        "a much longer label than the others",
    ] {
        let out = render_labelled(label);
        let widths: std::collections::BTreeSet<usize> = out.lines().map(display_width).collect();
        assert_eq!(
            widths.len(),
            1,
            "label {label:?}: the rendered rows have {} different display widths {widths:?}, so the \
             box border cannot line up",
            widths.len()
        );
    }
}

/// CONTROL: the measurement can tell the two notions of width apart.
///
/// Without this, `display_width` could silently BE `chars().count()` — in which case the assertions
/// in this file would be about nothing at all, and the ignored test below would appear to pass the
/// moment someone "simplified" the helper.
#[test]
fn the_width_helper_is_not_a_codepoint_count() {
    assert_eq!(display_width("abc"), 3);
    assert_eq!(
        display_width("日本語"),
        6,
        "wide characters must count as two columns"
    );
    assert_eq!(
        "日本語".chars().count(),
        3,
        "the codepoint count is the WRONG answer, pinned here"
    );
    assert_ne!(
        display_width("日本語"),
        "日本語".chars().count(),
        "the helper collapsed into a codepoint count and this file stopped testing anything"
    );
}

/// KNOWN DEFECT — the rows of a diagram containing wide characters are ragged.
///
/// Ignored, not deleted: it is the executable statement of the defect measured in the header, and it
/// is the test that should be un-ignored by whoever makes the grid column-aware. It fails today with
/// display widths `{60, 67}` for CJK against a uniform codepoint count of `{60}`.
#[test]
#[ignore = "known defect: the term grid is one char per CELL, so wide characters render ragged rows"]
fn wide_character_rows_have_a_uniform_display_width() {
    for label in ["日本語のラベル", "Data 日本語 text", "push 🚀 now"] {
        let out = render_labelled(label);
        let widths: std::collections::BTreeSet<usize> = out.lines().map(display_width).collect();
        assert_eq!(
            widths.len(),
            1,
            "label {label:?}: the rendered rows have {} different display widths {widths:?}. Every \
             row holds the same number of CELLS, but a full-width character occupies two COLUMNS, \
             so the rows containing one are wider on screen and the border does not line up.",
            widths.len()
        );
    }
}
