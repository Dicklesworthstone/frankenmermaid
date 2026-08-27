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
        // The notched pentagon takes the conservative full box, its cuts only REMOVING area.
        NodeShape::FlippedTriangle => flipped_triangle_path(bounds),
        NodeShape::NotchedPentagon => rounded_rect_path(bounds, 0.0),
        NodeShape::Pentagon => polygon_path(bounds, 5, -std::f32::consts::FRAC_PI_2),
        NodeShape::Star => star_path(bounds, 5),
        // The burst is sparse like the bolt, so its own outline is the boundary rather than a box.
        // The curved trapezoid reaches the box edges at its bulges and is narrow elsewhere; the box
        // is the conservative stop and its own outline would be fiddly for little gain.
        NodeShape::CurvedTrapezoid | NodeShape::BowTieRect => rounded_rect_path(bounds, 0.0),
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
        // bd-7ls21. This function returns the OUTER BOUNDARY used for edge clipping and hit
        // testing, not the drawn decoration — the same reason `Subroutine` returns a plain box
        // rather than its double frame. A lined rectangle's rule and a notched rectangle's cut are
        // interior detail; only the notch actually changes the silhouette, and it does so by
        // REMOVING area, so a full box is the conservative boundary an edge can safely stop at.
        NodeShape::NotchedRect | NodeShape::LinedRect | NodeShape::DividedRect => {
            rounded_rect_path(bounds, 0.0)
        }
        // The small circle is a fixed-radius marker, but its BOUNDS still come from layout, so the
        // boundary follows the bounds like every other round shape.
        NodeShape::SmallCircle | NodeShape::FramedCircle => polygon_ellipse_path(bounds, 24),
        // bd-7ls21. The sloped rectangle gets its OWN boundary — its top edge is not horizontal, so
        // a full box would let an edge stop inside the empty wedge above the slope.
        NodeShape::SlopedRect => sloped_rect_path(bounds),
        // ⚠️ The horizontal cylinder takes the conservative BOX, and deliberately does NOT reuse
        // `cylinder_path`: that one is the VERTICAL capsule, whose caps are on the top and bottom.
        // Reusing it would be the `FlippedTriangle` mistake — a boundary rotated ninety degrees away
        // from the drawn shape. A box is a superset of the capsule, so an edge stops slightly early
        // rather than in the wrong place; a rotated capsule builder would be strictly better and is
        // the obvious follow-up.
        NodeShape::HorizontalCylinder => rounded_rect_path(bounds, 0.0),
        // The tagged rectangle keeps its FULL box — the fold is drawn over the corner, not cut out of
        // it, so unlike `NotchedRect` there is no removed area to be conservative about. The lined
        // cylinder is a cylinder with one extra rim, so it shares `cylinder_path` exactly.
        NodeShape::TaggedRect => rounded_rect_path(bounds, 0.0),
        NodeShape::LinedCylinder => cylinder_path(bounds),
        // The document's wave dips BELOW its box and crests above the bottom edge, so neither the
        // box nor a tighter outline is a clean superset. The box is the conservative choice: an
        // edge stops on the nominal bottom rather than inside the trough.
        NodeShape::Document | NodeShape::LinedDocument | NodeShape::TaggedDocument => {
            rounded_rect_path(bounds, 0.0)
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
