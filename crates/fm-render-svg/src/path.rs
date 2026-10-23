//! SVG path builder with fluent API.
//!
//! Provides `PathBuilder` for constructing SVG path `d` attribute strings
//! using standard path commands (M, L, C, Q, A, Z, etc.).

use std::fmt::Write;

/// SVG path commands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    /// Move to (absolute)
    MoveTo { x: f32, y: f32 },
    /// Move to (relative)
    MoveToRel { dx: f32, dy: f32 },
    /// Line to (absolute)
    LineTo { x: f32, y: f32 },
    /// Line to (relative)
    LineToRel { dx: f32, dy: f32 },
    /// Horizontal line to (absolute)
    HorizontalTo { x: f32 },
    /// Horizontal line to (relative)
    HorizontalToRel { dx: f32 },
    /// Vertical line to (absolute)
    VerticalTo { y: f32 },
    /// Vertical line to (relative)
    VerticalToRel { dy: f32 },
    /// Cubic bezier curve (absolute)
    CurveTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x: f32,
        y: f32,
    },
    /// Cubic bezier curve (relative)
    CurveToRel {
        dx1: f32,
        dy1: f32,
        dx2: f32,
        dy2: f32,
        dx: f32,
        dy: f32,
    },
    /// Smooth cubic bezier (absolute)
    SmoothCurveTo { x2: f32, y2: f32, x: f32, y: f32 },
    /// Smooth cubic bezier (relative)
    SmoothCurveToRel {
        dx2: f32,
        dy2: f32,
        dx: f32,
        dy: f32,
    },
    /// Quadratic bezier curve (absolute)
    QuadraticTo { x1: f32, y1: f32, x: f32, y: f32 },
    /// Quadratic bezier curve (relative)
    QuadraticToRel {
        dx1: f32,
        dy1: f32,
        dx: f32,
        dy: f32,
    },
    /// Smooth quadratic bezier (absolute)
    SmoothQuadraticTo { x: f32, y: f32 },
    /// Smooth quadratic bezier (relative)
    SmoothQuadraticToRel { dx: f32, dy: f32 },
    /// Arc (absolute)
    Arc {
        rx: f32,
        ry: f32,
        x_rotation: f32,
        large_arc: bool,
        sweep: bool,
        x: f32,
        y: f32,
    },
    /// Arc (relative)
    ArcRel {
        rx: f32,
        ry: f32,
        x_rotation: f32,
        large_arc: bool,
        sweep: bool,
        dx: f32,
        dy: f32,
    },
    /// Close path
    Close,
}

impl PathCommand {
    /// Render the command to SVG path syntax.
    fn render(&self, output: &mut String) {
        match self {
            Self::MoveTo { x, y } => {
                output.push('M');
                let _ = FmtNum(*x).write_into(output);
                output.push(' ');
                let _ = FmtNum(*y).write_into(output);
            }
            Self::MoveToRel { dx, dy } => {
                output.push('m');
                let _ = FmtNum(*dx).write_into(output);
                output.push(' ');
                let _ = FmtNum(*dy).write_into(output);
            }
            Self::LineTo { x, y } => {
                output.push('L');
                let _ = FmtNum(*x).write_into(output);
                output.push(' ');
                let _ = FmtNum(*y).write_into(output);
            }
            Self::LineToRel { dx, dy } => {
                output.push('l');
                let _ = FmtNum(*dx).write_into(output);
                output.push(' ');
                let _ = FmtNum(*dy).write_into(output);
            }
            Self::HorizontalTo { x } => {
                let _ = write!(output, "H{}", FmtNum(*x));
            }
            Self::HorizontalToRel { dx } => {
                let _ = write!(output, "h{}", FmtNum(*dx));
            }
            Self::VerticalTo { y } => {
                let _ = write!(output, "V{}", FmtNum(*y));
            }
            Self::VerticalToRel { dy } => {
                let _ = write!(output, "v{}", FmtNum(*dy));
            }
            Self::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                write_cubic(output, 'C', (*x1, *y1), (*x2, *y2), (*x, *y));
            }
            Self::CurveToRel {
                dx1,
                dy1,
                dx2,
                dy2,
                dx,
                dy,
            } => {
                write_cubic(output, 'c', (*dx1, *dy1), (*dx2, *dy2), (*dx, *dy));
            }
            Self::SmoothCurveTo { x2, y2, x, y } => {
                let _ = write!(
                    output,
                    "S{} {},{} {}",
                    FmtNum(*x2),
                    FmtNum(*y2),
                    FmtNum(*x),
                    FmtNum(*y)
                );
            }
            Self::SmoothCurveToRel { dx2, dy2, dx, dy } => {
                let _ = write!(
                    output,
                    "s{} {},{} {}",
                    FmtNum(*dx2),
                    FmtNum(*dy2),
                    FmtNum(*dx),
                    FmtNum(*dy)
                );
            }
            Self::QuadraticTo { x1, y1, x, y } => {
                let _ = write!(
                    output,
                    "Q{} {},{} {}",
                    FmtNum(*x1),
                    FmtNum(*y1),
                    FmtNum(*x),
                    FmtNum(*y)
                );
            }
            Self::QuadraticToRel { dx1, dy1, dx, dy } => {
                let _ = write!(
                    output,
                    "q{} {},{} {}",
                    FmtNum(*dx1),
                    FmtNum(*dy1),
                    FmtNum(*dx),
                    FmtNum(*dy)
                );
            }
            Self::SmoothQuadraticTo { x, y } => {
                let _ = write!(output, "T{} {}", FmtNum(*x), FmtNum(*y));
            }
            Self::SmoothQuadraticToRel { dx, dy } => {
                let _ = write!(output, "t{} {}", FmtNum(*dx), FmtNum(*dy));
            }
            Self::Arc {
                rx,
                ry,
                x_rotation,
                large_arc,
                sweep,
                x,
                y,
            } => {
                let _ = write!(
                    output,
                    "A{} {} {} {} {} {} {}",
                    FmtNum(*rx),
                    FmtNum(*ry),
                    FmtNum(*x_rotation),
                    i32::from(*large_arc),
                    i32::from(*sweep),
                    FmtNum(*x),
                    FmtNum(*y)
                );
            }
            Self::ArcRel {
                rx,
                ry,
                x_rotation,
                large_arc,
                sweep,
                dx,
                dy,
            } => {
                let _ = write!(
                    output,
                    "a{} {} {} {} {} {} {}",
                    FmtNum(*rx),
                    FmtNum(*ry),
                    FmtNum(*x_rotation),
                    i32::from(*large_arc),
                    i32::from(*sweep),
                    FmtNum(*dx),
                    FmtNum(*dy)
                );
            }
            Self::Close => output.push('Z'),
        }
    }
}

/// Helper for efficient, zero-allocation number formatting in SVG.
struct FmtNum(f32);

impl FmtNum {
    /// Write the number directly into `out`, bypassing the `fmt::Formatter` indirection on
    /// the hot per-path-command serialization path. [`Display`](std::fmt::Display) delegates here.
    fn write_into<W: std::fmt::Write>(&self, out: &mut W) -> std::fmt::Result {
        let n = self.0;
        if !n.is_finite() {
            return out.write_str("0");
        }
        // For finite `n`, `n as i32` is a saturating cast, so the round-trip compare
        // `i as f32 == n` reproduces the old `n.fract() == 0.0 && n in i32 range` branch
        // byte-for-byte (a fractional value truncates and fails the compare; a whole
        // in-range value round-trips; an out-of-range whole saturates and fails). This
        // drops the `f32::fract` → `truncf` libm call — ~9% of coordinate-heavy render
        // on the per-path-coordinate serialization hot loop.
        let i = n as i32;
        if i as f32 == n {
            crate::attributes::write_int_into(out, i)
        } else {
            crate::attributes::write_fixed2(out, n)
        }
    }
}

impl std::fmt::Display for FmtNum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write_into(f)
    }
}

/// Write a cubic-Bézier path segment (`C`/`c`) directly, bypassing the `write!`/
/// `fmt::Formatter` machinery on this per-edge-segment hot path. Byte-identical to
/// `{prefix}{x1} {y1},{x2} {y2},{x} {y}`.
fn write_cubic(output: &mut String, prefix: char, c1: (f32, f32), c2: (f32, f32), end: (f32, f32)) {
    output.push(prefix);
    let _ = FmtNum(c1.0).write_into(output);
    output.push(' ');
    let _ = FmtNum(c1.1).write_into(output);
    output.push(',');
    let _ = FmtNum(c2.0).write_into(output);
    output.push(' ');
    let _ = FmtNum(c2.1).write_into(output);
    output.push(',');
    let _ = FmtNum(end.0).write_into(output);
    output.push(' ');
    let _ = FmtNum(end.1).write_into(output);
}

fn write_point(output: &mut String, prefix: char, x: f32, y: f32) {
    output.push(prefix);
    let _ = FmtNum(x).write_into(output);
    output.push(' ');
    let _ = FmtNum(y).write_into(output);
}

/// Build a smooth path `d` string directly, with no intermediate `Vec<PathCommand>`.
/// Orthogonal routes receive small rounded elbows; other routes use bounded
/// Catmull-Rom→cubic conversion (tension 1/4). Commands are joined by single spaces.
pub(crate) fn build_smooth_path_by<F>(n: usize, point_at: F) -> String
where
    F: FnMut(usize) -> (f32, f32),
{
    // n<=2 needs only M or M+L. Longer paths can emit six-coordinate cubic segments or a
    // line plus a quadratic elbow per corner; allow room for either without reallocation.
    let mut d = String::with_capacity(if n < 3 { n * 24 } else { 24 + (n - 1) * 56 });
    build_smooth_path_by_into(&mut d, n, point_at);
    d
}

/// Append smooth path `d` data directly into `out` — byte-identical to
/// [`build_smooth_path_by`] but writing into a caller-provided buffer so a whole edge fragment can be
/// assembled without allocating the per-edge `d` String first. Pinned by
/// `build_smooth_path_into_matches_build_smooth_path`.
pub(crate) fn build_smooth_path_by_into<F>(out: &mut String, n: usize, mut point_at: F)
where
    F: FnMut(usize) -> (f32, f32),
{
    if n == 0 {
        return;
    }
    let first = point_at(0);
    write_point(out, 'M', first.0, first.1);
    if n == 1 {
        return;
    }
    if n == 2 {
        let second = point_at(1);
        out.push(' ');
        write_point(out, 'L', second.0, second.1);
        return;
    }

    // Near-collinear collapse (GH#7): orthogonal routes whose source and target almost line up
    // produce a polyline with a tiny mid-route jog (a few px). Catmull-Rom smoothing of that jog
    // rendered as a visible S-wiggle in an otherwise straight edge. If every interior waypoint
    // sits within `STRAIGHT_EPS` of the start→end chord, the honest shape is a straight line.
    const STRAIGHT_EPS: f32 = 2.0;
    let last = point_at(n - 1);
    let chord = (last.0 - first.0, last.1 - first.1);
    let chord_len = (chord.0 * chord.0 + chord.1 * chord.1).sqrt();
    if chord_len > f32::EPSILON {
        let mut straight = true;
        for i in 1..(n - 1) {
            let p = point_at(i);
            let dev = ((p.0 - first.0) * chord.1 - (p.1 - first.1) * chord.0).abs() / chord_len;
            // Reject points that deviate laterally OR lie outside the chord span (backtracking).
            let along = ((p.0 - first.0) * chord.0 + (p.1 - first.1) * chord.1) / chord_len;
            if dev > STRAIGHT_EPS || along < -STRAIGHT_EPS || along > chord_len + STRAIGHT_EPS {
                straight = false;
                break;
            }
        }
        if straight {
            out.push(' ');
            write_point(out, 'L', last.0, last.1);
            return;
        }
    }

    // Clamping cubic controls into each individual segment prevents overshoot, but an
    // axis-aligned segment's bounding box has zero width or height. Every such cubic therefore
    // becomes a straight line and its corners stay sharp. Round the *junctions* instead, with
    // a local trim that cannot turn into a large spline bulge or consume a whole port stub.
    let mut previous = first;
    let orthogonal = (1..n).all(|index| {
        let point = point_at(index);
        let aligned = point.0 == previous.0 || point.1 == previous.1;
        previous = point;
        aligned && point.0.is_finite() && point.1.is_finite()
    });
    if orthogonal && first.0.is_finite() && first.1.is_finite() {
        append_rounded_orthogonal_path(out, n, &mut point_at);
        return;
    }

    let t: f32 = 0.25;
    for i in 0..(n - 1) {
        let p_prev = if i == 0 { point_at(0) } else { point_at(i - 1) };
        let p_cur = point_at(i);
        let p_next = point_at(i + 1);
        let p_next2 = if i + 2 < n {
            point_at(i + 2)
        } else {
            point_at(n - 1)
        };

        // Catmull-Rom tangents, then clamp each control point into the current segment's
        // bounding box (GH#7). Unclamped neighbor-chord tangents overshoot at right-angle
        // waypoints — an edge leaving a node straight down and then turning left first bulged
        // ~25px to the RIGHT, so sibling branches from one port crossed each other in a
        // pretzel. The cubic convex-hull property means clamped control points keep every
        // segment inside the channel the router actually chose, so smoothed edges can no
        // longer swing wide or cross where their polylines do not.
        let (seg_lo_x, seg_hi_x) = if p_cur.0 <= p_next.0 {
            (p_cur.0, p_next.0)
        } else {
            (p_next.0, p_cur.0)
        };
        let (seg_lo_y, seg_hi_y) = if p_cur.1 <= p_next.1 {
            (p_cur.1, p_next.1)
        } else {
            (p_next.1, p_cur.1)
        };

        let cp1x = (p_cur.0 + (p_next.0 - p_prev.0) * t).clamp(seg_lo_x, seg_hi_x);
        let cp1y = (p_cur.1 + (p_next.1 - p_prev.1) * t).clamp(seg_lo_y, seg_hi_y);
        let cp2x = (p_next.0 - (p_next2.0 - p_cur.0) * t).clamp(seg_lo_x, seg_hi_x);
        let cp2y = (p_next.1 - (p_next2.1 - p_cur.1) * t).clamp(seg_lo_y, seg_hi_y);

        out.push(' ');
        write_cubic(out, 'C', (cp1x, cp1y), (cp2x, cp2y), (p_next.0, p_next.1));
    }
}

/// Append a route after its initial `M`, preserving the router's terminal directions.
/// A quadratic elbow with a 6px trim strays at most 1.5px from the original L-shaped
/// centerline, within the obstacle router's intended 8px clearance. Half-segment caps
/// keep neighboring elbows disjoint and leave straight terminal stubs for arrow markers.
fn append_rounded_orthogonal_path<F>(out: &mut String, n: usize, point_at: &mut F)
where
    F: FnMut(usize) -> (f32, f32),
{
    const CORNER_TRIM: f32 = 6.0;
    let mut previous = point_at(0);
    let mut cursor = previous;
    let mut index = 1;
    let Some(mut corner) = next_orthogonal_corner(previous, n, &mut index, point_at) else {
        return;
    };
    while let Some(next) = next_orthogonal_corner(corner, n, &mut index, point_at) {
        let incoming = (corner.0 - previous.0, corner.1 - previous.1);
        let outgoing = (next.0 - corner.0, next.1 - corner.1);
        let turns = (incoming.0 == 0.0) != (outgoing.0 == 0.0);
        if turns {
            let incoming_length = incoming.0.abs() + incoming.1.abs();
            let outgoing_length = outgoing.0.abs() + outgoing.1.abs();
            let trim = CORNER_TRIM
                .min(incoming_length * 0.5)
                .min(outgoing_length * 0.5);
            let entry = (
                corner.0 - incoming.0 / incoming_length * trim,
                corner.1 - incoming.1 / incoming_length * trim,
            );
            let exit = (
                corner.0 + outgoing.0 / outgoing_length * trim,
                corner.1 + outgoing.1 / outgoing_length * trim,
            );
            if entry != cursor {
                out.push(' ');
                write_point(out, 'L', entry.0, entry.1);
            }
            out.push(' ');
            PathCommand::QuadraticTo {
                x1: corner.0,
                y1: corner.1,
                x: exit.0,
                y: exit.1,
            }
            .render(out);
            cursor = exit;
        } else {
            // Preserve reversals. Rounding a 180-degree turn would invent a different route.
            out.push(' ');
            write_point(out, 'L', corner.0, corner.1);
            cursor = corner;
        }
        previous = corner;
        corner = next;
    }
    if corner != cursor {
        out.push(' ');
        write_point(out, 'L', corner.0, corner.1);
    }
}

/// Skip duplicate and forward-collinear waypoints before measuring either side of an elbow.
/// Keeping the original corners for both lengths makes the radius independent of how many
/// redundant waypoints the router happened to put along a straight channel.
fn next_orthogonal_corner<F>(
    origin: (f32, f32),
    n: usize,
    index: &mut usize,
    point_at: &mut F,
) -> Option<(f32, f32)>
where
    F: FnMut(usize) -> (f32, f32),
{
    let mut candidate = origin;
    while *index < n {
        let next = point_at(*index);
        if candidate != origin && next != candidate {
            let incoming = (candidate.0 - origin.0, candidate.1 - origin.1);
            let outgoing = (next.0 - candidate.0, next.1 - candidate.1);
            if (incoming.0 == 0.0) != (outgoing.0 == 0.0)
                || incoming.0 * outgoing.0 + incoming.1 * outgoing.1 <= 0.0
            {
                break;
            }
        }
        candidate = next;
        *index += 1;
    }
    (candidate != origin).then_some(candidate)
}

/// Fluent builder for SVG path `d` attribute strings.
#[derive(Debug, Clone, Default)]
pub struct PathBuilder {
    commands: Vec<PathCommand>,
}

impl PathBuilder {
    /// Create a new empty path builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Move to an absolute position.
    #[must_use]
    pub fn move_to(mut self, x: f32, y: f32) -> Self {
        self.commands.push(PathCommand::MoveTo { x, y });
        self
    }

    /// Move to a relative position.
    #[must_use]
    pub fn move_to_rel(mut self, dx: f32, dy: f32) -> Self {
        self.commands.push(PathCommand::MoveToRel { dx, dy });
        self
    }

    /// Draw a line to an absolute position.
    #[must_use]
    pub fn line_to(mut self, x: f32, y: f32) -> Self {
        self.commands.push(PathCommand::LineTo { x, y });
        self
    }

    /// Draw a line to a relative position.
    #[must_use]
    pub fn line_to_rel(mut self, dx: f32, dy: f32) -> Self {
        self.commands.push(PathCommand::LineToRel { dx, dy });
        self
    }

    /// Draw a horizontal line to an absolute x position.
    #[must_use]
    pub fn horizontal_to(mut self, x: f32) -> Self {
        self.commands.push(PathCommand::HorizontalTo { x });
        self
    }

    /// Draw a horizontal line to a relative x position.
    #[must_use]
    pub fn horizontal_to_rel(mut self, dx: f32) -> Self {
        self.commands.push(PathCommand::HorizontalToRel { dx });
        self
    }

    /// Draw a vertical line to an absolute y position.
    #[must_use]
    pub fn vertical_to(mut self, y: f32) -> Self {
        self.commands.push(PathCommand::VerticalTo { y });
        self
    }

    /// Draw a vertical line to a relative y position.
    #[must_use]
    pub fn vertical_to_rel(mut self, dy: f32) -> Self {
        self.commands.push(PathCommand::VerticalToRel { dy });
        self
    }

    /// Draw a cubic bezier curve to an absolute position.
    #[must_use]
    pub fn curve_to(mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) -> Self {
        self.commands.push(PathCommand::CurveTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        });
        self
    }

    /// Draw a cubic bezier curve to a relative position.
    #[must_use]
    pub fn curve_to_rel(
        mut self,
        dx1: f32,
        dy1: f32,
        dx2: f32,
        dy2: f32,
        dx: f32,
        dy: f32,
    ) -> Self {
        self.commands.push(PathCommand::CurveToRel {
            dx1,
            dy1,
            dx2,
            dy2,
            dx,
            dy,
        });
        self
    }

    /// Draw a smooth cubic bezier curve to an absolute position.
    #[must_use]
    pub fn smooth_curve_to(mut self, x2: f32, y2: f32, x: f32, y: f32) -> Self {
        self.commands
            .push(PathCommand::SmoothCurveTo { x2, y2, x, y });
        self
    }

    /// Draw a smooth cubic bezier curve to a relative position.
    #[must_use]
    pub fn smooth_curve_to_rel(mut self, dx2: f32, dy2: f32, dx: f32, dy: f32) -> Self {
        self.commands
            .push(PathCommand::SmoothCurveToRel { dx2, dy2, dx, dy });
        self
    }

    /// Draw a quadratic bezier curve to an absolute position.
    #[must_use]
    pub fn quadratic_to(mut self, x1: f32, y1: f32, x: f32, y: f32) -> Self {
        self.commands
            .push(PathCommand::QuadraticTo { x1, y1, x, y });
        self
    }

    /// Draw a quadratic bezier curve to a relative position.
    #[must_use]
    pub fn quadratic_to_rel(mut self, dx1: f32, dy1: f32, dx: f32, dy: f32) -> Self {
        self.commands
            .push(PathCommand::QuadraticToRel { dx1, dy1, dx, dy });
        self
    }

    /// Draw a smooth quadratic bezier curve to an absolute position.
    #[must_use]
    pub fn smooth_quadratic_to(mut self, x: f32, y: f32) -> Self {
        self.commands.push(PathCommand::SmoothQuadraticTo { x, y });
        self
    }

    /// Draw a smooth quadratic bezier curve to a relative position.
    #[must_use]
    pub fn smooth_quadratic_to_rel(mut self, dx: f32, dy: f32) -> Self {
        self.commands
            .push(PathCommand::SmoothQuadraticToRel { dx, dy });
        self
    }

    /// Draw an elliptical arc to an absolute position.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn arc_to(
        mut self,
        rx: f32,
        ry: f32,
        x_rotation: f32,
        large_arc: bool,
        sweep: bool,
        x: f32,
        y: f32,
    ) -> Self {
        self.commands.push(PathCommand::Arc {
            rx,
            ry,
            x_rotation,
            large_arc,
            sweep,
            x,
            y,
        });
        self
    }

    /// Draw an elliptical arc to a relative position.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn arc_to_rel(
        mut self,
        rx: f32,
        ry: f32,
        x_rotation: f32,
        large_arc: bool,
        sweep: bool,
        dx: f32,
        dy: f32,
    ) -> Self {
        self.commands.push(PathCommand::ArcRel {
            rx,
            ry,
            x_rotation,
            large_arc,
            sweep,
            dx,
            dy,
        });
        self
    }

    /// Close the current sub-path.
    #[must_use]
    pub fn close(mut self) -> Self {
        self.commands.push(PathCommand::Close);
        self
    }

    /// Build the path string.
    #[must_use]
    pub fn build(&self) -> String {
        let mut output = String::with_capacity(self.commands.len() * 16);
        for (i, cmd) in self.commands.iter().enumerate() {
            if i > 0 {
                output.push(' ');
            }
            cmd.render(&mut output);
        }
        output
    }

    /// Get the number of commands in the path.
    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Check if the path is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_smooth_path_into_matches_build_smooth_path() {
        // The append-into variant must be byte-identical to the String-returning builder for every
        // point count (M-only, M+L, and the cubic-Bézier path), incl. negative/fractional coords.
        let point_sets: &[&[(f32, f32)]] = &[
            &[],
            &[(0.0, 0.0)],
            &[(0.0, 0.0), (10.0, 10.0)],
            &[(0.0, 0.0), (40.0, 0.0), (40.0, 80.0), (120.0, 80.0)],
            &[(0.0, 0.0), (10.0, 10.0), (20.0, 5.0), (30.0, 30.0)],
            &[
                (-5.25, 0.0),
                (0.0, -3.5),
                (12.75, 8.0),
                (40.0, -1.0),
                (50.0, 50.0),
            ],
        ];
        for pts in point_sets {
            let returned = build_smooth_path_by(pts.len(), |i| pts[i]);
            let mut appended = String::from("PREFIX:");
            build_smooth_path_by_into(&mut appended, pts.len(), |i| pts[i]);
            assert_eq!(
                appended,
                format!("PREFIX:{returned}"),
                "into-variant diverged for {} points",
                pts.len()
            );
        }
    }

    #[test]
    fn builds_simple_path() {
        let path = PathBuilder::new()
            .move_to(0.0, 0.0)
            .line_to(100.0, 100.0)
            .close()
            .build();
        assert_eq!(path, "M0 0 L100 100 Z");
    }

    #[test]
    fn builds_rectangle() {
        let path = PathBuilder::new()
            .move_to(10.0, 10.0)
            .horizontal_to(90.0)
            .vertical_to(90.0)
            .horizontal_to(10.0)
            .close()
            .build();
        assert_eq!(path, "M10 10 H90 V90 H10 Z");
    }

    #[test]
    fn builds_cubic_bezier() {
        let path = PathBuilder::new()
            .move_to(0.0, 0.0)
            .curve_to(25.0, 50.0, 75.0, 50.0, 100.0, 0.0)
            .build();
        assert!(path.contains("C25 50,75 50,100 0"));
    }

    #[test]
    fn smooth_path_by_index_matches_expected_bytes() {
        let points = [(10.0, 20.0), (30.0, 60.0), (70.0, 60.0), (90.0, 20.0)];
        let path = build_smooth_path_by(points.len(), |index| points[index]);

        // Middle-segment control points clamp into the segment's bounding box (GH#7): the
        // unclamped Catmull-Rom tangents put them at y=70, 10px above the 60→60 crest.
        assert_eq!(
            path,
            "M10 20 C15 30,15 50,30 60 C45 60,55 60,70 60 C85 50,85 30,90 20"
        );
    }

    #[test]
    fn orthogonal_elbows_preserve_endpoint_positions_and_tangents() {
        let points = [(0.0, 0.0), (40.0, 0.0), (40.0, 80.0), (120.0, 80.0)];
        let path = build_smooth_path_by(points.len(), |index| points[index]);
        // The terminal lines retain the horizontal port/arrow directions. Each quadratic's
        // control point shares the incoming axis with its start and the outgoing axis with
        // its end, so there is no kink where the straight leg meets the elbow.
        assert_eq!(path, "M0 0 L34 0 Q40 0,40 6 L40 74 Q40 80,46 80 L120 80");
    }

    #[test]
    fn short_orthogonal_segments_share_trim_without_reversing() {
        let points = [
            (0.0, 0.0),
            (40.0, 0.0),
            (40.0, 2.0),
            (80.0, 2.0),
            (80.0, 40.0),
        ];
        let path = build_smooth_path_by(points.len(), |index| points[index]);
        assert_eq!(
            path,
            "M0 0 L39 0 Q40 0,40 1 Q40 2,41 2 L74 2 Q80 2,80 8 L80 40"
        );
        for pair in sample_rounded_path(&path).windows(2) {
            assert!(pair[0].0 <= pair[1].0 && pair[0].1 <= pair[1].1);
        }

        let short_ports = [(0.0, 0.0), (4.0, 0.0), (4.0, 12.0), (8.0, 12.0)];
        assert_eq!(
            build_smooth_path_by(short_ports.len(), |index| short_ports[index]),
            "M0 0 L2 0 Q4 0,4 2 L4 10 Q4 12,6 12 L8 12"
        );
    }

    #[test]
    fn orthogonal_rounding_ignores_duplicate_and_collinear_waypoints() {
        let points = [
            (0.0, 0.0),
            (0.0, 0.0),
            (39.0, 0.0),
            (40.0, 0.0),
            (40.0, 0.0),
            (40.0, 1.0),
            (40.0, 79.0),
            (40.0, 80.0),
            (41.0, 80.0),
            (120.0, 80.0),
            (120.0, 80.0),
        ];
        assert_eq!(
            build_smooth_path_by(points.len(), |index| points[index]),
            "M0 0 L34 0 Q40 0,40 6 L40 74 Q40 80,46 80 L120 80"
        );
        assert_eq!(build_smooth_path_by(4, |_| (5.0, 9.0)), "M5 9");
    }

    #[test]
    fn orthogonal_rounding_preserves_near_straight_collapse_and_reversals() {
        let nearly_straight = [(0.0, 0.0), (40.0, 0.0), (40.0, 1.0), (120.0, 1.0)];
        assert_eq!(
            build_smooth_path_by(nearly_straight.len(), |index| nearly_straight[index]),
            "M0 0 L120 1"
        );
        let reversal = [(0.0, 0.0), (40.0, 0.0), (0.0, 0.0), (0.0, 40.0)];
        assert_eq!(
            build_smooth_path_by(reversal.len(), |index| reversal[index]),
            "M0 0 L40 0 L6 0 Q0 0,0 6 L0 40"
        );
    }

    #[test]
    fn acute_nonorthogonal_routes_keep_the_bounded_cubic_curve() {
        let points = [(0.0, 0.0), (40.0, 40.0), (4.0, 80.0)];
        assert_eq!(
            build_smooth_path_by(points.len(), |index| points[index]),
            "M0 0 C10 10,39 20,40 40 C40 60,13 70,4 80"
        );
    }

    #[test]
    fn rounded_elbows_stay_in_the_router_clearance_corridor() {
        // Obstacles occupy the inside of both turns, exactly 8px from the routed legs.
        // Sample the emitted geometry, including mirrored turns, rather than just inspecting
        // the control points: a smooth path must neither hit either obstacle nor overshoot
        // the endpoint bounds anywhere between them.
        let route = [(0.0, 0.0), (40.0, 0.0), (40.0, 80.0), (120.0, 80.0)];
        for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
            let path = build_smooth_path_by(route.len(), |index| {
                (route[index].0 * sx, route[index].1 * sy)
            });
            assert!(
                path.contains('Q'),
                "route must actually have rounded elbows"
            );
            for (x, y) in sample_rounded_path(&path) {
                let (x, y) = (x * sx, y * sy);
                assert!((0.0..=120.0).contains(&x) && (0.0..=80.0).contains(&y));
                for (left, top, right, bottom) in [(8.0, 8.0, 32.0, 72.0), (48.0, 8.0, 112.0, 72.0)]
                {
                    assert!(x < left || x > right || y < top || y > bottom);
                }
                let distance = route
                    .windows(2)
                    .map(|pair| {
                        let (dx, dy) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
                        let t = (((x - pair[0].0) * dx + (y - pair[0].1) * dy)
                            / (dx * dx + dy * dy))
                            .clamp(0.0, 1.0);
                        (x - pair[0].0 - t * dx).hypot(y - pair[0].1 - t * dy)
                    })
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    distance <= 1.501,
                    "curve escaped the local channel: {distance}"
                );
            }
        }
    }

    /// Evaluate the emitted SVG line/quadratic commands independently of the path builder.
    fn sample_rounded_path(path: &str) -> Vec<(f32, f32)> {
        let commands = path.chars().filter(|ch| matches!(ch, 'M' | 'L' | 'Q'));
        let arguments = path.split(['M', 'L', 'Q']).skip(1);
        let mut current = (0.0, 0.0);
        let mut samples = Vec::new();
        for (command, arguments) in commands.zip(arguments) {
            let values: Vec<f32> = arguments
                .split(|ch: char| ch.is_whitespace() || ch == ',')
                .filter(|value| !value.is_empty())
                .map(|value| value.parse().expect("finite path coordinate"))
                .collect();
            let end = (values[values.len() - 2], values[values.len() - 1]);
            if command == 'M' {
                samples.push(end);
            } else {
                for step in 1..=32 {
                    let t = step as f32 / 32.0;
                    let u = 1.0 - t;
                    samples.push(if command == 'Q' {
                        (
                            u * u * current.0 + 2.0 * u * t * values[0] + t * t * end.0,
                            u * u * current.1 + 2.0 * u * t * values[1] + t * t * end.1,
                        )
                    } else {
                        (u * current.0 + t * end.0, u * current.1 + t * end.1)
                    });
                }
            }
            current = end;
        }
        samples
    }

    #[test]
    fn builds_quadratic_bezier() {
        let path = PathBuilder::new()
            .move_to(0.0, 0.0)
            .quadratic_to(50.0, 100.0, 100.0, 0.0)
            .build();
        assert!(path.contains("Q50 100,100 0"));
    }

    #[test]
    fn builds_arc() {
        let path = PathBuilder::new()
            .move_to(10.0, 10.0)
            .arc_to(20.0, 20.0, 0.0, false, true, 50.0, 50.0)
            .build();
        assert!(path.contains("A20 20 0 0 1 50 50"));
    }

    #[test]
    fn builds_relative_path() {
        let path = PathBuilder::new()
            .move_to(0.0, 0.0)
            .line_to_rel(10.0, 10.0)
            .horizontal_to_rel(20.0)
            .vertical_to_rel(20.0)
            .build();
        assert!(path.contains("l10 10"));
        assert!(path.contains("h20"));
        assert!(path.contains("v20"));
    }

    #[test]
    fn formats_floats_correctly() {
        let path = PathBuilder::new()
            .move_to(10.5, 20.25)
            .line_to(30.0, 40.0)
            .build();
        assert!(path.contains("M10.50 20.25"));
        assert!(path.contains("L30 40"));
    }
}
