use crate::{LayoutRect, PathCmd};
use fm_core::NodeShape;
use std::f32::consts::PI;

/// The smallest extent these formulas can scale without landing in the subnormal range (bd-1s1g.6).
///
/// Some ARM configurations FLUSH SUBNORMALS TO ZERO while x86_64 keeps them, so a geometry path
/// that produces a subnormal renders differently on the two platforms — and the divergence is
/// invisible on x86_64, which is where it gets tested. `Rect` reached one the direct way: a box of
/// `f32::MIN_POSITIVE` is a normal number, but its own half is not, so `width / 2.0` produced
/// `5.877472e-39` for a perfectly finite input.
///
/// The bead offers two acceptable outcomes — no subnormal results, or explicit flush-to-zero
/// EVERYWHERE — and this takes the second: below this threshold an extent is defined to be exactly
/// zero on every target, rather than left to the platform's subnormal policy.
///
/// The 1024 is not arbitrary. The smallest coefficient any shape here applies to an extent is well
/// above 1/1024 (the tightest are the hexagon and trapezoid insets at ~0.2, and corner radii which
/// are absolute, not scaled), so an extent at or above this bound cannot be scaled into the
/// subnormal range by any of them. Anything below it is 30-plus orders of magnitude smaller than
/// the smallest geometry a diagram can produce, so nothing real is being rounded away.
const MIN_NORMAL_EXTENT: f32 = f32::MIN_POSITIVE * 1024.0;

/// Flush a degenerate coordinate to exactly `+0.0` so every target agrees on it.
///
/// Returns `+0.0` for both signs of a tiny magnitude: `-0.0` and `+0.0` compare equal but are not
/// bit-identical, and this bead's whole subject is bit-identical output across targets.
#[inline]
fn canonical_extent(value: f32) -> f32 {
    if value.abs() < MIN_NORMAL_EXTENT {
        0.0
    } else {
        value
    }
}

/// Canonicalize a box once per path, rather than every coordinate it emits.
///
/// Four comparisons per node against a path of dozens of coordinates — the check is at the entry
/// point precisely so it does not sit inside the emit loops. It is NOT free, and it has not been
/// measured; it is claimed only to be cheap relative to allocating and filling the `Vec<PathCmd>`
/// it guards.
#[inline]
fn canonical_bounds(bounds: LayoutRect) -> LayoutRect {
    LayoutRect {
        x: canonical_extent(bounds.x),
        y: canonical_extent(bounds.y),
        width: canonical_extent(bounds.width),
        height: canonical_extent(bounds.height),
    }
}

#[must_use]
pub fn node_path(bounds: LayoutRect, shape: NodeShape) -> Vec<PathCmd> {
    // Every shape dispatches from here, so one canonicalization covers all of them. Putting it in
    // the individual builders would be the same rule written fifteen times, and a sixteenth shape
    // added later would silently miss it.
    let bounds = canonical_bounds(bounds);
    match shape {
        NodeShape::Rect => rounded_rect_path(bounds, 5.0),
        NodeShape::Rounded => rounded_rect_path(bounds, 10.0),
        NodeShape::Stadium => stadium_path(bounds),
        NodeShape::Diamond => diamond_path(bounds),
        NodeShape::Hexagon => hexagon_path(bounds),
        NodeShape::Circle | NodeShape::FilledCircle | NodeShape::DoubleCircle => {
            polygon_ellipse_path(bounds, 24)
        }
        NodeShape::Cylinder => cylinder_path(bounds),
        NodeShape::Trapezoid => trapezoid_path(bounds),
        NodeShape::HorizontalBar => horizontal_bar_path(bounds),
        NodeShape::InvTrapezoid => inv_trapezoid_path(bounds),
        NodeShape::Parallelogram => parallelogram_path(bounds),
        NodeShape::InvParallelogram => inv_parallelogram_path(bounds),
        NodeShape::Asymmetric => asymmetric_path(bounds),
        NodeShape::Note => note_path(bounds),
        NodeShape::Triangle => triangle_path(bounds),
        // bd-7ls21. The flipped triangle gets its OWN boundary rather than reusing `triangle_path`:
        // the two fill opposite halves of the box, so sharing one would clip edges into empty space.
        NodeShape::FlippedTriangle => flipped_triangle_path(bounds),
        NodeShape::NotchedPentagon => notched_pentagon_path(bounds),
        NodeShape::Pentagon => polygon_path(bounds, 5, -std::f32::consts::FRAC_PI_2),
        NodeShape::Star => star_path(bounds, 5),
        // The burst is sparse like the bolt, so its own outline is the boundary rather than a box.
        // The curved trapezoid reaches the box edges at its bulges and is narrow elsewhere; the box
        // is the conservative stop and its own outline would be fiddly for little gain.
        NodeShape::CurvedTrapezoid | NodeShape::BowTieRect => rounded_rect_path(bounds, 0.0),
        // The hourglass is sparse — two lobes with an empty waist — so its own outline is a
        // better edge stop than the box, the same call the bolt and the burst take.
        NodeShape::Hourglass => hourglass_path(bounds),
        // bd-7ls21's last batch. The window pane and the data store are full boxes whose extra
        // rules are INTERIOR detail, so the box is their exact boundary, not a conservative one.
        NodeShape::WindowPane | NodeShape::DataStore => rounded_rect_path(bounds, 0.0),
        // A text block paints nothing, so an edge has no outline to stop at and the box is the only
        // honest answer: it is where the LABEL is, which is what an arrow to a text block points at.
        NodeShape::TextBlock => rounded_rect_path(bounds, 0.0),
        // The braces are sparse in the extreme — a stroke down one edge and nothing else — but the
        // box is still the right stop, for the same reason as the text block: the thing an edge is
        // aimed at is the commented label, not the bracket beside it.
        NodeShape::BraceLeft | NodeShape::BraceRight | NodeShape::Braces => {
            rounded_rect_path(bounds, 0.0)
        }
        NodeShape::Bang => {
            star_path_with_ratio(bounds, fm_core::BANG_POINTS, fm_core::BANG_INNER_RATIO)
        }
        NodeShape::Cloud => cloud_path(bounds),
        NodeShape::Tag => tag_path(bounds),
        NodeShape::Subroutine => {
            // For composite shapes, we use the primary boundary path.
            // Inner lines are added by specialized render logic if needed,
            // but for simple path representation we return the outer box.
            rounded_rect_path(bounds, 0.0)
        }
        NodeShape::CrossedCircle => polygon_ellipse_path(bounds, 24),
        // Rules are interior decoration, but a notch changes the actual outer boundary.
        NodeShape::NotchedRect => notched_rect_path(bounds),
        NodeShape::LinedRect | NodeShape::DividedRect => rounded_rect_path(bounds, 0.0),
        // These markers have a fixed outer radius in SVG and Canvas, independent of the label box.
        // FramedCircle's inner ring is decoration and must not become another clipping boundary.
        NodeShape::SmallCircle | NodeShape::FramedCircle => small_circle_path(bounds),
        // bd-7ls21. The sloped rectangle gets its OWN boundary — its top edge is not horizontal, so
        // a full box would let an edge stop inside the empty wedge above the slope.
        NodeShape::SlopedRect => sloped_rect_path(bounds),
        // Side caps, not the top/bottom caps of cylinder_path, and no interior near-rim subpath.
        NodeShape::HorizontalCylinder => horizontal_cylinder_path(bounds),
        // The tagged rectangle keeps its FULL box — the fold is drawn over the corner, not cut out of
        // it, so unlike `NotchedRect` there is no removed area to be conservative about. The lined
        // cylinder is a cylinder with one extra rim, so it shares `cylinder_path` exactly.
        NodeShape::TaggedRect => rounded_rect_path(bounds, 0.0),
        NodeShape::LinedCylinder => cylinder_path(bounds),
        // All three share the same wavy perimeter. The rule and fold are interior decoration.
        NodeShape::Document | NodeShape::LinedDocument | NodeShape::TaggedDocument => {
            document_path(bounds)
        }
        // The bolt is mostly empty box — a full-box boundary would let edges stop far from any ink.
        // Its own outline is the right stop, and unlike the cylinder there is no rotation subtlety.
        NodeShape::LightningBolt => lightning_bolt_path(bounds),
        // The flag mostly fills its box (only the wave crests cut in), so the box is the right
        // conservative boundary — the opposite call from the bolt, which is sparse.
        NodeShape::Flag => rounded_rect_path(bounds, 0.0),
        // The half-rounded rectangle fills its box except for the two corners the cap rounds off,
        // so the box is a close superset and the right conservative boundary.
        NodeShape::HalfRoundedRect => rounded_rect_path(bounds, 0.0),
        // The stack fills its box corner to corner between the back and front copies, so the box is
        // the right boundary; an edge stopping on it lands on one of the three outlines.
        NodeShape::StackedDocument | NodeShape::StackedRect => rounded_rect_path(bounds, 0.0),
    }
}

/// A single top-left cut, matching the SVG and Canvas notch rather than its enclosing rectangle.
#[must_use]
pub fn notched_rect_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let LayoutRect { x, y, width: w, height: h } = bounds;
    let notch = (w.min(h) * 0.31).min(w / 2.0).min(h / 2.0);
    vec![
        PathCmd::MoveTo { x: x + notch, y },
        PathCmd::LineTo { x: x + w, y },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::LineTo { x, y: y + notch },
        PathCmd::Close,
    ]
}

/// Two top-corner cuts, with the same independently scaled axes as the rendered shape.
#[must_use]
pub fn notched_pentagon_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let LayoutRect { x, y, width: w, height: h } = bounds;
    let cut_x = w * fm_core::NOTCHED_PENTAGON_CUT_X_RATIO;
    let cut_y = h * fm_core::NOTCHED_PENTAGON_CUT_Y_RATIO;
    vec![
        PathCmd::MoveTo { x: x + cut_x, y },
        PathCmd::LineTo { x: x + w - cut_x, y },
        PathCmd::LineTo { x: x + w, y: y + cut_y },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::LineTo { x, y: y + cut_y },
        PathCmd::Close,
    ]
}

/// The horizontal cylinder's perimeter, using four quarter-ellipse cubics as Canvas does.
///
/// Only the outside belongs here: including the near rim would create a second hit/clip contour.
#[must_use]
pub fn horizontal_cylinder_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let LayoutRect { x, y, width: w, height: h } = bounds;
    let rx = w * 0.1;
    let ry = h / 2.0;
    let left = x + rx;
    let right = x + w - rx;
    let cy = y + ry;
    // 4/3 * tan(pi/8), rounded once to f32; no target-dependent trig in the path builder.
    let kx = rx * 0.552_284_8;
    let ky = ry * 0.552_284_8;
    vec![
        PathCmd::MoveTo { x: left, y },
        PathCmd::LineTo { x: right, y },
        PathCmd::CubicTo {
            c1x: right + kx, c1y: y,
            c2x: x + w, c2y: cy - ky,
            x: x + w, y: cy,
        },
        PathCmd::CubicTo {
            c1x: x + w, c1y: cy + ky,
            c2x: right + kx, c2y: y + h,
            x: right, y: y + h,
        },
        PathCmd::LineTo { x: left, y: y + h },
        PathCmd::CubicTo {
            c1x: left - kx, c1y: y + h,
            c2x: x, c2y: cy + ky,
            x, y: cy,
        },
        PathCmd::CubicTo {
            c1x: x, c1y: cy - ky,
            c2x: left - kx, c2y: y,
            x: left, y,
        },
        PathCmd::Close,
    ]
}

/// A document's wavy bottom, using the same two quadratics as SVG and Canvas.
#[must_use]
pub fn document_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let LayoutRect { x, y, width: w, height: h } = bounds;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo { x: x + w, y },
        PathCmd::LineTo { x: x + w, y: y + h * 0.80 },
        PathCmd::QuadTo {
            cx: x + w * 0.75, cy: y + h * 0.84,
            x: x + w * 0.5, y: y + h * 0.95,
        },
        PathCmd::QuadTo {
            cx: x + w * 0.25, cy: y + h * 1.06,
            x, y: y + h * 0.90,
        },
        PathCmd::Close,
    ]
}

/// Fixed-size marker perimeter centered in the layout box, not stretched to the box dimensions.
#[must_use]
pub fn small_circle_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let radius = fm_core::SMALL_CIRCLE_RADIUS;
    polygon_ellipse_path(
        LayoutRect {
            x: bounds.x + bounds.width / 2.0 - radius,
            y: bounds.y + bounds.height / 2.0 - radius,
            width: radius * 2.0,
            height: radius * 2.0,
        },
        24,
    )
}

#[must_use]
pub fn stadium_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let r = bounds.width.min(bounds.height) / 2.0;
    rounded_rect_path(bounds, r)
}

#[must_use]
pub fn hexagon_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let cy = y + h / 2.0;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x: x + inset, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y,
        },
        PathCmd::LineTo { x: x + w, y: cy },
        PathCmd::LineTo {
            x: x + w - inset,
            y: y + h,
        },
        PathCmd::LineTo {
            x: x + inset,
            y: y + h,
        },
        PathCmd::LineTo { x, y: cy },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn cylinder_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let ry = (h * 0.1).max(2.0);
    let rx = w / 2.0;

    vec![
        PathCmd::MoveTo { x, y: y + ry },
        PathCmd::QuadTo {
            cx: x + rx,
            cy: y - ry,
            x: x + w,
            y: y + ry,
        },
        PathCmd::LineTo {
            x: x + w,
            y: y + h - ry,
        },
        PathCmd::QuadTo {
            cx: x + rx,
            cy: y + h + ry,
            x,
            y: y + h - ry,
        },
        PathCmd::LineTo { x, y: y + ry },
        PathCmd::Close,
        PathCmd::MoveTo { x, y: y + ry },
        PathCmd::QuadTo {
            cx: x + rx,
            cy: y + (ry * 3.0),
            x: x + w,
            y: y + ry,
        },
    ]
}

#[must_use]
pub fn trapezoid_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x: x + inset, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y,
        },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn inv_trapezoid_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo { x: x + w, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y: y + h,
        },
        PathCmd::LineTo {
            x: x + inset,
            y: y + h,
        },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn parallelogram_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x: x + inset, y },
        PathCmd::LineTo { x: x + w, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y: y + h,
        },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn inv_parallelogram_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let inset = w * 0.15;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo {
            x: x + w - inset,
            y,
        },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo {
            x: x + inset,
            y: y + h,
        },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn asymmetric_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let flag = w * 0.15;
    let cy = y + h / 2.0;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo { x: x + w - flag, y },
        PathCmd::LineTo { x: x + w, y: cy },
        PathCmd::LineTo {
            x: x + w - flag,
            y: y + h,
        },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn note_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let fold = 10.0_f32.min(w * 0.4);
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo { x: x + w - fold, y },
        PathCmd::LineTo {
            x: x + w,
            y: y + fold,
        },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn triangle_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let cx = x + w / 2.0;
    vec![
        PathCmd::MoveTo { x: cx, y },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

/// [`NodeShape::Hourglass`]'s boundary: the self-crossing bowtie.
#[must_use]
pub fn hourglass_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo { x: x + w, y },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::Close,
    ]
}

/// [`NodeShape::LightningBolt`]'s boundary: the bolt's own six-vertex outline.
///
/// Given rather than a box because the bolt leaves most of its box EMPTY — a box boundary would stop
/// edges in white space on either side of the zigzag.
#[must_use]
pub fn lightning_bolt_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    vec![
        PathCmd::MoveTo { x: x + w, y },
        PathCmd::LineTo {
            x: x + w * 0.023,
            y: y + h * 0.55,
        },
        PathCmd::LineTo {
            x: x + w * 0.586,
            y: y + h * 0.55,
        },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::LineTo {
            x: x + w * 0.977,
            y: y + h * 0.45,
        },
        PathCmd::LineTo {
            x: x + w * 0.414,
            y: y + h * 0.45,
        },
        PathCmd::Close,
    ]
}

/// [`NodeShape::SlopedRect`]'s boundary: bottom edge full width, top edge sloping UP to the right.
///
/// The top-left corner drops `SLOPED_RECT_DROP_RATIO` of the height, matching the drawn shape, so an
/// edge arriving from the upper left stops on the slope rather than in the empty wedge above it.
#[must_use]
pub fn sloped_rect_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let drop = h * fm_core::SLOPED_RECT_DROP_RATIO;
    vec![
        PathCmd::MoveTo { x, y: y + drop },
        PathCmd::LineTo { x: x + w, y },
        PathCmd::LineTo { x: x + w, y: y + h },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

/// [`NodeShape::FlippedTriangle`]'s boundary: full-width top edge, apex at the bottom centre.
///
/// ⚠️ NOT `triangle_path` MIRRORED BY ACCIDENT — it is a genuinely different boundary. The upward
/// triangle fills the BOTTOM of its box and the flipped one fills the TOP, so reusing either for the
/// other puts an edge's clip point in the empty half. That is invisible in a shape test and shows up
/// only as an arrow ending in white space.
#[must_use]
pub fn flipped_triangle_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo { x: x + w, y },
        PathCmd::LineTo {
            x: x + w / 2.0,
            y: y + h,
        },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn horizontal_bar_path(bounds: LayoutRect) -> Vec<PathCmd> {
    rounded_rect_path(bounds, (bounds.height / 2.0).min(4.0))
}

#[must_use]
pub fn polygon_path(bounds: LayoutRect, sides: usize, angle_offset: f32) -> Vec<PathCmd> {
    let cx = bounds.x + (bounds.width / 2.0);
    let cy = bounds.y + (bounds.height / 2.0);
    let r = bounds.width.min(bounds.height) / 2.0;
    let mut cmds = Vec::with_capacity(sides + 1);
    for i in 0..sides {
        let angle = angle_offset + (i as f32) * 2.0 * PI / (sides as f32);
        let px = cx + r * angle.cos();
        let py = cy + r * angle.sin();
        if i == 0 {
            cmds.push(PathCmd::MoveTo { x: px, y: py });
        } else {
            cmds.push(PathCmd::LineTo { x: px, y: py });
        }
    }
    cmds.push(PathCmd::Close);
    cmds
}

#[must_use]
pub fn star_path(bounds: LayoutRect, points: usize) -> Vec<PathCmd> {
    star_path_with_ratio(bounds, points, 0.4)
}

/// A star with an explicit inner-to-outer radius ratio.
///
/// Split out for `bang` (bd-7ls21), whose 0.616 makes a rounded burst where `Star`'s 0.4 makes a
/// spiky one. `star_path` keeps its 0.4 so no existing shape moves.
#[must_use]
pub fn star_path_with_ratio(bounds: LayoutRect, points: usize, inner_ratio: f32) -> Vec<PathCmd> {
    let cx = bounds.x + (bounds.width / 2.0);
    let cy = bounds.y + (bounds.height / 2.0);
    let outer_r = bounds.width.min(bounds.height) / 2.0;
    let inner_r = outer_r * inner_ratio;
    let angle_offset = -std::f32::consts::FRAC_PI_2;
    let total_points = points * 2;
    let mut cmds = Vec::with_capacity(total_points + 1);
    for i in 0..total_points {
        let r = if i % 2 == 0 { outer_r } else { inner_r };
        let angle = angle_offset + (i as f32) * PI / (points as f32);
        let px = cx + r * angle.cos();
        let py = cy + r * angle.sin();
        if i == 0 {
            cmds.push(PathCmd::MoveTo { x: px, y: py });
        } else {
            cmds.push(PathCmd::LineTo { x: px, y: py });
        }
    }
    cmds.push(PathCmd::Close);
    cmds
}

#[must_use]
pub fn cloud_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    #[allow(clippy::many_single_char_names)]
    let r = h / 3.0;
    // Simplified cloud path
    vec![
        PathCmd::MoveTo {
            x: x + r,
            y: h.mul_add(0.6, y),
        },
        PathCmd::LineTo {
            x: x + r * 2.0,
            y: h.mul_add(0.3, y),
        },
        PathCmd::LineTo {
            x: w.mul_add(0.5, x),
            y: y + r * 0.5,
        },
        PathCmd::LineTo {
            x: x + w - r * 2.0,
            y: h.mul_add(0.3, y),
        },
        PathCmd::LineTo {
            x: x + w - r,
            y: h.mul_add(0.6, y),
        },
        PathCmd::LineTo {
            x: x + w - r,
            y: h.mul_add(0.8, y),
        },
        PathCmd::LineTo {
            x: x + r,
            y: h.mul_add(0.8, y),
        },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn tag_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;
    let point = w * 0.2;
    let cy = y + h / 2.0;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo {
            x: x + w - point,
            y,
        },
        PathCmd::LineTo { x: x + w, y: cy },
        PathCmd::LineTo {
            x: x + w - point,
            y: y + h,
        },
        PathCmd::LineTo { x, y: y + h },
        PathCmd::Close,
    ]
}

#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn rounded_rect_path(bounds: LayoutRect, radius: f32) -> Vec<PathCmd> {
    let mut commands = Vec::with_capacity(10);
    let r = radius.min(bounds.width / 2.0).min(bounds.height / 2.0);
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;

    commands.push(PathCmd::MoveTo { x: x + r, y });
    commands.push(PathCmd::LineTo { x: x + w - r, y });
    commands.push(PathCmd::QuadTo {
        cx: x + w,
        cy: y,
        x: x + w,
        y: y + r,
    });
    commands.push(PathCmd::LineTo {
        x: x + w,
        y: y + h - r,
    });
    commands.push(PathCmd::QuadTo {
        cx: x + w,
        cy: y + h,
        x: x + w - r,
        y: y + h,
    });
    commands.push(PathCmd::LineTo { x: x + r, y: y + h });
    commands.push(PathCmd::QuadTo {
        cx: x,
        cy: y + h,
        x,
        y: y + h - r,
    });
    commands.push(PathCmd::LineTo { x, y: y + r });
    commands.push(PathCmd::QuadTo {
        cx: x,
        cy: y,
        x: x + r,
        y,
    });
    commands.push(PathCmd::Close);

    commands
}

#[must_use]
pub fn diamond_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let cx = bounds.x + (bounds.width / 2.0);
    let cy = bounds.y + (bounds.height / 2.0);
    vec![
        PathCmd::MoveTo { x: cx, y: bounds.y },
        PathCmd::LineTo {
            x: bounds.x + bounds.width,
            y: cy,
        },
        PathCmd::LineTo {
            x: cx,
            y: bounds.y + bounds.height,
        },
        PathCmd::LineTo { x: bounds.x, y: cy },
        PathCmd::Close,
    ]
}

#[must_use]
pub fn polygon_ellipse_path(bounds: LayoutRect, segments: usize) -> Vec<PathCmd> {
    let segment_count = segments.max(8);
    let cx = bounds.x + (bounds.width / 2.0);
    let cy = bounds.y + (bounds.height / 2.0);
    let rx = bounds.width / 2.0;
    let ry = bounds.height / 2.0;

    let mut commands = Vec::with_capacity(segment_count + 2);
    for index in 0..segment_count {
        let theta = (index as f32 / segment_count as f32) * 2.0 * PI;
        let x = cx + (rx * theta.cos());
        let y = cy + (ry * theta.sin());
        if index == 0 {
            commands.push(PathCmd::MoveTo { x, y });
        } else {
            commands.push(PathCmd::LineTo { x, y });
        }
    }
    commands.push(PathCmd::Close);
    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_bounds() -> LayoutRect {
        LayoutRect {
            x: 10.0,
            y: 20.0,
            width: 200.0,
            height: 100.0,
        }
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.000_1, "{actual} != {expected}");
    }

    fn vertices(path: &[PathCmd]) -> Vec<(f32, f32)> {
        path.iter()
            .filter_map(|cmd| match *cmd {
                PathCmd::MoveTo { x, y } | PathCmd::LineTo { x, y } => Some((x, y)),
                _ => None,
            })
            .collect()
    }

    fn coordinates(path: &[PathCmd]) -> Vec<f32> {
        let mut out = Vec::new();
        for cmd in path {
            match *cmd {
                PathCmd::MoveTo { x, y } | PathCmd::LineTo { x, y } => {
                    out.extend_from_slice(&[x, y]);
                }
                PathCmd::QuadTo { cx, cy, x, y } => out.extend_from_slice(&[cx, cy, x, y]),
                PathCmd::CubicTo { c1x, c1y, c2x, c2y, x, y } => {
                    out.extend_from_slice(&[c1x, c1y, c2x, c2y, x, y]);
                }
                _ => {}
            }
        }
        out
    }

    fn assert_vertices(path: &[PathCmd], expected: &[(f32, f32)]) {
        let actual = vertices(path);
        assert_eq!(actual.len(), expected.len());
        for ((x, y), (expected_x, expected_y)) in actual.iter().zip(expected) {
            assert_near(*x, *expected_x);
            assert_near(*y, *expected_y);
        }
    }

    #[test]
    fn notch_boundary_omits_the_removed_top_left_corner() {
        let path = node_path(test_bounds(), NodeShape::NotchedRect);
        assert_eq!(path.len(), 6);
        assert_vertices(
            &path,
            &[(41.0, 20.0), (210.0, 20.0), (210.0, 120.0), (10.0, 120.0), (10.0, 51.0)],
        );
        assert!(matches!(path.last(), Some(PathCmd::Close)));
    }

    #[test]
    fn notched_pentagon_boundary_preserves_both_top_corner_cuts() {
        let path = node_path(test_bounds(), NodeShape::NotchedPentagon);
        assert_eq!(path.len(), 7);
        assert_vertices(
            &path,
            &[(30.0, 20.0), (190.0, 20.0), (210.0, 40.0),
                (210.0, 120.0), (10.0, 120.0), (10.0, 40.0)],
        );
        assert!(matches!(path.last(), Some(PathCmd::Close)));
    }

    #[test]
    fn horizontal_cylinder_boundary_is_one_closed_contour_with_side_caps() {
        let path = node_path(test_bounds(), NodeShape::HorizontalCylinder);
        assert_eq!(path.len(), 8);
        assert_vertices(&path, &[(30.0, 20.0), (190.0, 20.0), (30.0, 120.0)]);
        assert_eq!(path.iter().filter(|cmd| matches!(cmd, PathCmd::MoveTo { .. })).count(), 1);
        let mut start = (30.0, 20.0);
        let mut curves = 0;
        for cmd in &path {
            match *cmd {
                PathCmd::MoveTo { x, y } | PathCmd::LineTo { x, y } => start = (x, y),
                PathCmd::CubicTo { c1x, c1y, c2x, c2y, x, y } => {
                    let center_x = if curves < 2 { 190.0 } else { 30.0 };
                    for step in 0..=32 {
                        let t = step as f32 / 32.0;
                        let s = 1.0 - t;
                        let px = s.powi(3) * start.0 + 3.0 * s * s * t * c1x
                            + 3.0 * s * t * t * c2x + t.powi(3) * x;
                        let py = s.powi(3) * start.1 + 3.0 * s * s * t * c1y
                            + 3.0 * s * t * t * c2y + t.powi(3) * y;
                        let ellipse = ((px - center_x) / 20.0).powi(2)
                            + ((py - 70.0) / 50.0).powi(2);
                        assert!((ellipse - 1.0).abs() < 0.000_7);
                    }
                    start = (x, y);
                    curves += 1;
                }
                _ => {}
            }
        }
        assert_eq!(curves, 4, "interior near rim is not part of the boundary");
        assert!(matches!(path.last(), Some(PathCmd::Close)));
    }

    #[test]
    fn document_variants_share_the_wave_but_not_interior_decorations() {
        for shape in [NodeShape::Document, NodeShape::LinedDocument, NodeShape::TaggedDocument] {
            let path = node_path(test_bounds(), shape);
            assert_eq!(path.len(), 6);
            assert_vertices(&path, &[(10.0, 20.0), (210.0, 20.0), (210.0, 100.0)]);
            for (cmd, expected) in path[3..5].iter().zip([
                [160.0, 104.0, 110.0, 115.0],
                [60.0, 126.0, 10.0, 110.0],
            ]) {
                let PathCmd::QuadTo { cx, cy, x, y } = cmd else {
                    panic!("document bottom must be quadratic");
                };
                for (actual, expected) in [*cx, *cy, *x, *y].into_iter().zip(expected) {
                    assert_near(actual, expected);
                }
            }
            assert!(matches!(path.last(), Some(PathCmd::Close)));
        }
    }

    #[test]
    fn marker_boundaries_keep_fixed_radius_in_wide_and_square_layout_boxes() {
        for (width, height) in [(14.0, 14.0), (240.0, 80.0)] {
            let bounds = LayoutRect { width, height, ..test_bounds() };
            for shape in [NodeShape::SmallCircle, NodeShape::FramedCircle] {
                let path = node_path(bounds, shape);
                let points = vertices(&path);
                assert_eq!(points.len(), 24);
                for (x, y) in points {
                    let dx = x - (bounds.x + width / 2.0);
                    let dy = y - (bounds.y + height / 2.0);
                    assert!((dx * dx + dy * dy - 49.0).abs() < 0.001);
                }
                assert!(matches!(path.last(), Some(PathCmd::Close)));
            }
        }
    }

    #[test]
    fn completed_boundaries_remain_finite_closed_and_repeatable_for_degenerate_extents() {
        for (width, height) in [
            (0.0, 0.0), (0.0, 20.0), (2.0, 1000.0), (1000.0, 2.0),
            (f32::MIN_POSITIVE, f32::MIN_POSITIVE),
        ] {
            let bounds = LayoutRect { x: -15.5, y: 7.25, width, height };
            for shape in [
                NodeShape::NotchedRect, NodeShape::NotchedPentagon, NodeShape::HorizontalCylinder,
                NodeShape::Document, NodeShape::LinedDocument, NodeShape::TaggedDocument,
                NodeShape::SmallCircle, NodeShape::FramedCircle,
            ] {
                let path = node_path(bounds, shape);
                assert!(matches!(path.first(), Some(PathCmd::MoveTo { .. })));
                assert!(matches!(path.last(), Some(PathCmd::Close)));
                let actual = coordinates(&path);
                assert!(actual.iter().all(|value| value.is_finite() && !value.is_subnormal()));
                assert_eq!(
                    actual.iter().map(|value| value.to_bits()).collect::<Vec<_>>(),
                    coordinates(&node_path(bounds, shape)).iter()
                        .map(|value| value.to_bits()).collect::<Vec<_>>(),
                );
            }
        }
    }

    #[test]
    fn cylinder_path_contains_curved_caps() {
        let path = cylinder_path(LayoutRect {
            x: 10.0,
            y: 20.0,
            width: 80.0,
            height: 40.0,
        });

        assert!(matches!(path.first(), Some(PathCmd::MoveTo { .. })));
        assert!(path.iter().any(|cmd| matches!(cmd, PathCmd::QuadTo { .. })));
        assert_eq!(
            path.iter()
                .filter(|cmd| matches!(cmd, PathCmd::QuadTo { .. }))
                .count(),
            3
        );
        assert!(path.iter().any(|cmd| matches!(cmd, PathCmd::Close)));
    }

    #[test]
    fn crossed_circle_uses_circular_primary_boundary() {
        let path = node_path(
            LayoutRect {
                x: 10.0,
                y: 20.0,
                width: 60.0,
                height: 60.0,
            },
            NodeShape::CrossedCircle,
        );

        assert!(matches!(path.first(), Some(PathCmd::MoveTo { .. })));
        assert_eq!(
            path.iter()
                .filter(|cmd| matches!(cmd, PathCmd::LineTo { .. }))
                .count(),
            23
        );
        assert!(path.iter().any(|cmd| matches!(cmd, PathCmd::Close)));
    }
}
