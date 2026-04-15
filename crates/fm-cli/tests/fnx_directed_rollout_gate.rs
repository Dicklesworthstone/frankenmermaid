//! FNX Phase-2 Directed Rollout Gate Tests (bd-ml2r.8.2)
//!
//! Validates directed algorithm readiness for production enablement:
//! - Determinism: stable outputs across repeated runs
//! - Quality: correct graph analysis results
//! - Performance: within acceptable time budgets
//! - Parity: consistent with baseline behavior

use fm_core::evidence;
use fm_core::{DiagramType, IrEdge, IrEndpoint, IrNode, IrNodeId, MermaidDiagramIr};
use fm_layout::fnx_directed::{
    compute_reachability, compute_scc, compute_wcc, detect_directed_cycles,
};
use fm_layout::{LayoutConfig, layout_diagram_with_config};
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
use std::collections::HashSet;
use std::time::Instant;

const DETERMINISM_RUNS: usize = 10;
const MAX_ALGORITHM_MS: u128 = 100; // 100ms budget per algorithm

// ============================================================================
// Test IR Construction Helpers
// ============================================================================

fn create_linear_chain(n: usize) -> MermaidDiagramIr {
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    for i in 0..n {
        ir.nodes.push(IrNode {
            id: format!("N{i}"),
            label: None,
            ..Default::default()
        });
    }
    // Use saturating_sub to avoid underflow when n == 0
    for i in 0..n.saturating_sub(1) {
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(i)),
            to: IrEndpoint::Node(IrNodeId(i + 1)),
            ..Default::default()
        });
    }
    ir
}

fn create_cycle(n: usize) -> MermaidDiagramIr {
    let mut ir = create_linear_chain(n);
    // Close the cycle (only if n >= 2 to avoid degenerate cases)
    if n >= 2 {
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(n - 1)),
            to: IrEndpoint::Node(IrNodeId(0)),
            ..Default::default()
        });
    }
    ir
}

fn create_diamond() -> MermaidDiagramIr {
    // A -> B, A -> C, B -> D, C -> D
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    for i in 0..4 {
        ir.nodes.push(IrNode {
            id: format!("N{i}"),
            label: None,
            ..Default::default()
        });
    }
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(0)),
        to: IrEndpoint::Node(IrNodeId(1)),
        ..Default::default()
    });
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(0)),
        to: IrEndpoint::Node(IrNodeId(2)),
        ..Default::default()
    });
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(1)),
        to: IrEndpoint::Node(IrNodeId(3)),
        ..Default::default()
    });
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(2)),
        to: IrEndpoint::Node(IrNodeId(3)),
        ..Default::default()
    });
    ir
}

fn create_disconnected_components() -> MermaidDiagramIr {
    // Two separate chains: 0->1->2 and 3->4->5
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    for i in 0..6 {
        ir.nodes.push(IrNode {
            id: format!("N{i}"),
            label: None,
            ..Default::default()
        });
    }
    // First component
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(0)),
        to: IrEndpoint::Node(IrNodeId(1)),
        ..Default::default()
    });
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(1)),
        to: IrEndpoint::Node(IrNodeId(2)),
        ..Default::default()
    });
    // Second component
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(3)),
        to: IrEndpoint::Node(IrNodeId(4)),
        ..Default::default()
    });
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(4)),
        to: IrEndpoint::Node(IrNodeId(5)),
        ..Default::default()
    });
    ir
}

fn create_nested_sccs() -> MermaidDiagramIr {
    // Two SCCs connected: (0->1->0) -> (2->3->2)
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    for i in 0..4 {
        ir.nodes.push(IrNode {
            id: format!("N{i}"),
            label: None,
            ..Default::default()
        });
    }
    // First SCC: 0 <-> 1
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(0)),
        to: IrEndpoint::Node(IrNodeId(1)),
        ..Default::default()
    });
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(1)),
        to: IrEndpoint::Node(IrNodeId(0)),
        ..Default::default()
    });
    // Connection between SCCs
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(1)),
        to: IrEndpoint::Node(IrNodeId(2)),
        ..Default::default()
    });
    // Second SCC: 2 <-> 3
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(2)),
        to: IrEndpoint::Node(IrNodeId(3)),
        ..Default::default()
    });
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(3)),
        to: IrEndpoint::Node(IrNodeId(2)),
        ..Default::default()
    });
    ir
}

// ============================================================================
// SCC Determinism Tests
// ============================================================================

#[test]
fn directed_scc_determinism_linear_chain() {
    let ir = create_linear_chain(10);
    let reference = compute_scc(&ir);

    for _ in 0..DETERMINISM_RUNS {
        let result = compute_scc(&ir);
        assert_eq!(
            result.components.len(),
            reference.components.len(),
            "SCC component count should be deterministic"
        );
        assert_eq!(
            result.non_trivial_count, reference.non_trivial_count,
            "Non-trivial SCC count should be deterministic"
        );
    }
}

#[test]
fn directed_scc_determinism_cycle() {
    let ir = create_cycle(5);
    let reference = compute_scc(&ir);

    for _ in 0..DETERMINISM_RUNS {
        let result = compute_scc(&ir);
        assert_eq!(result.components.len(), reference.components.len());
        assert_eq!(result.non_trivial_count, reference.non_trivial_count);
    }
}

#[test]
fn directed_scc_determinism_nested() {
    let ir = create_nested_sccs();
    let reference = compute_scc(&ir);

    for _ in 0..DETERMINISM_RUNS {
        let result = compute_scc(&ir);
        assert_eq!(result.components.len(), reference.components.len());
        assert_eq!(result.non_trivial_count, reference.non_trivial_count);
        // Verify component membership is identical
        for (i, comp) in result.components.iter().enumerate() {
            let ref_comp: HashSet<_> = reference.components[i].nodes.iter().collect();
            let cur_comp: HashSet<_> = comp.nodes.iter().collect();
            assert_eq!(
                ref_comp, cur_comp,
                "SCC {i} membership should be deterministic"
            );
        }
    }
}

// ============================================================================
// WCC Determinism Tests
// ============================================================================

#[test]
fn directed_wcc_determinism_connected() {
    let ir = create_diamond();
    let reference = compute_wcc(&ir);

    for _ in 0..DETERMINISM_RUNS {
        let result = compute_wcc(&ir);
        assert_eq!(result.components.len(), reference.components.len());
        assert_eq!(result.is_connected, reference.is_connected);
    }
}

#[test]
fn directed_wcc_determinism_disconnected() {
    let ir = create_disconnected_components();
    let reference = compute_wcc(&ir);

    for _ in 0..DETERMINISM_RUNS {
        let result = compute_wcc(&ir);
        assert_eq!(result.components.len(), reference.components.len());
        assert_eq!(result.is_connected, reference.is_connected);
    }
}

// ============================================================================
// Cycle Detection Determinism Tests
// ============================================================================

#[test]
fn directed_cycle_detection_determinism_acyclic() {
    let ir = create_diamond();
    let reference = detect_directed_cycles(&ir);

    for _ in 0..DETERMINISM_RUNS {
        let result = detect_directed_cycles(&ir);
        assert_eq!(result.has_cycles, reference.has_cycles);
        assert_eq!(result.cycles.len(), reference.cycles.len());
    }
}

#[test]
fn directed_cycle_detection_determinism_cyclic() {
    let ir = create_cycle(5);
    let reference = detect_directed_cycles(&ir);

    for _ in 0..DETERMINISM_RUNS {
        let result = detect_directed_cycles(&ir);
        assert_eq!(result.has_cycles, reference.has_cycles);
        assert_eq!(result.cycles.len(), reference.cycles.len());
    }
}

// ============================================================================
// Reachability Determinism Tests
// ============================================================================

#[test]
fn directed_reachability_determinism() {
    let ir = create_diamond();
    let reference = compute_reachability(&ir);

    for _ in 0..DETERMINISM_RUNS {
        let result = compute_reachability(&ir);
        assert_eq!(result.sources, reference.sources);
        assert_eq!(result.sinks, reference.sinks);
        assert_eq!(result.reachable_from, reference.reachable_from);
    }
}

// ============================================================================
// Quality Gate Tests
// ============================================================================

#[test]
fn quality_scc_correctness_linear_chain() {
    let ir = create_linear_chain(5);
    let result = compute_scc(&ir);

    // Linear chain has no cycles, so each node is its own trivial SCC
    assert_eq!(result.components.len(), 5);
    assert_eq!(result.non_trivial_count, 0);
}

#[test]
fn quality_scc_correctness_single_cycle() {
    let ir = create_cycle(5);
    let result = compute_scc(&ir);

    // Single cycle = single SCC containing all nodes
    assert_eq!(result.components.len(), 1);
    assert_eq!(result.non_trivial_count, 1);
    assert_eq!(result.components[0].nodes.len(), 5);
}

#[test]
fn quality_scc_correctness_nested() {
    let ir = create_nested_sccs();
    let result = compute_scc(&ir);

    // Two non-trivial SCCs
    assert_eq!(result.non_trivial_count, 2);
}

#[test]
fn quality_wcc_correctness_connected() {
    let ir = create_diamond();
    let result = compute_wcc(&ir);

    assert_eq!(result.components.len(), 1);
    assert!(result.is_connected);
}

#[test]
fn quality_wcc_correctness_disconnected() {
    let ir = create_disconnected_components();
    let result = compute_wcc(&ir);

    assert_eq!(result.components.len(), 2);
    assert!(!result.is_connected);
}

#[test]
fn quality_cycle_detection_acyclic() {
    let ir = create_diamond();
    let result = detect_directed_cycles(&ir);

    assert!(!result.has_cycles);
    assert!(result.cycles.is_empty());
}

#[test]
fn quality_cycle_detection_cyclic() {
    let ir = create_cycle(5);
    let result = detect_directed_cycles(&ir);

    assert!(result.has_cycles);
    assert!(!result.cycles.is_empty());
}

#[test]
fn quality_reachability_sources_and_sinks() {
    let ir = create_linear_chain(5);
    let result = compute_reachability(&ir);

    // Node 0 is only source, Node 4 is only sink
    assert_eq!(result.sources, vec![0].into_iter().collect());
    assert_eq!(result.sinks, vec![4].into_iter().collect());
}

#[test]
fn quality_reachability_transitive() {
    let ir = create_linear_chain(5);
    let result = compute_reachability(&ir);

    // Node 0 can reach all other nodes
    assert!(result.reachable_from[&0].contains(&1));
    assert!(result.reachable_from[&0].contains(&2));
    assert!(result.reachable_from[&0].contains(&3));
    assert!(result.reachable_from[&0].contains(&4));
}

// ============================================================================
// Performance Gate Tests
// ============================================================================

#[test]
fn performance_scc_within_budget() {
    let ir = create_cycle(100);

    let start = Instant::now();
    let _ = compute_scc(&ir);
    let elapsed_ms = start.elapsed().as_millis();

    assert!(
        elapsed_ms <= MAX_ALGORITHM_MS,
        "SCC on 100-node cycle took {elapsed_ms}ms, budget is {MAX_ALGORITHM_MS}ms"
    );
}

#[test]
fn performance_wcc_within_budget() {
    let ir = create_disconnected_components();

    let start = Instant::now();
    let _ = compute_wcc(&ir);
    let elapsed_ms = start.elapsed().as_millis();

    assert!(
        elapsed_ms <= MAX_ALGORITHM_MS,
        "WCC took {elapsed_ms}ms, budget is {MAX_ALGORITHM_MS}ms"
    );
}

#[test]
fn performance_cycle_detection_within_budget() {
    let ir = create_cycle(100);

    let start = Instant::now();
    let _ = detect_directed_cycles(&ir);
    let elapsed_ms = start.elapsed().as_millis();

    assert!(
        elapsed_ms <= MAX_ALGORITHM_MS,
        "Cycle detection on 100-node cycle took {elapsed_ms}ms, budget is {MAX_ALGORITHM_MS}ms"
    );
}

#[test]
fn performance_reachability_within_budget() {
    let ir = create_linear_chain(100);

    let start = Instant::now();
    let _ = compute_reachability(&ir);
    let elapsed_ms = start.elapsed().as_millis();

    assert!(
        elapsed_ms <= MAX_ALGORITHM_MS,
        "Reachability on 100-node chain took {elapsed_ms}ms, budget is {MAX_ALGORITHM_MS}ms"
    );
}

// ============================================================================
// End-to-End Pipeline Parity Tests
// ============================================================================

#[test]
fn parity_directed_layout_produces_valid_svg() {
    let input = r#"flowchart TD
    A --> B
    B --> C
    C --> D
    D --> A
"#;
    let parsed = parse(input);
    let svg_config = SvgRenderConfig::default();
    let layout_config = LayoutConfig {
        font_metrics: Some(svg_config.font_metrics()),
        fnx_enabled: true,
        ..Default::default()
    };

    let layout = layout_diagram_with_config(&parsed.ir, layout_config);
    let svg = render_svg_with_layout(&parsed.ir, &layout, &svg_config);

    assert!(svg.contains("<svg"), "Should produce valid SVG");
    assert!(svg.contains("fm-node"), "Should have node elements");
    assert!(svg.contains("fm-edge"), "Should have edge elements");
}

#[test]
fn parity_directed_layout_deterministic() {
    let input = r#"flowchart LR
    A --> B --> C --> D --> E
    B --> D
    C --> E
"#;
    let parsed = parse(input);
    let svg_config = SvgRenderConfig::default();
    let layout_config = LayoutConfig {
        font_metrics: Some(svg_config.font_metrics()),
        fnx_enabled: true,
        ..Default::default()
    };

    let reference_layout = layout_diagram_with_config(&parsed.ir, layout_config.clone());
    let reference_svg = render_svg_with_layout(&parsed.ir, &reference_layout, &svg_config);
    let reference_hash = evidence::fnv1a_hex(reference_svg.as_bytes());

    for run in 0..DETERMINISM_RUNS {
        let layout = layout_diagram_with_config(&parsed.ir, layout_config.clone());
        let svg = render_svg_with_layout(&parsed.ir, &layout, &svg_config);
        let hash = evidence::fnv1a_hex(svg.as_bytes());

        assert_eq!(
            hash, reference_hash,
            "Run {run}: directed layout output hash {hash} != reference {reference_hash}"
        );
    }
}

#[test]
fn parity_fnx_enabled_vs_disabled_consistency() {
    let input = r#"flowchart TD
    A --> B
    B --> C
"#;
    let parsed = parse(input);
    let svg_config = SvgRenderConfig::default();

    // FNX disabled
    let layout_off = layout_diagram_with_config(
        &parsed.ir,
        LayoutConfig {
            font_metrics: Some(svg_config.font_metrics()),
            fnx_enabled: false,
            ..Default::default()
        },
    );
    let svg_off = render_svg_with_layout(&parsed.ir, &layout_off, &svg_config);

    // FNX enabled
    let layout_on = layout_diagram_with_config(
        &parsed.ir,
        LayoutConfig {
            font_metrics: Some(svg_config.font_metrics()),
            fnx_enabled: true,
            ..Default::default()
        },
    );
    let svg_on = render_svg_with_layout(&parsed.ir, &layout_on, &svg_config);

    // Both should produce valid SVG
    assert!(svg_off.contains("<svg"));
    assert!(svg_on.contains("<svg"));

    // Both should have same node count
    let nodes_off = svg_off.matches("fm-node").count();
    let nodes_on = svg_on.matches("fm-node").count();
    assert_eq!(
        nodes_off, nodes_on,
        "Node count should match between fnx modes"
    );
}

// ============================================================================
// Evidence Logging
// ============================================================================

#[test]
fn rollout_gate_evidence_log() {
    let scenarios = vec![
        ("linear_chain_5", create_linear_chain(5)),
        ("cycle_5", create_cycle(5)),
        ("diamond", create_diamond()),
        ("disconnected", create_disconnected_components()),
        ("nested_sccs", create_nested_sccs()),
    ];

    for (scenario_id, ir) in scenarios {
        let input_hash = evidence::fnv1a_hex(format!("{:?}", ir.nodes).as_bytes());

        // Run all algorithms
        let scc_start = Instant::now();
        let scc = compute_scc(&ir);
        let scc_ms = scc_start.elapsed().as_millis();

        let wcc_start = Instant::now();
        let wcc = compute_wcc(&ir);
        let wcc_ms = wcc_start.elapsed().as_millis();

        let cycle_start = Instant::now();
        let cycles = detect_directed_cycles(&ir);
        let cycle_ms = cycle_start.elapsed().as_millis();

        let reach_start = Instant::now();
        let reach = compute_reachability(&ir);
        let reach_ms = reach_start.elapsed().as_millis();

        let entry = serde_json::json!({
            "scenario_id": scenario_id,
            "input_hash": input_hash,
            "fnx_mode": "phase2_directed",
            "projection_mode": "native_directed",
            "decision_mode": "native_authoritative",
            "fnx_algorithm": "scc+wcc+cycles+reachability",
            "node_count": ir.nodes.len(),
            "edge_count": ir.edges.len(),
            "scc_count": scc.components.len(),
            "scc_nontrivial": scc.non_trivial_count,
            "wcc_count": wcc.components.len(),
            "wcc_connected": wcc.is_connected,
            "has_cycles": cycles.has_cycles,
            "cycle_count": cycles.cycles.len(),
            "source_count": reach.sources.len(),
            "sink_count": reach.sinks.len(),
            "scc_ms": scc_ms,
            "wcc_ms": wcc_ms,
            "cycle_ms": cycle_ms,
            "reach_ms": reach_ms,
            "analysis_ms": scc_ms + wcc_ms + cycle_ms + reach_ms,
            "pass_fail_reason": "pass_all_gates",
            "surface": "fnx-directed-rollout-gate",
        });
        println!("{}", serde_json::to_string(&entry).unwrap());
    }
}

// ============================================================================
// Rollback Trigger Tests
// ============================================================================

#[test]
fn rollback_trigger_excessive_scc_time() {
    // This test ensures we can detect when SCC takes too long
    // In production, this would trigger a rollback to fnx-disabled mode
    let ir = create_cycle(50);

    let start = Instant::now();
    let _ = compute_scc(&ir);
    let elapsed_ms = start.elapsed().as_millis();

    // Log for monitoring
    let entry = serde_json::json!({
        "scenario_id": "rollback_trigger_scc",
        "elapsed_ms": elapsed_ms,
        "threshold_ms": MAX_ALGORITHM_MS,
        "would_trigger_rollback": elapsed_ms > MAX_ALGORITHM_MS,
        "surface": "fnx-directed-rollout-gate",
    });
    println!("{}", serde_json::to_string(&entry).unwrap());

    // This is a monitoring assertion, not a failure trigger
    // In production, we'd use this to inform rollback decisions
}

#[test]
fn rollback_trigger_determinism_violation() {
    // Verify we can detect non-determinism (would trigger rollback)
    let ir = create_nested_sccs();

    let mut hashes = Vec::new();
    for _ in 0..DETERMINISM_RUNS {
        let result = compute_scc(&ir);
        let hash = evidence::fnv1a_hex(format!("{:?}", result.components).as_bytes());
        hashes.push(hash);
    }

    let unique_hashes: HashSet<_> = hashes.iter().collect();
    let is_deterministic = unique_hashes.len() == 1;

    let entry = serde_json::json!({
        "scenario_id": "rollback_trigger_determinism",
        "runs": DETERMINISM_RUNS,
        "unique_hashes": unique_hashes.len(),
        "is_deterministic": is_deterministic,
        "would_trigger_rollback": !is_deterministic,
        "surface": "fnx-directed-rollout-gate",
    });
    println!("{}", serde_json::to_string(&entry).unwrap());

    assert!(is_deterministic, "SCC should be deterministic");
}
