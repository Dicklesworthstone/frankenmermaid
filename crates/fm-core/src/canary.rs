//! Canary rollout and deterministic rollback for FNX feature enablement.
//!
//! This module defines:
//! - Phased rollout states with health criteria
//! - Automatic rollback triggers from compatibility/regression signals
//! - Observable rollout status for structured logs/evidence bundles

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Maximum number of recent accepted FNX latencies retained for percentile checks.
///
/// P99 is exact for these retained samples. The observation window separately
/// controls phase maturity; this is not a time-bucketed latency history.
pub const MAX_LATENCY_SAMPLES: usize = 4096;

/// Bounded latency history, serialized as a sequence of microsecond values.
///
/// The private deque prevents callers and restored snapshots from bypassing the
/// retention limit while the public type permits `RolloutState` struct updates.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(transparent)]
pub struct LatencySamples(std::collections::VecDeque<u64>);

impl LatencySamples {
    /// Number of retained observations, distinct from the phase's total traffic.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no observed latency samples.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn record(&mut self, latency_us: u64) {
        if self.0.len() == MAX_LATENCY_SAMPLES {
            let _ = self.0.pop_front();
        }
        self.0.push_back(latency_us);
    }

    fn clear(&mut self) {
        self.0.clear();
    }

    fn p99_us(&self) -> Option<u64> {
        let sample_count = self.0.len();
        if sample_count == 0 {
            return None;
        }

        // Nearest-rank P99: ceil(99 * n / 100), as a zero-based index without
        // multiplying and risking overflow.
        let rank_index = sample_count - sample_count / 100 - 1;
        let mut samples: Vec<u64> = self.0.iter().copied().collect();
        let (_, percentile, _) = samples.select_nth_unstable(rank_index);
        Some(*percentile)
    }
}

impl<'de> Deserialize<'de> for LatencySamples {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct LatencySamplesVisitor;

        impl<'de> serde::de::Visitor<'de> for LatencySamplesVisitor {
            type Value = LatencySamples;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a sequence of microsecond latency samples")
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut samples = LatencySamples::default();
                // Do not collect an unbounded Vec or trust the input size hint.
                while let Some(latency_us) = sequence.next_element::<u64>()? {
                    samples.record(latency_us);
                }
                Ok(samples)
            }
        }

        deserializer.deserialize_seq(LatencySamplesVisitor)
    }
}

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
    /// Minimum accepted request count before evaluation. Latency checks also
    /// require this many retained samples, up to `MAX_LATENCY_SAMPLES`.
    pub min_sample_size: usize,
    /// Minimum elapsed phase time before health evaluation.
    pub observation_window: Duration,
}

impl Default for HealthCriteria {
    fn default() -> Self {
        Self {
            max_error_rate: 0.01,                         // 1% max error rate
            max_latency_increase_pct: 25.0,               // 25% max latency increase
            min_sample_size: 100,                         // 100 samples minimum
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
    LatencyRegressionExceeded {
        observed_pct: f64,
        threshold_pct: f64,
    },
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
            Self::ErrorRateExceeded {
                observed,
                threshold,
            } => {
                write!(f, "error_rate_exceeded: {observed:.2}% > {threshold:.2}%")
            }
            Self::LatencyRegressionExceeded {
                observed_pct,
                threshold_pct,
            } => {
                write!(
                    f,
                    "latency_regression: {observed_pct:.1}% > {threshold_pct:.1}%"
                )
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
    /// Total accepted FNX attempts in the current phase, including failures.
    pub requests_processed: u64,
    /// Error count in current phase.
    pub error_count: u64,
    /// Sum of latencies for average telemetry only (microseconds).
    pub latency_sum_us: u64,
    /// Recent actual latencies for P99 calculation. Old snapshots without
    /// samples must collect fresh observations before latency health evaluation.
    #[serde(default)]
    pub latency_samples: LatencySamples,
    /// Baseline P99 latency for comparison (microseconds).
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
            latency_samples: LatencySamples::default(),
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
        self.latency_sum_us
            .checked_div(self.requests_processed)
            .unwrap_or(0)
    }

    /// Get the exact nearest-rank P99 of the retained latency samples.
    ///
    /// Returns `None` when no actual samples are known, including old snapshots
    /// containing only aggregate counters. It never substitutes the mean.
    #[must_use]
    pub fn p99_latency_us(&self) -> Option<u64> {
        self.latency_samples.p99_us()
    }

    /// Get P99 latency increase compared to a nonzero baseline, when both exist.
    #[must_use]
    pub fn latency_increase_pct(&self) -> Option<f64> {
        let baseline = self.baseline_latency_us.filter(|&baseline| baseline > 0)?;
        let current = self.p99_latency_us()?;
        Some(((current as f64 - baseline as f64) / baseline as f64) * 100.0)
    }

    /// Record a request with its latency and success status.
    pub fn record_request(&mut self, latency_us: u64, is_error: bool) {
        // Telemetry is allowed to saturate but never to wrap. A long-lived process (or one bad
        // restored snapshot) used to panic in checked builds and silently reset these counters in
        // release, making an unhealthy canary look as if it had almost no traffic or latency.
        self.requests_processed = self.requests_processed.saturating_add(1);
        self.latency_sum_us = self.latency_sum_us.saturating_add(latency_us);
        self.latency_samples.record(latency_us);
        if is_error {
            self.error_count = self.error_count.saturating_add(1);
        }
    }

    /// Set the baseline P99 latency for comparison.
    pub fn set_baseline(&mut self, latency_us: u64) {
        self.baseline_latency_us = Some(latency_us);
    }

    /// Check health criteria at an explicit epoch-millisecond timestamp.
    ///
    /// Evaluation waits for both phase maturity and enough accepted FNX
    /// requests. A clock before the phase start leaves health undecided. The
    /// caller supplies time so this core API also works on WASM and in replays.
    #[must_use]
    pub fn check_health(
        &self,
        criteria: &HealthCriteria,
        timestamp_ms: u64,
    ) -> Option<RollbackReason> {
        let elapsed_ms = timestamp_ms.checked_sub(self.phase_started_at)?;
        if Duration::from_millis(elapsed_ms) < criteria.observation_window
            || self.requests_processed < criteria.min_sample_size as u64
        {
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

        // Aggregate counters from an old snapshot are not percentile samples.
        // A minimum above capacity still waits for that much accepted traffic,
        // then evaluates the most recent complete retained sample set.
        if self.latency_samples.len() >= criteria.min_sample_size.min(MAX_LATENCY_SAMPLES)
            && let Some(latency_increase) = self.latency_increase_pct()
            && latency_increase > criteria.max_latency_increase_pct
        {
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
        self.latency_samples.clear();
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
    #[serde(default)]
    pub latency_sample_count: usize,
    #[serde(default)]
    pub p99_latency_us: Option<u64>,
    pub latency_increase_pct: Option<f64>,
}

impl From<&RolloutState> for RolloutMetrics {
    fn from(state: &RolloutState) -> Self {
        Self {
            requests_processed: state.requests_processed,
            error_rate: state.error_rate(),
            avg_latency_us: state.avg_latency_us(),
            latency_sample_count: state.latency_samples.len(),
            p99_latency_us: state.p99_latency_us(),
            latency_increase_pct: state.latency_increase_pct(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_history_distinguishes_missing_and_zero_latency() {
        let mut samples = LatencySamples::default();
        assert!(samples.is_empty());
        assert_eq!(samples.p99_us(), None);
        samples.record(0);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples.p99_us(), Some(0));
        samples.clear();
        assert!(samples.is_empty());
        assert_eq!(samples.p99_us(), None);
    }

    #[test]
    fn latency_history_uses_nearest_rank_p99_instead_of_the_mean_or_maximum() {
        let mut samples = LatencySamples::default();
        for _ in 0..99 {
            samples.record(100);
        }
        samples.record(1_000);
        // A single slow observation out of exactly 100 lies above P99.
        assert_eq!(samples.p99_us(), Some(100));
        samples.record(1_000);
        // Two slow observations out of 101 enter P99, while the mean stays below 120us.
        assert_eq!(samples.p99_us(), Some(1_000));
    }

    #[test]
    fn latency_history_percentile_does_not_depend_on_observation_order() {
        let mut ascending = LatencySamples::default();
        let mut descending = LatencySamples::default();
        for latency_us in 1..=100 {
            ascending.record(latency_us);
        }
        for latency_us in (1..=100).rev() {
            descending.record(latency_us);
        }
        assert_eq!(ascending.p99_us(), Some(99));
        assert_eq!(descending.p99_us(), Some(99));
        let mut largest = LatencySamples::default();
        largest.record(u64::MAX);
        assert_eq!(largest.p99_us(), Some(u64::MAX));
    }

    #[test]
    fn latency_history_evicts_oldest_observations_at_the_capacity_limit() {
        let mut samples = LatencySamples::default();
        for latency_us in 0..(MAX_LATENCY_SAMPLES + 17) {
            samples.record(u64::try_from(latency_us).unwrap());
        }
        assert_eq!(samples.len(), MAX_LATENCY_SAMPLES);
        assert_eq!(samples.0.front(), Some(&17));
        assert_eq!(
            samples.0.back(),
            Some(&u64::try_from(MAX_LATENCY_SAMPLES + 16).unwrap())
        );
        for _ in 0..MAX_LATENCY_SAMPLES {
            samples.record(100);
        }
        assert_eq!(samples.len(), MAX_LATENCY_SAMPLES);
        assert_eq!(samples.p99_us(), Some(100));
        assert!(samples.0.iter().all(|sample| *sample == 100));
    }

    #[test]
    fn latency_history_round_trips_as_a_plain_sample_sequence() {
        let mut samples = LatencySamples::default();
        samples.record(11);
        samples.record(22);
        samples.record(33);
        let json = serde_json::to_string(&samples).unwrap();
        assert_eq!(json, "[11,22,33]");
        let restored: LatencySamples = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.len(), 3);
        assert_eq!(restored.p99_us(), Some(33));
        assert_eq!(serde_json::to_string(&restored).unwrap(), json);
    }

    #[test]
    fn latency_history_caps_restored_samples_and_preserves_the_newest_values() {
        let input: Vec<u64> = (0..(MAX_LATENCY_SAMPLES + 17))
            .map(|sample| u64::try_from(sample).unwrap())
            .collect();
        let json = serde_json::to_string(&input).unwrap();
        let restored: LatencySamples = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.len(), MAX_LATENCY_SAMPLES);
        assert_eq!(restored.0.front(), Some(&17));
        assert_eq!(
            restored.0.back(),
            Some(&u64::try_from(MAX_LATENCY_SAMPLES + 16).unwrap())
        );
        let retained: Vec<u64> =
            serde_json::from_str(&serde_json::to_string(&restored).unwrap()).unwrap();
        assert_eq!(retained, input[17..]);
    }

    #[test]
    fn latency_history_rejects_invalid_sample_values() {
        for json in ["null", "[-1]", "[1.5]", r#"["100"]"#, "[null]"] {
            assert!(
                serde_json::from_str::<LatencySamples>(json).is_err(),
                "invalid latency history was accepted: {json}"
            );
        }
    }

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
            assert!(
                state.should_enable_fnx(i),
                "request {i} should be enabled in full phase"
            );
        }
    }

    #[test]
    fn canary_phase_enables_one_percent() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Canary, 1000);

        let enabled_count: usize = (0..1000).filter(|&i| state.should_enable_fnx(i)).count();
        assert_eq!(
            enabled_count, 10,
            "canary should enable 1% of traffic (10/1000)"
        );
    }

    #[test]
    fn partial_phase_enables_ten_percent() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Partial, 1000);

        let enabled_count: usize = (0..100).filter(|&i| state.should_enable_fnx(i)).count();
        assert_eq!(
            enabled_count, 10,
            "partial should enable 10% of traffic (10/100)"
        );
    }

    #[test]
    fn error_rate_calculation() {
        let mut state = RolloutState::new();
        state.record_request(100, false);
        state.record_request(100, false);
        state.record_request(100, true);

        let error_rate = state.error_rate();
        assert!(
            (error_rate - 0.333).abs() < 0.01,
            "error rate should be ~33%"
        );
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

        let reason = state.check_health(&criteria, 301_000);
        assert!(matches!(
            reason,
            Some(RollbackReason::ErrorRateExceeded { .. })
        ));
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

        let reason = state.check_health(&criteria, 301_000);
        assert!(matches!(
            reason,
            Some(RollbackReason::LatencyRegressionExceeded { .. })
        ));
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
        let reason = state.check_health(&criteria, 301_000);
        assert!(reason.is_none(), "health check should pass");
    }

    #[test]
    fn rollback_preserves_previous_phase() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Canary, 1000);

        state.rollback(
            RollbackReason::ManualTrigger {
                operator: "test".to_string(),
            },
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
        assert!(state.latency_samples.is_empty());
        assert_eq!(state.p99_latency_us(), None);
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
        let reason = state.check_health(&criteria, 301_000);
        assert!(
            reason.is_none(),
            "should skip check with insufficient samples"
        );
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

    #[test]
    fn health_check_detects_a_slow_tail_hidden_by_a_healthy_mean() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Canary, 1_000);
        state.set_baseline(100);
        for _ in 0..98 {
            state.record_request(100, false);
        }
        for _ in 0..2 {
            state.record_request(1_000, false);
        }

        assert_eq!(state.avg_latency_us(), 118);
        assert_eq!(state.p99_latency_us(), Some(1_000));
        assert_eq!(state.latency_increase_pct(), Some(900.0));
        let criteria = HealthCriteria::default();
        assert_eq!(state.check_health(&criteria, 300_999), None);
        assert!(matches!(
            state.check_health(&criteria, 301_000),
            Some(RollbackReason::LatencyRegressionExceeded {
                observed_pct,
                threshold_pct,
            }) if observed_pct == 900.0 && threshold_pct == 25.0
        ));

        let restored: RolloutState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(restored.p99_latency_us(), Some(1_000));
        assert_eq!(restored.check_health(&criteria, 300_999), None);
        assert_eq!(
            restored.check_health(&criteria, 301_000),
            state.check_health(&criteria, 301_000)
        );
        let metrics = RolloutMetrics::from(&restored);
        assert_eq!(metrics.latency_sample_count, 100);
        assert_eq!(metrics.avg_latency_us, 118);
        assert_eq!(metrics.p99_latency_us, Some(1_000));
        assert_eq!(metrics.latency_increase_pct, Some(900.0));
    }

    #[test]
    fn health_check_waits_for_the_window_and_rejects_a_clock_before_phase_start() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Full, 1_000);
        state.record_request(100, true);
        let criteria = HealthCriteria {
            min_sample_size: 1,
            observation_window: Duration::from_millis(10),
            ..Default::default()
        };

        assert_eq!(state.check_health(&criteria, 999), None);
        assert_eq!(state.check_health(&criteria, 1_000), None);
        assert_eq!(state.check_health(&criteria, 1_009), None);
        assert!(matches!(
            state.check_health(&criteria, 1_010),
            Some(RollbackReason::ErrorRateExceeded { .. })
        ));
    }

    #[test]
    fn health_check_preserves_sub_millisecond_windows_and_clock_limits() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Full, 1_000);
        state.record_request(100, true);
        let mut criteria = HealthCriteria {
            min_sample_size: 1,
            observation_window: Duration::from_nanos(1_000_001),
            ..Default::default()
        };
        assert_eq!(state.check_health(&criteria, 1_001), None);
        assert!(state.check_health(&criteria, 1_002).is_some());

        state.phase_started_at = u64::MAX - 1;
        criteria.observation_window = Duration::from_millis(2);
        assert_eq!(state.check_health(&criteria, u64::MAX), None);
        criteria.observation_window = Duration::from_millis(1);
        assert!(state.check_health(&criteria, u64::MAX).is_some());
        criteria.observation_window = Duration::MAX;
        assert_eq!(state.check_health(&criteria, u64::MAX), None);
        criteria.observation_window = Duration::ZERO;
        assert!(state.check_health(&criteria, u64::MAX - 1).is_some());
        assert_eq!(state.check_health(&criteria, 0), None);
    }

    #[test]
    fn health_check_can_require_more_total_requests_than_the_retention_capacity() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Full, 1_000);
        state.set_baseline(100);
        let criteria = HealthCriteria {
            min_sample_size: MAX_LATENCY_SAMPLES + 17,
            ..Default::default()
        };
        for _ in 1..criteria.min_sample_size {
            state.record_request(1_000, false);
        }
        assert_eq!(state.latency_samples.len(), MAX_LATENCY_SAMPLES);
        assert_eq!(state.check_health(&criteria, 301_000), None);
        state.record_request(1_000, false);
        assert!(matches!(
            state.check_health(&criteria, 301_000),
            Some(RollbackReason::LatencyRegressionExceeded { .. })
        ));
    }

    #[test]
    fn legacy_snapshot_collects_fresh_samples_without_inventing_historical_p99() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Canary, 1_000);
        state.set_baseline(100);
        for _ in 0..100 {
            state.record_request(1_000, false);
        }
        let mut snapshot = serde_json::to_value(&state).unwrap();
        let _ = snapshot.as_object_mut().unwrap().remove("latency_samples");
        let mut restored: RolloutState = serde_json::from_value(snapshot.clone()).unwrap();
        let criteria = HealthCriteria::default();
        assert_eq!(restored.avg_latency_us(), 1_000);
        assert_eq!(restored.p99_latency_us(), None);
        assert_eq!(restored.latency_increase_pct(), None);
        assert_eq!(restored.check_health(&criteria, 301_000), None);
        let metrics = RolloutMetrics::from(&restored);
        assert_eq!(metrics.latency_sample_count, 0);
        assert_eq!(metrics.p99_latency_us, None);
        assert_eq!(metrics.latency_increase_pct, None);

        // Known error counters remain actionable without a percentile history.
        snapshot["error_count"] = serde_json::json!(3);
        let with_errors: RolloutState = serde_json::from_value(snapshot).unwrap();
        assert!(matches!(
            with_errors.check_health(&criteria, 301_000),
            Some(RollbackReason::ErrorRateExceeded { .. })
        ));

        for _ in 0..99 {
            restored.record_request(1_000, false);
        }
        assert_eq!(restored.check_health(&criteria, 301_000), None);
        restored.record_request(1_000, false);
        assert!(matches!(
            restored.check_health(&criteria, 301_000),
            Some(RollbackReason::LatencyRegressionExceeded { .. })
        ));
    }

    #[test]
    fn percentile_health_requires_a_known_nonzero_baseline() {
        let mut state = RolloutState::new();
        state.transition_to(RolloutPhase::Full, 1_000);
        let criteria = HealthCriteria {
            min_sample_size: 0,
            ..Default::default()
        };
        state.set_baseline(100);
        assert_eq!(state.latency_increase_pct(), None);
        assert_eq!(state.check_health(&criteria, 301_000), None);
        state.record_request(0, false);
        assert_eq!(state.latency_increase_pct(), Some(-100.0));
        state.baseline_latency_us = None;
        assert_eq!(state.latency_increase_pct(), None);
        state.set_baseline(0);
        assert_eq!(state.latency_increase_pct(), None);
        assert_eq!(state.check_health(&criteria, 301_000), None);
    }

    #[test]
    fn request_counters_saturate_instead_of_wrapping() {
        let mut state = RolloutState {
            requests_processed: u64::MAX,
            error_count: u64::MAX,
            latency_sum_us: u64::MAX,
            ..RolloutState::default()
        };

        state.record_request(1, true);

        assert_eq!(state.requests_processed, u64::MAX);
        assert_eq!(state.error_count, u64::MAX);
        assert_eq!(state.latency_sum_us, u64::MAX);
        assert_eq!(state.error_rate(), 1.0);
    }
}
