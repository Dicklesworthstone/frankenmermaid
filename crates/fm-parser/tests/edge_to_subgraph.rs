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
//!   edge BEFORE the subgraph   forward reference: the subgraph does not exist at intern time,
//!                              so the phantom is still created (reference: 2 nodes, ours 4)
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

/// ⚠️ RESIDUE 1, PINNED: a forward reference still creates the phantom.
///
/// `s1 --> s2` written BEFORE the subgraph blocks cannot resolve — the subgraph does not exist when
/// the endpoint is interned. Fixing it means a post-lowering pass that repoints edges and removes
/// the node, which is index surgery over `IrNodeId` and a separate piece of work.
///
/// Asserted at its current value so that fixing it fails HERE and forces this note to be updated.
#[test]
fn a_forward_reference_still_creates_the_phantom() {
    let source = format!(
        "flowchart LR\n  s1 {ARROW} s2\n  subgraph s1\n    A\n  end\n  subgraph s2\n    B\n  end\n"
    );
    assert_eq!(
        counts(&source),
        (4, 1, 2),
        "the forward-reference phantom is fixed — that is an improvement; update this test and the \
         notes in this file rather than deleting them"
    );
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
