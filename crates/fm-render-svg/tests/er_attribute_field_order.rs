//! An ER attribute's fields are drawn in mermaid's order: type, name, key, comment.
//!
//! THE DIVERGENCE. We led with the key — `PK int id` — putting the modifier where mermaid puts the
//! type. Measured in Chromium 151 against the pinned 11.15.0 bundle, an attribute renders as column
//! cells at fixed offsets, and the key comes LAST:
//!
//! ```text
//!   int    x=29     id     x=93    PK   x=158     y=68
//!   string x=29     name   x=93                   y=111
//!   string x=29     email  x=93    UK   x=158     y=154
//! ```
//!
//! THE FUSION IS FIXED TOO. Both SVG writers now emit ONE TEXT ELEMENT PER FIELD at shared column
//! offsets (`er_cell_columns`), so `int id PK` is three runs as mermaid draws it. er_basic's
//! attribute cells now match the incumbent exactly — its only remaining surplus is the cardinality
//! text, which is a separate question (mermaid encodes cardinality as crow's-foot MARKERS and we
//! draw none, so our text is the only carrier we have).
//!
//! ⚠️ THE REAL CHANGE IS THAT THERE IS NOW ONE COMPOSITION. `IrEntityAttribute::display_row` replaces
//! five hand-rolled copies of the same five lines — both fm-render-svg writers, fm-render-canvas,
//! fm-render-term, and fm-layout's `er_attribute_row_width`. They had ALREADY drifted: the streaming
//! SVG writer appended the attribute comment and the `Element` writer did not, so whether an author's
//! comment appeared depended on `embed_theme_css`. Five copies of a rule is five chances to disagree;
//! the layout copy is the one that decides how wide the entity box is, so a disagreement there spills
//! a row outside its own entity.

fn attribute_rows(embed_theme_css: bool, source: &str) -> Vec<String> {
    let ir = fm_parser::parse(source).ir;
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

const ENTITY: &str = "erDiagram\n    USER {\n        int id PK\n        string name\n        string email UK\n    }\n";

#[test]
fn each_field_is_its_own_cell_in_mermaids_order() {
    let rows = attribute_rows(true, ENTITY);
    // Three attributes: (int,id,PK), (string,name), (string,email,UK) => 8 cells.
    assert_eq!(
        rows,
        vec!["int", "id", "PK", "string", "name", "string", "email", "UK"],
        "each field must be its own run, in type/name/key order"
    );
}

/// ⚠️ THE NEGATIVE CONTROL, and the order exactly as it shipped. `PK int id` contains every field, so
/// any check phrased as "does the key appear?" or "does the type appear?" passes on it. Only the
/// absence of the key-first spelling distinguishes the two orders.
#[test]
fn the_key_first_spelling_is_never_drawn() {
    let rows = attribute_rows(true, ENTITY);
    assert!(
        !rows.iter().any(|row| row.contains(' ')),
        "a row is still fused into one run instead of separate cells: {rows:?}"
    );
    // The key must FOLLOW its name, never precede its type.
    let key_at = rows.iter().position(|row| row == "PK").expect("PK cell");
    let name_at = rows.iter().position(|row| row == "id").expect("id cell");
    assert!(
        key_at > name_at,
        "the key cell precedes the name it belongs to: {rows:?}"
    );
}

/// ⚠️ THE LOCKSTEP ASSERTION. Every consumer must render what `display_row` returns — that is the
/// whole point of collapsing five copies into one. Checked through both SVG paths, which is where the
/// drift actually happened.
#[test]
fn both_svg_paths_draw_identical_cells() {
    let streaming = attribute_rows(true, ENTITY);
    assert!(!streaming.is_empty(), "the fixture drew no attribute cells");
    assert_eq!(
        streaming,
        attribute_rows(false, ENTITY),
        "the streaming and Element ER writers disagree about the drawn cells"
    );
}

/// CONTROL for a COMPOSITE key, which is why `key_prefix` exists at all: `PK, FK` is one token
/// `PK,FK`, and it still belongs at the end of the row.
#[test]
fn a_composite_key_stays_one_token_and_stays_last() {
    let source = "erDiagram\n    J {\n        string a PK, FK\n    }\n";
    let rows = attribute_rows(true, source);
    assert_eq!(
        rows,
        vec!["string", "a", "PK,FK"],
        "a composite key must remain ONE comma-joined cell in the final position"
    );
}

/// CONTROL: an attribute with no key draws no stray separator where the key would have been.
#[test]
fn an_attribute_without_a_key_has_no_trailing_gap() {
    let rows = attribute_rows(true, ENTITY);
    // `string name` contributes exactly two cells and no empty key cell.
    assert!(
        rows.iter().all(|row| !row.is_empty()),
        "an empty cell was emitted where a field is absent: {rows:?}"
    );
    assert_eq!(
        rows.iter().filter(|row| row.as_str() == "string").count(),
        2,
        "both string-typed attributes must contribute a type cell: {rows:?}"
    );
}
