//! `radar-beta`: the second family from the registry sweep (bd-sk4dv).
//!
//! THE GAP. `registry_probe.mjs` asks the pinned mermaid 11.15.0 bundle to detect and render 27
//! candidate headers; it ships `radar`, `treemap` and `info` and we had none. bd-9ghyo built
//! treemap; this builds radar. Before it, `radar-beta` was DETECTED AS A FLOWCHART and its `axis`
//! and `curve` lines became graph nodes.
//!
//! REFERENCE GEOMETRY, measured against that bundle in Chromium 151 (`scratchpad/radar_probe.mjs`)
//! and reproduced here EXACTLY, not approximately. For `axis a, b, c / curve x{1,2,3}` upstream
//! emits, about a centre of its own:
//!
//! ```text
//! graticule  circles at r = 60, 120, 180, 240, 300
//! axis tips  (0,-300)  (259.808,150)  (-259.808,150)      i.e. -90 deg + i*360/n, clockwise
//! curve      M 0,-100
//!            C 73.612,-108.5   217.372,57.5    173.205,100
//!            C 129.038,142.5  -230.363,184    -259.808,150
//!            C -289.252,116    -73.612,-91.5   0,-100  Z
//! ```
//!
//! The smoothing is a CLOSED CARDINAL SPLINE with k = 0.17, solved from those control points rather
//! than guessed: `C1_i = P_i + (P_{i+1} - P_{i-1})*k` and `C2_i = P_{i+1} - (P_{i+2} - P_i)*k`
//! reproduce every one of them to the digit.
//!
//! Scale: `radius = (v - min)/(max - min) * 300`, `min` defaulting to 0 and `max` to the largest
//! value present — which is why `{1,2,3}` and `{2,4,6}` render byte-identically upstream.
//!
//! THE NEGATIVE CASE is this bead's rule transposed from shapes: a new family must render
//! DIFFERENTLY from the fallback it used to collapse into — `Rect` for a shape, the flowchart here.

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

const BASIC: &str = "radar-beta\n  axis a, b, c\n  curve x{1,2,3}\n";

/// Our wheel centre, so measured upstream coordinates (which are relative to ITS centre) can be
/// compared directly. Layout puts the wheel at 350,350 on a 700x700 canvas and the document adds
/// the renderer's standard 40px padding.
const CX: f64 = 390.0;
const CY: f64 = 390.0;

fn numbers(svg: &str, key: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let mut rest = svg;
    let needle = format!("{key}=\"");
    while let Some(at) = rest.find(&needle) {
        let tail = &rest[at + needle.len()..];
        let end = tail.find('"').expect("unterminated attribute");
        if let Ok(v) = tail[..end].parse::<f64>() {
            out.push(v);
        }
        rest = &tail[end..];
    }
    out
}

/// The first curve's `d`, with every coordinate shifted back to be relative to the wheel centre.
fn curve_points(svg: &str) -> Vec<(f64, f64)> {
    let at = svg.find("fm-radar-curve").expect("no radar curve drawn");
    let tail = &svg[at..];
    let d_at = tail.find("d=\"").expect("curve has no d");
    let d = &tail[d_at + 3..];
    let d = &d[..d.find('"').expect("unterminated d")];
    let mut out = Vec::new();
    for token in d
        .replace(['M', 'C', 'Z'], " ")
        .split_whitespace()
        .flat_map(|t| t.split(' '))
        .filter(|t| !t.is_empty())
    {
        let Some((x, y)) = token.split_once(',') else {
            continue;
        };
        if let (Ok(x), Ok(y)) = (x.parse::<f64>(), y.parse::<f64>()) {
            out.push((x - CX, y - CY));
        }
    }
    out
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.02
}

/// THE NEGATIVE CASE: a radar must not render as the flowchart it used to collapse into.
#[test]
fn a_radar_does_not_render_as_the_flowchart_it_used_to_be() {
    let radar = render(BASIC);
    let as_flowchart = render("flowchart TD\n  axis a, b, c\n  curve x{1,2,3}\n");
    assert_ne!(
        radar, as_flowchart,
        "a radar renders identically to the flowchart fallback"
    );
    assert!(
        !radar.contains("class=\"fm-edge"),
        "a radar drew graph edges: it is still being routed as a flowchart"
    );
    assert!(
        !radar.contains("fm-node-shape-"),
        "a radar drew graph node shapes: its axis and curve lines became nodes"
    );
    assert!(radar.contains("fm-radar-curve"), "no curve was drawn");
}

/// Only `radar-beta` is accepted, because only `radar-beta` is accepted upstream.
///
/// A bare `radar` measurably fails to parse there ("No diagram type detected matching given
/// configuration for text: radar"), and this repo already answers for it by NAMING the `-beta`
/// spelling. Accepting the bare form would both render a document mermaid refuses and silence the
/// message that tells the author the working spelling — the second being the worse half.
#[test]
fn the_bare_radar_spelling_is_still_refused_and_still_names_the_working_one() {
    let beta = fm_parser::parse(BASIC);
    assert_eq!(beta.ir.diagram_type, fm_core::DiagramType::Radar);
    assert!(beta.warnings.is_empty(), "warnings: {:?}", beta.warnings);

    let bare = fm_parser::parse("radar\n  axis a, b\n  curve x{1,2}\n");
    assert_ne!(
        bare.ir.diagram_type,
        fm_core::DiagramType::Radar,
        "the bare `radar` spelling was accepted; mermaid rejects it"
    );
    assert!(
        bare.warnings.iter().any(|w| w.contains("radar-beta")),
        "the bare spelling no longer names the one that works: {:?}",
        bare.warnings
    );
}

/// The graticule is `ticks` rings evenly spaced out to 300, defaulting to 5.
#[test]
fn the_graticule_is_five_evenly_spaced_rings_by_default() {
    let radii = numbers(&render(BASIC), "r");
    assert_eq!(radii, vec![60.0, 120.0, 180.0, 240.0, 300.0]);
    let three = numbers(
        &render("radar-beta\n  axis a, b, c\n  curve x{1,2,3}\n  ticks 3\n"),
        "r",
    );
    assert_eq!(three, vec![100.0, 200.0, 300.0], "`ticks 3` was ignored");
}

/// Axis 0 points straight up and the rest run clockwise: `-90 + i*360/n`.
#[test]
fn axes_start_at_twelve_oclock_and_run_clockwise() {
    let svg = render("radar-beta\n  axis a, b, c, d\n  curve x{1,1,1,1}\n");
    let xs = numbers(&svg, "x2");
    let ys = numbers(&svg, "y2");
    assert_eq!(xs.len(), 4, "expected four spokes");
    let relative: Vec<(f64, f64)> = xs
        .iter()
        .zip(&ys)
        .map(|(x, y)| ((x - CX).round(), (y - CY).round()))
        .collect();
    assert_eq!(
        relative,
        vec![(0.0, -300.0), (300.0, 0.0), (0.0, 300.0), (-300.0, 0.0)],
        "four axes are not at -90, 0, 90, 180 degrees"
    );
}

/// THE SCALE, and the negative half of it: a radius must depend on the value.
///
/// A wheel that placed every vertex on the outer ring would look like a radar and say nothing. The
/// discriminating assertion is that `{1,2,3}` puts its three vertices at DIFFERENT radii in the
/// measured 100/200/300 ratio, which no value-blind implementation produces.
#[test]
fn vertex_radius_is_proportional_to_value() {
    let points = curve_points(&render(BASIC));
    // The path is M P0, then per segment C c1 c2 P_next — so vertices are at 0, 3, 6.
    let radius = |p: (f64, f64)| p.0.hypot(p.1);
    let r0 = radius(points[0]);
    let r1 = radius(points[3]);
    let r2 = radius(points[6]);
    assert!(close(r0, 100.0), "value 1 is at r={r0}, not 100");
    assert!(close(r1, 200.0), "value 2 is at r={r1}, not 200");
    assert!(close(r2, 300.0), "value 3 is at r={r2}, not 300");
    assert!(
        (r0 - r1).abs() > 50.0 && (r1 - r2).abs() > 50.0,
        "the vertices share a radius, so the wheel is value-blind"
    );
}

/// The scale is a pure RATIO: doubling every value changes nothing.
#[test]
fn the_scale_is_relative_so_doubling_every_value_changes_nothing() {
    assert_eq!(
        curve_points(&render(BASIC)),
        curve_points(&render("radar-beta\n  axis a, b, c\n  curve x{2,4,6}\n")),
        "the scale is absolute, but upstream renders {{1,2,3}} and {{2,4,6}} identically"
    );
}

/// `min` is part of the scale, not a display bound.
///
/// The discriminating case: `min 1` over `{1,2,3}` must put the FIRST vertex at the origin.
/// A `v / max` implementation puts it at r=100 and passes every other assertion in this file.
#[test]
fn min_moves_the_origin_of_the_scale() {
    let points = curve_points(&render(
        "radar-beta\n  axis a, b, c\n  curve x{1,2,3}\n  min 1\n",
    ));
    let r0 = points[0].0.hypot(points[0].1);
    assert!(
        close(r0, 0.0),
        "with min 1 the value 1 should sit at the origin, but r={r0}"
    );
}

/// `max` overrides the observed maximum.
#[test]
fn max_overrides_the_observed_maximum() {
    let points = curve_points(&render(
        "radar-beta\n  axis a, b, c\n  curve x{1,2,3}\n  max 10\n",
    ));
    let r0 = points[0].0.hypot(points[0].1);
    assert!(
        close(r0, 30.0),
        "with max 10 the value 1 sits at r={r0}, not 30"
    );
}

/// The smoothing reproduces upstream's control points exactly.
///
/// These six numbers are READ OFF the pinned bundle, not derived from our own output. They are the
/// reason the tension is 0.17 and not a plausible-looking round number: any other k reproduces the
/// vertices and none of the control points, which is a difference no vertex-only test can see.
#[test]
fn the_curve_smoothing_matches_upstream_control_points() {
    let points = curve_points(&render(BASIC));
    let expected = [
        (0.0, -100.0),
        (73.612, -108.5),
        (217.372, 57.5),
        (173.205, 100.0),
        (129.038, 142.5),
        (-230.363, 184.0),
        (-259.808, 150.0),
    ];
    for (index, &(ex, ey)) in expected.iter().enumerate() {
        let (ax, ay) = points[index];
        assert!(
            (ax - ex).abs() < 0.02 && (ay - ey).abs() < 0.02,
            "point {index} is ({ax}, {ay}); upstream has ({ex}, {ey})"
        );
    }
}

/// `graticule polygon` changes the graticule AND the curve, which its name does not suggest.
#[test]
fn graticule_polygon_also_straightens_the_curve() {
    let svg = render("radar-beta\n  axis a, b, c\n  curve x{1,2,3}\n  graticule polygon\n");
    assert!(
        !svg.contains("<circle class=\"fm-radar-graticule\""),
        "polygon mode still drew circular rings"
    );
    assert_eq!(
        svg.matches("<polygon class=\"fm-radar-graticule\"").count(),
        5,
        "expected five polygon rings"
    );
    let at = svg.find("fm-radar-curve").expect("no curve");
    let d_at = svg[at..].find("d=\"").expect("no d") + at;
    let d = &svg[d_at + 3..];
    let d = &d[..d.find('"').expect("unterminated d")];
    assert!(
        !d.contains('C'),
        "polygon mode still smoothed the curve: {d}"
    );
    assert_eq!(d.matches('L').count(), 2, "expected two straight segments");
}

/// Display labels are drawn when given, identifiers when not.
#[test]
fn display_labels_win_over_identifiers() {
    let plain = render(BASIC);
    assert!(
        plain.contains(">a</text>"),
        "the axis identifier is not drawn"
    );
    let labelled = render(
        "radar-beta\n  axis a[\"Alpha\"], b[\"Beta\"], c[\"Gamma\"]\n  curve x[\"Ex\"]{1,2,3}\n",
    );
    for expected in [
        ">Alpha</text>",
        ">Beta</text>",
        ">Gamma</text>",
        ">Ex</text>",
    ] {
        assert!(labelled.contains(expected), "missing {expected}");
    }
    assert!(
        !labelled.contains(">a</text>"),
        "the identifier was drawn alongside its display label"
    );
}

/// `showLegend false` suppresses the legend, and it is drawn otherwise.
#[test]
fn the_legend_is_drawn_unless_suppressed() {
    assert!(
        render(BASIC).contains("fm-radar-legend-box"),
        "no legend was drawn by default"
    );
    assert!(
        !render("radar-beta\n  axis a, b, c\n  curve x{1,2,3}\n  showLegend false\n")
            .contains("fm-radar-legend-box"),
        "`showLegend false` was ignored"
    );
}

/// Two curves are drawn as two distinctly-coloured series.
#[test]
fn each_series_gets_its_own_curve_and_colour() {
    let svg = render("radar-beta\n  axis a, b, c\n  curve x{1,2,3}\n  curve y{3,2,1}\n");
    assert_eq!(
        svg.matches("class=\"fm-radar-curve fm-radar-curve-")
            .count(),
        2,
        "expected two curves"
    );
    assert!(svg.contains("fm-radar-curve-0") && svg.contains("fm-radar-curve-1"));
    assert!(
        svg.contains(">x</text>") && svg.contains(">y</text>"),
        "the legend does not name both series"
    );
}
