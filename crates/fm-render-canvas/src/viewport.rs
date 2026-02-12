//! Viewport transformation for canvas rendering.
//!
//! Handles fitting diagrams to canvas dimensions with proper
//! padding, zoom, and pan support.

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

    /// Set the device pixel ratio for high-DPI displays.
    #[must_use]
    pub const fn with_dpr(mut self, dpr: f64) -> Self {
        self.device_pixel_ratio = dpr;
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
        ViewportTransform {
            a: self.zoom,
            b: 0.0,
            c: 0.0,
            d: self.zoom,
            e: self.offset_x,
            f: self.offset_y,
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
