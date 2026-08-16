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

/// Component-to-basis masks in the public grade-major component ordering.
///
/// Bit 0 is `e1`, bit 1 is `e2`, bit 2 is `e+`, and bit 3 is `e-`.
const BLADE_MASKS: [u8; 16] = [0, 1, 2, 4, 8, 3, 5, 9, 6, 10, 12, 7, 11, 13, 14, 15];

/// Basis-mask-to-component lookup for [`BLADE_MASKS`].
const BLADE_INDEX_BY_MASK: [usize; 16] = [0, 1, 2, 5, 3, 6, 8, 11, 4, 7, 9, 12, 10, 13, 14, 15];

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
        let mut r = [0.0_f64; 16];

        for (lhs_index, &lhs) in self.components.iter().enumerate() {
            if lhs == 0.0 {
                continue;
            }
            let lhs_mask = BLADE_MASKS[lhs_index];
            for (rhs_index, &rhs) in other.components.iter().enumerate() {
                if rhs == 0.0 {
                    continue;
                }
                let rhs_mask = BLADE_MASKS[rhs_index];
                let mut sign = 1.0;
                let mut result_mask = lhs_mask;

                for basis in 0..4 {
                    let basis_bit = 1_u8 << basis;
                    if rhs_mask & basis_bit == 0 {
                        continue;
                    }

                    let lower_or_equal = (basis_bit << 1) - 1;
                    if (result_mask & !lower_or_equal).count_ones() % 2 == 1 {
                        sign = -sign;
                    }
                    if basis == 3 && result_mask & basis_bit != 0 {
                        sign = -sign;
                    }
                    result_mask ^= basis_bit;
                }

                let result_index = BLADE_INDEX_BY_MASK[usize::from(result_mask)];
                r[result_index] += sign * lhs * rhs;
            }
        }

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

/// Recover a positive uniform scale from the `e+∧e-` rotor component.
///
/// A dilation rotor stores `sinh(ln(scale) / 2)`, so extracting the scale
/// requires the inverse hyperbolic sine before exponentiating. Keeping this
/// conversion in one place prevents transform inspection from disagreeing
/// with affine conversion.
#[must_use]
fn dilation_scale(epm: f64) -> f64 {
    let half_log_scale = epm.asinh();
    let scale = (2.0 * half_log_scale).exp();
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
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
        assert!(factor > 0.0, "scale factor must be positive, got {factor}");
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

    fn to_multivector(self) -> Multivector {
        let mut components = [0.0; 16];
        components[blade::SCALAR] = self.components[0];
        components[blade::E12] = self.components[1];
        components[blade::E1P] = self.components[2];
        components[blade::E1M] = self.components[3];
        components[blade::E2P] = self.components[4];
        components[blade::E2M] = self.components[5];
        components[blade::EPM] = self.components[6];
        components[blade::E12PM] = self.components[7];
        Multivector { components }
    }

    fn from_multivector(multivector: Multivector) -> Self {
        let components = multivector.components;
        Self {
            components: [
                components[blade::SCALAR],
                components[blade::E12],
                components[blade::E1P],
                components[blade::E1M],
                components[blade::E2P],
                components[blade::E2M],
                components[blade::EPM],
                components[blade::E12PM],
            ],
        }
    }

    /// Compose two rotors: self * other (geometric product of even subalgebra).
    #[must_use]
    pub fn compose(self, other: Self) -> Self {
        Self::from_multivector(
            self.to_multivector()
                .geometric_product(other.to_multivector()),
        )
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
    /// Returns `None` for singular or non-finite rotors. For normalized rotors
    /// (|R|² = 1), the inverse equals the reverse.
    #[must_use]
    pub fn inverse(self) -> Option<Self> {
        let norm_sq = self.norm_squared();
        if !norm_sq.is_finite() || norm_sq.abs() < f64::EPSILON {
            return None;
        }
        if (norm_sq - 1.0).abs() < 1e-12 {
            // Already normalized, reverse is the inverse
            Some(self.reverse())
        } else {
            // Scale each component of the reverse by 1/norm_squared
            let rev = self.reverse();
            let inv_norm = 1.0 / norm_sq;
            Some(Self {
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
            })
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
        let e12pm = self.components[7];

        // Decompose the composite rotor M = T·S·R. Writing L = ln(scale)/2 and h = θ/2, the
        // geometric product distributes the dilation across BOTH the scalar/e12 pair and the
        // e+-/e12+- pair:
        //
        //     s     = cos(h)·cosh(L)      e12   = sin(h)·cosh(L)
        //     epm   = cos(h)·sinh(L)      e12pm = sin(h)·sinh(L)
        //
        // The previous code read `epm` as sinh(L) directly and normalized the half-angle by
        // sqrt(1 + epm²). Both are only true when the other factor is absent, i.e. for a rotor
        // carrying a dilation OR a rotation but never both — which is precisely the case the unit
        // tests covered. Recover each factor from its own invariant instead.
        let cosh_l = s.hypot(e12);
        let (cos_half, sin_half) = if cosh_l > 1e-12 {
            (s / cosh_l, e12 / cosh_l)
        } else {
            (s, e12)
        };
        // Projecting (epm, e12pm) onto the half-angle direction yields sinh(L) with its sign,
        // since cos(h)² + sin(h)² = 1.
        let sinh_l = epm.mul_add(cos_half, e12pm * sin_half);
        // e^L = cosh(L) + sinh(L), and scale = e^(2L) = (cosh(L) + sinh(L))².
        let half_dilation = cosh_l + sinh_l;
        let scale_factor = if half_dilation.is_finite() && half_dilation > 1e-12 {
            half_dilation * half_dilation
        } else {
            dilation_scale(sinh_l)
        };

        // Recover full rotation angle using double angle formulas
        let cos_theta = cos_half * cos_half - sin_half * sin_half;
        let sin_theta = 2.0 * cos_half * sin_half;

        // Translation from e1+/e1- and e2+/e2- components.
        //
        // These are NOT the translation directly once a rotation or dilation is also present.
        // `Rotor::translation` stores t/2 on each of e1e±/e2e±, but composing it with a rotation
        // (`compose` = geometric product) mixes those components: (e1e±)·e12 yields an e2e± term
        // and vice versa. For the canonical composite M = T·S·R the raw sums come out as
        //
        //     raw = sqrt(scale) · Rot(θ/2) · t
        //
        // — the stored translation rotated by HALF the rotation angle and dilated by the half-
        // dilation. Reading `e1p + e1m` back as `tx` was therefore only correct for a pure
        // translation, which is the only case the unit tests covered. Undo both to recover t.
        let raw_tx = e1p + e1m;
        let raw_ty = e2p + e2m;
        let (raw_tx, raw_ty) = if half_dilation.is_finite() && half_dilation > 1e-12 {
            (raw_tx / half_dilation, raw_ty / half_dilation)
        } else {
            (raw_tx, raw_ty)
        };
        // Inverse of a Rot(θ/2), i.e. Rot(-θ/2), using the already-normalized half-angle terms.
        let tx = raw_tx.mul_add(cos_half, raw_ty * sin_half);
        let ty = raw_ty.mul_add(cos_half, -(raw_tx * sin_half));

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
    fn translation_survives_composition_with_rotation_and_scale() {
        // Regression for the composite case: `translation_rotor_produces_correct_matrix`,
        // `rotation_rotor_90_degrees` and `scale_rotor_produces_correct_matrix` each exercise one
        // primitive in isolation, so all three passed while T·R and T·S·R silently rotated and
        // dilated the translation. Composing is exactly where the extraction used to break.
        for &(dx, dy) in &[(5.0_f64, 10.0_f64), (-3.0, 7.5), (0.0, -4.0)] {
            for &angle in &[
                0.0_f64,
                std::f64::consts::FRAC_PI_2,
                -std::f64::consts::FRAC_PI_3,
                2.5,
            ] {
                for &scale in &[1.0_f64, 2.0, 0.25] {
                    let composed = Rotor::translation(dx, dy)
                        .compose(Rotor::scale(scale).compose(Rotor::rotation(angle)));
                    let m = composed.to_affine_matrix();

                    assert!(
                        (m.tx - dx).abs() < 1e-9 && (m.ty - dy).abs() < 1e-9,
                        "translation ({dx}, {dy}) must survive rotation {angle} and scale {scale}, got ({}, {})",
                        m.tx,
                        m.ty
                    );
                    // The rotation/scale block must stay correct too, so the fix cannot be a
                    // translation-only patch that corrupts the linear part.
                    assert!(
                        (m.a - angle.cos() * scale).abs() < 1e-9
                            && (m.b + angle.sin() * scale).abs() < 1e-9
                            && (m.c - angle.sin() * scale).abs() < 1e-9
                            && (m.d - angle.cos() * scale).abs() < 1e-9,
                        "linear block wrong for angle {angle} scale {scale}"
                    );
                }
            }
        }
    }

    #[test]
    fn affine_matrix_rotor_round_trip_preserves_similarity_transforms() {
        // `AffineMatrix2D -> to_rotor -> to_affine_matrix` must be the identity on similarity
        // transforms, and must agree with the matrix's own `apply` on a probe point.
        for &angle in &[0.0_f64, std::f64::consts::FRAC_PI_2, 2.0, -1.25] {
            for &scale in &[1.0_f64, 3.0, 0.5] {
                for &(tx, ty) in &[(0.0_f64, 0.0_f64), (5.0, 10.0), (-2.5, 4.0)] {
                    let original = AffineMatrix2D {
                        a: angle.cos() * scale,
                        b: -angle.sin() * scale,
                        tx,
                        c: angle.sin() * scale,
                        d: angle.cos() * scale,
                        ty,
                    };
                    let round_tripped = original.to_rotor().to_affine_matrix();

                    for &(px, py) in &[(1.0_f64, 0.0_f64), (0.0, 1.0), (-3.0, 2.0)] {
                        let (ex, ey) = original.apply(px, py);
                        let (gx, gy) = round_tripped.apply(px, py);
                        assert!(
                            (gx - ex).abs() < 1e-9 && (gy - ey).abs() < 1e-9,
                            "round trip changed ({px}, {py}): expected ({ex}, {ey}), got ({gx}, {gy}) \
                             for angle {angle} scale {scale} translation ({tx}, {ty})"
                        );
                    }
                }
            }
        }
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
    fn rotor_compose_preserves_full_even_subalgebra_terms() {
        let left = Rotor {
            components: [0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        };
        let right = Rotor {
            components: [0.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0],
        };

        let composed = left.compose(right);
        assert_eq!(
            composed.components[7], -6.0,
            "e1+ * e2- should produce -6 e12+-"
        );
        assert_eq!(
            composed.to_multivector(),
            left.to_multivector()
                .geometric_product(right.to_multivector())
        );
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
    /// Composed state immediately before each pushed rotor.
    previous_composed: Vec<Rotor>,
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
            previous_composed: Vec::new(),
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
        self.previous_composed.push(self.composed);
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
        if self.stack.pop().is_some() {
            self.composed = self.previous_composed.pop().unwrap_or_else(Rotor::identity);
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
    ///
    /// Derived from `to_affine_matrix` rather than read straight off the e1e±/e2e± components:
    /// once the stack also carries a rotation or dilation those raw sums are the translation
    /// rotated by half the angle and dilated by the half-dilation, not the translation itself.
    /// Going through the decomposition also guarantees this can never disagree with `apply`.
    #[must_use]
    pub fn translation(&self) -> (f64, f64) {
        let matrix = self.composed.to_affine_matrix();
        (matrix.tx, matrix.ty)
    }

    /// Get the scale factor of the composed transform.
    ///
    /// The e+- component alone is `cos(θ/2)·sinh(ln(scale)/2)`, so reading it as `sinh` understates
    /// the dilation of any stack that also rotates. Recover it from the linear block instead:
    /// `a = cos(θ)·scale` and `c = sin(θ)·scale`, so `hypot(a, c) = scale`.
    #[must_use]
    pub fn scale_factor(&self) -> f64 {
        let matrix = self.composed.to_affine_matrix();
        matrix.a.hypot(matrix.c)
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
        self.previous_composed.clear();
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

    fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Embed this Euclidean point as a null vector in the conformal model.
    ///
    /// With `e∞ = e+ + e-` and `eo = (e- - e+)/2`, the embedding is
    /// `x e1 + y e2 + (x² + y²) e∞ / 2 + eo`.
    #[must_use]
    pub fn to_multivector(self) -> Multivector {
        let radius_squared = self.x.mul_add(self.x, self.y * self.y);
        let mut components = [0.0; 16];
        components[blade::E1] = self.x;
        components[blade::E2] = self.y;
        components[blade::EP] = (radius_squared - 1.0) / 2.0;
        components[blade::EM] = (radius_squared + 1.0) / 2.0;
        Multivector { components }
    }

    /// Conformal inner product with another embedded point.
    ///
    /// For finite Euclidean points this is `-distance_squared / 2`.
    #[must_use]
    pub fn inner_product(&self, other: &Self) -> f64 {
        self.to_multivector()
            .geometric_product(other.to_multivector())
            .scalar_part()
    }

    /// Squared distance to another point.
    ///
    /// The conformal identity d²(P, Q) = -2 P·Q holds exactly in real arithmetic, and
    /// [`Self::inner_product`] still evaluates it, but it is NOT how this is computed. The
    /// embedding squares each coordinate before subtracting, which is catastrophic
    /// cancellation whenever the separation is small relative to the coordinates: for points
    /// around 90 apart from the origin, a true distance of 1e-9 came back as exactly 0.0, and
    /// a point known to lie on a segment measured 6.1e-5 off it. Squaring also overflows near
    /// 1.3e154, far below the range of the distance itself, and the `inf - inf` that follows is
    /// NaN rather than an overflow.
    ///
    /// So the separation is taken directly, which is both better conditioned and cheaper. If
    /// that still overflows, it is recomputed in an exactly-scaled frame; the result may be
    /// `+inf` when d² genuinely exceeds `f64`, but it is never NaN for finite input.
    #[must_use]
    pub fn distance_squared(&self, other: &CgaPoint) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        let squared_distance = dx * dx + dy * dy;
        if squared_distance.is_finite() {
            return squared_distance;
        }

        let (separation, scale) = self.scaled_separation(*other);
        separation * scale * scale
    }

    /// Distance to another point.
    ///
    /// Finite for every pair of finite points whose separation is representable — including
    /// pairs whose SQUARED separation is not, which is why this does not simply take the square
    /// root of [`Self::distance_squared`].
    #[must_use]
    pub fn distance(&self, other: &CgaPoint) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        let squared_distance = dx * dx + dy * dy;
        if squared_distance.is_finite() {
            return squared_distance.sqrt();
        }

        let (separation, scale) = self.scaled_separation(*other);
        separation.sqrt() * scale
    }

    /// Squared separation measured in a power-of-two scaled frame, with the scale factor.
    ///
    /// Dividing by a power of two only shifts exponents, so the scaled frame introduces no
    /// rounding of its own.
    fn scaled_separation(self, other: Self) -> (f64, f64) {
        let scale = power_of_two_scale(
            [self.x, self.y, other.x, other.y]
                .into_iter()
                .fold(0.0_f64, |acc, value| acc.max(value.abs())),
        );
        let dx = other.x / scale - self.x / scale;
        let dy = other.y / scale - self.y / scale;
        (dx * dx + dy * dy, scale)
    }
}

/// Largest power of two not exceeding `magnitude`, or `1.0` when there is nothing to scale.
///
/// Used to bring an overflowing configuration back into range. Multiplying or dividing by a
/// power of two only shifts exponents, so a scaled retry adds no rounding of its own.
fn power_of_two_scale(magnitude: f64) -> f64 {
    let biased_exponent = (magnitude.to_bits() >> 52) & 0x7ff;
    if biased_exponent == 0 {
        // Zero or subnormal: already as small as it gets, and nothing here can overflow.
        return 1.0;
    }
    f64::from_bits(biased_exponent << 52)
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
        if !self.start.is_finite() || !self.end.is_finite() {
            return (0.0, 0.0);
        }
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
    /// Returns a deterministic intersection point when the segments meet.
    ///
    /// For collinear overlaps, this returns the first overlapping point while
    /// traversing `self`; this gives routing and hit testing a stable contact
    /// point even though the geometric intersection is not unique.
    /// Uses parametric line-line intersection.
    #[must_use]
    pub fn intersect(&self, other: &CgaLineSegment) -> Option<CgaPoint> {
        if !self.start.is_finite()
            || !self.end.is_finite()
            || !other.start.is_finite()
            || !other.end.is_finite()
        {
            return None;
        }

        // Parametric form: P = start + t*(end - start)
        // Solve for t1, t2 where the lines cross
        let d1x = self.end.x - self.start.x;
        let d1y = self.end.y - self.start.y;
        let d2x = other.end.x - other.start.x;
        let d2y = other.end.y - other.start.y;

        let cross = d1x * d2y - d1y * d2x;
        if cross.abs() < f64::EPSILON {
            let offset_cross =
                (other.start.x - self.start.x) * d1y - (other.start.y - self.start.y) * d1x;
            if offset_cross.abs() >= f64::EPSILON {
                return None;
            }

            let self_length_squared = d1x * d1x + d1y * d1y;
            let other_length_squared = d2x * d2x + d2y * d2y;
            if self_length_squared < f64::EPSILON {
                if other_length_squared < f64::EPSILON {
                    return (self.start == other.start).then_some(self.start);
                }

                let other_t = ((self.start.x - other.start.x) * d2x
                    + (self.start.y - other.start.y) * d2y)
                    / other_length_squared;
                return (0.0..=1.0).contains(&other_t).then_some(self.start);
            }

            let first_other_t = ((other.start.x - self.start.x) * d1x
                + (other.start.y - self.start.y) * d1y)
                / self_length_squared;
            let second_other_t = ((other.end.x - self.start.x) * d1x
                + (other.end.y - self.start.y) * d1y)
                / self_length_squared;
            let overlap_start = first_other_t.min(second_other_t).max(0.0);
            let overlap_end = first_other_t.max(second_other_t).min(1.0);

            return (overlap_start <= overlap_end).then(|| {
                CgaPoint::new(
                    self.start.x + overlap_start * d1x,
                    self.start.y + overlap_start * d1y,
                )
            });
        }

        let ox = other.start.x - self.start.x;
        let oy = other.start.y - self.start.y;

        let t1 = (ox * d2y - oy * d2x) / cross;
        let t2 = (ox * d1y - oy * d1x) / cross;

        // Check if intersection is within both segments [0, 1]
        if (0.0..=1.0).contains(&t1) && (0.0..=1.0).contains(&t2) {
            Some(CgaPoint::new(
                self.start.x + t1 * d1x,
                self.start.y + t1 * d1y,
            ))
        } else {
            None
        }
    }

    /// Project `point` onto the segment `start`..`end`.
    ///
    /// Returns `None` when the arithmetic leaves the representable range, which happens for a
    /// segment whose squared length overflows, and also for a finite segment queried from the
    /// opposite end of the range: `point.x - start.x` can overflow to infinity while `dx` is
    /// exactly `0.0`, and `inf * 0.0` is NaN.
    fn project(start: CgaPoint, end: CgaPoint, point: CgaPoint) -> Option<CgaPoint> {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let len_sq = dx * dx + dy * dy;

        if !len_sq.is_finite() {
            return None;
        }

        if len_sq < f64::EPSILON {
            return Some(start);
        }

        let t = ((point.x - start.x) * dx + (point.y - start.y) * dy) / len_sq;
        let t = t.clamp(0.0, 1.0);

        let projected = CgaPoint::new(start.x + t * dx, start.y + t * dy);
        projected.is_finite().then_some(projected)
    }

    /// Largest power of two not exceeding the biggest coordinate magnitude involved.
    fn retry_scale(points: [CgaPoint; 3]) -> f64 {
        power_of_two_scale(
            points
                .iter()
                .flat_map(|p| [p.x.abs(), p.y.abs()])
                .fold(0.0_f64, f64::max),
        )
    }

    /// Find closest point on the segment to a given point.
    ///
    /// Every finite input yields a finite point within the segment's bounding box. When the
    /// direct computation overflows, the projection is retried in an exactly-scaled frame, so
    /// long segments and far-apart queries get a real answer instead of a sentinel. Precision
    /// degrades gracefully: if the segment is so much shorter than the coordinates that its
    /// scaled length underflows, the result is `start`, matching the degenerate-segment case.
    /// Only non-finite input still fails closed to the origin.
    #[must_use]
    pub fn closest_point(&self, point: &CgaPoint) -> CgaPoint {
        if !self.start.is_finite() || !self.end.is_finite() || !point.is_finite() {
            return CgaPoint::origin();
        }

        if let Some(projected) = Self::project(self.start, self.end, *point) {
            return projected;
        }

        let scale = Self::retry_scale([self.start, self.end, *point]);
        let start = CgaPoint::new(self.start.x / scale, self.start.y / scale);
        let end = CgaPoint::new(self.end.x / scale, self.end.y / scale);
        let query = CgaPoint::new(point.x / scale, point.y / scale);

        Self::project(start, end, query).map_or_else(CgaPoint::origin, |projected| {
            CgaPoint::new(projected.x * scale, projected.y * scale)
        })
    }

    /// Distance from a point to this line segment.
    ///
    /// May be infinite when the two are genuinely further apart than `f64` can express, but
    /// no longer reports infinity merely because the segment is long.
    #[must_use]
    pub fn distance_to_point(&self, point: &CgaPoint) -> f64 {
        if !self.start.is_finite() || !self.end.is_finite() || !point.is_finite() {
            return f64::INFINITY;
        }

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

    fn is_valid(self) -> bool {
        self.center.is_finite() && self.radius.is_finite() && self.radius >= 0.0
    }

    /// Check if a point is inside the circle.
    #[must_use]
    pub fn contains(&self, point: &CgaPoint) -> bool {
        self.is_valid()
            && point.is_finite()
            && point.distance_squared(&self.center) <= self.radius * self.radius
    }

    /// Check if a point is strictly inside the circle (not on boundary).
    #[must_use]
    pub fn contains_strict(&self, point: &CgaPoint) -> bool {
        self.is_valid()
            && point.is_finite()
            && point.distance_squared(&self.center) < self.radius * self.radius
    }

    /// Find intersection points with a line segment.
    ///
    /// Returns 0, 1, or 2 intersection points.
    #[must_use]
    pub fn intersect_segment(&self, segment: &CgaLineSegment) -> Vec<CgaPoint> {
        if !self.is_valid() || !segment.start.is_finite() || !segment.end.is_finite() {
            return Vec::new();
        }

        // Line parametric: P = start + t * (end - start)
        // Circle: |P - center|² = r²
        // Substitute and solve quadratic
        let dx = segment.end.x - segment.start.x;
        let dy = segment.end.y - segment.start.y;
        let fx = segment.start.x - self.center.x;
        let fy = segment.start.y - self.center.y;

        let a = dx * dx + dy * dy;
        if a < f64::EPSILON {
            // A degenerate segment intersects the circle only when its lone point
            // lies on the boundary; an interior point has no boundary crossing.
            let radius_squared = self.radius * self.radius;
            let distance_squared = segment.start.distance_squared(&self.center);
            let tolerance = f64::EPSILON * radius_squared.max(1.0) * 8.0;
            if (distance_squared - radius_squared).abs() <= tolerance {
                return vec![segment.start];
            }
            return Vec::new();
        }

        let b = 2.0 * (fx * dx + fy * dy);
        let c = fx * fx + fy * fy - self.radius * self.radius;

        let discriminant = b * b - 4.0 * a * c;
        let discriminant_tolerance = f64::EPSILON * (b * b + (4.0 * a * c).abs()).max(1.0) * 8.0;
        if discriminant < -discriminant_tolerance {
            return Vec::new();
        }

        let mut points = Vec::new();
        let sqrt_disc = discriminant.max(0.0).sqrt();
        let t1 = (-b - sqrt_disc) / (2.0 * a);
        let t2 = (-b + sqrt_disc) / (2.0 * a);

        // For tangent case (discriminant ≈ 0), t1 ≈ t2, only add once
        if (0.0..=1.0).contains(&t1) {
            points.push(CgaPoint::new(
                segment.start.x + t1 * dx,
                segment.start.y + t1 * dy,
            ));
        }

        // Only add second point if it's distinct from first
        if (t2 - t1).abs() > 1e-10 && (0.0..=1.0).contains(&t2) {
            points.push(CgaPoint::new(
                segment.start.x + t2 * dx,
                segment.start.y + t2 * dy,
            ));
        }

        points
    }

    /// Find intersection points with another circle.
    ///
    /// Returns zero points for separate, nested, coincident, or invalid
    /// circles; one point for tangency; and two points for a proper crossing.
    /// The returned order is stable for a fixed pair of input circles.
    #[must_use]
    pub fn intersect_circle(&self, other: &Self) -> Vec<CgaPoint> {
        if !self.is_valid() || !other.is_valid() {
            return Vec::new();
        }

        let dx = other.center.x - self.center.x;
        let dy = other.center.y - self.center.y;
        let center_distance = dx.hypot(dy);
        if center_distance == 0.0
            || center_distance > self.radius + other.radius
            || center_distance < (self.radius - other.radius).abs()
        {
            return Vec::new();
        }

        let self_radius_squared = self.radius * self.radius;
        let other_radius_squared = other.radius * other.radius;
        let distance_squared = center_distance * center_distance;
        let along_centers = (self_radius_squared - other_radius_squared + distance_squared)
            / (2.0 * center_distance);
        let height_squared = self_radius_squared - along_centers * along_centers;
        let tolerance = f64::EPSILON
            * (self_radius_squared + along_centers * along_centers + distance_squared).max(1.0)
            * 8.0;
        if height_squared < -tolerance {
            return Vec::new();
        }
        let height = height_squared.max(0.0).sqrt();

        let midpoint = CgaPoint::new(
            self.center.x + along_centers * dx / center_distance,
            self.center.y + along_centers * dy / center_distance,
        );
        if height == 0.0 {
            return vec![midpoint];
        }

        let offset_x = -dy * height / center_distance;
        let offset_y = dx * height / center_distance;
        vec![
            CgaPoint::new(midpoint.x + offset_x, midpoint.y + offset_y),
            CgaPoint::new(midpoint.x - offset_x, midpoint.y - offset_y),
        ]
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
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= 0.0
            && self.height >= 0.0
            && (self.x + self.width).is_finite()
            && (self.y + self.height).is_finite()
    }

    /// Check if a point is inside the rectangle.
    #[must_use]
    pub fn contains(&self, point: &CgaPoint) -> bool {
        self.is_valid()
            && point.is_finite()
            && point.x >= self.x
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
        if !self.is_valid() || !segment.start.is_finite() || !segment.end.is_finite() {
            return Vec::new();
        }

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
        if !self.is_valid() || !point.is_finite() {
            return CgaPoint::origin();
        }

        let mut closest = self.edges()[0].closest_point(point);
        let mut min_dist_squared = point.distance_squared(&closest);
        let mut min_dist = min_dist_squared.sqrt();

        for edge in self.edges().iter().skip(1) {
            let p = edge.closest_point(point);
            let dist_squared = point.distance_squared(&p);
            if dist_squared < min_dist_squared {
                let dist = dist_squared.sqrt();
                if dist < min_dist {
                    min_dist_squared = dist_squared;
                    min_dist = dist;
                    closest = p;
                }
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
    fn transform_stack_scale_factor_recovers_dilation() {
        let mut stack = TransformStack::new();
        stack.push_scale(2.0);

        assert!(
            (stack.scale_factor() - 2.0).abs() < 1e-10,
            "scale factor should recover the dilation, got {}",
            stack.scale_factor()
        );
    }

    #[test]
    fn transform_stack_scale_factor_composes_dilations() {
        let mut stack = TransformStack::new();
        stack.push_scale(2.0);
        stack.push_scale(0.5);

        assert!(
            (stack.scale_factor() - 1.0).abs() < 1e-10,
            "inverse dilations should compose to identity, got {}",
            stack.scale_factor()
        );
    }

    #[test]
    fn transform_stack_accessors_survive_mixed_rotation_scale_translation() {
        // `transform_stack_translation`, `transform_stack_rotation_angle_extraction` and
        // `transform_stack_scale_factor_recovers_dilation` each push ONE kind of transform, so the
        // accessors reading raw rotor components passed all three while a mixed stack reported a
        // rotated/dilated translation and an understated scale.
        let mut stack = TransformStack::new();
        stack.push_translation(5.0, 10.0);
        stack.push_scale(2.0);
        stack.push_rotation(std::f64::consts::FRAC_PI_2);

        let (tx, ty) = stack.translation();
        assert!(
            (tx - 5.0).abs() < 1e-9 && (ty - 10.0).abs() < 1e-9,
            "translation must stay (5, 10) under rotation and scale, got ({tx}, {ty})"
        );
        assert!(
            (stack.scale_factor() - 2.0).abs() < 1e-9,
            "scale must stay 2.0 under rotation, got {}",
            stack.scale_factor()
        );
        assert!(
            (stack.rotation_angle() - std::f64::consts::FRAC_PI_2).abs() < 1e-9,
            "rotation must stay 90deg under scale, got {}",
            stack.rotation_angle()
        );
        // The accessors must agree with what the transform actually does to a point.
        let (px, py) = stack.apply(1.0, 0.0);
        assert!(
            (px - 5.0).abs() < 1e-9 && (py - 12.0).abs() < 1e-9,
            "scale 2 then rotate 90 then translate (5,10) sends (1,0) to (5,12), got ({px}, {py})"
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
        let _after_rot = stack.apply(0.0, 0.0);

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
    fn transform_stack_pop_restores_exact_state_for_raw_rotors() {
        let raw_rotor = Rotor {
            components: [1.0, 0.0, 2.0, 0.0, 0.0, 3.0, 0.0, 0.0],
        };
        let mut stack = TransformStack::new();
        let before_push = stack.rotor();

        stack.push_rotor(raw_rotor);
        assert!(stack.pop());
        assert_eq!(stack.rotor().components, before_push.components);
        assert!(stack.is_empty());
    }

    #[test]
    fn transform_stack_multiple_pops() {
        // Start simple: only translation transforms (which are normalized)
        let mut stack = TransformStack::new();
        let test_point = (5.0, 7.0);

        // Push three translations
        stack.push_translation(10.0, 0.0);
        let after_t1 = stack.apply(test_point.0, test_point.1);
        assert!(
            (after_t1.0 - 15.0).abs() < 1e-9,
            "after_t1 x: {}",
            after_t1.0
        );

        stack.push_translation(0.0, 3.0);
        let after_t2 = stack.apply(test_point.0, test_point.1);
        assert!(
            (after_t2.1 - 10.0).abs() < 1e-9,
            "after_t2 y: {}",
            after_t2.1
        );

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
        let _ = Rotor::scale(0.0);
    }

    #[test]
    #[should_panic(expected = "scale factor must be positive")]
    fn rotor_scale_negative_panics() {
        let _ = Rotor::scale(-1.0);
    }

    #[test]
    fn rotor_inverse_rejects_singular_and_non_finite_rotors() {
        assert!(
            Rotor {
                components: [0.0; 8]
            }
            .inverse()
            .is_none()
        );
        assert!(
            Rotor {
                components: [f64::NAN, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
            }
            .inverse()
            .is_none()
        );
        assert_eq!(Rotor::identity().inverse(), Some(Rotor::identity()));
    }

    #[test]
    fn affine_to_rotor_roundtrip() {
        let original = AffineMatrix2D::translation(5.0, 10.0);
        let rotor = original.to_rotor();
        let recovered = rotor.to_affine_matrix();
        assert!((recovered.tx - 5.0).abs() < 1e-10, "tx: {}", recovered.tx);
        assert!((recovered.ty - 10.0).abs() < 1e-10, "ty: {}", recovered.ty);
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
    fn conformal_point_embedding_is_null_and_encodes_distance() {
        let origin = CgaPoint::origin();
        let point = CgaPoint::new(3.0, 4.0);

        assert!(point.to_multivector().norm_squared().abs() < 1e-12);
        assert!((origin.inner_product(&point) + 12.5).abs() < 1e-12);
        assert!((origin.distance_squared(&point) - 25.0).abs() < 1e-12);
    }

    #[test]
    fn line_segment_length() {
        let seg = CgaLineSegment::new(CgaPoint::new(0.0, 0.0), CgaPoint::new(3.0, 4.0));
        assert!((seg.length() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn line_segment_direction_rejects_non_finite_endpoints() {
        let segment = CgaLineSegment::new(
            CgaPoint::new(f64::NAN, 0.0),
            CgaPoint::new(f64::INFINITY, 1.0),
        );
        assert_eq!(segment.direction(), (0.0, 0.0));
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
    fn line_segment_intersection_returns_first_collinear_overlap() {
        let segment = CgaLineSegment::new(CgaPoint::new(0.0, 0.0), CgaPoint::new(4.0, 0.0));
        let overlapping = CgaLineSegment::new(CgaPoint::new(3.0, 0.0), CgaPoint::new(1.0, 0.0));
        let touching_point = CgaLineSegment::new(CgaPoint::new(4.0, 0.0), CgaPoint::new(4.0, 0.0));

        assert_eq!(
            segment.intersect(&overlapping),
            Some(CgaPoint::new(1.0, 0.0))
        );
        assert_eq!(
            segment.intersect(&touching_point),
            Some(CgaPoint::new(4.0, 0.0))
        );
    }

    #[test]
    fn line_segment_no_intersection_not_crossing() {
        // Lines would cross if extended, but segments don't
        let seg1 = CgaLineSegment::new(CgaPoint::new(0.0, 0.0), CgaPoint::new(1.0, 1.0));
        let seg2 = CgaLineSegment::new(CgaPoint::new(2.0, 0.0), CgaPoint::new(3.0, 1.0));
        assert!(seg1.intersect(&seg2).is_none());
    }

    #[test]
    fn line_segment_intersection_rejects_non_finite_endpoints() {
        let finite = CgaLineSegment::new(CgaPoint::origin(), CgaPoint::new(1.0, 1.0));
        let with_nan = CgaLineSegment::new(CgaPoint::new(f64::NAN, 0.0), CgaPoint::new(1.0, 0.0));
        let with_infinity =
            CgaLineSegment::new(CgaPoint::new(0.0, 1.0), CgaPoint::new(f64::INFINITY, 0.0));

        assert!(finite.intersect(&with_nan).is_none());
        assert!(with_infinity.intersect(&finite).is_none());
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
    fn line_segment_closest_point_and_distance_reject_non_finite_inputs() {
        let valid = CgaLineSegment::new(CgaPoint::origin(), CgaPoint::new(4.0, 0.0));
        let invalid = CgaLineSegment::new(CgaPoint::new(f64::NAN, 0.0), CgaPoint::new(4.0, 0.0));

        assert_eq!(
            valid.closest_point(&CgaPoint::new(f64::INFINITY, 0.0)),
            CgaPoint::origin()
        );
        assert!(
            valid
                .distance_to_point(&CgaPoint::new(f64::NAN, 0.0))
                .is_infinite()
        );
        assert_eq!(
            invalid.closest_point(&CgaPoint::origin()),
            CgaPoint::origin()
        );
        assert!(invalid.distance_to_point(&CgaPoint::origin()).is_infinite());

        // A segment long enough to overflow a squared length is NOT a non-finite input, and it
        // is no longer treated as one (bd-34yo). Both endpoints and the query are finite, and
        // the query lies exactly ON this segment, so the honest answers are the point itself
        // and a distance of zero. Reporting the origin sentinel and an infinite distance --
        // the same values this test demands for genuinely malformed input above -- made a
        // correct answer indistinguishable from a rejection.
        let overflowing =
            CgaLineSegment::new(CgaPoint::new(-f64::MAX, 0.0), CgaPoint::new(f64::MAX, 0.0));
        assert!(overflowing.start.is_finite() && overflowing.end.is_finite());
        assert_eq!(
            overflowing.closest_point(&CgaPoint::new(0.0, 0.0)),
            CgaPoint::origin()
        );
        assert_eq!(overflowing.distance_to_point(&CgaPoint::new(0.0, 0.0)), 0.0);

        // ...and an off-segment query against the same long segment projects onto it rather
        // than collapsing, which is what distinguishes a real projection from the sentinel.
        //
        // The answer is BOUNDED, not pinned, and deliberately so: the exact projection of
        // (1.0, 7.0) is (1.0, 0.0) at distance 7.0, but one ULP at ±f64::MAX is about 2e292,
        // so an offset of 1.0 along a segment of length 3.6e308 is far below the resolution of
        // its own endpoints. The projection lands on (0.0, 0.0) and the distance comes back as
        // sqrt(50). That is the representable limit of the input, not slack in the algorithm —
        // demanding 7.0 here would be demanding precision f64 cannot carry. What must hold is
        // that the result is finite, lies on the segment, and is close to the true distance
        // instead of the infinity this case used to report.
        let query = CgaPoint::new(1.0, 7.0);
        let off_segment = overflowing.closest_point(&query);
        assert!(off_segment.is_finite());
        assert!((off_segment.y - 0.0).abs() < 1e-10);
        let off_distance = overflowing.distance_to_point(&query);
        assert!(
            off_distance.is_finite() && (7.0..8.0).contains(&off_distance),
            "expected a real distance near 7.0, got {off_distance}"
        );
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
    fn circle_intersect_segment_tangent_with_fractional_coordinates() {
        let circle = CgaCircle::new(CgaPoint::new(0.2, -0.3), 0.7);
        let seg = CgaLineSegment::new(CgaPoint::new(-1.4, 0.4), CgaPoint::new(1.8, 0.4));
        let points = circle.intersect_segment(&seg);

        assert_eq!(points.len(), 1);
        assert!((points[0].x - 0.2).abs() < 1e-10);
        assert!((points[0].y - 0.4).abs() < 1e-10);
    }

    #[test]
    fn circle_intersect_segment_no_intersection() {
        let circle = CgaCircle::new(CgaPoint::origin(), 1.0);
        let seg = CgaLineSegment::new(CgaPoint::new(-2.0, 2.0), CgaPoint::new(2.0, 2.0));
        let points = circle.intersect_segment(&seg);
        assert!(points.is_empty());
    }

    #[test]
    fn circle_intersect_segment_degenerate_point_must_be_on_boundary() {
        let circle = CgaCircle::new(CgaPoint::origin(), 2.0);
        let interior = CgaLineSegment::new(CgaPoint::new(1.0, 0.0), CgaPoint::new(1.0, 0.0));
        let boundary = CgaLineSegment::new(CgaPoint::new(2.0, 0.0), CgaPoint::new(2.0, 0.0));

        assert!(circle.intersect_segment(&interior).is_empty());
        assert_eq!(
            circle.intersect_segment(&boundary),
            [CgaPoint::new(2.0, 0.0)]
        );
    }

    #[test]
    fn circle_intersect_circle_reports_crossings_and_tangency() {
        let unit_at_origin = CgaCircle::new(CgaPoint::origin(), 1.0);
        let overlapping = CgaCircle::new(CgaPoint::new(1.0, 0.0), 1.0);
        let crossings = unit_at_origin.intersect_circle(&overlapping);
        assert_eq!(crossings.len(), 2);
        for point in crossings {
            assert!((point.x - 0.5).abs() < 1e-12);
            assert!((point.y.abs() - 3.0_f64.sqrt() / 2.0).abs() < 1e-12);
        }

        let tangent = CgaCircle::new(CgaPoint::new(2.0, 0.0), 1.0);
        assert_eq!(
            unit_at_origin.intersect_circle(&tangent),
            [CgaPoint::new(1.0, 0.0)]
        );
    }

    #[test]
    fn circle_intersect_circle_rejects_non_unique_and_invalid_inputs() {
        let unit_at_origin = CgaCircle::new(CgaPoint::origin(), 1.0);
        assert!(unit_at_origin.intersect_circle(&unit_at_origin).is_empty());
        assert!(
            unit_at_origin
                .intersect_circle(&CgaCircle::new(CgaPoint::new(3.0, 0.0), 1.0))
                .is_empty()
        );
        assert!(
            unit_at_origin
                .intersect_circle(&CgaCircle::new(CgaPoint::new(0.0, 0.0), -1.0))
                .is_empty()
        );
        assert!(!CgaCircle::new(CgaPoint::origin(), -1.0).contains(&CgaPoint::origin()));
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
    fn rect_queries_reject_invalid_geometry_and_points() {
        let segment = CgaLineSegment::new(CgaPoint::new(-1.0, 0.5), CgaPoint::new(2.0, 0.5));
        let invalid_rectangles = [
            CgaRect::new(f64::NAN, 0.0, 1.0, 1.0),
            CgaRect::new(0.0, f64::INFINITY, 1.0, 1.0),
            CgaRect::new(0.0, 0.0, -1.0, 1.0),
            CgaRect::new(0.0, 0.0, 1.0, -1.0),
            CgaRect::new(f64::MAX, 0.0, f64::MAX, 1.0),
        ];

        for rect in invalid_rectangles {
            assert!(!rect.contains(&CgaPoint::new(0.5, 0.5)));
            assert!(rect.intersect_segment(&segment).is_empty());
        }

        let valid_rect = CgaRect::new(0.0, 0.0, 1.0, 1.0);
        assert!(!valid_rect.contains(&CgaPoint::new(f64::NAN, 0.5)));
        assert!(!valid_rect.contains(&CgaPoint::new(0.5, f64::INFINITY)));
        assert_eq!(
            valid_rect.closest_boundary_point(&CgaPoint::new(f64::NAN, 0.5)),
            CgaPoint::origin()
        );
        assert_eq!(
            invalid_rectangles[0].closest_boundary_point(&CgaPoint::origin()),
            CgaPoint::origin()
        );
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

    fn closest_boundary_cases() -> [(CgaRect, CgaPoint); 8] {
        [
            (CgaRect::new(0.0, 0.0, 10.0, 5.0), CgaPoint::new(5.0, 2.5)),
            (CgaRect::new(0.0, 0.0, 10.0, 5.0), CgaPoint::new(14.0, 1.0)),
            (CgaRect::new(0.0, 0.0, 10.0, 5.0), CgaPoint::new(6.0, 9.0)),
            (CgaRect::new(0.0, 0.0, 10.0, 5.0), CgaPoint::new(-4.0, 3.0)),
            (
                CgaRect::new(-100.0, -50.0, 200.0, 100.0),
                CgaPoint::new(130.0, -80.0),
            ),
            (
                CgaRect::new(1.5, -2.5, 0.75, 30.0),
                CgaPoint::new(1.875, 17.0),
            ),
            (
                CgaRect::new(-1_000_000.0, 750_000.0, 40.0, 80.0),
                CgaPoint::new(-999_970.0, 749_950.0),
            ),
            (
                CgaRect::new(-8.0, -4.0, 16.0, 8.0),
                CgaPoint::new(-11.0, 9.0),
            ),
        ]
    }

    /// Independent sqrt-based reference for `CgaRect::closest_boundary_point`.
    ///
    /// It must model the SAME contract, including the fail-closed precondition: a malformed rect
    /// (non-finite, negative extent, or overflowing far edge) or a non-finite query point yields
    /// the origin. Without this guard the reference silently computed a boundary point for
    /// geometry the real function rejects, so the two disagreed on every negative-extent rect.
    ///
    /// The guard is kept load-bearing by `rejected_cases`, which runs BOTH implementations over
    /// the fail-closed domain. Dropping the `is_valid` clause here must turn the test red.
    #[inline(never)]
    fn closest_boundary_reference(rect: &CgaRect, point: &CgaPoint) -> CgaPoint {
        if !rect.is_valid() || !point.is_finite() {
            return CgaPoint::origin();
        }

        let mut closest = rect.edges()[0].closest_point(point);
        let mut min_dist = point.distance(&closest);

        for edge in rect.edges().iter().skip(1) {
            let candidate = edge.closest_point(point);
            let dist = point.distance(&candidate);
            if dist < min_dist {
                min_dist = dist;
                closest = candidate;
            }
        }
        closest
    }

    fn assert_point_bits_eq(actual: CgaPoint, expected: CgaPoint) {
        assert_eq!(actual.x.to_bits(), expected.x.to_bits());
        assert_eq!(actual.y.to_bits(), expected.y.to_bits());
    }

    #[test]
    fn rect_closest_boundary_matches_sqrt_reference() {
        for (rect, point) in closest_boundary_cases() {
            assert_point_bits_eq(
                rect.closest_boundary_point(&point),
                closest_boundary_reference(&rect, &point),
            );
        }

        // Fail-closed contract, asserted DIRECTLY against the origin rather than against the
        // reference. Comparing two guarded implementations to each other would pass even if the
        // guard condition itself were wrong, so these pin the actual specified result.
        let rejected_cases = [
            (
                CgaRect::new(0.0, 0.0, 10.0, 5.0),
                CgaPoint::new(f64::NAN, 1.0),
            ),
            (
                CgaRect::new(0.0, 0.0, 10.0, 5.0),
                CgaPoint::new(f64::INFINITY, f64::NEG_INFINITY),
            ),
            // Far edge overflows to +inf even though every field is finite.
            (
                CgaRect::new(f64::MAX, -f64::MAX, f64::MAX, f64::MAX),
                CgaPoint::new(0.0, -0.0),
            ),
            // Negative extent — this is the shape the random loop below used to emit, which is
            // how the reference and the real function came to disagree.
            (CgaRect::new(0.0, 0.0, -5.0, 5.0), CgaPoint::new(1.0, 1.0)),
            (CgaRect::new(0.0, 0.0, 5.0, -5.0), CgaPoint::new(1.0, 1.0)),
        ];
        for (rect, point) in rejected_cases {
            assert!(!rect.is_valid() || !point.is_finite());
            assert_point_bits_eq(rect.closest_boundary_point(&point), CgaPoint::origin());
            // ...and the reference must reach the same verdict by its own route, so the guard in
            // `closest_boundary_reference` stays exercised. This is the assertion that was missing
            // originally: the unguarded reference returned a real boundary point here.
            assert_point_bits_eq(
                closest_boundary_reference(&rect, &point),
                CgaPoint::origin(),
            );
        }

        // Negative control for the guard: a VALID rect far out in the representable range must
        // still produce a real boundary point. Without this, tightening `is_valid` further could
        // swallow the whole extreme-value domain and every assertion above would still pass.
        // 1e150 is chosen because its squared edge length (1e300) is still representable.
        let extreme = CgaRect::new(0.0, 0.0, 1e150, 1e150);
        assert!(extreme.is_valid());
        let extreme_hit = extreme.closest_boundary_point(&CgaPoint::new(1.0, 2.0));
        assert!(
            extreme_hit.is_finite() && extreme_hit != CgaPoint::origin(),
            "a valid extreme rect must still return a real boundary point, got {extreme_hit:?}"
        );

        // bd-34yo, RESOLVED: a rect whose edges are long enough to overflow a squared length
        // used to degrade to the origin sentinel even though `is_valid` accepted it, so callers
        // could not tell "no answer" from "the answer is (0,0)". `closest_point` now retries the
        // projection in an exactly-scaled frame, so the whole is_valid-accepted domain returns a
        // real boundary point. This assertion previously pinned the origin; it is flipped
        // deliberately because the guarded behaviour was the defect, not the contract.
        let overflowing_extent = CgaRect::new(0.0, 0.0, f64::MAX, f64::MAX);
        assert!(overflowing_extent.is_valid());
        let overflowing_hit = overflowing_extent.closest_boundary_point(&CgaPoint::new(1.0, 2.0));
        assert!(
            overflowing_hit.is_finite()
                && overflowing_hit.x >= 0.0
                && overflowing_hit.x <= f64::MAX
                && overflowing_hit.y >= 0.0
                && overflowing_hit.y <= f64::MAX,
            "an is_valid rect must yield a real boundary point, got {overflowing_hit:?}"
        );

        // Degenerate but VALID: -0.0 extents satisfy `>= 0.0`, so this takes the real path.
        let degenerate = CgaRect::new(-0.0, 0.0, -0.0, 0.0);
        let degenerate_point = CgaPoint::new(-0.0, 0.0);
        assert!(degenerate.is_valid());
        assert_point_bits_eq(
            degenerate.closest_boundary_point(&degenerate_point),
            closest_boundary_reference(&degenerate, &degenerate_point),
        );

        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next_finite = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (f64::from_bits(0x3ff0_0000_0000_0000 | (state >> 12)) - 1.0) * 2_000.0 - 1_000.0
        };
        for _ in 0..20_000 {
            // Extents are magnitudes so every generated rect is VALID. Emitting negative extents
            // here would make both sides short-circuit to the origin, degenerating 20k iterations
            // of differential testing into `origin == origin` — proving nothing about the search.
            let rect = CgaRect::new(
                next_finite(),
                next_finite(),
                next_finite().abs(),
                next_finite().abs(),
            );
            let point = CgaPoint::new(next_finite(), next_finite());
            assert!(
                rect.is_valid() && point.is_finite(),
                "random case must exercise the real boundary search, not the guard"
            );
            assert_point_bits_eq(
                rect.closest_boundary_point(&point),
                closest_boundary_reference(&rect, &point),
            );
        }
    }

    /// Correctness of the intersection queries on realistic geometry (bd-2q3f.3 criterion 1:
    /// line-line, line-circle, circle-circle "verified against analytic solutions").
    ///
    /// The checks are deliberately NOT a second quadratic solver — re-deriving the same algebra
    /// and comparing would mostly prove the two copies agree. Instead each result is verified
    /// against the DEFINITION of an intersection (the point lies on both objects) plus a
    /// topological count that a plausible wrong implementation fails: a segment with one
    /// endpoint strictly inside a circle and the other strictly outside crosses the boundary
    /// exactly once, whatever the algebra says.
    #[test]
    fn intersection_queries_agree_with_their_geometric_definition() {
        let mut state = 0x853c_49e6_748f_ea9b_u64;
        let mut next_unit = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            f64::from_bits(0x3ff0_0000_0000_0000 | (state >> 12)) - 1.0
        };
        // Realistic magnitudes: extreme-range behaviour is the robustness sweep's job, and
        // mixing the two would make a precision failure look like a correctness failure.
        let mut next_coordinate = move || next_unit() * 200.0 - 100.0;
        let mut next_radius = move || next_unit().mul_add(40.0, 1.0);

        let mut crossing_cases = 0_u32;
        for _ in 0..20_000 {
            let circle = CgaCircle::new(
                CgaPoint::new(next_coordinate(), next_coordinate()),
                next_radius(),
            );
            let segment = CgaLineSegment::new(
                CgaPoint::new(next_coordinate(), next_coordinate()),
                CgaPoint::new(next_coordinate(), next_coordinate()),
            );

            // Line-circle: every reported point lies on the circle AND on the segment.
            let hits = circle.intersect_segment(&segment);
            assert!(hits.len() <= 2, "a segment meets a circle at most twice");
            for hit in &hits {
                let radial_error = (hit.distance(&circle.center) - circle.radius).abs();
                assert!(
                    radial_error < 1e-6 * circle.radius.max(1.0),
                    "{hit:?} is not on {circle:?} (radial error {radial_error})"
                );
                let off_segment = segment.distance_to_point(hit);
                assert!(
                    off_segment < 1e-6 * segment.start.distance(&segment.end).max(1.0),
                    "{hit:?} is not on {segment:?} (off by {off_segment})"
                );
            }

            // Topological control, independent of the algebra: strictly-inside to
            // strictly-outside must cross the boundary exactly once. Endpoints near the
            // boundary are skipped so the test never depends on tie-breaking at a tangency.
            let start_depth = segment.start.distance(&circle.center) - circle.radius;
            let end_depth = segment.end.distance(&circle.center) - circle.radius;
            let margin = 1e-3 * circle.radius.max(1.0);
            if start_depth < -margin && end_depth > margin {
                crossing_cases += 1;
                assert_eq!(
                    hits.len(),
                    1,
                    "inside {start_depth} -> outside {end_depth} must cross once, got {hits:?}"
                );
            }

            // Circle-circle: every reported point lies on both circles.
            let other_circle = CgaCircle::new(
                CgaPoint::new(next_coordinate(), next_coordinate()),
                next_radius(),
            );
            for hit in circle.intersect_circle(&other_circle) {
                for owner in [&circle, &other_circle] {
                    let radial_error = (hit.distance(&owner.center) - owner.radius).abs();
                    assert!(
                        radial_error < 1e-6 * owner.radius.max(1.0),
                        "{hit:?} is not on {owner:?} (radial error {radial_error})"
                    );
                }
            }

            // Line-line: a reported crossing lies on both segments.
            let other_segment = CgaLineSegment::new(
                CgaPoint::new(next_coordinate(), next_coordinate()),
                CgaPoint::new(next_coordinate(), next_coordinate()),
            );
            if let Some(crossing) = segment.intersect(&other_segment) {
                for owner in [&segment, &other_segment] {
                    let off = owner.distance_to_point(&crossing);
                    assert!(
                        off < 1e-6 * owner.start.distance(&owner.end).max(1.0),
                        "{crossing:?} is not on {owner:?} (off by {off})"
                    );
                }
            }
        }

        // The topological control must actually have been exercised; otherwise the assertion
        // above is dead and this test silently proves less than it claims.
        assert!(
            crossing_cases > 100,
            "expected many inside->outside segments, saw {crossing_cases}"
        );
    }

    /// `CgaPoint::distance` measures separation through the conformal inner product, which
    /// squares each coordinate. That embedding overflows at around 1.3e154 — far below the
    /// point where the distance itself stops being representable — and the resulting
    /// `inf - inf` inside the geometric product is NaN, not an overflow. So two perfectly
    /// finite points, whose separation is comfortably representable, reported a NaN distance.
    ///
    /// Found by the randomised sweep below; pinned here so the defect keeps a named test even
    /// if that generator is ever retuned.
    #[test]
    fn distance_between_far_apart_finite_points_is_finite() {
        // The exact pair the sweep surfaced.
        let near = CgaPoint::new(59.213_211_327_964_64, -1.272_648_833_201_636e-308);
        let far = CgaPoint::new(-5.880_064_458_331_531e299, -1.305_365_161_648_630_6e-308);

        let distance = near.distance(&far);
        assert!(
            distance.is_finite(),
            "distance between two finite points must be finite, got {distance}"
        );
        // The separation is dominated by the x gap, which is representable.
        assert!(
            (distance - 5.880_064_458_331_531e299).abs() < 1e285,
            "expected the true separation, got {distance}"
        );

        // d² genuinely exceeds f64 here, so infinity is the honest answer -- but never NaN.
        let squared = near.distance_squared(&far);
        assert!(!squared.is_nan() && squared > 0.0);

        // Ordinary geometry is exact.
        let a = CgaPoint::new(3.0, 4.0);
        assert_eq!(a.distance(&CgaPoint::origin()), 5.0);
        assert_eq!(a.distance_squared(&CgaPoint::origin()), 25.0);
    }

    /// Small separations between points far from the origin must survive.
    ///
    /// Evaluating d² as -2 P·Q through the conformal embedding squares each coordinate before
    /// subtracting, so a tiny gap between two distant points is annihilated. This pins the
    /// conditioning, not just the value: the identity is exact in real arithmetic, so a future
    /// change routing distance back through `inner_product` would still look mathematically
    /// correct while silently reintroducing the loss.
    #[test]
    fn small_separations_between_distant_points_survive() {
        let base = CgaPoint::new(70.020_074_126_673_5, -44.414_915_631_233_34);
        let nudged = CgaPoint::new(base.x + 1e-9, base.y);

        // Compare against the separation actually STORED, not the nominal 1e-9: at this
        // magnitude `base.x + 1e-9` lands on a neighbouring representable value, so the real
        // gap is 1.0000036e-9. Asserting the nominal figure would be asserting that f64 can
        // hold a number it cannot.
        let stored_separation = nudged.x - base.x;
        let distance = base.distance(&nudged);
        assert!(
            (distance - stored_separation).abs() < 1e-24,
            "expected the stored {stored_separation} separation to survive, got {distance}"
        );

        // The conformal identity is exact in real arithmetic but destroys this in f64 --
        // it reports exactly zero. Asserting the gap keeps the two evaluations from being
        // quietly swapped back.
        let conformal = (-2.0 * base.inner_product(&nudged)).max(0.0).sqrt();
        assert_eq!(
            conformal, 0.0,
            "conformal evaluation is expected to annihilate this separation"
        );
        assert!(distance > conformal);
    }

    /// Randomised robustness sweep over every CGA query, at magnitudes that reach the edge of
    /// the representable range (bd-2q3f.3: "degenerate cases handled without NaN/panic").
    ///
    /// The shared contract asserted here is narrow on purpose, because it is the one every
    /// caller relies on and none of them can defend against: given finite input, no query may
    /// panic, and no query may hand back a non-finite coordinate or a NaN distance. Accuracy is
    /// NOT asserted — at these magnitudes the exact answer is often unrepresentable, and that is
    /// checked against a reference elsewhere in this module on realistic geometry.
    #[test]
    fn cga_queries_never_produce_non_finite_output_for_finite_input() {
        // Deterministic xorshift; the interesting inputs are the extremes, so the generator
        // deliberately spends most of its draws on magnitudes near the limits rather than on
        // comfortable mid-range values that could never overflow anything.
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        let mut next_bits = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut next_coordinate = move || {
            let bits = next_bits();
            let unit = (f64::from_bits(0x3ff0_0000_0000_0000 | (bits >> 12)) - 1.0) * 2.0 - 1.0;
            match bits % 8 {
                0 => f64::MAX * unit,
                1 => f64::MIN_POSITIVE * unit,
                2 => 0.0,
                3 => -0.0,
                4 => unit * 1e300,
                5 => unit * 1e-300,
                6 => unit * 1e150,
                _ => unit * 1_000.0,
            }
        };
        let mut next_point = move || CgaPoint::new(next_coordinate(), next_coordinate());

        for _ in 0..20_000 {
            let segment = CgaLineSegment::new(next_point(), next_point());
            let other = CgaLineSegment::new(next_point(), next_point());
            let query = next_point();
            let circle = CgaCircle::new(next_point(), next_coordinate().abs());
            let rect = CgaRect::new(
                next_coordinate(),
                next_coordinate(),
                next_coordinate().abs(),
                next_coordinate().abs(),
            );

            // Every generated input is finite by construction, so every guard below is being
            // asked about well-formed geometry rather than about rejected garbage.
            assert!(segment.start.is_finite() && segment.end.is_finite());
            assert!(other.start.is_finite() && other.end.is_finite());
            assert!(query.is_finite() && circle.center.is_finite() && circle.radius.is_finite());

            let projected = segment.closest_point(&query);
            assert!(
                projected.is_finite(),
                "closest_point({segment:?}, {query:?}) -> {projected:?}"
            );

            let distance = segment.distance_to_point(&query);
            assert!(
                !distance.is_nan() && distance >= 0.0,
                "distance_to_point({segment:?}, {query:?}) -> {distance}"
            );

            if let Some(crossing) = segment.intersect(&other) {
                assert!(
                    crossing.is_finite(),
                    "intersect({segment:?}, {other:?}) -> {crossing:?}"
                );
            }

            for hit in circle.intersect_segment(&segment) {
                assert!(
                    hit.is_finite(),
                    "circle intersect_segment({circle:?}, {segment:?}) -> {hit:?}"
                );
            }

            for hit in rect.intersect_segment(&segment) {
                assert!(
                    hit.is_finite(),
                    "rect intersect_segment({rect:?}, {segment:?}) -> {hit:?}"
                );
            }

            let boundary = rect.closest_boundary_point(&query);
            assert!(
                boundary.is_finite(),
                "closest_boundary_point({rect:?}, {query:?}) -> {boundary:?}"
            );

            let point_distance = query.distance(&circle.center);
            assert!(
                !point_distance.is_nan() && point_distance >= 0.0,
                "distance({query:?}, {:?}) -> {point_distance}",
                circle.center
            );

            // Predicates must stay total: they may answer either way, but must not panic.
            let _ = circle.contains(&query);
            let _ = circle.contains_strict(&query);
            let _ = rect.contains(&query);
        }
    }

    /// `CgaLineSegment::closest_point` documents that callers never receive a non-finite
    /// projected point. Finite inputs must therefore always yield a finite result — the
    /// projection may be inexact at the edge of the representable range, but it may never be
    /// NaN or infinite, and it must lie on the segment's own bounding box.
    #[test]
    fn segment_closest_point_is_finite_for_all_finite_inputs() {
        let cases = [
            // Short vertical segment at the far negative edge, queried from the far positive
            // edge: `point.x - start.x` overflows to +inf while dx is exactly 0.0, so the
            // numerator forms inf * 0.0 = NaN.
            (
                CgaLineSegment::new(CgaPoint::new(-f64::MAX, 0.0), CgaPoint::new(-f64::MAX, 1.0)),
                CgaPoint::new(f64::MAX, 0.5),
            ),
            // Same shape with the axes swapped: dy is 0.0 and the y difference overflows.
            (
                CgaLineSegment::new(CgaPoint::new(0.0, -f64::MAX), CgaPoint::new(1.0, -f64::MAX)),
                CgaPoint::new(0.5, f64::MAX),
            ),
            // Segment whose squared length overflows even though every endpoint is finite.
            (
                CgaLineSegment::new(CgaPoint::new(0.0, 0.0), CgaPoint::new(f64::MAX, f64::MAX)),
                CgaPoint::new(1.0, 2.0),
            ),
            // Endpoint difference itself overflows.
            (
                CgaLineSegment::new(CgaPoint::new(-f64::MAX, 0.0), CgaPoint::new(f64::MAX, 0.0)),
                CgaPoint::new(0.0, 1.0),
            ),
        ];

        for (segment, point) in cases {
            assert!(segment.start.is_finite() && segment.end.is_finite() && point.is_finite());
            let got = segment.closest_point(&point);
            assert!(
                got.is_finite(),
                "closest_point({segment:?}, {point:?}) returned non-finite {got:?}"
            );
            // A projection must land within the segment's bounding box, which also rules out
            // the origin sentinel being returned for a perfectly well-formed segment.
            let (lo_x, hi_x) = (
                segment.start.x.min(segment.end.x),
                segment.start.x.max(segment.end.x),
            );
            let (lo_y, hi_y) = (
                segment.start.y.min(segment.end.y),
                segment.start.y.max(segment.end.y),
            );
            assert!(
                got.x >= lo_x && got.x <= hi_x && got.y >= lo_y && got.y <= hi_y,
                "closest_point({segment:?}, {point:?}) returned {got:?}, outside the segment box"
            );
        }
    }
}

/// Floating-point fault tests for rotors (bd-1s1g.2).
///
/// These cover the numeric edges a transform stack actually reaches: long composition chains,
/// coordinates whose conformal embedding squares near the top of `f64`, rotations too small or too
/// close to π to be well conditioned, and non-finite input. Each asserts the OBSERVABLE
/// consequence — what a point does under the transform — not just an internal invariant, because a
/// rotor whose norm is fine can still move geometry and a rotor whose norm drifts is only a defect
/// if it scales something.
#[cfg(test)]
mod rotor_fault_tests {
    use super::*;

    /// The distance a transform must preserve, expressed on a point rather than on the rotor.
    fn radius_after(rotor: Rotor, x: f64, y: f64) -> f64 {
        let (rx, ry) = rotor.to_affine_matrix().apply(x, y);
        rx.hypot(ry)
    }

    /// Composing many small rotations must not accumulate scale.
    ///
    /// This is the drift that has a visible consequence: a rotor is applied through
    /// `to_affine_matrix`, so a norm that creeps away from 1 becomes a progressive scaling of every
    /// transformed coordinate. 10,000 steps of 0.001 rad is 10 rad total, which also wraps past 2π
    /// several times and so exercises the sign structure rather than a single quadrant.
    #[test]
    fn chained_small_rotations_do_not_accumulate_scale() {
        let step = Rotor::rotation(0.001);
        let mut chained = Rotor::identity();
        for _ in 0..10_000 {
            chained = chained.compose(step);
        }

        let norm_sq = chained.norm_squared();
        assert!(
            norm_sq.is_finite(),
            "10k composed rotations produced a non-finite norm: {norm_sq}"
        );
        assert!(
            (norm_sq - 1.0).abs() < 1e-9,
            "rotor norm drifted to {norm_sq} after 10k compositions (want 1 within 1e-9)"
        );

        // The consequence, measured on geometry: a rotation may not change a point's radius.
        let radius = radius_after(chained, 1.0, 0.0);
        assert!(
            (radius - 1.0).abs() < 1e-9,
            "10k composed rotations scaled a unit point to radius {radius}"
        );

        // And it must still be the rotation it claims to be: 10,000 * 0.001 rad = 10 rad.
        let (cx, cy) = chained.to_affine_matrix().apply(1.0, 0.0);
        let (ex, ey) = Rotor::rotation(10.0).to_affine_matrix().apply(1.0, 0.0);
        assert!(
            (cx - ex).abs() < 1e-6 && (cy - ey).abs() < 1e-6,
            "chained rotation landed at ({cx}, {cy}), single rotation(10.0) lands at ({ex}, {ey})"
        );
    }

    /// Extreme coordinates must stay finite through the conformal embedding.
    ///
    /// The embedding squares coordinates, so (1e15)^2 = 1e30 — large but representable. The failure
    /// this guards is the one already recorded for distances: an intermediate that overflows to
    /// infinity and then meets a zero, because `inf * 0.0` is NaN and a NaN coordinate silently
    /// removes a node from the output rather than misplacing it.
    #[test]
    fn extreme_coordinates_transform_without_nan() {
        for rotor in [
            Rotor::identity(),
            Rotor::rotation(std::f64::consts::FRAC_PI_4),
            Rotor::translation(1.0, -1.0),
            Rotor::scale(2.0),
        ] {
            let (x, y) = rotor.to_affine_matrix().apply(1e15, 1e15);
            assert!(
                !x.is_nan() && !y.is_nan(),
                "rotor {:?} turned (1e15, 1e15) into a NaN coordinate ({x}, {y})",
                rotor.components
            );
            assert!(
                x.is_finite() && y.is_finite(),
                "rotor {:?} turned (1e15, 1e15) into a non-finite coordinate ({x}, {y})",
                rotor.components
            );
        }
    }

    /// A rotation far below the angle resolution of the geometry must still be a valid rotor.
    #[test]
    fn near_zero_rotation_is_a_valid_identity_like_rotor() {
        let tiny = Rotor::rotation(1e-15);
        assert!(
            (tiny.norm_squared() - 1.0).abs() < 1e-12,
            "rotation(1e-15) has norm {} (want 1)",
            tiny.norm_squared()
        );
        assert!(
            (tiny.components[0] - 1.0).abs() < 1e-15,
            "cos(theta/2) should be ~1 for a 1e-15 rotation, got {}",
            tiny.components[0]
        );
        let (x, y) = tiny.to_affine_matrix().apply(1.0, 0.0);
        assert!(
            (x - 1.0).abs() < 1e-12 && y.abs() < 1e-12,
            "a 1e-15 rotation moved (1,0) to ({x}, {y})"
        );
    }

    /// Just short of π is where a half-angle formulation can flip sign.
    ///
    /// The rotor stores cos(θ/2) and sin(θ/2); at θ ≈ π the scalar part passes through zero, so a
    /// sign error here produces a rotation by -π instead of π — visually a mirror, and silent.
    #[test]
    fn near_pi_rotation_does_not_flip_sign() {
        let angle = std::f64::consts::PI - 1e-15;
        let rotor = Rotor::rotation(angle);
        let (x, y) = rotor.to_affine_matrix().apply(1.0, 0.0);
        assert!(
            (x + 1.0).abs() < 1e-9,
            "rotation by ~pi should send (1,0) to (-1,0), got ({x}, {y})"
        );
        assert!(
            y.abs() < 1e-6,
            "rotation by ~pi put a large imaginary-axis component at ({x}, {y})"
        );
        // The sign of the rotation direction must match a slightly smaller angle, i.e. approaching
        // pi from below must not jump to approaching it from above.
        let just_under = Rotor::rotation(angle - 1e-3).to_affine_matrix().apply(1.0, 0.0);
        assert!(
            just_under.1.signum() == y.signum() || y.abs() < 1e-12,
            "the rotation direction flipped between {} and {angle} rad: {just_under:?} vs ({x}, {y})",
            angle - 1e-3
        );
    }

    /// A zero translation must be exactly the identity, not merely close to it.
    #[test]
    fn zero_translation_is_exactly_identity() {
        assert_eq!(
            Rotor::translation(0.0, 0.0).components,
            Rotor::identity().components,
            "translation(0,0) must be bit-identical to the identity rotor"
        );
    }

    /// A negative scale is refused rather than silently producing a wrong transform.
    ///
    /// A reflection is not a rotor: `scale` is built from `cosh(ln s / 2)`, and `ln` of a negative
    /// number is undefined. The API asserts, and this pins that contract — the alternative, letting
    /// a NaN rotor through, would mirror geometry or delete it depending on where the NaN landed.
    #[test]
    #[should_panic(expected = "scale factor must be positive")]
    fn negative_scale_is_refused() {
        let _ = Rotor::scale(-1.0);
    }

    /// A unit scale is the identity, which is the positive half of the contract above.
    #[test]
    fn unit_scale_is_identity_like() {
        let rotor = Rotor::scale(1.0);
        let (x, y) = rotor.to_affine_matrix().apply(3.0, -7.0);
        assert!(
            (x - 3.0).abs() < 1e-12 && (y + 7.0).abs() < 1e-12,
            "scale(1.0) moved (3,-7) to ({x}, {y})"
        );
    }

    /// A NaN coordinate must stay detectable rather than becoming a plausible number.
    ///
    /// The transform API returns `(f64, f64)` and so cannot report a structured error; what it must
    /// not do is launder a NaN into a finite coordinate, because a finite-but-wrong point is placed
    /// in the diagram and never questioned, while a NaN is caught by the finite-coordinate
    /// invariants the layout and renderers already assert.
    #[test]
    fn nan_input_stays_nan_rather_than_becoming_a_plausible_point() {
        let rotor = Rotor::rotation(std::f64::consts::FRAC_PI_3);
        let (x, y) = rotor.to_affine_matrix().apply(f64::NAN, 0.0);
        assert!(
            x.is_nan() || y.is_nan(),
            "a NaN input coordinate produced the finite point ({x}, {y}); a NaN must stay visible"
        );
    }

    /// Inverting a drifted rotor must still undo it.
    ///
    /// `inverse` takes a fast path when the norm is within 1e-12 of 1 (returning the reverse) and a
    /// division path otherwise. A long chain lands near but not exactly on that boundary, so this
    /// exercises whichever path the drift selects and requires the round trip either way.
    #[test]
    fn inverse_undoes_a_long_composition_chain() {
        let step = Rotor::rotation(0.001);
        let mut chained = Rotor::identity();
        for _ in 0..10_000 {
            chained = chained.compose(step);
        }
        let inverse = chained.inverse().expect("a composed rotation chain must be invertible");
        let round_trip = chained.compose(inverse);
        let (x, y) = round_trip.to_affine_matrix().apply(5.0, -2.0);
        assert!(
            (x - 5.0).abs() < 1e-6 && (y + 2.0).abs() < 1e-6,
            "rotor composed with its inverse moved (5,-2) to ({x}, {y})"
        );
    }
}
