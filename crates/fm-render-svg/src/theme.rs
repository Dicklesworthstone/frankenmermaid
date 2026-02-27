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
                background: "#f9fafb".into(),
                text: "#111827".into(),
                node_fill: "#ffffff".into(),
                node_stroke: "#e5e7eb".into(),
                edge: "#9ca3af".into(),
                cluster_fill: "rgba(243,244,246,0.5)".into(),
                cluster_stroke: "#d1d5db".into(),
                accents: [
                    "#3b82f6".into(), // Blue
                    "#0ea5e9".into(), // Sky
                    "#10b981".into(), // Emerald
                    "#8b5cf6".into(), // Violet
                    "#f59e0b".into(), // Amber
                    "#f43f5e".into(), // Rose
                    "#14b8a6".into(), // Teal
                    "#6366f1".into(), // Indigo
                ],
            },

            ThemePreset::Dark => Self {
                background: "#0f172a".into(),
                text: "#f8fafc".into(),
                node_fill: "#1e293b".into(),
                node_stroke: "#334155".into(),
                edge: "#94a3b8".into(),
                cluster_fill: "rgba(30,41,59,0.6)".into(),
                cluster_stroke: "#475569".into(),
                accents: [
                    "#38bdf8".into(), // Sky
                    "#34d399".into(), // Emerald
                    "#a78bfa".into(), // Violet
                    "#fbbf24".into(), // Amber
                    "#fb7185".into(), // Rose
                    "#2dd4bf".into(), // Teal
                    "#818cf8".into(), // Indigo
                    "#f472b6".into(), // Pink
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
                background: "#f8fafc".into(),
                text: "#0f172a".into(),
                node_fill: "#ffffff".into(),
                node_stroke: "#93c5fd".into(), // blue-300
                edge: "#64748b".into(),        // slate-500
                cluster_fill: "rgba(239,246,255,0.6)".into(), // blue-50
                cluster_stroke: "#bfdbfe".into(), // blue-200
                accents: [
                    "#1d4ed8".into(), // blue-700
                    "#0369a1".into(), // sky-700
                    "#0f766e".into(), // teal-700
                    "#4338ca".into(), // indigo-700
                    "#1e40af".into(), // blue-800
                    "#0ea5e9".into(), // sky-500
                    "#3b82f6".into(), // blue-500
                    "#6366f1".into(), // indigo-500
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
            family: "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif"
                .into(),
            size: 15.0,
            weight: 500,
            web_font_url: Some("https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap".into()),
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
    pub fn to_svg_style(&self, shadows: bool) -> String {
        let mut css = String::with_capacity(4096);
        css.push_str(&self.colors.to_css_vars());
        css.push_str(&self.font.to_css());

        let shadow_filter = if shadows {
            "filter: drop-shadow(0 4px 6px rgba(0, 0, 0, 0.04)) drop-shadow(0 1px 3px rgba(0, 0, 0, 0.02));"
        } else {
            ""
        };

        let hover_shadow_filter = if shadows {
            "filter: drop-shadow(0 10px 15px rgba(0, 0, 0, 0.06)) drop-shadow(0 4px 6px rgba(0, 0, 0, 0.04));"
        } else {
            ""
        };

        // Add utility classes
        css.push_str(
            &format!(r#"
:root {{
  --fm-edge-muted: var(--fm-cluster-stroke);
  --fm-edge-label-bg: var(--fm-bg);
  --fm-edge-label-border: var(--fm-cluster-stroke);
  --fm-edge-label-text: var(--fm-text-color);
  --fm-cluster-label-color: var(--fm-text-color);
  --fm-cluster-c4-fill: var(--fm-cluster-fill);
  --fm-cluster-c4-stroke: var(--fm-cluster-stroke);
  --fm-cluster-swimlane-fill: var(--fm-cluster-fill);
  --fm-cluster-swimlane-stroke: var(--fm-cluster-stroke);
  --fm-surface-shadow: rgba(15, 23, 42, 0.1);
}}
svg {{
  background: var(--fm-bg);
  background-image: linear-gradient(
    180deg,
    var(--fm-bg) 0%,
    color-mix(in srgb, var(--fm-bg) 92%, var(--fm-node-stroke) 8%) 100%
  );
}}
.fm-node {{
  isolation: isolate;
  --fm-node-accent: var(--fm-node-stroke);
  --fm-node-hover-accent: var(--fm-edge-color);
}}
.fm-node rect,
.fm-node path,
.fm-node circle,
.fm-node ellipse,
.fm-node polygon {{
  fill: var(--fm-node-fill);
  stroke: var(--fm-node-accent);
  stroke-width: 1.5;
  vector-effect: non-scaling-stroke;
  shape-rendering: geometricPrecision;
  {}
  transition: fill 200ms ease, stroke 200ms ease, filter 200ms ease, transform 200ms cubic-bezier(0.4, 0, 0.2, 1);
}}
.fm-node line {{
  stroke: var(--fm-node-accent);
  stroke-width: 1.5;
  vector-effect: non-scaling-stroke;
}}
.fm-node text {{
  fill: var(--fm-text-color);
  font-weight: 500;
  letter-spacing: -0.01em;
  text-rendering: optimizeLegibility;
  font-feature-settings: "kern" 1, "liga" 1;
}}
.fm-node:hover rect,
.fm-node:hover path,
.fm-node:hover circle,
.fm-node:hover ellipse,
.fm-node:hover polygon {{
  stroke: var(--fm-node-hover-accent);
  {}
  transform: translateY(-1px);
}}
.fm-node-accent-1 {{
  --fm-node-accent: var(--fm-node-stroke);
  --fm-node-accent: color-mix(in srgb, var(--fm-accent-1) 50%, var(--fm-node-stroke));
  --fm-node-hover-accent: var(--fm-accent-1);
}}
.fm-node-accent-2 {{
  --fm-node-accent: var(--fm-node-stroke);
  --fm-node-accent: color-mix(in srgb, var(--fm-accent-2) 50%, var(--fm-node-stroke));
  --fm-node-hover-accent: var(--fm-accent-2);
}}
.fm-node-accent-3 {{
  --fm-node-accent: var(--fm-node-stroke);
  --fm-node-accent: color-mix(in srgb, var(--fm-accent-3) 50%, var(--fm-node-stroke));
  --fm-node-hover-accent: var(--fm-accent-3);
}}
.fm-node-accent-4 {{
  --fm-node-accent: var(--fm-node-stroke);
  --fm-node-accent: color-mix(in srgb, var(--fm-accent-4) 50%, var(--fm-node-stroke));
  --fm-node-hover-accent: var(--fm-accent-4);
}}
.fm-node-accent-5 {{
  --fm-node-accent: var(--fm-node-stroke);
  --fm-node-accent: color-mix(in srgb, var(--fm-accent-5) 50%, var(--fm-node-stroke));
  --fm-node-hover-accent: var(--fm-accent-5);
}}
.fm-node-accent-6 {{
  --fm-node-accent: var(--fm-node-stroke);
  --fm-node-accent: color-mix(in srgb, var(--fm-accent-6) 50%, var(--fm-node-stroke));
  --fm-node-hover-accent: var(--fm-accent-6);
}}
.fm-node-accent-7 {{
  --fm-node-accent: var(--fm-node-stroke);
  --fm-node-accent: color-mix(in srgb, var(--fm-accent-7) 50%, var(--fm-node-stroke));
  --fm-node-hover-accent: var(--fm-accent-7);
}}
.fm-node-accent-8 {{
  --fm-node-accent: var(--fm-node-stroke);
  --fm-node-accent: color-mix(in srgb, var(--fm-accent-8) 50%, var(--fm-node-stroke));
  --fm-node-hover-accent: var(--fm-accent-8);
}}
.fm-node.fm-node-shape-note path,
.fm-node.fm-node-shape-note rect {{
  fill: var(--fm-node-fill);
  fill: color-mix(in srgb, #fef3c7 40%, var(--fm-node-fill));
}}
.fm-node.fm-node-shape-cloud path {{
  fill: var(--fm-node-fill);
  fill: color-mix(in srgb, var(--fm-accent-2) 15%, var(--fm-node-fill));
}}
.fm-node.fm-node-shape-cylinder path {{
  fill: var(--fm-node-fill);
  fill: color-mix(in srgb, var(--fm-accent-1) 12%, var(--fm-node-fill));
}}
.fm-node.fm-node-shape-star path,
.fm-node.fm-node-shape-pentagon path {{
  stroke-width: 1.8;
}}
.fm-edge {{
  fill: none;
  stroke: var(--fm-edge-color);
  stroke-linecap: round;
  stroke-linejoin: round;
  vector-effect: non-scaling-stroke;
  transition: stroke 200ms ease, opacity 200ms ease, stroke-width 200ms ease;
  cursor: crosshair;
}}
.fm-edge:hover {{
  stroke: var(--fm-accent-1);
  stroke-width: 2.5;
  opacity: 1;
}}
.fm-edge-solid {{
  stroke-dasharray: none;
}}
.fm-edge-dashed {{
  stroke-dasharray: 6 6;
}}
.fm-edge-thick {{
  stroke-width: 2.5;
}}
.fm-edge-thick:hover {{
  stroke-width: 3.5;
}}
.fm-edge-back {{
  stroke: var(--fm-edge-muted);
  opacity: 0.8;
  stroke-dasharray: 4 4;
}}
.fm-edge-labeled > rect {{
  fill: var(--fm-edge-label-bg);
  stroke: var(--fm-edge-label-border);
  stroke-width: 1.5;
  rx: 6px;
  ry: 6px;
}}
/* Add a backdrop filter for modern glassmorphism effect on the rect */
@supports (backdrop-filter: blur(4px)) {{
  .fm-edge-labeled > rect {{
    fill: color-mix(in srgb, var(--fm-edge-label-bg) 85%, transparent);
    backdrop-filter: blur(8px);
  }}
}}
.edge-label {{
  fill: var(--fm-edge-label-text);
  font-weight: 500;
  font-size: 0.95em;
  letter-spacing: -0.01em;
  text-rendering: optimizeLegibility;
}}
marker#arrow-end path,
marker#arrow-filled path,
marker#arrow-circle path,
marker#arrow-diamond path {{
  fill: var(--fm-edge-color);
  stroke: none;
  transition: fill 200ms ease;
}}
.fm-edge:hover ~ marker#arrow-end path,
.fm-edge:hover ~ marker#arrow-filled path,
.fm-edge:hover ~ marker#arrow-circle path,
.fm-edge:hover ~ marker#arrow-diamond path {{
  fill: var(--fm-accent-1);
}}
marker#arrow-open path {{
  stroke: var(--fm-edge-muted);
  fill: none;
  stroke-width: 1.8;
  transition: stroke 200ms ease;
}}
marker#arrow-cross path {{
  stroke: var(--fm-edge-color);
  fill: none;
  stroke-width: 1.8;
  transition: stroke 200ms ease;
}}
.fm-cluster {{
  fill: var(--fm-cluster-fill);
  stroke: var(--fm-cluster-stroke);
  stroke-width: 1.5;
  stroke-dasharray: 8 4;
}}
.fm-cluster-label {{
  fill: var(--fm-cluster-label-color);
  font-weight: 600;
  letter-spacing: -0.01em;
}}
.fm-cluster-c4 {{
  fill: var(--fm-cluster-c4-fill);
  stroke: var(--fm-cluster-c4-stroke);
  stroke-dasharray: none;
}}
.fm-cluster-swimlane {{
  fill: var(--fm-cluster-swimlane-fill);
  stroke: var(--fm-cluster-swimlane-stroke);
  stroke-dasharray: none;
}}
.fm-label {{
  fill: var(--fm-text-color);
}}
@media (prefers-reduced-motion: reduce) {{
  .fm-node rect,
  .fm-node path,
  .fm-node circle,
  .fm-node ellipse,
  .fm-node polygon,
  .fm-edge {{
    transition: none;
    transform: none;
  }}
}}
"#, shadow_filter, hover_shadow_filter)
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
    if hex.len() != 6 || !hex.is_ascii() {
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
        let style = theme.to_svg_style(true);
        assert!(style.contains(":root {"));
        assert!(style.contains(".fm-node"));
        assert!(style.contains(".fm-edge"));
        assert!(style.contains(".fm-cluster"));
        assert!(style.contains(".fm-node-accent-1"));
        assert!(style.contains(".fm-node.fm-node-shape-note"));
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
