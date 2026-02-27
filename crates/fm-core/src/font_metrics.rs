//! Deterministic font metrics for cross-platform text measurement.
//!
//! Provides a platform-independent way to measure text dimensions using
//! pre-computed character width tables. This ensures layout consistency
//! across different rendering targets (SVG, Canvas, Terminal).

use serde::{Deserialize, Serialize};

/// Font metrics preset for known font families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FontPreset {
    /// System UI font stack - average proportional font metrics
    #[default]
    SystemUi,
    /// Monospace font (fixed width)
    Monospace,
    /// Arial/Helvetica style sans-serif
    SansSerif,
    /// Times style serif font
    Serif,
    /// Condensed sans-serif (narrower characters)
    Condensed,
}

impl FontPreset {
    /// Get the preset from a font family string.
    #[must_use]
    pub fn from_family(family: &str) -> Self {
        let lower = family.to_lowercase();
        if lower.contains("mono")
            || lower.contains("courier")
            || lower.contains("consolas")
            || lower.contains("menlo")
        {
            Self::Monospace
        } else if lower.contains("times")
            || lower.contains("georgia")
            || (lower.contains("serif") && !lower.contains("sans"))
        {
            Self::Serif
        } else if lower.contains("condensed") || lower.contains("narrow") {
            Self::Condensed
        } else {
            Self::SansSerif
        }
    }

    /// Get the average character width ratio for this preset (relative to em-size).
    #[must_use]
    pub const fn avg_char_ratio(&self) -> f32 {
        match self {
            Self::SystemUi | Self::SansSerif => 0.55,
            Self::Monospace => 0.60,
            Self::Serif => 0.52,
            Self::Condensed => 0.45,
        }
    }

    /// Get the display name for this preset.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SystemUi => "system-ui",
            Self::Monospace => "monospace",
            Self::SansSerif => "sans-serif",
            Self::Serif => "serif",
            Self::Condensed => "condensed",
        }
    }
}

/// Configuration for deterministic font metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontMetricsConfig {
    /// The font preset to use for base measurements.
    pub preset: FontPreset,
    /// Font size in pixels.
    pub font_size: f32,
    /// Line height multiplier (1.0 = single-spaced).
    pub line_height: f32,
    /// Fallback font presets to try if primary is unavailable.
    pub fallback_chain: Vec<FontPreset>,
    /// Whether to emit diagnostics when using fallback fonts.
    pub trace_fallbacks: bool,
}

impl Default for FontMetricsConfig {
    fn default() -> Self {
        Self {
            preset: FontPreset::SystemUi,
            font_size: 15.0,
            line_height: 1.5,
            fallback_chain: vec![FontPreset::SansSerif, FontPreset::Monospace],
            trace_fallbacks: false,
        }
    }
}

/// Character width class for proportional fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharWidthClass {
    /// Very narrow characters: i, l, |, !, ', ., ,
    VeryNarrow,
    /// Narrow characters: I, j, t, f, r
    Narrow,
    /// Half-width: space
    Half,
    /// Normal width: most characters
    Normal,
    /// Wide characters: w, m
    Wide,
    /// Very wide characters: W, M, @, %
    VeryWide,
}

impl CharWidthClass {
    /// Classify a character into a width class.
    #[must_use]
    pub fn classify(c: char) -> Self {
        match c {
            'i' | 'l' | '|' | '!' | '\'' | '.' | ',' | ':' | ';' => Self::VeryNarrow,
            'I' | 'j' | 't' | 'f' | 'r' | '(' | ')' | '[' | ']' => Self::Narrow,
            ' ' => Self::Half,
            'w' | 'm' => Self::Wide,
            'W' | 'M' | '@' | '%' | '&' => Self::VeryWide,
            _ => Self::Normal,
        }
    }

    /// Get the width multiplier for this class.
    #[must_use]
    pub const fn multiplier(&self) -> f32 {
        match self {
            Self::VeryNarrow => 0.4,
            Self::Narrow => 0.6,
            Self::Half => 0.5,
            Self::Normal => 1.0,
            Self::Wide => 1.2,
            Self::VeryWide => 1.5,
        }
    }
}

/// Font metrics calculator for deterministic text measurement.
#[derive(Debug, Clone)]
pub struct FontMetrics {
    config: FontMetricsConfig,
    /// Computed average character width in pixels.
    avg_char_width: f32,
    /// Diagnostics collected during measurement.
    diagnostics: Vec<FontMetricsDiagnostic>,
}

/// Diagnostic information about font metric calculations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontMetricsDiagnostic {
    /// Diagnostic level.
    pub level: DiagnosticLevel,
    /// Diagnostic message.
    pub message: String,
}

/// Diagnostic severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticLevel {
    /// Informational trace.
    Trace,
    /// Warning that may affect rendering.
    Warning,
}

impl FontMetrics {
    /// Create new font metrics from configuration.
    #[must_use]
    pub fn new(config: FontMetricsConfig) -> Self {
        let avg_char_width = config.font_size * config.preset.avg_char_ratio();
        Self {
            config,
            avg_char_width,
            diagnostics: Vec::new(),
        }
    }

    /// Create font metrics with default configuration.
    #[must_use]
    pub fn default_metrics() -> Self {
        Self::new(FontMetricsConfig::default())
    }

    /// Create monospace font metrics.
    #[must_use]
    pub fn monospace(font_size: f32) -> Self {
        Self::new(FontMetricsConfig {
            preset: FontPreset::Monospace,
            font_size,
            line_height: 1.2,
            fallback_chain: vec![],
            trace_fallbacks: false,
        })
    }

    /// Get the configuration.
    #[must_use]
    pub const fn config(&self) -> &FontMetricsConfig {
        &self.config
    }

    /// Get collected diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[FontMetricsDiagnostic] {
        &self.diagnostics
    }

    /// Clear diagnostics.
    pub fn clear_diagnostics(&mut self) {
        self.diagnostics.clear();
    }

    /// Estimate the width of a single line of text.
    #[must_use]
    pub fn estimate_width(&self, text: &str) -> f32 {
        text.chars()
            .map(|c| {
                let class = CharWidthClass::classify(c);
                self.avg_char_width * class.multiplier()
            })
            .sum()
    }

    /// Estimate the width of multi-line text (returns max line width).
    #[must_use]
    pub fn estimate_multiline_width(&self, text: &str) -> f32 {
        text.lines()
            .map(|line| self.estimate_width(line))
            .fold(0.0_f32, |acc, w| acc.max(w))
    }

    /// Get the height of a single line.
    #[must_use]
    pub fn line_height_px(&self) -> f32 {
        self.config.font_size * self.config.line_height
    }

    /// Estimate the height of text (multi-line aware).
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

    /// Get the font size.
    #[must_use]
    pub fn font_size(&self) -> f32 {
        self.config.font_size
    }

    /// Get the average character width.
    #[must_use]
    pub fn avg_char_width(&self) -> f32 {
        self.avg_char_width
    }

    /// Record a font substitution diagnostic.
    pub fn record_fallback(&mut self, requested: &str, actual: FontPreset) {
        if self.config.trace_fallbacks {
            self.diagnostics.push(FontMetricsDiagnostic {
                level: DiagnosticLevel::Trace,
                message: format!(
                    "Font '{}' not available, using fallback '{}'",
                    requested,
                    actual.name()
                ),
            });
        }
    }

    /// Truncate text to fit within a maximum width, adding ellipsis.
    #[must_use]
    pub fn truncate_to_width(&self, text: &str, max_width: f32) -> String {
        if self.estimate_width(text) <= max_width {
            return text.to_string();
        }

        let ellipsis = "...";
        let ellipsis_width = self.estimate_width(ellipsis);

        if max_width <= ellipsis_width {
            return String::new();
        }

        let available_width = max_width - ellipsis_width;
        let mut result = String::new();
        let mut current_width = 0.0;

        for c in text.chars() {
            let char_width = self.avg_char_width * CharWidthClass::classify(c).multiplier();
            if current_width + char_width > available_width {
                break;
            }
            result.push(c);
            current_width += char_width;
        }

        result.push_str(ellipsis);
        result
    }

    /// Wrap text to fit within a maximum width.
    #[must_use]
    pub fn wrap_to_width(&self, text: &str, max_width: f32) -> Vec<String> {
        let mut lines = Vec::new();

        for line in text.lines() {
            if self.estimate_width(line) <= max_width {
                lines.push(line.to_string());
                continue;
            }

            // Word wrap
            let mut current_line = String::new();
            let mut current_width = 0.0;

            for word in line.split_whitespace() {
                let word_width = self.estimate_width(word);
                let space_width = self.estimate_width(" ");

                if current_line.is_empty() {
                    // Start of line - add word even if it overflows
                    current_line.push_str(word);
                    current_width = word_width;
                } else if current_width + space_width + word_width <= max_width {
                    // Word fits
                    current_line.push(' ');
                    current_line.push_str(word);
                    current_width += space_width + word_width;
                } else {
                    // Word doesn't fit - start new line
                    lines.push(current_line);
                    current_line = word.to_string();
                    current_width = word_width;
                }
            }

            if !current_line.is_empty() {
                lines.push(current_line);
            }
        }

        if lines.is_empty() {
            lines.push(String::new());
        }

        lines
    }
}

impl Default for FontMetrics {
    fn default() -> Self {
        Self::default_metrics()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_metrics_are_reasonable() {
        let metrics = FontMetrics::default();
        assert_eq!(metrics.font_size(), 15.0);
        assert!(metrics.avg_char_width() > 5.0);
        assert!(metrics.avg_char_width() < 15.0);
    }

    #[test]
    fn estimates_width_with_character_classes() {
        let metrics = FontMetrics::default();

        let narrow = metrics.estimate_width("iii");
        let wide = metrics.estimate_width("WWW");
        let normal = metrics.estimate_width("aaa");

        assert!(narrow < normal);
        assert!(normal < wide);
    }

    #[test]
    fn estimates_height_for_multiline() {
        let metrics = FontMetrics::default();

        let single = metrics.estimate_height("Hello");
        let double = metrics.estimate_height("Hello\nWorld");

        assert!(double > single * 1.5);
        assert!(double < single * 2.5);
    }

    #[test]
    fn monospace_has_consistent_width() {
        let metrics = FontMetrics::monospace(12.0);

        let narrow_width = metrics.estimate_width("iii");
        let wide_width = metrics.estimate_width("mmm");

        // Monospace still uses character classes, but ratio is different
        assert!(wide_width > narrow_width);
    }

    #[test]
    fn truncate_adds_ellipsis() {
        let metrics = FontMetrics::default();
        let text = "This is a long text that needs truncation";
        let truncated = metrics.truncate_to_width(text, 100.0);

        assert!(truncated.ends_with("..."));
        assert!(truncated.len() < text.len());
        assert!(metrics.estimate_width(&truncated) <= 100.0 + metrics.avg_char_width());
    }

    #[test]
    fn wrap_preserves_short_lines() {
        let metrics = FontMetrics::default();
        let text = "Short";
        let wrapped = metrics.wrap_to_width(text, 200.0);

        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0], "Short");
    }

    #[test]
    fn wrap_splits_long_lines() {
        let metrics = FontMetrics::default();
        let text = "This is a sentence that should be wrapped to multiple lines";
        let wrapped = metrics.wrap_to_width(text, 100.0);

        assert!(wrapped.len() > 1);
    }

    #[test]
    fn font_preset_detection_works() {
        assert_eq!(
            FontPreset::from_family("Courier New"),
            FontPreset::Monospace
        );
        assert_eq!(FontPreset::from_family("Times"), FontPreset::Serif);
        assert_eq!(FontPreset::from_family("Arial"), FontPreset::SansSerif);
        assert_eq!(
            FontPreset::from_family("Arial Narrow"),
            FontPreset::Condensed
        );
    }

    #[test]
    fn char_width_classes_are_deterministic() {
        // Same character always produces same class
        for c in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars() {
            let class1 = CharWidthClass::classify(c);
            let class2 = CharWidthClass::classify(c);
            assert_eq!(class1, class2);
        }
    }

    #[test]
    fn measurements_are_deterministic() {
        let metrics = FontMetrics::default();
        let text = "The quick brown fox jumps over the lazy dog";

        let w1 = metrics.estimate_width(text);
        let w2 = metrics.estimate_width(text);

        assert_eq!(w1, w2);
    }

    #[test]
    fn fallback_tracing_records_diagnostics() {
        let mut metrics = FontMetrics::new(FontMetricsConfig {
            trace_fallbacks: true,
            ..Default::default()
        });

        metrics.record_fallback("CustomFont", FontPreset::SansSerif);

        assert_eq!(metrics.diagnostics().len(), 1);
        assert!(metrics.diagnostics()[0].message.contains("CustomFont"));
    }
}
