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
//! ⚠️ THIS FIXES THE ORDER, NOT THE FUSION, and the distinction is deliberate. mermaid draws three
//! separate text elements; we still draw one run (`int id PK`). Splitting the row into real cells
//! needs per-column measurement agreed across all five consumers at once and is the open half of
//! bd-xxvch. Fixing the order is complete and testable on its own, and the cells will need this
//! order anyway.
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
fn the_key_is_drawn_after_the_name_not_before_the_type() {
    let rows = attribute_rows(true, ENTITY);
    assert!(
        rows.iter().any(|row| row == "int id PK"),
        "the fields must read type, name, key; drew {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row == "string email UK"),
        "the fields must read type, name, key; drew {rows:?}"
    );
}

/// ⚠️ THE NEGATIVE CONTROL, and the order exactly as it shipped. `PK int id` contains every field, so
/// any check phrased as "does the key appear?" or "does the type appear?" passes on it. Only the
/// absence of the key-first spelling distinguishes the two orders.
#[test]
fn the_key_first_spelling_is_never_drawn() {
    let rows = attribute_rows(true, ENTITY);
    assert!(
        !rows
            .iter()
            .any(|row| row.starts_with("PK ") || row.starts_with("UK ")),
        "an attribute still leads with its key modifier: {rows:?}"
    );
}

/// ⚠️ THE LOCKSTEP ASSERTION. Every consumer must render what `display_row` returns — that is the
/// whole point of collapsing five copies into one. Checked through both SVG paths, which is where the
/// drift actually happened.
#[test]
fn both_svg_paths_render_exactly_display_row() {
    let ir = fm_parser::parse(ENTITY).ir;
    let expected: Vec<String> = ir
        .nodes
        .iter()
        .flat_map(|node| node.members.iter())
        .map(fm_core::IrEntityAttribute::display_row)
        .collect();
    assert!(!expected.is_empty(), "the fixture declared no attributes");
    for embed_theme_css in [true, false] {
        assert_eq!(
            attribute_rows(embed_theme_css, ENTITY),
            expected,
            "embed_theme_css={embed_theme_css}: the drawn rows differ from display_row"
        );
    }
}

/// CONTROL for a COMPOSITE key, which is why `key_prefix` exists at all: `PK, FK` is one token
/// `PK,FK`, and it still belongs at the end of the row.
#[test]
fn a_composite_key_stays_one_token_and_stays_last() {
    let source = "erDiagram\n    J {\n        string a PK, FK\n    }\n";
    let rows = attribute_rows(true, source);
    assert_eq!(
        rows,
        vec!["string a PK,FK".to_string()],
        "a composite key must remain one comma-joined token in the final position"
    );
}

/// CONTROL: an attribute with no key draws no stray separator where the key would have been.
#[test]
fn an_attribute_without_a_key_has_no_trailing_gap() {
    let rows = attribute_rows(true, ENTITY);
    assert!(
        rows.iter().any(|row| row == "string name"),
        "an unkeyed attribute must draw exactly its type and name: {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.ends_with(' ')),
        "a row ends with a dangling separator: {rows:?}"
    );
}
