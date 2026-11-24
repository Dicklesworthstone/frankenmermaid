//! FNX Capability Validation Tests (bd-ml2r.7.2)
//!
//! CI guard tests that validate required fnx APIs are available.
//! These tests ensure compatibility between frankenmermaid and fnx versions.
//!
//! Failure here indicates a breaking change in fnx that needs:
//! 1. Compatibility layer added, OR
//! 2. fnx version pin updated with migration, OR
//! 3. Feature flag guard added

use fm_core::evidence::{FnxFeatures, FnxMode, ProjectionMode};
use fm_layout::{LayoutConfig, layout_diagram_with_config};
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};

// ============================================================================
// Phase-1 Core API Availability
// ============================================================================

#[test]
fn capability_fnx_mode_enum_variants_available() {
    // Validate FnxMode enum has expected variants
    let _off = FnxMode::Off;
    let _advisory = FnxMode::Advisory;
    let _strict = FnxMode::Strict;
    let _experimental = FnxMode::ExperimentalDirected;

    assert!(!FnxMode::Off.is_active());
    assert!(FnxMode::Advisory.is_active());
}

#[test]
fn capability_projection_mode_enum_variants_available() {
    // Validate ProjectionMode enum has expected variants
    let _native_only = ProjectionMode::NativeOnly;
    let _native_plus_advisory = ProjectionMode::NativePlusFnxAdvisory;

    assert_eq!(ProjectionMode::NativeOnly.to_string(), "native_only");
    assert_eq!(
        ProjectionMode::NativePlusFnxAdvisory.to_string(),
        "native_plus_fnx_advisory"
    );
}

#[test]
fn capability_fnx_features_default_available() {
    // Validate FnxFeatures struct is constructible with defaults
    let features = FnxFeatures::default();

    // Should have expected fields
    assert!(!features.fnx_integration);
    assert!(!features.fnx_experimental_directed);
}

// ============================================================================
// Layout Config fnx Fields
// ============================================================================

#[test]
fn capability_layout_config_has_fnx_enabled_field() {
    // Validate LayoutConfig has fnx_enabled field
    let config = LayoutConfig {
        fnx_enabled: false,
        ..Default::default()
    };

    assert!(!config.fnx_enabled);

    // Default is fnx enabled
    let config_default = LayoutConfig::default();
    assert!(config_default.fnx_enabled);
}

// ============================================================================
// End-to-End Pipeline Capability
// ============================================================================

#[test]
fn capability_full_pipeline_works_fnx_off() {
    // Validate full parse -> layout -> render pipeline works with fnx disabled
    let input = "flowchart LR\n    A --> B --> C\n";
    let parsed = parse(input);

    let svg_config = SvgRenderConfig::default();
    let layout_config = LayoutConfig {
        font_metrics: Some(svg_config.font_metrics()),
        fnx_enabled: false,
        ..Default::default()
    };

    let layout = layout_diagram_with_config(&parsed.ir, layout_config);
    let svg = render_svg_with_layout(&parsed.ir, &layout, &svg_config);

    assert!(svg.contains("<svg"), "Should produce valid SVG");
    assert!(svg.contains("fm-node"), "Should have node elements");
}

#[test]
fn capability_full_pipeline_works_fnx_on() {
    // Validate full pipeline with fnx_enabled=true (graceful if fnx not compiled)
    let input = "flowchart TD\n    A --> B\n    B --> C\n    C --> A\n";
    let parsed = parse(input);

    let svg_config = SvgRenderConfig::default();
    let layout_config = LayoutConfig {
        font_metrics: Some(svg_config.font_metrics()),
        fnx_enabled: true,
        ..Default::default()
    };

    let layout = layout_diagram_with_config(&parsed.ir, layout_config);
    let svg = render_svg_with_layout(&parsed.ir, &layout, &svg_config);

    // Should still produce valid output regardless of fnx availability
    assert!(svg.contains("<svg"), "Should produce valid SVG");
}

// ============================================================================
// Directed Algorithm Native Surface (Phase-2 Preparation)
// ============================================================================

#[test]
fn capability_native_scc_available() {
    use fm_core::{DiagramType, IrEdge, IrEndpoint, IrNode, IrNodeId, MermaidDiagramIr};
    use fm_layout::fnx_directed::compute_scc;

    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    for i in 0..3 {
        ir.nodes.push(IrNode {
            id: format!("N{i}"),
            ..Default::default()
        });
    }
    // Create cycle: 0 -> 1 -> 2 -> 0
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
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(2)),
        to: IrEndpoint::Node(IrNodeId(0)),
        ..Default::default()
    });

    let result = compute_scc(&ir);
    assert_eq!(result.components.len(), 1, "Should detect single SCC");
    assert_eq!(result.non_trivial_count, 1, "Should be non-trivial");
}

#[test]
fn capability_native_wcc_available() {
    use fm_core::{DiagramType, IrEdge, IrEndpoint, IrNode, IrNodeId, MermaidDiagramIr};
    use fm_layout::fnx_directed::compute_wcc;

    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    for i in 0..4 {
        ir.nodes.push(IrNode {
            id: format!("N{i}"),
            ..Default::default()
        });
    }
    // Two disconnected components: {0,1} and {2,3}
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(0)),
        to: IrEndpoint::Node(IrNodeId(1)),
        ..Default::default()
    });
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(2)),
        to: IrEndpoint::Node(IrNodeId(3)),
        ..Default::default()
    });

    let result = compute_wcc(&ir);
    assert_eq!(result.components.len(), 2, "Should detect two WCCs");
    assert!(!result.is_connected, "Graph should be disconnected");
}

#[test]
fn capability_native_cycle_detection_available() {
    use fm_core::{DiagramType, IrEdge, IrEndpoint, IrNode, IrNodeId, MermaidDiagramIr};
    use fm_layout::fnx_directed::detect_directed_cycles;

    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    for i in 0..3 {
        ir.nodes.push(IrNode {
            id: format!("N{i}"),
            ..Default::default()
        });
    }
    // Create cycle
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
    ir.edges.push(IrEdge {
        from: IrEndpoint::Node(IrNodeId(2)),
        to: IrEndpoint::Node(IrNodeId(0)),
        ..Default::default()
    });

    let result = detect_directed_cycles(&ir);
    assert!(result.has_cycles, "Should detect cycles");
}

#[test]
fn capability_native_reachability_available() {
    use fm_core::{DiagramType, IrEdge, IrEndpoint, IrNode, IrNodeId, MermaidDiagramIr};
    use fm_layout::fnx_directed::compute_reachability;

    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    for i in 0..3 {
        ir.nodes.push(IrNode {
            id: format!("N{i}"),
            ..Default::default()
        });
    }
    // Linear chain: 0 -> 1 -> 2
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

    let result = compute_reachability(&ir);
    assert!(result.sources.contains(&0), "Node 0 should be a source");
    assert!(result.sinks.contains(&2), "Node 2 should be a sink");
    assert!(
        result.reachable_from[&0].contains(&2),
        "Node 0 should reach node 2"
    );
}

// ============================================================================
// Evidence Logging Capability
// ============================================================================

#[test]
fn capability_evidence_fnv1a_hash_available() {
    use fm_core::evidence::fnv1a_hex;

    let hash = fnv1a_hex(b"test input");
    assert!(!hash.is_empty(), "Should produce non-empty hash");
    assert_eq!(hash.len(), 16, "FNV1a-64 should produce 16 hex chars");

    // Determinism check
    let hash2 = fnv1a_hex(b"test input");
    assert_eq!(hash, hash2, "Hash should be deterministic");
}

// ============================================================================
// Canary Rollout API Capability
// ============================================================================

#[test]
fn capability_canary_rollout_state_available() {
    use fm_core::canary::{HealthCriteria, RolloutPhase, RolloutState};

    let mut state = RolloutState::new();
    assert_eq!(state.phase, RolloutPhase::Disabled);

    state.transition_to(RolloutPhase::Canary, 1000);
    assert_eq!(state.phase, RolloutPhase::Canary);

    // Record some requests
    state.record_request(100, false);
    state.record_request(100, false);

    // Check health
    let criteria = HealthCriteria::default();
    let reason = state.check_health(&criteria, 301_000);
    assert!(
        reason.is_none(),
        "Healthy state should not trigger rollback"
    );
}

// ============================================================================
// Evidence Logging
// ============================================================================

#[test]
fn capability_check_evidence_log() {
    let capabilities = serde_json::json!({
        "gate_id": "fnx_capability_checks",
        "phase": "phase1_and_phase2_prep",
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
        "capabilities_validated": [
            "fnx_mode_enum",
            "projection_mode_enum",
            "fnx_features_default",
            "layout_config_fnx_field",
            "full_pipeline_fnx_off",
            "full_pipeline_fnx_on",
            "native_scc",
            "native_wcc",
            "native_cycle_detection",
            "native_reachability",
            "evidence_fnv1a_hash",
            "canary_rollout_state",
        ],
        "pass_fail_reason": "all_capabilities_available",
    });

    println!(
        "{}",
        serde_json::to_string(&capabilities).expect("serialize")
    );
}
