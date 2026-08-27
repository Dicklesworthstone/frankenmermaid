//! `radar-beta` layout: the wheel's polar geometry (bd-sk4dv).
//!
//! The SVG-level tests in `fm-render-svg/tests/radar_render.rs` pin the drawn output against
//! coordinates measured from the pinned mermaid 11.15.0 bundle. This file pins the two properties
//! that live in layout and are hard to see from the markup: that a radar is TILED rather than
//! routed as a graph, and that a curve always closes over every axis even when the document gives
//! it fewer values than there are axes.

use fm_layout::layout_diagram;

fn radar(source: &str) -> fm_layout::LayoutRadar {
    let ir = fm_parser::parse(source).ir;
    layout_diagram(&ir)
        .extensions
        .radar
        .clone()
        .expect("no radar in the layout extensions")
}

/// A radar reaches the radar layout, not the general graph one.
///
/// Asserted on the artifact the renderer consumes: no other algorithm in the crate writes
/// `extensions.radar`, so its presence IS the evidence the wheel was laid out rather than routed.
#[test]
fn a_radar_reaches_the_radar_layout() {
    let ir = fm_parser::parse("radar-beta\n  axis a, b, c\n  curve x{1,2,3}\n").ir;
    assert_eq!(ir.diagram_type, fm_core::DiagramType::Radar);
    let layout = layout_diagram(&ir);
    assert!(
        layout.extensions.radar.is_some(),
        "a radar produced no wheel, so it fell through to the general graph selector"
    );
    assert!(
        layout.nodes.is_empty(),
        "a radar produced graph node boxes, which nothing draws"
    );
}

/// A curve closes over EVERY axis, even when it declares fewer values than there are axes.
///
/// The discriminating case: three axes, two values. Emitting one vertex per VALUE gives a
/// two-point "ring" that closes across the wheel and looks like a deliberate shape; the missing
/// axis has to be drawn at the scale origin instead so the gap is visible as a gap.
#[test]
fn a_short_curve_still_closes_over_every_axis() {
    let wheel = radar("radar-beta\n  axis a, b, c\n  curve x{1,2}\n");
    assert_eq!(wheel.axes.len(), 3);
    assert_eq!(wheel.curves.len(), 1);
    assert_eq!(
        wheel.curves[0].points.len(),
        3,
        "the curve has one vertex per VALUE rather than one per AXIS"
    );
    // The absent third value sits at the scale origin, i.e. on the centre.
    let third = wheel.curves[0].points[2];
    let distance = (f64::from(third.x) - f64::from(wheel.center.x))
        .hypot(f64::from(third.y) - f64::from(wheel.center.y));
    assert!(
        distance < 0.01,
        "the missing value was not placed at the origin: r={distance}"
    );
}

/// Ring radii divide the outer radius evenly, whatever `ticks` says.
#[test]
fn rings_divide_the_outer_radius_evenly() {
    for (ticks, expected) in [
        ("", vec![60.0_f32, 120.0, 180.0, 240.0, 300.0]),
        ("  ticks 2\n", vec![150.0, 300.0]),
        ("  ticks 6\n", vec![50.0, 100.0, 150.0, 200.0, 250.0, 300.0]),
    ] {
        let source = format!("radar-beta\n  axis a, b, c\n  curve x{{1,2,3}}\n{ticks}");
        let wheel = radar(&source);
        assert_eq!(wheel.rings, expected, "wrong rings for {ticks:?}");
        assert!((wheel.outer_radius - 300.0).abs() < f32::EPSILON);
    }
}

/// A document with no curves still lays out its wheel rather than collapsing.
///
/// `scale_max` has nothing to observe here, and a zero denominator would make every radius NaN —
/// which propagates into the path data and produces an SVG that renders as nothing at all, with no
/// error anywhere to say why.
#[test]
fn a_wheel_with_no_curves_is_still_finite() {
    let wheel = radar("radar-beta\n  axis a, b, c\n");
    assert_eq!(wheel.axes.len(), 3);
    assert!(wheel.curves.is_empty());
    for axis in &wheel.axes {
        assert!(
            axis.tip.x.is_finite() && axis.tip.y.is_finite(),
            "an axis tip is not finite: {axis:?}"
        );
    }
}
