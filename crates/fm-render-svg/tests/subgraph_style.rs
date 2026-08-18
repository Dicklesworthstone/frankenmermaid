//! `style mySubgraph fill:#f00` must colour the subgraph (bd-xfmm).
//!
//! bd-xfmm found that a `style` target resolving to nothing was dropped in silence and taught the
//! parser to warn. It could not do more: a subgraph id is not a node, so `node_id_by_key` misses
//! it, and `IrStyleTarget` had no `Cluster` variant to hold it "even if it were found". The
//! directive was therefore reported and still ignored.
//!
//! The variant exists now, the parser resolves a subgraph target to it, and this is the half that
//! makes it visible. Without a consumer the variant would be one more parsed-stored-drawn-by-nothing
//! field — the class bd-jgco, bd-jerh and bd-bk7h all belong to.

const STYLED: &str =
    "flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n  style one fill:#ff0000\n";

/// The declared fill reaches the cluster.
#[test]
fn a_subgraph_style_reaches_the_svg() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(STYLED).ir);

    assert!(
        svg.contains("#ff0000"),
        "the subgraph's declared fill never reached the SVG:\n{svg}"
    );
}

/// NON-VACUITY: the parser must record it against a CLUSTER, not merely emit the colour somewhere.
///
/// `#ff0000` appearing in the document is not proof the SUBGRAPH got it — a future change that
/// applied the style to the contained node would satisfy the test above while leaving the subgraph
/// unstyled, which is the defect.
#[test]
fn the_style_is_recorded_against_a_cluster_not_a_node() {
    let ir = fm_parser::parse(STYLED).ir;

    assert!(
        ir.style_refs.iter().any(|style_ref| matches!(
            style_ref.target,
            fm_core::IrStyleTarget::Cluster(_)
        )),
        "the subgraph style was not recorded against a cluster: {:?}",
        ir.style_refs
    );
}

/// CONTROL: a subgraph with NO style declared must gain none. This is what stops the resolver
/// applying an empty or inherited style attribute to every cluster.
#[test]
fn an_unstyled_subgraph_gains_no_style_attribute() {
    let svg = fm_render_svg::render_svg(
        &fm_parser::parse("flowchart TD\n  subgraph one[One]\n    a[A]\n  end\n").ir,
    );

    // The cluster rect must still be drawn — otherwise "no style attribute" is vacuous.
    assert!(
        svg.contains("fm-cluster"),
        "no cluster was drawn, so this control proves nothing:\n{svg}"
    );
    assert!(
        !svg.contains("#ff0000"),
        "an unstyled subgraph gained a colour from nowhere:\n{svg}"
    );
}

/// CONTROL: an ordinary node style still works and is unaffected by the cluster path. A regression
/// that routed every `style` target to the cluster lookup would satisfy the tests above.
#[test]
fn a_node_style_still_applies() {
    let ir = fm_parser::parse("flowchart TD\n  a[A] --> b[B]\n  style a fill:#00ff00\n").ir;

    assert!(
        ir.style_refs.iter().any(|style_ref| matches!(
            style_ref.target,
            fm_core::IrStyleTarget::Node(_)
        )),
        "an ordinary node style stopped being recorded against a node: {:?}",
        ir.style_refs
    );
}
