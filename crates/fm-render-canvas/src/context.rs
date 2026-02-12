//! Canvas2D context abstraction.
//!
//! Provides a trait-based abstraction over Canvas2D drawing operations,
//! allowing the renderer to work with both real web-sys contexts and
//! mock contexts for testing.

/// A 2D point for canvas operations.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[allow(dead_code)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[allow(dead_code)]
impl Point {
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A color representation for canvas operations.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f64,
}

#[allow(dead_code)]
impl Color {
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: f64) -> Self {
        Self { r, g, b, a }
    }

    #[must_use]
    pub fn to_css_string(&self) -> String {
        if (self.a - 1.0).abs() < f64::EPSILON {
            format!("rgb({},{},{})", self.r, self.g, self.b)
        } else {
            format!("rgba({},{},{},{})", self.r, self.g, self.b, self.a)
        }
    }

    /// Parse a CSS color string (basic support).
    #[must_use]
    pub fn from_css(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.starts_with('#') && s.len() == 7 {
            let r = u8::from_str_radix(&s[1..3], 16).ok()?;
            let g = u8::from_str_radix(&s[3..5], 16).ok()?;
            let b = u8::from_str_radix(&s[5..7], 16).ok()?;
            Some(Self::rgb(r, g, b))
        } else if s.starts_with('#') && s.len() == 4 {
            let r = u8::from_str_radix(&s[1..2], 16).ok()? * 17;
            let g = u8::from_str_radix(&s[2..3], 16).ok()? * 17;
            let b = u8::from_str_radix(&s[3..4], 16).ok()? * 17;
            Some(Self::rgb(r, g, b))
        } else {
            None
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::rgb(0, 0, 0)
    }
}

/// Text alignment options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

/// Text baseline options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextBaseline {
    Top,
    Middle,
    #[default]
    Alphabetic,
    Bottom,
}

impl TextBaseline {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Middle => "middle",
            Self::Alphabetic => "alphabetic",
            Self::Bottom => "bottom",
        }
    }
}

/// Line cap style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

impl LineCap {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Butt => "butt",
            Self::Round => "round",
            Self::Square => "square",
        }
    }
}

/// Line join style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

impl LineJoin {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Miter => "miter",
            Self::Round => "round",
            Self::Bevel => "bevel",
        }
    }
}

/// Text measurement result.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextMetrics {
    pub width: f64,
    pub height: f64,
}

/// Trait for Canvas2D-like drawing contexts.
///
/// This abstraction allows the renderer to work with both real Canvas2D
/// contexts (via web-sys) and mock contexts for testing.
pub trait Canvas2dContext {
    /// Get the canvas width.
    fn width(&self) -> f64;

    /// Get the canvas height.
    fn height(&self) -> f64;

    /// Save the current drawing state.
    fn save(&mut self);

    /// Restore the previously saved drawing state.
    fn restore(&mut self);

    /// Set the current fill style.
    fn set_fill_style(&mut self, color: &str);

    /// Set the current stroke style.
    fn set_stroke_style(&mut self, color: &str);

    /// Set the line width.
    fn set_line_width(&mut self, width: f64);

    /// Set the line cap style.
    fn set_line_cap(&mut self, cap: LineCap);

    /// Set the line join style.
    fn set_line_join(&mut self, join: LineJoin);

    /// Set the line dash pattern.
    fn set_line_dash(&mut self, pattern: &[f64]);

    /// Set the global alpha (transparency).
    fn set_global_alpha(&mut self, alpha: f64);

    /// Set the font.
    fn set_font(&mut self, font: &str);

    /// Set text alignment.
    fn set_text_align(&mut self, align: TextAlign);

    /// Set text baseline.
    fn set_text_baseline(&mut self, baseline: TextBaseline);

    /// Begin a new path.
    fn begin_path(&mut self);

    /// Close the current path.
    fn close_path(&mut self);

    /// Move to a point.
    fn move_to(&mut self, x: f64, y: f64);

    /// Draw a line to a point.
    fn line_to(&mut self, x: f64, y: f64);

    /// Draw a quadratic bezier curve.
    fn quadratic_curve_to(&mut self, cpx: f64, cpy: f64, x: f64, y: f64);

    /// Draw a cubic bezier curve.
    fn bezier_curve_to(&mut self, cp1x: f64, cp1y: f64, cp2x: f64, cp2y: f64, x: f64, y: f64);

    /// Draw an arc.
    fn arc(&mut self, x: f64, y: f64, radius: f64, start_angle: f64, end_angle: f64);

    /// Draw an arc with direction.
    fn arc_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, radius: f64);

    /// Draw a rectangle path.
    fn rect(&mut self, x: f64, y: f64, width: f64, height: f64);

    /// Fill the current path.
    fn fill(&mut self);

    /// Stroke the current path.
    fn stroke(&mut self);

    /// Fill a rectangle.
    fn fill_rect(&mut self, x: f64, y: f64, width: f64, height: f64);

    /// Stroke a rectangle.
    fn stroke_rect(&mut self, x: f64, y: f64, width: f64, height: f64);

    /// Clear a rectangle.
    fn clear_rect(&mut self, x: f64, y: f64, width: f64, height: f64);

    /// Fill text.
    fn fill_text(&mut self, text: &str, x: f64, y: f64);

    /// Stroke text.
    fn stroke_text(&mut self, text: &str, x: f64, y: f64);

    /// Measure text.
    fn measure_text(&self, text: &str) -> TextMetrics;

    /// Set the transform matrix.
    fn set_transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64);

    /// Reset the transform to identity.
    fn reset_transform(&mut self);

    /// Translate the canvas.
    fn translate(&mut self, x: f64, y: f64);

    /// Scale the canvas.
    fn scale(&mut self, x: f64, y: f64);

    /// Rotate the canvas.
    fn rotate(&mut self, angle: f64);

    /// Create a clipping region from the current path.
    fn clip(&mut self);

    /// Set shadow blur.
    fn set_shadow_blur(&mut self, blur: f64);

    /// Set shadow color.
    fn set_shadow_color(&mut self, color: &str);

    /// Set shadow offset.
    fn set_shadow_offset(&mut self, x: f64, y: f64);
}

/// A mock Canvas2D context for testing.
///
/// This records all drawing operations for verification.
#[derive(Debug, Clone)]
pub struct MockCanvas2dContext {
    width: f64,
    height: f64,
    operations: Vec<DrawOperation>,
    state_stack: Vec<DrawState>,
    current_state: DrawState,
}

/// A recorded drawing operation.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawOperation {
    Save,
    Restore,
    SetFillStyle(String),
    SetStrokeStyle(String),
    SetLineWidth(f64),
    BeginPath,
    ClosePath,
    MoveTo(f64, f64),
    LineTo(f64, f64),
    BezierCurveTo(f64, f64, f64, f64, f64, f64),
    Arc(f64, f64, f64, f64, f64),
    Rect(f64, f64, f64, f64),
    Fill,
    Stroke,
    FillRect(f64, f64, f64, f64),
    StrokeRect(f64, f64, f64, f64),
    ClearRect(f64, f64, f64, f64),
    FillText(String, f64, f64),
    Translate(f64, f64),
    Scale(f64, f64),
    Rotate(f64),
    Clip,
}

/// Drawing state for save/restore.
#[derive(Debug, Clone, Default)]
struct DrawState {
    fill_style: String,
    stroke_style: String,
    line_width: f64,
    font: String,
    text_align: TextAlign,
    text_baseline: TextBaseline,
    global_alpha: f64,
}

impl MockCanvas2dContext {
    /// Create a new mock context with the given dimensions.
    #[must_use]
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            operations: Vec::new(),
            state_stack: Vec::new(),
            current_state: DrawState {
                fill_style: String::from("#000000"),
                stroke_style: String::from("#000000"),
                line_width: 1.0,
                font: String::from("14px sans-serif"),
                text_align: TextAlign::Left,
                text_baseline: TextBaseline::Alphabetic,
                global_alpha: 1.0,
            },
        }
    }

    /// Get the recorded operations.
    #[must_use]
    pub fn operations(&self) -> &[DrawOperation] {
        &self.operations
    }

    /// Get the number of operations recorded.
    #[must_use]
    pub fn operation_count(&self) -> usize {
        self.operations.len()
    }

    /// Clear all recorded operations.
    pub fn clear(&mut self) {
        self.operations.clear();
    }
}

impl Canvas2dContext for MockCanvas2dContext {
    fn width(&self) -> f64 {
        self.width
    }

    fn height(&self) -> f64 {
        self.height
    }

    fn save(&mut self) {
        self.state_stack.push(self.current_state.clone());
        self.operations.push(DrawOperation::Save);
    }

    fn restore(&mut self) {
        if let Some(state) = self.state_stack.pop() {
            self.current_state = state;
        }
        self.operations.push(DrawOperation::Restore);
    }

    fn set_fill_style(&mut self, color: &str) {
        self.current_state.fill_style = color.to_string();
        self.operations
            .push(DrawOperation::SetFillStyle(color.to_string()));
    }

    fn set_stroke_style(&mut self, color: &str) {
        self.current_state.stroke_style = color.to_string();
        self.operations
            .push(DrawOperation::SetStrokeStyle(color.to_string()));
    }

    fn set_line_width(&mut self, width: f64) {
        self.current_state.line_width = width;
        self.operations.push(DrawOperation::SetLineWidth(width));
    }

    fn set_line_cap(&mut self, _cap: LineCap) {}
    fn set_line_join(&mut self, _join: LineJoin) {}
    fn set_line_dash(&mut self, _pattern: &[f64]) {}

    fn set_global_alpha(&mut self, alpha: f64) {
        self.current_state.global_alpha = alpha;
    }

    fn set_font(&mut self, font: &str) {
        self.current_state.font = font.to_string();
    }

    fn set_text_align(&mut self, align: TextAlign) {
        self.current_state.text_align = align;
    }

    fn set_text_baseline(&mut self, baseline: TextBaseline) {
        self.current_state.text_baseline = baseline;
    }

    fn begin_path(&mut self) {
        self.operations.push(DrawOperation::BeginPath);
    }

    fn close_path(&mut self) {
        self.operations.push(DrawOperation::ClosePath);
    }

    fn move_to(&mut self, x: f64, y: f64) {
        self.operations.push(DrawOperation::MoveTo(x, y));
    }

    fn line_to(&mut self, x: f64, y: f64) {
        self.operations.push(DrawOperation::LineTo(x, y));
    }

    fn quadratic_curve_to(&mut self, _cpx: f64, _cpy: f64, _x: f64, _y: f64) {}

    fn bezier_curve_to(&mut self, cp1x: f64, cp1y: f64, cp2x: f64, cp2y: f64, x: f64, y: f64) {
        self.operations
            .push(DrawOperation::BezierCurveTo(cp1x, cp1y, cp2x, cp2y, x, y));
    }

    fn arc(&mut self, x: f64, y: f64, radius: f64, start_angle: f64, end_angle: f64) {
        self.operations
            .push(DrawOperation::Arc(x, y, radius, start_angle, end_angle));
    }

    fn arc_to(&mut self, _x1: f64, _y1: f64, _x2: f64, _y2: f64, _radius: f64) {}

    fn rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.operations
            .push(DrawOperation::Rect(x, y, width, height));
    }

    fn fill(&mut self) {
        self.operations.push(DrawOperation::Fill);
    }

    fn stroke(&mut self) {
        self.operations.push(DrawOperation::Stroke);
    }

    fn fill_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.operations
            .push(DrawOperation::FillRect(x, y, width, height));
    }

    fn stroke_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.operations
            .push(DrawOperation::StrokeRect(x, y, width, height));
    }

    fn clear_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.operations
            .push(DrawOperation::ClearRect(x, y, width, height));
    }

    fn fill_text(&mut self, text: &str, x: f64, y: f64) {
        self.operations
            .push(DrawOperation::FillText(text.to_string(), x, y));
    }

    fn stroke_text(&mut self, _text: &str, _x: f64, _y: f64) {}

    fn measure_text(&self, text: &str) -> TextMetrics {
        // Estimate text width based on character count
        let char_width = 8.0;
        TextMetrics {
            width: text.len() as f64 * char_width,
            height: 14.0,
        }
    }

    fn set_transform(&mut self, _a: f64, _b: f64, _c: f64, _d: f64, _e: f64, _f: f64) {}
    fn reset_transform(&mut self) {}

    fn translate(&mut self, x: f64, y: f64) {
        self.operations.push(DrawOperation::Translate(x, y));
    }

    fn scale(&mut self, x: f64, y: f64) {
        self.operations.push(DrawOperation::Scale(x, y));
    }

    fn rotate(&mut self, angle: f64) {
        self.operations.push(DrawOperation::Rotate(angle));
    }

    fn clip(&mut self) {
        self.operations.push(DrawOperation::Clip);
    }

    fn set_shadow_blur(&mut self, _blur: f64) {}
    fn set_shadow_color(&mut self, _color: &str) {}
    fn set_shadow_offset(&mut self, _x: f64, _y: f64) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_context_records_operations() {
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);

        ctx.begin_path();
        ctx.move_to(10.0, 10.0);
        ctx.line_to(100.0, 100.0);
        ctx.stroke();

        assert_eq!(ctx.operation_count(), 4);
        assert_eq!(ctx.operations()[0], DrawOperation::BeginPath);
        assert_eq!(ctx.operations()[1], DrawOperation::MoveTo(10.0, 10.0));
    }

    #[test]
    fn mock_context_handles_save_restore() {
        let mut ctx = MockCanvas2dContext::new(800.0, 600.0);

        ctx.set_fill_style("#ff0000");
        ctx.save();
        ctx.set_fill_style("#00ff00");
        ctx.restore();

        assert!(
            ctx.operations()
                .contains(&DrawOperation::SetFillStyle("#ff0000".into()))
        );
    }

    #[test]
    fn color_from_css() {
        let color = Color::from_css("#ff0000").unwrap();
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);

        let color2 = Color::from_css("#f00").unwrap();
        assert_eq!(color2.r, 255);
    }
}
