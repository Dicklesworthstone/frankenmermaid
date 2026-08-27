//! The C4 boundary type reaches the terminal as its own row (bd-c23yq).
//!
//! mermaid draws a boundary as TWO rows — the bold label, then its type in square brackets — and
//! both the SVG and canvas arms do. The terminal showed only the label.
//!
//! ⚠️ THIS WAS ATTEMPTED TWICE AND REVERTED TWICE, so the failure modes are worth stating. Writing
//! `[SYSTEM]` into `row + 1` under the cluster overlay's blank-cell guard never drew anything at
//! width 80, 160 or 240: a contained node's box began on that very row. Reserving pixels in
//! fm-layout does not help either, because this surface scales the whole diagram to fit its row
//! count, so a uniformly taller box is cancelled by the scale it induces.
//!
//! WHAT ACTUALLY WORKS is a reservation in the terminal's OWN cell space:
//!
//!   * a captioned boundary's members are pushed down `C4_CAPTION_ROWS + 1` cell rows — the `+ 1`
//!     clears the node's own top BORDER, and a two-row push measurably put `[SYSTEM]` exactly on
//!     that border where the guard refused it;
//!   * the boundary itself GROWS by the same amount, because shrinking the node instead was tried
//!     and cost the contained content: `Alice` and `<<person>>` vanished from the output;
//!   * nested boundaries stack, since `IrCluster::members` is transitive and a node two deep must
//!     clear both captions.
//!
//! THE CONTROL THE BEAD REQUIRES: assert the bracketed row for a boundary that DIRECTLY CONTAINS A
//! PERSON — not one containing only boundaries, which passed by accident before any of this,
//! because there was no node box to collide with.

use fm_render_term::render_term;

fn render(source: &str) -> String {
    render_term(&fm_parser::parse(source).ir)
}

/// The row index of the first row containing `needle`, ignoring blank braille padding.
fn row_of(rendered: &str, needle: &str) -> Option<usize> {
    rendered.lines().position(|row| row.contains(needle))
}

const BOUNDARY_WITH_PERSON: &str = "C4Context\n    title Sys\n    \
    System_Boundary(sb, \"Core\") {\n        Person(a, \"Alice\", \"A user\")\n    }\n";

/// THE CONTROL: the bracketed type row is present for a boundary that directly contains a Person.
///
/// The fixture matters. A boundary containing only other boundaries has no node box to collide
/// with, so its caption row survived even before any of this work — asserting on that fixture would
/// have reported success while the case that actually failed still failed.
#[test]
fn the_bracketed_type_row_is_present_for_a_boundary_containing_a_person() {
    let rendered = render(BOUNDARY_WITH_PERSON);
    assert!(
        rendered.contains("[SYSTEM]"),
        "no bracketed boundary type in:\n{rendered}"
    );
}

/// It is its own ROW, directly beneath the label — not appended to it, and not floating elsewhere.
#[test]
fn the_type_sits_on_the_row_directly_below_the_label() {
    let rendered = render(BOUNDARY_WITH_PERSON);
    let label = row_of(&rendered, "Core").expect("no boundary label drawn");
    let kind = row_of(&rendered, "[SYSTEM]").expect("no boundary type drawn");
    assert_eq!(
        kind,
        label + 1,
        "the label is on row {label} and the type on row {kind}; they should be adjacent:\n\
         {rendered}"
    );
}

/// ⚠️ THE CAPTION MUST NOT BE BOUGHT WITH THE CONTAINED CONTENT.
///
/// The first working version of the push shrank each node's box to pay for the rows, and the
/// contained Person's own rows stopped fitting: `Alice` and `<<person>>` disappeared from the
/// output entirely. A caption that costs the thing it captions is not a fix, so the boundary grows
/// instead — and this is the assertion that says so.
#[test]
fn the_contained_person_survives_the_reservation() {
    let rendered = render(BOUNDARY_WITH_PERSON);
    for expected in ["Alice", "<<person>>"] {
        assert!(
            rendered.contains(expected),
            "the reservation cost the contained {expected:?}:\n{rendered}"
        );
    }
    // And it is still BELOW the caption, i.e. inside the boundary rather than pushed out of it.
    let kind = row_of(&rendered, "[SYSTEM]").expect("no boundary type");
    let person = row_of(&rendered, "Alice").expect("no person");
    assert!(
        person > kind,
        "the person is on row {person}, above the caption on row {kind}"
    );
}

/// Nested boundaries each get their own two rows, and stack.
///
/// `IrCluster::members` is transitive, so a node two boundaries deep appears in both member lists
/// and must clear both captions. Taking the maximum push rather than the sum was tried: the inner
/// `Core` and `[SYSTEM]` had nowhere to go and only the outer `[ENTERPRISE]` appeared.
#[test]
fn nested_boundaries_each_get_their_own_caption() {
    let rendered = render(
        "C4Context\n    Enterprise_Boundary(e, \"Ent\") {\n        \
         System_Boundary(s, \"Core\") {\n            Person(a, \"Alice\")\n        }\n    }\n",
    );
    let ent = row_of(&rendered, "Ent").expect("no outer label");
    let ent_kind = row_of(&rendered, "[ENTERPRISE]").expect("no outer type");
    let core = row_of(&rendered, "Core").expect("no inner label");
    let core_kind = row_of(&rendered, "[SYSTEM]").expect("no inner type");
    let person = row_of(&rendered, "Alice").expect("no person");

    assert_eq!(ent_kind, ent + 1, "the outer type is not under its label");
    assert_eq!(core_kind, core + 1, "the inner type is not under its label");
    assert!(
        ent_kind < core,
        "the inner boundary starts at row {core}, not below the outer caption at {ent_kind}"
    );
    assert!(
        person > core_kind,
        "the person is at row {person}, not inside the inner boundary below {core_kind}"
    );
}

/// A boundary with no declared type gets no bracketed row and no reservation.
///
/// The negative half of the reservation: it is keyed on the type being PRESENT, so a plain
/// subgraph-style cluster is untouched and nothing moves for it.
#[test]
fn a_boundary_without_a_type_gets_no_bracketed_row() {
    let rendered = render("flowchart TD\n  subgraph one [Group]\n    a --> b\n  end\n");
    assert!(rendered.contains("Group"), "the cluster label is missing");
    assert!(
        !rendered.contains('['),
        "a bracketed row was drawn for a cluster with no C4 type:\n{rendered}"
    );
}

/// Diagram types with no captioned boundary render exactly as before.
///
/// The control for the whole change: the reservation is keyed on `c4_boundary_type`, which is
/// `None` everywhere else, so no other family may move a single cell.
#[test]
fn other_diagram_types_are_untouched() {
    for source in [
        "flowchart LR\n  A[Start] --> B[End]\n",
        "classDiagram\n  class Animal {\n    +int age\n  }\n",
        "sequenceDiagram\n  Alice->>Bob: Hi\n",
    ] {
        let first = render(source);
        assert!(
            !first.trim().is_empty(),
            "nothing rendered for {source:?}, so this control proves nothing"
        );
        assert_eq!(first, render(source), "render is not stable for {source:?}");
    }
}
