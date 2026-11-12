use crate::{LayoutPoint, LayoutRect, PathCmd};
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

/// The square occupied by a circle drawn inside possibly non-square layout bounds.
pub(crate) fn circle_bounds(bounds: LayoutRect) -> LayoutRect {
    let diameter = bounds.width.min(bounds.height).max(0.0);
    LayoutRect {
        x: bounds.x + (bounds.width - diameter) / 2.0,
        y: bounds.y + (bounds.height - diameter) / 2.0,
        width: diameter,
        height: diameter,
    }
}

/// A port on the polygon's painted boundary, viewed from `direction` along a line through
/// `origin`. Using the emitted outline keeps routing in agreement with the renderer, including
/// the centred square used by regular polygons inside non-square layout bounds.
///
/// The line crosses the whole shape rather than stopping at its centre: an offset port can lie
/// beside a star's concave valley, with several crossings before it reaches the opposite side.
/// The first intersection approached from outside is the one whose terminal stub stays clear.
pub(crate) fn polygon_face_port(
    bounds: LayoutRect,
    shape: NodeShape,
    origin: LayoutPoint,
    direction: LayoutPoint,
) -> Option<LayoutPoint> {
    use fm_core::cga::{CgaLineSegment, CgaPoint};

    if !matches!(
        shape,
        NodeShape::Triangle | NodeShape::FlippedTriangle | NodeShape::Pentagon | NodeShape::Star
    ) || ![
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        origin.x,
        origin.y,
        direction.x,
        direction.y,
    ]
    .into_iter()
    .all(f32::is_finite)
        || bounds.width <= 0.0
        || bounds.height <= 0.0
    {
        return None;
    }
    let (dx, dy) = (f64::from(direction.x), f64::from(direction.y));
    let length = dx.hypot(dy);
    if length <= f64::EPSILON {
        return None;
    }
    let (dx, dy) = (dx / length, dy / length);
    let center_x = f64::from(bounds.x) + f64::from(bounds.width) / 2.0;
    let center_y = f64::from(bounds.y) + f64::from(bounds.height) / 2.0;
    let (ox, oy) = (f64::from(origin.x), f64::from(origin.y));
    let reach = f64::from(bounds.width).hypot(f64::from(bounds.height))
        + (ox - center_x).hypot(oy - center_y)
        + 1.0;
    let query = CgaLineSegment::new(
        CgaPoint::new(ox + dx * reach, oy + dy * reach),
        CgaPoint::new(ox - dx * reach, oy - dy * reach),
    );
    let mut nearest: Option<(f64, CgaPoint)> = None;
    let mut intersect = |from, to| {
        if let Some(hit) = query.intersect(&CgaLineSegment::new(from, to)) {
            let distance = (query.start.x - hit.x).hypot(query.start.y - hit.y);
            if nearest.is_none_or(|(previous, _)| distance < previous) {
                nearest = Some((distance, hit));
            }
        }
    };
    let mut first = None;
    let mut previous = None;
    for command in node_path(bounds, shape) {
        match command {
            PathCmd::MoveTo { x, y } => {
                let point = CgaPoint::new(f64::from(x), f64::from(y));
                first = Some(point);
                previous = Some(point);
            }
            PathCmd::LineTo { x, y } => {
                let point = CgaPoint::new(f64::from(x), f64::from(y));
                if let Some(from) = previous {
                    intersect(from, point);
                }
                previous = Some(point);
            }
            PathCmd::Close => {
                if let Some((from, to)) = previous.zip(first) {
                    intersect(from, to);
                }
            }
            _ => return None,
        }
    }
    nearest.map(|(_, point)| LayoutPoint {
        x: if direction.x == 0.0 {
            origin.x
        } else {
            point.x as f32
        },
        y: if direction.y == 0.0 {
            origin.y
        } else {
            point.y as f32
        },
    })
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
            polygon_ellipse_path(circle_bounds(bounds), 24)
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
        NodeShape::CrossedCircle => polygon_ellipse_path(circle_bounds(bounds), 24),
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
        // The arrow's own outline: its heads leave most of the box corners empty.
        NodeShape::BlockArrowRight
        | NodeShape::BlockArrowLeft
        | NodeShape::BlockArrowUp
        | NodeShape::BlockArrowDown
        | NodeShape::BlockArrowX
        | NodeShape::BlockArrowY => {
            let outline = fm_core::block_arrow_outline(
                shape,
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
            )
            .unwrap_or_default();
            let mut path = Vec::with_capacity(outline.len() + 1);
            for (x, y) in outline {
                path.push(if path.is_empty() {
                    PathCmd::MoveTo { x, y }
                } else {
                    PathCmd::LineTo { x, y }
                });
            }
            path.push(PathCmd::Close);
            path
        }
    }
}

/// One ordered piece of a node's visible geometry.
///
/// Fill and stroke are independent: an open data store has a filled body and only two stroked
/// edges, while braces and composite-shape details are never filled. Consumers paint these
/// pieces in order so the front sheets of a stack cover the sheets behind them.
#[derive(Debug, Clone, PartialEq)]
pub struct NodePaintPath {
    pub commands: Vec<PathCmd>,
    pub fill: bool,
    pub stroke: bool,
}

impl NodePaintPath {
    fn body(commands: Vec<PathCmd>) -> Self {
        Self {
            commands,
            fill: true,
            stroke: true,
        }
    }

    fn outline(commands: Vec<PathCmd>) -> Self {
        Self {
            commands,
            fill: false,
            stroke: true,
        }
    }
}

/// Visible node geometry, distinct from [`node_path`]'s routing boundary.
///
/// A text node still has a routing box but paints no box. Interior rules, folds, rings and
/// stacked sheets carry meaning and must reach scene and GPU renderers just as they reach the
/// direct SVG and Canvas surfaces. A double circle's inner ring is unfilled here; a state
/// renderer may fill that second piece to express a terminal state.
#[must_use]
pub fn node_paint_paths(bounds: LayoutRect, shape: NodeShape) -> Vec<NodePaintPath> {
    let bounds = canonical_bounds(bounds);
    let LayoutRect {
        x,
        y,
        width: w,
        height: h,
    } = bounds;
    if ![x, y, w, h].into_iter().all(f32::is_finite) || w < 0.0 || h < 0.0 {
        return Vec::new();
    }
    match shape {
        NodeShape::TextBlock => return Vec::new(),
        NodeShape::BraceLeft => {
            return vec![NodePaintPath::outline(brace_paint_path(bounds, true))];
        }
        NodeShape::BraceRight => {
            return vec![NodePaintPath::outline(brace_paint_path(bounds, false))];
        }
        NodeShape::Braces => {
            return vec![
                NodePaintPath::outline(brace_paint_path(bounds, true)),
                NodePaintPath::outline(brace_paint_path(bounds, false)),
            ];
        }
        NodeShape::DataStore => {
            return vec![
                NodePaintPath {
                    commands: rounded_rect_path(bounds, 0.0),
                    fill: true,
                    stroke: false,
                },
                NodePaintPath::outline(vec![
                    PathCmd::MoveTo { x, y },
                    PathCmd::LineTo { x: x + w, y },
                    PathCmd::MoveTo { x, y: y + h },
                    PathCmd::LineTo { x: x + w, y: y + h },
                ]),
            ];
        }
        NodeShape::StackedRect | NodeShape::StackedDocument => {
            let (dx, dy) = if shape == NodeShape::StackedDocument {
                (w * 0.165, h * 0.169)
            } else {
                (w * 0.099, h * 0.078)
            };
            return [2.0, 1.0, 0.0]
                .into_iter()
                .map(|step| {
                    let leaf = LayoutRect {
                        x: x + dx * step,
                        y: y + dy * (2.0 - step),
                        width: w - 2.0 * dx,
                        height: h - 2.0 * dy,
                    };
                    NodePaintPath::body(if shape == NodeShape::StackedDocument {
                        document_path(leaf)
                    } else {
                        rounded_rect_path(leaf, 0.0)
                    })
                })
                .collect();
        }
        _ => {}
    }

    let body = match shape {
        NodeShape::Circle
        | NodeShape::FilledCircle
        | NodeShape::DoubleCircle
        | NodeShape::CrossedCircle => ellipse_paint_path(circle_bounds(bounds)),
        NodeShape::SmallCircle | NodeShape::FramedCircle => {
            centered_circle_paint_path(bounds, fm_core::SMALL_CIRCLE_RADIUS)
        }
        NodeShape::Cylinder | NodeShape::LinedCylinder => cylinder_paint_body(bounds),
        NodeShape::CurvedTrapezoid => curved_trapezoid_paint_path(bounds),
        NodeShape::BowTieRect => bow_tie_paint_path(bounds),
        NodeShape::Flag => flag_paint_path(bounds),
        NodeShape::HalfRoundedRect => half_rounded_paint_path(bounds),
        _ => node_path(bounds, shape),
    };
    let mut paths = vec![NodePaintPath::body(body)];
    let mut detail = Vec::new();
    let mut line = |x1, y1, x2, y2| {
        detail.push(PathCmd::MoveTo { x: x1, y: y1 });
        detail.push(PathCmd::LineTo { x: x2, y: y2 });
    };
    match shape {
        NodeShape::Subroutine => {
            let inset = 8.0_f32.min(w * 0.5);
            line(x + inset, y, x + inset, y + h);
            line(x + w - inset, y, x + w - inset, y + h);
        }
        NodeShape::CrossedCircle => {
            let center = bounds.center();
            let offset = w.min(h) * 0.5 * std::f32::consts::FRAC_1_SQRT_2;
            line(
                center.x - offset,
                center.y - offset,
                center.x + offset,
                center.y + offset,
            );
            line(
                center.x + offset,
                center.y - offset,
                center.x - offset,
                center.y + offset,
            );
        }
        NodeShape::DoubleCircle => {
            let radius = w.min(h) * 0.5;
            paths.push(NodePaintPath::outline(centered_circle_paint_path(
                bounds,
                (radius - 4.0).max(radius * 0.5),
            )));
        }
        NodeShape::FramedCircle => paths.push(NodePaintPath::body(centered_circle_paint_path(
            bounds,
            fm_core::FRAMED_CIRCLE_INNER_RADIUS,
        ))),
        NodeShape::LinedRect => {
            let rule = x + (w * 0.14).min(16.0);
            line(rule, y, rule, y + h);
        }
        NodeShape::DividedRect => {
            let band = y + h * fm_core::DIVIDED_RECT_HEADER_RATIO;
            line(x, band, x + w, band);
        }
        NodeShape::WindowPane => {
            let inset_x = fm_core::WINDOW_PANE_INSET.min(w * 0.4);
            let inset_y = fm_core::WINDOW_PANE_INSET.min(h * 0.4);
            line(x, y + inset_y, x + w, y + inset_y);
            line(x + inset_x, y, x + inset_x, y + h);
        }
        NodeShape::LinedDocument => line(x + w * 0.045, y, x + w * 0.045, y + h * 0.94),
        NodeShape::TaggedRect => {
            let fold = h * fm_core::TAGGED_RECT_FOLD_RATIO;
            detail = vec![
                PathCmd::MoveTo {
                    x: x + w - fold,
                    y: y + h,
                },
                PathCmd::LineTo { x: x + w, y: y + h },
                PathCmd::LineTo {
                    x: x + w,
                    y: y + h - fold,
                },
                PathCmd::Close,
            ];
        }
        NodeShape::Note => {
            let fold = 10.0_f32.min(w * 0.4);
            detail = vec![
                PathCmd::MoveTo { x: x + w - fold, y },
                PathCmd::LineTo {
                    x: x + w - fold,
                    y: y + fold,
                },
                PathCmd::LineTo {
                    x: x + w,
                    y: y + fold,
                },
            ];
        }
        NodeShape::TaggedDocument => {
            detail = scaled_paint_path(
                bounds,
                &[
                    PathCmd::MoveTo {
                        x: 0.818,
                        y: 0.8384,
                    },
                    PathCmd::LineTo { x: 1.0, y: 0.58 },
                    PathCmd::LineTo { x: 1.0, y: 0.80 },
                    PathCmd::QuadTo {
                        cx: 0.909,
                        cy: 0.8146,
                        x: 0.818,
                        y: 0.8384,
                    },
                    PathCmd::Close,
                ],
            )
        }
        NodeShape::Cylinder | NodeShape::LinedCylinder => {
            detail = cylinder_paint_rim(bounds, y + h * 0.1);
            if shape == NodeShape::LinedCylinder {
                detail.extend(cylinder_paint_rim(bounds, y + h * 0.2));
            }
        }
        NodeShape::HorizontalCylinder => {
            let rx = w * 0.1;
            let ry = h * 0.5;
            let left = x + rx;
            let cy = y + ry;
            let kx = rx * ELLIPSE_KAPPA;
            let ky = ry * ELLIPSE_KAPPA;
            detail = vec![
                PathCmd::MoveTo { x: left, y },
                PathCmd::CubicTo {
                    c1x: left + kx,
                    c1y: y,
                    c2x: left + rx,
                    c2y: cy - ky,
                    x: left + rx,
                    y: cy,
                },
                PathCmd::CubicTo {
                    c1x: left + rx,
                    c1y: cy + ky,
                    c2x: left + kx,
                    c2y: y + h,
                    x: left,
                    y: y + h,
                },
            ];
        }
        _ => {}
    }
    if !detail.is_empty() {
        paths.push(NodePaintPath::outline(detail));
    }
    paths
}

const ELLIPSE_KAPPA: f32 = 0.552_284_8;

fn scaled_paint_path(bounds: LayoutRect, commands: &[PathCmd]) -> Vec<PathCmd> {
    let x = |value| bounds.x + bounds.width * value;
    let y = |value| bounds.y + bounds.height * value;
    commands
        .iter()
        .map(|command| match *command {
            PathCmd::MoveTo { x: px, y: py } => PathCmd::MoveTo { x: x(px), y: y(py) },
            PathCmd::LineTo { x: px, y: py } => PathCmd::LineTo { x: x(px), y: y(py) },
            PathCmd::QuadTo {
                cx,
                cy,
                x: px,
                y: py,
            } => PathCmd::QuadTo {
                cx: x(cx),
                cy: y(cy),
                x: x(px),
                y: y(py),
            },
            PathCmd::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x: px,
                y: py,
            } => PathCmd::CubicTo {
                c1x: x(c1x),
                c1y: y(c1y),
                c2x: x(c2x),
                c2y: y(c2y),
                x: x(px),
                y: y(py),
            },
            PathCmd::Close => PathCmd::Close,
        })
        .collect()
}

fn centered_circle_paint_path(bounds: LayoutRect, radius: f32) -> Vec<PathCmd> {
    let center = bounds.center();
    ellipse_paint_path(LayoutRect {
        x: center.x - radius,
        y: center.y - radius,
        width: 2.0 * radius,
        height: 2.0 * radius,
    })
}

fn ellipse_paint_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let k = ELLIPSE_KAPPA * 0.5;
    scaled_paint_path(
        bounds,
        &[
            PathCmd::MoveTo { x: 1.0, y: 0.5 },
            PathCmd::CubicTo {
                c1x: 1.0,
                c1y: 0.5 + k,
                c2x: 0.5 + k,
                c2y: 1.0,
                x: 0.5,
                y: 1.0,
            },
            PathCmd::CubicTo {
                c1x: 0.5 - k,
                c1y: 1.0,
                c2x: 0.0,
                c2y: 0.5 + k,
                x: 0.0,
                y: 0.5,
            },
            PathCmd::CubicTo {
                c1x: 0.0,
                c1y: 0.5 - k,
                c2x: 0.5 - k,
                c2y: 0.0,
                x: 0.5,
                y: 0.0,
            },
            PathCmd::CubicTo {
                c1x: 0.5 + k,
                c1y: 0.0,
                c2x: 1.0,
                c2y: 0.5 - k,
                x: 1.0,
                y: 0.5,
            },
            PathCmd::Close,
        ],
    )
}

fn cylinder_paint_body(bounds: LayoutRect) -> Vec<PathCmd> {
    scaled_paint_path(
        bounds,
        &[
            PathCmd::MoveTo { x: 0.0, y: 0.1 },
            PathCmd::LineTo { x: 0.0, y: 0.9 },
            PathCmd::CubicTo {
                c1x: 0.0,
                c1y: 0.955,
                c2x: 0.22,
                c2y: 1.0,
                x: 0.5,
                y: 1.0,
            },
            PathCmd::CubicTo {
                c1x: 0.78,
                c1y: 1.0,
                c2x: 1.0,
                c2y: 0.955,
                x: 1.0,
                y: 0.9,
            },
            PathCmd::LineTo { x: 1.0, y: 0.1 },
            PathCmd::CubicTo {
                c1x: 1.0,
                c1y: 0.045,
                c2x: 0.78,
                c2y: 0.0,
                x: 0.5,
                y: 0.0,
            },
            PathCmd::CubicTo {
                c1x: 0.22,
                c1y: 0.0,
                c2x: 0.0,
                c2y: 0.045,
                x: 0.0,
                y: 0.1,
            },
            PathCmd::Close,
        ],
    )
}

fn cylinder_paint_rim(bounds: LayoutRect, top: f32) -> Vec<PathCmd> {
    scaled_paint_path(
        LayoutRect {
            y: top,
            height: bounds.height * 0.1,
            ..bounds
        },
        &[
            PathCmd::MoveTo { x: 0.0, y: 0.0 },
            PathCmd::CubicTo {
                c1x: 0.0,
                c1y: 0.55,
                c2x: 0.22,
                c2y: 1.0,
                x: 0.5,
                y: 1.0,
            },
            PathCmd::CubicTo {
                c1x: 0.78,
                c1y: 1.0,
                c2x: 1.0,
                c2y: 0.55,
                x: 1.0,
                y: 0.0,
            },
        ],
    )
}

fn flag_paint_path(bounds: LayoutRect) -> Vec<PathCmd> {
    scaled_paint_path(
        bounds,
        &[
            PathCmd::MoveTo { x: 0.0, y: 0.917 },
            PathCmd::QuadTo {
                cx: 0.25,
                cy: 1.09,
                x: 0.5,
                y: 0.91,
            },
            PathCmd::QuadTo {
                cx: 0.75,
                cy: 0.79,
                x: 1.0,
                y: 0.893,
            },
            PathCmd::LineTo { x: 1.0, y: 0.083 },
            PathCmd::QuadTo {
                cx: 0.75,
                cy: -0.09,
                x: 0.5,
                y: 0.09,
            },
            PathCmd::QuadTo {
                cx: 0.25,
                cy: 0.21,
                x: 0.0,
                y: 0.107,
            },
            PathCmd::Close,
        ],
    )
}

fn half_rounded_paint_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let LayoutRect {
        x,
        y,
        width: w,
        height: h,
    } = bounds;
    let radius = (h * 0.5).min(w * 0.5);
    let right = x + w - radius;
    let mut path = vec![PathCmd::MoveTo { x, y }, PathCmd::LineTo { x: right, y }];
    ellipse_arc_to(&mut path, (right, y), (right, y + h), radius, h * 0.5, true);
    path.push(PathCmd::LineTo { x, y: y + h });
    path.push(PathCmd::Close);
    path
}

fn curved_trapezoid_paint_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let mut path = vec![
        PathCmd::MoveTo { x: 0.265, y: 0.0 },
        PathCmd::LineTo { x: 0.469, y: 0.0 },
    ];
    ellipse_arc_to(&mut path, (0.469, 0.0), (0.438, 1.0), 0.547, 0.5, true);
    path.push(PathCmd::LineTo { x: 0.302, y: 1.0 });
    ellipse_arc_to(&mut path, (0.302, 1.0), (0.265, 0.0), 0.276, 0.5, true);
    path.push(PathCmd::Close);
    scaled_paint_path(bounds, &path)
}

fn bow_tie_paint_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let mut path = vec![
        PathCmd::MoveTo { x: 0.179, y: 0.0 },
        PathCmd::LineTo { x: 1.0, y: 0.0 },
    ];
    ellipse_arc_to(&mut path, (1.0, 0.0), (0.978, 1.0), 0.101, 0.5, false);
    path.push(PathCmd::LineTo { x: 0.157, y: 1.0 });
    ellipse_arc_to(&mut path, (0.157, 1.0), (0.179, 0.0), 0.138, 0.5, false);
    path.push(PathCmd::Close);
    scaled_paint_path(bounds, &path)
}

/// Convert a non-rotated SVG small arc to cubic segments no larger than a quarter turn.
/// The radius correction matters for the measured asymmetric ends of display/stored-data
/// shapes: one cubic spanning the whole half ellipse visibly flattens their sides.
fn ellipse_arc_to(
    path: &mut Vec<PathCmd>,
    from: (f32, f32),
    to: (f32, f32),
    radius_x: f32,
    radius_y: f32,
    sweep: bool,
) {
    if radius_x <= 0.0 || radius_y <= 0.0 || from == to {
        path.push(PathCmd::LineTo { x: to.0, y: to.1 });
        return;
    }
    let (dx, dy) = (
        (f64::from(from.0) - f64::from(to.0)) * 0.5,
        (f64::from(from.1) - f64::from(to.1)) * 0.5,
    );
    let (mut rx, mut ry) = (f64::from(radius_x), f64::from(radius_y));
    let scale = ((dx / rx).powi(2) + (dy / ry).powi(2)).sqrt().max(1.0);
    rx *= scale;
    ry *= scale;
    let denominator = (rx * dy).powi(2) + (ry * dx).powi(2);
    let factor = (((rx * ry).powi(2) - denominator) / denominator)
        .max(0.0)
        .sqrt()
        * if sweep { -1.0 } else { 1.0 };
    let (offset_x, offset_y) = (factor * rx * dy / ry, -factor * ry * dx / rx);
    let (cx, cy) = (
        (f64::from(from.0) + f64::from(to.0)) * 0.5 + offset_x,
        (f64::from(from.1) + f64::from(to.1)) * 0.5 + offset_y,
    );
    let (ux, uy) = ((dx - offset_x) / rx, (dy - offset_y) / ry);
    let (vx, vy) = ((-dx - offset_x) / rx, (-dy - offset_y) / ry);
    let start = uy.atan2(ux);
    let mut angle = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    if sweep && angle < 0.0 {
        angle += std::f64::consts::TAU;
    } else if !sweep && angle > 0.0 {
        angle -= std::f64::consts::TAU;
    }
    let segments = (angle.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = angle / segments as f64;
    let tangent = (step * 0.25).tan() * (4.0 / 3.0);
    for index in 0..segments {
        let first = start + index as f64 * step;
        let last = first + step;
        let (sin_first, cos_first) = first.sin_cos();
        let (sin_last, cos_last) = last.sin_cos();
        path.push(PathCmd::CubicTo {
            c1x: (cx + rx * (cos_first - tangent * sin_first)) as f32,
            c1y: (cy + ry * (sin_first + tangent * cos_first)) as f32,
            c2x: (cx + rx * (cos_last + tangent * sin_last)) as f32,
            c2y: (cy + ry * (sin_last - tangent * cos_last)) as f32,
            x: if index + 1 == segments {
                to.0
            } else {
                (cx + rx * cos_last) as f32
            },
            y: if index + 1 == segments {
                to.1
            } else {
                (cy + ry * sin_last) as f32
            },
        });
    }
}

fn brace_paint_path(bounds: LayoutRect, left: bool) -> Vec<PathCmd> {
    let LayoutRect {
        x,
        y,
        width: w,
        height: h,
    } = bounds;
    let radius = (h * fm_core::BRACE_RADIUS_RATIO)
        .max(fm_core::BRACE_MIN_RADIUS)
        .min(h * 0.25)
        .min(w * 0.25);
    let (edge, direction) = if left {
        (x + 2.0 * radius, -1.0)
    } else {
        (x + w - 2.0 * radius, 1.0)
    };
    let spine = edge + direction * radius;
    let spur = edge + direction * 2.0 * radius;
    let mid = y + h * 0.5;
    let k = radius * ELLIPSE_KAPPA;
    vec![
        PathCmd::MoveTo { x: edge, y },
        PathCmd::CubicTo {
            c1x: edge + direction * k,
            c1y: y,
            c2x: spine,
            c2y: y + radius - k,
            x: spine,
            y: y + radius,
        },
        PathCmd::LineTo {
            x: spine,
            y: mid - radius,
        },
        PathCmd::CubicTo {
            c1x: spine,
            c1y: mid - radius + k,
            c2x: spur - direction * k,
            c2y: mid,
            x: spur,
            y: mid,
        },
        PathCmd::CubicTo {
            c1x: spur - direction * k,
            c1y: mid,
            c2x: spine,
            c2y: mid + radius - k,
            x: spine,
            y: mid + radius,
        },
        PathCmd::LineTo {
            x: spine,
            y: y + h - radius,
        },
        PathCmd::CubicTo {
            c1x: spine,
            c1y: y + h - radius + k,
            c2x: edge + direction * k,
            c2y: y + h,
            x: edge,
            y: y + h,
        },
    ]
}

/// A single top-left cut, matching the SVG and Canvas notch rather than its enclosing rectangle.
#[must_use]
pub fn notched_rect_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let LayoutRect {
        x,
        y,
        width: w,
        height: h,
    } = bounds;
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
    let LayoutRect {
        x,
        y,
        width: w,
        height: h,
    } = bounds;
    let cut_x = w * fm_core::NOTCHED_PENTAGON_CUT_X_RATIO;
    let cut_y = h * fm_core::NOTCHED_PENTAGON_CUT_Y_RATIO;
    vec![
        PathCmd::MoveTo { x: x + cut_x, y },
        PathCmd::LineTo {
            x: x + w - cut_x,
            y,
        },
        PathCmd::LineTo {
            x: x + w,
            y: y + cut_y,
        },
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
    let LayoutRect {
        x,
        y,
        width: w,
        height: h,
    } = bounds;
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
            c1x: right + kx,
            c1y: y,
            c2x: x + w,
            c2y: cy - ky,
            x: x + w,
            y: cy,
        },
        PathCmd::CubicTo {
            c1x: x + w,
            c1y: cy + ky,
            c2x: right + kx,
            c2y: y + h,
            x: right,
            y: y + h,
        },
        PathCmd::LineTo { x: left, y: y + h },
        PathCmd::CubicTo {
            c1x: left - kx,
            c1y: y + h,
            c2x: x,
            c2y: cy + ky,
            x,
            y: cy,
        },
        PathCmd::CubicTo {
            c1x: x,
            c1y: cy - ky,
            c2x: left - kx,
            c2y: y,
            x: left,
            y,
        },
        PathCmd::Close,
    ]
}

/// A document's wavy bottom, using the same two quadratics as SVG and Canvas.
#[must_use]
pub fn document_path(bounds: LayoutRect) -> Vec<PathCmd> {
    let LayoutRect {
        x,
        y,
        width: w,
        height: h,
    } = bounds;
    vec![
        PathCmd::MoveTo { x, y },
        PathCmd::LineTo { x: x + w, y },
        PathCmd::LineTo {
            x: x + w,
            y: y + h * 0.80,
        },
        PathCmd::QuadTo {
            cx: x + w * 0.75,
            cy: y + h * 0.84,
            x: x + w * 0.5,
            y: y + h * 0.95,
        },
        PathCmd::QuadTo {
            cx: x + w * 0.25,
            cy: y + h * 1.06,
            x,
            y: y + h * 0.90,
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
        assert!(
            (actual - expected).abs() < 0.000_1,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn polygon_face_ports_reject_invalid_geometry() {
        let bounds = test_bounds();
        let origin = bounds.center();
        let direction = crate::LayoutPoint { x: 1.0, y: 0.0 };
        assert!(super::polygon_face_port(bounds, NodeShape::Rect, origin, direction).is_none());
        assert!(
            super::polygon_face_port(
                bounds,
                NodeShape::Star,
                origin,
                crate::LayoutPoint { x: 0.0, y: 0.0 },
            )
            .is_none()
        );
        for invalid in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            assert!(
                super::polygon_face_port(
                    LayoutRect {
                        width: invalid,
                        ..bounds
                    },
                    NodeShape::Star,
                    origin,
                    direction,
                )
                .is_none()
            );
        }
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
                PathCmd::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => {
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
    fn node_paint_paths_keep_text_empty_and_braces_open() {
        let bounds = test_bounds();
        assert!(!node_path(bounds, NodeShape::TextBlock).is_empty());
        assert!(node_paint_paths(bounds, NodeShape::TextBlock).is_empty());
        let braces = node_paint_paths(bounds, NodeShape::Braces);
        assert_eq!(braces.len(), 2);
        for piece in &braces {
            assert!(!piece.fill && piece.stroke);
            assert!(!piece.commands.contains(&PathCmd::Close));
            assert_eq!(
                piece
                    .commands
                    .iter()
                    .filter(|cmd| matches!(cmd, PathCmd::CubicTo { .. }))
                    .count(),
                4,
            );
        }
        let left = coordinates(&braces[0].commands);
        let right = coordinates(&braces[1].commands);
        for (left, right) in left.as_chunks::<2>().0.iter().zip(right.as_chunks::<2>().0) {
            assert_near(left[0] + right[0], 2.0 * bounds.center().x);
            assert_near(left[1], right[1]);
        }
        assert!(braces[0].commands.iter().any(|cmd| matches!(cmd,
            PathCmd::CubicTo { x, y, .. } if *x == bounds.x && *y == bounds.center().y)));
        assert!(braces[1].commands.iter().any(|cmd| matches!(cmd,
            PathCmd::CubicTo { x, y, .. } if *x == bounds.x + bounds.width && *y == bounds.center().y)));
        assert_eq!(
            node_paint_paths(bounds, NodeShape::BraceLeft),
            [braces[0].clone()]
        );
        assert_eq!(
            node_paint_paths(bounds, NodeShape::BraceRight),
            [braces[1].clone()]
        );
    }

    #[test]
    fn node_paint_paths_preserve_open_sides_and_composite_details() {
        let bounds = test_bounds();
        let store = node_paint_paths(bounds, NodeShape::DataStore);
        assert_eq!(store.len(), 2);
        assert!(store[0].fill && !store[0].stroke);
        assert!(!store[1].fill && store[1].stroke);
        assert_vertices(
            &store[1].commands,
            &[(10.0, 20.0), (210.0, 20.0), (10.0, 120.0), (210.0, 120.0)],
        );
        assert!(!store[1].commands.contains(&PathCmd::Close));

        let subroutine = node_paint_paths(bounds, NodeShape::Subroutine);
        assert_eq!(subroutine.len(), 2);
        assert!(subroutine[0].fill && !subroutine[1].fill);
        assert_vertices(
            &subroutine[1].commands,
            &[(18.0, 20.0), (18.0, 120.0), (202.0, 20.0), (202.0, 120.0)],
        );

        let crossed = node_paint_paths(bounds, NodeShape::CrossedCircle);
        assert_eq!(crossed.len(), 2);
        assert!(!crossed[1].fill);
        for (x, y) in vertices(&crossed[1].commands) {
            assert_near((x - bounds.center().x).hypot(y - bounds.center().y), 50.0);
        }
        for (shape, radius, inner_radius, inner_fill) in [
            (NodeShape::DoubleCircle, 50.0, 46.0, false),
            (
                NodeShape::FramedCircle,
                fm_core::SMALL_CIRCLE_RADIUS,
                fm_core::FRAMED_CIRCLE_INNER_RADIUS,
                true,
            ),
        ] {
            let circles = node_paint_paths(bounds, shape);
            assert_eq!(circles.len(), 2);
            assert!(circles[0].fill);
            assert_eq!(circles[1].fill, inner_fill);
            for (piece, radius) in circles.iter().zip([radius, inner_radius]) {
                assert_eq!(
                    piece.commands[0],
                    PathCmd::MoveTo {
                        x: bounds.center().x + radius,
                        y: bounds.center().y
                    }
                );
                assert!(piece.stroke);
            }
        }
    }

    #[test]
    fn node_paint_paths_stack_sheets_back_to_front() {
        let bounds = test_bounds();
        for (shape, dx, dy) in [
            (
                NodeShape::StackedRect,
                bounds.width * 0.099,
                bounds.height * 0.078,
            ),
            (
                NodeShape::StackedDocument,
                bounds.width * 0.165,
                bounds.height * 0.169,
            ),
        ] {
            let pieces = node_paint_paths(bounds, shape);
            assert_eq!(pieces.len(), 3);
            for (piece, step) in pieces.iter().zip([2.0, 1.0, 0.0]) {
                assert!(piece.fill && piece.stroke);
                assert_eq!(
                    piece.commands[0],
                    PathCmd::MoveTo {
                        x: bounds.x + dx * step,
                        y: bounds.y + dy * (2.0 - step)
                    }
                );
                assert!(piece.commands.contains(&PathCmd::Close));
                if shape == NodeShape::StackedDocument {
                    assert_eq!(
                        piece
                            .commands
                            .iter()
                            .filter(|cmd| matches!(cmd, PathCmd::QuadTo { .. }))
                            .count(),
                        2
                    );
                }
            }
        }
    }

    #[test]
    fn node_paint_paths_keep_measured_curves_instead_of_routing_boxes() {
        let bounds = test_bounds();
        for shape in [
            NodeShape::CurvedTrapezoid,
            NodeShape::BowTieRect,
            NodeShape::HalfRoundedRect,
            NodeShape::Flag,
        ] {
            let paint = node_paint_paths(bounds, shape);
            assert_eq!(paint.len(), 1);
            assert_ne!(paint[0].commands, node_path(bounds, shape));
        }
        let display = node_paint_paths(bounds, NodeShape::CurvedTrapezoid);
        let curve_ends: Vec<_> = display[0]
            .commands
            .iter()
            .filter_map(|cmd| match *cmd {
                PathCmd::CubicTo { x, y, .. } => Some((x, y)),
                _ => None,
            })
            .collect();
        assert_eq!(
            curve_ends.len(),
            4,
            "two quarter arcs preserve each half ellipse's fullness"
        );
        assert!(curve_ends[0].0 > bounds.x + bounds.width * 0.99);
        assert!(curve_ends[2].0 < bounds.x + bounds.width * 0.015);
        assert_near(curve_ends[1].0, bounds.x + bounds.width * 0.438);
        assert_near(curve_ends[1].1, bounds.y + bounds.height);
        let bow = node_paint_paths(bounds, NodeShape::BowTieRect);
        assert!(bow[0].commands.iter().any(|cmd| matches!(cmd,
            PathCmd::CubicTo { x, y, .. } if (*x - (bounds.x + bounds.width * 0.978)).abs() < 0.0001 && *y == bounds.y + bounds.height)));

        let tall = LayoutRect {
            width: 20.0,
            height: 100.0,
            ..bounds
        };
        let delay = node_paint_paths(tall, NodeShape::HalfRoundedRect);
        for point in coordinates(&delay[0].commands).as_chunks::<2>().0 {
            assert!(point[0] >= tall.x && point[0] <= tall.x + tall.width);
            assert!(point[1] >= tall.y && point[1] <= tall.y + tall.height);
        }
        assert!(
            delay[0].commands.iter().any(|cmd| matches!(cmd,
            PathCmd::CubicTo { x, y, .. } if *x == tall.x + tall.width && *y == tall.center().y)),
            "a narrow delay cap is elliptical and stays within its routing box"
        );
    }

    #[test]
    fn node_paint_paths_reject_invalid_boxes_and_canonicalize_tiny_extents() {
        let bounds = test_bounds();
        let shapes = [
            NodeShape::Rect,
            NodeShape::TextBlock,
            NodeShape::Braces,
            NodeShape::CrossedCircle,
            NodeShape::DoubleCircle,
            NodeShape::FramedCircle,
            NodeShape::Cylinder,
            NodeShape::LinedCylinder,
            NodeShape::HorizontalCylinder,
            NodeShape::StackedDocument,
            NodeShape::StackedRect,
            NodeShape::Flag,
            NodeShape::HalfRoundedRect,
            NodeShape::CurvedTrapezoid,
            NodeShape::BowTieRect,
        ];
        for shape in shapes {
            for invalid in [f32::NAN, f32::INFINITY, -1.0] {
                assert!(
                    node_paint_paths(
                        LayoutRect {
                            width: invalid,
                            ..bounds
                        },
                        shape
                    )
                    .is_empty(),
                    "{shape:?}, {invalid}"
                );
            }
            for extent in [0.0, f32::MIN_POSITIVE, f32::MIN_POSITIVE * 0.5, 0.0001] {
                let tiny = LayoutRect {
                    x: -0.0,
                    y: f32::MIN_POSITIVE,
                    width: extent,
                    height: extent,
                };
                let pieces = node_paint_paths(tiny, shape);
                assert_eq!(pieces, node_paint_paths(tiny, shape));
                for coordinate in pieces.iter().flat_map(|piece| coordinates(&piece.commands)) {
                    assert!(
                        coordinate.is_finite() && !coordinate.is_subnormal(),
                        "{shape:?}, {extent}: {coordinate}"
                    );
                }
            }
        }
    }

    #[test]
    fn notch_boundary_omits_the_removed_top_left_corner() {
        let path = node_path(test_bounds(), NodeShape::NotchedRect);
        assert_eq!(path.len(), 6);
        assert_vertices(
            &path,
            &[
                (41.0, 20.0),
                (210.0, 20.0),
                (210.0, 120.0),
                (10.0, 120.0),
                (10.0, 51.0),
            ],
        );
        assert!(matches!(path.last(), Some(PathCmd::Close)));
    }

    #[test]
    fn notched_pentagon_boundary_preserves_both_top_corner_cuts() {
        let path = node_path(test_bounds(), NodeShape::NotchedPentagon);
        assert_eq!(path.len(), 7);
        assert_vertices(
            &path,
            &[
                (30.0, 20.0),
                (190.0, 20.0),
                (210.0, 40.0),
                (210.0, 120.0),
                (10.0, 120.0),
                (10.0, 40.0),
            ],
        );
        assert!(matches!(path.last(), Some(PathCmd::Close)));
    }

    #[test]
    fn horizontal_cylinder_boundary_is_one_closed_contour_with_side_caps() {
        let path = node_path(test_bounds(), NodeShape::HorizontalCylinder);
        assert_eq!(path.len(), 8);
        assert_vertices(&path, &[(30.0, 20.0), (190.0, 20.0), (30.0, 120.0)]);
        assert_eq!(
            path.iter()
                .filter(|cmd| matches!(cmd, PathCmd::MoveTo { .. }))
                .count(),
            1
        );
        let mut start = (30.0, 20.0);
        let mut curves = 0;
        for cmd in &path {
            match *cmd {
                PathCmd::MoveTo { x, y } | PathCmd::LineTo { x, y } => start = (x, y),
                PathCmd::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => {
                    let center_x = if curves < 2 { 190.0 } else { 30.0 };
                    for step in 0..=32 {
                        let t = step as f32 / 32.0;
                        let s = 1.0 - t;
                        let px = s.powi(3) * start.0
                            + 3.0 * s * s * t * c1x
                            + 3.0 * s * t * t * c2x
                            + t.powi(3) * x;
                        let py = s.powi(3) * start.1
                            + 3.0 * s * s * t * c1y
                            + 3.0 * s * t * t * c2y
                            + t.powi(3) * y;
                        let ellipse =
                            ((px - center_x) / 20.0).powi(2) + ((py - 70.0) / 50.0).powi(2);
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
        for shape in [
            NodeShape::Document,
            NodeShape::LinedDocument,
            NodeShape::TaggedDocument,
        ] {
            let path = node_path(test_bounds(), shape);
            assert_eq!(path.len(), 6);
            assert_vertices(&path, &[(10.0, 20.0), (210.0, 20.0), (210.0, 100.0)]);
            for (cmd, expected) in path[3..5]
                .iter()
                .zip([[160.0, 104.0, 110.0, 115.0], [60.0, 126.0, 10.0, 110.0]])
            {
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
            let bounds = LayoutRect {
                width,
                height,
                ..test_bounds()
            };
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
            (0.0, 0.0),
            (0.0, 20.0),
            (2.0, 1000.0),
            (1000.0, 2.0),
            (f32::MIN_POSITIVE, f32::MIN_POSITIVE),
        ] {
            let bounds = LayoutRect {
                x: -15.5,
                y: 7.25,
                width,
                height,
            };
            for shape in [
                NodeShape::NotchedRect,
                NodeShape::NotchedPentagon,
                NodeShape::HorizontalCylinder,
                NodeShape::Document,
                NodeShape::LinedDocument,
                NodeShape::TaggedDocument,
                NodeShape::SmallCircle,
                NodeShape::FramedCircle,
            ] {
                let path = node_path(bounds, shape);
                assert!(matches!(path.first(), Some(PathCmd::MoveTo { .. })));
                assert!(matches!(path.last(), Some(PathCmd::Close)));
                let actual = coordinates(&path);
                assert!(
                    actual
                        .iter()
                        .all(|value| value.is_finite() && !value.is_subnormal())
                );
                assert_eq!(
                    actual
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>(),
                    coordinates(&node_path(bounds, shape))
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>(),
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
    fn circular_paths_follow_the_drawn_circle_in_non_square_bounds() {
        for (width, height) in [(180.0, 60.0), (60.0, 180.0)] {
            let bounds = LayoutRect {
                x: 10.0,
                y: 20.0,
                width,
                height,
            };
            let center = bounds.center();
            for shape in [
                NodeShape::Circle,
                NodeShape::FilledCircle,
                NodeShape::DoubleCircle,
                NodeShape::CrossedCircle,
            ] {
                let path = node_path(bounds, shape);
                let mut vertices = 0;
                for command in path {
                    if let PathCmd::MoveTo { x, y } | PathCmd::LineTo { x, y } = command {
                        let distance = (x - center.x).hypot(y - center.y);
                        assert!(
                            (distance - 30.0).abs() < 0.001,
                            "{shape:?}: radius {distance}"
                        );
                        vertices += 1;
                    }
                }
                assert_eq!(vertices, 24);
            }
        }
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
