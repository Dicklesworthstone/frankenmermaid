//! CGA-based transform stack for SVG rendering.
//!
//! This module provides a `CgaTransformStack` that uses Conformal Geometric Algebra
//! rotor composition internally while producing SVG-compatible output.
//!
//! # Advantages over matrix-based transforms
//!
//! 1. **Easy rotation extraction**: `rotation_angle()` trivially extracts rotation
//!    for text counter-rotation, without needing SVD decomposition.
//! 2. **Interpolation**: Rotors can be slerped for smooth transform interpolation.
//! 3. **Unified algebra**: Translations, rotations, and scales compose naturally.

use fm_core::cga::{AffineMatrix2D, TransformStack};

/// A CGA-backed transform stack for SVG rendering.
///
/// Wraps `fm_core::cga::TransformStack` and provides SVG-specific utilities.
#[derive(Debug, Clone, Default)]
pub struct CgaTransformStack {
    inner: TransformStack,
}

impl CgaTransformStack {
    /// Create a new empty transform stack (identity).
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: TransformStack::new(),
        }
    }

    /// Push a translation transform.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.inner.push_translation(f64::from(dx), f64::from(dy));
    }

    /// Push a uniform scale transform.
    pub fn scale(&mut self, factor: f32) {
        self.inner.push_scale(f64::from(factor));
    }

    /// Push a rotation transform (angle in degrees).
    pub fn rotate(&mut self, degrees: f32) {
        let radians = f64::from(degrees) * std::f64::consts::PI / 180.0;
        self.inner.push_rotation(radians);
    }

    /// Push a rotation transform (angle in radians).
    pub fn rotate_rad(&mut self, radians: f32) {
        self.inner.push_rotation(f64::from(radians));
    }

    /// Pop the most recent transform.
    pub fn pop(&mut self) -> bool {
        self.inner.pop()
    }

    /// Reset to identity transform.
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Get the rotation angle in degrees for text counter-rotation.
    ///
    /// This is the key advantage of CGA: extracting rotation is trivial
    /// and doesn't require matrix decomposition.
    #[must_use]
    pub fn rotation_degrees(&self) -> f32 {
        let radians = self.inner.rotation_angle();
        (radians * 180.0 / std::f64::consts::PI) as f32
    }

    /// Get the rotation angle in radians.
    #[must_use]
    pub fn rotation_radians(&self) -> f64 {
        self.inner.rotation_angle()
    }

    /// Get the translation component.
    #[must_use]
    pub fn translation(&self) -> (f32, f32) {
        let (x, y) = self.inner.translation();
        (x as f32, y as f32)
    }

    /// Get the scale factor.
    #[must_use]
    pub fn scale_factor(&self) -> f32 {
        self.inner.scale_factor() as f32
    }

    /// Apply the transform to a point.
    #[must_use]
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        let (rx, ry) = self.inner.apply(f64::from(x), f64::from(y));
        (rx as f32, ry as f32)
    }

    /// Convert to SVG matrix transform attribute value.
    ///
    /// Returns a string like "matrix(a,b,c,d,e,f)" suitable for SVG transform attribute.
    #[must_use]
    pub fn to_svg_matrix(&self) -> String {
        self.inner.to_svg_transform()
    }

    /// Check if this is the identity transform.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.inner.is_identity()
    }

    /// Get the underlying affine matrix.
    #[must_use]
    pub fn to_affine(&self) -> AffineMatrix2D {
        self.inner.to_affine_matrix()
    }
}

/// Convert a RenderTransform to an SVG matrix string.
///
/// For bit-identical output with the existing TransformBuilder approach,
/// this directly formats the matrix without rotor round-trip conversion.
pub fn render_transform_to_svg_matrix(transform: fm_layout::RenderTransform) -> String {
    match transform {
        fm_layout::RenderTransform::Matrix { a, b, c, d, e, f } => {
            // Format directly matching TransformBuilder output
            format!(
                "matrix({},{},{},{},{},{})",
                fmt_svg_num(a),
                fmt_svg_num(b),
                fmt_svg_num(c),
                fmt_svg_num(d),
                fmt_svg_num(e),
                fmt_svg_num(f)
            )
        }
    }
}

/// Format a number for SVG transform output (matches TransformBuilder).
fn fmt_svg_num(n: f32) -> String {
    if !n.is_finite() {
        return "0".to_string();
    }
    if n.fract() == 0.0 && n >= i32::MIN as f32 && n <= i32::MAX as f32 {
        format!("{}", n as i32)
    } else {
        format!("{n:.2}")
    }
}

/// Convert a similarity `RenderTransform` to a CGA transform stack operation.
///
/// CGA rotors represent translation, rotation, and uniform positive scale. SVG matrices may also
/// encode reflection, shear, or non-uniform scale; those have no exact rotor representation, so
/// they return `None` rather than silently changing the rendered transform.
#[must_use]
pub fn try_render_transform_to_cga(
    transform: fm_layout::RenderTransform,
) -> Option<CgaTransformStack> {
    let mut stack = CgaTransformStack::new();
    match transform {
        fm_layout::RenderTransform::Matrix { a, b, c, d, e, f } => {
            // SVG uses x' = a*x + c*y + e, y' = b*x + d*y + f; AffineMatrix2D stores
            // x' = a*x + b*y + tx, y' = c*x + d*y + ty.
            let matrix = AffineMatrix2D {
                a: f64::from(a),
                b: f64::from(c),
                tx: f64::from(e),
                c: f64::from(b),
                d: f64::from(d),
                ty: f64::from(f),
            };
            // Keep the renderer on the core CGA contract. A rotor has no exact representation for
            // a shear, reflection, or anisotropic scale, so accepting one here would silently
            // change the transform when the stack decomposes it again.
            if !stack.inner.try_push_matrix(matrix) {
                return None;
            }
        }
    }
    Some(stack)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cga_stack_identity() {
        let stack = CgaTransformStack::new();
        assert!(stack.is_identity());
        let (x, y) = stack.apply(1.0, 2.0);
        assert!((x - 1.0).abs() < 0.001);
        assert!((y - 2.0).abs() < 0.001);
    }

    #[test]
    fn cga_stack_translate() {
        let mut stack = CgaTransformStack::new();
        stack.translate(10.0, 20.0);
        let (x, y) = stack.apply(0.0, 0.0);
        assert!((x - 10.0).abs() < 0.001);
        assert!((y - 20.0).abs() < 0.001);
    }

    #[test]
    fn cga_stack_rotate_90() {
        let mut stack = CgaTransformStack::new();
        stack.rotate(90.0);
        let (x, y) = stack.apply(1.0, 0.0);
        // Rotating (1,0) by 90° gives (0,1)
        assert!(x.abs() < 0.001, "x should be ~0, got {x}");
        assert!((y - 1.0).abs() < 0.001, "y should be ~1, got {y}");
    }

    #[test]
    fn cga_stack_rotation_extraction() {
        let mut stack = CgaTransformStack::new();
        stack.rotate(45.0);
        let extracted = stack.rotation_degrees();
        assert!(
            (extracted - 45.0).abs() < 0.1,
            "expected ~45, got {extracted}"
        );
    }

    #[test]
    fn cga_stack_to_svg() {
        let mut stack = CgaTransformStack::new();
        stack.translate(10.0, 20.0);
        let svg = stack.to_svg_matrix();
        assert!(svg.starts_with("matrix("));
        assert!(svg.contains("10"));
        assert!(svg.contains("20"));
    }

    #[test]
    fn render_transform_conversion_preserves_svg_axis_order() {
        use fm_layout::RenderTransform;
        let transform = RenderTransform::Matrix {
            a: 0.0,
            b: 1.0,
            c: -1.0,
            d: 0.0,
            e: 5.0,
            f: 10.0,
        };
        let transformed = try_render_transform_to_cga(transform).map(|stack| stack.apply(1.0, 0.0));
        assert!(
            matches!(transformed, Some((x, y)) if (x - 5.0).abs() < 0.001 && (y - 11.0).abs() < 0.001)
        );
    }

    #[test]
    fn render_transform_conversion_rejects_non_rotor_matrices() {
        use fm_layout::RenderTransform;
        for transform in [
            RenderTransform::Matrix {
                a: 1.0,
                b: 0.0,
                c: 0.5,
                d: 1.0,
                e: 0.0,
                f: 0.0,
            },
            RenderTransform::Matrix {
                a: 2.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: 0.0,
                f: 0.0,
            },
            RenderTransform::Matrix {
                a: -1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: 0.0,
                f: 0.0,
            },
            RenderTransform::Matrix {
                a: f32::NAN,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: 0.0,
                f: 0.0,
            },
        ] {
            assert!(try_render_transform_to_cga(transform).is_none());
        }
    }

    #[test]
    fn render_transform_conversion_refuses_nearby_but_anisotropic_scale() {
        use fm_layout::RenderTransform;

        // A permissive renderer-local epsilon used to accept this as a "similarity" even though
        // converting it to a rotor drops the distinct y scale. The core contract is deliberately
        // stricter: the public adapter must either preserve the transform exactly or decline it.
        let almost_uniform = RenderTransform::Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.000_01,
            e: 4.0,
            f: -3.0,
        };

        assert!(
            try_render_transform_to_cga(almost_uniform).is_none(),
            "a rotor must not silently collapse an anisotropic SVG matrix"
        );
    }
}
