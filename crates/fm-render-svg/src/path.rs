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
        if n.fract() == 0.0 && n >= i32::MIN as f32 && n <= i32::MAX as f32 {
            write!(out, "{}", n as i32)
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

/// Build a smooth cubic-Bézier path `d` string directly, with no intermediate
/// `Vec<PathCommand>` — Catmull-Rom→cubic conversion (tension 1/4), the same shape a
/// `PathBuilder` of `move_to`/`line_to`/`curve_to` would produce. Byte-identical to the
/// builder output (commands joined by single spaces). This is the per-edge hot path on
/// curve-heavy graphs, where the builder's per-segment enum push + dispatch is pure
/// overhead on top of the byte writing.
pub(crate) fn build_smooth_path(points: &[(f32, f32)]) -> String {
    let n = points.len();
    if n == 0 {
        return String::new();
    }
    let mut d = String::with_capacity(n * 24);
    write_point(&mut d, 'M', points[0].0, points[0].1);
    if n == 1 {
        return d;
    }
    if n == 2 {
        d.push(' ');
        write_point(&mut d, 'L', points[1].0, points[1].1);
        return d;
    }

    let t: f32 = 0.25;
    for i in 0..(n - 1) {
        let p_prev = if i == 0 { points[0] } else { points[i - 1] };
        let p_cur = points[i];
        let p_next = points[i + 1];
        let p_next2 = if i + 2 < n {
            points[i + 2]
        } else {
            points[n - 1]
        };

        let cp1x = p_cur.0 + (p_next.0 - p_prev.0) * t;
        let cp1y = p_cur.1 + (p_next.1 - p_prev.1) * t;
        let cp2x = p_next.0 - (p_next2.0 - p_cur.0) * t;
        let cp2y = p_next.1 - (p_next2.1 - p_cur.1) * t;

        d.push(' ');
        write_cubic(
            &mut d,
            'C',
            (cp1x, cp1y),
            (cp2x, cp2y),
            (p_next.0, p_next.1),
        );
    }
    d
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
