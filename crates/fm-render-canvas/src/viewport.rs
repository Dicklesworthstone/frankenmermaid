//! Viewport transformation for canvas rendering.
//!
//! Handles fitting diagrams to canvas dimensions with proper
//! padding, zoom, and pan support.

use fm_core::cga::TransformStack;

/// A viewport defines the visible area of the diagram.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// X offset (pan).
    pub offset_x: f64,
    /// Y offset (pan).
    pub offset_y: f64,
    /// Zoom level (1.0 = 100%).
    pub zoom: f64,
    /// Canvas width in pixels.
    pub canvas_width: f64,
    /// Canvas height in pixels.
    pub canvas_height: f64,
    /// Device pixel ratio for high-DPI displays.
    pub device_pixel_ratio: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            zoom: 1.0,
            canvas_width: 800.0,
            canvas_height: 600.0,
            device_pixel_ratio: 1.0,
        }
    }
}

impl Viewport {
    /// Create a new viewport with the given canvas dimensions.
    #[must_use]
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            canvas_width: width,
            canvas_height: height,
            ..Default::default()
        }
    }

    /// Fit layout bounds into an exported image without changing their aspect ratio.
    ///
    /// Unlike the interactive fit, exports may enlarge a diagram. Bounds may have a negative
    /// origin or zero extent; unused image space is centered around the actual layout bounds.
    /// Invalid dimensions and transforms that cannot be represented return `None`.
    #[must_use]
    pub fn fit_image(bounds: [f64; 4], canvas_width: f64, canvas_height: f64) -> Option<Self> {
        let [x, y, width, height] = bounds;
        if !bounds.into_iter().all(f64::is_finite)
            || width < 0.0
            || height < 0.0
            || !canvas_width.is_finite()
            || !canvas_height.is_finite()
            || canvas_width <= 0.0
            || canvas_height <= 0.0
        {
            return None;
        }
        let zoom = (canvas_width / width.max(1.0)).min(canvas_height / height.max(1.0));
        let offset_x = (canvas_width - width * zoom) / 2.0 - x * zoom;
        let offset_y = (canvas_height - height * zoom) / 2.0 - y * zoom;
        if !zoom.is_finite() || zoom <= 0.0 || !offset_x.is_finite() || !offset_y.is_finite() {
            return None;
        }
        Some(Self {
            offset_x,
            offset_y,
            zoom,
            canvas_width,
            canvas_height,
            device_pixel_ratio: 1.0,
        })
    }

    /// Map layout coordinates to GPU clip space, including the canvas-to-clip Y reversal.
    #[must_use]
    pub fn clip_transform(&self) -> Option<[f32; 4]> {
        if !self.canvas_width.is_finite()
            || !self.canvas_height.is_finite()
            || self.canvas_width <= 0.0
            || self.canvas_height <= 0.0
            || !self.zoom.is_finite()
            || self.zoom <= 0.0
        {
            return None;
        }
        let camera = [
            (2.0 * self.zoom / self.canvas_width) as f32,
            (-2.0 * self.zoom / self.canvas_height) as f32,
            (2.0 * self.offset_x / self.canvas_width - 1.0) as f32,
            (1.0 - 2.0 * self.offset_y / self.canvas_height) as f32,
        ];
        (camera.into_iter().all(f32::is_finite) && camera[0] > 0.0 && camera[1] < 0.0)
            .then_some(camera)
    }

    /// Set the device pixel ratio for high-DPI displays.
    ///
    /// Values are clamped to the range `[0.25, 4.0]` to prevent rendering
    /// artifacts from zero, negative, or extreme ratios.
    #[must_use]
    pub fn with_dpr(mut self, dpr: f64) -> Self {
        self.device_pixel_ratio = dpr.clamp(0.25, 4.0);
        self
    }

    /// Pan the viewport by the given delta.
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.offset_x += dx;
        self.offset_y += dy;
    }

    /// Zoom the viewport around a point.
    pub fn zoom_at(&mut self, factor: f64, center_x: f64, center_y: f64) {
        let old_zoom = self.zoom;
        self.zoom = (self.zoom * factor).clamp(0.1, 10.0);

        // Adjust offset to keep the center point stationary
        let zoom_change = self.zoom / old_zoom;
        self.offset_x = center_x - (center_x - self.offset_x) * zoom_change;
        self.offset_y = center_y - (center_y - self.offset_y) * zoom_change;
    }

    /// Reset zoom and pan to default.
    pub fn reset(&mut self) {
        self.offset_x = 0.0;
        self.offset_y = 0.0;
        self.zoom = 1.0;
    }

    /// Convert a canvas point to diagram coordinates.
    #[must_use]
    pub fn canvas_to_diagram(&self, canvas_x: f64, canvas_y: f64) -> (f64, f64) {
        let x = (canvas_x - self.offset_x) / self.zoom;
        let y = (canvas_y - self.offset_y) / self.zoom;
        (x, y)
    }

    /// Convert a diagram point to canvas coordinates.
    #[must_use]
    pub fn diagram_to_canvas(&self, diagram_x: f64, diagram_y: f64) -> (f64, f64) {
        let x = diagram_x * self.zoom + self.offset_x;
        let y = diagram_y * self.zoom + self.offset_y;
        (x, y)
    }

    /// Get the transform matrix for this viewport.
    #[must_use]
    pub fn transform(&self) -> ViewportTransform {
        // Translation FIRST. `push_rotor` composes as `composed = composed * rotor`, so the first
        // push ends up leftmost, and `Rotor::to_affine_matrix` decomposes the canonical
        // `M = T * S * R` — the same order the `TransformStack` doc example pushes in.
        //
        // Pushing scale first instead built `S * T`, which that decomposition cannot read back:
        // it recovered the offset without the zoom, so `transform()` mapped (25, 40) to
        // (100, 105) where `diagram_to_canvas` gives (150, 130). This is the diagram-to-canvas
        // mapping, `zoom * p + offset`, so the offset must be applied AFTER the zoom.
        let mut transforms = TransformStack::new();
        transforms.push_translation(self.offset_x, self.offset_y);
        transforms.push_scale(self.zoom);
        let affine = transforms.to_affine_matrix();
        ViewportTransform {
            a: affine.a,
            b: affine.c,
            c: affine.b,
            d: affine.d,
            e: affine.tx,
            f: affine.ty,
        }
    }
}

/// A 2D affine transformation matrix.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ViewportTransform {
    pub a: f64, // scale x
    pub b: f64, // skew y
    pub c: f64, // skew x
    pub d: f64, // scale y
    pub e: f64, // translate x
    pub f: f64, // translate y
}

impl ViewportTransform {
    /// Create an identity transform.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Create a translation transform.
    #[must_use]
    pub const fn translate(x: f64, y: f64) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: x,
            f: y,
        }
    }

    /// Create a scale transform.
    #[must_use]
    pub const fn scale(sx: f64, sy: f64) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Multiply two transforms.
    #[must_use]
    pub fn multiply(&self, other: &Self) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }

    /// Apply this transform to a point.
    #[must_use]
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
}

/// Compute the transform needed to fit a diagram within a viewport.
#[must_use]
pub fn fit_to_viewport(
    diagram_width: f64,
    diagram_height: f64,
    canvas_width: f64,
    canvas_height: f64,
    padding: f64,
) -> Viewport {
    // Clamp available dimensions so pathological padding values can't produce
    // negative scales/zooms (which would mirror the diagram).
    let available_width = (canvas_width - 2.0 * padding).max(1.0);
    let available_height = (canvas_height - 2.0 * padding).max(1.0);

    if diagram_width <= 0.0 || diagram_height <= 0.0 {
        return Viewport::new(canvas_width, canvas_height);
    }

    let scale_x = available_width / diagram_width;
    let scale_y = available_height / diagram_height;
    let zoom = scale_x.min(scale_y).min(1.0); // Don't zoom in beyond 100%

    let scaled_width = diagram_width * zoom;
    let scaled_height = diagram_height * zoom;

    let offset_x = (canvas_width - scaled_width) / 2.0;
    let offset_y = (canvas_height - scaled_height) / 2.0;

    Viewport {
        offset_x,
        offset_y,
        zoom,
        canvas_width,
        canvas_height,
        device_pixel_ratio: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_fit_preserves_aspect_ratio_and_centers_negative_origins() {
        for (canvas_width, canvas_height) in [(300.0, 100.0), (100.0, 300.0)] {
            let vp = Viewport::fit_image([-50.0, -25.0, 100.0, 50.0], canvas_width, canvas_height)
                .expect("finite image bounds");
            let center = vp.diagram_to_canvas(0.0, 0.0);
            assert_eq!(center, (canvas_width / 2.0, canvas_height / 2.0));
            let horizontal = vp.diagram_to_canvas(10.0, 0.0).0 - center.0;
            let vertical = vp.diagram_to_canvas(0.0, 10.0).1 - center.1;
            assert!((horizontal - vertical).abs() < 1e-9);
            let [sx, sy, tx, ty] = vp.clip_transform().expect("representable clip transform");
            for (x, y) in [(-50.0, -25.0), (0.0, 0.0), (50.0, 25.0)] {
                let (px, py) = vp.diagram_to_canvas(x, y);
                assert!(
                    (f64::from(sx) * x + f64::from(tx) - (2.0 * px / canvas_width - 1.0)).abs()
                        < 1e-6
                );
                assert!(
                    (f64::from(sy) * y + f64::from(ty) - (1.0 - 2.0 * py / canvas_height)).abs()
                        < 1e-6
                );
            }
        }
    }

    #[test]
    fn image_fit_can_enlarge_shrink_and_center_empty_layouts() {
        let enlarged = Viewport::fit_image([0.0, 0.0, 10.0, 10.0], 100.0, 100.0).unwrap();
        assert_eq!(enlarged.zoom, 10.0);
        let reduced = Viewport::fit_image([0.0, 0.0, 1000.0, 500.0], 100.0, 100.0).unwrap();
        assert_eq!(reduced.zoom, 0.1);
        assert_eq!(reduced.diagram_to_canvas(0.0, 0.0), (0.0, 25.0));
        let empty = Viewport::fit_image([-10.0, -20.0, 0.0, 0.0], 100.0, 200.0).unwrap();
        assert_eq!(empty.diagram_to_canvas(-10.0, -20.0), (50.0, 100.0));
        assert!(empty.clip_transform().is_some());
    }

    #[test]
    fn image_fit_rejects_invalid_dimensions_and_unrepresentable_transforms() {
        for bounds in [
            [f64::NAN, 0.0, 1.0, 1.0],
            [0.0, f64::INFINITY, 1.0, 1.0],
            [0.0, 0.0, -1.0, 1.0],
            [0.0, 0.0, 1.0, -1.0],
        ] {
            assert!(Viewport::fit_image(bounds, 100.0, 100.0).is_none());
        }
        for (width, height) in [
            (0.0, 100.0),
            (100.0, 0.0),
            (-1.0, 100.0),
            (f64::NAN, 100.0),
            (100.0, f64::INFINITY),
        ] {
            assert!(Viewport::fit_image([0.0, 0.0, 1.0, 1.0], width, height).is_none());
        }
        let invalid = Viewport {
            offset_x: f64::INFINITY,
            ..Viewport::default()
        };
        assert!(invalid.clip_transform().is_none());
        let unrepresentable = Viewport {
            zoom: f64::MIN_POSITIVE,
            ..Viewport::default()
        };
        assert!(unrepresentable.clip_transform().is_none());
    }

    #[test]
    fn viewport_pan() {
        let mut vp = Viewport::new(800.0, 600.0);
        vp.pan(10.0, 20.0);
        assert_eq!(vp.offset_x, 10.0);
        assert_eq!(vp.offset_y, 20.0);
    }

    #[test]
    fn viewport_zoom() {
        let mut vp = Viewport::new(800.0, 600.0);
        vp.zoom_at(2.0, 400.0, 300.0);
        assert!((vp.zoom - 2.0).abs() < 0.001);
    }

    #[test]
    fn coordinate_conversion() {
        let vp = Viewport {
            offset_x: 100.0,
            offset_y: 50.0,
            zoom: 2.0,
            canvas_width: 800.0,
            canvas_height: 600.0,
            device_pixel_ratio: 1.0,
        };

        let (dx, dy) = vp.canvas_to_diagram(300.0, 150.0);
        assert!((dx - 100.0).abs() < 0.001);
        assert!((dy - 50.0).abs() < 0.001);

        let (cx, cy) = vp.diagram_to_canvas(100.0, 50.0);
        assert!((cx - 300.0).abs() < 0.001);
        assert!((cy - 150.0).abs() < 0.001);
    }

    #[test]
    fn cga_viewport_transform_matches_coordinate_conversion() {
        let vp = Viewport {
            offset_x: 100.0,
            offset_y: 50.0,
            zoom: 2.0,
            canvas_width: 800.0,
            canvas_height: 600.0,
            device_pixel_ratio: 1.0,
        };

        let via_viewport = vp.diagram_to_canvas(25.0, 40.0);
        let via_transform = vp.transform().apply(25.0, 40.0);

        // Compared within a tolerance rather than bit-for-bit. `transform()` composes rotors and
        // recovers a matrix through a 16-component geometric product, so it cannot reproduce
        // `diagram_to_canvas`'s two multiply-adds exactly — the residual here is a few ULP
        // (149.99999999999997 against 150.0). Demanding `assert_eq!` specified something the
        // rotor path cannot deliver; see the measured contract on bd-9mqa.
        //
        // 1e-9 is still four orders of magnitude tighter than any pixel could care about, and it
        // does NOT blunt this test: the ordering defect it caught (bd-qift) was off by 50.0 and
        // 20.0, so it would fail here by ten orders of magnitude.
        assert!(
            (via_transform.0 - via_viewport.0).abs() < 1e-9
                && (via_transform.1 - via_viewport.1).abs() < 1e-9,
            "rotor transform {via_transform:?} disagrees with direct mapping {via_viewport:?}"
        );
    }

    #[test]
    fn fit_small_diagram() {
        let vp = fit_to_viewport(100.0, 100.0, 800.0, 600.0, 20.0);
        // Small diagram should not zoom in beyond 100%
        assert!((vp.zoom - 1.0).abs() < 0.001);
    }

    #[test]
    fn fit_large_diagram() {
        let vp = fit_to_viewport(2000.0, 1000.0, 800.0, 600.0, 20.0);
        // Large diagram should zoom out to fit
        assert!(vp.zoom < 1.0);
    }

    #[test]
    fn excessive_padding_does_not_produce_negative_zoom() {
        let vp = fit_to_viewport(100.0, 100.0, 10.0, 10.0, 20.0);
        assert!(vp.zoom.is_finite());
        assert!(vp.zoom > 0.0);
        assert!(vp.zoom <= 1.0);
    }
}
