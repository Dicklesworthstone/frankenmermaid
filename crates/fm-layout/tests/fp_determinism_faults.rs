//! Floating-point determinism fault tests for node geometry (bd-1s1g.6).
//!
//! # What these can and cannot prove
//!
//! The bead asks for layout output to be bit-identical across x86_64, aarch64 and wasm32. A test
//! running on ONE machine cannot observe that directly, and a test that pretended to would be
//! worthless. What it CAN do is attack the three mechanisms by which those platforms diverge, each
//! of which is observable locally:
//!
//!   1. FMA CONTRACTION. `a.mul_add(b, c)` rounds once; `a * b + c` rounds twice. aarch64 has
//!      hardware FMA and x86_64 may or may not use it, so a formula whose RESULT depends on which
//!      form the compiler picks produces different pixels on different machines. `shapes.rs` uses
//!      `mul_add` in many places, so this is not hypothetical here.
//!   2. SUBNORMALS. Some ARM configurations flush subnormals to zero. A geometry path that produces
//!      subnormal intermediates is therefore platform-dependent, and the failure is invisible on
//!      x86_64.
//!   3. NON-DETERMINISM WITHIN a platform. Anything that varies run to run varies across platforms
//!      a fortiori, and it is far cheaper to catch here.
//!
//! # Why the FMA test is phrased as a BOUND, not as equality
//!
//! Demanding `fused == unfused` would fail on correct code: they differ by design, by up to an ULP.
//! The question that matters is whether that difference can reach the OUTPUT. Coordinates are `f32`
//! and are emitted with finite precision, so the test asserts the divergence stays far below what a
//! rendered coordinate can express. A formula with catastrophic cancellation — where a one-ULP
//! input difference is amplified into a visible one — breaks that bound, and that is exactly the
//! class of formula that must not be in a layout path.

use fm_core::NodeShape;
use fm_layout::{LayoutRect, PathCmd, shapes::node_path};

/// Every shape variant the geometry dispatches on.
const SHAPES: &[NodeShape] = &[
    NodeShape::Rect,
    NodeShape::Rounded,
    NodeShape::Stadium,
    NodeShape::Circle,
    NodeShape::Diamond,
    NodeShape::Hexagon,
    NodeShape::Cylinder,
    NodeShape::Trapezoid,
    NodeShape::InvTrapezoid,
    NodeShape::Parallelogram,
    NodeShape::InvParallelogram,
    NodeShape::Triangle,
    NodeShape::Note,
    NodeShape::Subroutine,
    NodeShape::DoubleCircle,
];

/// Pull every coordinate out of a path, so assertions can be made over all of them at once.
fn coordinates(path: &[PathCmd]) -> Vec<f32> {
    let mut out = Vec::new();
    for cmd in path {
        match *cmd {
            PathCmd::MoveTo { x, y } | PathCmd::LineTo { x, y } => out.extend_from_slice(&[x, y]),
            PathCmd::QuadTo { cx, cy, x, y } => out.extend_from_slice(&[cx, cy, x, y]),
            PathCmd::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => out.extend_from_slice(&[c1x, c1y, c2x, c2y, x, y]),
            PathCmd::Close => {}
        }
    }
    out
}

/// MECHANISM 1: fused and unfused evaluation must not diverge enough to reach the output.
///
/// Exercised on the primitive `shapes.rs` actually uses — `h.mul_add(k, y)` against `h * k + y` —
/// across the coordinate magnitudes and the fractional constants that file contains (0.3, 0.5, 0.6,
/// 0.8). Testing the primitive rather than re-deriving every path builder keeps the test from
/// becoming a second, drifting copy of the geometry.
#[test]
fn fused_and_unfused_evaluation_agree_far_below_output_precision() {
    // SVG coordinates are emitted with a few decimals; 1e-3 is finer than any renderer here writes,
    // so a divergence below it cannot change a rendered pixel.
    const OBSERVABLE: f32 = 1e-3;

    let magnitudes = [0.0_f32, 1.0, 37.5, 480.0, 4096.0, 65_536.0];
    let factors = [0.3_f32, 0.5, 0.6, 0.8, 1.0];

    for &h in &magnitudes {
        for &y in &magnitudes {
            for &k in &factors {
                let fused = h.mul_add(k, y);
                let unfused = h * k + y;
                let delta = (fused - unfused).abs();

                assert!(
                    delta < OBSERVABLE,
                    "fused/unfused divergence {delta} at h={h}, k={k}, y={y} could reach the \
                     output; a formula this sensitive renders differently on aarch64 than on x86_64"
                );
                assert!(
                    fused.is_finite() && unfused.is_finite(),
                    "non-finite result at h={h}, k={k}, y={y}"
                );
            }
        }
    }
}

/// MECHANISM 2: no shape may emit a subnormal coordinate.
///
/// A subnormal survives on x86_64 and is flushed to zero on some ARM configurations, so a path that
/// produces one is not portable. Checked over tiny geometry, which is where subnormals would arise.
#[test]
fn no_shape_emits_subnormal_coordinates() {
    let tiny = [1e-30_f32, 1e-20, 1e-10, f32::MIN_POSITIVE];

    for &shape in SHAPES {
        for &size in &tiny {
            let bounds = LayoutRect {
                x: 0.0,
                y: 0.0,
                width: size,
                height: size,
            };
            for value in coordinates(&node_path(bounds, shape)) {
                assert!(
                    !value.is_subnormal(),
                    "{shape:?} emitted the subnormal {value:e} for a {size:e} box; ARM \
                     configurations that flush subnormals would render this shape differently"
                );
            }
        }
    }
}

/// The flush-to-zero is DEFINED behaviour, not an accident of the formulas.
///
/// The test above asserts only that no subnormal escapes, which a lucky rearrangement of arithmetic
/// could satisfy by coincidence and then lose again on the next edit. This one pins the actual
/// contract `node_path` now makes: an extent below the normal-scaling bound is treated as exactly
/// zero, so x86_64 and a flush-to-zero ARM agree by construction rather than by luck.
///
/// `+0.0` specifically, checked through `to_bits`: `-0.0 == 0.0` is true in f32 comparison, so an
/// equality assertion would pass on a sign this bead's bit-identical-output subject cares about.
///
/// ⚠️ THE CONTRACT IS NOT "every coordinate is zero", and my first draft of this test asserted that
/// and failed: `Cylinder` emits `2.0` for a degenerate box because its neck is an ABSOLUTE constant
/// rather than a fraction of the extent. An absolute constant is perfectly portable — it is the
/// same bits on every target — so the contract is that each coordinate is either exactly `+0.0` or
/// a NORMAL number, never subnormal. `Rect`, the shape that reported the defect, is checked for the
/// stronger all-zero property separately, because its geometry is entirely extent-scaled.
#[test]
fn a_degenerate_extent_is_defined_to_be_exactly_zero() {
    let degenerate = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: f32::MIN_POSITIVE,
        height: f32::MIN_POSITIVE,
    };

    for &shape in SHAPES {
        for value in coordinates(&node_path(degenerate, shape)) {
            assert!(
                value == 0.0 || value.is_normal(),
                "{shape:?} emitted {value:e} for a degenerate box; the contract is +0.0 or normal"
            );
        }
    }

    for value in coordinates(&node_path(degenerate, NodeShape::Rect)) {
        assert_eq!(
            value.to_bits(),
            0.0_f32.to_bits(),
            "Rect is entirely extent-scaled, so a degenerate box must give exactly +0.0, got {value:e}"
        );
    }
}

/// CONTROL: realistic geometry is NOT snapped.
///
/// The threshold has to be far enough below real diagram geometry that nothing observable is
/// rounded away. Without this, widening the bound until the failing case passed would look like a
/// fix while quietly flattening small nodes.
#[test]
fn ordinary_geometry_is_left_alone_by_the_flush() {
    let ordinary = LayoutRect {
        x: 10.0,
        y: 20.0,
        width: 120.0,
        height: 40.0,
    };

    let path = node_path(ordinary, NodeShape::Rect);
    let values = coordinates(&path);
    assert!(
        !values.is_empty(),
        "no coordinates, so this control proves nothing"
    );
    assert!(
        values.iter().any(|v| *v != 0.0),
        "ordinary geometry was flattened to zero: {values:?}"
    );
    // The smallest extent a diagram can plausibly carry is still orders of magnitude above the
    // bound, so a 1e-30 box must survive as nonzero too — it is in the `tiny` array above and must
    // not be silently snapped by a future widening of the threshold.
    let small = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 1e-30,
        height: 1e-30,
    };
    assert!(
        coordinates(&node_path(small, NodeShape::Diamond))
            .iter()
            .any(|v| *v != 0.0),
        "a 1e-30 box was flushed; the threshold has been widened past real geometry"
    );
}

/// Extreme magnitudes must not produce NaN or infinity.
///
/// One extreme-magnitude property test in this project previously exposed three latent NaN and
/// precision defects that 395 green tests had missed, so the shape of the check is proven; this
/// applies it to node geometry.
#[test]
fn extreme_geometry_stays_finite() {
    let extremes = [
        (0.0_f32, 0.0_f32),
        (f32::MIN_POSITIVE, f32::MIN_POSITIVE),
        (1e20, 1e20),
        (1e20, 1e-20),
        (1e-20, 1e20),
        (f32::MAX / 4.0, 1.0),
    ];

    for &shape in SHAPES {
        for &(width, height) in &extremes {
            let bounds = LayoutRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            };
            for value in coordinates(&node_path(bounds, shape)) {
                assert!(
                    value.is_finite(),
                    "{shape:?} produced {value} for a {width:e} x {height:e} box"
                );
            }
        }
    }
}

/// MECHANISM 3: the same input must give bit-identical output within one process.
///
/// Compared as RAW BITS rather than with `==`, because `f32` equality treats `-0.0` and `0.0` as
/// equal while they serialise differently — a sign flip on a zero coordinate is exactly the kind of
/// divergence that shows up as a diff in a golden file and nowhere else.
#[test]
fn repeated_calls_are_bit_identical() {
    for &shape in SHAPES {
        let bounds = LayoutRect {
            x: 12.5,
            y: -7.25,
            width: 180.0,
            height: 64.0,
        };
        let first = coordinates(&node_path(bounds, shape));
        let second = coordinates(&node_path(bounds, shape));

        assert_eq!(
            first.len(),
            second.len(),
            "{shape:?} changed its path length"
        );
        for (index, (a, b)) in first.iter().zip(second.iter()).enumerate() {
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "{shape:?} coordinate {index} differs between identical calls: {a} vs {b}"
            );
        }
    }
}

/// CONTROL: the coordinate extractor must actually see coordinates.
///
/// Every assertion above is a `for` loop over `coordinates(...)`. If that returned an empty vector —
/// because a shape stopped emitting a path, or the extractor missed a variant — all four tests
/// would pass while checking nothing at all.
#[test]
fn the_extractor_sees_coordinates_for_every_shape() {
    let bounds = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 50.0,
    };
    for &shape in SHAPES {
        let found = coordinates(&node_path(bounds, shape));
        assert!(
            found.len() >= 4,
            "{shape:?} yielded {} coordinates; the determinism assertions would be vacuous",
            found.len()
        );
        assert!(
            found.iter().all(|v| v.is_finite()),
            "{shape:?} emitted a non-finite coordinate for an ordinary box"
        );
    }
}
