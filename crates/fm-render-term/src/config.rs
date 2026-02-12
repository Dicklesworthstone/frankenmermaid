//! Terminal renderer configuration types.

use fm_core::{MermaidGlyphMode, MermaidRenderMode, MermaidTier};

/// Configuration for terminal diagram rendering.
#[derive(Debug, Clone)]
pub struct TermRenderConfig {
    /// Rendering fidelity tier (Compact/Normal/Rich/Auto).
    pub tier: MermaidTier,
    /// Sub-cell rendering mode (CellOnly/Braille/Block/HalfBlock/Auto).
    pub render_mode: MermaidRenderMode,
    /// Glyph mode (Unicode box-drawing vs ASCII fallback).
    pub glyph_mode: MermaidGlyphMode,
    /// Maximum width in terminal columns.
    pub max_width: usize,
    /// Maximum height in terminal rows.
    pub max_height: usize,
    /// Maximum label characters before truncation.
    pub max_label_chars: usize,
    /// Maximum label lines before truncation.
    pub max_label_lines: usize,
    /// Show selection highlighting.
    pub show_selection: bool,
    /// Show cluster decorations.
    pub show_clusters: bool,
    /// Enable diagonal edge optimization.
    pub diagonal_edges: bool,
    /// Padding around the diagram (in cells).
    pub padding: usize,
}

impl Default for TermRenderConfig {
    fn default() -> Self {
        Self {
            tier: MermaidTier::Auto,
            render_mode: MermaidRenderMode::Braille,
            glyph_mode: MermaidGlyphMode::Unicode,
            max_width: 120,
            max_height: 40,
            max_label_chars: 24,
            max_label_lines: 2,
            show_selection: false,
            show_clusters: true,
            diagonal_edges: true,
            padding: 1,
        }
    }
}

impl TermRenderConfig {
    /// Create a compact configuration for small terminals.
    #[must_use]
    pub fn compact() -> Self {
        Self {
            tier: MermaidTier::Compact,
            render_mode: MermaidRenderMode::CellOnly,
            max_width: 80,
            max_height: 24,
            max_label_chars: 12,
            max_label_lines: 1,
            show_clusters: false,
            diagonal_edges: false,
            ..Self::default()
        }
    }

    /// Create a rich configuration for large high-resolution terminals.
    #[must_use]
    pub fn rich() -> Self {
        Self {
            tier: MermaidTier::Rich,
            render_mode: MermaidRenderMode::Braille,
            max_width: 200,
            max_height: 60,
            max_label_chars: 48,
            max_label_lines: 3,
            ..Self::default()
        }
    }

    /// Resolve the effective tier based on available space.
    #[must_use]
    pub fn effective_tier(&self, available_cols: usize, available_rows: usize) -> MermaidTier {
        match self.tier {
            MermaidTier::Auto => {
                let area = available_cols.saturating_mul(available_rows);
                if area < 1000 {
                    MermaidTier::Compact
                } else if area < 5000 {
                    MermaidTier::Normal
                } else {
                    MermaidTier::Rich
                }
            }
            other => other,
        }
    }

    /// Resolve the effective render mode based on tier.
    #[must_use]
    pub fn effective_render_mode(&self, tier: MermaidTier) -> MermaidRenderMode {
        match self.render_mode {
            MermaidRenderMode::Auto => match tier {
                MermaidTier::Compact => MermaidRenderMode::CellOnly,
                MermaidTier::Normal => MermaidRenderMode::HalfBlock,
                MermaidTier::Rich | MermaidTier::Auto => MermaidRenderMode::Braille,
            },
            other => other,
        }
    }
}

/// Resolved configuration after auto-detection.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedConfig {
    pub tier: MermaidTier,
    pub render_mode: MermaidRenderMode,
    pub glyph_mode: MermaidGlyphMode,
    pub cols: usize,
    pub rows: usize,
    pub max_label_chars: usize,
    pub max_label_lines: usize,
    pub show_clusters: bool,
    pub diagonal_edges: bool,
    pub padding: usize,
}

impl ResolvedConfig {
    /// Resolve configuration for the given terminal size.
    #[must_use]
    pub fn resolve(config: &TermRenderConfig, cols: usize, rows: usize) -> Self {
        let available_cols = cols.min(config.max_width);
        let available_rows = rows.min(config.max_height);
        let tier = config.effective_tier(available_cols, available_rows);
        let render_mode = config.effective_render_mode(tier);

        // Adjust label limits based on tier.
        let (max_label_chars, max_label_lines) = match tier {
            MermaidTier::Compact => (config.max_label_chars.min(12), 1),
            MermaidTier::Normal => (config.max_label_chars.min(24), config.max_label_lines.min(2)),
            MermaidTier::Rich | MermaidTier::Auto => (config.max_label_chars, config.max_label_lines),
        };

        Self {
            tier,
            render_mode,
            glyph_mode: config.glyph_mode,
            cols: available_cols,
            rows: available_rows,
            max_label_chars,
            max_label_lines,
            show_clusters: config.show_clusters && !matches!(tier, MermaidTier::Compact),
            diagonal_edges: config.diagonal_edges,
            padding: config.padding,
        }
    }

    /// Get the sub-cell resolution multiplier for the render mode.
    #[must_use]
    pub const fn subcell_multiplier(&self) -> (usize, usize) {
        match self.render_mode {
            MermaidRenderMode::Braille => (2, 4), // 2 columns x 4 rows per cell
            MermaidRenderMode::Block => (2, 2),   // 2x2 per cell
            MermaidRenderMode::HalfBlock => (1, 2), // 1x2 per cell
            MermaidRenderMode::CellOnly | MermaidRenderMode::Auto => (1, 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sane() {
        let config = TermRenderConfig::default();
        assert!(config.max_width >= 80);
        assert!(config.max_height >= 24);
    }

    #[test]
    fn auto_tier_selects_based_on_area() {
        let config = TermRenderConfig::default();
        assert_eq!(config.effective_tier(40, 20), MermaidTier::Compact);
        assert_eq!(config.effective_tier(80, 40), MermaidTier::Normal);
        assert_eq!(config.effective_tier(200, 60), MermaidTier::Rich);
    }

    #[test]
    fn resolved_config_respects_max_bounds() {
        let config = TermRenderConfig {
            max_width: 100,
            max_height: 30,
            ..Default::default()
        };
        let resolved = ResolvedConfig::resolve(&config, 200, 60);
        assert_eq!(resolved.cols, 100);
        assert_eq!(resolved.rows, 30);
    }

    #[test]
    fn braille_has_2x4_multiplier() {
        let config = ResolvedConfig {
            render_mode: MermaidRenderMode::Braille,
            tier: MermaidTier::Rich,
            glyph_mode: MermaidGlyphMode::Unicode,
            cols: 80,
            rows: 24,
            max_label_chars: 24,
            max_label_lines: 2,
            show_clusters: true,
            diagonal_edges: true,
            padding: 1,
        };
        assert_eq!(config.subcell_multiplier(), (2, 4));
    }
}
