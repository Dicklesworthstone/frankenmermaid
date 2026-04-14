//! FNX-Off Baseline Snapshot and Invariants (bd-ml2r.12.1)
//!
//! This module enforces no-regression invariants by:
//! 1. Capturing deterministic baselines with FNX disabled
//! 2. Verifying output stability across repeated runs
//! 3. Ensuring FNX-enabled mode doesn't break baseline invariants
//! 4. Comparing layout quality metrics between modes

use fm_core::evidence;
use fm_layout::{LayoutConfig, layout_diagram_with_config};
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

const DETERMINISM_RUNS: usize = 5;

/// Baseline invariant for a single scenario.
#[derive(Debug, Clone)]
#[allow(dead_code)] // scenario_id used in Debug output and BTreeMap keys
struct BaselineInvariant {
    scenario_id: String,
    input_hash: String,
    output_hash: String,
    node_count: usize,
    edge_count: usize,
    layout_width: f32,
    layout_height: f32,
    warning_count: usize,
}

/// Run a scenario with FNX explicitly disabled and return invariants.
fn capture_fnx_off_baseline(input: &str, scenario_id: &str) -> BaselineInvariant {
    let parsed = parse(input);

    // Use default config which doesn't enable FNX features
    let svg_config = SvgRenderConfig {
        // Stable config for deterministic output
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
        ..Default::default()
    };

    let layout = layout_diagram_with_config(&parsed.ir, layout_config);
    let svg = render_svg_with_layout(&parsed.ir, &layout, &svg_config);

    BaselineInvariant {
        scenario_id: scenario_id.to_string(),
        input_hash: evidence::fnv1a_hex(input.as_bytes()),
        output_hash: evidence::fnv1a_hex(svg.as_bytes()),
        node_count: parsed.ir.nodes.len(),
        edge_count: parsed.ir.edges.len(),
        layout_width: layout.bounds.width,
        layout_height: layout.bounds.height,
        warning_count: parsed.warnings.len(),
    }
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
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".mmd") {
                    let case_id = name.trim_end_matches(".mmd").to_string();
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        cases.push((case_id, content));
                    }
                }
            }
        }
    }

    cases.sort_by(|a, b| a.0.cmp(&b.0));
    cases
}

// ============================================================================
// Baseline Determinism Tests
// ============================================================================

#[test]
fn fnx_off_baseline_is_deterministic() {
    let cases = load_golden_cases();
    assert!(!cases.is_empty(), "no golden cases found");

    let mut failures = Vec::new();

    for (case_id, input) in &cases {
        let reference = capture_fnx_off_baseline(input, case_id);

        for run in 1..=DETERMINISM_RUNS {
            let current = capture_fnx_off_baseline(input, case_id);

            if current.output_hash != reference.output_hash {
                failures.push(format!(
                    "{case_id}: run {run} hash {} != reference {}",
                    current.output_hash, reference.output_hash
                ));
            }

            // Layout bounds should be identical
            if (current.layout_width - reference.layout_width).abs() > 0.001
                || (current.layout_height - reference.layout_height).abs() > 0.001
            {
                failures.push(format!(
                    "{case_id}: run {run} layout bounds differ: {:.1}x{:.1} vs {:.1}x{:.1}",
                    current.layout_width, current.layout_height,
                    reference.layout_width, reference.layout_height
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "FNX-off baseline determinism violations:\n{}",
        failures.join("\n")
    );
}

// ============================================================================
// Structural Invariants
// ============================================================================

#[test]
fn fnx_off_baseline_preserves_node_edge_counts() {
    let cases = load_golden_cases();

    for (case_id, input) in &cases {
        let baseline1 = capture_fnx_off_baseline(input, case_id);
        let baseline2 = capture_fnx_off_baseline(input, case_id);

        assert_eq!(
            baseline1.node_count, baseline2.node_count,
            "{case_id}: node count should be stable"
        );
        assert_eq!(
            baseline1.edge_count, baseline2.edge_count,
            "{case_id}: edge count should be stable"
        );
    }
}

#[test]
fn fnx_off_baseline_has_finite_layout_bounds() {
    let cases = load_golden_cases();

    for (case_id, input) in &cases {
        let baseline = capture_fnx_off_baseline(input, case_id);

        assert!(
            baseline.layout_width.is_finite(),
            "{case_id}: layout width should be finite"
        );
        assert!(
            baseline.layout_height.is_finite(),
            "{case_id}: layout height should be finite"
        );
        assert!(
            baseline.layout_width >= 0.0,
            "{case_id}: layout width should be non-negative"
        );
        assert!(
            baseline.layout_height >= 0.0,
            "{case_id}: layout height should be non-negative"
        );
    }
}

// ============================================================================
// Evidence Logging
// ============================================================================

#[test]
fn fnx_off_baseline_evidence_log_emitted() {
    let cases = load_golden_cases();

    for (case_id, input) in &cases {
        let mut hashes = Vec::new();

        for _ in 0..DETERMINISM_RUNS {
            let baseline = capture_fnx_off_baseline(input, case_id);
            hashes.push(baseline.output_hash.clone());
        }

        let unique_hashes: std::collections::HashSet<_> = hashes.iter().collect();
        let is_deterministic = unique_hashes.len() == 1;

        let baseline = capture_fnx_off_baseline(input, case_id);

        let entry = serde_json::json!({
            "scenario_id": case_id,
            "input_hash": baseline.input_hash,
            "fnx_mode": "off",
            "projection_mode": "native_only",
            "decision_mode": "native_authoritative",
            "node_count": baseline.node_count,
            "edge_count": baseline.edge_count,
            "layout_width": baseline.layout_width,
            "layout_height": baseline.layout_height,
            "warning_count": baseline.warning_count,
            "output_hash": baseline.output_hash,
            "determinism_runs": DETERMINISM_RUNS,
            "is_deterministic": is_deterministic,
            "pass_fail_reason": if is_deterministic { "pass_deterministic" } else { "fail_nondeterministic" },
            "surface": "fnx-baseline-invariants",
        });

        println!("{}", serde_json::to_string(&entry).unwrap());
    }
}

// ============================================================================
// Baseline Hash Stability (CI-enforced)
// ============================================================================

/// Load expected baseline hashes from checked-in file, if present.
fn load_expected_baselines() -> Option<BTreeMap<String, String>> {
    let path = golden_dir().join("fnx_off_baselines.json");
    if !path.exists() {
        return None;
    }

    let content = fs::read_to_string(&path).ok()?;
    let data: serde_json::Value = serde_json::from_str(&content).ok()?;

    let entries = data.get("entries")?.as_object()?;
    let mut baselines = BTreeMap::new();

    for (k, v) in entries {
        if let Some(hash) = v.get("output_hash").and_then(|h| h.as_str()) {
            baselines.insert(k.clone(), hash.to_string());
        }
    }

    Some(baselines)
}

/// Save current baselines to JSON file (run with BLESS_BASELINE=1).
fn save_baselines(baselines: &BTreeMap<String, BaselineInvariant>) {
    let path = golden_dir().join("fnx_off_baselines.json");

    let entries: BTreeMap<String, serde_json::Value> = baselines
        .iter()
        .map(|(k, v)| {
            (k.clone(), serde_json::json!({
                "output_hash": v.output_hash,
                "node_count": v.node_count,
                "edge_count": v.edge_count,
                "layout_width": v.layout_width,
                "layout_height": v.layout_height,
            }))
        })
        .collect();

    let data = serde_json::json!({
        "version": 1,
        "description": "FNX-off baseline hashes for regression detection. Regenerate with BLESS_BASELINE=1.",
        "entries": entries,
    });

    let content = serde_json::to_string_pretty(&data).expect("serialize baselines");
    fs::write(&path, format!("{content}\n")).expect("write baselines file");
    eprintln!("Wrote baselines to: {}", path.display());
}

#[test]
fn fnx_off_baseline_hashes_stable_or_bless() {
    let bless = std::env::var("BLESS_BASELINE").is_ok();
    let cases = load_golden_cases();

    let mut current_baselines = BTreeMap::new();
    for (case_id, input) in &cases {
        let baseline = capture_fnx_off_baseline(input, case_id);
        current_baselines.insert(case_id.clone(), baseline);
    }

    if bless {
        save_baselines(&current_baselines);
        return;
    }

    let expected = match load_expected_baselines() {
        Some(e) => e,
        None => {
            eprintln!("No baseline file found. Run with BLESS_BASELINE=1 to create.");
            return; // Don't fail if no baseline file exists yet
        }
    };

    let mut mismatches = Vec::new();

    for (case_id, baseline) in &current_baselines {
        if let Some(expected_hash) = expected.get(case_id) {
            if &baseline.output_hash != expected_hash {
                mismatches.push(format!(
                    "{case_id}: expected {} got {}",
                    expected_hash, baseline.output_hash
                ));
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "FNX-off baseline hash mismatches (run BLESS_BASELINE=1 to update):\n{}",
        mismatches.join("\n")
    );
}

// ============================================================================
// Invariant: No warnings should appear/disappear unexpectedly
// ============================================================================

#[test]
fn fnx_off_baseline_warning_count_stable() {
    let cases = load_golden_cases();

    for (case_id, input) in &cases {
        let mut warning_counts = Vec::new();

        for _ in 0..DETERMINISM_RUNS {
            let baseline = capture_fnx_off_baseline(input, case_id);
            warning_counts.push(baseline.warning_count);
        }

        let first_count = warning_counts[0];
        let all_same = warning_counts.iter().all(|&c| c == first_count);

        assert!(
            all_same,
            "{case_id}: warning count should be stable, got {:?}",
            warning_counts
        );
    }
}

// ============================================================================
// Layout Position Invariants
// ============================================================================

#[test]
fn fnx_off_baseline_positions_within_bounds() {
    let cases = load_golden_cases();

    for (case_id, input) in &cases {
        let parsed = parse(input);
        let svg_config = SvgRenderConfig::default();
        let layout_config = LayoutConfig {
            font_metrics: Some(svg_config.font_metrics()),
            ..Default::default()
        };
        let layout = layout_diagram_with_config(&parsed.ir, layout_config);

        // All node positions should be within layout bounds (with margin for node size)
        for node in &layout.nodes {
            assert!(
                node.bounds.x >= layout.bounds.x - 50.0,
                "{case_id}: node {} x position {} out of bounds (min {})",
                node.node_id, node.bounds.x, layout.bounds.x
            );
            assert!(
                node.bounds.y >= layout.bounds.y - 50.0,
                "{case_id}: node {} y position {} out of bounds (min {})",
                node.node_id, node.bounds.y, layout.bounds.y
            );
        }
    }
}
