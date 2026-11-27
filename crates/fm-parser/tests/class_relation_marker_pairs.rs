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
    co_arrow: Option<ArrowType>,
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
    let dashed = matches!(
        e.arrow,
        ArrowType::DottedArrow | ArrowType::DoubleDottedArrow | ArrowType::DottedLine
    ) || e
        .inline_style
        .as_ref()
        .is_some_and(|style| style.properties.contains_key("stroke-dasharray"));
    Edge {
        arrow: e.arrow,
        co_arrow: e.co_arrow(),
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

/// Decode the IR's marker vocabulary independently of the Mermaid spelling. A co-arrow supplies
/// the target marker, so combining it with an existing target marker would be ambiguous between
/// renderers and is rejected here.
fn marker_pair(arrow: ArrowType) -> (Option<u8>, Option<u8>) {
    match arrow {
        ArrowType::Line | ArrowType::DottedLine => (None, None),
        ArrowType::Arrow | ArrowType::DottedArrow => (None, Some(3)),
        ArrowType::DoubleArrow | ArrowType::DoubleDottedArrow => (Some(3), Some(3)),
        ArrowType::Aggregation => (Some(0), None),
        ArrowType::AggregationReverse => (None, Some(0)),
        ArrowType::Inheritance => (Some(1), None),
        ArrowType::InheritanceReverse => (None, Some(1)),
        ArrowType::Composition => (Some(2), None),
        ArrowType::CompositionReverse => (None, Some(2)),
        ArrowType::Lollipop => (Some(4), None),
        ArrowType::LollipopReverse => (None, Some(4)),
        other => panic!("unexpected class relation arrow {other:?}"),
    }
}

fn authored_marker_pair(edge: &Edge) -> (Option<u8>, Option<u8>) {
    let (start, mut end) = marker_pair(edge.arrow);
    if let Some(co_arrow) = edge.co_arrow {
        let (co_start, co_end) = marker_pair(co_arrow);
        assert_eq!(co_start, None, "a co-arrow must mark the target");
        assert!(co_end.is_some(), "a co-arrow must carry a marker");
        assert_eq!(end, None, "two markers compete for the target");
        end = co_end;
    }
    match (edge.from.as_str(), edge.to.as_str()) {
        ("Alpha", "Beta") => (start, end),
        ("Beta", "Alpha") => (end, start),
        ends => panic!("unexpected class relation endpoints {ends:?}"),
    }
}

/// Compare both physical ends with the pinned Mermaid fixture, including reversed IR edges.
/// Checking only the primary ArrowType missed all ten dependency-start, two-ended spellings:
/// `<--*` looked like `<--` while its arrow landed on Beta and its diamond disappeared (bd-onsya).
#[test]
fn every_relation_keeps_both_markers_on_the_authored_classes() {
    let mut bad = Vec::new();
    for row in rows() {
        let edge = edge(&row.op);
        let actual = authored_marker_pair(&edge);
        let expected = (row.type1, row.type2);
        if actual != expected {
            bad.push(format!(
                "  {:<8} expected {expected:?}, got {actual:?} ({} -> {}, {:?} + {:?})",
                row.op, edge.from, edge.to, edge.arrow, edge.co_arrow
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{} spelling(s) disagree with the incumbent's endpoint markers:\n{}",
        bad.len(),
        bad.join("\n")
    );
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
    for op in [
        "<--", "<..", "<--o", "<--*", "<--|>", "<--()", "<..o", "<..*", "<..|>", "<..()",
    ] {
        let e = edge(op);
        assert_eq!(
            (e.from.as_str(), e.to.as_str()),
            ("Beta", "Alpha"),
            "`Alpha {op} Beta` must draw its head at Alpha"
        );
    }
    // Forward and native double-arrow spellings keep the authored order.
    for op in ["-->", "..>", "<-->", "<..>"] {
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

/// Mixed marker pairs need a target co-arrow, including dependency-start pairs (bd-onsya).
/// Native double arrows carry both markers themselves and must not duplicate their target marker.
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
        let native_double_arrow = row.type1 == Some(3) && row.type2 == Some(3);
        match (both_ends && !native_double_arrow, e.co_arrow()) {
            (true, None) => missing.push(format!(
                "  {:<8} type1={:?} type2={:?}",
                row.op, row.type1, row.type2
            )),
            (false, Some(co)) => {
                spurious.push(format!("  {:<8} unexpected co_arrow {co:?}", row.op))
            }
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

/// Endpoint normalization must move multiplicities with their classes while keeping generics and
/// the relationship label intact. Exercise the full fixture because marker direction and line
/// style are independent of these grammar fields.
#[test]
fn relation_metadata_stays_with_its_authored_class_after_normalization() {
    for row in rows() {
        let source = format!(
            "classDiagram\n  Alpha~T~ \"1\" {} \"many\" Beta~U~ : maps List~T~\n",
            row.op
        );
        let parsed = fm_parser::parse(&source);
        assert!(
            parsed.warnings.is_empty(),
            "`{}` emitted warnings: {:?}",
            row.op,
            parsed.warnings
        );
        let ir = parsed.ir;
        assert_eq!(ir.nodes.len(), 2, "`{}` changed the class set", row.op);
        assert_eq!(ir.edges.len(), 1, "`{}` changed the relation count", row.op);
        let edge = &ir.edges[0];
        for (endpoint, cardinality) in [
            (&edge.from, edge.source_cardinality()),
            (&edge.to, edge.target_cardinality()),
        ] {
            let IrEndpoint::Node(id) = endpoint else {
                panic!("`{}` has a non-class endpoint: {endpoint:?}", row.op);
            };
            let node = &ir.nodes[id.0];
            let (expected_cardinality, expected_generic) = match node.id.as_str() {
                "Alpha" => ("1", "T"),
                "Beta" => ("many", "U"),
                other => panic!("`{}` invented class {other:?}", row.op),
            };
            assert_eq!(
                cardinality,
                Some(expected_cardinality),
                "`{}` moved the multiplicity away from {}",
                row.op,
                node.id
            );
            assert_eq!(
                node.class_meta.as_ref().unwrap().generics,
                [expected_generic],
                "`{}` changed the generic for {}",
                row.op,
                node.id
            );
        }
        let label = edge
            .label
            .expect("relationship label survives normalization");
        assert_eq!(ir.labels[label.0].text, "maps List~T~", "`{}`", row.op);
    }
}
