//! `A ()-- B` is a UML lollipop (provided interface), not a plain link (bd-lkm9i).
//!
//! THE DEFECT. mermaid's class grammar carries FIVE relation types —
//! `relationType {AGGREGATION:0, EXTENSION:1, COMPOSITION:2, DEPENDENCY:3, LOLLIPOP:4}` — and
//! `CLASS_OPERATORS` knew four. With no `()` spelling in the table the operator scan fell through to
//! the bare `--` and produced `ArrowType::Line` with NO markers and no warning: a provided interface
//! drawn byte-identically to the `A -- B` link beside it.
//!
//! This is the THIRD instance of one recurring defect in one table, after `<--` (bd-lfucm, an
//! association with no arrowhead) and `o--`/`*--` (bd-92b6, aggregation and composition lost). Each
//! time, a declared relationship form was silently demoted to a line because the longer spelling was
//! missing from the table.
//!
//! ⚠️ HOW IT WAS FOUND, AND WHY THE CORPUS COULD NOT FIND IT. Not by the equivalence corpus: a fresh
//! full `ci_docs_2000` run on current HEAD reports class 190/190 equivalent, 0 divergent, with this
//! bug live. Two reasons, and both matter for what this test has to assert. First, the corpus
//! contains no lollipop relation at all. Second, and less obvious: unlike `o--`, the lollipop mints
//! NO PHANTOM NODE — identifier normalization absorbs the `()`, so the node SET is already correct
//! and a node-ID diff is blind to the defect by construction. It was found by diffing the pinned
//! incumbent's own class LEXER against this table.
//!
//! The incumbent's acceptance was measured, not assumed. `parse_probe.mjs` reports every accepted
//! classDiagram input as RUNTIME ERROR (the known no-DOM DOMPurify gap), so the verdict was
//! calibrated first: `<|--` and `-->` land in the same ACCEPTED bucket that `()--`, `--()`, `()..`
//! and `..()` do, while `@@##@@` and `<|-->>|<` return SYNTAX ERROR. The bucket discriminates.

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

/// Sorted node ids, to state what the diagram is actually made of.
fn node_ids(source: &str) -> Vec<String> {
    let ir = fm_parser::parse(source).ir;
    let mut ids: Vec<String> = ir.nodes.iter().map(|n| n.id.clone()).collect();
    ids.sort();
    ids
}

/// ⚠️ THE NEGATIVE CASE: a lollipop must not render as the plain link it collapsed into.
///
/// Asserting `Lollipop` alone would still pass if `--` were ALSO changed to `Lollipop`; comparing
/// the two forms is what makes this a statement about the collapse rather than about one constant.
/// Both halves are asserted, exactly as `an_association_does_not_render_as_a_plain_link` does.
#[test]
fn a_lollipop_does_not_render_as_a_plain_link() {
    let (socket, _, _) = edge("classDiagram\n  A ()-- B\n");
    let (plain, _, _) = edge("classDiagram\n  A -- B\n");
    assert_ne!(
        socket, plain,
        "`A ()-- B` still draws the same edge as `A -- B`"
    );
    assert_eq!(socket, ArrowType::Lollipop, "the lollipop has no socket");
    assert_eq!(plain, ArrowType::Line, "the plain link gained a socket");
}

/// The socket sits on the end the author wrote it on, which the arrow-type check alone cannot see.
///
/// `()--` provides at the SOURCE, `--()` at the TARGET. Collapsing both onto one variant would keep
/// every node id correct and still state the wrong half of the relationship — which interface is
/// provided by which class.
#[test]
fn the_socket_sits_on_the_end_the_author_wrote() {
    let (start, from, to) = edge("classDiagram\n  A ()-- B\n");
    assert_eq!(start, ArrowType::Lollipop);
    assert_eq!((from.as_str(), to.as_str()), ("A", "B"));

    let (end, rfrom, rto) = edge("classDiagram\n  A --() B\n");
    assert_eq!(end, ArrowType::LollipopReverse);
    assert_eq!((rfrom.as_str(), rto.as_str()), ("A", "B"));

    assert_ne!(
        start, end,
        "`()--` and `--()` put the socket on the same end"
    );
}

/// ⚠️ THE REGRESSION GUARD THE SIBLING BUGS NEEDED: no phantom node, on any spelling.
///
/// `o--` minted `C3-o` from `C3 o-- C4` (bd-92b6) and the flowchart `o--o` minted `A_o` (bd-zdpwd),
/// both because a leading marker was absorbed into the source id. The lollipop never did — which is
/// precisely why the corpus gate could not see this defect — so this asserts the property that was
/// already true and must survive the table edit, rather than one the fix established.
#[test]
fn no_spelling_mints_a_phantom_node() {
    for spelling in ["()--", "--()", "()..", "..()", "--", ".."] {
        let source = format!("classDiagram\n  A {spelling} B\n");
        assert_eq!(
            node_ids(&source),
            vec!["A".to_string(), "B".to_string()],
            "`A {spelling} B` did not build exactly the two classes the author declared"
        );
    }
}

/// The DOTTED spelling carries the same socket and adds the dash — it is not a third relation.
///
/// mermaid's class relation is a PRODUCT of a relation type and a line type
/// (`[relation][line][relation]` over `lineType {LINE, DOTTED_LINE}`), so `()..` is a lollipop that
/// happens to be dashed. The socket therefore rides on `ArrowType::Lollipop` and the dash rides on
/// the edge's inline style, exactly as UML realization does — asserting the arrow type alone would
/// not notice a fix that dashed `()--` too.
#[test]
fn the_dotted_spelling_is_dashed_and_the_solid_one_is_not() {
    let dashed = fm_parser::parse("classDiagram\n  A ()..  B\n").ir;
    let solid = fm_parser::parse("classDiagram\n  A ()-- B\n").ir;

    assert_eq!(dashed.edges[0].arrow, ArrowType::Lollipop);
    assert_eq!(solid.edges[0].arrow, ArrowType::Lollipop);

    let dash_of = |ir: &fm_core::MermaidDiagramIr| {
        ir.edges[0]
            .inline_style
            .as_ref()
            .and_then(|s| s.properties.get("stroke-dasharray"))
            .cloned()
    };
    assert_eq!(
        dash_of(&dashed).as_deref(),
        Some("5"),
        "`()..` is not dashed, so it is indistinguishable from `()--`"
    );
    assert_eq!(
        dash_of(&solid),
        None,
        "`()--` gained a dash it never asked for"
    );
}

/// ⚠️ A LOLLIPOP IS NOT THE FLOWCHART CIRCLE, and the distinction is the point of the marker.
///
/// The `--o` terminator is a FILLED circle; UML draws a provided interface as an unfilled socket.
/// Mapping the lollipop onto `ArrowType::Circle` would keep every node id and every endpoint right
/// and still draw a ball where the socket belongs — re-creating the "two declared forms, one
/// picture" defect this whole family is about.
#[test]
fn a_lollipop_is_not_the_filled_circle_terminator() {
    let (socket, _, _) = edge("classDiagram\n  A ()-- B\n");
    assert_ne!(socket, ArrowType::Circle);
    assert_ne!(socket, ArrowType::CircleBoth);

    let (aggregation, _, _) = edge("classDiagram\n  A o-- B\n");
    assert_ne!(
        socket, aggregation,
        "a lollipop and an aggregation are the same edge"
    );
}

/// ⚠️ THE TOKEN-BOUNDARY HAZARD, which is how the sibling fix nearly caused a worse regression.
///
/// bd-zdpwd records that adding leading-marker forms to the flowchart table without a boundary
/// guard turned `Foo--o Bar` into the node `Fo`: the operator matched INSIDE an identifier. `()` is
/// a far more common substring in a class diagram than a leading `o` — every method row ends with
/// it — so a member line must not be mistaken for a relation now that `()` opens one.
#[test]
fn a_method_row_is_not_a_lollipop_relation() {
    for source in [
        "classDiagram\n  class A {\n    +run() void\n  }\n",
        "classDiagram\n  A : +run() void\n",
        "classDiagram\n  A : -calc() int\n",
    ] {
        let ir = fm_parser::parse(source).ir;
        assert!(
            ir.edges.is_empty(),
            "a method row became a relation in {source:?}: {:?}",
            ir.edges
                .iter()
                .map(|e| e.arrow)
                .collect::<Vec<fm_core::ArrowType>>()
        );
    }
}

/// The table must try `()--` before the bare `--` it contains.
///
/// `operator_tables_are_longest_prefix_first` enforces the ordering rule table-wide; this states the
/// consequence for THIS pair specifically, so a reorder that keeps the invariant satisfied for the
/// table as a whole but breaks the lollipop still fails a test that names it.
#[test]
fn the_longer_spelling_wins_over_the_bare_link_it_contains() {
    let (arrow, from, to) = edge("classDiagram\n  A ()-- B\n");
    assert_eq!(
        arrow,
        ArrowType::Lollipop,
        "`--` matched first and swallowed the socket"
    );
    assert_eq!(
        (from.as_str(), to.as_str()),
        ("A", "B"),
        "the split landed in the wrong place and left `()` glued to an id"
    );
}
