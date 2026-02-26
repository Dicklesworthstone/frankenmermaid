//! SVG text rendering utilities.
//!
//! Provides `TextBuilder` for creating text elements with multi-line support,
//! and `TextMetrics` for estimating text dimensions.

use crate::element::Element;

/// Text anchor position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAnchor {
    /// Anchor at the start (left for LTR text).
    #[default]
    Start,
    /// Anchor at the middle (center).
    Middle,
    /// Anchor at the end (right for LTR text).
    End,
}

impl TextAnchor {
    /// Get the SVG attribute value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Middle => "middle",
            Self::End => "end",
        }
    }
}

/// Dominant baseline setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DominantBaseline {
    /// Auto (default browser behavior).
    #[default]
    Auto,
    /// Middle of the text.
    Middle,
    /// Central baseline.
    Central,
    /// Hanging baseline.
    Hanging,
    /// Alphabetic baseline.
    Alphabetic,
    /// Mathematical baseline.
    Mathematical,
}

impl DominantBaseline {
    /// Get the SVG attribute value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Middle => "middle",
            Self::Central => "central",
            Self::Hanging => "hanging",
            Self::Alphabetic => "alphabetic",
            Self::Mathematical => "mathematical",
        }
    }
}

/// Text metrics for estimating text dimensions.
///
/// Since we don't have access to font metrics in pure Rust without dependencies,
/// we use a character-count heuristic with configurable average character width.
#[derive(Debug, Clone)]
pub struct TextMetrics {
    /// Average character width in pixels.
    pub avg_char_width: f32,
    /// Font size in pixels.
    pub font_size: f32,
    /// Line height multiplier.
    pub line_height: f32,
}

impl Default for TextMetrics {
    fn default() -> Self {
        Self {
            avg_char_width: 8.0,
            font_size: 14.0,
            line_height: 1.4,
        }
    }
}

impl TextMetrics {
    /// Create new text metrics with the given parameters.
    #[must_use]
    pub fn new(avg_char_width: f32, font_size: f32, line_height: f32) -> Self {
        Self {
            avg_char_width,
            font_size,
            line_height,
        }
    }

    /// Estimate the width of a single line of text.
    #[must_use]
    pub fn estimate_width(&self, text: &str) -> f32 {
        // Count characters, weighting for typical character widths
        let mut width = 0.0;
        for c in text.chars() {
            width += match c {
                'W' | 'M' | '@' | '%' => self.avg_char_width * 1.5,
                'w' | 'm' => self.avg_char_width * 1.2,
                'i' | 'l' | '|' | '!' | '\'' | '.' | ',' => self.avg_char_width * 0.4,
                'I' | 'j' | 't' | 'f' => self.avg_char_width * 0.6,
                ' ' => self.avg_char_width * 0.5,
                _ => self.avg_char_width,
            };
        }
        width
    }

    /// Estimate the width of multi-line text (returns max line width).
    #[must_use]
    pub fn estimate_multiline_width(&self, text: &str) -> f32 {
        text.lines()
            .map(|line| self.estimate_width(line))
            .fold(0.0_f32, |acc, w| acc.max(w))
    }

    /// Estimate the height of a single line of text.
    #[must_use]
    pub fn line_height_px(&self) -> f32 {
        self.font_size * self.line_height
    }

    /// Estimate the height of multi-line text.
    #[must_use]
    pub fn estimate_height(&self, text: &str) -> f32 {
        let line_count = text.lines().count().max(1);
        line_count as f32 * self.line_height_px()
    }

    /// Estimate both width and height.
    #[must_use]
    pub fn estimate_dimensions(&self, text: &str) -> (f32, f32) {
        (
            self.estimate_multiline_width(text),
            self.estimate_height(text),
        )
    }
}

/// Builder for SVG text elements.
#[derive(Debug, Clone)]
pub struct TextBuilder {
    text: String,
    x: f32,
    y: f32,
    font_family: Option<String>,
    font_size: Option<f32>,
    font_weight: Option<String>,
    font_style: Option<String>,
    fill: Option<String>,
    anchor: TextAnchor,
    baseline: DominantBaseline,
    class: Option<String>,
    line_height: f32,
}

impl TextBuilder {
    /// Create a new text builder with the given text content.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            x: 0.0,
            y: 0.0,
            font_family: None,
            font_size: None,
            font_weight: None,
            font_style: None,
            fill: None,
            anchor: TextAnchor::Start,
            baseline: DominantBaseline::Auto,
            class: None,
            line_height: 1.4,
        }
    }

    /// Set the x position.
    #[must_use]
    pub fn x(mut self, x: f32) -> Self {
        self.x = x;
        self
    }

    /// Set the y position.
    #[must_use]
    pub fn y(mut self, y: f32) -> Self {
        self.y = y;
        self
    }

    /// Set the font family.
    #[must_use]
    pub fn font_family(mut self, family: &str) -> Self {
        self.font_family = Some(family.to_string());
        self
    }

    /// Set the font size.
    #[must_use]
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }

    /// Set the font weight.
    #[must_use]
    pub fn font_weight(mut self, weight: &str) -> Self {
        self.font_weight = Some(weight.to_string());
        self
    }

    /// Make the text bold.
    #[must_use]
    pub fn bold(self) -> Self {
        self.font_weight("bold")
    }

    /// Set the font style.
    #[must_use]
    pub fn font_style(mut self, style: &str) -> Self {
        self.font_style = Some(style.to_string());
        self
    }

    /// Make the text italic.
    #[must_use]
    pub fn italic(self) -> Self {
        self.font_style("italic")
    }

    /// Set the fill color.
    #[must_use]
    pub fn fill(mut self, color: &str) -> Self {
        self.fill = Some(color.to_string());
        self
    }

    /// Set the text anchor.
    #[must_use]
    pub fn anchor(mut self, anchor: TextAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Set the dominant baseline.
    #[must_use]
    pub fn baseline(mut self, baseline: DominantBaseline) -> Self {
        self.baseline = baseline;
        self
    }

    /// Set the CSS class.
    #[must_use]
    pub fn class(mut self, class: &str) -> Self {
        self.class = Some(class.to_string());
        self
    }

    /// Set the line height multiplier for multi-line text.
    #[must_use]
    pub fn line_height(mut self, height: f32) -> Self {
        self.line_height = height;
        self
    }

    /// Build the text element.
    #[must_use]
    pub fn build(self) -> Element {
        let lines: Vec<&str> = self.text.lines().collect();
        let font_size = self.font_size.unwrap_or(14.0);
        let line_height_px = font_size * self.line_height;

        let mut elem = Element::text()
            .x(self.x)
            .y(self.y)
            .attr("text-anchor", self.anchor.as_str());

        if self.baseline != DominantBaseline::Auto {
            elem = elem.attr("dominant-baseline", self.baseline.as_str());
        }

        if let Some(ref family) = self.font_family {
            elem = elem.attr("font-family", family);
        }

        if let Some(size) = self.font_size {
            elem = elem.attr_num("font-size", size);
        }

        if let Some(ref weight) = self.font_weight {
            elem = elem.attr("font-weight", weight);
        }

        if let Some(ref style) = self.font_style {
            elem = elem.attr("font-style", style);
        }

        if let Some(ref fill) = self.fill {
            elem = elem.fill(fill);
        }

        if let Some(ref class) = self.class {
            elem = elem.class(class);
        }

        if lines.len() <= 1 {
            // Single line text
            elem = elem.content(&self.text);
        } else {
            // Multi-line text using tspan elements
            for (i, line) in lines.iter().enumerate() {
                let dy = if i == 0 { 0.0 } else { line_height_px };
                let tspan = Element::tspan().x(self.x).attr_num("dy", dy).content(*line);
                elem = elem.child(tspan);
            }
        }

        elem
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_simple_text() {
        let elem = TextBuilder::new("Hello World").x(50.0).y(50.0).build();
        let svg = elem.render();
        assert!(svg.contains("<text"));
        assert!(svg.contains("x=\"50\""));
        assert!(svg.contains("y=\"50\""));
        assert!(svg.contains("Hello World"));
    }

    #[test]
    fn builds_styled_text() {
        let elem = TextBuilder::new("Styled")
            .font_family("Arial")
            .font_size(16.0)
            .bold()
            .fill("#333")
            .build();
        let svg = elem.render();
        assert!(svg.contains("font-family=\"Arial\""));
        assert!(svg.contains("font-size=\"16\""));
        assert!(svg.contains("font-weight=\"bold\""));
        assert!(svg.contains("fill=\"#333\""));
    }

    #[test]
    fn builds_centered_text() {
        let elem = TextBuilder::new("Centered")
            .anchor(TextAnchor::Middle)
            .build();
        let svg = elem.render();
        assert!(svg.contains("text-anchor=\"middle\""));
    }

    #[test]
    fn builds_multiline_text() {
        let elem = TextBuilder::new("Line 1\nLine 2\nLine 3")
            .x(10.0)
            .y(10.0)
            .build();
        let svg = elem.render();
        assert!(svg.contains("<tspan"));
        assert!(svg.contains("Line 1"));
        assert!(svg.contains("Line 2"));
        assert!(svg.contains("Line 3"));
    }

    #[test]
    fn escapes_special_characters() {
        let elem = TextBuilder::new("A & B < C > D").build();
        let svg = elem.render();
        assert!(svg.contains("A &amp; B &lt; C &gt; D"));
    }

    #[test]
    fn estimates_text_width() {
        let metrics = TextMetrics::default();
        let width = metrics.estimate_width("Hello");
        assert!(width > 0.0);

        // Wider characters should give larger width
        let wide = metrics.estimate_width("WWWWW");
        let narrow = metrics.estimate_width("iiiii");
        assert!(wide > narrow);
    }

    #[test]
    fn estimates_multiline_dimensions() {
        let metrics = TextMetrics::default();
        let (width, height) = metrics.estimate_dimensions("Line 1\nLine 2\nLine 3");
        assert!(width > 0.0);
        assert!(height > metrics.font_size * 2.0); // At least 2+ lines
    }
}
