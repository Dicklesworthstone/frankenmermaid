//! An ER attribute's comment must be drawn on BOTH SVG paths, not just the streaming one.
//!
//! THE DIVERGENCE, and it is an internal one: this renderer has two writers for an ER attribute row.
//! The streaming writer appended the attribute's comment; the `Element` writer did not. So
//!
//! ```text
//!   erDiagram
//!       USER { string name "the display name" }
//! ```
//!
//! drew the comment under the default config and DROPPED it whenever `embed_theme_css` was off — a
//! rendering switch with nothing to do with content deciding whether the author's text appears at
//! all. Measured before the fix:
//!
//! ```text
//!   streaming (embed_theme_css=true)     comment_drawn=true   rows=["string name the display name"]
//!   Element path (embed_theme_css=false) comment_drawn=false  rows=["string name"]
//! ```
//!
//! ⚠️ LAYOUT WAS ALREADY ON THE STREAMING PATH'S SIDE. `er_attribute_row_width` measures the
//! concatenation INCLUDING the comment, so the `Element` path reserved width for a string it then
//! refused to draw — the box was the right size for text that was not there.
//!
//! ⚠️ NOTE WHAT THIS IS NOT. mermaid does not draw the row as one string at all: it lays each
//! attribute out as COLUMN CELLS. Measured in Chromium against the pinned 11.15.0 bundle, `int id PK`
//! renders as three separate runs at fixed column offsets:
//!
//! ```text
//!   int  x=29    id    x=93    PK  x=158    y=68
//!   string x=29  name  x=93                 y=111
//!   string x=29  email x=93    UK  x=158    y=154
//! ```
//!
//! Matching that means splitting the row into measured cells across both SVG writers, the canvas and
//! terminal renderers, and `er_attribute_row_width` — a cross-cutting change filed separately with
//! those numbers rather than half-done here. This file pins only that the two SVG paths agree, which
//! is a precondition for that work and a defect on its own.

fn attribute_rows(embed_theme_css: bool) -> Vec<String> {
    let ir = fm_parser::parse("erDiagram\n    USER {\n        string name \"the display name\"\n        int id PK\n    }\n").ir;
    let config = fm_render_svg::SvgRenderConfig {
        embed_theme_css,
        ..Default::default()
    };
    let svg = fm_render_svg::render_svg_with_config(&ir, &config);
    let needle = "class=\"fm-er-attribute\">";
    let mut out = Vec::new();
    let mut rest = svg.as_str();
    while let Some(at) = rest.find(needle) {
        rest = &rest[at + needle.len()..];
        if let Some(end) = rest.find('<') {
            let text = rest[..end]
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&")
                .replace("&quot;", "\"");
            if !text.trim().is_empty() {
                out.push(text.trim().to_string());
            }
        }
    }
    out
}

#[test]
fn the_comment_is_drawn_on_both_svg_paths() {
    for embed_theme_css in [true, false] {
        let rows = attribute_rows(embed_theme_css);
        assert!(
            rows.iter().any(|row| row.contains("the display name")),
            "embed_theme_css={embed_theme_css}: the attribute comment was dropped; drew {rows:?}"
        );
    }
}

/// ⚠️ THE STRONGER FORM, and the one that keeps the two writers honest as they change: the paths
/// must produce the SAME rows, not merely both contain the comment. A future edit that adds a field
/// to one writer alone fails here even if every `contains` assertion still passes.
#[test]
fn both_svg_paths_draw_identical_attribute_rows() {
    assert_eq!(
        attribute_rows(true),
        attribute_rows(false),
        "the streaming and Element ER writers disagree about what an attribute row says"
    );
}

/// NON-VACUITY: the rows are actually being read. A helper that silently returned an empty vector
/// would satisfy the equality above and prove nothing.
#[test]
fn the_reader_actually_finds_the_rows() {
    let rows = attribute_rows(true);
    assert_eq!(
        rows.len(),
        2,
        "two attributes were declared, so two rows must be read: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("id")),
        "the uncommented attribute is missing: {rows:?}"
    );
}
