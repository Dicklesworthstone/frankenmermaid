//! The terminal's layout-to-cell transform, composed as a CGA rotor plus an explicit aspect
//! (bd-2q3f.2).
//!
//! # Why this is not simply a rotor
//!
//! A character cell is taller than it is wide, so the terminal maps layout units to cells with
//! DIFFERENT x and y factors. That is an anisotropic scale — a squeeze — and a conformal rotor
//! cannot represent one: rotors carry rotation, translation and UNIFORM dilation, and the squeeze is
//! not in that subgroup. `AffineMatrix2D::to_rotor` demonstrates the consequence, reading scale from
//! the x axis alone and dropping the y factor without a word; `try_to_rotor` now refuses such input
//! instead.
//!
//! So this type splits the transform honestly rather than forcing it into one object:
//!
//! ```text
//!   layout point ──[ rotor: translate + UNIFORM scale ]──▶ square cells ──[ aspect ]──▶ terminal
//! ```
//!
//! The rotor half composes and inverts with all the machinery in `fm_core::cga`; the aspect is one
//! scalar applied LAST, in device space, which is exactly where a display-aspect correction belongs.
//! Folding it into the rotor to make the types line up is the silent-loss path, and it is the one
//! thing this file exists to prevent.
//!
//! # Why the equivalence test is the deliverable
//!
//! The terminal already renders correctly with plain `x * scale_x` arithmetic. A transform pipeline
//! is worth having for composition and for extracting rotation later, but ONLY if it reproduces that
//! arithmetic exactly — a refactor that shifts a single cell boundary would move glyphs for no
//! benefit the reader can see. The tests below therefore compare against the raw multiplication
//! rather than against hand-computed expectations.

use fm_core::cga::TransformStack;

/// The terminal's layout-to-cell transform.
#[derive(Debug, Clone)]
pub struct TermTransform {
    /// Rotor-composed part: translation and the uniform scale.
    stack: TransformStack,
    /// Uniform scale taken from the x axis.
    uniform: f64,
    /// `scale_y / scale_x`. Exactly 1.0 when the grid is square, which is the case where this whole
    /// transform collapses to a pure rotor.
    aspect: f64,
}

impl TermTransform {
    /// Build the transform for a grid whose cells scale layout units by `scale_x` and `scale_y`.
    ///
    /// Returns `None` for a degenerate or non-finite scale. A zero x scale would make the aspect
    /// undefined, and silently substituting 1.0 there would render a diagram at the wrong shape
    /// while looking deliberate.
    #[must_use]
    pub fn new(scale_x: f32, scale_y: f32) -> Option<Self> {
        let (sx, sy) = (f64::from(scale_x), f64::from(scale_y));
        if !sx.is_finite() || !sy.is_finite() || sx <= 0.0 || sy <= 0.0 {
            return None;
        }

        // Keep layout-space translations on the stack so `pop` only undoes an explicit pan.
        // `rotor_part` applies the grid's fixed uniform scale after those translations.
        let stack = TransformStack::new();

        Some(Self {
            stack,
            uniform: sx,
            aspect: sy / sx,
        })
    }

    /// Translate in LAYOUT space, before the scale.
    ///
    /// Pushed onto the rotor stack, so panning composes with everything already there instead of
    /// being tracked as a separate pair of offsets.
    ///
    /// Both the input and the stored translation use layout units. The rotor applies the uniform
    /// scale afterwards, and the terminal's aspect correction remains last.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.stack.push_translation(f64::from(dx), f64::from(dy));
    }

    /// Undo the most recent pushed transform. `false` when nothing was pushed.
    pub fn pop(&mut self) -> bool {
        self.stack.pop()
    }

    /// The uniform scale factor the rotor carries.
    #[must_use]
    pub fn uniform_scale(&self) -> f64 {
        self.uniform
    }

    /// `scale_y / scale_x`; 1.0 exactly when the grid is square.
    #[must_use]
    pub fn aspect(&self) -> f64 {
        self.aspect
    }

    /// Whether a rotor alone could express this transform.
    ///
    /// True only for a square grid. Callers that want to hand the transform to CGA machinery
    /// wholesale must check this first rather than assume it.
    #[must_use]
    pub fn is_pure_rotor(&self) -> bool {
        (self.aspect - 1.0).abs() <= 1e-12
    }

    /// Map a layout point to terminal cell space.
    ///
    /// Rotor first, aspect last. The order is not interchangeable once a rotation is on the stack:
    /// a squeeze applied before a rotation is a different map from one applied after, and the
    /// terminal's aspect belongs to the DEVICE, so it is applied in device space.
    #[must_use]
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        let (rx, ry) = self
            .rotor_part()
            .to_affine_matrix()
            .apply(f64::from(x), f64::from(y));
        // The grid is f32 throughout and the f64 rotor is only an intermediate. No #[expect] here:
        // the crate already allows cast_possible_truncation at the top of lib.rs, so an expectation
        // would go UNFULFILLED, and an unfulfilled expectation is itself a warning that CI turns
        // into an error under -D warnings.
        (rx as f32, (ry * self.aspect) as f32)
    }

    /// The composed rotor, for callers that need the CGA object itself.
    ///
    /// Deliberately NOT the whole transform: the aspect is not in it, and a caller that treats this
    /// as the full mapping gets the square-grid answer. `is_pure_rotor` is how to find out whether
    /// that distinction matters for the current grid.
    #[must_use]
    pub fn rotor_part(&self) -> fm_core::cga::Rotor {
        // S * T maps a layout point to s * (point + pan). This order keeps translations in the
        // caller's units and leaves the fixed scale outside the undo stack.
        fm_core::cga::Rotor::scale(self.uniform).compose(self.stack.rotor())
    }
}

#[cfg(test)]
mod tests {
    use super::TermTransform;

    /// EQUIVALENCE: the composed transform must reproduce the raw arithmetic exactly.
    ///
    /// This is the whole point of the refactor. Compared against `x * scale_x` rather than against
    /// hand-written expectations, so the test cannot drift into asserting whatever the new code
    /// happens to do.
    #[test]
    fn the_transform_reproduces_the_raw_scale_arithmetic() {
        // Deliberately anisotropic, and by a large factor: a terminal cell is roughly twice as tall
        // as it is wide, so this is the shape the renderer actually runs at.
        let cases = [(2.0_f32, 4.0_f32), (0.5, 1.0), (3.25, 1.75), (1.0, 1.0)];

        for (scale_x, scale_y) in cases {
            let transform = TermTransform::new(scale_x, scale_y).expect("a valid grid");
            for (x, y) in [(0.0_f32, 0.0_f32), (1.0, 1.0), (37.5, -12.25), (1e4, 5e3)] {
                let (got_x, got_y) = transform.apply(x, y);
                let (want_x, want_y) = (x * scale_x, y * scale_y);

                let tolerance = 1e-3 * want_x.abs().max(want_y.abs()).max(1.0);
                assert!(
                    (got_x - want_x).abs() <= tolerance && (got_y - want_y).abs() <= tolerance,
                    "scale ({scale_x}, {scale_y}) point ({x}, {y}): got ({got_x}, {got_y}), \
                     raw arithmetic gives ({want_x}, {want_y})"
                );
            }
        }
    }

    /// A square grid IS a pure rotor; a terminal grid is not.
    ///
    /// The control that keeps `is_pure_rotor` meaningful: something that always answered "yes"
    /// would let a caller hand an anisotropic transform to rotor-only machinery, which is the exact
    /// silent loss this module was written to avoid.
    #[test]
    fn only_a_square_grid_is_a_pure_rotor() {
        assert!(
            TermTransform::new(2.0, 2.0)
                .expect("square")
                .is_pure_rotor()
        );
        assert!(
            !TermTransform::new(2.0, 4.0)
                .expect("tall cells")
                .is_pure_rotor(),
            "an anisotropic grid must not claim to be a rotor"
        );
    }

    /// Translation composes through the rotor, and still lands where the raw arithmetic would.
    #[test]
    fn translation_composes_and_matches_the_raw_arithmetic() {
        let (scale_x, scale_y) = (2.0_f32, 4.0_f32);
        let mut transform = TermTransform::new(scale_x, scale_y).expect("a valid grid");
        transform.translate(10.0, 20.0);

        // Translation is pushed in layout space, so the expected result scales the SHIFTED point.
        let (got_x, got_y) = transform.apply(3.0, 5.0);
        let (want_x, want_y) = ((3.0 + 10.0) * scale_x, (5.0 + 20.0) * scale_y);

        assert!(
            (got_x - want_x).abs() < 1e-3 && (got_y - want_y).abs() < 1e-3,
            "got ({got_x}, {got_y}), wanted ({want_x}, {want_y})"
        );

        assert!(transform.pop(), "the pushed translation should pop");
        let (got_x, got_y) = transform.apply(3.0, 5.0);
        assert!(
            (got_x - 3.0 * scale_x).abs() < 1e-3 && (got_y - 5.0 * scale_y).abs() < 1e-3,
            "popping did not restore the untranslated mapping: ({got_x}, {got_y})"
        );
    }

    #[test]
    fn nested_layout_pans_preserve_aspect_inverse_and_undo() {
        for (scale_x, scale_y) in [(0.25_f32, 0.5_f32), (3.0, 1.5), (2.0, 2.0)] {
            let mut transform = TermTransform::new(scale_x, scale_y).expect("valid grid");
            transform.translate(10.0, -20.0);
            transform.translate(-3.0, 5.0);
            let (x, y) = transform.apply(4.0, 8.0);
            assert!((x - 11.0 * scale_x).abs() < 1e-5);
            assert!((y + 7.0 * scale_y).abs() < 1e-5);

            let inverse = transform.rotor_part().inverse().expect("invertible rotor");
            let (layout_x, layout_y) = inverse
                .to_affine_matrix()
                .apply(f64::from(x), f64::from(y) / transform.aspect());
            assert!((layout_x - 4.0).abs() < 1e-9);
            assert!((layout_y - 8.0).abs() < 1e-9);

            assert!(transform.pop());
            let (x, y) = transform.apply(4.0, 8.0);
            assert!((x - 14.0 * scale_x).abs() < 1e-5);
            assert!((y + 12.0 * scale_y).abs() < 1e-5);
            assert!(transform.pop());
            assert!(!transform.pop());
            let (x, y) = transform.apply(4.0, 8.0);
            assert!((x - 4.0 * scale_x).abs() < 1e-5);
            assert!((y - 8.0 * scale_y).abs() < 1e-5);
        }
    }

    /// CONTROL: a degenerate grid is refused rather than silently normalised.
    #[test]
    fn a_degenerate_grid_is_refused() {
        assert!(
            TermTransform::new(0.0, 4.0).is_none(),
            "a zero x scale has no defined aspect"
        );
        assert!(TermTransform::new(2.0, 0.0).is_none());
        assert!(TermTransform::new(-1.0, 4.0).is_none());
        assert!(TermTransform::new(f32::NAN, 4.0).is_none());
        assert!(TermTransform::new(f32::INFINITY, 4.0).is_none());
    }
}
