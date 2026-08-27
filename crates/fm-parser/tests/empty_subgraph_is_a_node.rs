//! An EMPTY `subgraph` is a node, not a cluster (bd-kat55).
//!
//! THE DEFECT. `subgraph s1 … end` with nothing between the lines drew an empty cluster and no
//! node. Measured in Chromium 151 against the pinned mermaid 11.15.0 bundle, the reference renders
//! it as an ordinary rect vertex with id `s1` and NO cluster at all — byte-for-byte the same node
//! markup as a plain `s1[Title]` vertex, shape included.
//!
//! ⚠️ THE BEAD SAID THE REFERENCE "DROPS THE CLUSTER ENTIRELY". That is only half of it, and the
//! missing half is the half that matters: it also GAINS A NODE. A fix that merely dropped the
//! cluster would leave the diagram a box short of the reference, and every count-based assertion
//! about clusters would have passed. The bead title was corrected before the fix was written.
//!
//! ⚠️ AND THE INSTRUMENT THAT FOUND IT WAS WRONG FIRST. The probe read mermaid's own `g.node` /
//! `g.cluster` selectors against OUR markup, which uses `fm-node` / `fm-cluster`, and reported
//! `0/0` for every case — including the non-empty control, which is what gave it away. The control
//! row is the reason the corrected numbers below can be trusted at all.
//!
//! MEASURED, reference (node text) / cluster count:
//!
//! ```text
//!   subgraph s1 … end       + A --> B    nodes s1,A,B     0 clusters
//!   subgraph s1[Title] … end + A --> B   nodes Title,A,B  0 clusters   (id stays `s1`)
//!   subgraph s1 … end alone              node  s1         0 clusters
//!   outer { inner {} }      + A --> B    nodes inner,A,B  1 cluster `outer`
//!   subgraph s1 { A } … end + A --> B    nodes A,B        1 cluster `s1`   <- control
//! ```
//!
//! Bodies that intern nothing all behave identically — blank, comment-only, `direction TB`,
//! `class A red`, `style A fill:#f00` — so `flow_body_declares_content` asks "does lowering produce
//! a node", not "is this one of a list of blessed keywords".

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

/// Text as drawn: the label when there is one, else the id.
fn node_texts(source: &str) -> Vec<String> {
    let ir = fm_parser::parse(source).ir;
    ir.nodes
        .iter()
        .map(|n| {
            n.label
                .and_then(|id| ir.labels.get(id.0))
                .map_or_else(|| n.id.clone(), |l| l.text.clone())
        })
        .collect()
}

/// ⚠️ THE NEGATIVE CASE: the empty subgraph must become a NODE, not merely stop being a cluster.
///
/// Deleting the subgraph outright also yields "0 clusters", and would pass any assertion phrased
/// about clusters alone — while drawing one box fewer than the reference. The node is asserted by
/// id and the surviving nodes by name, so neither collapsing nor deleting can pass here.
#[test]
fn an_empty_subgraph_becomes_a_node_and_not_just_a_missing_cluster() {
    let source = format!("flowchart LR\n  subgraph s1\n  end\n  A {ARROW} B\n");
    assert_eq!(
        counts(&source),
        (3, 0, 1),
        "expected the reference's 3 nodes / 0 clusters / 1 edge, got {:?}",
        node_ids(&source)
    );
    let ids = node_ids(&source);
    assert!(
        ids.contains(&"s1".to_string()),
        "the empty subgraph left no node behind — the diagram is a box short: {ids:?}"
    );
    assert!(
        ids.contains(&"A".to_string()) && ids.contains(&"B".to_string()),
        "the empty subgraph took the rest of the diagram with it: {ids:?}"
    );
}

/// An empty subgraph on its own is a one-node diagram, not an empty one.
#[test]
fn an_empty_subgraph_alone_is_a_single_node() {
    let source = "flowchart LR\n  subgraph s1\n  end\n";
    assert_eq!(counts(source), (1, 0, 0));
    assert_eq!(node_ids(source), vec!["s1".to_string()]);
}

/// ⚠️ A TITLE BECOMES THE LABEL AND THE ID STAYS THE ID.
///
/// The reference draws `Title` and still answers to `s1`: an implementation that used the title as
/// the node's id would look right on screen and silently break every edge that names the subgraph.
/// Both halves are asserted, and the edge below is what makes the id observable.
#[test]
fn a_titled_empty_subgraph_keeps_its_id_and_draws_its_title() {
    let source = format!("flowchart LR\n  subgraph s1[Title]\n  end\n  s1 {ARROW} B\n");
    assert_eq!(counts(&source), (2, 0, 1));
    assert_eq!(
        node_texts(&source),
        vec!["Title".to_string(), "B".to_string()]
    );
    assert_eq!(node_ids(&source), vec!["s1".to_string(), "B".to_string()]);

    let ir = fm_parser::parse(&source).ir;
    let e = ir.edges.first().expect("the edge survived");
    let name = |end: &fm_core::IrEndpoint| match end {
        fm_core::IrEndpoint::Node(id) => ir.nodes[id.0].id.clone(),
        other => format!("{other:?}"),
    };
    assert_eq!(
        (name(&e.from).as_str(), name(&e.to).as_str()),
        ("s1", "B"),
        "an edge naming the subgraph did not reach the node it became"
    );
}

/// ⚠️ AN EDGE NAMING AN EMPTY SUBGRAPH REACHES THAT ONE NODE — it does not duplicate it.
///
/// bd-pfibz resolves an endpoint naming a subgraph to that subgraph's first member. An empty one
/// has no members, so the endpoint falls through to interning `s1` — which must be the SAME node
/// this fix already created, not a second one beside it. The count is what proves it.
#[test]
fn an_edge_to_an_empty_subgraph_does_not_duplicate_the_node() {
    let source = format!("flowchart LR\n  subgraph s1\n  end\n  s1 {ARROW} B\n");
    assert_eq!(
        counts(&source),
        (2, 0, 1),
        "the empty subgraph was interned twice: {:?}",
        node_ids(&source)
    );
}

/// ⚠️ A SUBGRAPH HOLDING ONLY AN EMPTY SUBGRAPH IS NOT ITSELF EMPTY.
///
/// `inner` becomes a node, which makes `outer` non-empty, so the reference keeps `outer`'s cluster
/// and puts the new node inside it. Recursing "empty means no members" without this would delete
/// both. Membership is asserted, not just the counts — a node drawn OUTSIDE the cluster it was
/// declared in is a different picture with identical totals.
#[test]
fn a_subgraph_holding_only_an_empty_subgraph_keeps_its_cluster() {
    let source = format!(
        "flowchart LR\n  subgraph outer\n    subgraph inner\n    end\n  end\n  A {ARROW} B\n"
    );
    assert_eq!(counts(&source), (3, 1, 1));
    let ir = fm_parser::parse(&source).ir;
    let inner = ir
        .nodes
        .iter()
        .position(|n| n.id == "inner")
        .expect("the empty inner subgraph became a node");
    let cluster = ir.clusters.first().expect("outer kept its cluster");
    assert!(
        cluster.members.iter().any(|m| m.0 == inner),
        "the node was drawn outside the subgraph it was declared in"
    );
}

/// Bodies that intern nothing are all empty, whatever they contain.
///
/// Each of these was measured to render as a node with no cluster. `class` and `style` are the
/// interesting ones: both name a node, and neither interns it.
#[test]
fn bodies_that_intern_nothing_are_empty() {
    for body in [
        "",
        "    %% just a comment\n",
        "    direction TB\n",
        "    class A red\n",
        "    style A fill:#f00\n",
        "\n",
    ] {
        let source = format!("flowchart LR\n  subgraph s1\n{body}  end\n  A {ARROW} B\n");
        assert_eq!(
            counts(&source),
            (3, 0, 1),
            "body {body:?} was not treated as empty: {:?}",
            node_ids(&source)
        );
    }
}

/// CONTROL: a subgraph with a node in it is still a cluster, and gains no extra node.
///
/// This is the case a too-eager predicate breaks, and the one the whole feature is bounded by.
#[test]
fn a_subgraph_with_content_is_still_a_cluster() {
    let source = format!("flowchart LR\n  subgraph s1\n    A\n  end\n  A {ARROW} B\n");
    assert_eq!(counts(&source), (2, 1, 1));
    let ids = node_ids(&source);
    assert!(
        !ids.contains(&"s1".to_string()),
        "a non-empty subgraph was also interned as a node: {ids:?}"
    );
}

/// CONTROL: bodies whose only statement is an edge, or a bare node, keep their cluster.
#[test]
fn a_subgraph_whose_only_statement_is_an_edge_is_still_a_cluster() {
    let source = format!("flowchart LR\n  subgraph s1\n    A {ARROW} B\n  end\n  B {ARROW} C\n");
    assert_eq!(counts(&source), (3, 1, 2));
    assert!(!node_ids(&source).contains(&"s1".to_string()));
}

/// An empty and a non-empty subgraph side by side: one node, one cluster, no crossover.
#[test]
fn an_empty_and_a_non_empty_subgraph_coexist() {
    let source =
        format!("flowchart LR\n  subgraph s1\n  end\n  subgraph s2\n    A\n  end\n  A {ARROW} B\n");
    assert_eq!(counts(&source), (3, 1, 1));
    let ids = node_ids(&source);
    assert!(ids.contains(&"s1".to_string()), "{ids:?}");
    assert!(!ids.contains(&"s2".to_string()), "{ids:?}");
}
