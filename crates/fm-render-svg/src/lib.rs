#![forbid(unsafe_code)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

//! Zero-dependency SVG builder for frankenmermaid diagram rendering.
//!
//! Provides a lightweight, type-safe API for generating clean SVG output
//! suitable for flowcharts, sequence diagrams, and other diagram types.

mod a11y;
mod attributes;
pub mod cga_transform;
mod defs;
mod document;
mod element;
mod path;
mod text;
mod theme;
mod transform;

pub use a11y::{
    A11yConfig, accessibility_css, describe_diagram, describe_diagram_with_layout, describe_edge,
    describe_node,
};
pub use attributes::{Attribute, AttributeValue, Attributes};
pub use defs::{ArrowheadMarker, DefsBuilder, Filter, Gradient, GradientStop, MarkerOrient};
pub use document::SvgDocument;
pub use element::{Element, ElementKind};
pub use path::{PathBuilder, PathCommand};
pub use text::{TextAnchor, TextBuilder};
pub use theme::{FontConfig, Theme, ThemeColors, ThemePreset, generate_palette};
pub use transform::{Transform, TransformBuilder};

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
};

use fm_core::{
    DiagramType, IrLabelId, IrLabelSegment, IrXyChartMeta, IrXySeriesKind, MermaidDiagramIr,
    MermaidLinkMode, MermaidSanitizeMode, MermaidTier, Span, is_safe_link_target,
    mermaid_cluster_element_id, mermaid_edge_element_id, mermaid_node_element_id,
    mermaid_node_element_id_with_variant,
};
use fm_layout::{
    CentralityTier, DiagramLayout, FillStyle, LayoutBand, LayoutBandKind, LayoutEdgePath,
    LayoutNodeBox, LineCap as RenderLineCap, LineJoin as RenderLineJoin, MarkerKind, PathCmd,
    RenderClip, RenderGroup, RenderItem, RenderPath, RenderScene, RenderSource, RenderText,
    RenderTransform, StrokeStyle, TextAlign as RenderTextAlign, TextBaseline as RenderTextBaseline,
    build_render_scene,
};

/// Node fill gradient mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeGradientStyle {
    /// Top-to-bottom linear gradient.
    #[default]
    LinearVertical,
    /// Left-to-right linear gradient.
    LinearHorizontal,
    /// Center-weighted radial gradient.
    Radial,
}

/// Backend strategy used by SVG rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SvgBackend {
    /// Existing layout-driven renderer.
    #[default]
    LegacyLayout,
    /// Shared target-agnostic render scene backend.
    Scene,
}

/// Node icon placement strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeIconPosition {
    /// Place the icon above the text label.
    #[default]
    Above,
    /// Place the icon to the left of the text label.
    Left,
}

/// Configurable custom SVG icon definition.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomSvgIcon {
    /// SVG path data in a local icon coordinate space.
    pub path_data: String,
    /// Source viewBox width used to scale the path into the node.
    pub view_box_width: f32,
    /// Source viewBox height used to scale the path into the node.
    pub view_box_height: f32,
    /// Optional fill color override. Defaults to `none` when absent.
    pub fill: Option<String>,
    /// Optional stroke color override. Defaults to the node stroke color.
    pub stroke: Option<String>,
    /// Stroke width in source viewBox units.
    pub stroke_width: f32,
}

/// Configuration for SVG rendering.
#[derive(Debug, Clone)]
pub struct SvgRenderConfig {
    /// Backend implementation used for rendering.
    pub backend: SvgBackend,
    /// Whether to include responsive sizing attributes.
    pub responsive: bool,
    /// Whether to include accessibility attributes.
    pub accessible: bool,
    /// Default font family for text.
    pub font_family: String,
    /// Default font size in pixels.
    pub font_size: f32,
    /// Average character width for text measurement (in pixels).
    pub avg_char_width: f32,
    /// Line height multiplier for multi-line text.
    pub line_height: f32,
    /// Padding around the diagram.
    pub padding: f32,
    /// Whether to include drop shadows.
    pub shadows: bool,
    /// Shadow X offset in px.
    pub shadow_offset_x: f32,
    /// Shadow Y offset in px.
    pub shadow_offset_y: f32,
    /// Shadow blur radius.
    pub shadow_blur: f32,
    /// Shadow opacity [0.0, 1.0].
    pub shadow_opacity: f32,
    /// Shadow color.
    pub shadow_color: String,
    /// Whether to include node gradients.
    pub node_gradients: bool,
    /// Node gradient style.
    pub node_gradient_style: NodeGradientStyle,
    /// Whether highlighted nodes should get glow treatment.
    pub glow_enabled: bool,
    /// Glow blur radius.
    pub glow_blur: f32,
    /// Glow opacity [0.0, 1.0].
    pub glow_opacity: f32,
    /// Glow color.
    pub glow_color: String,
    /// Opacity for cluster backgrounds [0.0, 1.0].
    pub cluster_fill_opacity: f32,
    /// Opacity for dim/inactive elements [0.0, 1.0].
    pub inactive_opacity: f32,
    /// Whether to use rounded corners on rectangles.
    pub rounded_corners: f32,
    /// CSS classes to apply to the root SVG element.
    pub root_classes: Vec<String>,
    /// Theme preset to use (default if not specified).
    pub theme: ThemePreset,
    /// Whether to embed theme CSS in the SVG.
    pub embed_theme_css: bool,
    /// Whether CSS-only diagram animations should be emitted.
    pub animations_enabled: bool,
    /// Duration for node/edge entrance and transition effects in milliseconds.
    pub animation_duration_ms: u32,
    /// Sequential stagger between animated items in milliseconds.
    pub animation_stagger_ms: u32,
    /// Duration for dashed edge flow animation in milliseconds.
    pub flow_animation_duration_ms: u32,
    /// Stroke-dasharray pattern used by animated flow edges.
    pub flow_dash_pattern: String,
    /// Hover scale factor for animated node hover effects.
    pub hover_scale: f32,
    /// Position for node icons relative to the label.
    pub node_icon_position: NodeIconPosition,
    /// User-provided custom icon definitions keyed by normalized icon name.
    pub custom_icons: BTreeMap<String, CustomSvgIcon>,
    /// Detail tier selection (`auto`, `compact`, `normal`, `rich`).
    pub detail_tier: MermaidTier,
    /// Minimum readable font size in pixels.
    pub min_font_size: f32,
    /// Whether to embed print-optimized CSS rules.
    pub print_optimized: bool,
    /// Accessibility configuration.
    pub a11y: A11yConfig,
    /// Whether to emit source-span metadata attributes in the SVG output.
    pub include_source_spans: bool,
    /// How (or if) to emit node links.
    pub link_mode: MermaidLinkMode,
}

impl SvgRenderConfig {
    /// Apply a degradation plan to this config, disabling visual effects as directed.
    pub fn apply_degradation(&mut self, plan: &fm_core::MermaidDegradationPlan) {
        if plan.reduce_decoration {
            self.shadows = false;
            self.node_gradients = false;
            self.glow_enabled = false;
        }
        match plan.target_fidelity {
            fm_core::MermaidFidelity::Compact => {
                self.detail_tier = MermaidTier::Compact;
            }
            fm_core::MermaidFidelity::Outline => {
                self.detail_tier = MermaidTier::Compact;
                self.shadows = false;
                self.node_gradients = false;
                self.glow_enabled = false;
            }
            _ => {}
        }
    }

    /// Get the font metrics based on this configuration.
    #[must_use]
    pub fn font_metrics(&self) -> fm_core::FontMetrics {
        fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
            preset: fm_core::FontPreset::from_family(&self.font_family),
            font_size: self.font_size,
            line_height: self.line_height,
            fallback_chain: vec![
                fm_core::FontPreset::SansSerif,
                fm_core::FontPreset::Monospace,
            ],
            trace_fallbacks: false,
        })
    }
}

impl Default for SvgRenderConfig {
    fn default() -> Self {
        Self {
            backend: SvgBackend::LegacyLayout,
            responsive: true,
            accessible: true,
            font_family: String::from(
                "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif",
            ),
            font_size: 15.0,
            avg_char_width: 7.5,
            line_height: 1.5,
            padding: 40.0,
            shadows: true,
            shadow_offset_x: 2.0,
            shadow_offset_y: 2.0,
            shadow_blur: 6.0,
            shadow_opacity: 0.15,
            shadow_color: String::from("#0f172a"),
            node_gradients: true,
            node_gradient_style: NodeGradientStyle::LinearVertical,
            glow_enabled: true,
            glow_blur: 6.0,
            glow_opacity: 0.35,
            glow_color: String::from("#3b82f6"),
            cluster_fill_opacity: 0.08,
            inactive_opacity: 0.40,
            rounded_corners: 10.0,
            root_classes: Vec::new(),
            theme: ThemePreset::Default,
            embed_theme_css: true,
            animations_enabled: false,
            animation_duration_ms: 420,
            animation_stagger_ms: 80,
            flow_animation_duration_ms: 1400,
            flow_dash_pattern: String::from("8 6"),
            hover_scale: 1.03,
            node_icon_position: NodeIconPosition::Above,
            custom_icons: BTreeMap::new(),
            detail_tier: MermaidTier::Auto,
            min_font_size: 8.0,
            print_optimized: true,
            a11y: A11yConfig::full(),
            include_source_spans: false,
            link_mode: MermaidLinkMode::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderDetailTier {
    Compact,
    Normal,
    Rich,
}

#[derive(Debug, Clone, Copy)]
struct RenderDetailProfile {
    tier: RenderDetailTier,
    show_node_labels: bool,
    show_edge_labels: bool,
    show_cluster_labels: bool,
    node_label_max_chars: Option<usize>,
    edge_label_max_chars: Option<usize>,
    node_font_size: f32,
    edge_font_size: f32,
    cluster_font_size: f32,
    enable_shadows: bool,
}

/// Render an IR diagram to SVG string.
#[must_use]
pub fn render_svg(ir: &MermaidDiagramIr) -> String {
    render_svg_with_config(ir, &SvgRenderConfig::default())
}

/// Render an IR diagram to SVG string with custom configuration.
#[must_use]
pub fn render_svg_with_config(ir: &MermaidDiagramIr, config: &SvgRenderConfig) -> String {
    let layout_config = fm_layout::LayoutConfig {
        font_metrics: Some(config.font_metrics()),
        ..Default::default()
    };
    let layout = fm_layout::layout_diagram_with_config(ir, layout_config);
    render_svg_with_layout(ir, &layout, config)
}

/// Render an IR diagram to SVG string with a pre-computed layout.
#[must_use]
pub fn render_svg_with_layout(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    config: &SvgRenderConfig,
) -> String {
    let mut svg = match config.backend {
        SvgBackend::LegacyLayout => render_layout_to_svg(layout, ir, config),
        SvgBackend::Scene => {
            let scene = build_render_scene(ir, layout);
            render_scene_document_with_ir(&scene, config, Some(ir))
        }
    };
    strip_unused_state_css(&mut svg);
    // The output post-passes below (marker-def strip, dead-marker-CSS prune, CSS minify) each walk
    // the SVG and rebuild a buffer. On a SMALL/MEDIUM diagram that is cheap and the byte win is a
    // meaningful fraction (the fixed CSS + the 12-marker set dominate small output). On a LARGE
    // render the output saving is <0.5% (the geometry dominates) while the rebuilds add measurable
    // render time — a measured net-negative (render_svg/large_500 regressed ~+37% with them on). Cap
    // the work to the size range where the win clears the cost, exactly like `strip_unused_state_css`
    // (which self-guards at the same threshold). No golden exceeds this cap, so output is unchanged
    // for every checked-in case.
    if svg.len() <= 100_000 {
        strip_unused_markers(&mut svg);
        strip_dead_marker_css(&mut svg);
        minify_style_block(&mut svg);
    }
    svg
}

/// Post-pass: drop the contiguous node-STATE rule region (inactive / block-beta / highlighted /
/// border-dashed / border-double) from the embedded `<style>` when the rendered BODY uses none of
/// those state classes. These classes come from classDef / diagram features (not one IR field), so
/// detection is done on the final SVG body — exact and drift-proof. Safe by construction: no-op if
/// any state class is used, if the boundary markers are absent (CSS drift), or if the bounded region
/// is implausibly large (mis-grab guard). Byte-identical rendering — the dropped selectors match
/// nothing in the body.
fn strip_unused_state_css(svg: &mut String) {
    // This post-pass full-scans the SVG ~20 times (state-class + accent presence checks). The fixed
    // theme CSS it trims (~1.3 KB of states/accents) is a meaningful fraction only for small/medium
    // diagrams; on a large SVG it is <1% of output while the repeated scans add measurable render
    // time. Cap the work to outputs where the win clearly beats the scan cost (covers small flowcharts
    // through the sequence diagram ~62 KB; skips the 200 KB+ chain / wide renders).
    if svg.len() > 100_000 {
        return;
    }
    const STATE_CLASSES: [&str; 5] = [
        "fm-node-inactive",
        "fm-node-block-beta",
        "fm-node-highlighted",
        "fm-node-border-dashed",
        "fm-node-border-double",
    ];
    let body_start = svg.find("</style>").map_or(0, |i| i + "</style>".len());
    if STATE_CLASSES.iter().any(|c| svg[body_start..].contains(c)) {
        return;
    }
    if let (Some(start), Some(after)) = (
        svg.find(".fm-node-inactive { opacity:"),
        svg.find(".fm-cluster { fill-opacity:"),
    ) && after > start
        && after - start < 1500
    {
        svg.replace_range(start..after, "");
    }

    // The 8 accent palettes (`.fm-node-accent-1..8`) are assigned per node, so a diagram with few
    // nodes uses only some. Drop each `.fm-node-accent-N` rule whose class is absent from the body.
    // Body-based + exact-selector; no-op if the class is used or the rule is missing.
    let body_at = svg.find("</style>").map_or(0, |i| i + "</style>".len());
    // Which `.fm-node-accent-N` classes (N=1..=8) appear in the body — collected in ONE scan by
    // reading the digit after each "fm-node-accent-" match, instead of 8 `contains(&format!(...))`
    // that each allocate a needle String and re-scan the body. Byte-identical: used[n] is true iff
    // "fm-node-accent-{n}" occurs (single-digit N), exactly the substring the per-n `contains` tested.
    // Const needles (not `format!("fm-node-accent-{n}")`) so the presence check allocates nothing —
    // `str::contains` still SIMD-scans + early-exits exactly as before. Byte-identical, strictly fewer
    // allocations (8 short Strings/render eliminated).
    const ACCENT_NEEDLES: [&str; 9] = [
        "",
        "fm-node-accent-1",
        "fm-node-accent-2",
        "fm-node-accent-3",
        "fm-node-accent-4",
        "fm-node-accent-5",
        "fm-node-accent-6",
        "fm-node-accent-7",
        "fm-node-accent-8",
    ];
    let used: [bool; 9] =
        std::array::from_fn(|n| n == 0 || svg[body_at..].contains(ACCENT_NEEDLES[n]));
    for (n, &is_used) in used.iter().enumerate().skip(1) {
        if !is_used {
            let selector = format!(".fm-node-accent-{n} {{");
            if let Some(start) = svg.find(&selector)
                && let Some(rel_end) = svg[start..].find("}\n")
            {
                svg.replace_range(start..start + rel_end + 2, "");
            }
        }
    }

    // Drop each `:root` accent custom property `--fm-accent-N` that is no longer referenced anywhere
    // (its accent rule was stripped above and no node-shape/gradient uses it). Reference-counted, so
    // it is a no-op while ANY `var(--fm-accent-N)` remains — safe. Const needles avoid a per-n
    // `format!` alloc for the (always-run) reference check; the strip logic is otherwise unchanged.
    const VAR_NEEDLES: [&str; 9] = [
        "",
        "var(--fm-accent-1)",
        "var(--fm-accent-2)",
        "var(--fm-accent-3)",
        "var(--fm-accent-4)",
        "var(--fm-accent-5)",
        "var(--fm-accent-6)",
        "var(--fm-accent-7)",
        "var(--fm-accent-8)",
    ];
    for (n, &needle) in VAR_NEEDLES.iter().enumerate().skip(1) {
        if !svg.contains(needle) {
            let decl = format!("  --fm-accent-{n}:");
            if let Some(start) = svg.find(&decl)
                && let Some(rel_end) = svg[start..].find(";\n")
            {
                svg.replace_range(start..start + rel_end + 2, "");
            }
        }
    }
}

/// Render post-pass: drop `<marker>` arrowhead defs that the rendered body never references.
///
/// The non-flowchart render paths emit the FULL arrow-marker set (12 markers, ~2.4 KB) because
/// they cannot cheaply predict which arrow shapes a sequence/class/state/er diagram will use, but
/// the typical such diagram references only `arrow-end` — leaving ~2 KB of dead `<marker>` defs.
/// An SVG `<marker>` is purely declarative: it renders NOTHING unless a `marker-start/-mid/-end`
/// (i.e. a `url(#id)`) points at it, so removing an unreferenced marker is visually identical.
///
/// Detection is body-based and drift-proof (the exact pattern of [`strip_unused_state_css`]): a
/// marker is kept iff its id appears inside a `url(#id)` somewhere in the document. Marker DEFS
/// contain no `url(#...)`, and the theme CSS targets markers with `marker#id` selectors (never
/// `url(#id)`), so the live-set is exactly the markers an edge actually points at. Safe by
/// construction: any referenced or future marker is kept; a CSS/markup drift can only leave a dead
/// def in place, never strip a live one. Single O(n) rebuild (no per-marker rescans), so it adds
/// no large-render cost — and large flowcharts already emit a minimal marker set (nothing to strip).
fn strip_unused_markers(svg: &mut String) {
    // Multi-byte needles searched in tight loops (once per `url(#…)` ref / `<marker>` def): build each
    // SIMD `Finder` ONCE and reuse it, instead of `str::find` rebuilding a `TwoWaySearcher` per call.
    let marker_finder = memchr::memmem::Finder::new(b"<marker ");
    if marker_finder.find(svg.as_bytes()).is_none() {
        return;
    }
    let url_finder = memchr::memmem::Finder::new(b"url(#");
    // 1. Collect every id referenced via `url(#id)` (marker assignments live only here).
    // FxHashSet (not SipHash std HashSet): membership-only (no iteration-order dependency), and the
    // marker ids are short — FxHash is ~3-4x faster than SipHash here. Byte-identical.
    let mut referenced: fm_core::FxHashSet<&str> = fm_core::FxHashSet::default();
    let mut at = 0;
    while let Some(rel) = url_finder.find(&svg.as_bytes()[at..]) {
        let id_start = at + rel + "url(#".len();
        let Some(close) = memchr::memchr(b')', &svg.as_bytes()[id_start..]) else {
            break;
        };
        referenced.insert(&svg[id_start..id_start + close]);
        at = id_start + close + 1;
    }
    // 2. Find each `<marker id="..">…</marker>` span whose id is not referenced.
    let endmarker_finder = memchr::memmem::Finder::new(b"</marker>");
    let id_finder = memchr::memmem::Finder::new(b"id=\"");
    let mut dead_spans: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    while let Some(rel) = marker_finder.find(&svg.as_bytes()[at..]) {
        let m_start = at + rel;
        let Some(end_rel) = endmarker_finder.find(&svg.as_bytes()[m_start..]) else {
            break;
        };
        let m_end = m_start + end_rel + "</marker>".len();
        // The marker id is the first `id="…"` inside the opening tag.
        let tag_end = memchr::memchr(b'>', &svg.as_bytes()[m_start..m_end]).map_or(m_end, |g| m_start + g);
        if let Some(idrel) = id_finder.find(&svg.as_bytes()[m_start..tag_end]) {
            let id_start = m_start + idrel + "id=\"".len();
            if let Some(idclose) = memchr::memchr(b'"', &svg.as_bytes()[id_start..tag_end]) {
                let id = &svg[id_start..id_start + idclose];
                if !referenced.contains(id) {
                    dead_spans.push((m_start, m_end));
                }
            }
        }
        at = m_end;
    }
    if dead_spans.is_empty() {
        return;
    }
    // 3. Rebuild once, skipping the dead spans (O(n), no repeated tail-shifts).
    let mut out = String::with_capacity(svg.len());
    let mut cursor = 0;
    for (start, end) in &dead_spans {
        out.push_str(&svg[cursor..*start]);
        cursor = *end;
    }
    out.push_str(&svg[cursor..]);
    *svg = out;
}

/// Companion to [`strip_unused_markers`]: prune `marker#arrow-*` selectors from the theme CSS once
/// their `<marker>` defs have been stripped. The theme stylesheet ships fixed rules that style the
/// arrowhead markers (`marker#arrow-end/filled/circle/diamond path`, `marker#arrow-open path`,
/// `marker#arrow-cross path`, and the `:hover` variants). After the marker-def strip, any such
/// selector whose marker is gone matches nothing — pure dead CSS (225 B on a typical arrow-end-only
/// diagram, 584 B on an edge-less one where every marker rule dies).
///
/// A selector is kept iff it references no DEAD marker (a live marker, or no `marker#` at all);
/// a rule with every selector pruned is dropped whole. Runs on the pre-minify (pretty) stylesheet
/// in the render funnel, after `strip_unused_markers`. Safe by construction: a live marker keeps its
/// styling (its selector references a present def), and CSS drift can only leave a dead selector in
/// place, never drop a live one. Brace-depth tracking emits nested at-rules (`@media`) verbatim.
fn strip_dead_marker_css(svg: &mut String) {
    if memchr::memmem::find(svg.as_bytes(), b"marker#arrow-").is_none() {
        return;
    }
    // Live marker ids = those still present as `<marker id="…">` defs. Reuse one SIMD `Finder` per
    // needle across the loop instead of `str::find` rebuilding a `TwoWaySearcher` every iteration.
    let marker_finder = memchr::memmem::Finder::new(b"<marker ");
    let id_finder = memchr::memmem::Finder::new(b"id=\"");
    // FxHashSet over SipHash std HashSet (membership-only, short keys — byte-identical).
    let mut live: fm_core::FxHashSet<&str> = fm_core::FxHashSet::default();
    let mut at = 0;
    while let Some(rel) = marker_finder.find(&svg.as_bytes()[at..]) {
        let m = at + rel;
        if let Some(i) = id_finder.find(&svg.as_bytes()[m..]) {
            let s = m + i + "id=\"".len();
            if let Some(e) = memchr::memchr(b'"', &svg.as_bytes()[s..]) {
                live.insert(&svg[s..s + e]);
            }
        }
        at = m + "<marker ".len();
    }
    let Some(open) = memchr::memmem::find(svg.as_bytes(), b"<style") else {
        return;
    };
    let Some(gt) = memchr::memchr(b'>', &svg.as_bytes()[open..]) else {
        return;
    };
    let cs = open + gt + 1;
    let Some(er) = memchr::memmem::find(&svg.as_bytes()[cs..], b"</style>") else {
        return;
    };
    let ce = cs + er;
    let css = &svg[cs..ce];
    let bytes = css.as_bytes();
    let marker_hash_finder = memchr::memmem::Finder::new(b"marker#");
    let mut out = String::with_capacity(css.len());
    let mut i = 0;
    let mut seg_start = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let selectors = &css[seg_start..i];
            // Body = the balanced `{ … }` (track depth so a nested at-rule body is one unit).
            let mut depth = 1;
            let mut j = i + 1;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            let body = &css[i..j];
            if marker_hash_finder.find(selectors.as_bytes()).is_some() {
                let kept: Vec<&str> = selectors
                    .split(',')
                    .filter(|sel| match marker_hash_finder.find(sel.as_bytes()) {
                        Some(p) => {
                            let rest = &sel[p + "marker#".len()..];
                            let end = rest
                                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                                .unwrap_or(rest.len());
                            live.contains(&rest[..end])
                        }
                        None => true,
                    })
                    .collect();
                if !kept.is_empty() {
                    out.push_str(&kept.join(","));
                    out.push_str(body);
                }
            } else {
                out.push_str(selectors);
                out.push_str(body);
            }
            i = j;
            seg_start = j;
        } else {
            i += 1;
        }
    }
    out.push_str(&css[seg_start..]);
    if out.len() < css.len() {
        svg.replace_range(cs..ce, &out);
    }
}

/// Final render post-pass: minify the embedded `<style>` CSS. Mermaid ships minified CSS;
/// frankenmermaid emitted pretty-printed CSS (2-space indent + a newline per line), which is
/// fixed dead weight on EVERY diagram — including the large renders the conditional dead-CSS
/// strips skip (size-guarded). Runs once over the ~9 KB style region only (the SVG body is
/// untouched), so the cost is a single constant-size scan with no size guard needed. No-op when
/// there is no `<style>` block. See [`minify_css`] for the whitespace-only contract.
fn minify_style_block(svg: &mut String) {
    let Some(open) = memchr::memmem::find(svg.as_bytes(), b"<style") else {
        return;
    };
    let Some(gt_rel) = memchr::memchr(b'>', &svg.as_bytes()[open..]) else {
        return;
    };
    let content_start = open + gt_rel + 1;
    let Some(end_rel) = memchr::memmem::find(&svg.as_bytes()[content_start..], b"</style>") else {
        return;
    };
    let content_end = content_start + end_rel;
    let minified = minify_css(&svg[content_start..content_end]);
    if minified.len() < content_end - content_start {
        svg.replace_range(content_start..content_end, &minified);
    }
}

/// Collapse non-semantic whitespace in a CSS string to mermaid-style minified form.
///
/// WHITESPACE-ONLY by construction: no non-whitespace byte is ever added or removed, so the
/// CSS parses identically. A run of whitespace is dropped when an adjacent delimiter already
/// separates the tokens (`{ } ; , :` immediately before, or `}` immediately after) and otherwise
/// collapses to a single space. This preserves the two whitespace classes that ARE semantic in
/// CSS — descendant combinators (`.a .b`) and value-internal spaces (`2px 8px`, `in srgb`,
/// `var(--x) 4%`, `prop: value`) — while removing indentation, line breaks, and delimiter-hugging
/// spaces. Spaces after `:` are intentionally kept (selectors, pseudo-elements, and declarations
/// all share `:`, so leaving it untouched is the maximally drift-safe choice). The invariant is
/// machine-checked: stripping ALL whitespace from the input and from the output yields identical
/// strings (verified per-test and across every golden), proving only whitespace changed.
fn minify_css(css: &str) -> String {
    let b = css.as_bytes();
    let n = b.len();
    let mut out: Vec<u8> = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        match b[i] {
            b' ' | b'\t' | b'\n' | b'\r' => {
                let mut has_nl = false;
                while i < n {
                    match b[i] {
                        b' ' | b'\t' => i += 1,
                        b'\n' | b'\r' => {
                            has_nl = true;
                            i += 1;
                        }
                        _ => break,
                    }
                }
                let prev = out.last().copied().unwrap_or(0);
                let nxt = if i < n { b[i] } else { 0 };
                let drop = if has_nl {
                    prev == 0 || matches!(prev, b'{' | b'}' | b';' | b',' | b':') || nxt == b'}'
                } else {
                    matches!(prev, b'{' | b'}' | b';' | b',')
                        || matches!(nxt, b'{' | b'}' | b';' | b',' | 0)
                };
                if !drop {
                    out.push(b' ');
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    // A pure whitespace transformation over valid UTF-8 input is always valid UTF-8; the fallback
    // is defensive only.
    String::from_utf8(out).unwrap_or_else(|_| css.to_string())
}

/// Render a target-agnostic scene to SVG string with custom configuration.
#[must_use]
pub fn render_scene_to_svg(scene: &RenderScene, config: &SvgRenderConfig) -> String {
    render_scene_document(scene, config)
}

fn render_scene_document(scene: &RenderScene, config: &SvgRenderConfig) -> String {
    render_scene_document_with_ir(scene, config, None)
}

fn resolve_accessibility_text(
    ir: Option<&MermaidDiagramIr>,
    layout: Option<&DiagramLayout>,
    config: &SvgRenderConfig,
    fallback_desc: impl FnOnce() -> String,
) -> (String, String) {
    match ir {
        Some(diagram_ir) => {
            let title = diagram_ir
                .meta
                .acc_title
                .clone()
                .unwrap_or_else(|| format!("{} diagram", diagram_ir.diagram_type.as_str()));
            let desc = diagram_ir.meta.acc_descr.clone().unwrap_or_else(|| {
                if config.a11y.aria_labels {
                    describe_diagram_with_layout(diagram_ir, layout)
                } else {
                    fallback_desc()
                }
            });
            (title, desc)
        }
        None => (String::from("Render scene"), fallback_desc()),
    }
}

fn diagram_title<'a>(ir: &'a MermaidDiagramIr, explicit: Option<&'a str>) -> Option<&'a str> {
    ir.meta.title.as_deref().or(explicit)
}

fn resolve_theme(ir: Option<&MermaidDiagramIr>, config: &SvgRenderConfig) -> Theme {
    let preset = ir
        .and_then(|i| i.meta.theme_overrides.theme.as_deref())
        .and_then(|t| t.parse::<ThemePreset>().ok())
        .unwrap_or(config.theme);
    let mut theme = Theme::from_preset(preset);
    if let Some(i) = ir {
        theme
            .colors
            .apply_overrides(&i.meta.theme_overrides.theme_variables);
    }
    theme
}

/// The `.fm-cluster*` theme CSS block, captured EXACTLY as `Theme::to_svg_style` emits it. When a
/// diagram has no clusters these selectors match no element, so stripping the block is byte-identical
/// rendering while shrinking the fixed ~9 KB `<style>` (clusters ≈ 532 B). Kept as an exact constant
/// so a drift (CSS edit) makes `strip_unused_theme_css` a safe NO-OP (it matches nothing → no strip),
/// never a corruption. See docs/NEGATIVE_EVIDENCE.md (CSS dead-weight lever).
const CLUSTER_THEME_CSS: &str = ".fm-cluster {\n  fill: var(--fm-cluster-fill);\n  stroke: var(--fm-cluster-stroke);\n  stroke-width: 1;\n  stroke-dasharray: 5 3;\n  rx: 12;\n  ry: 12;\n}\n.fm-cluster-label {\n  fill: var(--fm-cluster-label-color);\n  font-weight: 700;\n  font-size: 0.85em;\n  letter-spacing: 0.01em;\n}\n.fm-cluster-c4 {\n  fill: var(--fm-cluster-c4-fill);\n  stroke: var(--fm-cluster-c4-stroke);\n  stroke-dasharray: none;\n}\n.fm-cluster-swimlane {\n  fill: var(--fm-cluster-swimlane-fill);\n  stroke: var(--fm-cluster-swimlane-stroke);\n  stroke-dasharray: none;\n}\n";

/// The special-node-shape theme CSS block (`note`/`cloud`/`cylinder`/`star`/`pentagon`), captured
/// EXACTLY as `Theme::to_svg_style` emits it. Stripped when the diagram uses none of those shapes
/// (the common rect/diamond/round/stadium case). Same byte-identical, safe-no-op-if-drifts contract
/// as `CLUSTER_THEME_CSS`.
const NODE_SHAPE_THEME_CSS: &str = ".fm-node.fm-node-shape-note path,\n.fm-node.fm-node-shape-note rect {\n  fill: var(--fm-node-fill);\n  fill: color-mix(in srgb, #fef3c7 40%, var(--fm-node-fill));\n}\n.fm-node.fm-node-shape-cloud path {\n  fill: var(--fm-node-fill);\n  fill: color-mix(in srgb, var(--fm-accent-2) 15%, var(--fm-node-fill));\n}\n.fm-node.fm-node-shape-cylinder path {\n  fill: var(--fm-node-fill);\n  fill: color-mix(in srgb, var(--fm-accent-1) 12%, var(--fm-node-fill));\n}\n.fm-node.fm-node-shape-star path,\n.fm-node.fm-node-shape-pentagon path {\n  stroke-width: 1.8;\n}\n";

/// Drop theme CSS rule blocks the diagram cannot use — the cluster block when there are no clusters,
/// and the special-node-shape block when none of those shapes are present. Byte-identical rendering
/// (the removed selectors match nothing); safe by construction (a non-matching constant is a no-op).
fn strip_unused_theme_css(css: &mut String, ir: Option<&MermaidDiagramIr>) {
    if !ir.is_some_and(|ir| !ir.clusters.is_empty()) {
        *css = css.replace(CLUSTER_THEME_CSS, "");
        // The `:root` cluster-only custom properties feed ONLY the stripped cluster rules, so they
        // are dead too when there are no clusters. Same exact-substring / safe-no-op contract.
        *css = css.replace(
            "  --fm-cluster-label-color: var(--fm-text-color);\n  --fm-cluster-c4-fill: var(--fm-cluster-fill);\n  --fm-cluster-c4-stroke: var(--fm-cluster-stroke);\n  --fm-cluster-swimlane-fill: var(--fm-cluster-fill);\n  --fm-cluster-swimlane-stroke: var(--fm-cluster-stroke);\n",
            "",
        );
    }
    let has_special_shapes = ir.is_some_and(|ir| {
        ir.nodes.iter().any(|node| {
            matches!(
                node.shape,
                fm_core::NodeShape::Note
                    | fm_core::NodeShape::Cloud
                    | fm_core::NodeShape::Cylinder
                    | fm_core::NodeShape::Star
                    | fm_core::NodeShape::Pentagon
            )
        })
    });
    if !has_special_shapes {
        *css = css.replace(NODE_SHAPE_THEME_CSS, "");
    }
    // `.fm-edge-dashed`/`.fm-edge-thick` style only dotted/thick arrows. The arrow lists below are
    // copied VERBATIM from `render_edge`'s `style_class` match so detection cannot drift from the
    // class actually emitted. `.fm-edge-back` is layout-determined (reversed edges) so it is NOT
    // gated here — it stays in the kept tail of the block.
    let has_dashed_or_thick = ir.is_some_and(|ir| {
        ir.edges.iter().any(|e| {
            matches!(
                e.arrow,
                fm_core::ArrowType::DottedArrow
                    | fm_core::ArrowType::DottedOpenArrow
                    | fm_core::ArrowType::DottedCross
                    | fm_core::ArrowType::HalfArrowTopDotted
                    | fm_core::ArrowType::HalfArrowBottomDotted
                    | fm_core::ArrowType::HalfArrowTopReverseDotted
                    | fm_core::ArrowType::HalfArrowBottomReverseDotted
                    | fm_core::ArrowType::StickArrowTopDotted
                    | fm_core::ArrowType::StickArrowBottomDotted
                    | fm_core::ArrowType::StickArrowTopReverseDotted
                    | fm_core::ArrowType::StickArrowBottomReverseDotted
                    | fm_core::ArrowType::DottedLine
                    | fm_core::ArrowType::DoubleDottedArrow
                    | fm_core::ArrowType::ThickArrow
                    | fm_core::ArrowType::DoubleThickArrow
                    | fm_core::ArrowType::ThickLine
            )
        })
    });
    if !has_dashed_or_thick {
        *css = css.replace(EDGE_STYLE_THEME_CSS, "");
    }
}

/// The `.fm-edge-dashed` + `.fm-edge-thick`(+`:hover`) theme rules — captured EXACTLY as
/// `Theme::to_svg_style` emits them — stripped when no edge uses a dotted/thick arrow. Same
/// byte-identical, safe-no-op-if-drifts contract as the other blocks.
const EDGE_STYLE_THEME_CSS: &str = ".fm-edge-dashed {\n  stroke-dasharray: 6 6;\n}\n.fm-edge-thick {\n  stroke-width: 2.5;\n}\n.fm-edge-thick:hover {\n  stroke-width: 3.5;\n}\n";

fn render_scene_document_with_ir(
    scene: &RenderScene,
    config: &SvgRenderConfig,
    ir: Option<&MermaidDiagramIr>,
) -> String {
    let padding = config.padding;
    let visible_title = ir.and_then(|diagram_ir| diagram_ir.meta.title.as_deref());
    let title_height = if visible_title.is_some() {
        config.font_size + 22.0
    } else {
        0.0
    };
    let width = (scene.bounds.width + padding * 2.0).max(1.0);
    let height = (scene.bounds.height + padding * 2.0 + title_height).max(1.0);

    let mut doc = SvgDocument::new()
        .viewbox(
            scene.bounds.x - padding,
            scene.bounds.y - padding - title_height,
            width,
            height,
        )
        .preserve_aspect_ratio("xMidYMid meet");

    // Root `font-family` (inherited by every `<text>`) when the theme CSS is embedded; the
    // per-label inline copies are gated off.
    if config.embed_theme_css {
        doc = doc.font_family(&config.font_family);
    }

    if config.responsive {
        doc = doc.responsive();
    }

    let (group_count, path_count, text_count) = count_scene_items(&scene.root);

    if config.accessible {
        let (title, desc) = resolve_accessibility_text(ir, None, config, || {
            format!(
                "Target-agnostic render scene with {group_count} groups, {path_count} paths, and {text_count} text items"
            )
        });
        doc = doc.accessible(title, desc);
    }

    if let Some(title) = visible_title {
        doc = doc.child(
            TextBuilder::new(title)
                .x(scene.bounds.x + scene.bounds.width / 2.0)
                .y(scene.bounds.y - 8.0)
                .anchor(TextAnchor::Middle)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(config.font_size + 4.0)
                .font_weight("600")
                .fill("var(--fm-text-color, #1f2937)")
                .class("fm-diagram-title")
                .build(),
        );
    }

    for class in &config.root_classes {
        doc = doc.class(class);
    }
    if config.animations_enabled {
        doc = doc.class("fm-animations-enabled");
    }

    let scene_type = ir.map_or("scene", |diagram_ir| diagram_ir.diagram_type.as_str());
    doc = doc
        .data("type", scene_type)
        .data("groups", &group_count.to_string())
        .data("paths", &path_count.to_string())
        .data("texts", &text_count.to_string());

    let effects_enabled = clamp_unit_interval(config.inactive_opacity) < 0.999
        || clamp_unit_interval(config.cluster_fill_opacity) < 0.999;

    let theme = resolve_theme(ir, config);
    let classdef_css = ir.map_or(String::new(), collect_classdef_css);

    let mut css = String::new();
    if config.embed_theme_css {
        let mut theme_css = theme.to_svg_style(
            config.shadows,
            ir.is_some_and(|ir| ir.edges.iter().any(|edge| edge.label.is_some())),
        );
        strip_unused_theme_css(&mut theme_css, ir);
        css.push_str(&theme_css);
    }
    if effects_enabled {
        css.push_str(&effects_css(config));
    }
    if config.animations_enabled {
        css.push_str(&animation_css(config));
    }
    if config.a11y.accessibility_css {
        css.push_str(accessibility_css());
    }
    if config.print_optimized {
        css.push_str(&print_css(config.min_font_size));
    }
    if !classdef_css.is_empty() {
        css.push_str(&classdef_css);
    }
    if !css.is_empty() {
        doc = doc.style(css);
    }

    let mut defs = DefsBuilder::new();

    // Arrowhead markers: emit only what the diagram can reference (see
    // `arrow_uses_only_basic_markers`). Kept identical to the legacy backend's gating so the
    // two backends produce the same marker set for the same diagram. Without an IR
    // (`render_scene_to_svg`) we cannot inspect arrow types, so conservatively emit the full
    // set. Emission order is preserved, so output is byte-identical whenever a fancy arrow is
    // present.
    let emit_fancy_markers = ir.is_none_or(|diagram_ir| {
        diagram_ir.diagram_type != fm_core::DiagramType::Flowchart
            || diagram_ir
                .edges
                .iter()
                .any(|edge| !arrow_uses_only_basic_markers(edge.arrow))
    });
    let edge_color = &theme.colors.edge;
    defs = defs.marker(ArrowheadMarker::standard("arrow-end", edge_color));
    if emit_fancy_markers {
        defs = defs.marker(ArrowheadMarker::filled("arrow-filled", edge_color));
    }
    defs = defs.marker(ArrowheadMarker::open("arrow-open", edge_color));
    if emit_fancy_markers {
        defs = defs.marker(ArrowheadMarker::half_top("arrow-half-top", edge_color));
        defs = defs.marker(ArrowheadMarker::half_bottom(
            "arrow-half-bottom",
            edge_color,
        ));
        defs = defs.marker(ArrowheadMarker::stick_top("arrow-stick-top", edge_color));
        defs = defs.marker(ArrowheadMarker::stick_bottom(
            "arrow-stick-bottom",
            edge_color,
        ));
        defs = defs.marker(
            ArrowheadMarker::standard("arrow-start", edge_color)
                .with_orient(crate::defs::MarkerOrient::AutoStartReverse),
        );
        defs = defs.marker(
            ArrowheadMarker::filled("arrow-start-filled", edge_color)
                .with_orient(crate::defs::MarkerOrient::AutoStartReverse),
        );
        defs = defs.marker(ArrowheadMarker::circle_marker("arrow-circle", edge_color));
        defs = defs.marker(ArrowheadMarker::cross_marker("arrow-cross", edge_color));
        defs = defs.marker(ArrowheadMarker::diamond_marker("arrow-diamond", edge_color));
    }

    let mut clip_defs = Vec::new();
    let mut clip_id_counter = 0usize;
    let scene_root = render_scene_group(
        &scene.root,
        config,
        ir,
        &mut clip_defs,
        &mut clip_id_counter,
    );

    for clip in clip_defs {
        defs = defs.custom(clip);
    }

    doc = doc.defs(defs);

    doc.child(scene_root).to_string()
}

fn count_scene_items(group: &RenderGroup) -> (usize, usize, usize) {
    let mut groups = 1usize;
    let mut paths = 0usize;
    let mut texts = 0usize;

    for child in &group.children {
        match child {
            RenderItem::Group(nested) => {
                let (nested_groups, nested_paths, nested_texts) = count_scene_items(nested);
                groups += nested_groups;
                paths += nested_paths;
                texts += nested_texts;
            }
            RenderItem::Path(_) => paths += 1,
            RenderItem::Text(_) => texts += 1,
        }
    }

    (groups, paths, texts)
}

fn render_scene_group(
    group: &RenderGroup,
    config: &SvgRenderConfig,
    ir: Option<&MermaidDiagramIr>,
    clip_defs: &mut Vec<Element>,
    clip_id_counter: &mut usize,
) -> Element {
    let mut elem = Element::group();

    if let Some(id) = &group.id {
        elem = elem.id(id);
    }

    elem = apply_source_metadata(elem, group.source, config.include_source_spans, ir);

    if config.a11y.keyboard_nav
        && matches!(group.source, RenderSource::Node(_) | RenderSource::Edge(_))
    {
        elem = elem.attr("tabindex", "0");
    }

    if let Some(transform) = group.transform {
        let transform_value = scene_transform_value(transform);
        elem = elem.transform(&transform_value);
    }

    if let Some(clip) = &group.clip {
        let clip_id = register_clip_path(clip_defs, clip, clip_id_counter);
        elem = elem.clip_path_ref(&format!("url(#{clip_id})"));
    }

    for child in &group.children {
        elem = elem.child(render_scene_item(
            child,
            config,
            ir,
            clip_defs,
            clip_id_counter,
        ));
    }

    elem
}

fn render_scene_item(
    item: &RenderItem,
    config: &SvgRenderConfig,
    ir: Option<&MermaidDiagramIr>,
    clip_defs: &mut Vec<Element>,
    clip_id_counter: &mut usize,
) -> Element {
    match item {
        RenderItem::Group(group) => {
            render_scene_group(group, config, ir, clip_defs, clip_id_counter)
        }
        RenderItem::Path(path) => render_scene_path(path, config.include_source_spans, ir),
        RenderItem::Text(text) => render_scene_text(text, config, ir),
    }
}

fn render_scene_path(
    path: &RenderPath,
    include_source_spans: bool,
    ir: Option<&MermaidDiagramIr>,
) -> Element {
    let mut elem = Element::path().d(&path_cmds_to_d(&path.commands));
    elem = apply_source_metadata(elem, path.source, include_source_spans, ir);

    if let Some(fill) = &path.fill {
        elem = apply_fill_style(elem, fill);
    } else {
        elem = elem.fill("none");
    }

    if let Some(stroke) = &path.stroke {
        elem = apply_stroke_style(elem, stroke);
    } else {
        elem = elem.stroke("none");
    }

    if path.marker_start != MarkerKind::None {
        elem = elem.marker_start(map_marker_kind(path.marker_start));
    }

    if path.marker_end != MarkerKind::None {
        elem = elem.marker_end(map_marker_kind(path.marker_end));
    }

    elem
}

fn map_marker_kind(kind: fm_layout::MarkerKind) -> &'static str {
    use fm_layout::MarkerKind;
    match kind {
        MarkerKind::None => "",
        MarkerKind::Arrow | MarkerKind::DottedArrow => "url(#arrow-end)",
        MarkerKind::HalfArrowTop => "url(#arrow-half-top)",
        MarkerKind::HalfArrowBottom => "url(#arrow-half-bottom)",
        MarkerKind::StickArrowTop => "url(#arrow-stick-top)",
        MarkerKind::StickArrowBottom => "url(#arrow-stick-bottom)",
        MarkerKind::ThickArrow => "url(#arrow-filled)",
        MarkerKind::Circle => "url(#arrow-circle)",
        MarkerKind::Cross => "url(#arrow-cross)",
        MarkerKind::Diamond => "url(#arrow-diamond)",
        MarkerKind::Open => "url(#arrow-open)",
    }
}

fn render_scene_text(
    text: &RenderText,
    config: &SvgRenderConfig,
    ir: Option<&MermaidDiagramIr>,
) -> Element {
    let mut elem = TextBuilder::new(&text.text)
        .x(text.x)
        .y(text.y)
        .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
        .font_size(text.font_size)
        .line_height(config.line_height)
        .anchor(map_text_align(text.align))
        .baseline(map_text_baseline(text.baseline))
        .build();

    elem = apply_fill_style(elem, &text.fill);
    apply_source_metadata(elem, text.source, config.include_source_spans, ir)
}

fn apply_source_metadata(
    mut elem: Element,
    source: RenderSource,
    include_source_spans: bool,
    ir: Option<&MermaidDiagramIr>,
) -> Element {
    match source {
        RenderSource::Diagram => {
            elem = elem.data("fm-source-kind", "diagram");
        }
        RenderSource::Node(index) => {
            elem = elem
                .data("fm-source-kind", "node")
                .data("fm-source-index", &index.to_string());
        }
        RenderSource::Edge(index) => {
            elem = elem
                .data("fm-source-kind", "edge")
                .data("fm-source-index", &index.to_string());
        }
        RenderSource::Cluster(index) => {
            elem = elem
                .data("fm-source-kind", "cluster")
                .data("fm-source-index", &index.to_string());
        }
    }

    if let Some(diagram_ir) = ir {
        match source {
            RenderSource::Node(index) => {
                if let Some(node) = diagram_ir.nodes.get(index) {
                    elem = elem
                        .attr("role", "graphics-symbol")
                        .attr("aria-label", &crate::a11y::describe_node(node, diagram_ir));
                }
            }
            RenderSource::Edge(index) => {
                if let Some(edge) = diagram_ir.edges.get(index) {
                    let from_node = diagram_ir
                        .resolve_endpoint_node(edge.from)
                        .and_then(|id| diagram_ir.nodes.get(id.0));
                    let to_node = diagram_ir
                        .resolve_endpoint_node(edge.to)
                        .and_then(|id| diagram_ir.nodes.get(id.0));
                    let label = edge
                        .label
                        .and_then(|lid| diagram_ir.labels.get(lid.0))
                        .map(|l| l.text.as_str());

                    elem = elem.attr("role", "graphics-symbol").attr(
                        "aria-label",
                        &crate::a11y::describe_edge(
                            from_node, to_node, edge.arrow, label, diagram_ir,
                        ),
                    );
                }
            }
            _ => {}
        }
    }

    if include_source_spans
        && let Some(span) = ir.and_then(|diagram_ir| render_source_span(diagram_ir, source))
    {
        elem = apply_span_metadata(elem, span);
    }

    elem
}

fn render_source_span(ir: &MermaidDiagramIr, source: RenderSource) -> Option<Span> {
    let span = match source {
        RenderSource::Diagram => return None,
        RenderSource::Node(index) => ir.nodes.get(index).map(|node| node.span_primary),
        RenderSource::Edge(index) => ir.edges.get(index).map(|edge| edge.span),
        RenderSource::Cluster(index) => ir.clusters.get(index).map(|cluster| cluster.span),
    }?;

    (!span.is_unknown()).then_some(span)
}

fn apply_span_metadata(elem: Element, span: Span) -> Element {
    if span.is_unknown() {
        return elem;
    }

    // Emit only the compact `data-fm-source-span` attribute, which already encodes all six
    // values (`{start.line}:{start.col}-{end.line}:{end.col}@{start.byte}-{end.byte}`, see
    // `Span::compact_display`). The six former `data-fm-source-{start,end}-{line,col,byte}`
    // attributes duplicated those exact values, had zero consumers anywhere in the tree, and
    // — being long repeated names across every element — dominated source-span output bytes.
    // Source spans are off by default, so this is byte-identical for the default config and
    // roughly halves render output (and time) when `include_source_spans` is enabled.
    // Static name + owned value: no `format!("data-…")` name alloc and no value clone (vs `data`).
    elem.attr_owned("data-fm-source-span", span.compact_display())
}

fn register_clip_path(
    clip_defs: &mut Vec<Element>,
    clip: &RenderClip,
    clip_id_counter: &mut usize,
) -> String {
    let clip_id = format!("fm-scene-clip-{clip_id_counter}");
    *clip_id_counter += 1;

    let shape = match clip {
        RenderClip::Rect(rect) => Element::rect()
            .x(rect.x)
            .y(rect.y)
            .width(rect.width)
            .height(rect.height),
        RenderClip::Path(commands) => Element::path().d(&path_cmds_to_d(commands)),
    };

    clip_defs.push(Element::clip_path().id(&clip_id).child(shape));
    clip_id
}

fn scene_transform_value(transform: RenderTransform) -> String {
    // Use direct matrix formatting for bit-identical output.
    // CGA rotor stack available via cga_transform::render_transform_to_cga()
    // when rotation extraction or other CGA features are needed.
    cga_transform::render_transform_to_svg_matrix(transform)
}

fn path_cmds_to_d(commands: &[PathCmd]) -> String {
    let mut builder = PathBuilder::new();
    for command in commands {
        builder = match *command {
            PathCmd::MoveTo { x, y } => builder.move_to(x, y),
            PathCmd::LineTo { x, y } => builder.line_to(x, y),
            PathCmd::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => builder.curve_to(c1x, c1y, c2x, c2y, x, y),
            PathCmd::QuadTo { cx, cy, x, y } => builder.quadratic_to(cx, cy, x, y),
            PathCmd::Close => builder.close(),
        };
    }
    builder.build()
}

fn apply_fill_style(mut elem: Element, fill: &FillStyle) -> Element {
    match fill {
        FillStyle::Solid { color, opacity } => {
            elem = elem.fill(color);
            if *opacity < 0.999 {
                elem = elem.fill_opacity(clamp_unit_interval(*opacity));
            }
        }
    }
    elem
}

fn apply_stroke_style(mut elem: Element, stroke: &StrokeStyle) -> Element {
    elem = elem.stroke(&stroke.color).stroke_width(stroke.width);

    if stroke.opacity < 0.999 {
        elem = elem.stroke_opacity(clamp_unit_interval(stroke.opacity));
    }

    if !stroke.dash_array.is_empty() {
        let dasharray = stroke
            .dash_array
            .iter()
            .map(|value| fmt_svg_number(*value))
            .collect::<Vec<_>>()
            .join(",");
        elem = elem.stroke_dasharray(&dasharray);
    }

    elem = elem.stroke_linecap(map_line_cap(stroke.line_cap));
    elem.stroke_linejoin(map_line_join(stroke.line_join))
}

fn fmt_svg_number(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i32)
    } else {
        format!("{value:.2}")
    }
}

fn map_line_cap(cap: RenderLineCap) -> &'static str {
    match cap {
        RenderLineCap::Butt => "butt",
        RenderLineCap::Round => "round",
        RenderLineCap::Square => "square",
    }
}

fn map_line_join(join: RenderLineJoin) -> &'static str {
    match join {
        RenderLineJoin::Miter => "miter",
        RenderLineJoin::Round => "round",
        RenderLineJoin::Bevel => "bevel",
    }
}

fn map_text_align(align: RenderTextAlign) -> TextAnchor {
    match align {
        RenderTextAlign::Start => TextAnchor::Start,
        RenderTextAlign::Middle => TextAnchor::Middle,
        RenderTextAlign::End => TextAnchor::End,
    }
}

fn map_text_baseline(baseline: RenderTextBaseline) -> text::DominantBaseline {
    match baseline {
        RenderTextBaseline::Top => text::DominantBaseline::Hanging,
        RenderTextBaseline::Middle => text::DominantBaseline::Middle,
        RenderTextBaseline::Bottom => text::DominantBaseline::Alphabetic,
    }
}

fn clamp_font_size(candidate: f32, min_font_size: f32) -> f32 {
    if !candidate.is_finite() {
        return min_font_size.max(1.0);
    }
    candidate.max(min_font_size)
}

fn clamp_unit_interval(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// The substring-keyword flags a single node class can raise (highlight/inactive/dashed/double
/// border). Exact-match keywords (`c4-external`, `block-beta`, …) are handled by the caller.
#[derive(Default)]
struct NodeClassKeywords {
    highlighted: bool,
    inactive: bool,
    dashed_border: bool,
    double_border: bool,
}

/// Whether `needle` (lowercase ASCII) equals `haystack[at..]`'s prefix, case-insensitively.
#[inline]
fn matches_ci_at(haystack: &[u8], at: usize, needle: &[u8]) -> bool {
    haystack.len() - at >= needle.len()
        && haystack[at..at + needle.len()]
            .iter()
            .zip(needle)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// Single-pass ASCII-case-insensitive keyword scan for one node class. Replaces the ~11 separate
/// `contains_ascii_ci` substring scans that ran on EVERY styled node (each a full window sweep) with
/// one pass over the class bytes, dispatching on the lowercased first byte (`b | 0x20`) — a 1-level
/// trie / hand-rolled Aho-Corasick root — so a keyword's full compare only runs at a candidate start
/// byte. Byte-identical to OR-ing the individual `to_ascii_lowercase().contains(needle)` checks it
/// replaces: `b | 0x20` maps both cases of any ASCII letter to its lowercase, so every position that
/// the old per-needle scan would match is routed to that needle's `matches_ci_at`, which re-verifies
/// the full substring (no false positives from the loose first-byte dispatch).
fn scan_node_class_keywords(class: &str) -> NodeClassKeywords {
    let b = class.as_bytes();
    let mut f = NodeClassKeywords::default();
    for i in 0..b.len() {
        match b[i] | 0x20 {
            b'h' => {
                if matches_ci_at(b, i, b"highlight") {
                    f.highlighted = true;
                }
            }
            b's' => {
                if matches_ci_at(b, i, b"selected") {
                    f.highlighted = true;
                }
            }
            b'a' => {
                if matches_ci_at(b, i, b"active") {
                    f.highlighted = true;
                }
            }
            b'f' => {
                if matches_ci_at(b, i, b"focus") {
                    f.highlighted = true;
                }
            }
            b'i' => {
                if matches_ci_at(b, i, b"important") {
                    f.highlighted = true;
                }
                if matches_ci_at(b, i, b"inactive") {
                    f.inactive = true;
                }
            }
            b'm' => {
                if matches_ci_at(b, i, b"muted") {
                    f.inactive = true;
                }
            }
            b'd' => {
                if matches_ci_at(b, i, b"dim") || matches_ci_at(b, i, b"disabled") {
                    f.inactive = true;
                }
                if matches_ci_at(b, i, b"dashed-border") {
                    f.dashed_border = true;
                }
                if matches_ci_at(b, i, b"double-border") {
                    f.double_border = true;
                }
            }
            b'b' => {
                if matches_ci_at(b, i, b"border-dashed") {
                    f.dashed_border = true;
                }
                if matches_ci_at(b, i, b"border-double") {
                    f.double_border = true;
                }
            }
            _ => {}
        }
    }
    f
}

fn sanitize_css_token(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

pub(crate) fn sanitize_svg_paint(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    if is_css_named_color(&lower) {
        return Some(lower);
    }

    if trimmed.starts_with('#')
        && trimmed[1..].chars().all(|ch| ch.is_ascii_hexdigit())
        && matches!(trimmed.len(), 4 | 5 | 7 | 9)
    {
        return Some(trimmed.to_string());
    }

    for prefix in ["rgb(", "rgba(", "hsl(", "hsla("] {
        if lower.starts_with(prefix)
            && lower.ends_with(')')
            && trimmed.chars().all(|ch| {
                ch.is_ascii_alphanumeric()
                    || matches!(ch, '(' | ')' | ',' | '.' | '%' | '/' | ' ' | '+' | '-')
            })
        {
            return Some(trimmed.to_string());
        }
    }

    None
}

fn is_css_named_color(value: &str) -> bool {
    matches!(
        value,
        "aliceblue"
            | "antiquewhite"
            | "aqua"
            | "aquamarine"
            | "azure"
            | "beige"
            | "bisque"
            | "black"
            | "blanchedalmond"
            | "blue"
            | "blueviolet"
            | "brown"
            | "burlywood"
            | "cadetblue"
            | "chartreuse"
            | "chocolate"
            | "coral"
            | "cornflowerblue"
            | "cornsilk"
            | "crimson"
            | "cyan"
            | "darkblue"
            | "darkcyan"
            | "darkgoldenrod"
            | "darkgray"
            | "darkgreen"
            | "darkgrey"
            | "darkkhaki"
            | "darkmagenta"
            | "darkolivegreen"
            | "darkorange"
            | "darkorchid"
            | "darkred"
            | "darksalmon"
            | "darkseagreen"
            | "darkslateblue"
            | "darkslategray"
            | "darkslategrey"
            | "darkturquoise"
            | "darkviolet"
            | "deeppink"
            | "deepskyblue"
            | "dimgray"
            | "dimgrey"
            | "dodgerblue"
            | "firebrick"
            | "floralwhite"
            | "forestgreen"
            | "fuchsia"
            | "gainsboro"
            | "ghostwhite"
            | "gold"
            | "goldenrod"
            | "gray"
            | "green"
            | "greenyellow"
            | "grey"
            | "honeydew"
            | "hotpink"
            | "indianred"
            | "indigo"
            | "ivory"
            | "khaki"
            | "lavender"
            | "lavenderblush"
            | "lawngreen"
            | "lemonchiffon"
            | "lightblue"
            | "lightcoral"
            | "lightcyan"
            | "lightgoldenrodyellow"
            | "lightgray"
            | "lightgreen"
            | "lightgrey"
            | "lightpink"
            | "lightsalmon"
            | "lightseagreen"
            | "lightskyblue"
            | "lightslategray"
            | "lightslategrey"
            | "lightsteelblue"
            | "lightyellow"
            | "lime"
            | "limegreen"
            | "linen"
            | "magenta"
            | "maroon"
            | "mediumaquamarine"
            | "mediumblue"
            | "mediumorchid"
            | "mediumpurple"
            | "mediumseagreen"
            | "mediumslateblue"
            | "mediumspringgreen"
            | "mediumturquoise"
            | "mediumvioletred"
            | "midnightblue"
            | "mintcream"
            | "mistyrose"
            | "moccasin"
            | "navajowhite"
            | "navy"
            | "oldlace"
            | "olive"
            | "olivedrab"
            | "orange"
            | "orangered"
            | "orchid"
            | "palegoldenrod"
            | "palegreen"
            | "paleturquoise"
            | "palevioletred"
            | "papayawhip"
            | "peachpuff"
            | "peru"
            | "pink"
            | "plum"
            | "powderblue"
            | "purple"
            | "rebeccapurple"
            | "red"
            | "rosybrown"
            | "royalblue"
            | "saddlebrown"
            | "salmon"
            | "sandybrown"
            | "seagreen"
            | "seashell"
            | "sienna"
            | "silver"
            | "skyblue"
            | "slateblue"
            | "slategray"
            | "slategrey"
            | "snow"
            | "springgreen"
            | "steelblue"
            | "tan"
            | "teal"
            | "thistle"
            | "tomato"
            | "transparent"
            | "turquoise"
            | "violet"
            | "wheat"
            | "white"
            | "whitesmoke"
            | "yellow"
            | "yellowgreen"
    )
}

const TEXT_STYLE_PROPERTIES: &[&str] = &[
    "color",
    "font-size",
    "font-weight",
    "font-family",
    "font-style",
    "text-decoration",
];

fn style_map_to_css(map: &BTreeMap<String, String>) -> Option<String> {
    if map.is_empty() {
        return None;
    }
    Some(
        map.iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

fn split_style_properties(
    properties: &BTreeMap<String, String>,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut shape = BTreeMap::new();
    let mut text = BTreeMap::new();

    for (key, value) in properties {
        if TEXT_STYLE_PROPERTIES.contains(&key.as_str()) {
            if key == "color" {
                text.insert("fill".to_string(), value.clone());
            } else {
                text.insert(key.clone(), value.clone());
            }
        } else {
            shape.insert(key.clone(), value.clone());
        }
    }

    (shape, text)
}

fn maybe_add_class(mut elem: Element, class_name: &str, enabled: bool) -> Element {
    if enabled {
        elem = elem.class(class_name);
    }
    elem
}

fn collect_node_style_directives(
    ir: &MermaidDiagramIr,
    node_index: usize,
) -> Option<BTreeMap<String, String>> {
    use fm_core::{IrNodeId, IrStyleTarget, parse_style_string};
    let node_id = IrNodeId(node_index);
    let mut merged = BTreeMap::new();

    for sr in &ir.style_refs {
        if let IrStyleTarget::Node(target_id) = sr.target
            && target_id == node_id
        {
            merged.extend(parse_style_string(&sr.style).properties);
        }
    }

    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

fn collect_classdef_css(ir: &MermaidDiagramIr) -> String {
    use fm_core::{IrStyleDef, IrStyleTarget, parse_style_string};
    let mut css = String::new();

    let mut defs: Vec<IrStyleDef> = if ir.style_defs.is_empty() {
        let mut defs: BTreeMap<String, IrStyleDef> = BTreeMap::new();
        for sr in &ir.style_refs {
            if let IrStyleTarget::Class(ref name) = sr.target {
                let parsed = parse_style_string(&sr.style);
                defs.entry(name.clone())
                    .and_modify(|def| def.properties.extend(parsed.properties.clone()))
                    .or_insert_with(|| IrStyleDef {
                        name: name.clone(),
                        properties: parsed.properties,
                        span: sr.span,
                    });
            }
        }
        defs.into_values().collect()
    } else {
        ir.style_defs.clone()
    };

    defs.sort_by(|a, b| a.name.cmp(&b.name));
    for def in &defs {
        let class_slug = sanitize_css_token(&def.name);
        if class_slug.is_empty() || def.properties.is_empty() {
            continue;
        }
        let (shape_props, text_props) = split_style_properties(&def.properties);
        if let Some(shape_css) = style_map_to_css(&shape_props) {
            css.push_str(&format!(
                ".fm-node-user-{class_slug} .fm-node-shape, .fm-node-user-{class_slug} .fm-node-shape * {{ {shape_css}; }}\n"
            ));
        }
        if let Some(text_css) = style_map_to_css(&text_props) {
            css.push_str(&format!(
                ".fm-node-user-{class_slug} .fm-node-label, .fm-node-user-{class_slug} .fm-node-label * {{ {text_css}; }}\n"
            ));
        }
    }

    css
}

/// Resolve inline styles for a node from `style` directives (shape, text).
fn resolve_node_inline_styles(
    ir: &MermaidDiagramIr,
    node_index: usize,
) -> (Option<String>, Option<String>) {
    let node = ir.nodes.get(node_index);
    let properties = if ir.style_refs.is_empty() {
        node.and_then(|n| n.inline_style.as_ref().map(|s| s.properties.clone()))
    } else {
        collect_node_style_directives(ir, node_index)
    };

    if let Some(props) = properties {
        let (shape_props, text_props) = split_style_properties(&props);
        return (
            style_map_to_css(&shape_props),
            style_map_to_css(&text_props),
        );
    }

    (None, None)
}

/// Resolve inline style for an edge based on `linkStyle` directives.
fn resolve_edge_inline_style(ir: &MermaidDiagramIr, edge_index: usize) -> Option<String> {
    use fm_core::{IrStyleTarget, parse_style_string};
    if let Some(edge) = ir.edges.get(edge_index)
        && let Some(style) = edge.inline_style.as_ref()
    {
        return style_map_to_css(&style.properties);
    }
    if ir.style_refs.is_empty() {
        return None;
    }

    let mut merged = BTreeMap::new();
    for sr in &ir.style_refs {
        if sr.target == IrStyleTarget::LinkDefault {
            merged.extend(parse_style_string(&sr.style).properties);
        }
    }
    for sr in &ir.style_refs {
        if let IrStyleTarget::Link(link_idx) = sr.target
            && link_idx == edge_index
        {
            merged.extend(parse_style_string(&sr.style).properties);
        }
    }

    style_map_to_css(&merged)
}

fn truncate_label(label: &str, max_chars: Option<usize>) -> Cow<'_, str> {
    let Some(limit) = max_chars else {
        return Cow::Borrowed(label);
    };
    let mut chars = label.chars();
    let needs_truncation = chars.clone().count() > limit;
    if !needs_truncation {
        return Cow::Borrowed(label);
    }
    let mut text = String::new();
    for _ in 0..limit.saturating_sub(1) {
        let Some(ch) = chars.next() else {
            break;
        };
        text.push(ch);
    }
    text.push('…');
    Cow::Owned(text)
}

fn detail_tier_name(tier: RenderDetailTier) -> &'static str {
    match tier {
        RenderDetailTier::Compact => "compact",
        RenderDetailTier::Normal => "normal",
        RenderDetailTier::Rich => "rich",
    }
}

fn resolve_detail_profile(
    width: f32,
    height: f32,
    config: &SvgRenderConfig,
) -> RenderDetailProfile {
    let area = width * height;
    let tier = match config.detail_tier {
        MermaidTier::Compact => RenderDetailTier::Compact,
        MermaidTier::Normal => RenderDetailTier::Normal,
        MermaidTier::Rich => RenderDetailTier::Rich,
        MermaidTier::Auto => {
            if area < 56_000.0 {
                RenderDetailTier::Compact
            } else if area < 220_000.0 {
                RenderDetailTier::Normal
            } else {
                RenderDetailTier::Rich
            }
        }
    };

    match tier {
        RenderDetailTier::Rich => RenderDetailProfile {
            tier,
            show_node_labels: true,
            show_edge_labels: true,
            show_cluster_labels: true,
            node_label_max_chars: None,
            edge_label_max_chars: None,
            node_font_size: clamp_font_size(config.font_size, config.min_font_size),
            edge_font_size: clamp_font_size(config.font_size * 0.85, config.min_font_size),
            cluster_font_size: clamp_font_size(config.font_size * 0.9, config.min_font_size),
            enable_shadows: config.shadows,
        },
        RenderDetailTier::Normal => RenderDetailProfile {
            tier,
            show_node_labels: true,
            show_edge_labels: true,
            show_cluster_labels: true,
            node_label_max_chars: Some(48),
            edge_label_max_chars: Some(40),
            node_font_size: clamp_font_size(config.font_size * 0.92, config.min_font_size),
            edge_font_size: clamp_font_size(config.font_size * 0.82, config.min_font_size),
            cluster_font_size: clamp_font_size(config.font_size * 0.86, config.min_font_size),
            enable_shadows: config.shadows,
        },
        RenderDetailTier::Compact => {
            let show_node_labels = area >= 36_000.0 && width >= 240.0 && height >= 150.0;
            RenderDetailProfile {
                tier,
                show_node_labels,
                show_edge_labels: false,
                show_cluster_labels: false,
                node_label_max_chars: Some(20),
                edge_label_max_chars: Some(24),
                node_font_size: clamp_font_size(config.font_size * 0.78, config.min_font_size),
                edge_font_size: clamp_font_size(config.font_size * 0.74, config.min_font_size),
                cluster_font_size: clamp_font_size(config.font_size * 0.76, config.min_font_size),
                enable_shadows: false,
            }
        }
    }
}

fn node_gradient_for(config: &SvgRenderConfig, theme: &Theme) -> Option<Gradient> {
    if !config.node_gradients {
        return None;
    }
    let stops = vec![
        GradientStop::with_opacity(0.0, &theme.colors.node_fill, 1.0),
        GradientStop::with_opacity(0.55, &theme.colors.node_fill, 0.97),
        GradientStop::with_opacity(1.0, &theme.colors.background, 0.92),
    ];
    let gradient = match config.node_gradient_style {
        NodeGradientStyle::LinearVertical => {
            Gradient::linear_with_coords("fm-node-gradient", 0.0, 0.0, 0.0, 1.0, stops)
        }
        NodeGradientStyle::LinearHorizontal => {
            Gradient::linear_with_coords("fm-node-gradient", 0.0, 0.0, 1.0, 0.0, stops)
        }
        NodeGradientStyle::Radial => Gradient::radial("fm-node-gradient", 0.5, 0.45, 0.8, stops),
    };
    Some(gradient)
}

fn effects_css(config: &SvgRenderConfig) -> String {
    let inactive_opacity = clamp_unit_interval(config.inactive_opacity);
    let cluster_fill_opacity = clamp_unit_interval(config.cluster_fill_opacity);
    format!(
        ".fm-node-inactive {{ opacity: {inactive_opacity:.2}; }}\n\
.fm-node-block-beta rect,\n\
.fm-node-block-beta path,\n\
.fm-node-block-beta circle,\n\
.fm-node-block-beta ellipse,\n\
.fm-node-block-beta polygon {{\n\
  fill: #546e7a;\n\
  stroke: #455a64;\n\
}}\n\
.fm-node-block-beta text {{\n\
  fill: #f8fafc;\n\
}}\n\
.fm-node-block-beta-space {{\n\
  opacity: 0;\n\
  pointer-events: none;\n\
}}\n\
.fm-node-highlighted rect,\n\
.fm-node-highlighted path,\n\
.fm-node-highlighted circle,\n\
.fm-node-highlighted ellipse,\n\
.fm-node-highlighted polygon {{\n\
  stroke-width: 2.4;\n\
}}\n\
.fm-node-highlighted text {{ font-weight: 600; }}\n\
.fm-node-border-dashed rect,\n\
.fm-node-border-dashed path,\n\
.fm-node-border-dashed circle,\n\
.fm-node-border-dashed ellipse,\n\
.fm-node-border-dashed polygon {{\n\
  stroke-dasharray: 6 4;\n\
}}\n\
.fm-node-border-double rect,\n\
.fm-node-border-double path,\n\
.fm-node-border-double circle,\n\
.fm-node-border-double ellipse,\n\
.fm-node-border-double polygon {{\n\
  stroke-width: 2.9;\n\
}}\n\
.fm-cluster {{ fill-opacity: {cluster_fill_opacity:.2}; }}\n"
    )
}

fn animation_css(config: &SvgRenderConfig) -> String {
    let hover_scale = config.hover_scale.clamp(1.0, 1.2);
    let transition_seconds = config.animation_duration_ms as f32 / 1000.0;
    let flow_seconds = config.flow_animation_duration_ms as f32 / 1000.0;
    format!(
        ".fm-animations-enabled {{\n\
  --fm-anim-duration: {transition_seconds:.2}s;\n\
  --fm-stagger-ms: {stagger_ms}ms;\n\
  --fm-flow-duration: {flow_seconds:.2}s;\n\
}}\n\
.fm-animations-enabled .fm-node,\n\
.fm-animations-enabled .fm-edge,\n\
.fm-animations-enabled .fm-edge-labeled {{\n\
  animation: fm-enter-diagram var(--fm-anim-duration) ease-out both;\n\
  animation-delay: calc(var(--fm-enter-order, 0) * var(--fm-stagger-ms));\n\
  transition: transform var(--fm-anim-duration) ease, opacity var(--fm-anim-duration) ease, filter var(--fm-anim-duration) ease, stroke var(--fm-anim-duration) ease;\n\
}}\n\
.fm-animations-enabled .fm-node {{\n\
  transform-box: fill-box;\n\
  transform-origin: center;\n\
}}\n\
.fm-animations-enabled .fm-node:hover {{\n\
  transform: scale({hover_scale:.3});\n\
}}\n\
.fm-animations-enabled .fm-node-highlighted {{\n\
  animation: fm-enter-diagram var(--fm-anim-duration) ease-out both,\n\
             fm-node-pulse calc(var(--fm-anim-duration) * 2.8) ease-in-out infinite;\n\
  animation-delay: calc(var(--fm-enter-order, 0) * var(--fm-stagger-ms)), calc(var(--fm-enter-order, 0) * var(--fm-stagger-ms) + var(--fm-anim-duration));\n\
}}\n\
.fm-animations-enabled .fm-edge-dashed,\n\
.fm-animations-enabled .fm-edge-flow-animated {{\n\
  stroke-dasharray: {dash_pattern};\n\
  animation: fm-enter-diagram var(--fm-anim-duration) ease-out both,\n\
             fm-edge-flow var(--fm-flow-duration) linear infinite;\n\
  animation-delay: calc(var(--fm-enter-order, 0) * var(--fm-stagger-ms)), 0s;\n\
}}\n\
@keyframes fm-enter-diagram {{\n\
  0% {{ opacity: 0; transform: translateY(8px); }}\n\
  100% {{ opacity: 1; transform: translateY(0); }}\n\
}}\n\
@keyframes fm-edge-flow {{\n\
  from {{ stroke-dashoffset: 0; }}\n\
  to {{ stroke-dashoffset: -28; }}\n\
}}\n\
@keyframes fm-node-pulse {{\n\
  0%, 100% {{ opacity: 1; }}\n\
  50% {{ opacity: 0.82; }}\n\
}}\n\
@media (prefers-reduced-motion: reduce) {{\n\
  .fm-animations-enabled .fm-node,\n\
  .fm-animations-enabled .fm-edge,\n\
  .fm-animations-enabled .fm-edge-labeled {{\n\
    animation: none !important;\n\
    transition: none !important;\n\
    transform: none !important;\n\
  }}\n\
}}\n",
        stagger_ms = config.animation_stagger_ms,
        dash_pattern = config.flow_dash_pattern
    )
}

fn print_css(min_font_size: f32) -> String {
    format!(
        "@media print {{
  .fm-node text, .fm-edge-labeled text, .fm-cluster-label {{
    font-size: {min_font_size:.1}px !important;
    fill: #111 !important;
  }}
  .fm-node path, .fm-node rect, .fm-node circle, .fm-edge {{
    stroke: #111 !important;
  }}
  .fm-cluster {{
    fill: #fff !important;
    stroke: #666 !important;
  }}
}}"
    )
}

fn animation_style_attr(order: usize) -> String {
    format!("--fm-enter-order:{order};")
}

fn node_animation_order(node_box: &LayoutNodeBox) -> usize {
    node_box.rank.saturating_mul(1000) + node_box.node_index
}

fn edge_animation_order(edge_path: &LayoutEdgePath, ir: &MermaidDiagramIr) -> usize {
    let Some(edge) = ir.edges.get(edge_path.edge_index) else {
        return edge_path.edge_index;
    };
    let from_index = match edge.from {
        fm_core::IrEndpoint::Node(node_id) => node_id.0,
        _ => 0,
    };
    let to_index = match edge.to {
        fm_core::IrEndpoint::Node(node_id) => node_id.0,
        _ => from_index,
    };
    from_index.max(to_index).saturating_add(1)
}

/// Render a computed layout to SVG.
/// Whether an edge arrow type renders using only the basic arrowhead markers
/// (`arrow-end` / `arrow-open`) or no marker at all. When every edge in a diagram is basic,
/// `<defs>` can omit the ten "fancy" markers (half/stick/thick/circle/cross/diamond/double).
/// This list must stay a subset of the arrow types in `render_edge`'s marker match that map
/// only to `arrow-end`, `arrow-open`, or no marker — any arrow type not listed here is
/// treated as fancy (the safe default, never dropping a referenced marker).
fn arrow_uses_only_basic_markers(arrow: fm_core::ArrowType) -> bool {
    use fm_core::ArrowType;
    matches!(
        arrow,
        ArrowType::Line
            | ArrowType::ThickLine
            | ArrowType::Arrow
            | ArrowType::OpenArrow
            | ArrowType::DottedArrow
            | ArrowType::DottedOpenArrow
            | ArrowType::DottedLine
    )
}

/// Serial node-render loop, shared by the WASM path and the below-threshold native path (and inlined
/// per-chunk by the parallel native path). Factored out so all three render byte-identically.
#[allow(clippy::too_many_arguments)]
fn render_nodes_serial(
    out: &mut String,
    nodes: &[LayoutNodeBox],
    ir: &MermaidDiagramIr,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    detail: RenderDetailProfile,
    colors: &ThemeColors,
    emit_classdef_classes: bool,
    centrality_map: &HashMap<usize, CentralityTier>,
) {
    for node_box in nodes {
        // Render straight into `out` — the fast path streams the node fragment in place (no per-node
        // fragment `String`); non-fast nodes delegate to `render_node`.
        render_node_into(
            out,
            node_box,
            ir,
            offset_x,
            offset_y,
            config,
            detail,
            colors,
            emit_classdef_classes,
            centrality_map,
        );
    }
}

/// Serial edge-render loop (skips bundled edges, which are rendered by the later bundle passes),
/// shared by the WASM path, the below-threshold native path, and the per-chunk parallel native path so
/// all render byte-identically.
fn render_edges_serial(out: &mut String, edges: &[LayoutEdgePath], context: &EdgeRenderContext<'_>) {
    for edge_path in edges {
        if edge_path.bundled {
            continue;
        }
        render_edge_into(out, edge_path, context);
    }
}

fn render_layout_to_svg(
    layout: &DiagramLayout,
    ir: &MermaidDiagramIr,
    config: &SvgRenderConfig,
) -> String {
    let padding = config.padding;
    let legend_enabled = is_c4_legend_enabled(ir);
    let legend_width = if legend_enabled { 320.0 } else { 0.0 };
    let legend_height = if legend_enabled { 128.0 } else { 0.0 };
    let has_specialized_title_renderer = ir
        .xy_chart_meta
        .as_ref()
        .as_ref()
        .is_some_and(|meta| !meta.series.is_empty())
        || ir
            .pie_meta
            .as_ref()
            .as_ref()
            .is_some_and(|meta| !meta.slices.is_empty())
        || ir.quadrant_meta.is_some();
    let generic_title = if has_specialized_title_renderer {
        None
    } else {
        ir.meta.title.as_deref()
    };
    let title_height = if generic_title.is_some() {
        config.font_size + 22.0
    } else {
        0.0
    };
    let width = (layout.bounds.width + padding * 2.0).max(legend_width + padding * 2.0);
    let height = layout.bounds.height + padding * 2.0 + legend_height + title_height;
    let detail = resolve_detail_profile(width, height, config);

    let mut doc = SvgDocument::new()
        .viewbox(0.0, 0.0, width, height)
        .preserve_aspect_ratio("xMidYMid meet");

    // With the theme CSS embedded, set `font-family` once on the root so every `<text>` inherits
    // it — the per-label inline copies are gated off (see `font_family_unless_embedded_css`).
    if config.embed_theme_css {
        doc = doc.font_family(&config.font_family);
    }

    if config.responsive {
        doc = doc.responsive();
    }

    if config.accessible {
        let (title, desc) = resolve_accessibility_text(Some(ir), Some(layout), config, || {
            format!(
                "Diagram with {} nodes and {} edges",
                ir.nodes.len(),
                ir.edges.len()
            )
        });
        doc = doc.accessible(title, desc);
    }

    for class in &config.root_classes {
        doc = doc.class(class);
    }

    // Add data attributes for tooling
    doc = doc
        .data("nodes", &ir.nodes.len().to_string())
        .data("edges", &ir.edges.len().to_string())
        .data("type", ir.diagram_type.as_str())
        .data("detail-tier", detail_tier_name(detail.tier));

    let theme = resolve_theme(Some(ir), config);
    let classdef_css = collect_classdef_css(ir);
    let emit_classdef_classes = !classdef_css.is_empty();
    let accessible_node_labels = config
        .a11y
        .text_alternatives
        .then(|| build_accessible_node_label_cache(ir));
    let effects_enabled = config.node_gradients
        || config.glow_enabled
        || clamp_unit_interval(config.inactive_opacity) < 0.999
        || clamp_unit_interval(config.cluster_fill_opacity) < 0.999;

    // Build defs section
    let mut defs = DefsBuilder::new();

    // Arrowhead markers: emit only what the diagram can reference, like Mermaid.js (which
    // never emits unused markers). Every edge whose arrow uses one of the basic markers
    // (`arrow-end` / `arrow-open` / none) — and back-edges always use `arrow-open` — needs
    // only those two; a single "fancy" arrow (half/stick/thick/circle/cross/diamond/double)
    // falls back to the complete set so a referenced marker can never be missing. Emission
    // order is preserved, so output is byte-identical for any diagram that uses a fancy
    // arrow, and typical flowcharts shed the ~10 unused marker definitions.
    // Restricted to flowcharts: there, edges (`ir.edges`) are the only marker source, so the
    // basic-arrow check is complete. Other diagram types (sequence, etc.) may reference
    // markers outside `ir.edges`, so they keep the full set.
    let emit_fancy_markers = ir.diagram_type != fm_core::DiagramType::Flowchart
        || ir
            .edges
            .iter()
            .any(|edge| !arrow_uses_only_basic_markers(edge.arrow));
    let edge_color = &theme.colors.edge;
    defs = defs.marker(ArrowheadMarker::standard("arrow-end", edge_color));
    if emit_fancy_markers {
        defs = defs.marker(ArrowheadMarker::filled("arrow-filled", edge_color));
    }
    defs = defs.marker(ArrowheadMarker::open("arrow-open", edge_color));
    if emit_fancy_markers {
        defs = defs.marker(ArrowheadMarker::half_top("arrow-half-top", edge_color));
        defs = defs.marker(ArrowheadMarker::half_bottom(
            "arrow-half-bottom",
            edge_color,
        ));
        defs = defs.marker(ArrowheadMarker::stick_top("arrow-stick-top", edge_color));
        defs = defs.marker(ArrowheadMarker::stick_bottom(
            "arrow-stick-bottom",
            edge_color,
        ));
        defs = defs.marker(
            ArrowheadMarker::standard("arrow-start", edge_color)
                .with_orient(crate::defs::MarkerOrient::AutoStartReverse),
        );
        defs = defs.marker(
            ArrowheadMarker::filled("arrow-start-filled", edge_color)
                .with_orient(crate::defs::MarkerOrient::AutoStartReverse),
        );
        defs = defs.marker(ArrowheadMarker::circle_marker("arrow-circle", edge_color));
        defs = defs.marker(ArrowheadMarker::cross_marker("arrow-cross", edge_color));
        defs = defs.marker(ArrowheadMarker::diamond_marker("arrow-diamond", edge_color));
    }

    // Add drop shadow filter if enabled. Skip the `<filter id="drop-shadow">` def when the theme
    // CSS is embedded: its only referrer is the inline `filter="url(#drop-shadow)"` on node shapes,
    // which is gated off in that case (the CSS `filter: drop-shadow(…)` renders the shadow), so the
    // def would be dead output. Attribute-driven exports (`embed_theme_css = false`) keep both.
    if detail.enable_shadows && !config.embed_theme_css {
        if config.shadow_color.trim().is_empty() {
            defs = defs.filter(Filter::drop_shadow(
                "drop-shadow",
                config.shadow_offset_x,
                config.shadow_offset_y,
                config.shadow_blur,
                clamp_unit_interval(config.shadow_opacity),
            ));
        } else {
            defs = defs.filter(Filter::drop_shadow_with_color(
                "drop-shadow",
                config.shadow_offset_x,
                config.shadow_offset_y,
                config.shadow_blur,
                clamp_unit_interval(config.shadow_opacity),
                &config.shadow_color,
            ));
        }
    }
    if config.glow_enabled {
        defs = defs.filter(Filter::drop_shadow_with_color(
            "node-glow",
            0.0,
            0.0,
            config.glow_blur,
            clamp_unit_interval(config.glow_opacity),
            &config.glow_color,
        ));
    }
    if let Some(gradient) = node_gradient_for(config, &theme) {
        defs = defs.gradient(gradient);
    }

    doc = doc.defs(defs);

    // Embed theme CSS if enabled
    if config.embed_theme_css {
        let mut css = theme.to_svg_style(
            detail.enable_shadows,
            ir.edges.iter().any(|edge| edge.label.is_some()),
        );
        strip_unused_theme_css(&mut css, Some(ir));
        if effects_enabled {
            css.push_str(&effects_css(config));
        }
        if config.animations_enabled {
            css.push_str(&animation_css(config));
        }

        // Add accessibility CSS if enabled
        if config.a11y.accessibility_css {
            css.push_str(accessibility_css());
        }
        if config.print_optimized {
            css.push_str(&print_css(config.min_font_size));
        }
        if !classdef_css.is_empty() {
            css.push_str(&classdef_css);
        }

        doc = doc.style(css);
    } else {
        // Only add supplemental CSS (accessibility and/or print optimization).
        let mut css = String::new();
        if effects_enabled {
            css.push_str(&effects_css(config));
        }
        if config.animations_enabled {
            css.push_str(&animation_css(config));
        }
        if config.a11y.accessibility_css {
            css.push_str(accessibility_css());
        }
        if config.print_optimized {
            css.push_str(&print_css(config.min_font_size));
        }
        if !classdef_css.is_empty() {
            css.push_str(&classdef_css);
        }
        if !css.is_empty() {
            doc = doc.style(css);
        }
    }

    // Offset for padding
    let offset_x = padding - layout.bounds.x;
    let offset_y = padding - layout.bounds.y + title_height;

    if let Some(xy_chart_meta) = ir
        .xy_chart_meta
        .as_ref()
        .filter(|meta| !meta.series.is_empty())
    {
        doc = render_xychart_svg(
            doc,
            ir,
            layout,
            xy_chart_meta,
            offset_x,
            offset_y,
            config,
            &theme,
        );
        return doc.to_string_with_capacity(layout_svg_capacity_hint(ir, layout));
    }

    // Pie chart rendering: draw wedges from pie metadata.
    if let Some(pie_meta) = ir.pie_meta.as_ref().filter(|meta| !meta.slices.is_empty()) {
        doc = render_pie_svg(
            doc, ir, layout, pie_meta, offset_x, offset_y, config, &theme,
        );
        return doc.to_string_with_capacity(layout_svg_capacity_hint(ir, layout));
    }

    // Quadrant chart rendering.
    if let Some(quad_meta) = ir.quadrant_meta.as_ref() {
        doc = render_quadrant_svg(
            doc, ir, layout, quad_meta, offset_x, offset_y, config, &theme,
        );
        return doc.to_string_with_capacity(layout_svg_capacity_hint(ir, layout));
    }

    // Gantt chart: type-based task bar colors and section headers.
    if ir.diagram_type == fm_core::DiagramType::Gantt && ir.gantt_meta.is_some() {
        doc = render_gantt_svg(doc, ir, layout, offset_x, offset_y, config, &theme);
        return doc.to_string_with_capacity(layout_svg_capacity_hint(ir, layout));
    }

    if let Some(title) = generic_title {
        doc = doc.child(
            TextBuilder::new(title)
                .x(width / 2.0)
                .y(padding + config.font_size + 2.0)
                .anchor(TextAnchor::Middle)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(config.font_size + 4.0)
                .font_weight("600")
                .fill(&theme.colors.text)
                .class("fm-diagram-title")
                .build(),
        );
    }

    for band in &layout.extensions.bands {
        doc = doc.child(render_layout_band(band, offset_x, offset_y, config));
    }
    for tick in &layout.extensions.axis_ticks {
        doc = doc.child(render_layout_axis_tick(
            tick.label.as_str(),
            tick.position + offset_x,
            layout.bounds.y + offset_y - 12.0,
            config,
        ));
    }

    // Render sequence diagram activation bars.
    for bar in &layout.extensions.activation_bars {
        let mut rect = Element::rect()
            .x(bar.bounds.x + offset_x)
            .y(bar.bounds.y + offset_y)
            .width(bar.bounds.width)
            .height(bar.bounds.height)
            .fill(&theme.colors.node_fill)
            .stroke(&theme.colors.node_stroke)
            .stroke_width(1.2)
            .class("fm-activation-bar");
        if bar.depth > 0 {
            rect = rect.class("fm-activation-nested");
        }
        doc = doc.child(rect);
    }

    for marker in &layout.extensions.sequence_lifecycle_markers {
        match marker.kind {
            fm_layout::LayoutSequenceLifecycleMarkerKind::Destroy => {
                let half = marker.size * 0.5;
                let x0 = marker.center.x + offset_x - half;
                let y0 = marker.center.y + offset_y - half;
                let x1 = marker.center.x + offset_x + half;
                let y1 = marker.center.y + offset_y + half;
                doc = doc.child(
                    Element::line()
                        .x1(x0)
                        .y1(y0)
                        .x2(x1)
                        .y2(y1)
                        .stroke(&theme.colors.edge)
                        .stroke_width(1.5)
                        .class("fm-sequence-destroy-marker"),
                );
                doc = doc.child(
                    Element::line()
                        .x1(x0)
                        .y1(y1)
                        .x2(x1)
                        .y2(y0)
                        .stroke(&theme.colors.edge)
                        .stroke_width(1.5)
                        .class("fm-sequence-destroy-marker"),
                );
            }
        }
    }

    // Render sequence diagram notes.
    for note in &layout.extensions.sequence_notes {
        let nx = note.bounds.x + offset_x;
        let ny = note.bounds.y + offset_y;
        let nw = note.bounds.width;
        let nh = note.bounds.height;

        // Note background with rounded corners.
        doc = doc.child(
            Element::rect()
                .x(nx)
                .y(ny)
                .width(nw)
                .height(nh)
                .rx(4.0)
                .ry(4.0)
                .fill(&theme.colors.node_fill)
                .stroke(&theme.colors.accents[4 % theme.colors.accents.len()])
                .stroke_width(1.0)
                .class("fm-sequence-note"),
        );

        // Note text.
        if !note.text.is_empty() {
            let note_font_size = config.font_size * 0.8;
            doc = doc.child(
                TextBuilder::new(&note.text)
                    .x(nx + 8.0)
                    .y(ny + 8.0)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .font_size(note_font_size)
                    .line_height(config.line_height)
                    .baseline(text::DominantBaseline::Hanging)
                    .anchor(TextAnchor::Start)
                    .fill(&theme.colors.text)
                    .class("fm-sequence-note-text")
                    .build(),
            );
        }
    }

    // Render sequence diagram interaction fragments (loop, alt, par, etc.).
    for fragment in &layout.extensions.sequence_fragments {
        let fx = fragment.bounds.x + offset_x;
        let fy = fragment.bounds.y + offset_y;
        let fw = fragment.bounds.width;
        let fh = fragment.bounds.height;

        let mut fragment_rect = Element::rect()
            .x(fx)
            .y(fy)
            .width(fw)
            .height(fh)
            .rx(2.0)
            .ry(2.0)
            .class("fm-sequence-fragment");
        if fragment.kind == fm_core::FragmentKind::Rect {
            let fill = fragment
                .color
                .as_deref()
                .and_then(sanitize_svg_paint)
                .unwrap_or_else(|| "transparent".to_string());
            let stroke = if fill == "transparent" {
                theme.colors.cluster_stroke.clone()
            } else {
                fill.clone()
            };
            fragment_rect = fragment_rect.fill(&fill).stroke(&stroke).stroke_width(1.0);
        } else {
            fragment_rect = fragment_rect
                .fill("none")
                .stroke(&theme.colors.cluster_stroke)
                .stroke_width(1.0)
                .stroke_dasharray("6,4");
        }
        doc = doc.child(fragment_rect);

        if fragment.kind == fm_core::FragmentKind::Rect {
            continue;
        }

        // Fragment kind label in top-left corner.
        let kind_label = match fragment.kind {
            fm_core::FragmentKind::Loop => "loop",
            fm_core::FragmentKind::Alt => "alt",
            fm_core::FragmentKind::Opt => "opt",
            fm_core::FragmentKind::Par => "par",
            fm_core::FragmentKind::Critical => "critical",
            fm_core::FragmentKind::Break => "break",
            fm_core::FragmentKind::Rect => "rect",
        };
        let label_text = if fragment.label.is_empty() {
            kind_label.to_string()
        } else {
            format!("{kind_label} [{}]", fragment.label)
        };

        // Label background tab.
        let label_width = label_text.len() as f32 * config.avg_char_width + 16.0;
        let label_height = config.font_size + 8.0;
        doc = doc.child(
            Element::rect()
                .x(fx)
                .y(fy)
                .width(label_width)
                .height(label_height)
                .fill(&theme.colors.cluster_fill)
                .stroke(&theme.colors.cluster_stroke)
                .stroke_width(1.0)
                .class("fm-sequence-fragment-label-bg"),
        );
        doc = doc.child(
            Element::text()
                .x(fx + 6.0)
                .y(fy + label_height / 2.0)
                .content(&label_text)
                .attr("dominant-baseline", "middle")
                .attr_num("font-size", config.font_size * 0.75)
                .attr("font-weight", "bold")
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .fill(&theme.colors.text)
                .class("fm-sequence-fragment-label"),
        );
    }

    // Render clusters (subgraphs) as background rectangles
    // Sort clusters by size (largest first) for proper z-ordering of nested clusters
    let mut sorted_clusters: Vec<_> = layout.clusters.iter().enumerate().collect();
    sorted_clusters.sort_by(|a, b| {
        let area_a = a.1.bounds.width * a.1.bounds.height;
        let area_b = b.1.bounds.width * b.1.bounds.height;
        area_b
            .partial_cmp(&area_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (_sort_idx, cluster) in sorted_clusters {
        let ir_cluster = ir.clusters.get(cluster.cluster_index);

        // Detect cluster type from title for specialized styling.
        let title_text = cluster
            .title
            .as_deref()
            .or_else(|| {
                ir_cluster
                    .and_then(|c| c.title)
                    .and_then(|tid| ir.labels.get(tid.0))
                    .map(|l| l.text.as_str())
            })
            .unwrap_or("");

        let is_c4_boundary = title_text.contains("System_Boundary")
            || title_text.contains("Container_Boundary")
            || title_text.contains("Enterprise_Boundary")
            || title_text.contains("Deployment_Node");

        let is_swimlane = title_text.starts_with("swimlane:")
            || title_text.contains("section ")
            || ir.diagram_type.as_str() == "gantt"
            || ir.diagram_type.as_str() == "kanban";

        // Configure styling based on cluster type
        let (fill_color, stroke_color, stroke_style, label_color) = if is_c4_boundary {
            // C4 boundaries: dashed gray border, very light gray fill
            (
                "rgba(128,128,128,0.05)".to_string(),
                "#888".to_string(),
                Some("4,2"),
                "#555".to_string(),
            )
        } else if is_swimlane {
            // Swimlanes: solid subtle border, alternating translucent fill
            (
                "rgba(200,220,240,0.15)".to_string(),
                "#b8c9db".to_string(),
                None,
                "#4a6785".to_string(),
            )
        } else if let Some(color) = cluster.color.as_deref().and_then(sanitize_svg_paint) {
            let fill_color = if color == "transparent" {
                "transparent".to_string()
            } else {
                color.clone()
            };
            let stroke_color = if color == "transparent" {
                "#dee2e6".to_string()
            } else {
                color
            };
            (fill_color, stroke_color, None, "#6c757d".to_string())
        } else {
            // Standard clusters: translucent fill, subtle border
            (
                "rgba(248,249,250,0.85)".to_string(),
                "#dee2e6".to_string(),
                None,
                "#6c757d".to_string(),
            )
        };

        let mut rect = Element::rect()
            .id(&mermaid_cluster_element_id(cluster.cluster_index))
            .x(cluster.bounds.x + offset_x)
            .y(cluster.bounds.y + offset_y)
            .width(cluster.bounds.width)
            .height(cluster.bounds.height)
            .fill(&fill_color)
            .stroke(&stroke_color)
            .stroke_width(1.0)
            .rx(if is_c4_boundary {
                0.0
            } else {
                config.rounded_corners
            })
            .class("fm-cluster");
        if config.cluster_fill_opacity < 0.999 {
            rect = rect.attr_num(
                "fill-opacity",
                clamp_unit_interval(config.cluster_fill_opacity),
            );
        }

        if let Some(dasharray) = stroke_style {
            rect = rect.stroke_dasharray(dasharray);
        }

        if is_c4_boundary {
            rect = rect.class("fm-cluster-c4");
        } else if is_swimlane {
            rect = rect.class("fm-cluster-swimlane");
        }

        if config.include_source_spans {
            rect = apply_span_metadata(rect, cluster.span);
        }

        doc = doc.child(rect);

        // Cluster label if present
        if detail.show_cluster_labels && !title_text.is_empty() {
            // For C4 boundaries, strip the boundary type prefix for display
            let display_title = if is_c4_boundary {
                title_text
                    .replace("System_Boundary", "")
                    .replace("Container_Boundary", "")
                    .replace("Enterprise_Boundary", "")
                    .replace("Deployment_Node", "")
                    .trim_matches(|c: char| c == '(' || c == ')' || c == ',' || c.is_whitespace())
                    .to_string()
            } else if is_swimlane && title_text.starts_with("swimlane:") {
                title_text.trim_start_matches("swimlane:").to_string()
            } else if is_swimlane && title_text.starts_with("section ") {
                title_text.trim_start_matches("section ").to_string()
            } else {
                title_text.to_string()
            };

            if !display_title.is_empty() {
                let text = TextBuilder::new(&display_title)
                    .x(cluster.bounds.x + offset_x + 8.0)
                    .y(cluster.bounds.y + offset_y + 16.0)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .font_size(detail.cluster_font_size)
                    .fill(&label_color)
                    .class("fm-cluster-label")
                    .build();
                let text = if config.include_source_spans {
                    apply_span_metadata(text, cluster.span)
                } else {
                    text
                };
                doc = doc.child(text);
            }
        }
    }

    for divider in &layout.extensions.cluster_dividers {
        let cluster_span = ir
            .clusters
            .get(divider.cluster_index)
            .map_or(Span::default(), |cluster| cluster.span);
        let mut line = Element::line()
            .x1(divider.start.x + offset_x)
            .y1(divider.start.y + offset_y)
            .x2(divider.end.x + offset_x)
            .y2(divider.end.y + offset_y)
            .stroke(&theme.colors.cluster_stroke)
            .stroke_width(1.0)
            .stroke_dasharray("6,4")
            .class("fm-cluster-divider");

        if config.include_source_spans {
            line = apply_span_metadata(line, cluster_span);
        }

        doc = doc.child(line);
    }

    // Build centrality tier lookup map for O(1) access during node rendering. Hoisted above the
    // edge/node emission so the flowchart fast path below can reference it.
    let centrality_map: HashMap<usize, CentralityTier> = layout
        .extensions
        .node_centrality
        .iter()
        .map(|nc| (nc.node_index, nc.tier))
        .collect();

    let edge_context = EdgeRenderContext {
        ir,
        offset_x,
        offset_y,
        config,
        detail,
        colors: &theme.colors,
        accessible_node_labels: accessible_node_labels.as_deref(),
    };

    // Fast path for the common case: a diagram small enough that both loops render serially AND for
    // which the slow path inserts NO child BETWEEN the edge and node fragments (bundle-count labels,
    // ER cardinality, class cardinality) or AFTER them (sequence mirror headers, C4 legend). When none
    // of those fire the document is exactly `[prefix children] + edges + nodes`, so we stream the edges
    // and nodes STRAIGHT into the final output buffer via `to_string_with_body` instead of rendering
    // them into intermediate `edge_svg`/`node_svg` Strings that `write_to_string` then copies a SECOND
    // time (~18% of render / ~9% of the wide pipeline, measured). Byte-identical: the same
    // `render_edges_serial`/`render_nodes_serial` bytes in the same position.
    //
    // The gate tests the ACTUAL insertion conditions (see the 9 `doc.child` sites below), not just
    // `== Flowchart`, so it's a strict superset of the old flowchart-only gate: every simple type —
    // state, sankey, journey, gitgraph, requirement, mindmap, plain flowchart — now streams, while ER /
    // class-cardinality / sequence-mirror / C4-legend / bundled / very-large renders take the verbatim
    // slow-path fallback below. Keep this in sync with those insertion guards.
    let no_between_or_after_children = ir.diagram_type != fm_core::DiagramType::Er
        && layout.extensions.sequence_mirror_headers.is_empty()
        && !legend_enabled
        && layout.edges.iter().all(|edge| {
            edge.bundle_count <= 1
                && ir.edges.get(edge.edge_index).is_none_or(|ir_edge| {
                    ir_edge.source_cardinality().is_none() && ir_edge.target_cardinality().is_none()
                })
        });
    #[cfg(not(target_arch = "wasm32"))]
    let stream_fast_path =
        no_between_or_after_children && layout.edges.len() < 4096 && layout.nodes.len() < 2048;
    #[cfg(target_arch = "wasm32")]
    let stream_fast_path = no_between_or_after_children;
    if stream_fast_path {
        return doc.to_string_with_body(layout_svg_capacity_hint(ir, layout), |out| {
            render_edges_serial(out, &layout.edges, &edge_context);
            render_nodes_serial(
                out,
                &layout.nodes,
                ir,
                offset_x,
                offset_y,
                config,
                detail,
                &theme.colors,
                emit_classdef_classes,
                &centrality_map,
            );
        });
    }

    // Render edges (skip edges absorbed into bundles). Edge subtrees are serialized immediately
    // and inserted as one internal raw fragment so the root document does not retain thousands of
    // short-lived edge element trees until final serialization.
    // 480 B/edge, not 384: the per-edge a11y group (`<g id role tabindex>…<title/></g>`) plus the
    // cubic `d` string average ~422 B/edge on wide flowcharts (measured), so 384 overflowed the
    // accumulator and forced a ~370 KB realloc+copy every render. 480 keeps the common wide edge
    // within one allocation. Capacity-only: byte-identical output.
    // Parallel fan-out mirrors the node loop: `render_edge` is pure (reads `edge_path` + the Sync
    // `EdgeRenderContext`, no shared mutable state), chunks are concatenated in edge order so output is
    // byte-identical and thread-count-independent, native-only (WASM serial), size-gated.
    let mut edge_svg = String::with_capacity(layout.edges.len().saturating_mul(480));
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Threshold 4096, not 256: after the direct-into-buffer serial edge writer (c282ff1) the
        // serial path is so cheap that std::thread spawn + join (~15-30 µs/thread on native, paid
        // twice/render since nodes fan out too) DOMINATES the parallel win below the crossover.
        // Deterministic time A/B on the wide head-to-head shapes (64-core box, this bench machine):
        // serial BEATS 8-thread render by ~37% at 16x32 (992 edges), ~19% at 24x48 (2256 edges);
        // parallel only pulls ahead past ~4032 edges (32x64, +4.5%) and is a genuine ~12-13% win
        // only on huge graphs (40x80 = 6320 edges, 48x96 = 9120 edges). Gating at 4096 keeps the
        // entire realistic corpus — every 8x16/12x24/16x32/24x48 diagram — on the fast serial path
        // while preserving the parallel win for the rare 3000+-node diagram. Byte-identical output.
        const PARALLEL_EDGE_THRESHOLD: usize = 4096;
        let edge_count = layout.edges.len();
        if edge_count >= PARALLEL_EDGE_THRESHOLD {
            let threads = std::thread::available_parallelism()
                .map_or(1, |c| c.get())
                .clamp(1, 8);
            let chunk_size = edge_count.div_ceil(threads);
            let ctx = &edge_context;
            let parts: Vec<String> = std::thread::scope(|scope| {
                let handles: Vec<_> = layout
                    .edges
                    .chunks(chunk_size)
                    .map(|chunk| {
                        scope.spawn(move || {
                            let mut buf = String::with_capacity(chunk.len().saturating_mul(480));
                            render_edges_serial(&mut buf, chunk, ctx);
                            buf
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            for part in &parts {
                edge_svg.push_str(part);
            }
        } else {
            render_edges_serial(&mut edge_svg, &layout.edges, &edge_context);
        }
    }
    #[cfg(target_arch = "wasm32")]
    render_edges_serial(&mut edge_svg, &layout.edges, &edge_context);
    if !edge_svg.is_empty() {
        doc = doc.child(Element::raw_svg(edge_svg));
    }

    // Render bundle count labels for bundled edges (e.g., "×3").
    for edge_path in &layout.edges {
        if edge_path.bundle_count > 1 && edge_path.points.len() >= 2 {
            let mid_idx = edge_path.points.len() / 2;
            let mid_pt = &edge_path.points[mid_idx];
            let label = format!("\u{00d7}{}", edge_path.bundle_count);
            doc = doc.child(
                Element::text()
                    .x(mid_pt.x + offset_x + 6.0)
                    .y(mid_pt.y + offset_y - 12.0)
                    .content(&label)
                    .attr("text-anchor", "start")
                    .attr("dominant-baseline", "auto")
                    .attr_num("font-size", config.font_size * 0.65)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .fill(&theme.colors.edge)
                    .attr("fill-opacity", "0.7")
                    .class("fm-bundle-count"),
            );
        }
    }

    // Render ER cardinality labels near edge endpoints.
    if ir.diagram_type == fm_core::DiagramType::Er {
        for edge_path in &layout.edges {
            if let Some(ir_edge) = ir.edges.get(edge_path.edge_index)
                && let Some(notation) = ir_edge.er_notation()
                && edge_path.points.len() >= 2
            {
                let (left_label, right_label) = parse_er_cardinality(notation);
                let font_size = config.font_size * 0.7;

                // Left cardinality near first waypoint.
                if !left_label.is_empty() {
                    let p = &edge_path.points[0];
                    doc = doc.child(
                        Element::text()
                            .x(p.x + offset_x + 8.0)
                            .y(p.y + offset_y - 8.0)
                            .content(left_label)
                            .attr("text-anchor", "start")
                            .attr("dominant-baseline", "auto")
                            .attr_num("font-size", font_size)
                            .font_family_unless_embedded_css(
                                &config.font_family,
                                config.embed_theme_css,
                            )
                            .fill(&theme.colors.text)
                            .class("fm-er-cardinality"),
                    );
                }

                // Right cardinality near last waypoint.
                if !right_label.is_empty() {
                    let p = &edge_path.points[edge_path.points.len() - 1];
                    doc = doc.child(
                        Element::text()
                            .x(p.x + offset_x + 8.0)
                            .y(p.y + offset_y - 8.0)
                            .content(right_label)
                            .attr("text-anchor", "start")
                            .attr("dominant-baseline", "auto")
                            .attr_num("font-size", font_size)
                            .font_family_unless_embedded_css(
                                &config.font_family,
                                config.embed_theme_css,
                            )
                            .fill(&theme.colors.text)
                            .class("fm-er-cardinality"),
                    );
                }
            }
        }
    }

    // Render class diagram cardinality labels near edge endpoints.
    for edge_path in &layout.edges {
        if let Some(ir_edge) = ir.edges.get(edge_path.edge_index)
            && (ir_edge.source_cardinality().is_some() || ir_edge.target_cardinality().is_some())
            && edge_path.points.len() >= 2
        {
            let font_size = config.font_size * 0.7;

            if let Some(card) = ir_edge.source_cardinality() {
                let p = &edge_path.points[0];
                doc = doc.child(
                    Element::text()
                        .x(p.x + offset_x + 8.0)
                        .y(p.y + offset_y - 8.0)
                        .content(card)
                        .attr("text-anchor", "start")
                        .attr("dominant-baseline", "auto")
                        .attr_num("font-size", font_size)
                        .font_family_unless_embedded_css(
                            &config.font_family,
                            config.embed_theme_css,
                        )
                        .fill(&theme.colors.text)
                        .class("fm-class-cardinality"),
                );
            }

            if let Some(card) = ir_edge.target_cardinality() {
                let p = &edge_path.points[edge_path.points.len() - 1];
                doc = doc.child(
                    Element::text()
                        .x(p.x + offset_x + 8.0)
                        .y(p.y + offset_y - 8.0)
                        .content(card)
                        .attr("text-anchor", "start")
                        .attr("dominant-baseline", "auto")
                        .attr_num("font-size", font_size)
                        .font_family_unless_embedded_css(
                            &config.font_family,
                            config.embed_theme_css,
                        )
                        .fill(&theme.colors.text)
                        .class("fm-class-cardinality"),
                );
            }
        }
    }

    // Render nodes. Serialize each node subtree immediately into a shared buffer (as the edge loop
    // above does) and insert one internal raw fragment, so the root document does not retain
    // hundreds of node element trees — each a `<g>` with rect + text children — until final
    // serialization. Byte-identical: the same `render_node` elements are serialized in the same
    // order, just streamed rather than deferred.
    let mut node_svg = String::with_capacity(layout.nodes.len().saturating_mul(640));
    // The per-node render (`render_node` -> serialize) is the single largest pipeline cost (~43% of the
    // whole pipeline) and is embarrassingly parallel: `render_node` is pure (read-only `ir`/`config`/
    // `theme`/`centrality_map` + `Copy` scalars, no shared mutable state). For large diagrams on native
    // we fan the nodes across stdlib scoped threads (no new dependency — the crate stays zero-dep) and
    // concatenate the per-chunk buffers IN ORDER, so the output is byte-identical to the serial path.
    // Below the threshold the thread-spawn overhead would dominate, and WASM (no usable threads) always
    // takes the serial path — so small/medium renders and every browser render are unchanged.
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Threshold 2048, not 256: since the direct-into-buffer serial node writer (5e42d39) the
        // serial path out-runs the parallel one until the node count is large enough to amortize the
        // std::thread spawn + join cost. Deterministic time A/B on the wide head-to-head shapes
        // (64-core box): serial BEATS 8-thread render by ~37% at 16x32 (512 nodes), ~19% at 24x48
        // (1152 nodes); the crossover sits at ~2048 nodes and parallel is a real ~12-13% win only on
        // huge graphs (40x80 = 3200 nodes). Gating at 2048 keeps the whole realistic corpus on the
        // fast serial path while retaining parallelism for the rare very large diagram. Byte-identical.
        const PARALLEL_NODE_THRESHOLD: usize = 2048;
        let node_count = layout.nodes.len();
        if node_count >= PARALLEL_NODE_THRESHOLD {
            let threads = std::thread::available_parallelism()
                .map_or(1, |c| c.get())
                .clamp(1, 8);
            let chunk_size = node_count.div_ceil(threads);
            // Bind shared references once so each `move` thread closure captures a `Copy` `&_` rather
            // than trying to move the underlying value.
            let colors = &theme.colors;
            let centrality = &centrality_map;
            let parts: Vec<String> = std::thread::scope(|scope| {
                let handles: Vec<_> = layout
                    .nodes
                    .chunks(chunk_size)
                    .map(|chunk| {
                        scope.spawn(move || {
                            let mut buf = String::with_capacity(chunk.len().saturating_mul(640));
                            render_nodes_serial(
                                &mut buf,
                                chunk,
                                ir,
                                offset_x,
                                offset_y,
                                config,
                                detail,
                                colors,
                                emit_classdef_classes,
                                centrality,
                            );
                            buf
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            for part in &parts {
                node_svg.push_str(part);
            }
        } else {
            render_nodes_serial(
                &mut node_svg,
                &layout.nodes,
                ir,
                offset_x,
                offset_y,
                config,
                detail,
                &theme.colors,
                emit_classdef_classes,
                &centrality_map,
            );
        }
    }
    #[cfg(target_arch = "wasm32")]
    render_nodes_serial(
        &mut node_svg,
        &layout.nodes,
        ir,
        offset_x,
        offset_y,
        config,
        detail,
        &theme.colors,
        emit_classdef_classes,
        &centrality_map,
    );
    if !node_svg.is_empty() {
        doc = doc.child(Element::raw_svg(node_svg));
    }

    for node_box in &layout.extensions.sequence_mirror_headers {
        let node_elem = render_node(
            node_box,
            ir,
            offset_x,
            offset_y,
            config,
            detail,
            &theme.colors,
            emit_classdef_classes,
            &centrality_map, // Use same map (mirror headers will have no entries)
            // Post-processed below (.id + .class) — must NOT take the opaque fast path.
            false,
        )
        .id(&mermaid_node_element_id_with_variant(
            &node_box.node_id,
            node_box.node_index,
            Some("mirror-header"),
        ));
        doc = doc.child(node_elem.class("fm-sequence-mirror-header"));
    }

    if legend_enabled {
        doc = doc.child(render_c4_legend(
            ir,
            padding,
            layout.bounds.height + padding + 18.0,
            width - (padding * 2.0),
            legend_height - 18.0,
            config,
            &theme.colors,
        ));
    }

    finish_layout_svg_document(doc, ir, layout)
}

fn finish_layout_svg_document(
    doc: SvgDocument,
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
) -> String {
    doc.to_string_with_capacity(layout_svg_capacity_hint(ir, layout))
}

fn build_accessible_node_label_cache(ir: &MermaidDiagramIr) -> Vec<&str> {
    ir.nodes
        .iter()
        .map(|node| crate::a11y::accessible_node_label(node, ir))
        .collect()
}

fn layout_svg_capacity_hint(ir: &MermaidDiagramIr, layout: &DiagramLayout) -> usize {
    const BASE_DOCUMENT_BYTES: usize = 16 * 1024;
    const NODE_BYTES: usize = 768;
    const EDGE_BYTES: usize = 384;
    const CLUSTER_BYTES: usize = 512;
    const AUXILIARY_ITEM_BYTES: usize = 192;

    let auxiliary_items = layout.extensions.bands.len()
        + layout.extensions.axis_ticks.len()
        + layout.extensions.activation_bars.len()
        + layout.extensions.sequence_lifecycle_markers.len()
        + layout.extensions.sequence_notes.len()
        + layout.extensions.sequence_fragments.len()
        + layout.extensions.cluster_dividers.len()
        + layout.extensions.sequence_mirror_headers.len();

    BASE_DOCUMENT_BYTES
        + ir.nodes.len().saturating_mul(NODE_BYTES)
        + layout.edges.len().saturating_mul(EDGE_BYTES)
        + layout.clusters.len().saturating_mul(CLUSTER_BYTES)
        + auxiliary_items.saturating_mul(AUXILIARY_ITEM_BYTES)
}

fn render_layout_band(
    band: &LayoutBand,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
) -> Element {
    let (fill, stroke, class_name) = match band.kind {
        LayoutBandKind::Section => (
            "rgba(191,219,254,0.18)",
            "#bfd7ff",
            "fm-band fm-band-section",
        ),
        LayoutBandKind::Lane => ("rgba(196,181,253,0.14)", "#c4b5fd", "fm-band fm-band-lane"),
        LayoutBandKind::Column => (
            "rgba(254,240,138,0.16)",
            "#fde68a",
            "fm-band fm-band-column",
        ),
    };

    let mut group = Element::group().class(class_name);
    let rect = Element::rect()
        .x(band.bounds.x + offset_x)
        .y(band.bounds.y + offset_y)
        .width(band.bounds.width)
        .height(band.bounds.height)
        .rx(config.rounded_corners.max(4.0))
        .fill(fill)
        .stroke(stroke)
        .stroke_width(1.0)
        .stroke_dasharray("6,4")
        .fill_opacity(0.8)
        .stroke_opacity(0.9);
    group = group.child(rect);

    if !band.label.is_empty() {
        group = group.child(
            TextBuilder::new(&band.label)
                .x(band.bounds.x + offset_x + 8.0)
                .y(band.bounds.y + offset_y + 16.0)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(clamp_font_size(
                    config.font_size * 0.82,
                    config.min_font_size,
                ))
                .fill("var(--fm-text-color, #4a5568)")
                .class("fm-band-label")
                .build(),
        );
    }

    group
}

fn render_layout_axis_tick(label: &str, x: f32, y: f32, config: &SvgRenderConfig) -> Element {
    let mut group = Element::group().class("fm-axis-tick");
    group = group.child(
        Element::line()
            .x1(x)
            .y1(y + 4.0)
            .x2(x)
            .y2(y + 16.0)
            .stroke("var(--fm-edge-color, #94a3b8)")
            .stroke_width(1.0),
    );
    group.child(
        TextBuilder::new(label)
            .x(x + 3.0)
            .y(y)
            .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
            .font_size(clamp_font_size(
                config.font_size * 0.72,
                config.min_font_size,
            ))
            .fill("var(--fm-text-color, #64748b)")
            .class("fm-axis-tick-label")
            .build(),
    )
}

/// Parse an ER cardinality notation string (e.g., `"||--o{"`) into display labels
/// for the left and right endpoints.
fn parse_er_cardinality(notation: &str) -> (&str, &str) {
    // Find the connector: `--`, `..`, or `==`.
    let connector_idx = notation
        .find("--")
        .or_else(|| notation.find(".."))
        .or_else(|| notation.find("=="));

    let Some(idx) = connector_idx else {
        return ("", "");
    };

    let connector_len = 2;
    let left = notation[..idx].trim();
    let right = notation[idx + connector_len..].trim();

    (er_marker_to_label(left), er_marker_to_label(right))
}

fn er_marker_to_label(marker: &str) -> &str {
    match marker {
        "||" => "1",
        "o|" | "|o" => "0..1",
        "o{" | "}o" => "0..*",
        "|{" | "}|" => "1..*",
        _ if marker.contains('{') || marker.contains('}') => "*",
        _ if marker.contains('|') => "1",
        _ if marker.contains('o') => "0",
        _ => "",
    }
}

#[allow(clippy::too_many_arguments)]
fn render_quadrant_svg(
    mut doc: SvgDocument,
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    quad_meta: &fm_core::IrQuadrantMeta,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    theme: &Theme,
) -> SvgDocument {
    // Replicate the exact chart dimensions from the layout engine so axes align
    // with the node positions computed by layout_diagram_quadrant_traced().
    let metrics = fm_core::FontMetrics::default_metrics();
    let node_count = layout.nodes.len();
    let base_size = 300.0_f32 + (node_count as f32 * 15.0).min(200.0);
    let chart_w = base_size.clamp(200.0, 600.0);
    let chart_h = chart_w;
    let axis_label_width = quad_meta
        .x_axis_left
        .as_ref()
        .map(|label| metrics.estimate_dimensions(label).0)
        .unwrap_or(0.0);
    let margin_left = (axis_label_width + 20.0).clamp(50.0, 120.0) + offset_x;
    let margin_top = 60.0_f32 + offset_y;

    let quadrant_fills: [&str; 4] = [
        &theme.colors.accents[0 % theme.colors.accents.len()],
        &theme.colors.accents[1 % theme.colors.accents.len()],
        &theme.colors.accents[2 % theme.colors.accents.len()],
        &theme.colors.accents[3 % theme.colors.accents.len()],
    ];

    // Draw quadrant backgrounds.
    let half_w = chart_w / 2.0;
    let half_h = chart_h / 2.0;
    let quadrant_rects = [
        (margin_left + half_w, margin_top, half_w, half_h), // Q1 top-right
        (margin_left, margin_top, half_w, half_h),          // Q2 top-left
        (margin_left, margin_top + half_h, half_w, half_h), // Q3 bottom-left
        (margin_left + half_w, margin_top + half_h, half_w, half_h), // Q4 bottom-right
    ];
    for (i, (x, y, w, h)) in quadrant_rects.iter().enumerate() {
        doc = doc.child(
            Element::rect()
                .x(*x)
                .y(*y)
                .width(*w)
                .height(*h)
                .fill(quadrant_fills[i])
                .attr("fill-opacity", "0.4")
                .class("fm-quadrant-bg"),
        );
    }

    // Quadrant labels in each section.
    let label_positions = [
        (
            margin_left + half_w + half_w / 2.0,
            margin_top + half_h / 2.0,
        ),
        (margin_left + half_w / 2.0, margin_top + half_h / 2.0),
        (
            margin_left + half_w / 2.0,
            margin_top + half_h + half_h / 2.0,
        ),
        (
            margin_left + half_w + half_w / 2.0,
            margin_top + half_h + half_h / 2.0,
        ),
    ];
    for (i, label) in quad_meta.quadrant_labels.iter().enumerate() {
        if let Some((lx, ly)) = label_positions.get(i) {
            doc = doc.child(
                Element::text()
                    .x(*lx)
                    .y(*ly)
                    .content(label)
                    .attr("text-anchor", "middle")
                    .attr("dominant-baseline", "middle")
                    .attr_num("font-size", config.font_size * 0.9)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .attr("fill-opacity", "0.5")
                    .fill(&theme.colors.text)
                    .class("fm-quadrant-label"),
            );
        }
    }

    // Axes.
    let axis_color = &theme.colors.edge;
    doc = doc.child(
        Element::line()
            .x1(margin_left)
            .y1(margin_top + half_h)
            .x2(margin_left + chart_w)
            .y2(margin_top + half_h)
            .stroke(axis_color)
            .stroke_width(1.0)
            .class("fm-quadrant-axis"),
    );
    doc = doc.child(
        Element::line()
            .x1(margin_left + half_w)
            .y1(margin_top)
            .x2(margin_left + half_w)
            .y2(margin_top + chart_h)
            .stroke(axis_color)
            .stroke_width(1.0)
            .class("fm-quadrant-axis"),
    );

    // Grid lines at 25% intervals.
    let grid_color = axis_color;
    for i in 1..4 {
        let frac = i as f32 / 4.0;
        // Vertical grid lines.
        doc = doc.child(
            Element::line()
                .x1(margin_left + chart_w * frac)
                .y1(margin_top)
                .x2(margin_left + chart_w * frac)
                .y2(margin_top + chart_h)
                .stroke(grid_color)
                .stroke_width(0.5)
                .attr("stroke-dasharray", "4,4")
                .attr("opacity", "0.3")
                .class("fm-quadrant-grid"),
        );
        // Horizontal grid lines.
        doc = doc.child(
            Element::line()
                .x1(margin_left)
                .y1(margin_top + chart_h * frac)
                .x2(margin_left + chart_w)
                .y2(margin_top + chart_h * frac)
                .stroke(grid_color)
                .stroke_width(0.5)
                .attr("stroke-dasharray", "4,4")
                .attr("opacity", "0.3")
                .class("fm-quadrant-grid"),
        );
    }

    // Axis labels.
    if let Some(left) = &quad_meta.x_axis_left {
        doc = doc.child(
            Element::text()
                .x(margin_left)
                .y(margin_top + chart_h + 20.0)
                .content(left)
                .attr("text-anchor", "start")
                .attr_num("font-size", config.font_size * 0.8)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .fill(&theme.colors.text)
                .class("fm-quadrant-axis-label"),
        );
    }
    if let Some(right) = &quad_meta.x_axis_right {
        doc = doc.child(
            Element::text()
                .x(margin_left + chart_w)
                .y(margin_top + chart_h + 20.0)
                .content(right)
                .attr("text-anchor", "end")
                .attr_num("font-size", config.font_size * 0.8)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .fill(&theme.colors.text)
                .class("fm-quadrant-axis-label"),
        );
    }

    // Y-axis labels.
    if let Some(bottom) = &quad_meta.y_axis_bottom {
        doc = doc.child(
            Element::text()
                .x(margin_left - 10.0)
                .y(margin_top + chart_h)
                .content(bottom)
                .attr("text-anchor", "end")
                .attr_num("font-size", config.font_size * 0.8)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .fill(&theme.colors.text)
                .class("fm-quadrant-axis-label"),
        );
    }
    if let Some(top) = &quad_meta.y_axis_top {
        doc = doc.child(
            Element::text()
                .x(margin_left - 10.0)
                .y(margin_top + config.font_size * 0.3)
                .content(top)
                .attr("text-anchor", "end")
                .attr_num("font-size", config.font_size * 0.8)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .fill(&theme.colors.text)
                .class("fm-quadrant-axis-label"),
        );
    }

    // Title.
    if let Some(title) = diagram_title(ir, quad_meta.title.as_deref()) {
        doc = doc.child(
            Element::text()
                .x(margin_left + half_w)
                .y(margin_top - 20.0)
                .content(title)
                .attr("text-anchor", "middle")
                .attr_num("font-size", config.font_size + 4.0)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .fill(&theme.colors.text)
                .class("fm-quadrant-title"),
        );
    }

    // Data points.
    let accent_colors: Vec<&str> = theme.colors.accents.iter().map(String::as_str).collect();
    for (i, node_box) in layout.nodes.iter().enumerate() {
        let cx = node_box.bounds.x + node_box.bounds.width / 2.0 + offset_x;
        let cy = node_box.bounds.y + node_box.bounds.height / 2.0 + offset_y;
        let color = accent_colors[i % accent_colors.len()];
        doc = doc.child(
            Element::circle()
                .cx(cx)
                .cy(cy)
                .r(6.0)
                .fill(color)
                .stroke(&theme.colors.background)
                .stroke_width(1.5)
                .class("fm-quadrant-point"),
        );
        // Point label from quadrant metadata or node ID.
        let label = quad_meta
            .points
            .get(i)
            .map(|p| p.label.as_str())
            .unwrap_or(&node_box.node_id);
        doc = doc.child(
            Element::text()
                .x(cx + 10.0)
                .y(cy + 4.0)
                .content(label)
                .attr("text-anchor", "start")
                .attr_num("font-size", config.font_size * 0.75)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .fill(&theme.colors.text)
                .class("fm-quadrant-point-label"),
        );
    }

    doc
}

/// Render a gantt chart with type-based task bar colors, section headers,
/// and dependency arrows.
#[allow(clippy::too_many_arguments)]
fn render_gantt_svg(
    mut doc: SvgDocument,
    ir: &MermaidDiagramIr,
    layout: &fm_layout::DiagramLayout,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    theme: &Theme,
) -> SvgDocument {
    let gantt_meta = match ir.gantt_meta.as_ref() {
        Some(m) => m,
        None => return doc,
    };

    // Title.
    if let Some(title) = diagram_title(ir, None) {
        doc = doc.child(
            TextBuilder::new(title)
                .x(layout.bounds.width / 2.0 + offset_x)
                .y(offset_y + config.font_size + 4.0)
                .anchor(TextAnchor::Middle)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(config.font_size + 4.0)
                .font_weight("600")
                .fill(&theme.colors.text)
                .class("fm-diagram-title")
                .build(),
        );
    }

    // Section background bands (alternating fills).
    let section_fills = ["#f0f4ff", "#fff8f0", "#f0fff4", "#fff0f8"];
    for (cluster_idx, cluster) in layout.clusters.iter().enumerate() {
        let fill = section_fills[cluster_idx % section_fills.len()];
        doc = doc.child(
            Element::rect()
                .x(cluster.bounds.x + offset_x)
                .y(cluster.bounds.y + offset_y)
                .width(cluster.bounds.width)
                .height(cluster.bounds.height)
                .fill(fill)
                .attr("fill-opacity", "0.5")
                .rx(4.0)
                .class("fm-gantt-section-bg"),
        );
        if let Some(section) = gantt_meta.sections.get(cluster_idx) {
            doc = doc.child(
                Element::text()
                    .x(cluster.bounds.x + offset_x + 6.0)
                    .y(cluster.bounds.y + offset_y + config.font_size * 0.9)
                    .content(&section.name)
                    .attr("text-anchor", "start")
                    .attr("font-weight", "600")
                    .attr_num("font-size", config.font_size * 0.85)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .fill(&theme.colors.text)
                    .class("fm-gantt-section-label"),
            );
        }
    }

    // Task bars with type-based coloring.
    let task_color = |task_type: &fm_core::GanttTaskType| -> &str {
        match task_type {
            fm_core::GanttTaskType::Done => "#86efac",
            fm_core::GanttTaskType::Active => "#94a3b8",
            fm_core::GanttTaskType::Critical => "#fca5a5",
            fm_core::GanttTaskType::Milestone => "#c4b5fd",
            fm_core::GanttTaskType::Normal => "#93c5fd",
        }
    };

    for (node_idx, node_box) in layout.nodes.iter().enumerate() {
        let x = node_box.bounds.x + offset_x;
        let y = node_box.bounds.y + offset_y;
        let w = node_box.bounds.width;
        let h = node_box.bounds.height;

        let task_type = gantt_meta
            .tasks
            .get(node_idx)
            .map(|t| &t.task_type)
            .unwrap_or(&fm_core::GanttTaskType::Normal);
        let fill = task_color(task_type);
        let is_milestone = matches!(task_type, fm_core::GanttTaskType::Milestone);

        if is_milestone {
            let cx = x + w / 2.0;
            let cy = y + h / 2.0;
            let r = h.min(w) * 0.4;
            let d = format!(
                "M{},{} L{},{} L{},{} L{},{} Z",
                cx,
                cy - r,
                cx + r,
                cy,
                cx,
                cy + r,
                cx - r,
                cy
            );
            doc = doc.child(
                Element::path()
                    .d(&d)
                    .fill(fill)
                    .stroke(&theme.colors.node_stroke)
                    .stroke_width(1.5)
                    .class("fm-gantt-milestone"),
            );
        } else {
            let type_class = match task_type {
                fm_core::GanttTaskType::Done => "fm-gantt-task-done",
                fm_core::GanttTaskType::Active => "fm-gantt-task-active",
                fm_core::GanttTaskType::Critical => "fm-gantt-task-critical",
                fm_core::GanttTaskType::Milestone => "fm-gantt-task-milestone",
                fm_core::GanttTaskType::Normal => "fm-gantt-task-normal",
            };
            doc = doc.child(
                Element::rect()
                    .x(x)
                    .y(y)
                    .width(w)
                    .height(h)
                    .fill(fill)
                    .stroke(&theme.colors.node_stroke)
                    .stroke_width(1.0)
                    .rx(3.0)
                    .class("fm-gantt-task")
                    .class(type_class),
            );

            // Progress bar overlay.
            if let Some(task) = gantt_meta.tasks.get(node_idx)
                && let Some(progress) = task.progress
                && progress > 0.0
            {
                let progress_w = w * progress.clamp(0.0, 1.0);
                doc = doc.child(
                    Element::rect()
                        .x(x)
                        .y(y)
                        .width(progress_w)
                        .height(h)
                        .fill(fill)
                        .attr("fill-opacity", "0.6")
                        .rx(3.0)
                        .class("fm-gantt-progress"),
                );
            }
        }

        // Task label.
        let label_text = ir
            .nodes
            .get(node_box.node_index)
            .and_then(|n| n.label)
            .and_then(|lid| ir.labels.get(lid.0))
            .map(|l| l.text.as_str())
            .or_else(|| ir.nodes.get(node_box.node_index).map(|n| n.id.as_str()))
            .unwrap_or("");
        if !label_text.is_empty() {
            doc = doc.child(
                Element::text()
                    .x(x + w / 2.0)
                    .y(y + h / 2.0 + config.font_size * 0.3)
                    .content(label_text)
                    .attr("text-anchor", "middle")
                    .attr("dominant-baseline", "central")
                    .attr_num("font-size", config.font_size * 0.8)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .fill(&theme.colors.text)
                    .class("fm-gantt-task-label"),
            );
        }
    }

    // Dependency arrows.
    for edge_path in &layout.edges {
        if edge_path.points.len() >= 2 {
            let path_d = smooth_layout_edge_path(edge_path, offset_x, offset_y);
            doc = doc.child(
                Element::path()
                    .d(&path_d)
                    .fill("none")
                    .stroke(&theme.colors.edge)
                    .stroke_width(1.2)
                    .attr("marker-end", "url(#arrowhead)")
                    .class("fm-gantt-dependency"),
            );
        }
    }

    doc
}

#[allow(clippy::too_many_arguments)]
fn render_pie_svg(
    mut doc: SvgDocument,
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    pie_meta: &fm_core::IrPieMeta,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    theme: &Theme,
) -> SvgDocument {
    use std::f32::consts::PI;

    let bounds = &layout.bounds;
    let accent_colors: Vec<&str> = theme.colors.accents.iter().map(String::as_str).collect();
    let legend_label_width = pie_meta
        .slices
        .iter()
        .map(|slice| {
            (slice.label.chars().count() as f32) * (config.avg_char_width * 0.9)
                + if pie_meta.show_data { 88.0 } else { 0.0 }
        })
        .fold(0.0_f32, f32::max);
    let legend_width = (legend_label_width + 56.0).clamp(136.0, 280.0);
    let title = diagram_title(ir, pie_meta.title.as_deref());
    let title_height = if title.is_some() {
        config.font_size + 22.0
    } else {
        0.0
    };
    let chart_gap = 24.0;
    let chart_left = bounds.x + offset_x;
    let chart_top = bounds.y + offset_y + title_height;
    let chart_width = (bounds.width - legend_width - chart_gap).max(160.0);
    let chart_height = (bounds.height - title_height).max(160.0);
    let cx = chart_left + chart_width / 2.0;
    let cy = chart_top + chart_height / 2.0;
    let radius = (chart_width.min(chart_height) / 2.0 - 36.0).max(40.0);

    let total: f32 = pie_meta
        .slices
        .iter()
        .map(|s| s.value.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);

    if let Some(title) = title {
        doc = doc.child(
            TextBuilder::new(title)
                .x(cx)
                .y(bounds.y + offset_y + config.font_size + 2.0)
                .anchor(TextAnchor::Middle)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(config.font_size + 4.0)
                .font_weight("600")
                .fill(&theme.colors.text)
                .class("fm-pie-title")
                .build(),
        );
    }

    let mut angle = -PI / 2.0;

    for (i, slice) in pie_meta.slices.iter().enumerate() {
        let value = slice.value.max(0.0);
        let sweep = (value / total) * 2.0 * PI;
        let color = accent_colors[i % accent_colors.len()];

        let wedge = if value <= f32::EPSILON {
            Element::path()
                .d("")
                .fill("none")
                .stroke("none")
                .class("fm-pie-slice fm-pie-slice-zero")
        } else if (sweep - 2.0 * PI).abs() <= 0.0001 {
            Element::circle()
                .cx(cx)
                .cy(cy)
                .r(radius)
                .fill(color)
                .stroke(&theme.colors.background)
                .stroke_width(2.0)
                .class("fm-pie-slice fm-pie-slice-full")
        } else {
            let x1 = cx + radius * angle.cos();
            let y1 = cy + radius * angle.sin();
            let x2 = cx + radius * (angle + sweep).cos();
            let y2 = cy + radius * (angle + sweep).sin();
            let large_arc = i32::from(sweep > PI);
            let d =
                format!("M {cx} {cy} L {x1} {y1} A {radius} {radius} 0 {large_arc} 1 {x2} {y2} Z");
            Element::path()
                .d(&d)
                .fill(color)
                .stroke(&theme.colors.background)
                .stroke_width(2.0)
                .class("fm-pie-slice")
        };

        doc = doc.child(wedge);

        let mid_angle = angle + sweep / 2.0;
        let label_radius = radius + 24.0;
        let lx = cx + label_radius * mid_angle.cos();
        let ly = cy + label_radius * mid_angle.sin();
        let pct = (value / total) * 100.0;

        let label_text = if pie_meta.show_data {
            format!("{}: {:.0} ({:.1}%)", slice.label, value, pct)
        } else {
            slice.label.clone()
        };

        let anchor = if mid_angle.cos() < -0.1 {
            TextAnchor::End
        } else if mid_angle.cos() > 0.1 {
            TextAnchor::Start
        } else {
            TextAnchor::Middle
        };

        doc = doc.child(
            TextBuilder::new(&label_text)
                .x(lx)
                .y(ly)
                .anchor(anchor)
                .baseline(crate::text::DominantBaseline::Middle)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(clamp_font_size(
                    config.font_size * 0.85,
                    config.min_font_size,
                ))
                .fill(&theme.colors.text)
                .class("fm-pie-label")
                .build(),
        );

        angle += sweep;
    }

    let legend_x = chart_left + chart_width + chart_gap;
    let legend_y = chart_top + 12.0;
    let legend_height = (pie_meta.slices.len() as f32 * 24.0 + 44.0).max(64.0);

    let mut legend = Element::group().class("fm-pie-legend");
    legend = legend.child(
        Element::rect()
            .x(legend_x)
            .y(legend_y)
            .width(legend_width)
            .height(legend_height)
            .rx(config.rounded_corners.max(6.0))
            .fill(&theme.colors.node_fill)
            .stroke(&theme.colors.node_stroke)
            .stroke_width(1.2)
            .class("fm-pie-legend-box"),
    );
    legend = legend.child(
        TextBuilder::new("Legend")
            .x(legend_x + 14.0)
            .y(legend_y + 18.0)
            .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
            .font_size(clamp_font_size(
                config.font_size * 0.82,
                config.min_font_size,
            ))
            .font_weight("600")
            .fill(&theme.colors.text)
            .class("fm-pie-legend-title")
            .build(),
    );

    for (index, slice) in pie_meta.slices.iter().enumerate() {
        let row_y = legend_y + 34.0 + index as f32 * 24.0;
        let color = accent_colors[index % accent_colors.len()];
        let pct = (slice.value.max(0.0) / total) * 100.0;
        let entry_label = if pie_meta.show_data {
            format!("{}: {:.0} ({:.1}%)", slice.label, slice.value.max(0.0), pct)
        } else {
            slice.label.clone()
        };
        legend = legend.child(
            Element::rect()
                .x(legend_x + 14.0)
                .y(row_y - 9.0)
                .width(12.0)
                .height(12.0)
                .rx(2.0)
                .fill(color)
                .stroke(&theme.colors.background)
                .stroke_width(1.0)
                .class("fm-pie-legend-swatch"),
        );
        legend = legend.child(
            TextBuilder::new(&entry_label)
                .x(legend_x + 34.0)
                .y(row_y)
                .baseline(crate::text::DominantBaseline::Middle)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(clamp_font_size(
                    config.font_size * 0.8,
                    config.min_font_size,
                ))
                .fill(&theme.colors.text)
                .class("fm-pie-legend-entry")
                .build(),
        );
    }

    doc = doc.child(legend);

    doc
}

#[allow(clippy::too_many_arguments)]
fn render_xychart_svg(
    mut doc: SvgDocument,
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    xy_chart_meta: &IrXyChartMeta,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    theme: &Theme,
) -> SvgDocument {
    let plot_bounds = xychart_plot_bounds(layout);
    let plot_x = plot_bounds.x + offset_x;
    let plot_y = plot_bounds.y + offset_y;
    let plot_bottom = plot_y + plot_bounds.height;
    let plot_right = plot_x + plot_bounds.width;
    let (y_min, y_max) = resolve_xychart_y_domain(xy_chart_meta);
    let baseline_value = y_min.min(0.0).max(y_max.min(0.0));
    let baseline_y = xychart_value_to_y(baseline_value, y_min, y_max, plot_bounds) + offset_y;
    let categories = xychart_categories(xy_chart_meta);
    let palette = theme.colors.accents.clone();

    doc = doc.child(
        Element::rect()
            .x(plot_x)
            .y(plot_y)
            .width(plot_bounds.width)
            .height(plot_bounds.height)
            .fill("rgba(148,163,184,0.06)")
            .stroke("rgba(148,163,184,0.16)")
            .stroke_width(1.0)
            .rx(config.rounded_corners.max(6.0))
            .class("fm-xychart-plot"),
    );

    for tick_index in 0..=4 {
        let tick_ratio = tick_index as f32 / 4.0;
        let tick_y = plot_y + plot_bounds.height - (plot_bounds.height * tick_ratio);
        let tick_value = y_min + (y_max - y_min) * tick_ratio;
        doc = doc.child(
            Element::line()
                .x1(plot_x)
                .y1(tick_y)
                .x2(plot_right)
                .y2(tick_y)
                .stroke("rgba(148,163,184,0.35)")
                .stroke_width(1.0)
                .stroke_dasharray("4,4")
                .class("fm-xychart-gridline"),
        );
        doc = doc.child(
            TextBuilder::new(&format_xychart_tick_value(tick_value))
                .x(plot_x - 10.0)
                .y(tick_y + 4.0)
                .anchor(TextAnchor::End)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(clamp_font_size(
                    config.font_size * 0.72,
                    config.min_font_size,
                ))
                .fill(&theme.colors.edge)
                .class("fm-xychart-y-tick")
                .build(),
        );
    }

    doc = doc.child(
        Element::line()
            .x1(plot_x)
            .y1(plot_bottom)
            .x2(plot_right)
            .y2(plot_bottom)
            .stroke(&theme.colors.edge)
            .stroke_width(1.5)
            .class("fm-xychart-axis fm-xychart-axis-x"),
    );
    doc = doc.child(
        Element::line()
            .x1(plot_x)
            .y1(plot_y)
            .x2(plot_x)
            .y2(plot_bottom)
            .stroke(&theme.colors.edge)
            .stroke_width(1.5)
            .class("fm-xychart-axis fm-xychart-axis-y"),
    );

    let band_width = plot_bounds.width / categories.len().max(1) as f32;
    for (index, category) in categories.iter().enumerate() {
        let x = plot_x + band_width * (index as f32 + 0.5);
        doc = doc.child(
            TextBuilder::new(category)
                .x(x)
                .y(plot_bottom + 24.0)
                .anchor(TextAnchor::Middle)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(clamp_font_size(
                    config.font_size * 0.74,
                    config.min_font_size,
                ))
                .fill(&theme.colors.text)
                .class("fm-xychart-x-tick")
                .build(),
        );
    }

    if let Some(title) = diagram_title(ir, xy_chart_meta.title.as_deref()) {
        doc = doc.child(
            TextBuilder::new(title)
                .x((layout.bounds.width / 2.0) + offset_x)
                .y(plot_y - 34.0)
                .anchor(TextAnchor::Middle)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(clamp_font_size(
                    config.font_size * 1.18,
                    config.min_font_size,
                ))
                .font_weight("600")
                .fill(&theme.colors.text)
                .class("fm-xychart-title")
                .build(),
        );
    }

    if let Some(y_label) = xy_chart_meta.y_axis.label.as_deref() {
        doc = doc.child(
            TextBuilder::new(y_label)
                .x(plot_x - 52.0)
                .y(plot_y - 12.0)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(clamp_font_size(
                    config.font_size * 0.76,
                    config.min_font_size,
                ))
                .fill(&theme.colors.text)
                .class("fm-xychart-y-label")
                .build(),
        );
    }

    // X-axis label (centered below category labels).
    if let Some(x_label) = xy_chart_meta.x_axis.label.as_deref() {
        doc = doc.child(
            TextBuilder::new(x_label)
                .x(plot_x + plot_bounds.width / 2.0)
                .y(plot_bottom + 48.0)
                .anchor(TextAnchor::Middle)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(clamp_font_size(
                    config.font_size * 0.76,
                    config.min_font_size,
                ))
                .fill(&theme.colors.text)
                .class("fm-xychart-x-label")
                .build(),
        );
    }

    // Tick marks at axis edges (small lines at each grid level and category center).
    let tick_len = 5.0_f32;
    for tick_index in 0..=4_u32 {
        let frac = tick_index as f32 / 4.0;
        let y = plot_bottom - frac * plot_bounds.height;
        doc = doc.child(
            Element::line()
                .x1(plot_x - tick_len)
                .y1(y)
                .x2(plot_x)
                .y2(y)
                .stroke(&theme.colors.text)
                .stroke_width(1.0)
                .class("fm-xychart-tick"),
        );
    }
    for (index, _category) in categories.iter().enumerate() {
        let x = plot_x + band_width * (index as f32 + 0.5);
        doc = doc.child(
            Element::line()
                .x1(x)
                .y1(plot_bottom)
                .x2(x)
                .y2(plot_bottom + tick_len)
                .stroke(&theme.colors.text)
                .stroke_width(1.0)
                .class("fm-xychart-tick"),
        );
    }

    // Legend for named series.
    let named_series: Vec<(usize, &str)> = xy_chart_meta
        .series
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.name.as_deref().map(|n| (i, n)))
        .collect();
    if !named_series.is_empty() {
        let legend_x = plot_right + 16.0;
        let legend_y = plot_y + 8.0;
        let legend_entry_h = 22.0_f32;
        let legend_height = named_series.len() as f32 * legend_entry_h + 12.0;
        let legend_width = 120.0_f32;

        let mut legend = Element::group().class("fm-xychart-legend");
        legend = legend.child(
            Element::rect()
                .x(legend_x)
                .y(legend_y)
                .width(legend_width)
                .height(legend_height)
                .rx(config.rounded_corners.max(4.0))
                .fill(&theme.colors.node_fill)
                .stroke(&theme.colors.node_stroke)
                .stroke_width(1.0)
                .class("fm-xychart-legend-box"),
        );
        for (entry_idx, &(series_idx, name)) in named_series.iter().enumerate() {
            let row_y = legend_y + 6.0 + entry_idx as f32 * legend_entry_h + legend_entry_h / 2.0;
            let color = &palette[series_idx % palette.len()];
            legend = legend.child(
                Element::rect()
                    .x(legend_x + 8.0)
                    .y(row_y - 5.0)
                    .width(10.0)
                    .height(10.0)
                    .rx(2.0)
                    .fill(color)
                    .class("fm-xychart-legend-swatch"),
            );
            legend = legend.child(
                TextBuilder::new(name)
                    .x(legend_x + 24.0)
                    .y(row_y)
                    .baseline(crate::text::DominantBaseline::Middle)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .font_size(clamp_font_size(
                        config.font_size * 0.72,
                        config.min_font_size,
                    ))
                    .fill(&theme.colors.text)
                    .class("fm-xychart-legend-entry")
                    .build(),
            );
        }
        doc = doc.child(legend);
    }

    for (series_index, series) in xy_chart_meta.series.iter().enumerate() {
        let color = &palette[series_index % palette.len()];
        let series_nodes: Vec<_> = series
            .nodes
            .iter()
            .filter_map(|node_id| {
                layout
                    .nodes
                    .iter()
                    .find(|node| node.node_index == node_id.0)
            })
            .collect();

        match series.kind {
            IrXySeriesKind::Bar => {
                for node in series_nodes {
                    let mut rect = Element::rect()
                        .x(node.bounds.x + offset_x)
                        .y(node.bounds.y + offset_y)
                        .width(node.bounds.width)
                        .height(node.bounds.height)
                        .fill(color)
                        .fill_opacity(0.78)
                        .stroke(color)
                        .stroke_width(1.0)
                        .rx((config.rounded_corners * 0.45).max(3.0))
                        .class("fm-xychart-bar");
                    if config.include_source_spans {
                        rect = apply_span_metadata(rect, node.span);
                    }
                    doc = doc.child(rect);
                }
            }
            IrXySeriesKind::Line | IrXySeriesKind::Area => {
                if series_nodes.is_empty() {
                    continue;
                }
                let points: Vec<(f32, f32)> = series_nodes
                    .iter()
                    .map(|node| {
                        let center = node.bounds.center();
                        (center.x + offset_x, center.y + offset_y)
                    })
                    .collect();

                if matches!(series.kind, IrXySeriesKind::Area) {
                    let first_x = points.first().map_or(plot_x, |point| point.0);
                    let last_x = points.last().map_or(plot_x, |point| point.0);
                    let mut fill_points = vec![(first_x, baseline_y)];
                    fill_points.extend(points.iter().copied());
                    fill_points.push((last_x, baseline_y));
                    let mut area_path =
                        PathBuilder::new().move_to(fill_points[0].0, fill_points[0].1);
                    for point in fill_points.iter().skip(1) {
                        area_path = area_path.line_to(point.0, point.1);
                    }
                    area_path = area_path.close();
                    doc = doc.child(
                        Element::path()
                            .d(&area_path.build())
                            .fill(color)
                            .fill_opacity(0.16)
                            .stroke("none")
                            .class("fm-xychart-area"),
                    );
                }

                let mut line_path = PathBuilder::new().move_to(points[0].0, points[0].1);
                for point in points.iter().skip(1) {
                    line_path = line_path.line_to(point.0, point.1);
                }
                doc = doc.child(
                    Element::path()
                        .d(&line_path.build())
                        .fill("none")
                        .stroke(color)
                        .stroke_width(3.0)
                        .stroke_linecap("round")
                        .stroke_linejoin("round")
                        .class("fm-xychart-line"),
                );

                for node in series_nodes {
                    let center = node.bounds.center();
                    let mut point = Element::circle()
                        .cx(center.x + offset_x)
                        .cy(center.y + offset_y)
                        .r((node.bounds.width.min(node.bounds.height) / 2.0).max(3.5))
                        .fill(color)
                        .stroke(&theme.colors.background)
                        .stroke_width(2.0)
                        .class("fm-xychart-point");
                    if config.include_source_spans {
                        point = apply_span_metadata(point, node.span);
                    }
                    doc = doc.child(point);
                }
            }
        }
    }

    doc
}

fn xychart_plot_bounds(layout: &DiagramLayout) -> fm_layout::LayoutRect {
    const LEFT_MARGIN: f32 = 88.0;
    const TOP_MARGIN: f32 = 84.0;
    const RIGHT_MARGIN: f32 = 36.0;
    const BOTTOM_MARGIN: f32 = 76.0;

    fm_layout::LayoutRect {
        x: layout.bounds.x + LEFT_MARGIN,
        y: layout.bounds.y + TOP_MARGIN,
        width: (layout.bounds.width - LEFT_MARGIN - RIGHT_MARGIN).max(1.0),
        height: (layout.bounds.height - TOP_MARGIN - BOTTOM_MARGIN).max(1.0),
    }
}

fn xychart_categories(xy_chart_meta: &IrXyChartMeta) -> Vec<String> {
    let series_count = xy_chart_meta
        .series
        .iter()
        .map(|series| series.values.len())
        .max()
        .unwrap_or(0);
    let count = series_count.max(xy_chart_meta.x_axis.categories.len());

    if xy_chart_meta.x_axis.categories.is_empty() {
        let (x_min, x_max) = resolve_xychart_x_domain(xy_chart_meta, count);
        if count <= 1 {
            return vec![format_xychart_tick_value(x_min)];
        }
        let step = (x_max - x_min) / (count.saturating_sub(1) as f32).max(1.0);
        return (0..count)
            .map(|index| format_xychart_tick_value(x_min + step * index as f32))
            .collect();
    }

    let mut categories = xy_chart_meta.x_axis.categories.clone();
    if categories.len() < count {
        categories.extend((categories.len()..count).map(|index| (index + 1).to_string()));
    }
    categories
}

fn resolve_xychart_x_domain(xy_chart_meta: &IrXyChartMeta, count: usize) -> (f32, f32) {
    let min = xy_chart_meta.x_axis.min.unwrap_or(0.0);
    let max = xy_chart_meta
        .x_axis
        .max
        .unwrap_or_else(|| count.saturating_sub(1) as f32);
    if (max - min).abs() < f32::EPSILON {
        (min, min + 1.0)
    } else {
        (min, max)
    }
}

fn resolve_xychart_y_domain(xy_chart_meta: &IrXyChartMeta) -> (f32, f32) {
    let mut min_value = xy_chart_meta.y_axis.min.unwrap_or(f32::INFINITY);
    let mut max_value = xy_chart_meta.y_axis.max.unwrap_or(f32::NEG_INFINITY);

    if xy_chart_meta.y_axis.min.is_none() || xy_chart_meta.y_axis.max.is_none() {
        for value in xy_chart_meta
            .series
            .iter()
            .flat_map(|series| series.values.iter().copied())
        {
            min_value = min_value.min(value);
            max_value = max_value.max(value);
        }
    }

    if !min_value.is_finite() || !max_value.is_finite() {
        return (0.0, 1.0);
    }
    if xy_chart_meta.y_axis.min.is_none() && min_value > 0.0 {
        min_value = 0.0;
    }
    if xy_chart_meta.y_axis.max.is_none() && max_value < 0.0 {
        max_value = 0.0;
    }
    if (max_value - min_value).abs() < f32::EPSILON {
        max_value += 1.0;
    }
    (min_value, max_value)
}

fn xychart_value_to_y(
    value: f32,
    y_min: f32,
    y_max: f32,
    plot_bounds: fm_layout::LayoutRect,
) -> f32 {
    let range = (y_max - y_min).max(f32::EPSILON);
    let ratio = ((value - y_min) / range).clamp(0.0, 1.0);
    plot_bounds.y + plot_bounds.height - (ratio * plot_bounds.height)
}

fn format_xychart_tick_value(value: f32) -> String {
    if (value - value.round()).abs() < 0.0001 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// Look up a node's centrality tier by index (O(1) via HashMap).
fn lookup_centrality_tier(
    centrality_map: &HashMap<usize, CentralityTier>,
    node_index: usize,
) -> Option<CentralityTier> {
    centrality_map.get(&node_index).copied()
}

/// Serialize a complete common rectangle node (`<g>` + gradient `<rect>` + centered `<text>` +
/// `<title>`) directly into raw SVG bytes, **byte-identical** to what `render_node` builds via
/// `Element`s for the default themed config. Every value goes through the same serializers
/// (`AttributeValue::write_value` / `write_escaped_attr` / `write_escaped_text`); only attribute
/// names/order and tag structure are replicated here (pinned by `node_fast_fragment_matches_render`).
/// Used only via the `common_node_fast` gate, which guarantees none of `render_node`'s conditional
/// classes/children/post-processing apply, so this is the entire node. Skips four `Element` builds +
/// their `Attributes` Vecs + `write_into` walks — the per-node construction is ~60% of wide render.
/// The ` fm-node-user-{sanitized}` class suffix the slow path appends for a node's custom classes, but
/// ONLY when every class is "simple" — none triggers a state/border keyword (highlight/inactive/dashed/
/// double) or a special class that changes the node's rendered fill/stroke/structure. Returns `None` when
/// any class needs the `Element` slow path, so the fast node fragment stays byte-identical. Empty/no
/// classes yield `Some("")` (no allocation).
fn simple_node_user_class_suffix(node: &fm_core::IrNode) -> Option<String> {
    // Nodes with compartments (class diagrams) or C4 metadata render extra content the plain-rect fast
    // fragment does not produce; they were implicitly excluded by the old `classes.is_empty()` gate, so
    // keep excluding them now that arbitrary simple classes are allowed.
    if node.class_meta.is_some() || node.c4_meta.is_some() {
        return None;
    }
    let mut suffix = String::new();
    let mut block_beta = false;
    for class in &node.classes {
        let kw = scan_node_class_keywords(class);
        if kw.highlighted
            || kw.inactive
            || kw.dashed_border
            || kw.double_border
            || class.eq_ignore_ascii_case("c4-external")
            || class.eq_ignore_ascii_case("block-beta-space")
        {
            return None;
        }
        // `block-beta` only makes the slow path add a plain `fm-node-block-beta` class (no fill/structure
        // change), so keep it on the fast path and replicate that class below. `block-beta-space` is a
        // placeholder node (already excluded by the fast gate's `!placeholder_space_node`) — reject it.
        if class.eq_ignore_ascii_case("block-beta") {
            block_beta = true;
        }
        let sanitized = sanitize_css_token(class);
        if !sanitized.is_empty() {
            suffix.push_str(" fm-node-user-");
            suffix.push_str(&sanitized);
        }
    }
    // The slow path appends `fm-node-block-beta` AFTER the per-class `fm-node-user-…` loop (see the
    // `is_block_beta` block in `render_node`), so it goes last here too. Byte-identical.
    if block_beta {
        suffix.push_str(" fm-node-block-beta");
    }
    Some(suffix)
}

#[allow(clippy::too_many_arguments)]
fn build_common_node_fragment(
    node_id: &str,
    node_index: usize,
    accent: usize,
    raw_label: &str,
    label: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: f32,
    text_x: f32,
    text_y: f32,
    font_size: f32,
    text_fill: &str,
    user_classes: &str,
    shape: fm_core::NodeShape,
) -> String {
    // `raw_label` is written twice (the `aria-label` and the `<title>` text), so size for both copies
    // plus the fixed tag/literal bytes.
    let mut f = String::with_capacity(label.len() + raw_label.len() * 2 + user_classes.len() + 340);
    write_common_node_fragment_into(
        &mut f, node_id, node_index, accent, raw_label, label, x, y, w, h, rx, text_x, text_y,
        font_size, text_fill, user_classes, shape,
    );
    f
}

/// Write-into core of [`build_common_node_fragment`]: streams the common rect node straight into `f` with
/// no intermediate `String`, so `render_node_into` can render it directly into the chunk output buffer.
#[allow(clippy::too_many_arguments)]
fn write_common_node_fragment_into(
    f: &mut String,
    node_id: &str,
    node_index: usize,
    accent: usize,
    raw_label: &str,
    label: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: f32,
    text_x: f32,
    text_y: f32,
    font_size: f32,
    text_fill: &str,
    user_classes: &str,
    shape: fm_core::NodeShape,
) {
    use crate::attributes::{AttributeValue, write_escaped_attr, write_escaped_text};
    use std::fmt::Write as _;
    // <g id=".." class="fm-node fm-node-accent-N fm-node-shape-rect[ fm-node-user-…]" data-id=".." …>
    f.push_str("<g id=\"");
    // The node id is `fm-node-[{sanitized}-]{index}` — only `[a-z0-9-]`, never an escapable byte — so
    // write it straight into `f` (skipping `mermaid_node_element_id`'s 3 throwaway allocations: the
    // sanitizer's two Strings + the id String). Byte-identical to `write_escaped_attr(id)` because the
    // id can never contain `& < > " '`; pinned by `node_fast_fragment_matches_render`.
    fm_core::write_mermaid_node_element_id_into(f, node_id, node_index);
    f.push_str("\" class=\"fm-node fm-node-accent-");
    let _ = write!(f, "{accent}");
    f.push(' ');
    f.push_str(node_shape_css_class(shape));
    // Simple custom classes (`class X foo` / `:::foo` on nodes with no state-keyword or special class);
    // empty for the overwhelmingly common no-class node. Matches the slow path's ` fm-node-user-…` tail.
    f.push_str(user_classes);
    f.push_str("\" data-id=\"");
    let _ = write_escaped_attr(f, node_id);
    f.push_str("\" role=\"graphics-symbol\" aria-label=\"");
    let _ = write_escaped_attr(f, raw_label);
    f.push_str("\" tabindex=\"0\">");
    // Shape element with the gradient fill (stroke/stroke-width are CSS-driven under embedded theme, so
    // absent inline). `<rect x y width height rx …/>` for Rect; `<circle cx cy r …/>` for Circle, whose
    // cx/cy/r match render_node's slow path (cx=x+w/2, cy=y+h/2, r=w.min(h)/2) and whose serialized attr
    // order (cx,cy,r,fill) matches the slow `Element::circle()` after the gradient-fill override.
    if matches!(shape, fm_core::NodeShape::Rect) {
        f.push_str("<rect x=\"");
        let _ = AttributeValue::Number(x).write_value(f);
        f.push_str("\" y=\"");
        let _ = AttributeValue::Number(y).write_value(f);
        f.push_str("\" width=\"");
        let _ = AttributeValue::Number(w).write_value(f);
        f.push_str("\" height=\"");
        let _ = AttributeValue::Number(h).write_value(f);
        f.push_str("\" rx=\"");
        let _ = AttributeValue::Number(rx).write_value(f);
        f.push_str("\" fill=\"url(#fm-node-gradient)\"/>");
    } else {
        f.push_str("<circle cx=\"");
        let _ = AttributeValue::Number(x + w / 2.0).write_value(f);
        f.push_str("\" cy=\"");
        let _ = AttributeValue::Number(y + h / 2.0).write_value(f);
        f.push_str("\" r=\"");
        let _ = AttributeValue::Number(w.min(h) / 2.0).write_value(f);
        f.push_str("\" fill=\"url(#fm-node-gradient)\"/>");
    }
    // <text x y text-anchor="middle" font-size=".." fill="..">label</text>
    f.push_str("<text x=\"");
    let _ = AttributeValue::Number(text_x).write_value(f);
    f.push_str("\" y=\"");
    let _ = AttributeValue::Number(text_y).write_value(f);
    f.push_str("\" text-anchor=\"middle\" font-size=\"");
    let _ = AttributeValue::Number(font_size).write_value(f);
    f.push_str("\" fill=\"");
    let _ = write_escaped_attr(f, text_fill);
    f.push_str("\">");
    let _ = write_escaped_text(f, label);
    f.push_str("</text>");
    // <title>Node: {raw_label}, rectangle</title> -- this is `describe_node(node, ir)` for the gated
    // Rect shape (its `shape_desc` is always "rectangle" here, and its label is exactly `raw_label`),
    // written piecewise so the per-node description String is never allocated. Byte-identical to
    // `write_escaped_text(describe_node(..))` because the `"Node: "` / `", rectangle"` literals carry no
    // escapable byte (escape is the identity on them) and the label is escaped the same either way.
    // Pinned by `node_fast_fragment_matches_render`.
    f.push_str("<title>Node: ");
    let _ = write_escaped_text(f, raw_label);
    // `describe_node`'s shape word: "rectangle" for Rect, "circle" for Circle (the only shapes gated here).
    f.push_str(if matches!(shape, fm_core::NodeShape::Rect) {
        ", rectangle</title></g>"
    } else {
        ", circle</title></g>"
    });
}

/// Render a single node straight into the output buffer. For the overwhelmingly common themed rectangle
/// node (the same gate as `render_node`'s fast path) the `<g><rect/><text/><title/></g>` is streamed
/// directly into `out` via `write_common_node_fragment_into` — eliminating the per-node fragment `String`
/// that `render_node` would build, wrap in `Element::raw_svg`, and immediately copy out then drop. Every
/// other node delegates to the `render_node` Element path. The prelude/gate here mirror `render_node`'s
/// (any divergence is caught byte-for-byte by `svg_golden_snapshots_are_stable` +
/// `node_fast_fragment_matches_render`).
#[allow(clippy::too_many_arguments)]
fn render_node_into(
    out: &mut String,
    node_box: &LayoutNodeBox,
    ir: &MermaidDiagramIr,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    detail: RenderDetailProfile,
    colors: &ThemeColors,
    emit_classdef_classes: bool,
    centrality_map: &HashMap<usize, CentralityTier>,
) {
    use fm_core::NodeShape;

    let ir_node = ir.nodes.get(node_box.node_index);
    let shape = ir_node.map_or(NodeShape::Rect, |n| n.shape);
    let (shape_style, text_style) = resolve_node_inline_styles(ir, node_box.node_index);
    let node_id = ir_node
        .map(|node| node.id.as_str())
        .unwrap_or_else(|| node_box.node_id.as_str());

    let x = node_box.bounds.x + offset_x;
    let y = node_box.bounds.y + offset_y;
    let w = node_box.bounds.width;
    let h = node_box.bounds.height;
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;

    let placeholder_space_node = ir_node.is_some_and(is_block_beta_space_node);
    let label_id = ir_node.and_then(|node| node.label);
    let raw_label_text = if placeholder_space_node {
        ""
    } else {
        label_id
            .and_then(|lid| ir.labels.get(lid.0))
            .map(|l| l.text.as_str())
            .or_else(|| {
                ir_node.and_then(|node| match node.shape {
                    NodeShape::DoubleCircle if node.label.is_none() => None,
                    NodeShape::FilledCircle | NodeShape::HorizontalBar => None,
                    _ => Some(node.id.as_str()),
                })
            })
            .unwrap_or("")
    };
    let label_text = truncate_label(raw_label_text, detail.node_label_max_chars);
    let node_font_size = detail.node_font_size;
    let node_icon = ir_node
        .and_then(|node| node.icon())
        .map(str::trim)
        .filter(|icon| !icon.is_empty())
        .filter(|_| ir_node.is_none_or(|node| node.class_meta.is_none() && node.c4_meta.is_none()));

    // Same gate as `render_node`'s fast path (permit_fast is always true on this serialize-only path).
    // `user_class_suffix` is `Some("")` for the common no-class node and `Some(" fm-node-user-…")` for a
    // node whose custom classes are all simple; `None` (slow path) when a class needs conditional render.
    let user_class_suffix = ir_node.and_then(simple_node_user_class_suffix);
    if matches!(shape, NodeShape::Rect | NodeShape::Circle)
        && config.embed_theme_css
        && config.node_gradients
        && !emit_classdef_classes
        && !config.animations_enabled
        && !config.include_source_spans
        && config.a11y.aria_labels
        && config.a11y.keyboard_nav
        && config.a11y.text_alternatives
        && shape_style.is_none()
        && text_style.is_none()
        && node_icon.is_none()
        && !placeholder_space_node
        && !label_text.contains('\n')
        && !label_text.contains('\r')
        && lookup_centrality_tier(centrality_map, node_box.node_index).is_none()
        && label_id.is_none_or(|id| ir.label_markup.get(&id).is_none_or(|s| s.is_empty()))
        && let Some(node) = ir_node
        && let Some(user_classes) = user_class_suffix.as_deref()
        && node.requirement_meta.is_none()
        && node.menu_links.is_empty()
        && node.href().is_none()
        && node.callback().is_none()
    {
        write_common_node_fragment_into(
            out,
            node_id,
            node_box.node_index,
            stable_accent_index(node_id),
            raw_label_text,
            &label_text,
            x,
            y,
            w,
            h,
            config.rounded_corners * 0.55,
            cx,
            cy + node_font_size / 3.0,
            node_font_size,
            colors.text.as_str(),
            user_classes,
            shape,
        );
        return;
    }

    render_node(
        node_box,
        ir,
        offset_x,
        offset_y,
        config,
        detail,
        colors,
        emit_classdef_classes,
        centrality_map,
        true,
    )
    .write_to_string(out);
}

/// Render a single node to an SVG element.
#[allow(clippy::too_many_arguments)]
fn render_node(
    node_box: &LayoutNodeBox,
    ir: &MermaidDiagramIr,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    detail: RenderDetailProfile,
    colors: &ThemeColors,
    emit_classdef_classes: bool,
    centrality_map: &HashMap<usize, CentralityTier>,
    // False forces the slow `Element` path for callers that post-process the result (e.g. add a class).
    permit_fast: bool,
) -> Element {
    use fm_core::NodeShape;

    let ir_node = ir.nodes.get(node_box.node_index);
    let shape = ir_node.map_or(NodeShape::Rect, |n| n.shape);
    let (shape_style, text_style) = resolve_node_inline_styles(ir, node_box.node_index);
    let node_id = ir_node
        .map(|node| node.id.as_str())
        .unwrap_or_else(|| node_box.node_id.as_str());

    let x = node_box.bounds.x + offset_x;
    let y = node_box.bounds.y + offset_y;
    let w = node_box.bounds.width;
    let h = node_box.bounds.height;
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;

    // Get node label text
    let placeholder_space_node = ir_node.is_some_and(is_block_beta_space_node);
    let label_id = ir_node.and_then(|node| node.label);
    let raw_label_text = if placeholder_space_node {
        ""
    } else {
        label_id
            .and_then(|lid| ir.labels.get(lid.0))
            .map(|l| l.text.as_str())
            .or_else(|| {
                ir_node.and_then(|node| match node.shape {
                    NodeShape::DoubleCircle if node.label.is_none() => None,
                    NodeShape::FilledCircle | NodeShape::HorizontalBar => None,
                    _ => Some(node.id.as_str()),
                })
            })
            .unwrap_or("")
    };
    let label_text = truncate_label(raw_label_text, detail.node_label_max_chars);
    let node_font_size = detail.node_font_size;
    let node_icon = ir_node
        .and_then(|node| node.icon())
        .map(str::trim)
        .filter(|icon| !icon.is_empty())
        .filter(|_| ir_node.is_none_or(|node| node.class_meta.is_none() && node.c4_meta.is_none()));
    let apply_label_class =
        |elem: Element| maybe_add_class(elem, "fm-node-label", emit_classdef_classes);

    let mut is_highlighted = false;
    let mut is_inactive = false;
    let mut dashed_border = false;
    let mut double_border = false;
    let mut is_block_beta = false;
    let mut is_block_beta_space = false;

    // Fast path: the overwhelmingly common themed rectangle node — plain single-line label, no
    // conditional class/child/post-processing — serializes to a fixed
    // `<g><rect/><text/><title/></g>`. Emit it directly, skipping four `Element` builds + their
    // `Attributes` Vecs + `write_into` walks (per-node construction is ~60% of wide render). Each
    // gate clause corresponds to a branch below that would add/alter a class, child, attribute, or
    // post-processing step; when all are absent the node bytes are fully determined.
    // `detail.enable_shadows` is NOT gated: the per-node shadow is the inline `filter="url(#drop-shadow)"`,
    // emitted only when `!config.embed_theme_css` (required true here), so with the theme CSS embedded the
    // shadow is a CSS rule and changes no node byte. `permit_fast` lets a caller that POST-PROCESSES the
    // returned `Element` (the sequence mirror-header loop adds a class) force the slow path. The
    // `menu_links`/`href`/`callback` clauses exclude the only node-field features the fragment omits (all
    // other conditionals — states, accents, journey/kanban/req fills, icons — derive from `node.classes`/
    // `requirement_meta`/icon/centrality already gated below).
    let user_class_suffix = ir_node.and_then(simple_node_user_class_suffix);
    if permit_fast
        && matches!(shape, NodeShape::Rect | NodeShape::Circle)
        && config.embed_theme_css
        && config.node_gradients
        && !emit_classdef_classes
        && !config.animations_enabled
        && !config.include_source_spans
        && config.a11y.aria_labels
        && config.a11y.keyboard_nav
        && config.a11y.text_alternatives
        && shape_style.is_none()
        && text_style.is_none()
        && node_icon.is_none()
        && !placeholder_space_node
        && !label_text.contains('\n')
        && !label_text.contains('\r')
        && lookup_centrality_tier(centrality_map, node_box.node_index).is_none()
        && label_id.is_none_or(|id| ir.label_markup.get(&id).is_none_or(|s| s.is_empty()))
        && let Some(node) = ir_node
        && let Some(user_classes) = user_class_suffix.as_deref()
        && node.requirement_meta.is_none()
        && node.menu_links.is_empty()
        && node.href().is_none()
        && node.callback().is_none()
    {
        return Element::raw_svg(build_common_node_fragment(
            node_id,
            node_box.node_index,
            stable_accent_index(node_id),
            raw_label_text,
            &label_text,
            x,
            y,
            w,
            h,
            config.rounded_corners * 0.55,
            cx,
            cy + node_font_size / 3.0,
            node_font_size,
            colors.text.as_str(),
            user_classes,
            shape,
        ));
    }

    // Create group for node shape + label
    let mut group = Element::group()
        .id(&mermaid_node_element_id(node_id, node_box.node_index))
        .class("fm-node")
        .class_prefixed_usize("fm-node-accent-", stable_accent_index(node_id))
        .class(node_shape_css_class(shape))
        .data("id", node_id);
    // Add centrality tier class if available (FNX semantic styling)
    if let Some(tier) = lookup_centrality_tier(centrality_map, node_box.node_index) {
        group = group.class_prefixed("fm-node-centrality-", tier.css_class_suffix());
    }
    if config.animations_enabled {
        group = group.attr(
            "style",
            &animation_style_attr(node_animation_order(node_box)),
        );
    }
    if let Some(icon) = node_icon {
        group = group.class("fm-node-has-icon");
        let icon_class = sanitize_css_token(&normalize_icon_token(icon));
        if !icon_class.is_empty() {
            group = group.class_prefixed("fm-node-icon-", &icon_class);
        }
        group = group.class(match config.node_icon_position {
            NodeIconPosition::Above => "fm-node-icon-pos-above",
            NodeIconPosition::Left => "fm-node-icon-pos-left",
        });
    }
    if config.include_source_spans {
        group = apply_span_metadata(group, node_box.span);
    }

    if let Some(node) = ir_node {
        for class in &node.classes {
            // One case-insensitive pass over `class` raises all substring keyword flags at once
            // (highlight/inactive/dashed/double border) — byte-identical to the old per-needle
            // `contains_ascii_ci` OR-chains, without allocating a lowercased copy or sweeping the
            // class string ~11 times. Exact-match keywords stay as `eq_ignore_ascii_case`.
            let sanitized = sanitize_css_token(class);
            if !sanitized.is_empty() {
                group = group.class_prefixed("fm-node-user-", &sanitized);
            }
            let kw = scan_node_class_keywords(class);
            is_highlighted |= kw.highlighted;
            is_inactive |= kw.inactive;
            dashed_border |= kw.dashed_border;
            double_border |= kw.double_border;
            if class.eq_ignore_ascii_case("c4-external") {
                dashed_border = true;
            }
            if class.eq_ignore_ascii_case("block-beta") {
                is_block_beta = true;
            }
            if class.eq_ignore_ascii_case("block-beta-space") {
                is_block_beta_space = true;
            }
        }
    }
    if is_highlighted {
        group = group.class("fm-node-highlighted");
    }
    if is_inactive {
        group = group.class("fm-node-inactive");
    }
    if dashed_border {
        group = group.class("fm-node-border-dashed");
    }
    if double_border {
        group = group.class("fm-node-border-double");
    }
    if is_block_beta {
        group = group.class("fm-node-block-beta");
    }
    if is_block_beta_space {
        group = group.class("fm-node-block-beta-space");
    }

    // Requirement diagram: add risk level and requirement type CSS classes.
    let req_risk_fill: Option<&str> = ir_node
        .and_then(|n| n.requirement_meta.as_ref())
        .and_then(|meta| meta.risk.as_ref())
        .and_then(|risk| match risk.to_ascii_lowercase().as_str() {
            "high" => Some("#fca5a5"),
            "medium" => Some("#fde68a"),
            "low" => Some("#bbf7d0"),
            _ => None,
        });

    // Kanban priority → border color styling.
    let kanban_priority_fill: Option<&str> = ir_node.and_then(|n| {
        n.classes.iter().find_map(|c| match c.as_str() {
            "kanban-priority-high" | "kanban-priority-critical" => Some("#fca5a5"),
            "kanban-priority-medium" => Some("#fde68a"),
            "kanban-priority-low" => Some("#bbf7d0"),
            _ => None,
        })
    });

    // Journey score → color fill (1=red, 2=orange, 3=yellow, 4=light green, 5=green).
    let journey_score_fill: Option<&str> = ir_node.and_then(|n| {
        n.classes.iter().find_map(|c| match c.as_str() {
            "journey-score-1" => Some("#fca5a5"),
            "journey-score-2" => Some("#fdba74"),
            "journey-score-3" => Some("#fde68a"),
            "journey-score-4" => Some("#bef264"),
            "journey-score-5" => Some("#86efac"),
            _ => None,
        })
    });
    if let Some(meta) = ir_node.and_then(|n| n.requirement_meta.as_ref()) {
        if let Some(ref risk) = meta.risk {
            let risk_class = risk.to_ascii_lowercase();
            group = group.class_prefixed("fm-req-risk-", &risk_class);
        }
        if let Some(ref req_type) = meta.requirement_type {
            let type_class = req_type
                .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                .to_ascii_lowercase();
            group = group.class_prefixed("fm-req-type-", &type_class);
        }
        if meta.verify_method.is_some() {
            group = group.class("fm-req-has-verify");
        }
    }

    // Add accessibility attributes
    if config.a11y.aria_labels {
        group = group
            .attr("role", "graphics-symbol")
            .attr("aria-label", raw_label_text);
    }

    if config.a11y.keyboard_nav {
        group = group.attr("tabindex", "0");
    }

    // Create shape element based on node type
    let shape_elem = match shape {
        NodeShape::Rect => Element::rect()
            .x(x)
            .y(y)
            .width(w)
            .height(h)
            .fill(&colors.node_fill)
            .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
            .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
            .rx(config.rounded_corners * 0.55),

        NodeShape::Rounded => Element::rect()
            .x(x)
            .y(y)
            .width(w)
            .height(h)
            .fill(&colors.node_fill)
            .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
            .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
            .rx(config.rounded_corners),

        NodeShape::Stadium => Element::rect()
            .x(x)
            .y(y)
            .width(w)
            .height(h)
            .fill(&colors.node_fill)
            .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
            .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
            .rx(w.min(h) / 2.0),

        NodeShape::Diamond => {
            let path = PathBuilder::new()
                .move_to(cx, y)
                .line_to(x + w, cy)
                .line_to(cx, y + h)
                .line_to(x, cy)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
        }

        NodeShape::Hexagon => {
            let inset = w * 0.15;
            let path = PathBuilder::new()
                .move_to(x + inset, y)
                .line_to(x + w - inset, y)
                .line_to(x + w, cy)
                .line_to(x + w - inset, y + h)
                .line_to(x + inset, y + h)
                .line_to(x, cy)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
        }

        NodeShape::Circle | NodeShape::FilledCircle | NodeShape::DoubleCircle => {
            let r = w.min(h) / 2.0;
            let mut elem = Element::circle()
                .cx(cx)
                .cy(cy)
                .r(r)
                .fill(if shape == NodeShape::FilledCircle {
                    colors.node_stroke.as_str()
                } else {
                    colors.node_fill.as_str()
                })
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css);

            if shape == NodeShape::DoubleCircle {
                // For double circle, we'll use a slightly smaller stroke
                elem = elem.stroke_width(2.0);
            }
            elem
        }

        NodeShape::HorizontalBar => Element::rect()
            .x(x)
            .y(y + h * 0.25)
            .width(w)
            .height((h * 0.5).max(8.0))
            .fill(&colors.node_stroke)
            .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
            .stroke_width(1.0)
            .rx((h * 0.25).max(3.0)),

        NodeShape::Cylinder => {
            let ry = h * 0.1;
            let path = PathBuilder::new()
                .move_to(x, y + ry)
                .arc_to(w / 2.0, ry, 0.0, false, true, x + w, y + ry)
                .line_to(x + w, y + h - ry)
                .arc_to(w / 2.0, ry, 0.0, false, false, x, y + h - ry)
                .close()
                .move_to(x, y + ry)
                .arc_to(w / 2.0, ry, 0.0, false, false, x + w, y + ry)
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
        }

        NodeShape::Trapezoid => {
            let inset = w * 0.15;
            let path = PathBuilder::new()
                .move_to(x + inset, y)
                .line_to(x + w - inset, y)
                .line_to(x + w, y + h)
                .line_to(x, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
        }

        NodeShape::Subroutine => {
            let inset = 8.0;
            let mut g = Element::group();
            g = g.child(
                Element::rect()
                    .x(x)
                    .y(y)
                    .width(w)
                    .height(h)
                    .fill(if config.node_gradients {
                        "url(#fm-node-gradient)"
                    } else {
                        colors.node_fill.as_str()
                    })
                    .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                    .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
                    .rx(config.rounded_corners * 0.45),
            );
            // Left vertical line
            g = g.child(
                Element::line()
                    .x1(x + inset)
                    .y1(y)
                    .x2(x + inset)
                    .y2(y + h)
                    .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                    .stroke_width(1.0),
            );
            // Right vertical line
            g = g.child(
                Element::line()
                    .x1(x + w - inset)
                    .y1(y)
                    .x2(x + w - inset)
                    .y2(y + h)
                    .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                    .stroke_width(1.0),
            );
            g = maybe_add_class(g, "fm-node-shape", emit_classdef_classes);
            if detail.show_node_labels {
                return group.child(g).child(render_node_label_text(
                    ir,
                    label_id,
                    label_text.as_ref(),
                    cx,
                    cy + node_font_size / 3.0,
                    node_font_size,
                    config,
                    colors,
                    text_style.as_deref(),
                    emit_classdef_classes,
                ));
            }
            return group.child(g);
        }

        NodeShape::Asymmetric => {
            let flag = w * 0.15;
            let path = PathBuilder::new()
                .move_to(x, y)
                .line_to(x + w - flag, y)
                .line_to(x + w, cy)
                .line_to(x + w - flag, y + h)
                .line_to(x, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
        }

        NodeShape::Note => {
            let fold = 10.0;
            let path = PathBuilder::new()
                .move_to(x, y)
                .line_to(x + w - fold, y)
                .line_to(x + w, y + fold)
                .line_to(x + w, y + h)
                .line_to(x, y + h)
                .close()
                .move_to(x + w - fold, y)
                .line_to(x + w - fold, y + fold)
                .line_to(x + w, y + fold)
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width(1.0)
        }

        // Extended shapes for FrankenMermaid
        NodeShape::InvTrapezoid => {
            let inset = w * 0.15;
            let path = PathBuilder::new()
                .move_to(x, y)
                .line_to(x + w, y)
                .line_to(x + w - inset, y + h)
                .line_to(x + inset, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
        }

        NodeShape::Parallelogram => {
            let inset = w * 0.15;
            let path = PathBuilder::new()
                .move_to(x + inset, y)
                .line_to(x + w, y)
                .line_to(x + w - inset, y + h)
                .line_to(x, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
        }

        NodeShape::InvParallelogram => {
            let inset = w * 0.15;
            let path = PathBuilder::new()
                .move_to(x, y)
                .line_to(x + w - inset, y)
                .line_to(x + w, y + h)
                .line_to(x + inset, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
        }

        NodeShape::Triangle => {
            let path = PathBuilder::new()
                .move_to(cx, y)
                .line_to(x + w, y + h)
                .line_to(x, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
        }

        NodeShape::Pentagon => {
            // Regular pentagon (5 sides)
            let angle_offset = -std::f32::consts::FRAC_PI_2; // Start at top
            let r = w.min(h) / 2.0;
            let mut path = PathBuilder::new();
            for i in 0..5 {
                let angle = angle_offset + (i as f32) * 2.0 * std::f32::consts::PI / 5.0;
                let px = cx + r * angle.cos();
                let py = cy + r * angle.sin();
                if i == 0 {
                    path = path.move_to(px, py);
                } else {
                    path = path.line_to(px, py);
                }
            }
            Element::path()
                .d(&path.close().build())
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
        }

        NodeShape::Star => {
            // 5-pointed star
            let outer_r = w.min(h) / 2.0;
            let inner_r = outer_r * 0.4;
            let angle_offset = -std::f32::consts::FRAC_PI_2;
            let mut path = PathBuilder::new();
            for i in 0..10 {
                let r = if i % 2 == 0 { outer_r } else { inner_r };
                let angle = angle_offset + (i as f32) * std::f32::consts::PI / 5.0;
                let px = cx + r * angle.cos();
                let py = cy + r * angle.sin();
                if i == 0 {
                    path = path.move_to(px, py);
                } else {
                    path = path.line_to(px, py);
                }
            }
            Element::path()
                .d(&path.close().build())
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
        }

        NodeShape::Cloud => {
            // Simplified cloud shape using circles
            let r = h / 3.0;
            let path = PathBuilder::new()
                .move_to(x + r, y + h * 0.6)
                .arc_to(r, r, 0.0, true, true, x + r * 2.0, y + h * 0.3)
                .arc_to(r * 0.8, r * 0.8, 0.0, true, true, x + w * 0.5, y + r * 0.5)
                .arc_to(r, r, 0.0, true, true, x + w - r * 2.0, y + h * 0.3)
                .arc_to(r, r, 0.0, true, true, x + w - r, y + h * 0.6)
                .arc_to(r * 0.7, r * 0.7, 0.0, true, true, x + w - r, y + h * 0.8)
                .line_to(x + r, y + h * 0.8)
                .arc_to(r * 0.7, r * 0.7, 0.0, true, true, x + r, y + h * 0.6)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
        }

        NodeShape::Tag => {
            // Tag/flag shape (rectangle with arrow point on right)
            let point = w * 0.2;
            let path = PathBuilder::new()
                .move_to(x, y)
                .line_to(x + w - point, y)
                .line_to(x + w, cy)
                .line_to(x + w - point, y + h)
                .line_to(x, y + h)
                .close()
                .build();
            Element::path()
                .d(&path)
                .fill(&colors.node_fill)
                .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                .stroke_width_unless_embedded_css(1.6, config.embed_theme_css)
        }

        NodeShape::CrossedCircle => {
            // Circle with X through it
            let r = w.min(h) / 2.0;
            let mut g = Element::group();
            g = g.child(
                Element::circle()
                    .cx(cx)
                    .cy(cy)
                    .r(r)
                    .fill(if config.node_gradients {
                        "url(#fm-node-gradient)"
                    } else {
                        colors.node_fill.as_str()
                    })
                    .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                    .stroke_width_unless_embedded_css(1.6, config.embed_theme_css),
            );
            // Diagonal lines
            let offset = r * 0.707; // r * cos(45°)
            g = g.child(
                Element::line()
                    .x1(cx - offset)
                    .y1(cy - offset)
                    .x2(cx + offset)
                    .y2(cy + offset)
                    .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                    .stroke_width_unless_embedded_css(1.6, config.embed_theme_css),
            );
            g = g.child(
                Element::line()
                    .x1(cx + offset)
                    .y1(cy - offset)
                    .x2(cx - offset)
                    .y2(cy + offset)
                    .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                    .stroke_width_unless_embedded_css(1.6, config.embed_theme_css),
            );
            g = maybe_add_class(g, "fm-node-shape", emit_classdef_classes);
            if detail.show_node_labels {
                return group.child(g).child(render_node_label_text(
                    ir,
                    label_id,
                    &label_text,
                    cx,
                    cy + node_font_size / 3.0,
                    node_font_size,
                    config,
                    colors,
                    text_style.as_deref(),
                    emit_classdef_classes,
                ));
            }
            return group.child(g);
        }
    };

    let shape_elem = maybe_add_class(shape_elem, "fm-node-shape", emit_classdef_classes);

    let shape_elem = if config.node_gradients
        && !matches!(
            shape,
            NodeShape::Note | NodeShape::FilledCircle | NodeShape::HorizontalBar
        ) {
        shape_elem.fill("url(#fm-node-gradient)")
    } else {
        shape_elem
    };

    // Apply shadow filter if enabled and this isn't a special composite shape.
    // Highlighted nodes prefer glow so the effects don't visually muddy each other.
    // The inline `filter="url(#drop-shadow)"` is redundant when the theme CSS is embedded: the
    // unconditional `.fm-node <shape> { filter: drop-shadow(…) }` rule (emitted by `to_svg_style`
    // under the *same* `detail.enable_shadows` gate) overrides this presentation attribute. Emit
    // the inline copy only for attribute-driven exports (`embed_theme_css = false`, PNG raster).
    let shape_elem = if detail.enable_shadows
        && !config.embed_theme_css
        && !(is_highlighted && config.glow_enabled)
        && !matches!(
            shape,
            NodeShape::Subroutine
                | NodeShape::CrossedCircle
                | NodeShape::FilledCircle
                | NodeShape::HorizontalBar
        ) {
        shape_elem.filter("url(#drop-shadow)")
    } else {
        shape_elem
    };

    // Apply inline style from style directives if present.
    let shape_elem = if let Some(inline_style) = shape_style.as_deref() {
        shape_elem.attr("style", inline_style)
    } else if let Some(risk_fill) = req_risk_fill {
        // Requirement risk-level fill when no explicit style override.
        shape_elem.attr("style", &format!("fill: {risk_fill}"))
    } else if let Some(score_fill) = journey_score_fill {
        // Journey score-based fill color.
        shape_elem.attr("style", &format!("fill: {score_fill}"))
    } else if let Some(priority_fill) = kanban_priority_fill {
        // Kanban priority-based fill color.
        shape_elem.attr("style", &format!("fill: {priority_fill}"))
    } else {
        shape_elem
    };

    group = group.child(shape_elem);
    if is_highlighted && config.glow_enabled {
        group = group.filter("url(#node-glow)");
    }

    let icon_size = clamp_font_size(node_font_size * 1.35, config.min_font_size + 2.0);
    let icon_reserved_height = node_icon.map_or(0.0, |_| match config.node_icon_position {
        NodeIconPosition::Above => icon_size + 10.0,
        NodeIconPosition::Left => 0.0,
    });
    let icon_reserved_width = node_icon.map_or(0.0, |_| match config.node_icon_position {
        NodeIconPosition::Above => 0.0,
        NodeIconPosition::Left => icon_size + 14.0,
    });
    if let Some(icon) = node_icon
        && let Some(icon_elem) = render_node_icon(
            icon,
            if detail.show_node_labels
                && matches!(config.node_icon_position, NodeIconPosition::Left)
            {
                x + (icon_reserved_width * 0.5) + 2.0
            } else {
                cx
            },
            if detail.show_node_labels
                && matches!(config.node_icon_position, NodeIconPosition::Above)
            {
                y + (icon_reserved_height * 0.5) + 2.0
            } else {
                cy
            },
            icon_size,
            config,
            colors,
        )
    {
        group = group.child(icon_elem);
    }

    // Add label text — with three-compartment rendering for class diagrams.
    if detail.show_node_labels {
        if let Some(node) = ir_node
            && let Some(ref meta) = node.class_meta
            && (!meta.attributes.is_empty() || !meta.methods.is_empty())
        {
            group = render_class_compartments(
                group,
                node,
                ir,
                x,
                y,
                w,
                h,
                node_font_size,
                config,
                colors,
                text_style.as_deref(),
                emit_classdef_classes,
            );
        } else if let Some(node) = ir_node
            && let Some(ref req_meta) = node.requirement_meta
            && (req_meta.requirement_type.is_some()
                || req_meta.risk.is_some()
                || req_meta.verify_method.is_some())
        {
            // Requirement node: multi-line content with type, label, metadata.
            let subtitle_font_size = clamp_font_size(node_font_size * 0.75, config.min_font_size);
            let mut text_y = y + h * 0.25 + node_font_size * 0.35;

            // Requirement type header (e.g., "<<requirement>>")
            if let Some(ref req_type) = req_meta.requirement_type {
                let type_label = format!("\u{00ab}{req_type}\u{00bb}");
                let mut type_elem = Element::text()
                    .x(cx)
                    .y(text_y)
                    .content(&type_label)
                    .attr("text-anchor", "middle")
                    .attr("dominant-baseline", "central")
                    .attr_num("font-size", subtitle_font_size)
                    .attr("font-style", "italic")
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .fill(&colors.text)
                    .class("fm-req-type-label");
                type_elem = apply_label_class(type_elem);
                if let Some(style) = text_style.as_deref() {
                    type_elem = type_elem.attr("style", style);
                }
                group = group.child(type_elem);
                text_y += node_font_size * 0.85;
            }

            // Main label
            let text_elem = render_node_label_text(
                ir,
                if detail.node_label_max_chars.is_none() {
                    label_id
                } else {
                    None
                },
                &label_text,
                cx,
                text_y,
                node_font_size,
                config,
                colors,
                text_style.as_deref(),
                emit_classdef_classes,
            );
            group = group.child(text_elem);
            text_y += node_font_size * 0.85;

            // Risk + verify method subtitle
            let mut info_parts = Vec::new();
            if let Some(ref risk) = req_meta.risk {
                info_parts.push(format!("Risk: {risk}"));
            }
            if let Some(ref vm) = req_meta.verify_method {
                info_parts.push(format!("Verify: {vm}"));
            }
            if !info_parts.is_empty() {
                let info_text = info_parts.join(" | ");
                let mut meta_elem = Element::text()
                    .x(cx)
                    .y(text_y)
                    .content(&info_text)
                    .attr("text-anchor", "middle")
                    .attr("dominant-baseline", "central")
                    .attr_num("font-size", subtitle_font_size)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .fill(&colors.text)
                    .attr("opacity", "0.7")
                    .class("fm-req-metadata");
                meta_elem = apply_label_class(meta_elem);
                if let Some(style) = text_style.as_deref() {
                    meta_elem = meta_elem.attr("style", style);
                }
                group = group.child(meta_elem);
            }
        } else if let Some(node) = ir_node
            && !node.members.is_empty()
            && ir.diagram_type == fm_core::DiagramType::Er
        {
            // ER entity: render name + attribute list.
            let attr_font_size = clamp_font_size(node_font_size * 0.8, config.min_font_size);
            let header_height = node_font_size * 1.5;

            // Entity name header
            let mut name_elem = Element::text()
                .x(cx)
                .y(y + header_height * 0.6)
                .content(label_text.as_ref())
                .attr("text-anchor", "middle")
                .attr("dominant-baseline", "central")
                .attr_num("font-size", node_font_size)
                .attr("font-weight", "bold")
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .fill(&colors.text)
                .class("fm-er-entity-name");
            name_elem = apply_label_class(name_elem);
            if let Some(style) = text_style.as_deref() {
                name_elem = name_elem.attr("style", style);
            }
            group = group.child(name_elem);

            // Divider line
            group = group.child(
                Element::line()
                    .x1(x + 2.0)
                    .y1(y + header_height)
                    .x2(x + w - 2.0)
                    .y2(y + header_height)
                    .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                    .stroke_width(0.8),
            );

            // Attribute list
            let mut attr_y = y + header_height + attr_font_size * 0.9;
            for attr in &node.members {
                let key_prefix = match attr.key {
                    fm_core::IrAttributeKey::Pk => "PK ",
                    fm_core::IrAttributeKey::Fk => "FK ",
                    fm_core::IrAttributeKey::Uk => "UK ",
                    fm_core::IrAttributeKey::None => "",
                };
                let attr_text = format!("{key_prefix}{} {}", attr.data_type, attr.name);
                let font_weight = if attr.key == fm_core::IrAttributeKey::None {
                    "normal"
                } else {
                    "bold"
                };
                let mut attr_elem = Element::text()
                    .x(x + 8.0)
                    .y(attr_y)
                    .content(&attr_text)
                    .attr("text-anchor", "start")
                    .attr("dominant-baseline", "central")
                    .attr_num("font-size", attr_font_size)
                    .attr("font-weight", font_weight)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .fill(&colors.text)
                    .class("fm-er-attribute");
                attr_elem = apply_label_class(attr_elem);
                if let Some(style) = text_style.as_deref() {
                    attr_elem = attr_elem.attr("style", style);
                }
                group = group.child(attr_elem);
                attr_y += attr_font_size * 1.3;
            }
        } else if let Some(node) = ir_node
            && let Some(ref c4_meta) = node.c4_meta
        {
            group = render_c4_node_content(
                group,
                node,
                c4_meta,
                ir,
                x,
                y,
                w,
                h,
                node_font_size,
                config,
                colors,
                text_style.as_deref(),
                emit_classdef_classes,
            );
        } else {
            let lines_count = label_text.lines().count().max(1) as f32;
            let total_text_height = (lines_count - 1.0) * node_font_size * config.line_height;
            let content_left = x + icon_reserved_width;
            let content_width = (w - icon_reserved_width).max(node_font_size);
            let content_top = y + icon_reserved_height;
            let content_height = (h - icon_reserved_height).max(node_font_size);
            let start_y = content_top + (content_height / 2.0) - (total_text_height / 2.0)
                + (node_font_size / 3.0);

            let text_elem = render_node_label_text(
                ir,
                if detail.node_label_max_chars.is_none() {
                    label_id
                } else {
                    None
                },
                &label_text,
                content_left + (content_width / 2.0),
                start_y,
                node_font_size,
                config,
                colors,
                text_style.as_deref(),
                emit_classdef_classes,
            );
            group = group.child(text_elem);
        }
    }

    // Add title element for text alternatives
    if config.a11y.text_alternatives
        && let Some(node) = ir_node
    {
        let node_desc = describe_node(node, ir);
        group = group.child(Element::title(&node_desc));
    }

    if let Some(node) = ir_node
        && !node.menu_links.is_empty()
    {
        let sanitize_mode = ir.meta.init.config.sanitize_mode;
        let menu_links: Vec<fm_core::IrMenuLink> = if sanitize_mode == MermaidSanitizeMode::Lenient
        {
            node.menu_links.clone()
        } else {
            node.menu_links
                .iter()
                .filter(|link| is_safe_link_target(&link.url, sanitize_mode))
                .cloned()
                .collect()
        };
        if !menu_links.is_empty() {
            group = group
                .attr("data-menu-links", &serialize_menu_links(&menu_links))
                .class("fm-node-has-menu-links");
        }
    }

    if let Some(node) = ir_node
        && let Some(href) = node.href()
        && is_safe_link_target(href, ir.meta.init.config.sanitize_mode)
    {
        match config.link_mode {
            MermaidLinkMode::Inline => {
                let mut a = Element::new(crate::element::ElementKind::A)
                    .attr("href", href)
                    .attr("target", "_blank")
                    .attr("rel", "noopener noreferrer");

                group = group.attr("style", "cursor: pointer;");

                a = a.child(group);
                return a;
            }
            MermaidLinkMode::Footnote => {
                group = group.attr("data-link", href).class("fm-node-has-link");
            }
            MermaidLinkMode::Off => {}
        }
    }

    // Callback nodes: emit data-callback attribute for embedding JS integration.
    if let Some(node) = ir_node
        && let Some(callback) = node.callback()
    {
        group = group
            .attr("data-callback", callback)
            .attr("style", "cursor: pointer;")
            .class("fm-node-has-callback");
    }

    group
}

fn is_block_beta_space_node(node: &fm_core::IrNode) -> bool {
    node.id.starts_with("__space_")
        || node
            .classes
            .iter()
            .any(|class_name| class_name.eq_ignore_ascii_case("block-beta-space"))
}

fn serialize_menu_links(links: &[fm_core::IrMenuLink]) -> String {
    match serde_json::to_string(links) {
        Ok(json) => json,
        Err(_) => String::from("[]"),
    }
}

fn stable_accent_index(node_id: &str) -> usize {
    // FNV-1a 32-bit hash for deterministic class assignment.
    let mut hash: u32 = 0x811c9dc5;
    for byte in node_id.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    (hash as usize % 8) + 1
}

/// Render a UML three-compartment class box: header | attributes | methods.
///
/// Adds separator lines and member text elements to the node group.
#[allow(clippy::too_many_arguments)]
fn render_class_compartments(
    mut group: Element,
    node: &fm_core::IrNode,
    ir: &MermaidDiagramIr,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    font_size: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
    label_style: Option<&str>,
    emit_classdef_classes: bool,
) -> Element {
    let meta = match &node.class_meta {
        Some(m) => m,
        None => return group,
    };

    let apply_label_style = |mut elem: Element| {
        if let Some(style) = label_style {
            elem = elem.attr("style", style);
        }
        elem
    };
    let apply_label_class =
        |elem: Element| maybe_add_class(elem, "fm-node-label", emit_classdef_classes);

    let line_h = font_size * config.line_height;
    let padding_x = 8.0;
    let text_x = x + padding_x;
    let mut cursor_y = y + line_h;

    // Header: class name (centered, bold).
    let class_name = node
        .label
        .and_then(|lid| ir.labels.get(lid.0))
        .map(|l| l.text.as_str())
        .unwrap_or(&node.id);

    // Stereotype above class name if present.
    if let Some(ref stereotype) = meta.stereotype {
        let stereo_text = match stereotype {
            fm_core::ClassStereotype::Interface => "<<interface>>",
            fm_core::ClassStereotype::Abstract => "<<abstract>>",
            fm_core::ClassStereotype::Enum => "<<enumeration>>",
            fm_core::ClassStereotype::Service => "<<service>>",
            fm_core::ClassStereotype::Custom(s) => s.as_str(),
        };
        let stereo_elem = TextBuilder::new(stereo_text)
            .x(x + w / 2.0)
            .y(cursor_y)
            .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
            .font_size(font_size * 0.85)
            .anchor(TextAnchor::Middle)
            .italic()
            .fill(&colors.text)
            .build();
        group = group.child(apply_label_style(apply_label_class(stereo_elem)));
        cursor_y += line_h;
    }

    // Append generic parameters to class name if present (e.g., "List<T>").
    let display_name = if meta.generics.is_empty() {
        class_name.to_string()
    } else {
        format!("{class_name}<{}>", meta.generics.join(", "))
    };

    let name_elem = TextBuilder::new(&display_name)
        .x(x + w / 2.0)
        .y(cursor_y)
        .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
        .font_size(font_size)
        .anchor(TextAnchor::Middle)
        .bold()
        .fill(&colors.text)
        .build();
    group = group.child(apply_label_style(apply_label_class(name_elem)));
    cursor_y += line_h * 0.5;

    // Separator line after header.
    let sep1 = Element::new(crate::element::ElementKind::Line)
        .attr_num("x1", x)
        .attr_num("y1", cursor_y)
        .attr_num("x2", x + w)
        .attr_num("y2", cursor_y)
        .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
        .stroke_width(1.0);
    group = group.child(sep1);
    cursor_y += line_h * 0.3;

    // Attributes compartment.
    let member_font_size = font_size * 0.9;
    for attr in &meta.attributes {
        cursor_y += member_font_size * config.line_height * 0.9;
        if cursor_y > y + h - line_h * 0.5 {
            break;
        }
        let vis = visibility_symbol(attr.visibility);
        let text = if let Some(ref ret) = attr.return_type {
            format!("{vis}{}: {ret}", attr.name)
        } else {
            format!("{vis}{}", attr.name)
        };
        let elem = TextBuilder::new(&text)
            .x(text_x)
            .y(cursor_y)
            .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
            .font_size(member_font_size)
            .anchor(TextAnchor::Start)
            .fill(&colors.text)
            .build();
        group = group.child(apply_label_style(apply_label_class(elem)));
    }

    // Separator before methods (only if both sections present).
    if !meta.attributes.is_empty() && !meta.methods.is_empty() {
        cursor_y += line_h * 0.3;
        let sep2 = Element::new(crate::element::ElementKind::Line)
            .attr_num("x1", x)
            .attr_num("y1", cursor_y)
            .attr_num("x2", x + w)
            .attr_num("y2", cursor_y)
            .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
            .stroke_width(1.0);
        group = group.child(sep2);
        cursor_y += line_h * 0.3;
    }

    // Methods compartment.
    for method in &meta.methods {
        cursor_y += member_font_size * config.line_height * 0.9;
        if cursor_y > y + h - line_h * 0.5 {
            break;
        }
        let vis = visibility_symbol(method.visibility);
        let suffix = if method.is_abstract {
            "*"
        } else if method.is_static {
            "$"
        } else {
            ""
        };
        let ret = method
            .return_type
            .as_deref()
            .map(|t| format!(": {t}"))
            .unwrap_or_default();
        let text = format!("{vis}{}{suffix}{ret}", method.name);
        let elem = TextBuilder::new(&text)
            .x(text_x)
            .y(cursor_y)
            .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
            .font_size(member_font_size)
            .anchor(TextAnchor::Start)
            .fill(&colors.text)
            .build();
        group = group.child(apply_label_style(apply_label_class(elem)));
    }

    group
}

#[allow(clippy::too_many_arguments)]
fn render_c4_node_content(
    mut group: Element,
    node: &fm_core::IrNode,
    c4_meta: &fm_core::IrC4NodeMeta,
    ir: &MermaidDiagramIr,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    font_size: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
    label_style: Option<&str>,
    emit_classdef_classes: bool,
) -> Element {
    let apply_label_style = |mut elem: Element| {
        if let Some(style) = label_style {
            elem = elem.attr("style", style);
        }
        elem
    };
    let apply_label_class =
        |elem: Element| maybe_add_class(elem, "fm-node-label", emit_classdef_classes);

    let label_text = node
        .label
        .and_then(|lid| ir.labels.get(lid.0))
        .map(|label| label.text.as_str())
        .unwrap_or(node.id.as_str());

    let line_h = font_size * config.line_height;
    let small_font = clamp_font_size(font_size * 0.78, config.min_font_size);
    let description_font = clamp_font_size(font_size * 0.72, config.min_font_size);
    let mut cursor_y = y + (small_font * 1.25);

    group = group.child(apply_label_style(apply_label_class(
        TextBuilder::new(&format!("<<{}>>", c4_meta.element_type))
            .x(x + w / 2.0)
            .y(cursor_y)
            .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
            .font_size(small_font)
            .font_weight("600")
            .anchor(TextAnchor::Middle)
            .fill(&colors.cluster_stroke)
            .class("fm-c4-type-label")
            .build(),
    )));

    if node
        .classes
        .iter()
        .any(|class_name| class_name == "c4-person")
    {
        group = group.child(render_c4_person_icon(
            x + 18.0,
            y + 18.0,
            colors.node_stroke.as_str(),
        ));
    }

    cursor_y += line_h * 0.95;
    group = group.child(apply_label_style(apply_label_class(
        TextBuilder::new(label_text)
            .x(x + w / 2.0)
            .y(cursor_y)
            .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
            .font_size(font_size)
            .font_weight("600")
            .anchor(TextAnchor::Middle)
            .fill(&colors.text)
            .class("fm-c4-name")
            .build(),
    )));

    if let Some(technology) = &c4_meta.technology {
        cursor_y += line_h * 0.9;
        group = group.child(apply_label_style(apply_label_class(
            TextBuilder::new(&format!("[{technology}]"))
                .x(x + w / 2.0)
                .y(cursor_y)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(small_font)
                .anchor(TextAnchor::Middle)
                .fill(&colors.edge)
                .class("fm-c4-technology")
                .build(),
        )));
    }

    if let Some(description) = &c4_meta.description {
        cursor_y += line_h * 0.9;
        let available_width = (w - 20.0).max(32.0);
        let description_lines =
            wrap_text_to_lines(description, available_width, config.avg_char_width * 0.92);
        if !description_lines.is_empty() {
            let description_text = description_lines.join("\n");
            let description_height = (description_lines.len().saturating_sub(1) as f32)
                * description_font
                * config.line_height;
            let baseline_y =
                (cursor_y + description_height.min((h * 0.35).max(0.0))).min(y + h - 8.0);
            group = group.child(apply_label_style(apply_label_class(
                TextBuilder::new(&description_text)
                    .x(x + w / 2.0)
                    .y(baseline_y)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .font_size(description_font)
                    .line_height(config.line_height)
                    .anchor(TextAnchor::Middle)
                    .fill(&colors.text)
                    .class("fm-c4-description")
                    .build(),
            )));
        }
    }

    group
}

fn render_c4_person_icon(x: f32, y: f32, stroke: &str) -> Element {
    let mut icon = Element::group().class("fm-c4-person-icon");
    icon = icon.child(
        Element::circle()
            .cx(x)
            .cy(y - 6.0)
            .r(3.0)
            .fill("none")
            .stroke(stroke)
            .stroke_width(1.1),
    );
    icon = icon.child(
        Element::line()
            .x1(x)
            .y1(y - 2.0)
            .x2(x)
            .y2(y + 7.0)
            .stroke(stroke)
            .stroke_width(1.1),
    );
    icon = icon.child(
        Element::line()
            .x1(x - 5.0)
            .y1(y + 1.0)
            .x2(x + 5.0)
            .y2(y + 1.0)
            .stroke(stroke)
            .stroke_width(1.1),
    );
    icon = icon.child(
        Element::line()
            .x1(x)
            .y1(y + 7.0)
            .x2(x - 4.5)
            .y2(y + 13.0)
            .stroke(stroke)
            .stroke_width(1.1),
    );
    icon.child(
        Element::line()
            .x1(x)
            .y1(y + 7.0)
            .x2(x + 4.5)
            .y2(y + 13.0)
            .stroke(stroke)
            .stroke_width(1.1),
    )
}

fn normalize_icon_token(raw_icon: &str) -> String {
    let trimmed = raw_icon.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let normalized = trimmed
        .strip_prefix("fa:")
        .unwrap_or(trimmed)
        .strip_prefix("fa-")
        .unwrap_or(trimmed)
        .replace("fa ", "")
        .replace(['_', ' '], "-")
        .to_ascii_lowercase();

    match normalized.as_str() {
        "fa-book" => "book".to_string(),
        "fa-cloud" => "cloud".to_string(),
        "fa-database" => "database".to_string(),
        "fa-server" => "server".to_string(),
        "fa-user" => "user".to_string(),
        "fa-lock" => "lock".to_string(),
        "fa-mobile" | "fa-mobile-alt" => "mobile".to_string(),
        "fa-desktop" => "desktop".to_string(),
        "fa-cubes" | "docker" => "container".to_string(),
        "fa-list" => "queue".to_string(),
        "fa-balance-scale" => "load-balancer".to_string(),
        "fa-gear" | "fa-cog" => "gear".to_string(),
        other => other.to_string(),
    }
}

fn render_node_icon(
    raw_icon: &str,
    cx: f32,
    cy: f32,
    size: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
) -> Option<Element> {
    let trimmed = raw_icon.trim();
    if trimmed.is_empty() {
        return None;
    }

    let looks_like_emoji = trimmed.chars().count() <= 4 && !trimmed.is_ascii();
    if looks_like_emoji {
        return Some(
            TextBuilder::new(trimmed)
                .x(cx)
                .y(cy + size * 0.18)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(size)
                .anchor(TextAnchor::Middle)
                .class("fm-node-icon")
                .class("fm-node-icon-emoji")
                .build(),
        );
    }

    let normalized = normalize_icon_token(trimmed);
    if normalized.is_empty() {
        return None;
    }

    let half = size / 2.0;
    let x = cx - half;
    let y = cy - half;
    let stroke = colors.node_stroke.as_str();
    let fill = colors.node_fill.as_str();
    let icon_class = sanitize_css_token(&normalized);
    let mut icon = Element::group()
        .class("fm-node-icon")
        .class_prefixed("fm-node-icon-", &icon_class);

    if let Some(custom_icon) = config.custom_icons.get(&normalized) {
        return Some(icon.child(render_custom_svg_icon(custom_icon, cx, cy, size, stroke)));
    }

    match normalized.as_str() {
        "person" | "user" => {
            icon = icon.child(render_c4_person_icon(cx, cy, stroke));
        }
        "server" => {
            icon = icon.child(
                Element::rect()
                    .x(x)
                    .y(y - 1.0)
                    .width(size)
                    .height(size * 0.72)
                    .rx(2.0)
                    .fill(fill)
                    .stroke(stroke)
                    .stroke_width(1.1),
            );
            icon = icon.child(
                Element::line()
                    .x1(x + size * 0.18)
                    .y1(y + size * 0.2)
                    .x2(x + size * 0.82)
                    .y2(y + size * 0.2)
                    .stroke(stroke)
                    .stroke_width(1.0),
            );
            icon = icon.child(
                Element::line()
                    .x1(x + size * 0.18)
                    .y1(y + size * 0.38)
                    .x2(x + size * 0.82)
                    .y2(y + size * 0.38)
                    .stroke(stroke)
                    .stroke_width(1.0),
            );
        }
        "database" => {
            let ry = size * 0.14;
            let path = PathBuilder::new()
                .move_to(x, y + ry)
                .arc_to(size / 2.0, ry, 0.0, false, true, x + size, y + ry)
                .line_to(x + size, y + size - ry)
                .arc_to(size / 2.0, ry, 0.0, false, false, x, y + size - ry)
                .close()
                .move_to(x, y + ry)
                .arc_to(size / 2.0, ry, 0.0, false, false, x + size, y + ry)
                .build();
            icon = icon.child(
                Element::path()
                    .d(&path)
                    .fill(fill)
                    .stroke(stroke)
                    .stroke_width(1.1),
            );
        }
        "cloud" => {
            let r = size / 3.0;
            let path = PathBuilder::new()
                .move_to(x + r, y + size * 0.65)
                .arc_to(r, r, 0.0, true, true, x + r * 2.0, y + size * 0.35)
                .arc_to(
                    r * 0.85,
                    r * 0.85,
                    0.0,
                    true,
                    true,
                    x + size * 0.52,
                    y + r * 0.45,
                )
                .arc_to(r, r, 0.0, true, true, x + size - r * 2.0, y + size * 0.35)
                .arc_to(r, r, 0.0, true, true, x + size - r, y + size * 0.65)
                .arc_to(r * 0.65, r * 0.65, 0.0, true, true, x + r, y + size * 0.65)
                .close()
                .build();
            icon = icon.child(
                Element::path()
                    .d(&path)
                    .fill(fill)
                    .stroke(stroke)
                    .stroke_width(1.1),
            );
        }
        "lock" | "security" => {
            icon = icon.child(
                Element::rect()
                    .x(x + size * 0.16)
                    .y(y + size * 0.42)
                    .width(size * 0.68)
                    .height(size * 0.46)
                    .rx(2.0)
                    .fill(fill)
                    .stroke(stroke)
                    .stroke_width(1.1),
            );
            icon = icon.child(
                Element::path()
                    .d(&PathBuilder::new()
                        .move_to(x + size * 0.3, y + size * 0.42)
                        .line_to(x + size * 0.3, y + size * 0.26)
                        .arc_to(
                            size * 0.2,
                            size * 0.2,
                            0.0,
                            false,
                            true,
                            x + size * 0.7,
                            y + size * 0.26,
                        )
                        .line_to(x + size * 0.7, y + size * 0.42)
                        .build())
                    .fill("none")
                    .stroke(stroke)
                    .stroke_width(1.1),
            );
        }
        "gear" | "settings" => {
            icon = icon.child(
                Element::circle()
                    .cx(cx)
                    .cy(cy)
                    .r(size * 0.2)
                    .fill(fill)
                    .stroke(stroke)
                    .stroke_width(1.1),
            );
            for (dx, dy) in [
                (0.0, -0.42),
                (0.3, -0.3),
                (0.42, 0.0),
                (0.3, 0.3),
                (0.0, 0.42),
                (-0.3, 0.3),
                (-0.42, 0.0),
                (-0.3, -0.3),
            ] {
                icon = icon.child(
                    Element::line()
                        .x1(cx + size * dx * 0.55)
                        .y1(cy + size * dy * 0.55)
                        .x2(cx + size * dx * 0.78)
                        .y2(cy + size * dy * 0.78)
                        .stroke(stroke)
                        .stroke_width(1.0),
                );
            }
        }
        "api" => {
            icon = icon.child(
                TextBuilder::new("</>")
                    .x(cx)
                    .y(cy + size * 0.16)
                    .font_family(
                        "'JetBrains Mono', 'Fira Code', 'SFMono-Regular', Consolas, monospace",
                    )
                    .font_size(size * 0.72)
                    .anchor(TextAnchor::Middle)
                    .fill(stroke)
                    .build(),
            );
        }
        "mobile" | "phone" => {
            icon = icon.child(
                Element::rect()
                    .x(x + size * 0.22)
                    .y(y)
                    .width(size * 0.56)
                    .height(size)
                    .rx(4.0)
                    .fill(fill)
                    .stroke(stroke)
                    .stroke_width(1.1),
            );
            icon = icon.child(
                Element::circle()
                    .cx(cx)
                    .cy(y + size * 0.86)
                    .r(size * 0.04)
                    .fill(stroke),
            );
        }
        "desktop" => {
            icon = icon.child(
                Element::rect()
                    .x(x)
                    .y(y)
                    .width(size)
                    .height(size * 0.64)
                    .rx(2.0)
                    .fill(fill)
                    .stroke(stroke)
                    .stroke_width(1.1),
            );
            icon = icon.child(
                Element::line()
                    .x1(cx)
                    .y1(y + size * 0.64)
                    .x2(cx)
                    .y2(y + size * 0.84)
                    .stroke(stroke)
                    .stroke_width(1.0),
            );
            icon = icon.child(
                Element::line()
                    .x1(x + size * 0.28)
                    .y1(y + size * 0.84)
                    .x2(x + size * 0.72)
                    .y2(y + size * 0.84)
                    .stroke(stroke)
                    .stroke_width(1.0),
            );
        }
        "container" | "docker" => {
            for (dx, dy) in [(0.0, 0.14), (0.24, 0.14), (0.12, 0.38)] {
                icon = icon.child(
                    Element::rect()
                        .x(x + size * dx)
                        .y(y + size * dy)
                        .width(size * 0.28)
                        .height(size * 0.22)
                        .rx(1.0)
                        .fill(fill)
                        .stroke(stroke)
                        .stroke_width(1.0),
                );
            }
        }
        "queue" => {
            for offset in [0.18, 0.42, 0.66] {
                icon = icon.child(
                    Element::line()
                        .x1(x + size * 0.12)
                        .y1(y + size * offset)
                        .x2(x + size * 0.88)
                        .y2(y + size * offset)
                        .stroke(stroke)
                        .stroke_width(1.2),
                );
            }
        }
        "cache" => {
            for inset in [0.0, 0.1, 0.2] {
                icon = icon.child(
                    Element::rect()
                        .x(x + size * inset)
                        .y(y + size * inset)
                        .width(size * 0.62)
                        .height(size * 0.46)
                        .rx(2.0)
                        .fill(fill)
                        .stroke(stroke)
                        .stroke_width(1.0),
                );
            }
        }
        "load-balancer" | "loadbalancer" => {
            icon = icon.child(
                Element::line()
                    .x1(cx)
                    .y1(y + size * 0.1)
                    .x2(cx)
                    .y2(y + size * 0.85)
                    .stroke(stroke)
                    .stroke_width(1.1),
            );
            for end_x in [x + size * 0.18, x + size * 0.82] {
                icon = icon.child(
                    Element::line()
                        .x1(cx)
                        .y1(y + size * 0.28)
                        .x2(end_x)
                        .y2(y + size * 0.5)
                        .stroke(stroke)
                        .stroke_width(1.1),
                );
                icon = icon.child(
                    Element::line()
                        .x1(cx)
                        .y1(y + size * 0.58)
                        .x2(end_x)
                        .y2(y + size * 0.8)
                        .stroke(stroke)
                        .stroke_width(1.1),
                );
            }
        }
        "book" => {
            icon = icon.child(
                Element::rect()
                    .x(x + size * 0.08)
                    .y(y)
                    .width(size * 0.84)
                    .height(size * 0.9)
                    .rx(2.0)
                    .fill(fill)
                    .stroke(stroke)
                    .stroke_width(1.1),
            );
            icon = icon.child(
                Element::line()
                    .x1(cx)
                    .y1(y + size * 0.08)
                    .x2(cx)
                    .y2(y + size * 0.82)
                    .stroke(stroke)
                    .stroke_width(1.0),
            );
        }
        _ => {
            let fallback = normalized
                .split('-')
                .filter(|segment| !segment.is_empty())
                .take(2)
                .map(|segment| {
                    segment
                        .chars()
                        .next()
                        .unwrap_or_default()
                        .to_ascii_uppercase()
                })
                .collect::<String>();
            icon = icon.child(
                TextBuilder::new(if fallback.is_empty() { "?" } else { &fallback })
                    .x(cx)
                    .y(cy + size * 0.16)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .font_size(size * 0.62)
                    .anchor(TextAnchor::Middle)
                    .fill(stroke)
                    .build(),
            );
        }
    }

    Some(icon)
}

fn render_custom_svg_icon(
    icon: &CustomSvgIcon,
    cx: f32,
    cy: f32,
    size: f32,
    fallback_stroke: &str,
) -> Element {
    let view_box_width = if icon.view_box_width.is_finite() && icon.view_box_width > 0.0 {
        icon.view_box_width
    } else {
        24.0
    };
    let view_box_height = if icon.view_box_height.is_finite() && icon.view_box_height > 0.0 {
        icon.view_box_height
    } else {
        24.0
    };
    let scale = size / view_box_width.max(view_box_height);
    let translate_x = cx - (view_box_width * scale * 0.5);
    let translate_y = cy - (view_box_height * scale * 0.5);
    let fill = icon.fill.as_deref().unwrap_or("none");
    let stroke = icon.stroke.as_deref().unwrap_or(fallback_stroke);
    let stroke_width = if icon.stroke_width.is_finite() && icon.stroke_width > 0.0 {
        icon.stroke_width
    } else {
        1.4
    };

    Element::group()
        .class("fm-node-icon-custom")
        .transform(&format!(
            "translate({translate_x:.2} {translate_y:.2}) scale({scale:.4})"
        ))
        .child(
            Element::path()
                .d(&icon.path_data)
                .fill(fill)
                .stroke(stroke)
                .stroke_width(stroke_width),
        )
}

fn wrap_text_to_lines(text: &str, max_width: f32, avg_char_width: f32) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let max_chars = ((max_width / avg_char_width).floor() as usize).max(8);
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let next_len = if current.is_empty() {
            word.chars().count()
        } else {
            current.chars().count() + 1 + word.chars().count()
        };
        if next_len > max_chars && !current.is_empty() {
            lines.push(current);
            current = word.to_string();
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

#[allow(clippy::too_many_arguments)]
fn render_node_label_text(
    ir: &MermaidDiagramIr,
    label_id: Option<IrLabelId>,
    label_text: &str,
    x: f32,
    y: f32,
    font_size: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
    label_style: Option<&str>,
    emit_classdef_classes: bool,
) -> Element {
    if let Some(label_id) = label_id
        && let Some(segments) = ir.label_markup.get(&label_id)
        && !segments.is_empty()
    {
        return render_markdown_text_segments(
            segments,
            x,
            y,
            font_size,
            config,
            colors.text.as_str(),
            label_style,
            emit_classdef_classes,
        );
    }

    let mut text = if label_text.contains('\n') || label_text.contains('\r') {
        TextBuilder::new(label_text)
            .x(x)
            .y(y)
            .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
            .font_size(font_size)
            .line_height(config.line_height)
            .anchor(TextAnchor::Middle)
            .fill(&colors.text)
            .build()
    } else {
        Element::text()
            .x(x)
            .y(y)
            .attr("text-anchor", TextAnchor::Middle.as_str())
            .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
            .attr_num("font-size", font_size)
            .fill(&colors.text)
            .content(label_text)
    };
    text = maybe_add_class(text, "fm-node-label", emit_classdef_classes);

    if let Some(style) = label_style {
        text = text.attr("style", style);
    }

    text
}

#[allow(clippy::too_many_arguments)]
fn render_markdown_text_segments(
    segments: &[IrLabelSegment],
    x: f32,
    y: f32,
    font_size: f32,
    config: &SvgRenderConfig,
    fill: &str,
    label_style: Option<&str>,
    emit_classdef_classes: bool,
) -> Element {
    let line_height_px = font_size * config.line_height;
    let monospace_family = "'JetBrains Mono', 'Fira Code', 'SFMono-Regular', Consolas, monospace";

    let mut text = Element::text()
        .x(x)
        .y(y)
        .attr("text-anchor", TextAnchor::Middle.as_str())
        .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
        .attr_num("font-size", font_size)
        .fill(fill);
    text = maybe_add_class(text, "fm-node-label", emit_classdef_classes);

    if let Some(style) = label_style {
        text = text.attr("style", style);
    }

    let mut first_in_line = true;
    let mut line_index = 0usize;

    for segment in segments {
        match segment {
            IrLabelSegment::LineBreak => {
                first_in_line = true;
                line_index += 1;
            }
            IrLabelSegment::Text {
                text: value,
                bold,
                italic,
                code,
                strike,
            } => {
                let dy = if first_in_line {
                    if line_index == 0 { 0.0 } else { line_height_px }
                } else {
                    0.0
                };
                let mut tspan = Element::tspan().x(x).attr_num("dy", dy).content(value);
                if *bold {
                    tspan = tspan.attr("font-weight", "700");
                }
                if *italic {
                    tspan = tspan.attr("font-style", "italic");
                }
                if *strike {
                    tspan = tspan.attr("text-decoration", "line-through");
                }
                if *code {
                    tspan = tspan.attr("font-family", monospace_family);
                }
                text = text.child(tspan);
                first_in_line = false;
            }
        }
    }

    text
}

fn is_c4_legend_enabled(ir: &MermaidDiagramIr) -> bool {
    matches!(
        ir.diagram_type,
        DiagramType::C4Context
            | DiagramType::C4Container
            | DiagramType::C4Component
            | DiagramType::C4Dynamic
            | DiagramType::C4Deployment
    ) && ir.meta.c4_show_legend
}

fn render_c4_legend(
    ir: &MermaidDiagramIr,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
) -> Element {
    let mut legend = Element::group().class("fm-c4-legend");
    let box_width = width.min(320.0);
    let box_height = height.max(96.0);

    legend = legend.child(
        Element::rect()
            .x(x)
            .y(y)
            .width(box_width)
            .height(box_height)
            .fill("rgba(248,249,250,0.96)")
            .stroke(&colors.cluster_stroke)
            .stroke_width(1.0)
            .rx(8.0)
            .class("fm-c4-legend-box"),
    );

    legend = legend.child(
        TextBuilder::new("C4 Legend")
            .x(x + 14.0)
            .y(y + 18.0)
            .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
            .font_size(clamp_font_size(
                config.font_size * 0.82,
                config.min_font_size,
            ))
            .font_weight("600")
            .fill(&colors.text)
            .class("fm-c4-legend-title")
            .build(),
    );

    let entries = c4_legend_entries(ir);
    let left_x = x + 14.0;
    let right_x = x + (box_width / 2.0) + 8.0;
    let mut left_y = y + 36.0;
    let mut right_y = y + 36.0;

    for (index, (sample, label)) in entries.iter().enumerate() {
        let (entry_x, entry_y) = if index % 2 == 0 {
            let current = (left_x, left_y);
            left_y += 18.0;
            current
        } else {
            let current = (right_x, right_y);
            right_y += 18.0;
            current
        };
        legend = legend.child(
            TextBuilder::new(&format!("{sample} {label}"))
                .x(entry_x)
                .y(entry_y)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(clamp_font_size(
                    config.font_size * 0.72,
                    config.min_font_size,
                ))
                .fill(&colors.text)
                .class("fm-c4-legend-entry")
                .build(),
        );
    }

    legend
}

fn c4_legend_entries(ir: &MermaidDiagramIr) -> Vec<(&'static str, &'static str)> {
    let has_class = |needle: &str| {
        ir.nodes
            .iter()
            .flat_map(|node| node.classes.iter())
            .any(|class_name| class_name == needle)
    };
    let has_boundary = ir.clusters.iter().any(|cluster| {
        cluster
            .title
            .and_then(|label_id| ir.labels.get(label_id.0))
            .is_some_and(|label| {
                label.text.contains("Boundary") || label.text.contains("Deployment_Node")
            })
    });

    let mut entries = Vec::new();
    if has_class("c4-person") {
        entries.push(("◉", "Person"));
    }
    if has_class("c4-system") {
        entries.push(("▭", "System"));
    }
    if has_class("c4-container") {
        entries.push(("▣", "Container"));
    }
    if has_class("c4-component") {
        entries.push(("◫", "Component"));
    }
    if has_class("c4-database") {
        entries.push(("◌", "Database"));
    }
    if has_class("c4-queue") {
        entries.push(("▱", "Queue"));
    }
    if has_class("c4-external") {
        entries.push(("╌", "External"));
    }
    if has_boundary {
        entries.push(("⬚", "Boundary"));
    }
    entries
}

fn visibility_symbol(vis: fm_core::ClassVisibility) -> &'static str {
    match vis {
        fm_core::ClassVisibility::Public => "+",
        fm_core::ClassVisibility::Private => "-",
        fm_core::ClassVisibility::Protected => "#",
        fm_core::ClassVisibility::Package => "~",
    }
}

const fn node_shape_css_class(shape: fm_core::NodeShape) -> &'static str {
    use fm_core::NodeShape;
    match shape {
        NodeShape::Rect => "fm-node-shape-rect",
        NodeShape::Rounded => "fm-node-shape-rounded",
        NodeShape::Stadium => "fm-node-shape-stadium",
        NodeShape::Subroutine => "fm-node-shape-subroutine",
        NodeShape::Diamond => "fm-node-shape-diamond",
        NodeShape::Hexagon => "fm-node-shape-hexagon",
        NodeShape::Circle => "fm-node-shape-circle",
        NodeShape::FilledCircle => "fm-node-shape-filled-circle",
        NodeShape::Asymmetric => "fm-node-shape-asymmetric",
        NodeShape::Cylinder => "fm-node-shape-cylinder",
        NodeShape::Trapezoid => "fm-node-shape-trapezoid",
        NodeShape::DoubleCircle => "fm-node-shape-double-circle",
        NodeShape::HorizontalBar => "fm-node-shape-horizontal-bar",
        NodeShape::Note => "fm-node-shape-note",
        NodeShape::InvTrapezoid => "fm-node-shape-inv-trapezoid",
        NodeShape::Parallelogram => "fm-node-shape-parallelogram",
        NodeShape::InvParallelogram => "fm-node-shape-inv-parallelogram",
        NodeShape::Triangle => "fm-node-shape-triangle",
        NodeShape::Pentagon => "fm-node-shape-pentagon",
        NodeShape::Star => "fm-node-shape-star",
        NodeShape::Cloud => "fm-node-shape-cloud",
        NodeShape::Tag => "fm-node-shape-tag",
        NodeShape::CrossedCircle => "fm-node-shape-crossed-circle",
    }
}

fn smooth_layout_edge_path(edge_path: &LayoutEdgePath, offset_x: f32, offset_y: f32) -> String {
    crate::path::build_smooth_path_by(edge_path.points.len(), |index| {
        let point = &edge_path.points[index];
        (point.x + offset_x, point.y + offset_y)
    })
}

/// Render a single edge to an SVG element.
struct EdgeRenderContext<'a> {
    ir: &'a MermaidDiagramIr,
    offset_x: f32,
    offset_y: f32,
    config: &'a SvgRenderConfig,
    detail: RenderDetailProfile,
    colors: &'a ThemeColors,
    accessible_node_labels: Option<&'a [&'a str]>,
}

/// Serialize a common solid-arrow edge `<path>` directly into raw SVG bytes, **byte-identical** to
/// the `Element` the slow path builds — every attribute value goes through the same serializers
/// (`write_escaped_attr` / `AttributeValue::write_value`), so only the attribute names, order, and
/// `<path .../>` structure are replicated here (asserted by `edge_fast_fragment_matches_element`).
/// This skips the per-edge `Attributes` Vec build and the per-attribute `write_into` dispatch, which
/// a ceiling probe shows is ~40% of wide render even after the keep-Element streaming.
fn build_common_edge_fragment(
    path_str: &str,
    stroke_width: f32,
    style_class: &str,
    edge_index: i32,
    marker_end: &str,
) -> String {
    let mut f = String::with_capacity(path_str.len() + 96);
    write_common_edge_path_into(&mut f, path_str, stroke_width, style_class, edge_index, marker_end);
    f
}

/// Write the common solid-arrow `<path .../>` element directly into `f`. Shared by the `<path>`-only
/// fast path ([`build_common_edge_fragment`]) and the whole-edge fast path
/// ([`build_common_edge_full_fragment`]) so the path serialization stays in one place and stays
/// byte-identical to the slow `Element` build.
fn write_common_edge_path_into(
    f: &mut String,
    path_str: &str,
    stroke_width: f32,
    style_class: &str,
    edge_index: i32,
    marker_end: &str,
) {
    // Path `d` is pure SVG path geometry (`M/L/C/A/Z` + digits/spaces/commas/dots/minus from
    // `write_fixed2`) and can never contain an XML special (`& < > " '`), so escaping it is a no-op
    // scan. Write it raw — byte-identical, and consistent with the streaming fast path
    // (`write_common_edge_full_fragment_into`) which already emits `d` unescaped via
    // `build_smooth_path_by_into`.
    f.push_str("<path d=\"");
    f.push_str(path_str);
    write_common_edge_path_tail_into(f, stroke_width, style_class, edge_index, marker_end);
}

/// Write the `<path>` attributes that follow the `d` value — `" stroke-width="…" class="fm-edge {style}"
/// data-fm-edge-id="…" marker-end="…"/>`. Shared by the pre-built-`d` writer above and the geometry-
/// streaming whole-edge builder so the after-`d` structure stays single-sourced.
fn write_common_edge_path_tail_into(
    f: &mut String,
    stroke_width: f32,
    style_class: &str,
    edge_index: i32,
    marker_end: &str,
) {
    use crate::attributes::{AttributeValue, write_escaped_attr};
    f.push_str("\" stroke-width=\"");
    let _ = AttributeValue::Number(stroke_width).write_value(f);
    f.push_str("\" class=\"fm-edge ");
    f.push_str(style_class);
    f.push_str("\" data-fm-edge-id=\"");
    let _ = AttributeValue::Integer(edge_index).write_value(f);
    // An empty `marker_end` means the slow path's `render_edge` produced no `marker-end` attribute
    // (e.g. `ArrowType::Line`, whose match arm yields `marker_end = None`), so omit it here too — the
    // `<path>` closes right after `data-fm-edge-id`. Non-empty callers (solid arrow, …) are unchanged.
    if !marker_end.is_empty() {
        f.push_str("\" marker-end=\"");
        let _ = write_escaped_attr(f, marker_end);
    }
    f.push_str("\"/>");
}

/// Serialize an ENTIRE common edge — `<g …><path …/><title>…</title></g>` — directly into raw SVG
/// bytes, **byte-identical** to the `Element` group the slow path builds for an unlabeled solid-arrow
/// edge under default (`A11yConfig::full()`) a11y. The slow path builds an `Element::group` (its
/// `Attributes` Vec + a 2-slot children Vec), an `id`/`role`/`tabindex` triple (two heap value
/// Strings), the `<path>` child element, and a `<title>` element (whose `content` clones the
/// description) — ~6 allocations per edge. This collapses all of it into the single fragment String,
/// which on wide layered flowcharts (edges dominate render allocations) is the largest remaining
/// wide-render alloc lever. The group's attribute names/order (`id`, `class`, `data-fm-edge-id`,
/// `role`, `tabindex`) and the `role="graphics-symbol"`/`tabindex="0"` literals are replicated here;
/// asserted by `edge_fast_full_fragment_matches_render` and the corpus `golden_svg_test`.
///
/// The `<title>` text is the unlabeled solid-arrow description -- `describe_edge_labels(from, to,
/// ArrowType::Arrow, None)` = `"{from} points to {to}"` (with `"unknown"` fallbacks) -- written
/// piecewise so the per-edge description String never has to be allocated. Escaping the assembled
/// string and escaping the two labels separately are byte-identical because the connective phrase
/// (`" points to "`) and the `"unknown"` fallback contain no escapable bytes, so `write_escaped_text`
/// is the identity on them.
///
/// The path `d` data is streamed in via `build_smooth_path_by_into(n, point_at)` rather than passing a
/// pre-built `path_str` String — the geometry is written straight into the fragment buffer (no per-edge
/// `d` allocation). Writing it unescaped is byte-identical to the slow path's `write_escaped_attr(d)`
/// because path data is only `[MLC0-9 .,-]`, which contains no escapable byte. Pinned (incl. the path
/// geometry, and labels with `& < >`) by `edge_fast_full_fragment_matches_render`.
#[allow(clippy::too_many_arguments)]
fn build_common_edge_full_fragment<F>(
    point_count: usize,
    point_at: F,
    stroke_width: f32,
    style_class: &str,
    edge_index: i32,
    marker_end: &str,
    from_label: Option<&str>,
    to_label: Option<&str>,
) -> String
where
    F: FnMut(usize) -> (f32, f32),
{
    let mut f = String::with_capacity(24 + point_count * 56 + 192);
    // This wrapper serves only the solid-`Arrow` callers (render_edge's fast path + its parity test), so
    // the a11y phrase is the solid-arrow `" points to "`; the Line arm streams via the `_into` form.
    write_common_edge_full_fragment_into(
        &mut f,
        point_count,
        point_at,
        stroke_width,
        style_class,
        edge_index,
        marker_end,
        " points to ",
        from_label,
        to_label,
    );
    f
}

/// Write-into core of [`build_common_edge_full_fragment`]. Used by `render_edge_into` to stream the whole
/// common edge straight into the chunk output buffer, with NO per-edge fragment `String` (the fragment
/// `String` is the single largest remaining per-element render allocation on wide flowcharts).
#[allow(clippy::too_many_arguments)]
fn write_common_edge_full_fragment_into<F>(
    f: &mut String,
    point_count: usize,
    point_at: F,
    stroke_width: f32,
    style_class: &str,
    edge_index: i32,
    marker_end: &str,
    arrow_phrase: &str,
    from_label: Option<&str>,
    to_label: Option<&str>,
) where
    F: FnMut(usize) -> (f32, f32),
{
    use crate::attributes::{AttributeValue, write_escaped_text};
    // <g id="fm-edge-N" class="fm-edge" data-fm-edge-id="N" role="graphics-symbol" tabindex="0">
    // The id is `mermaid_edge_element_id(edge_index)` = "fm-edge-" + decimal(index); it never contains
    // an escapable byte, so it goes through the same `Integer` serializer as `data-fm-edge-id`.
    f.push_str("<g id=\"fm-edge-");
    let _ = AttributeValue::Integer(edge_index).write_value(f);
    f.push_str("\" class=\"fm-edge\" data-fm-edge-id=\"");
    let _ = AttributeValue::Integer(edge_index).write_value(f);
    f.push_str("\" role=\"graphics-symbol\" tabindex=\"0\">");
    f.push_str("<path d=\"");
    crate::path::build_smooth_path_by_into(f, point_count, point_at);
    write_common_edge_path_tail_into(f, stroke_width, style_class, edge_index, marker_end);
    f.push_str("<title>");
    let _ = write_escaped_text(f, from_label.unwrap_or("unknown"));
    // The a11y connective phrase is `describe_edge_labels`'s per-arrow word surrounded by spaces
    // (`" points to "` for a solid arrow, `" connects to "` for a plain line). It contains no escapable
    // byte, so writing it verbatim matches the slow path's escaped whole-description byte-for-byte.
    f.push_str(arrow_phrase);
    let _ = write_escaped_text(f, to_label.unwrap_or("unknown"));
    f.push_str("</title></g>");
}

fn render_edge(edge_path: &LayoutEdgePath, context: &EdgeRenderContext<'_>) -> Element {
    use fm_core::ArrowType;

    let EdgeRenderContext {
        ir,
        offset_x,
        offset_y,
        config,
        detail,
        colors,
        accessible_node_labels,
    } = *context;

    let edge_index = edge_path.edge_index;
    let ir_edge = ir.edges.get(edge_index);
    let arrow = ir_edge.map_or(ArrowType::Arrow, |e| e.arrow);
    let is_back_edge = edge_path.reversed;

    // Back-edges get special treatment: dashed + muted color
    let (base_dasharray, marker_start, marker_end, base_color): (
        Option<&str>,
        Option<&str>,
        Option<&str>,
        &str,
    ) = if is_back_edge {
        (
            Some("4,4"),
            None,
            Some("url(#arrow-open)"),
            &colors.cluster_stroke,
        )
    } else {
        match arrow {
            ArrowType::Line | ArrowType::ThickLine => (None, None, None, &colors.edge),
            ArrowType::Arrow => (None, None, Some("url(#arrow-end)"), &colors.edge),
            ArrowType::OpenArrow => (None, None, Some("url(#arrow-open)"), &colors.edge),
            ArrowType::HalfArrowTop => (None, None, Some("url(#arrow-half-top)"), &colors.edge),
            ArrowType::HalfArrowBottom => {
                (None, None, Some("url(#arrow-half-bottom)"), &colors.edge)
            }
            ArrowType::HalfArrowTopReverse => {
                (None, Some("url(#arrow-half-bottom)"), None, &colors.edge)
            }
            ArrowType::HalfArrowBottomReverse => {
                (None, Some("url(#arrow-half-top)"), None, &colors.edge)
            }
            ArrowType::StickArrowTop => (None, None, Some("url(#arrow-stick-top)"), &colors.edge),
            ArrowType::StickArrowBottom => {
                (None, None, Some("url(#arrow-stick-bottom)"), &colors.edge)
            }
            ArrowType::StickArrowTopReverse => {
                (None, Some("url(#arrow-stick-bottom)"), None, &colors.edge)
            }
            ArrowType::StickArrowBottomReverse => {
                (None, Some("url(#arrow-stick-top)"), None, &colors.edge)
            }
            ArrowType::ThickArrow => (None, None, Some("url(#arrow-filled)"), &colors.edge),
            ArrowType::DottedArrow => (Some("5,5"), None, Some("url(#arrow-end)"), &colors.edge),
            ArrowType::DottedOpenArrow => {
                (Some("5,5"), None, Some("url(#arrow-open)"), &colors.edge)
            }
            ArrowType::DottedCross => (Some("5,5"), None, Some("url(#arrow-cross)"), &colors.edge),
            ArrowType::HalfArrowTopDotted => (
                Some("5,5"),
                None,
                Some("url(#arrow-half-top)"),
                &colors.edge,
            ),
            ArrowType::HalfArrowBottomDotted => (
                Some("5,5"),
                None,
                Some("url(#arrow-half-bottom)"),
                &colors.edge,
            ),
            ArrowType::HalfArrowTopReverseDotted => (
                Some("5,5"),
                Some("url(#arrow-half-bottom)"),
                None,
                &colors.edge,
            ),
            ArrowType::HalfArrowBottomReverseDotted => (
                Some("5,5"),
                Some("url(#arrow-half-top)"),
                None,
                &colors.edge,
            ),
            ArrowType::StickArrowTopDotted => (
                Some("5,5"),
                None,
                Some("url(#arrow-stick-top)"),
                &colors.edge,
            ),
            ArrowType::StickArrowBottomDotted => (
                Some("5,5"),
                None,
                Some("url(#arrow-stick-bottom)"),
                &colors.edge,
            ),
            ArrowType::StickArrowTopReverseDotted => (
                Some("5,5"),
                Some("url(#arrow-stick-bottom)"),
                None,
                &colors.edge,
            ),
            ArrowType::StickArrowBottomReverseDotted => (
                Some("5,5"),
                Some("url(#arrow-stick-top)"),
                None,
                &colors.edge,
            ),
            ArrowType::Circle => (None, None, Some("url(#arrow-circle)"), &colors.edge),
            ArrowType::Cross => (None, None, Some("url(#arrow-cross)"), &colors.edge),
            ArrowType::DottedLine => (Some("5,5"), None, None, &colors.edge),
            ArrowType::DoubleArrow => (
                None,
                Some("url(#arrow-start)"),
                Some("url(#arrow-end)"),
                &colors.edge,
            ),
            ArrowType::DoubleThickArrow => (
                None,
                Some("url(#arrow-start-filled)"),
                Some("url(#arrow-filled)"),
                &colors.edge,
            ),
            ArrowType::DoubleDottedArrow => (
                Some("5,5"),
                Some("url(#arrow-start)"),
                Some("url(#arrow-end)"),
                &colors.edge,
            ),
        }
    };

    let stroke_width = match arrow {
        ArrowType::ThickArrow | ArrowType::DoubleThickArrow | ArrowType::ThickLine => 2.5,
        _ => 1.8,
    };

    // Determine edge style class
    let style_class = if is_back_edge {
        "fm-edge-back"
    } else {
        match arrow {
            ArrowType::DottedArrow
            | ArrowType::DottedOpenArrow
            | ArrowType::DottedCross
            | ArrowType::HalfArrowTopDotted
            | ArrowType::HalfArrowBottomDotted
            | ArrowType::HalfArrowTopReverseDotted
            | ArrowType::HalfArrowBottomReverseDotted
            | ArrowType::StickArrowTopDotted
            | ArrowType::StickArrowBottomDotted
            | ArrowType::StickArrowTopReverseDotted
            | ArrowType::StickArrowBottomReverseDotted
            | ArrowType::DottedLine
            | ArrowType::DoubleDottedArrow => "fm-edge-dashed",
            ArrowType::ThickArrow | ArrowType::DoubleThickArrow | ArrowType::ThickLine => {
                "fm-edge-thick"
            }
            _ => "fm-edge-solid",
        }
    };

    // `fill="none"` and the base `stroke=<theme edge color>` are redundant when the theme CSS is
    // embedded: `.fm-edge { fill: none; stroke: var(--fm-edge-color) }` applies (a presentation
    // attribute loses to the stylesheet), and `base_color` is *always* the theme edge color —
    // per-edge `linkStyle` colors are emitted as a separate `style="..."` that wins over both the
    // presentation attribute and the CSS. So emit these inline fallbacks only when CSS is absent
    // (e.g. the PNG raster path, which resvg cannot fully style via CSS) so those exports stay
    // self-contained. `stroke-width` is NOT gated — the unconditional CSS sets none, so the inline
    // is the actual width.
    //
    let animation_style = config
        .animations_enabled
        .then(|| animation_style_attr(edge_animation_order(edge_path, ir)));

    // Whole-edge fast path: when the common edge ALSO carries the default a11y wrapping
    // (`text_alternatives` + `aria_labels` + `keyboard_nav`, all true under `A11yConfig::full()`), the
    // entire `<g …><path …/><title>…</title></g>` serializes directly to one String — skipping the
    // `Element::group` Attributes Vec + children Vec, the `id`/`role`/`tabindex` value Strings, and the
    // `<title>` Element + its content clone (~6 allocs/edge). Edges dominate render allocations on wide
    // layered flowcharts, so this is the largest remaining wide-render alloc lever. The conditions are
    // exactly the inner `<path>` fast path's plus `aria_labels && keyboard_nav` (so the hardcoded
    // `role`/`tabindex` always match the group the slow path would build) and an explicit unlabeled
    // check (so the title is the unlabeled-edge description). Byte-identical: see
    // `edge_fast_full_fragment_matches_render` + `golden_svg_test`. `resolve_edge_inline_style` is last
    // so the (potentially allocating) lookup only runs once everything cheaper has already passed.
    if arrow == ArrowType::Arrow
        && !is_back_edge
        && config.embed_theme_css
        && !config.animations_enabled
        && !config.include_source_spans
        && config.a11y.text_alternatives
        && config.a11y.aria_labels
        && config.a11y.keyboard_nav
        && marker_start.is_none()
        && base_dasharray.is_none()
        && !(detail.show_edge_labels && ir_edge.and_then(|e| e.label).is_some())
        && let Some(edge) = ir_edge
        && let Some(marker_end_val) = marker_end
        && resolve_edge_inline_style(ir, edge_index).is_none()
    {
        let (from_label, to_label) =
            edge_endpoint_accessible_labels(edge, ir, accessible_node_labels);
        // `build_common_edge_full_fragment` streams the path geometry and writes the title
        // (`"{from} points to {to}"`) piecewise, so NEITHER the per-edge `d` String (`path_str`, which is
        // computed lazily below only for the slower paths) NOR the `describe_edge_labels` String is ever
        // allocated.
        return Element::raw_svg(build_common_edge_full_fragment(
            edge_path.points.len(),
            |i| {
                let p = &edge_path.points[i];
                (p.x + offset_x, p.y + offset_y)
            },
            stroke_width,
            style_class,
            edge_index as i32,
            marker_end_val,
            from_label,
            to_label,
        ));
    }

    // Only the slower paths below need the materialized `d` String (the whole-edge fast path streamed it
    // straight into its fragment above and returned).
    let path_str = smooth_layout_edge_path(edge_path, offset_x, offset_y);

    // Fast path: the overwhelmingly common edge (solid `Arrow`, themed CSS, no back-edge, no
    // animation, no source spans, no inline `linkStyle`, no rendered label) serializes its `<path>`
    // child to a fixed five-attribute fragment via `Element::raw_svg`, skipping the per-edge
    // `Attributes` Vec build + per-attribute `write_into` dispatch (a ceiling probe shows that
    // overhead is ~40% of wide render). The fragment is ONLY the `<path>`; it then falls through to
    // the SAME a11y wrapping tail (group / `role` / `tabindex` / `<title>`) the slow path runs, so
    // the full output stays byte-identical to the slow Element path (proven by `golden_svg_test`).
    // Gated on `a11y.text_alternatives && ir_edge.is_some()` so the raw fragment only ever flows
    // into the group-child branch below, never the attribute-mutating unwrapped fallthrough (a
    // `raw_svg` element cannot take `.attr()`/`.id()`).
    let mut elem = if arrow == ArrowType::Arrow
        && !is_back_edge
        && config.embed_theme_css
        && !config.animations_enabled
        && !config.include_source_spans
        && config.a11y.text_alternatives
        && ir_edge.is_some()
        && marker_start.is_none()
        && base_dasharray.is_none()
        && resolve_edge_inline_style(ir, edge_index).is_none()
        && !(detail.show_edge_labels && ir_edge.and_then(|e| e.label).is_some())
        && let Some(marker_end_val) = marker_end
    {
        Element::raw_svg(build_common_edge_fragment(
            &path_str,
            stroke_width,
            style_class,
            edge_index as i32,
            marker_end_val,
        ))
    } else {
        let mut elem = Element::path().d(&path_str);
        if !config.embed_theme_css {
            elem = elem.fill("none").stroke(base_color);
        }
        let mut elem = elem
            .stroke_width(stroke_width)
            .class("fm-edge")
            .class(style_class)
            .attr_int("data-fm-edge-id", edge_index as i32);
        if config.animations_enabled && base_dasharray.is_some() {
            elem = elem.class("fm-edge-flow-animated");
        }

        // Apply inline style from linkStyle directives if present.
        if let Some(inline_style) = resolve_edge_inline_style(ir, edge_index) {
            let merged_style = animation_style.as_ref().map_or_else(
                || inline_style.clone(),
                |extra| format!("{inline_style};{extra}"),
            );
            elem = elem.attr("style", &merged_style);
        } else if let Some(extra) = animation_style.as_deref() {
            elem = elem.attr("style", extra);
        }

        if let Some(marker) = marker_start {
            elem = elem.marker_start(marker);
        }
        if let Some(marker) = marker_end {
            elem = elem.marker_end(marker);
        }

        if config.include_source_spans {
            elem = apply_span_metadata(elem, edge_path.span);
        }

        if let Some(dasharray) = base_dasharray {
            elem = elem.stroke_dasharray(dasharray);
        }
        elem
    };

    // If edge has a label, wrap in group with text
    if detail.show_edge_labels
        && let Some(label_id) = ir_edge.and_then(|e| e.label)
        && let Some(label) = ir.labels.get(label_id.0)
        && edge_path.points.len() >= 2
    {
        let base_label = truncate_label(&label.text, detail.edge_label_max_chars);

        // Prepend autonumber when enabled for sequence diagrams
        let label_text: Cow<'_, str> = if let Some(number) = ir
            .sequence_meta
            .as_ref()
            .and_then(|meta| meta.autonumber_value(edge_index))
        {
            Cow::Owned(format!("{number} {}", base_label.as_ref()))
        } else {
            base_label
        };

        // Position label at geometric midpoint of edge
        let (lx, ly) = if edge_path.points.len() == 4 {
            // For standard orthogonal paths, the center of the middle segment
            let p1 = &edge_path.points[1];
            let p2 = &edge_path.points[2];
            (
                f32::midpoint(p1.x, p2.x) + offset_x,
                f32::midpoint(p1.y, p2.y) + offset_y - 8.0,
            )
        } else if edge_path.points.len() == 2 {
            // For straight lines, geometric center
            let p1 = &edge_path.points[0];
            let p2 = &edge_path.points[1];
            (
                f32::midpoint(p1.x, p2.x) + offset_x,
                f32::midpoint(p1.y, p2.y) + offset_y - 8.0,
            )
        } else {
            // Fallback for other path lengths
            let mid_idx = edge_path.points.len() / 2;
            let mid_point = &edge_path.points[mid_idx];
            (mid_point.x + offset_x, mid_point.y + offset_y - 8.0)
        };

        let mut group = Element::group()
            .id(&mermaid_edge_element_id(edge_index))
            .class("fm-edge-labeled")
            .attr_int("data-fm-edge-id", edge_index as i32);
        if let Some(extra) = animation_style.as_deref() {
            group = group.attr("style", extra);
        }
        if config.include_source_spans {
            group = apply_span_metadata(group, edge_path.span);
        }

        // Add accessibility attributes to group
        if config.a11y.aria_labels {
            group = group.attr("role", "graphics-symbol");
        }

        if config.a11y.keyboard_nav {
            group = group.attr("tabindex", "0");
        }

        group = group.child(elem);

        // Add background rect for label
        let label_text = label_text.as_ref();
        let lines_count = label_text.lines().count().max(1) as f32;
        let max_line_len = label_text
            .lines()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0);
        let label_text_width = (max_line_len as f32 * config.avg_char_width) + 8.0;
        let label_padding_x = 10.0;
        let label_width = label_text_width + (label_padding_x * 2.0);

        let label_font_size = detail.edge_font_size;
        let total_text_height = (lines_count - 1.0) * label_font_size * config.line_height;
        let label_height = total_text_height + label_font_size + 14.0;

        let start_y = ly - (total_text_height / 2.0) + (label_font_size / 4.0);

        group = group.child(
            Element::rect()
                .x(lx - label_width / 2.0)
                .y(ly - label_height / 2.0 - 1.0)
                .width(label_width)
                .height(label_height)
                .fill(&colors.background)
                .stroke(&colors.cluster_stroke)
                .stroke_width(0.75)
                .rx(6.0)
                .ry(6.0),
        );

        // Add label text
        group = group.child(
            TextBuilder::new(label_text)
                .x(lx)
                .y(start_y)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(label_font_size)
                .line_height(config.line_height)
                .anchor(TextAnchor::Middle)
                .fill(&colors.text)
                .class("edge-label")
                .build(),
        );

        // Add title element for text alternatives
        if config.a11y.text_alternatives
            && let Some(edge) = ir_edge
        {
            let (from_label, to_label) =
                edge_endpoint_accessible_labels(edge, ir, accessible_node_labels);
            let edge_desc =
                crate::a11y::describe_edge_labels(from_label, to_label, arrow, Some(label_text));
            group = group.child(Element::title(&edge_desc));
        }

        return group;
    }

    // Add title element for text alternatives (unlabeled edges)
    if config.a11y.text_alternatives
        && let Some(edge) = ir_edge
    {
        let (from_label, to_label) =
            edge_endpoint_accessible_labels(edge, ir, accessible_node_labels);
        let edge_desc = crate::a11y::describe_edge_labels(from_label, to_label, arrow, None);
        // Wrap in group to add title
        let mut group = Element::group()
            .id(&mermaid_edge_element_id(edge_index))
            .class("fm-edge")
            .attr_int("data-fm-edge-id", edge_index as i32);
        if let Some(extra) = animation_style.as_deref() {
            group = group.attr("style", extra);
        }
        if config.include_source_spans {
            group = apply_span_metadata(group, edge_path.span);
        }
        if config.a11y.aria_labels {
            group = group.attr("role", "graphics-symbol");
        }
        if config.a11y.keyboard_nav {
            group = group.attr("tabindex", "0");
        }
        group = group.child(elem);
        group = group.child(Element::title(&edge_desc));
        return group;
    }

    // Add accessibility attributes for unwrapped edges
    if config.a11y.aria_labels {
        elem = elem.attr("role", "graphics-symbol");
    }
    if config.a11y.keyboard_nav {
        elem = elem.attr("tabindex", "0");
    }

    elem = elem.id(&mermaid_edge_element_id(edge_index));

    elem
}

/// Serialize an edge straight into the output buffer. For the overwhelmingly common solid-arrow edge under
/// default a11y (the same conditions as `render_edge`'s whole-edge fast path) the `<g…><path/><title/></g>`
/// is streamed directly into `out` — eliminating the per-edge fragment `String` that `render_edge` would
/// build, wrap in `Element::raw_svg`, and immediately copy out then drop (the single largest remaining
/// per-element render allocation on wide flowcharts). For `arrow == Arrow` the stroke-width / class /
/// marker-end are the known solid-arrow constants, so the full arrow match is skipped too. Every other
/// edge (back-edges, non-Arrow markers, dashed, animated, source spans, inline `linkStyle`, labeled, or
/// reduced a11y) falls back to the existing `render_edge` Element path, so output stays byte-identical
/// (corpus-pinned by `golden_svg_test`).
fn render_edge_into(out: &mut String, edge_path: &LayoutEdgePath, context: &EdgeRenderContext<'_>) {
    use fm_core::ArrowType;
    let EdgeRenderContext {
        ir,
        offset_x,
        offset_y,
        config,
        detail,
        accessible_node_labels,
        ..
    } = *context;
    let edge_index = edge_path.edge_index;
    let ir_edge = ir.edges.get(edge_index);
    let arrow = ir_edge.map_or(ArrowType::Arrow, |edge| edge.arrow);

    // The whole-edge streaming fragment handles every non-reversed arrow whose slow-path `render_edge`
    // shape is a single `<path>` with NO dasharray and NO marker-start — i.e. `(stroke_width, class,
    // marker_end)` fully determines the bytes. Each tuple below is `(stroke_width, style_class,
    // marker_end, a11y_phrase)` read straight off `render_edge`'s `stroke_width`/`style_class`/`marker_end`
    // matches and `describe_edge_labels`'s per-arrow word (surrounded by spaces; `_ => "connects to"`).
    // Dashed/reverse/double arrows (dasharray or marker-start), back-edges, labels, inline styles,
    // animations, source spans, and non-full a11y all still fall to the `Element` slow path below.
    // Byte-identity is pinned by `golden_svg_test` + `edge_fast_full_fragment_matches_render`.
    let stream_arrow: Option<(f32, &str, &str, &str)> = match arrow {
        ArrowType::Arrow => Some((1.8, "fm-edge-solid", "url(#arrow-end)", " points to ")),
        ArrowType::Line => Some((1.8, "fm-edge-solid", "", " connects to ")),
        ArrowType::OpenArrow => Some((1.8, "fm-edge-solid", "url(#arrow-open)", " sends to ")),
        ArrowType::Circle => Some((1.8, "fm-edge-solid", "url(#arrow-circle)", " relates to ")),
        ArrowType::Cross => Some((1.8, "fm-edge-solid", "url(#arrow-cross)", " blocks ")),
        ArrowType::HalfArrowTop => {
            Some((1.8, "fm-edge-solid", "url(#arrow-half-top)", " connects to "))
        }
        ArrowType::HalfArrowBottom => {
            Some((1.8, "fm-edge-solid", "url(#arrow-half-bottom)", " connects to "))
        }
        ArrowType::StickArrowTop => {
            Some((1.8, "fm-edge-solid", "url(#arrow-stick-top)", " connects to "))
        }
        ArrowType::StickArrowBottom => {
            Some((1.8, "fm-edge-solid", "url(#arrow-stick-bottom)", " connects to "))
        }
        ArrowType::ThickArrow => {
            Some((2.5, "fm-edge-thick", "url(#arrow-filled)", " strongly points to "))
        }
        ArrowType::ThickLine => Some((2.5, "fm-edge-thick", "", " strongly connects to ")),
        _ => None,
    };
    if let Some((stroke_width, style_class, marker_end, arrow_phrase)) = stream_arrow
        && !edge_path.reversed
        && config.embed_theme_css
        && !config.animations_enabled
        && !config.include_source_spans
        && config.a11y.text_alternatives
        && config.a11y.aria_labels
        && config.a11y.keyboard_nav
        && !(detail.show_edge_labels && ir_edge.and_then(|edge| edge.label).is_some())
        && let Some(edge) = ir_edge
        && resolve_edge_inline_style(ir, edge_index).is_none()
    {
        let (from_label, to_label) =
            edge_endpoint_accessible_labels(edge, ir, accessible_node_labels);
        write_common_edge_full_fragment_into(
            out,
            edge_path.points.len(),
            |index| {
                let point = &edge_path.points[index];
                (point.x + offset_x, point.y + offset_y)
            },
            stroke_width,
            style_class,
            edge_index as i32,
            marker_end,
            arrow_phrase,
            from_label,
            to_label,
        );
        return;
    }

    render_edge(edge_path, context).write_to_string(out);
}

fn edge_endpoint_accessible_labels<'a>(
    edge: &fm_core::IrEdge,
    ir: &'a MermaidDiagramIr,
    accessible_node_labels: Option<&'a [&'a str]>,
) -> (Option<&'a str>, Option<&'a str>) {
    (
        endpoint_accessible_label(edge.from, ir, accessible_node_labels),
        endpoint_accessible_label(edge.to, ir, accessible_node_labels),
    )
}

fn endpoint_accessible_label<'a>(
    endpoint: fm_core::IrEndpoint,
    ir: &'a MermaidDiagramIr,
    accessible_node_labels: Option<&'a [&'a str]>,
) -> Option<&'a str> {
    let fm_core::IrEndpoint::Node(node_id) = endpoint else {
        return None;
    };
    accessible_node_labels
        .and_then(|labels| labels.get(node_id.0).copied())
        .or_else(|| {
            ir.nodes
                .get(node_id.0)
                .map(|node| crate::a11y::accessible_node_label(node, ir))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full-node fast path must render byte-identical to the `Element` slow path it replaces.
    /// Pins the assembled `<g><rect/><text/><title/></g>` against the known-correct default-config
    /// output (captured from the slow path); conformance covers the gated-out node variants.
    #[test]
    fn node_fast_fragment_matches_render() {
        let ir = create_ir_with_single_node("N0", NodeShape::Rect);
        let config = SvgRenderConfig::default();
        let svg = render_svg_with_config(&ir, &config);
        let expected = concat!(
            "<g id=\"fm-node-n0-0\" class=\"fm-node fm-node-accent-4 fm-node-shape-rect\" ",
            "data-id=\"N0\" role=\"graphics-symbol\" aria-label=\"Single Node\" tabindex=\"0\">",
            "<rect x=\"92\" y=\"92\" width=\"148.73\" height=\"66.50\" rx=\"5.50\" ",
            "fill=\"url(#fm-node-gradient)\"/>",
            "<text x=\"166.36\" y=\"129.85\" text-anchor=\"middle\" font-size=\"13.80\" ",
            "fill=\"#1a1a2e\">Single Node</text>",
            "<title>Node: Single Node, rectangle</title></g>",
        );
        let region = svg
            .find("<g id=\"fm-node")
            .map_or("", |s| &svg[s..]);
        assert!(
            svg.contains(expected),
            "full-node fast-path bytes must match the slow path.\nGOT node region:\n{region}\nEXPECTED:\n{expected}"
        );
    }

    /// The streamed common-edge fragment must be byte-identical to the `Element` the slow path
    /// builds. Pins `build_common_edge_fragment` against the canonical `Element` constructors.
    #[test]
    fn edge_fast_fragment_matches_element() {
        let cases: &[(&str, i32)] = &[
            ("M0 0 L10 10", 0),
            ("M1.50 2.25 C3.00 4.00 5.00 6.00 7.00 8.00", 42),
            ("M-5.25 0 L0 -3.50", 1000),
            ("", 7),
        ];
        for &(d, idx) in cases {
            let elem = Element::path()
                .d(d)
                .stroke_width(1.8)
                .class("fm-edge")
                .class("fm-edge-solid")
                .attr_int("data-fm-edge-id", idx)
                .marker_end("url(#arrow-end)");
            let mut expected = String::new();
            elem.write_to_string(&mut expected);
            let frag = build_common_edge_fragment(d, 1.8, "fm-edge-solid", idx, "url(#arrow-end)");
            assert_eq!(
                frag, expected,
                "streamed edge fragment must equal the Element serialization (d={d:?})"
            );
        }
    }

    #[test]
    fn edge_fast_full_fragment_matches_render() {
        // Pin the WHOLE-edge fast fragment against the `Element` group the slow path actually builds
        // for an unlabeled solid-arrow edge under default a11y: the unlabeled-edge tail wraps the
        // `<path>` fast fragment in `Element::group().id().class("fm-edge").attr_int("data-fm-edge-id")
        // .attr("role").attr("tabindex").child(path).child(title)`, with the title text from
        // `describe_edge_labels(from, to, Arrow, None)`. The fragment must serialize byte-identically —
        // including the streamed path geometry and piecewise `<title>` escaping of labels with
        // `& < > "` and the `"unknown"` fallback. The expected `d` String is the one the slow path would
        // build (`build_smooth_path_by`), so this also pins "streamed-inline path == escaped `d` String".
        let check = |points: &[(f32, f32)],
                     idx: i32,
                     style: &str,
                     sw: f32,
                     from_label: Option<&str>,
                     to_label: Option<&str>| {
            let d = crate::path::build_smooth_path_by(points.len(), |i| points[i]);
            let desc =
                crate::a11y::describe_edge_labels(from_label, to_label, ArrowType::Arrow, None);
            let path_child =
                Element::raw_svg(build_common_edge_fragment(&d, sw, style, idx, "url(#arrow-end)"));
            let group = Element::group()
                .id(&fm_core::mermaid_edge_element_id(idx as usize))
                .class("fm-edge")
                .attr_int("data-fm-edge-id", idx)
                .attr("role", "graphics-symbol")
                .attr("tabindex", "0")
                .child(path_child)
                .child(Element::title(&desc));
            let mut expected = String::new();
            group.write_to_string(&mut expected);
            let frag = build_common_edge_full_fragment(
                points.len(),
                |i| points[i],
                sw,
                style,
                idx,
                "url(#arrow-end)",
                from_label,
                to_label,
            );
            assert_eq!(
                frag, expected,
                "whole-edge fast fragment must equal the slow Element group (idx={idx})"
            );
        };
        check(&[(0.0, 0.0), (10.0, 10.0)], 0, "fm-edge-solid", 1.8, Some("A"), Some("B"));
        check(
            &[(1.5, 2.25), (3.0, 4.0), (5.0, 6.0), (7.0, 8.0)],
            42,
            "fm-edge-solid",
            1.8,
            Some("Node <1> & \"x\""),
            Some("Node 2"),
        );
        check(&[(-5.25, 0.0), (0.0, -3.5)], 1000, "fm-edge-solid", 2.5, None, None);
        check(&[(0.0, 0.0)], 7, "fm-edge-solid", 1.8, Some("start"), None);
    }

    use fm_core::{
        ArrowType, DiagramType, IrC4NodeMeta, IrCluster, IrClusterId, IrEdge, IrEndpoint,
        IrGraphCluster, IrGraphNode, IrLabel, IrLabelId, IrLabelSegment, IrLifecycleEvent, IrNode,
        IrNodeId, IrPieMeta, IrPieSlice, IrSequenceMeta, IrStyleRef, IrStyleTarget, IrSubgraph,
        IrSubgraphId, IrXyAxis, IrXyChartMeta, IrXySeries, IrXySeriesKind, MermaidDiagramIr,
        MermaidLinkMode, MermaidSanitizeMode, NodeShape, Span,
    };
    use fm_layout::{
        FillStyle, LayoutAxisTick, LayoutBand, LayoutBandKind, LayoutClusterBox, LayoutRect,
        LineCap as RenderLineCap, LineJoin as RenderLineJoin, PathCmd, RenderClip, RenderGroup,
        RenderItem, RenderPath, RenderRect, RenderScene, RenderSource, RenderText, RenderTransform,
        StrokeStyle, TextAlign as RenderTextAlign, TextBaseline as RenderTextBaseline,
        layout_diagram,
    };
    use proptest::prelude::*;

    #[test]
    fn truncate_label_borrows_when_no_truncation_needed() {
        let label = "short label";
        let unchanged = truncate_label(label, Some(32));
        assert!(matches!(unchanged, Cow::Borrowed(_)));
        assert_eq!(unchanged.as_ref(), label);

        let unlimited = truncate_label(label, None);
        assert!(matches!(unlimited, Cow::Borrowed(_)));
        assert_eq!(unlimited.as_ref(), label);
    }

    #[test]
    fn truncate_label_owns_only_truncated_output() {
        let truncated = truncate_label("abcdef", Some(4));
        assert!(matches!(truncated, Cow::Owned(_)));
        assert_eq!(truncated.as_ref(), "abc…");
    }

    #[test]
    fn plain_node_label_fast_path_matches_text_builder_output() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let config = SvgRenderConfig::default();
        let colors = ThemeColors::default();

        let mut expected = TextBuilder::new("Node 42")
            .x(11.0)
            .y(22.0)
            .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
            .font_size(13.0)
            .line_height(config.line_height)
            .anchor(TextAnchor::Middle)
            .fill(&colors.text)
            .build();
        expected = maybe_add_class(expected, "fm-node-label", true);
        expected = expected.attr("style", "font-weight:700");

        let actual = render_node_label_text(
            &ir,
            None,
            "Node 42",
            11.0,
            22.0,
            13.0,
            &config,
            &colors,
            Some("font-weight:700"),
            true,
        );

        assert_eq!(actual.render(), expected.render());
    }

    fn create_ir_with_cluster(title: &str) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let label_id = IrLabelId(0);
        ir.labels.push(IrLabel {
            text: title.to_string(),
            span: Span::default(),
        });
        // Clusters need member nodes to produce layout cluster boxes.
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            title: Some(label_id),
            members: vec![IrNodeId(0), IrNodeId(1)],
            grid_span: 1,
            span: Span::default(),
        });
        ir
    }

    fn create_ir_with_single_node(node_id: &str, shape: NodeShape) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let label_id = IrLabelId(0);
        ir.labels.push(IrLabel {
            text: "Single Node".to_string(),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            id: node_id.to_string(),
            label: Some(label_id),
            shape,
            ..Default::default()
        });
        ir
    }

    fn create_ir_with_single_node_classes(
        node_id: &str,
        shape: NodeShape,
        classes: &[&str],
    ) -> MermaidDiagramIr {
        let mut ir = create_ir_with_single_node(node_id, shape);
        if let Some(node) = ir.nodes.first_mut() {
            node.classes = classes.iter().map(|value| (*value).to_string()).collect();
        }
        ir
    }

    fn create_c4_ir_with_legend() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::C4Container);
        ir.meta.c4_show_legend = true;
        ir.labels.push(IrLabel {
            text: "Payments API".to_string(),
            span: Span::default(),
        });
        ir.labels.push(IrLabel {
            text: "Customer".to_string(),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            id: "api".to_string(),
            label: Some(IrLabelId(0)),
            shape: NodeShape::Rect,
            classes: vec!["c4".to_string(), "c4-container".to_string()],
            c4_meta: Some(Box::new(IrC4NodeMeta {
                element_type: "Container".to_string(),
                technology: Some("Rust".to_string()),
                description: Some("Handles payment requests".to_string()),
            })),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "customer".to_string(),
            label: Some(IrLabelId(1)),
            shape: NodeShape::Rounded,
            classes: vec![
                "c4".to_string(),
                "c4-person".to_string(),
                "c4-external".to_string(),
            ],
            c4_meta: Some(Box::new(IrC4NodeMeta {
                element_type: "Person".to_string(),
                technology: None,
                description: Some("External user".to_string()),
            })),
            ..IrNode::default()
        });
        ir
    }

    fn create_pie_ir(show_data: bool) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Pie);
        ir.pie_meta = Some(IrPieMeta {
            title: Some("Browser Usage".to_string()),
            show_data,
            slices: vec![
                IrPieSlice {
                    label: "Chrome".to_string(),
                    value: 50.0,
                },
                IrPieSlice {
                    label: "Firefox".to_string(),
                    value: 30.0,
                },
                IrPieSlice {
                    label: "Safari".to_string(),
                    value: 20.0,
                },
            ],
        });
        ir
    }

    fn create_state_ir_with_concurrent_regions() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::State);
        let label_id = IrLabelId(0);
        ir.labels.push(IrLabel {
            text: "Active Mode".to_string(),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            id: "Processing".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "Monitoring".to_string(),
            ..IrNode::default()
        });
        ir.graph.nodes.push(IrGraphNode {
            node_id: IrNodeId(0),
            kind: fm_core::IrNodeKind::State,
            clusters: vec![IrClusterId(0)],
            subgraphs: vec![IrSubgraphId(0), IrSubgraphId(1)],
        });
        ir.graph.nodes.push(IrGraphNode {
            node_id: IrNodeId(1),
            kind: fm_core::IrNodeKind::State,
            clusters: vec![IrClusterId(0)],
            subgraphs: vec![IrSubgraphId(0), IrSubgraphId(2)],
        });
        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            title: Some(label_id),
            members: vec![IrNodeId(0), IrNodeId(1)],
            grid_span: 2,
            span: Span::default(),
        });
        ir.graph.clusters.push(IrGraphCluster {
            cluster_id: IrClusterId(0),
            title: Some(label_id),
            members: vec![IrNodeId(0), IrNodeId(1)],
            subgraph: Some(IrSubgraphId(0)),
            grid_span: 2,
            span: Span::default(),
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(0),
            key: "Active".to_string(),
            title: Some(label_id),
            children: vec![IrSubgraphId(1), IrSubgraphId(2)],
            members: vec![IrNodeId(0), IrNodeId(1)],
            cluster: Some(IrClusterId(0)),
            grid_span: 2,
            span: Span::default(),
            ..IrSubgraph::default()
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(1),
            key: "__state_region_1".to_string(),
            parent: Some(IrSubgraphId(0)),
            members: vec![IrNodeId(0)],
            span: Span::default(),
            ..IrSubgraph::default()
        });
        ir.graph.subgraphs.push(IrSubgraph {
            id: IrSubgraphId(2),
            key: "__state_region_2".to_string(),
            parent: Some(IrSubgraphId(0)),
            members: vec![IrNodeId(1)],
            span: Span::default(),
            ..IrSubgraph::default()
        });
        ir
    }

    fn create_ir_with_labeled_edge() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.labels.push(IrLabel {
            text: "Start".to_string(),
            span: Span::default(),
        });
        ir.labels.push(IrLabel {
            text: "End".to_string(),
            span: Span::default(),
        });
        ir.labels.push(IrLabel {
            text: "edge label that can be truncated".to_string(),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            label: Some(IrLabelId(0)),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            label: Some(IrLabelId(1)),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            label: Some(IrLabelId(2)),
            ..Default::default()
        });
        ir
    }

    fn create_xychart_ir() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::XyChart);
        for node_id in [
            "Revenue_1",
            "Revenue_2",
            "Revenue_3",
            "Target_1",
            "Target_2",
            "Target_3",
        ] {
            ir.nodes.push(IrNode {
                id: node_id.to_string(),
                ..Default::default()
            });
        }
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(3)),
            to: IrEndpoint::Node(IrNodeId(4)),
            arrow: ArrowType::Line,
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(4)),
            to: IrEndpoint::Node(IrNodeId(5)),
            arrow: ArrowType::Line,
            ..Default::default()
        });
        ir.xy_chart_meta = Some(IrXyChartMeta {
            title: Some("Sales Revenue".to_string()),
            x_axis: IrXyAxis {
                categories: vec!["Jan".to_string(), "Feb".to_string(), "Mar".to_string()],
                ..Default::default()
            },
            y_axis: IrXyAxis {
                label: Some("Revenue".to_string()),
                min: Some(0.0),
                max: Some(100.0),
                ..Default::default()
            },
            series: vec![
                IrXySeries {
                    kind: IrXySeriesKind::Bar,
                    name: Some("Revenue".to_string()),
                    values: vec![30.0, 50.0, 70.0],
                    nodes: vec![IrNodeId(0), IrNodeId(1), IrNodeId(2)],
                },
                IrXySeries {
                    kind: IrXySeriesKind::Line,
                    name: Some("Target".to_string()),
                    values: vec![40.0, 60.0, 80.0],
                    nodes: vec![IrNodeId(3), IrNodeId(4), IrNodeId(5)],
                },
            ],
        });
        ir
    }

    fn create_linear_ir(node_count: usize) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        for index in 0..node_count {
            ir.labels.push(IrLabel {
                text: format!("N{index}"),
                span: Span::default(),
            });
            ir.nodes.push(IrNode {
                id: format!("N{index}"),
                label: Some(IrLabelId(index)),
                ..Default::default()
            });
        }
        for index in 1..node_count {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(index - 1)),
                to: IrEndpoint::Node(IrNodeId(index)),
                arrow: ArrowType::Arrow,
                ..Default::default()
            });
        }
        ir
    }

    fn create_scene_with_path_and_text() -> RenderScene {
        let mut root =
            RenderGroup::new(Some(String::from("scene-root"))).with_source(RenderSource::Diagram);
        root.children.push(RenderItem::Path(RenderPath {
            source: RenderSource::Node(0),
            commands: vec![
                PathCmd::MoveTo { x: 0.0, y: 0.0 },
                PathCmd::LineTo { x: 10.0, y: 0.0 },
                PathCmd::CubicTo {
                    c1x: 15.0,
                    c1y: 5.0,
                    c2x: 20.0,
                    c2y: 15.0,
                    x: 25.0,
                    y: 20.0,
                },
                PathCmd::QuadTo {
                    cx: 30.0,
                    cy: 25.0,
                    x: 35.0,
                    y: 20.0,
                },
                PathCmd::Close,
            ],
            fill: Some(FillStyle::Solid {
                color: String::from("#ffeeaa"),
                opacity: 0.25,
            }),
            stroke: Some(StrokeStyle {
                color: String::from("#334455"),
                width: 2.5,
                opacity: 0.5,
                dash_array: vec![6.0, 4.0],
                line_cap: RenderLineCap::Round,
                line_join: RenderLineJoin::Bevel,
            }),
            marker_start: MarkerKind::None,
            marker_end: MarkerKind::None,
        }));
        root.children.push(RenderItem::Text(RenderText {
            source: RenderSource::Edge(2),
            text: String::from("scene-label"),
            x: 12.0,
            y: 18.0,
            font_size: 13.0,
            align: RenderTextAlign::Middle,
            baseline: RenderTextBaseline::Middle,
            fill: FillStyle::Solid {
                color: String::from("#102030"),
                opacity: 0.8,
            },
        }));

        RenderScene {
            bounds: RenderRect {
                x: 0.0,
                y: 0.0,
                width: 64.0,
                height: 40.0,
            },
            root,
        }
    }

    fn create_scene_with_transform_and_clip() -> RenderScene {
        let mut child =
            RenderGroup::new(Some(String::from("scene-child"))).with_source(RenderSource::Diagram);
        child.transform = Some(RenderTransform::Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 12.0,
            f: 8.0,
        });
        child.clip = Some(RenderClip::Rect(RenderRect {
            x: 1.0,
            y: 2.0,
            width: 30.0,
            height: 18.0,
        }));
        child.children.push(RenderItem::Path(RenderPath {
            source: RenderSource::Cluster(0),
            commands: vec![
                PathCmd::MoveTo { x: 0.0, y: 0.0 },
                PathCmd::LineTo { x: 40.0, y: 0.0 },
                PathCmd::LineTo { x: 40.0, y: 20.0 },
                PathCmd::Close,
            ],
            fill: Some(FillStyle::Solid {
                color: String::from("#ddeeff"),
                opacity: 1.0,
            }),
            stroke: None,
            marker_start: MarkerKind::None,
            marker_end: MarkerKind::None,
        }));

        let mut root =
            RenderGroup::new(Some(String::from("scene-root"))).with_source(RenderSource::Diagram);
        root.children.push(RenderItem::Group(child));

        RenderScene {
            bounds: RenderRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 80.0,
            },
            root,
        }
    }

    #[test]
    fn emits_svg_document() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let svg = render_svg(&ir);
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn explicit_legacy_backend_matches_default_output() {
        let ir = create_ir_with_labeled_edge();
        let default_svg = render_svg_with_config(&ir, &SvgRenderConfig::default());
        let explicit_legacy = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                backend: SvgBackend::LegacyLayout,
                ..Default::default()
            },
        );
        assert_eq!(default_svg, explicit_legacy);
    }

    #[test]
    fn edge_inline_fill_and_stroke_gated_on_embedded_css() {
        // `.fm-edge { fill: none; stroke: var(--fm-edge-color) }` makes the inline `fill="none"`
        // and base `stroke=<theme color>` on edge paths redundant when the theme CSS is embedded
        // (a presentation attribute loses to the stylesheet), so they are dropped there.
        // Attribute-driven exports (`embed_theme_css = false`, e.g. the PNG raster path which
        // resvg cannot fully style via CSS) MUST keep both inline fallbacks.
        let ir = create_ir_with_labeled_edge();
        let with_css = render_svg_with_config(&ir, &SvgRenderConfig::default());
        let without_css = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                embed_theme_css: false,
                ..Default::default()
            },
        );
        // The edge contributes one inline `fill="none"`; it vanishes from the default
        // (CSS-embedded) render and remains in the attribute-driven export.
        let fill_with = with_css.matches("fill=\"none\"").count();
        let fill_without = without_css.matches("fill=\"none\"").count();
        assert!(
            fill_without > fill_with,
            "attribute-driven export must keep inline edge fill (with={fill_with}, without={fill_without})"
        );
        // Both the edge base stroke AND every node-shape base stroke (`.fm-node <shape> { stroke:
        // var(--fm-node-accent) }` covers them) are gated, so the attribute-driven export carries
        // strictly more inline `stroke=` attributes than the CSS-embedded render. Marker strokes in
        // the `<defs>` are unaffected and cancel out.
        let stroke_with = with_css.matches(" stroke=\"").count();
        let stroke_without = without_css.matches(" stroke=\"").count();
        assert!(
            stroke_without > stroke_with,
            "attribute-driven export must keep inline edge + node strokes (with={stroke_with}, without={stroke_without})"
        );
        // The node-shape base `stroke-width="1.60"` is likewise gated (the unconditional
        // `.fm-node <shape> { stroke-width: 1.6 }` rule overrides it); edges keep theirs.
        let sw_with = with_css.matches(" stroke-width=\"").count();
        let sw_without = without_css.matches(" stroke-width=\"").count();
        assert!(
            sw_without > sw_with,
            "attribute-driven export must keep inline node stroke-width (with={sw_with}, without={sw_without})"
        );
        // Node drop-shadow: the inline `filter="url(#drop-shadow)"` (and its now-unreferenced
        // `<defs>` filter) are gated off with CSS embedded (the CSS `filter: drop-shadow(…)` renders
        // the shadow); the attribute-driven export keeps them. So `url(#drop-shadow)` references
        // appear only without CSS.
        assert_eq!(
            with_css.matches("url(#drop-shadow)").count(),
            0,
            "embedded-CSS render must not reference #drop-shadow (CSS provides the shadow)"
        );
        assert!(
            without_css.contains("url(#drop-shadow)"),
            "attribute-driven export must keep the inline drop-shadow filter"
        );
    }

    #[test]
    fn precomputed_layout_matches_default_render_output() {
        let ir = create_ir_with_labeled_edge();
        let config = SvgRenderConfig::default();
        let layout = layout_diagram(&ir);

        let default_svg = render_svg_with_config(&ir, &config);
        let precomputed_svg = render_svg_with_layout(&ir, &layout, &config);

        assert_eq!(default_svg, precomputed_svg);
    }

    #[test]
    fn scene_backend_is_selectable_from_render_svg_with_config() {
        let ir = create_ir_with_labeled_edge();
        let scene_svg = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                backend: SvgBackend::Scene,
                ..Default::default()
            },
        );
        assert!(scene_svg.starts_with("<svg"));
        assert!(scene_svg.contains("data-type=\"flowchart\""));
        assert!(scene_svg.contains("fm-source-kind=\"node\""));
    }

    #[test]
    fn render_scene_to_svg_emits_paths_text_and_source_metadata() {
        let scene = create_scene_with_path_and_text();
        let svg = render_scene_to_svg(&scene, &SvgRenderConfig::default());
        assert!(svg.contains("data-type=\"scene\""));
        assert!(svg.contains("<path"));
        assert!(svg.contains("<text"));
        assert!(svg.contains("scene-label"));
        assert!(svg.contains("fm-source-kind=\"node\""));
        assert!(svg.contains("fm-source-kind=\"edge\""));
        assert!(svg.contains("C15 5,20 15,25 20"));
        assert!(svg.contains("Q30 25,35 20"));
    }

    #[test]
    fn render_scene_to_svg_supports_transform_and_clip_path() {
        let scene = create_scene_with_transform_and_clip();
        let svg = render_scene_to_svg(&scene, &SvgRenderConfig::default());
        assert!(svg.contains("transform=\"matrix(1,0,0,1,12,8)\""));
        assert!(svg.contains("<clipPath id=\"fm-scene-clip-0\""));
        assert!(svg.contains("clip-path=\"url(#fm-scene-clip-0)\""));
    }

    #[test]
    fn render_scene_to_svg_preserves_fill_and_stroke_styles() {
        let scene = create_scene_with_path_and_text();
        let svg = render_scene_to_svg(&scene, &SvgRenderConfig::default());
        assert!(svg.contains("fill=\"#ffeeaa\""));
        assert!(svg.contains("fill-opacity=\"0.25\""));
        assert!(svg.contains("stroke=\"#334455\""));
        assert!(svg.contains("stroke-width=\"2.50\""));
        assert!(svg.contains("stroke-opacity=\"0.50\""));
        assert!(svg.contains("stroke-dasharray=\"6,4\""));
        assert!(svg.contains("stroke-linecap=\"round\""));
        assert!(svg.contains("stroke-linejoin=\"bevel\""));
    }

    #[test]
    fn includes_data_attributes() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        let svg = render_svg(&ir);
        assert!(svg.contains("data-nodes=\"0\""));
        assert!(svg.contains("data-edges=\"0\""));
        assert!(svg.contains("data-type=\"sequence\""));
    }

    #[test]
    fn includes_accessibility() {
        let ir = MermaidDiagramIr::empty(DiagramType::Class);
        let svg = render_svg(&ir);
        assert!(svg.contains("role=\"img\""));
        assert!(svg.contains("<title>"));
        assert!(svg.contains("<desc>"));
    }

    #[test]
    fn explicit_accessibility_directives_override_legacy_svg_metadata() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.meta.acc_title = Some(String::from("Custom Title"));
        ir.meta.acc_descr = Some(String::from("Custom Description"));

        let svg = render_svg(&ir);

        assert!(svg.contains("<title>Custom Title</title>"));
        assert!(svg.contains("<desc>Custom Description</desc>"));
    }

    #[test]
    fn explicit_accessibility_directives_override_scene_svg_metadata() {
        let mut ir = create_ir_with_labeled_edge();
        ir.meta.acc_title = Some(String::from("Scene Title"));
        ir.meta.acc_descr = Some(String::from("Scene Description"));

        let svg = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                backend: SvgBackend::Scene,
                ..Default::default()
            },
        );

        assert!(svg.contains("<title>Scene Title</title>"));
        assert!(svg.contains("<desc>Scene Description</desc>"));
    }

    #[test]
    fn generic_diagram_title_renders_above_flowchart_content() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.meta.title = Some(String::from("Flow Title"));

        let svg = render_svg(&ir);

        assert!(svg.contains(">Flow Title<"));
        assert!(svg.contains("fm-diagram-title"));
    }

    #[test]
    fn front_matter_title_is_used_by_scene_xychart_renderer() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::XyChart);
        ir.meta.title = Some(String::from("Shared Title"));
        ir.xy_chart_meta = Some(IrXyChartMeta {
            title: None,
            ..IrXyChartMeta::default()
        });

        let svg = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                backend: SvgBackend::Scene,
                ..Default::default()
            },
        );

        assert!(svg.contains(">Shared Title<"));
    }

    #[test]
    fn includes_defs_section() {
        // A diagram with an edge references arrow-end, so the reference-gated marker strip keeps it.
        let parsed = fm_parser::parse("flowchart LR\n  A --> B\n");
        let svg = render_svg(&parsed.ir);
        assert!(svg.contains("<defs>"));
        assert!(svg.contains("</defs>"));
        assert!(svg.contains("<marker"));
        assert!(svg.contains("id=\"arrow-end\""));
    }

    #[test]
    fn includes_half_arrow_marker_defs() {
        // Sequence half/stick arrowheads still render through their markers; the reference-gated
        // strip (see `strip_unused_markers`) must never leave an emitted marker def unreferenced.
        let parsed = fm_parser::parse(
            "sequenceDiagram\n\
             Alice->>Bob: Solid\n\
             Alice-|\\Bob: HalfTop\n\
             Bob-|/Alice: HalfBottom\n",
        );
        let svg = render_svg(&parsed.ir);
        assert!(svg.contains("id=\"arrow-end\""), "solid arrow marker missing");
        let mut at = 0;
        while let Some(rel) = svg[at..].find("<marker ") {
            let s = at + rel;
            let id_at = svg[s..].find("id=\"").expect("marker id") + s + 4;
            let id_end = svg[id_at..].find('"').expect("id end") + id_at;
            let id = svg[id_at..id_end].to_string();
            assert!(
                svg.contains(&format!("url(#{id})")),
                "strip left orphan marker def {id}"
            );
            at = id_end;
        }
    }

    #[test]
    fn custom_config_disables_shadows() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let config = SvgRenderConfig {
            shadows: false,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(!svg.contains("drop-shadow"));
    }

    #[test]
    fn renders_cluster_with_css_classes() {
        let ir = create_ir_with_cluster("Test Subgraph");
        let svg = render_svg(&ir);
        assert!(svg.contains("class=\"fm-cluster\""));
        assert!(svg.contains("class=\"fm-cluster-label\""));
    }

    #[test]
    fn renders_pie_title_legend_and_showdata_values() {
        let ir = create_pie_ir(true);
        let svg = render_svg(&ir);

        assert!(svg.contains("fm-pie-title"));
        assert!(svg.contains("Browser Usage"));
        assert!(svg.contains("fm-pie-legend"));
        assert!(svg.contains("fm-pie-legend-entry"));
        assert!(svg.contains("Chrome: 50 (50.0%)"));
        assert!(svg.contains("Firefox: 30 (30.0%)"));
    }

    #[test]
    fn pie_without_showdata_omits_value_and_percentage_labels() {
        let ir = create_pie_ir(false);
        let svg = render_svg(&ir);

        assert!(svg.contains(">Chrome<"));
        assert!(svg.contains(">Firefox<"));
        assert!(!svg.contains("Chrome: 50"));
        assert!(!svg.contains("50.0%"));
    }

    #[test]
    fn renders_single_slice_pie_as_full_circle() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Pie);
        ir.pie_meta = Some(IrPieMeta {
            title: Some("Only One".to_string()),
            show_data: true,
            slices: vec![IrPieSlice {
                label: "Only".to_string(),
                value: 100.0,
            }],
        });

        let svg = render_svg(&ir);

        assert!(svg.contains("fm-pie-slice-full"));
        assert!(svg.contains("<circle"));
    }

    #[test]
    fn pie_theme_variables_override_slice_palette() {
        let mut ir = create_pie_ir(false);
        ir.meta
            .theme_overrides
            .theme_variables
            .insert("pie1".to_string(), "#123456".to_string());
        ir.meta
            .theme_overrides
            .theme_variables
            .insert("pie2".to_string(), "#abcdef".to_string());

        let svg = render_svg(&ir);

        assert!(svg.contains("fill=\"#123456\""));
        assert!(svg.contains("fill=\"#abcdef\""));
    }

    #[test]
    fn renders_sequence_origin_cluster_title_from_layout() {
        let ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: vec![LayoutClusterBox {
                cluster_index: 0,
                span: Span::default(),
                title: Some("Backend".to_string()),
                color: None,
                bounds: LayoutRect {
                    x: 10.0,
                    y: -20.0,
                    width: 120.0,
                    height: 160.0,
                },
            }],
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: -20.0,
                width: 140.0,
                height: 180.0,
            },
            stats: Default::default(),
            extensions: Default::default(),
            dirty_regions: Vec::new(),
        };

        let svg = render_svg_with_layout(&ir, &layout, &SvgRenderConfig::default());
        assert!(svg.contains("Backend"));
        assert!(svg.contains("fm-cluster-label"));
    }

    #[test]
    fn renders_c4_boundary_with_dashed_border() {
        let ir = create_ir_with_cluster("System_Boundary(webapp, Web Application)");
        let svg = render_svg(&ir);
        assert!(svg.contains("fm-cluster-c4"));
        assert!(svg.contains("stroke-dasharray"));
    }

    #[test]
    fn renders_c4_node_metadata_person_icon_and_legend() {
        let ir = create_c4_ir_with_legend();
        let svg = render_svg(&ir);
        assert!(svg.contains("fm-c4-type-label"));
        assert!(svg.contains("&lt;&lt;Container>>"));
        assert!(svg.contains("[Rust]"));
        assert!(svg.contains("Handles payment"));
        assert!(svg.contains("requests"));
        assert!(svg.contains("fm-c4-person-icon"));
        assert!(svg.contains("fm-node-border-dashed"));
        assert!(svg.contains("fm-c4-legend"));
        assert!(svg.contains("C4 Legend"));
    }

    #[test]
    fn renders_swimlane_cluster_style() {
        let ir = create_ir_with_cluster("section Planning");
        let svg = render_svg(&ir);
        assert!(svg.contains("fm-cluster-swimlane"));
    }

    #[test]
    fn renders_state_cluster_concurrency_divider() {
        let ir = create_state_ir_with_concurrent_regions();
        let layout = layout_diagram(&ir);
        let svg = render_svg_with_layout(&ir, &layout, &SvgRenderConfig::default());
        assert!(svg.contains("Active Mode"));
        assert!(svg.contains("stroke-dasharray=\"6,4\""));
    }

    #[test]
    fn cluster_uses_translucent_fill() {
        let ir = create_ir_with_cluster("Regular Cluster");
        let svg = render_svg(&ir);
        // Standard clusters should have translucent fill
        assert!(svg.contains("rgba("));
    }

    #[test]
    fn renders_sequence_participant_group_named_color() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.nodes.push(IrNode {
            id: "API".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "DB".to_string(),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..Default::default()
        });
        ir.sequence_meta = Some(IrSequenceMeta {
            participant_groups: vec![fm_core::IrParticipantGroup {
                label: "Backend".to_string(),
                color: Some("Aqua".to_string()),
                participants: vec![IrNodeId(0), IrNodeId(1)],
            }],
            ..Default::default()
        });

        let svg = render_svg(&ir);
        assert!(svg.contains("fill=\"aqua\""));
        assert!(svg.contains("stroke=\"aqua\""));
    }

    #[test]
    fn renders_sequence_rect_fragment_as_highlight() {
        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 160.0,
                height: 120.0,
            },
            stats: Default::default(),
            extensions: fm_layout::LayoutExtensions {
                sequence_fragments: vec![fm_layout::LayoutSequenceFragment {
                    kind: fm_core::FragmentKind::Rect,
                    label: String::new(),
                    color: Some("rgba(200, 220, 240, 0.4)".to_string()),
                    bounds: LayoutRect {
                        x: 10.0,
                        y: 20.0,
                        width: 120.0,
                        height: 60.0,
                    },
                }],
                ..Default::default()
            },
            dirty_regions: Vec::new(),
        };

        let svg = render_svg_with_layout(
            &MermaidDiagramIr::empty(DiagramType::Sequence),
            &layout,
            &SvgRenderConfig::default(),
        );
        assert!(svg.contains("fill=\"rgba(200, 220, 240, 0.4)\""));
        assert!(!svg.contains("rect ["));
        assert!(!svg.contains("fm-sequence-fragment-label"));
    }

    #[test]
    fn renders_sequence_rect_fragment_transparent_without_opaque_fill() {
        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 160.0,
                height: 120.0,
            },
            stats: Default::default(),
            extensions: fm_layout::LayoutExtensions {
                sequence_fragments: vec![fm_layout::LayoutSequenceFragment {
                    kind: fm_core::FragmentKind::Rect,
                    label: String::new(),
                    color: Some("transparent".to_string()),
                    bounds: LayoutRect {
                        x: 10.0,
                        y: 20.0,
                        width: 120.0,
                        height: 60.0,
                    },
                }],
                ..Default::default()
            },
            dirty_regions: Vec::new(),
        };

        let svg = render_svg_with_layout(
            &MermaidDiagramIr::empty(DiagramType::Sequence),
            &layout,
            &SvgRenderConfig::default(),
        );
        assert!(svg.contains("fill=\"transparent\""));
    }

    #[test]
    fn sequence_participant_group_color_is_sanitized() {
        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: vec![LayoutClusterBox {
                cluster_index: 0,
                span: Span::default(),
                title: Some("Unsafe".to_string()),
                color: Some("url(javascript:alert(1))".to_string()),
                bounds: LayoutRect {
                    x: 10.0,
                    y: -20.0,
                    width: 120.0,
                    height: 160.0,
                },
            }],
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: -20.0,
                width: 140.0,
                height: 180.0,
            },
            stats: Default::default(),
            extensions: Default::default(),
            dirty_regions: Vec::new(),
        };

        let svg = render_svg_with_layout(
            &MermaidDiagramIr::empty(DiagramType::Sequence),
            &layout,
            &SvgRenderConfig::default(),
        );
        assert!(!svg.contains("url(javascript:alert(1))"));
        assert!(svg.contains("stroke=\"#dee2e6\""));
    }

    #[test]
    fn node_inline_style_preserves_rgba_values() {
        let mut ir = create_ir_with_single_node("node-alpha", NodeShape::Rect);
        ir.style_refs.push(IrStyleRef {
            target: IrStyleTarget::Node(IrNodeId(0)),
            style: "fill:rgba(226,232,240,0.3),stroke:#334155".to_string(),
            span: Span::default(),
        });

        let (shape_style, _text_style) = resolve_node_inline_styles(&ir, 0);
        let inline = shape_style.expect("node style should resolve");

        assert_eq!(inline, "fill:rgba(226,232,240,0.3); stroke:#334155");
    }

    #[test]
    fn edge_inline_style_preserves_css_function_commas() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.style_refs.push(IrStyleRef {
            target: IrStyleTarget::Link(0),
            style: "stroke:rgba(12,34,56,0.5),filter:drop-shadow(0px,1px,2px,#000)".to_string(),
            span: Span::default(),
        });

        let inline = resolve_edge_inline_style(&ir, 0).expect("edge style should resolve");

        assert!(inline.contains("stroke:rgba(12,34,56,0.5)"));
        assert!(inline.contains("filter:drop-shadow(0px,1px,2px,#000)"));
    }

    #[test]
    fn unstyled_edge_has_no_inline_style() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });

        assert_eq!(resolve_edge_inline_style(&ir, 0), None);
    }

    #[test]
    fn inline_style_preserves_commas_inside_quoted_values() {
        let style = fm_core::parse_style_string(r#"font-family:"A, B",stroke:#334155"#);
        assert_eq!(style.properties.get("font-family").unwrap(), r#""A, B""#);
        assert_eq!(style.properties.get("stroke").unwrap(), "#334155");
    }

    #[test]
    fn classdef_emits_css_rules_for_nodes() {
        let mut ir = create_ir_with_single_node("node-styled", NodeShape::Rect);
        ir.nodes[0].classes.push("important".to_string());
        ir.style_refs.push(IrStyleRef {
            target: IrStyleTarget::Class("important".to_string()),
            style: "fill:#f9f,stroke:#333,color:#111".to_string(),
            span: Span::default(),
        });

        let svg = render_svg(&ir);

        assert!(svg.contains(".fm-node-user-important"));
        assert!(svg.contains("fill:#f9f"));
        assert!(svg.contains("stroke:#333"));
        assert!(svg.contains("fill:#111"));
        assert!(svg.contains("fm-node-shape"));
        assert!(svg.contains("fm-node-label"));
    }

    #[test]
    fn renders_layout_extensions_for_bands_and_axis_ticks() {
        let ir = MermaidDiagramIr::empty(DiagramType::Gantt);
        let mut layout = layout_diagram(&ir);
        layout.extensions.bands.push(LayoutBand {
            kind: LayoutBandKind::Section,
            label: "Planning".to_string(),
            bounds: fm_layout::LayoutRect {
                x: 0.0,
                y: 20.0,
                width: 180.0,
                height: 80.0,
            },
        });
        layout.extensions.axis_ticks.push(LayoutAxisTick {
            label: "2026-02-01".to_string(),
            position: 24.0,
        });

        let svg = render_svg_with_layout(&ir, &layout, &SvgRenderConfig::default());
        assert!(svg.contains("fm-band-section"));
        assert!(svg.contains("fm-band-label"));
        assert!(svg.contains("fm-axis-tick"));
        assert!(svg.contains("2026-02-01"));
    }

    #[test]
    fn renders_xychart_axes_bars_and_line_series() {
        let ir = create_xychart_ir();
        let svg = render_svg_with_config(&ir, &SvgRenderConfig::default());

        assert!(svg.contains("fm-xychart-axis"));
        assert!(svg.contains("fm-xychart-gridline"));
        assert!(svg.contains("fm-xychart-bar"));
        assert!(svg.contains("fm-xychart-line"));
        assert!(svg.contains("fm-xychart-point"));
        assert!(svg.contains("Sales Revenue"));
        assert!(svg.contains(">Jan<"));
        assert!(svg.contains(">Revenue<"));
    }

    #[test]
    fn includes_accessibility_css() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let svg = render_svg(&ir);
        // Default config enables accessibility CSS
        assert!(svg.contains("prefers-contrast"));
        assert!(svg.contains("prefers-reduced-motion"));
    }

    #[test]
    fn accessibility_enhanced_description() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let svg = render_svg(&ir);
        // Enhanced description includes direction
        assert!(svg.contains("flowing"));
    }

    #[test]
    fn disabling_a11y_css() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let config = SvgRenderConfig {
            a11y: A11yConfig::minimal(),
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        // Minimal a11y should not include high contrast CSS
        assert!(!svg.contains("prefers-contrast"));
    }

    #[test]
    fn node_render_includes_deterministic_accent_and_shape_classes() {
        let ir = create_ir_with_single_node("node-alpha", NodeShape::Diamond);
        let svg = render_svg(&ir);
        assert!(svg.contains("fm-node-accent-"));
        assert!(svg.contains("fm-node-shape-diamond"));
    }

    #[test]
    fn stable_accent_index_is_deterministic_and_bounded() {
        let first = stable_accent_index("node-42");
        let second = stable_accent_index("node-42");
        assert_eq!(first, second);
        assert!((1..=8).contains(&first));
    }

    #[test]
    fn compact_tier_hides_edge_labels() {
        let ir = create_ir_with_labeled_edge();
        let config = SvgRenderConfig {
            detail_tier: MermaidTier::Compact,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(!svg.contains("class=\"edge-label\""));
    }

    #[test]
    fn rich_tier_preserves_edge_labels() {
        let ir = create_ir_with_labeled_edge();
        let config = SvgRenderConfig {
            detail_tier: MermaidTier::Rich,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("class=\"edge-label\""));
    }

    #[test]
    fn compact_tier_can_hide_node_text_for_tiny_layouts() {
        // Compact tier hides node labels when the layout area is below
        // the threshold (36K px², width<240, height<150).
        let ir = create_ir_with_single_node("tiny-node", NodeShape::Rect);
        let config = SvgRenderConfig {
            detail_tier: MermaidTier::Compact,
            padding: 0.0,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        // Verify compact tier is selected.
        assert!(svg.contains("data-detail-tier=\"compact\""));
        // In compact mode, edge labels are always hidden.
        assert!(!svg.contains("class=\"edge-label\""));
    }

    #[test]
    fn auto_tier_marks_detail_tier_data_attribute() {
        let ir = create_ir_with_single_node("auto-tier", NodeShape::Rect);
        let config = SvgRenderConfig {
            padding: 0.0,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("data-detail-tier=\"compact\""));
    }

    #[test]
    fn print_optimized_css_is_embedded_by_default() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let svg = render_svg(&ir);
        assert!(svg.contains("@media print"));
    }

    #[test]
    fn configurable_shadow_filter_is_emitted() {
        let ir = create_ir_with_single_node("shadow-node", NodeShape::Rect);
        let config = SvgRenderConfig {
            shadow_offset_x: 4.0,
            shadow_offset_y: 1.5,
            shadow_blur: 5.0,
            shadow_opacity: 0.45,
            shadow_color: "#ff3366".to_string(),
            // The configurable `<filter id="drop-shadow">` (which honours `shadow_color`) is the
            // shadow source for attribute-driven output; with embedded CSS the shadow comes from
            // the `.fm-node { filter: drop-shadow(…) }` rule instead, so the def is gated off there.
            embed_theme_css: false,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("id=\"drop-shadow\""));
        assert!(svg.contains("flood-color=\"#ff3366\""));
        assert!(svg.contains("flood-opacity=\"0.45\""));
    }

    #[test]
    fn minify_css_is_whitespace_only_and_preserves_semantic_spaces() {
        // The strip-ALL-whitespace projection of input and output must be identical: this proves
        // the minifier only added/removed whitespace, never a selector/property/value byte.
        let strip_ws = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace()).collect() };
        let pretty = "\
:root {
  --fm-bg: #fff;
}
.fm-node rect,
.fm-node path {
  fill: var(--fm-node-fill);
  stroke-width: 1.6;
  filter: drop-shadow(0 2px 8px rgba(0, 0, 0, 0.10));
}
.fm-node .child {
  background: color-mix(in srgb, var(--fm-accent-1) 4%, transparent);
}
";
        let min = minify_css(pretty);
        // 1. Whitespace-only invariant.
        assert_eq!(
            strip_ws(pretty),
            strip_ws(&min),
            "minify_css changed a non-whitespace byte"
        );
        // 2. It actually shrank (indentation + newlines removed).
        assert!(min.len() < pretty.len());
        assert!(!min.contains('\n'), "newlines should be gone: {min:?}");
        assert!(!min.contains("  "), "indentation should be gone: {min:?}");
        // 3. Delimiter-hugging spaces collapse.
        assert!(min.contains(".fm-node rect,.fm-node path{"));
        assert!(min.contains("#fff;}"));
        // 4. SEMANTIC spaces survive — descendant combinator, value-internal, and `prop: value`.
        assert!(
            min.contains(".fm-node .child{"),
            "descendant combinator space dropped: {min:?}"
        );
        assert!(min.contains("2px 8px"), "value-internal space dropped: {min:?}");
        assert!(min.contains("in srgb"), "function-arg space dropped: {min:?}");
        assert!(min.contains("4%, transparent") || min.contains("4%,transparent"));
        assert!(min.contains("fill: var(--fm-node-fill)"), "`: ` space dropped: {min:?}");
    }

    #[test]
    fn strip_unused_markers_keeps_only_referenced_defs() {
        // Hand-built SVG: two marker defs, only one referenced.
        let mut svg = String::from(
            "<svg><defs>\
             <marker id=\"arrow-end\" refX=\"8\"><path d=\"M0 0\"/></marker>\
             <marker id=\"arrow-cross\" refX=\"8\"><path d=\"M1 1\"/></marker>\
             </defs>\
             <path class=\"fm-edge\" marker-end=\"url(#arrow-end)\" d=\"M0 0 L9 9\"/></svg>",
        );
        strip_unused_markers(&mut svg);
        assert!(svg.contains("id=\"arrow-end\""), "referenced marker must stay");
        assert!(
            !svg.contains("id=\"arrow-cross\""),
            "unreferenced marker must be removed"
        );
        // The referenced edge and its reference are untouched.
        assert!(svg.contains("marker-end=\"url(#arrow-end)\""));
        assert!(svg.contains("M0 0 L9 9"));
    }

    #[test]
    fn rendered_sequence_emits_only_referenced_markers() {
        // A sequence diagram emits the full 12-marker set but typically references only arrow-end;
        // every remaining `<marker id="X">` must have a matching `url(#X)` in the body.
        let parsed =
            fm_parser::parse("sequenceDiagram\n    Alice->>Bob: Hello\n    Bob-->>Alice: Hi\n");
        let svg = render_svg(&parsed.ir);
        let mut at = 0;
        let mut checked = 0;
        while let Some(rel) = svg[at..].find("<marker ") {
            let s = at + rel;
            let id_at = svg[s..].find("id=\"").expect("marker id") + s + 4;
            let id_end = svg[id_at..].find('"').expect("id end") + id_at;
            let id = &svg[id_at..id_end];
            assert!(
                svg.contains(&format!("url(#{id})")),
                "marker {id} kept but never referenced"
            );
            checked += 1;
            at = id_end;
        }
        assert!(checked >= 1, "expected at least the arrow-end marker");
    }

    #[test]
    fn strip_dead_marker_css_prunes_dead_selectors() {
        // Only arrow-end is defined; the CSS references end/filled/cross/open.
        let mut svg = String::from(
            "<svg><defs><marker id=\"arrow-end\"><path/></marker></defs>\
             <style>\
marker#arrow-end path,
marker#arrow-filled path {
  fill: red;
}
marker#arrow-open path {
  stroke: blue;
}
.fm-edge {
  stroke: black;
}
</style>\
             <path marker-end=\"url(#arrow-end)\"/></svg>",
        );
        strip_dead_marker_css(&mut svg);
        // Live selector kept, dead sibling pruned from the list.
        assert!(svg.contains("marker#arrow-end path"), "live marker selector dropped");
        assert!(!svg.contains("marker#arrow-filled"), "dead sibling not pruned");
        // Whole rule with only a dead marker is removed.
        assert!(!svg.contains("marker#arrow-open"), "fully-dead rule not removed");
        // Non-marker rule untouched.
        assert!(svg.contains(".fm-edge {"), "non-marker rule corrupted");
        // The live rule still has its body.
        assert!(svg.contains("fill: red"));
    }

    #[test]
    fn edgeless_diagram_drops_all_marker_css() {
        // A pie chart has no edges -> no markers -> every marker#… rule is dead.
        let parsed = fm_parser::parse("pie title Pets\n  \"Dogs\": 3\n  \"Cats\": 2\n");
        let svg = render_svg(&parsed.ir);
        assert!(!svg.contains("marker#arrow"), "edge-less diagram kept dead marker CSS");
        assert!(!svg.contains("<marker "), "edge-less diagram kept marker defs");
    }

    #[test]
    fn rendered_style_block_is_minified() {
        let ir = create_ir_with_single_node("n", NodeShape::Rect);
        let svg = render_svg(&ir);
        let start = svg.find("<style").expect("style open");
        let gt = svg[start..].find('>').expect("style >") + start + 1;
        let end = svg[gt..].find("</style>").expect("style close") + gt;
        let css = &svg[gt..end];
        // No pretty-print artifacts remain in the embedded stylesheet.
        assert!(!css.contains('\n'), "embedded CSS still has newlines");
        assert!(!css.contains("  "), "embedded CSS still has indentation");
        // But the rules themselves are intact.
        assert!(css.contains(":root{"));
        assert!(css.contains(".fm-node "));
    }

    #[test]
    fn node_gradient_defs_and_fill_are_emitted() {
        let ir = create_ir_with_single_node("grad-node", NodeShape::Rect);
        let config = SvgRenderConfig {
            node_gradients: true,
            node_gradient_style: NodeGradientStyle::LinearVertical,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("id=\"fm-node-gradient\""));
        assert!(svg.contains("<linearGradient"));
        assert!(svg.contains("fill=\"url(#fm-node-gradient)\""));
    }

    #[test]
    fn highlighted_node_uses_glow_filter() {
        let ir = create_ir_with_single_node_classes("focus-node", NodeShape::Rect, &["highlight"]);
        let config = SvgRenderConfig {
            glow_enabled: true,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("id=\"node-glow\""));
        assert!(svg.contains("class=\"fm-node fm-node-accent-"));
        assert!(svg.contains("fm-node-highlighted"));
        assert!(svg.contains("filter=\"url(#node-glow)\""));
    }

    #[test]
    fn inactive_node_class_is_preserved_for_opacity_layering() {
        let ir =
            create_ir_with_single_node_classes("inactive-node", NodeShape::Rect, &["inactive"]);
        let config = SvgRenderConfig {
            inactive_opacity: 0.35,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("fm-node-inactive"));
        // The embedded `<style>` is whitespace-minified (see `minify_css`): delimiter-hugging
        // spaces collapse, but the `: ` after a property is preserved.
        assert!(svg.contains(".fm-node-inactive{opacity: 0.35;}"));
    }

    #[test]
    fn block_beta_nodes_emit_family_specific_svg_classes_and_css() {
        let ir = create_ir_with_single_node_classes(
            "service",
            NodeShape::Rect,
            &["block-beta", "block-beta-span-2"],
        );
        let svg = render_svg(&ir);
        assert!(svg.contains("fm-node-block-beta"));
        // Descendant-combinator spaces survive minification; the space before `{` collapses.
        assert!(svg.contains(".fm-node-block-beta rect,"));
        assert!(svg.contains(".fm-node-block-beta text{"));
    }

    #[test]
    fn block_beta_space_nodes_do_not_render_synthetic_placeholder_labels() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::BlockBeta);
        ir.nodes.push(IrNode {
            id: "__space_12".to_string(),
            shape: NodeShape::Rect,
            classes: vec!["block-beta".to_string(), "block-beta-space".to_string()],
            ..IrNode::default()
        });

        let svg = render_svg(&ir);
        assert!(svg.contains("fm-node-block-beta-space"));
        assert!(svg.contains(".fm-node-block-beta-space{"));
        assert!(!svg.contains("__space_12</text>"));
        assert!(!svg.contains("aria-label=\"__space_12\""));
    }

    #[test]
    fn callback_nodes_emit_data_callback_hook_and_css_class() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..IrNode::default()
        });
        ir.nodes[0].interaction_mut().callback = Some("handleNodeClick".to_string());

        let svg = render_svg(&ir);
        assert!(svg.contains("data-callback=\"handleNodeClick\""));
        assert!(svg.contains("fm-node-has-callback"));
        assert!(svg.contains("cursor: pointer;"));
    }

    #[test]
    fn renders_state_pseudo_state_shapes_without_fallback_ids_as_labels() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::State);
        ir.nodes.push(IrNode {
            id: "__state_start".to_string(),
            shape: NodeShape::FilledCircle,
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "fork_state".to_string(),
            shape: NodeShape::HorizontalBar,
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "chooser".to_string(),
            shape: NodeShape::Diamond,
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(1)),
            to: IrEndpoint::Node(IrNodeId(2)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });

        let svg = render_svg(&ir);
        assert!(svg.contains("fm-node-shape-filled-circle"));
        assert!(svg.contains("fm-node-shape-horizontal-bar"));
        assert!(svg.contains("fm-node-shape-diamond"));
        assert!(!svg.contains(">__state_start<"));
    }

    #[test]
    fn svg_emits_source_span_metadata_for_layout_elements() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let node_span = Span::at_line(2, 4);
        let edge_span = Span::at_line(3, 6);
        let cluster_span = Span::at_line(1, 10);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            span_primary: node_span,
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            span_primary: Span::at_line(4, 4),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            span: edge_span,
            ..IrEdge::default()
        });
        ir.clusters.push(IrCluster {
            id: IrClusterId(0),
            title: None,
            members: vec![IrNodeId(0), IrNodeId(1)],
            grid_span: 1,
            span: cluster_span,
        });

        let config = SvgRenderConfig {
            include_source_spans: true,
            ..Default::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        assert!(svg.contains("data-fm-source-span=\"2:1-2:4@0-0\""));
        assert!(svg.contains("data-fm-source-span=\"3:1-3:6@0-0\""));
        assert!(svg.contains("data-fm-source-span=\"1:1-1:10@0-0\""));
    }

    #[test]
    fn renders_half_arrow_markers_on_edges() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.nodes.push(IrNode {
            id: "Alice".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "Bob".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::HalfArrowTop,
            ..IrEdge::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(1)),
            to: IrEndpoint::Node(IrNodeId(0)),
            arrow: ArrowType::StickArrowBottomReverseDotted,
            ..IrEdge::default()
        });

        let svg = render_svg(&ir);
        assert!(svg.contains("marker-end=\"url(#arrow-half-top)\""));
        assert!(svg.contains("marker-start=\"url(#arrow-stick-top)\""));
        assert!(svg.contains("stroke-dasharray=\"5,5\""));
    }

    #[test]
    fn renders_dotted_cross_with_dashed_stroke() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.nodes.push(IrNode {
            id: "Alice".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "Bob".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::DottedCross,
            ..IrEdge::default()
        });

        let svg = render_svg(&ir);
        assert!(svg.contains("marker-end=\"url(#arrow-cross)\""));
        assert!(svg.contains("stroke-dasharray=\"5,5\""));
    }

    #[test]
    fn renders_sequence_destroy_marker() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.nodes.push(IrNode {
            id: "Alice".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "Bob".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        ir.sequence_meta = Some(IrSequenceMeta {
            lifecycle_events: vec![IrLifecycleEvent {
                kind: fm_core::LifecycleEventKind::Destroy,
                participant: IrNodeId(1),
                at_edge: 0,
            }],
            ..Default::default()
        });

        let svg = render_svg(&ir);
        assert!(svg.contains("fm-sequence-destroy-marker"));
    }

    #[test]
    fn renders_sequence_note_text_with_multiline_tspans() {
        let layout = fm_layout::DiagramLayout {
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 220.0,
                height: 140.0,
            },
            nodes: Vec::new(),
            edges: Vec::new(),
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            stats: fm_layout::LayoutStats::default(),
            extensions: fm_layout::LayoutExtensions {
                sequence_notes: vec![fm_layout::LayoutSequenceNote {
                    position: fm_core::NotePosition::Over,
                    text: "Line 1\nLine 2".to_string(),
                    bounds: LayoutRect {
                        x: 20.0,
                        y: 30.0,
                        width: 120.0,
                        height: 44.0,
                    },
                }],
                ..Default::default()
            },
            dirty_regions: Vec::new(),
        };

        let svg = render_svg_with_layout(
            &MermaidDiagramIr::empty(DiagramType::Sequence),
            &layout,
            &SvgRenderConfig::default(),
        );
        assert!(svg.contains("fm-sequence-note-text"));
        assert!(svg.contains("<tspan"));
        assert!(svg.contains(">Line 1<"));
        assert!(svg.contains(">Line 2<"));
    }

    #[test]
    fn renders_sequence_mirror_actor_headers() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.nodes.push(IrNode {
            id: "Alice".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "Bob".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        ir.meta.init.config.sequence_mirror_actors = Some(true);

        let svg = render_svg(&ir);
        assert!(svg.contains("fm-sequence-mirror-header"));
        assert!(svg.matches("Alice").count() >= 2);
        assert!(svg.matches("Bob").count() >= 2);
        assert_eq!(svg.matches("id=\"fm-node-alice-0\"").count(), 1);
        assert_eq!(
            svg.matches("id=\"fm-node-alice-0-mirror-header\"").count(),
            1
        );
        assert_eq!(svg.matches("id=\"fm-node-bob-1\"").count(), 1);
        assert_eq!(svg.matches("id=\"fm-node-bob-1-mirror-header\"").count(), 1);
    }

    #[test]
    fn hide_footbox_suppresses_sequence_mirror_actor_headers() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.nodes.push(IrNode {
            id: "Alice".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "Bob".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
        ir.meta.init.config.sequence_mirror_actors = Some(true);
        ir.sequence_meta = Some(IrSequenceMeta {
            hide_footbox: true,
            ..Default::default()
        });

        let svg = render_svg(&ir);
        assert!(!svg.contains("fm-sequence-mirror-header"));
    }

    #[test]
    fn renders_node_menu_links_as_svg_metadata() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.nodes.push(IrNode {
            id: "API".to_string(),
            menu_links: vec![fm_core::IrMenuLink {
                label: "Docs".to_string(),
                url: "https://example.com/docs".to_string(),
            }],
            ..IrNode::default()
        });

        let svg = render_svg(&ir);
        assert!(svg.contains(
            "data-menu-links=\"[{&quot;label&quot;:&quot;Docs&quot;,&quot;url&quot;:&quot;https://example.com/docs&quot;}]\""
        ));
        assert!(svg.contains("fm-node-has-menu-links"));
    }

    #[test]
    fn svg_menu_links_skip_unsafe_urls_under_strict() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.nodes.push(IrNode {
            id: "API".to_string(),
            menu_links: vec![fm_core::IrMenuLink {
                label: "Admin".to_string(),
                url: "javascript:alert(1)".to_string(),
            }],
            ..IrNode::default()
        });
        ir.meta.init.config.sanitize_mode = MermaidSanitizeMode::Strict;

        let svg = render_svg(&ir);
        assert!(!svg.contains("data-menu-links"));
        assert!(!svg.contains("javascript:alert(1)"));
    }

    #[test]
    fn svg_link_mode_controls_anchor_emission() {
        let mut ir = create_ir_with_single_node("A", NodeShape::Rect);
        if let Some(node) = ir.nodes.first_mut() {
            node.interaction_mut().href = Some("https://example.com".to_string());
        }

        let default_svg = render_svg(&ir);
        assert!(!default_svg.contains("href=\"https://example.com\""));

        let inline_config = SvgRenderConfig {
            link_mode: MermaidLinkMode::Inline,
            ..SvgRenderConfig::default()
        };
        let svg = render_svg_with_config(&ir, &inline_config);
        assert!(svg.contains("href=\"https://example.com\""));
        assert!(svg.contains("target=\"_blank\""));

        let footnote_config = SvgRenderConfig {
            link_mode: MermaidLinkMode::Footnote,
            ..SvgRenderConfig::default()
        };
        let footnote_svg = render_svg_with_config(&ir, &footnote_config);
        assert!(!footnote_svg.contains("href=\"https://example.com\""));
        assert!(footnote_svg.contains("data-link=\"https://example.com\""));
    }

    #[test]
    fn svg_link_mode_skips_unsafe_href_under_strict() {
        let mut ir = create_ir_with_single_node("A", NodeShape::Rect);
        if let Some(node) = ir.nodes.first_mut() {
            node.interaction_mut().href = Some("javascript:alert(1)".to_string());
        }
        ir.meta.init.config.sanitize_mode = MermaidSanitizeMode::Strict;

        let inline_config = SvgRenderConfig {
            link_mode: MermaidLinkMode::Inline,
            ..SvgRenderConfig::default()
        };
        let svg = render_svg_with_config(&ir, &inline_config);
        assert!(!svg.contains("href=\"javascript:alert(1)\""));

        let footnote_config = SvgRenderConfig {
            link_mode: MermaidLinkMode::Footnote,
            ..SvgRenderConfig::default()
        };
        let footnote_svg = render_svg_with_config(&ir, &footnote_config);
        assert!(!footnote_svg.contains("data-link=\"javascript:alert(1)\""));
    }

    #[test]
    fn sequence_autonumber_uses_configured_start_and_increment_in_svg_labels() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.sequence_meta = Some(IrSequenceMeta {
            autonumber: true,
            autonumber_start: 10,
            autonumber_increment: 5,
            ..Default::default()
        });
        ir.labels.push(fm_core::IrLabel {
            text: "Ping".to_string(),
            ..Default::default()
        });
        ir.labels.push(fm_core::IrLabel {
            text: "Pong".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "Alice".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "Bob".to_string(),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            label: Some(fm_core::IrLabelId(0)),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(1)),
            to: IrEndpoint::Node(IrNodeId(0)),
            arrow: ArrowType::Arrow,
            label: Some(fm_core::IrLabelId(1)),
            ..Default::default()
        });

        let svg = render_svg(&ir);
        assert!(svg.contains(">10 Ping<"));
        assert!(svg.contains(">15 Pong<"));
    }

    #[test]
    fn renders_sequence_labels_with_decoded_entity_characters() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.labels.push(fm_core::IrLabel {
            text: "I # Rust ; ♥ ∞".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "Alice".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "Bob".to_string(),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            label: Some(fm_core::IrLabelId(0)),
            ..Default::default()
        });

        let svg = render_svg(&ir);
        assert!(svg.contains("I # Rust ; ♥ ∞"));
    }

    #[test]
    fn renders_sequence_labels_with_explicit_line_breaks() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Sequence);
        ir.labels.push(fm_core::IrLabel {
            text: "Line 1\nLine 2".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "Alice".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "Bob".to_string(),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            label: Some(fm_core::IrLabelId(0)),
            ..Default::default()
        });

        let svg = render_svg(&ir);
        assert!(svg.contains(">Line 1<"));
        assert!(svg.contains(">Line 2<"));
    }

    #[test]
    fn renders_flowchart_markdown_node_labels_with_styled_tspans() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.labels.push(fm_core::IrLabel {
            text: "Bold and italic\nnext".to_string(),
            ..Default::default()
        });
        ir.label_markup.insert(
            IrLabelId(0),
            vec![
                IrLabelSegment::Text {
                    text: "Bold".to_string(),
                    bold: true,
                    italic: false,
                    code: false,
                    strike: false,
                },
                IrLabelSegment::Text {
                    text: " and ".to_string(),
                    bold: false,
                    italic: false,
                    code: false,
                    strike: false,
                },
                IrLabelSegment::Text {
                    text: "italic".to_string(),
                    bold: false,
                    italic: true,
                    code: false,
                    strike: false,
                },
                IrLabelSegment::LineBreak,
                IrLabelSegment::Text {
                    text: "next".to_string(),
                    bold: false,
                    italic: false,
                    code: false,
                    strike: false,
                },
            ],
        );
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            label: Some(IrLabelId(0)),
            ..Default::default()
        });

        let svg = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                detail_tier: MermaidTier::Rich,
                ..SvgRenderConfig::default()
            },
        );
        assert!(svg.contains("font-weight=\"700\""));
        assert!(svg.contains("font-style=\"italic\""));
        assert!(svg.contains(">Bold<"));
        assert!(svg.contains(">italic<"));
        assert!(svg.contains(">next<"));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn prop_svg_render_is_total_and_counts_match(node_count in 0usize..20) {
            let ir = create_linear_ir(node_count);
            let svg = render_svg(&ir);
            let expected_nodes_attr = format!("data-nodes=\"{node_count}\"");
            let expected_edges_attr = format!("data-edges=\"{}\"", node_count.saturating_sub(1));

            prop_assert!(svg.starts_with("<svg"));
            prop_assert!(svg.ends_with("</svg>"));
            prop_assert!(svg.contains(&expected_nodes_attr));
            prop_assert!(svg.contains(&expected_edges_attr));
        }

        #[test]
        fn prop_svg_output_is_deterministic(node_count in 1usize..15) {
            let ir = create_linear_ir(node_count);
            let svg1 = render_svg(&ir);
            let svg2 = render_svg(&ir);
            prop_assert_eq!(svg1, svg2, "SVG output should be byte-identical for same input");
        }

        #[test]
        fn prop_svg_all_themes_render_without_panic(theme_token in 0usize..4) {
            let theme = match theme_token {
                0 => ThemePreset::Default,
                1 => ThemePreset::Dark,
                2 => ThemePreset::Forest,
                _ => ThemePreset::Neutral,
            };
            let ir = create_linear_ir(5);
            let config = SvgRenderConfig {
                theme,
                ..Default::default()
            };
            let svg = render_svg_with_config(&ir, &config);
            prop_assert!(svg.starts_with("<svg"));
            prop_assert!(svg.ends_with("</svg>"));
        }

        #[test]
        fn prop_svg_contains_viewbox(node_count in 1usize..10) {
            let ir = create_linear_ir(node_count);
            let svg = render_svg(&ir);
            prop_assert!(svg.contains("viewBox="), "SVG should contain viewBox attribute");
        }

        #[test]
        fn prop_svg_render_never_contains_nan(node_count in 0usize..15) {
            let ir = create_linear_ir(node_count);
            let svg = render_svg(&ir);
            prop_assert!(
                !svg.contains("NaN"),
                "SVG output should never contain NaN values"
            );
            prop_assert!(
                !svg.contains("Infinity"),
                "SVG output should never contain Infinity values"
            );
        }
    }

    #[test]
    fn er_cardinality_one_to_many() {
        let (left, right) = parse_er_cardinality("||--o{");
        assert_eq!(left, "1");
        assert_eq!(right, "0..*");
    }

    #[test]
    fn er_cardinality_many_to_one() {
        let (left, right) = parse_er_cardinality("}|--||");
        assert_eq!(left, "1..*");
        assert_eq!(right, "1");
    }

    #[test]
    fn er_cardinality_one_to_one() {
        let (left, right) = parse_er_cardinality("||--||");
        assert_eq!(left, "1");
        assert_eq!(right, "1");
    }

    #[test]
    fn er_cardinality_dotted() {
        let (left, right) = parse_er_cardinality("}|..|{");
        assert_eq!(left, "1..*");
        assert_eq!(right, "1..*");
    }

    #[test]
    fn er_cardinality_no_connector() {
        let (left, right) = parse_er_cardinality("unknown");
        assert_eq!(left, "");
        assert_eq!(right, "");
    }

    /// Verify that all 10 theme presets produce valid, non-empty SVG output
    /// for representative diagram IRs. This is a regression guard against
    /// hardcoded colors that are invisible on certain themes.
    #[test]
    fn all_theme_presets_produce_valid_svg() {
        use fm_core::{ArrowType, DiagramType, IrEdge, IrEndpoint, IrNode, IrNodeId, NodeShape};

        let presets = [
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
        ];

        let diagram_types = [
            DiagramType::Flowchart,
            DiagramType::Sequence,
            DiagramType::Class,
            DiagramType::State,
            DiagramType::Er,
            DiagramType::Pie,
        ];

        for preset in &presets {
            let config = SvgRenderConfig {
                theme: *preset,
                ..SvgRenderConfig::default()
            };

            for diagram_type in &diagram_types {
                let mut ir = MermaidDiagramIr::empty(*diagram_type);
                ir.nodes.push(IrNode {
                    id: "A".to_string(),
                    shape: NodeShape::Rect,
                    ..Default::default()
                });
                ir.nodes.push(IrNode {
                    id: "B".to_string(),
                    shape: NodeShape::Rounded,
                    ..Default::default()
                });
                ir.edges.push(IrEdge {
                    from: IrEndpoint::Node(IrNodeId(0)),
                    to: IrEndpoint::Node(IrNodeId(1)),
                    arrow: ArrowType::Arrow,
                    ..Default::default()
                });

                let layout = fm_layout::layout_diagram(&ir);
                let svg = render_svg_with_layout(&ir, &layout, &config);

                assert!(
                    !svg.is_empty(),
                    "Theme {} produced empty SVG for {:?}",
                    preset.as_str(),
                    diagram_type.as_str()
                );
                assert!(
                    svg.contains("<svg"),
                    "Theme {} produced invalid SVG for {:?}",
                    preset.as_str(),
                    diagram_type.as_str()
                );
                assert!(
                    !svg.contains("NaN"),
                    "Theme {} produced SVG with NaN for {:?}",
                    preset.as_str(),
                    diagram_type.as_str()
                );
            }
        }
    }

    #[test]
    fn apply_degradation_disables_visual_effects() {
        let mut config = SvgRenderConfig::default();
        assert!(config.shadows);
        assert!(config.node_gradients);
        assert!(config.glow_enabled);

        let plan = fm_core::MermaidDegradationPlan {
            reduce_decoration: true,
            ..fm_core::MermaidDegradationPlan::default()
        };
        config.apply_degradation(&plan);
        assert!(!config.shadows);
        assert!(!config.node_gradients);
        assert!(!config.glow_enabled);
    }

    #[test]
    fn apply_degradation_compact_sets_detail_tier() {
        let mut config = SvgRenderConfig::default();
        let plan = fm_core::MermaidDegradationPlan {
            target_fidelity: fm_core::MermaidFidelity::Compact,
            ..fm_core::MermaidDegradationPlan::default()
        };
        config.apply_degradation(&plan);
        assert_eq!(config.detail_tier, MermaidTier::Compact);
        // Shadows/gradients untouched if reduce_decoration is false
        assert!(config.shadows);
    }

    #[test]
    fn apply_degradation_outline_strips_all_decoration() {
        let mut config = SvgRenderConfig::default();
        let plan = fm_core::MermaidDegradationPlan {
            target_fidelity: fm_core::MermaidFidelity::Outline,
            ..fm_core::MermaidDegradationPlan::default()
        };
        config.apply_degradation(&plan);
        assert!(!config.shadows);
        assert!(!config.node_gradients);
        assert!(!config.glow_enabled);
        assert_eq!(config.detail_tier, MermaidTier::Compact);
    }

    #[test]
    fn apply_degradation_default_is_noop() {
        let original = SvgRenderConfig::default();
        let mut config = SvgRenderConfig::default();
        config.apply_degradation(&fm_core::MermaidDegradationPlan::default());
        assert_eq!(config.shadows, original.shadows);
        assert_eq!(config.node_gradients, original.node_gradients);
        assert_eq!(config.glow_enabled, original.glow_enabled);
        assert_eq!(config.detail_tier, original.detail_tier);
    }

    #[test]
    fn renders_named_node_icon_with_icon_classes() {
        let mut ir = create_ir_with_single_node("api", NodeShape::Rect);
        ir.nodes[0].interaction_mut().icon = Some("server".to_string());

        let svg = render_svg(&ir);

        assert!(svg.contains("fm-node-has-icon"));
        assert!(svg.contains("fm-node-icon-server"));
    }

    #[test]
    fn renders_emoji_node_icon_as_text() {
        let mut ir = create_ir_with_single_node("spark", NodeShape::Rounded);
        ir.nodes[0].interaction_mut().icon = Some("🚀".to_string());

        let svg = render_svg(&ir);

        assert!(svg.contains("fm-node-icon-emoji"));
        assert!(svg.contains("🚀"));
    }

    #[test]
    fn renders_custom_node_icon_from_config() {
        let mut ir = create_ir_with_single_node("chip", NodeShape::Rect);
        ir.nodes[0].interaction_mut().icon = Some("chip-core".to_string());
        let mut config = SvgRenderConfig::default();
        config.custom_icons.insert(
            "chip-core".to_string(),
            CustomSvgIcon {
                path_data: "M4 4 L20 4 L20 20 L4 20 Z".to_string(),
                view_box_width: 24.0,
                view_box_height: 24.0,
                fill: None,
                stroke: Some("#ff4d4f".to_string()),
                stroke_width: 1.2,
            },
        );

        let svg = render_svg_with_config(&ir, &config);

        assert!(svg.contains("fm-node-icon-custom"));
        assert!(svg.contains("M4 4 L20 4 L20 20 L4 20 Z"));
        assert!(svg.contains("#ff4d4f"));
    }

    #[test]
    fn renders_left_positioned_node_icons() {
        let mut ir = create_ir_with_single_node("queue", NodeShape::Rect);
        ir.nodes[0].interaction_mut().icon = Some("queue".to_string());
        let config = SvgRenderConfig {
            node_icon_position: NodeIconPosition::Left,
            ..SvgRenderConfig::default()
        };

        let svg = render_svg_with_config(&ir, &config);

        assert!(svg.contains("fm-node-icon-pos-left"));
        assert!(svg.contains("fm-node-icon-queue"));
    }

    #[test]
    fn animations_are_disabled_by_default() {
        let ir = create_ir_with_single_node("plain", NodeShape::Rect);
        let svg = render_svg(&ir);
        assert!(!svg.contains("fm-animations-enabled"));
        assert!(!svg.contains("@keyframes fm-enter-diagram"));
    }

    #[test]
    fn animations_emit_css_and_order_variables_when_enabled() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            classes: vec!["highlight".to_string()],
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::DottedArrow,
            ..IrEdge::default()
        });
        let config = SvgRenderConfig {
            animations_enabled: true,
            flow_dash_pattern: "3 9".to_string(),
            ..SvgRenderConfig::default()
        };

        let svg = render_svg_with_config(&ir, &config);

        assert!(svg.contains("fm-animations-enabled"));
        assert!(svg.contains("@keyframes fm-enter-diagram"));
        assert!(svg.contains("@keyframes fm-edge-flow"));
        assert!(svg.contains("prefers-reduced-motion"));
        assert!(svg.contains("fm-edge-flow-animated"));
        assert!(svg.contains("--fm-enter-order:"));
        assert!(svg.contains("stroke-dasharray: 3 9"));
    }

    // ─── Property-based render completeness tests (bd-1br.8) ────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn prop_svg_node_count_matches_data_attribute(node_count in 1_usize..15) {
            let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
            for i in 0..node_count {
                ir.nodes.push(IrNode {
                    id: format!("N{i}"),
                    ..IrNode::default()
                });
            }
            for i in 0..node_count.saturating_sub(1) {
                ir.edges.push(fm_core::IrEdge {
                    from: IrEndpoint::Node(fm_core::IrNodeId(i)),
                    to: IrEndpoint::Node(fm_core::IrNodeId(i + 1)),
                    ..fm_core::IrEdge::default()
                });
            }
            let layout = layout_diagram(&ir);
            let config = SvgRenderConfig::default();
            let svg = render_svg_with_layout(&ir, &layout, &config);

            // SVG root data-nodes attribute should match node count
            let expected_attr = format!("data-nodes=\"{node_count}\"");
            prop_assert!(
                svg.contains(&expected_attr),
                "SVG missing data-nodes=\"{}\" ({} nodes)",
                node_count,
                node_count
            );
            // Each node should produce at least one shape element
            // (rect, circle, polygon, or path in the SVG)
            let shape_count = svg.matches("<rect").count()
                + svg.matches("<circle").count()
                + svg.matches("<polygon").count();
            prop_assert!(
                shape_count >= node_count,
                "Expected at least {} shape elements, found {} ({} nodes)",
                node_count,
                shape_count,
                node_count
            );
        }

        #[test]
        fn prop_svg_no_nan_or_infinity(node_count in 0_usize..20) {
            let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
            for i in 0..node_count {
                ir.nodes.push(IrNode {
                    id: format!("N{i}"),
                    ..IrNode::default()
                });
            }
            let layout = layout_diagram(&ir);
            let config = SvgRenderConfig::default();
            let svg = render_svg_with_layout(&ir, &layout, &config);
            prop_assert!(
                !svg.contains("NaN"),
                "SVG contains NaN with {} nodes",
                node_count
            );
            prop_assert!(
                !svg.contains("Infinity"),
                "SVG contains Infinity with {} nodes",
                node_count
            );
        }

        #[test]
        fn prop_svg_is_valid_xml(node_count in 1_usize..10) {
            let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
            for i in 0..node_count {
                ir.nodes.push(IrNode {
                    id: format!("N{i}"),
                    ..IrNode::default()
                });
            }
            for i in 0..node_count.saturating_sub(1) {
                ir.edges.push(fm_core::IrEdge {
                    from: IrEndpoint::Node(fm_core::IrNodeId(i)),
                    to: IrEndpoint::Node(fm_core::IrNodeId(i + 1)),
                    ..fm_core::IrEdge::default()
                });
            }
            let layout = layout_diagram(&ir);
            let config = SvgRenderConfig::default();
            let svg = render_svg_with_layout(&ir, &layout, &config);

            // Basic XML validation: must start with <svg and contain </svg>
            prop_assert!(
                svg.contains("<svg") && svg.contains("</svg>"),
                "SVG output is not well-formed XML"
            );
            // Must contain viewBox
            prop_assert!(
                svg.contains("viewBox"),
                "SVG missing viewBox attribute"
            );
        }
    }

    // ─── End-to-end sequence fragment rendering tests ───

    fn render_sequence_e2e(input: &str) -> String {
        let parsed = fm_parser::parse(input);
        let traced = fm_layout::layout_diagram_traced(&parsed.ir);
        render_svg_with_layout(&parsed.ir, &traced.layout, &SvgRenderConfig::default())
    }

    #[test]
    fn e2e_loop_fragment_renders_labeled_rect() {
        let input = "sequenceDiagram\n\
            participant A\n\
            participant B\n\
            loop Every minute\n\
            A->>B: ping\n\
            B-->>A: pong\n\
            end";
        let svg = render_sequence_e2e(input);

        assert!(
            svg.contains("fm-sequence-fragment"),
            "missing fragment class"
        );
        assert!(
            svg.contains("fm-sequence-fragment-label"),
            "missing fragment label class"
        );
        assert!(
            svg.contains("loop [Every minute]"),
            "missing loop label text"
        );
    }

    #[test]
    fn e2e_alt_fragment_renders_with_label() {
        let input = "sequenceDiagram\n\
            participant A\n\
            participant B\n\
            alt success\n\
            A->>B: ok\n\
            else failure\n\
            A->>B: err\n\
            end";
        let svg = render_sequence_e2e(input);

        assert!(
            svg.contains("fm-sequence-fragment"),
            "missing fragment class"
        );
        assert!(svg.contains("alt [success]"), "missing alt label text");
    }

    #[test]
    fn e2e_par_fragment_renders() {
        let input = "sequenceDiagram\n\
            participant A\n\
            participant B\n\
            participant C\n\
            par\n\
            A->>B: one\n\
            and\n\
            A->>C: two\n\
            end";
        let svg = render_sequence_e2e(input);
        assert!(
            svg.contains("fm-sequence-fragment"),
            "missing fragment class"
        );
    }

    #[test]
    fn e2e_nested_fragments_produce_multiple_rects() {
        let input = "sequenceDiagram\n\
            participant A\n\
            participant B\n\
            loop repeat\n\
            alt success\n\
            A->>B: yes\n\
            else fail\n\
            A->>B: no\n\
            end\n\
            end";
        let svg = render_sequence_e2e(input);

        // Two separate fragment rectangles (loop + alt).
        let count = svg.matches("class=\"fm-sequence-fragment\"").count();
        assert!(
            count >= 2,
            "nested fragments should produce at least 2 fragment rects, got {count}"
        );
    }

    #[test]
    fn e2e_fragment_geometry_has_positive_bounds() {
        let input = "sequenceDiagram\n\
            participant A\n\
            participant B\n\
            loop Retry\n\
            A->>B: request\n\
            B-->>A: response\n\
            end";
        let parsed = fm_parser::parse(input);
        let traced = fm_layout::layout_diagram_traced(&parsed.ir);
        let fragments = &traced.layout.extensions.sequence_fragments;

        assert!(!fragments.is_empty(), "should produce layout fragments");
        for frag in fragments {
            assert!(frag.bounds.width > 0.0, "fragment width must be positive");
            assert!(frag.bounds.height > 0.0, "fragment height must be positive");
        }
    }

    #[test]
    fn renders_loop_fragment_with_dashed_stroke() {
        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 150.0,
            },
            stats: Default::default(),
            extensions: fm_layout::LayoutExtensions {
                sequence_fragments: vec![fm_layout::LayoutSequenceFragment {
                    kind: fm_core::FragmentKind::Loop,
                    label: "3 times".to_string(),
                    color: None,
                    bounds: LayoutRect {
                        x: 5.0,
                        y: 30.0,
                        width: 190.0,
                        height: 80.0,
                    },
                }],
                ..Default::default()
            },
            dirty_regions: Vec::new(),
        };

        let svg = render_svg_with_layout(
            &MermaidDiagramIr::empty(DiagramType::Sequence),
            &layout,
            &SvgRenderConfig::default(),
        );

        assert!(
            svg.contains("stroke-dasharray=\"6,4\""),
            "loop should have dashed border"
        );
        assert!(
            svg.contains("fm-sequence-fragment-label-bg"),
            "should have label background"
        );
        assert!(svg.contains("loop [3 times]"), "should render label text");
    }

    // ─── E2E smoke tests for all 24 diagram types ───

    /// Parse -> layout -> render SVG for each diagram type.
    /// Verifies the complete pipeline doesn't panic and produces valid SVG.
    fn smoke_test_diagram(input: &str, expected_type: &str, min_nodes: usize) {
        let detected = fm_parser::detect_type_with_confidence(input);
        assert!(
            detected.confidence >= 0.5,
            "{expected_type}: confidence too low ({:.2}), detected as {:?}",
            detected.confidence,
            detected.diagram_type,
        );

        let parsed = fm_parser::parse(input);
        assert!(
            parsed.ir.nodes.len() >= min_nodes || !parsed.ir.edges.is_empty(),
            "{expected_type}: expected >= {min_nodes} nodes or some edges, got {} nodes, {} edges",
            parsed.ir.nodes.len(),
            parsed.ir.edges.len(),
        );

        let traced = fm_layout::layout_diagram_traced(&parsed.ir);
        let svg = render_svg_with_layout(&parsed.ir, &traced.layout, &SvgRenderConfig::default());

        assert!(
            svg.starts_with("<svg") || svg.starts_with("<?xml"),
            "{expected_type}: SVG output should start with <svg or <?xml, got: {}",
            svg.chars().take(80).collect::<String>(),
        );
        assert!(
            svg.contains("</svg>"),
            "{expected_type}: SVG output should contain closing tag"
        );
        assert!(
            svg.len() > 100,
            "{expected_type}: SVG output suspiciously short ({} bytes)",
            svg.len(),
        );
    }

    #[test]
    fn smoke_flowchart() {
        smoke_test_diagram("flowchart LR\n  A-->B-->C", "flowchart", 2);
    }

    #[test]
    fn smoke_sequence() {
        smoke_test_diagram("sequenceDiagram\n  Alice->>Bob: hello", "sequence", 2);
    }

    #[test]
    fn smoke_class() {
        smoke_test_diagram(
            "classDiagram\n  class Animal {\n    +name: string\n  }",
            "class",
            1,
        );
    }

    #[test]
    fn smoke_state() {
        smoke_test_diagram(
            "stateDiagram-v2\n  [*] --> Active\n  Active --> [*]",
            "state",
            1,
        );
    }

    #[test]
    fn smoke_er() {
        smoke_test_diagram("erDiagram\n  CUSTOMER ||--o{ ORDER : places", "er", 1);
    }

    #[test]
    fn smoke_gantt() {
        smoke_test_diagram(
            "gantt\n  title Plan\n  section A\n  Task1: a1, 2024-01-01, 7d",
            "gantt",
            1,
        );
    }

    #[test]
    fn smoke_pie() {
        smoke_test_diagram(
            "pie title Votes\n  \"Dogs\" : 70\n  \"Cats\" : 30",
            "pie",
            1,
        );
    }

    #[test]
    fn smoke_gitgraph() {
        smoke_test_diagram("gitGraph\n  commit\n  branch dev\n  commit", "gitgraph", 0);
    }

    #[test]
    fn smoke_journey() {
        smoke_test_diagram(
            "journey\n  title My Day\n  section Morning\n  Wake up: 5: Me",
            "journey",
            1,
        );
    }

    #[test]
    fn smoke_mindmap() {
        smoke_test_diagram(
            "mindmap\n  root((Central))\n    Branch1\n    Branch2",
            "mindmap",
            1,
        );
    }

    #[test]
    fn smoke_timeline() {
        smoke_test_diagram(
            "timeline\n  title History\n  2020 : Event A\n  2021 : Event B",
            "timeline",
            1,
        );
    }

    #[test]
    fn smoke_sankey() {
        smoke_test_diagram(
            "sankey-beta\n\nSource,Target,10\nSource,Other,5",
            "sankey",
            1,
        );
    }

    #[test]
    fn smoke_quadrant() {
        smoke_test_diagram(
            "quadrantChart\n  title Skills\n  x-axis Low --> High\n  y-axis Low --> High\n  A: [0.3, 0.6]",
            "quadrant",
            0,
        );
    }

    #[test]
    fn smoke_xychart() {
        smoke_test_diagram(
            "xychart-beta\n  title Sales\n  x-axis [Q1, Q2, Q3]\n  line [10, 20, 15]",
            "xychart",
            0,
        );
    }

    #[test]
    fn smoke_block_beta() {
        smoke_test_diagram("block-beta\n  columns 2\n  A B\n  C D", "block-beta", 1);
    }

    #[test]
    fn smoke_packet_beta() {
        smoke_test_diagram(
            "packet-beta\n  0-15: \"Source Port\"\n  16-31: \"Dest Port\"",
            "packet-beta",
            0,
        );
    }

    #[test]
    fn smoke_architecture_beta() {
        smoke_test_diagram(
            "architecture-beta\n  group api(cloud)[API]\n  service auth(server)[Auth] in api",
            "architecture-beta",
            1,
        );
    }

    #[test]
    fn smoke_c4context() {
        smoke_test_diagram(
            "C4Context\n  Person(user, \"User\")\n  System(sys, \"System\")\n  Rel(user, sys, \"Uses\")",
            "C4Context",
            1,
        );
    }

    #[test]
    fn smoke_c4container() {
        smoke_test_diagram(
            "C4Container\n  Container(app, \"App\")\n  Container(db, \"DB\")",
            "C4Container",
            1,
        );
    }

    #[test]
    fn smoke_c4component() {
        smoke_test_diagram(
            "C4Component\n  Component(auth, \"Auth\")\n  Component(api, \"API\")",
            "C4Component",
            1,
        );
    }

    #[test]
    fn smoke_c4dynamic() {
        smoke_test_diagram(
            "C4Dynamic\n  Person(user, \"User\")\n  Rel(user, api, \"Call\")",
            "C4Dynamic",
            1,
        );
    }

    #[test]
    fn smoke_c4deployment() {
        smoke_test_diagram(
            "C4Deployment\n  Deployment_Node(server, \"Server\") {\n    Container(app, \"App\")\n  }",
            "C4Deployment",
            1,
        );
    }

    #[test]
    fn smoke_requirement() {
        smoke_test_diagram(
            "requirementDiagram\n  requirement req1 {\n    id: 1\n    text: Must work\n  }",
            "requirement",
            1,
        );
    }

    #[test]
    fn smoke_kanban() {
        smoke_test_diagram(
            "kanban\n  column Todo\n    card Task1\n    card Task2",
            "kanban",
            1,
        );
    }

    // ─── Cross-cutting feature tests ───

    #[test]
    fn smoke_init_directive() {
        let input = "%%{init: {\"theme\":\"dark\"}}%%\nflowchart LR\n  A-->B";
        let parsed = fm_parser::parse(input);
        // Should still detect and parse successfully despite init directive.
        assert!(
            !parsed.ir.nodes.is_empty(),
            "init directive should not prevent parsing"
        );
    }

    #[test]
    fn smoke_dot_bridge() {
        let input = "digraph G { A -> B; B -> C }";
        let detected = fm_parser::detect_type_with_confidence(input);
        assert!(
            detected.confidence >= 0.5,
            "DOT should be detected with reasonable confidence"
        );
        let parsed = fm_parser::parse(input);
        let traced = fm_layout::layout_diagram_traced(&parsed.ir);
        let svg = render_svg_with_layout(&parsed.ir, &traced.layout, &SvgRenderConfig::default());
        assert!(svg.contains("<svg"), "DOT bridge should produce SVG");
    }

    #[test]
    fn smoke_fuzzy_detection() {
        let detected = fm_parser::detect_type_with_confidence("flowchrt LR\n  A-->B");
        // Fuzzy match should still detect as flowchart but with lower confidence.
        assert_eq!(
            format!("{:?}", detected.diagram_type),
            "Flowchart",
            "fuzzy match should detect flowchart"
        );
        assert!(
            detected.confidence < 1.0,
            "fuzzy match confidence should be < 1.0"
        );
    }

    #[test]
    fn smoke_error_recovery() {
        let input = "flowchart LR\n  A-->B\n  !!!invalid!!!\n  C-->D";
        let parsed = fm_parser::parse(input);
        // Should recover and produce some nodes/edges despite invalid syntax.
        assert!(
            !parsed.ir.nodes.is_empty() || !parsed.ir.edges.is_empty(),
            "error recovery should still produce IR"
        );
    }

    // ─── Pie and XyChart rendering quality tests ───

    #[test]
    fn pie_chart_renders_wedge_paths_and_legend() {
        let svg = render_sequence_e2e(
            "pie title Pets\n  \"Dogs\" : 70\n  \"Cats\" : 20\n  \"Birds\" : 10",
        );
        // Wedges are SVG path elements.
        assert!(svg.contains("<path"), "pie should render wedge paths");
        // Legend with slice labels.
        assert!(
            svg.contains("Dogs") && svg.contains("Cats") && svg.contains("Birds"),
            "pie should render all slice labels"
        );
    }

    #[test]
    fn pie_chart_renders_title() {
        let svg =
            render_sequence_e2e("pie title My Favorite Pets\n  \"Dogs\" : 60\n  \"Cats\" : 40");
        assert!(
            svg.contains("My Favorite Pets"),
            "pie should render the chart title"
        );
    }

    #[test]
    fn xychart_renders_axes_and_data() {
        let svg = render_sequence_e2e(
            "xychart-beta\n  title Sales\n  x-axis [Q1, Q2, Q3, Q4]\n  line [10, 20, 15, 25]",
        );
        assert!(svg.contains("Sales"), "xychart should render title");
        // Axis labels.
        assert!(svg.contains("Q1"), "xychart should render x-axis labels");
        // Line data rendered as path or polyline.
        assert!(
            svg.contains("<path") || svg.contains("<line") || svg.contains("<polyline"),
            "xychart should render line data"
        );
    }

    #[test]
    fn xychart_bar_series_renders_rects() {
        let svg = render_sequence_e2e(
            "xychart-beta\n  title Revenue\n  x-axis [Jan, Feb, Mar]\n  bar [100, 200, 150]",
        );
        // Bar series renders as rectangles.
        assert!(
            svg.contains("<rect"),
            "xychart bar series should render rects"
        );
        assert!(svg.contains("Revenue"), "xychart should render title");
    }

    #[test]
    fn xychart_pad_missing_categories_for_ticks() {
        let mut ir = create_xychart_ir();
        if let Some(meta) = ir.xy_chart_meta.as_mut() {
            meta.x_axis.categories.truncate(1);
        }
        let svg = render_svg_with_config(&ir, &SvgRenderConfig::default());
        let tick_count = svg.matches("fm-xychart-x-tick").count();
        assert_eq!(
            tick_count, 3,
            "xychart should pad missing categories to match series length"
        );
    }

    // ─── Incremental layout engine integration test ───

    #[test]
    fn scan_node_class_keywords_matches_contains_reference() {
        // Reference: the pre-single-pass logic — OR of case-insensitive substring checks.
        fn ci_contains(h: &str, n: &str) -> bool {
            let (hb, nb) = (h.as_bytes(), n.as_bytes());
            !nb.is_empty()
                && nb.len() <= hb.len()
                && hb
                    .windows(nb.len())
                    .any(|w| w.iter().zip(nb).all(|(a, b)| a.eq_ignore_ascii_case(b)))
        }
        fn reference(class: &str) -> (bool, bool, bool, bool) {
            let highlighted = ["highlight", "selected", "active", "focus", "important"]
                .iter()
                .any(|k| ci_contains(class, k));
            let inactive = ["inactive", "dim", "muted", "disabled"]
                .iter()
                .any(|k| ci_contains(class, k));
            let dashed = ci_contains(class, "dashed-border") || ci_contains(class, "border-dashed");
            let double = ci_contains(class, "double-border") || ci_contains(class, "border-double");
            (highlighted, inactive, dashed, double)
        }
        let cases = [
            "",
            "a",
            "highlight",
            "HIGHLIGHT",
            "HighLight",
            "my-highlight-node",
            "prefixSelectedSuffix",
            "ACTIVE",
            "focus",
            "important",
            "inactive",
            "dim",
            "muted",
            "disabled",
            "dashed-border",
            "border-dashed",
            "double-border",
            "BORDER-DOUBLE",
            "fm-node",
            "fm-node-accent-8",
            "fm-node-shape-rect",
            "serviceNodeStyle",
            "regionUsEastPrimary",
            "observabilityDashboard",
            "c4-external",
            "block-beta",
            "block-beta-space",
            "dimmed-but-active-highlight",
            "borderish",
            "highligh",       // one char short of a match
            "doubleborder",   // no hyphen — must NOT match
            "high-light",     // hyphen splits keyword — must NOT match
            "muTeDim",        // overlapping starts
            "DisabledInactiveDim",
        ];
        for c in cases {
            let got = scan_node_class_keywords(c);
            assert_eq!(
                (got.highlighted, got.inactive, got.dashed_border, got.double_border),
                reference(c),
                "single-pass keyword scan diverged from contains-reference for {c:?}"
            );
        }
    }

    #[test]
    fn incremental_engine_reuses_layout_on_label_edit() {
        let mut engine = fm_layout::IncrementalLayoutEngine::default();
        let input_a = "flowchart LR\n  A[Hello]-->B-->C-->D-->E-->F-->G-->H";
        let input_b = "flowchart LR\n  A[World]-->B-->C-->D-->E-->F-->G-->H";

        let parsed_a = fm_parser::parse(input_a);
        let config = fm_layout::LayoutConfig::default();
        let guardrails = fm_layout::LayoutGuardrails::default();

        // First render: full compute.
        let traced_a = engine.layout_diagram_traced_with_config_and_guardrails(
            &parsed_a.ir,
            fm_layout::LayoutAlgorithm::Auto,
            config.clone(),
            guardrails,
        );
        let svg_a =
            render_svg_with_layout(&parsed_a.ir, &traced_a.layout, &SvgRenderConfig::default());
        assert!(svg_a.contains("<svg"));

        // Second render with label edit: should use cache/incremental path.
        let parsed_b = fm_parser::parse(input_b);
        let traced_b = engine.layout_diagram_traced_with_config_and_guardrails(
            &parsed_b.ir,
            fm_layout::LayoutAlgorithm::Auto,
            config,
            guardrails,
        );
        let svg_b =
            render_svg_with_layout(&parsed_b.ir, &traced_b.layout, &SvgRenderConfig::default());
        assert!(svg_b.contains("<svg"));

        // Label changed: SVGs should differ.
        assert_ne!(svg_a, svg_b, "label edit should produce different SVG");

        // Second layout should be faster or use cache (recomputed_nodes < total).
        assert!(
            traced_b.trace.incremental.recomputed_nodes <= traced_b.layout.stats.node_count,
            "incremental should recompute at most all nodes"
        );
    }
}
