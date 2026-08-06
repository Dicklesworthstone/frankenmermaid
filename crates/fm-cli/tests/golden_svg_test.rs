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
/// Was ignored while bd-e69x was open: every flow rendered at stroke-width 1.80 although the
/// values span 4.8x. Widths are now proportional, so this is live — it is the acceptance gate for
/// that fix and must stay green.
#[test]
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
        // Bound each segment at its OWN closing '>' before inspecting it. Without this the
        // arrowhead <marker>'s inner <path> swallows the rest of the document and matches a
        // data-fm-edge-id and stroke-width belonging to elements far below it.
        .filter_map(|segment| segment.split_once('>').map(|(tag, _)| tag))
        .filter(|tag| tag.contains("data-fm-edge-id"))
        .filter_map(|tag| {
            let at = tag.find("stroke-width=\"")? + "stroke-width=\"".len();
            let rest = &tag[at..];
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

/// gitGraph branches must occupy distinct, visually separable lanes (bd-iicc, fixed by bd-5wbp).
///
/// This was written `#[ignore]`d against a real defect: every commit rendered at the same x
/// regardless of branch, so main and develop were drawn collinear. Branch membership was present in
/// the class attribute but invisible in the picture, and distinct lanes are the whole visual grammar
/// of a gitGraph — a merge is legible precisely because it joins two lanes. Un-ignoring it was
/// bd-5wbp's acceptance gate. Do not relax the separation distance to whatever a fix happens to
/// produce.
#[test]
fn gitgraph_branches_occupy_distinct_lanes() {
    let input_path = golden_dir().join("gitgraph_basic.mmd");
    let input = fs::read_to_string(&input_path)
        .map_err(|err| format!("failed reading {}: {err}", input_path.display()))
        .expect("read gitgraph fixture");
    let parsed = parse(&input);
    let rendered = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());

    // Lane coordinate per branch, taken from the rendered commit circles. Grouped by the branch
    // tag the renderer itself emits, so this asserts the picture agrees with the classification
    // rather than assuming either one.
    let mut lanes: Vec<(u32, f64, f64)> = Vec::new();
    for segment in rendered.split("<g ").skip(1) {
        let Some(tag_at) = segment.find("git-branch-") else {
            continue;
        };
        let rest = &segment[tag_at + "git-branch-".len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let Ok(branch) = digits.parse::<u32>() else {
            continue;
        };
        let Some(cx_at) = segment.find("cx=\"") else {
            continue;
        };
        let after_cx = &segment[cx_at + 4..];
        let Some(cx_end) = after_cx.find('"') else {
            continue;
        };
        let Ok(cx) = after_cx[..cx_end].parse::<f64>() else {
            continue;
        };
        let radius = segment
            .find("r=\"")
            .and_then(|at| {
                let rest = &segment[at + 3..];
                rest[..rest.find('"')?].parse::<f64>().ok()
            })
            .unwrap_or(6.0);
        lanes.push((branch, cx, radius));
    }

    assert!(
        lanes.len() >= 4,
        "fixture has commits on two branches; found {} tagged commits",
        lanes.len()
    );

    // Commits on the SAME branch share a lane.
    for (branch, cx, _) in &lanes {
        let peer = lanes.iter().find(|(b, _, _)| b == branch).expect("self");
        assert!(
            (cx - peer.1).abs() < 0.5,
            "commits on branch {branch} disagree on their lane: {cx} vs {}",
            peer.1
        );
    }

    // DIFFERENT branches occupy DIFFERENT lanes, separated enough to read as distinct columns.
    // A tolerance-sized difference would satisfy "not equal" while still drawing one column.
    let radius = lanes[0].2;
    for (branch_a, cx_a, _) in &lanes {
        for (branch_b, cx_b, _) in &lanes {
            if branch_a != branch_b {
                assert!(
                    (cx_a - cx_b).abs() >= radius * 2.0,
                    "branches {branch_a} and {branch_b} share lane {cx_a}; they must be at least \
                     one commit diameter apart to be visually separable"
                );
            }
        }
    }
}

/// ER cardinality labels must match the crow's-foot symbols the fixture declares (bd-iicc).
///
/// Passes today — the mapping is already right. It is worth pinning because cardinality is the
/// entire semantic payload of an ER relationship: swapping `0..*` for `1..*` turns an optional
/// association into a mandatory one, which is a data-model error that renders as a tidy,
/// plausible-looking diagram and that a byte golden would bless without complaint.
#[test]
fn er_cardinality_labels_match_declared_relationships() {
    let input_path = golden_dir().join("er_basic.mmd");
    let input = fs::read_to_string(&input_path)
        .map_err(|err| format!("failed reading {}: {err}", input_path.display()))
        .expect("read er fixture");
    let parsed = parse(&input);
    let rendered = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());

    // Derive the expectation from the FIXTURE's crow's-foot symbols rather than restating the
    // renderer's output, so this cannot be satisfied by re-blessing.
    //   ||  exactly one      o{  zero or many      |{  one or many
    let mut expected: Vec<&str> = Vec::new();
    for line in input.lines() {
        let Some((left, _)) = line.split_once(" : ") else {
            continue;
        };
        let Some(symbols) = left.split_whitespace().nth(1) else {
            continue;
        };
        let Some((near, far)) = symbols.split_once("--") else {
            continue;
        };
        expected.push(match near {
            "||" => "1",
            "}o" | "o" => "0..*",
            "}|" => "1..*",
            other => panic!("unhandled near symbol {other}"),
        });
        expected.push(match far {
            "||" => "1",
            "o{" => "0..*",
            "|{" => "1..*",
            other => panic!("unhandled far symbol {other}"),
        });
    }
    assert_eq!(
        expected,
        ["1", "0..*", "1", "1..*"],
        "fixture parse produced unexpected cardinalities"
    );

    let rendered_labels: Vec<String> = rendered
        .split("fm-er-cardinality")
        .skip(1)
        .filter_map(|segment| {
            let at = segment.find('>')? + 1;
            let rest = &segment[at..];
            Some(rest[..rest.find("</text>")?].to_string())
        })
        .collect();

    assert_eq!(
        rendered_labels, expected,
        "rendered cardinality labels must match the declared crow's-foot symbols, in order"
    );
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

// ── Semantic guards for grid- and sequence-structured diagram types (bd-iicc) ──────────────
//
// These four were written as one hypothesis-driven round. The pattern behind every wrong-picture
// defect found so far is a diagram type with its OWN coordinate system reusing generic node layout,
// so the round targeted the four unguarded types that have one. It came out 2 hits / 2 nulls, and the
// split refines the hypothesis: the HITS are the two GRID-structured types (block-beta columns,
// kanban lanes), which have no grid layout algorithm and fall back to generic placement. The NULLS
// are the two types that already own a dedicated algorithm (timeline sequencing, mindmap radial), and
// both encode their coordinates correctly.

/// Every node's centre, keyed by its first text run: `(label, centre_x, centre_y, width)`.
///
/// FAILS LOUDLY ON A SHAPE IT CANNOT READ rather than skipping it. That distinction is the whole
/// point of this helper's design: an earlier version read only `<rect>`, so a mindmap `root((..))` —
/// which renders as a circle — was silently dropped, and the guard downstream reported
/// "Central Topic not rendered". That reads as a layout defect and is not one. A check that quietly
/// skips what it cannot parse produces a verdict without having looked at the thing it is judging,
/// which is worse than no check at all.
///
/// So an unreadable node group is a HARD ERROR naming the group and the tags it does contain, and a
/// node that is genuinely absent is a separate, differently-worded failure at the lookup site.
/// Extend the shape list when a new shape appears; do not narrow it.
fn node_centres(svg: &str) -> Vec<(String, f32, f32, f32)> {
    let mut out = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();
    for chunk in svg.split("<g id=\"fm-node-").skip(1) {
        let body = chunk.split("<g id=\"fm-node-").next().unwrap_or(chunk);
        let group_id: String = body.chars().take_while(|c| *c != '"').collect();
        let num = |key: &str, from: &str| -> Option<f32> {
            let at = from.find(&format!("{key}=\""))?;
            let rest = &from[at + key.len() + 2..];
            rest[..rest.find('"')?].parse().ok()
        };
        let label = body
            .split("<text")
            .skip(1)
            .filter_map(|t| t.split_once('>').and_then(|(_, r)| r.split_once("</text>")))
            .map(|(text, _)| text.trim().to_string())
            .find(|t| !t.is_empty())
            .unwrap_or_default();

        // Shapes vary by diagram type: rect for most nodes, circle/ellipse for a mindmap root or a
        // gitGraph commit, polygon/path for diamonds and slanted shapes.
        let geometry = if let Some(at) = body.find("<rect ") {
            let r = &body[at..];
            match (num("x", r), num("y", r), num("width", r), num("height", r)) {
                (Some(x), Some(y), Some(w), Some(h)) => Some((x + w / 2.0, y + h / 2.0, w)),
                _ => None,
            }
        } else if let Some(at) = body.find("<ellipse ") {
            let e = &body[at..];
            match (num("cx", e), num("cy", e), num("rx", e)) {
                (Some(cx), Some(cy), Some(rx)) => Some((cx, cy, rx * 2.0)),
                _ => None,
            }
        } else if let Some(at) = body.find("<circle ") {
            let c = &body[at..];
            match (num("cx", c), num("cy", c), num("r", c)) {
                (Some(cx), Some(cy), Some(r)) => Some((cx, cy, r * 2.0)),
                _ => None,
            }
        } else {
            None
        };

        match geometry {
            Some((cx, cy, w)) => {
                if !label.is_empty() {
                    out.push((label, cx, cy, w));
                }
            }
            // A group with no readable shape is reported, not skipped — UNLESS it carries no shape
            // element at all, which is how deliberately invisible nodes (block-beta `space`) render.
            None => {
                let tags: Vec<&str> = ["rect", "ellipse", "circle", "polygon", "path", "line"]
                    .into_iter()
                    .filter(|t| body.contains(&format!("<{t} ")))
                    .collect();
                if !tags.is_empty() {
                    unreadable.push(format!(
                        "fm-node-{group_id} (label {label:?}) has shape tag(s) {tags:?} this guard \
                         cannot read"
                    ));
                }
            }
        }
    }
    assert!(
        unreadable.is_empty(),
        "node_centres cannot read {} node shape(s), so any absence it reports would be meaningless \
         — extend the shape list rather than narrowing the assertion:\n  {}",
        unreadable.len(),
        unreadable.join("\n  ")
    );
    out
}

fn render_fixture(name: &str) -> String {
    let read = fs::read_to_string(golden_dir().join(format!("{name}.mmd")));
    assert!(read.is_ok(), "read {name} fixture: {:?}", read.err());
    let input = read.unwrap_or_default();
    render_svg_with_config(&parse(&input).ir, &SvgRenderConfig::default())
}

/// Look up one rendered node by label, failing the test rather than panicking from a closure.
fn centre_of(centres: &[(String, f32, f32, f32)], label: &str) -> (f32, f32, f32) {
    let found = centres.iter().find(|(l, ..)| l == label);
    assert!(found.is_some(), "{label} not rendered");
    found.map_or((f32::NAN, f32::NAN, f32::NAN), |(_, cx, cy, w)| {
        (*cx, *cy, *w)
    })
}

/// A timeline's periods run left to right in declaration order, and every event sits in its own
/// period's column (bd-iicc). PASSES: timeline has a dedicated layout algorithm and uses it.
#[test]
fn timeline_periods_order_left_to_right_with_events_in_their_column() {
    let svg = render_fixture("timeline_basic");
    assert!(
        !svg.contains("transform="),
        "a transform would make these coordinates non-final"
    );
    let centres = node_centres(&svg);
    let at = |label: &str| -> f32 { centre_of(&centres, label).0 };

    // Periods strictly increase in x, in declaration order.
    assert!(
        at("2020") < at("2021") && at("2021") < at("2022"),
        "periods out of order: 2020={} 2021={} 2022={}",
        at("2020"),
        at("2021"),
        at("2022")
    );

    // Each event shares its period's column. 2021 declares TWO events, and both must sit under it —
    // that is the case a byte golden would happily pin with one of them misplaced.
    let period_width = at("2021") - at("2020");
    for (event, period) in [
        ("Event A", "2020"),
        ("Event B", "2021"),
        ("Event C", "2021"),
        ("Event D", "2022"),
    ] {
        let offset = (at(event) - at(period)).abs();
        assert!(
            offset < period_width / 2.0,
            "{event} at {} is not in {period}'s column at {} (period pitch {period_width})",
            at(event),
            at(period)
        );
    }

    // The two events of 2021 are distinct rows, not drawn on top of each other.
    let row_of = |label: &str| -> f32 {
        centres
            .iter()
            .find(|(l, ..)| l == label)
            .map(|(_, _, cy, _)| *cy)
            .unwrap_or(f32::NAN)
    };
    assert!(
        (row_of("Event B") - row_of("Event C")).abs() > 1.0,
        "the two 2021 events are stacked at the same y"
    );
}

/// A mindmap encodes tree depth as radial distance from the root (bd-iicc). PASSES: mindmap
/// dispatches to the radial layout, and both depth and the angular fan are correct.
#[test]
fn mindmap_depth_maps_to_radial_distance_from_root() {
    let svg = render_fixture("mindmap_basic");
    let centres = node_centres(&svg);
    let pos = |label: &str| -> (f32, f32) {
        let (cx, cy, _) = centre_of(&centres, label);
        (cx, cy)
    };
    let root = pos("Central Topic");
    let radius = |label: &str| -> f32 {
        let (x, y) = pos(label);
        ((x - root.0).powi(2) + (y - root.1).powi(2)).sqrt()
    };

    // Same depth => same radius. Asserted as a property so it cannot be satisfied by pinning numbers.
    assert!(
        (radius("Branch A") - radius("Branch B")).abs() < 1.0,
        "siblings at depth 1 disagree on radius: {} vs {}",
        radius("Branch A"),
        radius("Branch B")
    );
    for pair in [("Leaf 1", "Leaf 2"), ("Leaf 2", "Leaf 3")] {
        assert!(
            (radius(pair.0) - radius(pair.1)).abs() < 1.0,
            "depth-2 nodes {} and {} disagree on radius",
            pair.0,
            pair.1
        );
    }
    // And depth increases outward: a child is strictly farther out than its parent.
    for (leaf, parent) in [
        ("Leaf 1", "Branch A"),
        ("Leaf 2", "Branch A"),
        ("Leaf 3", "Branch B"),
    ] {
        assert!(
            radius(leaf) > radius(parent),
            "{leaf} (r={}) is not farther from the root than {parent} (r={})",
            radius(leaf),
            radius(parent)
        );
    }
}

/// block-beta must honour a CONTAINER's column span, and must place `space` where it was declared
/// (bd-iicc).
///
/// IGNORED because it FAILS against two real defects, filed as bd-7ute. Note what is NOT broken: the
/// Grid algorithm reads the declared column count and NODE-level spans work — `A["a"]:3` correctly
/// renders 460px across a 180px column pitch. The defects are narrower:
///
/// 1. A span on a CONTAINER (`block:name:N`) is dropped. The parser attaches the span as a class on a
///    NODE, so a container's span reaches nothing with a width: `block:wide:3` containing a
///    one-character label renders 100px while a SINGLE-column block with a long label renders
///    367.35px. That is why the committed block_basic fixture is wrong — it uses the container form.
/// 2. `space` is ordered LAST rather than at its declared grid position, so every later cell shifts
///    one place earlier and the requested gap never appears.
///
/// Un-ignoring this is bd-7ute's acceptance gate. Do not relax either assertion to whatever a fix
/// happens to produce.
#[test]
#[ignore = "fails against bd-7ute: block-beta drops container spans and mis-orders `space`"]
fn block_beta_span_width_is_proportional_to_declared_columns() {
    let src = "block-beta\n  columns 3\n  block:wide:3\n    W[\"x\"]\n  end\n  \
               N[\"a single column with a very long label indeed\"]\n  space\n  M[\"y\"]";
    let svg = render_svg_with_config(&parse(src).ir, &SvgRenderConfig::default());
    let centres = node_centres(&svg);
    let width = |label: &str| -> f32 { centre_of(&centres, label).2 };
    // DEFECT 1: the 3-column span must dominate a 1-column cell regardless of label length.
    assert!(
        width("x") > width("a single column with a very long label indeed") * 1.5,
        "a 3-column span ({}) must be much wider than a 1-column cell ({}), independent of labels",
        width("x"),
        width("a single column with a very long label indeed")
    );

    // DEFECT 2: `space` must hold its declared cell, so the cells after it keep their positions.
    // Declared A, space, C, D, E, F over 3 columns puts A and C in row 1 with a GAP between them;
    // today `space` is emitted last, so A and C end up adjacent and every later cell shifts.
    let grid = "block-beta\n  columns 3\n  A[\"a\"]\n  space\n  C[\"c\"]\n  D[\"d\"]\n  \
                E[\"e\"]\n  F[\"f\"]";
    let grid_svg = render_svg_with_config(&parse(grid).ir, &SvgRenderConfig::default());
    let cells = node_centres(&grid_svg);
    let cell = |label: &str| -> (f32, f32) {
        let (cx, cy, _) = centre_of(&cells, label);
        (cx, cy)
    };
    // Column pitch = the smallest positive gap between distinct node columns. Derived rather than
    // assumed, and NOT as `E.x - A.x`: E sits in the same column as A one row down, so that
    // difference is zero and would make the assertion below vacuous. `space` is invisible and
    // carries no text, so `node_centres` already excludes it and cannot skew this.
    let mut xs: Vec<f32> = cells.iter().map(|(_, cx, ..)| *cx).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    let pitch = xs
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|gap| *gap > 1.0)
        .fold(f32::INFINITY, f32::min);
    assert!(
        pitch.is_finite(),
        "could not derive a column pitch from {xs:?}"
    );
    assert!(
        cell("a").1 == cell("c").1 && (cell("c").0 - cell("a").0).abs() > pitch * 1.5,
        "`space` did not hold its cell: A at {:?} and C at {:?} are {} apart against a pitch of \
         {pitch}, so the declared gap is missing",
        cell("a"),
        cell("c"),
        (cell("c").0 - cell("a").0).abs()
    );
}

/// Kanban cards must sit in their own lane's column (bd-iicc).
///
/// IGNORED because it FAILS against a real defect, filed as bd-eg44: every card in every lane renders
/// at x=144.0, because the swimlanes are laid out as horizontal bands stacked down the page instead of
/// side-by-side columns. A kanban board's columns ARE its grammar — which lane a card is in is the
/// only thing the diagram exists to show. Same class as bd-5wbp (gitGraph branches drawn collinear).
/// Un-ignoring this is bd-eg44's acceptance gate.
#[test]
#[ignore = "fails against the bd-eg44 defect: kanban lanes are horizontal bands, not columns"]
fn kanban_cards_occupy_their_lane_column() {
    let svg = render_fixture("kanban_basic");
    let centres = node_centres(&svg);
    let cx = |label: &str| -> f32 { centre_of(&centres, label).0 };
    // Cards within one lane share a column.
    for pair in [
        ("Task A", "Task B"),
        ("Task C", "Task D"),
        ("Task F", "Task G"),
    ] {
        assert!(
            (cx(pair.0) - cx(pair.1)).abs() < 1.0,
            "{} and {} are in the same lane but different columns",
            pair.0,
            pair.1
        );
    }
    // DIFFERENT lanes occupy DIFFERENT columns, separated enough to read as columns.
    let card_width = centres
        .iter()
        .find(|(l, ..)| l == "Task A")
        .map(|(_, _, _, w)| *w)
        .unwrap_or(100.0);
    for pair in [
        ("Task A", "Task C"),
        ("Task C", "Task F"),
        ("Task A", "Task F"),
    ] {
        assert!(
            (cx(pair.0) - cx(pair.1)).abs() >= card_width,
            "lanes of {} ({}) and {} ({}) overlap; a kanban board's columns are its grammar",
            pair.0,
            cx(pair.0),
            pair.1,
            cx(pair.1)
        );
    }
}
