//! SVG theming system for frankenmermaid diagrams.
//!
//! Provides preset themes, CSS custom property generation, and color palette utilities.

use std::str::FromStr;

/// Theme preset identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemePreset {
    /// Clean neutral colors (blue/gray) - default mermaid-compatible
    #[default]
    Default,
    /// Dark background with bright accents
    Dark,
    /// Green/brown earth tones
    Forest,
    /// Minimal grayscale
    Neutral,
    /// Professional blue/gray/white
    Corporate,
    /// Bright neon on dark (FrankenMermaid extension)
    Neon,
    /// Soft muted colors (FrankenMermaid extension)
    Pastel,
    /// WCAG AA compliant high contrast (FrankenMermaid extension)
    HighContrast,
    /// Black and white only (FrankenMermaid extension)
    Monochrome,
    /// White-on-blue technical drawing style (FrankenMermaid extension)
    Blueprint,
}

/// Error type for theme preset parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseThemePresetError(String);

impl std::fmt::Display for ParseThemePresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown theme preset: {}", self.0)
    }
}

impl std::error::Error for ParseThemePresetError {}

impl FromStr for ThemePreset {
    type Err = ParseThemePresetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "default" => Ok(Self::Default),
            "dark" => Ok(Self::Dark),
            "forest" => Ok(Self::Forest),
            "neutral" => Ok(Self::Neutral),
            "corporate" => Ok(Self::Corporate),
            "neon" => Ok(Self::Neon),
            "pastel" => Ok(Self::Pastel),
            "high-contrast" | "highcontrast" => Ok(Self::HighContrast),
            "monochrome" | "mono" => Ok(Self::Monochrome),
            "blueprint" => Ok(Self::Blueprint),
            _ => Err(ParseThemePresetError(s.to_string())),
        }
    }
}

impl ThemePreset {
    /// Get the string identifier for this preset.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Dark => "dark",
            Self::Forest => "forest",
            Self::Neutral => "neutral",
            Self::Corporate => "corporate",
            Self::Neon => "neon",
            Self::Pastel => "pastel",
            Self::HighContrast => "high-contrast",
            Self::Monochrome => "monochrome",
            Self::Blueprint => "blueprint",
        }
    }
}

/// Theme color configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeColors {
    /// Background color
    pub background: String,
    /// Primary text color
    pub text: String,
    /// Node fill color
    pub node_fill: String,
    /// Node stroke color
    pub node_stroke: String,
    /// Edge/arrow color
    pub edge: String,
    /// Cluster fill color
    pub cluster_fill: String,
    /// Cluster stroke color
    pub cluster_stroke: String,
    /// Accent colors for variety (8 colors)
    pub accents: [String; 8],
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self::from_preset(ThemePreset::Default)
    }
}

impl ThemeColors {
    /// Create colors from a preset.
    #[must_use]
    pub fn from_preset(preset: ThemePreset) -> Self {
        match preset {
            ThemePreset::Default => Self {
                background: "#ffffff".into(),
                text: "#333333".into(),
                node_fill: "#ffffff".into(),
                node_stroke: "#333333".into(),
                edge: "#333333".into(),
                cluster_fill: "rgba(248,249,250,0.85)".into(),
                cluster_stroke: "#dee2e6".into(),
                accents: [
                    "#4285f4".into(), // Google blue
                    "#34a853".into(), // Google green
                    "#fbbc05".into(), // Google yellow
                    "#ea4335".into(), // Google red
                    "#9c27b0".into(), // Purple
                    "#00bcd4".into(), // Cyan
                    "#ff9800".into(), // Orange
                    "#607d8b".into(), // Blue gray
                ],
            },

            ThemePreset::Dark => Self {
                background: "#1a1a2e".into(),
                text: "#eaeaea".into(),
                node_fill: "#16213e".into(),
                node_stroke: "#0f3460".into(),
                edge: "#e94560".into(),
                cluster_fill: "rgba(22,33,62,0.8)".into(),
                cluster_stroke: "#0f3460".into(),
                accents: [
                    "#e94560".into(),
                    "#00d9ff".into(),
                    "#ff6b6b".into(),
                    "#ffd93d".into(),
                    "#6bcb77".into(),
                    "#ff8e72".into(),
                    "#a855f7".into(),
                    "#38bdf8".into(),
                ],
            },

            ThemePreset::Forest => Self {
                background: "#f5f5dc".into(),
                text: "#2d4a22".into(),
                node_fill: "#d4edbc".into(),
                node_stroke: "#4a7c31".into(),
                edge: "#6b8e23".into(),
                cluster_fill: "rgba(212,237,188,0.6)".into(),
                cluster_stroke: "#8fbc8f".into(),
                accents: [
                    "#228b22".into(),
                    "#6b8e23".into(),
                    "#8b4513".into(),
                    "#daa520".into(),
                    "#556b2f".into(),
                    "#9acd32".into(),
                    "#a0522d".into(),
                    "#808000".into(),
                ],
            },

            ThemePreset::Neutral => Self {
                background: "#fafafa".into(),
                text: "#444444".into(),
                node_fill: "#f0f0f0".into(),
                node_stroke: "#888888".into(),
                edge: "#666666".into(),
                cluster_fill: "rgba(200,200,200,0.3)".into(),
                cluster_stroke: "#aaaaaa".into(),
                accents: [
                    "#555555".into(),
                    "#777777".into(),
                    "#999999".into(),
                    "#bbbbbb".into(),
                    "#444444".into(),
                    "#666666".into(),
                    "#888888".into(),
                    "#aaaaaa".into(),
                ],
            },

            ThemePreset::Corporate => Self {
                background: "#ffffff".into(),
                text: "#1a365d".into(),
                node_fill: "#ebf8ff".into(),
                node_stroke: "#2b6cb0".into(),
                edge: "#2c5282".into(),
                cluster_fill: "rgba(235,248,255,0.7)".into(),
                cluster_stroke: "#90cdf4".into(),
                accents: [
                    "#2b6cb0".into(),
                    "#2f855a".into(),
                    "#c05621".into(),
                    "#744210".into(),
                    "#553c9a".into(),
                    "#234e52".into(),
                    "#702459".into(),
                    "#285e61".into(),
                ],
            },

            ThemePreset::Neon => Self {
                background: "#0d0d0d".into(),
                text: "#ffffff".into(),
                node_fill: "#1a1a1a".into(),
                node_stroke: "#00ff88".into(),
                edge: "#ff00ff".into(),
                cluster_fill: "rgba(0,255,136,0.1)".into(),
                cluster_stroke: "#00ff88".into(),
                accents: [
                    "#00ff88".into(),
                    "#ff00ff".into(),
                    "#00ffff".into(),
                    "#ffff00".into(),
                    "#ff3366".into(),
                    "#33ff99".into(),
                    "#ff6600".into(),
                    "#9933ff".into(),
                ],
            },

            ThemePreset::Pastel => Self {
                background: "#fefefe".into(),
                text: "#5a5a5a".into(),
                node_fill: "#fce4ec".into(),
                node_stroke: "#f48fb1".into(),
                edge: "#ce93d8".into(),
                cluster_fill: "rgba(225,190,231,0.4)".into(),
                cluster_stroke: "#e1bee7".into(),
                accents: [
                    "#f8bbd9".into(),
                    "#e1bee7".into(),
                    "#d1c4e9".into(),
                    "#c5cae9".into(),
                    "#b3e5fc".into(),
                    "#b2dfdb".into(),
                    "#c8e6c9".into(),
                    "#fff9c4".into(),
                ],
            },

            ThemePreset::HighContrast => Self {
                background: "#ffffff".into(),
                text: "#000000".into(),
                node_fill: "#ffffff".into(),
                node_stroke: "#000000".into(),
                edge: "#000000".into(),
                cluster_fill: "rgba(255,255,0,0.3)".into(),
                cluster_stroke: "#000000".into(),
                accents: [
                    "#0000ff".into(),
                    "#ff0000".into(),
                    "#008000".into(),
                    "#ff00ff".into(),
                    "#800000".into(),
                    "#000080".into(),
                    "#008080".into(),
                    "#800080".into(),
                ],
            },

            ThemePreset::Monochrome => Self {
                background: "#ffffff".into(),
                text: "#000000".into(),
                node_fill: "#ffffff".into(),
                node_stroke: "#000000".into(),
                edge: "#000000".into(),
                cluster_fill: "rgba(200,200,200,0.3)".into(),
                cluster_stroke: "#000000".into(),
                accents: [
                    "#000000".into(),
                    "#333333".into(),
                    "#666666".into(),
                    "#999999".into(),
                    "#000000".into(),
                    "#333333".into(),
                    "#666666".into(),
                    "#999999".into(),
                ],
            },

            ThemePreset::Blueprint => Self {
                background: "#00264d".into(),
                text: "#ffffff".into(),
                node_fill: "#003366".into(),
                node_stroke: "#ffffff".into(),
                edge: "#ffffff".into(),
                cluster_fill: "rgba(0,51,102,0.5)".into(),
                cluster_stroke: "#66b3ff".into(),
                accents: [
                    "#ffffff".into(),
                    "#99ccff".into(),
                    "#66b3ff".into(),
                    "#3399ff".into(),
                    "#0080ff".into(),
                    "#99ccff".into(),
                    "#cce6ff".into(),
                    "#ffffff".into(),
                ],
            },
        }
    }

    /// Apply theme variables mapping from standard Mermaid configs.
    pub fn apply_overrides(&mut self, vars: &std::collections::BTreeMap<String, String>) {
        if let Some(v) = vars.get("background") {
            self.background = v.clone();
        }
        if let Some(v) = vars.get("primaryTextColor").or(vars.get("textColor")) {
            self.text = v.clone();
        }
        if let Some(v) = vars.get("primaryColor") {
            self.node_fill = v.clone();
        }
        if let Some(v) = vars.get("primaryBorderColor") {
            self.node_stroke = v.clone();
        }
        if let Some(v) = vars.get("lineColor") {
            self.edge = v.clone();
        }
        if let Some(v) = vars.get("clusterBkg") {
            self.cluster_fill = v.clone();
        }
        if let Some(v) = vars.get("clusterBorder") {
            self.cluster_stroke = v.clone();
        }
    }

    /// Generate CSS custom properties for this theme.
    #[must_use]
    pub fn to_css_vars(&self) -> String {
        let mut css = String::with_capacity(512);
        css.push_str(":root {\n");
        css.push_str(&format!("  --fm-bg: {};\n", self.background));
        css.push_str(&format!("  --fm-text-color: {};\n", self.text));
        css.push_str(&format!("  --fm-node-fill: {};\n", self.node_fill));
        css.push_str(&format!("  --fm-node-stroke: {};\n", self.node_stroke));
        css.push_str(&format!("  --fm-edge-color: {};\n", self.edge));
        css.push_str(&format!("  --fm-cluster-fill: {};\n", self.cluster_fill));
        css.push_str(&format!(
            "  --fm-cluster-stroke: {};\n",
            self.cluster_stroke
        ));
        for (i, accent) in self.accents.iter().enumerate() {
            css.push_str(&format!("  --fm-accent-{}: {};\n", i + 1, accent));
        }
        css.push_str("}\n");
        css
    }
}

/// Font configuration for SVG rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct FontConfig {
    /// Font family stack
    pub family: String,
    /// Base font size in pixels
    pub size: f32,
    /// Font weight (100-900)
    pub weight: u16,
    /// Optional web font URL to embed
    pub web_font_url: Option<String>,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "system-ui, -apple-system, sans-serif".into(),
            size: 14.0,
            weight: 400,
            web_font_url: None,
        }
    }
}

impl FontConfig {
    /// Generate CSS for the font configuration.
    #[must_use]
    pub fn to_css(&self) -> String {
        let mut css = String::with_capacity(256);

        // Embed web font if provided
        if let Some(url) = &self.web_font_url {
            css.push_str(&format!("@import url('{}');\n", url));
        }

        css.push_str(".fm-text {\n");
        css.push_str(&format!("  font-family: {};\n", self.family));
        css.push_str(&format!("  font-size: {}px;\n", self.size));
        css.push_str(&format!("  font-weight: {};\n", self.weight));
        css.push_str("}\n");

        css
    }
}

/// Complete theme configuration combining colors and fonts.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Theme {
    /// Color configuration
    pub colors: ThemeColors,
    /// Font configuration
    pub font: FontConfig,
}

impl Theme {
    /// Create a theme from a preset.
    #[must_use]
    pub fn from_preset(preset: ThemePreset) -> Self {
        Self {
            colors: ThemeColors::from_preset(preset),
            font: FontConfig::default(),
        }
    }

    /// Generate the complete CSS style block for embedding in SVG.
    #[must_use]
    pub fn to_svg_style(&self) -> String {
        let mut css = String::with_capacity(1024);
        css.push_str(&self.colors.to_css_vars());
        css.push_str(&self.font.to_css());

        // Add utility classes
        css.push_str(
            r"
.fm-node { fill: var(--fm-node-fill); stroke: var(--fm-node-stroke); }
.fm-edge { stroke: var(--fm-edge-color); }
.fm-cluster { fill: var(--fm-cluster-fill); stroke: var(--fm-cluster-stroke); }
.fm-label { fill: var(--fm-text-color); }
",
        );

        css
    }
}

/// Generate a harmonious color palette from a base color using HSL rotation.
///
/// Given a base hex color, generates `count` distinct colors by rotating
/// around the color wheel while maintaining similar saturation and lightness.
#[must_use]
pub fn generate_palette(base_hex: &str, count: usize) -> Vec<String> {
    let (h, s, l) = hex_to_hsl(base_hex);
    let step = 360.0 / count as f32;

    (0..count)
        .map(|i| {
            let new_h = (h + step * i as f32) % 360.0;
            hsl_to_hex(new_h, s, l)
        })
        .collect()
}

/// Convert hex color to HSL.
fn hex_to_hsl(hex: &str) -> (f32, f32, f32) {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return (0.0, 0.0, 0.5); // fallback gray
    }

    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(128) as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(128) as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(128) as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if (max - r).abs() < f32::EPSILON {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) * 60.0
    } else if (max - g).abs() < f32::EPSILON {
        ((b - r) / d + 2.0) * 60.0
    } else {
        ((r - g) / d + 4.0) * 60.0
    };

    (h, s, l)
}

/// Convert HSL to hex color.
fn hsl_to_hex(h: f32, s: f32, l: f32) -> String {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = match (h / 60.0) as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let r = ((r + m) * 255.0) as u8;
    let g = ((g + m) * 255.0) as u8;
    let b = ((b + m) * 255.0) as u8;

    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_from_str_works() {
        assert_eq!("dark".parse::<ThemePreset>().ok(), Some(ThemePreset::Dark));
        assert_eq!(
            "FOREST".parse::<ThemePreset>().ok(),
            Some(ThemePreset::Forest)
        );
        assert_eq!(
            "high-contrast".parse::<ThemePreset>().ok(),
            Some(ThemePreset::HighContrast)
        );
        assert!("invalid".parse::<ThemePreset>().is_err());
    }

    #[test]
    fn preset_as_str_round_trips() {
        for preset in [
            ThemePreset::Default,
            ThemePreset::Dark,
            ThemePreset::Forest,
            ThemePreset::Neutral,
            ThemePreset::Corporate,
            ThemePreset::Neon,
            ThemePreset::Pastel,
            ThemePreset::HighContrast,
            ThemePreset::Monochrome,
            ThemePreset::Blueprint,
        ] {
            let name = preset.as_str();
            let parsed: Result<ThemePreset, _> = name.parse();
            assert_eq!(
                parsed.ok(),
                Some(preset),
                "preset {:?} should round-trip",
                name
            );
        }
    }

    #[test]
    fn colors_generate_css_vars() {
        let colors = ThemeColors::from_preset(ThemePreset::Default);
        let css = colors.to_css_vars();
        assert!(css.contains("--fm-bg:"));
        assert!(css.contains("--fm-text-color:"));
        assert!(css.contains("--fm-node-fill:"));
        assert!(css.contains("--fm-accent-1:"));
        assert!(css.contains("--fm-accent-8:"));
    }

    #[test]
    fn dark_theme_has_dark_background() {
        let colors = ThemeColors::from_preset(ThemePreset::Dark);
        assert!(
            colors.background.starts_with("#1") || colors.background.starts_with("#0"),
            "dark theme background should be dark: {}",
            colors.background
        );
    }

    #[test]
    fn font_config_generates_css() {
        let font = FontConfig::default();
        let css = font.to_css();
        assert!(css.contains("font-family:"));
        assert!(css.contains("font-size:"));
    }

    #[test]
    fn font_config_embeds_web_font() {
        let font = FontConfig {
            web_font_url: Some("https://fonts.example.com/font.css".into()),
            ..Default::default()
        };
        let css = font.to_css();
        assert!(css.contains("@import url('https://fonts.example.com/font.css')"));
    }

    #[test]
    fn theme_generates_complete_style() {
        let theme = Theme::from_preset(ThemePreset::Default);
        let style = theme.to_svg_style();
        assert!(style.contains(":root {"));
        assert!(style.contains(".fm-node"));
        assert!(style.contains(".fm-edge"));
        assert!(style.contains(".fm-cluster"));
    }

    #[test]
    fn palette_generates_distinct_colors() {
        let palette = generate_palette("#4285f4", 5);
        assert_eq!(palette.len(), 5);

        // All colors should be valid hex
        for color in &palette {
            assert!(color.starts_with('#'));
            assert_eq!(color.len(), 7);
        }

        // All colors should be distinct
        for i in 0..palette.len() {
            for j in (i + 1)..palette.len() {
                assert_ne!(palette[i], palette[j], "colors should be distinct");
            }
        }
    }

    #[test]
    fn hex_to_hsl_handles_primary_colors() {
        // Red
        let (h, s, l) = hex_to_hsl("#ff0000");
        assert!((h - 0.0).abs() < 1.0);
        assert!((s - 1.0).abs() < 0.01);
        assert!((l - 0.5).abs() < 0.01);

        // Green
        let (h, _, _) = hex_to_hsl("#00ff00");
        assert!((h - 120.0).abs() < 1.0);

        // Blue
        let (h, _, _) = hex_to_hsl("#0000ff");
        assert!((h - 240.0).abs() < 1.0);
    }

    #[test]
    fn hsl_to_hex_round_trips() {
        let original = "#4285f4";
        let (h, s, l) = hex_to_hsl(original);
        let result = hsl_to_hex(h, s, l);
        // Allow small rounding differences
        assert!(
            result.to_lowercase().starts_with("#42") || result.to_lowercase().starts_with("#43"),
            "expected similar color, got {}",
            result
        );
    }
}
