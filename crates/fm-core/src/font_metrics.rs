//! Deterministic font metrics for cross-platform text measurement.
//!
//! Provides a platform-independent way to measure text dimensions using
//! pre-computed character width tables. This ensures layout consistency
//! across different rendering targets (SVG, Canvas, Terminal).

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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

/// Split at whitespace without detaching a mark from a whitespace-based grapheme. Ordinary
/// whitespace still separates words; a space carrying a combining mark remains authored text.
fn grapheme_words(text: &str) -> impl Iterator<Item = &str> {
    let mut graphemes = text.grapheme_indices(true);
    std::iter::from_fn(move || {
        let (start, _) =
            graphemes.find(|(_, grapheme)| !grapheme.chars().all(char::is_whitespace))?;
        for (offset, grapheme) in graphemes.by_ref() {
            if grapheme.chars().all(char::is_whitespace) {
                return Some(&text[start..offset]);
            }
        }
        Some(&text[start..])
    })
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

/// Whether a code point renders with zero advance width.
///
/// These characters draw nothing of their own: a combining mark composes onto the preceding base
/// glyph, a variation selector picks a presentation form, a skin-tone modifier recolours the
/// preceding emoji, and the zero-width space/joiner family are invisible formatting controls. Billing
/// them as full characters sizes a node box for glyphs that are never drawn — and because `Café` has
/// two Unicode spellings (`caf\u{e9}` and `cafe\u{301}`), it also makes the SAME label measure two
/// different widths depending on how the author's editor normalised the file.
///
/// ⚠️ THIS MUST BE TESTED BEFORE [`is_east_asian_wide`] IN [`CharWidthClass::classify`]. The skin-tone
/// modifiers `U+1F3FB..=U+1F3FF` sit INSIDE the `0x1F300..=0x1F9FF` emoji block that
/// `is_east_asian_wide` reports as full-width, so checking wide-ness first bills a zero-width
/// modifier at the widest multiplier there is.
///
/// This compact classification remains available to callers and the const ASCII table. Actual
/// text measurement also consults Unicode width tables, and wrapping uses extended grapheme
/// boundaries, so an unlisted script or emoji sequence does not depend on this curated list.
#[must_use]
pub const fn is_zero_width(c: char) -> bool {
    let cp = c as u32;
    // Fast path, mirroring `is_east_asian_wide`: every range below starts at U+0300, so all ASCII
    // and Latin-1 text answers in one comparison instead of walking the list. The ASCII width table
    // is built from `classify`, so this must stay false below 0x80 for that table to be unchanged.
    if cp < 0x0300 {
        return false;
    }
    matches!(cp,
        // Combining Diacritical Marks (and its supplement/extended/symbol blocks)
        0x0300..=0x036F
        | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF
        | 0x20D0..=0x20F0
        // Combining marks for Cyrillic, Hebrew, Arabic and Devanagari
        | 0x0483..=0x0489
        | 0x0591..=0x05BD
        | 0x0610..=0x061A
        | 0x064B..=0x065F
        | 0x0670
        | 0x0900..=0x0903
        | 0x093A..=0x093C
        | 0x0941..=0x0948
        | 0x094D
        // Zero-width space, ZWNJ, ZWJ, and the bidi marks
        | 0x200B..=0x200F
        // Combining Half Marks
        | 0xFE20..=0xFE2F
        // Variation selectors: presentation form of the PRECEDING character
        | 0xFE00..=0xFE0F
        // Emoji skin-tone modifiers — inside the full-width emoji block, hence the ordering note
        | 0x1F3FB..=0x1F3FF
        // Emoji tag sequences select a subdivision flag without adding visible glyphs.
        | 0xE0020..=0xE007F
        // Variation Selectors Supplement
        | 0xE0100..=0xE01EF
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
    /// Zero-width: combining marks, variation selectors, joiners — they draw onto or modify the
    /// preceding glyph and advance the pen by nothing. See [`is_zero_width`].
    ZeroWidth,
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
            // BEFORE the wide check: the skin-tone modifiers live inside the emoji block that
            // `is_east_asian_wide` matches, so testing wide-ness first bills them at 2.0.
            c if is_zero_width(c) => Self::ZeroWidth,
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
            Self::ZeroWidth => 0.0,
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

    /// Resolve authored font properties before measuring or positioning text.
    ///
    /// Sizes use the same pixel interpretation as diagram styles. Unsupported or invalid
    /// sizes retain the caller's font; a family-only override retains its size and line height.
    /// `None` keeps the common unstyled path borrowing its existing metrics.
    #[must_use]
    pub fn with_style_overrides(
        &self,
        font_size: Option<&str>,
        font_family: Option<&str>,
    ) -> Option<Self> {
        let font_size = font_size.and_then(|raw| {
            raw.trim()
                .trim_end_matches("px")
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|size| size.is_finite() && *size > 0.0 && *size <= 512.0)
        });
        let font_family = font_family
            .map(str::trim)
            .filter(|family| !family.is_empty());
        if font_size.is_none() && font_family.is_none() {
            return None;
        }
        Some(Self::new(FontMetricsConfig {
            font_size: font_size.unwrap_or(self.config.font_size),
            preset: font_family.map_or(self.config.preset, FontPreset::from_family),
            ..self.config.clone()
        }))
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
        // Keep the established ASCII estimates and their left-to-right accumulation. Unicode
        // needs complete graphemes: a flag, keycap, or joined emoji has one advance even though
        // it contains several characters, and a combining mark never adds a monospace cell.
        if text.is_ascii() {
            if self.config.preset != FontPreset::Monospace {
                let avg = self.avg_char_width;
                return text
                    .bytes()
                    .map(|b| avg * ASCII_WIDTH_MULT[b as usize])
                    .sum();
            }
            return text.bytes().map(|_| self.avg_char_width).sum();
        }

        text.graphemes(true)
            .map(|grapheme| self.grapheme_width(grapheme))
            .sum()
    }

    fn char_width(&self, c: char) -> f32 {
        if c.is_ascii() {
            return if self.config.preset == FontPreset::Monospace {
                self.avg_char_width
            } else {
                self.avg_char_width * ASCII_WIDTH_MULT[c as usize]
            };
        }
        // Scalar consumers such as the GPU glyph atlas place modifiers over the preceding
        // glyph. Preserve that zero advance; a complete emoji sequence is measured below using
        // UnicodeWidthStr, which accounts for the base and modifier together.
        if matches!(c, '\u{1f3fb}'..='\u{1f3ff}') {
            return 0.0;
        }

        let columns = UnicodeWidthChar::width(c).unwrap_or(0);
        if columns == 0 {
            return 0.0;
        }
        if self.config.preset == FontPreset::Monospace || columns == 2 {
            return self.avg_char_width * columns as f32;
        }
        // The public character classes are a compact approximation. The Unicode tables decide
        // whether an unlisted combining mark is invisible or a newer emoji is wide; proportional
        // letter shapes keep the existing narrow/wide calibration.
        let multiplier = match CharWidthClass::classify(c) {
            CharWidthClass::FullWidth | CharWidthClass::ZeroWidth => 1.0,
            class => class.multiplier(),
        };
        self.avg_char_width * multiplier
    }

    fn grapheme_width(&self, grapheme: &str) -> f32 {
        if grapheme.is_ascii() {
            return self.estimate_width(grapheme);
        }
        let columns = UnicodeWidthStr::width(grapheme);
        let mut scalar_columns = 0usize;
        let mut measured = 0.0;
        for character in grapheme.chars() {
            scalar_columns += UnicodeWidthChar::width(character).unwrap_or(0);
            measured += self.char_width(character);
        }
        if columns != scalar_columns {
            // Unicode presentation and ligature rules are sequence-aware. Summing scalar widths
            // overcounts flags/ZWJ/modifier sequences and undercounts emoji keycaps and VS16.
            self.avg_char_width * columns as f32
        } else {
            measured
        }
    }

    /// Estimate the width of multi-line text (returns max line width).
    #[must_use]
    pub fn estimate_multiline_width(&self, text: &str) -> f32 {
        // `memchr`-split lines fed through the same left-to-right `f32::max` from a 0.0 seed
        // (empty text yields 0.0, matching `fold(0.0, f32::max)` on an empty `lines()`).
        let mut max_width = 0.0_f32;
        for_each_line(text, |line| {
            max_width = max_width.max(self.estimate_width(line))
        });
        max_width
    }

    /// Conservative width for labels whose complete text must remain visible.
    ///
    /// `avg_char_width` is the caller's calibration at this metrics instance's base font size.
    /// Keep a larger calibration, account for wide Latin glyphs, and reserve one eighth for
    /// differences between the preset and the font face actually used by the renderer.
    #[must_use]
    pub fn estimate_preserved_width(&self, text: &str, font_size: f32, avg_char_width: f32) -> f32 {
        let scale = font_size / self.font_size().max(1.0);
        let average = avg_char_width.max(self.avg_char_width) * scale;
        text.lines()
            .map(|line| {
                let wide_allowance: f32 = line
                    .chars()
                    .map(|character| match CharWidthClass::classify(character) {
                        CharWidthClass::VeryWide => (font_size - average * 1.5).max(0.0),
                        CharWidthClass::Wide => (font_size - average * 1.2).max(0.0),
                        _ => 0.0,
                    })
                    .sum();
                (self.estimate_width(line) * (average / self.avg_char_width) + wide_allowance)
                    * 1.125
            })
            .fold(0.0, f32::max)
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
    ///
    /// Only complete extended graphemes are retained. A non-positive or NaN width permits no
    /// text; positive infinity is unbounded. The ellipsis itself must fit the requested width.
    #[must_use]
    pub fn truncate_to_width(&self, text: &str, max_width: f32) -> String {
        if max_width.is_nan() || max_width <= 0.0 {
            return String::new();
        }
        if self.estimate_width(text) <= max_width {
            return text.to_string();
        }

        let ellipsis = "...";
        let ellipsis_width = self.estimate_width(ellipsis);

        if max_width < ellipsis_width {
            return String::new();
        }

        let mut prefix_end = 0;
        let mut current_width = 0.0;
        let dot_width = self.char_width('.');
        for (offset, grapheme) in text.grapheme_indices(true) {
            let next_width = current_width + self.grapheme_width(grapheme);
            // Add the suffix in drawing order, rather than subtracting it from the budget.
            // Near an f32 boundary those operations can disagree and admit an overflowing label.
            let with_ellipsis = ((next_width + dot_width) + dot_width) + dot_width;
            if with_ellipsis > max_width {
                break;
            }
            prefix_end = offset + grapheme.len();
            current_width = next_width;
        }

        let mut result = text[..prefix_end].to_string();
        result.push_str(ellipsis);
        // Per-cluster sums can round differently from measuring the final string. Enforce
        // the actual budget at a boundary without ever cutting through the retained cluster.
        while prefix_end > 0 && self.estimate_width(&result) > max_width {
            prefix_end = text[..prefix_end]
                .grapheme_indices(true)
                .next_back()
                .map_or(0, |(offset, _)| offset);
            result.truncate(prefix_end);
            result.push_str(ellipsis);
        }
        result
    }

    /// Wrap complete text without collapsing spaces or splitting joined glyphs.
    ///
    /// Rows are slices of the authored lines, so concatenating the rows for one source line
    /// recovers that line exactly. Widths use the conservative measurement used by card layout.
    /// A single indivisible glyph may exceed the requested width, but is never truncated.
    #[must_use]
    pub fn wrap_preserved_to_width(&self, text: &str, max_width: f32) -> Vec<String> {
        let mut rows = Vec::new();
        let mut authored_lines = text.split('\n').peekable();
        while let Some(line) = authored_lines.next() {
            let line = if authored_lines.peek().is_some() {
                line.strip_suffix('\r').unwrap_or(line)
            } else {
                line
            };
            if line.is_empty() || !max_width.is_finite() || max_width <= 0.0 {
                rows.push(line.to_owned());
                continue;
            }
            // Width classifications cannot determine legal break positions: spacing marks,
            // Indic conjuncts, Hangul jamo, and emoji tags also belong to complete clusters.
            let mut boundaries: Vec<_> = line
                .grapheme_indices(true)
                .map(|(offset, _)| offset)
                .collect();
            boundaries.push(line.len());
            let mut start = 0;
            while start + 1 < boundaries.len() {
                let mut end = start + 1;
                let mut word_break = None;
                while end < boundaries.len() {
                    let candidate = &line[boundaries[start]..boundaries[end]];
                    let width = self.estimate_preserved_width(
                        candidate,
                        self.font_size(),
                        self.avg_char_width(),
                    );
                    if width > max_width && end > start + 1 {
                        break;
                    }
                    let last_grapheme = &line[boundaries[end - 1]..boundaries[end]];
                    if last_grapheme.starts_with(char::is_whitespace) {
                        word_break = Some(end);
                    }
                    end += 1;
                }
                let end = if end == boundaries.len() {
                    end - 1
                } else {
                    word_break.unwrap_or(end - 1)
                };
                rows.push(line[boundaries[start]..boundaries[end]].to_owned());
                start = end;
            }
        }
        rows
    }

    /// Balance a short paragraph without increasing its row count or widest row.
    ///
    /// Greedy wrapping can leave a single word below an almost full-width line. Choosing the
    /// word boundaries together gives a more compact, evenly filled label while preserving every
    /// character. Authored line breaks and words that already need glyph-level splitting retain
    /// the preserved wrapping above. The bounded search is reserved for ordinary node captions;
    /// unusually long prose keeps the same complete, width-bounded fallback.
    #[must_use]
    pub fn wrap_balanced_to_width(&self, text: &str, max_width: f32) -> Vec<String> {
        const MAX_ROWS: usize = 8;
        const MAX_WORDS: usize = 96;
        const WIDTH_TOLERANCE: f32 = 0.01;

        let original = self.wrap_preserved_to_width(text, max_width);
        let row_count = original.len();
        if !(2..=MAX_ROWS).contains(&row_count)
            || !max_width.is_finite()
            || max_width <= 0.0
            || text.contains('\n')
            || text.contains('\r')
        {
            return original;
        }

        // Break only after complete whitespace runs, keeping those spaces with their word.
        // A combining mark attached to a space stays with that space rather than becoming the
        // first glyph of a new line. No candidate can split a word, emoji sequence, or flag pair.
        let mut boundaries = vec![0];
        let mut after_space = false;
        let mut seen_word = false;
        for (offset, grapheme) in text.grapheme_indices(true) {
            let space = grapheme.starts_with(char::is_whitespace);
            if after_space && !space && seen_word {
                boundaries.push(offset);
                if boundaries.len() > MAX_WORDS {
                    return original;
                }
            }
            seen_word |= !space;
            after_space = space;
        }
        boundaries.push(text.len());
        let word_count = boundaries.len() - 1;
        if word_count < row_count {
            return original;
        }

        let measure = |line: &str| {
            self.estimate_preserved_width(line, self.font_size(), self.avg_char_width())
        };
        let widest = original
            .iter()
            .map(|line| measure(line))
            .fold(0.0, f32::max);
        let width_limit = max_width.min(widest);
        let mut prefix_widths = Vec::with_capacity(boundaries.len());
        prefix_widths.push(0.0_f64);
        for pair in boundaries.windows(2) {
            let width = measure(&text[pair[0]..pair[1]]);
            // A word wider than the existing column needs the glyph-safe fallback, not a
            // narrower column that introduces a different split through that word.
            if !width.is_finite() || width > width_limit + WIDTH_TOLERANCE {
                return original;
            }
            prefix_widths.push(prefix_widths.last().copied().unwrap_or(0.0) + f64::from(width));
        }
        let target_width = prefix_widths[word_count] / row_count as f64;
        let stride = word_count + 1;
        let mut costs = vec![f64::INFINITY; (row_count + 1) * stride];
        let mut starts = vec![usize::MAX; costs.len()];
        costs[0] = 0.0;

        // Minimize raggedness across the fixed number of rows. Stable iteration order resolves
        // equal costs in favour of a fuller earlier line, with no clock or platform heuristics.
        for row in 1..=row_count {
            for end in row..=word_count - (row_count - row) {
                let cell = row * stride + end;
                for start in (row - 1..end).rev() {
                    let width = prefix_widths[end] - prefix_widths[start];
                    if width > f64::from(width_limit + WIDTH_TOLERANCE) {
                        break;
                    }
                    let deviation = width - target_width;
                    let cost = costs[(row - 1) * stride + start] + deviation * deviation;
                    if cost < costs[cell] {
                        costs[cell] = cost;
                        starts[cell] = start;
                    }
                }
            }
        }

        let mut balanced = Vec::with_capacity(row_count);
        let mut end = word_count;
        for row in (1..=row_count).rev() {
            let start = starts[row * stride + end];
            if start == usize::MAX {
                return original;
            }
            balanced.push(text[boundaries[start]..boundaries[end]].to_owned());
            end = start;
        }
        balanced.reverse();

        // Prefix sums avoid repeatedly measuring every candidate substring. Check the selected
        // rows with the actual text metric as well, so floating-point accumulation cannot admit
        // a wider card or a less balanced result at a width boundary.
        if balanced
            .iter()
            .any(|line| measure(line) > width_limit + WIDTH_TOLERANCE)
        {
            return original;
        }
        let raggedness = |lines: &[String]| {
            lines
                .iter()
                .map(|line| {
                    let deviation = f64::from(measure(line)) - target_width;
                    deviation * deviation
                })
                .sum::<f64>()
        };
        if raggedness(&balanced) + f64::from(WIDTH_TOLERANCE) < raggedness(&original) {
            balanced
        } else {
            original
        }
    }

    /// Wrap text at word boundaries, retaining authored line breaks and every word.
    ///
    /// A word wider than the available space is split without truncation. Combining marks,
    /// joined emoji, and flag pairs stay together, even when one glyph alone exceeds the width.
    /// Non-positive or non-finite widths leave the authored lines unchanged.
    #[must_use]
    pub fn wrap_to_width(&self, text: &str, max_width: f32) -> Vec<String> {
        let mut lines = Vec::new();

        let mut authored_lines = text.split('\n').peekable();
        while let Some(line) = authored_lines.next() {
            // Normalize CRLF as `str::lines` did, while preserving blank/trailing rows. A final
            // standalone CR is authored content, so only strip CR when a newline follows it.
            let line = if authored_lines.peek().is_some() {
                line.strip_suffix('\r').unwrap_or(line)
            } else {
                line
            };
            if !max_width.is_finite() || max_width <= 0.0 || self.estimate_width(line) <= max_width
            {
                lines.push(line.to_string());
                continue;
            }

            let mut current_line = String::new();
            let mut current_width = 0.0;
            let space_width = self.estimate_width(" ");

            // Collapse only complete whitespace clusters. `split_whitespace()` could detach
            // a combining mark from an authored space before the glyph-safe wrapper saw it.
            for word in grapheme_words(line) {
                let word_width = self.estimate_width(word);
                if !current_line.is_empty() && current_width + space_width + word_width <= max_width
                {
                    current_line.push(' ');
                    current_line.push_str(word);
                    current_width += space_width + word_width;
                    continue;
                }

                if !current_line.is_empty() {
                    lines.push(std::mem::take(&mut current_line));
                }
                current_width = 0.0;
                let mut start = 0;
                for (offset, grapheme) in word.grapheme_indices(true) {
                    let advance = self.grapheme_width(grapheme);
                    let can_break = offset > start && current_width > 0.0 && advance > 0.0;
                    if can_break && current_width + advance > max_width {
                        lines.push(word[start..offset].to_string());
                        start = offset;
                        current_width = 0.0;
                    }
                    current_width += advance;
                }
                current_line.push_str(&word[start..]);
            }

            // Empty authored lines still occupy a line in the note's layout.
            lines.push(current_line);
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

    /// One non-ASCII character must not cost a label its width table, and the mixed-script path
    /// must agree with the per-character reference BIT for BIT.
    ///
    /// Exact equality is the right assertion, not an epsilon: node sizes are derived from this
    /// number and every committed golden encodes the resulting geometry, so a one-ulp drift here
    /// is a corpus-wide diff. `to_bits()` is what makes "bit-identical" checkable rather than
    /// asserted in a comment.
    #[test]
    fn mixed_script_width_matches_the_per_char_reference_bit_for_bit() {
        let metrics = FontMetrics::default_metrics();
        let reference = |text: &str| -> f32 {
            text.chars()
                .map(|c| metrics.avg_char_width() * CharWidthClass::classify(c).multiplier())
                .sum()
        };

        for text in [
            "Cafe\u{301}",          // combining accent: ASCII base, non-ASCII mark
            "naïve façade",         // mostly ASCII with two accents — the docs_site_50 shape
            "日本語ラベル",         // full-width, no ASCII at all
            "Ünicode — em dash",    // leading non-ASCII plus a wide punctuation char
            "mixed 日本 and ASCII", // alternating runs
            "ASCII only",           // pure ASCII: must still take the byte path
            "",                     // empty
            "é",                    // single non-ASCII char
        ] {
            assert_eq!(
                metrics.estimate_width(text).to_bits(),
                reference(text).to_bits(),
                "width drifted for {text:?}: got {} want {}",
                metrics.estimate_width(text),
                reference(text)
            );
        }
    }

    /// Monospace Latin letters retain equal advances, while CJK occupies two cells. Counting
    /// every scalar as one cell underestimated wide text and charged invisible marks as letters.
    #[test]
    fn monospace_width_uses_display_cells_without_proportional_letter_classes() {
        let metrics = FontMetrics::monospace(14.0);
        for text in ["abc", "iii", "WWW", "aéz"] {
            let expected: f32 = [metrics.avg_char_width(); 3].into_iter().sum();
            assert_eq!(
                metrics.estimate_width(text).to_bits(),
                expected.to_bits(),
                "ordinary monospace advances changed for {text:?}"
            );
        }
        let whole_cells = FontMetrics::monospace(10.0);
        assert_eq!(whole_cells.estimate_width("日本語"), 36.0);
        assert_eq!(whole_cells.estimate_width("aé日"), 24.0);
    }
    use super::*;

    #[test]
    fn unicode_widths_keep_combining_marks_and_emoji_sequences_together() {
        for metrics in [FontMetrics::default_metrics(), FontMetrics::monospace(10.0)] {
            for modifier in ["\u{301}", "\u{200d}", "\u{fe0f}", "🏽"] {
                assert_eq!(
                    metrics.estimate_width(modifier),
                    0.0,
                    "scalar modifier consumers must not advance for {modifier:?}"
                );
            }
            for (composed, base) in [
                ("e\u{301}", "e"),
                ("א\u{5c7}", "א"),
                ("ก\u{e31}", "ก"),
                ("a\u{200d}b", "ab"),
                ("a\u{200d}界", "a界"),
                ("W\u{fe0f}", "W"),
                ("i\u{e0100}", "i"),
                ("\u{1100}\u{1161}\u{11a8}", "각"),
            ] {
                assert_eq!(
                    metrics.estimate_width(composed),
                    metrics.estimate_width(base),
                    "invisible marks or decomposed glyphs changed {composed:?}"
                );
            }
            let wide = metrics.estimate_width("界");
            for glyph in [
                "👨‍👩‍👧‍👦",
                "👍🏽",
                "🇺🇸",
                "1️⃣",
                "♥️",
                "🫶🏽",
                "🏴\u{e0067}\u{e0062}\u{e0073}\u{e0063}\u{e0074}\u{e007f}",
            ] {
                assert_eq!(metrics.estimate_width(glyph), wide, "width of {glyph}");
                assert_eq!(
                    metrics.estimate_width(&glyph.repeat(2)),
                    wide * 2.0,
                    "adjacent glyphs must retain independent advances"
                );
            }
        }
    }

    #[test]
    fn unicode_wrapping_and_truncation_never_cut_an_extended_grapheme() {
        for metrics in [FontMetrics::default_metrics(), FontMetrics::monospace(10.0)] {
            for glyph in [
                "e\u{301}",
                " \u{301}",
                "א\u{5c7}",
                "ก\u{e31}",
                "\u{1100}\u{1161}\u{11a8}",
                "क्ष",
                "कि",
                "👩‍❤️‍💋‍👨",
                "👍🏽",
                "🇺🇸",
                "1️⃣",
                "🏴\u{e0067}\u{e0062}\u{e0073}\u{e0063}\u{e0074}\u{e007f}",
            ] {
                let text = glyph.repeat(4);
                let width = metrics.estimate_width(glyph) + 0.01;
                assert_eq!(
                    metrics.wrap_to_width(&text, width),
                    vec![glyph.to_string(); 4],
                    "word wrapping split {glyph:?}"
                );
                let preserved_width = metrics.estimate_preserved_width(
                    glyph,
                    metrics.font_size(),
                    metrics.avg_char_width(),
                ) + 0.01;
                assert_eq!(
                    metrics.wrap_preserved_to_width(&text, preserved_width),
                    vec![glyph.to_string(); 4],
                    "preserved wrapping split {glyph:?}"
                );
                let expected = format!("{glyph}...");
                let budget = metrics.estimate_width(&expected);
                let truncated = metrics.truncate_to_width(&format!("{text}abcdef"), budget);
                assert_eq!(truncated, expected, "truncation split {glyph:?}");
                assert!(metrics.estimate_width(&truncated) <= budget);
            }
        }
    }

    #[test]
    fn truncation_obeys_invalid_tiny_and_exact_width_budgets() {
        let metrics = FontMetrics::default_metrics();
        let text = "👨‍👩‍👧‍👦 abcdef";
        for width in [f32::NAN, f32::NEG_INFINITY, -1.0, 0.0] {
            assert!(metrics.truncate_to_width(text, width).is_empty());
        }
        assert_eq!(metrics.truncate_to_width(text, f32::INFINITY), text);
        let ellipsis_width = metrics.estimate_width("...");
        assert!(
            metrics
                .truncate_to_width(text, ellipsis_width.next_down())
                .is_empty()
        );
        assert_eq!(metrics.truncate_to_width(text, ellipsis_width), "...");
        let one_glyph = metrics.estimate_width("👨‍👩‍👧‍👦...");
        assert_eq!(
            metrics.truncate_to_width(text, one_glyph.next_down()),
            "..."
        );
        assert_eq!(metrics.truncate_to_width(text, one_glyph), "👨‍👩‍👧‍👦...");
    }

    #[test]
    fn default_metrics_are_reasonable() {
        let metrics = FontMetrics::default();
        assert!((metrics.font_size() - 15.0).abs() < f32::EPSILON);
        assert!(metrics.avg_char_width() > 5.0);
        assert!(metrics.avg_char_width() < 15.0);
    }

    #[test]
    fn authored_fonts_keep_the_base_line_height_and_measure_the_effective_face() {
        let metrics = FontMetrics::new(FontMetricsConfig {
            line_height: 1.75,
            ..FontMetricsConfig::default()
        });
        let styled = metrics
            .with_style_overrides(Some(" 32px "), Some("monospace"))
            .unwrap();
        assert_eq!(styled.font_size(), 32.0);
        assert_eq!(styled.line_height_px(), 56.0);
        assert_eq!(styled.estimate_width("iii"), styled.estimate_width("WWW"));
        let family_only = metrics
            .with_style_overrides(None, Some("monospace"))
            .unwrap();
        assert_eq!(family_only.font_size(), metrics.font_size());
        assert_eq!(family_only.line_height_px(), metrics.line_height_px());
        for size in ["0", "-1px", "NaN", "inf", "513px", "large", "2em"] {
            assert!(metrics.with_style_overrides(Some(size), None).is_none());
        }
        assert!(metrics.with_style_overrides(None, Some(" ")).is_none());
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
    fn truncation_keeps_extended_graphemes_whole_and_respects_the_width() {
        for metrics in [FontMetrics::default(), FontMetrics::monospace(15.0)] {
            let ellipsis_width = metrics.estimate_width("...");
            for glyph in [
                "e\u{301}",
                "कि",
                "क्षि",
                "ಕಿ",
                "\u{1100}\u{1161}\u{11a8}",
                "กำ",
                "👩🏽‍💻",
                "🇺🇸",
                "🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}",
            ] {
                let text = format!("{glyph}{glyph}WWWW");
                let glyph_width = metrics.estimate_width(glyph);
                let width = glyph_width + ellipsis_width + 0.01;
                let truncated = metrics.truncate_to_width(&text, width);
                assert_eq!(truncated, format!("{glyph}..."), "{glyph:?}");
                assert!(metrics.estimate_width(&truncated) <= width);

                let narrow_width = glyph_width * 0.5 + ellipsis_width;
                assert_eq!(
                    metrics.truncate_to_width(&text, narrow_width),
                    "...",
                    "a glyph that does not fit must not be cut in half: {glyph:?}",
                );
            }
        }
    }

    #[test]
    fn truncation_handles_empty_exact_and_invalid_width_budgets() {
        let metrics = FontMetrics::default();
        let ellipsis_width = metrics.estimate_width("...");
        assert_eq!(
            metrics.truncate_to_width("long label", ellipsis_width),
            "..."
        );
        assert_eq!(
            metrics.truncate_to_width("long label", ellipsis_width - 0.01),
            ""
        );
        assert_eq!(metrics.truncate_to_width("", ellipsis_width), "");
        assert_eq!(metrics.truncate_to_width("short", f32::INFINITY), "short");
        for width in [0.0, -1.0, f32::NEG_INFINITY, f32::NAN] {
            assert_eq!(metrics.truncate_to_width("long label", width), "");
        }
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
    fn preserved_wrapping_keeps_spaces_and_every_authored_row() {
        let metrics = FontMetrics::default();
        for text in [
            "  Keep   these    spaces  ",
            "                         ",
            "αβγ δει ζηθ averylongwordwithoutbreaks0123456789",
        ] {
            let rows = metrics.wrap_preserved_to_width(text, 90.0);
            assert_eq!(rows.concat(), text);
            assert!(rows.iter().all(|row| metrics.estimate_preserved_width(
                row,
                metrics.font_size(),
                metrics.avg_char_width(),
            ) <= 90.01));
        }
        assert_eq!(
            metrics.wrap_preserved_to_width("First\r\n\r\nLast\r\n", 200.0),
            ["First", "", "Last", ""],
        );
        for width in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                metrics
                    .wrap_preserved_to_width("a  b\n\nend\n", width)
                    .join("\n"),
                "a  b\n\nend\n"
            );
        }
    }

    #[test]
    fn preserved_wrapping_never_splits_combining_or_joined_glyphs() {
        let metrics = FontMetrics::default();
        for glyph in ["e\u{301}", "👨\u{200d}👩\u{200d}👧", "🇺🇸"] {
            let rows = metrics.wrap_preserved_to_width(&glyph.repeat(3), 1.0);
            assert_eq!(rows, vec![glyph.to_string(); 3]);
        }
    }

    #[test]
    fn balanced_wrapping_removes_an_orphan_without_adding_rows_or_width() {
        let metrics = FontMetrics::monospace(10.0);
        let text = "alpha alpha alpha alpha";
        let original = metrics.wrap_preserved_to_width(text, 130.0);
        let balanced = metrics.wrap_balanced_to_width(text, 130.0);
        assert_eq!(original, ["alpha alpha alpha ", "alpha"]);
        assert_eq!(balanced, ["alpha alpha ", "alpha alpha"]);
        assert_eq!(balanced.concat(), text);
        assert_eq!(balanced.len(), original.len());
        let widest = |lines: &[String]| {
            lines
                .iter()
                .map(|line| {
                    metrics.estimate_preserved_width(
                        line,
                        metrics.font_size(),
                        metrics.avg_char_width(),
                    )
                })
                .fold(0.0, f32::max)
        };
        assert!(widest(&balanced) < widest(&original) * 0.75);
    }

    #[test]
    fn balanced_wrapping_preserves_spaces_and_complete_unicode_glyphs() {
        let metrics = FontMetrics::default();
        let text = "  Keep  cafe\u{301} and 👩🏽‍💻 together before reviewing 🇺🇸 deployment plans  ";
        let original = metrics.wrap_preserved_to_width(text, 220.0);
        let balanced = metrics.wrap_balanced_to_width(text, 220.0);
        assert_eq!(balanced.concat(), text);
        assert_eq!(balanced.len(), original.len());
        assert_eq!(balanced, metrics.wrap_balanced_to_width(text, 220.0));
        for glyph in ["cafe\u{301}", "👩🏽‍💻", "🇺🇸"] {
            assert!(
                balanced.iter().any(|line| line.contains(glyph)),
                "split {glyph}"
            );
        }
        assert!(balanced.iter().all(|line| metrics.estimate_preserved_width(
            line,
            metrics.font_size(),
            metrics.avg_char_width(),
        ) <= 220.01));
    }

    #[test]
    fn balanced_wrapping_retains_authored_rows_and_bounded_fallbacks() {
        let metrics = FontMetrics::default();
        for text in [
            "First authored line\n\nSecond authored line\n",
            "First authored line\r\nSecond authored line\r\n",
            "an_identifier_without_any_legal_word_boundary_at_all",
            "👩🏽‍💻👩🏽‍💻👩🏽‍💻🇺🇸🇺🇸🇺🇸",
            "short",
            "",
        ] {
            assert_eq!(
                metrics.wrap_balanced_to_width(text, 90.0),
                metrics.wrap_preserved_to_width(text, 90.0)
            );
        }
        for width in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let text = "Do not reflow this paragraph";
            assert_eq!(metrics.wrap_balanced_to_width(text, width), [text]);
        }
        let long = "a long paragraph whose complete contents remain visible ".repeat(100);
        assert_eq!(
            metrics.wrap_balanced_to_width(&long, 90.0),
            metrics.wrap_preserved_to_width(&long, 90.0)
        );
    }

    #[test]
    fn wrap_splits_long_lines() {
        let metrics = FontMetrics::default();
        let text = "This is a sentence that should be wrapped to multiple lines";
        let wrapped = metrics.wrap_to_width(text, 100.0);

        assert!(wrapped.len() > 1);
    }

    #[test]
    fn wrap_preserves_authored_breaks_and_bounds_long_tokens() {
        let metrics = FontMetrics::default();
        let text = "First\n\nabcdefghijklmnopqrstuvwxyz0123456789\nLast\n";
        let wrapped = metrics.wrap_to_width(text, 80.0);
        assert_eq!(&wrapped[..2], &["First", ""]);
        assert_eq!(&wrapped[wrapped.len() - 2..], &["Last", ""]);
        assert_eq!(
            wrapped[2..wrapped.len() - 2].concat(),
            "abcdefghijklmnopqrstuvwxyz0123456789"
        );
        assert!(
            wrapped
                .iter()
                .all(|line| metrics.estimate_width(line) <= 80.01)
        );

        for width in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(metrics.wrap_to_width(text, width).join("\n"), text);
        }
    }

    #[test]
    fn wrap_keeps_combining_marks_and_emoji_with_their_glyphs() {
        let metrics = FontMetrics::default();
        for glyph in ["e\u{301}", "👨\u{200d}👩\u{200d}👧", "🇺🇸"] {
            let text = glyph.repeat(3);
            let wrapped = metrics.wrap_to_width(&text, metrics.estimate_width(glyph) + 0.01);
            assert_eq!(wrapped, vec![glyph.to_string(); 3]);
            assert_eq!(wrapped.concat(), text);
        }
        assert_eq!(metrics.wrap_to_width("界界", 1.0), ["界", "界"]);
    }

    #[test]
    fn all_wrapping_modes_keep_extended_graphemes_whole() {
        for metrics in [FontMetrics::default(), FontMetrics::monospace(15.0)] {
            for glyph in [
                "कि",
                "क्षि",
                "ಕಿ",
                "\u{1100}\u{1161}\u{11a8}",
                "กำ",
                "🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}",
                "\u{600}A",
            ] {
                let text = glyph.repeat(3);
                let expected = vec![glyph.to_owned(); 3];
                assert_eq!(metrics.wrap_to_width(&text, 1.0), expected, "{glyph:?}");
                assert_eq!(
                    metrics.wrap_preserved_to_width(&text, 1.0),
                    expected,
                    "{glyph:?}"
                );
                assert_eq!(
                    metrics.wrap_balanced_to_width(&text, 1.0),
                    expected,
                    "{glyph:?}"
                );
            }
        }
    }

    #[test]
    fn word_wrapping_does_not_detach_a_combining_mark_from_a_space() {
        let metrics = FontMetrics::default();
        let text = "A \u{301}B";
        assert_eq!(metrics.wrap_to_width(text, 1.0), ["A", " \u{301}", "B"]);
        assert_eq!(
            metrics.wrap_preserved_to_width(text, 1.0),
            ["A", " \u{301}", "B"]
        );
    }

    #[test]
    fn wrap_normalizes_crlf_without_losing_authored_rows() {
        let metrics = FontMetrics::default();
        for width in [0.0, 200.0] {
            assert_eq!(
                metrics.wrap_to_width("First\r\n\r\nLast\r\n", width),
                ["First", "", "Last", ""]
            );
            assert_eq!(
                metrics.wrap_to_width("First\r\nLast\r", width),
                ["First", "Last\r"],
                "a standalone CR is not a CRLF line ending"
            );
        }
        let wrapped = metrics.wrap_to_width("abcdefghijklmnopqrstuvwxyz\r\nLast", 80.0);
        assert!(wrapped.iter().all(|line| !line.contains('\r')));
        assert_eq!(wrapped.concat(), "abcdefghijklmnopqrstuvwxyzLast");
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
