//! ER attribute cells on the terminal align into character columns when they fit (bd-jbrzc).
//!
//! THE DECISION THIS BEAD ASKED FOR. Its first instruction was to decide whether column cells are
//! even the right model for a character grid, and it offered "no, close as working as intended" as
//! a legitimate answer. Measured before choosing:
//!
//!   * The entity box is already sized from COLUMN geometry (`fm_core::er_cell_columns`), so a
//!     fused row is drawn into a box reserving width it does not use. On the bead's skew fixture
//!     the interior is ~54 cells and aligned columns need 41 — the room is there and was going
//!     unused.
//!   * A fused row can never be WIDER than the columns (each field is at most its column's max, and
//!     the gutter is at least the separator), so neither model overflows. The defect is alignment,
//!     not spill — which is why the answer is "align", not "leave it".
//!
//! ⚠️ PIXEL OFFSETS ARE NOT REUSED HERE. `fm_core::er_cell_columns` measures proportional font
//! metrics; on a grid every glyph is one cell. Sharing that helper would share a name rather than a
//! rule, so the terminal counts characters — and because the two measures have no guaranteed
//! relationship, the drawing falls back to the fused row when the columns do not fit.

use fm_render_term::render_term;

/// The fixture the bead names: widest type and widest name on DIFFERENT attributes.
const SKEW: &str = "erDiagram\n    T {\n        verylongtypename a\n        \
                    t verylongattributename PK\n    }\n";

fn render(source: &str) -> String {
    render_term(&fm_parser::parse(source).ir)
}

/// Rows of the render that carry text, as char vectors.
fn text_rows(rendered: &str) -> Vec<Vec<char>> {
    rendered
        .lines()
        .filter(|row| row.chars().any(char::is_alphanumeric))
        .map(|row| row.chars().collect())
        .collect()
}

/// The column each run of non-blank characters starts at, past the left border.
fn field_starts(row: &[char]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut prev_blank = true;
    for (index, ch) in row.iter().enumerate() {
        let blank = *ch == '\u{2800}' || *ch == ' ';
        if prev_blank && !blank && index > 1 {
            starts.push(index);
        }
        prev_blank = blank;
    }
    starts
}

/// THE ALIGNMENT: corresponding fields share a column across rows.
///
/// This is what the fused row could not do, and what the box was already paying for. Asserted as an
/// equality between two rows rather than against a literal column, so it survives the box moving.
///
/// `field_starts` reports runs preceded by a blank cell, so the TYPE cell — which butts against the
/// left border — is not among them and the first reported start is the NAME column. That is the
/// column the two rows must agree on, and the one a fused row cannot make them agree on: `a` would
/// sit right after `verylongtypename` while `verylongattributename` sat right after `t`.
#[test]
fn corresponding_fields_share_a_character_column() {
    let rendered = render(SKEW);
    let rows = text_rows(&rendered);
    assert!(rows.len() >= 3, "expected three text rows:\n{rendered}");

    let first = field_starts(&rows[1]);
    let second = field_starts(&rows[2]);
    assert!(
        !first.is_empty() && !second.is_empty(),
        "no separated fields on one of the rows, so the cells are still fused:\n{rendered}"
    );
    assert_eq!(
        first[0], second[0],
        "the two name cells start at columns {} and {}, so they are not a column:\n{rendered}",
        first[0], second[0]
    );
    // And the name column is genuinely indented past the type column, which starts at 2.
    assert!(
        first[0] > 3,
        "the name column starts at {}, so every cell is stacked at the left edge:\n{rendered}",
        first[0]
    );
}

/// Every field is still drawn — the split must not lose content.
#[test]
fn every_field_is_still_drawn() {
    let rendered = render(SKEW);
    for field in ["verylongtypename", "a", "t", "verylongattributename", "PK"] {
        assert!(
            rendered.contains(field),
            "the terminal lost {field:?}:\n{rendered}"
        );
    }
}

/// CONTAINMENT: no cell is drawn past the entity's right border.
///
/// The terminal analogue of the control the bead requires. The border is located from the render
/// itself, so this cannot pass by drawing the border around wherever the text happened to land.
#[test]
fn no_cell_is_drawn_past_the_right_border() {
    let rendered = render(SKEW);
    let rows = text_rows(&rendered);
    for row in &rows {
        // The right border is the last box-drawing glyph on the row.
        // Any braille glyph that is not the blank pattern is box geometry; the last one on the
        // row is the right border.
        let Some(border) = row
            .iter()
            .rposition(|ch| ('\u{2801}'..='\u{28ff}').contains(ch))
        else {
            continue;
        };
        let last_text = row.iter().rposition(|ch| ch.is_alphanumeric()).unwrap_or(0);
        assert!(
            last_text < border,
            "text runs to column {last_text}, past the right border at {border}:\n{rendered}"
        );
    }
}

/// The ordinary single-attribute case still draws every field.
#[test]
fn the_common_case_is_unchanged_in_content() {
    let rendered = render("erDiagram\n    U {\n        string name PK\n    }\n");
    for field in ["string", "name", "PK"] {
        assert!(
            rendered.contains(field),
            "the ordinary case lost {field:?}:\n{rendered}"
        );
    }
}

/// A diagram type that has no ER attributes is untouched.
#[test]
fn other_diagram_types_are_unaffected() {
    let flowchart = "flowchart LR\n  A[Start] --> B[End]\n";
    let before = render(flowchart);
    assert!(before.contains("Start") && before.contains("End"));
    assert_eq!(before, render(flowchart), "terminal render is not stable");
}
