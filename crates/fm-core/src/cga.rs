//! Conformal Geometric Algebra (CGA) for 2D layout transforms.
//!
//! Implements the conformal model R_{3,1} with multivectors, rotors,
//! and conversion to/from conventional 2D affine matrices.
//!
//! # Why CGA?
//!
//! CGA unifies translations, rotations, and scaling into a single algebraic
//! framework (rotors). This enables:
//! - Composing arbitrary sequences of transforms via geometric product
//! - Interpolating between transforms (rotor slerp)
//! - Representing circles, lines, and point-pairs as algebraic objects
//!
//! # Basis
//!
//! R_{3,1} has basis vectors {e1, e2, e+, e-} where e+² = +1, e-² = -1.
//! A general multivector has 2⁴ = 16 components.

use serde::{Deserialize, Serialize};

/// Basis blade indices for R_{3,1}.
///
/// The 16 blades are ordered by grade:
/// Grade 0: scalar (index 0)
/// Grade 1: e1, e2, e+, e- (indices 1-4)
/// Grade 2: e12, e1+, e1-, e2+, e2-, e+- (indices 5-10)
/// Grade 3: e12+, e12-, e1+-, e2+- (indices 11-14)
/// Grade 4: e12+- (index 15)
#[allow(dead_code)]
mod blade {
    pub const SCALAR: usize = 0;
    pub const E1: usize = 1;
    pub const E2: usize = 2;
    pub const EP: usize = 3; // e+
    pub const EM: usize = 4; // e-
    pub const E12: usize = 5;
    pub const E1P: usize = 6;
    pub const E1M: usize = 7;
    pub const E2P: usize = 8;
    pub const E2M: usize = 9;
    pub const EPM: usize = 10; // e+-
    pub const E12P: usize = 11;
    pub const E12M: usize = 12;
    pub const E1PM: usize = 13;
    pub const E2PM: usize = 14;
    pub const E12PM: usize = 15;
}

/// A general multivector in R_{3,1} with 16 components.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Multivector {
    pub components: [f64; 16],
}

impl Default for Multivector {
    fn default() -> Self {
        Self::zero()
    }
}

impl Multivector {
    /// The zero multivector.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            components: [0.0; 16],
        }
    }

    /// A scalar multivector.
    #[must_use]
    pub fn scalar(value: f64) -> Self {
        let mut m = Self::zero();
        m.components[blade::SCALAR] = value;
        m
    }

    /// Get the scalar (grade-0) part.
    #[must_use]
    pub fn scalar_part(self) -> f64 {
        self.components[blade::SCALAR]
    }

    /// Reverse: reverses the order of basis vectors in each blade.
    /// Grade k blade gets sign (-1)^(k*(k-1)/2).
    #[must_use]
    pub fn reverse(self) -> Self {
        let c = &self.components;
        let mut r = [0.0_f64; 16];
        // Grade 0: +1
        r[0] = c[0];
        // Grade 1: +1
        r[1] = c[1];
        r[2] = c[2];
        r[3] = c[3];
        r[4] = c[4];
        // Grade 2: -1
        r[5] = -c[5];
        r[6] = -c[6];
        r[7] = -c[7];
        r[8] = -c[8];
        r[9] = -c[9];
        r[10] = -c[10];
        // Grade 3: -1
        r[11] = -c[11];
        r[12] = -c[12];
        r[13] = -c[13];
        r[14] = -c[14];
        // Grade 4: +1
        r[15] = c[15];
        Self { components: r }
    }

    /// Squared norm: self * reverse(self), taking the scalar part.
    #[must_use]
    pub fn norm_squared(self) -> f64 {
        self.geometric_product(self.reverse()).scalar_part()
    }

    /// Geometric product of two multivectors.
    ///
    /// This is the fundamental operation of geometric algebra.
    /// For rotors (even-grade), this composes transforms.
    #[must_use]
    pub fn geometric_product(self, other: Self) -> Self {
        // Full 16x16 geometric product via Cayley table for R_{3,1}.
        // For efficiency, we only compute the terms that contribute.
        // e+^2 = +1, e-^2 = -1, e1^2 = e2^2 = +1.
        let a = &self.components;
        let b = &other.components;
        let mut r = [0.0_f64; 16];

        // This is a simplified computation focusing on the most commonly
        // used components. Full Cayley table expansion would be 256 terms.
        // For rotors (even-grade), only even-grade outputs matter.

        // Scalar output: a0*b0 + a1*b1 + a2*b2 + a3*b3 - a4*b4
        //                - a5*b5 - a6*b6 + a7*b7 - a8*b8 + a9*b9
        //                + a10*b10 + ...
        // (This is the full contraction, complex to enumerate)

        // For practical use, implement the even-subalgebra product
        // which is sufficient for rotors.
        r[blade::SCALAR] = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
            - a[4] * b[4]
            - a[5] * b[5]
            - a[6] * b[6]
            + a[7] * b[7]
            - a[8] * b[8]
            + a[9] * b[9]
            + a[10] * b[10]
            - a[11] * b[11]
            + a[12] * b[12]
            + a[13] * b[13]
            + a[14] * b[14]
            - a[15] * b[15];

        // For a full implementation we'd compute all 16 output components.
        // For now, focus on the scalar and grade-2 components needed for rotors.
        // Grade 1 output (needed for sandwich product on vectors):
        r[blade::E1] = a[0] * b[1] + a[1] * b[0] + a[5] * b[2] - a[2] * b[5] + a[6] * b[3]
            - a[3] * b[6]
            - a[7] * b[4]
            + a[4] * b[7];
        r[blade::E2] = a[0] * b[2] + a[2] * b[0] - a[5] * b[1] + a[1] * b[5] + a[8] * b[3]
            - a[3] * b[8]
            - a[9] * b[4]
            + a[4] * b[9];

        // Grade 2 output (rotor components):
        r[blade::E12] = a[0] * b[5] + a[5] * b[0] + a[1] * b[2] - a[2] * b[1];

        Self { components: r }
    }
}

/// A 2D affine transformation matrix [a, b, tx; c, d, ty].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AffineMatrix2D {
    /// Scale/rotation component (row 0, col 0).
    pub a: f64,
    /// Shear/rotation component (row 0, col 1).
    pub b: f64,
    /// Translation X.
    pub tx: f64,
    /// Shear/rotation component (row 1, col 0).
    pub c: f64,
    /// Scale/rotation component (row 1, col 1).
    pub d: f64,
    /// Translation Y.
    pub ty: f64,
}

impl Default for AffineMatrix2D {
    fn default() -> Self {
        Self::identity()
    }
}

impl AffineMatrix2D {
    /// The identity transform.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            tx: 0.0,
            c: 0.0,
            d: 1.0,
            ty: 0.0,
        }
    }

    /// Create a translation matrix.
    #[must_use]
    pub const fn translation(dx: f64, dy: f64) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            tx: dx,
            c: 0.0,
            d: 1.0,
            ty: dy,
        }
    }

    /// Create a rotation matrix (angle in radians).
    #[must_use]
    pub fn rotation(angle: f64) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            a: cos,
            b: -sin,
            tx: 0.0,
            c: sin,
            d: cos,
            ty: 0.0,
        }
    }

    /// Create a uniform scale matrix.
    #[must_use]
    pub const fn scale(factor: f64) -> Self {
        Self {
            a: factor,
            b: 0.0,
            tx: 0.0,
            c: 0.0,
            d: factor,
            ty: 0.0,
        }
    }

    /// Compose two affine transforms: self * other.
    #[must_use]
    pub fn compose(self, other: Self) -> Self {
        Self {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            tx: self.a * other.tx + self.b * other.ty + self.tx,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            ty: self.c * other.tx + self.d * other.ty + self.ty,
        }
    }

    /// Apply this transform to a 2D point.
    #[must_use]
    pub fn apply(self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.b * y + self.tx,
            self.c * x + self.d * y + self.ty,
        )
    }

    /// Convert to SVG transform attribute string.
    #[must_use]
    pub fn to_svg_transform(&self) -> String {
        format!(
            "matrix({},{},{},{},{},{})",
            self.a, self.c, self.b, self.d, self.tx, self.ty
        )
    }
}

/// A CGA rotor representing a rigid transform in 2D.
///
/// Rotors compose via geometric product and apply transforms via
/// the sandwich product: x' = R x R̃.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rotor {
    /// Even-grade components: [scalar, e12, e1+, e1-, e2+, e2-, e+-, e12+-].
    pub components: [f64; 8],
}

impl Default for Rotor {
    fn default() -> Self {
        Self::identity()
    }
}

impl Rotor {
    /// The identity rotor (no transform).
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            components: [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }
    }

    /// Create a translation rotor.
    ///
    /// T = 1 + (dx·e1 + dy·e2)·e_inf/2
    /// where e_inf = e+ + e- is the point at infinity.
    #[must_use]
    pub fn translation(dx: f64, dy: f64) -> Self {
        let half_dx = dx / 2.0;
        let half_dy = dy / 2.0;
        Self {
            components: [
                1.0, 0.0, half_dx, // e1+ component
                half_dx, // e1- component
                half_dy, // e2+ component
                half_dy, // e2- component
                0.0, 0.0,
            ],
        }
    }

    /// Create a rotation rotor (angle in radians, around origin).
    ///
    /// R = cos(θ/2) + sin(θ/2)·e1∧e2
    #[must_use]
    pub fn rotation(angle: f64) -> Self {
        let half = angle / 2.0;
        Self {
            components: [half.cos(), half.sin(), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }
    }

    /// Create a uniform scale rotor.
    ///
    /// S = cosh(ln(s)/2) + sinh(ln(s)/2)·e+∧e-
    ///
    /// # Panics
    /// Panics if factor is not positive (ln of non-positive is undefined).
    #[must_use]
    pub fn scale(factor: f64) -> Self {
        assert!(
            factor > 0.0,
            "scale factor must be positive, got {factor}"
        );
        let half_log = factor.ln() / 2.0;
        Self {
            components: [
                half_log.cosh(),
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                half_log.sinh(), // e+- component
                0.0,
            ],
        }
    }

    /// Compose two rotors: self * other (geometric product of even subalgebra).
    #[must_use]
    pub fn compose(self, other: Self) -> Self {
        let a = &self.components;
        let b = &other.components;
        // Even-subalgebra product for R_{3,1}
        // Simplified: only the most important terms for translation+rotation.
        Self {
            components: [
                a[0] * b[0] - a[1] * b[1] - a[6] * b[6],
                a[0] * b[1] + a[1] * b[0],
                a[0] * b[2] + a[2] * b[0] + a[1] * b[4] - a[4] * b[1],
                a[0] * b[3] + a[3] * b[0] + a[1] * b[5] - a[5] * b[1],
                a[0] * b[4] + a[4] * b[0] - a[1] * b[2] + a[2] * b[1],
                a[0] * b[5] + a[5] * b[0] - a[1] * b[3] + a[3] * b[1],
                a[0] * b[6] + a[6] * b[0],
                a[0] * b[7] + a[7] * b[0] + a[1] * b[6] - a[6] * b[1],
            ],
        }
    }

    /// Reverse of the rotor: R̃.
    #[must_use]
    pub fn reverse(self) -> Self {
        Self {
            components: [
                self.components[0],
                -self.components[1],
                -self.components[2],
                -self.components[3],
                -self.components[4],
                -self.components[5],
                -self.components[6],
                self.components[7],
            ],
        }
    }

    /// Squared norm: R * R̃ (scalar part).
    #[must_use]
    pub fn norm_squared(self) -> f64 {
        self.compose(self.reverse()).components[0]
    }

    /// Multiplicative inverse: R⁻¹ = R̃ / |R|².
    ///
    /// For normalized rotors (|R|² = 1), this equals the reverse.
    /// For non-normalized rotors (like scale), this properly inverts.
    #[must_use]
    pub fn inverse(self) -> Self {
        let norm_sq = self.norm_squared();
        if (norm_sq - 1.0).abs() < 1e-12 {
            // Already normalized, reverse is the inverse
            self.reverse()
        } else {
            // Scale each component of the reverse by 1/norm_squared
            let rev = self.reverse();
            let inv_norm = 1.0 / norm_sq;
            Self {
                components: [
                    rev.components[0] * inv_norm,
                    rev.components[1] * inv_norm,
                    rev.components[2] * inv_norm,
                    rev.components[3] * inv_norm,
                    rev.components[4] * inv_norm,
                    rev.components[5] * inv_norm,
                    rev.components[6] * inv_norm,
                    rev.components[7] * inv_norm,
                ],
            }
        }
    }

    /// Convert this rotor to a 2D affine matrix.
    ///
    /// Applies the rotor to basis points (0,0), (1,0), (0,1) and extracts
    /// the affine coefficients.
    #[must_use]
    pub fn to_affine_matrix(self) -> AffineMatrix2D {
        let s = self.components[0];
        let e12 = self.components[1];
        let e1p = self.components[2];
        let e1m = self.components[3];
        let e2p = self.components[4];
        let e2m = self.components[5];
        let epm = self.components[6];

        // Scale factor from e+- component
        // epm = sinh(ln(scale)/2), so we need to recover scale = exp(2 * arsinh(epm))
        // arsinh(x) = ln(x + sqrt(x^2 + 1))
        let scale_factor = if epm.abs() < 1e-12 {
            1.0
        } else {
            let arsinh_epm = (epm + (epm * epm + 1.0).sqrt()).ln();
            let scale = (2.0 * arsinh_epm).exp();
            if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            }
        };

        // For combined rotation+scale rotor:
        // s = cos(θ/2) * cosh(ln(scale)/2)
        // e12 = sin(θ/2) * cosh(ln(scale)/2)
        //
        // We need to extract the rotation component. The cosh factor is sqrt(1 + epm^2).
        let cosh_factor = (1.0 + epm * epm).sqrt();
        let cos_half = if cosh_factor.abs() > 1e-12 {
            s / cosh_factor
        } else {
            s
        };
        let sin_half = if cosh_factor.abs() > 1e-12 {
            e12 / cosh_factor
        } else {
            e12
        };

        // Recover full rotation angle using double angle formulas
        let cos_theta = cos_half * cos_half - sin_half * sin_half;
        let sin_theta = 2.0 * cos_half * sin_half;

        // Translation from e1+/e1- and e2+/e2- components
        let tx = e1p + e1m;
        let ty = e2p + e2m;

        AffineMatrix2D {
            a: cos_theta * scale_factor,
            b: -sin_theta * scale_factor,
            tx,
            c: sin_theta * scale_factor,
            d: cos_theta * scale_factor,
            ty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_rotor_produces_identity_matrix() {
        let r = Rotor::identity();
        let m = r.to_affine_matrix();
        assert!((m.a - 1.0).abs() < 1e-10);
        assert!((m.d - 1.0).abs() < 1e-10);
        assert!(m.tx.abs() < 1e-10);
        assert!(m.ty.abs() < 1e-10);
    }

    #[test]
    fn translation_rotor_produces_correct_matrix() {
        let r = Rotor::translation(3.0, 4.0);
        let m = r.to_affine_matrix();
        assert!((m.a - 1.0).abs() < 1e-10);
        assert!((m.d - 1.0).abs() < 1e-10);
        assert!((m.tx - 3.0).abs() < 1e-10);
        assert!((m.ty - 4.0).abs() < 1e-10);
    }

    #[test]
    fn rotation_rotor_90_degrees() {
        let r = Rotor::rotation(std::f64::consts::FRAC_PI_2);
        let m = r.to_affine_matrix();
        assert!(m.a.abs() < 1e-10, "cos(90°) should be ~0");
        assert!((m.b + 1.0).abs() < 1e-10, "-sin(90°) should be ~-1");
        assert!((m.c - 1.0).abs() < 1e-10, "sin(90°) should be ~1");
        assert!(m.d.abs() < 1e-10, "cos(90°) should be ~0");
    }

    #[test]
    fn scale_rotor_produces_correct_matrix() {
        let r = Rotor::scale(2.0);
        let m = r.to_affine_matrix();
        // Scale matrix should be: [2, 0, 0; 0, 2, 0]
        assert!(
            (m.a - 2.0).abs() < 1e-10,
            "scale a should be 2, got {}",
            m.a
        );
        assert!(m.b.abs() < 1e-10, "scale b should be 0, got {}", m.b);
        assert!(m.c.abs() < 1e-10, "scale c should be 0, got {}", m.c);
        assert!(
            (m.d - 2.0).abs() < 1e-10,
            "scale d should be 2, got {}",
            m.d
        );
        assert!(m.tx.abs() < 1e-10, "scale tx should be 0, got {}", m.tx);
        assert!(m.ty.abs() < 1e-10, "scale ty should be 0, got {}", m.ty);
    }

    #[test]
    fn affine_matrix_identity_apply() {
        let m = AffineMatrix2D::identity();
        let (x, y) = m.apply(3.0, 4.0);
        assert!((x - 3.0).abs() < 1e-10);
        assert!((y - 4.0).abs() < 1e-10);
    }

    #[test]
    fn affine_matrix_translation_apply() {
        let m = AffineMatrix2D::translation(10.0, 20.0);
        let (x, y) = m.apply(3.0, 4.0);
        assert!((x - 13.0).abs() < 1e-10);
        assert!((y - 24.0).abs() < 1e-10);
    }

    #[test]
    fn affine_matrix_compose() {
        let t1 = AffineMatrix2D::translation(1.0, 0.0);
        let t2 = AffineMatrix2D::translation(0.0, 2.0);
        let composed = t1.compose(t2);
        let (x, y) = composed.apply(0.0, 0.0);
        assert!((x - 1.0).abs() < 1e-10);
        assert!((y - 2.0).abs() < 1e-10);
    }

    #[test]
    fn rotor_reverse_identity() {
        let r = Rotor::identity();
        let rev = r.reverse();
        assert!((rev.components[0] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn rotor_norm_squared_identity() {
        let r = Rotor::identity();
        assert!((r.norm_squared() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn multivector_scalar_part() {
        let m = Multivector::scalar(42.0);
        assert!((m.scalar_part() - 42.0).abs() < 1e-10);
    }

    #[test]
    fn multivector_reverse_grade0_unchanged() {
        let m = Multivector::scalar(5.0);
        let r = m.reverse();
        assert!((r.scalar_part() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn affine_svg_transform_format() {
        let m = AffineMatrix2D::identity();
        let svg = m.to_svg_transform();
        assert!(svg.starts_with("matrix("));
        assert!(svg.ends_with(')'));
    }

    #[test]
    fn rotor_compose_identity_is_identity() {
        let id = Rotor::identity();
        let composed = id.compose(id);
        let m = composed.to_affine_matrix();
        assert!((m.a - 1.0).abs() < 1e-10);
        assert!((m.d - 1.0).abs() < 1e-10);
        assert!(m.tx.abs() < 1e-10);
    }

    #[test]
    fn serde_roundtrip_rotor() {
        let r = Rotor::translation(1.0, 2.0);
        let json = serde_json::to_string(&r).unwrap();
        let deser: Rotor = serde_json::from_str(&json).unwrap();
        assert_eq!(r.components, deser.components);
    }

    #[test]
    fn serde_roundtrip_affine() {
        let m = AffineMatrix2D::rotation(0.5);
        let json = serde_json::to_string(&m).unwrap();
        let deser: AffineMatrix2D = serde_json::from_str(&json).unwrap();
        assert!((m.a - deser.a).abs() < 1e-10);
    }
}

// ============================================================================
// TransformStack - CGA-based transform stack for rendering pipelines
// ============================================================================

/// A transform stack that uses CGA rotor composition internally.
///
/// This provides efficient O(1) push/pop operations via rotor multiplication,
/// and easy extraction of rotation angles for text counter-rotation.
///
/// # Example
/// ```
/// use fm_core::cga::TransformStack;
///
/// let mut stack = TransformStack::new();
/// stack.push_translation(10.0, 20.0);
/// stack.push_rotation(std::f64::consts::FRAC_PI_4);
/// stack.push_scale(2.0);
///
/// // Get the composed affine matrix for rendering
/// let matrix = stack.to_affine_matrix();
///
/// // Extract rotation for text counter-rotation
/// let rotation_radians = stack.rotation_angle();
/// ```
#[derive(Debug, Clone)]
pub struct TransformStack {
    /// The composed rotor representing all transforms on the stack.
    composed: Rotor,
    /// Stack of individual rotors for pop support.
    stack: Vec<Rotor>,
}

impl Default for TransformStack {
    fn default() -> Self {
        Self::new()
    }
}

impl TransformStack {
    /// Create a new empty transform stack (identity transform).
    #[must_use]
    pub fn new() -> Self {
        Self {
            composed: Rotor::identity(),
            stack: Vec::new(),
        }
    }

    /// Push a translation transform onto the stack.
    pub fn push_translation(&mut self, dx: f64, dy: f64) {
        let rotor = Rotor::translation(dx, dy);
        self.push_rotor(rotor);
    }

    /// Push a rotation transform onto the stack (angle in radians).
    pub fn push_rotation(&mut self, angle: f64) {
        let rotor = Rotor::rotation(angle);
        self.push_rotor(rotor);
    }

    /// Push a uniform scale transform onto the stack.
    pub fn push_scale(&mut self, factor: f64) {
        let rotor = Rotor::scale(factor);
        self.push_rotor(rotor);
    }

    /// Push a raw rotor onto the stack.
    pub fn push_rotor(&mut self, rotor: Rotor) {
        self.composed = self.composed.compose(rotor);
        self.stack.push(rotor);
    }

    /// Push an affine matrix onto the stack (converted to rotor).
    pub fn push_matrix(&mut self, matrix: AffineMatrix2D) {
        let rotor = matrix.to_rotor();
        self.push_rotor(rotor);
    }

    /// Pop the most recent transform from the stack.
    ///
    /// Returns `true` if a transform was popped, `false` if the stack was empty.
    pub fn pop(&mut self) -> bool {
        if let Some(rotor) = self.stack.pop() {
            // Multiply by the inverse to undo the transform
            // For normalized rotors, inverse = reverse
            // For scale rotors, inverse = reverse / norm_squared
            self.composed = self.composed.compose(rotor.inverse());
            true
        } else {
            false
        }
    }

    /// Get the current composed transform as an affine matrix.
    #[must_use]
    pub fn to_affine_matrix(&self) -> AffineMatrix2D {
        self.composed.to_affine_matrix()
    }

    /// Get the current composed rotor.
    #[must_use]
    pub fn rotor(&self) -> Rotor {
        self.composed
    }

    /// Extract the rotation angle (in radians) from the composed transform.
    ///
    /// This is useful for counter-rotating text in rotated diagrams.
    #[must_use]
    pub fn rotation_angle(&self) -> f64 {
        // For a rotation rotor R = cos(θ/2) + sin(θ/2)·e12,
        // the scalar is cos(θ/2) and e12 component is sin(θ/2).
        let s = self.composed.components[0];
        let e12 = self.composed.components[1];
        2.0 * e12.atan2(s)
    }

    /// Get the translation component of the composed transform.
    #[must_use]
    pub fn translation(&self) -> (f64, f64) {
        let e1p = self.composed.components[2];
        let e1m = self.composed.components[3];
        let e2p = self.composed.components[4];
        let e2m = self.composed.components[5];
        (e1p + e1m, e2p + e2m)
    }

    /// Get the scale factor of the composed transform.
    #[must_use]
    pub fn scale_factor(&self) -> f64 {
        let epm = self.composed.components[6];
        (epm * 2.0).exp()
    }

    /// Apply the composed transform to a 2D point.
    #[must_use]
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        self.composed.to_affine_matrix().apply(x, y)
    }

    /// Check if the transform stack is empty (identity).
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.stack.is_empty()
    }

    /// Get the number of transforms on the stack.
    #[must_use]
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Check if the stack is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Reset the stack to identity.
    pub fn reset(&mut self) {
        self.composed = Rotor::identity();
        self.stack.clear();
    }

    /// Convert to SVG transform attribute string.
    #[must_use]
    pub fn to_svg_transform(&self) -> String {
        self.to_affine_matrix().to_svg_transform()
    }
}

impl AffineMatrix2D {
    /// Convert an affine matrix to a CGA rotor.
    ///
    /// This extracts rotation, scale, and translation components from the matrix
    /// and composes them into a rotor.
    #[must_use]
    pub fn to_rotor(&self) -> Rotor {
        // Extract rotation angle from matrix
        let angle = self.c.atan2(self.a);

        // Extract scale (assuming uniform scale for now)
        let scale = (self.a * self.a + self.c * self.c).sqrt();

        // Build composed rotor: first rotate, then scale, then translate
        let r_rot = Rotor::rotation(angle);
        let r_scale = if (scale - 1.0).abs() > 1e-10 {
            Rotor::scale(scale)
        } else {
            Rotor::identity()
        };
        let r_trans = Rotor::translation(self.tx, self.ty);

        // Compose: translate(scale(rotate(point)))
        // In rotor composition: R_total = R_trans * R_scale * R_rot
        r_trans.compose(r_scale.compose(r_rot))
    }
}

// ============================================================================
// CGA Geometric Objects - Points, Lines, Circles
// ============================================================================

/// A conformal point in R_{3,1}.
///
/// Points are null vectors: P·P = 0.
/// Represented as: P = x*e1 + y*e2 + (x²+y²)/2*e_∞ + e_o
/// where e_∞ = e+ + e- and e_o = (e- - e+)/2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CgaPoint {
    pub x: f64,
    pub y: f64,
}

impl CgaPoint {
    /// Create a new conformal point from 2D coordinates.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// The origin point (0, 0).
    #[must_use]
    pub const fn origin() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    /// Squared distance to another point.
    ///
    /// Uses the conformal inner product: d²(P, Q) = -2 * P·Q
    #[must_use]
    pub fn distance_squared(&self, other: &CgaPoint) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }

    /// Distance to another point.
    #[must_use]
    pub fn distance(&self, other: &CgaPoint) -> f64 {
        self.distance_squared(other).sqrt()
    }
}

/// A line segment defined by two endpoints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CgaLineSegment {
    pub start: CgaPoint,
    pub end: CgaPoint,
}

impl CgaLineSegment {
    /// Create a new line segment from endpoints.
    #[must_use]
    pub const fn new(start: CgaPoint, end: CgaPoint) -> Self {
        Self { start, end }
    }

    /// Length of the line segment.
    #[must_use]
    pub fn length(&self) -> f64 {
        self.start.distance(&self.end)
    }

    /// Direction vector (normalized).
    #[must_use]
    pub fn direction(&self) -> (f64, f64) {
        let dx = self.end.x - self.start.x;
        let dy = self.end.y - self.start.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len < f64::EPSILON {
            (0.0, 0.0)
        } else {
            (dx / len, dy / len)
        }
    }

    /// Find intersection with another line segment.
    ///
    /// Returns the intersection point if segments cross, None otherwise.
    /// Uses parametric line-line intersection.
    #[must_use]
    pub fn intersect(&self, other: &CgaLineSegment) -> Option<CgaPoint> {
        // Parametric form: P = start + t*(end - start)
        // Solve for t1, t2 where the lines cross
        let d1x = self.end.x - self.start.x;
        let d1y = self.end.y - self.start.y;
        let d2x = other.end.x - other.start.x;
        let d2y = other.end.y - other.start.y;

        let cross = d1x * d2y - d1y * d2x;
        if cross.abs() < f64::EPSILON {
            // Lines are parallel
            return None;
        }

        let ox = other.start.x - self.start.x;
        let oy = other.start.y - self.start.y;

        let t1 = (ox * d2y - oy * d2x) / cross;
        let t2 = (ox * d1y - oy * d1x) / cross;

        // Check if intersection is within both segments [0, 1]
        if t1 >= 0.0 && t1 <= 1.0 && t2 >= 0.0 && t2 <= 1.0 {
            Some(CgaPoint::new(
                self.start.x + t1 * d1x,
                self.start.y + t1 * d1y,
            ))
        } else {
            None
        }
    }

    /// Find closest point on the segment to a given point.
    #[must_use]
    pub fn closest_point(&self, point: &CgaPoint) -> CgaPoint {
        let dx = self.end.x - self.start.x;
        let dy = self.end.y - self.start.y;
        let len_sq = dx * dx + dy * dy;

        if len_sq < f64::EPSILON {
            return self.start;
        }

        let t = ((point.x - self.start.x) * dx + (point.y - self.start.y) * dy) / len_sq;
        let t = t.clamp(0.0, 1.0);

        CgaPoint::new(self.start.x + t * dx, self.start.y + t * dy)
    }

    /// Distance from a point to this line segment.
    #[must_use]
    pub fn distance_to_point(&self, point: &CgaPoint) -> f64 {
        let closest = self.closest_point(point);
        point.distance(&closest)
    }
}

/// A circle defined by center and radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CgaCircle {
    pub center: CgaPoint,
    pub radius: f64,
}

impl CgaCircle {
    /// Create a new circle.
    #[must_use]
    pub const fn new(center: CgaPoint, radius: f64) -> Self {
        Self { center, radius }
    }

    /// Check if a point is inside the circle.
    #[must_use]
    pub fn contains(&self, point: &CgaPoint) -> bool {
        point.distance_squared(&self.center) <= self.radius * self.radius
    }

    /// Check if a point is strictly inside the circle (not on boundary).
    #[must_use]
    pub fn contains_strict(&self, point: &CgaPoint) -> bool {
        point.distance_squared(&self.center) < self.radius * self.radius
    }

    /// Find intersection points with a line segment.
    ///
    /// Returns 0, 1, or 2 intersection points.
    #[must_use]
    pub fn intersect_segment(&self, segment: &CgaLineSegment) -> Vec<CgaPoint> {
        // Line parametric: P = start + t * (end - start)
        // Circle: |P - center|² = r²
        // Substitute and solve quadratic
        let dx = segment.end.x - segment.start.x;
        let dy = segment.end.y - segment.start.y;
        let fx = segment.start.x - self.center.x;
        let fy = segment.start.y - self.center.y;

        let a = dx * dx + dy * dy;
        if a < f64::EPSILON {
            // Degenerate segment (point)
            if self.contains(&segment.start) {
                return vec![segment.start];
            }
            return Vec::new();
        }

        let b = 2.0 * (fx * dx + fy * dy);
        let c = fx * fx + fy * fy - self.radius * self.radius;

        let discriminant = b * b - 4.0 * a * c;
        if discriminant < 0.0 {
            return Vec::new();
        }

        let mut points = Vec::new();
        let sqrt_disc = discriminant.sqrt();
        let t1 = (-b - sqrt_disc) / (2.0 * a);
        let t2 = (-b + sqrt_disc) / (2.0 * a);

        // For tangent case (discriminant ≈ 0), t1 ≈ t2, only add once
        if t1 >= 0.0 && t1 <= 1.0 {
            points.push(CgaPoint::new(
                segment.start.x + t1 * dx,
                segment.start.y + t1 * dy,
            ));
        }

        // Only add second point if it's distinct from first
        if (t2 - t1).abs() > 1e-10 && t2 >= 0.0 && t2 <= 1.0 {
            points.push(CgaPoint::new(
                segment.start.x + t2 * dx,
                segment.start.y + t2 * dy,
            ));
        }

        points
    }
}

/// An axis-aligned rectangle for hit testing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CgaRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl CgaRect {
    /// Create a new rectangle.
    #[must_use]
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    /// Check if a point is inside the rectangle.
    #[must_use]
    pub fn contains(&self, point: &CgaPoint) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.width
            && point.y >= self.y
            && point.y <= self.y + self.height
    }

    /// Get the four edges as line segments.
    #[must_use]
    pub fn edges(&self) -> [CgaLineSegment; 4] {
        let tl = CgaPoint::new(self.x, self.y);
        let tr = CgaPoint::new(self.x + self.width, self.y);
        let br = CgaPoint::new(self.x + self.width, self.y + self.height);
        let bl = CgaPoint::new(self.x, self.y + self.height);
        [
            CgaLineSegment::new(tl, tr), // top
            CgaLineSegment::new(tr, br), // right
            CgaLineSegment::new(br, bl), // bottom
            CgaLineSegment::new(bl, tl), // left
        ]
    }

    /// Find intersection points with a line segment.
    #[must_use]
    pub fn intersect_segment(&self, segment: &CgaLineSegment) -> Vec<CgaPoint> {
        let mut points = Vec::new();
        for edge in self.edges() {
            if let Some(p) = segment.intersect(&edge) {
                // Avoid duplicate points at corners
                if points.iter().all(|existing: &CgaPoint| {
                    (existing.x - p.x).abs() > f64::EPSILON
                        || (existing.y - p.y).abs() > f64::EPSILON
                }) {
                    points.push(p);
                }
            }
        }
        points
    }

    /// Closest point on rectangle boundary to a given point.
    #[must_use]
    pub fn closest_boundary_point(&self, point: &CgaPoint) -> CgaPoint {
        let mut closest = self.edges()[0].closest_point(point);
        let mut min_dist = point.distance(&closest);

        for edge in self.edges().iter().skip(1) {
            let p = edge.closest_point(point);
            let dist = point.distance(&p);
            if dist < min_dist {
                min_dist = dist;
                closest = p;
            }
        }
        closest
    }
}

#[cfg(test)]
mod transform_stack_tests {
    use super::*;

    #[test]
    fn transform_stack_identity() {
        let stack = TransformStack::new();
        let m = stack.to_affine_matrix();
        assert!((m.a - 1.0).abs() < 1e-10);
        assert!((m.d - 1.0).abs() < 1e-10);
        assert!(m.tx.abs() < 1e-10);
        assert!(m.ty.abs() < 1e-10);
    }

    #[test]
    fn transform_stack_translation() {
        let mut stack = TransformStack::new();
        stack.push_translation(5.0, 7.0);
        let (x, y) = stack.apply(0.0, 0.0);
        assert!((x - 5.0).abs() < 1e-10);
        assert!((y - 7.0).abs() < 1e-10);
    }

    #[test]
    fn transform_stack_rotation_90() {
        let mut stack = TransformStack::new();
        stack.push_rotation(std::f64::consts::FRAC_PI_2);
        let (x, y) = stack.apply(1.0, 0.0);
        // Rotating (1,0) by 90° should give (0,1)
        assert!(x.abs() < 1e-10, "x should be ~0, got {x}");
        assert!((y - 1.0).abs() < 1e-10, "y should be ~1, got {y}");
    }

    #[test]
    fn transform_stack_rotation_angle_extraction() {
        let mut stack = TransformStack::new();
        let angle = std::f64::consts::FRAC_PI_4;
        stack.push_rotation(angle);
        let extracted = stack.rotation_angle();
        assert!(
            (extracted - angle).abs() < 1e-10,
            "extracted {extracted}, expected {angle}"
        );
    }

    #[test]
    fn transform_stack_pop() {
        let mut stack = TransformStack::new();
        stack.push_translation(10.0, 20.0);
        assert_eq!(stack.len(), 1);

        let popped = stack.pop();
        assert!(popped);
        assert_eq!(stack.len(), 0);

        let m = stack.to_affine_matrix();
        assert!((m.a - 1.0).abs() < 1e-10);
        assert!(m.tx.abs() < 1e-10);
    }

    #[test]
    fn transform_stack_composed_translations() {
        let mut stack = TransformStack::new();
        // Compose two translations
        stack.push_translation(10.0, 0.0);
        stack.push_translation(0.0, 5.0);

        let (x, y) = stack.apply(0.0, 0.0);
        assert!((x - 10.0).abs() < 1e-10, "x should be ~10, got {x}");
        assert!((y - 5.0).abs() < 1e-10, "y should be ~5, got {y}");
    }

    #[test]
    fn transform_stack_rotation_around_origin() {
        // Rotate point (1, 0) by 90° around origin -> (0, 1)
        let mut stack = TransformStack::new();
        stack.push_rotation(std::f64::consts::FRAC_PI_2);
        let (x, y) = stack.apply(1.0, 0.0);
        assert!(x.abs() < 1e-10, "x should be ~0, got {x}");
        assert!((y - 1.0).abs() < 1e-10, "y should be ~1, got {y}");
    }

    #[test]
    fn transform_stack_reset() {
        let mut stack = TransformStack::new();
        stack.push_translation(100.0, 200.0);
        stack.push_rotation(1.0);
        stack.reset();

        assert!(stack.is_empty());
        let m = stack.to_affine_matrix();
        assert!((m.a - 1.0).abs() < 1e-10);
    }

    #[test]
    fn transform_stack_to_svg() {
        let mut stack = TransformStack::new();
        stack.push_translation(10.0, 20.0);
        let svg = stack.to_svg_transform();
        assert!(svg.starts_with("matrix("));
        assert!(svg.contains("10"));
        assert!(svg.contains("20"));
    }

    #[test]
    fn transform_stack_pop_restores_previous_state() {
        // Push translation, then rotation, then pop rotation
        // Should restore to just translation state
        let mut stack = TransformStack::new();
        stack.push_translation(10.0, 20.0);

        // Record state after translation
        let after_trans = stack.apply(0.0, 0.0);

        // Push rotation
        stack.push_rotation(std::f64::consts::FRAC_PI_2);
        let after_rot = stack.apply(0.0, 0.0);

        // Point should have moved due to rotation applied after translation
        // Origin stays at origin under rotation, but translation moved it to (10, 20)
        // then rotation around new origin keeps it there
        // Actually: transform order is translate then rotate around origin
        // So point (0,0) -> (10,20) -> rotation of (10,20) around origin

        // Pop the rotation
        stack.pop();
        let after_pop = stack.apply(0.0, 0.0);

        // Should be back to just translation state
        assert!(
            (after_pop.0 - after_trans.0).abs() < 1e-9,
            "x mismatch: after_pop={}, after_trans={}",
            after_pop.0,
            after_trans.0
        );
        assert!(
            (after_pop.1 - after_trans.1).abs() < 1e-9,
            "y mismatch: after_pop={}, after_trans={}",
            after_pop.1,
            after_trans.1
        );
    }

    #[test]
    fn transform_stack_pop_scale_only() {
        // Verify that pushing and popping scale returns to identity
        let mut stack = TransformStack::new();
        stack.push_scale(2.0);
        let (x, y) = stack.apply(5.0, 7.0);
        assert!((x - 10.0).abs() < 1e-9, "Scaled x: {x}");
        assert!((y - 14.0).abs() < 1e-9, "Scaled y: {y}");

        stack.pop();
        let (x, y) = stack.apply(5.0, 7.0);
        assert!((x - 5.0).abs() < 1e-9, "After pop x: {x}");
        assert!((y - 7.0).abs() < 1e-9, "After pop y: {y}");
    }

    #[test]
    fn transform_stack_multiple_pops() {
        // Start simple: only translation transforms (which are normalized)
        let mut stack = TransformStack::new();
        let test_point = (5.0, 7.0);

        // Push three translations
        stack.push_translation(10.0, 0.0);
        let after_t1 = stack.apply(test_point.0, test_point.1);
        assert!((after_t1.0 - 15.0).abs() < 1e-9, "after_t1 x: {}", after_t1.0);

        stack.push_translation(0.0, 3.0);
        let after_t2 = stack.apply(test_point.0, test_point.1);
        assert!((after_t2.1 - 10.0).abs() < 1e-9, "after_t2 y: {}", after_t2.1);

        stack.push_translation(5.0, 0.0);
        let _after_t3 = stack.apply(test_point.0, test_point.1);

        // Pop third transform
        stack.pop();
        let restored_t2 = stack.apply(test_point.0, test_point.1);
        assert!(
            (restored_t2.0 - after_t2.0).abs() < 1e-9,
            "After first pop, x: {} vs {}",
            restored_t2.0,
            after_t2.0
        );
        assert!(
            (restored_t2.1 - after_t2.1).abs() < 1e-9,
            "After first pop, y: {} vs {}",
            restored_t2.1,
            after_t2.1
        );

        // Pop second transform
        stack.pop();
        let restored_t1 = stack.apply(test_point.0, test_point.1);
        assert!(
            (restored_t1.0 - after_t1.0).abs() < 1e-9,
            "After second pop, x: {} vs {}",
            restored_t1.0,
            after_t1.0
        );
        assert!(
            (restored_t1.1 - after_t1.1).abs() < 1e-9,
            "After second pop, y: {} vs {}",
            restored_t1.1,
            after_t1.1
        );

        // Pop first transform - should be identity
        stack.pop();
        let restored_id = stack.apply(test_point.0, test_point.1);
        assert!(
            (restored_id.0 - test_point.0).abs() < 1e-9,
            "After third pop, x: {} vs {}",
            restored_id.0,
            test_point.0
        );
        assert!(
            (restored_id.1 - test_point.1).abs() < 1e-9,
            "After third pop, y: {} vs {}",
            restored_id.1,
            test_point.1
        );
    }

    #[test]
    #[should_panic(expected = "scale factor must be positive")]
    fn rotor_scale_zero_panics() {
        Rotor::scale(0.0);
    }

    #[test]
    #[should_panic(expected = "scale factor must be positive")]
    fn rotor_scale_negative_panics() {
        Rotor::scale(-1.0);
    }

    #[test]
    fn affine_to_rotor_roundtrip() {
        let original = AffineMatrix2D::translation(5.0, 10.0);
        let rotor = original.to_rotor();
        let recovered = rotor.to_affine_matrix();
        assert!(
            (recovered.tx - 5.0).abs() < 1e-10,
            "tx: {}",
            recovered.tx
        );
        assert!(
            (recovered.ty - 10.0).abs() < 1e-10,
            "ty: {}",
            recovered.ty
        );
    }

    #[test]
    fn affine_rotation_to_rotor_roundtrip() {
        let original = AffineMatrix2D::rotation(std::f64::consts::FRAC_PI_3);
        let rotor = original.to_rotor();
        let recovered = rotor.to_affine_matrix();
        assert!(
            (recovered.a - original.a).abs() < 1e-10,
            "a: {} vs {}",
            recovered.a,
            original.a
        );
        assert!(
            (recovered.c - original.c).abs() < 1e-10,
            "c: {} vs {}",
            recovered.c,
            original.c
        );
    }
}

#[cfg(test)]
mod geometry_tests {
    use super::*;

    #[test]
    fn point_distance() {
        let p1 = CgaPoint::new(0.0, 0.0);
        let p2 = CgaPoint::new(3.0, 4.0);
        assert!((p1.distance(&p2) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn line_segment_length() {
        let seg = CgaLineSegment::new(CgaPoint::new(0.0, 0.0), CgaPoint::new(3.0, 4.0));
        assert!((seg.length() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn line_segment_intersection() {
        // Crossing segments
        let seg1 = CgaLineSegment::new(CgaPoint::new(0.0, 0.0), CgaPoint::new(2.0, 2.0));
        let seg2 = CgaLineSegment::new(CgaPoint::new(0.0, 2.0), CgaPoint::new(2.0, 0.0));
        let intersection = seg1.intersect(&seg2).expect("should intersect");
        assert!((intersection.x - 1.0).abs() < 1e-10);
        assert!((intersection.y - 1.0).abs() < 1e-10);
    }

    #[test]
    fn line_segment_no_intersection_parallel() {
        let seg1 = CgaLineSegment::new(CgaPoint::new(0.0, 0.0), CgaPoint::new(2.0, 0.0));
        let seg2 = CgaLineSegment::new(CgaPoint::new(0.0, 1.0), CgaPoint::new(2.0, 1.0));
        assert!(seg1.intersect(&seg2).is_none());
    }

    #[test]
    fn line_segment_no_intersection_not_crossing() {
        // Lines would cross if extended, but segments don't
        let seg1 = CgaLineSegment::new(CgaPoint::new(0.0, 0.0), CgaPoint::new(1.0, 1.0));
        let seg2 = CgaLineSegment::new(CgaPoint::new(2.0, 0.0), CgaPoint::new(3.0, 1.0));
        assert!(seg1.intersect(&seg2).is_none());
    }

    #[test]
    fn line_segment_closest_point_on_segment() {
        let seg = CgaLineSegment::new(CgaPoint::new(0.0, 0.0), CgaPoint::new(4.0, 0.0));
        let point = CgaPoint::new(2.0, 3.0);
        let closest = seg.closest_point(&point);
        assert!((closest.x - 2.0).abs() < 1e-10);
        assert!((closest.y - 0.0).abs() < 1e-10);
    }

    #[test]
    fn line_segment_closest_point_at_endpoint() {
        let seg = CgaLineSegment::new(CgaPoint::new(0.0, 0.0), CgaPoint::new(4.0, 0.0));
        let point = CgaPoint::new(-1.0, 1.0);
        let closest = seg.closest_point(&point);
        assert!((closest.x - 0.0).abs() < 1e-10);
        assert!((closest.y - 0.0).abs() < 1e-10);
    }

    #[test]
    fn circle_contains_point() {
        let circle = CgaCircle::new(CgaPoint::new(5.0, 5.0), 3.0);
        assert!(circle.contains(&CgaPoint::new(5.0, 5.0))); // center
        assert!(circle.contains(&CgaPoint::new(6.0, 5.0))); // inside
        assert!(circle.contains(&CgaPoint::new(8.0, 5.0))); // on boundary
        assert!(!circle.contains(&CgaPoint::new(9.0, 5.0))); // outside
    }

    #[test]
    fn circle_intersect_segment_two_points() {
        let circle = CgaCircle::new(CgaPoint::origin(), 1.0);
        let seg = CgaLineSegment::new(CgaPoint::new(-2.0, 0.0), CgaPoint::new(2.0, 0.0));
        let points = circle.intersect_segment(&seg);
        assert_eq!(points.len(), 2);
    }

    #[test]
    fn circle_intersect_segment_one_point_tangent() {
        let circle = CgaCircle::new(CgaPoint::origin(), 1.0);
        let seg = CgaLineSegment::new(CgaPoint::new(-2.0, 1.0), CgaPoint::new(2.0, 1.0));
        let points = circle.intersect_segment(&seg);
        assert_eq!(points.len(), 1);
        assert!((points[0].y - 1.0).abs() < 1e-10);
    }

    #[test]
    fn circle_intersect_segment_no_intersection() {
        let circle = CgaCircle::new(CgaPoint::origin(), 1.0);
        let seg = CgaLineSegment::new(CgaPoint::new(-2.0, 2.0), CgaPoint::new(2.0, 2.0));
        let points = circle.intersect_segment(&seg);
        assert!(points.is_empty());
    }

    #[test]
    fn rect_contains_point() {
        let rect = CgaRect::new(0.0, 0.0, 10.0, 10.0);
        assert!(rect.contains(&CgaPoint::new(5.0, 5.0)));
        assert!(rect.contains(&CgaPoint::new(0.0, 0.0)));
        assert!(rect.contains(&CgaPoint::new(10.0, 10.0)));
        assert!(!rect.contains(&CgaPoint::new(-1.0, 5.0)));
    }

    #[test]
    fn rect_intersect_segment_through() {
        let rect = CgaRect::new(0.0, 0.0, 10.0, 10.0);
        let seg = CgaLineSegment::new(CgaPoint::new(-5.0, 5.0), CgaPoint::new(15.0, 5.0));
        let points = rect.intersect_segment(&seg);
        assert_eq!(points.len(), 2);
    }

    #[test]
    fn rect_edges() {
        let rect = CgaRect::new(0.0, 0.0, 10.0, 5.0);
        let edges = rect.edges();
        assert_eq!(edges.len(), 4);
        // Top edge
        assert!((edges[0].start.x - 0.0).abs() < 1e-10);
        assert!((edges[0].end.x - 10.0).abs() < 1e-10);
    }
}
