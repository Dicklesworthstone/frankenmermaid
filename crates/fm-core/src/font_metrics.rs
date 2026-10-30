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

/// U+200D ZERO WIDTH JOINER — fuses the following character into the preceding glyph.
const ZERO_WIDTH_JOINER: char = '\u{200D}';

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
/// This is a curated range list covering the marks that appear in diagram labels, not a full Unicode
/// character-database lookup — the crate carries no UCD table and this runs per character on the
/// measurement hot path. Every range below is general category `Mn`, `Me`, or `Cf`.
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
        // ASCII fast path (common labels, non-monospace): a direct multiplier-table load per
        // byte replaces the per-char `classify().multiplier()` match chain. Bit-identical —
        // `ASCII_WIDTH_MULT[b]` IS `classify(b as char).multiplier()`, and the `avg * mult`
        // op + left-to-right f32 sum are unchanged. Monospace / non-ASCII keep the char path.
        if self.config.preset != FontPreset::Monospace {
            let avg = self.avg_char_width;
            // Pure ASCII: unchanged. `bytes()` is the shape the auto-vectorizer likes, so the
            // overwhelmingly common label keeps the fastest path it already had.
            if text.is_ascii() {
                return text
                    .bytes()
                    .map(|b| avg * ASCII_WIDTH_MULT[b as usize])
                    .sum();
            }
            // MIXED SCRIPT. The `is_ascii()` gate above used to be the ONLY gate, so a label like
            // `Café` — three ASCII chars and one accented one — sent every character down the
            // `classify()` match chain. One non-ASCII byte cost the whole label its table.
            //
            // That is the wrong shape for `docs_site_50`, the worst ratio this engine actually
            // renders at (434.107779x): the ledger describes it as flowchart-dominated with
            // "escaping/non-ASCII labels", and it measures 940 node labels across 50 diagrams. A
            // label that is mostly ASCII with an accent or a dash is exactly its common case, and
            // it was taking the slow path end to end.
            //
            // ⚠️ THE BIT-IDENTITY BELOW IS ABOUT THE *ASCII* CHARACTERS OF A MIXED LABEL, AND IS
            // NARROWER THAN IT WAS. Non-ASCII widths deliberately MOVED when zero-width code points
            // stopped being billed as full characters: a combining mark, variation selector,
            // skin-tone modifier or joiner now contributes 0.0 (see `is_zero_width`), and a
            // ZWJ-joined character contributes 0.0 via the loop below. That was a correctness fix —
            // `Café` measured 29.7 spelled NFC and 37.95 spelled NFD, two widths for one label — so
            // any golden containing those code points is EXPECTED to change, and a golden that does
            // not contain them cannot.
            //
            // For ASCII the identity is structural rather than a claim: `ASCII_WIDTH_MULT`
            // is BUILT at compile time as `classify(i as char).multiplier()` for every i < 128, so
            // for an ASCII char the table and the match chain are the same f32 by construction.
            // Every term is still `avg * multiplier(c)` and the terms are still summed
            // left-to-right over `chars()` in order, so the f32 accumulation sequence is unchanged.
            // Node sizes therefore cannot move, and no golden can shift.
            //
            // The monospace branch of `char_width` is also hoisted out of the loop: the preset
            // cannot change per character, so testing it once is the same answer with one branch
            // instead of one per char.
            // A ZERO-WIDTH JOINER FUSES THE FOLLOWING CHARACTER INTO THE PRECEDING GLYPH, so the
            // sequence `👨 ZWJ 👩 ZWJ 👧` draws as ONE family emoji roughly one emoji wide — not as
            // three emoji plus two joiners. Classifying the joiner itself as zero-width is not
            // enough: without this, the family still measures 3x a single emoji. One bool of state
            // is the whole grapheme-clustering this needs, because the only character that fuses a
            // following glyph is the joiner.
            //
            // The accumulation is still `avg * multiplier` summed left-to-right over `chars()`, and
            // a joined character contributes a literal `0.0` term rather than being skipped, so the
            // f32 addition SEQUENCE is unchanged for every string that contains no joiner.
            let mut total = 0.0_f32;
            let mut after_joiner = false;
            for c in text.chars() {
                let (multiplier, is_full_width) = if c.is_ascii() {
                    (ASCII_WIDTH_MULT[c as usize], false)
                } else {
                    let class = CharWidthClass::classify(c);
                    (
                        class.multiplier(),
                        matches!(class, CharWidthClass::FullWidth),
                    )
                };
                // ⚠️ A JOINER FUSES AN EMOJI, NOT ANY CHARACTER. `👨 ZWJ 👩` is one family glyph, but
                // `a ZWJ b` still draws two Latin letters — the joiner only requests a joined form,
                // which Latin fonts do not provide. Suppressing every character after a joiner
                // measured `a\u{200D}b` as one character wide, which is why only a FULL-WIDTH
                // follower is treated as fused.
                total += avg
                    * if after_joiner && is_full_width {
                        0.0
                    } else {
                        multiplier
                    };
                after_joiner = c == ZERO_WIDTH_JOINER;
            }
            return total;
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
            let mut boundaries = vec![0];
            let mut after_joiner = false;
            let mut regional_count = 0;
            for (offset, character) in line.char_indices() {
                let regional = matches!(character, '\u{1f1e6}'..='\u{1f1ff}');
                if offset > 0
                    && !is_zero_width(character)
                    && !after_joiner
                    && (!regional || regional_count % 2 == 0)
                {
                    boundaries.push(offset);
                }
                after_joiner = character == ZERO_WIDTH_JOINER;
                regional_count = if regional { regional_count + 1 } else { 0 };
            }
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
                    if candidate.ends_with(char::is_whitespace) {
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
        for (offset, character) in text.char_indices() {
            let space = character.is_whitespace();
            if after_space && !space && seen_word && !is_zero_width(character) {
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

            for word in line.split_whitespace() {
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
                let mut after_joiner = false;
                let mut regional_count = 0;
                for (offset, c) in word.char_indices() {
                    let regional = matches!(c, '\u{1f1e6}'..='\u{1f1ff}');
                    let advance = if after_joiner
                        && is_east_asian_wide(c)
                        && self.config.preset != FontPreset::Monospace
                    {
                        0.0
                    } else {
                        self.char_width(c)
                    };
                    let can_break = offset > start
                        && current_width > 0.0
                        && !is_zero_width(c)
                        && !after_joiner
                        && (!regional || regional_count % 2 == 0);
                    if can_break && current_width + advance > max_width {
                        lines.push(word[start..offset].to_string());
                        start = offset;
                        current_width = 0.0;
                    }
                    current_width += advance;
                    after_joiner = c == ZERO_WIDTH_JOINER;
                    regional_count = if regional { regional_count + 1 } else { 0 };
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

    /// Control: the monospace preset is a different rule entirely — every character is one
    /// `avg_char_width`, including full-width ones — and the mixed-script path must not capture it.
    /// Without this, mapping monospace onto the width table would look like a simplification and
    /// would silently widen every CJK label in a monospace diagram.
    #[test]
    fn monospace_width_is_unaffected_by_the_mixed_script_path() {
        let metrics = FontMetrics::monospace(14.0);
        for text in ["日本語", "abc", "aé日"] {
            // Summed, NOT `avg * count`: repeated f32 addition and a single multiply are not
            // bit-equal in general, and this test asserts bits. The reference must mirror how the
            // implementation accumulates, or the control fails on correct code.
            let expected: f32 = text.chars().map(|_| metrics.avg_char_width()).sum();
            assert_eq!(
                metrics.estimate_width(text).to_bits(),
                expected.to_bits(),
                "monospace width changed for {text:?}"
            );
        }
    }
    use super::*;

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
