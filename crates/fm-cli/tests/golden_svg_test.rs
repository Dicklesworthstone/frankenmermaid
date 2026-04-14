//! Golden snapshot harness for SVG rendering determinism and stability.

use fm_layout::layout_diagram;
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_config};
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const CASE_IDS: &[&str] = &[
    "dense_flowchart_stress",
    "flowchart_simple",
    "flowchart_cycle",
    "cycle_braid",
    "cycle_feedback",
    "cycle_ladder",
    "cycle_scc_heavy",
    "fuzzy_keyword_recovery",
    "sequence_basic",
    "sequence_advanced",
    "class_basic",
    "state_basic",
    "state_composite",
    "gantt_basic",
    "pie_basic",
    "malformed_recovery",
    "er_basic",
    "quadrant_basic",
    "gitgraph_basic",
    "xychart_basic",
    "xychart_comprehensive",
    "mindmap_basic",
    "timeline_basic",
    "all_node_shapes",
    "all_edge_types",
    "requirement_basic",
    "c4_basic",
    "stress_120_nodes",
    "empty_diagram",
    "single_node",
    "kanban_basic",
    "packet_basic",
    "architecture_basic",
    "journey_basic",
    "sankey_basic",
    "block_basic",
];

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn resilience_suite_path() -> PathBuf {
    repo_root()
        .join("evidence")
        .join("demo_resilience_fixture_suite.json")
}

#[derive(Debug, Deserialize)]
struct ResilienceSuite {
    scenarios: Vec<ResilienceScenario>,
}

#[derive(Debug, Deserialize)]
struct ResilienceScenario {
    scenario_id: String,
    input_path: String,
    svg_path: String,
    expected_warning_substrings: Vec<String>,
    min_warning_count: usize,
    max_warning_count: usize,
    expected_degradation_tier: String,
    min_node_count: usize,
    min_edge_count: usize,
}

fn load_resilience_suite() -> ResilienceSuite {
    let path = resilience_suite_path();
    let content = fs::read_to_string(&path)
        .map_err(|err| format!("failed reading {}: {err}", path.display()))
        .expect("read resilience suite");
    serde_json::from_str(&content)
        .map_err(|err| format!("failed parsing {}: {err}", path.display()))
        .expect("parse resilience suite")
}

fn resilience_expectation(case_id: &str) -> Option<ResilienceScenario> {
    load_resilience_suite()
        .scenarios
        .into_iter()
        .find(|scenario| scenario.scenario_id == case_id)
}

fn normalize_svg(svg: &str) -> String {
    let mut normalized = svg.replace("\r\n", "\n");
    if !normalized.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn fnv_hex(value: &str) -> String {
    format!("{:016x}", fnv1a_64(value.as_bytes()))
}

fn run_case(case_id: &str, bless: bool) {
    let base = golden_dir();
    let input_path = base.join(format!("{case_id}.mmd"));
    let expected_path = base.join(format!("{case_id}.svg"));

    let input = fs::read_to_string(&input_path)
        .map_err(|err| format!("failed reading {}: {err}", input_path.display()))
        .expect("read golden svg input");

    let parse_start = Instant::now();
    let parsed = parse(&input);
    let parse_ms = parse_start.elapsed().as_millis();

    let layout_start = Instant::now();
    let layout = layout_diagram(&parsed.ir);
    let layout_ms = layout_start.elapsed().as_millis();

    // Keep golden fixtures focused on structural rendering stability.
    // Visual-effect defaults evolve frequently; pinning these values avoids noisy churn.
    let config = SvgRenderConfig {
        node_gradients: false,
        glow_enabled: false,
        cluster_fill_opacity: 1.0,
        inactive_opacity: 1.0,
        shadow_blur: 3.0,
        shadow_color: String::new(),
        ..Default::default()
    };
    let config_hash = fnv_hex(&format!("{config:?}"));
    let input_hash = fnv_hex(&input);

    let render_start = Instant::now();
    let rendered = render_svg_with_config(&parsed.ir, &config);
    let render_ms = render_start.elapsed().as_millis();
    let rendered = normalize_svg(&rendered);
    let output_hash = fnv_hex(&rendered);
    let degradation_tier = if parsed.warnings.is_empty() {
        "full"
    } else {
        "degraded"
    };

    let rerender = normalize_svg(&render_svg_with_config(&parsed.ir, &config));
    assert_eq!(
        rendered, rerender,
        "determinism violation for case {case_id}"
    );

    if bless {
        fs::create_dir_all(&base)
            .map_err(|err| format!("failed creating {}: {err}", base.display()))
            .expect("create golden svg directory");
        fs::write(&expected_path, &rendered)
            .map_err(|err| format!("failed writing {}: {err}", expected_path.display()))
            .expect("write golden svg snapshot");
    }

    let expected = fs::read_to_string(&expected_path)
        .map_err(|err| {
            format!(
                "missing golden snapshot {} ({err}). run with BLESS=1 to generate",
                expected_path.display()
            )
        })
        .expect("read golden svg snapshot");
    let expected = normalize_svg(&expected);
    let expected_hash = fnv_hex(&expected);

    assert_eq!(
        output_hash, expected_hash,
        "FNV hash mismatch for case {case_id}"
    );
    assert_eq!(
        rendered, expected,
        "golden snapshot content mismatch for case {case_id}"
    );

    if let Some(expectation) = resilience_expectation(case_id) {
        assert!(
            parsed.warnings.len() >= expectation.min_warning_count,
            "expected at least {} warnings for {case_id}, got {:?}",
            expectation.min_warning_count,
            parsed.warnings
        );
        assert!(
            parsed.warnings.len() <= expectation.max_warning_count,
            "expected at most {} warnings for {case_id}, got {:?}",
            expectation.max_warning_count,
            parsed.warnings
        );
        assert_eq!(
            degradation_tier, expectation.expected_degradation_tier,
            "unexpected degradation tier for {case_id}"
        );
        assert!(
            parsed.ir.nodes.len() >= expectation.min_node_count,
            "expected at least {} nodes for {case_id}, got {}",
            expectation.min_node_count,
            parsed.ir.nodes.len()
        );
        assert!(
            parsed.ir.edges.len() >= expectation.min_edge_count,
            "expected at least {} edges for {case_id}, got {}",
            expectation.min_edge_count,
            parsed.ir.edges.len()
        );
        for fragment in expectation.expected_warning_substrings {
            assert!(
                parsed
                    .warnings
                    .iter()
                    .any(|warning| warning.contains(&fragment)),
                "expected warning containing '{fragment}' for {case_id}, got {:?}",
                parsed.warnings
            );
        }
    }

    let evidence = json!({
        "scenario_id": case_id,
        "input_hash": input_hash,
        "surface": "cli-integration",
        "renderer": "svg",
        "theme": "default",
        "config_hash": config_hash,
        "parse_ms": parse_ms,
        "layout_ms": layout_ms,
        "render_ms": render_ms,
        "node_count": parsed.ir.nodes.len(),
        "edge_count": parsed.ir.edges.len(),
        "layout_width": layout.bounds.width,
        "layout_height": layout.bounds.height,
        "diagnostic_count": parsed.warnings.len(),
        "degradation_tier": degradation_tier,
        "output_artifact_hash": output_hash,
        "pass_fail_reason": if bless { "bless-updated" } else { "matched-golden" },
    });
    println!("{evidence}");
}

fn selected_case_ids() -> Vec<&'static str> {
    let filter = std::env::var("FM_GOLDEN_CASE").ok();
    match filter.as_deref() {
        Some(case_id) => {
            let selected: Vec<&'static str> = CASE_IDS
                .iter()
                .copied()
                .filter(|candidate| candidate == &case_id)
                .collect();
            assert!(
                !selected.is_empty(),
                "FM_GOLDEN_CASE {case_id} is not a known golden case id"
            );
            selected
        }
        None => CASE_IDS.to_vec(),
    }
}

#[test]
fn svg_golden_snapshots_are_stable() {
    let bless = std::env::var("BLESS").is_ok_and(|v| v == "1");
    for case_id in selected_case_ids() {
        run_case(case_id, bless);
    }
}

#[test]
fn resilience_suite_manifest_matches_checked_in_fixtures() {
    let manifest = load_resilience_suite();
    let bless = std::env::var("BLESS").is_ok_and(|v| v == "1");
    let expected_base = repo_root().join("crates/fm-cli/tests/golden");

    for scenario in manifest.scenarios {
        assert!(
            CASE_IDS.contains(&scenario.scenario_id.as_str()),
            "scenario {} must be covered by the SVG golden harness",
            scenario.scenario_id
        );

        let input_path = repo_root().join(&scenario.input_path);
        let svg_path = repo_root().join(&scenario.svg_path);
        let expected_input_path = expected_base.join(format!("{}.mmd", scenario.scenario_id));
        let expected_svg_path = expected_base.join(format!("{}.svg", scenario.scenario_id));

        assert_eq!(
            input_path, expected_input_path,
            "scenario {} input_path must point at the canonical golden fixture",
            scenario.scenario_id
        );
        assert_eq!(
            svg_path, expected_svg_path,
            "scenario {} svg_path must point at the canonical golden fixture",
            scenario.scenario_id
        );

        assert!(
            input_path.exists(),
            "missing fixture {}",
            input_path.display()
        );
        if !bless {
            assert!(svg_path.exists(), "missing fixture {}", svg_path.display());
        }
    }
}
