//! Incremental layout must produce the same layout a full recompute produces (bd-s6tkz).
//!
//! That is this repo's stated contract, in two places: the CLI integration test comments "Verify
//! incremental produces a visually identical (but possibly translated) layout", and a sibling test
//! asserts the incremental and full SVGs are BYTE-IDENTICAL. The engine broke it.
//!
//! WHAT WENT WRONG: once a guardrail forces a budget, the layout is ITERATION-LIMITED. Selective
//! relayout seeds it from the PREVIOUS positions while a full recompute seeds from scratch, and the
//! cap stops either from converging — so the two land in different places. Measured on a 72-node
//! dense flowchart after a single label edit: bounds `17741x276` against `18515x178`, a 774x98
//! difference with ALL 72 nodes moved.
//!
//! ⚠️ THE THRESHOLD IS WHY NOTHING CAUGHT IT. Below the guardrail the two agree exactly; the
//! existing tests use 56 and 64 nodes and never cross it. This file deliberately brackets the
//! threshold — 66 nodes (`within_budget`) and 72 (`guardrail_forced_multi_budget`) — so a fix that
//! only works on small graphs fails here.
//!
//! A COLD engine on the edited IR always matched the full recompute, which is what identified the
//! warm seed rather than the budget as the thing at fault.

use fm_layout::{
    IncrementalLayoutEngine, LayoutAlgorithm, LayoutConfig, LayoutGuardrails,
    layout_diagram_traced_with_config_and_guardrails,
};

/// The dense generator from the CLI integration suite, which is what pushes the cost estimate over
/// the guardrail budget. A sparse chain of the same size never trips it.
fn dense_flowchart(node_count: usize) -> String {
    let mut lines = vec!["flowchart LR".to_string()];
    for index in 0..node_count {
        lines.push(format!("    N{index}[Widget {index}]"));
    }
    for index in 0..node_count.saturating_sub(1) {
        lines.push(format!("    N{index} --> N{}", index + 1));
    }
    for index in 0..node_count.saturating_sub(3) {
        if index % 3 == 0 {
            lines.push(format!("    N{index} --> N{}", index + 3));
        }
    }
    for index in 0..node_count.saturating_sub(8) {
        if index % 5 == 0 {
            lines.push(format!("    N{index} --> N{}", index + 8));
        }
    }
    lines.join("\n")
}

/// Warm an engine on `node_count` nodes, edit one label, and return `(incremental, full)` bounds.
fn bounds_after_label_edit(node_count: usize) -> (fm_layout::LayoutRect, fm_layout::LayoutRect) {
    let ir = fm_parser::parse(&dense_flowchart(node_count)).ir;
    let config = LayoutConfig::default();
    let guardrails = LayoutGuardrails::default();

    let mut engine = IncrementalLayoutEngine::default();
    let _warm = engine.layout_diagram_traced_with_config_and_guardrails(
        &ir,
        LayoutAlgorithm::Auto,
        config.clone(),
        guardrails,
    );

    // A LABEL EDIT, not a structural one: it changes a node's measured width without touching the
    // graph, which is the cheapest edit that still forces a relayout — and the one the CLI stress
    // test performs on its even-numbered steps.
    let mut edited = ir.clone();
    let label_id = edited.nodes[0]
        .label
        .expect("the generator labels every node")
        .0;
    edited.labels[label_id].text = "Widget 0 rev 0".to_string();

    let incremental = engine.layout_diagram_traced_with_config_and_guardrails(
        &edited,
        LayoutAlgorithm::Auto,
        config.clone(),
        guardrails,
    );
    let full = layout_diagram_traced_with_config_and_guardrails(
        &edited,
        LayoutAlgorithm::Auto,
        config,
        guardrails,
    );
    (incremental.layout.bounds, full.layout.bounds)
}

/// ABOVE the guardrail threshold — the case that was broken.
#[test]
fn incremental_matches_full_recompute_when_a_guardrail_forces_a_budget() {
    let (incremental, full) = bounds_after_label_edit(72);
    assert!(
        (incremental.width - full.width).abs() < 1.0
            && (incremental.height - full.height).abs() < 1.0,
        "incremental {incremental:?} diverged from full {full:?} above the guardrail threshold"
    );
}

/// BELOW the threshold — the case that always worked, pinned so a fix cannot trade one for the other.
#[test]
fn incremental_matches_full_recompute_within_budget() {
    let (incremental, full) = bounds_after_label_edit(66);
    assert!(
        (incremental.width - full.width).abs() < 1.0
            && (incremental.height - full.height).abs() < 1.0,
        "incremental {incremental:?} diverged from full {full:?} below the guardrail threshold"
    );
}

/// NODE POSITIONS, not just the bounding box.
///
/// Two layouts can share a bounding box and still place every node differently. The original defect
/// moved all 72; a bounds-only check is necessary but not sufficient.
#[test]
fn every_node_lands_in_the_same_place_as_a_full_recompute() {
    let ir = fm_parser::parse(&dense_flowchart(72)).ir;
    let config = LayoutConfig::default();
    let guardrails = LayoutGuardrails::default();

    let mut engine = IncrementalLayoutEngine::default();
    let _warm = engine.layout_diagram_traced_with_config_and_guardrails(
        &ir,
        LayoutAlgorithm::Auto,
        config.clone(),
        guardrails,
    );
    let mut edited = ir.clone();
    let label_id = edited.nodes[0].label.expect("labelled").0;
    edited.labels[label_id].text = "Widget 0 rev 0".to_string();

    let incremental = engine.layout_diagram_traced_with_config_and_guardrails(
        &edited,
        LayoutAlgorithm::Auto,
        config.clone(),
        guardrails,
    );
    let full = layout_diagram_traced_with_config_and_guardrails(
        &edited,
        LayoutAlgorithm::Auto,
        config,
        guardrails,
    );

    assert_eq!(
        incremental.layout.nodes.len(),
        full.layout.nodes.len(),
        "CONTROL FAILED: the two layouts do not even contain the same node count"
    );
    let moved = incremental
        .layout
        .nodes
        .iter()
        .zip(full.layout.nodes.iter())
        .filter(|(a, b)| {
            (a.bounds.x - b.bounds.x).abs() > 0.01 || (a.bounds.y - b.bounds.y).abs() > 0.01
        })
        .count();
    assert_eq!(
        moved,
        0,
        "{moved} of {} nodes sit somewhere a full recompute would not put them",
        incremental.layout.nodes.len()
    );
}

/// CONTROL: a COLD engine agreed with the full recompute all along.
///
/// This is what proves the defect was the warm seed and not the budget itself — and it must keep
/// holding, or a "fix" that changed budgeted layout for everyone would pass the tests above while
/// silently moving every large diagram.
#[test]
fn a_cold_engine_has_always_matched_the_full_recompute() {
    let ir = fm_parser::parse(&dense_flowchart(72)).ir;
    let config = LayoutConfig::default();
    let guardrails = LayoutGuardrails::default();

    let mut cold = IncrementalLayoutEngine::default();
    let first = cold.layout_diagram_traced_with_config_and_guardrails(
        &ir,
        LayoutAlgorithm::Auto,
        config.clone(),
        guardrails,
    );
    let full = layout_diagram_traced_with_config_and_guardrails(
        &ir,
        LayoutAlgorithm::Auto,
        config,
        guardrails,
    );
    assert!(
        (first.layout.bounds.width - full.layout.bounds.width).abs() < 1.0
            && (first.layout.bounds.height - full.layout.bounds.height).abs() < 1.0,
        "a cold engine diverged from the full recompute: {:?} vs {:?}",
        first.layout.bounds,
        full.layout.bounds
    );
}
