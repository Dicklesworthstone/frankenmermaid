//! The terminal renderer emits PLAIN TEXT, and that is a decision rather than a gap (bd-lvj3).
//!
//! bd-lvj3 measured that the canvas ignored every user styling channel and counted the reads to
//! prove it. The same count called the terminal a PARTIAL case — `classes 4`, `inline_style 0`,
//! `style_refs 0` — and left it open as "worth its own measurement rather than assuming it is
//! either fine or equally broken". This file is that measurement, and the answer is that the
//! terminal is not a partial implementation of canvas-style colouring at all:
//!
//!   * Its `classes` reads are `is_block_beta_space_node`, which asks whether a node is a
//!     block-beta SPACER. That is structure, not style. The count that made the terminal look
//!     half-finished was reading a different feature.
//!   * The main render path has no colour channel by design. `config.rs` states it: colour output
//!     "must be OPT-IN because `-f term` is routinely piped to a file and escape bytes there are
//!     corruption, not colour". `MinimapConfig::use_color` defaults to `false`, and colour is
//!     confined to the minimap and the diff summary.
//!   * The renderer actively STRIPS escape bytes out of user text, which is an injection defence:
//!     a label is attacker-controlled in any tool that renders someone else's diagram, and an
//!     unfiltered `\x1b]0;…\x07` or CSI sequence retitles the window or moves the cursor.
//!
//! So "the terminal ignores styling" and "the canvas ignores styling" are not the same finding,
//! and the fix that was right for the canvas — resolve the declared colour and paint with it —
//! would be a REGRESSION here: it corrupts every piped `-f term` capture. These tests exist so
//! that someone closing the terminal half of bd-lvj3 discovers that from a failing test rather
//! than from a user whose log file filled with escape bytes.

use fm_core::MermaidDiagramIr;
use fm_render_term::render_term;

/// Declares all three styling channels the canvas half of bd-lvj3 had to learn to read.
const STYLED: &str = "flowchart TD\n  a[Alpha] --> b[Beta]\n  style a fill:#ff0000,color:#00ff00\n  classDef hot fill:#0000ff\n  class b hot\n";

fn ir(source: &str) -> MermaidDiagramIr {
    fm_parser::parse(source).ir
}

/// CONTROL, and the one that makes every other assertion in this file mean something.
///
/// "The output contains no colour" is trivially true of an IR that never carried any. This asserts
/// the declarations SURVIVED PARSING, so the terminal's plain output is a choice about real data.
/// Without it, a parser regression that dropped `style` and `classDef` entirely would make the
/// tests below pass more easily, which is the wrong direction for a test to fail in.
#[test]
fn the_fixture_really_declares_styling() {
    let parsed = ir(STYLED);

    assert!(
        !parsed.style_refs.is_empty(),
        "the fixture's style/classDef directives never reached the IR, so the assertions about \
         the terminal dropping them prove nothing"
    );
    assert!(
        parsed.nodes.iter().any(|node| !node.classes.is_empty()),
        "`class b hot` never reached the IR, so the classDef channel is untested here"
    );
}

/// A styled diagram renders to the terminal with NO escape bytes at all.
#[test]
fn declared_colour_never_becomes_an_escape_sequence() {
    let output = render_term(&ir(STYLED));

    // NON-VACUITY: an empty render would satisfy every scan below.
    assert!(
        output.contains("Alpha") && output.contains("Beta"),
        "the diagram did not render its own node labels, so the scans below are vacuous: {output}"
    );
    assert!(
        !output.contains('\u{1b}'),
        "the terminal emitted an escape byte; `-f term` is routinely piped to a file, where that \
         is corruption rather than colour"
    );
    // The declared colours must not appear as TEXT either. Stripping the escape while writing
    // `#ff0000` into the diagram body would pass the check above and still be wrong.
    for declared in ["ff0000", "0000ff", "00ff00"] {
        assert!(
            !output.contains(declared),
            "the declared colour {declared:?} was written into the terminal output as text"
        );
    }
}

/// An escape sequence in a LABEL is stripped, not forwarded.
///
/// This is the security half and it is the reason the contract above is worth pinning. A label is
/// attacker-controlled whenever a tool renders a diagram someone else wrote. `renderer.rs` has a
/// unit test for this on a constructed label; this one drives the whole public path from source
/// text, which is where a future "terminal styling" change would actually land.
#[test]
fn an_escape_sequence_in_a_label_does_not_reach_the_terminal() {
    let hostile = "flowchart TD\n  a[Safe\u{1b}[31mText] --> b[Plain]\n";
    let output = render_term(&ir(hostile));

    assert!(
        output.contains("Plain"),
        "the diagram did not render, so this proves nothing: {output}"
    );
    assert!(
        !output.contains('\u{1b}'),
        "an escape sequence embedded in a node label reached the terminal output"
    );
}

/// CONTROL: an UNSTYLED diagram renders the same body as the styled one.
///
/// This separates "the terminal drops styling" from "the terminal chokes on styling". If a
/// declaration changed the geometry or truncated the labels, the renderer would be reacting to
/// style in a way it should not, and neither the escape scan nor the colour scan would notice.
#[test]
fn styling_does_not_perturb_the_rendered_body() {
    let styled = render_term(&ir(STYLED));
    let plain = render_term(&ir("flowchart TD\n  a[Alpha] --> b[Beta]\n"));

    assert_eq!(
        styled, plain,
        "declaring style/classDef changed the terminal rendering; the terminal is supposed to \
         ignore these channels, not react to them"
    );
}

/// A click TOOLTIP is not drawn into the terminal body (bd-bk7h).
///
/// `click a "url" "some tooltip"` populates `IrNodeInteraction.tooltip`, and fm-render-svg emits it
/// as a `title` attribute — which is what the incumbent does (`t.tooltip && n.attr("title", ...)`)
/// and what a browser shows ON HOVER.
///
/// ⚠️ A TERMINAL HAS NO HOVER, and that makes "render it as visible text" a DIVERGENCE rather than
/// the missing half of the feature. mermaid never shows a tooltip until the user points at the
/// node; a terminal that printed it unconditionally would display text the incumbent does not, in
/// every diagram that uses `click`, forever. The bead recorded this as a product decision rather
/// than an omission and did not take it. This pins the decision so it cannot be "fixed" by
/// accident.
///
/// Note the terminal DIFF is a separate surface and legitimately reports `TooltipChanged`:
/// comparing two IRs is not rendering one.
#[test]
fn a_click_tooltip_is_not_drawn_into_the_terminal_body() {
    let source =
        "flowchart TD\n  a[Alpha] --> b[Beta]\n  click a \"https://example.com\" \"HOVERTEXT\"\n";
    let output = render_term(&ir(source));

    assert!(
        output.contains("Alpha") && output.contains("Beta"),
        "the diagram did not render, so this proves nothing: {output}"
    );
    assert!(
        !output.contains("HOVERTEXT"),
        "a click tooltip was printed into the terminal body; the incumbent shows it on HOVER \
         only, so printing it unconditionally shows text mermaid never shows: {output}"
    );
    assert!(
        !output.contains("example.com"),
        "a click URL was printed into the terminal body: {output}"
    );
}
