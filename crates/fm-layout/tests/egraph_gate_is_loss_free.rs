//! The e-graph width gate must stay a SPEED gate, not a quality gate (bd-1buv.4).
//!
//! The crossing path runs barycenter, then hands each rank to an e-graph ordering pass — but only
//! when two guards allow it (`lib.rs:13939`):
//!
//!   * `current_order.len() < 2` — a rank with fewer than two nodes has exactly ONE possible order,
//!     so optimising it cannot change the output. Skipping it is loss-free by construction.
//!   * `should_use_egraph(len)` — a WIDTH ceiling, currently 100 nodes per layer.
//!
//! bd-1buv.4 proposes tightening that second gate on density and budget, and states the constraint
//! that makes this test worth having: "do not keep speedups that materially degrade visual quality
//! unless an explicit quality mode requested it".
//!
//! A width gate is exactly the knob where that goes wrong quietly. Lower it far enough and the
//! e-graph stops running on the diagrams it was written for; nothing fails, no error appears, and
//! the only symptom is more edge crossings in output nobody diffs numerically. This pins the gate
//! from BOTH sides so neither drift is silent.
//!
//! It deliberately does not assert the constant is 100. The threshold is a tuning decision and
//! should be free to move on evidence; what must not happen is it moving to a value that disables
//! the pass for ordinary diagrams, or removes the ceiling that keeps a pathological layer from
//! saturating an e-graph.

use fm_layout::egraph_ordering::{
    LayerEdges, LayerOrdering, crossing_count, optimize_layer_ordering,
    should_optimize_egraph_layer, should_use_egraph,
};

/// The gate must ADMIT the layer widths real diagrams produce.
///
/// This is the control that stops the gate becoming "never". A gate that refused everything would
/// make the crossing path faster on every benchmark and worse on every diagram, and no timing test
/// would object.
#[test]
fn ordinary_layer_widths_are_admitted() {
    for width in [2_usize, 3, 5, 8, 12, 20, 40] {
        assert!(
            should_use_egraph(width),
            "a layer of {width} nodes was refused; that is the size of an ordinary diagram's \
             widest rank, and refusing it silently trades crossings for speed"
        );
    }
}

/// The gate must REFUSE a pathological width.
///
/// The other direction: without a ceiling, one very wide layer hands the e-graph a search space it
/// cannot finish, and the wall-clock valve becomes the thing shaping the output.
#[test]
fn a_pathological_layer_width_is_refused() {
    for width in [1_000_usize, 10_000, 100_000] {
        assert!(
            !should_use_egraph(width),
            "a layer of {width} nodes was admitted; the pass would be shaped by its liveness valve \
             rather than by its counted budget"
        );
    }
}

/// The gate must be MONOTONE: once refused, never admitted again at a larger width.
///
/// A non-monotone gate is the shape a density heuristic can accidentally produce — admitting 50 and
/// 200 while refusing 100 — and it makes behaviour unpredictable per diagram in a way no single
/// threshold test would catch.
#[test]
fn the_gate_is_monotone_in_width() {
    let mut refused_at: Option<usize> = None;
    for width in 0_usize..2_000 {
        let admitted = should_use_egraph(width);
        match refused_at {
            None if !admitted => refused_at = Some(width),
            Some(first) => assert!(
                !admitted,
                "width {width} was admitted after {first} was refused; the gate is not monotone, \
                 so whether the e-graph runs depends on layer width in a way nobody can predict"
            ),
            None => {}
        }
    }

    assert!(
        refused_at.is_some(),
        "no width up to 2000 was refused, so there is no ceiling at all and the pathological case \
         is unbounded"
    );
}

#[test]
fn budgeted_complete_neighborhood_is_refused_without_losing_crossing_quality() {
    let width = 40_usize;
    let upper = LayerOrdering::new((0..width).collect());
    let current = LayerOrdering::new((width..width * 2).rev().collect());
    let lower = LayerOrdering::new((width * 2..width * 3).collect());
    let upper_edges = complete_edges(&upper, &current);
    let lower_edges = complete_edges(&current, &lower);

    assert!(
        !should_optimize_egraph_layer(
            &current,
            Some((&upper, &upper_edges)),
            Some((&lower, &lower_edges)),
        ),
        "a complete 40-node neighborhood is above the budget and has no ordering leverage"
    );

    let before = crossing_count(&upper, &current, &upper_edges)
        + crossing_count(&current, &lower, &lower_edges);
    let optimized = optimize_layer_ordering(
        &current,
        Some((&upper, &upper_edges)),
        Some((&lower, &lower_edges)),
    );
    assert_eq!(optimized.crossing_count, before);
    assert_eq!(optimized.ordering, current);
}

#[test]
fn budgeted_neighborhood_with_a_missing_edge_stays_admitted() {
    let width = 40_usize;
    let upper = LayerOrdering::new((0..width).collect());
    let current = LayerOrdering::new((width..width * 2).rev().collect());
    let mut upper_edges = complete_edges(&upper, &current);
    upper_edges.edges.pop();

    assert!(
        should_optimize_egraph_layer(&current, Some((&upper, &upper_edges)), None),
        "one missing edge restores ordering leverage, so the existing optimizer must run"
    );
}

fn complete_edges(upper: &LayerOrdering, lower: &LayerOrdering) -> LayerEdges {
    LayerEdges {
        edges: upper
            .order
            .iter()
            .flat_map(|&source| lower.order.iter().map(move |&target| (source, target)))
            .collect(),
    }
}
