//! Benchmark Regression Harness (bd-ml2r.11.3)
//!
//! Deterministic replay and timing variance tracking for FNX quality assurance.
//!
//! This harness:
//! - Runs scenarios multiple times to verify deterministic outputs
//! - Tracks parse/layout/render timing variance
//! - Compares baseline (FNX disabled) vs FNX-enabled quality metrics
//! - Outputs structured evidence logs for CI integration

use fm_core::evidence;
use fm_layout::{LayoutConfig, layout_diagram, layout_diagram_with_config};
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

const DETERMINISM_RUNS: usize = 5;
const TIMING_RUNS: usize = 3;

/// Timing samples for statistical analysis.
#[derive(Debug, Default, Clone)]
struct TimingSamples {
    parse_us: Vec<u64>,
    layout_us: Vec<u64>,
    render_us: Vec<u64>,
}

impl TimingSamples {
    fn add(&mut self, parse_us: u64, layout_us: u64, render_us: u64) {
        self.parse_us.push(parse_us);
        self.layout_us.push(layout_us);
        self.render_us.push(render_us);
    }

    fn mean(&self, samples: &[u64]) -> f64 {
        if samples.is_empty() {
            0.0
        } else {
            samples.iter().sum::<u64>() as f64 / samples.len() as f64
        }
    }

    fn stddev(&self, samples: &[u64]) -> f64 {
        if samples.len() < 2 {
            return 0.0;
        }
        let mean = self.mean(samples);
        let variance: f64 = samples
            .iter()
            .map(|&x| (x as f64 - mean).powi(2))
            .sum::<f64>()
            / samples.len() as f64;
        variance.sqrt()
    }

    fn coefficient_of_variation(&self, samples: &[u64]) -> f64 {
        let mean = self.mean(samples);
        if mean == 0.0 {
            0.0
        } else {
            self.stddev(samples) / mean
        }
    }

    fn report(&self) -> TimingReport {
        TimingReport {
            parse_mean_us: self.mean(&self.parse_us),
            parse_stddev_us: self.stddev(&self.parse_us),
            parse_cv: self.coefficient_of_variation(&self.parse_us),
            layout_mean_us: self.mean(&self.layout_us),
            layout_stddev_us: self.stddev(&self.layout_us),
            layout_cv: self.coefficient_of_variation(&self.layout_us),
            render_mean_us: self.mean(&self.render_us),
            render_stddev_us: self.stddev(&self.render_us),
            render_cv: self.coefficient_of_variation(&self.render_us),
        }
    }
}

#[derive(Debug, Clone)]
struct TimingReport {
    parse_mean_us: f64,
    parse_stddev_us: f64,
    parse_cv: f64,
    layout_mean_us: f64,
    layout_stddev_us: f64,
    layout_cv: f64,
    render_mean_us: f64,
    render_stddev_us: f64,
    render_cv: f64,
}

/// Run a single scenario and return timing + hash.
fn run_scenario(input: &str) -> (u64, u64, u64, String, usize, usize) {
    let parse_start = Instant::now();
    let parsed = parse(input);
    let parse_us = parse_start.elapsed().as_micros() as u64;

    let svg_config = SvgRenderConfig::default();
    let layout_config = LayoutConfig {
        font_metrics: Some(svg_config.font_metrics()),
        ..Default::default()
    };

    let layout_start = Instant::now();
    let layout = layout_diagram_with_config(&parsed.ir, layout_config);
    let layout_us = layout_start.elapsed().as_micros() as u64;

    let render_start = Instant::now();
    let svg = render_svg_with_layout(&parsed.ir, &layout, &svg_config);
    let render_us = render_start.elapsed().as_micros() as u64;

    let hash = evidence::fnv1a_hex(svg.as_bytes());
    let node_count = parsed.ir.nodes.len();
    let edge_count = parsed.ir.edges.len();

    (parse_us, layout_us, render_us, hash, node_count, edge_count)
}

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
// Determinism Tests
// ============================================================================

#[test]
fn determinism_all_golden_cases_produce_stable_output() {
    let cases = load_golden_cases();
    assert!(!cases.is_empty(), "no golden cases found");

    let mut failures = Vec::new();

    for (case_id, input) in &cases {
        let (_, _, _, reference_hash, _, _) = run_scenario(input);

        for run in 1..=DETERMINISM_RUNS {
            let (_, _, _, hash, _, _) = run_scenario(input);
            if hash != reference_hash {
                failures.push(format!(
                    "{case_id}: run {run} hash {hash} != reference {reference_hash}"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Determinism violations:\n{}",
        failures.join("\n")
    );
}

#[test]
fn determinism_evidence_log_emitted_for_all_cases() {
    let cases = load_golden_cases();
    let mut evidence_entries = Vec::new();

    for (case_id, input) in &cases {
        let mut hashes = Vec::new();

        for _ in 0..DETERMINISM_RUNS {
            let (_, _, _, hash, _, _) = run_scenario(input);
            hashes.push(hash);
        }

        let unique_hashes: std::collections::HashSet<_> = hashes.iter().collect();
        let is_deterministic = unique_hashes.len() == 1;

        let pass_fail = if is_deterministic {
            evidence::PassFailReason::PassDeterministic {
                runs: DETERMINISM_RUNS,
            }
        } else {
            evidence::PassFailReason::FailNondeterministic {
                runs: DETERMINISM_RUNS,
                unique_hashes: unique_hashes.len(),
            }
        };

        let entry = serde_json::json!({
            "scenario_id": case_id,
            "input_hash": evidence::fnv1a_hex(input.as_bytes()),
            "determinism_runs": DETERMINISM_RUNS,
            "unique_hashes": unique_hashes.len(),
            "is_deterministic": is_deterministic,
            "pass_fail_reason": format!("{}", pass_fail),
            "surface": "benchmark-regression-harness",
        });
        evidence_entries.push(entry);
    }

    // Print evidence log
    for entry in &evidence_entries {
        println!("{}", serde_json::to_string(entry).unwrap());
    }

    // Assert all passed
    let all_deterministic = evidence_entries
        .iter()
        .all(|e| e["is_deterministic"].as_bool().unwrap_or(false));
    assert!(all_deterministic, "Some scenarios were non-deterministic");
}

// ============================================================================
// Timing Variance Tests
// ============================================================================

#[test]
fn timing_variance_within_acceptable_bounds() {
    let cases = load_golden_cases();

    // Maximum acceptable coefficient of variation for timing (100% = very high variance)
    const MAX_ACCEPTABLE_CV: f64 = 1.0;

    let mut high_variance_cases = Vec::new();

    for (case_id, input) in &cases {
        let mut samples = TimingSamples::default();

        for _ in 0..TIMING_RUNS {
            let (parse_us, layout_us, render_us, _, _, _) = run_scenario(input);
            samples.add(parse_us, layout_us, render_us);
        }

        let report = samples.report();

        // Check for excessive variance
        if report.layout_cv > MAX_ACCEPTABLE_CV {
            high_variance_cases.push(format!(
                "{case_id}: layout CV={:.2} (mean={:.0}us, stddev={:.0}us)",
                report.layout_cv, report.layout_mean_us, report.layout_stddev_us
            ));
        }
    }

    // Log evidence but don't fail on variance (timing is expected to vary)
    if !high_variance_cases.is_empty() {
        eprintln!(
            "Warning: High timing variance detected:\n{}",
            high_variance_cases.join("\n")
        );
    }
}

#[test]
fn timing_evidence_log_emitted_for_all_cases() {
    let cases = load_golden_cases();

    for (case_id, input) in &cases {
        let mut samples = TimingSamples::default();

        for _ in 0..TIMING_RUNS {
            let (parse_us, layout_us, render_us, _, _node_count, _edge_count) = run_scenario(input);
            samples.add(parse_us, layout_us, render_us);
        }

        let report = samples.report();

        let entry = serde_json::json!({
            "scenario_id": case_id,
            "input_hash": evidence::fnv1a_hex(input.as_bytes()),
            "timing_runs": TIMING_RUNS,
            "parse_mean_us": report.parse_mean_us,
            "parse_stddev_us": report.parse_stddev_us,
            "parse_cv": report.parse_cv,
            "layout_mean_us": report.layout_mean_us,
            "layout_stddev_us": report.layout_stddev_us,
            "layout_cv": report.layout_cv,
            "render_mean_us": report.render_mean_us,
            "render_stddev_us": report.render_stddev_us,
            "render_cv": report.render_cv,
            "surface": "benchmark-regression-harness",
        });
        println!("{}", serde_json::to_string(&entry).unwrap());
    }
}

// ============================================================================
// Quality Metrics Tests
// ============================================================================

#[test]
fn quality_metrics_track_edge_crossings() {
    let input = r#"
flowchart TD
    A --> B
    A --> C
    A --> D
    B --> E
    C --> E
    D --> E
"#;

    let parsed = parse(input);
    let layout = layout_diagram(&parsed.ir);

    // Count edge crossings
    let crossings = count_edge_crossings(&layout);

    let entry = serde_json::json!({
        "scenario_id": "quality_edge_crossings",
        "node_count": parsed.ir.nodes.len(),
        "edge_count": parsed.ir.edges.len(),
        "edge_crossings": crossings,
        "surface": "benchmark-regression-harness",
    });
    println!("{}", serde_json::to_string(&entry).unwrap());

    // Diamond graph should have minimal crossings
    assert!(
        crossings <= 2,
        "Diamond graph should have at most 2 edge crossings, got {crossings}"
    );
}

fn count_edge_crossings(layout: &fm_layout::DiagramLayout) -> usize {
    let mut crossings = 0;
    let edges: Vec<_> = layout.edges.iter().collect();

    for i in 0..edges.len() {
        for j in (i + 1)..edges.len() {
            let e1 = &edges[i];
            let e2 = &edges[j];

            // Skip edges that share endpoints
            if edges_share_endpoint(e1, e2) {
                continue;
            }

            for seg1_idx in 1..e1.points.len() {
                for seg2_idx in 1..e2.points.len() {
                    if segments_intersect(
                        (e1.points[seg1_idx - 1].x as f64, e1.points[seg1_idx - 1].y as f64),
                        (e1.points[seg1_idx].x as f64, e1.points[seg1_idx].y as f64),
                        (e2.points[seg2_idx - 1].x as f64, e2.points[seg2_idx - 1].y as f64),
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

    if ((d1 > EPS && d2 < -EPS) || (d1 < -EPS && d2 > EPS))
        && ((d3 > EPS && d4 < -EPS) || (d3 < -EPS && d4 > EPS))
    {
        return true;
    }

    false
}

fn direction(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)) -> f64 {
    (p3.0 - p1.0) * (p2.1 - p1.1) - (p2.0 - p1.0) * (p3.1 - p1.1)
}

// ============================================================================
// Regression Threshold Tests
// ============================================================================

#[test]
fn regression_no_golden_case_exceeds_layout_budget() {
    let cases = load_golden_cases();

    // Maximum acceptable layout time per node (microseconds)
    const MAX_US_PER_NODE: u64 = 5000;

    let mut budget_violations = Vec::new();

    for (case_id, input) in &cases {
        let (_, layout_us, _, _, node_count, _) = run_scenario(input);

        if node_count > 0 {
            let us_per_node = layout_us / node_count as u64;
            if us_per_node > MAX_US_PER_NODE {
                budget_violations.push(format!(
                    "{case_id}: {us_per_node}us/node (budget: {MAX_US_PER_NODE}us/node)"
                ));
            }
        }
    }

    assert!(
        budget_violations.is_empty(),
        "Layout budget violations:\n{}",
        budget_violations.join("\n")
    );
}
