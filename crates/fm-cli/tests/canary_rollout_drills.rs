//! Canary Rollout and Rollback Drills (bd-ml2r.12.4)
//!
//! End-to-end tests validating the canary rollout state machine and
//! deterministic rollback behavior.

use fm_core::canary::{
    HealthCriteria, RollbackReason, RolloutEvent, RolloutEventType, RolloutMetrics, RolloutPhase,
    RolloutState,
};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time since epoch")
        .as_millis() as u64
}

// ============================================================================
// Rollout Phase Progression Tests
// ============================================================================

#[test]
fn rollout_progression_disabled_to_canary() {
    let mut state = RolloutState::new();
    let timestamp = current_timestamp();

    assert_eq!(state.phase, RolloutPhase::Disabled);
    state.transition_to(RolloutPhase::Canary, timestamp);

    assert_eq!(state.phase, RolloutPhase::Canary);
    assert_eq!(state.previous_phase, Some(RolloutPhase::Disabled));
    assert_eq!(state.phase_started_at, timestamp);

    emit_drill_evidence("progression_disabled_to_canary", &state, None);
}

#[test]
fn rollout_progression_canary_to_partial() {
    let mut state = RolloutState::new();
    let timestamp = current_timestamp();

    state.transition_to(RolloutPhase::Canary, timestamp);

    // Simulate healthy canary period
    for _ in 0..200 {
        state.record_request(100, false);
    }

    assert!(state.check_health(&HealthCriteria::default()).is_none());

    state.transition_to(RolloutPhase::Partial, timestamp + 300_000);

    assert_eq!(state.phase, RolloutPhase::Partial);
    assert_eq!(state.previous_phase, Some(RolloutPhase::Canary));

    emit_drill_evidence("progression_canary_to_partial", &state, None);
}

#[test]
fn rollout_progression_partial_to_full() {
    let mut state = RolloutState::new();
    let timestamp = current_timestamp();

    state.transition_to(RolloutPhase::Partial, timestamp);

    // Simulate healthy partial period
    for _ in 0..1000 {
        state.record_request(100, false);
    }

    state.transition_to(RolloutPhase::Full, timestamp + 600_000);

    assert_eq!(state.phase, RolloutPhase::Full);
    assert_eq!(state.previous_phase, Some(RolloutPhase::Partial));

    emit_drill_evidence("progression_partial_to_full", &state, None);
}

// ============================================================================
// Rollback Drills
// ============================================================================

#[test]
fn rollback_drill_error_rate_exceeded() {
    let mut state = RolloutState::new();
    let timestamp = current_timestamp();

    state.transition_to(RolloutPhase::Canary, timestamp);

    // Simulate high error rate
    for i in 0..100 {
        state.record_request(100, i < 10); // 10% error rate
    }

    let criteria = HealthCriteria {
        max_error_rate: 0.05, // 5% threshold
        min_sample_size: 100,
        ..Default::default()
    };

    let reason = state.check_health(&criteria);
    assert!(matches!(reason, Some(RollbackReason::ErrorRateExceeded { .. })));

    if let Some(reason) = reason {
        state.rollback(reason.clone(), timestamp + 1000);

        assert_eq!(state.phase, RolloutPhase::RolledBack);
        emit_drill_evidence("rollback_error_rate_exceeded", &state, Some(&reason));
    }
}

#[test]
fn rollback_drill_latency_regression() {
    let mut state = RolloutState::new();
    let timestamp = current_timestamp();

    state.transition_to(RolloutPhase::Canary, timestamp);
    state.set_baseline(100); // 100us baseline

    // Simulate latency regression
    for _ in 0..100 {
        state.record_request(150, false); // 50% increase
    }

    let criteria = HealthCriteria {
        max_latency_increase_pct: 30.0, // 30% threshold
        min_sample_size: 100,
        ..Default::default()
    };

    let reason = state.check_health(&criteria);
    assert!(matches!(reason, Some(RollbackReason::LatencyRegressionExceeded { .. })));

    if let Some(reason) = reason {
        state.rollback(reason.clone(), timestamp + 1000);

        assert_eq!(state.phase, RolloutPhase::RolledBack);
        emit_drill_evidence("rollback_latency_regression", &state, Some(&reason));
    }
}

#[test]
fn rollback_drill_manual_trigger() {
    let mut state = RolloutState::new();
    let timestamp = current_timestamp();

    state.transition_to(RolloutPhase::Partial, timestamp);

    // Manual rollback
    let reason = RollbackReason::ManualTrigger {
        operator: "qa-engineer".to_string(),
    };

    state.rollback(reason.clone(), timestamp + 1000);

    assert_eq!(state.phase, RolloutPhase::RolledBack);
    assert!(!state.should_enable_fnx(42));

    emit_drill_evidence("rollback_manual_trigger", &state, Some(&reason));
}

#[test]
fn rollback_drill_quality_regression() {
    let mut state = RolloutState::new();
    let timestamp = current_timestamp();

    state.transition_to(RolloutPhase::Canary, timestamp);

    // Simulate quality regression detected by external monitoring
    let reason = RollbackReason::QualityRegression {
        metric: "edge_crossings".to_string(),
        delta: 15.0,
    };

    state.rollback(reason.clone(), timestamp + 1000);

    assert_eq!(state.phase, RolloutPhase::RolledBack);
    emit_drill_evidence("rollback_quality_regression", &state, Some(&reason));
}

#[test]
fn rollback_drill_determinism_violation() {
    let mut state = RolloutState::new();
    let timestamp = current_timestamp();

    state.transition_to(RolloutPhase::Full, timestamp);

    // Simulate determinism violation
    let reason = RollbackReason::DeterminismViolation {
        scenario_id: "flowchart_complex".to_string(),
    };

    state.rollback(reason.clone(), timestamp + 1000);

    assert_eq!(state.phase, RolloutPhase::RolledBack);
    emit_drill_evidence("rollback_determinism_violation", &state, Some(&reason));
}

// ============================================================================
// Recovery Drills
// ============================================================================

#[test]
fn recovery_drill_rolled_back_to_disabled() {
    let mut state = RolloutState::new();
    let timestamp = current_timestamp();

    state.transition_to(RolloutPhase::Canary, timestamp);
    state.rollback(
        RollbackReason::ErrorRateExceeded {
            observed: 5.0,
            threshold: 1.0,
        },
        timestamp + 1000,
    );

    assert_eq!(state.phase, RolloutPhase::RolledBack);

    // Recovery: transition back to disabled
    state.transition_to(RolloutPhase::Disabled, timestamp + 2000);

    assert_eq!(state.phase, RolloutPhase::Disabled);
    assert_eq!(state.previous_phase, Some(RolloutPhase::RolledBack));
    assert!(state.rollback_reason.is_none()); // Cleared by transition

    emit_drill_evidence("recovery_rolled_back_to_disabled", &state, None);
}

#[test]
fn recovery_drill_retry_canary_after_fix() {
    let mut state = RolloutState::new();
    let timestamp = current_timestamp();

    // Initial canary fails
    state.transition_to(RolloutPhase::Canary, timestamp);
    state.rollback(
        RollbackReason::LatencyRegressionExceeded {
            observed_pct: 50.0,
            threshold_pct: 25.0,
        },
        timestamp + 1000,
    );

    // After fix, retry canary
    state.transition_to(RolloutPhase::Canary, timestamp + 3600_000);

    assert_eq!(state.phase, RolloutPhase::Canary);
    assert_eq!(state.previous_phase, Some(RolloutPhase::RolledBack));
    assert_eq!(state.requests_processed, 0); // Metrics reset

    emit_drill_evidence("recovery_retry_canary_after_fix", &state, None);
}

// ============================================================================
// Determinism Tests
// ============================================================================

#[test]
fn determinism_rollout_state_serialization_stable() {
    let mut state = RolloutState::new();
    state.transition_to(RolloutPhase::Canary, 1000);

    for _ in 0..50 {
        state.record_request(100, false);
    }
    for _ in 0..5 {
        state.record_request(100, true);
    }

    // Serialize multiple times and verify stability
    let json1 = serde_json::to_string(&state).expect("serialize");
    let json2 = serde_json::to_string(&state).expect("serialize");
    let json3 = serde_json::to_string(&state).expect("serialize");

    assert_eq!(json1, json2);
    assert_eq!(json2, json3);

    // Deserialize and verify round-trip
    let restored: RolloutState = serde_json::from_str(&json1).expect("deserialize");
    assert_eq!(restored.phase, state.phase);
    assert_eq!(restored.requests_processed, state.requests_processed);
    assert_eq!(restored.error_count, state.error_count);
}

#[test]
fn determinism_traffic_sampling_is_deterministic() {
    let mut state = RolloutState::new();
    state.transition_to(RolloutPhase::Canary, 1000);

    // Same request IDs should always have same FNX decision
    for _ in 0..5 {
        let results: Vec<bool> = (0..1000).map(|i| state.should_enable_fnx(i)).collect();

        // Verify determinism by checking consistency
        for i in 0..1000 {
            assert_eq!(
                state.should_enable_fnx(i),
                results[i as usize],
                "request {i} should have consistent sampling"
            );
        }
    }
}

// ============================================================================
// Evidence Logging
// ============================================================================

fn emit_drill_evidence(drill_id: &str, state: &RolloutState, reason: Option<&RollbackReason>) {
    let event = RolloutEvent {
        timestamp: current_timestamp(),
        event_type: if reason.is_some() {
            RolloutEventType::Rollback
        } else {
            RolloutEventType::Transition
        },
        from_phase: state.previous_phase.unwrap_or(RolloutPhase::Disabled),
        to_phase: state.phase,
        rollback_reason: reason.cloned(),
        metrics: RolloutMetrics::from(state),
    };

    let evidence = json!({
        "drill_id": drill_id,
        "scenario_id": format!("canary_drill_{drill_id}"),
        "rollout_event": event,
        "state_snapshot": {
            "phase": state.phase.to_string(),
            "requests_processed": state.requests_processed,
            "error_rate": state.error_rate(),
            "avg_latency_us": state.avg_latency_us(),
            "latency_increase_pct": state.latency_increase_pct(),
        },
        "pass_fail_reason": "drill_completed",
    });

    // Emit to stdout as structured JSON (CI can capture)
    println!("{}", serde_json::to_string(&evidence).expect("serialize evidence"));
}
