//! Trapezoid-family nodes survive alongside each other in ONE statement.
//!
//! TWO INDEPENDENT DEFECTS, both of which made well-formed mermaid vanish from the diagram. Found by
//! a render-level differential against the pinned bundle: `fc trapezoid` drew `2/1/0` in mermaid and
//! `0/0/0` here — not a wrong shape, NOTHING.
//!
//! **1. The grammar's slanted matchers ran past their own node.** `flow_statement_parser`'s four
//! slanted alternatives each stopped only at their OWN closer, so a matcher whose closer was absent
//! from its node kept going and matched a LATER node's, swallowing the arrow between them as label
//! text. `parallelogram` (`[/`…`/]`) is tried first, so:
//!
//! ```text
//!   A[/t\] --> B[/x/]     opens at `A[/`, finds no `/]` until the SECOND node,
//!                         takes `t\] --> B[/x` as its label, and the statement then failed whole
//! ```
//!
//! It is NOT about mixed slash directions, which is what the symptom first looked like:
//! `A[/t/] --> B[/x\]` was always fine, because the first node carries the `/]` the matcher wants.
//! `trap -> para` and `invtrap -> invpara` — same slash on both openers — died too.
//!
//! **2. `auto_close_delimiters` let the parallelogram probe eat a well-formed trapezoid.** On the
//! `parse_node_token_core` path the same four shapes are probed in order, and auto-close (ON by
//! default) tells each probe to accept a token that does NOT end with its closer, so it can rescue an
//! unterminated `[/x`. First-probe-wins then meant `A[/a\]` was salvaged as an unterminated
//! PARALLELOGRAM labelled `a\]`, whose unmatched `]` tripped `warn_if_label_holds_unmatched_bracket`
//! and dropped the node. Short statements escaped this because they reach the grammar instead; this
//! path takes over for 3+-hop chains, so `A[/a\] --> B[/b\] --> C[/c\]` drew nothing while its 2-hop
//! prefix was correct.
//!
//! Defect 2 was proven by MECHANISM, not by reading: every failing chain parses correctly with
//! `auto_close_delimiters: false` and the rect control is unchanged either way.
//!
//! MEASURED REFERENCE — pinned mermaid 11.15.0 rendered in Chromium 151, counting drawn `g.node` /
//! `path.flowchart-link` and reading each node's text. Every one of these draws every node:
//!
//! ```text
//!   A[/t\] --> B[\x/]                    nodes=2 edges=1   t  x
//!   A[/t\] --> B[/x/]                    nodes=2 edges=1   t  x
//!   A[\t/] --> B[\x\]                    nodes=2 edges=1   t  x
//!   A[/t/] --> B[/x\]                    nodes=2 edges=1   t  x
//!   A[/a\] --> B[/b\] --> C[/c/]         nodes=3 edges=2   a  b  c
//! ```
//!
//! (The reference draws every slanted shape as a `polygon`, so the drawn TAG cannot tell trapezoid
//! from parallelogram. The labels and the counts are what it pins; the shape assignments below are
//! pinned against mermaid's documented delimiters, which is the only thing that distinguishes them.)

use fm_core::{MermaidParseMode, NodeShape};
use fm_parser::ParserConfig;

const ARROW: &str = "-->";

/// `(id, shape, label)` for every node, in IR order.
fn nodes(source: &str) -> Vec<(String, NodeShape, String)> {
    let ir = fm_parser::parse(source).ir;
    ir.nodes
        .iter()
        .map(|node| {
            let label = node
                .label
                .and_then(|id| ir.labels.get(id.0))
                .map(|label| label.text.clone())
                .unwrap_or_default();
            (node.id.clone(), node.shape, label)
        })
        .collect()
}

fn edge_count(source: &str) -> usize {
    fm_parser::parse(source).ir.edges.len()
}

/// ⚠️ THE PLANTED NEGATIVE: no label may contain a delimiter or an arrow.
///
/// Both defects failed by letting SYNTAX become label text — `t\] --> B[/x` from the grammar,
/// `a\]` from the auto-close salvage. A count-only or shape-only assertion is not enough to catch a
/// wrong fix: a repair that made these statements parse while still handing the label the rest of
/// the token would satisfy both. The label is the evidence that the node boundary was found in the
/// right place, so it is asserted for every node, and the pre-fix text is rejected by name.
fn assert_label_is_not_syntax(label: &str, case: &str) {
    for fragment in [']', '[', '/', '\\'] {
        assert!(
            !label.contains(fragment),
            "{case}: label {label:?} still holds the delimiter {fragment:?} — \
             the node boundary was not found, the token was salvaged past its own end"
        );
    }
    assert!(
        !label.contains(ARROW),
        "{case}: label {label:?} swallowed the arrow — a matcher ran into the next node"
    );
}

/// Two slanted nodes in one statement: every combination keeps both nodes, its shapes and its edge.
///
/// The first four rows are the ones that used to drop the ENTIRE statement (0 nodes, 0 edges). The
/// last four always worked and are kept as controls — the fix changes a shared matcher, so the
/// combinations that were already right must be shown still right.
#[test]
fn a_slanted_pair_in_one_statement_keeps_both_nodes() {
    for (case, source, left, right) in [
        (
            "trap -> invtrap",
            format!("flowchart LR\n  A[/t\\] {ARROW} B[\\x/]\n"),
            NodeShape::Trapezoid,
            NodeShape::InvTrapezoid,
        ),
        (
            "trap -> para",
            format!("flowchart LR\n  A[/t\\] {ARROW} B[/x/]\n"),
            NodeShape::Trapezoid,
            NodeShape::Parallelogram,
        ),
        (
            "invtrap -> invpara",
            format!("flowchart LR\n  A[\\t/] {ARROW} B[\\x\\]\n"),
            NodeShape::InvTrapezoid,
            NodeShape::InvParallelogram,
        ),
        (
            "para -> trap",
            format!("flowchart LR\n  A[/t/] {ARROW} B[/x\\]\n"),
            NodeShape::Parallelogram,
            NodeShape::Trapezoid,
        ),
        (
            "CONTROL trap -> trap",
            format!("flowchart LR\n  A[/t\\] {ARROW} B[/x\\]\n"),
            NodeShape::Trapezoid,
            NodeShape::Trapezoid,
        ),
        (
            "CONTROL para -> para",
            format!("flowchart LR\n  A[/t/] {ARROW} B[/x/]\n"),
            NodeShape::Parallelogram,
            NodeShape::Parallelogram,
        ),
        (
            "CONTROL invpara -> invpara",
            format!("flowchart LR\n  A[\\t\\] {ARROW} B[\\x\\]\n"),
            NodeShape::InvParallelogram,
            NodeShape::InvParallelogram,
        ),
        (
            "CONTROL invtrap -> invtrap",
            format!("flowchart LR\n  A[\\t/] {ARROW} B[\\x/]\n"),
            NodeShape::InvTrapezoid,
            NodeShape::InvTrapezoid,
        ),
    ] {
        let parsed = nodes(&source);
        assert_eq!(
            parsed.len(),
            2,
            "{case}: the reference draws two nodes, we produced {parsed:?}"
        );
        assert_eq!(edge_count(&source), 1, "{case}: the edge was lost");

        assert_eq!(parsed[0].0, "A", "{case}: wrong source id");
        assert_eq!(parsed[0].1, left, "{case}: wrong source shape");
        assert_eq!(parsed[0].2, "t", "{case}: wrong source label");
        assert_label_is_not_syntax(&parsed[0].2, case);

        assert_eq!(parsed[1].0, "B", "{case}: wrong target id");
        assert_eq!(parsed[1].1, right, "{case}: wrong target shape");
        assert_eq!(parsed[1].2, "x", "{case}: wrong target label");
        assert_label_is_not_syntax(&parsed[1].2, case);
    }
}

/// A trapezoid survives a chain of three or more hops — the second defect's signature.
///
/// A 2-hop statement reaches the grammar and was already correct after the first fix, so a chain is
/// the only shape that exercises `parse_node_token_core`'s probe order. The single-trapezoid rows
/// matter as much as the all-trapezoid ones: the node that vanished was whichever hop the trapezoid
/// sat in, so a fix that repaired only the all-slanted case would pass a lazier test.
#[test]
fn a_trapezoid_survives_a_multi_hop_chain() {
    for (case, source, ids, shapes) in [
        (
            "trap trap trap",
            format!("flowchart LR\n  A[/a\\] {ARROW} B[/b\\] {ARROW} C[/c\\]\n"),
            vec!["A", "B", "C"],
            vec![
                NodeShape::Trapezoid,
                NodeShape::Trapezoid,
                NodeShape::Trapezoid,
            ],
        ),
        (
            "trap trap para",
            format!("flowchart LR\n  A[/a\\] {ARROW} B[/b\\] {ARROW} C[/c/]\n"),
            vec!["A", "B", "C"],
            vec![
                NodeShape::Trapezoid,
                NodeShape::Trapezoid,
                NodeShape::Parallelogram,
            ],
        ),
        (
            "rect trap rect — the trapezoid is in the MIDDLE",
            format!("flowchart LR\n  A[a] {ARROW} B[/b\\] {ARROW} C[c]\n"),
            vec!["A", "B", "C"],
            vec![NodeShape::Rect, NodeShape::Trapezoid, NodeShape::Rect],
        ),
        (
            "trap rect rect — the trapezoid is FIRST",
            format!("flowchart LR\n  A[/a\\] {ARROW} B[b] {ARROW} C[c]\n"),
            vec!["A", "B", "C"],
            vec![NodeShape::Trapezoid, NodeShape::Rect, NodeShape::Rect],
        ),
        (
            "rect rect trap — the trapezoid is LAST",
            format!("flowchart LR\n  A[a] {ARROW} B[b] {ARROW} C[/c\\]\n"),
            vec!["A", "B", "C"],
            vec![NodeShape::Rect, NodeShape::Rect, NodeShape::Trapezoid],
        ),
        (
            "invtrap x3",
            format!("flowchart LR\n  A[\\a/] {ARROW} B[\\b/] {ARROW} C[\\c/]\n"),
            vec!["A", "B", "C"],
            vec![
                NodeShape::InvTrapezoid,
                NodeShape::InvTrapezoid,
                NodeShape::InvTrapezoid,
            ],
        ),
        (
            "four hops, all trapezoid",
            format!("flowchart LR\n  A[/a\\] {ARROW} B[/b\\] {ARROW} C[/c\\] {ARROW} D[/d\\]\n"),
            vec!["A", "B", "C", "D"],
            vec![
                NodeShape::Trapezoid,
                NodeShape::Trapezoid,
                NodeShape::Trapezoid,
                NodeShape::Trapezoid,
            ],
        ),
        (
            "CONTROL rect x3 — never broken, must stay unbroken",
            format!("flowchart LR\n  A[a] {ARROW} B[b] {ARROW} C[c]\n"),
            vec!["A", "B", "C"],
            vec![NodeShape::Rect, NodeShape::Rect, NodeShape::Rect],
        ),
        (
            "CONTROL para x3 — the probe that used to win",
            format!("flowchart LR\n  A[/a/] {ARROW} B[/b/] {ARROW} C[/c/]\n"),
            vec!["A", "B", "C"],
            vec![
                NodeShape::Parallelogram,
                NodeShape::Parallelogram,
                NodeShape::Parallelogram,
            ],
        ),
        (
            "CONTROL cylinder x3 — a multi-char delimiter probed before the slanted four",
            format!("flowchart LR\n  A[(a)] {ARROW} B[(b)] {ARROW} C[(c)]\n"),
            vec!["A", "B", "C"],
            vec![
                NodeShape::Cylinder,
                NodeShape::Cylinder,
                NodeShape::Cylinder,
            ],
        ),
    ] {
        let parsed = nodes(&source);
        assert_eq!(
            parsed.len(),
            ids.len(),
            "{case}: expected the reference's {} nodes, got {parsed:?}",
            ids.len()
        );
        assert_eq!(
            edge_count(&source),
            ids.len() - 1,
            "{case}: a hop was dropped"
        );
        for (index, (expected_id, expected_shape)) in ids.iter().zip(shapes).enumerate() {
            assert_eq!(&parsed[index].0, expected_id, "{case}: wrong id at {index}");
            assert_eq!(
                parsed[index].1, expected_shape,
                "{case}: wrong shape at {index}"
            );
            assert_eq!(
                parsed[index].2,
                expected_id.to_ascii_lowercase(),
                "{case}: wrong label at {index}"
            );
            assert_label_is_not_syntax(&parsed[index].2, case);
        }
    }
}

/// CONTROL: the auto-close SALVAGE the second fix reordered is still reachable.
///
/// ⚠️ This is the control that stops the fix from being a regression dressed as a repair. Defect 2
/// was caused by auto-close firing too eagerly, and the cheapest "fix" — never auto-closing a
/// slanted token — would make every assertion above pass while silently deleting the recovery of
/// genuinely unterminated input. The exact-match round runs FIRST; the salvage round still runs
/// after it, and this pins that it does.
#[test]
fn an_unterminated_slanted_token_is_still_salvaged() {
    let source = format!("flowchart LR\n  A[/a {ARROW} B\n");
    let parsed = nodes(&source);
    assert_eq!(parsed.len(), 1, "the salvage produced {parsed:?}");
    assert_eq!(parsed[0].0, "A");
    assert_eq!(
        parsed[0].1,
        NodeShape::Parallelogram,
        "an unterminated `[/` is still recovered as the shape its opener names"
    );

    // And the toggle still turns it off, which is what makes the round-two guard meaningful.
    let strict = ParserConfig {
        auto_close_delimiters: false,
        ..ParserConfig::default()
    };
    let ir = fm_parser::parse_with_mode_and_config(&source, MermaidParseMode::Compat, &strict).ir;
    assert!(
        ir.nodes
            .iter()
            .all(|node| node.shape != NodeShape::Parallelogram),
        "with auto_close_delimiters off nothing may be salvaged as a parallelogram: {:?}",
        ir.nodes.iter().map(|n| n.shape).collect::<Vec<_>>()
    );
}

/// CONTROL: a well-formed slanted token parses identically with auto-close ON and OFF.
///
/// This is the mechanism that proved defect 2 stated as a standing invariant. Before the fix the two
/// configurations DISAGREED — off gave three trapezoids, on gave nothing — and that disagreement is
/// precisely the bug. A valid token must never depend on a recovery setting.
#[test]
fn auto_close_does_not_change_a_well_formed_slanted_chain() {
    let source = format!("flowchart LR\n  A[/a\\] {ARROW} B[/b\\] {ARROW} C[\\c/]\n");
    let strict = ParserConfig {
        auto_close_delimiters: false,
        ..ParserConfig::default()
    };
    let lenient = ParserConfig::default();
    assert!(
        lenient.auto_close_delimiters,
        "this control is vacuous unless the default really is ON"
    );

    let shapes = |config: &ParserConfig| {
        fm_parser::parse_with_mode_and_config(&source, MermaidParseMode::Compat, config)
            .ir
            .nodes
            .iter()
            .map(|node| node.shape)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        shapes(&strict),
        shapes(&lenient),
        "a well-formed token must not depend on the recovery setting"
    );
    assert_eq!(
        shapes(&lenient),
        vec![
            NodeShape::Trapezoid,
            NodeShape::Trapezoid,
            NodeShape::InvTrapezoid
        ]
    );
}
