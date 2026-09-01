//! A class relation is a PRODUCT of two ends and a line type, and the table only knew 18 of its 72
//! spellings (bd-92b6 family).
//!
//! WHAT THE INCUMBENT ACTUALLY DOES, measured rather than assumed. mermaid's class grammar is
//!
//! ```text
//!   relation : relationType lineType relationType | relationType lineType
//!            | lineType relationType             | lineType
//!   relationType ::= 0 AGGREGATION | 1 EXTENSION | 2 COMPOSITION | 3 DEPENDENCY | 4 LOLLIPOP
//!   lineType     ::= 0 LINE (solid) | 1 DOTTED_LINE
//! ```
//!
//! `scripts/headtohead/class_relation_battery.mjs` asks the pinned 11.15.0 bundle's own db for
//! `Alpha <op> Beta` at every point of that product — 6 starts x 2 lines x 6 ends — and writes the
//! answers to `fixtures/mermaid_class_relations.tsv`. The incumbent ACCEPTS ALL 72. This file is
//! driven by that fixture rather than by a list written here, so the contract cannot quietly shrink.
//!
//! ⚠️ THE DEFECT WAS A NODE-SET DEFECT, NOT A MISSING MARKER. `CLASS_OPERATORS` lists spellings, so
//! a spelling it lacks does not fail — it matches a SHORTER entry and leaves the rest of the
//! operator glued to an endpoint. Measured on the table alone, four spellings invented a class:
//!
//! ```text
//!   Alpha o--o Beta   ->  Alpha -> o_Beta       matched `o--`, kept `o Beta`
//!   Alpha o..  Beta   ->  Alpha_o -> Beta       reached the trailing `..`, kept `Alpha o`
//!   Alpha ..o  Beta   ->  Alpha -> o_Beta
//!   Alpha o..o Beta   ->  Alpha_o -> o_Beta     both ends at once
//! ```
//!
//! and five more drew an unmarked line for a marked relation (`o..`, `*..`, `..o`, `..*`, `o..o`
//! all came back `DottedLine`). `*--*` and `<|--|>` escaped the node defect only because identifier
//! normalization happens to absorb a leading `*` or `|>` — the same accident that hid the lollipop.
//!
//! ⚠️ WHY THE CORPUS COULD NOT SEE ANY OF IT. ci_docs_2000 reads class=190/190 and ci_docs_5000
//! reads class=482/482 with every one of these live, because the generated corpus only ever writes
//! the four single-ended solid forms. This was found by asking the incumbent's db, not by diffing
//! the corpus — the same way bd-lkm9i was found, and the reason the fixture exists.

use std::collections::BTreeSet;

use fm_core::{ArrowType, IrEndpoint};

const FIXTURE: &str = include_str!("fixtures/mermaid_class_relations.tsv");

/// One incumbent-measured row: the spelling and what mermaid's db made of it.
struct Row {
    op: String,
    /// Marker at the SOURCE end, `None` when mermaid reported `none`.
    type1: Option<u8>,
    /// Marker at the TARGET end.
    type2: Option<u8>,
    dotted: bool,
}

fn rows() -> Vec<Row> {
    let mut rows = Vec::new();
    for line in FIXTURE.lines().skip(1).filter(|line| !line.is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 5, "malformed fixture row {line:?}");
        let marker = |field: &str| match field {
            "none" => None,
            other => Some(other.parse::<u8>().expect("a relationType is 0..=4")),
        };
        rows.push(Row {
            op: fields[0].to_string(),
            type1: marker(fields[2]),
            type2: marker(fields[3]),
            dotted: fields[4] == "1",
        });
    }
    rows
}

struct Edge {
    arrow: ArrowType,
    from: String,
    to: String,
    dashed: bool,
}

fn edge(op: &str) -> Edge {
    let source = format!("classDiagram\n  Alpha {op} Beta\n");
    let ir = fm_parser::parse(&source).ir;
    let e = ir
        .edges
        .first()
        .unwrap_or_else(|| panic!("`{op}` produced NO EDGE at all"));
    let name = |end: &IrEndpoint| match end {
        IrEndpoint::Node(id) => ir.nodes[id.0].id.clone(),
        other => format!("{other:?}"),
    };
    // A relation is drawn dashed either because its ArrowType is dotted BY NAME, or because the
    // lowering site attached the dash as an inline style — the mechanism bd-u9hcc chose for
    // realization so that a dotted variant of every relation would not double the ArrowType enum.
    // Both count; asking about only one of them is how the realization dash went unnoticed in the
    // partition sweep (see class_association_arrow.rs).
    let dashed = matches!(e.arrow, ArrowType::DottedArrow | ArrowType::DottedLine)
        || e.inline_style
            .as_ref()
            .is_some_and(|style| style.properties.contains_key("stroke-dasharray"));
    Edge {
        arrow: e.arrow,
        from: name(&e.from),
        to: name(&e.to),
        dashed,
    }
}

/// The 72 spellings really are all distinct and all present — the fixture is the contract, so a
/// truncated fixture must fail here rather than quietly weaken every test below it.
#[test]
fn the_fixture_covers_the_whole_relation_product() {
    let rows = rows();
    assert_eq!(rows.len(), 72, "6 starts x 2 line types x 6 ends");
    let unique: BTreeSet<&str> = rows.iter().map(|row| row.op.as_str()).collect();
    assert_eq!(unique.len(), 72, "the fixture repeats a spelling");
}

/// ⚠️ THE SEVERE HALF. A lost marker draws the wrong edge; a phantom node adds a class the author
/// never declared, that no other statement can refer to, and that changes the node set the layout
/// and every equivalence check reason about.
///
/// Swept over all 72 with the whole failure table reported, because these fail in two DIRECTIONS —
/// the marker lands on the target for `o--o` and on the SOURCE for `o..` — and a per-case test that
/// stopped at the first failure would have hidden the second shape.
#[test]
fn no_class_relation_spelling_mints_a_phantom_node() {
    // ⚠️ THE INVARIANT IS OUR OWN NODE SET, NOT THE FIXTURE'S `ids` COLUMN. For `()--` and `()..`
    // the incumbent reports `ids=[Beta]`, because it SYNTHESISES an `interface0` pseudo-node for
    // the lollipop's socket end and does not count it as a class; for `--()` and `..()` it reports
    // `ids=[Alpha]` for the same reason. We model the socket as a marker on a real edge between the
    // two declared classes (bd-lkm9i), so we keep both. That is a deliberate difference and not a
    // phantom: a phantom is a node whose id contains OPERATOR BYTES, which is what is asserted.
    //
    // ⚠️ AS A SET, NOT AS A PAIR. `<--` and `<..` legitimately come back `Beta -> Alpha`: they map
    // onto the forward `Arrow`/`DottedArrow` and the lowering site SWAPS the endpoints so the head
    // lands where the author pointed it. `the_left_pointing_dependencies_still_swap_their_endpoints`
    // is what holds that direction; this test is about the node SET, and an ordered comparison here
    // would have reported the deliberate swap as an invented class.
    let mut bad = Vec::new();
    for row in rows() {
        let e = edge(&row.op);
        let ends: BTreeSet<&str> = [e.from.as_str(), e.to.as_str()].into_iter().collect();
        if ends != BTreeSet::from(["Alpha", "Beta"]) {
            bad.push(format!(
                "  {:<8} {} -> {}   (arrow {:?})",
                row.op, e.from, e.to, e.arrow
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{} of 72 class relation spellings mint a phantom node:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// The LINE TYPE is the incumbent's own `lineType` column, and it is independent of the markers.
///
/// ⚠️ ASSERTED IN BOTH DIRECTIONS. Dashing everything would pass a one-sided test while turning
/// every solid relation into a dashed one, so the solid rows are checked too.
#[test]
fn the_line_type_matches_the_incumbent() {
    let mut bad = Vec::new();
    for row in rows() {
        let e = edge(&row.op);
        if e.dashed != row.dotted {
            bad.push(format!(
                "  {:<8} mermaid line={} but we draw {}  (arrow {:?})",
                row.op,
                u8::from(row.dotted),
                if e.dashed { "dashed" } else { "solid" },
                e.arrow
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{} spelling(s) disagree with the incumbent's line type:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// The marker we draw must be the one the author wrote at the end they wrote it.
///
/// ⚠️ THIS IS A RELATIONAL TEST, NOT A RESTATEMENT OF THE MAPPING. It never names an expected
/// `ArrowType`; it asserts that two spellings carrying the SAME incumbent marker on the SAME end
/// produce the same `ArrowType` here, taking the single-ended solid spelling as the reference. A
/// test that instead listed `("o..", Aggregation)` would pass by construction against the very
/// table it is meant to check.
///
/// ⚠️ SPELLINGS MARKED AT BOTH ENDS ARE INCLUDED NOW (bd-f9t0r). They used to be skipped, because
/// `ArrowType` names an edge rather than a pair of ends and `o--*` could not be expressed at all.
/// The second marker is now carried beside the primary as `IrEdgeExtras::co_arrow` — the
/// `ArrowType` that already draws that marker on that end — so a both-ends spelling has a
/// source-end marker to check like any other, and the exclusion is gone rather than reworded.
#[test]
fn a_single_ended_relation_keeps_its_marker_and_its_end() {
    let reference = |type1: Option<u8>, type2: Option<u8>| -> Option<String> {
        // The canonical SOLID spelling carrying this marker on this end.
        let start = match type1 {
            Some(0) => "o",
            Some(1) => "<|",
            Some(2) => "*",
            Some(3) => "<",
            Some(4) => "()",
            None => "",
            _ => return None,
        };
        let end = match type2 {
            Some(0) => "o",
            Some(1) => "|>",
            Some(2) => "*",
            Some(3) => ">",
            Some(4) => "()",
            None => "",
            _ => return None,
        };
        Some(format!("{start}--{end}"))
    };

    let mut bad = Vec::new();
    for row in rows() {
        // The SOURCE-end marker is the primary `ArrowType` whether or not the target end is marked
        // too, so a both-ends row is compared against the canonical spelling carrying the same
        // start marker and no end marker.
        let canonical_type2 = if row.type1.is_some() {
            None
        } else {
            row.type2
        };
        let Some(canonical) = reference(row.type1, canonical_type2) else {
            continue;
        };
        if canonical == row.op {
            continue;
        }
        let got = edge(&row.op);
        let want = edge(&canonical);
        if !same_marker(got.arrow, want.arrow) {
            bad.push(format!(
                "  {:<8} -> {:?}, but {:<8} -> {:?} and they carry the same marker on the same end",
                row.op, got.arrow, canonical, want.arrow
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{} spelling(s) lose the marker their solid twin keeps:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// Do these two arrow types carry the same MARKER on the same end, ignoring the line type?
///
/// ⚠️ TWO PAIRS, AND ONLY TWO. Almost every relation carries its line type as the inline
/// `stroke-dasharray` the lowering site attaches, so the dotted and solid spellings share one
/// `ArrowType` and compare equal. The dependency and the bare link are the exceptions: their line
/// type is in the VARIANT NAME (`Arrow`/`DottedArrow`, `Line`/`DottedLine`), which predates this
/// work and is why `..>` and `-->` are two variants rather than one plus a dash.
///
/// Naming the exceptions here rather than widening the comparison keeps this test about the marker.
/// The line type itself is not waived — `the_line_type_matches_the_incumbent` checks all 72 rows
/// against the incumbent's own `lineType` column, in both directions.
fn same_marker(a: ArrowType, b: ArrowType) -> bool {
    a == b
        || matches!(
            (a, b),
            (ArrowType::DottedArrow, ArrowType::Arrow)
                | (ArrowType::Arrow, ArrowType::DottedArrow)
                | (ArrowType::DottedLine, ArrowType::Line)
                | (ArrowType::Line, ArrowType::DottedLine)
        )
}

/// ⚠️ AND THE MARKED RELATIONS MUST STILL DIFFER FROM THE UNMARKED LINE. Splitting the endpoints
/// correctly while answering `Line` would clear every test above and still draw a plain link for an
/// aggregation — which is exactly the `<--` defect (bd-lfucm), moved one spelling to the right.
#[test]
fn no_marked_relation_collapses_onto_the_plain_link() {
    let plain = edge("--").arrow;
    let dotted_plain = edge("..").arrow;
    let mut collapsed = Vec::new();
    for row in rows() {
        if row.type1.is_none() && row.type2.is_none() {
            continue;
        }
        let arrow = edge(&row.op).arrow;
        if arrow == plain || arrow == dotted_plain {
            collapsed.push(format!("  {:<8} -> {arrow:?}", row.op));
        }
    }
    assert!(
        collapsed.is_empty(),
        "{} marked relation(s) draw as the UNMARKED link ({plain:?} / {dotted_plain:?}):\n{}",
        collapsed.len(),
        collapsed.join("\n")
    );
}

/// ⚠️ THE REGRESSION GUARD. The grammar matcher REPLACES the literal table for every class
/// position, so it can change an answer the table already got right. These are the spellings
/// `CLASS_OPERATORS` carried before this change, with the arrows it produced, asserted verbatim:
/// the point of the change is the 54 spellings the table could not name, not a new answer for the
/// 18 it could.
#[test]
fn the_spellings_the_table_already_knew_are_unchanged() {
    const LEGACY: [(&str, ArrowType); 18] = [
        ("<|--", ArrowType::Inheritance),
        ("--|>", ArrowType::InheritanceReverse),
        ("..|>", ArrowType::InheritanceReverse),
        ("<|..", ArrowType::Inheritance),
        ("..>", ArrowType::DottedArrow),
        ("<..", ArrowType::DottedArrow),
        ("<--", ArrowType::Arrow),
        ("o--", ArrowType::Aggregation),
        ("*--", ArrowType::Composition),
        ("--o", ArrowType::AggregationReverse),
        ("--*", ArrowType::CompositionReverse),
        ("-->", ArrowType::Arrow),
        ("()--", ArrowType::Lollipop),
        ("--()", ArrowType::LollipopReverse),
        ("()..", ArrowType::Lollipop),
        ("..()", ArrowType::LollipopReverse),
        ("--", ArrowType::Line),
        ("..", ArrowType::DottedLine),
    ];
    for (op, want) in LEGACY {
        assert_eq!(edge(op).arrow, want, "`{op}` changed arrow type");
    }
}

/// ⚠️ AND THE TWO SPELLINGS THAT POINT LEFT STILL POINT LEFT. `<--` and `<..` map onto the FORWARD
/// `Arrow`/`DottedArrow` and rely on the lowering site swapping the endpoints; that swap used to be
/// keyed off a literal list of two spellings and is now derived from the token's parts, so it is
/// exactly the kind of thing this change could break without any arrow type moving.
#[test]
fn the_left_pointing_dependencies_still_swap_their_endpoints() {
    for op in ["<--", "<.."] {
        let e = edge(op);
        assert_eq!(
            (e.from.as_str(), e.to.as_str()),
            ("Beta", "Alpha"),
            "`Alpha {op} Beta` must draw its head at Alpha"
        );
    }
    // The negative control: the mirror spellings must NOT swap.
    for op in ["-->", "..>"] {
        let e = edge(op);
        assert_eq!(
            (e.from.as_str(), e.to.as_str()),
            ("Alpha", "Beta"),
            "`Alpha {op} Beta` must not swap"
        );
    }
}

/// ⚠️ AN `o` INSIDE AN IDENTIFIER IS NOT A MARKER (bd-zdpwd's rule). The matcher scans every
/// position, so without the token-boundary guard the second `o` of `Foo` starts a perfect `o--`
/// and splits the source into `Fo`. This is the guard's only test.
#[test]
fn a_trailing_o_in_a_class_name_is_not_an_aggregation_marker() {
    let ir = fm_parser::parse("classDiagram\n  Foo-- Bar\n").ir;
    let ids: Vec<&str> = ir.nodes.iter().map(|node| node.id.as_str()).collect();
    assert!(
        ids.contains(&"Foo") && ids.contains(&"Bar"),
        "`Foo-- Bar` split inside the identifier: {ids:?}"
    );
}

/// ⚠️ AND THE FAR MARKER MUST ACTUALLY ARRIVE (bd-f9t0r). Every test above reads the PRIMARY
/// `ArrowType`, which carries the source-end marker — so all of them passed while a both-ends
/// relation silently drew one marker and dropped the other. That is the defect this file's earlier
/// exclusion was recording, and only `co_arrow` can see it.
///
/// Driven by the fixture's own `type2` column, so the expectation is the incumbent's answer rather
/// than a list written here: every spelling mermaid reports with BOTH ends marked must carry a
/// co-arrow, and every spelling it reports with at most one must not.
#[test]
fn a_relation_marked_at_both_ends_carries_its_far_marker() {
    let mut missing = Vec::new();
    let mut spurious = Vec::new();
    for row in rows() {
        let source = format!("classDiagram\n  Alpha {} Beta\n", row.op);
        let ir = fm_parser::parse(&source).ir;
        let e = ir
            .edges
            .first()
            .unwrap_or_else(|| panic!("`{}` produced NO EDGE at all", row.op));
        let both_ends = row.type1.is_some() && row.type2.is_some();
        // The start-side DEPENDENCY spellings are the documented exception: they map onto the
        // forward `Arrow`/`DottedArrow` and rely on the endpoints being SWAPPED, so there is no
        // reverse variant to pair against them — see `class_relation_co_arrow`.
        let swaps = row.type1 == Some(3);
        match (both_ends && !swaps, e.co_arrow()) {
            (true, None) => missing.push(format!("  {:<8} type1={:?} type2={:?}", row.op, row.type1, row.type2)),
            (false, Some(co)) => spurious.push(format!("  {:<8} unexpected co_arrow {co:?}", row.op)),
            _ => {}
        }
    }
    assert!(
        missing.is_empty() && spurious.is_empty(),
        "{} both-ends spelling(s) lost their far marker:\n{}\n{} spelling(s) gained one they \
         should not have:\n{}",
        missing.len(),
        missing.join("\n"),
        spurious.len(),
        spurious.join("\n")
    );
}
