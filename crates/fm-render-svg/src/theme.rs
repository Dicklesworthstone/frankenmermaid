//! SVG theming system for frankenmermaid diagrams.
//!
//! Provides preset themes, CSS custom property generation, and color palette utilities.

use std::{fmt::Write as _, str::FromStr};

/// Local font stack shared by the theme and the SVG render configuration.
///
/// Diagram exports must be readable immediately, including when loaded from disk or converted
/// to an image. Web fonts remain opt-in through [`FontConfig::web_font_url`].
pub const DEFAULT_FONT_FAMILY: &str =
    "system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";

/// Theme preset identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemePreset {
    /// Clean neutral colors (blue/gray) - default mermaid-compatible
    #[default]
    Default,
    /// 1:1 exact visual reproduction of Mermaid.js classic default palette (lavender/purple)
    Mermaid,
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
            "mermaid" | "mermaid-default" | "mermaid-classic" | "classic" => Ok(Self::Mermaid),
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
            Self::Mermaid => "mermaid",
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
                text: "#1e293b".into(),
                node_fill: "#f8fafc".into(),
                node_stroke: "#7c8da5".into(),
                edge: "#64748b".into(),
                cluster_fill: "#f1f5f9".into(),
                cluster_stroke: "#cbd5e1".into(),
                accents: fm_core::mindmap::DEFAULT_BRANCH_ACCENTS.map(String::from),
            },

            ThemePreset::Mermaid => Self {
                background: "#ffffff".into(),
                text: "#333333".into(),
                node_fill: "#ECECFF".into(),
                node_stroke: "#9370DB".into(),
                edge: "#333333".into(),
                cluster_fill: "#ffffde".into(),
                cluster_stroke: "#aaaa33".into(),
                accents: [
                    "#9370DB".into(), // Classic purple
                    "#ECECFF".into(), // Lavender fill
                    "#ffffde".into(), // Pale yellow
                    "#81B1DB".into(), // Actor blue
                    "#d4edbc".into(), // Pale green
                    "#fce4ec".into(), // Pale pink
                    "#666666".into(), // Neutral dark
                    "#aaaa33".into(), // Cluster border
                ],
            },

            ThemePreset::Dark => Self {
                background: "#0f172a".into(),
                text: "#f8fafc".into(),
                node_fill: "#1e293b".into(),
                node_stroke: "#64748b".into(),
                edge: "#94a3b8".into(),
                cluster_fill: "rgba(30,41,59,0.6)".into(),
                cluster_stroke: "#475569".into(),
                // Keep each accent in the same hue family as the default palette so a
                // mindmap branch or shape role keeps its identity when the theme changes.
                accents: [
                    "#818cf8".into(), // Indigo
                    "#38bdf8".into(), // Sky
                    "#2dd4bf".into(), // Teal
                    "#a78bfa".into(), // Violet
                    "#fbbf24".into(), // Amber
                    "#f472b6".into(), // Pink
                    "#34d399".into(), // Emerald
                    "#fb7185".into(), // Rose
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
                node_stroke: "#cbd5e1".into(),
                edge: "#64748b".into(),
                cluster_fill: "rgba(241,245,249,0.5)".into(),
                cluster_stroke: "#94a3b8".into(),
                accents: [
                    "#2563eb".into(), // Blue-600
                    "#0284c7".into(), // Sky-600
                    "#0891b2".into(), // Cyan-600
                    "#4f46e5".into(), // Indigo-600
                    "#7c3aed".into(), // Violet-600
                    "#0d9488".into(), // Teal-600
                    "#059669".into(), // Emerald-600
                    "#4338ca".into(), // Indigo-700
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
        if let Some(color) = vars.get("background").and_then(|value| {
            // `none` means no surface; normalize it to a color usable in CSS and SVG.
            crate::sanitize_svg_paint(if value.trim().eq_ignore_ascii_case("none") {
                "transparent"
            } else {
                value
            })
        }) {
            self.background = color;
        }
        if let Some(color) = vars
            .get("primaryTextColor")
            .or_else(|| vars.get("textColor"))
            .and_then(|value| crate::sanitize_svg_paint(value))
        {
            self.text = color;
        }
        if let Some(color) = vars
            .get("primaryColor")
            .and_then(|value| crate::sanitize_svg_paint(value))
        {
            self.node_fill = color;
        }
        if let Some(color) = vars
            .get("primaryBorderColor")
            .and_then(|value| crate::sanitize_svg_paint(value))
        {
            self.node_stroke = color;
        }
        if let Some(color) = vars
            .get("lineColor")
            .and_then(|value| crate::sanitize_svg_paint(value))
        {
            self.edge = color;
        }
        if let Some(color) = vars
            .get("clusterBkg")
            .and_then(|value| crate::sanitize_svg_paint(value))
        {
            self.cluster_fill = color;
        }
        if let Some(color) = vars
            .get("clusterBorder")
            .and_then(|value| crate::sanitize_svg_paint(value))
        {
            self.cluster_stroke = color;
        }
        for (index, accent) in self.accents.iter_mut().enumerate() {
            let key = format!("pie{}", index + 1);
            if let Some(color) = vars
                .get(&key)
                .and_then(|value| crate::sanitize_svg_paint(value))
            {
                *accent = color;
            }
        }
    }

    /// Generate CSS custom properties for this theme.
    #[must_use]
    pub fn to_css_vars(&self) -> String {
        let mut css = String::with_capacity(512);
        self.write_css_vars(&mut css);
        css
    }

    /// Write the `:root` custom-property block directly into `css`. Byte-identical to
    /// [`Self::to_css_vars`] but writes straight into the caller's buffer, avoiding a temp `String`
    /// per declaration (the `push_str(&format!(..))` anti-pattern) plus the intermediate `String`
    /// and its copy when the render funnel composes the full stylesheet.
    pub(crate) fn write_css_vars(&self, css: &mut String) {
        css.push_str("svg {\n");
        let _ = writeln!(css, "  --fm-bg: {};", self.background);
        let _ = writeln!(css, "  --fm-text-color: {};", self.text);
        let _ = writeln!(css, "  --fm-node-fill: {};", self.node_fill);
        let _ = writeln!(css, "  --fm-node-stroke: {};", self.node_stroke);
        let _ = writeln!(css, "  --fm-edge-color: {};", self.edge);
        let _ = writeln!(css, "  --fm-cluster-fill: {};", self.cluster_fill);
        let _ = writeln!(css, "  --fm-cluster-stroke: {};", self.cluster_stroke);
        for (i, accent) in self.accents.iter().enumerate() {
            let _ = writeln!(css, "  --fm-accent-{}: {};", i + 1, accent);
        }
        css.push_str("}\n");
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
            family: DEFAULT_FONT_FAMILY.into(),
            size: 15.0,
            weight: 500,
            web_font_url: None,
        }
    }
}

impl FontConfig {
    /// Generate CSS for the font configuration.
    #[must_use]
    pub fn to_css(&self) -> String {
        let mut css = String::with_capacity(256);
        self.write_css(&mut css);
        css
    }

    /// Write the inherited SVG font defaults (and optional `@import`) directly into `css`.
    /// Byte-identical to [`Self::to_css`]; writes straight into the caller's buffer to avoid the temp `String` per
    /// declaration and the intermediate `String` + copy on the render path.
    /// ⚠️ EVERY BYTE WRITTEN HERE SHIPS IN EVERY SVG. Explanations belong in Rust comments like
    /// this one, never in a `/* … */` inside the emitted payload.
    ///
    /// bd-d9mw learned that the expensive way: its rationale for the `.fm-node.mindmap-no-border`
    /// rule below was written as a CSS comment, so 931 bytes of prose were embedded in the
    /// stylesheet of every diagram this renderer produced. It showed up as all 37 SVG goldens
    /// changing at once, which is what a global output regression looks like — and it was working
    /// directly against the output dead-weight levers this crate has already landed
    /// (`strip_unused_theme_css`, CSS minification).
    ///
    /// The rationale it carried, kept here where it costs the user nothing: `A` and `A[A]` both
    /// parse to `NodeShape::Rect` and used to render byte-identically, while mermaid 11.15.0 types
    /// them distinctly — its mindmap switch maps DEFAULT to the shape name "no-border" and RECT to
    /// "rect", with DEFAULT:0 and NO_BORDER:0 sharing an enum value. The incumbent draws the
    /// difference in CSS rather than geometry (it emits `node-bkg node-no-border`), which is why
    /// the Rust side needs a marker class and no new `NodeShape` variant. The rule is scoped under
    /// `.fm-node` so it can only affect a node carrying the marker, and placed after the shape rule
    /// so it overrides that rule's `stroke` rather than relying on specificity. `fill` is left
    /// ALONE deliberately: mermaid's default node keeps its section fill and drops only the border.
    pub(crate) fn write_css(&self, css: &mut String) {
        // Embed web font if provided
        if let Some(url) = &self.web_font_url {
            css.push_str("@import url('");
            css.extend(
                url.chars()
                    .filter(|&c| c != '\'' && c != ')' && c != '\\' && c != '"'),
            );
            css.push_str("');\n");
        }

        // Defaults belong on the root. A selector targeting a text element wins over its SVG
        // presentation attributes, replacing fitted sizes and specialized heading weights with
        // the base size. Inheritance lets each label keep the measurements used by layout.
        css.push_str("svg {\n");
        let _ = writeln!(css, "  font-family: {};", self.family);
        let _ = writeln!(css, "  font-size: {}px;", self.size);
        let _ = writeln!(css, "  font-weight: {};", self.weight);
        css.push_str("}\n");
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
    pub fn to_svg_style(&self, shadows: bool, has_edge_labels: bool) -> String {
        let mut css = String::with_capacity(4096);
        // An explicitly requested web-font import must precede all ordinary CSS rules.
        self.font.write_css(&mut css);
        self.colors.write_css_vars(&mut css);

        let shadow_filter = if shadows {
            "filter:drop-shadow(0 1px 2px rgba(15,23,42,.06));"
        } else {
            ""
        };

        // Hover feedback is stroke-accent + shadow ONLY — never a transform (GH#6). A CSS
        // transform on SVG shape children resolves `transform-origin: center` against the
        // view-box (these rules set no `transform-box: fill-box`), so even a 1% scale
        // displaced off-center nodes by view-box-relative distances and made boxes jump on
        // hover/unhover. Motion belongs to the opt-in `.fm-animations-enabled` group hover,
        // which scales correctly via `transform-box: fill-box`.
        let hover_shadow_filter = if shadows {
            "filter:drop-shadow(0 2px 4px rgba(15,23,42,.10));"
        } else {
            ""
        };

        // A quiet canvas and continuous cluster boundaries preserve the hierarchy between a
        // diagram's content and its decoration. Shadows must not change grouping semantics.
        // Keep typography inherited: individual SVG font-size/font-weight/fill attributes carry
        // the node fitting and the class/C4 heading hierarchy, and a text selector overrides them.
        let _ = write!(
            css,
            r"svg{{--fm-edge-muted:var(--fm-edge-color);--fm-edge-label-bg:var(--fm-bg);--fm-edge-label-border:none;--fm-edge-label-text:var(--fm-text-color);--fm-cluster-label-color:var(--fm-text-color);--fm-cluster-c4-fill:var(--fm-cluster-fill);--fm-cluster-c4-stroke:var(--fm-cluster-stroke);--fm-cluster-swimlane-fill:var(--fm-cluster-fill);--fm-cluster-swimlane-stroke:var(--fm-cluster-stroke);--fm-surface-shadow:rgba(15,23,42,.06)}}
svg{{shape-rendering:geometricPrecision;background:var(--fm-bg)}}
.fm-node{{isolation:isolate;fill:var(--fm-node-fill);--fm-node-accent:var(--fm-node-stroke);--fm-node-hover-accent:var(--fm-edge-color)}}
.fm-node rect,.fm-node path,.fm-node circle,.fm-node ellipse,.fm-node polygon{{stroke:var(--fm-node-accent);stroke-width:1.25;vector-effect:non-scaling-stroke;shape-rendering:geometricPrecision;{shadow_filter}transition:fill 150ms ease,stroke 150ms ease,filter 150ms ease}}
.fm-node.fm-node-user-mindmap-no-border :where(rect,path,polygon){{stroke:none}}
.fm-node line{{stroke:var(--fm-node-accent);stroke-width:1.25;vector-effect:non-scaling-stroke}}
.fm-node text{{text-rendering:optimizeLegibility;font-kerning:normal}}
.fm-node:hover rect,.fm-node:hover path,.fm-node:hover circle,.fm-node:hover ellipse,.fm-node:hover polygon{{stroke:var(--fm-node-hover-accent);{hover_shadow_filter}}}
"
        );

        // Stable identity classes are useful to interactive hosts, but an arbitrary identifier
        // is not a category. Reserve their colors for hover rather than painting unrelated nodes
        // as if they encoded eight different types. Diagram-specific semantic fills stay intact.
        for index in 1..=8 {
            let _ = write!(
                css,
                ".fm-node-accent-{index}{{--fm-node-hover-accent:var(--fm-accent-{index})}}"
            );
        }
        // A mindmap branch is a semantic group: descendants inherit the first-level branch's
        // palette. Set local theme variables so author classDef and inline paints still win.
        for (branch, accent) in self.colors.accents.iter().enumerate() {
            let fill = fm_core::mindmap::branch_fill(&self.colors.node_fill, accent, false);
            let index = branch + 1;
            let _ = write!(
                css,
                ".fm-node-user-mindmap-branch-{branch}{{--fm-node-fill:{fill};--fm-mindmap-fill:{fill};--fm-node-accent:var(--fm-accent-{index});--fm-node-hover-accent:var(--fm-accent-{index})}}"
            );
        }
        let root_fill =
            fm_core::mindmap::branch_fill(&self.colors.node_fill, &self.colors.accents[0], true);
        let _ = write!(
            css,
            ".fm-node-user-mindmap-root{{--fm-node-fill:{root_fill};--fm-mindmap-fill:{root_fill}}}.fm-node-user-mindmap-root text{{font-weight:600}}"
        );
        // Mindmap fills encode branches; hollow shape details must remain hollow.
        // Keep specificity low enough for later author classDef paint rules to win.
        css.push_str(
            r#".fm-node:where(.fm-node-user-mindmap-root,[class*="fm-node-user-mindmap-branch-"]) :where(rect,path,circle,ellipse,polygon):not([fill="none"]){fill:var(--fm-mindmap-fill)}
"#,
        );
        css.push_str(
            r".fm-node.fm-node-shape-star :where(path),.fm-node.fm-node-shape-pentagon :where(path){stroke-width:1.8}
.fm-edge{fill:none;stroke:var(--fm-edge-color);stroke-linecap:round;stroke-linejoin:round;vector-effect:non-scaling-stroke;paint-order:stroke;transition:stroke 150ms ease,opacity 150ms ease,stroke-width 150ms ease;cursor:default}
.fm-edge:hover{stroke:var(--fm-accent-1);opacity:1}
.fm-edge-solid{stroke-dasharray:none}
.fm-edge-dashed{stroke-dasharray:5 5}
.fm-edge-animation-fast,.fm-edge-animation-slow{stroke-dasharray:9 5!important;stroke-dashoffset:900;stroke-linecap:round}
.fm-edge-animation-fast{animation:fm-edge-dash-march 20s linear infinite}
.fm-edge-animation-slow{animation:fm-edge-dash-march 50s linear infinite}
@keyframes fm-edge-dash-march{to{stroke-dashoffset:0}}
@media (prefers-reduced-motion:reduce){.fm-edge-animation-fast,.fm-edge-animation-slow{animation:none!important}}
.fm-edge-back{stroke:var(--fm-edge-muted);opacity:.8;stroke-dasharray:4 4}
marker#arrow-end path,marker#arrow-filled path,marker#arrow-circle path,marker#arrow-diamond path{fill:var(--fm-edge-color);stroke:none;transition:fill 150ms ease}
.fm-edge:hover~marker#arrow-end path,.fm-edge:hover~marker#arrow-filled path,.fm-edge:hover~marker#arrow-circle path,.fm-edge:hover~marker#arrow-diamond path{fill:var(--fm-accent-1)}
marker#arrow-open path{stroke:var(--fm-edge-muted);fill:none;stroke-width:1.5;transition:stroke 150ms ease}
marker#arrow-cross path{stroke:var(--fm-edge-color);fill:none;stroke-width:1.5;transition:stroke 150ms ease}
.fm-cluster{fill:var(--fm-cluster-fill);stroke:var(--fm-cluster-stroke);stroke-width:1}
.fm-cluster-label{fill:var(--fm-cluster-label-color);font-weight:600}
.fm-cluster-c4{fill:var(--fm-cluster-c4-fill);stroke:var(--fm-cluster-c4-stroke)}
.fm-cluster-swimlane{fill:var(--fm-cluster-swimlane-fill);stroke:var(--fm-cluster-swimlane-stroke)}
.fm-label{fill:var(--fm-text-color)}
@media (prefers-reduced-motion:reduce){.fm-node rect,.fm-node path,.fm-node circle,.fm-node ellipse,.fm-node polygon,.fm-edge{transition:none;transform:none}}
.fm-node:focus-visible{outline:2px solid var(--fm-accent-1);outline-offset:3px}",
        );

        // Edge-label rules apply only to `.fm-edge-labeled`/`.edge-label` elements, emitted only
        // when an edge actually carries a label. Appending them conditionally keeps otherwise-dead
        // CSS out of the common unlabeled-edge SVG. Order is irrelevant (distinct selectors).
        if has_edge_labels {
            css.push_str(
                r"
.fm-edge-labeled > rect {
  fill: var(--fm-edge-label-bg);
  stroke: var(--fm-edge-label-border);
  stroke-width: 0.75;
  rx: 4px;
  ry: 4px;
}
.edge-label {
  fill: var(--fm-edge-label-text);
  font-weight: 400;
  text-rendering: optimizeLegibility;
}
",
            );
        }

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
    let (r, g, b) = if hex.len() == 3 && hex.is_ascii() {
        let r = u8::from_str_radix(&hex[0..1], 16).unwrap_or(0) as f32 / 15.0;
        let g = u8::from_str_radix(&hex[1..2], 16).unwrap_or(0) as f32 / 15.0;
        let b = u8::from_str_radix(&hex[2..3], 16).unwrap_or(0) as f32 / 15.0;
        // Correct 3-digit hex: repeat the digit
        (r, g, b)
    } else if hex.len() >= 6 && hex.is_ascii() {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(128) as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(128) as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(128) as f32 / 255.0;
        (r, g, b)
    } else {
        return (0.0, 0.0, 0.5); // fallback gray
    };

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = f32::midpoint(max, min);

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

    let sector = (h / 60.0).floor().rem_euclid(6.0) as u8;
    let (r, g, b) = match sector {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let r = ((r + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = ((g + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = ((b + m).clamp(0.0, 1.0) * 255.0).round() as u8;

    format!("#{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn css_rule<'a>(css: &'a str, selector: &str) -> &'a str {
        let marker = format!("{selector}{{");
        css.split_once(&marker)
            .expect("theme must emit the selected rule")
            .1
            .split_once('}')
            .expect("theme rule must be complete")
            .0
    }

    fn luminance(hex: &str) -> f64 {
        let hex = hex.strip_prefix('#').expect("palette color must be hex");
        let channel = |offset| {
            let srgb = f64::from(
                u8::from_str_radix(&hex[offset..offset + 2], 16)
                    .expect("palette channel must be hexadecimal"),
            ) / 255.0;
            if srgb <= 0.04045 {
                srgb / 12.92
            } else {
                ((srgb + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
    }

    fn contrast_ratio(foreground: &str, background: &str) -> f64 {
        let a = luminance(foreground);
        let b = luminance(background);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn default_palette_keeps_labels_readable_on_every_surface() {
        let colors = ThemeColors::default();
        for background in [&colors.background, &colors.node_fill, &colors.cluster_fill] {
            assert!(
                contrast_ratio(&colors.text, background) >= 7.0,
                "default labels must remain readable on {background}"
            );
            assert!(
                contrast_ratio(&colors.edge, background) >= 3.0,
                "connectors must remain distinguishable on {background}"
            );
            assert!(
                contrast_ratio(&colors.node_stroke, background) >= 3.0,
                "node outlines must remain distinguishable on {background}"
            );
        }
    }

    #[test]
    fn semantic_shape_palettes_keep_text_and_outlines_readable() {
        for preset in [ThemePreset::Default, ThemePreset::Dark] {
            let colors = ThemeColors::from_preset(preset);
            assert!(contrast_ratio(&colors.node_stroke, &colors.node_fill) >= 3.0);
            let mut fills = std::collections::BTreeSet::new();
            for shape in [
                fm_core::NodeShape::Diamond,
                fm_core::NodeShape::Stadium,
                fm_core::NodeShape::Cylinder,
                fm_core::NodeShape::Cloud,
            ] {
                let style = fm_core::semantic_shape_style(
                    fm_core::DiagramType::Flowchart,
                    shape,
                    &colors.node_fill,
                    &colors.node_stroke,
                    &colors.text,
                    &colors.accents,
                )
                .expect("default palettes must support semantic shape colors");
                assert_ne!(style.fill, colors.node_fill);
                assert!(
                    contrast_ratio(&colors.text, &style.fill) >= 4.5,
                    "{preset:?} {shape:?} must retain readable labels"
                );
                assert!(
                    contrast_ratio(&style.stroke, &style.fill) >= 3.0,
                    "{preset:?} {shape:?} must retain a visible outline"
                );
                fills.insert(style.fill);
            }
            assert_eq!(fills.len(), 4, "shape roles must have distinct surfaces");
        }
    }

    #[test]
    fn switching_to_dark_preserves_branch_and_shape_role_hues() {
        let light = ThemeColors::default();
        let dark = ThemeColors::from_preset(ThemePreset::Dark);
        for (index, (light_accent, dark_accent)) in
            light.accents.iter().zip(&dark.accents).enumerate()
        {
            let hue_difference = (hex_to_hsl(light_accent).0 - hex_to_hsl(dark_accent).0).abs();
            let hue_distance = hue_difference.min(360.0 - hue_difference);
            assert!(
                hue_distance <= 20.0,
                "accent slot {index} must keep its hue family across themes"
            );
            assert!(
                luminance(dark_accent) > luminance(light_accent),
                "accent slot {index} must brighten against the dark background"
            );
        }
    }

    #[test]
    fn font_defaults_do_not_override_fitted_labels_or_heading_hierarchy() {
        let theme = Theme {
            font: FontConfig {
                family: "Georgia, serif".to_string(),
                size: 19.0,
                weight: 450,
                web_font_url: None,
            },
            ..Theme::default()
        };
        let css = crate::minify_css(&theme.to_svg_style(true, true));
        let root = css_rule(&css, "svg");
        assert!(root.contains("font-family: Georgia,serif"));
        assert!(root.contains("font-size: 19px"));
        assert!(root.contains("font-weight: 450"));
        assert_eq!(
            css.matches("font-size:").count(),
            1,
            "only the inherited root default may set a font size; text attributes carry fitting"
        );
        let node_text = css_rule(&css, ".fm-node text");
        assert!(!node_text.contains("font-weight:"));
        assert!(!node_text.contains("fill:"));
        assert!(css_rule(&css, ".edge-label").contains("font-weight: 400"));
        assert!(css_rule(&css, ".fm-cluster-label").contains("font-weight:600"));
    }

    #[test]
    fn default_diagrams_need_no_remote_font_and_explicit_imports_are_valid() {
        let default_css = Theme::default().to_svg_style(false, false);
        assert!(!default_css.contains("@import"));
        assert!(default_css.contains(DEFAULT_FONT_FAMILY));

        let theme = Theme {
            font: FontConfig {
                web_font_url: Some("https://fonts.example.com/font.css".to_string()),
                ..FontConfig::default()
            },
            ..Theme::default()
        };
        assert!(
            theme
                .to_svg_style(false, false)
                .starts_with("@import url('https://fonts.example.com/font.css');"),
            "CSS imports placed after ordinary rules are ignored by browsers"
        );
    }

    #[test]
    fn node_identity_does_not_change_its_resting_color() {
        for preset in [
            ThemePreset::Default,
            ThemePreset::Dark,
            ThemePreset::Monochrome,
        ] {
            let css = crate::minify_css(&Theme::from_preset(preset).to_svg_style(false, false));
            assert!(css_rule(&css, ".fm-node").contains("--fm-node-accent:var(--fm-node-stroke)"));
            for index in 1..=8 {
                let rule = css_rule(&css, &format!(".fm-node-accent-{index}"));
                assert!(rule.contains("--fm-node-hover-accent:"));
                assert!(!rule.contains("--fm-node-accent:"));
            }
            assert!(!css.contains("color-mix("));
        }
    }

    #[test]
    fn theme_preserves_explicit_shape_paints_and_hollow_details() {
        let shape_selector =
            ".fm-node rect,.fm-node path,.fm-node circle,.fm-node ellipse,.fm-node polygon";
        let mindmap_selector = concat!(
            ".fm-node:where(.fm-node-user-mindmap-root,",
            "[class*=\"fm-node-user-mindmap-branch-\"]) ",
            ":where(rect,path,circle,ellipse,polygon):not([fill=\"none\"])",
        );

        for preset in [ThemePreset::Default, ThemePreset::Dark] {
            let css = crate::minify_css(&Theme::from_preset(preset).to_svg_style(false, false));
            assert!(css_rule(&css, ".fm-node").contains("fill:var(--fm-node-fill)"));
            assert!(
                !css_rule(&css, shape_selector)
                    .split(';')
                    .any(|declaration| declaration.trim_start().starts_with("fill:")),
                "generic shape rules must preserve explicit solid, gradient, and hollow paints"
            );
            assert_eq!(
                css_rule(&css, mindmap_selector),
                "fill:var(--fm-mindmap-fill)",
                "mindmap branches retain semantic fills without repainting hollow details"
            );
        }
    }

    #[test]
    fn mindmap_tints_keep_labels_readable_in_light_and_dark_presets() {
        for preset in [
            ThemePreset::Default,
            ThemePreset::Mermaid,
            ThemePreset::Dark,
            ThemePreset::Pastel,
            ThemePreset::Monochrome,
        ] {
            let colors = ThemeColors::from_preset(preset);
            for accent in &colors.accents {
                for is_root in [false, true] {
                    let fill = fm_core::mindmap::branch_fill(&colors.node_fill, accent, is_root);
                    let (foreground, background) = (luminance(&colors.text), luminance(&fill));
                    let contrast =
                        (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05);
                    assert!(
                        contrast >= 4.5,
                        "{preset:?}: {fill} against {}",
                        colors.text
                    );
                }
            }
        }
    }

    #[test]
    fn shadow_toggle_preserves_grouping_and_edge_label_clarity() {
        let theme = Theme::default();
        let with_shadows = crate::minify_css(&theme.to_svg_style(true, true));
        let without_shadows = crate::minify_css(&theme.to_svg_style(false, true));
        assert!(with_shadows.contains("drop-shadow("));
        assert!(!without_shadows.contains("drop-shadow("));
        assert_eq!(
            css_rule(&with_shadows, ".fm-cluster"),
            css_rule(&without_shadows, ".fm-cluster")
        );
        for selector in [".fm-cluster", ".fm-cluster-c4", ".fm-cluster-swimlane"] {
            let rule = css_rule(&with_shadows, selector);
            assert!(!rule.contains("stroke-dasharray:"));
            assert!(!rule.contains("rx:"));
            assert!(!rule.contains("ry:"));
        }
        for css in [&with_shadows, &without_shadows] {
            assert!(css.contains("--fm-edge-label-border:none"));
            assert!(!css.contains("backdrop-filter"));
            assert!(!css.contains("background-image"));
        }
    }

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
            ThemePreset::Mermaid,
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
                "preset {name:?} should round-trip"
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
    fn theme_overrides_ignore_invalid_colors() {
        let mut colors = ThemeColors::from_preset(ThemePreset::Default);
        let original = colors.node_fill.clone();

        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert(
            "primaryColor".to_string(),
            "red; } .fm-node { fill: url(javascript:alert(1))".to_string(),
        );

        colors.apply_overrides(&overrides);
        assert_eq!(colors.node_fill, original);
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
        // The pipeline runs the assembled stylesheet through the whitespace-only minifier
        // before embedding; assert against the same transform so the contract is format-
        // stable rather than coupled to the writer's indentation.
        let style = crate::minify_css(&theme.to_svg_style(true, true));
        assert!(style.contains("svg{--fm-bg:"));
        assert!(
            !style.contains(":root{--fm-bg:"),
            "embedded theme variables must stay scoped to their SVG instead of leaking into the \
             host document"
        );
        assert!(style.contains(".fm-node"));
        assert!(style.contains(".fm-edge"));
        assert!(style.contains(".fm-cluster"));
        assert!(style.contains(".fm-node-accent-1"));
        assert!(style.contains(".fm-node.fm-node-shape-star"));
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
            "expected similar color, got {result}"
        );
    }
}
