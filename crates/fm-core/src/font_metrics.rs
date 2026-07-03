//! Deterministic font metrics for cross-platform text measurement.
//!
//! Provides a platform-independent way to measure text dimensions using
//! pre-computed character width tables. This ensures layout consistency
//! across different rendering targets (SVG, Canvas, Terminal).

use serde::{Deserialize, Serialize};

/// Invoke `f` on each line of `text`, byte-identically to [`str::lines`], but locate the `'\n'`
/// separators with a SIMD `memchr` scan instead of std's UTF-8-decoding `CharSearcher` (which
/// showed as ~2.5% of layout self-time on node-label sizing). Semantics match `str::lines`
/// exactly: split on `'\n'`; strip a single trailing `'\r'` only when it immediately precedes a
/// `'\n'` (a lone `'\r'` is kept); and emit no trailing empty segment when `text` ends in `'\n'`.
#[inline]
fn for_each_line(text: &str, mut f: impl FnMut(&str)) {
    let bytes = text.as_bytes();
    let mut start = 0usize;
    for nl in memchr::memchr_iter(b'\n', bytes) {
        let mut end = nl;
        if end > start && bytes[end - 1] == b'\r' {
            end -= 1;
        }
        // `'\n'`/`'\r'` are ASCII, so `start`/`end` land on char boundaries.
        f(&text[start..end]);
        start = nl + 1;
    }
    if start < bytes.len() {
        f(&text[start..]);
    }
}

/// Returns true for characters that occupy approximately 2 columns in
/// monospace/proportional fonts — CJK ideographs, fullwidth forms, and
/// common emoji. Based on UAX #11 East Asian Width property (W/F categories).
#[must_use]
pub const fn is_east_asian_wide(c: char) -> bool {
    let cp = c as u32;
    // Fast path: every East-Asian-wide range below begins at U+3000 or above, so any code
    // point under it (all ASCII / Latin / Cyrillic / … — the overwhelming majority of
    // label text) is not wide. This returns in a single comparison instead of walking the
    // ~14 range checks, which run for every non-special character during per-character
    // text-width measurement. Output-identical: the `matches!` is also false below U+3000.
    if cp < 0x3000 {
        return false;
    }
    matches!(cp,
        // CJK Unified Ideographs
        0x4E00..=0x9FFF
        // CJK Unified Ideographs Extension A
        | 0x3400..=0x4DBF
        // CJK Compatibility Ideographs
        | 0xF900..=0xFAFF
        // CJK Unified Ideographs Extension B+
        | 0x20000..=0x2FA1F
        // Hangul Syllables
        | 0xAC00..=0xD7AF
        // Fullwidth Forms
        | 0xFF01..=0xFF60
        | 0xFFE0..=0xFFE6
        // Katakana / Hiragana
        | 0x3040..=0x309F
        | 0x30A0..=0x30FF
        // CJK Symbols and Punctuation
        | 0x3000..=0x303F
        // Enclosed CJK Letters
        | 0x3200..=0x33FF
        // CJK Compatibility
        | 0xFE30..=0xFE4F
        // Common emoji (Miscellaneous Symbols + Dingbats + Emoticons + Transport)
        | 0x1F300..=0x1F9FF
        // Regional indicator symbols (flags)
        | 0x1F1E0..=0x1F1FF
    )
}

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
    /// Full-width: CJK ideographs, emoji, East Asian wide characters
    FullWidth,
}

impl CharWidthClass {
    /// Classify a character into a width class.
    #[must_use]
    pub const fn classify(c: char) -> Self {
        match c {
            'i' | 'l' | '|' | '!' | '\'' | '.' | ',' | ':' | ';' => Self::VeryNarrow,
            'I' | 'j' | 't' | 'f' | 'r' | '(' | ')' | '[' | ']' => Self::Narrow,
            ' ' => Self::Half,
            'w' | 'm' => Self::Wide,
            'W' | 'M' | '@' | '%' | '&' => Self::VeryWide,
            c if is_east_asian_wide(c) => Self::FullWidth,
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
            Self::FullWidth => 2.0,
        }
    }
}

/// Precomputed width multiplier per ASCII byte = `classify(byte).multiplier()`. A direct
/// `f32` table (no intermediate `CharWidthClass` enum) so per-char measurement on ASCII
/// text — the overwhelmingly common label case — is one array load + multiply instead of
/// the `classify`→`multiplier` match chain. Bit-identical to that chain: each entry is
/// exactly `classify(b as char).multiplier()`, computed at compile time from the same const
/// fns. (Unlike the rejected `CharWidthClass` lookup table, this stores the final multiplier,
/// so there is no intermediate enum load to break the compiler's match fusion.)
const ASCII_WIDTH_MULT: [f32; 128] = {
    let mut table = [0.0_f32; 128];
    let mut i = 0usize;
    while i < 128 {
        table[i] = CharWidthClass::classify(i as u8 as char).multiplier();
        i += 1;
    }
    table
};

/// Font metrics calculator for deterministic text measurement.
#[derive(Debug, Clone, PartialEq)]
pub struct FontMetrics {
    config: FontMetricsConfig,
    /// Computed average character width in pixels.
    avg_char_width: f32,
    /// Diagnostics collected during measurement.
    diagnostics: Vec<FontMetricsDiagnostic>,
}

/// Diagnostic information about font metric calculations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub fn new(mut config: FontMetricsConfig) -> Self {
        // Guard against zero or negative font size which would produce
        // zero-dimension layouts and potential division-by-zero downstream.
        if !config.font_size.is_finite() || config.font_size <= 0.0 {
            config.font_size = 15.0;
        }
        if !config.line_height.is_finite() || config.line_height <= 0.0 {
            config.line_height = 1.5;
        }
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
        // ASCII fast path (common labels, non-monospace): a direct multiplier-table load per
        // byte replaces the per-char `classify().multiplier()` match chain. Bit-identical —
        // `ASCII_WIDTH_MULT[b]` IS `classify(b as char).multiplier()`, and the `avg * mult`
        // op + left-to-right f32 sum are unchanged. Monospace / non-ASCII keep the char path.
        if self.config.preset != FontPreset::Monospace && text.is_ascii() {
            let avg = self.avg_char_width;
            return text
                .bytes()
                .map(|b| avg * ASCII_WIDTH_MULT[b as usize])
                .sum();
        }
        text.chars().map(|c| self.char_width(c)).sum()
    }

    fn char_width(&self, c: char) -> f32 {
        if self.config.preset == FontPreset::Monospace {
            self.avg_char_width
        } else {
            self.avg_char_width * CharWidthClass::classify(c).multiplier()
        }
    }

    /// Estimate the width of multi-line text (returns max line width).
    #[must_use]
    pub fn estimate_multiline_width(&self, text: &str) -> f32 {
        // `memchr`-split lines fed through the same left-to-right `f32::max` from a 0.0 seed
        // (empty text yields 0.0, matching `fold(0.0, f32::max)` on an empty `lines()`).
        let mut max_width = 0.0_f32;
        for_each_line(text, |line| max_width = max_width.max(self.estimate_width(line)));
        max_width
    }

    /// Get the height of a single line.
    #[must_use]
    pub fn line_height_px(&self) -> f32 {
        self.config.font_size * self.config.line_height
    }

    /// Estimate the height of text (multi-line aware).
    #[must_use]
    pub fn estimate_height(&self, text: &str) -> f32 {
        // `str::lines().count()` == number of `'\n'` plus one when the text is non-empty and
        // does not end in `'\n'` (no trailing empty line). Counting `'\n'` with `memchr` avoids
        // materializing/scanning line slices for the height-only path.
        let bytes = text.as_bytes();
        let newline_count = memchr::memchr_iter(b'\n', bytes).count();
        let trailing = usize::from(!bytes.is_empty() && bytes[bytes.len() - 1] != b'\n');
        let line_count = (newline_count + trailing).max(1);
        #[allow(clippy::cast_precision_loss)]
        let line_count_f32 = u32::try_from(line_count).unwrap_or(u32::MAX) as f32;
        line_count_f32 * self.line_height_px()
    }

    /// Estimate both width and height.
    #[must_use]
    pub fn estimate_dimensions(&self, text: &str) -> (f32, f32) {
        // Single `lines()` traversal for both axes: the split-out `estimate_multiline_width`
        // + `estimate_height` each scanned the whole string for '\n' independently, so every
        // node label was walked twice. Fusing folds max-width and line-count in one pass.
        // Byte-identical: same left-to-right `f32::max` over `estimate_width(line)` and the
        // same `lines().count().max(1)` height (an empty string still yields 1 line).
        let mut max_width = 0.0_f32;
        let mut line_count: u32 = 0;
        for_each_line(text, |line| {
            max_width = max_width.max(self.estimate_width(line));
            line_count = line_count.saturating_add(1);
        });
        #[allow(clippy::cast_precision_loss)]
        let line_count_f32 = line_count.max(1) as f32;
        (max_width, line_count_f32 * self.line_height_px())
    }

    /// Get the font size.
    #[must_use]
    pub const fn font_size(&self) -> f32 {
        self.config.font_size
    }

    /// Get the average character width.
    #[must_use]
    pub const fn avg_char_width(&self) -> f32 {
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
            let char_width = self.char_width(c);
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
        assert!((metrics.font_size() - 15.0).abs() < f32::EPSILON);
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

        // Monospace should have consistent width for all characters
        assert!((wide_width - narrow_width).abs() < f32::EPSILON);
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

        assert!((w1 - w2).abs() < f32::EPSILON);
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

    #[test]
    fn cjk_characters_classified_as_fullwidth() {
        // CJK Unified Ideographs
        assert_eq!(CharWidthClass::classify('中'), CharWidthClass::FullWidth);
        assert_eq!(CharWidthClass::classify('文'), CharWidthClass::FullWidth);
        assert_eq!(CharWidthClass::classify('字'), CharWidthClass::FullWidth);
        // Hiragana
        assert_eq!(CharWidthClass::classify('あ'), CharWidthClass::FullWidth);
        // Katakana
        assert_eq!(CharWidthClass::classify('ア'), CharWidthClass::FullWidth);
        // Hangul
        assert_eq!(CharWidthClass::classify('한'), CharWidthClass::FullWidth);
    }

    #[test]
    fn fullwidth_multiplier_is_two() {
        assert!((CharWidthClass::FullWidth.multiplier() - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cjk_text_is_wider_than_latin() {
        let metrics = FontMetrics::default();
        let latin = metrics.estimate_width("ABC");
        let cjk = metrics.estimate_width("中文字");
        assert!(
            cjk > latin,
            "CJK text ({cjk}) should be wider than Latin ({latin})"
        );
    }

    #[test]
    fn emoji_classified_as_fullwidth() {
        assert_eq!(CharWidthClass::classify('😀'), CharWidthClass::FullWidth);
        assert_eq!(CharWidthClass::classify('🎉'), CharWidthClass::FullWidth);
        assert_eq!(CharWidthClass::classify('🚀'), CharWidthClass::FullWidth);
    }
}
