//! `@{ shape: fork }` and `@{ shape: join }` name a shape this renderer already draws (bd-3ra5y's
//! family, extended).
//!
//! REFERENCE BEHAVIOR, measured in Chromium 151 against the pinned mermaid 11.15.0 bundle. Both
//! names produce the IDENTICAL outline — a 70x10 bar:
//!
//! ```text
//!   fork   path d="M-35 -5 L35 -5 L35 5 L-35 5"
//!   join   path d="M-35 -5 L35 -5 L35 5 L-35 5"
//! ```
//!
//! ⚠️ MERMAID EMITS A SECOND, ENORMOUS PATH FOR EACH AND IT IS NOT GEOMETRY. These shapes are drawn
//! through rough.js, whose sketch overlay re-traces the same rectangle as dozens of jittered cubic
//! segments — `doc` comes out as a 5 KB `d`. Reading that as the shape, or trying to match it byte
//! for byte, would be a category error: the FIRST path is the true outline and the rest is a
//! hand-drawn effect this renderer does not reproduce at all.
//!
//! THE DEFECT: `NodeShape::HorizontalBar` is a 72x8 rounded bar — the same silhouette — and this
//! renderer has drawn it all along, but only for `state f <<fork>>` and the sequence `queue`
//! participant. It had no flowchart spelling, and both names sat in `UNIMPLEMENTED_UPSTREAM_SHAPES`,
//! so an author writing a correct mermaid name was told the shape was unsupported AND handed a plain
//! rectangle. Exactly what bd-3ra5y found for `lean-l` and `trap-t`.

use fm_core::NodeShape;

fn shape_of(source: &str) -> NodeShape {
    fm_parser::parse(source).ir.nodes[0].shape
}

fn warnings(source: &str) -> Vec<String> {
    fm_parser::parse(source).warnings
}

#[test]
fn fork_and_join_name_the_bar_this_renderer_draws() {
    for name in ["fork", "join"] {
        assert_eq!(
            shape_of(&format!(
                "flowchart TD\n  A@{{ shape: {name}, label: \"X\" }}\n"
            )),
            NodeShape::HorizontalBar,
            "`shape: {name}` did not resolve to the bar"
        );
    }
}

/// ⚠️ THE DISCRIMINATING CONTROL, and the one a wrong implementation fails.
///
/// The failure mode here is not "no shape" — it is a SILENT FALLBACK to `NodeShape::Rect`, which
/// renders a perfectly ordinary box. Asserting only that parsing succeeds, or that some shape came
/// back, passes on exactly that. The bar must differ from the rectangle.
///
/// This is the bd-vfxu shape: two declared shapes that render identically, where every "does it
/// parse?" assertion is satisfied while the picture is wrong.
#[test]
fn fork_is_not_the_default_rectangle() {
    let fork = shape_of("flowchart TD\n  A@{ shape: fork, label: \"X\" }\n");
    let rect = shape_of("flowchart TD\n  A@{ shape: rect, label: \"X\" }\n");
    assert_eq!(rect, NodeShape::Rect, "the control itself moved");
    assert_ne!(
        fork, rect,
        "`shape: fork` fell back to the default rectangle, which is what the bug looked like"
    );
}

/// ⚠️ AND THE WARNING MUST STOP FOR THESE TWO ONLY.
///
/// Removing a name from `UNIMPLEMENTED_UPSTREAM_SHAPES` is a one-line edit that could just as easily
/// have emptied the list or broken the lookup. A genuinely unimplemented name must STILL warn, or
/// this change traded a wrong shape for a silent one — which is worse, and is the property bd-xfmm
/// spent a whole bead establishing.
#[test]
fn a_still_unimplemented_shape_still_warns() {
    for name in ["fork", "join"] {
        assert!(
            warnings(&format!("flowchart TD\n  A@{{ shape: {name} }}\n")).is_empty(),
            "`shape: {name}` is implemented now and must not warn"
        );
    }
    // ⚠️ ANCHORED ON THE TWO CONFIRMED NON-SHAPES, deliberately. `win-pane` and `datastore` are
    // names mermaid 11.15.0 publishes and draws as a PLAIN RECTANGLE — measured, recorded on
    // bd-7ls21, and therefore never going to be implemented here. Every other name in this list has
    // had to be swapped out the moment someone implemented it, three lists at a time; these two
    // cannot be. The third entry rotates and is expected to churn.
    for name in ["win-pane", "datastore", "hourglass"] {
        assert!(
            !warnings(&format!("flowchart TD\n  A@{{ shape: {name} }}\n")).is_empty(),
            "`shape: {name}` is still unimplemented and must still warn"
        );
    }
}

/// CONTROL: an unknown name is still reported, so the allowlist edit did not turn the whole
/// diagnostic off.
#[test]
fn an_unknown_shape_name_still_warns() {
    assert!(
        !warnings("flowchart TD\n  A@{ shape: definitely-not-a-shape }\n").is_empty(),
        "a nonsense shape name produced no diagnostic at all"
    );
}

/// CONTROL: the state-diagram spelling that always worked still works. `fork` reaching the flowchart
/// metadata table must not disturb `state f <<fork>>`, which resolves through a different path
/// (`StatePseudoState`) to the same shape.
#[test]
fn the_state_diagram_fork_is_unchanged() {
    let ir = fm_parser::parse("stateDiagram-v2\n  state f <<fork>>\n  A --> f\n").ir;
    assert!(
        ir.nodes
            .iter()
            .any(|node| node.shape == NodeShape::HorizontalBar),
        "the state-diagram fork lost its bar: {:?}",
        ir.nodes.iter().map(|n| n.shape).collect::<Vec<_>>()
    );
}
