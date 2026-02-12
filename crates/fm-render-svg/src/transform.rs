//! SVG transform builder.
//!
//! Provides `TransformBuilder` for constructing SVG transform attribute values.

use std::fmt::Write;

/// Individual transform operations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Transform {
    /// Translate by (x, y).
    Translate { x: f32, y: f32 },
    /// Scale by (sx, sy).
    Scale { sx: f32, sy: f32 },
    /// Rotate by angle (degrees) around optional center point.
    Rotate {
        angle: f32,
        cx: Option<f32>,
        cy: Option<f32>,
    },
    /// Skew along X axis by angle (degrees).
    SkewX { angle: f32 },
    /// Skew along Y axis by angle (degrees).
    SkewY { angle: f32 },
    /// Apply a transformation matrix.
    Matrix {
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
    },
}

impl Transform {
    /// Render the transform to SVG syntax.
    fn render(&self, output: &mut String) {
        match self {
            Self::Translate { x, y } => {
                if *y == 0.0 {
                    let _ = write!(output, "translate({})", fmt_num(*x));
                } else {
                    let _ = write!(output, "translate({},{})", fmt_num(*x), fmt_num(*y));
                }
            }
            Self::Scale { sx, sy } => {
                if (sx - sy).abs() < f32::EPSILON {
                    let _ = write!(output, "scale({})", fmt_num(*sx));
                } else {
                    let _ = write!(output, "scale({},{})", fmt_num(*sx), fmt_num(*sy));
                }
            }
            Self::Rotate { angle, cx, cy } => {
                if let (Some(cx), Some(cy)) = (cx, cy) {
                    let _ = write!(
                        output,
                        "rotate({},{},{})",
                        fmt_num(*angle),
                        fmt_num(*cx),
                        fmt_num(*cy)
                    );
                } else {
                    let _ = write!(output, "rotate({})", fmt_num(*angle));
                }
            }
            Self::SkewX { angle } => {
                let _ = write!(output, "skewX({})", fmt_num(*angle));
            }
            Self::SkewY { angle } => {
                let _ = write!(output, "skewY({})", fmt_num(*angle));
            }
            Self::Matrix { a, b, c, d, e, f } => {
                let _ = write!(
                    output,
                    "matrix({},{},{},{},{},{})",
                    fmt_num(*a),
                    fmt_num(*b),
                    fmt_num(*c),
                    fmt_num(*d),
                    fmt_num(*e),
                    fmt_num(*f)
                );
            }
        }
    }
}

/// Format a number for SVG transform output.
fn fmt_num(n: f32) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i32)
    } else {
        format!("{:.2}", n)
    }
}

/// Fluent builder for SVG transform attribute values.
#[derive(Debug, Clone, Default)]
pub struct TransformBuilder {
    transforms: Vec<Transform>,
}

impl TransformBuilder {
    /// Create a new empty transform builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            transforms: Vec::new(),
        }
    }

    /// Add a translation transform.
    #[must_use]
    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.transforms.push(Transform::Translate { x, y });
        self
    }

    /// Add a translation in the X direction only.
    #[must_use]
    pub fn translate_x(self, x: f32) -> Self {
        self.translate(x, 0.0)
    }

    /// Add a translation in the Y direction only.
    #[must_use]
    pub fn translate_y(self, y: f32) -> Self {
        self.translate(0.0, y)
    }

    /// Add a uniform scale transform.
    #[must_use]
    pub fn scale_uniform(mut self, s: f32) -> Self {
        self.transforms.push(Transform::Scale { sx: s, sy: s });
        self
    }

    /// Add a non-uniform scale transform.
    #[must_use]
    pub fn scale(mut self, sx: f32, sy: f32) -> Self {
        self.transforms.push(Transform::Scale { sx, sy });
        self
    }

    /// Add a scale in the X direction only.
    #[must_use]
    pub fn scale_x(self, sx: f32) -> Self {
        self.scale(sx, 1.0)
    }

    /// Add a scale in the Y direction only.
    #[must_use]
    pub fn scale_y(self, sy: f32) -> Self {
        self.scale(1.0, sy)
    }

    /// Add a rotation transform around the origin.
    #[must_use]
    pub fn rotate(mut self, angle: f32) -> Self {
        self.transforms.push(Transform::Rotate {
            angle,
            cx: None,
            cy: None,
        });
        self
    }

    /// Add a rotation transform around a specific point.
    #[must_use]
    pub fn rotate_around(mut self, angle: f32, cx: f32, cy: f32) -> Self {
        self.transforms.push(Transform::Rotate {
            angle,
            cx: Some(cx),
            cy: Some(cy),
        });
        self
    }

    /// Add a skew transform along the X axis.
    #[must_use]
    pub fn skew_x(mut self, angle: f32) -> Self {
        self.transforms.push(Transform::SkewX { angle });
        self
    }

    /// Add a skew transform along the Y axis.
    #[must_use]
    pub fn skew_y(mut self, angle: f32) -> Self {
        self.transforms.push(Transform::SkewY { angle });
        self
    }

    /// Add a matrix transform.
    #[must_use]
    pub fn matrix(mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> Self {
        self.transforms.push(Transform::Matrix { a, b, c, d, e, f });
        self
    }

    /// Build the transform string.
    #[must_use]
    pub fn build(&self) -> String {
        let mut output = String::with_capacity(self.transforms.len() * 24);
        for (i, transform) in self.transforms.iter().enumerate() {
            if i > 0 {
                output.push(' ');
            }
            transform.render(&mut output);
        }
        output
    }

    /// Check if the transform is empty (identity).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.transforms.is_empty()
    }

    /// Get the number of transforms.
    #[must_use]
    pub fn len(&self) -> usize {
        self.transforms.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_translate() {
        let t = TransformBuilder::new().translate(10.0, 20.0).build();
        assert_eq!(t, "translate(10,20)");
    }

    #[test]
    fn builds_translate_x_only() {
        let t = TransformBuilder::new().translate_x(15.0).build();
        assert_eq!(t, "translate(15)");
    }

    #[test]
    fn builds_uniform_scale() {
        let t = TransformBuilder::new().scale_uniform(2.0).build();
        assert_eq!(t, "scale(2)");
    }

    #[test]
    fn builds_non_uniform_scale() {
        let t = TransformBuilder::new().scale(2.0, 3.0).build();
        assert_eq!(t, "scale(2,3)");
    }

    #[test]
    fn builds_rotate() {
        let t = TransformBuilder::new().rotate(45.0).build();
        assert_eq!(t, "rotate(45)");
    }

    #[test]
    fn builds_rotate_around() {
        let t = TransformBuilder::new()
            .rotate_around(90.0, 50.0, 50.0)
            .build();
        assert_eq!(t, "rotate(90,50,50)");
    }

    #[test]
    fn builds_skew_x() {
        let t = TransformBuilder::new().skew_x(30.0).build();
        assert_eq!(t, "skewX(30)");
    }

    #[test]
    fn builds_skew_y() {
        let t = TransformBuilder::new().skew_y(15.0).build();
        assert_eq!(t, "skewY(15)");
    }

    #[test]
    fn builds_matrix() {
        let t = TransformBuilder::new()
            .matrix(1.0, 0.0, 0.0, 1.0, 10.0, 20.0)
            .build();
        assert_eq!(t, "matrix(1,0,0,1,10,20)");
    }

    #[test]
    fn chains_multiple_transforms() {
        let t = TransformBuilder::new()
            .translate(10.0, 20.0)
            .rotate(45.0)
            .scale_uniform(2.0)
            .build();
        assert_eq!(t, "translate(10,20) rotate(45) scale(2)");
    }

    #[test]
    fn formats_floats_correctly() {
        let t = TransformBuilder::new().translate(10.5, 20.25).build();
        assert!(t.contains("10.50"));
        assert!(t.contains("20.25"));
    }
}
