//! Golden snapshot harness for SVG rendering determinism and stability.

use fm_core::{DiagramType, GanttDate};
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
    "flowchart_classdef",
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
fn gantt_basic_fixture_preserves_task_and_dependency_semantics() {
    let input_path = golden_dir().join("gantt_basic.mmd");
    let input = fs::read_to_string(&input_path)
        .map_err(|err| format!("failed reading {}: {err}", input_path.display()))
        .expect("read gantt fixture");
    let parsed = parse(&input);
    let gantt = parsed
        .ir
        .gantt_meta
        .as_ref()
        .expect("gantt fixture metadata");

    assert_eq!(parsed.ir.diagram_type, DiagramType::Gantt);
    assert_eq!(parsed.ir.nodes.len(), 2);
    assert_eq!(parsed.ir.edges.len(), 1);
    assert_eq!(gantt.title.as_deref(), Some("Roadmap"));
    assert_eq!(gantt.sections.len(), 1);
    assert_eq!(gantt.sections[0].name, "Core");
    assert_eq!(gantt.tasks.len(), 2);
    assert_eq!(gantt.tasks[0].task_id.as_deref(), Some("a1"));
    assert_eq!(
        gantt.tasks[0].start.as_ref(),
        Some(&GanttDate::Absolute("2026-01-01".to_string()))
    );
    assert_eq!(
        gantt.tasks[0].end.as_ref(),
        Some(&GanttDate::DurationDays(3))
    );
    assert_eq!(gantt.tasks[1].task_id.as_deref(), Some("a2"));
    assert_eq!(gantt.tasks[1].depends_on, ["a1"]);
    assert_eq!(
        gantt.tasks[1].end.as_ref(),
        Some(&GanttDate::DurationDays(4))
    );

    let rendered = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());
    assert_eq!(
        rendered
            .matches("fm-gantt-task fm-gantt-task-normal")
            .count(),
        2,
        "both Gantt tasks must render as task bars"
    );
    assert!(rendered.contains(">Design</text>"));
    assert!(rendered.contains(">Build</text>"));
    assert!(rendered.contains("fm-gantt-dependency"));
}

/// Pie slice ANGLES must be proportional to their values (bd-5k51 item 5).
///
/// The byte goldens next door prove the output has not CHANGED; they cannot prove it is RIGHT. A
/// pie rendered with equal slices for 40/30/30, or with a slice sweeping the wrong way, would be
/// blessed and pinned exactly as happily as a correct one — which is how bd-bg07's regression
/// nearly got baked into the fixtures. This asserts the geometry the golden is blind to.
///
/// Reads the rendered arc endpoints and recovers each sweep from them, rather than trusting any
/// number the renderer reports about itself.
#[test]
fn pie_basic_slice_angles_are_proportional_to_values() {
    let input_path = golden_dir().join("pie_basic.mmd");
    let input = fs::read_to_string(&input_path)
        .map_err(|err| format!("failed reading {}: {err}", input_path.display()))
        .expect("read pie fixture");
    let parsed = parse(&input);
    let rendered = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());

    // "M cx cy L x1 y1 A r r 0 <large-arc> 1 x2 y2 Z" — recover the sweep from the two radii.
    let mut sweeps: Vec<(f64, bool)> = Vec::new();
    for segment in rendered.split("<path").skip(1) {
        let Some(d_start) = segment.find(" d=\"") else {
            continue;
        };
        let rest = &segment[d_start + 4..];
        let Some(d_end) = rest.find('"') else { continue };
        let d = &rest[..d_end];
        if !d.contains(" A ") {
            continue;
        }
        let nums: Vec<f64> = d
            .split_whitespace()
            .filter_map(|token| token.parse::<f64>().ok())
            .collect();
        // cx cy x1 y1 r r 0 large sweep x2 y2
        if nums.len() < 11 {
            continue;
        }
        let (cx, cy, x1, y1) = (nums[0], nums[1], nums[2], nums[3]);
        let large_arc = nums[7] != 0.0;
        let (x2, y2) = (nums[9], nums[10]);
        let start = (y1 - cy).atan2(x1 - cx);
        let end = (y2 - cy).atan2(x2 - cx);
        let sweep = (end - start).rem_euclid(std::f64::consts::TAU).to_degrees();
        sweeps.push((sweep, large_arc));
    }

    assert_eq!(
        sweeps.len(),
        3,
        "fixture declares three slices; found {} arc paths",
        sweeps.len()
    );

    // Apples 40, Bananas 30, Berries 30 of 100 -> 144, 108, 108 degrees.
    for (actual, expected) in sweeps.iter().zip([144.0_f64, 108.0, 108.0]) {
        assert!(
            (actual.0 - expected).abs() < 0.05,
            "slice sweep {:.3}deg should be {expected:.3}deg for its share of the total",
            actual.0
        );
    }

    let total: f64 = sweeps.iter().map(|(sweep, _)| sweep).sum();
    assert!(
        (total - 360.0).abs() < 0.05,
        "slices must tile the full circle exactly once, got {total:.3}deg"
    );

    // A slice at or under a half-turn must NOT set the large-arc flag. Getting this backwards
    // draws the complement — a 144deg slice rendered as 216deg — which is a wrong PICTURE that
    // leaves every coordinate in the file looking plausible.
    for (sweep, large_arc) in &sweeps {
        assert_eq!(
            *large_arc,
            *sweep > 180.0,
            "large-arc flag {large_arc} disagrees with a {sweep:.3}deg sweep"
        );
    }
}

/// Sankey flow widths must be proportional to flow values (bd-iicc).
///
/// IGNORED because it currently FAILS against a real defect, filed as bd-e69x: every flow in
/// sankey_basic renders at stroke-width 1.80 although the values span 4.8x (100/50/75/25/120/
/// 55/50/25). Proportional ribbon width IS the sankey diagram — uniform width makes it a generic
/// directed graph — so this is a wrong picture that the byte golden pins as happily as a right one.
///
/// It is committed rather than withheld so the specification is reviewable now and executable
/// later: removing the `#[ignore]` is bd-e69x's acceptance gate. Do not weaken it to make it pass.
#[test]
#[ignore = "fails against the bd-e69x defect: sankey flows render at uniform width"]
fn sankey_basic_flow_widths_are_proportional_to_values() {
    let input_path = golden_dir().join("sankey_basic.mmd");
    let input = fs::read_to_string(&input_path)
        .map_err(|err| format!("failed reading {}: {err}", input_path.display()))
        .expect("read sankey fixture");
    let parsed = parse(&input);
    let rendered = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());

    // Pair each rendered flow with the value the FIXTURE declares for it, in document order, so
    // the assertion is anchored to the input rather than to whatever the golden happens to hold.
    let declared: Vec<f64> = input
        .lines()
        .filter_map(|line| line.rsplit_once(',').and_then(|(_, v)| v.trim().parse().ok()))
        .collect();
    assert_eq!(declared.len(), 8, "fixture declares eight flows");

    let widths: Vec<f64> = rendered
        .split("<path")
        .skip(1)
        .filter(|segment| segment.contains("fm-edge"))
        .filter_map(|segment| {
            let at = segment.find("stroke-width=\"")? + "stroke-width=\"".len();
            let rest = &segment[at..];
            rest[..rest.find('"')?].parse().ok()
        })
        .collect();
    assert_eq!(
        widths.len(),
        declared.len(),
        "every declared flow must render as an edge"
    );

    // Equal values must render equal widths. A scale keyed on edge index rather than value would
    // satisfy monotonicity by accident but fails here, because the two 50s and the two 25s sit at
    // different indices.
    for (i, (vi, wi)) in declared.iter().zip(&widths).enumerate() {
        for (vj, wj) in declared.iter().zip(&widths).skip(i + 1) {
            if (vi - vj).abs() < f64::EPSILON {
                assert!(
                    (wi - wj).abs() < 1e-6,
                    "equal flows {vi} rendered at different widths {wi} and {wj}"
                );
            } else if vi > vj {
                assert!(
                    wi > wj,
                    "flow {vi} must render wider than flow {vj}, got {wi} vs {wj}"
                );
            }
        }
    }

    // The smallest flow must stay visible rather than collapsing to a hairline.
    let min_width = widths.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(
        min_width >= 0.5,
        "smallest flow must remain visible, got width {min_width}"
    );
}

/// XyChart bar heights must be proportional to their values, and anchored to the axis (bd-iicc).
///
/// Unlike the sankey guard next door, this one passes today — the geometry is already correct, and
/// the point is to keep it that way. A byte golden would let a rescale or a baseline drift through
/// as "output changed, bless it"; this states what the picture MEANS, so a wrong rescale fails
/// even after a bless.
#[test]
fn xychart_bar_heights_are_proportional_and_axis_anchored() {
    let input_path = golden_dir().join("xychart_comprehensive.mmd");
    let input = fs::read_to_string(&input_path)
        .map_err(|err| format!("failed reading {}: {err}", input_path.display()))
        .expect("read xychart fixture");
    let parsed = parse(&input);
    let rendered = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());

    // Declared bar series, in document order: Sales then Expenses, interleaved per category by the
    // renderer. Taken from the FIXTURE so re-blessing cannot satisfy this.
    let declared: Vec<f64> = vec![30.0, 20.0, 60.0, 40.0, 95.0, 55.0, 45.0, 35.0];

    let mut bars: Vec<(f64, f64, f64)> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for segment in rendered.split("<rect").skip(1) {
        let Some(end) = segment.find('>') else { continue };
        let tag = &segment[..end];
        if !tag.contains("fm-xychart-bar") || seen.contains(&tag) {
            continue;
        }
        seen.push(tag);
        let attr = |name: &str| -> Option<f64> {
            let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
            let rest = &tag[at..];
            rest[..rest.find('"')?].parse().ok()
        };
        if let (Some(x), Some(y), Some(h)) = (attr("x"), attr("y"), attr("height")) {
            bars.push((x, y, h));
        }
    }
    bars.sort_by(|a, b| a.0.total_cmp(&b.0));

    assert_eq!(
        bars.len(),
        declared.len(),
        "fixture declares {} bar values", declared.len()
    );

    // ONE scale for every bar. Deriving it from the first and checking the rest catches a
    // per-series or per-category rescale, which would still look tidy on screen while making two
    // series incomparable — the whole reason to draw them on shared axes.
    let scale = bars[0].2 / declared[0];
    assert!(scale > 0.0, "bar scale must be positive");
    for ((_, _, height), value) in bars.iter().zip(&declared) {
        let expected = value * scale;
        assert!(
            (height - expected).abs() < 0.05,
            "bar for value {value} has height {height:.3}, expected {expected:.3} at the shared scale"
        );
    }

    // Every bar sits ON the axis. A bar that encodes its value correctly but floats off the
    // baseline is a wrong picture that per-bar height checks alone would pass.
    let baseline = bars[0].1 + bars[0].2;
    for (x, y, height) in &bars {
        assert!(
            ((y + height) - baseline).abs() < 0.05,
            "bar at x={x} ends at {:.3}, not on the shared baseline {baseline:.3}",
            y + height
        );
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
