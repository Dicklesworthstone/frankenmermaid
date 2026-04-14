//! Canary rollout and deterministic rollback for FNX feature enablement.
//!
//! This module defines:
//! - Phased rollout states with health criteria
//! - Automatic rollback triggers from compatibility/regression signals
//! - Observable rollout status for structured logs/evidence bundles

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Canary rollout phase for FNX feature enablement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RolloutPhase {
    /// FNX is disabled for all requests.
    #[default]
    Disabled,
    /// FNX is enabled for canary traffic only (e.g., 1% of requests).
    Canary,
    /// FNX is enabled for partial traffic (e.g., 10-50% of requests).
    Partial,
    /// FNX is enabled for all requests.
    Full,
    /// FNX has been rolled back due to health criteria violation.
    RolledBack,
}

impl std::fmt::Display for RolloutPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "disabled"),
            Self::Canary => write!(f, "canary"),
            Self::Partial => write!(f, "partial"),
            Self::Full => write!(f, "full"),
            Self::RolledBack => write!(f, "rolled_back"),
        }
    }
}

/// Health criteria for canary rollout progression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCriteria {
    /// Maximum acceptable error rate (0.0 - 1.0).
    pub max_error_rate: f64,
    /// Maximum acceptable P99 latency increase (percentage).
    pub max_latency_increase_pct: f64,
    /// Minimum sample size before evaluation.
    pub min_sample_size: usize,
    /// Observation window before phase transition.
    pub observation_window: Duration,
}

impl Default for HealthCriteria {
    fn default() -> Self {
        Self {
            max_error_rate: 0.01,              // 1% max error rate
            max_latency_increase_pct: 25.0,    // 25% max latency increase
            min_sample_size: 100,              // 100 samples minimum
            observation_window: Duration::from_secs(300), // 5 minute window
        }
    }
}

/// Rollback trigger reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackReason {
    /// Error rate exceeded threshold.
    ErrorRateExceeded { observed: f64, threshold: f64 },
    /// Latency regression exceeded threshold.
    LatencyRegressionExceeded { observed_pct: f64, threshold_pct: f64 },
    /// Manual rollback triggered by operator.
    ManualTrigger { operator: String },
    /// Quality metrics regression detected.
    QualityRegression { metric: String, delta: f64 },
    /// Determinism violation detected.
    DeterminismViolation { scenario_id: String },
}

impl std::fmt::Display for RollbackReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ErrorRateExceeded { observed, threshold } => {
                write!(f, "error_rate_exceeded: {observed:.2}% > {threshold:.2}%")
            }
            Self::LatencyRegressionExceeded { observed_pct, threshold_pct } => {
                write!(f, "latency_regression: {observed_pct:.1}% > {threshold_pct:.1}%")
            }
            Self::ManualTrigger { operator } => {
                write!(f, "manual_trigger by {operator}")
            }
            Self::QualityRegression { metric, delta } => {
                write!(f, "quality_regression: {metric} delta={delta:.2}")
            }
            Self::DeterminismViolation { scenario_id } => {
                write!(f, "determinism_violation: {scenario_id}")
            }
        }
    }
}

/// Rollout state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutState {
    /// Current rollout phase.
    pub phase: RolloutPhase,
    /// Previous phase (for rollback tracking).
    pub previous_phase: Option<RolloutPhase>,
    /// Timestamp when current phase began (epoch milliseconds).
    pub phase_started_at: u64,
    /// Rollback reason if in RolledBack phase.
    pub rollback_reason: Option<RollbackReason>,
    /// Total requests processed in current phase.
    pub requests_processed: u64,
    /// Error count in current phase.
    pub error_count: u64,
    /// Sum of latencies for P99 calculation (microseconds).
    pub latency_sum_us: u64,
    /// Baseline latency for comparison (microseconds).
    pub baseline_latency_us: Option<u64>,
}

impl Default for RolloutState {
    fn default() -> Self {
        Self {
            phase: RolloutPhase::Disabled,
            previous_phase: None,
            phase_started_at: 0,
            rollback_reason: None,
            requests_processed: 0,
            error_count: 0,
            latency_sum_us: 0,
            baseline_latency_us: None,
        }
    }
}

impl RolloutState {
    /// Create a new rollout state in disabled phase.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get current error rate.
    #[must_use]
    pub fn error_rate(&self) -> f64 {
        if self.requests_processed == 0 {
            0.0
        } else {
            self.error_count as f64 / self.requests_processed as f64
        }
    }

    /// Get current average latency in microseconds.
    #[must_use]
    pub fn avg_latency_us(&self) -> u64 {
        self.latency_sum_us.checked_div(self.requests_processed).unwrap_or(0)
    }

    /// Get latency increase percentage compared to baseline.
    #[must_use]
    pub fn latency_increase_pct(&self) -> f64 {
        match self.baseline_latency_us {
            Some(baseline) if baseline > 0 => {
                let current = self.avg_latency_us();
                ((current as f64 - baseline as f64) / baseline as f64) * 100.0
            }
            _ => 0.0,
        }
    }

    /// Record a request with its latency and success status.
    pub fn record_request(&mut self, latency_us: u64, is_error: bool) {
        self.requests_processed += 1;
        self.latency_sum_us += latency_us;
        if is_error {
            self.error_count += 1;
        }
    }

    /// Set baseline latency for comparison.
    pub fn set_baseline(&mut self, latency_us: u64) {
        self.baseline_latency_us = Some(latency_us);
    }

    /// Check health criteria and return rollback reason if violated.
    #[must_use]
    pub fn check_health(&self, criteria: &HealthCriteria) -> Option<RollbackReason> {
        // Skip if not enough samples
        if self.requests_processed < criteria.min_sample_size as u64 {
            return None;
        }

        // Check error rate
        let error_rate = self.error_rate();
        if error_rate > criteria.max_error_rate {
            return Some(RollbackReason::ErrorRateExceeded {
                observed: error_rate * 100.0,
                threshold: criteria.max_error_rate * 100.0,
            });
        }

        // Check latency regression
        let latency_increase = self.latency_increase_pct();
        if latency_increase > criteria.max_latency_increase_pct {
            return Some(RollbackReason::LatencyRegressionExceeded {
                observed_pct: latency_increase,
                threshold_pct: criteria.max_latency_increase_pct,
            });
        }

        None
    }

    /// Transition to next phase.
    pub fn transition_to(&mut self, next_phase: RolloutPhase, timestamp: u64) {
        self.previous_phase = Some(self.phase);
        self.phase = next_phase;
        self.phase_started_at = timestamp;
        self.requests_processed = 0;
        self.error_count = 0;
        self.latency_sum_us = 0;
        self.rollback_reason = None;
    }

    /// Trigger rollback with reason.
    pub fn rollback(&mut self, reason: RollbackReason, timestamp: u64) {
        self.previous_phase = Some(self.phase);
        self.phase = RolloutPhase::RolledBack;
        self.phase_started_at = timestamp;
        self.rollback_reason = Some(reason);
    }

    /// Check if FNX should be enabled for this request based on current phase
    /// and traffic sampling.
    #[must_use]
    pub fn should_enable_fnx(&self, request_id: u64) -> bool {
        match self.phase {
            RolloutPhase::Disabled | RolloutPhase::RolledBack => false,
            RolloutPhase::Full => true,
            RolloutPhase::Canary => {
                // 1% canary traffic
                request_id.is_multiple_of(100)
            }
            RolloutPhase::Partial => {
                // 10% partial traffic
                request_id.is_multiple_of(10)
            }
        }
    }
}

/// Rollout event for evidence logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutEvent {
    /// Timestamp (epoch milliseconds).
    pub timestamp: u64,
    /// Event type.
    pub event_type: RolloutEventType,
    /// From phase.
    pub from_phase: RolloutPhase,
    /// To phase.
    pub to_phase: RolloutPhase,
    /// Rollback reason if applicable.
    pub rollback_reason: Option<RollbackReason>,
    /// Metrics at transition time.
    pub metrics: RolloutMetrics,
}

/// Rollout event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RolloutEventType {
    /// Phase transition (forward progression).
    Transition,
    /// Rollback triggered.
    Rollback,
    /// Health check passed.
    HealthCheckPassed,
    /// Health check failed (warning, not yet rollback).
    HealthCheckFailed,
}

/// Rollout metrics snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RolloutMetrics {
    pub requests_processed: u64,
    pub error_rate: f64,
    pub avg_latency_us: u64,
    pub latency_increase_pct: f64,
}

impl From<&RolloutState> for RolloutMetrics {
    fn from(state: &RolloutState) -> Self {
        Self {
            requests_processed: state.requests_processed,
            error_rate: state.error_rate(),
            avg_latency_us: state.avg_latency_us(),
            latency_increase_pct: state.latency_increase_pct(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_disabled() {
        let state = RolloutState::new();
        assert_eq!(state.phase, RolloutPhase::Disabled);
        assert!(!state.should_enable_fnx(42));
    }

    #[test]
    fn full_phase_enables_all_requests() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Full, 1000);

        for i in 0..100 {
            assert!(state.should_enable_fnx(i), "request {i} should be enabled in full phase");
        }
    }

    #[test]
    fn canary_phase_enables_one_percent() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Canary, 1000);

        let enabled_count: usize = (0..1000).filter(|&i| state.should_enable_fnx(i)).count();
        assert_eq!(enabled_count, 10, "canary should enable 1% of traffic (10/1000)");
    }

    #[test]
    fn partial_phase_enables_ten_percent() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Partial, 1000);

        let enabled_count: usize = (0..100).filter(|&i| state.should_enable_fnx(i)).count();
        assert_eq!(enabled_count, 10, "partial should enable 10% of traffic (10/100)");
    }

    #[test]
    fn error_rate_calculation() {
        let mut state = RolloutState::new();
        state.record_request(100, false);
        state.record_request(100, false);
        state.record_request(100, true);

        let error_rate = state.error_rate();
        assert!((error_rate - 0.333).abs() < 0.01, "error rate should be ~33%");
    }

    #[test]
    fn health_check_triggers_rollback_on_high_error_rate() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Canary, 1000);

        // Record 100 requests with 5 errors (5% error rate)
        for i in 0..100 {
            state.record_request(100, i < 5);
        }

        let criteria = HealthCriteria {
            max_error_rate: 0.01, // 1% threshold
            ..Default::default()
        };

        let reason = state.check_health(&criteria);
        assert!(matches!(reason, Some(RollbackReason::ErrorRateExceeded { .. })));
    }

    #[test]
    fn health_check_triggers_rollback_on_latency_regression() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Canary, 1000);
        state.set_baseline(100); // 100us baseline

        // Record 100 requests at 200us (100% increase)
        for _ in 0..100 {
            state.record_request(200, false);
        }

        let criteria = HealthCriteria {
            max_latency_increase_pct: 25.0, // 25% threshold
            ..Default::default()
        };

        let reason = state.check_health(&criteria);
        assert!(matches!(reason, Some(RollbackReason::LatencyRegressionExceeded { .. })));
    }

    #[test]
    fn health_check_passes_when_within_criteria() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Canary, 1000);
        state.set_baseline(100);

        // Record 100 requests at 110us (10% increase) with 0 errors
        for _ in 0..100 {
            state.record_request(110, false);
        }

        let criteria = HealthCriteria::default();
        let reason = state.check_health(&criteria);
        assert!(reason.is_none(), "health check should pass");
    }

    #[test]
    fn rollback_preserves_previous_phase() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Canary, 1000);

        state.rollback(
            RollbackReason::ManualTrigger { operator: "test".to_string() },
            2000,
        );

        assert_eq!(state.phase, RolloutPhase::RolledBack);
        assert_eq!(state.previous_phase, Some(RolloutPhase::Canary));
        assert!(!state.should_enable_fnx(42));
    }

    #[test]
    fn transition_resets_metrics() {
        let mut state = RolloutState::new();
        state.record_request(100, true);
        assert_eq!(state.error_count, 1);

        state.transition_to(RolloutPhase::Canary, 1000);

        assert_eq!(state.requests_processed, 0);
        assert_eq!(state.error_count, 0);
        assert_eq!(state.latency_sum_us, 0);
    }

    #[test]
    fn health_check_skips_insufficient_samples() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Canary, 1000);

        // Only 10 samples (below min_sample_size of 100)
        for _ in 0..10 {
            state.record_request(100, true); // 100% error rate
        }

        let criteria = HealthCriteria::default();
        let reason = state.check_health(&criteria);
        assert!(reason.is_none(), "should skip check with insufficient samples");
    }

    #[test]
    fn rollout_event_serialization() {
        let event = RolloutEvent {
            timestamp: 1000,
            event_type: RolloutEventType::Rollback,
            from_phase: RolloutPhase::Canary,
            to_phase: RolloutPhase::RolledBack,
            rollback_reason: Some(RollbackReason::ErrorRateExceeded {
                observed: 5.0,
                threshold: 1.0,
            }),
            metrics: RolloutMetrics::default(),
        };

        let json = serde_json::to_string(&event).expect("should serialize");
        assert!(json.contains("rollback"));
        assert!(json.contains("error_rate_exceeded"));
    }
}
