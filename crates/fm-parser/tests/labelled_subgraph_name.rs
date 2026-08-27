//! `s1[Box]` where `s1` is a subgraph is that subgraph, and the label is discarded (bd-honvo).
//!
//! bd-pfibz stopped a BARE endpoint naming a subgraph from becoming a node. A LABELLED one —
//! `s1[Box]` — still did, drawing a third box captioned `Box` beside the cluster called `s1`.
//!
//! ⚠️ THE BEAD SAID THE REFERENCE RE-LABELS THE SUBGRAPH. IT DOES NOT. I wrote that from a guess
//! when filing; measuring a Chromium 151 render of the pinned mermaid 11.15.0 bundle showed the
//! cluster keeps its OWN title and the label is simply dropped:
//!
//! ```text
//!   subgraph s1 … end          + s1[Box] --> B    reference: 2 nodes, cluster titled `s1`
//!   subgraph s1[Original] … end + s1[Box] --> B   reference: 2 nodes, cluster titled `Original`
//! ```
//!
//! Neither draws `Box` anywhere. The bead's title was corrected to match before the fix was written.
//!
//! ⚠️ AND THE REFERENCE TREATS THE STANDALONE STATEMENT THE SAME WAY, which is why both spellings
//! are guarded. Measuring that mattered twice over: the first fix guarded only
//! `intern_flow_ast_node` and left `s1[Box]` on its own line still drawing the box, because a
//! standalone node has its own fast lowering path that never reaches that function.
//!
//! The guards sit at the flowchart sites rather than inside `intern_node_label`, which is shared
//! with sequence, ER and requirement — a sequence `box` is also a subgraph, and a participant whose
//! name collided with one would have been swallowed.

const ARROW: &str = "-->";

fn node_ids(source: &str) -> Vec<String> {
    fm_parser::parse(source)
        .ir
        .nodes
        .iter()
        .map(|n| n.id.clone())
        .collect()
}

fn cluster_titles(source: &str) -> Vec<String> {
    let parsed = fm_parser::parse(source);
    let ir = &parsed.ir;
    ir.clusters
        .iter()
        .map(|c| {
            c.title
                .and_then(|id| ir.labels.get(id.0))
                .map_or_else(String::new, |l| l.text.clone())
        })
        .collect()
}

/// ⚠️ THE NEGATIVE CASE: no `Box` node, and the diagram is still drawn.
///
/// "There is no node called `s1`" passes if the statement took the whole diagram with it. The
/// surviving nodes are asserted by name, not just counted.
#[test]
fn a_labelled_endpoint_naming_a_subgraph_adds_no_node() {
    for (name, source) in [
        (
            "edge endpoint",
            format!("flowchart LR\n  subgraph s1\n    A\n  end\n  s1[Box] {ARROW} B\n"),
        ),
        (
            "standalone statement",
            format!("flowchart LR\n  subgraph s1\n    A\n  end\n  s1[Box]\n  A {ARROW} B\n"),
        ),
    ] {
        let ids = node_ids(&source);
        assert_eq!(
            ids,
            vec!["A".to_string(), "B".to_string()],
            "{name}: expected the reference's two nodes"
        );
    }
}

/// ⚠️ AND THE LABEL IS DISCARDED, NOT PROMOTED TO THE SUBGRAPH'S TITLE.
///
/// This is the half the bead got wrong. Asserting only "no `Box` node" would pass if the label had
/// been moved onto the cluster instead — a different picture from the reference, and one that
/// silently renames the author's subgraph.
#[test]
fn the_label_is_discarded_and_the_subgraph_keeps_its_own_title() {
    let untitled = format!("flowchart LR\n  subgraph s1\n    A\n  end\n  s1[Box] {ARROW} B\n");
    assert_eq!(
        cluster_titles(&untitled),
        vec!["s1".to_string()],
        "the endpoint's label was promoted onto the subgraph"
    );

    let titled =
        format!("flowchart LR\n  subgraph s1[Original]\n    A\n  end\n  s1[Box] {ARROW} B\n");
    assert_eq!(
        cluster_titles(&titled),
        vec!["Original".to_string()],
        "the endpoint's label overwrote the subgraph's own title"
    );
}

/// The edge still exists and connects the subgraph's member.
#[test]
fn the_edge_survives_and_reaches_the_subgraphs_member() {
    let source = format!("flowchart LR\n  subgraph s1\n    A\n  end\n  s1[Box] {ARROW} B\n");
    let ir = fm_parser::parse(&source).ir;
    assert_eq!(ir.edges.len(), 1, "the edge was dropped with the node");
    let e = &ir.edges[0];
    let name = |end: &fm_core::IrEndpoint| match end {
        fm_core::IrEndpoint::Node(id) => ir.nodes[id.0].id.clone(),
        other => format!("{other:?}"),
    };
    assert_eq!(
        (name(&e.from).as_str(), name(&e.to).as_str()),
        ("A", "B"),
        "the edge was rewired to the wrong elements"
    );
}

/// CONTROL: a labelled node whose name is NOT a subgraph is still a node with its label.
///
/// The guard runs on every flowchart node, so the ordinary declaration has to be shown intact —
/// this is the case that would break if the check were widened to any known id.
#[test]
fn an_ordinary_labelled_node_keeps_its_label() {
    let source = format!("flowchart LR\n  subgraph s1\n    A\n  end\n  other[Box] {ARROW} B\n");
    let parsed = fm_parser::parse(&source);
    let ir = &parsed.ir;
    let label = ir
        .nodes
        .iter()
        .find(|n| n.id == "other")
        .and_then(|n| n.label)
        .and_then(|id| ir.labels.get(id.0))
        .map(|l| l.text.as_str());
    assert_eq!(
        label,
        Some("Box"),
        "an ordinary labelled node lost its label"
    );
}

/// CONTROL: a labelled node declared where NO subgraph exists at all is untouched.
///
/// `subgraph_endpoint_member` returns early when the diagram has no subgraphs, so the common
/// flowchart pays nothing and behaves exactly as before.
#[test]
fn a_diagram_without_subgraphs_is_unaffected() {
    let source = format!("flowchart LR\n  s1[Box] {ARROW} B\n");
    let parsed = fm_parser::parse(&source);
    let ir = &parsed.ir;
    assert_eq!(node_ids(&source), vec!["s1".to_string(), "B".to_string()]);
    let label = ir
        .nodes
        .iter()
        .find(|n| n.id == "s1")
        .and_then(|n| n.label)
        .and_then(|id| ir.labels.get(id.0))
        .map(|l| l.text.as_str());
    assert_eq!(label, Some("Box"));
}

/// CONTROL: a shape declared on the name is discarded with the label, not applied to something else.
///
/// `s1{Diamond}` names the subgraph too. The shape must not leak onto the member the endpoint
/// resolves to — that would silently restyle a node the author never mentioned.
#[test]
fn a_shape_on_the_subgraph_name_does_not_leak_onto_the_member() {
    let source = format!("flowchart LR\n  subgraph s1\n    A\n  end\n  s1{{Diamond}} {ARROW} B\n");
    let parsed = fm_parser::parse(&source);
    let ir = &parsed.ir;
    let a = ir.nodes.iter().find(|n| n.id == "A").expect("member A");
    assert_eq!(
        a.shape,
        fm_core::NodeShape::Rect,
        "the discarded shape leaked onto the subgraph's member"
    );
    assert_eq!(node_ids(&source), vec!["A".to_string(), "B".to_string()]);
}
