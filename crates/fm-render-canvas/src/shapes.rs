//! Canvas2D shape drawing functions.
//!
//! Implements drawing for all diagram node shapes using the Canvas2D API.

use crate::context::Canvas2dContext;
use fm_core::NodeShape;
use fm_layout::{ErMarkerGlyph, MarkerKind};
use std::f64::consts::PI;

/// Draw a node shape to the canvas context.
#[allow(clippy::too_many_arguments)]
pub fn draw_shape<C: Canvas2dContext>(
    ctx: &mut C,
    shape: NodeShape,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    fill: &str,
    stroke: &str,
    stroke_width: f64,
) {
    ctx.set_fill_style(fill);
    ctx.set_stroke_style(stroke);
    ctx.set_line_width(stroke_width);

    match shape {
        // bd-7ls21. Canvas draws the OUTER BOX for both rectangle variants: the notch and the rule
        // are interior detail this surface has no primitive for, and a box is what the shape reduces
        // to without them. Unlike the ER crow's feet (bd-hh0o7), a plain box here is not a FALSE
        // statement — it is the same family of shape, just undecorated. The small circle does draw
        // correctly, since a circle is a primitive canvas has.
        NodeShape::NotchedRect | NodeShape::LinedRect | NodeShape::DividedRect => {
            draw_rect(ctx, x, y, width, height, 0.0)
        }
        // bd-7ls21's last batch. Unlike the notched and lined rectangles above, these are NOT
        // reduced to a box: canvas has every primitive each one needs, so drawing the box instead
        // would be a choice to ship the wrong shape rather than a limit of the surface.
        NodeShape::WindowPane => draw_window_pane(ctx, x, y, width, height),
        NodeShape::DataStore => draw_data_store(ctx, x, y, width, height),
        // ⚠️ A TEXT BLOCK PAINTS NOTHING, and the empty arm is the implementation. Falling through
        // to `draw_rect` would put a box around a shape whose entire definition is the absence of
        // one — and every "did it render?" assertion would still pass.
        NodeShape::TextBlock => {}
        NodeShape::BraceLeft => draw_brace(ctx, x, y, width, height, true),
        NodeShape::BraceRight => draw_brace(ctx, x, y, width, height, false),
        NodeShape::Braces => {
            draw_brace(ctx, x, y, width, height, true);
            draw_brace(ctx, x, y, width, height, false);
        }
        NodeShape::SmallCircle | NodeShape::FramedCircle => draw_circle(ctx, x, y, width, height),
        // The pentagon reduces to its box; the flipped triangle is drawn properly, since canvas has
        // the primitives for it and an UPSIDE-DOWN triangle would be a different shape, not a
        // simplified one.
        NodeShape::NotchedPentagon => draw_rect(ctx, x, y, width, height, 0.0),
        NodeShape::FlippedTriangle => draw_flipped_triangle(ctx, x, y, width, height),
        // The sloped top is drawable with the primitives canvas has; the horizontal cylinder
        // reduces to its box, as the notched shapes do.
        NodeShape::SlopedRect => draw_sloped_rect(ctx, x, y, width, height),
        NodeShape::HorizontalCylinder => draw_rect(ctx, x, y, width, height, 0.0),
        // The tag fold is a plain triangle canvas can draw; the lined cylinder falls back to the
        // plain cylinder, its extra rim being interior detail.
        NodeShape::TaggedRect => draw_tagged_rect(ctx, x, y, width, height),
        NodeShape::LinedCylinder => draw_cylinder(ctx, x, y, width, height),
        NodeShape::Document => draw_document(ctx, x, y, width, height),
        // The rule is interior detail; canvas draws the document body without it.
        NodeShape::LinedDocument => draw_document(ctx, x, y, width, height),
        NodeShape::LightningBolt => draw_lightning_bolt(ctx, x, y, width, height),
        NodeShape::Flag => draw_flag(ctx, x, y, width, height),
        NodeShape::HalfRoundedRect => draw_half_rounded_rect(ctx, x, y, width, height),
        NodeShape::StackedDocument => draw_stacked_document(ctx, x, y, width, height),
        NodeShape::StackedRect => draw_stacked_rect(ctx, x, y, width, height),
        NodeShape::Bang => draw_bang(ctx, x, y, width, height),
        NodeShape::CurvedTrapezoid => draw_curved_trapezoid(ctx, x, y, width, height),
        // The document body; the fold is interior detail this surface omits.
        NodeShape::TaggedDocument => draw_document(ctx, x, y, width, height),
        NodeShape::BowTieRect => draw_bow_tie_rect(ctx, x, y, width, height),
        NodeShape::Hourglass => draw_hourglass(ctx, x, y, width, height),
        NodeShape::Rect => draw_rect(ctx, x, y, width, height, 0.0),
        NodeShape::Rounded => draw_rect(ctx, x, y, width, height, 4.0),
        NodeShape::Stadium => draw_stadium(ctx, x, y, width, height),
        NodeShape::Diamond => draw_diamond(ctx, x, y, width, height),
        NodeShape::Hexagon => draw_hexagon(ctx, x, y, width, height),
        NodeShape::Circle | NodeShape::FilledCircle | NodeShape::DoubleCircle => {
            if shape == NodeShape::FilledCircle {
                ctx.set_fill_style(stroke);
            }
            draw_circle(ctx, x, y, width, height);
        }
        NodeShape::Cylinder => draw_cylinder(ctx, x, y, width, height),
        NodeShape::Trapezoid => draw_trapezoid(ctx, x, y, width, height),
        NodeShape::HorizontalBar => draw_rect(ctx, x, y + height * 0.25, width, height * 0.5, 4.0),
        NodeShape::Subroutine => draw_subroutine(ctx, x, y, width, height),
        NodeShape::Asymmetric => draw_asymmetric(ctx, x, y, width, height),
        NodeShape::Note => draw_note(ctx, x, y, width, height),
        // Extended shapes
        NodeShape::InvTrapezoid => draw_inv_trapezoid(ctx, x, y, width, height),
        NodeShape::Parallelogram => draw_parallelogram(ctx, x, y, width, height),
        NodeShape::InvParallelogram => draw_inv_parallelogram(ctx, x, y, width, height),
        NodeShape::Triangle => draw_triangle(ctx, x, y, width, height),
        NodeShape::Pentagon => draw_pentagon(ctx, x, y, width, height),
        NodeShape::Star => draw_star(ctx, x, y, width, height),
        NodeShape::Cloud => draw_cloud(ctx, x, y, width, height),
        NodeShape::Tag => draw_tag(ctx, x, y, width, height),
        NodeShape::CrossedCircle => draw_crossed_circle(ctx, x, y, width, height),
    }

    // Draw double circle outer ring if needed
    if shape == NodeShape::DoubleCircle {
        let cx = x + width / 2.0;
        let cy = y + height / 2.0;
        let r = width.min(height) / 2.0;
        ctx.begin_path();
        ctx.arc(cx, cy, r + 4.0, 0.0, 2.0 * PI);
        ctx.stroke();
    }
}

/// Draw a rectangle with optional rounded corners.
fn draw_rect<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64, radius: f64) {
    if radius <= 0.0 {
        ctx.begin_path();
        ctx.rect(x, y, w, h);
        ctx.fill();
        ctx.stroke();
    } else {
        draw_rounded_rect(ctx, x, y, w, h, radius);
    }
}

/// Draw a rounded rectangle.
fn draw_rounded_rect<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    ctx.begin_path();
    ctx.move_to(x + r, y);
    ctx.line_to(x + w - r, y);
    ctx.arc_to(x + w, y, x + w, y + r, r);
    ctx.line_to(x + w, y + h - r);
    ctx.arc_to(x + w, y + h, x + w - r, y + h, r);
    ctx.line_to(x + r, y + h);
    ctx.arc_to(x, y + h, x, y + h - r, r);
    ctx.line_to(x, y + r);
    ctx.arc_to(x, y, x + r, y, r);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a stadium (pill) shape.
fn draw_stadium<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let r = h / 2.0;
    draw_rounded_rect(ctx, x, y, w, h, r);
}

/// Draw a diamond shape.
fn draw_diamond<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;

    ctx.begin_path();
    ctx.move_to(cx, y);
    ctx.line_to(x + w, cy);
    ctx.line_to(cx, y + h);
    ctx.line_to(x, cy);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a hexagon shape.
fn draw_hexagon<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let inset = w * 0.15;
    let cy = y + h / 2.0;

    ctx.begin_path();
    ctx.move_to(x + inset, y);
    ctx.line_to(x + w - inset, y);
    ctx.line_to(x + w, cy);
    ctx.line_to(x + w - inset, y + h);
    ctx.line_to(x + inset, y + h);
    ctx.line_to(x, cy);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a circle shape.
fn draw_circle<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let r = w.min(h) / 2.0;

    ctx.begin_path();
    ctx.arc(cx, cy, r, 0.0, 2.0 * PI);
    ctx.fill();
    ctx.stroke();
}

/// Draw a cylinder shape (database).
fn draw_cylinder<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let ry = h * 0.1;
    let cx = x + w / 2.0;

    // Main body
    ctx.begin_path();
    ctx.move_to(x, y + ry);
    ctx.line_to(x, y + h - ry);

    // Bottom ellipse
    ctx.bezier_curve_to(x, y + h - ry + ry * 0.55, x + w * 0.22, y + h, cx, y + h);
    ctx.bezier_curve_to(
        x + w * 0.78,
        y + h,
        x + w,
        y + h - ry + ry * 0.55,
        x + w,
        y + h - ry,
    );

    ctx.line_to(x + w, y + ry);

    // Top ellipse (outer edge)
    ctx.bezier_curve_to(x + w, y + ry - ry * 0.55, x + w * 0.78, y, cx, y);
    ctx.bezier_curve_to(x + w * 0.22, y, x, y + ry - ry * 0.55, x, y + ry);

    ctx.close_path();
    ctx.fill();
    ctx.stroke();

    // Top ellipse inner curve (visible top surface)
    ctx.begin_path();
    ctx.move_to(x, y + ry);
    ctx.bezier_curve_to(
        x,
        y + ry + ry * 0.55,
        x + w * 0.22,
        y + ry * 2.0,
        cx,
        y + ry * 2.0,
    );
    ctx.bezier_curve_to(
        x + w * 0.78,
        y + ry * 2.0,
        x + w,
        y + ry + ry * 0.55,
        x + w,
        y + ry,
    );
    ctx.stroke();
}

/// Draw a trapezoid shape.
fn draw_trapezoid<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let inset = w * fm_core::SLANTED_SHAPE_INSET_RATIO_F64;

    ctx.begin_path();
    ctx.move_to(x + inset, y);
    ctx.line_to(x + w - inset, y);
    ctx.line_to(x + w, y + h);
    ctx.line_to(x, y + h);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a subroutine shape (double-bordered rectangle).
fn draw_subroutine<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let inset = 8.0;

    // Main rectangle
    ctx.begin_path();
    ctx.rect(x, y, w, h);
    ctx.fill();
    ctx.stroke();

    // Left vertical line
    ctx.begin_path();
    ctx.move_to(x + inset, y);
    ctx.line_to(x + inset, y + h);
    ctx.stroke();

    // Right vertical line
    ctx.begin_path();
    ctx.move_to(x + w - inset, y);
    ctx.line_to(x + w - inset, y + h);
    ctx.stroke();
}

/// Draw an asymmetric (flag/arrow) shape.
fn draw_asymmetric<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let flag = w * 0.15;
    let cy = y + h / 2.0;

    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x + w - flag, y);
    ctx.line_to(x + w, cy);
    ctx.line_to(x + w - flag, y + h);
    ctx.line_to(x, y + h);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a note shape (rectangle with folded corner).
fn draw_note<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let fold = 10.0;

    // Main shape
    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x + w - fold, y);
    ctx.line_to(x + w, y + fold);
    ctx.line_to(x + w, y + h);
    ctx.line_to(x, y + h);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();

    // Fold triangle
    ctx.begin_path();
    ctx.move_to(x + w - fold, y);
    ctx.line_to(x + w - fold, y + fold);
    ctx.line_to(x + w, y + fold);
    ctx.stroke();
}

/// Draw an inverted trapezoid shape.
fn draw_inv_trapezoid<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let inset = w * fm_core::SLANTED_SHAPE_INSET_RATIO_F64;

    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x + w, y);
    ctx.line_to(x + w - inset, y + h);
    ctx.line_to(x + inset, y + h);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a parallelogram shape.
fn draw_parallelogram<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let inset = w * fm_core::SLANTED_SHAPE_INSET_RATIO_F64;

    ctx.begin_path();
    ctx.move_to(x + inset, y);
    ctx.line_to(x + w, y);
    ctx.line_to(x + w - inset, y + h);
    ctx.line_to(x, y + h);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw an inverted parallelogram shape.
fn draw_inv_parallelogram<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let inset = w * fm_core::SLANTED_SHAPE_INSET_RATIO_F64;

    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x + w - inset, y);
    ctx.line_to(x + w, y + h);
    ctx.line_to(x + inset, y + h);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a triangle shape.
fn draw_triangle<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let cx = x + w / 2.0;

    ctx.begin_path();
    ctx.move_to(cx, y);
    ctx.line_to(x + w, y + h);
    ctx.line_to(x, y + h);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw two triangles meeting point-to-point — mermaid's `hourglass` (bd-7ls21).
///
/// One self-crossing path, so the nonzero fill rule fills both lobes without a seam at the waist.
fn draw_hourglass<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x + w, y);
    ctx.line_to(x, y + h);
    ctx.line_to(x + w, y + h);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw the stored-data block — mermaid's `bow-rect` (bd-7ls21).
///
/// Both sides bow the SAME way so the blocks tile; cubics stand in for the SVG path's arcs.
fn draw_bow_tie_rect<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    ctx.begin_path();
    ctx.move_to(x + w * 0.179, y);
    ctx.line_to(x + w, y);
    ctx.bezier_curve_to(
        x + w * 0.887,
        y + h * 0.15,
        x + w * 0.873,
        y + h * 0.85,
        x + w * 0.948,
        y + h,
    );
    ctx.line_to(x + w * 0.127, y + h);
    ctx.bezier_curve_to(
        x + w * 0.010,
        y + h * 0.85,
        x - w * 0.010,
        y + h * 0.15,
        x + w * 0.179,
        y,
    );
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw the CRT-screen silhouette — mermaid's `curv-trap` (bd-7ls21). Same measured controls.
fn draw_curved_trapezoid<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    ctx.begin_path();
    ctx.move_to(x + w * 0.265, y);
    ctx.line_to(x + w * 0.469, y);
    // Canvas has no elliptical-arc-to-point primitive, so the SVG path's arcs are approximated
    // here by their cubic equivalents through the same measured apexes.
    ctx.bezier_curve_to(
        x + w * 1.02,
        y + h * 0.16,
        x + w * 1.02,
        y + h * 0.84,
        x + w * 0.438,
        y + h,
    );
    ctx.line_to(x + w * 0.302, y + h);
    ctx.bezier_curve_to(
        x - w * 0.02,
        y + h * 0.84,
        x - w * 0.02,
        y + h * 0.16,
        x + w * 0.265,
        y,
    );
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a 14-pointed starburst — mermaid's `bang` (bd-7ls21).
///
/// Point count and inner ratio both measured; see `NodeShape::Bang`.
fn draw_bang<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let outer_r = w.min(h) / 2.0;
    let inner_r = outer_r * f64::from(fm_core::BANG_INNER_RATIO);
    let total = fm_core::BANG_POINTS * 2;

    ctx.begin_path();
    for index in 0..total {
        let r = if index % 2 == 0 { outer_r } else { inner_r };
        let angle = -PI / 2.0 + (index as f64) * PI / (fm_core::BANG_POINTS as f64);
        let px = cx + r * angle.cos();
        let py = cy + r * angle.sin();
        if index == 0 {
            ctx.move_to(px, py);
        } else {
            ctx.line_to(px, py);
        }
    }
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw three offset rectangles — mermaid's `st-rect` (bd-7ls21). Back to front, each filled.
fn draw_stacked_rect<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let offset_x = w * 0.099;
    let offset_y = h * 0.078;
    let leaf_w = w - offset_x * 2.0;
    let leaf_h = h - offset_y * 2.0;

    for step in (0..3).rev() {
        let leaf_x = x + offset_x * f64::from(step);
        let leaf_y = y + offset_y * f64::from(2 - step);
        draw_rect(ctx, leaf_x, leaf_y, leaf_w, leaf_h, 0.0);
    }
}

/// Draw three offset documents — mermaid's `docs` (bd-7ls21).
///
/// Back to front, each FILLED, so the copies in front occlude the ones behind; drawn the other way
/// the stack shows through itself and stops reading as depth.
fn draw_stacked_document<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let offset_x = w * 0.165;
    let offset_y = h * 0.169;
    let leaf_w = w - offset_x * 2.0;
    let leaf_h = h - offset_y * 2.0;

    for step in (0..3).rev() {
        let leaf_x = x + offset_x * f64::from(step);
        let leaf_y = y + offset_y * f64::from(2 - step);
        draw_document(ctx, leaf_x, leaf_y, leaf_w, leaf_h);
    }
}

/// Draw a rectangle with a semicircular right end — mermaid's `delay` (bd-7ls21).
///
/// The radius is half the HEIGHT, matching the measurement; a width fraction would balloon the cap
/// on a wide node.
fn draw_half_rounded_rect<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let radius = (h / 2.0).min(w / 2.0);

    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x + w - radius, y);
    ctx.arc(x + w - radius, y + radius, radius, -PI / 2.0, PI / 2.0);
    ctx.line_to(x, y + h);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a banner with both edges waved in opposite phase — mermaid's `flag` (bd-7ls21).
fn draw_flag<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    ctx.begin_path();
    ctx.move_to(x, y + h * 0.917);
    ctx.quadratic_curve_to(x + w * 0.25, y + h * 1.09, x + w * 0.5, y + h * 0.91);
    ctx.quadratic_curve_to(x + w * 0.75, y + h * 0.79, x + w, y + h * 0.893);
    ctx.line_to(x + w, y + h * 0.083);
    ctx.quadratic_curve_to(x + w * 0.75, y - h * 0.09, x + w * 0.5, y + h * 0.09);
    ctx.quadratic_curve_to(x + w * 0.25, y + h * 0.21, x, y + h * 0.107);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a lightning bolt — mermaid's `bolt` (bd-7ls21). Same six measured vertices as the SVG path.
fn draw_lightning_bolt<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    ctx.begin_path();
    ctx.move_to(x + w, y);
    ctx.line_to(x + w * 0.023, y + h * 0.55);
    ctx.line_to(x + w * 0.586, y + h * 0.55);
    ctx.line_to(x, y + h);
    ctx.line_to(x + w * 0.977, y + h * 0.45);
    ctx.line_to(x + w * 0.414, y + h * 0.45);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a rectangle with a wavy bottom edge — mermaid's `doc` (bd-7ls21).
///
/// The same two-quadratic approximation of the measured wave the SVG path uses.
fn draw_document<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x + w, y);
    ctx.line_to(x + w, y + h * 0.80);
    ctx.quadratic_curve_to(x + w * 0.75, y + h * 0.84, x + w * 0.5, y + h * 0.95);
    ctx.quadratic_curve_to(x + w * 0.25, y + h * 1.06, x, y + h * 0.90);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a rectangle with a folded bottom-right corner — mermaid's `tag-rect` (bd-7ls21).
///
/// The box is drawn WHOLE and the fold laid over it; cutting the corner would be `notch-rect`.
fn draw_tagged_rect<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let fold = h * f64::from(fm_core::TAGGED_RECT_FOLD_RATIO);

    ctx.begin_path();
    ctx.rect(x, y, w, h);
    ctx.fill();
    ctx.stroke();

    ctx.begin_path();
    ctx.move_to(x + w - fold, y + h);
    ctx.line_to(x + w, y + h);
    ctx.line_to(x + w, y + h - fold);
    ctx.close_path();
    ctx.stroke();
}

/// Draw a rectangle whose top edge slopes up to the right — mermaid's `sl-rect` (bd-7ls21).
fn draw_sloped_rect<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let drop = h * f64::from(fm_core::SLOPED_RECT_DROP_RATIO);

    ctx.begin_path();
    ctx.move_to(x, y + drop);
    ctx.line_to(x + w, y);
    ctx.line_to(x + w, y + h);
    ctx.line_to(x, y + h);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a downward-pointing triangle — mermaid's `flip-tri` (bd-7ls21).
///
/// The vertical mirror of [`draw_triangle`], written out rather than derived: an upside-down
/// triangle is a DIFFERENT shape, not a variant, and mermaid publishes both names.
fn draw_flipped_triangle<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let cx = x + w / 2.0;

    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x + w, y);
    ctx.line_to(cx, y + h);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a pentagon shape.
fn draw_pentagon<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let r = w.min(h) / 2.0;
    let angle_offset = -PI / 2.0;

    ctx.begin_path();
    for i in 0..5 {
        let angle = angle_offset + (i as f64) * 2.0 * PI / 5.0;
        let px = cx + r * angle.cos();
        let py = cy + r * angle.sin();
        if i == 0 {
            ctx.move_to(px, py);
        } else {
            ctx.line_to(px, py);
        }
    }
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a 5-pointed star shape.
fn draw_star<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let outer_r = w.min(h) / 2.0;
    let inner_r = outer_r * 0.4;
    let angle_offset = -PI / 2.0;

    ctx.begin_path();
    for i in 0..10 {
        let r = if i % 2 == 0 { outer_r } else { inner_r };
        let angle = angle_offset + (i as f64) * PI / 5.0;
        let px = cx + r * angle.cos();
        let py = cy + r * angle.sin();
        if i == 0 {
            ctx.move_to(px, py);
        } else {
            ctx.line_to(px, py);
        }
    }
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a cloud shape (simplified using arcs).
fn draw_cloud<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let r = h / 3.0;

    ctx.begin_path();
    // Simplified cloud as overlapping circles in a single path
    ctx.arc(x + r * 1.2, cy, r, 0.0, 2.0 * PI);
    ctx.arc(cx, y + r * 0.8, r * 0.9, 0.0, 2.0 * PI);
    ctx.arc(x + w - r * 1.2, cy, r, 0.0, 2.0 * PI);
    // Bottom connecting rect
    ctx.rect(x + r * 0.5, cy, w - r, h * 0.4);
    ctx.fill();
    ctx.stroke();
}

/// Draw a tag/flag shape.
fn draw_tag<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let point = w * 0.2;
    let cy = y + h / 2.0;

    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x + w - point, y);
    ctx.line_to(x + w, cy);
    ctx.line_to(x + w - point, y + h);
    ctx.line_to(x, y + h);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
}

/// Draw a crossed circle shape.
fn draw_crossed_circle<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let r = w.min(h) / 2.0;
    let offset = r * 0.707; // r * cos(45°)

    // Circle
    ctx.begin_path();
    ctx.arc(cx, cy, r, 0.0, 2.0 * PI);
    ctx.fill();
    ctx.stroke();

    // X through the circle
    ctx.begin_path();
    ctx.move_to(cx - offset, cy - offset);
    ctx.line_to(cx + offset, cy + offset);
    ctx.stroke();

    ctx.begin_path();
    ctx.move_to(cx + offset, cy - offset);
    ctx.line_to(cx - offset, cy + offset);
    ctx.stroke();
}

/// Draw an arrowhead at the end of an edge.
pub fn draw_arrowhead<C: Canvas2dContext>(
    ctx: &mut C,
    x: f64,
    y: f64,
    angle: f64,
    size: f64,
    fill: &str,
) {
    ctx.save();
    ctx.translate(x, y);
    ctx.rotate(angle);

    ctx.set_fill_style(fill);
    ctx.begin_path();
    ctx.move_to(0.0, 0.0);
    ctx.line_to(-size, -size / 2.0);
    ctx.line_to(-size, size / 2.0);
    ctx.close_path();
    ctx.fill();

    ctx.restore();
}

/// Draw a UML diamond marker with its outer tip anchored at the edge endpoint.
///
/// A filled diamond denotes composition; passing `None` draws the hollow aggregation marker.
pub fn draw_diamond_marker<C: Canvas2dContext>(
    ctx: &mut C,
    x: f64,
    y: f64,
    angle: f64,
    size: f64,
    fill: Option<&str>,
    stroke: &str,
) {
    ctx.save();
    ctx.translate(x, y);
    ctx.rotate(angle);
    ctx.begin_path();
    ctx.move_to(0.0, 0.0);
    ctx.line_to(-size / 2.0, -size / 2.0);
    ctx.line_to(-size, 0.0);
    ctx.line_to(-size / 2.0, size / 2.0);
    ctx.close_path();

    if let Some(fill) = fill {
        ctx.set_fill_style(fill);
        ctx.fill();
    } else {
        ctx.set_stroke_style(stroke);
        ctx.stroke();
    }

    ctx.restore();
}

/// Draw a hollow UML inheritance triangle with its point anchored at the edge endpoint.
pub fn draw_open_triangle_marker<C: Canvas2dContext>(
    ctx: &mut C,
    x: f64,
    y: f64,
    angle: f64,
    size: f64,
    stroke: &str,
) {
    ctx.save();
    ctx.translate(x, y);
    ctx.rotate(angle);
    ctx.set_stroke_style(stroke);
    ctx.begin_path();
    ctx.move_to(0.0, 0.0);
    ctx.line_to(-size, -size / 2.0);
    ctx.line_to(-size, size / 2.0);
    ctx.close_path();
    ctx.stroke();
    ctx.restore();
}

/// Draw one of mermaid's eight ER crow's-foot cardinality markers (bd-hh0o7).
///
/// GEOMETRY IS THE SVG RENDERER'S, TRANSLATED, NOT REDERIVED. `er_marker_def` in fm-render-svg
/// holds the numbers read out of a Chromium render of the pinned mermaid 11.15.0 bundle, as
/// `<marker>` definitions with a `markerWidth`/`markerHeight` box, a `refX`/`refY` anchor and
/// `orient="auto"`. The mapping into canvas space is exact and mechanical:
///
/// * `orient="auto"` aligns the marker's +x with the path direction, which is what the caller's
///   `ctx.rotate(angle)` already does — the caller passes the PATH direction at both ends
///   (`start -> next` and `prev -> end`), so no extra half-turn belongs anywhere here.
/// * The `refX`/`refY` point is the one that sits on the path endpoint, so a marker-space point
///   `(mx, my)` becomes `(mx - refX, my - refY)` once translated to that endpoint.
///
/// Every literal below is that subtraction applied to the SVG path data, so the two renderers
/// cannot disagree about a shape without one of them being edited.
///
/// ⚠️ START AND END ARE DIFFERENT GLYPHS, NOT ONE GLYPH ROTATED. Rotation is not mirroring: the
/// bar of a `one` marker sits on the far side of the foot from the entity, so reusing the start
/// glyph at the end puts the bar where the foot belongs and the diagram states a different
/// cardinality. That is why there are eight arms here and not four.
///
/// Returns the number of draw calls issued, so a caller's accounting stays honest.
pub fn draw_er_cardinality_marker<C: Canvas2dContext>(
    ctx: &mut C,
    marker: MarkerKind,
    x: f64,
    y: f64,
    angle: f64,
    bubble_fill: &str,
    stroke: &str,
) -> usize {
    let Some(glyph) = marker.er_glyph() else {
        return 0;
    };

    ctx.save();
    ctx.translate(x, y);
    ctx.rotate(angle);
    ctx.set_line_width(1.0);

    let mut calls = 0;
    // The "many" lens, drawn with the two quadratics the SVG marker uses rather than the sampled
    // polyline the GPU path needs — same shape, this surface's best primitive for it.
    if glyph.foot {
        let half = f64::from(ErMarkerGlyph::FOOT_HALF_LENGTH);
        ctx.set_stroke_style(stroke);
        ctx.begin_path();
        ctx.move_to(-half, 0.0);
        ctx.quadratic_curve_to(0.0, -half, half, 0.0);
        ctx.quadratic_curve_to(0.0, half, -half, 0.0);
        ctx.stroke();
        calls += 1;
    }
    for bar_x in &glyph.bars {
        let half = f64::from(ErMarkerGlyph::BAR_HALF_HEIGHT);
        ctx.set_stroke_style(stroke);
        ctx.begin_path();
        ctx.move_to(f64::from(*bar_x), -half);
        ctx.line_to(f64::from(*bar_x), half);
        ctx.stroke();
        calls += 1;
    }
    if let Some(bubble_x) = glyph.bubble {
        ctx.begin_path();
        ctx.arc(
            f64::from(bubble_x),
            0.0,
            f64::from(ErMarkerGlyph::BUBBLE_RADIUS),
            0.0,
            std::f64::consts::TAU,
        );
        // Filled, not hollow-by-omission: the bubble sits ON the edge line, so an unfilled circle
        // would have the stroke running straight through it and read as a bead rather than a zero.
        ctx.set_fill_style(bubble_fill);
        ctx.fill();
        ctx.set_stroke_style(stroke);
        ctx.stroke();
        calls += 1;
    }

    ctx.restore();
    calls
}

/// Draw a circle marker at the end of an edge.
/// `fill: None` draws the circle as an OUTLINE — the UML lollipop / provided-interface socket
/// (bd-lkm9i), as against the filled flowchart `--o` terminator.
///
/// The `Option` mirrors [`draw_diamond_marker`], which carries it for the identical reason: hollow
/// versus filled is not styling in UML, it is which relationship the diagram states (aggregation vs
/// composition there, provided interface vs terminator here). Both shapes therefore share one
/// geometry and differ only in the fill.
pub fn draw_circle_marker<C: Canvas2dContext>(
    ctx: &mut C,
    x: f64,
    y: f64,
    radius: f64,
    fill: Option<&str>,
    stroke: &str,
) {
    ctx.set_stroke_style(stroke);
    ctx.begin_path();
    ctx.arc(x, y, radius, 0.0, 2.0 * PI);
    if let Some(fill) = fill {
        ctx.set_fill_style(fill);
        ctx.fill();
    }
    ctx.stroke();
}

/// Draw a cross marker at the end of an edge.
pub fn draw_cross_marker<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, size: f64, stroke: &str) {
    ctx.set_stroke_style(stroke);
    ctx.set_line_width(2.0);

    ctx.begin_path();
    ctx.move_to(x - size / 2.0, y - size / 2.0);
    ctx.line_to(x + size / 2.0, y + size / 2.0);
    ctx.stroke();

    ctx.begin_path();
    ctx.move_to(x + size / 2.0, y - size / 2.0);
    ctx.line_to(x - size / 2.0, y + size / 2.0);
    ctx.stroke();
}

/// mermaid's `win-pane`: the box plus a horizontal rule below the top and a vertical rule right of
/// the left, both at a FIXED inset (see [`fm_core::WINDOW_PANE_INSET`]).
fn draw_window_pane<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    draw_rect(ctx, x, y, w, h, 0.0);
    let inset_x = f64::from(fm_core::WINDOW_PANE_INSET).min(w * 0.4);
    let inset_y = f64::from(fm_core::WINDOW_PANE_INSET).min(h * 0.4);
    ctx.begin_path();
    ctx.move_to(x, y + inset_y);
    ctx.line_to(x + w, y + inset_y);
    ctx.move_to(x + inset_x, y);
    ctx.line_to(x + inset_x, y + h);
    ctx.stroke();
}

/// mermaid's `datastore`: the fill of a rectangle with only its TOP AND BOTTOM edges stroked.
///
/// The fill and the stroke are separate paths on purpose — one path cannot fill a box while
/// stroking two of its four edges, which is the same constraint the SVG renderer meets by emitting
/// a group rather than a single element.
fn draw_data_store<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64) {
    ctx.begin_path();
    ctx.rect(x, y, w, h);
    ctx.fill();
    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x + w, y);
    ctx.move_to(x, y + h);
    ctx.line_to(x + w, y + h);
    ctx.stroke();
}

/// One curly brace down the left or right edge, matching the SVG renderer's `brace_element`.
///
/// Quarter-circle arcs of radius `f`, a straight spine one radius in, and a middle spur at two —
/// the spur is what separates a brace from a parenthesis. Stroked and never filled: mermaid's own
/// handler passes `fill: "none"`, and a filled brace would paint the label's background over it.
fn draw_brace<C: Canvas2dContext>(ctx: &mut C, x: f64, y: f64, w: f64, h: f64, on_left: bool) {
    use std::f64::consts::{FRAC_PI_2, PI};

    let f = (h * f64::from(fm_core::BRACE_RADIUS_RATIO))
        .max(f64::from(fm_core::BRACE_MIN_RADIUS))
        .min(h * 0.25)
        .min(w * 0.25);
    let (edge, dir) = if on_left {
        (x + 2.0 * f, -1.0)
    } else {
        (x + w - 2.0 * f, 1.0)
    };
    let spine = edge + dir * f;
    let spur = edge + dir * 2.0 * f;
    let mid = y + h / 2.0;
    // Arc angles are written from the side that owns them rather than mirrored arithmetically: on
    // the left the arm opens toward -x, so every centre and sweep is the reflection of the right's.
    let (top_from, top_to) = if on_left {
        (-FRAC_PI_2, 0.0)
    } else {
        (-FRAC_PI_2, -PI)
    };
    ctx.begin_path();
    ctx.move_to(edge, y);
    ctx.arc(spine, y + f, f, top_from, top_to);
    ctx.move_to(spine, y + f);
    ctx.line_to(spine, mid - f);
    ctx.arc(spur, mid - f, f, if on_left { PI } else { 0.0 }, FRAC_PI_2);
    ctx.move_to(spur, mid);
    ctx.arc(spur, mid + f, f, -FRAC_PI_2, if on_left { PI } else { 0.0 });
    ctx.move_to(spine, mid + f);
    ctx.line_to(spine, y + h - f);
    ctx.arc(
        spine,
        y + h - f,
        f,
        if on_left { 0.0 } else { -PI },
        FRAC_PI_2,
    );
    ctx.stroke();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::MockCanvas2dContext;

    #[test]
    fn draw_rect_records_operations() {
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        draw_shape(
            &mut ctx,
            NodeShape::Rect,
            10.0,
            10.0,
            100.0,
            50.0,
            "#fff",
            "#333",
            1.5,
        );
        assert!(ctx.operation_count() > 0);
    }

    #[test]
    fn draw_diamond_records_operations() {
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        draw_shape(
            &mut ctx,
            NodeShape::Diamond,
            10.0,
            10.0,
            100.0,
            100.0,
            "#fff",
            "#333",
            1.5,
        );
        assert!(ctx.operation_count() > 0);
    }

    #[test]
    fn draw_circle_records_operations() {
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);
        draw_shape(
            &mut ctx,
            NodeShape::Circle,
            10.0,
            10.0,
            60.0,
            60.0,
            "#fff",
            "#333",
            1.5,
        );
        assert!(ctx.operation_count() > 0);
    }
}
