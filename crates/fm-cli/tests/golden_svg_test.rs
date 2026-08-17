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

/// The config every byte golden in this file is rendered and pinned with.
///
/// Keeps golden fixtures focused on structural rendering stability: visual-effect defaults evolve
/// frequently and pinning them avoids noisy churn. Extracted so a guard can render through the
/// SAME config the goldens are pinned with and thereby make a checkable claim about what the byte
/// corpus does and does not cover — see `c4_external_marker_survives_the_config_the_goldens_pin`.
fn golden_render_config() -> SvgRenderConfig {
    SvgRenderConfig {
        node_gradients: false,
        glow_enabled: false,
        cluster_fill_opacity: 1.0,
        inactive_opacity: 1.0,
        shadow_blur: 3.0,
        shadow_color: String::new(),
        ..Default::default()
    }
}

/// Localise a golden mismatch to its first differing region.
///
/// `FNV hash mismatch for case X` names the case and says nothing about WHAT moved, so attributing a
/// stale golden to the change that caused it meant diffing two SVGs by hand. All seven goldens
/// re-blessed in 4d415dcd were attributed that way. This is the same walk, done by the tooling:
/// in from both ends to the first and last differing byte, quoting a bounded slice of each side.
fn first_difference(expected: &str, got: &str) -> String {
    let head = expected
        .bytes()
        .zip(got.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let head = {
        let mut h = head.min(expected.len()).min(got.len());
        while h > 0 && (!expected.is_char_boundary(h) || !got.is_char_boundary(h)) {
            h -= 1;
        }
        h
    };
    let clip = |text: &str| -> String {
        let mut start = head.min(text.len());
        while start > 0 && !text.is_char_boundary(start) {
            start -= 1;
        }
        let shown: String = text[start..].chars().take(140).collect();
        shown
    };
    format!(
        "    lengths {} vs {}, first difference at byte {head}\n    expected: {:?}\n    got:      {:?}",
        expected.len(),
        got.len(),
        clip(expected),
        clip(got),
    )
}

/// Returns `Some(report)` when this case's golden snapshot does not match.
///
/// It REPORTS rather than panicking so the driver can name every stale golden in one run. This test
/// used to abort on the first mismatch, so a run said `cycle_braid` and nothing else; discovering
/// that seven goldens were stale took six bless-and-revert cycles at one build each.
fn run_case(case_id: &str, bless: bool) -> Option<String> {
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

    let config = golden_render_config();
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

    let mismatch = if output_hash == expected_hash {
        // Equal hashes with differing content would be an FNV collision on this corpus. Assert it
        // rather than trusting the hash, because the hash is what the report quotes.
        assert_eq!(
            rendered, expected,
            "golden snapshot content mismatch for case {case_id} despite equal hashes"
        );
        None
    } else {
        Some(format!(
            "  {case_id}: expected {expected_hash}, got {output_hash}\n{}",
            first_difference(&expected, &rendered)
        ))
    };

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
        "pass_fail_reason": if bless {
            "bless-updated"
        } else if mismatch.is_some() {
            "golden-mismatch"
        } else {
            "matched-golden"
        },
    });
    println!("{evidence}");
    mismatch
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
    let mismatches: Vec<String> = selected_case_ids()
        .into_iter()
        .filter_map(|case_id| run_case(case_id, bless))
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} golden snapshot(s) out of date. EVERY stale case is listed, so one run attributes them \
         all; re-bless only the ones whose change you can explain:\n{}",
        mismatches.len(),
        mismatches.join("\n"),
    );
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
        let Some(d_end) = rest.find('"') else {
            continue;
        };
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
        .filter_map(|line| {
            line.rsplit_once(',')
                .and_then(|(_, v)| v.trim().parse().ok())
        })
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
        let Some(end) = segment.find('>') else {
            continue;
        };
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
        "fixture declares {} bar values",
        declared.len()
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

/// Every ON-CURVE point of an SVG path in order, or `Err(command)` for a command this cannot parse.
///
/// Consumes each command's exact parameter count and takes the trailing pair as the on-curve point, so
/// an arc's radii and a cubic's control points are never mistaken for positions. Relative (lowercase)
/// commands are deliberately REFUSED rather than guessed at: mis-consuming parameters yields a
/// plausible wrong result, which is worse than admitting the reader does not handle them.
///
/// Both `path_extent` (bounding box) and `path_endpoints` (start/end) derive from this, so an edge's
/// endpoints are read with the same arity walk as a shape's box rather than by scanning for the last
/// two numbers in the string — a scan that happens to work for the cubics our edge router emits and
/// would silently return an arc's radii the day one appears.
fn path_on_curve_points(d: &str) -> Result<Vec<(f32, f32)>, String> {
    let mut tokens = d
        .replace(',', " ")
        .replace('-', " -")
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>()
        .into_iter()
        .peekable();
    let mut points: Vec<(f32, f32)> = Vec::new();
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
        points.push(point);
    }
    if points.is_empty() {
        Err("no on-curve points".to_string())
    } else {
        Ok(points)
    }
}

/// Bounding box of an SVG path's on-curve points, or `Err(command)` for a command this cannot parse.
fn path_extent(d: &str) -> Result<(f32, f32, f32, f32), String> {
    let points = path_on_curve_points(d)?;
    let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for (x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    Ok((min_x, min_y, max_x, max_y))
}

/// A point in the rendered SVG's final coordinate space.
type Point = (f32, f32);

/// First and last on-curve points of an SVG path: where an edge STARTS and where it ENDS.
fn path_endpoints(d: &str) -> Result<(Point, Point), String> {
    let points = path_on_curve_points(d)?;
    match (points.first(), points.last()) {
        (Some(first), Some(last)) => Ok((*first, *last)),
        _ => Err("no on-curve points".to_string()),
    }
}

/// Text content of an SVG element body with any inner markup removed and whitespace collapsed.
///
/// A multi-line label (packet-beta emits "Source Port\n[0-15]") renders as `<tspan>` children, so the
/// raw span between `>` and `</text>` is MARKUP, not text. Reading it unstripped made `node_centres`
/// return the markup string and the guard downstream report "Source Port not rendered" — absence
/// standing in for unreadability, which is the failure mode this file keeps designing out.
fn strip_inner_tags(raw: &str) -> String {
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
        // Inner markup is stripped so a multi-line label reads as its joined text; see
        // `strip_inner_tags` for why reading it raw produced a false "not rendered".
        let text_elements: Vec<String> = body
            .split("<text")
            .skip(1)
            .filter_map(|t| t.split_once('>').and_then(|(_, r)| r.split_once("</text>")))
            .map(|(raw, _)| strip_inner_tags(raw))
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

/// An invisible `space` cell must carry no accessible content (bd-ukj2).
///
/// A block-beta `space` is a grid spacer drawn at `opacity: 0`. It used to emit
/// `<text …></text>` with nothing in it, `aria-label=""` on an element announced as a
/// `graphics-symbol`, `tabindex="0"` (an invisible empty cell in the keyboard tab order), and a
/// `<title>` reading `Node: __space_4, rectangle` — a generated internal id read out to a screen
/// reader. Dead output in the same family as an unreferenced `<defs>` entry.
///
/// It also blocked `block_beta_span_width_is_proportional_to_declared_columns`: `node_centres`
/// hard-errors on a node whose `<text>` it cannot read, deliberately, so that an absence it reports
/// can never be an unreadability in disguise. That guard runs directly above.
///
/// Both halves are asserted. The NEGATIVE half — that ordinary cells in the same document keep
/// their text and their accessible names — is what stops this from being satisfied by suppressing
/// labels generally.
#[test]
fn block_beta_space_carries_no_accessible_content() {
    let src = "block-beta\n  columns 3\n  A[\"a\"]\n  space\n  C[\"c\"]";
    let svg = render_svg_with_config(&parse(src).ir, &SvgRenderConfig::default());

    let space_group = svg
        .split("<g id=\"fm-node-")
        .skip(1)
        .find(|chunk| chunk.contains("fm-node-block-beta-space"))
        .map(|chunk| chunk.split("</g>").next().unwrap_or(chunk).to_string());
    assert!(
        space_group.is_some(),
        "the fixture must render a `space` node, or this guard is vacuous"
    );
    let space_group = space_group.unwrap_or_default();

    assert!(
        !space_group.contains("<text"),
        "the `space` cell emits a text element with nothing in it: {space_group}"
    );
    assert!(
        !space_group.contains("aria-label"),
        "the `space` cell carries an accessible name it has no content for: {space_group}"
    );
    assert!(
        !space_group.contains("tabindex"),
        "the `space` cell is in the keyboard tab order: {space_group}"
    );
    assert!(
        !space_group.contains("<title"),
        "the `space` cell announces its generated internal id: {space_group}"
    );
    assert!(
        space_group.contains("aria-hidden=\"true\""),
        "a decorative spacer must be hidden from the accessibility tree, not merely silent: \
         {space_group}"
    );

    // NEGATIVE HALF: real cells in the SAME document keep their text and their accessible names.
    for label in ["a", "c"] {
        let group = svg
            .split("<g id=\"fm-node-")
            .skip(1)
            .find(|chunk| chunk.contains(&format!(">{label}</text>")))
            .unwrap_or("");
        assert!(
            !group.is_empty() && group.contains("aria-label") && group.contains("tabindex"),
            "ordinary cell {label:?} lost its text or its accessible name"
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
/// BOTH LAYOUT DEFECTS ARE NOW FIXED and both assertions here pass at the layout level — see
/// `block_beta_container_span_reaches_the_geometry` and `block_beta_space_holds_its_declared_cell`
/// in tests/integration_test.rs, which assert exactly these two statements against `layout_diagram`.
///
/// This one stays #[ignore]d on a DIFFERENT blocker, filed as bd-ukj2: an invisible `space` node
/// still emits an empty `<text>` element, and `node_centres` hard-errors on a node whose text it
/// cannot read, so this test panics in the reader before reaching either assertion. The doc comment
/// above ("`space` is invisible and carries no text, so `node_centres` already excludes it") states
/// the premise bd-ukj2 violates. Un-ignoring this is bd-ukj2's acceptance gate now; do not relax
/// either assertion to whatever a fix happens to produce.
#[test]
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
/// This FAILED against a real defect, filed and fixed as bd-eg44: every card in every lane rendered
/// at x=144.0, because the swimlanes were laid out as horizontal bands stacked down the page instead
/// of side-by-side columns. A kanban board's columns ARE its grammar — which lane a card is in is the
/// only thing the diagram exists to show. Same class as bd-5wbp (gitGraph branches drawn collinear).
#[test]
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

/// The lane BOXES must not overlap each other, and cards must stack down their lane in declaration
/// order (bd-eg44, criteria 3 and 4).
///
/// `kanban_cards_occupy_their_lane_column` judges the CARDS; this judges the swimlane rectangles and
/// the within-lane ordering, which are separate defects. The lane boxes used to overlap by 68.5px on
/// the vertical axis — two swimlane rectangles drawn on top of each other is a visible artifact
/// regardless of which way the lanes run — and the constant 68.5 pointed at a band height that
/// double-counted a header.
///
/// The lane titles are read from the FIXTURE, not restated here, so this cannot be satisfied by
/// re-blessing and cannot silently judge fewer lanes than the board declares.
#[test]
fn kanban_lane_boxes_are_disjoint_and_cards_stack_in_declaration_order() {
    let input = fs::read_to_string(golden_dir().join("kanban_basic.mmd")).expect("read fixture");
    let svg = render_fixture("kanban_basic");
    assert!(
        !svg.contains("transform="),
        "a transform would make these coordinates non-final"
    );

    // Lane titles and their cards, in declaration order, straight from the fixture.
    let mut lanes: Vec<(String, Vec<String>)> = Vec::new();
    let mut lane_indent: Option<usize> = None;
    for line in input.lines() {
        let text = line.trim();
        if text.is_empty() || text == "kanban" {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if lane_indent.is_none_or(|declared| indent <= declared) {
            lane_indent = Some(indent);
            lanes.push((text.to_string(), Vec::new()));
        } else if let Some(lane) = lanes.last_mut() {
            lane.1.push(text.to_string());
        }
    }
    assert_eq!(
        lanes.len(),
        3,
        "fixture declares three lanes, read {lanes:?}"
    );

    // Swimlane rectangles, keyed by the title text the renderer draws next to each one.
    let mut boxes: Vec<(String, f32, f32, f32, f32)> = Vec::new();
    for chunk in svg.split("<rect id=\"fm-cluster-").skip(1) {
        let num = |key: &str| -> Option<f32> {
            let at = chunk.find(&format!("{key}=\""))?;
            let rest = &chunk[at + key.len() + 2..];
            rest[..rest.find('"')?].parse().ok()
        };
        let title = chunk
            .split_once("class=\"fm-cluster-label\">")
            .and_then(|(_, r)| r.split_once("</text>"))
            .map(|(t, _)| strip_inner_tags(t))
            .unwrap_or_default();
        match (num("x"), num("y"), num("width"), num("height")) {
            (Some(x), Some(y), Some(w), Some(h)) => boxes.push((title, x, y, w, h)),
            _ => panic!("unreadable swimlane rect for {title:?}"),
        }
    }
    for (title, ..) in &lanes {
        assert!(
            boxes.iter().any(|(t, ..)| t == title),
            "lane {title:?} has no swimlane box; read {:?}",
            boxes.iter().map(|(t, ..)| t).collect::<Vec<_>>()
        );
    }

    // Criterion 3: no two lane boxes overlap. Rectangles are disjoint when they are separated on
    // EITHER axis, so this is the exact statement and not a proxy for it.
    for (i, a) in boxes.iter().enumerate() {
        for b in boxes.iter().skip(i + 1) {
            let x_apart = a.1 + a.3 <= b.1 || b.1 + b.3 <= a.1;
            let y_apart = a.2 + a.4 <= b.2 || b.2 + b.4 <= a.2;
            assert!(
                x_apart || y_apart,
                "lane boxes {:?} at ({}, {}) {}x{} and {:?} at ({}, {}) {}x{} overlap",
                a.0,
                a.1,
                a.2,
                a.3,
                a.4,
                b.0,
                b.1,
                b.2,
                b.3,
                b.4
            );
        }
    }

    // Criterion 4: within a lane, cards stack downward in declaration order.
    let centres = node_centres(&svg);
    for (title, cards) in &lanes {
        assert!(cards.len() >= 2, "lane {title:?} needs 2+ cards to order");
        let ys: Vec<f32> = cards
            .iter()
            .map(|card| centre_of(&centres, card).1)
            .collect();
        assert!(
            ys.windows(2).all(|w| w[1] > w[0]),
            "lane {title:?} declares {cards:?} but they render at y {ys:?}"
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
            (width - bits * per_bit).abs() < 0.05,
            "{name} spans {bits} bits so at {per_bit}/bit (from {unit_name}) it should be {} wide, \
             but renders {width}",
            bits * per_bit
        );
    }
}

/// Read every rendered packet field as `(label, start_bit, end_bit, x, cy, width)`.
///
/// The bit range is PARSED FROM THE RENDERED LABEL, exactly as
/// `packet_field_widths_are_proportional_to_bit_ranges` does, so these guards describe themselves
/// from the output rather than restating a table that could drift from the fixture. A label whose
/// range cannot be read is a hard error: a guard that silently judged fewer fields than the diagram
/// declares would pass a renderer that dropped one.
fn packet_fields(svg: &str) -> Vec<(String, u32, u32, f32, f32, f32)> {
    let mut fields = Vec::new();
    for (label, cx, cy, width) in node_centres(svg) {
        let open = label.rfind('[');
        assert!(
            open.is_some(),
            "packet field {label:?} has no [start-end] range in its rendered label"
        );
        let range = &label[open.unwrap_or(0) + 1..label.len().saturating_sub(1)];
        let split = range.split_once('-');
        assert!(
            split.is_some(),
            "packet field {label:?} range {range:?} is not start-end"
        );
        let (lo, hi) = split.unwrap_or(("", ""));
        let (lo, hi) = (lo.trim().parse::<u32>(), hi.trim().parse::<u32>());
        assert!(
            lo.is_ok() && hi.is_ok(),
            "packet field {label:?} range {range:?} is not numeric"
        );
        fields.push((
            label.clone(),
            lo.unwrap_or(0),
            hi.unwrap_or(0),
            cx - width / 2.0,
            cy,
            width,
        ));
    }
    fields
}

/// A packet field begins at its start bit's offset within its row, and rows wrap every 32 bits
/// (bd-51tz, criterion 4).
///
/// This is the half of the encoding that width proportionality cannot catch. Before the fix the
/// fixture's eight fields rendered at x = 134.90, 435.40, 743.32, 92.00, 451.07, 768.49, 139.44,
/// 446.54 — a generic three-column grid jumping left and right, with no relationship to the bit
/// offsets those fields declare.
///
/// Both the per-bit scale and the row pitch are DERIVED FROM THE OUTPUT (narrowest field, and the
/// distinct rendered rows), so nothing here is a restatement of a constant in the layout code; if
/// layout and this guard both moved to a different `bitWidth` the assertions would still bite.
#[test]
fn packet_fields_start_at_their_bit_offset_and_rows_wrap_every_32_bits() {
    const BITS_PER_ROW: u32 = 32;

    let svg = render_fixture("packet_basic");
    assert!(
        !svg.contains("transform="),
        "a transform would make these coordinates non-final"
    );
    let fields = packet_fields(&svg);
    assert!(
        fields.len() >= 8,
        "expected the fixture's 8 packet fields, read {} — the guard is judging fewer fields than \
         the diagram declares",
        fields.len()
    );

    // Per-bit width from the narrowest field, so no scale constant is assumed here.
    let narrowest = fields
        .iter()
        .min_by(|a, b| (a.2 - a.1).cmp(&(b.2 - b.1)))
        .cloned()
        .unwrap_or_default();
    let per_bit = narrowest.5 / (narrowest.2 - narrowest.1 + 1) as f32;
    assert!(
        per_bit > 0.0,
        "narrowest field {} has non-positive per-bit width",
        narrowest.0
    );

    // The fixture must actually wrap, or this guard would be vacuous on it.
    let rows: Vec<u32> = {
        let mut r: Vec<u32> = fields.iter().map(|f| f.1 / BITS_PER_ROW).collect();
        r.sort_unstable();
        r.dedup();
        r
    };
    assert!(
        rows.len() >= 2,
        "packet_basic no longer spans more than one 32-bit row, so row wrapping is untested here"
    );

    let left = fields.iter().map(|f| f.3).fold(f32::INFINITY, f32::min);
    let top = fields.iter().map(|f| f.4).fold(f32::INFINITY, f32::min);

    for (label, start, _, x, cy, _) in &fields {
        // Horizontal: the field's left edge sits at its start bit's offset WITHIN its row.
        let expected_x = left + (start % BITS_PER_ROW) as f32 * per_bit;
        assert!(
            (x - expected_x).abs() < 0.05,
            "{label} starts at bit {start} (offset {} in its row), so its left edge should be \
             {expected_x} at {per_bit}/bit, but it renders at {x}",
            start % BITS_PER_ROW,
        );

        // Vertical: fields sharing a row share a y, and later rows are strictly lower.
        for (other_label, other_start, _, _, other_cy, _) in &fields {
            let same_row = start / BITS_PER_ROW == other_start / BITS_PER_ROW;
            if same_row {
                assert!(
                    (cy - other_cy).abs() < 0.5,
                    "{label} (bit {start}) and {other_label} (bit {other_start}) are both in \
                     32-bit row {} but render at y {cy} and {other_cy}",
                    start / BITS_PER_ROW
                );
            } else if start / BITS_PER_ROW < other_start / BITS_PER_ROW {
                assert!(
                    *cy < *other_cy,
                    "{label} is in row {} and {other_label} in row {}, so {label} must render \
                     above it, but the y values are {cy} and {other_cy}",
                    start / BITS_PER_ROW,
                    other_start / BITS_PER_ROW
                );
            }
        }
    }

    // And the top row is the row containing bit 0, not merely some row.
    let first_row_y = fields
        .iter()
        .filter(|f| f.1 / BITS_PER_ROW == 0)
        .map(|f| f.4)
        .fold(f32::INFINITY, f32::min);
    assert!(
        (first_row_y - top).abs() < 0.5,
        "the row holding bit 0 renders at y {first_row_y} but the topmost row is at {top}"
    );
}

/// Renaming a packet field must not move anything (bd-51tz, criterion 3).
///
/// The same invariant bd-h9gx needed for gantt bars, and for the same reason: label length must not
/// leak into the encoding. Two diagrams declaring IDENTICAL bit ranges with wildly different field
/// names must produce identical field geometry. A label-sized layout fails this immediately — it is
/// what produced the original defect, where "Acknowledgment Number" got the widest box in the
/// fixture purely for being the longest string.
///
/// The negative half is asserted too: the two sources really are different documents that really do
/// render different label text, so a renderer that ignored labels entirely could not pass by
/// accident.
#[test]
fn packet_field_geometry_does_not_depend_on_field_names() {
    let terse = "packet-beta\n0-15: \"A\"\n16-31: \"B\"\n32-63: \"C\"\n64-95: \"D\"\n";
    let verbose = "packet-beta\n\
                   0-15: \"An extremely long source port field name\"\n\
                   16-31: \"Bb\"\n\
                   32-63: \"A considerably longer sequence number field name than any other\"\n\
                   64-95: \"Dd\"\n";

    let terse_svg = render_svg_with_config(&parse(terse).ir, &SvgRenderConfig::default());
    let verbose_svg = render_svg_with_config(&parse(verbose).ir, &SvgRenderConfig::default());

    let terse_fields = packet_fields(&terse_svg);
    let verbose_fields = packet_fields(&verbose_svg);
    assert_eq!(
        terse_fields.len(),
        4,
        "expected 4 fields from the terse source, read {}",
        terse_fields.len()
    );
    assert_eq!(
        terse_fields.len(),
        verbose_fields.len(),
        "renaming changed how many fields render"
    );

    // Negative case: the labels really did change, so identical geometry below is not vacuous.
    let renamed = terse_fields
        .iter()
        .zip(&verbose_fields)
        .filter(|(a, b)| a.0 != b.0)
        .count();
    assert_eq!(
        renamed,
        terse_fields.len(),
        "the two sources were supposed to render different label text for every field"
    );

    for (a, b) in terse_fields.iter().zip(&verbose_fields) {
        assert_eq!(
            (a.1, a.2),
            (b.1, b.2),
            "the two sources declare the same ranges; read {}-{} vs {}-{}",
            a.1,
            a.2,
            b.1,
            b.2
        );
        assert!(
            (a.5 - b.5).abs() < 0.01,
            "bits {}-{} render {} wide as {:?} and {} wide as {:?} — width is tracking the name",
            a.1,
            a.2,
            a.5,
            a.0,
            b.5,
            b.0
        );
        assert!(
            (a.3 - b.3).abs() < 0.01 && (a.4 - b.4).abs() < 0.01,
            "bits {}-{} sit at ({}, {}) as {:?} but ({}, {}) as {:?} — position is tracking the name",
            a.1,
            a.2,
            a.3,
            a.4,
            a.0,
            b.3,
            b.4,
            b.0
        );
    }
}

/// The `Class : member` colon shorthand must become a MEMBER ROW, not a literal node label
/// (bd-4e2k).
///
/// class_basic was pinned for a long time by its byte golden alone, and the byte golden was happy:
/// the rendered text was `Animal : +name` and `Dog`, i.e. the whole source line drawn as one label,
/// with `+bark()` absent from the output entirely. Nothing was misplaced or malformed, so a hash
/// could not tell the wrong picture from the right one — the same blindness bd-iicc exists to cover.
///
/// The expected members are READ FROM THE FIXTURE rather than restated here, so this cannot drift
/// from what the diagram declares, and it cannot be satisfied by re-blessing.
#[test]
fn class_colon_shorthand_renders_as_member_rows() {
    let read = fs::read_to_string(golden_dir().join("class_basic.mmd"));
    assert!(read.is_ok(), "read class_basic fixture: {:?}", read.err());
    let source = read.unwrap_or_default();

    // `Class : member` lines, skipping the relation lines (which also carry `:` for edge labels).
    let mut declared: Vec<(String, String)> = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("classDiagram") {
            continue;
        }
        if line.contains("--") || line.contains("..") {
            continue;
        }
        if let Some((class, member)) = line.split_once(':') {
            let (class, member) = (class.trim(), member.trim());
            if !class.is_empty() && !member.is_empty() && !class.contains(char::is_whitespace) {
                declared.push((class.to_string(), member.to_string()));
            }
        }
    }
    assert!(
        declared.len() >= 2,
        "class_basic must declare at least two `Class : member` shorthands for this guard to mean \
         anything; read {declared:?}"
    );

    let svg = render_fixture("class_basic");
    let boxes = node_boxes_by_declared_id(&svg);

    for (class, member) in &declared {
        let node = boxes.iter().find(|b| &b.id == class);
        assert!(
            node.is_some(),
            "{class} declares member {member:?} but no node with that id was rendered; read {:?}",
            boxes.iter().map(|b| &b.id).collect::<Vec<_>>()
        );
        let rows: Vec<&str> = node
            .map(|n| n.texts.iter().map(|(t, _)| t.as_str()).collect())
            .unwrap_or_default();

        // The member is a row of its own...
        assert!(
            rows.iter().any(|row| row.trim() == member),
            "{class} declares member {member:?}, which is not a row of its box; rows are {rows:?}"
        );
        // ...the class name is a row of its own...
        assert!(
            rows.iter().any(|row| row.trim() == class),
            "{class}'s own name is not a row of its box; rows are {rows:?}"
        );
        // ...and NOT the whole source line as one label, which is the regression this bead names.
        let literal = format!("{class} : {member}");
        assert!(
            !rows.iter().any(|row| row.trim() == literal),
            "{class} renders {literal:?} as a single literal label, so the colon shorthand never \
             reached class_meta; rows are {rows:?}"
        );
    }
}

/// A sequence diagram's messages must run between the lifelines they were declared between, in the
/// direction they were declared, down the page in declaration order (bd-iicc).
///
/// sequenceDiagram was the last of the 21 golden diagram types with NO semantic guard, despite
/// having two fixtures. A byte golden proves the picture has not changed; it cannot prove the
/// arrows ever pointed at the right participants. Reversing replies, or collapsing two lifelines
/// onto one x, leaves every coordinate in the file looking perfectly reasonable.
///
/// Everything is read from the FIXTURE — participants, their aliases, and each message's sender,
/// receiver and order — so none of it can be satisfied by re-blessing, and geometry is recovered
/// from the rendered path endpoints rather than from anything the renderer says about itself.
#[test]
fn sequence_messages_run_between_their_declared_lifelines() {
    let read = fs::read_to_string(golden_dir().join("sequence_advanced.mmd"));
    assert!(
        read.is_ok(),
        "read sequence_advanced fixture: {:?}",
        read.err()
    );
    let source = read.unwrap_or_default();

    // Declared participants in order, as (id, display name).
    let mut participants: Vec<(String, String)> = Vec::new();
    // Declared messages in order, as (from_id, to_id, is_reply).
    let mut messages: Vec<(String, String, bool)> = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("participant ") {
            let (id, display) = rest
                .split_once(" as ")
                .map_or((rest, rest), |(i, d)| (i.trim(), d.trim()));
            participants.push((id.to_string(), display.to_string()));
            continue;
        }
        if line.starts_with("Note") || line.starts_with("alt ") || line.starts_with("else") {
            continue;
        }
        for (arrow, is_reply) in [("-->>", true), ("->>", false)] {
            let Some((from, rest)) = line.split_once(arrow) else {
                continue;
            };
            let Some((to, _label)) = rest.split_once(':') else {
                continue;
            };
            let (from, to) = (from.trim(), to.trim());
            if !from.is_empty() && !to.is_empty() {
                messages.push((from.to_string(), to.to_string(), is_reply));
            }
            break;
        }
    }
    assert!(
        participants.len() >= 3 && messages.len() >= 4,
        "sequence_advanced must declare 3+ participants and 4+ messages for this guard to mean \
         anything; read {participants:?} and {} messages",
        messages.len()
    );
    assert!(
        messages.iter().any(|(.., reply)| *reply) && messages.iter().any(|(.., reply)| !*reply),
        "the fixture must declare BOTH request and reply messages, or the direction assertion \
         below is vacuous"
    );

    let svg = render_fixture("sequence_advanced");
    assert!(
        !svg.contains("transform="),
        "a transform would make these coordinates non-final"
    );

    // Lifeline x per participant, from the rendered header box.
    let boxes = node_boxes_by_declared_id(&svg);
    let lifeline_x = |id: &str| -> f32 {
        let found = boxes.iter().find(|b| b.id == id);
        assert!(found.is_some(), "participant {id:?} was not rendered");
        found.map_or(f32::NAN, |b| b.x + b.width / 2.0)
    };

    // Participants march left to right in DECLARATION order. A collapse onto one x fails here.
    for pair in participants.windows(2) {
        let (left, right) = (&pair[0].0, &pair[1].0);
        assert!(
            lifeline_x(right) > lifeline_x(left) + 1.0,
            "{left} is declared before {right} but their lifelines are at x {} and {}",
            lifeline_x(left),
            lifeline_x(right)
        );
    }

    // Rendered message arrows, ordered down the page.
    let mut arrows: Vec<(f32, f32, f32)> = Vec::new(); // (y, start_x, end_x)
    for chunk in svg.split("<g id=\"fm-edge-").skip(1) {
        let body = chunk.split("<g id=\"fm-").next().unwrap_or(chunk);
        let Some(d) = body
            .find(" d=\"")
            .map(|at| &body[at + 4..])
            .and_then(|rest| rest.find('"').map(|end| &rest[..end]))
        else {
            continue;
        };
        let Ok((start, end)) = path_endpoints(d) else {
            continue;
        };
        arrows.push((start.1, start.0, end.0));
    }
    arrows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    assert_eq!(
        arrows.len(),
        messages.len(),
        "the fixture declares {} messages but {} arrows rendered — this guard must not judge \
         fewer messages than the diagram declares",
        messages.len(),
        arrows.len()
    );

    // Each message connects its declared pair, in its declared direction.
    for ((from, to, is_reply), (y, start_x, end_x)) in messages.iter().zip(&arrows) {
        let (want_start, want_end) = (lifeline_x(from), lifeline_x(to));
        assert!(
            (start_x - want_start).abs() < 1.0,
            "{from} -> {to} (reply={is_reply}) at y {y} starts at x {start_x}, but {from}'s \
             lifeline is at {want_start}"
        );
        assert!(
            (end_x - want_end).abs() < 1.0,
            "{from} -> {to} (reply={is_reply}) at y {y} ends at x {end_x}, but {to}'s lifeline \
             is at {want_end}"
        );
        // The sharpest of the three: an implementation that drew every arrow left-to-right, or
        // that swapped sender and receiver on replies, still produces plausible coordinates on
        // the right lifelines — and fails here, because the SIGN must match the declaration.
        let declared_sign = (want_end - want_start).signum();
        let rendered_sign = (end_x - start_x).signum();
        assert!(
            (declared_sign - rendered_sign).abs() < f32::EPSILON,
            "{from} -> {to} (reply={is_reply}) at y {y} is drawn in the wrong direction: declared \
             {want_start} -> {want_end}, rendered {start_x} -> {end_x}"
        );
    }

    // And messages descend the page in declaration order.
    assert!(
        arrows.windows(2).all(|w| w[1].0 > w[0].0),
        "messages must descend the page in declaration order; ys are {:?}",
        arrows.iter().map(|a| a.0).collect::<Vec<_>>()
    );
}

/// A sankey's rendered ribbons must CONSERVE at every intermediate node: the widths flowing in must
/// total the widths flowing out (bd-iicc).
///
/// bd-iicc names conservation as the sankey property a byte golden is most blind to, and it is
/// independent of the width guard above rather than implied by it. That guard asserts equal values
/// render equal widths and larger values render wider — both of which a sqrt scale, or a scale
/// recomputed per column, satisfies. Neither conserves: on this fixture a sqrt scale gives Process X
/// 18.66 in against 18.37 out. A sankey whose ribbons do not balance at a node is not a sankey; it
/// is a flow chart with tapering arrows.
///
/// Declared values and the flow topology are read FROM THE FIXTURE, and the balance is computed from
/// RENDERED widths, so this cannot be satisfied by re-blessing.
#[test]
fn sankey_flows_conserve_width_at_every_intermediate_node() {
    let input_path = golden_dir().join("sankey_basic.mmd");
    let input = fs::read_to_string(&input_path)
        .map_err(|err| format!("failed reading {}: {err}", input_path.display()))
        .expect("read sankey fixture");

    // (from, to, value) in document order — the same order the renderer emits edges in.
    let flows: Vec<(String, String, f64)> = input
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.trim().split(',').collect();
            if parts.len() != 3 {
                return None;
            }
            let value: f64 = parts[2].trim().parse().ok()?;
            Some((
                parts[0].trim().to_string(),
                parts[1].trim().to_string(),
                value,
            ))
        })
        .collect();
    assert_eq!(
        flows.len(),
        8,
        "fixture declares eight flows, read {flows:?}"
    );

    let parsed = parse(&input);
    let rendered = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());
    let widths: Vec<f64> = rendered
        .split("<path")
        .skip(1)
        // Bound each segment at its own closing '>' — see the width guard above for why.
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
        flows.len(),
        "every declared flow must render as an edge before conservation can be judged"
    );

    // Declared and rendered totals per node.
    let mut declared_in: std::collections::BTreeMap<&str, f64> = std::collections::BTreeMap::new();
    let mut declared_out: std::collections::BTreeMap<&str, f64> = std::collections::BTreeMap::new();
    let mut drawn_in: std::collections::BTreeMap<&str, f64> = std::collections::BTreeMap::new();
    let mut drawn_out: std::collections::BTreeMap<&str, f64> = std::collections::BTreeMap::new();
    for ((from, to, value), width) in flows.iter().zip(&widths) {
        *declared_out.entry(from.as_str()).or_default() += value;
        *declared_in.entry(to.as_str()).or_default() += value;
        *drawn_out.entry(from.as_str()).or_default() += width;
        *drawn_in.entry(to.as_str()).or_default() += width;
    }

    // Only nodes the FIXTURE itself balances can be judged; a fixture that did not conserve would
    // be a fixture bug, not a render bug, and this guard says so rather than blaming the renderer.
    let mut judged = 0_usize;
    for (node, incoming) in &declared_in {
        let Some(outgoing) = declared_out.get(node) else {
            continue; // a sink: nothing flows onward
        };
        if (incoming - outgoing).abs() > 1e-9 {
            continue;
        }
        judged += 1;
        let (drawn_i, drawn_o) = (drawn_in[node], drawn_out[node]);
        // ABSOLUTE, and tight. A linear value-to-width scale conserves EXACTLY, so the only slack
        // needed is the two decimal places the widths are printed with. A percentage tolerance is
        // what lets a non-conserving scale through: a sqrt scale imbalances Process X by only
        // 1.54% here, which any "within a few percent" reading would wave past, while being 0.63px
        // — more than ten times this bound.
        //
        // If a future fixture adds a flow small enough to hit the renderer's minimum-width floor,
        // this will fail. That is correct and worth surfacing: clamping a ribbon to stay visible
        // genuinely breaks conservation, and the trade-off should be argued rather than hidden by
        // a tolerance wide enough to cover it.
        assert!(
            (drawn_i - drawn_o).abs() < 0.05,
            "{node} declares {incoming} in and {outgoing} out — balanced — but renders {drawn_i} \
             of inbound width against {drawn_o} outbound, so the ribbons do not conserve"
        );
    }
    assert!(
        judged >= 2,
        "sankey_basic must contain at least two nodes whose declared inflow equals its outflow, \
         or this guard checks nothing; judged {judged}"
    );
}

/// A sequence diagram's activation bars and notes must attach to the participants they name
/// (bd-iicc).
///
/// The message guard above covers arrows only. These are the other two things sequence_advanced
/// declares that carry meaning positionally: `activate S` claims a span of S's lifeline, `Note right
/// of S` belongs beside S, and `Note over C,S` claims the region between two named participants.
/// Each is a relationship between declared input and rendered geometry, and each is invisible to a
/// byte golden — a note drawn beside the wrong participant, or an activation bar on the wrong
/// lifeline, is perfectly stable output.
///
/// Everything is read from the FIXTURE, so none of it can be satisfied by re-blessing.
#[test]
fn sequence_activations_and_notes_attach_to_their_declared_participants() {
    let read = fs::read_to_string(golden_dir().join("sequence_advanced.mmd"));
    assert!(
        read.is_ok(),
        "read sequence_advanced fixture: {:?}",
        read.err()
    );
    let source = read.unwrap_or_default();

    // Walk the fixture once, numbering messages so an activate/deactivate pair can be expressed as
    // the range of messages it encloses.
    let mut message_index = 0_usize;
    let mut activations: Vec<(String, usize, usize)> = Vec::new();
    let mut open: Vec<(String, usize)> = Vec::new();
    let mut notes_right: Vec<String> = Vec::new();
    let mut notes_over: Vec<(String, String)> = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("activate ") {
            open.push((rest.trim().to_string(), message_index));
            continue;
        }
        if let Some(rest) = line.strip_prefix("deactivate ") {
            let who = rest.trim().to_string();
            if let Some(pos) = open.iter().rposition(|(p, _)| *p == who) {
                let (_, from) = open.remove(pos);
                activations.push((who, from, message_index.saturating_sub(1)));
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("Note right of ") {
            if let Some((who, _)) = rest.split_once(':') {
                notes_right.push(who.trim().to_string());
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("Note over ") {
            if let Some((who, _)) = rest.split_once(':')
                && let Some((a, b)) = who.split_once(',')
            {
                notes_over.push((a.trim().to_string(), b.trim().to_string()));
            }
            continue;
        }
        if line.contains("->>") {
            message_index += 1;
        }
    }
    assert!(
        !activations.is_empty() && !notes_right.is_empty() && !notes_over.is_empty(),
        "sequence_advanced must declare an activate block, a `Note right of`, and a `Note over` \
         for this guard to mean anything; read {activations:?}, {notes_right:?}, {notes_over:?}"
    );

    let svg = render_fixture("sequence_advanced");
    let boxes = node_boxes_by_declared_id(&svg);
    let lifeline_x = |id: &str| -> f32 {
        let found = boxes.iter().find(|b| b.id == id);
        assert!(found.is_some(), "participant {id:?} was not rendered");
        found.map_or(f32::NAN, |b| b.x + b.width / 2.0)
    };

    // Message ys, in declaration order, recovered from the arrow paths.
    let mut message_ys: Vec<f32> = Vec::new();
    for chunk in svg.split("<g id=\"fm-edge-").skip(1) {
        let body = chunk.split("<g id=\"fm-").next().unwrap_or(chunk);
        let Some(d) = body
            .find(" d=\"")
            .map(|at| &body[at + 4..])
            .and_then(|rest| rest.find('"').map(|end| &rest[..end]))
        else {
            continue;
        };
        if let Ok((start, _)) = path_endpoints(d) {
            message_ys.push(start.1);
        }
    }
    message_ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Rendered rects carrying a given class.
    let rects = |class: &str| -> Vec<(f32, f32, f32, f32)> {
        let mut out = Vec::new();
        for tag in svg.split("<rect").skip(1) {
            let Some((tag, _)) = tag.split_once('>') else {
                continue;
            };
            if !tag.contains(class) {
                continue;
            }
            let num = |key: &str| -> Option<f32> {
                let at = tag.find(&format!("{key}=\""))? + key.len() + 2;
                let rest = &tag[at..];
                rest[..rest.find('"')?].parse().ok()
            };
            if let (Some(x), Some(y), Some(w), Some(h)) =
                (num("x"), num("y"), num("width"), num("height"))
            {
                out.push((x, y, w, h));
            }
        }
        out
    };

    // An activation bar sits ON its participant's lifeline and covers the messages it encloses.
    let bars = rects("fm-activation-bar");
    assert_eq!(
        bars.len(),
        activations.len(),
        "the fixture declares {} activation(s) but {} bar(s) rendered",
        activations.len(),
        bars.len()
    );
    for ((who, first, last), (x, y, w, h)) in activations.iter().zip(&bars) {
        let lifeline = lifeline_x(who);
        assert!(
            (x + w / 2.0 - lifeline).abs() < 1.0,
            "{who}'s activation bar is centred at {} but its lifeline is at {lifeline}",
            x + w / 2.0
        );
        let (top, bottom) = (*y, y + h);
        for index in *first..=*last {
            let Some(message_y) = message_ys.get(index) else {
                continue;
            };
            assert!(
                *message_y >= top - 1.0 && *message_y <= bottom + 1.0,
                "{who} is active across message {index} at y {message_y}, but its bar only spans \
                 {top}..{bottom}"
            );
        }
    }

    // `Note over A,B` must CONTAIN both lifelines; `Note right of X` must start right of X's.
    // Both are assertions a merely-present note passes and a misplaced one fails.
    let notes = rects("fm-sequence-note");
    assert_eq!(
        notes.len(),
        notes_right.len() + notes_over.len(),
        "the fixture declares {} note(s) but {} rendered",
        notes_right.len() + notes_over.len(),
        notes.len()
    );
    for (a, b) in &notes_over {
        let left = lifeline_x(a).min(lifeline_x(b));
        let right = lifeline_x(a).max(lifeline_x(b));
        let spanning = notes
            .iter()
            .find(|(x, _, w, _)| *x <= left + 1.0 && x + w >= right - 1.0);
        assert!(
            spanning.is_some(),
            "`Note over {a},{b}` must span both lifelines ({left}..{right}), but no note rect \
             covers that range; notes are {notes:?}"
        );
    }
    for who in &notes_right {
        let lifeline = lifeline_x(who);
        let beside = notes.iter().find(|(x, ..)| *x > lifeline);
        assert!(
            beside.is_some(),
            "`Note right of {who}` must start right of its lifeline at {lifeline}; notes are \
             {notes:?}"
        );
    }
}

/// An xychart's AXIS must describe the same scale its data is drawn on (bd-iicc).
///
/// The bar guard above proves every bar shares one scale and sits on the baseline; it never looks at
/// the axis. So a chart whose y-axis is labelled 4000..11000 while the bars are drawn to some other
/// range passes it, and passes the byte golden too — every number in the file is stable and each
/// element looks right on its own. The axis is what tells a reader what a bar HEIGHT means; if the
/// two disagree the picture lies while looking tidy.
///
/// Declared range and categories are read FROM THE FIXTURE, and the scale is recovered from the
/// rendered tick positions, so nothing here can be satisfied by re-blessing.
#[test]
fn xychart_axis_ticks_describe_the_scale_the_bars_are_drawn_on() {
    let read = fs::read_to_string(golden_dir().join("xychart_basic.mmd"));
    assert!(read.is_ok(), "read xychart_basic fixture: {:?}", read.err());
    let source = read.unwrap_or_default();

    // Declared y range: `y-axis "Revenue (in $)" 4000 --> 11000`.
    let (declared_min, declared_max) = source
        .lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix("y-axis ")?;
            let (lo, hi) = rest.split_once("-->")?;
            let lo: f64 = lo
                .rsplit(['"', ' '])
                .find(|t| !t.is_empty())?
                .parse()
                .ok()?;
            let hi: f64 = hi.trim().parse().ok()?;
            Some((lo, hi))
        })
        .expect("fixture declares a `y-axis ... lo --> hi` range");
    assert!(
        declared_max > declared_min,
        "declared y range must be non-degenerate, read {declared_min}..{declared_max}"
    );

    // Declared x categories: `x-axis [Jan, Feb, Mar, Apr, May]`.
    let categories: Vec<String> = source
        .lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix("x-axis ")?;
            let inner = rest.trim().strip_prefix('[')?.strip_suffix(']')?;
            Some(
                inner
                    .split(',')
                    .map(|c| c.trim().to_string())
                    .collect::<Vec<_>>(),
            )
        })
        .expect("fixture declares `x-axis [..]` categories");

    // Declared bar values.
    let bar_values: Vec<f64> = source
        .lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix("bar ")?;
            let inner = rest.trim().strip_prefix('[')?.strip_suffix(']')?;
            Some(
                inner
                    .split(',')
                    .filter_map(|v| v.trim().parse().ok())
                    .collect::<Vec<f64>>(),
            )
        })
        .expect("fixture declares a `bar [..]` series");
    assert_eq!(
        bar_values.len(),
        categories.len(),
        "fixture must declare one bar per category"
    );

    let svg = render_fixture("xychart_basic");

    // Rendered ticks, as (label, position).
    let ticks = |class: &str| -> Vec<(String, f64)> {
        let mut out = Vec::new();
        for chunk in svg.split("<text").skip(1) {
            let Some((tag, rest)) = chunk.split_once('>') else {
                continue;
            };
            if !tag.contains(class) {
                continue;
            }
            let Some((label, _)) = rest.split_once("</text>") else {
                continue;
            };
            let coord = if class.contains("y-tick") { "y" } else { "x" };
            let num = |key: &str| -> Option<f64> {
                let at = tag.find(&format!("{key}=\""))? + key.len() + 2;
                let tail = &tag[at..];
                tail[..tail.find('"')?].parse().ok()
            };
            if let Some(pos) = num(coord) {
                out.push((label.trim().to_string(), pos));
            }
        }
        out
    };

    // ── y axis ────────────────────────────────────────────────────────────
    let y_ticks = ticks("fm-xychart-y-tick");
    assert!(
        y_ticks.len() >= 2,
        "need at least two y ticks to define a scale, read {y_ticks:?}"
    );
    let y_values: Vec<f64> = y_ticks
        .iter()
        .map(|(label, _)| {
            label
                .parse::<f64>()
                .unwrap_or_else(|_| panic!("y tick {label:?} is not numeric"))
        })
        .collect();

    // The axis must span exactly what the fixture declared — not a rounded-out "nice" range that
    // silently redefines what the bars are measured against.
    assert!(
        (y_values[0] - declared_min).abs() < 1e-6,
        "first y tick is {} but the fixture declares a minimum of {declared_min}",
        y_values[0]
    );
    assert!(
        (y_values[y_values.len() - 1] - declared_max).abs() < 1e-6,
        "last y tick is {} but the fixture declares a maximum of {declared_max}",
        y_values[y_values.len() - 1]
    );

    // Larger values sit HIGHER (smaller y), and positions are linear in value.
    let (v0, p0) = (y_values[0], y_ticks[0].1);
    let (v1, p1) = (y_values[y_values.len() - 1], y_ticks[y_ticks.len() - 1].1);
    assert!(
        p1 < p0,
        "the y axis must increase upward: {v0} is at y {p0} and {v1} at y {p1}"
    );
    let px_per_unit = (p0 - p1) / (v1 - v0);
    for (value, (label, pos)) in y_values.iter().zip(&y_ticks) {
        let expected = p0 - (value - v0) * px_per_unit;
        assert!(
            (pos - expected).abs() < 0.05,
            "y tick {label} sits at {pos} but a linear axis puts it at {expected}"
        );
    }

    // ── x axis ────────────────────────────────────────────────────────────
    let x_ticks = ticks("fm-xychart-x-tick");
    assert_eq!(
        x_ticks.iter().map(|(l, _)| l.clone()).collect::<Vec<_>>(),
        categories,
        "x tick labels must be the declared categories, in declaration order"
    );
    assert!(
        x_ticks.windows(2).all(|w| w[1].1 > w[0].1),
        "categories must run left to right in declaration order: {x_ticks:?}"
    );

    // ── the cross-check ───────────────────────────────────────────────────
    // A bar's HEIGHT must be its value measured on the scale the TICKS define. This is the
    // assertion neither the bar guard nor the byte golden can make: the bar guard derives its
    // scale from the bars themselves, so a chart internally consistent but mislabelled passes it.
    let bar_heights: Vec<f64> = svg
        .split("<rect")
        .skip(1)
        .filter_map(|chunk| chunk.split_once('>').map(|(tag, _)| tag))
        .filter(|tag| tag.contains("fm-xychart-bar"))
        .filter_map(|tag| {
            let at = tag.find("height=\"")? + "height=\"".len();
            let rest = &tag[at..];
            rest[..rest.find('"')?].parse().ok()
        })
        .collect();
    assert_eq!(
        bar_heights.len(),
        bar_values.len(),
        "every declared bar must render"
    );
    for (value, height) in bar_values.iter().zip(&bar_heights) {
        let expected = (value - declared_min) * px_per_unit;
        assert!(
            (height - expected).abs() < 0.05,
            "a bar of {value} on an axis labelled {declared_min}..{declared_max} should stand \
             {expected} tall, but renders {height} — the axis and the data are on different scales"
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

    // `path_endpoints` shares the same arity walk, so an arc's RADII are not mistaken for the start
    // or the end. A "take the last two numbers in the string" reader would return (0, 1) here — the
    // arc's sweep flags — and call it the endpoint.
    assert_eq!(
        path_endpoints("M92 320.62 A87.66 9.88 0 0 1 267.31 320.62"),
        Ok(((92.0, 320.62), (267.31, 320.62)))
    );
    // Direction is preserved: start is the first on-curve point, end is the last, never sorted.
    assert_eq!(
        path_endpoints("M300 400 L100 200"),
        Ok(((300.0, 400.0), (100.0, 200.0)))
    );
}

// ── Round 5: the requirement diagram (bd-iicc) ──────────────────────────────────────────────
//
// requirement_basic was one of the last unguarded types. Its row containment was measured clean in an
// earlier round, but that measurement looked at the node's NAME row only; nothing had ever checked
// what else the requirement renderer draws inside the same box, and nothing had checked that the
// relationship arrows point where the fixture declares. Both turned out to matter.

/// One rendered node keyed by its DECLARED id: its box, its group's classes, and every text run
/// drawn inside it.
///
/// `node_centres` cannot serve these guards, and the reason is the trap this file keeps hitting. It
/// keys a node on its first non-empty text run, and a requirement node's first run is the type header
/// `«requirement»`, not the name — so looking up "AuthReq" through it would report
/// "AuthReq not rendered". That is absence standing in for a key mismatch, it reads exactly like a
/// layout defect, and it is not one. Node groups carry `data-id="AuthReq"` / `data-id="user"`, the
/// name AS DECLARED IN THE SOURCE, so key on that instead. C4 uses the same reader for the same
/// reason: a C4 node's first run is `<<Person>>`, not its alias.
///
/// Everything this cannot read is a hard error naming the group: a group with no `data-id`, no
/// `<rect>`, or a `<text>` whose content or `font-size` cannot be extracted. A guard that silently
/// judged fewer nodes than the diagram declares would produce a verdict without having looked.
#[derive(Debug)]
struct NodeBox {
    /// The `data-id`, i.e. the name the fixture declared.
    id: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    /// The group's `class` attribute verbatim — how a diagram encodes an element's KIND (C4
    /// external-ness, requirement risk) when the encoding is styling rather than geometry.
    classes: String,
    /// Every `<text>` drawn inside the box, as `(content, font_size)`, in document order.
    texts: Vec<(String, f32)>,
}

fn node_boxes_by_declared_id(svg: &str) -> Vec<NodeBox> {
    let mut out: Vec<NodeBox> = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();
    for chunk in svg.split("<g id=\"fm-node-").skip(1) {
        let body = chunk.split("<g id=\"fm-node-").next().unwrap_or(chunk);
        let group_id: String = body.chars().take_while(|c| *c != '"').collect();
        let head = body.split_once('>').map_or(body, |(h, _)| h);

        let attr = |key: &str, from: &str| -> Option<String> {
            let at = from.find(&format!("{key}=\""))?;
            let rest = &from[at + key.len() + 2..];
            Some(rest[..rest.find('"')?].to_string())
        };
        let num = |key: &str, from: &str| -> Option<f32> { attr(key, from)?.parse().ok() };

        let Some(id) = attr("data-id", head) else {
            unreadable.push(format!(
                "fm-node-{group_id} carries no data-id, so this guard cannot tell which declared \
                 element it is"
            ));
            continue;
        };

        // Rect OR circle. A stateDiagram's `[*]` pseudo-states render as circles, and the first
        // version of this reader took only `<rect>` — so it refused to judge state_basic at all
        // rather than reporting "__state_start not rendered". Refusing was correct; narrowing the
        // guard would not have been. A circle is reported as its bounding box, which is exact for a
        // circle (unlike the path case, see `path_extent`). Extend this list when a new shape
        // appears; do not narrow it.
        let geometry = if let Some(at) = body.find("<rect ") {
            let r = &body[at..];
            match (num("x", r), num("y", r), num("width", r), num("height", r)) {
                (Some(x), Some(y), Some(w), Some(h)) => Some((x, y, w, h)),
                _ => None,
            }
        } else if let Some(at) = body.find("<circle ") {
            let c = &body[at..];
            match (num("cx", c), num("cy", c), num("r", c)) {
                (Some(cx), Some(cy), Some(r)) => Some((cx - r, cy - r, r * 2.0, r * 2.0)),
                _ => None,
            }
        } else {
            None
        };
        let Some((x, y, width, height)) = geometry else {
            let tags: Vec<&str> = ["rect", "circle", "ellipse", "polygon", "path", "line"]
                .into_iter()
                .filter(|t| body.contains(&format!("<{t} ")))
                .collect();
            unreadable.push(format!(
                "fm-node-{group_id} (data-id {id:?}) has no shape this guard can turn into a box; \
                 it contains tag(s) {tags:?}"
            ));
            continue;
        };

        // Every text run WITH its own font-size. The requirement renderer draws the type header and
        // the metadata row at 0.75x the node font size, so a single assumed size would measure two of
        // the three rows wrongly — and measuring wrongly is how a containment guard invents a defect.
        let mut texts: Vec<(String, f32)> = Vec::new();
        for raw in body.split("<text").skip(1) {
            let Some((tag, rest)) = raw.split_once('>') else {
                continue;
            };
            let Some((content, _)) = rest.split_once("</text>") else {
                unreadable.push(format!(
                    "fm-node-{group_id} (data-id {id:?}) has an unterminated <text> element"
                ));
                continue;
            };
            let content = strip_inner_tags(content);
            if content.is_empty() {
                continue;
            }
            match num("font-size", tag) {
                Some(size) => texts.push((content, size)),
                None => unreadable.push(format!(
                    "fm-node-{group_id} (data-id {id:?}) draws {content:?} with no readable \
                     font-size, so this guard cannot measure it"
                )),
            }
        }

        out.push(NodeBox {
            id,
            x,
            y,
            width,
            height,
            classes: attr("class", head).unwrap_or_default(),
            texts,
        });
    }
    assert!(
        unreadable.is_empty(),
        "node_boxes_by_declared_id cannot read {} node(s), so any verdict it produced would be \
         meaningless — fix the reader rather than narrowing the assertion:\n  {}",
        unreadable.len(),
        unreadable.join("\n  ")
    );
    out
}

/// Every `SRC - type -> DST` relationship the fixture declares, in declaration order.
///
/// Read from the `.mmd` rather than restated in a table here, so these guards describe themselves
/// from the fixture and survive an edit to it. A fixture that declares no relationship at all is a
/// hard error: this guard must never pass by finding nothing to check.
fn declared_requirement_relationships(fixture: &str) -> Vec<(String, String, String)> {
    let read = fs::read_to_string(golden_dir().join(format!("{fixture}.mmd")));
    assert!(read.is_ok(), "read {fixture} fixture: {:?}", read.err());
    let source = read.unwrap_or_default();
    let mut out = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        let Some((left, dst)) = line.split_once("->") else {
            continue;
        };
        let Some((src, kind)) = left.split_once('-') else {
            continue;
        };
        let (src, kind, dst) = (src.trim(), kind.trim(), dst.trim());
        if src.is_empty() || kind.is_empty() || dst.is_empty() {
            continue;
        }
        out.push((src.to_string(), kind.to_string(), dst.to_string()));
    }
    assert!(
        !out.is_empty(),
        "{fixture} declares no `SRC - type -> DST` relationship; this guard would otherwise pass by \
         checking nothing"
    );
    out
}

/// An edge's label and the two ends of its rendered path.
type LabelledEdge = (String, Point, Point);

/// Every labelled edge in the output as `(label, start, end)`, read from the edge's own path.
fn labelled_edges(svg: &str) -> Vec<LabelledEdge> {
    let mut out = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();
    for chunk in svg.split("<g id=\"fm-edge-").skip(1) {
        let body = chunk.split("<g id=\"fm-").next().unwrap_or(chunk);
        let edge_id: String = body.chars().take_while(|c| *c != '"').collect();

        let label = body
            .split("<text")
            .skip(1)
            .filter_map(|raw| raw.split_once('>'))
            .filter_map(|(_, rest)| rest.split_once("</text>"))
            .map(|(content, _)| strip_inner_tags(content))
            .find(|content| !content.is_empty())
            .unwrap_or_default();

        // Scope the `d` lookup to the <path> TAG. Searching the whole group for `d="` matches inside
        // `data-fm-edge-id="0"` — the reader then parsed "0" as path data and every edge came back
        // "no on-curve points", which the hard error below correctly refused to call a missing
        // connection. Anchor on the leading space so an attribute merely ENDING in `d` cannot match.
        let d = body
            .find("<path")
            .map(|at| &body[at..])
            .and_then(|tag| tag.find('>').map(|end| &tag[..end]))
            .and_then(|tag| tag.find(" d=\"").map(|at| &tag[at + 4..]))
            .and_then(|rest| rest.find('"').map(|end| &rest[..end]));
        let Some(d) = d else {
            unreadable.push(format!("fm-edge-{edge_id} has no readable <path d=…>"));
            continue;
        };
        match path_endpoints(d) {
            Ok((start, end)) => out.push((label, start, end)),
            Err(err) => unreadable.push(format!(
                "fm-edge-{edge_id} path {d:?} could not be read: {err}"
            )),
        }
    }
    assert!(
        unreadable.is_empty(),
        "labelled_edges cannot read {} edge(s), so a missing-connection verdict would be \
         meaningless:\n  {}",
        unreadable.len(),
        unreadable.join("\n  ")
    );
    out
}

/// A requirement relationship must run FROM its declared source TO its declared target (bd-iicc).
///
/// PASSES. This is the connectivity a requirement diagram exists to record: `LoginModule - satisfies
/// -> LoginReq` is a traceability claim, and an arrow drawn the other way asserts the opposite one.
/// A byte golden pins a reversed arrow as contentedly as a correct one, because reversing it changes
/// no element and no attribute name — only which coordinate is first.
///
/// The sharp assertion is therefore the DIRECTED one: each edge's start must lie on the source box's
/// boundary and its end on the target's. An implementation that swapped source and target would still
/// draw two labelled edges between the right pair of boxes and would still satisfy any
/// "the diagram has 2 edges" check; it fails here. The arrowhead marker is asserted too, since
/// direction that is only in the coordinate order and not in the picture is not legible.
#[test]
fn requirement_relationships_run_from_source_to_target() {
    let svg = render_fixture("requirement_basic");
    assert!(
        !svg.contains("transform="),
        "a transform would make these coordinates non-final"
    );

    let boxes = node_boxes_by_declared_id(&svg);
    let declared = declared_requirement_relationships("requirement_basic");
    let edges = labelled_edges(&svg);

    assert_eq!(
        edges.len(),
        declared.len(),
        "the fixture declares {} relationship(s) but {} edge(s) were rendered",
        declared.len(),
        edges.len()
    );

    // Every declared endpoint must exist as a box before any connectivity claim is made, so a
    // missing edge is never blamed on a node that was simply never drawn.
    let box_of = |name: &str| -> &NodeBox {
        let found = boxes.iter().find(|b| b.id == name);
        assert!(
            found.is_some(),
            "declared requirement {name:?} was not rendered; read {:?}",
            boxes.iter().map(|b| &b.id).collect::<Vec<_>>()
        );
        found.unwrap_or_else(|| unreachable!("asserted present"))
    };
    let on_boundary = point_on_box_boundary;

    for (src, kind, dst) in &declared {
        let matching: Vec<_> = edges.iter().filter(|(label, ..)| label == kind).collect();
        assert_eq!(
            matching.len(),
            1,
            "relationship {src} - {kind} -> {dst} should render exactly one edge labelled {kind:?}, \
             found {} among {:?}",
            matching.len(),
            edges.iter().map(|(l, ..)| l).collect::<Vec<_>>()
        );
        let (_, start, end) = matching[0];
        let source_box = box_of(src);
        let target_box = box_of(dst);
        assert!(
            on_boundary(source_box, *start),
            "{src} - {kind} -> {dst} starts at {start:?}, which is not on {src}'s box \
             (x {}..{}, y {}..{}) — an edge that does not leave its declared source asserts a \
             different traceability claim than the one written down",
            source_box.x,
            source_box.x + source_box.width,
            source_box.y,
            source_box.y + source_box.height
        );
        assert!(
            on_boundary(target_box, *end),
            "{src} - {kind} -> {dst} ends at {end:?}, which is not on {dst}'s box \
             (x {}..{}, y {}..{})",
            target_box.x,
            target_box.x + target_box.width,
            target_box.y,
            target_box.y + target_box.height
        );
        // The two boxes are distinct, so start and end must be too: this is what fails if source
        // and target were swapped or collapsed.
        assert!(
            !on_boundary(target_box, *start),
            "{src} - {kind} -> {dst} starts on {dst}'s box, so the arrow is drawn backwards"
        );
    }

    // Direction must be visible, not merely implied by coordinate order.
    let arrowheads = svg.matches("marker-end=").count();
    assert!(
        arrowheads >= declared.len(),
        "expected an arrowhead on each of the {} relationships, found {arrowheads}",
        declared.len()
    );
}

/// Text width as the engine's own font model measures it, at the size the SVG says it was drawn.
///
/// Uses `fm_core::FontMetrics` with the preset derived from the rendered font family, i.e. the same
/// estimator the layout uses to size boxes. That matters: a containment guard measuring with a
/// DIFFERENT model than the engine sizes with would report disagreement between two estimators as a
/// rendering defect.
fn rendered_text_width(text: &str, font_size: f32) -> f32 {
    let metrics = fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
        preset: fm_core::FontPreset::from_family(&SvgRenderConfig::default().font_family),
        font_size,
        ..fm_core::FontMetricsConfig::default()
    });
    metrics.estimate_width(text)
}

/// Every row the requirement renderer draws inside a node box must FIT inside that box (bd-iicc).
///
/// IGNORED because it FAILS against a real defect, filed as bd-jnc1. `compute_node_size` sizes a node
/// from its label, then adds an explicit widening pass for each other kind of content the renderers
/// draw inside the box — class compartments, ER attribute rows (bd-090g), C4 descriptions (bd-9xjy).
/// There is no such pass for requirement metadata, so the type header and the `Risk: … | Verify: …`
/// row are drawn into a box sized for the node's NAME. This is the same defect class as bd-090g and
/// bd-9xjy, in the fourth and last member of it.
///
/// WHAT IT ACTUALLY MEASURES, rather than the tidier version: exactly ONE row in this fixture
/// overflows — LoginReq's `"Risk: Medium | Verify: Demonstration"` is 163.37 wide in a 133.05 box,
/// spilling 15.16 past each side. AuthReq's shorter `"Risk: High | Verify: Test"` fits, and both type
/// headers fit. That is the point rather than a weakness in the finding: whether a row escapes its
/// box depends on how long the declared risk and verify strings happen to be, because nothing sizes
/// the box for them at all. A fixture with a longer `verifymethod:` would overflow further; this one
/// merely shows the mechanism with the smallest margin that proves it.
///
/// THE CONTROL COMES FIRST, and it is what makes a failure here mean something. The node's own NAME
/// row is measured before anything else: the box is sized from that string, so if the estimator
/// cannot fit even the name, the estimator is wrong and the guard says so instead of reporting a
/// rendering defect. Only once the control passes is the overflow of the other rows a claim about the
/// renderer.
///
/// The assertion is the weakest containment claim available — text width must not EXCEED the box, no
/// padding demanded — so it cannot be satisfied by re-blessing and cannot fire on a near-miss.
/// Un-ignoring it is bd-jnc1's acceptance gate.
#[test]
fn requirement_rows_stay_inside_their_node_box() {
    let svg = render_fixture("requirement_basic");
    let boxes = node_boxes_by_declared_id(&svg);
    assert!(
        !boxes.is_empty(),
        "requirement_basic rendered no requirement nodes"
    );

    let mut overflows: Vec<String> = Vec::new();
    for node in &boxes {
        // CONTROL: the name row is the string the box was sized from. If it does not fit, the
        // estimator disagrees with the engine and nothing below this line means anything.
        let name_row = node.texts.iter().find(|(content, _)| *content == node.id);
        assert!(
            name_row.is_some(),
            "{} draws no text run equal to its declared name, so the control for this guard is \
             missing and its verdict would be unfounded; runs were {:?}",
            node.id,
            node.texts
        );
        if let Some((content, size)) = name_row {
            let width = rendered_text_width(content, *size);
            assert!(
                width <= node.width,
                "CONTROL FAILED: {}'s own name row measures {width:.2} in a {:.2} box, so this \
                 guard's width model disagrees with the engine's and it must not be used to judge \
                 the other rows",
                node.id,
                node.width
            );
        }

        for (content, size) in &node.texts {
            let width = rendered_text_width(content, *size);
            if width > node.width {
                overflows.push(format!(
                    "{}: {content:?} at font-size {size} measures {width:.2} but its box is only \
                     {:.2} wide (x {:.2}..{:.2}), so it spills {:.2} past each side",
                    node.id,
                    node.width,
                    node.x,
                    node.x + node.width,
                    (width - node.width) / 2.0
                ));
            }
        }
    }

    assert!(
        overflows.is_empty(),
        "{} requirement row(s) are drawn outside the box that contains them:\n  {}",
        overflows.len(),
        overflows.join("\n  ")
    );
}

/// A requirement's declared `id:` and `text:` must reach the rendered picture (bd-iicc).
///
/// IGNORED because it FAILS against a real defect, filed as bd-f3tc. The parser stores both fields on
/// `IrRequirementNodeMeta` (`req_id`, `text`) and NO renderer ever reads them: the SVG requirement
/// path emits the type header, the name, and a `Risk … | Verify …` row, and stops. So
/// `text: Users must authenticate` — the sentence the requirement actually IS — appears nowhere in
/// the output, and `id: REQ-001`, the traceability key the whole diagram type exists to carry, goes
/// with it. The information is parsed, held in the IR, and dropped on the floor.
///
/// This is the defect class a byte golden is blindest to. Nothing is misplaced and nothing is
/// malformed; content simply never appears, and a hash of the output cannot miss what was never
/// there to change.
///
/// The expected values are READ FROM THE FIXTURE, not restated here, so the guard describes itself
/// from what the diagram declares and cannot drift from it. A fixture declaring no `id:`/`text:` at
/// all is a hard error rather than a quiet pass. Un-ignoring this is bd-f3tc's acceptance gate.
#[test]
fn requirement_id_and_text_reach_the_output() {
    let read = fs::read_to_string(golden_dir().join("requirement_basic.mmd"));
    assert!(
        read.is_ok(),
        "read requirement_basic fixture: {:?}",
        read.err()
    );
    let source = read.unwrap_or_default();

    // Declared `id:`/`text:` values, paired with the block they were declared in.
    let mut declared: Vec<(String, &'static str, String)> = Vec::new();
    let mut current_block = String::new();
    for line in source.lines() {
        let line = line.trim();
        if let Some((keyword, rest)) = line.split_once(char::is_whitespace)
            && rest.trim_end().ends_with('{')
            && !keyword.is_empty()
        {
            current_block = rest.trim_end().trim_end_matches('{').trim().to_string();
            continue;
        }
        for field in ["id", "text"] {
            if let Some(value) = line.strip_prefix(&format!("{field}:")) {
                let value = value.trim().trim_matches('"').to_string();
                if !value.is_empty() && !current_block.is_empty() {
                    declared.push((
                        current_block.clone(),
                        if field == "id" { "id" } else { "text" },
                        value,
                    ));
                }
            }
        }
    }
    assert!(
        declared.len() >= 4,
        "requirement_basic must declare an id: and a text: on at least two requirements for this \
         guard to mean anything; found {declared:?}"
    );

    let svg = render_fixture("requirement_basic");
    let boxes = node_boxes_by_declared_id(&svg);

    let mut missing: Vec<String> = Vec::new();
    for (block, field, value) in &declared {
        let Some(node) = boxes.iter().find(|b| &b.id == block) else {
            missing.push(format!(
                "{block} declares {field}: {value:?} but no node with that id was rendered"
            ));
            continue;
        };
        let drawn = node
            .texts
            .iter()
            .map(|(content, _)| content.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if !drawn.contains(value.as_str()) {
            missing.push(format!(
                "{block} declares {field}: {value:?}, which appears in none of its rendered rows \
                 {:?}",
                node.texts
                    .iter()
                    .map(|(content, _)| content)
                    .collect::<Vec<_>>()
            ));
        }
    }

    assert!(
        missing.is_empty(),
        "{} declared requirement field(s) never reach the picture:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
}

// ── Round 6: C4 (bd-iicc) ───────────────────────────────────────────────────────────────────
//
// A METHOD CORRECTION THAT SHAPED THIS ROUND, recorded because it nearly cost a false bug report.
// The committed `.svg` goldens are rendered with visual effects deliberately OFF — `run_case` pins
// `node_gradients: false, inactive_opacity: 1.0, cluster_fill_opacity: 1.0` to keep the byte goldens
// focused on structure — while `render_fixture` (and every real caller) uses `SvgRenderConfig`'s
// DEFAULTS, where effects are on. So the golden file and the shipping render are different documents.
// Probing the golden bytes for a STYLING question therefore answers about a configuration nobody
// ships: the C4 external marker's CSS rule is absent from c4_basic.svg and present under the default
// config. Guards in this file render through `render_fixture`, i.e. the shipping config; probes must
// do the same.

/// Does `css` define `token` as a class selector, rather than merely mentioning it?
///
/// Requires the `.token` to be followed by a selector separator, so `.fm-node-border-double` cannot
/// satisfy a query for `fm-node-border-d` and `.fm-node-border-dashed` cannot be confused with it —
/// the substring trap that has produced wrong verdicts in this codebase before.
fn css_defines_class(css: &str, token: &str) -> bool {
    let needle = format!(".{token}");
    let mut from = 0usize;
    while let Some(at) = css[from..].find(&needle) {
        let end = from + at + needle.len();
        match css[end..].chars().next() {
            Some(c) if c.is_alphanumeric() || c == '-' || c == '_' => {}
            _ => return true,
        }
        from = end;
    }
    false
}

/// The emitted `<style>` block. A hard error when absent: a guard about styling must never pass by
/// finding no stylesheet to check.
fn style_block(svg: &str) -> String {
    let block = svg
        .find("<style>")
        .map(|at| at + "<style>".len())
        .and_then(|start| {
            svg[start..]
                .find("</style>")
                .map(|end| &svg[start..start + end])
        });
    assert!(
        block.is_some(),
        "the render has no <style> block, so any claim about whether a class is styled would be \
         unfounded"
    );
    block.unwrap_or_default().to_string()
}

/// Is `p` on `b`'s boundary? Edge routing leaves from a box side, so the tolerance covers the stroke,
/// not a whole node.
fn point_on_box_boundary(b: &NodeBox, (px, py): Point) -> bool {
    let pad = 2.0_f32;
    px >= b.x - pad && px <= b.x + b.width + pad && py >= b.y - pad && py <= b.y + b.height + pad
}

/// One `KEYWORD(alias, "Label", "Description")` declaration from a C4 fixture.
#[derive(Debug)]
struct C4Element {
    keyword: String,
    alias: String,
    label: String,
    description: Option<String>,
}

/// Split a C4 argument list on commas that sit OUTSIDE quotes, so a description containing a comma
/// cannot silently split into two arguments and shift every later field.
fn split_c4_args(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in args.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => {
                out.push(current.trim().trim_matches('"').to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().trim_matches('"').to_string());
    }
    out
}

/// The elements and relationships a C4 fixture declares, read from the `.mmd`.
///
/// Read from the source rather than restated here, so these guards describe themselves from the
/// fixture and survive an edit to it. A fixture declaring no element, or no relationship, is a hard
/// error — a guard that passed by finding nothing to check would be worse than absent.
fn c4_declarations(fixture: &str) -> (Vec<C4Element>, Vec<(String, String, String)>) {
    let read = fs::read_to_string(golden_dir().join(format!("{fixture}.mmd")));
    assert!(read.is_ok(), "read {fixture} fixture: {:?}", read.err());
    let source = read.unwrap_or_default();

    let mut elements = Vec::new();
    let mut relationships = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        let Some((keyword, rest)) = line.split_once('(') else {
            continue;
        };
        let Some(args) = rest.strip_suffix(')') else {
            continue;
        };
        let keyword = keyword.trim();
        if keyword.is_empty() {
            continue;
        }
        let parts = split_c4_args(args);
        // `Rel(src, dst, "label")` and its directional variants are relationships; everything else
        // with this shape is an element declaration.
        if keyword.starts_with("Rel") || keyword.starts_with("BiRel") {
            if parts.len() >= 3 {
                relationships.push((parts[0].clone(), parts[1].clone(), parts[2].clone()));
            }
            continue;
        }
        if parts.len() >= 2 {
            elements.push(C4Element {
                keyword: keyword.to_string(),
                alias: parts[0].clone(),
                label: parts[1].clone(),
                description: parts.get(2).cloned().filter(|d| !d.is_empty()),
            });
        }
    }
    assert!(
        !elements.is_empty(),
        "{fixture} declares no C4 element; this guard would otherwise pass by checking nothing"
    );
    assert!(
        !relationships.is_empty(),
        "{fixture} declares no C4 relationship; this guard would otherwise pass by checking nothing"
    );
    (elements, relationships)
}

/// Every C4 element must render the LABEL and the DESCRIPTION it declares (bd-iicc).
///
/// PASSES. This is the exact assertion that caught bd-f3tc one round earlier in requirement, where
/// `text:` was parsed into the IR and rendered by nobody. C4's description is the third positional
/// argument of `Person(alias, "Label", "Description")` — structurally the same kind of payload, in a
/// diagram type whose descriptions have already been mishandled once (bd-9xjy, where a long
/// description drew up to 220px below its own box). A byte golden is blind to a field that simply
/// never appears: nothing is misplaced, nothing is malformed, and a hash cannot miss what was never
/// there to change.
///
/// The declared strings are READ FROM THE FIXTURE, and both the label and the description are checked
/// against the node's OWN rendered rows rather than against the document as a whole — so a
/// description that leaked into the wrong node's box would fail rather than pass on a global search.
#[test]
fn c4_elements_render_their_declared_label_and_description() {
    let svg = render_fixture("c4_basic");
    let (elements, _) = c4_declarations("c4_basic");
    let boxes = node_boxes_by_declared_id(&svg);

    let mut missing: Vec<String> = Vec::new();
    let mut checked_descriptions = 0usize;
    for element in &elements {
        let Some(node) = boxes.iter().find(|b| b.id == element.alias) else {
            missing.push(format!(
                "{}({}) declared but no node with data-id {:?} was rendered; read {:?}",
                element.keyword,
                element.alias,
                element.alias,
                boxes.iter().map(|b| &b.id).collect::<Vec<_>>()
            ));
            continue;
        };
        let drawn = node
            .texts
            .iter()
            .map(|(content, _)| content.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if !drawn.contains(element.label.as_str()) {
            missing.push(format!(
                "{} declares label {:?}, which appears in none of its rendered rows {:?}",
                element.alias, element.label, node.texts
            ));
        }
        if let Some(description) = &element.description {
            checked_descriptions += 1;
            if !drawn.contains(description.as_str()) {
                missing.push(format!(
                    "{} declares description {:?}, which appears in none of its rendered rows {:?}",
                    element.alias, description, node.texts
                ));
            }
        }
    }

    assert!(
        checked_descriptions >= 2,
        "c4_basic must declare a description on at least two elements for this guard to mean \
         anything; checked {checked_descriptions}"
    );
    assert!(
        missing.is_empty(),
        "{} declared C4 field(s) never reach the picture:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
}

/// An EXTERNAL C4 system must be visually distinguishable from an internal one (bd-iicc).
///
/// PASSES. `System_Ext` vs `System` is the single most load-bearing distinction in a C4 context
/// diagram: it is what separates the system you own from the ones you merely talk to. Collapsing it
/// is a silent regression of exactly the shape as bd-5wbp (gitGraph branches drawn collinear) — the
/// information survives in a class attribute while disappearing from the picture — and a byte golden
/// pins the collapsed version as contentedly as the correct one.
///
/// TWO ASSERTIONS, and the second is the one worth having. First, the external node must carry a
/// class no internal node carries. Second, that class must be DEFINED AS A CSS SELECTOR in the
/// emitted stylesheet: a marker class with no rule behind it is invisible, which is indistinguishable
/// from having no marker at all. That is not hypothetical here — `strip_unused_state_css` deletes a
/// whole byte range of the effects stylesheet when its needle scan finds no live state class, and
/// `fm-node-border-dashed` sits inside that range. This guard is what makes that optimization's
/// boundary a checked one.
///
/// WHY ACCENT CLASSES ARE EXCLUDED, and why that exclusion is verified rather than assumed: the
/// external node also carries a different `fm-node-accent-N` than the internals, and accent IS
/// defined in CSS, so accent alone would satisfy a naive version of this guard while encoding nothing
/// about externality. The test therefore first PROVES accent is per-node decoration — the two
/// INTERNAL nodes must themselves carry different accents — and only then excludes the family. If
/// accents ever became semantic, that proof fails loudly instead of silently weakening the guard.
#[test]
fn c4_external_systems_are_visually_distinct_from_internal_ones() {
    let svg = render_fixture("c4_basic");
    let (elements, _) = c4_declarations("c4_basic");
    let boxes = node_boxes_by_declared_id(&svg);
    let css = style_block(&svg);

    let classes_of = |alias: &str| -> Vec<String> {
        let found = boxes.iter().find(|b| b.id == alias);
        assert!(found.is_some(), "C4 element {alias:?} was not rendered");
        found.map_or_else(Vec::new, |b| {
            b.classes.split_whitespace().map(str::to_string).collect()
        })
    };

    let externals: Vec<&C4Element> = elements
        .iter()
        .filter(|e| e.keyword.ends_with("_Ext"))
        .collect();
    let internals: Vec<&C4Element> = elements
        .iter()
        .filter(|e| !e.keyword.ends_with("_Ext"))
        .collect();
    assert!(
        !externals.is_empty() && internals.len() >= 2,
        "c4_basic must declare at least one _Ext element and two internal ones for this guard to \
         mean anything; found {} external and {} internal",
        externals.len(),
        internals.len()
    );

    // Accent is per-node decoration, not semantics — PROVEN here rather than assumed, so the
    // exclusion below cannot silently become a hole.
    let accent_of = |alias: &str| -> Option<String> {
        classes_of(alias)
            .into_iter()
            .find(|c| c.starts_with("fm-node-accent-"))
    };
    let first_internal_accent = accent_of(&internals[0].alias);
    let second_internal_accent = accent_of(&internals[1].alias);
    assert!(
        first_internal_accent.is_some() && first_internal_accent != second_internal_accent,
        "two INTERNAL C4 elements carry the same accent ({first_internal_accent:?}), so accent can \
         no longer be shown to be per-node decoration and this guard must not exclude it from the \
         externality check"
    );

    let internal_classes: Vec<String> = internals
        .iter()
        .flat_map(|e| classes_of(&e.alias))
        .collect();
    for external in &externals {
        let distinguishing: Vec<String> = classes_of(&external.alias)
            .into_iter()
            .filter(|c| !internal_classes.contains(c))
            .filter(|c| !c.starts_with("fm-node-accent-"))
            .collect();
        assert!(
            !distinguishing.is_empty(),
            "{}({}) is declared external but carries no class that an internal element lacks, so \
             external and internal render identically",
            external.keyword,
            external.alias
        );
        let styled: Vec<&String> = distinguishing
            .iter()
            .filter(|c| css_defines_class(&css, c))
            .collect();
        assert!(
            !styled.is_empty(),
            "{}({}) is marked external only by class(es) {distinguishing:?}, and NONE of them is \
             defined as a selector in the emitted stylesheet — the marker is invisible, which is \
             indistinguishable from having no marker at all",
            external.keyword,
            external.alias
        );
    }
}

/// A C4 relationship must run FROM its declared source TO its declared target (bd-iicc).
///
/// PASSES. `Rel(user, web, "Uses")` is a directed claim about who calls whom, and reversing it
/// changes no element and no attribute name — only which coordinate is first — so a byte golden pins
/// a backwards arrow happily. Same assertion, and the same anti-tautology control, as the requirement
/// relationship guard: an implementation that swapped source and target would still draw the right
/// number of labelled edges between the right pair of boxes and fails only this.
#[test]
fn c4_relationships_run_from_source_to_target() {
    let svg = render_fixture("c4_basic");
    assert!(
        !svg.contains("transform="),
        "a transform would make these coordinates non-final"
    );
    let (_, relationships) = c4_declarations("c4_basic");
    let boxes = node_boxes_by_declared_id(&svg);
    let edges = labelled_edges(&svg);

    assert_eq!(
        edges.len(),
        relationships.len(),
        "the fixture declares {} relationship(s) but {} edge(s) were rendered",
        relationships.len(),
        edges.len()
    );

    let box_of = |alias: &str| -> &NodeBox {
        let found = boxes.iter().find(|b| b.id == alias);
        assert!(
            found.is_some(),
            "C4 element {alias:?} was not rendered; read {:?}",
            boxes.iter().map(|b| &b.id).collect::<Vec<_>>()
        );
        found.unwrap_or_else(|| unreachable!("asserted present"))
    };

    for (src, dst, label) in &relationships {
        let matching: Vec<_> = edges.iter().filter(|(l, ..)| l == label).collect();
        assert_eq!(
            matching.len(),
            1,
            "Rel({src}, {dst}, {label:?}) should render exactly one edge with that label, found {} \
             among {:?}",
            matching.len(),
            edges.iter().map(|(l, ..)| l).collect::<Vec<_>>()
        );
        let (_, start, end) = matching[0];
        let source_box = box_of(src);
        let target_box = box_of(dst);
        assert!(
            point_on_box_boundary(source_box, *start),
            "Rel({src}, {dst}, {label:?}) starts at {start:?}, which is not on {src}'s box \
             (x {}..{}, y {}..{})",
            source_box.x,
            source_box.x + source_box.width,
            source_box.y,
            source_box.y + source_box.height
        );
        assert!(
            point_on_box_boundary(target_box, *end),
            "Rel({src}, {dst}, {label:?}) ends at {end:?}, which is not on {dst}'s box \
             (x {}..{}, y {}..{})",
            target_box.x,
            target_box.x + target_box.width,
            target_box.y,
            target_box.y + target_box.height
        );
        assert!(
            !point_on_box_boundary(target_box, *start),
            "Rel({src}, {dst}, {label:?}) starts on {dst}'s box, so the arrow is drawn backwards"
        );
    }
}

/// `css_defines_class` must not be fooled by a class name that PREFIXES another (bd-iicc).
///
/// Guard infrastructure self-test. The substring trap is live in this codebase — a keyword scan once
/// read class `inactive` as `active` — and the C4 externality guard's whole value rests on
/// distinguishing "this marker has a rule" from "some longer class name contains it".
#[test]
fn css_defines_class_requires_a_selector_boundary() {
    let css = ".fm-node-border-double rect { stroke-width: 3; }\n.fm-node-accent-8{--a:1;}\n\
               .fm-node-highlighted text, .fm-node-inactive { opacity: 0.4; }";
    // Defined, in each of the three separator positions that occur: space, `{`, and `,`.
    assert!(css_defines_class(css, "fm-node-border-double"));
    assert!(css_defines_class(css, "fm-node-accent-8"));
    assert!(css_defines_class(css, "fm-node-highlighted"));
    assert!(css_defines_class(css, "fm-node-inactive"));
    // NOT defined: a strict prefix of a class that IS defined must not count. This is the exact
    // failure that would let a dead `fm-node-border-dashed` marker pass by borrowing
    // `fm-node-border-double`'s rule.
    assert!(!css_defines_class(css, "fm-node-border-dashed"));
    assert!(!css_defines_class(css, "fm-node-border"));
    assert!(!css_defines_class(css, "fm-node-accent"));
    assert!(!css_defines_class(css, "fm-node-highlight"));
}

/// A marker class must have a rule behind it on the DIRECT flowchart path too (bd-w0f0).
///
/// `c4_external_marker_survives_the_config_the_goldens_pin` covers the general render path. Flowcharts
/// with embedded theme CSS take a separate `direct_minified_css` path that returns BEFORE
/// `strip_unused_state_css` runs, so it decides from the IR instead — and that decision is the one
/// that can silently under-report and put the defect straight back.
///
/// Both halves are asserted: a flowchart whose class raises a state keyword gets the rule, and a
/// flowchart with no such class does NOT carry the region as dead weight. Rendered with the same
/// effects-off config the 37 byte goldens are pinned with, which is where the defect lived.
#[test]
fn flowchart_state_marker_has_a_rule_under_the_golden_config() {
    let marked =
        "flowchart LR\n  A[Start] --> B[End]\n  classDef important fill:#f9f\n  class A important";
    let svg = render_svg_with_config(&parse(marked).ir, &golden_render_config());
    assert!(
        svg.contains("fm-node-highlighted"),
        "the fixture must actually mark a node, or this guard is vacuous"
    );
    assert!(
        svg.contains(".fm-node-highlighted"),
        "`fm-node-highlighted` is on a node but no rule defines it — a marker class with no rule is \
         indistinguishable from no marker at all"
    );

    // NEGATIVE HALF: an unmarked flowchart must not pay for the region it cannot use.
    let plain = "flowchart LR\n  A[Start] --> B[End]";
    let plain_svg = render_svg_with_config(&parse(plain).ir, &golden_render_config());
    assert!(
        !plain_svg.contains("fm-node-highlighted"),
        "the control fixture was supposed to carry no state class"
    );
    assert!(
        !plain_svg.contains(".fm-node-highlighted"),
        "an unmarked flowchart carries the state rules as dead weight"
    );
}

/// The C4 external marker must survive the config the BYTE GOLDENS are pinned with (bd-iicc).
///
/// IGNORED because it FAILS, and what it specifies is a gap in the golden corpus itself rather than a
/// defect in the shipping render. Filed as bd-w0f0.
///
/// `render_svg_with_config` gates the whole `effects_css` stylesheet on
///     effects_enabled = node_gradients || glow_enabled
///                    || inactive_opacity < 0.999 || cluster_fill_opacity < 0.999
/// — four COSMETIC knobs. Inside that stylesheet sit rules that are not cosmetic at all:
/// `.fm-node-border-dashed` (how a C4 `System_Ext` is shown to be external), `.fm-node-highlighted`,
/// the `.fm-node-block-beta` fills, and `.fm-node-block-beta-space { opacity: 0 }` which is the only
/// thing making a block-beta `space` cell invisible. Turning off gradients and setting both opacities
/// to 1.0 therefore removes SEMANTIC encoding, not just polish.
///
/// WHY THIS MATTERS TO THIS BEAD SPECIFICALLY, which is the reason it is worth a guard rather than a
/// shrug: `golden_render_config` — the config all 37 byte goldens are rendered with — sets exactly
/// those four knobs to the effects-off values. So every committed C4 golden pins a picture in which
/// an external system is INDISTINGUISHABLE from an internal one, and every block-beta golden pins one
/// where `space` cells are not invisible. The byte corpus is structurally incapable of catching a
/// regression in these markers, because it never renders them. That is this bead's thesis in its
/// purest form: the goldens are stable and blind at the same time.
///
/// Un-ignoring this is bd-w0f0's acceptance gate. The live sibling
/// `c4_external_systems_are_visually_distinct_from_internal_ones` covers the shipping default config
/// and must stay green regardless.
#[test]
fn c4_external_marker_survives_the_config_the_goldens_pin() {
    let read = fs::read_to_string(golden_dir().join("c4_basic.mmd"));
    assert!(read.is_ok(), "read c4_basic fixture: {:?}", read.err());
    let input = read.unwrap_or_default();
    let svg = render_svg_with_config(&parse(&input).ir, &golden_render_config());

    let (elements, _) = c4_declarations("c4_basic");
    let boxes = node_boxes_by_declared_id(&svg);
    let css = style_block(&svg);

    let externals: Vec<&C4Element> = elements
        .iter()
        .filter(|e| e.keyword.ends_with("_Ext"))
        .collect();
    assert!(
        !externals.is_empty(),
        "c4_basic declares no _Ext element, so this guard would pass by checking nothing"
    );

    let mut invisible: Vec<String> = Vec::new();
    for external in &externals {
        let Some(node) = boxes.iter().find(|b| b.id == external.alias) else {
            invisible.push(format!("{} was not rendered at all", external.alias));
            continue;
        };
        let marker_classes: Vec<&str> = node
            .classes
            .split_whitespace()
            .filter(|c| c.contains("external") || c.contains("border-"))
            .collect();
        assert!(
            !marker_classes.is_empty(),
            "{} carries no external/border marker class at all under the golden config, so this \
             guard cannot tell whether the marker is merely unstyled or absent entirely; classes \
             were {:?}",
            external.alias,
            node.classes
        );
        if !marker_classes.iter().any(|c| css_defines_class(&css, c)) {
            invisible.push(format!(
                "{} is marked external by {marker_classes:?}, none of which is defined in the \
                 stylesheet this config emits, so it renders identically to an internal system",
                external.alias
            ));
        }
    }

    assert!(
        invisible.is_empty(),
        "{} external C4 element(s) are invisible under the config the byte goldens are pinned \
         with:\n  {}",
        invisible.len(),
        invisible.join("\n  ")
    );
}

// ── Round 7: stateDiagram (bd-iicc) ─────────────────────────────────────────────────────────
//
// state_basic is clean and is guarded live below. state_composite is not, and the two defects it
// carries were separated by a CONTROLLED EXPERIMENT rather than by inspection: rendering a composite
// that declares no inner `[*]` produces a correct, tight cluster, which proves the cluster swell in
// state_composite is CAUSED by the pseudo-state merge rather than merely co-occurring with it. That
// is why both symptoms cite one bead (bd-w5j5) instead of two.

/// One rendered cluster (a composite state's container): its label and its box.
///
/// Hard-errors on a cluster rect whose geometry cannot be read, and on a cluster with no label —
/// an unlabelled container cannot be matched to the composite state that declared it, and a guard
/// that skipped it would judge fewer composites than the diagram declares.
#[derive(Debug)]
struct ClusterBox {
    label: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn cluster_boxes(svg: &str) -> Vec<ClusterBox> {
    let mut out = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();
    let labels: Vec<String> = svg
        .split("class=\"fm-cluster-label\"")
        .skip(1)
        .filter_map(|chunk| chunk.split_once('>'))
        .filter_map(|(_, rest)| rest.split_once("</text>"))
        .map(|(content, _)| strip_inner_tags(content))
        .collect();

    for (index, chunk) in svg.split("<rect id=\"fm-cluster-").skip(1).enumerate() {
        let tag = chunk.split_once('>').map_or(chunk, |(t, _)| t);
        let cluster_id: String = tag.chars().take_while(|c| *c != '"').collect();
        let num = |key: &str| -> Option<f32> {
            let at = tag.find(&format!("{key}=\""))?;
            let rest = &tag[at + key.len() + 2..];
            rest[..rest.find('"')?].parse().ok()
        };
        let (Some(x), Some(y), Some(width), Some(height)) =
            (num("x"), num("y"), num("width"), num("height"))
        else {
            unreadable.push(format!(
                "fm-cluster-{cluster_id} has a rect whose x/y/width/height this guard could not read"
            ));
            continue;
        };
        let Some(label) = labels.get(index) else {
            unreadable.push(format!(
                "fm-cluster-{cluster_id} has no readable label, so it cannot be matched to the \
                 composite state that declared it"
            ));
            continue;
        };
        out.push(ClusterBox {
            label: label.clone(),
            x,
            y,
            width,
            height,
        });
    }
    assert!(
        unreadable.is_empty(),
        "cluster_boxes cannot read {} cluster(s), so any containment verdict would be \
         meaningless:\n  {}",
        unreadable.len(),
        unreadable.join("\n  ")
    );
    out
}

/// What a stateDiagram fixture declares: the composite blocks and their members, and every
/// `[*]` transition tagged with the scope it was declared in.
///
/// `scope` is the enclosing composite's name, or `None` for the top level. That distinction is the
/// whole point: `[*] --> Validating` written inside `state Processing { … }` is Processing's initial
/// transition, NOT the machine's, and conflating the two invents a transition nobody declared.
#[derive(Debug, Default)]
struct StateDeclarations {
    /// `composite name -> states declared inside its block`.
    composites: Vec<(String, Vec<String>)>,
    /// `(scope, target)` for each `[*] --> target`.
    starts: Vec<(Option<String>, String)>,
    /// `(scope, source)` for each `source --> [*]`.
    ends: Vec<(Option<String>, String)>,
}

fn state_declarations(fixture: &str) -> StateDeclarations {
    let read = fs::read_to_string(golden_dir().join(format!("{fixture}.mmd")));
    assert!(read.is_ok(), "read {fixture} fixture: {:?}", read.err());
    let source = read.unwrap_or_default();

    let mut out = StateDeclarations::default();
    let mut scope: Option<String> = None;
    for line in source.lines() {
        let line = line.trim();
        if line == "}" {
            scope = None;
            continue;
        }
        // `state Name {` opens a composite. `state name <<fork>>` does not.
        if let Some(rest) = line.strip_prefix("state ")
            && let Some(name) = rest.strip_suffix('{')
        {
            let name = name.trim().to_string();
            out.composites.push((name.clone(), Vec::new()));
            scope = Some(name);
            continue;
        }
        let Some((left, right)) = line.split_once("-->") else {
            continue;
        };
        // Strip a `: label` suffix from the target side.
        let right = right.split(':').next().unwrap_or(right).trim().to_string();
        let left = left.trim().to_string();

        if left == "[*]" {
            out.starts.push((scope.clone(), right.clone()));
        } else if right == "[*]" {
            out.ends.push((scope.clone(), left.clone()));
        }
        // Record membership: any state NAMED inside a composite block belongs to it.
        if let Some(current) = &scope
            && let Some((_, members)) = out.composites.iter_mut().find(|(n, _)| n == current)
        {
            for name in [&left, &right] {
                if name != "[*]" && !name.is_empty() && !members.contains(name) {
                    members.push(name.clone());
                }
            }
            // A `[*]` written INSIDE the block declares that composite's own pseudo-state, so the
            // node the renderer draws for it is a member, not an intruder (bd-w5j5). Recorded as a
            // marker rather than a name because the id is the renderer's to choose.
            for (name, marker) in [(&left, "[*]-start"), (&right, "[*]-end")] {
                if name == "[*]" && !members.iter().any(|m| m == marker) {
                    members.push(marker.to_string());
                }
            }
        }
    }
    assert!(
        !out.starts.is_empty(),
        "{fixture} declares no `[*] -->` transition; these guards would pass by checking nothing"
    );
    out
}

/// Every declared state transition must run from its source to its target and carry its label
/// (bd-iicc). PASSES on state_basic, which has no composite states.
///
/// A state machine IS its transitions; an arrow drawn backwards asserts the opposite machine, and
/// reversing one changes no element and no attribute name — only which coordinate is first — so the
/// byte golden pins it happily. This is the same directed assertion as the requirement and C4
/// guards, applied to the type where direction carries the most meaning.
#[test]
fn state_transitions_run_from_source_to_target_with_their_labels() {
    let svg = render_fixture("state_basic");
    assert!(
        !svg.contains("transform="),
        "a transform would make these coordinates non-final"
    );
    let boxes = node_boxes_by_declared_id(&svg);
    let edges = labelled_edges(&svg);

    // `Idle --> Running : start` and `Running --> Idle : stop` — a PAIR of opposite transitions
    // between the same two states, which is exactly the shape that a direction-blind implementation
    // renders identically in both directions.
    let declared = [("Idle", "Running", "start"), ("Running", "Idle", "stop")];
    let box_of = |name: &str| -> &NodeBox {
        let found = boxes.iter().find(|b| b.id == name);
        assert!(
            found.is_some(),
            "state {name:?} was not rendered; read {:?}",
            boxes.iter().map(|b| &b.id).collect::<Vec<_>>()
        );
        found.unwrap_or_else(|| unreachable!("asserted present"))
    };

    for (src, dst, label) in declared {
        let matching: Vec<_> = edges.iter().filter(|(l, ..)| l == label).collect();
        assert_eq!(
            matching.len(),
            1,
            "transition {src} --> {dst} : {label} should render exactly one edge with that label, \
             found {} among {:?}",
            matching.len(),
            edges.iter().map(|(l, ..)| l).collect::<Vec<_>>()
        );
        let (_, start, end) = matching[0];
        assert!(
            point_on_box_boundary(box_of(src), *start),
            "{src} --> {dst} : {label} starts at {start:?}, which is not on {src}'s box"
        );
        assert!(
            point_on_box_boundary(box_of(dst), *end),
            "{src} --> {dst} : {label} ends at {end:?}, which is not on {dst}'s box"
        );
        assert!(
            !point_on_box_boundary(box_of(dst), *start),
            "{src} --> {dst} : {label} starts on {dst}'s box, so the arrow is drawn backwards"
        );
    }
}

/// A composite state's `[*]` must be ITS initial state, not the machine's (bd-iicc).
///
/// IGNORED because it FAILS against a real defect, filed as bd-w5j5. state_composite declares
/// `[*] --> Idle` at the top level and `[*] --> Validating` inside `state Processing { … }`. Those
/// are two different pseudo-states in two different scopes. The render collapses them into ONE node
/// (`__state_start`, a single group), which then emits BOTH `__state_start points to Idle` and
/// `__state_start points to Validating`.
///
/// That is not a cosmetic merge — it asserts a transition the source never declared: that the machine
/// can begin directly in `Validating`, bypassing `Idle` and the whole `Processing` entry. Reachability
/// is the one thing a state diagram exists to communicate. The same collapse happens at the terminal:
/// one `__state_end` receives `Formatting` (inner), `Error` and `Complete` (outer).
///
/// The assertion is derived from the fixture's own scoping, not from a count pinned here: no rendered
/// node may be the source of a top-level `[*]` transition AND of a composite-scoped one. Un-ignoring
/// this is part of bd-w5j5's acceptance gate.
#[test]
fn state_pseudo_states_are_scoped_to_their_composite() {
    let svg = render_fixture("state_composite");
    let declared = state_declarations("state_composite");

    let top_level_start_targets: Vec<&String> = declared
        .starts
        .iter()
        .filter(|(scope, _)| scope.is_none())
        .map(|(_, target)| target)
        .collect();
    let scoped_start_targets: Vec<&String> = declared
        .starts
        .iter()
        .filter(|(scope, _)| scope.is_some())
        .map(|(_, target)| target)
        .collect();
    assert!(
        !top_level_start_targets.is_empty() && !scoped_start_targets.is_empty(),
        "state_composite must declare BOTH a top-level `[*] -->` and one inside a composite for \
         this guard to mean anything; read top-level {top_level_start_targets:?} and scoped \
         {scoped_start_targets:?}"
    );

    // Which states each rendered node points at, taken from the edge titles the renderer writes
    // about itself. The invariant is that NO SINGLE node is the source of both a top-level `[*]`
    // transition and a composite-scoped one — two `[*]` in two scopes are two pseudo-states.
    //
    // GATE CORRECTION (bd-w5j5): this used to flag any source whose name merely CONTAINED
    // "state_start" and pointed at a composite's initial state. That is unsatisfiable by any correct
    // renderer — a composite's own start pseudo-state must point at its initial state, and the
    // fixture declares exactly that — so the guard would have failed on `[*] --> Validating`, a
    // transition its own message calls "never declared". It now states the invariant the bead names
    // and this file's doc comment already described, which is strictly what it was meant to check.
    let mut by_source: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for chunk in svg.split("<g id=\"fm-edge-").skip(1) {
        let body = chunk.split("<g id=\"fm-").next().unwrap_or(chunk);
        let Some(title) = body
            .find("<title>")
            .map(|at| &body[at + "<title>".len()..])
            .and_then(|rest| rest.find("</title>").map(|end| &rest[..end]))
        else {
            continue;
        };
        let Some((source, rest)) = title.split_once(" points to ") else {
            continue;
        };
        let target = rest.split(" with label").next().unwrap_or(rest).trim();
        by_source
            .entry(source.trim().to_string())
            .or_default()
            .push(target.to_string());
    }

    let mut merged: Vec<String> = Vec::new();
    for (source, targets) in &by_source {
        let serves_top = targets
            .iter()
            .any(|t| top_level_start_targets.iter().any(|d| d.as_str() == t));
        let serves_scoped = targets
            .iter()
            .any(|t| scoped_start_targets.iter().any(|d| d.as_str() == t));
        if serves_top && serves_scoped {
            merged.push(format!(
                "{source} points to {targets:?} — both the machine's own initial state and a \
                 COMPOSITE's, so one node is serving as two different pseudo-states"
            ));
        }
    }

    assert!(
        merged.is_empty(),
        "the machine's own start pseudo-state also serves as a composite's start, so the picture \
         asserts {} transition(s) the source never declared:\n  {}\n(top-level starts: {:?})",
        merged.len(),
        merged.join("\n  "),
        top_level_start_targets
    );
}

/// A composite state's container must hold ITS members and nothing else (bd-iicc).
///
/// IGNORED because it FAILS against the same defect as its sibling above, filed as bd-w5j5. The
/// `Processing` cluster is rendered at x 136.62..890.21, y 92..1242 — a box that geometrically
/// contains `Idle`, `Error`, `Complete`, `fork_state`, `join_state` and both pseudo-states, none of
/// which is declared inside `state Processing { … }`. A reader sees a composite state that swallows
/// almost the entire machine.
///
/// WHY THIS CITES THE SAME BEAD rather than a new one, and it was established by experiment rather
/// than assumed: rendering a composite that declares NO inner `[*]` produces a correct, tight cluster
/// with every non-member outside it. The swell is therefore caused by the pseudo-state merge pulling
/// shared start/end nodes — and the outer states positioned near them — into the composite's member
/// set. Fixing bd-w5j5 should fix this; if it does not, this guard is what says so.
///
/// Membership is read FROM THE FIXTURE's block structure, so the assertion cannot be satisfied by
/// re-blessing. Un-ignoring this is part of bd-w5j5's acceptance gate.
#[test]
fn composite_state_cluster_contains_only_its_declared_members() {
    let svg = render_fixture("state_composite");
    let declared = state_declarations("state_composite");
    let clusters = cluster_boxes(&svg);
    let boxes = node_boxes_by_declared_id(&svg);

    assert!(
        !declared.composites.is_empty(),
        "state_composite declares no composite state; this guard would pass by checking nothing"
    );

    let mut intruders: Vec<String> = Vec::new();
    for (composite, members) in &declared.composites {
        let Some(cluster) = clusters.iter().find(|c| &c.label == composite) else {
            intruders.push(format!(
                "composite {composite:?} has no cluster labelled for it; read {:?}",
                clusters.iter().map(|c| &c.label).collect::<Vec<_>>()
            ));
            continue;
        };
        assert!(
            !members.is_empty(),
            "composite {composite:?} parsed with no members, so this guard cannot tell an intruder \
             from a member"
        );
        for node in &boxes {
            // The composite's own node is excluded here: that it is drawn at all is a separate
            // defect (bd-9w54) with its own guard, and folding it in would make this one fail for
            // two reasons at once.
            if members.contains(&node.id) || &node.id == composite {
                continue;
            }
            // GATE CORRECTION (bd-w5j5): a pseudo-state node that belongs to THIS composite, whose
            // block declares the matching `[*]`, is a declared member. The membership reader used
            // to drop `[*]` entirely, which was harmless only while every `[*]` collapsed onto one
            // global node — once they are scoped, the composite's own start/end are drawn inside
            // its cluster, correctly, and were being reported as intruders.
            let owns_pseudo = |marker: &str, prefix: &str| {
                members.iter().any(|m| m == marker)
                    && node.id.starts_with(prefix)
                    && node.id[prefix.len()..].contains(composite.as_str())
            };
            if owns_pseudo("[*]-start", "__state_start") || owns_pseudo("[*]-end", "__state_end") {
                continue;
            }
            let inside = node.x >= cluster.x
                && node.x + node.width <= cluster.x + cluster.width
                && node.y >= cluster.y
                && node.y + node.height <= cluster.y + cluster.height;
            if inside {
                intruders.push(format!(
                    "{} (x {:.2}..{:.2}, y {:.2}..{:.2}) is NOT declared inside {composite:?} yet \
                     sits entirely within its cluster (x {:.2}..{:.2}, y {:.2}..{:.2})",
                    node.id,
                    node.x,
                    node.x + node.width,
                    node.y,
                    node.y + node.height,
                    cluster.x,
                    cluster.x + cluster.width,
                    cluster.y,
                    cluster.y + cluster.height
                ));
            }
        }
    }

    assert!(
        intruders.is_empty(),
        "{} state(s) are drawn inside a composite that does not declare them:\n  {}",
        intruders.len(),
        intruders.join("\n  ")
    );
}

/// A composite state must be drawn ONCE (bd-iicc).
///
/// IGNORED because it FAILS against a real defect, filed as bd-9w54. `state Processing { … }` renders
/// as a cluster container labelled "Processing" AND, separately, as a plain rounded node box also
/// labelled "Processing". The same state appears twice in one picture — in state_composite the extra
/// box sits inside its own container, which reads as "Processing contains a state called Processing".
///
/// It is not merely cosmetic duplication: `Idle --> Processing : start` attaches to the plain box, so
/// the container that actually holds the sub-states has no incoming transition and the composite's
/// entry point is drawn pointing at a decoy.
///
/// This is checked on BOTH state fixtures' composite declarations rather than on a pinned name, and
/// it is independent of bd-w5j5 — it reproduces in a composite with no inner `[*]`, where the cluster
/// is otherwise correct. Un-ignoring this is bd-9w54's acceptance gate.
#[test]
fn a_composite_state_is_not_also_drawn_as_a_plain_node() {
    let svg = render_fixture("state_composite");
    let declared = state_declarations("state_composite");
    let clusters = cluster_boxes(&svg);
    let boxes = node_boxes_by_declared_id(&svg);

    let mut duplicated: Vec<String> = Vec::new();
    for (composite, _) in &declared.composites {
        let has_cluster = clusters.iter().any(|c| &c.label == composite);
        assert!(
            has_cluster,
            "composite {composite:?} rendered no cluster container at all, so this guard cannot \
             tell duplication from a missing container; clusters read {:?}",
            clusters.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
        if let Some(node) = boxes.iter().find(|b| &b.id == composite) {
            duplicated.push(format!(
                "{composite:?} is drawn as its cluster container AND as a plain node box at \
                 (x {:.2}, y {:.2}, {:.2}x{:.2}) carrying the same label",
                node.x, node.y, node.width, node.height
            ));
        }
    }

    assert!(
        duplicated.is_empty(),
        "{} composite state(s) are drawn twice:\n  {}",
        duplicated.len(),
        duplicated.join("\n  ")
    );
}

/// One rendered flowchart node: its visible label and a signature of the geometry primitive the
/// renderer chose for it, normalised so position and label width cannot affect the comparison.
struct RenderedShape {
    label: String,
    /// Element name plus the shape-defining attribute, with absolute coordinates stripped: `rect`
    /// keyed on its corner radius as a FRACTION of its height, `path` on its vertex count, and the
    /// round family on the element name alone. Two declared shapes that produce equal signatures
    /// are indistinguishable on screen.
    signature: String,
    corner_radius_ratio: Option<f32>,
}

/// Pull each `fm-node` group's label and shape signature out of a rendered flowchart.
///
/// Reads the whole group rather than stopping at the first `</g>`: the subroutine shape wraps its
/// rect and its two inner bars in a nested `<g>`, so a naive split truncates before the `<text>` and
/// reports a labelled node as unlabelled — which is indistinguishable at a glance from a real defect.
fn rendered_node_shapes(svg: &str) -> Vec<RenderedShape> {
    let attr = |tag: &str, name: &str| -> Option<f32> {
        let key = format!("{name}=\"");
        let start = tag.find(&key)? + key.len();
        let rest = &tag[start..];
        rest[..rest.find('"')?].parse().ok()
    };

    let mut out = Vec::new();
    for chunk in svg.split("<g id=\"fm-node").skip(1) {
        // The group ends at the LAST `</g>` before the next node group starts, so take the whole
        // chunk: anything after it belongs to a different node and carries a different data-id.
        let label = chunk
            .split("<text")
            .skip(1)
            .filter_map(|t| {
                let open = t.find('>')?;
                let close = t[open + 1..].find("</text>")?;
                Some(t[open + 1..open + 1 + close].to_string())
            })
            .collect::<Vec<_>>()
            .join("");
        if label.is_empty() {
            continue;
        }
        let Some(tag) = chunk.split('<').find(|t| {
            t.starts_with("rect ")
                || t.starts_with("circle ")
                || t.starts_with("ellipse ")
                || t.starts_with("polygon ")
                || t.starts_with("path ")
        }) else {
            continue;
        };
        let element = tag
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        let (signature, corner_radius_ratio) = match element.as_str() {
            "rect" => {
                let height = attr(tag, "height").unwrap_or(1.0).max(1.0);
                let ratio = attr(tag, "rx").unwrap_or(0.0) / height;
                // Bars distinguish a subroutine from a plain rect of the same radius.
                let bars = chunk.matches("<line").count();
                (format!("rect(rx/h={ratio:.3},bars={bars})"), Some(ratio))
            }
            "path" => {
                // Counting vertices is NOT enough to identify a polygon: a diamond, a
                // parallelogram, a trapezoid and both their inversions are all four-sided, and a
                // vertex count reports them as the same picture (bd-p0y6). Normalise the vertices
                // into their own bounding box instead, so the signature is the SHAPE — scale- and
                // position-independent, which is what "these two look the same" actually means.
                let coords: Vec<(f32, f32)> = tag
                    .split(['M', 'L'])
                    .skip(1)
                    .filter_map(|part| {
                        let mut it = part.split_whitespace();
                        let x: f32 = it.next()?.parse().ok()?;
                        let y: f32 = it.next()?.trim_end_matches('Z').trim().parse().ok()?;
                        Some((x, y))
                    })
                    .collect();
                let (min_x, max_x, min_y, max_y) = coords.iter().fold(
                    (f32::MAX, f32::MIN, f32::MAX, f32::MIN),
                    |(a, b, c, d), &(x, y)| (a.min(x), b.max(x), c.min(y), d.max(y)),
                );
                let (w, h) = ((max_x - min_x).max(1e-3), (max_y - min_y).max(1e-3));
                let outline = coords
                    .iter()
                    .map(|&(x, y)| format!("{:.2},{:.2}", (x - min_x) / w, (y - min_y) / h))
                    .collect::<Vec<_>>()
                    .join(" ");
                (format!("path[{outline}]"), None)
            }
            other => (other.to_string(), None),
        };
        out.push(RenderedShape {
            label,
            signature,
            corner_radius_ratio,
        });
    }
    out
}

/// Every DECLARED node shape must render as a shape distinguishable from the others (bd-iicc).
///
/// A byte golden proves the picture has not changed; it cannot prove two different declared shapes
/// were ever drawn differently. `all_node_shapes.mmd` declares eight distinct shapes precisely so
/// that collapsing any pair is a defect, and the golden pins a collapse exactly as happily as a
/// correct render.
///
/// Un-ignored by bd-3w93, which added the missing `([…])` Stadium probe. This guard is what proved
/// the defect and is now its regression gate.
#[test]
fn flowchart_declared_node_shapes_stay_distinct() {
    let rendered = render_fixture("all_node_shapes");
    let shapes = rendered_node_shapes(&rendered);

    // Anchored to the FIXTURE's declarations, not to the golden's contents, so re-blessing cannot
    // satisfy it. Order matches all_node_shapes.mmd.
    let declared = [
        "Rectangle",
        "Rounded",
        "Stadium",
        "Subroutine",
        "Diamond",
        "Hexagon",
        "Circle",
        "Asymmetric",
        // The five bd-3w93 repaired. All five rendered as plain rectangles until 779817ed, and
        // nothing in the corpus drew them, so their render path was unpinned by both the byte
        // goldens and this guard (bd-p0y6).
        "Database",
        "Parallelogram",
        "InvParallelogram",
        "Trapezoid",
        "InvTrapezoid",
    ];

    // The label check is not cosmetic: shape delimiters are SYNTAX, so a label carrying them back
    // ("[Stadium]") is direct evidence the shape token was never recognised.
    for label in declared {
        assert!(
            shapes.iter().any(|s| s.label == label),
            "declared node {label:?} does not appear with that exact label; rendered labels are \
             {:?} — brackets surviving into a label mean the shape token was parsed as content",
            shapes.iter().map(|s| &s.label).collect::<Vec<_>>()
        );
    }

    // THE PROPERTY THIS GUARD EXISTS FOR: eight declared shapes, eight distinguishable pictures.
    let mut collisions = Vec::new();
    for (i, a) in shapes.iter().enumerate() {
        for b in shapes.iter().skip(i + 1) {
            if a.signature == b.signature {
                collisions.push(format!(
                    "{:?} and {:?} both render as {} — different declared shapes, identical picture",
                    a.label, b.label, a.signature
                ));
            }
        }
    }
    assert!(
        collisions.is_empty(),
        "{} declared node shape(s) collapsed onto another shape:\n  {}",
        collisions.len(),
        collisions.join("\n  ")
    );

    // A stadium is a PILL: its corner radius is exactly half its height, which is what makes the
    // ends semicircular. This is the assertion a plausible-looking wrong picture fails — a rounded
    // rectangle with a merely largish radius looks fine in isolation and is still the wrong shape.
    let stadium = shapes
        .iter()
        .find(|s| s.label == "Stadium")
        .and_then(|s| s.corner_radius_ratio);
    let rounded = shapes
        .iter()
        .find(|s| s.label == "Rounded")
        .and_then(|s| s.corner_radius_ratio);
    if let (Some(stadium), Some(rounded)) = (stadium, rounded) {
        assert!(
            (stadium - 0.5).abs() < 0.01,
            "stadium corner radius must be half its height to form a pill, but renders at \
             {stadium:.3} of its height"
        );
        assert!(
            stadium > rounded,
            "stadium ({stadium:.3} of height) must be more rounded than the plain rounded \
             rectangle ({rounded:.3}), otherwise the two shapes are the same picture"
        );
    }
}

/// Every DECLARED edge type must render with the decoration that distinguishes it (bd-iicc).
///
/// `all_edge_types.mmd` declares nine edge syntaxes whose whole purpose is to look different:
/// solid/dotted/thick strokes crossed with arrow, plain, circle, cross and bidirectional ends. A
/// byte golden cannot tell you whether two of them collapsed onto one rendering.
#[test]
fn flowchart_declared_edge_types_render_their_declared_decoration() {
    let rendered = render_fixture("all_edge_types");

    // (target label, dashed, stroke-width is the thick one, marker-end, marker-start)
    let expected = [
        ("Arrow", false, false, Some("arrow-end"), None),
        ("Line", false, false, None, None),
        ("Dotted Arrow", true, false, Some("arrow-end"), None),
        ("Dotted Line", true, false, None, None),
        ("Thick Arrow", false, true, Some("arrow-filled"), None),
        ("Thick Line", false, true, None, None),
        ("Circle End", false, false, Some("arrow-circle"), None),
        ("Cross End", false, false, Some("arrow-cross"), None),
        (
            "Double Arrow",
            false,
            false,
            Some("arrow-end"),
            Some("arrow-start"),
        ),
    ];

    let edges: Vec<(String, String)> = rendered
        .split("<g id=\"fm-edge")
        .skip(1)
        .filter_map(|chunk| {
            let end = chunk.find("</g>").unwrap_or(chunk.len());
            let chunk = &chunk[..end];
            let title_start = chunk.find("<title>")? + 7;
            let title_end = chunk[title_start..].find("</title>")?;
            Some((
                chunk[title_start..title_start + title_end].to_string(),
                chunk.to_string(),
            ))
        })
        .collect();

    let mut thin_widths = Vec::new();
    let mut thick_widths = Vec::new();
    for (target, dashed, thick, marker_end, marker_start) in expected {
        // The title names both endpoints, so matching on the target label ties each rendered edge
        // back to the fixture line that declared it.
        let found = edges.iter().find(|(title, _)| title.ends_with(target));
        assert!(
            found.is_some(),
            "no rendered edge ends at declared target {target:?}; rendered edges are {:?}",
            edges.iter().map(|(t, _)| t).collect::<Vec<_>>()
        );
        let Some((title, body)) = found else { continue };

        assert_eq!(
            body.contains("stroke-dasharray"),
            dashed,
            "edge {title:?} dashed-ness does not match its declared syntax"
        );
        assert_eq!(
            body.contains("marker-start"),
            marker_start.is_some(),
            "edge {title:?} start-marker presence does not match its declared syntax"
        );
        match marker_end {
            Some(marker) => assert!(
                body.contains(&format!("marker-end=\"url(#{marker})\"")),
                "edge {title:?} must terminate in {marker}, so its head is distinguishable from \
                 the other declared end styles"
            ),
            None => assert!(
                !body.contains("marker-end"),
                "edge {title:?} is declared without an arrowhead but renders one"
            ),
        }

        let width: f32 = body
            .split("stroke-width=\"")
            .nth(1)
            .and_then(|t| t.find('"').and_then(|i| t[..i].parse().ok()))
            .unwrap_or(f32::NAN);
        assert!(width.is_finite(), "edge {title:?} has no stroke-width");
        if thick {
            thick_widths.push(width);
        } else {
            thin_widths.push(width);
        }
    }

    // Relative, not a pinned number: `==>` must actually be drawn heavier than `-->`. Asserting the
    // literal widths would re-pin what the golden already pins and would break on a restyle that
    // preserves the distinction.
    let thinnest_thick = thick_widths.iter().copied().fold(f32::INFINITY, f32::min);
    let thickest_thin = thin_widths.iter().copied().fold(0.0_f32, f32::max);
    assert!(
        thinnest_thick > thickest_thin,
        "every thick edge must be drawn heavier than every normal edge, but the lightest thick \
         edge is {thinnest_thick} and the heaviest normal edge is {thickest_thin}"
    );
}

/// bd-a6l4: a `stateDiagram` note must reach the SVG.
///
/// `note right of X : text` parsed into `ir.state_notes` and was then read by exactly one thing in
/// the whole workspace — `memo_ir_equal`, the incremental equality probe. No layout pass positioned
/// it and no renderer drew it, so the engine accepted the statement and silently produced a document
/// that did not contain it. Measured before the fix: the complete `<text>` content of the diagram
/// below was `Idle | Running | Crashed` with the note string absent.
///
/// This is an END-TO-END assertion on the rendered bytes on purpose. The layout-side tests in
/// fm-layout pin the geometry, but only the rendered document proves the pipeline is connected —
/// the defect was precisely a stage that held correct data and never handed it on.
#[test]
fn state_diagram_notes_reach_the_rendered_svg() {
    let source = "stateDiagram-v2\n    Idle --> Running\n    Running --> Crashed\n    \
                  note right of Crashed : Restart required\n    note left of Idle : Waiting for input\n";
    let svg = render_svg_with_config(&parse(source).ir, &SvgRenderConfig::default());

    for expected in ["Restart required", "Waiting for input"] {
        assert!(
            svg.contains(expected),
            "the note text {expected:?} never reached the SVG; rendered text runs were {:?}",
            svg_text_runs(&svg)
        );
    }
    assert_eq!(
        svg.matches("fm-state-note-leader").count(),
        2,
        "each note must be joined to its state by a leader line"
    );

    // CONTAINMENT: a note drawn outside the viewBox is as invisible as one never drawn, which is
    // the failure mode bd-zwh3 documents for sequence fragment frames.
    let view_box = svg
        .split("viewBox=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_default()
        .to_string();
    let bounds: Vec<f32> = view_box
        .split_whitespace()
        .filter_map(|token| token.parse().ok())
        .collect();
    assert_eq!(bounds.len(), 4, "unparsable viewBox {view_box:?}");
    for note in svg.match_indices("class=\"fm-state-note\"") {
        let element_start = svg[..note.0].rfind("<rect").unwrap_or(0);
        let element = &svg[element_start..note.0];
        let attr = |name: &str| -> f32 {
            element
                .split(&format!("{name}=\""))
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .and_then(|value| value.parse().ok())
                .unwrap_or(f32::NAN)
        };
        let (x, y, w, h) = (attr("x"), attr("y"), attr("width"), attr("height"));
        assert!(
            x >= bounds[0] - 0.51
                && y >= bounds[1] - 0.51
                && x + w <= bounds[0] + bounds[2] + 0.51
                && y + h <= bounds[1] + bounds[3] + 0.51,
            "note box ({x:.2},{y:.2},{w:.2},{h:.2}) is drawn outside viewBox {view_box:?} and \
             would be clipped"
        );
    }
}

/// NEGATIVE CONTROL for the test above: a state diagram that declares no notes must render with no
/// note markup at all. Without this, an implementation that emitted an empty note box for every
/// state would still satisfy the assertions above.
#[test]
fn a_state_diagram_without_notes_emits_no_note_markup() {
    let svg = render_svg_with_config(
        &parse("stateDiagram-v2\n    Idle --> Running\n").ir,
        &SvgRenderConfig::default(),
    );
    assert!(!svg.contains("fm-state-note"));
}

/// Collect the text content of every `<text>`/`<tspan>` run, for failure messages that say what WAS
/// rendered instead of only what was not.
fn svg_text_runs(svg: &str) -> Vec<String> {
    let mut runs = Vec::new();
    let mut rest = svg;
    while let Some(open) = rest.find("<text") {
        let after = &rest[open..];
        let Some(gt) = after.find('>') else { break };
        let Some(close) = after.find("</text>") else {
            break;
        };
        if close > gt {
            runs.push(after[gt + 1..close].to_string());
        }
        rest = &after[close + 7..];
    }
    runs
}
