//! An edge naming a subgraph attaches to it, it does not invent a node (bd-pfibz).
//!
//! THE DEFECT. `s1 --> s2`, where `s1` and `s2` are subgraphs, interned NODES called `s1` and `s2`
//! beside the clusters of the same name. The diagram gained two boxes nobody wrote and drew both
//! names twice. Measured against the pinned mermaid 11.15.0 bundle: the reference renders 2 node
//! groups and 2 clusters, we rendered 4 node groups.
//!
//! ⚠️ TWO EARLIER ATTEMPTS AT THIS FIX WERE REVERTED, AND WHY IS THE USEFUL PART. Both were placed
//! on plausible code paths and both looked up the WRONG KEY: a cluster is registered under
//! `flow_subgraph_lookup_key`, which is `"{id}@title:{title}"` whenever a title exists — and
//! `subgraph s1` defaults its title to its own id (bd-ka77), so the key is `s1@title:s1` and never
//! `s1`. The lookup returned `None` every time and the guards were inert. `IrSubgraph::key` holds
//! the PUBLIC id, which is the map that can answer the question the author asked.
//!
//! The lesson is the one the bead was re-filed with: instrument which lookup actually succeeds
//! before editing, rather than reasoning about which function looks right.
//!
//! ⚠️ AND IT RESOLVES TO A MEMBER, NOT TO THE SUBGRAPH. `IrEndpoint` has no cluster or subgraph
//! variant; adding one means exhaustive matches through fm-layout and all three renderers, the cost
//! the realization dash declined for the same reason. The edge is drawn between the right two
//! regions rather than between their boundaries — geometry that differs from the reference, where
//! inventing a box differed from the author.
//!
//! ⚠️ THREE MEASURED CASES ARE NOT FIXED AND ARE PINNED BELOW, each a different fault:
//!
//! ```text
//!   edge BEFORE the subgraph   FIXED SINCE, as bd-dw2a9 — and RE-BROKEN once since (b9be26aa
//!                              flattened the fixed-point walk inside a test-consolidation
//!                              commit that never mentioned behaviour; bd-sy2pl). Pinned at the
//!                              broken value, the pin fired, and the note it carried — "fixing it
//!                              needs IrNodeId compaction" — turned out to be wrong: the document
//!                              is fully parsed before it is lowered, so nothing ever has to be
//!                              removed. A chain needs the fixed-point WALK, not one hop.
//!   empty subgraph             FIXED SINCE, as bd-kat55 — and the note filed here was wrong in
//!                              the same way bd-honvo's was. The reference does not just drop the
//!                              cluster, it renders the empty subgraph AS A NODE. Recorded here as
//!                              a cluster-only difference, it would have been "fixed" a box short
//!   s1[Box] with subgraph s1   FIXED SINCE, as bd-honvo. It was pinned here at the broken value,
//!                              that pin failed when the follow-up landed, and the note had to be
//!                              updated rather than left stale. Note the bead was filed saying the
//!                              reference RE-LABELS the subgraph; measuring showed it DISCARDS the
//!                              label, and the bead's title was corrected before any code changed
//! ```

const ARROW: &str = "-->";

fn node_ids(source: &str) -> Vec<String> {
    fm_parser::parse(source)
        .ir
        .nodes
        .iter()
        .map(|n| n.id.clone())
        .collect()
}

fn counts(source: &str) -> (usize, usize, usize) {
    let ir = fm_parser::parse(source).ir;
    (ir.nodes.len(), ir.edges.len(), ir.clusters.len())
}

/// ⚠️ THE NEGATIVE CASE: no phantom node, and the edge still exists.
///
/// "There is no node called `s1`" passes if the edge was dropped along with it — which is the other
/// way to make a phantom disappear, and strictly worse. The edge count is asserted with it.
#[test]
fn an_edge_naming_a_subgraph_creates_no_phantom_node() {
    let source = format!(
        "flowchart LR\n  subgraph s1\n    A\n  end\n  subgraph s2\n    B\n  end\n  s1 {ARROW} s2\n"
    );
    let (nodes, edges, clusters) = counts(&source);
    assert_eq!(
        (nodes, edges, clusters),
        (2, 1, 2),
        "expected the reference's 2 nodes / 1 edge / 2 clusters, got {:?}",
        node_ids(&source)
    );
    let ids = node_ids(&source);
    assert!(
        !ids.iter().any(|id| id == "s1" || id == "s2"),
        "a subgraph name was interned as a node: {ids:?}"
    );
}

/// ⚠️ AND THE EDGE CONNECTS THE TWO SUBGRAPHS' MEMBERS, not something arbitrary.
///
/// Resolving to *a* node would satisfy the count assertions while wiring the edge to the wrong
/// element entirely. The endpoints must be the members of the subgraphs the author named.
#[test]
fn the_edge_connects_the_named_subgraphs_members() {
    let source = format!(
        "flowchart LR\n  subgraph s1\n    A\n  end\n  subgraph s2\n    B\n  end\n  s1 {ARROW} s2\n"
    );
    let ir = fm_parser::parse(&source).ir;
    let e = ir.edges.first().expect("the edge survived");
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

/// ⚠️ IT ATTACHES TO THE SUBGRAPH'S FIRST MEMBER, WHICH NEEDS A TWO-MEMBER FIXTURE TO SEE.
///
/// Every other case here uses a one-member subgraph, where "first member" and "last member" are the
/// same node — a negative-control arm that switched `first()` for `last()` passed the whole suite.
/// This is the fixture that tells them apart, and the choice is not arbitrary: the first member is
/// the one nearest the subgraph's declaration, so an edge into it enters the region where a reader
/// expects it to.
#[test]
fn the_edge_attaches_to_the_subgraphs_first_member() {
    let source = format!("flowchart LR\n  subgraph s1\n    A\n    B\n  end\n  s1 {ARROW} C\n");
    let ir = fm_parser::parse(&source).ir;
    let e = ir.edges.first().expect("the edge survived");
    let from = match &e.from {
        fm_core::IrEndpoint::Node(id) => ir.nodes[id.0].id.clone(),
        other => format!("{other:?}"),
    };
    assert_eq!(
        from, "A",
        "the edge attached to a member other than the subgraph's first"
    );
    assert_eq!(counts(&source), (3, 1, 1));
}

/// One side naming a subgraph and the other a plain node.
#[test]
fn one_subgraph_endpoint_and_one_node_endpoint() {
    let source = format!("flowchart LR\n  subgraph s1\n    A\n  end\n  s1 {ARROW} B\n");
    assert_eq!(counts(&source), (2, 1, 1));
    assert!(!node_ids(&source).iter().any(|id| id == "s1"));
}

/// A subgraph with an explicit title resolves by its ID, not its title.
///
/// ⚠️ THIS IS THE CASE THAT KILLED THE FIRST TWO ATTEMPTS. The cluster key is
/// `"{id}@title:{title}"`, so a lookup keyed on the cluster map finds nothing for `s1` — and a
/// titled subgraph makes that unmistakable, where an untitled one hides it behind a default title
/// that happens to equal the id.
#[test]
fn a_titled_subgraph_resolves_by_its_id() {
    let source = format!("flowchart LR\n  subgraph s1[The One]\n    A\n  end\n  s1 {ARROW} B\n");
    assert_eq!(counts(&source), (2, 1, 1));
    let ids = node_ids(&source);
    assert!(
        !ids.iter().any(|id| id == "s1"),
        "a titled subgraph's id was interned as a node: {ids:?}"
    );
}

/// CONTROL: an ordinary edge between ordinary nodes is untouched.
///
/// The guard runs on every flowchart edge endpoint, so the common case has to be shown unchanged.
#[test]
fn a_plain_edge_is_unaffected() {
    let source = format!("flowchart LR\n  A {ARROW} B\n");
    assert_eq!(counts(&source), (2, 1, 0));
    assert_eq!(node_ids(&source), vec!["A".to_string(), "B".to_string()]);
}

/// CONTROL: a node that merely shares a name PREFIX with a subgraph is still a node.
#[test]
fn a_node_named_like_a_prefix_of_a_subgraph_is_still_a_node() {
    let source = format!("flowchart LR\n  subgraph s1\n    A\n  end\n  s1x {ARROW} B\n");
    let ids = node_ids(&source);
    assert!(
        ids.iter().any(|id| id == "s1x"),
        "`s1x` was swallowed by the subgraph `s1`: {ids:?}"
    );
    assert_eq!(counts(&source), (3, 1, 1));
}

/// ⚠️ RESIDUE 1 IS NOW FIXED, TWICE — AND BOTH TIMES THE PIN IS WHAT MADE THE FIX HONEST.
///
/// `s1 --> s2` written BEFORE the subgraph blocks resolves, because the document is fully parsed
/// before it is lowered: the forward-reference map exists when the endpoints are interned (this is
/// what bd-dw2a9's note "fixing it needs IrNodeId compaction" got wrong — nothing ever has to be
/// removed). This test used to pin `(4, 1, 2)` at the phantom; the pin fired when bd-dw2a9 landed.
///
/// It fired a SECOND time, from the other side, when b9be26aa flattened the fixed-point walk to a
/// single `if` inside a test-consolidation commit and re-pinned the phantom with a note claiming
/// the case was still unfixed (bd-sy2pl). The simple two-subgraph form here does NOT discriminate:
/// one hop and a full walk agree on it. The two tests below it are the ones that do.
#[test]
fn a_forward_reference_resolves_without_a_phantom() {
    let source = format!(
        "flowchart LR\n  s1 {ARROW} s2\n  subgraph s1\n    A\n  end\n  subgraph s2\n    B\n  end\n"
    );
    assert_eq!(
        counts(&source),
        (2, 1, 2),
        "the forward-reference phantom is back: {:?}",
        node_ids(&source)
    );
    let ids = node_ids(&source);
    assert!(
        !ids.contains(&"s1".to_string()),
        "s1 interned as a node: {ids:?}"
    );
    assert!(
        !ids.contains(&"s2".to_string()),
        "s2 interned as a node: {ids:?}"
    );
}

/// One hop and a full walk agree when the chain has a single link: this test alone was false
/// comfort, which is how the flattening survived once. Measured: the reference renders 3 nodes
/// (`X`, `Y`, `Z`) and 2 clusters.
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
    let ids = node_ids(&depth4);
    for phantom in ["s1", "s2", "s3", "s4"] {
        assert!(
            !ids.iter().any(|id| id == phantom),
            "`{phantom}` was interned as a node: {ids:?}"
        );
    }
}

/// ⚠️ RESIDUE 2 IS NOW FIXED, AND THE PIN IS WHY THIS NOTE IS ACCURATE.
///
/// When bd-pfibz landed, `s1[Box]` reusing a subgraph's name still added a node: the guard required
/// an endpoint with no label of its own. That was filed as bd-honvo and pinned HERE at the broken
/// value, so the follow-up could not land quietly — and it failed with an instruction to update
/// rather than delete it.
///
/// bd-honvo then measured what the reference actually does (it DISCARDS the label; it does not
/// re-label the subgraph, which is what that bead was filed claiming) and guarded both the
/// edge-endpoint and the standalone-statement paths. Its subject now lives in
/// `labelled_subgraph_name.rs`; what remains here is the assertion that the two fixes agree.
#[test]
fn a_labelled_endpoint_reusing_the_name_resolves_to_the_subgraph() {
    let source = format!("flowchart LR\n  subgraph s1\n    A\n  end\n  s1[Box] {ARROW} B\n");
    assert_eq!(
        counts(&source),
        (2, 1, 1),
        "the labelled-endpoint phantom is back"
    );
    assert!(
        !node_ids(&source).iter().any(|id| id == "s1"),
        "the subgraph name was interned as a node again"
    );
}
