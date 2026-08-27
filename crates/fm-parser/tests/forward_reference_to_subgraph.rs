//! An edge naming a subgraph declared LATER attaches to it, it does not invent a node (bd-dw2a9).
//!
//! THE DEFECT. bd-pfibz fixed `s1 --> B` written AFTER `subgraph s1`. Written BEFORE it, the same
//! edge still interned a phantom box: the subgraph did not exist yet, so the endpoint fell through
//! to creating a node called `s1` beside the cluster of the same name.
//!
//! ⚠️ THIS BEAD WAS FILED AS "BLOCKED ON MISSING INFRASTRUCTURE" BY ME, AND THAT NOTE WAS WRONG.
//! It reasoned that the phantom is created before the subgraph is known, so it can only be removed
//! afterwards — which needs `IrNodeId` compaction: remapping every id in `edges`, `clusters.members`,
//! `subgraphs.members`, `graph.nodes`, `graph.edges` and both id maps, where one missed reference
//! silently mis-wires a diagram. The premise was the mistake. `parse_flowchart_document` builds the
//! WHOLE item tree before a single item is lowered, so the answer is available before the phantom
//! would be created. Nothing is removed here because nothing wrong is ever created, and no
//! compaction was needed. The cheap alternative was never tested before the bead was written.
//!
//! MEASURED against the pinned mermaid 11.15.0 bundle in Chromium 151 (node text / cluster count):
//!
//! ```text
//!   s1 --> s2  before both blocks      ref nodes A,B      2 clusters   (ours: 4 nodes)
//!   s1 --> B   before subgraph s1      ref nodes A,B      1 cluster    (ours: 3)
//!   s1[Box] --> B before it            ref nodes A,B      1 cluster    label discarded
//!   s1 --> B   where s1 is EMPTY       ref nodes s1,B     0 clusters   s1 IS the node (bd-kat55)
//!   inner --> C, inner nested          ref nodes A,C      2 clusters
//! ```
//!
//! ⚠️ AND THE REFERENCE RESOLVES TRANSITIVELY, WHICH ONE PROBE ROW IS THE ONLY REASON I KNOW. A
//! subgraph whose own first statement is a forward reference resolves THROUGH it; a single hop
//! would have interned a phantom and drawn one box too many. See the chained test below.

const ARROW: &str = "-->";

fn counts(source: &str) -> (usize, usize, usize) {
    let ir = fm_parser::parse(source).ir;
    (ir.nodes.len(), ir.clusters.len(), ir.edges.len())
}

fn node_ids(source: &str) -> Vec<String> {
    fm_parser::parse(source)
        .ir
        .nodes
        .iter()
        .map(|n| n.id.clone())
        .collect()
}

fn edge_ends(source: &str) -> Vec<(String, String)> {
    let ir = fm_parser::parse(source).ir;
    let name = |end: &fm_core::IrEndpoint| match end {
        fm_core::IrEndpoint::Node(id) => ir.nodes[id.0].id.clone(),
        other => format!("{other:?}"),
    };
    ir.edges
        .iter()
        .map(|e| (name(&e.from), name(&e.to)))
        .collect()
}

/// ⚠️ THE NEGATIVE CASE: no phantom, AND the edge still connects the two subgraphs' members.
///
/// "There is no node called `s1`" is also true if the edge was dropped, or if the endpoint was
/// wired to whatever node happened to be interned first. The wiring is asserted by name.
#[test]
fn a_forward_reference_creates_no_phantom_node() {
    let source = format!(
        "flowchart LR\n  s1 {ARROW} s2\n  subgraph s1\n    A\n  end\n  subgraph s2\n    B\n  end\n"
    );
    assert_eq!(
        counts(&source),
        (2, 2, 1),
        "expected the reference's 2 nodes / 2 clusters / 1 edge, got {:?}",
        node_ids(&source)
    );
    assert_eq!(
        edge_ends(&source),
        vec![("A".to_string(), "B".to_string())],
        "the edge was dropped or rewired"
    );
}

/// One forward endpoint and one ordinary node.
#[test]
fn one_forward_endpoint_and_one_node_endpoint() {
    let source = format!("flowchart LR\n  s1 {ARROW} B\n  subgraph s1\n    A\n  end\n");
    assert_eq!(counts(&source), (2, 1, 1));
    assert_eq!(edge_ends(&source), vec![("A".to_string(), "B".to_string())]);
}

/// A titled subgraph resolves by its id, forward as well as backward.
#[test]
fn a_forward_reference_to_a_titled_subgraph_resolves_by_id() {
    let source = format!("flowchart LR\n  s1 {ARROW} B\n  subgraph s1[The One]\n    A\n  end\n");
    assert_eq!(counts(&source), (2, 1, 1));
    assert!(!node_ids(&source).iter().any(|id| id == "s1"));
}

/// ⚠️ FORWARD AND BACKWARD REFERENCES MUST LAND ON THE SAME NODE.
///
/// Both resolve to the subgraph's FIRST member. A two-member subgraph is the only fixture that can
/// tell "first" from "last" apart — with one member the two are the same node and an arm that swaps
/// them passes (that is how bd-pfibz's own control was found to be inert).
#[test]
fn a_forward_reference_attaches_to_the_same_member_as_a_backward_one() {
    let forward = format!("flowchart LR\n  s1 {ARROW} C\n  subgraph s1\n    A\n    B\n  end\n");
    let backward = format!("flowchart LR\n  subgraph s1\n    A\n    B\n  end\n  s1 {ARROW} C\n");
    assert_eq!(counts(&forward), (3, 1, 1));
    assert_eq!(
        edge_ends(&forward),
        vec![("A".to_string(), "C".to_string())],
        "the forward reference did not attach to the subgraph's first member"
    );
    assert_eq!(
        edge_ends(&forward),
        edge_ends(&backward),
        "the same edge resolves differently depending on where the subgraph is written"
    );
}

/// ⚠️ RESOLUTION IS TRANSITIVE, AND ONE LINK IS NOT ENOUGH TO PROVE IT.
///
/// This test alone was a false comfort: with a single chain link, "one hop" and "walk to a fixed
/// point" give the SAME answer, so a negative-control arm that replaced the loop with one `if`
/// passed it. A peer's commit did exactly that simplification while this bead was in flight, and
/// this test would not have caught it. `chains_of_any_depth_resolve_to_the_final_member` below is
/// the one that discriminates; this one stays as the minimal case.
///
/// Measured: the reference renders 3 nodes (`Z`, `Y`, `X`) and 2 clusters.
#[test]
fn a_forward_reference_through_another_subgraph_resolves_transitively() {
    let source = format!(
        "flowchart LR\n  s1 {ARROW} X\n  subgraph s1\n    s2 {ARROW} Y\n  end\n  subgraph s2\n    Z\n  end\n"
    );
    assert_eq!(
        counts(&source),
        (3, 2, 2),
        "expected the reference's 3 nodes / 2 clusters / 2 edges, got {:?}",
        node_ids(&source)
    );
    assert!(
        !node_ids(&source).iter().any(|id| id == "s2"),
        "a single-hop resolution left a phantom: {:?}",
        node_ids(&source)
    );
}

/// ⚠️ THE ARM THAT A ONE-HOP RESOLUTION ACTUALLY FAILS.
///
/// Each subgraph's first member forward-references the next, so the endpoint must be walked to the
/// END of the chain. Measured against the pinned bundle:
///
/// ```text
///   depth 2   ref 3 nodes / 2 clusters / 2 edges
///   depth 3   ref 4 nodes / 3 clusters / 3 edges
///   depth 4   ref 5 nodes / 4 clusters / 4 edges
/// ```
///
/// A single hop stops one short and interns a phantom for every link past the first.
#[test]
fn chains_of_any_depth_resolve_to_the_final_member() {
    let depth3 = format!(
        "flowchart LR\n  s1 {ARROW} X\n  subgraph s1\n    s2 {ARROW} Y\n  end\n  \
         subgraph s2\n    s3 {ARROW} W\n  end\n  subgraph s3\n    Z\n  end\n"
    );
    assert_eq!(
        counts(&depth3),
        (4, 3, 3),
        "a 3-link chain left a phantom: {:?}",
        node_ids(&depth3)
    );

    let depth4 = format!(
        "flowchart LR\n  s1 {ARROW} X\n  subgraph s1\n    s2 {ARROW} Y\n  end\n  \
         subgraph s2\n    s3 {ARROW} W\n  end\n  subgraph s3\n    s4 {ARROW} V\n  end\n  \
         subgraph s4\n    Z\n  end\n"
    );
    assert_eq!(
        counts(&depth4),
        (5, 4, 4),
        "a 4-link chain left a phantom: {:?}",
        node_ids(&depth4)
    );
    for phantom in ["s1", "s2", "s3", "s4"] {
        assert!(
            !node_ids(&depth4).iter().any(|id| id == phantom),
            "`{phantom}` was interned as a node: {:?}",
            node_ids(&depth4)
        );
    }
}

/// A label on a forward endpoint is discarded, exactly as bd-honvo measured for a backward one.
#[test]
fn a_label_on_a_forward_endpoint_is_discarded() {
    let source = format!("flowchart LR\n  s1[Box] {ARROW} B\n  subgraph s1\n    A\n  end\n");
    assert_eq!(counts(&source), (2, 1, 1));
    let ir = fm_parser::parse(&source).ir;
    assert!(
        !ir.labels.iter().any(|l| l.text == "Box"),
        "the discarded label was interned anyway"
    );
}

/// The standalone statement form is guarded too, forward as well as backward.
#[test]
fn a_standalone_statement_naming_a_later_subgraph_adds_no_node() {
    for source in [
        format!("flowchart LR\n  s1[Box]\n  subgraph s1\n    A\n  end\n  A {ARROW} B\n"),
        format!("flowchart LR\n  s1\n  subgraph s1\n    A\n  end\n  A {ARROW} B\n"),
    ] {
        assert_eq!(
            counts(&source),
            (2, 1, 1),
            "a standalone forward reference drew a box: {:?}",
            node_ids(&source)
        );
    }
}

/// ⚠️ A FORWARD REFERENCE TO AN EMPTY SUBGRAPH INTERNS THE NODE, because an empty subgraph IS a
/// node (bd-kat55). It is deliberately absent from the resolution map; swallowing it would draw one
/// box FEWER than the reference, the mirror image of the defect this bead is about.
#[test]
fn a_forward_reference_to_an_empty_subgraph_keeps_the_node() {
    let source = format!("flowchart LR\n  s1 {ARROW} B\n  subgraph s1\n  end\n");
    assert_eq!(counts(&source), (2, 0, 1));
    assert_eq!(node_ids(&source), vec!["s1".to_string(), "B".to_string()]);
}

/// A forward reference to a NESTED subgraph resolves to that subgraph's member.
#[test]
fn a_forward_reference_to_a_nested_subgraph_resolves() {
    let source = format!(
        "flowchart LR\n  inner {ARROW} C\n  subgraph outer\n    subgraph inner\n      A\n    end\n  end\n"
    );
    assert_eq!(counts(&source), (2, 2, 1));
    assert_eq!(edge_ends(&source), vec![("A".to_string(), "C".to_string())]);
}

/// The member's own label still lands, even though the edge interned it first.
///
/// The endpoint interns `A` with no label before `A[Hello]` is read. Measured: the reference draws
/// `Hello`. If the early intern froze the node label-less, this diagram would lose its caption.
#[test]
fn a_member_interned_early_by_an_edge_still_gets_its_label() {
    let source = format!("flowchart LR\n  s1 {ARROW} B\n  subgraph s1\n    A[Hello]\n  end\n");
    let ir = fm_parser::parse(&source).ir;
    let label = ir
        .nodes
        .iter()
        .find(|n| n.id == "A")
        .and_then(|n| n.label)
        .and_then(|id| ir.labels.get(id.0))
        .map(|l| l.text.as_str());
    assert_eq!(label, Some("Hello"), "the early intern lost the label");
}

/// CONTROL: an ordinary forward reference to a plain node is untouched.
///
/// `A --> B` written before `subgraph s1 { A }` must still be the same two nodes. This is the case
/// that breaks if the map is keyed on anything looser than a subgraph id.
#[test]
fn a_forward_reference_to_a_plain_node_is_unaffected() {
    let source = format!("flowchart LR\n  A {ARROW} B\n  subgraph s1\n    A\n  end\n");
    assert_eq!(counts(&source), (2, 1, 1));
    assert_eq!(node_ids(&source), vec!["A".to_string(), "B".to_string()]);
}

/// CONTROL: a node sharing a name PREFIX with a later subgraph is still its own node.
#[test]
fn a_node_named_like_a_prefix_of_a_later_subgraph_is_still_a_node() {
    let source = format!("flowchart LR\n  s1x {ARROW} B\n  subgraph s1\n    A\n  end\n");
    assert!(
        node_ids(&source).iter().any(|id| id == "s1x"),
        "`s1x` was swallowed by the subgraph `s1`: {:?}",
        node_ids(&source)
    );
    assert_eq!(counts(&source), (3, 1, 1));
}

/// ⚠️ A SELF-REFERENCING SUBGRAPH IS PINNED, NOT FIXED.
///
/// `subgraph s1 { s1 --> Q }` maps `s1` to itself. The pinned bundle REFUSES this input outright
/// ("Setting s1 as parent of s1 would create a cycle") while we render it, so the resolver stops at
/// the raw id and preserves our existing, more permissive output rather than inventing a third
/// behaviour. What this test really guarantees is that a cycle terminates.
#[test]
fn a_self_referencing_subgraph_terminates_and_keeps_its_current_shape() {
    let source = format!("flowchart LR\n  subgraph s1\n    s1 {ARROW} Q\n  end\n");
    assert_eq!(
        counts(&source),
        (2, 1, 1),
        "the reference REJECTS this input; if our shape changed, update this note rather than \
         deleting it"
    );
}
