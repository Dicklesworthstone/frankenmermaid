//! FNX Differential Quality/Performance Report (bd-ml2r.12.2)
//!
//! Generates systematic FNX-on vs FNX-off differential reports with defined
//! acceptance thresholds. Compares quality metrics, diagnostics changes, and
//! latency across modes.
//!
//! # Classification
//!
//! Deltas are classified as:
//! - **ExpectedImprovement**: FNX-on improves on baseline (e.g., fewer crossings)
//! - **Neutral**: No significant difference between modes
//! - **Regression**: FNX-on worse than baseline (fails threshold)

use fm_core::evidence;
use fm_layout::{LayoutConfig, layout_diagram_with_config};
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

const TIMING_RUNS: usize = 3;

/// Differential report version for tracking threshold policy changes.
const DIFFERENTIAL_REPORT_VERSION: &str = "1.0.0";

// ============================================================================
// Threshold Policy (versioned, explicit)
// ============================================================================

/// Threshold policy for differential analysis.
/// Changes to these values should be versioned and documented.
mod thresholds {
    /// Maximum acceptable layout time regression (percentage).
    /// FNX-on can be at most 50% slower than FNX-off.
    pub const MAX_LAYOUT_TIME_REGRESSION_PCT: f64 = 50.0;

    /// Maximum acceptable render time regression (percentage).
    /// Set high because render times are typically small (microseconds) and have high variance.
    pub const MAX_RENDER_TIME_REGRESSION_PCT: f64 = 100.0;

    /// Layout bounds must not differ by more than this percentage.
    pub const MAX_BOUNDS_DELTA_PCT: f64 = 1.0;

    /// Node/edge counts must be identical (structural invariant).
    pub const REQUIRE_IDENTICAL_STRUCTURE: bool = true;

    /// Maximum acceptable edge crossing increase.
    pub const MAX_CROSSING_REGRESSION: i32 = 2;

    /// Minimum improvement in crossings to count as "expected improvement".
    pub const MIN_CROSSING_IMPROVEMENT: i32 = 1;

    /// Output hash may differ (FNX adds classes) but structure must be stable.
    #[allow(dead_code)] // Reserved for future hash-comparison checks
    pub const ALLOW_OUTPUT_HASH_DIFFERENCE: bool = true;
}

// ============================================================================
// Data Structures
// ============================================================================

/// Classification of a delta between FNX-on and FNX-off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeltaClassification {
    ExpectedImprovement,
    Neutral,
    Regression,
}

impl std::fmt::Display for DeltaClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExpectedImprovement => write!(f, "expected_improvement"),
            Self::Neutral => write!(f, "neutral"),
            Self::Regression => write!(f, "regression"),
        }
    }
}

/// Metrics captured for a single scenario run.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used in Debug output and JSON serialization
struct ScenarioMetrics {
    scenario_id: String,
    fnx_mode: String,
    parse_us: u64,
    layout_us: u64,
    render_us: u64,
    output_hash: String,
    node_count: usize,
    edge_count: usize,
    layout_width: f32,
    layout_height: f32,
    warning_count: usize,
    edge_crossings: usize,
}

/// Differential report for a single scenario comparing FNX-on vs FNX-off.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used in Debug output and JSON serialization
struct DifferentialReport {
    scenario_id: String,
    input_hash: String,
    fnx_off: ScenarioMetrics,
    fnx_on: ScenarioMetrics,

    // Computed deltas
    layout_time_delta_pct: f64,
    render_time_delta_pct: f64,
    bounds_width_delta_pct: f64,
    bounds_height_delta_pct: f64,
    crossing_delta: i32,

    // Classifications
    timing_classification: DeltaClassification,
    quality_classification: DeltaClassification,
    structure_classification: DeltaClassification,
    overall_classification: DeltaClassification,

    // Gate pass/fail
    passes_gate: bool,
    failure_reasons: Vec<String>,
}

// ============================================================================
// Metrics Collection
// ============================================================================

fn run_scenario(input: &str, scenario_id: &str, fnx_enabled: bool) -> ScenarioMetrics {
    let parse_start = Instant::now();
    let parsed = parse(input);
    let parse_us = parse_start.elapsed().as_micros() as u64;

    let svg_config = SvgRenderConfig {
        node_gradients: false,
        glow_enabled: false,
        cluster_fill_opacity: 1.0,
        inactive_opacity: 1.0,
        shadow_blur: 3.0,
        shadow_color: String::new(),
        ..Default::default()
    };

    let layout_config = LayoutConfig {
        font_metrics: Some(svg_config.font_metrics()),
        fnx_enabled,
        ..Default::default()
    };

    let layout_start = Instant::now();
    let layout = layout_diagram_with_config(&parsed.ir, layout_config);
    let layout_us = layout_start.elapsed().as_micros() as u64;

    let render_start = Instant::now();
    let svg = render_svg_with_layout(&parsed.ir, &layout, &svg_config);
    let render_us = render_start.elapsed().as_micros() as u64;

    let edge_crossings = count_edge_crossings(&layout);

    ScenarioMetrics {
        scenario_id: scenario_id.to_string(),
        fnx_mode: if fnx_enabled {
            "on".to_string()
        } else {
            "off".to_string()
        },
        parse_us,
        layout_us,
        render_us,
        output_hash: evidence::fnv1a_hex(svg.as_bytes()),
        node_count: parsed.ir.nodes.len(),
        edge_count: parsed.ir.edges.len(),
        layout_width: layout.bounds.width,
        layout_height: layout.bounds.height,
        warning_count: parsed.warnings.len(),
        edge_crossings,
    }
}

/// Run scenario multiple times and return median metrics.
fn run_scenario_median(input: &str, scenario_id: &str, fnx_enabled: bool) -> ScenarioMetrics {
    let mut samples: Vec<ScenarioMetrics> = (0..TIMING_RUNS)
        .map(|_| run_scenario(input, scenario_id, fnx_enabled))
        .collect();

    // Sort by layout time and take median
    samples.sort_by_key(|m| m.layout_us);
    samples.swap_remove(samples.len() / 2)
}

fn count_edge_crossings(layout: &fm_layout::DiagramLayout) -> usize {
    let mut crossings = 0;
    let edges: Vec<_> = layout.edges.iter().collect();

    for i in 0..edges.len() {
        for j in (i + 1)..edges.len() {
            let e1 = &edges[i];
            let e2 = &edges[j];

            if edges_share_endpoint(e1, e2) {
                continue;
            }

            for seg1_idx in 1..e1.points.len() {
                for seg2_idx in 1..e2.points.len() {
                    if segments_intersect(
                        (
                            e1.points[seg1_idx - 1].x as f64,
                            e1.points[seg1_idx - 1].y as f64,
                        ),
                        (e1.points[seg1_idx].x as f64, e1.points[seg1_idx].y as f64),
                        (
                            e2.points[seg2_idx - 1].x as f64,
                            e2.points[seg2_idx - 1].y as f64,
                        ),
                        (e2.points[seg2_idx].x as f64, e2.points[seg2_idx].y as f64),
                    ) {
                        crossings += 1;
                    }
                }
            }
        }
    }
    crossings
}

fn edges_share_endpoint(e1: &fm_layout::LayoutEdgePath, e2: &fm_layout::LayoutEdgePath) -> bool {
    const EPS: f32 = 1.0;
    let e1_start = e1.points.first();
    let e1_end = e1.points.last();
    let e2_start = e2.points.first();
    let e2_end = e2.points.last();

    match (e1_start, e1_end, e2_start, e2_end) {
        (Some(s1), Some(e1), Some(s2), Some(e2)) => {
            points_close(s1, s2, EPS)
                || points_close(s1, e2, EPS)
                || points_close(e1, s2, EPS)
                || points_close(e1, e2, EPS)
        }
        _ => false,
    }
}

fn points_close(a: &fm_layout::LayoutPoint, b: &fm_layout::LayoutPoint, eps: f32) -> bool {
    (a.x - b.x).abs() < eps && (a.y - b.y).abs() < eps
}

fn segments_intersect(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), p4: (f64, f64)) -> bool {
    let d1 = direction(p3, p4, p1);
    let d2 = direction(p3, p4, p2);
    let d3 = direction(p1, p2, p3);
    let d4 = direction(p1, p2, p4);

    const EPS: f64 = 1e-9;

    ((d1 > EPS && d2 < -EPS) || (d1 < -EPS && d2 > EPS))
        && ((d3 > EPS && d4 < -EPS) || (d3 < -EPS && d4 > EPS))
}

fn direction(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)) -> f64 {
    (p3.0 - p1.0) * (p2.1 - p1.1) - (p2.0 - p1.0) * (p3.1 - p1.1)
}

// ============================================================================
// Differential Analysis
// ============================================================================

fn compute_pct_delta(baseline: f64, current: f64) -> f64 {
    if baseline == 0.0 {
        if current == 0.0 {
            0.0
        } else {
            100.0 // Infinite increase from 0
        }
    } else {
        ((current - baseline) / baseline) * 100.0
    }
}

fn generate_differential_report(input: &str, scenario_id: &str) -> DifferentialReport {
    let fnx_off = run_scenario_median(input, scenario_id, false);
    let fnx_on = run_scenario_median(input, scenario_id, true);

    let layout_time_delta_pct =
        compute_pct_delta(fnx_off.layout_us as f64, fnx_on.layout_us as f64);
    let render_time_delta_pct =
        compute_pct_delta(fnx_off.render_us as f64, fnx_on.render_us as f64);
    let bounds_width_delta_pct =
        compute_pct_delta(fnx_off.layout_width as f64, fnx_on.layout_width as f64);
    let bounds_height_delta_pct =
        compute_pct_delta(fnx_off.layout_height as f64, fnx_on.layout_height as f64);
    let crossing_delta = fnx_on.edge_crossings as i32 - fnx_off.edge_crossings as i32;

    // Classify timing
    let timing_classification = if layout_time_delta_pct
        > thresholds::MAX_LAYOUT_TIME_REGRESSION_PCT
        || render_time_delta_pct > thresholds::MAX_RENDER_TIME_REGRESSION_PCT
    {
        DeltaClassification::Regression
    } else if layout_time_delta_pct < -10.0 {
        DeltaClassification::ExpectedImprovement
    } else {
        DeltaClassification::Neutral
    };

    // Classify quality (crossings)
    let quality_classification = if crossing_delta > thresholds::MAX_CROSSING_REGRESSION {
        DeltaClassification::Regression
    } else if crossing_delta < -thresholds::MIN_CROSSING_IMPROVEMENT {
        DeltaClassification::ExpectedImprovement
    } else {
        DeltaClassification::Neutral
    };

    // Classify structure
    let structure_mismatch = thresholds::REQUIRE_IDENTICAL_STRUCTURE
        && (fnx_off.node_count != fnx_on.node_count || fnx_off.edge_count != fnx_on.edge_count);
    let bounds_exceeded = bounds_width_delta_pct.abs() > thresholds::MAX_BOUNDS_DELTA_PCT
        || bounds_height_delta_pct.abs() > thresholds::MAX_BOUNDS_DELTA_PCT;
    let structure_classification = if structure_mismatch || bounds_exceeded {
        DeltaClassification::Regression
    } else {
        DeltaClassification::Neutral
    };

    // Overall classification
    let overall_classification = if timing_classification == DeltaClassification::Regression
        || quality_classification == DeltaClassification::Regression
        || structure_classification == DeltaClassification::Regression
    {
        DeltaClassification::Regression
    } else if timing_classification == DeltaClassification::ExpectedImprovement
        || quality_classification == DeltaClassification::ExpectedImprovement
    {
        DeltaClassification::ExpectedImprovement
    } else {
        DeltaClassification::Neutral
    };

    // Collect failure reasons
    let mut failure_reasons = Vec::new();

    if layout_time_delta_pct > thresholds::MAX_LAYOUT_TIME_REGRESSION_PCT {
        failure_reasons.push(format!(
            "layout_time_regression: {:.1}% > {:.1}% threshold",
            layout_time_delta_pct,
            thresholds::MAX_LAYOUT_TIME_REGRESSION_PCT
        ));
    }
    if render_time_delta_pct > thresholds::MAX_RENDER_TIME_REGRESSION_PCT {
        failure_reasons.push(format!(
            "render_time_regression: {:.1}% > {:.1}% threshold",
            render_time_delta_pct,
            thresholds::MAX_RENDER_TIME_REGRESSION_PCT
        ));
    }
    if crossing_delta > thresholds::MAX_CROSSING_REGRESSION {
        failure_reasons.push(format!(
            "crossing_regression: +{} > {} threshold",
            crossing_delta,
            thresholds::MAX_CROSSING_REGRESSION
        ));
    }
    if thresholds::REQUIRE_IDENTICAL_STRUCTURE && fnx_off.node_count != fnx_on.node_count {
        failure_reasons.push(format!(
            "node_count_mismatch: {} vs {}",
            fnx_off.node_count, fnx_on.node_count
        ));
    }
    if thresholds::REQUIRE_IDENTICAL_STRUCTURE && fnx_off.edge_count != fnx_on.edge_count {
        failure_reasons.push(format!(
            "edge_count_mismatch: {} vs {}",
            fnx_off.edge_count, fnx_on.edge_count
        ));
    }
    if bounds_width_delta_pct.abs() > thresholds::MAX_BOUNDS_DELTA_PCT {
        failure_reasons.push(format!(
            "bounds_width_delta: {:.2}% > {:.1}% threshold",
            bounds_width_delta_pct.abs(),
            thresholds::MAX_BOUNDS_DELTA_PCT
        ));
    }
    if bounds_height_delta_pct.abs() > thresholds::MAX_BOUNDS_DELTA_PCT {
        failure_reasons.push(format!(
            "bounds_height_delta: {:.2}% > {:.1}% threshold",
            bounds_height_delta_pct.abs(),
            thresholds::MAX_BOUNDS_DELTA_PCT
        ));
    }

    let passes_gate = failure_reasons.is_empty();

    DifferentialReport {
        scenario_id: scenario_id.to_string(),
        input_hash: evidence::fnv1a_hex(input.as_bytes()),
        fnx_off,
        fnx_on,
        layout_time_delta_pct,
        render_time_delta_pct,
        bounds_width_delta_pct,
        bounds_height_delta_pct,
        crossing_delta,
        timing_classification,
        quality_classification,
        structure_classification,
        overall_classification,
        passes_gate,
        failure_reasons,
    }
}

// ============================================================================
// Test Utilities
// ============================================================================

fn golden_dir() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

fn load_golden_cases() -> Vec<(String, String)> {
    let dir = golden_dir();
    let mut cases = Vec::new();

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str()
                && name.ends_with(".mmd")
                && let Ok(content) = fs::read_to_string(entry.path())
            {
                let case_id = name.trim_end_matches(".mmd").to_string();
                cases.push((case_id, content));
            }
        }
    }

    cases.sort_by(|a, b| a.0.cmp(&b.0));
    cases
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn differential_all_golden_cases_pass_gate() {
    let cases = load_golden_cases();
    assert!(!cases.is_empty(), "no golden cases found");

    let mut failures = Vec::new();

    for (case_id, input) in &cases {
        let report = generate_differential_report(input, case_id);

        if !report.passes_gate {
            failures.push(format!(
                "{case_id}: {}\n  reasons: {:?}",
                report.overall_classification, report.failure_reasons
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Differential gate failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn differential_structure_invariant_holds() {
    let cases = load_golden_cases();

    for (case_id, input) in &cases {
        let report = generate_differential_report(input, case_id);

        assert_eq!(
            report.fnx_off.node_count, report.fnx_on.node_count,
            "{case_id}: node count must be identical across modes"
        );
        assert_eq!(
            report.fnx_off.edge_count, report.fnx_on.edge_count,
            "{case_id}: edge count must be identical across modes"
        );
    }
}

#[test]
fn differential_layout_bounds_stable() {
    let cases = load_golden_cases();

    for (case_id, input) in &cases {
        let report = generate_differential_report(input, case_id);

        assert!(
            report.bounds_width_delta_pct.abs() <= thresholds::MAX_BOUNDS_DELTA_PCT,
            "{case_id}: layout width delta {:.2}% exceeds threshold {:.1}%",
            report.bounds_width_delta_pct.abs(),
            thresholds::MAX_BOUNDS_DELTA_PCT
        );
        assert!(
            report.bounds_height_delta_pct.abs() <= thresholds::MAX_BOUNDS_DELTA_PCT,
            "{case_id}: layout height delta {:.2}% exceeds threshold {:.1}%",
            report.bounds_height_delta_pct.abs(),
            thresholds::MAX_BOUNDS_DELTA_PCT
        );
    }
}

#[test]
fn differential_no_severe_timing_regression() {
    let cases = load_golden_cases();

    for (case_id, input) in &cases {
        let report = generate_differential_report(input, case_id);

        assert!(
            report.layout_time_delta_pct <= thresholds::MAX_LAYOUT_TIME_REGRESSION_PCT,
            "{case_id}: layout time regression {:.1}% exceeds threshold {:.1}%",
            report.layout_time_delta_pct,
            thresholds::MAX_LAYOUT_TIME_REGRESSION_PCT
        );
    }
}

#[test]
fn differential_crossing_quality_maintained() {
    let cases = load_golden_cases();

    for (case_id, input) in &cases {
        let report = generate_differential_report(input, case_id);

        assert!(
            report.crossing_delta <= thresholds::MAX_CROSSING_REGRESSION,
            "{case_id}: edge crossing regression +{} exceeds threshold {}",
            report.crossing_delta,
            thresholds::MAX_CROSSING_REGRESSION
        );
    }
}

#[test]
fn differential_evidence_log_emitted() {
    let cases = load_golden_cases();

    for (case_id, input) in &cases {
        let report = generate_differential_report(input, case_id);

        let entry = serde_json::json!({
            "report_version": DIFFERENTIAL_REPORT_VERSION,
            "scenario_id": case_id,
            "input_hash": report.input_hash,
            "fnx_off": {
                "layout_us": report.fnx_off.layout_us,
                "render_us": report.fnx_off.render_us,
                "output_hash": report.fnx_off.output_hash,
                "node_count": report.fnx_off.node_count,
                "edge_count": report.fnx_off.edge_count,
                "layout_width": report.fnx_off.layout_width,
                "layout_height": report.fnx_off.layout_height,
                "edge_crossings": report.fnx_off.edge_crossings,
            },
            "fnx_on": {
                "layout_us": report.fnx_on.layout_us,
                "render_us": report.fnx_on.render_us,
                "output_hash": report.fnx_on.output_hash,
                "node_count": report.fnx_on.node_count,
                "edge_count": report.fnx_on.edge_count,
                "layout_width": report.fnx_on.layout_width,
                "layout_height": report.fnx_on.layout_height,
                "edge_crossings": report.fnx_on.edge_crossings,
            },
            "deltas": {
                "layout_time_pct": report.layout_time_delta_pct,
                "render_time_pct": report.render_time_delta_pct,
                "bounds_width_pct": report.bounds_width_delta_pct,
                "bounds_height_pct": report.bounds_height_delta_pct,
                "crossings": report.crossing_delta,
            },
            "classification": {
                "timing": format!("{}", report.timing_classification),
                "quality": format!("{}", report.quality_classification),
                "structure": format!("{}", report.structure_classification),
                "overall": format!("{}", report.overall_classification),
            },
            "passes_gate": report.passes_gate,
            "failure_reasons": report.failure_reasons,
            "threshold_policy_version": DIFFERENTIAL_REPORT_VERSION,
            "surface": "fnx-differential-report",
        });

        println!("{}", serde_json::to_string(&entry).unwrap());
    }
}

#[test]
fn differential_summary_statistics() {
    let cases = load_golden_cases();
    assert!(!cases.is_empty(), "no golden cases found");

    let mut total = 0;
    let mut improvements = 0;
    let mut neutrals = 0;
    let mut regressions = 0;
    let mut gate_passes = 0;

    for (case_id, input) in &cases {
        let report = generate_differential_report(input, case_id);
        total += 1;

        match report.overall_classification {
            DeltaClassification::ExpectedImprovement => improvements += 1,
            DeltaClassification::Neutral => neutrals += 1,
            DeltaClassification::Regression => regressions += 1,
        }

        if report.passes_gate {
            gate_passes += 1;
        }
    }

    let summary = serde_json::json!({
        "report_version": DIFFERENTIAL_REPORT_VERSION,
        "total_scenarios": total,
        "expected_improvements": improvements,
        "neutrals": neutrals,
        "regressions": regressions,
        "gate_passes": gate_passes,
        "gate_pass_rate": (gate_passes as f64 / total as f64) * 100.0,
        "improvement_rate": (improvements as f64 / total as f64) * 100.0,
        "surface": "fnx-differential-summary",
    });

    println!("{}", serde_json::to_string(&summary).unwrap());

    // Summary should show no regressions
    assert_eq!(
        regressions, 0,
        "Expected 0 regressions but found {regressions}"
    );
}

// ============================================================================
// Blessed Differential Baselines (CI-enforced)
// ============================================================================

fn load_expected_differentials() -> Option<BTreeMap<String, serde_json::Value>> {
    let path = golden_dir().join("fnx_differential_baselines.json");
    if !path.exists() {
        return None;
    }

    let content = fs::read_to_string(&path).ok()?;
    let data: serde_json::Value = serde_json::from_str(&content).ok()?;

    let entries = data.get("entries")?.as_object()?;
    let mut baselines = BTreeMap::new();

    for (k, v) in entries {
        baselines.insert(k.clone(), v.clone());
    }

    Some(baselines)
}

fn save_differential_baselines(reports: &BTreeMap<String, DifferentialReport>) {
    let path = golden_dir().join("fnx_differential_baselines.json");

    let entries: BTreeMap<String, serde_json::Value> = reports
        .iter()
        .map(|(k, r)| {
            (
                k.clone(),
                serde_json::json!({
                    "overall_classification": format!("{}", r.overall_classification),
                    "passes_gate": r.passes_gate,
                    "crossing_delta": r.crossing_delta,
                    "fnx_off_output_hash": r.fnx_off.output_hash,
                    "fnx_on_output_hash": r.fnx_on.output_hash,
                }),
            )
        })
        .collect();

    let data = serde_json::json!({
        "version": 1,
        "description": "FNX differential baselines for regression detection. Regenerate with BLESS_DIFFERENTIAL=1.",
        "threshold_policy_version": DIFFERENTIAL_REPORT_VERSION,
        "entries": entries,
    });

    let content = serde_json::to_string_pretty(&data).expect("serialize differentials");
    fs::write(&path, format!("{content}\n")).expect("write differentials file");
    eprintln!("Wrote differential baselines to: {}", path.display());
}

#[test]
fn differential_baselines_stable_or_bless() {
    let bless = std::env::var("BLESS_DIFFERENTIAL").is_ok();
    let cases = load_golden_cases();

    let mut current_reports = BTreeMap::new();
    for (case_id, input) in &cases {
        let report = generate_differential_report(input, case_id);
        current_reports.insert(case_id.clone(), report);
    }

    if bless {
        save_differential_baselines(&current_reports);
        return;
    }

    let expected = match load_expected_differentials() {
        Some(e) => e,
        None => {
            eprintln!(
                "No differential baseline file found. Run with BLESS_DIFFERENTIAL=1 to create."
            );
            return; // Don't fail if no baseline file exists yet
        }
    };

    let mut mismatches = Vec::new();
    let mut new_cases = Vec::new();

    for (case_id, report) in &current_reports {
        match expected.get(case_id) {
            Some(expected_entry) => {
                // Check gate status consistency
                let expected_passes = expected_entry
                    .get("passes_gate")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                if report.passes_gate != expected_passes {
                    mismatches.push(format!(
                        "{case_id}: gate status changed from {} to {}",
                        expected_passes, report.passes_gate
                    ));
                }

                // Check classification consistency
                let expected_class = expected_entry
                    .get("overall_classification")
                    .and_then(|v| v.as_str())
                    .unwrap_or("neutral");
                let current_class = format!("{}", report.overall_classification);

                if current_class != expected_class {
                    // Allow neutral -> improvement transitions (not regressions)
                    if !(expected_class == "neutral" && current_class == "expected_improvement") {
                        mismatches.push(format!(
                            "{case_id}: classification changed from {} to {}",
                            expected_class, current_class
                        ));
                    }
                }
            }
            None => {
                // New case not in blessed baselines
                new_cases.push(case_id.clone());
            }
        }
    }

    if !new_cases.is_empty() {
        eprintln!(
            "Warning: {} new case(s) not in differential baselines: {:?}. Run BLESS_DIFFERENTIAL=1 to add.",
            new_cases.len(),
            new_cases
        );
    }

    assert!(
        mismatches.is_empty(),
        "Differential baseline mismatches (run BLESS_DIFFERENTIAL=1 to update):\n{}",
        mismatches.join("\n")
    );
}

// ============================================================================
// Specific Scenario Tests
// ============================================================================

#[test]
fn differential_diamond_graph_quality() {
    let input = r#"
flowchart TD
    A --> B
    A --> C
    A --> D
    B --> E
    C --> E
    D --> E
"#;

    let report = generate_differential_report(input, "diamond_graph");

    // Diamond graph should have stable structure
    assert_eq!(report.fnx_off.node_count, report.fnx_on.node_count);
    assert_eq!(report.fnx_off.edge_count, report.fnx_on.edge_count);

    // FNX should not make crossings worse
    assert!(
        report.crossing_delta <= 0,
        "FNX should not increase crossings in diamond graph, got delta {}",
        report.crossing_delta
    );
}

#[test]
fn differential_hub_spoke_quality() {
    let input = r#"
flowchart TD
    Hub[Central Hub]
    A --> Hub
    B --> Hub
    C --> Hub
    D --> Hub
    Hub --> E
    Hub --> F
    Hub --> G
    Hub --> H
"#;

    let report = generate_differential_report(input, "hub_spoke");

    // Hub-spoke should have no crossings in either mode
    assert_eq!(
        report.fnx_off.edge_crossings, 0,
        "Hub-spoke FNX-off should have 0 crossings"
    );
    assert_eq!(
        report.fnx_on.edge_crossings, 0,
        "Hub-spoke FNX-on should have 0 crossings"
    );

    assert!(report.passes_gate, "Hub-spoke should pass gate");
}

#[test]
fn differential_chain_performance() {
    // Long chain should not have significant timing regression
    let mut input = "flowchart LR\n".to_string();
    for i in 0..50 {
        if i < 49 {
            input.push_str(&format!("    N{} --> N{}\n", i, i + 1));
        }
    }

    let report = generate_differential_report(&input, "long_chain");

    assert!(
        report.layout_time_delta_pct <= thresholds::MAX_LAYOUT_TIME_REGRESSION_PCT,
        "Long chain layout time regression {:.1}% exceeds threshold",
        report.layout_time_delta_pct
    );
}
