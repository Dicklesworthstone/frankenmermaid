//! Public-entrypoint coverage for real directed backend selection, validation, and rollback.

use fm_core::canary::{RolloutPhase, RolloutState};
use fm_core::{
    DiagramType, IrEdge, IrEndpoint, IrNode, IrNodeId, IrPort, IrPortId, MermaidDiagramIr,
};
use fm_layout::{analyze_cycles_with_fnx_canary, fnx_directed::compute_scc};

const DIRECTED_BACKEND_AVAILABLE: bool = cfg!(all(
    feature = "fnx-experimental-directed",
    not(target_arch = "wasm32")
));

fn graph() -> MermaidDiagramIr {
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    ir.nodes = (0..12)
        .map(|index| IrNode {
            id: format!("node_{index}"),
            ..Default::default()
        })
        .collect();
    ir.edges = [
        (0, 1),
        (1, 0),
        (2, 2),
        (9, 10),
        (10, 11),
        (11, 9),
        (1, 9),
        (4, 5),
    ]
    .into_iter()
    .map(|(source, target)| IrEdge {
        from: IrEndpoint::Node(IrNodeId(source)),
        to: IrEndpoint::Node(IrNodeId(target)),
        ..Default::default()
    })
    .collect();
    ir.ports.push(IrPort {
        node: IrNodeId(10),
        name: "input".to_string(),
        ..Default::default()
    });
    ir.edges[3].to = IrEndpoint::Port(IrPortId(0));
    ir.edges.push(ir.edges[3].clone());
    ir
}

#[test]
fn selected_backend_preserves_directed_components_ports_loops_and_numeric_order() {
    let ir = graph();
    let expected = compute_scc(&ir);
    assert_eq!(expected.non_trivial_count, 3);
    assert_eq!(expected.max_component_size, 3);
    assert_eq!(expected.components.last().unwrap().nodes, [9, 10, 11]);

    let mut rollout = RolloutState::new();
    rollout.transition_to(RolloutPhase::Full, 1_000);
    for request_id in 1..=3 {
        let result = analyze_cycles_with_fnx_canary(&ir, &mut rollout, request_id);
        assert_eq!(result, expected);
        assert_eq!(
            rollout.requests_processed,
            if DIRECTED_BACKEND_AVAILABLE {
                request_id
            } else {
                0
            }
        );
        assert_eq!(rollout.error_count, 0);
    }
}

#[test]
fn selected_backend_preserves_the_empty_graph_partition() {
    let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    let mut rollout = RolloutState::new();
    rollout.transition_to(RolloutPhase::Full, 1_000);
    assert_eq!(
        analyze_cycles_with_fnx_canary(&ir, &mut rollout, 1),
        compute_scc(&ir)
    );
    assert_eq!(
        rollout.requests_processed,
        u64::from(DIRECTED_BACKEND_AVAILABLE)
    );
    assert_eq!(rollout.error_count, 0);
}

#[test]
fn disabled_and_rolled_back_requests_do_not_enter_or_measure_the_backend() {
    let mut ir = graph();
    // Malformed caller input must use native best-effort analysis without being counted as
    // backend traffic, including request IDs that would otherwise pass traffic sampling.
    ir.edges[0].to = IrEndpoint::Node(IrNodeId(usize::MAX));
    let expected = compute_scc(&ir);
    for phase in [RolloutPhase::Disabled, RolloutPhase::RolledBack] {
        let mut rollout = RolloutState::new();
        rollout.transition_to(phase, 7_000);
        for request_id in [0, 10, 100, u64::MAX] {
            assert_eq!(
                analyze_cycles_with_fnx_canary(&ir, &mut rollout, request_id),
                expected
            );
        }
        assert_eq!(rollout.requests_processed, 0);
        assert_eq!(rollout.error_count, 0);
        assert_eq!(rollout.latency_sum_us, 0);
        assert_eq!(rollout.phase, phase);
        assert_eq!(rollout.phase_started_at, 7_000);
    }
}

#[test]
fn canary_and_partial_sampling_count_only_selected_backend_attempts() {
    let ir = graph();
    let expected = compute_scc(&ir);
    for (phase, selected_request) in [(RolloutPhase::Canary, 100), (RolloutPhase::Partial, 10)] {
        let mut rollout = RolloutState::new();
        rollout.transition_to(phase, 1_000);
        assert_eq!(
            analyze_cycles_with_fnx_canary(&ir, &mut rollout, selected_request - 1),
            expected
        );
        assert_eq!(rollout.requests_processed, 0);
        assert_eq!(
            analyze_cycles_with_fnx_canary(&ir, &mut rollout, selected_request),
            expected
        );
        assert_eq!(
            rollout.requests_processed,
            u64::from(DIRECTED_BACKEND_AVAILABLE)
        );
        assert_eq!(rollout.error_count, 0);
    }
}

#[cfg(all(feature = "fnx-experimental-directed", not(target_arch = "wasm32")))]
#[test]
fn observed_backend_health_errors_roll_back_after_a_real_selected_request() {
    let ir = graph();
    let expected = compute_scc(&ir);
    let mut rollout = RolloutState::new();
    rollout.transition_to(RolloutPhase::Full, 1_000);
    // Restore a prior observation window just short of the minimum sample size. This call must
    // execute the real backend and evaluate the resulting window, not fabricate a backend error
    // from malformed caller data merely to exercise rollback.
    for _ in 0..99 {
        rollout.record_request(0, true);
    }
    let before_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    assert_eq!(
        analyze_cycles_with_fnx_canary(&ir, &mut rollout, 7),
        expected
    );
    assert_eq!(rollout.requests_processed, 100);
    assert_eq!(rollout.error_count, 99);
    let after_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    assert_eq!(rollout.phase, RolloutPhase::RolledBack);
    assert_eq!(rollout.previous_phase, Some(RolloutPhase::Full));
    assert!(matches!(
        rollout.rollback_reason,
        Some(fm_core::canary::RollbackReason::ErrorRateExceeded { observed, threshold })
            if observed == 99.0 && threshold == 1.0
    ));
    assert!((before_ms..=after_ms).contains(&u128::from(rollout.phase_started_at)));

    // Rollback is a circuit breaker: later selected request IDs never enter the failed arm.
    assert_eq!(
        analyze_cycles_with_fnx_canary(&ir, &mut rollout, 200),
        expected
    );
    assert_eq!(rollout.requests_processed, 100);
    assert_eq!(rollout.error_count, 99);
}

#[test]
fn malformed_input_falls_back_without_affecting_backend_health() {
    let mut rollout = RolloutState::new();
    rollout.transition_to(RolloutPhase::Full, 1_000);
    for endpoint in [
        IrEndpoint::Node(IrNodeId(usize::MAX)),
        IrEndpoint::Port(IrPortId(usize::MAX)),
        IrEndpoint::Unresolved,
    ] {
        let mut ir = graph();
        ir.edges[0].to = endpoint;
        assert_eq!(
            analyze_cycles_with_fnx_canary(&ir, &mut rollout, 100),
            compute_scc(&ir)
        );
    }
    assert_eq!(rollout.phase, RolloutPhase::Full);
    assert_eq!(rollout.requests_processed, 0);
    assert_eq!(rollout.error_count, 0);
    assert_eq!(rollout.latency_sum_us, 0);
}

#[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
#[test]
fn configured_centrality_uses_the_selected_projection_topology() {
    use fm_layout::fnx_adapter::ProjectionConfig;
    use fm_layout::fnx_ordering::compute_centrality_scores_with_config;

    let ir = graph();
    let kept = compute_centrality_scores_with_config(&ir, &ProjectionConfig::default());
    let removed = compute_centrality_scores_with_config(
        &ir,
        &ProjectionConfig {
            preserve_self_loops: false,
            ..ProjectionConfig::default()
        },
    );
    assert!(kept.computed && removed.computed);
    assert!(kept.get_degree(2).raw() > 0);
    assert_eq!(removed.get_degree(2).raw(), 0);
    assert_eq!(kept.get_degree(0), removed.get_degree(0));
}

#[cfg(not(all(feature = "fnx-experimental-directed", not(target_arch = "wasm32"))))]
#[test]
fn unavailable_backend_never_manufactures_success_or_error_health_samples() {
    let mut ir = graph();
    ir.edges[0].to = IrEndpoint::Unresolved;
    let mut rollout = RolloutState::new();
    rollout.transition_to(RolloutPhase::Full, 1_000);
    for request_id in 1..=110 {
        assert_eq!(
            analyze_cycles_with_fnx_canary(&ir, &mut rollout, request_id),
            compute_scc(&ir)
        );
    }
    assert_eq!(rollout.phase, RolloutPhase::Full);
    assert_eq!(rollout.requests_processed, 0);
    assert_eq!(rollout.error_count, 0);
    assert_eq!(rollout.latency_sum_us, 0);
}
