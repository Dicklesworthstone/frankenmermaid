//! `A <-- B` is an association with an arrowhead, not a plain link (bd-lfucm).
//!
//! THE DEFECT. `<--` was absent from `CLASS_OPERATORS`, so the operator scan fell through to the
//! bare `--` and produced `ArrowType::Line`: an association with NO ARROWHEAD, drawn identically to
//! the `A -- B` link beside it. Two declared relationship forms, one picture — the defect bd-vfxu
//! named for node shapes, in a class diagram's edges.
//!
//! It is the mirror of `-->`, so it maps to the same `Arrow` and is SWAPPED at the lowering site,
//! exactly as `<..` is against `..>`. Mapping it without the swap would point the head at `B` and
//! say the opposite of what the author wrote, which is why direction is asserted here and not just
//! the presence of a head.
//!
//! ⚠️ HOW THIS WAS FOUND, AND WHAT THE SWEEP GOT WRONG. The dimension was "do two different arrow
//! syntaxes render DIFFERENTLY" — the shape worklist's rule applied to edges, comparing the
//! PARTITION of forms each engine draws alike rather than diffing markup across engines. Its first
//! run reported FOUR defects. Two of them were the instrument:
//!
//! ```text
//!   flowchart --o vs o--o   the signature merged marker-start and marker-end into one
//!                           de-duplicated set, so "which end carries the marker" was invisible
//!   class <|-- vs <|..      the realization dash rides on the edge's inline_style and is emitted
//!                           as a `style="stroke-dasharray:5"` attribute; the signature only read a
//!                           `stroke-dasharray=` ATTRIBUTE, so it never saw it
//! ```
//!
//! With both corrected, flowchart partitions 11/11, ER 6/6 and state 2/2 all agree with the
//! reference, and ONE defect survived. Reported as one, not four.
//!
//! ⚠️ ONE NEGATIVE-CONTROL ARM IS ALSO INERT, AND IS REPORTED. Moving `<--` ahead of `<|--` in the
//! operator table changes nothing, because the two are not prefixes of one another — see
//! `the_neighbouring_operators_are_unshadowed`.

use fm_core::{ArrowType, IrEndpoint};

/// (arrow type, source id, target id) for the diagram's single edge.
fn edge(source: &str) -> (ArrowType, String, String) {
    let ir = fm_parser::parse(source).ir;
    let e = ir.edges.first().unwrap_or_else(|| {
        panic!("no edge in {source:?} — nodes: {:?}", ir.nodes.len());
    });
    let name = |end: &IrEndpoint| match end {
        IrEndpoint::Node(id) => ir.nodes[id.0].id.clone(),
        other => format!("{other:?}"),
    };
    (e.arrow, name(&e.from), name(&e.to))
}

/// ⚠️ THE NEGATIVE CASE: `<--` must not render as the plain link it collapsed into.
///
/// Asserting `Arrow` alone would pass if `--` were ALSO changed to `Arrow`; comparing the two forms
/// is what makes this a statement about the collapse. Both halves are asserted.
#[test]
fn an_association_does_not_render_as_a_plain_link() {
    let (assoc, _, _) = edge("classDiagram\n  A <-- B\n");
    let (plain, _, _) = edge("classDiagram\n  A -- B\n");
    assert_ne!(
        assoc, plain,
        "`A <-- B` still draws the same edge as `A -- B`"
    );
    assert_eq!(assoc, ArrowType::Arrow, "the association has no arrowhead");
    assert_eq!(plain, ArrowType::Line, "the plain link gained a head");
}

/// ⚠️ AND IT POINTS AT `A`, which the arrow-type assertion cannot see.
///
/// `<--` is the mirror of `-->`. Giving it the right head and the wrong direction trades a missing
/// arrowhead for one pointing at the wrong class, which is worse: the diagram then states the
/// opposite relationship.
#[test]
fn the_association_points_the_way_the_author_wrote_it() {
    let (arrow, from, to) = edge("classDiagram\n  A <-- B\n");
    assert_eq!(arrow, ArrowType::Arrow);
    assert_eq!(
        (from.as_str(), to.as_str()),
        ("B", "A"),
        "`A <-- B` points the wrong way"
    );

    let (fwd_arrow, fwd_from, fwd_to) = edge("classDiagram\n  A --> B\n");
    assert_eq!(fwd_arrow, ArrowType::Arrow);
    assert_eq!(
        (fwd_from.as_str(), fwd_to.as_str()),
        ("A", "B"),
        "`A --> B` points the wrong way"
    );
}

/// It behaves exactly as `<..` does against `..>` — same head, swapped ends.
///
/// The two pairs are the same rule written twice in the operator table, so comparing them is what
/// says the new entry follows the existing convention rather than inventing one.
#[test]
fn the_left_pointing_forms_are_consistent_with_each_other() {
    let (_, solid_from, solid_to) = edge("classDiagram\n  A <-- B\n");
    let (_, dotted_from, dotted_to) = edge("classDiagram\n  A <.. B\n");
    assert_eq!(
        (solid_from.as_str(), solid_to.as_str()),
        (dotted_from.as_str(), dotted_to.as_str()),
        "`<--` and `<..` disagree about which end the head belongs to"
    );
}

/// The neighbouring operators are unaffected.
///
/// ⚠️ AND THE ORDERING HAZARD DOES NOT APPLY TO THIS ENTRY, WHICH THE CONTROLS PROVED RATHER THAN
/// ASSUMED. A negative-control arm moved `<--` to the top of the table, ahead of `<|--`, and every
/// test still passed — INERT. The reason is structural: `find_operator` picks the first POSITION
/// that can start an operator and then the first entry matching there, and neither operator is a
/// prefix of the other (`<|-- B` does not start with `<--`, and `<-- B` does not start with `<|--`).
/// The bd-92b6 trap was a genuine prefix pair, `--o` against `--`; this is not one, so no table
/// order can shadow it.
///
/// The test is kept as cheap insurance over the forms nearest the new entry, but the file does NOT
/// claim it proves an ordering constraint — because there is none to prove here.
#[test]
fn the_neighbouring_operators_are_unshadowed() {
    for (source, expected, from, to) in [
        (
            "classDiagram\n  A <|-- B\n",
            ArrowType::Inheritance,
            "A",
            "B",
        ),
        (
            "classDiagram\n  A <|.. B\n",
            ArrowType::Inheritance,
            "A",
            "B",
        ),
        (
            "classDiagram\n  A <.. B\n",
            ArrowType::DottedArrow,
            "B",
            "A",
        ),
        (
            "classDiagram\n  A ..> B\n",
            ArrowType::DottedArrow,
            "A",
            "B",
        ),
        (
            "classDiagram\n  A o-- B\n",
            ArrowType::Aggregation,
            "A",
            "B",
        ),
        (
            "classDiagram\n  A *-- B\n",
            ArrowType::Composition,
            "A",
            "B",
        ),
        ("classDiagram\n  A .. B\n", ArrowType::DottedLine, "A", "B"),
    ] {
        let (arrow, actual_from, actual_to) = edge(source);
        assert_eq!(arrow, expected, "{source:?} changed arrow type");
        assert_eq!(
            (actual_from.as_str(), actual_to.as_str()),
            (from, to),
            "{source:?} changed direction"
        );
    }
}

/// ⚠️ NO PHANTOM ENDPOINT, which is what the bd-92b6 shadowing defect actually produced.
///
/// When a shorter operator matches first it leaves a marker byte attached to the endpoint, and the
/// id normalizes into a node that does not exist — `C3 o-- C4` interned `C3-o`. The class count is
/// what catches that; the arrow type does not.
#[test]
fn the_association_interns_exactly_its_two_classes() {
    for source in [
        "classDiagram\n  A <-- B\n",
        "classDiagram\n  A -- B\n",
        "classDiagram\n  A <.. B\n",
    ] {
        let ir = fm_parser::parse(source).ir;
        // ⚠️ SORTED, BECAUSE INTERN ORDER FOLLOWS THE EDGE, NOT THE SOURCE TEXT. This first asserted
        // `["A", "B"]` and `A <-- B` gave `["B", "A"]` — correctly: after the swap the edge runs
        // B to A, so B is interned first. The property here is "exactly these two and nothing
        // else", and demanding an order made the test fail on behaviour that is right.
        let mut ids: Vec<&str> = ir.nodes.iter().map(|n| n.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            vec!["A", "B"],
            "{source:?} interned a phantom endpoint"
        );
    }
}

/// A label still attaches, and to the edge as it finally points.
///
/// The label is applied AFTER the endpoint swap, so a change to the swap can silently move it to
/// the wrong end.
#[test]
fn a_labelled_association_keeps_its_label() {
    let parsed = fm_parser::parse("classDiagram\n  A <-- B : uses\n");
    let ir = &parsed.ir;
    let e = ir.edges.first().expect("one edge");
    let label = e
        .label
        .and_then(|id| ir.labels.get(id.0))
        .map(|l| l.text.as_str());
    assert_eq!(label, Some("uses"), "the association lost its label");
    assert_eq!(e.arrow, ArrowType::Arrow);
}
