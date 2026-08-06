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

/// Bounding box of an SVG path's on-curve points, or `Err(command)` for a command this cannot parse.
///
/// Consumes each command's exact parameter count and takes the trailing pair as the on-curve point, so
/// an arc's radii and a cubic's control points are never mistaken for positions. Relative (lowercase)
/// commands are deliberately REFUSED rather than guessed at: mis-consuming parameters yields a
/// plausible wrong box, which is worse than admitting the reader does not handle them.
fn path_extent(d: &str) -> Result<(f32, f32, f32, f32), String> {
    let mut tokens = d
        .replace(',', " ")
        .replace('-', " -")
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>()
        .into_iter()
        .peekable();
    let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut last = (0.0_f32, 0.0_f32);
    while let Some(token) = tokens.next() {
        let Some(cmd) = token.chars().next() else {
            continue;
        };
        if cmd.is_ascii_digit() || cmd == '-' || cmd == '.' {
            continue; // a stray number; the command that owned it already consumed what it needed
        }
        let arity = match cmd {
            'M' | 'L' | 'T' => 2,
            'H' | 'V' => 1,
            'C' => 6,
            'S' | 'Q' => 4,
            'A' => 7,
            'Z' => 0,
            other => return Err(other.to_string()),
        };
        let mut params: Vec<f32> = Vec::with_capacity(arity);
        // A command letter may be glued to its first number, e.g. "M92".
        let glued = token[cmd.len_utf8()..].trim();
        if !glued.is_empty() {
            match glued.parse::<f32>() {
                Ok(v) => params.push(v),
                Err(_) => return Err(token.clone()),
            }
        }
        while params.len() < arity {
            match tokens.next() {
                Some(next) => match next.parse::<f32>() {
                    Ok(v) => params.push(v),
                    Err(_) => return Err(next),
                },
                None => return Err(format!("{cmd} truncated")),
            }
        }
        let point = match cmd {
            'Z' => last,
            'H' => (params[0], last.1),
            'V' => (last.0, params[0]),
            _ => (params[arity - 2], params[arity - 1]),
        };
        last = point;
        min_x = min_x.min(point.0);
        min_y = min_y.min(point.1);
        max_x = max_x.max(point.0);
        max_y = max_y.max(point.1);
    }
    if min_x.is_finite() && min_y.is_finite() {
        Ok((min_x, min_y, max_x, max_y))
    } else {
        Err("no on-curve points".to_string())
    }
}

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
        // A multi-line label (packet-beta emits "Source Port\n[0-15]") renders as <tspan> children, so
        // the raw span between > and </text> is MARKUP, not text. Reading it unstripped made this
        // return the markup string and the guard downstream report "Source Port not rendered" — the
        // absence-instead-of-unreadable failure again, one layer in. Strip inner tags and collapse
        // whitespace so a multi-line label reads as its joined text.
        let strip_tags = |raw: &str| -> String {
            let mut out = String::with_capacity(raw.len());
            let mut depth = 0usize;
            for ch in raw.chars() {
                match ch {
                    '<' => depth += 1,
                    '>' => depth = depth.saturating_sub(1),
                    c if depth == 0 => out.push(c),
                    _ => {}
                }
            }
            out.split_whitespace().collect::<Vec<_>>().join(" ")
        };
        let text_elements: Vec<String> = body
            .split("<text")
            .skip(1)
            .filter_map(|t| t.split_once('>').and_then(|(_, r)| r.split_once("</text>")))
            .map(|(raw, _)| strip_tags(raw))
            .collect();
        let has_text_element = !text_elements.is_empty();
        let label = text_elements
            .into_iter()
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
        } else if let Some(at) = body.find("<path ") {
            // Some services and nodes are drawn as paths, not primitives: architecture-beta's
            // `db(database)` is an arc-based cylinder, and class/state shapes use paths too. Without
            // this branch such a node is unreadable, and the guard above correctly refuses to judge —
            // but refusing forever is not useful, so read the path's ON-CURVE endpoints.
            //
            // STATED APPROXIMATION: a cubic's control points can lie outside the hull of its
            // endpoints, so this box can be slightly tight for curved shapes. That is fine for centre
            // and ordering assertions and is NOT sound for tight containment claims. An unrecognised
            // path command is reported rather than skipped, because silently mis-consuming parameters
            // would produce a confident wrong box — the exact failure this whole helper exists to
            // avoid.
            let d_start = body[at..].find("d=\"").map(|i| at + i + 3);
            match d_start.and_then(|i| body[i..].find('"').map(|e| &body[i..i + e])) {
                Some(d) => match path_extent(d) {
                    Ok((min_x, min_y, max_x, max_y)) => {
                        Some(((min_x + max_x) / 2.0, (min_y + max_y) / 2.0, max_x - min_x))
                    }
                    Err(cmd) => {
                        unreadable.push(format!(
                            "fm-node-{group_id} draws a path with command {cmd:?} this guard cannot \
                             parse"
                        ));
                        None
                    }
                },
                None => None,
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
                if label.is_empty() && has_text_element {
                    // The node HAS a <text> element and this reader still got nothing out of it, so
                    // it is unreadable, not unlabelled. Reporting absence here would be a lie.
                    unreadable.push(format!(
                        "fm-node-{group_id} has a <text> element whose content this guard could not \
                         extract"
                    ));
                } else if !label.is_empty() {
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

// ── Round 3: guards for types already measured but not yet specified (bd-iicc) ─────────────
//
// Coverage is published as a ratio on bd-iicc so it never reads as complete. These three close a gap
// in my own earlier work: I measured quadrant, journey and packet-beta and reported the results, but
// only packet-beta got a bead and none of them got an executable guard. A filed defect without a
// guard is a claim; a guard is a specification.

/// Quadrant points paired with their labels, `(label, cx, cy)`.
///
/// Points are `<circle class="fm-quadrant-point">` and live OUTSIDE any `fm-node` group, so
/// `node_centres` cannot see them — a guard that reused it would report every point as absent. Pairing
/// is by vertical proximity (the label baseline sits `cy + 4`), NOT by document order, because
/// order-based pairing silently mislabels geometry and I have made that mistake in this file before.
/// An unpaired point or label is a hard error, not a skip.
fn quadrant_points(svg: &str) -> Vec<(String, f32, f32)> {
    let attr = |chunk: &str, key: &str| -> Option<f32> {
        let at = chunk.find(&format!("{key}=\""))?;
        let rest = &chunk[at + key.len() + 2..];
        rest[..rest.find('"')?].parse().ok()
    };
    let mut circles: Vec<(f32, f32)> = Vec::new();
    for chunk in svg.split("<circle ").skip(1) {
        let head = chunk.split("/>").next().unwrap_or_default();
        if !head.contains("class=\"fm-quadrant-point\"") {
            continue;
        }
        let (cx, cy) = (attr(head, "cx"), attr(head, "cy"));
        assert!(
            cx.is_some() && cy.is_some(),
            "a fm-quadrant-point has no readable cx/cy: {head}"
        );
        circles.push((cx.unwrap_or(f32::NAN), cy.unwrap_or(f32::NAN)));
    }
    let mut labels: Vec<(String, f32)> = Vec::new();
    for chunk in svg.split("<text ").skip(1) {
        let head = chunk.split('>').next().unwrap_or_default();
        if !head.contains("class=\"fm-quadrant-point-label\"") {
            continue;
        }
        let text = chunk
            .split_once('>')
            .and_then(|(_, r)| r.split_once("</text>"))
            .map(|(t, _)| t.trim().to_string())
            .unwrap_or_default();
        let y = attr(head, "y");
        assert!(
            y.is_some(),
            "a fm-quadrant-point-label has no readable y: {head}"
        );
        labels.push((text, y.unwrap_or(f32::NAN)));
    }
    assert_eq!(
        circles.len(),
        labels.len(),
        "quadrant has {} points but {} point labels; this guard cannot pair them and any verdict \
         would be meaningless",
        circles.len(),
        labels.len()
    );

    let mut out = Vec::new();
    for (cx, cy) in circles {
        let paired = labels
            .iter()
            .find(|(_, ly)| (ly - (cy + 4.0)).abs() < 1.5)
            .or_else(|| labels.iter().find(|(_, ly)| (ly - cy).abs() < 12.0));
        assert!(
            paired.is_some(),
            "no label found near the quadrant point at ({cx}, {cy}); labels: {labels:?}"
        );
        let label = paired.map_or_else(String::new, |(l, _)| l.clone());
        out.push((label, cx, cy));
    }
    out
}

/// A quadrant chart plots each point at its declared `[x, y]` (bd-iicc).
///
/// PASSES: quadrant dispatches to a dedicated algorithm and both axes are honoured, including the y
/// inversion (a higher declared value sits HIGHER on screen, i.e. at a smaller SVG y). Asserted as
/// orderings derived from the fixture rather than as pinned coordinates, so re-blessing cannot satisfy
/// it and a rescaled-but-correct chart still passes.
#[test]
fn quadrant_points_plot_at_their_declared_coordinates() {
    let svg = render_fixture("quadrant_basic");
    assert!(
        !svg.contains("transform="),
        "a transform would make these coordinates non-final"
    );
    let points = quadrant_points(&svg);
    let at = |label: &str| -> (f32, f32) {
        let found = points.iter().find(|(l, ..)| l == label);
        assert!(found.is_some(), "{label} not plotted");
        found.map_or((f32::NAN, f32::NAN), |(_, cx, cy)| (*cx, *cy))
    };

    // Fixture declares A[0.8, 0.9], B[0.3, 0.7], C[0.6, 0.2].
    // x ascends left to right: B(0.3) < C(0.6) < A(0.8).
    assert!(
        at("Feature B").0 < at("Feature C").0 && at("Feature C").0 < at("Feature A").0,
        "x order wrong: B={} C={} A={}",
        at("Feature B").0,
        at("Feature C").0,
        at("Feature A").0
    );
    // y ascends UP the page, so a larger declared y is a SMALLER svg y: A(0.9) < B(0.7) < C(0.2).
    assert!(
        at("Feature A").1 < at("Feature B").1 && at("Feature B").1 < at("Feature C").1,
        "y order wrong (should be inverted): A={} B={} C={}",
        at("Feature A").1,
        at("Feature B").1,
        at("Feature C").1
    );
}

/// A journey task's satisfaction score must be distinguishable in the output (bd-iicc).
///
/// PASSES, and it pins a DELIBERATE divergence from mermaid-js so nobody "fixes" it by accident:
/// mermaid encodes the score as one of 12 face icons, we encode it as the task fill colour. Both keep
/// the information; the encodings simply differ. mermaid also lays journey tasks out in a row, so our
/// uniform task y is correct too and is asserted here.
///
/// What this guard forbids is the score becoming INVISIBLE — distinct scores collapsing to one fill,
/// which a byte golden would pin without complaint.
#[test]
fn journey_task_scores_are_distinguishable_in_the_output() {
    let svg = render_fixture("journey_basic");
    let scores: [(&str, u32); 7] = [
        ("Visit homepage", 5),
        ("Search products", 4),
        ("View product", 5),
        ("Add to cart", 4),
        ("Checkout", 3),
        ("Payment", 2),
        ("Confirmation", 5),
    ];

    let mut fill_by_score: Vec<(u32, String)> = Vec::new();
    let mut rows: Vec<f32> = Vec::new();
    for chunk in svg.split("<g id=\"fm-node-").skip(1) {
        let body = chunk.split("<g id=\"fm-node-").next().unwrap_or(chunk);
        let label = body
            .split("<text")
            .skip(1)
            .filter_map(|t| t.split_once('>').and_then(|(_, r)| r.split_once("</text>")))
            .map(|(t, _)| t.trim().to_string())
            .find(|t| !t.is_empty())
            .unwrap_or_default();
        let Some((_, score)) = scores.iter().find(|(name, _)| *name == label) else {
            continue;
        };
        // The score reaches the output as an inline fill on the task shape. If that ever moves to a
        // class or an icon, this must fail loudly rather than silently find nothing.
        let at = body.find("style=\"fill: ");
        assert!(
            at.is_some(),
            "journey task {label:?} has no inline fill; the score encoding moved and this guard can \
             no longer read it — update the guard rather than deleting the assertion"
        );
        let rest = &body[at.unwrap_or(0) + 13..];
        let fill = rest[..rest.find('"').unwrap_or(0)].to_string();
        fill_by_score.push((*score, fill));
        if let Some(y_at) = body.find("<rect ") {
            let r = &body[y_at..];
            if let Some(y_key) = r.find("y=\"") {
                let tail = &r[y_key + 3..];
                if let Ok(y) = tail[..tail.find('"').unwrap_or(0)].parse::<f32>() {
                    rows.push(y);
                }
            }
        }
    }
    assert_eq!(
        fill_by_score.len(),
        scores.len(),
        "expected a fill for all {} tasks, read {}",
        scores.len(),
        fill_by_score.len()
    );

    // Equal scores must share a fill, and different scores must not.
    for (score_a, fill_a) in &fill_by_score {
        for (score_b, fill_b) in &fill_by_score {
            if score_a == score_b {
                assert_eq!(
                    fill_a, fill_b,
                    "score {score_a} rendered two different fills, so the encoding is not a function \
                     of the score"
                );
            } else {
                assert_ne!(
                    fill_a, fill_b,
                    "scores {score_a} and {score_b} share fill {fill_a}, so the score is invisible"
                );
            }
        }
    }
    // mermaid rows journey tasks too, so uniform y is correct — pinned so a later change is deliberate.
    assert!(
        rows.windows(2).all(|w| (w[0] - w[1]).abs() < 0.01),
        "journey tasks are no longer on one row: {rows:?}"
    );
}

/// packet-beta field widths must be proportional to their bit ranges (bd-iicc).
///
/// IGNORED because it FAILS against a real defect, filed as bd-51tz: widths track LABEL TEXT instead
/// of bit count. The fixture's three 16-bit fields render 148.72 / 176.77 / 154.50 when they must be
/// identical, the 4-bit field (145.43) is WIDER than the 6-bit field (139.65), and the 32-bit fields
/// are 1.28x the 16-bit ones instead of 2x. Pinned mermaid renders a bit ruler
/// (`bitWidth: 32, bitsPerRow: 32`), so width is strictly `bits * bitWidth`.
///
/// The expected bit count is PARSED FROM THE RENDERED LABEL (`"Source Port[0-15]"`), not restated from
/// a table here, so the guard describes itself from the output and survives fixture edits. A label
/// whose range cannot be parsed is a hard error — this guard must never silently judge fewer fields
/// than the diagram has.
///
/// Un-ignoring this is bd-51tz's acceptance gate. The equal-bit-count assertion is the sharpest and is
/// checked first; do not weaken it to a tolerance the current output happens to satisfy.
#[test]
#[ignore = "fails against bd-51tz: packet-beta field widths track label text, not bit ranges"]
fn packet_field_widths_are_proportional_to_bit_ranges() {
    let svg = render_fixture("packet_basic");
    let centres = node_centres(&svg);

    // (label, bits, width), with bits derived from the trailing "[start-end]" in the label.
    let mut fields: Vec<(String, f32, f32)> = Vec::new();
    for (label, _, _, width) in &centres {
        let open = label.rfind('[');
        assert!(
            open.is_some(),
            "packet field {label:?} has no [start-end] range in its rendered label"
        );
        let open = open.unwrap_or(0);
        let range = &label[open + 1..label.len().saturating_sub(1)];
        let split = range.split_once('-');
        assert!(
            split.is_some(),
            "packet field {label:?} range {range:?} is not start-end"
        );
        let (lo, hi) = split.unwrap_or(("0", "0"));
        let (lo, hi) = (lo.trim().parse::<i64>(), hi.trim().parse::<i64>());
        assert!(
            lo.is_ok() && hi.is_ok(),
            "packet field {label:?} range {range:?} is not numeric"
        );
        let bits = (hi.unwrap_or(0) - lo.unwrap_or(0) + 1) as f32;
        fields.push((label.clone(), bits, *width));
    }
    assert!(
        fields.len() >= 8,
        "expected the fixture's 8 packet fields, read {} — the guard is judging fewer fields than \
         the diagram declares",
        fields.len()
    );

    // Equal bit counts => equal widths. Decisive on its own, and needs no scale factor.
    for (name_a, bits_a, width_a) in &fields {
        for (name_b, bits_b, width_b) in &fields {
            if (bits_a - bits_b).abs() < 0.01 {
                assert!(
                    (width_a - width_b).abs() < 0.5,
                    "{name_a} and {name_b} both span {bits_a} bits but render {width_a} and \
                     {width_b} wide"
                );
            }
        }
    }
    // And width scales with bit count, measured against the narrowest field so no constant is assumed.
    let (unit_name, unit_bits, unit_width) = fields
        .iter()
        .min_by(|a, b| a.1.partial_cmp(&b.1).expect("finite"))
        .cloned()
        .unwrap_or_default();
    let per_bit = unit_width / unit_bits;
    for (name, bits, width) in &fields {
        assert!(
            (width - bits * per_bit).abs() < 0.5,
            "{name} spans {bits} bits so at {per_bit}/bit (from {unit_name}) it should be {} wide, \
             but renders {width}",
            bits * per_bit
        );
    }
}

/// architecture-beta renders every declared service, and its edges fan out from the right one
/// (bd-iicc).
///
/// PASSES. The fixture declares three services and two edges, `api --> db` and `api --> cache`, so
/// BOTH edges must originate at `api` and terminate at distinct places. That is the connectivity the
/// diagram exists to show, and a byte golden would pin an edge wired to the wrong service.
///
/// Note `db(database)` renders as an arc-based cylinder `<path>`, not a rect — before the reader
/// handled paths it was reported as an unreadable shape, which is why the reader now parses on-curve
/// path endpoints. Its box is therefore approximate (see `path_extent`), so this guard asserts
/// connectivity and label presence, which do not depend on a tight box, rather than exact geometry.
#[test]
fn architecture_edges_fan_out_from_their_declared_source() {
    let svg = render_fixture("architecture_basic");
    assert!(
        !svg.contains("transform="),
        "a transform would make these coordinates non-final"
    );

    // Every declared service reaches the output with its label.
    let centres = node_centres(&svg);
    for label in ["API Gateway", "PostgreSQL", "Redis Cache"] {
        assert!(
            centres.iter().any(|(l, ..)| l == label),
            "service {label:?} was not rendered; read {:?}",
            centres.iter().map(|(l, ..)| l).collect::<Vec<_>>()
        );
    }

    // Each service carries an icon marker, since the icon encodes the service KIND — `cloud`,
    // `database`, `server` are not decoration, they are the declared type.
    let icon_nodes = svg.matches("fm-node-has-icon").count();
    assert!(
        icon_nodes >= 3,
        "expected an icon marker on all 3 services, found {icon_nodes}"
    );

    // Both edges start at the same point (api's boundary) and end apart. Collected from the edge
    // paths' own M and final coordinates rather than from node boxes, so the cylinder's approximate
    // box cannot affect the verdict.
    let mut starts: Vec<(f32, f32)> = Vec::new();
    let mut ends: Vec<(f32, f32)> = Vec::new();
    for chunk in svg.split("<path ").skip(1) {
        let head = chunk.split("/>").next().unwrap_or_default();
        if !head.contains("class=\"fm-edge") {
            continue;
        }
        let Some(at) = head.find("d=\"") else {
            continue;
        };
        let rest = &head[at + 3..];
        let d = &rest[..rest.find('"').unwrap_or(0)];
        let extent = path_extent(d);
        assert!(
            extent.is_ok(),
            "edge path {d:?} could not be parsed: {:?}",
            extent.err()
        );
        // First M point is the start; the final on-curve point is the end.
        let nums: Vec<f32> = d
            .replace(',', " ")
            .split_whitespace()
            .filter_map(|t| {
                t.trim_start_matches(|c: char| c.is_ascii_alphabetic())
                    .parse()
                    .ok()
            })
            .collect();
        assert!(
            nums.len() >= 4,
            "edge path {d:?} has too few coordinates to read endpoints"
        );
        starts.push((nums[0], nums[1]));
        ends.push((nums[nums.len() - 2], nums[nums.len() - 1]));
    }
    assert_eq!(
        starts.len(),
        2,
        "expected the fixture's 2 edges, read {}",
        starts.len()
    );

    // `api --> db` and `api --> cache` share a source, so the two starts coincide.
    assert!(
        (starts[0].0 - starts[1].0).abs() < 1.0 && (starts[0].1 - starts[1].1).abs() < 1.0,
        "both edges are declared from `api` but start at {:?} and {:?}",
        starts[0],
        starts[1]
    );
    // …and they go to different services, so the ends must not coincide.
    assert!(
        (ends[0].0 - ends[1].0).abs() > 1.0 || (ends[0].1 - ends[1].1).abs() > 1.0,
        "the two edges end at the same point {:?}, so they cannot be reaching db and cache",
        ends[0]
    );

    // The shared source is `api`: its box centre-x and bottom must match where the edges leave.
    let api = centre_of(&centres, "API Gateway");
    assert!(
        (starts[0].0 - api.0).abs() < 2.0,
        "edges leave at x={} but `api` is centred at x={}",
        starts[0].0,
        api.0
    );
}

/// `path_extent` must parse the absolute commands our renderer emits and REFUSE anything else
/// (bd-iicc).
///
/// This is a self-test of guard infrastructure, not of the engine. It exists because the failure mode
/// being designed out is a reader that mis-consumes parameters and returns a confident wrong box: an
/// arc's radii or a cubic's control points silently treated as positions. Refusal is the correct
/// answer for an unhandled command, and this pins that.
#[test]
fn path_extent_parses_absolute_commands_and_refuses_the_rest() {
    // A rectangle traced with M/L/Z: the box is exact.
    let square = path_extent("M10 20 L30 20 L30 50 L10 50 Z");
    assert_eq!(square, Ok((10.0, 20.0, 30.0, 50.0)));

    // An arc's RADII must not be mistaken for a position. Endpoint is (267.31, 320.62), and the radii
    // 87.66/9.88 must not enter the box.
    let arc = path_extent("M92 320.62 A87.66 9.88 0 0 1 267.31 320.62");
    assert_eq!(arc, Ok((92.0, 320.62, 267.31, 320.62)));

    // A cubic's CONTROL points must not be mistaken for endpoints either: only (311.64, 190.75) and
    // (344.64, 235.75) are on-curve here.
    let cubic = path_extent("M311.64 190.75 C311.64 205.75 344.64 220.75 344.64 235.75");
    assert_eq!(cubic, Ok((311.64, 190.75, 344.64, 235.75)));

    // Relative commands are REFUSED rather than guessed at.
    assert!(
        path_extent("m10 10 l20 20").is_err(),
        "relative commands must be refused, not silently treated as absolute"
    );
    // So is an unknown command letter.
    assert!(
        path_extent("M0 0 X5 5").is_err(),
        "unknown command must be refused"
    );
    // And a truncated command, rather than reading a short box.
    assert!(
        path_extent("M10 20 C1 2 3").is_err(),
        "a truncated command must be refused"
    );
}
