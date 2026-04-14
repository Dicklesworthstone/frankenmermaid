//! Structured evidence log schema for FNX-assisted execution paths.
//!
//! This module defines the canonical log format for capturing execution evidence,
//! including timing metrics, FNX participation state, and determinism verification
//! artifacts. All FNX e2e scripts and regression harnesses should emit logs
//! conforming to this schema.
//!
//! See: evidence/contracts/fnx-deterministic-decision-contract.md

use serde::{Deserialize, Serialize};

/// Canonical evidence log entry for a single diagram processing scenario.
///
/// This structure captures all required fields for FNX QA verification:
/// - Scenario identification and input fingerprinting
/// - FNX participation mode and decision authority
/// - Timing breakdown across pipeline phases
/// - Diagnostic and fallback metadata
/// - Output determinism verification hashes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct EvidenceLogEntry {
    /// Unique identifier for this test scenario (e.g., "flowchart_simple").
    pub scenario_id: String,

    /// FNV-1a hash of the raw input text for reproducibility.
    pub input_hash: String,

    /// FNX integration mode used for this run.
    pub fnx_mode: FnxMode,

    /// Graph projection strategy applied.
    pub projection_mode: ProjectionMode,

    /// Decision authority mode (who has final say).
    pub decision_mode: DecisionMode,

    /// FNX algorithm(s) invoked during analysis, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fnx_algorithm: Option<String>,

    /// Time spent parsing the input (milliseconds).
    pub parse_ms: f64,

    /// Time spent in FNX analysis (milliseconds, 0 if FNX disabled/skipped).
    pub analysis_ms: f64,

    /// Time spent in layout computation (milliseconds).
    pub layout_ms: f64,

    /// Time spent rendering output (milliseconds).
    pub render_ms: f64,

    /// Number of diagnostics emitted (warnings + errors).
    pub diagnostic_count: usize,

    /// Fallback reason code if degradation occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<FallbackReason>,

    /// Hash of FNX analysis witness data for determinism verification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub witness_hash: Option<String>,

    /// Hash of the final rendered output for regression detection.
    pub output_hash: String,

    /// Pass/fail status with explanation.
    pub pass_fail_reason: PassFailReason,
}

/// FNX integration mode.
///
/// Defines whether FNX is participating and in what capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FnxMode {
    /// FNX integration is disabled (feature flag off or --fnx-mode=disabled).
    #[default]
    Off,
    /// FNX provides advisory metadata only, native engine has authority.
    Advisory,
    /// FNX experimental directed mode (future use).
    ExperimentalDirected,
    /// Strict mode requiring FNX participation.
    Strict,
}

impl FnxMode {
    /// Returns true if FNX is actively participating (not off).
    #[must_use]
    pub fn is_active(self) -> bool {
        !matches!(self, Self::Off)
    }
}

impl std::fmt::Display for FnxMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "off"),
            Self::Advisory => write!(f, "advisory"),
            Self::ExperimentalDirected => write!(f, "experimental_directed"),
            Self::Strict => write!(f, "strict"),
        }
    }
}

/// Graph projection strategy for FNX analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionMode {
    /// No FNX projection, native engine only.
    #[default]
    NativeOnly,
    /// Native engine with FNX advisory overlay.
    NativePlusFnxAdvisory,
    /// FNX-first projection (experimental).
    FnxPrimary,
}

impl std::fmt::Display for ProjectionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NativeOnly => write!(f, "native_only"),
            Self::NativePlusFnxAdvisory => write!(f, "native_plus_fnx_advisory"),
            Self::FnxPrimary => write!(f, "fnx_primary"),
        }
    }
}

/// Decision authority mode for layout/parse decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DecisionMode {
    /// Native frankenmermaid engine is authoritative.
    #[default]
    NativeAuthoritative,
    /// FNX is authoritative (experimental, requires explicit opt-in).
    FnxAuthoritativeExperimental,
}

impl std::fmt::Display for DecisionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NativeAuthoritative => write!(f, "native_authoritative"),
            Self::FnxAuthoritativeExperimental => write!(f, "fnx_authoritative_experimental"),
        }
    }
}

/// Fallback reason when FNX analysis is degraded or skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackReason {
    /// FNX feature flag is disabled.
    FeatureDisabled,
    /// Analysis exceeded time budget.
    Timeout,
    /// Analysis returned an error.
    AnalysisError,
    /// Projection strategy produced invalid results.
    InvalidProjection,
    /// Native precedence rule applied.
    NativePrecedence,
    /// Diagram type not supported by FNX.
    UnsupportedDiagramType,
    /// FNX runtime unavailable.
    RuntimeUnavailable,
}

impl std::fmt::Display for FallbackReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FeatureDisabled => write!(f, "feature_disabled"),
            Self::Timeout => write!(f, "timeout"),
            Self::AnalysisError => write!(f, "analysis_error"),
            Self::InvalidProjection => write!(f, "invalid_projection"),
            Self::NativePrecedence => write!(f, "native_precedence"),
            Self::UnsupportedDiagramType => write!(f, "unsupported_diagram_type"),
            Self::RuntimeUnavailable => write!(f, "runtime_unavailable"),
        }
    }
}

/// Pass/fail determination for evidence validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassFailReason {
    /// Test passed - output matches expected.
    Pass,
    /// Test passed - output matches golden snapshot.
    PassMatchedGolden,
    /// Test passed - determinism verified across N runs.
    PassDeterministic { runs: usize },
    /// Test failed - output hash mismatch.
    FailHashMismatch {
        expected: String,
        actual: String,
    },
    /// Test failed - golden snapshot missing.
    FailMissingGolden,
    /// Test failed - parse error.
    FailParseError { message: String },
    /// Test failed - layout error.
    FailLayoutError { message: String },
    /// Test failed - render error.
    FailRenderError { message: String },
    /// Test failed - determinism violation.
    FailNondeterministic {
        runs: usize,
        unique_hashes: usize,
    },
    /// Test skipped.
    Skipped { reason: String },
}

impl PassFailReason {
    /// Returns true if this represents a passing result.
    #[must_use]
    pub fn is_pass(&self) -> bool {
        matches!(
            self,
            Self::Pass | Self::PassMatchedGolden | Self::PassDeterministic { .. }
        )
    }

    /// Returns true if this represents a failure.
    #[must_use]
    pub fn is_fail(&self) -> bool {
        matches!(
            self,
            Self::FailHashMismatch { .. }
                | Self::FailMissingGolden
                | Self::FailParseError { .. }
                | Self::FailLayoutError { .. }
                | Self::FailRenderError { .. }
                | Self::FailNondeterministic { .. }
        )
    }
}

impl std::fmt::Display for PassFailReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pass => write!(f, "pass"),
            Self::PassMatchedGolden => write!(f, "pass_matched_golden"),
            Self::PassDeterministic { runs } => {
                write!(f, "pass_deterministic_{runs}_runs")
            }
            Self::FailHashMismatch { expected, actual } => {
                write!(f, "fail_hash_mismatch: expected {expected}, got {actual}")
            }
            Self::FailMissingGolden => write!(f, "fail_missing_golden"),
            Self::FailParseError { message } => write!(f, "fail_parse_error: {message}"),
            Self::FailLayoutError { message } => write!(f, "fail_layout_error: {message}"),
            Self::FailRenderError { message } => write!(f, "fail_render_error: {message}"),
            Self::FailNondeterministic {
                runs,
                unique_hashes,
            } => {
                write!(
                    f,
                    "fail_nondeterministic: {unique_hashes} unique hashes in {runs} runs"
                )
            }
            Self::Skipped { reason } => write!(f, "skipped: {reason}"),
        }
    }
}

/// Evidence bundle containing multiple log entries and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EvidenceBundle {
    /// Schema version for forward compatibility.
    pub schema_version: String,
    /// Timestamp when this bundle was generated (ISO 8601).
    pub generated_at: String,
    /// Git commit hash for reproducibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    /// Build profile (debug/release).
    pub build_profile: String,
    /// FNX feature flags enabled.
    pub fnx_features: FnxFeatures,
    /// Individual log entries.
    pub entries: Vec<EvidenceLogEntry>,
    /// Summary statistics.
    pub summary: EvidenceSummary,
}

/// FNX feature flag state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct FnxFeatures {
    /// Whether fnx-integration feature is enabled.
    pub fnx_integration: bool,
    /// Whether fnx-experimental-directed feature is enabled.
    pub fnx_experimental_directed: bool,
}

/// Summary statistics for an evidence bundle.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct EvidenceSummary {
    /// Total number of scenarios.
    pub total: usize,
    /// Number of passing scenarios.
    pub passed: usize,
    /// Number of failing scenarios.
    pub failed: usize,
    /// Number of skipped scenarios.
    pub skipped: usize,
    /// Total parse time (ms).
    pub total_parse_ms: f64,
    /// Total analysis time (ms).
    pub total_analysis_ms: f64,
    /// Total layout time (ms).
    pub total_layout_ms: f64,
    /// Total render time (ms).
    pub total_render_ms: f64,
    /// Total diagnostic count.
    pub total_diagnostics: usize,
}

impl EvidenceBundle {
    /// Create a new empty evidence bundle.
    #[must_use]
    pub fn new(git_commit: Option<String>, build_profile: &str, fnx_features: FnxFeatures) -> Self {
        Self {
            schema_version: "1.0.0".to_string(),
            generated_at: chrono_now_iso8601(),
            git_commit,
            build_profile: build_profile.to_string(),
            fnx_features,
            entries: Vec::new(),
            summary: EvidenceSummary::default(),
        }
    }

    /// Add an entry and update summary statistics.
    pub fn add_entry(&mut self, entry: EvidenceLogEntry) {
        self.summary.total += 1;
        if entry.pass_fail_reason.is_pass() {
            self.summary.passed += 1;
        } else if entry.pass_fail_reason.is_fail() {
            self.summary.failed += 1;
        } else {
            self.summary.skipped += 1;
        }
        self.summary.total_parse_ms += entry.parse_ms;
        self.summary.total_analysis_ms += entry.analysis_ms;
        self.summary.total_layout_ms += entry.layout_ms;
        self.summary.total_render_ms += entry.render_ms;
        self.summary.total_diagnostics += entry.diagnostic_count;
        self.entries.push(entry);
    }
}

/// Compute FNV-1a hash and return as hex string.
#[must_use]
pub fn fnv1a_hex(data: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Get current timestamp in ISO 8601 format.
///
/// Uses a simple fallback if chrono is not available.
fn chrono_now_iso8601() -> String {
    // Simple UTC timestamp without chrono dependency
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Convert to ISO 8601 format manually
    let days_since_epoch = secs / 86400;
    let secs_today = secs % 86400;
    let hours = secs_today / 3600;
    let minutes = (secs_today % 3600) / 60;
    let seconds = secs_today % 60;

    // Simplified date calculation (accurate for 2000-2099)
    let mut days = days_since_epoch as i64;
    let mut year = 1970i64;
    loop {
        let days_in_year = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            366
        } else {
            365
        };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days: [i64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 0usize;
    for (i, &d) in month_days.iter().enumerate() {
        if days < d {
            month = i + 1;
            break;
        }
        days -= d;
    }
    let day = days + 1;

    format!(
        "{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z"
    )
}

// ============================================================================
// Config Lint for FNX Mode Combinations (bd-ml2r.12.3)
// ============================================================================

/// Severity level for config lint warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintSeverity {
    /// Informational note, no action needed.
    Info,
    /// Warning about suboptimal configuration.
    Warning,
    /// Error indicating unsupported or dangerous configuration.
    Error,
}

/// A single config lint warning with remediation guidance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigLintWarning {
    /// Severity of this warning.
    pub severity: LintSeverity,
    /// Short code for programmatic matching (e.g., "fnx-strict-fallback").
    pub code: String,
    /// Human-readable warning message.
    pub message: String,
    /// Recommended fix or alternative.
    pub recommendation: String,
}

/// Result of linting FNX configuration options.
#[derive(Debug, Clone, Default)]
pub struct ConfigLintResult {
    /// List of warnings/errors found.
    pub warnings: Vec<ConfigLintWarning>,
}

impl ConfigLintResult {
    /// Returns true if there are any errors (not just warnings).
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.warnings
            .iter()
            .any(|w| w.severity == LintSeverity::Error)
    }

    /// Returns true if there are any warnings or errors.
    #[must_use]
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

/// Configuration options for FNX linting.
#[derive(Debug, Clone, Copy, Default)]
pub struct FnxConfigLintInput {
    /// FNX mode being used.
    pub fnx_mode: FnxMode,
    /// Projection mode (undirected, directed, etc.).
    pub projection_mode: ProjectionMode,
    /// Whether FNX feature is available at compile time.
    pub fnx_available: bool,
    /// Whether running in WASM environment.
    pub is_wasm: bool,
    /// Whether strict fallback is requested.
    pub strict_fallback: bool,
    /// Whether directed projection was explicitly requested (future feature).
    pub directed_projection_requested: bool,
}

/// Lint FNX configuration and return any warnings.
#[must_use]
pub fn lint_fnx_config(input: &FnxConfigLintInput) -> ConfigLintResult {
    let mut result = ConfigLintResult::default();

    // Check: FNX enabled but not available
    if input.fnx_mode.is_active() && !input.fnx_available {
        result.warnings.push(ConfigLintWarning {
            severity: LintSeverity::Error,
            code: "fnx-unavailable".to_string(),
            message: "FNX mode enabled but fnx-integration feature is not available.".to_string(),
            recommendation: "Rebuild with --features fnx-integration or use --fnx-mode disabled."
                .to_string(),
        });
    }

    // Check: FNX requested in WASM
    if input.fnx_mode.is_active() && input.is_wasm {
        result.warnings.push(ConfigLintWarning {
            severity: LintSeverity::Warning,
            code: "fnx-wasm-unsupported".to_string(),
            message: "FNX integration is not supported in WebAssembly builds.".to_string(),
            recommendation: "FNX will be disabled automatically. Use --fnx-mode disabled to suppress this warning.".to_string(),
        });
    }

    // Check: Strict fallback with FNX enabled
    if input.fnx_mode.is_active() && input.strict_fallback {
        result.warnings.push(ConfigLintWarning {
            severity: LintSeverity::Warning,
            code: "fnx-strict-fallback".to_string(),
            message: "Strict fallback mode may fail unexpectedly on large graphs or resource constraints.".to_string(),
            recommendation: "Use --fnx-fallback graceful unless FNX is mandatory for your use case.".to_string(),
        });
    }

    // Check: Directed projection requested but not supported
    if input.directed_projection_requested {
        result.warnings.push(ConfigLintWarning {
            severity: LintSeverity::Warning,
            code: "fnx-directed-unsupported".to_string(),
            message: "Directed graph projection is not yet supported.".to_string(),
            recommendation: "Using undirected projection. Direction information is preserved in layout.".to_string(),
        });
    }

    // Check: FNX strict mode (future)
    if matches!(input.fnx_mode, FnxMode::Strict) {
        result.warnings.push(ConfigLintWarning {
            severity: LintSeverity::Info,
            code: "fnx-strict-mode".to_string(),
            message: "FNX strict mode requires FNX participation for all layout decisions.".to_string(),
            recommendation: "Consider advisory mode if strict FNX participation is not required.".to_string(),
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnx_mode_display() {
        assert_eq!(FnxMode::Off.to_string(), "off");
        assert_eq!(FnxMode::Advisory.to_string(), "advisory");
        assert_eq!(
            FnxMode::ExperimentalDirected.to_string(),
            "experimental_directed"
        );
        assert_eq!(FnxMode::Strict.to_string(), "strict");
    }

    #[test]
    fn fnx_mode_is_active() {
        assert!(!FnxMode::Off.is_active());
        assert!(FnxMode::Advisory.is_active());
        assert!(FnxMode::ExperimentalDirected.is_active());
        assert!(FnxMode::Strict.is_active());
    }

    #[test]
    fn projection_mode_display() {
        assert_eq!(ProjectionMode::NativeOnly.to_string(), "native_only");
        assert_eq!(
            ProjectionMode::NativePlusFnxAdvisory.to_string(),
            "native_plus_fnx_advisory"
        );
        assert_eq!(ProjectionMode::FnxPrimary.to_string(), "fnx_primary");
    }

    #[test]
    fn decision_mode_display() {
        assert_eq!(DecisionMode::NativeAuthoritative.to_string(), "native_authoritative");
        assert_eq!(
            DecisionMode::FnxAuthoritativeExperimental.to_string(),
            "fnx_authoritative_experimental"
        );
    }

    #[test]
    fn fallback_reason_display() {
        assert_eq!(FallbackReason::FeatureDisabled.to_string(), "feature_disabled");
        assert_eq!(FallbackReason::Timeout.to_string(), "timeout");
        assert_eq!(FallbackReason::AnalysisError.to_string(), "analysis_error");
    }

    #[test]
    fn pass_fail_reason_is_pass() {
        assert!(PassFailReason::Pass.is_pass());
        assert!(PassFailReason::PassMatchedGolden.is_pass());
        assert!(PassFailReason::PassDeterministic { runs: 5 }.is_pass());
        assert!(!PassFailReason::FailMissingGolden.is_pass());
    }

    #[test]
    fn pass_fail_reason_is_fail() {
        assert!(PassFailReason::FailMissingGolden.is_fail());
        assert!(PassFailReason::FailHashMismatch {
            expected: "a".to_string(),
            actual: "b".to_string()
        }
        .is_fail());
        assert!(!PassFailReason::Pass.is_fail());
        assert!(!PassFailReason::Skipped {
            reason: "test".to_string()
        }
        .is_fail());
    }

    #[test]
    fn fnv1a_hex_deterministic() {
        let hash1 = fnv1a_hex(b"test input");
        let hash2 = fnv1a_hex(b"test input");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 16);
    }

    #[test]
    fn evidence_log_entry_serializes() {
        let entry = EvidenceLogEntry {
            scenario_id: "flowchart_simple".to_string(),
            input_hash: fnv1a_hex(b"graph TD; A-->B"),
            fnx_mode: FnxMode::Advisory,
            projection_mode: ProjectionMode::NativePlusFnxAdvisory,
            decision_mode: DecisionMode::NativeAuthoritative,
            fnx_algorithm: Some("degree_centrality".to_string()),
            parse_ms: 1.5,
            analysis_ms: 2.3,
            layout_ms: 10.0,
            render_ms: 5.0,
            diagnostic_count: 0,
            fallback_reason: None,
            witness_hash: Some(fnv1a_hex(b"witness")),
            output_hash: fnv1a_hex(b"output"),
            pass_fail_reason: PassFailReason::Pass,
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        assert!(json.contains("flowchart_simple"));
        assert!(json.contains("advisory"));
        assert!(json.contains("native_authoritative"));
    }

    #[test]
    fn evidence_bundle_aggregates_stats() {
        let mut bundle = EvidenceBundle::new(
            Some("abc123".to_string()),
            "test",
            FnxFeatures {
                fnx_integration: true,
                fnx_experimental_directed: false,
            },
        );
        bundle.add_entry(EvidenceLogEntry {
            scenario_id: "test1".to_string(),
            input_hash: "a".to_string(),
            fnx_mode: FnxMode::Off,
            projection_mode: ProjectionMode::NativeOnly,
            decision_mode: DecisionMode::NativeAuthoritative,
            fnx_algorithm: None,
            parse_ms: 1.0,
            analysis_ms: 0.0,
            layout_ms: 2.0,
            render_ms: 1.0,
            diagnostic_count: 0,
            fallback_reason: None,
            witness_hash: None,
            output_hash: "b".to_string(),
            pass_fail_reason: PassFailReason::Pass,
        });
        bundle.add_entry(EvidenceLogEntry {
            scenario_id: "test2".to_string(),
            input_hash: "c".to_string(),
            fnx_mode: FnxMode::Off,
            projection_mode: ProjectionMode::NativeOnly,
            decision_mode: DecisionMode::NativeAuthoritative,
            fnx_algorithm: None,
            parse_ms: 2.0,
            analysis_ms: 0.0,
            layout_ms: 3.0,
            render_ms: 1.5,
            diagnostic_count: 1,
            fallback_reason: None,
            witness_hash: None,
            output_hash: "d".to_string(),
            pass_fail_reason: PassFailReason::FailMissingGolden,
        });
        assert_eq!(bundle.summary.total, 2);
        assert_eq!(bundle.summary.passed, 1);
        assert_eq!(bundle.summary.failed, 1);
        assert!((bundle.summary.total_parse_ms - 3.0).abs() < f64::EPSILON);
        assert!((bundle.summary.total_layout_ms - 5.0).abs() < f64::EPSILON);
        assert_eq!(bundle.summary.total_diagnostics, 1);
    }

    // ========================================================================
    // Config Lint Tests
    // ========================================================================

    #[test]
    fn lint_fnx_config_clean() {
        let input = FnxConfigLintInput {
            fnx_mode: FnxMode::Advisory,
            projection_mode: ProjectionMode::NativePlusFnxAdvisory,
            fnx_available: true,
            is_wasm: false,
            strict_fallback: false,
            ..Default::default()
        };
        let result = lint_fnx_config(&input);
        assert!(!result.has_errors());
        assert!(!result.has_warnings());
    }

    #[test]
    fn lint_fnx_config_disabled_clean() {
        let input = FnxConfigLintInput {
            fnx_mode: FnxMode::Off,
            projection_mode: ProjectionMode::NativeOnly,
            fnx_available: false,
            is_wasm: true,
            strict_fallback: true,
            ..Default::default()
        };
        let result = lint_fnx_config(&input);
        // Off mode should not trigger any warnings even with bad combinations
        assert!(!result.has_errors());
        assert!(!result.has_warnings());
    }

    #[test]
    fn lint_fnx_config_unavailable() {
        let input = FnxConfigLintInput {
            fnx_mode: FnxMode::Advisory,
            projection_mode: ProjectionMode::NativePlusFnxAdvisory,
            fnx_available: false, // FNX not available
            is_wasm: false,
            strict_fallback: false,
            ..Default::default()
        };
        let result = lint_fnx_config(&input);
        assert!(result.has_errors());
        assert!(result.warnings.iter().any(|w| w.code == "fnx-unavailable"));
    }

    #[test]
    fn lint_fnx_config_wasm() {
        let input = FnxConfigLintInput {
            fnx_mode: FnxMode::Advisory,
            projection_mode: ProjectionMode::NativePlusFnxAdvisory,
            fnx_available: true,
            is_wasm: true, // WASM environment
            strict_fallback: false,
            ..Default::default()
        };
        let result = lint_fnx_config(&input);
        assert!(!result.has_errors());
        assert!(result.has_warnings());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.code == "fnx-wasm-unsupported"));
    }

    #[test]
    fn lint_fnx_config_strict_fallback() {
        let input = FnxConfigLintInput {
            fnx_mode: FnxMode::Advisory,
            projection_mode: ProjectionMode::NativePlusFnxAdvisory,
            fnx_available: true,
            is_wasm: false,
            strict_fallback: true, // Strict fallback
            ..Default::default()
        };
        let result = lint_fnx_config(&input);
        assert!(!result.has_errors());
        assert!(result.has_warnings());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.code == "fnx-strict-fallback"));
    }

    #[test]
    fn lint_fnx_config_directed_projection() {
        let input = FnxConfigLintInput {
            fnx_mode: FnxMode::Advisory,
            projection_mode: ProjectionMode::NativePlusFnxAdvisory,
            fnx_available: true,
            is_wasm: false,
            strict_fallback: false,
            directed_projection_requested: true, // Directed projection requested
        };
        let result = lint_fnx_config(&input);
        assert!(!result.has_errors());
        assert!(result.has_warnings());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.code == "fnx-directed-unsupported"));
    }

    #[test]
    fn lint_fnx_config_strict_mode() {
        let input = FnxConfigLintInput {
            fnx_mode: FnxMode::Strict,
            projection_mode: ProjectionMode::FnxPrimary,
            fnx_available: true,
            is_wasm: false,
            strict_fallback: false,
            ..Default::default()
        };
        let result = lint_fnx_config(&input);
        assert!(!result.has_errors());
        assert!(result.has_warnings());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.code == "fnx-strict-mode"));
    }
}
