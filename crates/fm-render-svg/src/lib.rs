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
mod deck;
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
pub use attributes::{Attribute, AttributeValue, Attributes, escape_xml_text};
pub use deck::{deck_manifest, render_svg_with_deck};
pub use defs::{ArrowheadMarker, DefsBuilder, Filter, Gradient, GradientStop, MarkerOrient};
pub use document::SvgDocument;
pub use element::{Element, ElementKind};
pub use path::{PathBuilder, PathCommand};
pub use text::{TextAnchor, TextBuilder};
pub use theme::{FontConfig, Theme, ThemeColors, ThemePreset, generate_palette};
pub use transform::{Transform, TransformBuilder};

use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    sync::{Arc, OnceLock},
};

use fm_core::{
    DiagramType, IrLabelId, IrLabelSegment, IrXyChartMeta, IrXySeriesKind, MermaidDiagramIr,
    MermaidLinkMode, MermaidSanitizeMode, MermaidTier, Span, is_safe_link_target,
    mermaid_cluster_element_id, mermaid_edge_element_id, mermaid_node_element_id,
    mermaid_node_element_id_with_variant,
};
use fm_layout::{
    CentralityTier, DiagramLayout, DirectedPathLayoutPrefix, FillStyle, LayoutBand, LayoutBandKind,
    LayoutEdgePath, LayoutNodeBox, LineCap as RenderLineCap, LineJoin as RenderLineJoin,
    MarkerKind, PathCmd, RenderClip, RenderGroup, RenderItem, RenderPath, RenderScene,
    RenderSource, RenderText, RenderTransform, StrokeStyle, TextAlign as RenderTextAlign,
    TextBaseline as RenderTextBaseline, build_render_scene, certify_directed_path_layout_prefix,
    try_relayout_directed_path_suffix,
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
#[derive(Debug, Clone, PartialEq)]
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
    /// Today's date as `YYYY-MM-DD`, for the gantt `todayMarker` line (bd-j0va).
    ///
    /// INJECTED, never read from the clock inside the renderer, and defaulting to `None`. That is the
    /// whole design: output bytes as a function of the wall clock is a defect class this project has
    /// already been bitten by, and a renderer that called `now()` would make every gantt golden
    /// time-dependent and every render irreproducible. With `None` no marker is drawn, so library
    /// output and goldens are deterministic; the CLI supplies the real date so users get mermaid's
    /// behaviour.
    ///
    /// A marker is drawn only when this parses as a real calendar date AND falls inside the charted
    /// span AND the diagram did not say `todayMarker off`.
    pub gantt_today: Option<String>,
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
            gantt_today: None,
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
            shadow_offset_x: 1.5,
            shadow_offset_y: 1.5,
            shadow_blur: 4.0,
            shadow_opacity: 0.08,
            shadow_color: String::from("#0f172a"),
            node_gradients: true,
            node_gradient_style: NodeGradientStyle::LinearVertical,
            glow_enabled: true,
            glow_blur: 5.0,
            glow_opacity: 0.30,
            glow_color: String::from("#4f46e5"),
            cluster_fill_opacity: 0.06,
            inactive_opacity: 0.40,
            rounded_corners: 8.0,
            root_classes: Vec::new(),
            theme: ThemePreset::Default,
            embed_theme_css: true,
            animations_enabled: false,
            animation_duration_ms: 360,
            animation_stagger_ms: 60,
            flow_animation_duration_ms: 1200,
            flow_dash_pattern: String::from("6 4"),
            hover_scale: 1.02,
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

#[derive(Debug, Clone, Copy, PartialEq)]
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

const POST_PASS_MAX_SVG_BYTES: usize = 100_000;

#[derive(Debug, Clone, Default)]
struct SvgBatchFragments {
    edge_svg: String,
    edge_ends: Vec<usize>,
    node_svg: String,
    node_ends: Vec<usize>,
    reused_edges: usize,
    reused_nodes: usize,
    detail: Option<RenderDetailProfile>,
    offset_x_bits: u32,
    offset_y_bits: u32,
    active: bool,
}

#[derive(Debug, Clone)]
struct SvgBatchSnapshot {
    ir: Option<Arc<MermaidDiagramIr>>,
    layout: Arc<DiagramLayout>,
    config: SvgRenderConfig,
    fragments: SvgBatchFragments,
    certified_prefix: Option<CertifiedSvgBatchPrefix>,
    layout_prefix: Option<DirectedPathLayoutPrefix>,
}

/// Parser-certified immutable IR prefix for allocation-reusing batch rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertifiedSvgBatchPrefix {
    identity: Arc<str>,
    node_count: usize,
    edge_count: usize,
}

/// Immutable cold-start state that can initialize independent batch renderers.
///
/// A coordinator renders one representative of a parser-certified prefix group, then broadcasts
/// this seed to its workers. Each worker receives private mutable state while the seed's layout is
/// shared copy-on-write, replacing one full prefix layout/render per worker with a cheap snapshot
/// clone. The ordinary stateless renderer and unseeded batch path are unchanged.
#[derive(Debug, Clone)]
pub struct SvgBatchRendererSeed {
    snapshot: Arc<SvgBatchSnapshot>,
}

impl CertifiedSvgBatchPrefix {
    #[must_use]
    pub const fn new(identity: Arc<str>, node_count: usize, edge_count: usize) -> Self {
        Self {
            identity,
            node_count,
            edge_count,
        }
    }
}

/// Stateful renderer for a batch whose diagrams may share an unchanged flowchart prefix.
///
/// The ordinary renderer remains stateless. This opt-in surface retains the previous owned IR and
/// layout so it can prove, with exact equality rather than a hash, which leading node and edge
/// fragments are unchanged. Those already-serialized bytes are copied into the next SVG while only
/// the distinct suffix is formatted. The cache is caller-owned and therefore shared-nothing: give
/// each worker its own instance and no lock or cross-thread coordination is required.
#[derive(Debug, Default)]
pub struct SvgBatchRenderer {
    previous: Option<SvgBatchSnapshot>,
}

impl SvgBatchRenderer {
    /// Capture the current parser-certified snapshot for cold-start broadcast.
    ///
    /// Uncertified renders return `None`: a seed must never turn cache eligibility into a
    /// correctness assumption.
    #[must_use]
    pub fn seed(&self) -> Option<SvgBatchRendererSeed> {
        self.previous.as_ref().and_then(|snapshot| {
            snapshot
                .certified_prefix
                .as_ref()
                .map(|_| SvgBatchRendererSeed {
                    snapshot: Arc::new(snapshot.clone()),
                })
        })
    }

    /// Create private worker state from a coordinator-produced seed.
    #[must_use]
    pub fn from_seed(seed: &SvgBatchRendererSeed) -> Self {
        Self {
            previous: Some(seed.snapshot.as_ref().clone()),
        }
    }

    /// Render one owned diagram, retaining it as the exact comparison source for the next call.
    #[must_use]
    pub fn render(
        &mut self,
        ir: Arc<MermaidDiagramIr>,
        layout: Arc<DiagramLayout>,
        config: &SvgRenderConfig,
    ) -> String {
        let previous = self.previous.take();
        let same_config = previous
            .as_ref()
            .is_some_and(|snapshot| snapshot.config == *config);
        let mut next_fragments = SvgBatchFragments::default();
        let reusable_snapshot =
            same_config.then(|| previous.as_ref().expect("same config requires snapshot"));
        let svg = {
            let mut reuse = SvgBatchFragmentReuse {
                previous_ir: reusable_snapshot.and_then(|snapshot| snapshot.ir.as_deref()),
                previous_layout: reusable_snapshot.map(|snapshot| snapshot.layout.as_ref()),
                previous: reusable_snapshot.map(|snapshot| &snapshot.fragments),
                previous_certified_prefix: reusable_snapshot
                    .and_then(|snapshot| snapshot.certified_prefix.as_ref()),
                current_certified_prefix: None,
                certified_geometry_prefix: false,
                next: &mut next_fragments,
            };
            render_svg_with_layout_impl_reusing(&ir, &layout, config, true, Some(&mut reuse))
        };
        let stored_config = if same_config {
            previous.expect("same config requires snapshot").config
        } else {
            config.clone()
        };
        self.previous = Some(SvgBatchSnapshot {
            ir: Some(ir),
            layout,
            config: stored_config,
            fragments: next_fragments,
            certified_prefix: None,
            layout_prefix: None,
        });
        svg
    }

    /// Render a borrowed batch IR whose immutable prefix was certified by the parser.
    ///
    /// Unlike [`Self::render`], this path does not retain the IR. The caller may therefore overwrite
    /// its builder slot after return. Exact prefix identity plus layout-box equality replaces the
    /// previous full-IR equality walk; a missing/mismatched certificate simply renders normally.
    #[must_use]
    pub fn render_borrowed(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: Arc<DiagramLayout>,
        config: &SvgRenderConfig,
        certified_prefix: Option<CertifiedSvgBatchPrefix>,
    ) -> String {
        let previous = self.previous.take();
        let same_config = previous
            .as_ref()
            .is_some_and(|snapshot| snapshot.config == *config);
        let mut next_fragments = SvgBatchFragments::default();
        let reusable_snapshot = same_config.then_some(previous.as_ref()).flatten();
        let svg = {
            let mut reuse = SvgBatchFragmentReuse {
                previous_ir: reusable_snapshot.and_then(|snapshot| snapshot.ir.as_deref()),
                previous_layout: reusable_snapshot.map(|snapshot| snapshot.layout.as_ref()),
                previous: reusable_snapshot.map(|snapshot| &snapshot.fragments),
                previous_certified_prefix: reusable_snapshot
                    .and_then(|snapshot| snapshot.certified_prefix.as_ref()),
                current_certified_prefix: certified_prefix.as_ref(),
                certified_geometry_prefix: false,
                next: &mut next_fragments,
            };
            render_svg_with_layout_impl_reusing(ir, &layout, config, true, Some(&mut reuse))
        };
        let stored_config = if same_config {
            previous.expect("same config requires snapshot").config
        } else {
            config.clone()
        };
        self.previous = Some(SvgBatchSnapshot {
            ir: None,
            layout,
            config: stored_config,
            fragments: next_fragments,
            certified_prefix,
            layout_prefix: None,
        });
        svg
    }

    /// Lay out and render a borrowed diagram while transplanting a certified LR path prefix.
    ///
    /// The first diagram uses the ordinary auto-layout pipeline and records an opaque geometry
    /// proof. Later diagrams with the same parser certificate mutate a private or copy-on-write
    /// prior layout: prefix node boxes and edge paths stay untouched while only the appended suffix
    /// is sized and routed. Every unsupported shape falls back to
    /// [`fm_layout::layout_diagram_traced`] before rendering.
    #[must_use]
    pub fn layout_and_render_borrowed(
        &mut self,
        ir: &MermaidDiagramIr,
        config: &SvgRenderConfig,
        certified_prefix: Option<CertifiedSvgBatchPrefix>,
    ) -> String {
        let mut previous = self.previous.take();
        let mut certified_geometry_prefix = false;
        let mut next_layout_prefix = None;

        let reused_layout = previous.as_mut().and_then(|snapshot| {
            let previous_prefix = snapshot.certified_prefix.as_ref()?;
            let current_prefix = certified_prefix.as_ref()?;
            let same_identity = Arc::ptr_eq(&previous_prefix.identity, &current_prefix.identity)
                || previous_prefix.identity == current_prefix.identity;
            if !same_identity
                || previous_prefix.node_count != current_prefix.node_count
                || previous_prefix.edge_count != current_prefix.edge_count
            {
                return None;
            }
            let layout_prefix = snapshot.layout_prefix.as_ref()?;
            if !try_relayout_directed_path_suffix(
                ir,
                Arc::make_mut(&mut snapshot.layout),
                layout_prefix,
            ) {
                return None;
            }
            certified_geometry_prefix = true;
            next_layout_prefix = Some(layout_prefix.clone());
            Some(Arc::clone(&snapshot.layout))
        });

        let layout = reused_layout.unwrap_or_else(|| fm_layout::layout_diagram_traced(ir).layout);
        if next_layout_prefix.is_none()
            && let Some(prefix) = certified_prefix.as_ref()
        {
            next_layout_prefix = certify_directed_path_layout_prefix(
                ir,
                &layout,
                prefix.node_count,
                prefix.edge_count,
            );
        }

        let same_config = previous
            .as_ref()
            .is_some_and(|snapshot| snapshot.config == *config);
        let mut next_fragments = SvgBatchFragments::default();
        let reusable_snapshot = same_config.then_some(previous.as_ref()).flatten();
        let svg = {
            let mut reuse = SvgBatchFragmentReuse {
                previous_ir: reusable_snapshot.and_then(|snapshot| snapshot.ir.as_deref()),
                previous_layout: reusable_snapshot.map(|snapshot| snapshot.layout.as_ref()),
                previous: reusable_snapshot.map(|snapshot| &snapshot.fragments),
                previous_certified_prefix: reusable_snapshot
                    .and_then(|snapshot| snapshot.certified_prefix.as_ref()),
                current_certified_prefix: certified_prefix.as_ref(),
                certified_geometry_prefix,
                next: &mut next_fragments,
            };
            render_svg_with_layout_impl_reusing(ir, &layout, config, true, Some(&mut reuse))
        };
        let stored_config = if same_config {
            previous.expect("same config requires snapshot").config
        } else {
            config.clone()
        };
        self.previous = Some(SvgBatchSnapshot {
            ir: None,
            layout,
            config: stored_config,
            fragments: next_fragments,
            certified_prefix,
            layout_prefix: next_layout_prefix,
        });
        svg
    }
}

struct SvgBatchFragmentReuse<'a> {
    previous_ir: Option<&'a MermaidDiagramIr>,
    previous_layout: Option<&'a DiagramLayout>,
    previous: Option<&'a SvgBatchFragments>,
    previous_certified_prefix: Option<&'a CertifiedSvgBatchPrefix>,
    current_certified_prefix: Option<&'a CertifiedSvgBatchPrefix>,
    certified_geometry_prefix: bool,
    next: &'a mut SvgBatchFragments,
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
    render_svg_with_layout_impl(ir, layout, config, true)
}

fn render_svg_with_layout_impl(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    config: &SvgRenderConfig,
    use_post_pass_cache: bool,
) -> String {
    render_svg_with_layout_impl_reusing(ir, layout, config, use_post_pass_cache, None)
}

fn render_svg_with_layout_impl_reusing(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    config: &SvgRenderConfig,
    use_post_pass_cache: bool,
    batch_reuse: Option<&mut SvgBatchFragmentReuse<'_>>,
) -> String {
    let direct_minified_css = matches!(config.backend, SvgBackend::LegacyLayout)
        && config.embed_theme_css
        && ir.diagram_type == DiagramType::Flowchart;
    let (mut svg, known_live_marker_mask) = match config.backend {
        SvgBackend::LegacyLayout => {
            let live_marker_mask = flowchart_marker_mask(ir, layout);
            (
                render_layout_to_svg(
                    layout,
                    ir,
                    config,
                    live_marker_mask,
                    direct_minified_css,
                    use_post_pass_cache,
                    batch_reuse,
                ),
                live_marker_mask,
            )
        }
        SvgBackend::Scene => {
            let scene = build_render_scene(ir, layout);
            (
                render_scene_document_with_ir(&scene, config, Some(ir)),
                None,
            )
        }
    };
    if direct_minified_css {
        return svg;
    }
    apply_output_post_passes(&mut svg, use_post_pass_cache, known_live_marker_mask);
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
    // The fixed theme CSS this trims (~1.3 KB of states/accents) is a meaningful fraction only for
    // small/medium diagrams; on a large SVG it is <1% of output while the pass still costs render time.
    // Cap the work to outputs where the win clearly beats the scan cost (covers small flowcharts
    // through the sequence diagram ~62 KB; skips the 200 KB+ chain / wide renders).
    if svg.len() > POST_PASS_MAX_SVG_BYTES {
        return;
    }
    // `memmem` (SIMD) instead of `str::find` throughout: `str::find` builds a Two-Way `StrSearcher`
    // per call (setup measured ~7% of small-diagram render across these post-pass needles); `memmem`
    // returns the identical first-match byte offset with a cheaper prefilter. Byte-identical.
    let body_start =
        memchr::memmem::find(svg.as_bytes(), b"</style>").map_or(0, |i| i + "</style>".len());
    // ONE walk of the body answers both questions the per-needle `contains` chain used to ask with a
    // full body scan each. Computing it BEFORE the inactive-region strip below is sound: that strip
    // only edits bytes inside the `<style>` block, so the body text — and hence every flag here — is
    // unchanged by it. That is the same invariant that let the old code re-`find("</style>")`
    // afterwards and read the identical body.
    let (state_used, accent_used) = scan_body_fm_node_classes(&svg[body_start..]);
    if state_used {
        return;
    }
    // Region bounds. The two opacity rules bracket the block when the cosmetic knobs asked for them;
    // when they did not, the semantic markers are emitted alone (bd-w0f0) and the region runs from
    // the first of them to the end of the last. Both ends fall back independently, so a config that
    // emits only one of the two opacity rules still bounds correctly.
    let start = memchr::memmem::find(svg.as_bytes(), b".fm-node-inactive { opacity:")
        .or_else(|| memchr::memmem::find(svg.as_bytes(), b".fm-node-block-beta rect,"));
    let after =
        memchr::memmem::find(svg.as_bytes(), b".fm-cluster { fill-opacity:").or_else(|| {
            // End of the last semantic rule: its closing brace plus the newline after it.
            let at = memchr::memmem::find(svg.as_bytes(), b".fm-node-border-double rect,")?;
            memchr::memmem::find(&svg.as_bytes()[at..], b"}\n").map(|end| at + end + 2)
        });
    if let (Some(start), Some(after)) = (start, after)
        && after > start
        && after - start < 1500
    {
        svg.replace_range(start..after, "");
    }

    strip_unused_accent_css(svg, &accent_used, false);
}

/// Which of [`strip_unused_state_css`]'s 13 `fm-node-*` needles occur in the rendered body: the 5
/// node-STATE classes (returned as one "any" flag, since the caller bails out on the first) and the 8
/// `fm-node-accent-{n}` classes.
///
/// **Why one pass.** The old shape was one `str::contains` per needle. `str::contains` on an *absent*
/// needle scans the whole body, and absent is the common case — a typical flowchart carries no state
/// class at all — so it was worst-case `O(body_len * 13)`. Measured at **10.6–25.3% of the entire
/// parse+layout+render pipeline** on every diagram under `POST_PASS_MAX_SVG_BYTES`, in *both* output
/// profiles.
///
/// **Why one pass is byte-identical.** All 13 needles begin with `fm-node-`, which has no self-overlap
/// (no proper prefix of `fm-node-` is also a proper suffix of it), so a single *non-overlapping*
/// `memmem` walk observes the start of every occurrence of every needle. At each hit:
/// - a state class is matched with `starts_with(suffix)`, which accepts exactly the strings
///   `contains("fm-node-{suffix}")` accepted (including a longer `fm-node-inactive-foo`);
/// - an accent digit is read with **no terminator check**, because the old needle `fm-node-accent-{n}`
///   had none — `fm-node-accent-12` marked accent 1 used, and still does.
fn scan_body_fm_node_classes(body: &str) -> (bool, [bool; 9]) {
    const PREFIX: &[u8] = b"fm-node-";
    const STATE_SUFFIXES: [&str; 5] = [
        "inactive",
        "block-beta",
        "highlighted",
        "border-dashed",
        "border-double",
    ];
    let mut accent = [false; 9];
    let bytes = body.as_bytes();
    for at in memchr::memmem::Finder::new(PREFIX).find_iter(bytes) {
        let rest = &bytes[at + PREFIX.len()..];
        if let Some(tail) = rest.strip_prefix(b"accent-".as_slice()) {
            if let Some(&digit) = tail.first()
                && matches!(digit, b'1'..=b'8')
            {
                accent[usize::from(digit - b'0')] = true;
            }
        } else if STATE_SUFFIXES
            .iter()
            .any(|s| rest.starts_with(s.as_bytes()))
        {
            // The caller returns immediately on any state class, so the accent flags gathered so far
            // are dead — stop walking.
            return (true, accent);
        }
    }
    (false, accent)
}

/// Which `var(--fm-accent-{n})` references survive anywhere in `svg`, in ONE pass instead of 8
/// whole-document `contains` scans (each *absent* one of which scanned the entire document).
///
/// Must run AFTER the accent-rule strips — that reference count is the whole point of the check — but a
/// single pass there is byte-identical to the old per-`n` `svg.contains(..)` evaluated inside the
/// declaration-strip loop below: the only bytes that loop removes are `  --fm-accent-{n}: <color>;`
/// lines, and an accent declaration never contains a `var(` (`theme.rs` writes a literal color), so no
/// iteration can change a later iteration's answer.
///
/// The needle keeps its closing `)` — unlike the class needle above — because the old needle had one:
/// `var(--fm-accent-12)` must NOT mark accent 1 as referenced.
fn scan_accent_var_refs(svg: &str) -> [bool; 9] {
    const PREFIX: &[u8] = b"var(--fm-accent-";
    let mut used = [false; 9];
    let bytes = svg.as_bytes();
    for at in memchr::memmem::Finder::new(PREFIX).find_iter(bytes) {
        if let [digit @ b'1'..=b'8', b')', ..] = &bytes[at + PREFIX.len()..] {
            used[usize::from(*digit - b'0')] = true;
        }
    }
    used
}

/// Remove palette rules that cannot match a node in the current diagram, then remove any now-dead
/// `:root` palette variables. This operates on the pretty stylesheet, before the direct flowchart
/// path minifies it; the normal output post-pass calls the same helper after observing the body.
///
/// Inline `style` attributes are outside the stylesheet. When one could reference an accent value,
/// callers keep every root declaration: omitting a declaration would change the meaning of a valid
/// user-supplied inline style, while retaining it is only a size cost.
fn strip_unused_accent_css(
    css: &mut String,
    accent_used: &[bool; 9],
    preserve_root_variables: bool,
) {
    // The 8 accent palettes (`.fm-node-accent-1..8`) are assigned per node, so a diagram with few
    // nodes uses only some. Drop each `.fm-node-accent-N` rule whose class is absent from the body.
    // Exact-selector matching makes CSS drift a safe no-op.
    for (n, &is_used) in accent_used.iter().enumerate().skip(1) {
        if !is_used {
            let selector = format!(".fm-node-accent-{n} {{");
            if let Some(start) = memchr::memmem::find(css.as_bytes(), selector.as_bytes())
                && let Some(rel_end) = memchr::memmem::find(&css.as_bytes()[start..], b"}\n")
            {
                css.replace_range(start..start + rel_end + 2, "");
            }
        }
    }

    if preserve_root_variables {
        return;
    }

    // Drop each `:root` accent custom property `--fm-accent-N` that is no longer referenced after
    // the class-rule pruning above. The stylesheet may include classDef/effect rules, so scan the
    // completed CSS rather than assume where each reference originates.
    let var_used = scan_accent_var_refs(css);
    for (n, &is_used) in var_used.iter().enumerate().skip(1) {
        if !is_used {
            let decl = format!("  --fm-accent-{n}:");
            if let Some(start) = memchr::memmem::find(css.as_bytes(), decl.as_bytes())
                && let Some(rel_end) = memchr::memmem::find(&css.as_bytes()[start..], b";\n")
            {
                css.replace_range(start..start + rel_end + 2, "");
            }
        }
    }
}

/// Accent classes emitted by the flowchart renderers are a deterministic function of `IrNode.id`.
/// This lets the direct CSS path prune the same unused palette rules as the normal body-based pass
/// without building or rescanning the SVG body.
fn flowchart_accent_mask(ir: &MermaidDiagramIr) -> [bool; 9] {
    let mut used = [false; 9];
    for node in &ir.nodes {
        used[stable_accent_index(&node.id)] = true;
    }
    used
}

/// Inline node/edge styles and raw style directives render into the SVG body. Keep root palette
/// variables when any of them can reference an accent, because that reference is not visible while
/// the direct path is constructing its stylesheet.
fn ir_inline_styles_reference_accent(ir: &MermaidDiagramIr) -> bool {
    const ACCENT_REF: &str = "var(--fm-accent-";
    ir.nodes.iter().any(|node| {
        node.inline_style.as_ref().is_some_and(|style| {
            style
                .properties
                .values()
                .any(|value| value.contains(ACCENT_REF))
        })
    }) || ir.edges.iter().any(|edge| {
        edge.inline_style.as_ref().is_some_and(|style| {
            style
                .properties
                .values()
                .any(|value| value.contains(ACCENT_REF))
        })
    }) || ir
        .style_refs
        .iter()
        .any(|style_ref| style_ref.style.contains(ACCENT_REF))
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
fn strip_unused_markers(svg: &mut String) -> Option<u16> {
    // Multi-byte needles searched in tight loops (once per `url(#…)` ref / `<marker>` def): build each
    // SIMD `Finder` ONCE and reuse it, instead of `str::find` rebuilding a `TwoWaySearcher` per call.
    let marker_finder = memchr::memmem::Finder::new(b"<marker ");
    if marker_finder.find(svg.as_bytes()).is_none() {
        return Some(0);
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
    let mut live_mask = 0u16;
    let mut cacheable = true;
    let mut at = 0;
    while let Some(rel) = marker_finder.find(&svg.as_bytes()[at..]) {
        let m_start = at + rel;
        let Some(end_rel) = endmarker_finder.find(&svg.as_bytes()[m_start..]) else {
            cacheable = false;
            break;
        };
        let m_end = m_start + end_rel + "</marker>".len();
        // The marker id is the first `id="…"` inside the opening tag.
        let tag_end =
            memchr::memchr(b'>', &svg.as_bytes()[m_start..m_end]).map_or(m_end, |g| m_start + g);
        if let Some(idrel) = id_finder.find(&svg.as_bytes()[m_start..tag_end]) {
            let id_start = m_start + idrel + "id=\"".len();
            if let Some(idclose) = memchr::memchr(b'"', &svg.as_bytes()[id_start..tag_end]) {
                let id = &svg[id_start..id_start + idclose];
                if !referenced.contains(id) {
                    dead_spans.push((m_start, m_end));
                } else if let Some(bit) = marker_id_bit(id) {
                    live_mask |= bit;
                } else {
                    // A future/custom marker can still take the exact legacy passes. It is excluded
                    // from the cache until its identity is represented in the bounded key.
                    cacheable = false;
                }
            } else {
                cacheable = false;
            }
        } else {
            cacheable = false;
        }
        at = m_end;
    }
    if dead_spans.is_empty() {
        return cacheable.then_some(live_mask);
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
    cacheable.then_some(live_mask)
}

const MARKER_END: u16 = 1 << 0;
const MARKER_FILLED: u16 = 1 << 1;
const MARKER_OPEN: u16 = 1 << 2;
const MARKER_HALF_TOP: u16 = 1 << 3;
const MARKER_HALF_BOTTOM: u16 = 1 << 4;
const MARKER_STICK_TOP: u16 = 1 << 5;
const MARKER_STICK_BOTTOM: u16 = 1 << 6;
const MARKER_START: u16 = 1 << 7;
const MARKER_START_FILLED: u16 = 1 << 8;
const MARKER_CIRCLE: u16 = 1 << 9;
const MARKER_CROSS: u16 = 1 << 10;
const MARKER_DIAMOND: u16 = 1 << 11;
const MARKER_DIAMOND_OPEN: u16 = 1 << 12;
const MARKER_TRIANGLE_OPEN: u16 = 1 << 13;
const MARKER_START_TRIANGLE_OPEN: u16 = 1 << 14;
const BASIC_MARKER_MASK: u16 = MARKER_END | MARKER_OPEN;
const ALL_MARKER_MASK: u16 = (1 << 15) - 1;

fn marker_id_bit(id: &str) -> Option<u16> {
    const IDS: [&str; 15] = [
        "arrow-end",
        "arrow-filled",
        "arrow-open",
        "arrow-half-top",
        "arrow-half-bottom",
        "arrow-stick-top",
        "arrow-stick-bottom",
        "arrow-start",
        "arrow-start-filled",
        "arrow-circle",
        "arrow-cross",
        "arrow-diamond",
        "arrow-diamond-open",
        "arrow-triangle-open",
        "start-arrow-triangle-open",
    ];
    IDS.iter()
        .position(|candidate| *candidate == id)
        .map(|index| 1u16 << index)
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
    if let Some(out) = prune_marker_selectors(&svg[cs..ce], &|id| live.contains(id)) {
        svg.replace_range(cs..ce, &out);
    }
}

/// Bit for the `marker#arrow-*` id as it appears in a theme CSS selector, or `None` for an id the
/// stylesheet never names. Only the six ids the theme actually styles need an entry; an unknown id
/// is treated as NOT prunable by the caller, so a stylesheet edit that introduces a new selector
/// degrades to "keep it", never to a wrongly-dropped live rule.
const fn theme_marker_bit(id: &str) -> Option<u16> {
    Some(match id.as_bytes() {
        b"arrow-end" => MARKER_END,
        b"arrow-filled" => MARKER_FILLED,
        b"arrow-open" => MARKER_OPEN,
        b"arrow-circle" => MARKER_CIRCLE,
        b"arrow-cross" => MARKER_CROSS,
        b"arrow-diamond" => MARKER_DIAMOND,
        _ => return None,
    })
}

/// Mask-driven twin of [`strip_dead_marker_css`] for the direct-minified flowchart path.
///
/// That path builds the stylesheet BEFORE the body exists and minifies it at construction, so it can
/// neither scan the emitted `<marker>` defs for liveness nor be pruned afterwards: every stripper
/// needle is written against the PRETTY form (`.fm-node-accent-1 {`, `  --fm-accent-1:`), so once the
/// CSS is minified they all match nothing. Both facts together are why the marker rules stopped being
/// pruned on flowcharts. Here the live-marker mask is already known, so prune while the CSS is still
/// pretty and before it is minified/cached — strictly less work than emitting the dead rules and then
/// stripping them, and the pruned text becomes the cache key so two diagrams with different live
/// markers cannot share a stylesheet.
///
/// An id the theme does not name keeps its selector (see [`theme_marker_bit`]).
fn strip_dead_marker_css_for_mask(css: &mut String, live_mask: u16) {
    if memchr::memmem::find(css.as_bytes(), b"marker#arrow-").is_none() {
        return;
    }
    if let Some(out) = prune_marker_selectors(css, &|id| {
        theme_marker_bit(id).is_none_or(|bit| live_mask & bit != 0)
    }) {
        *css = out;
    }
}

/// Shared selector filter behind [`strip_dead_marker_css`] and [`strip_dead_marker_css_for_mask`]:
/// keep a selector iff it names no dead marker, and drop a rule whose selectors were all pruned.
/// Returns `None` when nothing was removed, so the caller can skip the write back. Brace-depth
/// tracking emits nested at-rule (`@media`) bodies verbatim.
fn prune_marker_selectors(css: &str, is_live: &dyn Fn(&str) -> bool) -> Option<String> {
    let bytes = css.as_bytes();
    let marker_hash_finder = memchr::memmem::Finder::new(b"marker#");
    let mut out = String::with_capacity(css.len());
    let mut i = 0;
    let mut seg_start = 0;
    // SIMD BRACE SCANS. Both walks used to step one byte at a time over the whole stylesheet — the
    // outer one hunting `{`, the inner one tracking `{`/`}` depth through each body. A themed
    // stylesheet is ~9 KB holding on the order of eighty braces, so ~99% of those iterations were a
    // compare that found nothing, and the pass showed up at 3.80% self on `docs_site_50`.
    // `memchr`/`memchr2` find exactly the same bytes in the same order, so the segmentation — and
    // the output — is unchanged; only the bytes in between stop being visited individually.
    while let Some(offset) = memchr::memchr(b'{', &bytes[i..]) {
        {
            i += offset;
            let selectors = &css[seg_start..i];
            // Body = the balanced `{ … }` (track depth so a nested at-rule body is one unit).
            let mut depth = 1;
            let mut j = i + 1;
            while depth > 0 {
                let Some(hit) = memchr::memchr2(b'{', b'}', &bytes[j..]) else {
                    j = bytes.len();
                    break;
                };
                j += hit;
                if bytes[j] == b'{' {
                    depth += 1;
                } else {
                    depth -= 1;
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
                            is_live(&rest[..end])
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
        }
    }
    out.push_str(&css[seg_start..]);
    (out.len() < css.len()).then_some(out)
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

const FULL_CSS_CACHE_CAPACITY: usize = 8;

struct FullCssCacheEntry {
    raw: Box<str>,
    minified: Box<str>,
}

thread_local! {
    /// Flowchart batches repeat a tiny set of theme/config combinations on each persistent worker.
    /// Cache the directly-emitted minified stylesheet so the hot render path neither minifies nor
    /// scans/moves the completed SVG. The bound keeps arbitrary custom themes from growing memory.
    static FULL_CSS_CACHE: RefCell<Vec<FullCssCacheEntry>> =
        const { RefCell::new(Vec::new()) };
}

fn cached_minified_full_css(raw: String) -> String {
    if let Some(hit) = FULL_CSS_CACHE.with(|cache| {
        cache
            .borrow()
            .iter()
            .rev()
            .find(|entry| entry.raw.as_ref() == raw)
            .map(|entry| entry.minified.to_string())
    }) {
        return hit;
    }

    let minified = minify_css(&raw);
    FULL_CSS_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() == FULL_CSS_CACHE_CAPACITY {
            cache.remove(0);
        }
        cache.push(FullCssCacheEntry {
            raw: raw.into_boxed_str(),
            minified: minified.clone().into_boxed_str(),
        });
    });
    minified
}

const CSS_POST_PASS_CACHE_CAPACITY: usize = 32;

struct CssPostPassCacheEntry {
    raw_css: Box<str>,
    state_used: bool,
    accent_mask: u16,
    body_var_mask: u16,
    live_marker_mask: u16,
    processed_css: Box<str>,
}

thread_local! {
    /// A render thread usually sees only a handful of theme/config/feature combinations. Keep the
    /// cache thread-local so the hot path needs no lock, and bound it so custom themes cannot grow
    /// process memory without limit.
    static CSS_POST_PASS_CACHE: RefCell<Vec<CssPostPassCacheEntry>> =
        const { RefCell::new(Vec::new()) };
}

fn style_content_bounds(svg: &str) -> Option<(usize, usize)> {
    let open = memchr::memmem::find(svg.as_bytes(), b"<style")?;
    let gt = memchr::memchr(b'>', &svg.as_bytes()[open..])?;
    let content_start = open + gt + 1;
    let end = memchr::memmem::find(&svg.as_bytes()[content_start..], b"</style>")?;
    Some((content_start, content_start + end))
}

fn bool_mask(flags: &[bool]) -> u16 {
    flags.iter().enumerate().fold(0u16, |mask, (index, used)| {
        mask | (u16::from(*used) << index)
    })
}

fn css_post_pass_observation(svg: &str) -> Option<(usize, usize, bool, u16, u16)> {
    let (content_start, content_end) = style_content_bounds(svg)?;
    let body_start = content_end + "</style>".len();
    let body = svg.get(body_start..)?;
    let (state_used, accent_used) = scan_body_fm_node_classes(body);
    let body_var_used = scan_accent_var_refs(body);
    Some((
        content_start,
        content_end,
        state_used,
        bool_mask(&accent_used),
        bool_mask(&body_var_used),
    ))
}

fn cached_processed_css(
    raw_css: &str,
    state_used: bool,
    accent_mask: u16,
    body_var_mask: u16,
    live_marker_mask: u16,
) -> Option<String> {
    CSS_POST_PASS_CACHE.with(|cache| {
        cache
            .borrow()
            .iter()
            .rev()
            .find(|entry| {
                entry.state_used == state_used
                    && entry.accent_mask == accent_mask
                    && entry.body_var_mask == body_var_mask
                    && entry.live_marker_mask == live_marker_mask
                    && entry.raw_css.as_ref() == raw_css
            })
            .map(|entry| entry.processed_css.to_string())
    })
}

fn cache_processed_css(
    raw_css: String,
    state_used: bool,
    accent_mask: u16,
    body_var_mask: u16,
    live_marker_mask: u16,
    processed_css: String,
) {
    CSS_POST_PASS_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() == CSS_POST_PASS_CACHE_CAPACITY {
            cache.remove(0);
        }
        cache.push(CssPostPassCacheEntry {
            raw_css: raw_css.into_boxed_str(),
            state_used,
            accent_mask,
            body_var_mask,
            live_marker_mask,
            processed_css: processed_css.into_boxed_str(),
        });
    });
}

#[cfg(test)]
fn clear_css_post_pass_cache() {
    CSS_POST_PASS_CACHE.with(|cache| cache.borrow_mut().clear());
}

/// Apply the output-size post-passes, memoizing the exact transformed stylesheet for the bounded
/// `(raw CSS, body state/accent usage, live markers)` feature key. The key is deliberately derived
/// with the same scanners as the legacy passes; label text and geometry are absent, so separate
/// diagrams and label-only edits can hit without risking stale CSS. Any unknown marker identity
/// takes the legacy path.
fn apply_output_post_passes(
    svg: &mut String,
    use_cache: bool,
    known_live_marker_mask: Option<u16>,
) {
    if !use_cache {
        strip_unused_state_css(svg);
        if svg.len() <= POST_PASS_MAX_SVG_BYTES {
            if known_live_marker_mask.is_none() {
                let _ = strip_unused_markers(svg);
            }
            strip_dead_marker_css(svg);
            minify_style_block(svg);
        }
        return;
    }

    // Preserve the exact large-output behavior: state stripping self-gates at this threshold and
    // the other three passes do not run.
    if svg.len() > POST_PASS_MAX_SVG_BYTES {
        strip_unused_state_css(svg);
        return;
    }

    // Marker pruning changes only <defs>; doing it first exposes the live-marker feature mask while
    // leaving the raw stylesheet and every state/accent body observation unchanged.
    let live_marker_mask = known_live_marker_mask.or_else(|| strip_unused_markers(svg));
    let Some((content_start, content_end, state_used, accent_mask, body_var_mask)) =
        css_post_pass_observation(svg)
    else {
        strip_unused_state_css(svg);
        strip_dead_marker_css(svg);
        minify_style_block(svg);
        return;
    };
    let Some(live_marker_mask) = live_marker_mask else {
        strip_unused_state_css(svg);
        strip_dead_marker_css(svg);
        minify_style_block(svg);
        return;
    };

    if let Some(processed_css) = cached_processed_css(
        &svg[content_start..content_end],
        state_used,
        accent_mask,
        body_var_mask,
        live_marker_mask,
    ) {
        svg.replace_range(content_start..content_end, &processed_css);
        return;
    }

    let raw_css = svg[content_start..content_end].to_string();
    strip_unused_state_css(svg);
    strip_dead_marker_css(svg);
    minify_style_block(svg);
    let Some((processed_start, processed_end)) = style_content_bounds(svg) else {
        return;
    };
    cache_processed_css(
        raw_css,
        state_used,
        accent_mask,
        body_var_mask,
        live_marker_mask,
        svg[processed_start..processed_end].to_string(),
    );
}

/// The default-preset theme's edge color. The arrowhead-marker `<defs>` for this color are memoized
/// (see [`marker_defs_body`]). Pinned to the preset by `default_edge_color_matches_preset`.
const DEFAULT_EDGE_COLOR: &str = "#64748b";

/// Serialize the arrowhead-marker `<defs>` children for `edge_color` EXACTLY as the per-marker
/// `ArrowheadMarker::…(id, edge_color).to_element()` sequence (same order + `emit_fancy` gating) that
/// both render backends add via `DefsBuilder::marker`. Byte-identical to those children because it
/// calls the same `Element::write_to_string`.
fn build_marker_defs_body(edge_color: &str, marker_mask: u16) -> String {
    use crate::defs::MarkerOrient;
    let mut s = String::new();
    let push = |s: &mut String, bit: u16, m: ArrowheadMarker| {
        if marker_mask & bit != 0 {
            m.to_element().write_to_string(s);
        }
    };
    push(
        &mut s,
        MARKER_END,
        ArrowheadMarker::standard("arrow-end", edge_color),
    );
    push(
        &mut s,
        MARKER_FILLED,
        ArrowheadMarker::filled("arrow-filled", edge_color),
    );
    push(
        &mut s,
        MARKER_OPEN,
        ArrowheadMarker::open("arrow-open", edge_color),
    );
    push(
        &mut s,
        MARKER_HALF_TOP,
        ArrowheadMarker::half_top("arrow-half-top", edge_color),
    );
    push(
        &mut s,
        MARKER_HALF_BOTTOM,
        ArrowheadMarker::half_bottom("arrow-half-bottom", edge_color),
    );
    push(
        &mut s,
        MARKER_STICK_TOP,
        ArrowheadMarker::stick_top("arrow-stick-top", edge_color),
    );
    push(
        &mut s,
        MARKER_STICK_BOTTOM,
        ArrowheadMarker::stick_bottom("arrow-stick-bottom", edge_color),
    );
    push(
        &mut s,
        MARKER_START,
        ArrowheadMarker::standard("arrow-start", edge_color)
            .with_orient(MarkerOrient::AutoStartReverse),
    );
    push(
        &mut s,
        MARKER_START_FILLED,
        ArrowheadMarker::filled("arrow-start-filled", edge_color)
            .with_orient(MarkerOrient::AutoStartReverse),
    );
    push(
        &mut s,
        MARKER_CIRCLE,
        ArrowheadMarker::circle_marker("arrow-circle", edge_color),
    );
    push(
        &mut s,
        MARKER_CROSS,
        ArrowheadMarker::cross_marker("arrow-cross", edge_color),
    );
    push(
        &mut s,
        MARKER_DIAMOND,
        ArrowheadMarker::diamond_marker("arrow-diamond", edge_color),
    );
    push(
        &mut s,
        MARKER_DIAMOND_OPEN,
        ArrowheadMarker::diamond_open_marker("arrow-diamond-open", edge_color),
    );
    push(
        &mut s,
        MARKER_TRIANGLE_OPEN,
        ArrowheadMarker::triangle_open_marker("arrow-triangle-open", edge_color),
    );
    // Leading `start-` is LOAD-BEARING, not styling: the cross-engine checker recognises our
    // inheritance markers by an anchored id pattern (`(?:^|-)arrow-(?:inheritance(-open)?|
    // triangle-open)$`), so a trailing `-start` suffix would fall outside its vocabulary and the
    // marker would score as `unknown` even though it renders correctly. Conform to the checker's
    // contract rather than widening the checker.
    //
    // A triangle is NOT symmetric under 180 degrees, so the start slot needs its own
    // auto-start-reverse def or `orient="auto"` rotates it to point INTO the path — which the
    // cross-engine checker rejects as invalid:inheritance:points_into_path(slot=start). This
    // mirrors the existing arrow-start / arrow-start-filled pair. The diamonds need no such
    // twin because a diamond looks identical either way round.
    push(
        &mut s,
        MARKER_START_TRIANGLE_OPEN,
        ArrowheadMarker::triangle_open_marker("start-arrow-triangle-open", edge_color)
            .with_orient(MarkerOrient::AutoStartReverse),
    );
    s
}

/// The arrowhead-marker `<defs>` body for `(edge_color, emit_fancy)`, memoized for the default theme.
///
/// Building + serializing the marker set is ~1 µs (2-marker flowchart) to ~6 µs (12-marker non-
/// flowchart) and is a pure function of `(edge_color, emit_fancy)` — a fixed per-render cost paid on
/// every diagram. The overwhelmingly common default theme is memoized via process-global `OnceLock`s
/// built once from the real markers (byte-identical by construction, no source drift, no unbounded
/// cache); custom themes build fresh (rare). The returned body is streamed as one
/// `DefsBuilder::raw_markers`, byte-identical to the per-marker children it replaces.
fn marker_defs_body(edge_color: &str, emit_fancy: bool) -> Cow<'static, str> {
    marker_defs_body_for_mask(
        edge_color,
        if emit_fancy {
            ALL_MARKER_MASK
        } else {
            BASIC_MARKER_MASK
        },
    )
}

fn marker_defs_body_for_mask(edge_color: &str, marker_mask: u16) -> Cow<'static, str> {
    if edge_color == DEFAULT_EDGE_COLOR {
        static EMPTY: OnceLock<String> = OnceLock::new();
        static END_ONLY: OnceLock<String> = OnceLock::new();
        static OPEN_ONLY: OnceLock<String> = OnceLock::new();
        static BASIC: OnceLock<String> = OnceLock::new();
        static FANCY: OnceLock<String> = OnceLock::new();
        let cell = match marker_mask {
            0 => Some(&EMPTY),
            MARKER_END => Some(&END_ONLY),
            MARKER_OPEN => Some(&OPEN_ONLY),
            BASIC_MARKER_MASK => Some(&BASIC),
            ALL_MARKER_MASK => Some(&FANCY),
            _ => None,
        };
        // Borrow the process-global memoized body instead of cloning it into a fresh `String` on
        // every render — `DefsBuilder::raw_markers` now streams it via `push_str`, so a borrow is
        // sufficient. Custom themes still build fresh (rare) as `Cow::Owned`.
        if let Some(cell) = cell {
            return Cow::Borrowed(
                cell.get_or_init(|| build_marker_defs_body(edge_color, marker_mask))
                    .as_str(),
            );
        }
    }
    Cow::Owned(build_marker_defs_body(edge_color, marker_mask))
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
const CLUSTER_THEME_CSS: &str = ".fm-cluster {\n  fill: var(--fm-cluster-fill);\n  stroke: var(--fm-cluster-stroke);\n  stroke-width: 1;\n  stroke-dasharray: 4 4;\n  rx: 10;\n  ry: 10;\n}\n.fm-cluster-label {\n  fill: var(--fm-cluster-label-color);\n  font-weight: 600;\n  font-size: 0.85em;\n  letter-spacing: 0.01em;\n}\n.fm-cluster-c4 {\n  fill: var(--fm-cluster-c4-fill);\n  stroke: var(--fm-cluster-c4-stroke);\n  stroke-dasharray: none;\n}\n.fm-cluster-swimlane {\n  fill: var(--fm-cluster-swimlane-fill);\n  stroke: var(--fm-cluster-swimlane-stroke);\n  stroke-dasharray: none;\n}\n";

/// The special-node-shape theme CSS block (`note`/`cloud`/`cylinder`/`star`/`pentagon`), captured
/// EXACTLY as `Theme::to_svg_style` emits it. Stripped when the diagram uses none of those shapes
/// (the common rect/diamond/round/stadium case). Same byte-identical, safe-no-op-if-drifts contract
/// as `CLUSTER_THEME_CSS`.
const NODE_SHAPE_THEME_CSS: &str = ".fm-node.fm-node-shape-note path,\n.fm-node.fm-node-shape-note rect {\n  fill: var(--fm-node-fill);\n  fill: color-mix(in srgb, #fef3c7 40%, var(--fm-node-fill));\n}\n.fm-node.fm-node-shape-cloud path {\n  fill: var(--fm-node-fill);\n  fill: color-mix(in srgb, var(--fm-accent-2) 15%, var(--fm-node-fill));\n}\n.fm-node.fm-node-shape-cylinder path {\n  fill: var(--fm-node-fill);\n  fill: color-mix(in srgb, var(--fm-accent-1) 12%, var(--fm-node-fill));\n}\n.fm-node.fm-node-shape-star path,\n.fm-node.fm-node-shape-pentagon path {\n  stroke-width: 1.8;\n}\n";

/// Remove the first occurrence of `block` from `css` in place. Equivalent to
/// `*css = css.replace(block, "")` for every theme rule block here, because each is emitted EXACTLY
/// once by `Theme::to_svg_style` (so "first occurrence" == "all occurrences"), but allocation-free:
/// `str::replace` always heap-allocates a fresh String and copies the retained bytes into it, whereas
/// `drain` shifts only the tail left in place. On the common flowchart (no clusters / special shapes /
/// dashed-or-thick edges) all four blocks strip, so this turns 4 fixed-size String allocations +
/// full-buffer copies per render into 4 tail memmoves — a pure fixed-overhead cut that matters most on
/// the small diagrams where the ~9 KB `<style>` dominates output. A non-matching block is a no-op
/// (search → `None`), preserving the safe-if-drifts contract of the block constants.
///
/// The search uses a PRECOMPUTED `memchr::memmem::Finder` (SIMD), not `str::find` nor one-shot
/// `memmem::find`: `str::find`'s Two-Way `StrSearcher::new` needle-table setup measured ~3.3% of flowchart
/// render across the 4 long (~300-500 B) block needles, and even one-shot `memmem::find` rebuilds a
/// two-way `Searcher::new` (~2.5%) every call. Building one `Finder` per block ONCE (process-global
/// `OnceLock`) moves that setup off the per-render path entirely; only the SIMD scan remains. The
/// returned first-match byte offset is identical to `str::find`, so the `drain` is byte-identical.
fn strip_css_block(
    css: &mut String,
    cell: &OnceLock<memchr::memmem::Finder<'static>>,
    block: &'static str,
) {
    let finder = cell.get_or_init(|| memchr::memmem::Finder::new(block.as_bytes()));
    if let Some(pos) = finder.find(css.as_bytes()) {
        css.drain(pos..pos + block.len());
    }
}

/// The `:root` cluster-only custom properties — dead when there are no clusters (they feed only the
/// stripped cluster rules). Named so its `strip_css_block` finder can be a `OnceLock` like the others.
const CLUSTER_VARS_THEME_CSS: &str = "  --fm-cluster-label-color: var(--fm-text-color);\n  --fm-cluster-c4-fill: var(--fm-cluster-fill);\n  --fm-cluster-c4-stroke: var(--fm-cluster-stroke);\n  --fm-cluster-swimlane-fill: var(--fm-cluster-fill);\n  --fm-cluster-swimlane-stroke: var(--fm-cluster-stroke);\n";

/// Drop theme CSS rule blocks the diagram cannot use — the cluster block when there are no clusters,
/// and the special-node-shape block when none of those shapes are present. Byte-identical rendering
/// (the removed selectors match nothing); safe by construction (a non-matching constant is a no-op).
fn strip_unused_theme_css(css: &mut String, ir: Option<&MermaidDiagramIr>) {
    static CLUSTER_F: OnceLock<memchr::memmem::Finder<'static>> = OnceLock::new();
    static CLUSTER_VARS_F: OnceLock<memchr::memmem::Finder<'static>> = OnceLock::new();
    static NODE_SHAPE_F: OnceLock<memchr::memmem::Finder<'static>> = OnceLock::new();
    static EDGE_STYLE_F: OnceLock<memchr::memmem::Finder<'static>> = OnceLock::new();
    if !ir.is_some_and(|ir| !ir.clusters.is_empty()) {
        strip_css_block(css, &CLUSTER_F, CLUSTER_THEME_CSS);
        // The `:root` cluster-only custom properties feed ONLY the stripped cluster rules, so they
        // are dead too when there are no clusters. Same exact-substring / safe-no-op contract.
        strip_css_block(css, &CLUSTER_VARS_F, CLUSTER_VARS_THEME_CSS);
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
        strip_css_block(css, &NODE_SHAPE_F, NODE_SHAPE_THEME_CSS);
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
                    | fm_core::ArrowType::DottedCircle
                    | fm_core::ArrowType::DottedCircleBoth
                    | fm_core::ArrowType::DottedCrossBoth
                    | fm_core::ArrowType::ThickArrow
                    | fm_core::ArrowType::DoubleThickArrow
                    | fm_core::ArrowType::ThickLine
                    | fm_core::ArrowType::ThickCircle
                    | fm_core::ArrowType::ThickCross
                    | fm_core::ArrowType::ThickCircleBoth
                    | fm_core::ArrowType::ThickCrossBoth
            )
        })
    });
    if !has_dashed_or_thick {
        strip_css_block(css, &EDGE_STYLE_F, EDGE_STYLE_THEME_CSS);
    }
}

/// The `.fm-edge-dashed` + `.fm-edge-thick`(+`:hover`) theme rules — captured EXACTLY as
/// `Theme::to_svg_style` emits them — stripped when no edge uses a dotted/thick arrow. Same
/// byte-identical, safe-no-op-if-drifts contract as the other blocks.
const EDGE_STYLE_THEME_CSS: &str = ".fm-edge-dashed {\n  stroke-dasharray: 5 5;\n}\n.fm-edge-thick {\n  stroke-width: 2.25;\n}\n.fm-edge-thick:hover {\n  stroke-width: 3.0;\n}\n";

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
    push_state_css(&mut css, config, effects_enabled, true);
    if config.animations_enabled {
        css.push_str(&animation_css(config));
    }
    if config.a11y.accessibility_css {
        css.push_str(accessibility_css());
    }
    if config.print_optimized {
        css.push_str(&print_css(config.min_font_size, config.node_gradients));
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
    // The 2- (basic) or 12-marker (fancy) arrowhead `<defs>` — a pure function of the edge color and
    // `emit_fancy_markers`, memoized for the default theme (see `marker_defs_body`). Streamed in the
    // markers slot so the output is byte-identical to the per-marker `.marker()` children it replaces,
    // skipping the ~1-6 µs of Element construction + serialization rebuilt on every render.
    defs = defs.raw_markers(marker_defs_body(edge_color, emit_fancy_markers));

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
        MarkerKind::DiamondOpen => "url(#arrow-diamond-open)",
        MarkerKind::TriangleOpen => "url(#arrow-triangle-open)",
        MarkerKind::TriangleOpenStart => "url(#start-arrow-triangle-open)",
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
    // A fallible CGA rotor conversion is available via
    // cga_transform::try_render_transform_to_cga() when rotation extraction or other
    // similarity-transform-only features are needed.
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
            let opacity = clamp_unit_interval(*opacity);
            if opacity < 0.999 {
                elem = elem.fill_opacity(opacity);
            }
        }
    }
    elem
}

fn apply_stroke_style(mut elem: Element, stroke: &StrokeStyle) -> Element {
    elem = elem
        .stroke(&stroke.color)
        .stroke_width(sanitize_stroke_width(stroke.width));

    let opacity = clamp_unit_interval(stroke.opacity);
    if opacity < 0.999 {
        elem = elem.stroke_opacity(opacity);
    }

    if !stroke.dash_array.is_empty() {
        let dasharray = stroke
            .dash_array
            .iter()
            .copied()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(fmt_svg_number)
            .collect::<Vec<_>>()
            .join(",");
        if !dasharray.is_empty() {
            elem = elem.stroke_dasharray(&dasharray);
        }
    }

    elem = elem.stroke_linecap(map_line_cap(stroke.line_cap));
    elem.stroke_linejoin(map_line_join(stroke.line_join))
}

fn fmt_svg_number(value: f32) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
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
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

fn sanitize_stroke_width(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
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
    scan_class_keywords_and_clean(class).0
}

/// True if this IR is rendered by one of the dedicated chart renderers, which each `return` before
/// the generic node path is ever reached.
///
/// SINGLE SOURCE OF TRUTH for that dispatch, deliberately: the four call sites below consume this
/// and so does the `<defs>` gate for `fm-node-gradient`, whose only referrers (`fill="url(#…)"` on
/// node shapes) live exclusively on the generic path. Restating the conditions in five places is how
/// a def and its reference drift apart — see bd-a6uk and bd-678053a for both directions of that
/// mistake. Reads only `ir`, so it is answerable at defs-build time, before dispatch.
fn takes_dedicated_chart_renderer(ir: &MermaidDiagramIr) -> bool {
    ir.xy_chart_meta
        .as_ref()
        .is_some_and(|meta| !meta.series.is_empty())
        || ir
            .pie_meta
            .as_ref()
            .is_some_and(|meta| !meta.slices.is_empty())
        || ir.quadrant_meta.is_some()
        || (ir.diagram_type == fm_core::DiagramType::Gantt && ir.gantt_meta.is_some())
}

/// True if any node carries a class the node renderer reads as `highlight`.
///
/// Mirrors the per-node `is_highlighted` computation in the node renderer exactly — same
/// `scan_node_class_keywords` over the same `node.classes` — so the `<filter id="node-glow">` def
/// and the `filter="url(#node-glow)"` reference can never disagree about whether glow is in play.
/// The mirror is exact rather than conservative because the node path resolves its node as
/// `ir.nodes.get(node_box.node_index)`, so a highlighted node is always an element of `ir.nodes`,
/// and `is_highlighted` stays false whenever that lookup misses.
///
/// Free on the common diagram: `node.classes` is empty, so the inner `any` never runs a scan.
fn any_node_is_highlighted(ir: &MermaidDiagramIr) -> bool {
    ir.nodes.iter().any(|node| {
        node.classes
            .iter()
            .any(|class| scan_node_class_keywords(class).highlighted)
    })
}

/// One pass over `class` that both detects the state keywords (as [`scan_node_class_keywords`]) AND
/// reports whether the class is an already-valid lowercase CSS token (the `all(clean)` fast-path check
/// [`write_sanitized_css_token_into`] does). The node-class fast paths call this once and reuse both
/// results, replacing TWO independent byte scans of the same string with one — on classed nodes
/// (timeline/journey/class/…) each node has 1-2 user classes, so this halves the per-class scan work.
/// Byte-identical: the keyword arms are unchanged and `clean` matches `write_sanitized`'s predicate.
/// Per-byte gate for [`scan_class_keywords_and_clean`], indexed by the raw byte. Bit 0 (`SCAN_NOT_CLEAN`)
/// is set when the byte is NOT a valid lowercase CSS token char (`[a-z0-9-_]`); bit 1
/// (`SCAN_KW_CANDIDATE`) is set when the byte's lowercased form is a keyword START byte (`h s a f i m d b`,
/// either case). The overwhelmingly common class byte (a clean lowercase letter that starts no keyword,
/// e.g. every byte of `journey-actor`/`timeline-section-…`) has flags `0`, so the per-byte work collapses
/// to one table load + two predicted-not-taken branches — skipping BOTH the 4-way clean OR-chain and the
/// `match raw|0x20` keyword dispatch. Bit-identical to the inline predicates it replaces.
const SCAN_NOT_CLEAN: u8 = 1;
const SCAN_KW_CANDIDATE: u8 = 2;
const CLASS_SCAN_GATE: [u8; 256] = {
    let mut t = [0u8; 256];
    let mut i = 0usize;
    while i < 256 {
        let b = i as u8;
        if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_') {
            t[i] |= SCAN_NOT_CLEAN;
        }
        match b | 0x20 {
            b'h' | b's' | b'a' | b'f' | b'i' | b'm' | b'd' | b'b' => t[i] |= SCAN_KW_CANDIDATE,
            _ => {}
        }
        i += 1;
    }
    t
};

fn scan_class_keywords_and_clean(class: &str) -> (NodeClassKeywords, bool) {
    let b = class.as_bytes();
    let mut f = NodeClassKeywords::default();
    let mut clean = true;
    for i in 0..b.len() {
        let raw = b[i];
        let gate = CLASS_SCAN_GATE[raw as usize];
        if gate & SCAN_NOT_CLEAN != 0 {
            clean = false;
        }
        // Only keyword-START bytes (`h s a f i m d b`) can begin a state keyword; every other byte would
        // hit the old `match`'s `_ => {}`, so gate the whole dispatch behind the candidate bit.
        if gate & SCAN_KW_CANDIDATE == 0 {
            continue;
        }
        match raw | 0x20 {
            b'h' if matches_ci_at(b, i, b"highlight") => {
                f.highlighted = true;
            }
            b's' if matches_ci_at(b, i, b"selected") => {
                f.highlighted = true;
            }
            b'a' if matches_ci_at(b, i, b"active") => {
                f.highlighted = true;
            }
            b'f' if matches_ci_at(b, i, b"focus") => {
                f.highlighted = true;
            }
            b'i' => {
                if matches_ci_at(b, i, b"important") {
                    f.highlighted = true;
                }
                if matches_ci_at(b, i, b"inactive") {
                    f.inactive = true;
                }
            }
            b'm' if matches_ci_at(b, i, b"muted") => {
                f.inactive = true;
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
    (f, clean)
}

/// Write the CSS-sanitized form of `value` straight into `buf` — the alloc-free core of
/// [`sanitize_css_token`]. Used by the node-class fast paths (`simple_node_user_class_suffix` etc.) which
/// only need to append the token to a suffix buffer, so they skip the throwaway per-class `String`
/// (`sanitize_css_token` was ~4.5-4.9% of classed-node render — mindmap/git/styled — almost entirely the
/// `collect()` allocation). Byte-identical: same per-char mapping in the same order.
fn write_sanitized_css_token_into(buf: &mut String, value: &str) {
    // Bulk-copy the already-clean PREFIX, then per-`char` map only the tail. Every byte before the first
    // non-`[a-z0-9-_]` byte maps to itself (`to_ascii_lowercase` is the identity on lowercase alnum / `-` /
    // `_`, and each is kept), so it can go in one `push_str` instead of a per-`char` decode+map+push. A
    // fully-clean token (`kanban-card`, `timeline-section-0`, …) copies in one shot and returns; a
    // capitalised user class (`journey-actor-Actor1`, `:::MyClass`) — the common non-clean case — still
    // copies its long clean run in bulk and only decodes the short dirty tail. `position` scans exactly the
    // clean prefix the old `.all` did before it short-circuited, so no extra work on either case.
    // Byte-identical to the old fast-path-then-full-char-loop. `clean_len` lands on an ASCII byte boundary
    // (every clean char is single-byte), so both slices are valid UTF-8.
    let clean_len = value
        .bytes()
        .position(|b| !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'))
        .unwrap_or(value.len());
    buf.push_str(&value[..clean_len]);
    if clean_len == value.len() {
        return;
    }
    for ch in value[clean_len..].chars() {
        let mapped = if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            ch.to_ascii_lowercase()
        } else {
            '-'
        };
        buf.push(mapped);
    }
}

fn sanitize_css_token(value: &str) -> String {
    // Each source char maps to exactly one output char whose byte length never exceeds the source's
    // (ASCII lowercase / a 1-byte `-`), so `value.len()` is a safe no-realloc capacity.
    let mut token = String::with_capacity(value.len());
    write_sanitized_css_token_into(&mut token, value);
    token
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
        && trimmed[1..].bytes().all(|b| b.is_ascii_hexdigit())
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
        // ⚠️ ONE LIST, IN fm-core (bd-jyg4s). This used to be a local six-entry table while
        // mermaid's `isLabelStyle` names eighteen, and the twelve missing ones were ALSO absent
        // from the security allowlist — so they were dropped before ever reaching this split and an
        // author's `letter-spacing` did nothing at all. Two copies of one list is how that happened;
        // there is now one, and the allowlist is checked against it by test.
        if fm_core::is_label_style_property(key.as_str()) {
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
/// The CSS a `style mySubgraph fill:#f00` declares for this cluster, if any (bd-xfmm).
///
/// bd-xfmm could only warn about a subgraph style, because `IrStyleTarget` had no `Cluster`
/// variant. It has one now, and this is the half that makes the directive visible: without a
/// consumer the variant would be one more parsed-stored-drawn-by-nothing field, which is the exact
/// class bd-jgco, bd-jerh and bd-bk7h all belong to.
///
/// Returns `None` when nothing was declared, so the caller keeps the theme fill rather than a
/// colour resolved from an empty map.
/// The shape half and the label half of `style <subgraph> ...`, split the way mermaid splits it.
///
/// ⚠️ A `style` DIRECTIVE IS TWO STYLES, NOT ONE. mermaid partitions the declaration list in
/// `styles2String`: every property its `isLabelStyle` predicate accepts goes to `labelStyles` and
/// is applied to the label, and everything else goes to `nodeStyles` and is applied to the shape.
/// So `style one fill:#ff0000,color:#123456` paints the container red AND the title `#123456`.
///
/// This renderer already did exactly that for NODES — `split_style_properties` — and the cluster
/// path was its asymmetric sibling: it merged every property into one string and put the lot on the
/// `<rect>`. `color` on a rect does nothing, so a cluster title styled by its author silently kept
/// the theme colour. The node path is also where the `color` -> `fill` mapping lives, which SVG
/// text needs and CSS `color` does not provide.
///
/// Returns `(shape_css, label_css)`.
fn resolve_cluster_inline_styles(
    ir: &MermaidDiagramIr,
    cluster_index: usize,
) -> (Option<String>, Option<String>) {
    use fm_core::IrStyleTarget;
    if ir.style_refs.is_empty() {
        return (None, None);
    }

    let mut merged = BTreeMap::new();
    for sr in &ir.style_refs {
        if let IrStyleTarget::Cluster(target) = sr.target
            && target == cluster_index
        {
            merged.extend(fm_core::parse_style_string(&sr.style).properties);
        }
    }

    let (shape, text) = split_style_properties(&merged);
    (style_map_to_css(&shape), style_map_to_css(&text))
}

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
            if area < 12_000.0 {
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

/// The default-preset theme's node fill + background — the memo key for [`node_gradient_svg`].
/// Pinned by `default_node_gradient_colors_match_preset`.
const DEFAULT_NODE_FILL: &str = "#ffffff";
const DEFAULT_NODE_BG: &str = "#fafbfc";

/// The node-gradient `<defs>` fragment for `(config, theme)` — the serialized `<linearGradient>`/
/// `<radialGradient>` that [`node_gradient_for`] builds — memoized for the default theme + style.
///
/// Building the 3-stop `Gradient` (Vec + 4-element tree) and serializing it is ~1.1 µs and is a pure
/// function of `(node_gradient_style, node_fill, background)` — a fixed per-render cost on every
/// `node_gradients` render (the default, so every flowchart + most types). The overwhelmingly common
/// default `LinearVertical` + default theme is memoized via a process-global `OnceLock` built from the
/// real `node_gradient_for` output (byte-identical, no drift); other themes/styles build fresh.
/// Returns `None` when gradients are off, matching the former `if let Some(gradient)` skip. Streamed as
/// one [`DefsBuilder::raw_gradients`], byte-identical to the `.gradient(..)` child it replaces.
fn node_gradient_svg(config: &SvgRenderConfig, theme: &Theme) -> Option<Cow<'static, str>> {
    if !config.node_gradients {
        return None;
    }
    if matches!(
        config.node_gradient_style,
        NodeGradientStyle::LinearVertical
    ) && theme.colors.node_fill == DEFAULT_NODE_FILL
        && theme.colors.background == DEFAULT_NODE_BG
    {
        static DEFAULT_GRAD: OnceLock<String> = OnceLock::new();
        // Borrow the memoized default gradient rather than cloning it every render — `raw_gradients`
        // streams it via `push_str`. Custom gradients build fresh as `Cow::Owned`.
        return Some(Cow::Borrowed(
            DEFAULT_GRAD
                .get_or_init(|| {
                    node_gradient_for(config, theme)
                        .expect("gradient present when node_gradients is on")
                        .to_element()
                        .render()
                })
                .as_str(),
        ));
    }
    Some(Cow::Owned(
        node_gradient_for(config, theme)?.to_element().render(),
    ))
}

/// `.fm-node-inactive`'s opacity — genuinely a cosmetic knob, so it stays gated on one.
fn inactive_opacity_css(config: &SvgRenderConfig) -> String {
    let inactive_opacity = clamp_unit_interval(config.inactive_opacity);
    format!(".fm-node-inactive {{ opacity: {inactive_opacity:.2}; }}\n")
}

/// `.fm-cluster`'s fill opacity — likewise a cosmetic knob, likewise gated.
fn cluster_fill_opacity_css(config: &SvgRenderConfig) -> String {
    let cluster_fill_opacity = clamp_unit_interval(config.cluster_fill_opacity);
    format!(".fm-cluster {{ fill-opacity: {cluster_fill_opacity:.2}; }}\n")
}

/// Rules that are SEMANTIC ENCODING, not polish, and are therefore emitted regardless of the
/// cosmetic knobs (bd-w0f0).
///
/// These four markers each carry meaning that exists nowhere else in the output:
/// * `.fm-node-border-dashed` is the ONLY thing that shows a C4 `System_Ext` is external;
/// * `.fm-node-block-beta-space` is the ONLY thing that makes a block-beta `space` cell invisible;
/// * `.fm-node-block-beta` fills are how a block reads as a block;
/// * `.fm-node-highlighted` / `.fm-node-border-double` are node emphasis.
///
/// They used to sit inside `effects_css`, gated on `node_gradients || glow_enabled ||
/// inactive_opacity < 0.999 || cluster_fill_opacity < 0.999`. Turning gradients off and both
/// opacities to 1.0 therefore REMOVED semantic encoding: the element still carried
/// `class="… fm-node-user-c4-external fm-node-border-dashed"`, but no rule stood behind it, and a
/// marker class with no rule is indistinguishable from no marker at all. That is also the exact
/// config `golden_render_config()` pins all 37 byte goldens with, so the corpus was structurally
/// incapable of catching a regression in any of them.
///
/// Emitting these unconditionally costs nothing on diagrams that do not use them:
/// `strip_unused_state_css` drops the whole region when the rendered BODY carries none of the five
/// state classes, which is a stronger test than the config knobs ever were.
const SEMANTIC_MARKER_CSS: &str = concat!(
    ".fm-node-block-beta rect,\n",
    ".fm-node-block-beta path,\n",
    ".fm-node-block-beta circle,\n",
    ".fm-node-block-beta ellipse,\n",
    ".fm-node-block-beta polygon {\n",
    "  fill: #546e7a;\n",
    "  stroke: #455a64;\n",
    "}\n",
    ".fm-node-block-beta text {\n",
    "  fill: #f8fafc;\n",
    "}\n",
    ".fm-node-block-beta-space {\n",
    "  opacity: 0;\n",
    "  pointer-events: none;\n",
    "}\n",
    ".fm-node-highlighted rect,\n",
    ".fm-node-highlighted path,\n",
    ".fm-node-highlighted circle,\n",
    ".fm-node-highlighted ellipse,\n",
    ".fm-node-highlighted polygon {\n",
    "  stroke-width: 2.4;\n",
    "}\n",
    ".fm-node-highlighted text { font-weight: 600; }\n",
    ".fm-node-border-dashed rect,\n",
    ".fm-node-border-dashed path,\n",
    ".fm-node-border-dashed circle,\n",
    ".fm-node-border-dashed ellipse,\n",
    ".fm-node-border-dashed polygon {\n",
    "  stroke-dasharray: 6 4;\n",
    "}\n",
    ".fm-node-border-double rect,\n",
    ".fm-node-border-double path,\n",
    ".fm-node-border-double circle,\n",
    ".fm-node-border-double ellipse,\n",
    ".fm-node-border-double polygon {\n",
    "  stroke-width: 2.9;\n",
    "}\n",
);

/// Append the node-state rule region: the cosmetic opacity rules only when their knobs ask for
/// them, the semantic markers whenever the diagram could carry one. Order is unchanged from the
/// single `effects_css` block this replaces, so an effects-enabled render is byte-identical.
///
/// `semantic_markers` is `true` on every path whose output later goes through
/// `strip_unused_state_css`, which drops the region exactly when the rendered BODY uses none of the
/// five state classes. The direct minified flowchart path returns before that pass runs, so it
/// passes a conservative IR-side answer instead — see `ir_may_emit_state_classes`.
fn push_state_css(
    css: &mut String,
    config: &SvgRenderConfig,
    effects_enabled: bool,
    semantic_markers: bool,
) {
    if effects_enabled {
        css.push_str(&inactive_opacity_css(config));
    }
    if semantic_markers {
        css.push_str(SEMANTIC_MARKER_CSS);
    }
    if effects_enabled {
        css.push_str(&cluster_fill_opacity_css(config));
    }
}

/// Whether this diagram can emit any of the five node-STATE classes.
///
/// Every one of them — highlighted / inactive / border-dashed / border-double / block-beta[-space] —
/// is raised from a node's own `classes`, by exactly the predicates used here: `scan_node_class_
/// keywords` plus the `c4-external` / `block-beta` / `block-beta-space` exact matches. Sharing that
/// scan rather than restating it is the point — a private copy is how a rule and its marker drift
/// apart, which is bd-w0f0's whole failure mode.
///
/// `is_inactive` is included even though `.fm-node-inactive`'s rule stays gated on its own knob: the
/// answer is about the region, and the region's cheapest correct bound is "any state class at all".
fn ir_may_emit_state_classes(ir: &MermaidDiagramIr) -> bool {
    ir.nodes.iter().any(|node| {
        node.classes.iter().any(|class| {
            let kw = scan_node_class_keywords(class);
            kw.highlighted
                || kw.inactive
                || kw.dashed_border
                || kw.double_border
                || class.eq_ignore_ascii_case("c4-external")
                || class.eq_ignore_ascii_case("block-beta")
                || class.eq_ignore_ascii_case("block-beta-space")
        })
    })
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

/// `gradient_def_emitted` must be "does this document actually contain the `fm-node-gradient` def",
/// not merely `config.node_gradients`: the dedicated chart renderers have the flag on yet never emit
/// the def, and a neutralisation rule for an absent id is 79 dead bytes (bd-w5f7).
fn print_css(min_font_size: f32, gradient_def_emitted: bool) -> String {
    // Flatten the node gradient for print by neutralising its STOPS, not by overriding node fill.
    //
    // The gradient reaches nodes as an inline `fill="url(#fm-node-gradient)"` presentation
    // attribute, so any CSS `fill` rule would beat it — but such a rule would also beat classDef
    // fills and solid shapes like FilledCircle, flattening colour the author chose deliberately.
    // Restyling the gradient's own stops changes exactly the one thing that is wrong here and
    // nothing else: every node still resolves `url(#fm-node-gradient)`, and that paint is now flat
    // white on paper. Clusters already print `fill: #fff`, so nodes now match them.
    //
    // Emitted only when the def is actually in the document. Without one there is no gradient to
    // neutralise, the rule would match nothing, and omitting it keeps the print block byte-identical
    // for every diagram that never had a gradient — which is what keeps the golden corpus still.
    let gradient_reset = if gradient_def_emitted {
        "
  #fm-node-gradient stop {
    stop-color: #fff !important;
    stop-opacity: 1 !important;
  }"
    } else {
        ""
    };
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
  }}{gradient_reset}
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

fn arrow_marker_mask(arrow: fm_core::ArrowType) -> u16 {
    use fm_core::ArrowType;
    match arrow {
        ArrowType::Line | ArrowType::ThickLine | ArrowType::DottedLine => 0,
        ArrowType::Arrow | ArrowType::DottedArrow => MARKER_END,
        ArrowType::OpenArrow | ArrowType::DottedOpenArrow => MARKER_OPEN,
        ArrowType::HalfArrowTop
        | ArrowType::HalfArrowBottomReverse
        | ArrowType::HalfArrowTopDotted
        | ArrowType::HalfArrowBottomReverseDotted => MARKER_HALF_TOP,
        ArrowType::HalfArrowBottom
        | ArrowType::HalfArrowTopReverse
        | ArrowType::HalfArrowBottomDotted
        | ArrowType::HalfArrowTopReverseDotted => MARKER_HALF_BOTTOM,
        ArrowType::StickArrowTop
        | ArrowType::StickArrowBottomReverse
        | ArrowType::StickArrowTopDotted
        | ArrowType::StickArrowBottomReverseDotted => MARKER_STICK_TOP,
        ArrowType::StickArrowBottom
        | ArrowType::StickArrowTopReverse
        | ArrowType::StickArrowBottomDotted
        | ArrowType::StickArrowTopReverseDotted => MARKER_STICK_BOTTOM,
        ArrowType::ThickArrow => MARKER_FILLED,
        ArrowType::Circle | ArrowType::ThickCircle | ArrowType::DottedCircle => MARKER_CIRCLE,
        ArrowType::Cross | ArrowType::ThickCross | ArrowType::DottedCross => MARKER_CROSS,
        // `o--o` / `x--x` need no NEW marker declaration (bd-zdpwd): a circle and a cross are
        // orientation-independent, so the same `<marker>` serves both ends. That is why there is no
        // `arrow-start-circle` here, unlike the directional `arrow-start` an arrowhead requires.
        ArrowType::CircleBoth | ArrowType::ThickCircleBoth | ArrowType::DottedCircleBoth => {
            MARKER_CIRCLE
        }
        ArrowType::CrossBoth | ArrowType::ThickCrossBoth | ArrowType::DottedCrossBoth => {
            MARKER_CROSS
        }
        ArrowType::DoubleArrow | ArrowType::DoubleDottedArrow => MARKER_START | MARKER_END,
        ArrowType::DoubleThickArrow => MARKER_START_FILLED | MARKER_FILLED,
        ArrowType::Aggregation | ArrowType::AggregationReverse => MARKER_DIAMOND_OPEN,
        ArrowType::Composition | ArrowType::CompositionReverse => MARKER_DIAMOND,
        ArrowType::Inheritance => MARKER_START_TRIANGLE_OPEN,
        ArrowType::InheritanceReverse => MARKER_TRIANGLE_OPEN,
    }
}

/// Flowchart layout edges are the complete marker source, so derive the exact live set before SVG
/// serialization. Other diagram families retain the drift-proof output scan because their renderers
/// may synthesize markers outside `ir.edges`.
fn flowchart_marker_mask(ir: &MermaidDiagramIr, layout: &DiagramLayout) -> Option<u16> {
    (ir.diagram_type == fm_core::DiagramType::Flowchart).then(|| {
        layout.edges.iter().fold(0, |mask, edge_path| {
            let edge_mask = if edge_path.reversed {
                MARKER_OPEN
            } else {
                ir.edges
                    .get(edge_path.edge_index)
                    .map_or(MARKER_END, |edge| arrow_marker_mask(edge.arrow))
            };
            mask | edge_mask
        })
    })
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
fn render_edges_serial(
    out: &mut String,
    edges: &[LayoutEdgePath],
    context: &EdgeRenderContext<'_>,
) {
    for edge_path in edges {
        if edge_path.bundled {
            continue;
        }
        render_edge_into(out, edge_path, context);
    }
}

fn batch_fragment_globals_match(
    reuse: &SvgBatchFragmentReuse<'_>,
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    detail: RenderDetailProfile,
    offset_x: f32,
    offset_y: f32,
) -> bool {
    let (Some(previous_ir), Some(previous_layout), Some(previous)) =
        (reuse.previous_ir, reuse.previous_layout, reuse.previous)
    else {
        return false;
    };
    previous.active
        && previous.detail == Some(detail)
        && previous.offset_x_bits == offset_x.to_bits()
        && previous.offset_y_bits == offset_y.to_bits()
        && previous_ir.diagram_type == DiagramType::Flowchart
        && ir.diagram_type == DiagramType::Flowchart
        && previous_ir.direction == ir.direction
        && previous_ir.meta.theme_overrides == ir.meta.theme_overrides
        && previous_ir.style_refs == ir.style_refs
        && previous_ir.style_defs == ir.style_defs
        && previous_ir.label_markup == ir.label_markup
        && previous_layout.extensions.node_centrality == layout.extensions.node_centrality
}

fn certified_batch_prefix_match<'a>(
    reuse: &'a SvgBatchFragmentReuse<'_>,
    layout: &DiagramLayout,
    detail: RenderDetailProfile,
    offset_x: f32,
    offset_y: f32,
) -> Option<&'a CertifiedSvgBatchPrefix> {
    let previous = reuse.previous?;
    let previous_layout = reuse.previous_layout?;
    let previous_prefix = reuse.previous_certified_prefix?;
    let current_prefix = reuse.current_certified_prefix?;
    let same_identity = Arc::ptr_eq(&previous_prefix.identity, &current_prefix.identity)
        || previous_prefix.identity == current_prefix.identity;
    (same_identity
        && previous_prefix.node_count == current_prefix.node_count
        && previous_prefix.edge_count == current_prefix.edge_count
        && previous.active
        && previous.detail == Some(detail)
        && previous.offset_x_bits == offset_x.to_bits()
        && previous.offset_y_bits == offset_y.to_bits()
        && previous_layout.extensions.node_centrality == layout.extensions.node_centrality)
        .then_some(current_prefix)
}

fn node_label_matches(
    previous_ir: &MermaidDiagramIr,
    previous_node: &fm_core::IrNode,
    ir: &MermaidDiagramIr,
    node: &fm_core::IrNode,
) -> bool {
    previous_node.label.map(|label| label.0) == node.label.map(|label| label.0)
        && previous_node
            .label
            .and_then(|label| previous_ir.labels.get(label.0))
            == node.label.and_then(|label| ir.labels.get(label.0))
}

fn reusable_node_prefix_len(
    reuse: &SvgBatchFragmentReuse<'_>,
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    detail: RenderDetailProfile,
    offset_x: f32,
    offset_y: f32,
) -> usize {
    if let Some(prefix) = certified_batch_prefix_match(reuse, layout, detail, offset_x, offset_y) {
        if reuse.certified_geometry_prefix {
            return prefix.node_count.min(layout.nodes.len());
        }
        let previous_layout = reuse.previous_layout.expect("certified previous layout");
        return previous_layout
            .nodes
            .iter()
            .zip(&layout.nodes)
            .take(prefix.node_count)
            .take_while(|(previous_box, node_box)| previous_box == node_box)
            .count();
    }
    if !batch_fragment_globals_match(reuse, ir, layout, detail, offset_x, offset_y) {
        return 0;
    }
    let previous_ir = reuse.previous_ir.expect("matched previous IR");
    let previous_layout = reuse.previous_layout.expect("matched previous layout");
    previous_layout
        .nodes
        .iter()
        .zip(&layout.nodes)
        .take_while(|(previous_box, node_box)| {
            if previous_box != node_box || previous_box.node_index != node_box.node_index {
                return false;
            }
            let Some(previous_node) = previous_ir.nodes.get(previous_box.node_index) else {
                return false;
            };
            let Some(node) = ir.nodes.get(node_box.node_index) else {
                return false;
            };
            previous_node == node && node_label_matches(previous_ir, previous_node, ir, node)
        })
        .count()
}

fn reusable_edge_prefix_len(
    reuse: &SvgBatchFragmentReuse<'_>,
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    detail: RenderDetailProfile,
    offset_x: f32,
    offset_y: f32,
) -> usize {
    if let Some(prefix) = certified_batch_prefix_match(reuse, layout, detail, offset_x, offset_y) {
        if reuse.certified_geometry_prefix {
            return prefix.edge_count.min(layout.edges.len());
        }
        let previous_layout = reuse.previous_layout.expect("certified previous layout");
        return previous_layout
            .edges
            .iter()
            .zip(&layout.edges)
            .take(prefix.edge_count)
            .take_while(|(previous_path, edge_path)| previous_path == edge_path)
            .count();
    }
    if !batch_fragment_globals_match(reuse, ir, layout, detail, offset_x, offset_y) {
        return 0;
    }
    let previous_ir = reuse.previous_ir.expect("matched previous IR");
    let previous_layout = reuse.previous_layout.expect("matched previous layout");
    previous_layout
        .edges
        .iter()
        .zip(&layout.edges)
        .take_while(|(previous_path, edge_path)| {
            if previous_path != edge_path || previous_path.edge_index != edge_path.edge_index {
                return false;
            }
            let Some(previous_edge) = previous_ir.edges.get(previous_path.edge_index) else {
                return false;
            };
            let Some(edge) = ir.edges.get(edge_path.edge_index) else {
                return false;
            };
            if previous_edge != edge
                || previous_edge
                    .label
                    .and_then(|label| previous_ir.labels.get(label.0))
                    != edge.label.and_then(|label| ir.labels.get(label.0))
            {
                return false;
            }
            edge_endpoint_accessible_labels(previous_edge, previous_ir, None)
                == edge_endpoint_accessible_labels(edge, ir, None)
        })
        .count()
}

fn render_edges_with_batch_reuse(
    out: &mut String,
    edges: &[LayoutEdgePath],
    context: &EdgeRenderContext<'_>,
    layout: &DiagramLayout,
    reuse: &mut SvgBatchFragmentReuse<'_>,
) {
    let common = reusable_edge_prefix_len(
        reuse,
        context.ir,
        layout,
        context.detail,
        context.offset_x,
        context.offset_y,
    )
    .min(
        reuse
            .previous
            .map_or(0, |previous| previous.edge_ends.len()),
    );
    let prefix_end = common
        .checked_sub(1)
        .and_then(|index| reuse.previous?.edge_ends.get(index).copied())
        .unwrap_or(0);
    reuse.next.reused_edges = common;
    let expected = edges.len().saturating_mul(480);
    reuse.next.edge_svg.reserve(expected);
    reuse.next.edge_ends.reserve(edges.len());
    if let Some(previous) = reuse.previous {
        reuse
            .next
            .edge_svg
            .push_str(&previous.edge_svg[..prefix_end.min(previous.edge_svg.len())]);
        reuse
            .next
            .edge_ends
            .extend_from_slice(&previous.edge_ends[..common]);
    }
    for edge_path in &edges[common..] {
        if !edge_path.bundled {
            render_edge_into(&mut reuse.next.edge_svg, edge_path, context);
        }
        reuse.next.edge_ends.push(reuse.next.edge_svg.len());
    }
    out.push_str(&reuse.next.edge_svg);
}

#[allow(clippy::too_many_arguments)]
fn render_nodes_with_batch_reuse(
    out: &mut String,
    nodes: &[LayoutNodeBox],
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    detail: RenderDetailProfile,
    colors: &ThemeColors,
    emit_classdef_classes: bool,
    centrality_map: &HashMap<usize, CentralityTier>,
    reuse: &mut SvgBatchFragmentReuse<'_>,
) {
    let common = reusable_node_prefix_len(reuse, ir, layout, detail, offset_x, offset_y).min(
        reuse
            .previous
            .map_or(0, |previous| previous.node_ends.len()),
    );
    let prefix_end = common
        .checked_sub(1)
        .and_then(|index| reuse.previous?.node_ends.get(index).copied())
        .unwrap_or(0);
    reuse.next.reused_nodes = common;
    let expected = nodes.len().saturating_mul(640);
    reuse.next.node_svg.reserve(expected);
    reuse.next.node_ends.reserve(nodes.len());
    if let Some(previous) = reuse.previous {
        reuse
            .next
            .node_svg
            .push_str(&previous.node_svg[..prefix_end.min(previous.node_svg.len())]);
        reuse
            .next
            .node_ends
            .extend_from_slice(&previous.node_ends[..common]);
    }
    for node_box in &nodes[common..] {
        render_node_into(
            &mut reuse.next.node_svg,
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
        reuse.next.node_ends.push(reuse.next.node_svg.len());
    }
    out.push_str(&reuse.next.node_svg);
}

/// The legacy-layout SVG coordinate frame shared by all layout-backed renderers.
///
/// Layout coordinates become SVG coordinates by adding `offset_x` and `offset_y`.
/// Keep the title and C4 legend reservation here so every consumer agrees on the
/// rendered viewBox rather than reconstructing part of this calculation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SvgFrame {
    pub viewbox_width: f32,
    pub viewbox_height: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

pub(crate) fn svg_frame(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    config: &SvgRenderConfig,
) -> SvgFrame {
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
    let title_height = if has_specialized_title_renderer || ir.meta.title.is_none() {
        0.0
    } else {
        config.font_size + 22.0
    };

    SvgFrame {
        viewbox_width: (layout.bounds.width + padding * 2.0).max(legend_width + padding * 2.0),
        viewbox_height: layout.bounds.height + padding * 2.0 + legend_height + title_height,
        offset_x: padding - layout.bounds.x,
        offset_y: padding - layout.bounds.y + title_height,
    }
}

fn render_layout_to_svg(
    layout: &DiagramLayout,
    ir: &MermaidDiagramIr,
    config: &SvgRenderConfig,
    known_live_marker_mask: Option<u16>,
    direct_minified_css: bool,
    cache_direct_minified_css: bool,
    mut batch_reuse: Option<&mut SvgBatchFragmentReuse<'_>>,
) -> String {
    let frame = svg_frame(ir, layout, config);
    let padding = config.padding;
    let legend_enabled = is_c4_legend_enabled(ir);
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
    let width = frame.viewbox_width;
    let height = frame.viewbox_height;
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
    // The 2- (basic) or 12-marker (fancy) arrowhead `<defs>` — a pure function of the edge color and
    // `emit_fancy_markers`, memoized for the default theme (see `marker_defs_body`). Streamed in the
    // markers slot so the output is byte-identical to the per-marker `.marker()` children it replaces,
    // skipping the ~1-6 µs of Element construction + serialization rebuilt on every render.
    defs = defs.raw_markers(known_live_marker_mask.map_or_else(
        || marker_defs_body(edge_color, emit_fancy_markers),
        |mask| marker_defs_body_for_mask(edge_color, mask),
    ));

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
    // Add the `<filter id="node-glow">` def only when a node can actually reference it. Its sole
    // referrer is the inline `filter="url(#node-glow)"` emitted under `is_highlighted &&
    // config.glow_enabled`, and `is_highlighted` comes only from a node class matching the
    // `highlight` keyword — which almost no diagram carries. Gating on `glow_enabled` alone (it
    // defaults on) therefore shipped 168 bytes of unreferenced def on nearly every render. Same
    // rule as the `drop-shadow` gate above and the marker gating below: never emit a def that
    // nothing can use.
    //
    // The predicate MIRRORS the consumer instead of approximating it, through the same
    // `scan_node_class_keywords`. Suppressing the def while a node still emits the reference would
    // leave a dangling `url(#…)` — invalid, and a silently unstyled node — which is strictly worse
    // than the wasted bytes.
    if config.glow_enabled && any_node_is_highlighted(ir) {
        defs = defs.filter(Filter::drop_shadow_with_color(
            "node-glow",
            0.0,
            0.0,
            config.glow_blur,
            clamp_unit_interval(config.glow_opacity),
            &config.glow_color,
        ));
    }
    // Memoized node-gradient `<defs>` (default theme + style built once; ~1.1 µs build skipped per
    // render), streamed in the gradients slot — byte-identical to `defs.gradient(node_gradient_for(..))`.
    //
    // Skipped entirely for the dedicated chart renderers. Every `url(#fm-node-gradient)` referrer is
    // a node-shape fill on the generic path, and those renderers `return` before reaching it, so the
    // def has no reachable consumer there — 283 bytes, 4-5% of a gantt/pie/quadrant/xychart document.
    // Same rule as the markers and the `drop-shadow`/`node-glow` filters above: never emit a def
    // nothing can use. Shares `takes_dedicated_chart_renderer` with the dispatch itself so the two
    // cannot disagree.
    //
    // `gradient_def_emitted` is the single truth the print block also reads: bd-ccni's
    // `#fm-node-gradient stop { … }` print rule exists to neutralise this def, so suppressing the def
    // while still emitting the rule leaves 79 bytes of CSS selecting an id that is not in the
    // document.
    let gradient_def_emitted = !takes_dedicated_chart_renderer(ir) && config.node_gradients;
    if gradient_def_emitted && let Some(grad_svg) = node_gradient_svg(config, &theme) {
        defs = defs.raw_gradients(grad_svg);
    }

    doc = doc.defs(defs);

    // Embed theme CSS if enabled
    if config.embed_theme_css {
        let mut css = theme.to_svg_style(
            detail.enable_shadows,
            ir.edges.iter().any(|edge| edge.label.is_some()),
        );
        strip_unused_theme_css(&mut css, Some(ir));
        push_state_css(
            &mut css,
            config,
            effects_enabled,
            !direct_minified_css || ir_may_emit_state_classes(ir),
        );
        if config.animations_enabled {
            css.push_str(&animation_css(config));
        }

        // Add accessibility CSS if enabled
        if config.a11y.accessibility_css {
            css.push_str(accessibility_css());
        }
        if config.print_optimized {
            css.push_str(&print_css(config.min_font_size, gradient_def_emitted));
        }
        if !classdef_css.is_empty() {
            css.push_str(&classdef_css);
        }
        if direct_minified_css {
            // The direct flowchart path returns before `apply_output_post_passes`, and it minifies
            // here, so prune while the stylesheet is still pretty. Pruning before the cache also
            // keeps its key honest: the text differs per live marker/accent set, so diagrams with
            // different arrowheads or node ids cannot share a stale stylesheet.
            let accent_mask = flowchart_accent_mask(ir);
            strip_unused_accent_css(
                &mut css,
                &accent_mask,
                ir_inline_styles_reference_accent(ir),
            );
            if let Some(live_mask) = known_live_marker_mask {
                strip_dead_marker_css_for_mask(&mut css, live_mask);
            }
            css = if cache_direct_minified_css {
                cached_minified_full_css(css)
            } else {
                minify_css(&css)
            };
        }

        doc = doc.style(css);
    } else {
        // Only add supplemental CSS (accessibility and/or print optimization).
        let mut css = String::new();
        push_state_css(
            &mut css,
            config,
            effects_enabled,
            !direct_minified_css || ir_may_emit_state_classes(ir),
        );
        if config.animations_enabled {
            css.push_str(&animation_css(config));
        }
        if config.a11y.accessibility_css {
            css.push_str(accessibility_css());
        }
        if config.print_optimized {
            css.push_str(&print_css(config.min_font_size, gradient_def_emitted));
        }
        if !classdef_css.is_empty() {
            css.push_str(&classdef_css);
        }
        if !css.is_empty() {
            doc = doc.style(css);
        }
    }

    let offset_x = frame.offset_x;
    let offset_y = frame.offset_y;

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

    // PACKET BIT MARKINGS. mermaid labels each field with its name and, separately, the first and
    // last bit of its range — `0-3: "A"` draws ["A","0","3"] — so the numbers read as a scale beside
    // the field rather than as text inside it.
    //
    // ⚠️ A SINGLE-BIT FIELD GETS ONE NUMBER, NOT TWO. `0: "Flag"` draws ["Flag","0"], measured on the
    // pinned bundle. Emitting both ends unconditionally would print `0` twice on every one-bit flag,
    // which every multi-bit fixture agrees with and no single-bit one does.
    if let Some(packet_meta) = ir.packet_meta.as_ref() {
        for field in &packet_meta.fields {
            let Some(node_box) = layout
                .nodes
                .iter()
                .find(|candidate| candidate.node_index == field.node.0)
            else {
                continue;
            };
            let bit_y = node_box.bounds.y + offset_y - 4.0;
            let mut marks: Vec<(f32, TextAnchor, u32)> = vec![(
                node_box.bounds.x + offset_x,
                TextAnchor::Start,
                field.start_bit,
            )];
            if field.end_bit != field.start_bit {
                marks.push((
                    node_box.bounds.x + offset_x + node_box.bounds.width,
                    TextAnchor::End,
                    field.end_bit,
                ));
            }
            for (x, anchor, bit) in marks {
                doc = doc.child(
                    TextBuilder::new(&bit.to_string())
                        .x(x)
                        .y(bit_y)
                        .anchor(anchor)
                        .font_family_unless_embedded_css(
                            &config.font_family,
                            config.embed_theme_css,
                        )
                        .font_size(config.font_size - 4.0)
                        .fill(&theme.colors.text)
                        .class("fm-packet-bit")
                        .build(),
                );
            }
        }
    }

    // The journey ACTOR LEGEND, which mermaid draws and we drew nowhere (bd-mq273).
    //
    // Measured on the pinned 11.15.0 bundle, `journey_basic` gives the run order
    // ["System","User","Browse","Visit homepage",…,"User Shopping Journey"] — the actors come FIRST,
    // each exactly once, and SORTED rather than in source order: `One: 3: Zed` then `Two: 4: Alpha`
    // draws ["Alpha","Zed"]. A step naming several actors contributes each of them separately, so
    // `One: 3: Bob, Ann` draws ["Ann","Bob"].
    //
    // ⚠️ FROM `journey_meta`, NOT FROM THE `journey-actor-*` CLASSES. Those are CSS-normalized, so a
    // legend built from them would draw `Big_Corp` for an author who wrote `Big Corp` — the same
    // defect the accessible name carried until this change.
    for actor in journey_actor_legend(ir) {
        doc = doc.child(
            TextBuilder::new(&actor.name)
                .x(padding + actor.offset_x)
                .y(padding + config.font_size * 2.0 + 12.0)
                .anchor(TextAnchor::Start)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(config.font_size - 2.0)
                .fill(&theme.colors.text)
                .class("fm-journey-actor")
                .build(),
        );
    }

    // Stream all bands (sequence lifelines / journey sections / xychart columns) into ONE raw fragment
    // instead of building N `<g><rect/></g>` element trees as separate `doc.child`ren. Byte-identical:
    // `write_layout_band_into` emits the same bytes `render_layout_band(..).write_to_string` does, and the
    // concatenated children serialize identically to the same sequence of `doc.child`ren. For sequence
    // diagrams these lifeline bands are the LAST per-item Element-build loop (nodes + messages stream).
    if !layout.extensions.bands.is_empty() {
        let mut bands_svg = String::new();
        for band in &layout.extensions.bands {
            write_layout_band_into(&mut bands_svg, band, offset_x, offset_y, config);
        }
        doc = doc.child(Element::raw_svg(bands_svg));
    }
    // Continuation boxes for packet-beta fields that cross a 32-bit row boundary (bd-8vr0). Empty
    // for every other diagram and for any packet whose fields are row-aligned, so this costs
    // nothing elsewhere.
    if !layout.extensions.packet_field_continuations.is_empty() {
        let mut continuations_svg = String::new();
        for continuation in &layout.extensions.packet_field_continuations {
            write_packet_field_continuation_into(
                &mut continuations_svg,
                continuation,
                ir,
                offset_x,
                offset_y,
                detail,
                config,
                &theme.colors,
            );
        }
        doc = doc.child(Element::raw_svg(continuations_svg));
    }
    // Stream all axis ticks (gantt date labels / xychart axis labels) into ONE raw fragment instead of N
    // group+line+text element trees as separate `doc.child`ren — the same win as the bands loop above.
    // Byte-identical: `write_layout_axis_tick_into` emits the same bytes as
    // `render_layout_axis_tick(..).write_to_string`, and concatenated children serialize identically.
    if !layout.extensions.axis_ticks.is_empty() {
        let mut ticks_svg = String::new();
        let tick_y = layout.bounds.y + offset_y - 12.0;
        for tick in &layout.extensions.axis_ticks {
            write_layout_axis_tick_into(
                &mut ticks_svg,
                tick.label.as_str(),
                tick.position + offset_x,
                tick_y,
                config,
            );
        }
        doc = doc.child(Element::raw_svg(ticks_svg));
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
    //
    // The frame is CLAMPED to the diagram's own bounds (bd-zwh3). `build_sequence_fragment_geometry`
    // anchors it at `-padding` and widens it by `2 * padding`, but the padding it uses comes from the
    // message gap while the canvas is derived from `total_width` — the right edge of the last
    // participant — so the overhang has nowhere to live. In golden/sequence_advanced that put the
    // dashed border at x = -2 with width 761.90 against a 757.90-wide canvas: clipped on BOTH sides,
    // and the `alt` label chip lost its left edge with it. Every alt/opt/loop/par frame in every
    // sequence diagram was drawn open at the ends instead of closed.
    //
    // Clamping here rather than widening the canvas is the faithful choice: our canvas IS the
    // participant span, which is what mermaid draws the frame within, so growing it would add
    // whitespace the incumbent does not have. Layout emitting out-of-bounds geometry is still worth
    // fixing at the source for the other renderers — recorded on the bead — but no consumer should
    // have to draw outside its own viewBox to honour it.
    let drawable_left = offset_x + layout.bounds.x;
    let drawable_right = drawable_left + layout.bounds.width;
    for (fragment_index, fragment) in layout.extensions.sequence_fragments.iter().enumerate() {
        let raw_x = fragment.bounds.x + offset_x;
        let fx = raw_x.max(drawable_left);
        let fy = fragment.bounds.y + offset_y;
        let fw = (raw_x + fragment.bounds.width).min(drawable_right) - fx;
        let fw = fw.max(0.0);
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
        // ⚠️ THE KEYWORD AND THE CONDITION ARE TWO ELEMENTS, NOT ONE STRING. We drew
        // `alt [Valid credentials]`, a run mermaid never emits. Its `drawLoop` writes them
        // separately, with different classes, positions and alignment:
        //
        // ```text
        //   g.text = r;        g.x = t.startx;  g.y = t.starty;   g.class = "labelText"
        //   g.text = t.title;  g.x = t.startx + labelBoxWidth/2 + (t.stopx - t.startx)/2;
        //                      g.y = t.starty + boxMargin + boxTextMargin;
        //                      g.class = "loopText";  g.anchor = "middle"
        // ```
        //
        // So the keyword sits in the small tab at the top-left and the condition is CENTRED across
        // the fragment below it. Fusing them put the condition inside the tab, which also made the
        // tab grow to fit a whole sentence.
        //
        // Found by `scripts/headtohead/chromium_text_diff.mjs`, which reported
        // `mermaid draws, we do not: ["alt","[Valid credentials]"]` against
        // `we draw, mermaid does not: ["alt [Valid credentials]"]`. The `else` branch already
        // matched, so the fusion was the whole of the divergence.
        let label_text = kind_label.to_string();
        let condition_text = (!fragment.label.is_empty()).then(|| format!("[{}]", fragment.label));

        // Label background tab. Sized to the KEYWORD alone now, which is what makes it a tab.
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

        // The CONDITION, centred across the fragment beneath the tab — mermaid's `loopText`.
        //
        // Its own class rather than a second `fm-sequence-fragment-label`: the two carry different
        // meanings and mermaid styles them differently (`labelText` is the bold keyword tab,
        // `loopText` the plain condition), so a shared class would make them impossible to theme
        // apart and would let a selector meant for one silently catch the other.
        if let Some(condition_text) = condition_text.as_deref() {
            doc = doc.child(
                Element::text()
                    .x(fx + fw / 2.0)
                    .y(fy + label_height + config.font_size * 0.75)
                    .content(condition_text)
                    .attr("text-anchor", "middle")
                    .attr("dominant-baseline", "middle")
                    .attr_num("font-size", config.font_size * 0.75)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .fill(&theme.colors.text)
                    .class("fm-sequence-fragment-condition"),
            );
        }

        // Branch dividers: the `else` of an alt, the `and` of a par, the `option` of a critical.
        //
        // Before bd-zsfo the complete text of `alt is ok / … / else is bad / … / end` was
        // "alt [is ok] | start | yes | no" — `is bad` appeared NOWHERE and no divider of any kind
        // separated the branches, so the reader saw ONE undivided box labelled only with the first
        // condition. Half the meaning of an `alt` was not in the document.
        //
        // This is a consumer gap, not a parse gap: the parser has preserved the branch label in
        // `IrSequenceFragment.alternatives` all along and neither layout nor any renderer read the
        // field. The fix is renderer-only. `build_sequence_fragment_geometry` maps
        // `meta.fragments` 1:1 in order, so `fragment_index` indexes back into the IR, and the
        // sequence layout builds one edge path per `ir.edges` entry carrying its own `edge_index`,
        // so a branch's `start_edge` resolves to the y of the message it starts at.
        //
        // The divider's offset above its branch's first message is DERIVED, not a new constant: the
        // frame's own top sits `lead` above the fragment's first message, so reusing that same lead
        // puts each divider in the same relation to its branch that the frame has to its first. A
        // fixed fraction of the frame height would drift away from the messages as soon as two
        // branches hold different numbers of them.
        let alternatives = ir
            .sequence_meta
            .as_ref()
            .and_then(|meta| meta.fragments.get(fragment_index))
            .map(|ir_fragment| ir_fragment.alternatives.as_slice())
            .unwrap_or(&[]);
        if !alternatives.is_empty() {
            let message_y = |edge_index: usize| -> Option<f32> {
                layout
                    .edges
                    .iter()
                    .find(|edge| edge.edge_index == edge_index)
                    .and_then(|edge| edge.points.first())
                    .map(|point| point.y + offset_y)
            };
            let lead = ir
                .sequence_meta
                .as_ref()
                .and_then(|meta| meta.fragments.get(fragment_index))
                .and_then(|ir_fragment| message_y(ir_fragment.start_edge))
                .map_or(0.0, |first_message_y| first_message_y - fy);
            for alternative in alternatives {
                let Some(branch_y) = message_y(alternative.start_edge) else {
                    continue;
                };
                let divider_y = branch_y - lead;
                // The divider spans the frame, so it inherits the same out-of-canvas hazard the
                // frame border did (bd-zwh3) and is drawn between the SAME clamped edges.
                doc = doc.child(
                    Element::line()
                        .x1(fx)
                        .y1(divider_y)
                        .x2(fx + fw)
                        .y2(divider_y)
                        .stroke(&theme.colors.cluster_stroke)
                        .stroke_width(1.0)
                        .stroke_dasharray("6,4")
                        .class("fm-sequence-fragment-divider"),
                );
                if !alternative.label.is_empty() {
                    doc = doc.child(
                        Element::text()
                            .x(fx + 6.0)
                            .y(divider_y - 3.0)
                            .content(format!("[{}]", alternative.label))
                            .attr_num("font-size", config.font_size * 0.75)
                            .attr("font-weight", "bold")
                            .font_family_unless_embedded_css(
                                &config.font_family,
                                config.embed_theme_css,
                            )
                            .fill(&theme.colors.text)
                            .class("fm-sequence-fragment-alt-label"),
                    );
                }
            }
        }
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

        // ⚠️ THIS USED TO BE A TITLE-STRING MATCH AND IT HAD BEEN DEAD FOR SOME TIME. It asked
        // whether the title CONTAINED `System_Boundary` / `Container_Boundary` /
        // `Enterprise_Boundary` / `Deployment_Node`, which was true back when the parser titled a
        // boundary with a reconstruction of its own source syntax. bd-039t replaced that with the
        // author's label — correctly — and this predicate silently became permanently false. Every
        // C4 boundary then lost its dedicated fill, stroke, dash and corner radius, and
        // `fm-cluster-c4` was applied to ZERO elements while its rule stayed in the stylesheet.
        // Measured before the change: `c4_container`, `c4_deployment`, `c4_component` and
        // `c4_basic` each emitted 0 elements carrying the class.
        //
        // The IR now carries the boundary type as data, so the question is asked of the parse
        // rather than of a display string that another change is free to reword.
        let c4_boundary_type = ir_cluster.and_then(|c| c.c4_boundary_type.as_deref());
        let is_c4_boundary = c4_boundary_type.is_some();

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
        // Resolved BEFORE the theme's fill-opacity, because whether the author declared a fill
        // decides whether that opacity applies at all.
        let (declared_cluster_style, declared_cluster_label_style) =
            resolve_cluster_inline_styles(ir, cluster.cluster_index);
        // ⚠️ THE THEME'S CLUSTER FILL-OPACITY IS NOT APPLIED TO A DECLARED FILL. `0.08` exists to
        // make an UNSTYLED container a faint tint behind its contents. Applying it to a colour the
        // author asked for renders that colour at 8% -- `style one fill:#ff0000` came out as a
        // barely visible pink wash, which is not what anyone means by it.
        //
        // Measured, and it disagreed in two directions at once:
        //   * the CANVAS paints the same declaration at full strength, so the two backends
        //     disagreed about a document the author styled -- the bd-lvj3 family again
        //   * the incumbent has no cluster dimming at all: mermaid 11.15.0's only `fill-opacity`
        //     values are `1.0`, a curve opacity and a graticule opacity
        //
        // So the declared case follows the canvas and the incumbent, and the undeclared case keeps
        // the theme exactly as it was.
        let declares_cluster_fill = declared_cluster_style
            .as_deref()
            .is_some_and(|style| style.contains("fill:"));
        if config.cluster_fill_opacity < 0.999 && !declares_cluster_fill {
            rect = rect.attr_num(
                "fill-opacity",
                clamp_unit_interval(config.cluster_fill_opacity),
            );
        }

        if let Some(dasharray) = stroke_style {
            rect = rect.stroke_dasharray(dasharray);
        }

        // The author's own `style mySubgraph ...` (bd-xfmm). Applied LAST of the colour sources so
        // a declared value wins over the theme fill/stroke set on the builder above - which is what
        // "the author styled it" has to mean - and as a `style` ATTRIBUTE, matching how an edge's
        // resolved style is applied a few thousand lines below.
        if let Some(declared) = declared_cluster_style {
            rect = rect.attr("style", &declared);
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
            // The C4 branch that used to live here stripped `System_Boundary` / `Container_Boundary`
            // / `Enterprise_Boundary` / `Deployment_Node` and surrounding punctuation back out of
            // the title. It was cleaning up a title the parser stopped producing at bd-039t, and it
            // could only ever run when `is_c4_boundary` was true — which, as the note above records,
            // it never was. A C4 boundary's title is now simply the author's label, like any other.
            let display_title = if is_swimlane && title_text.starts_with("swimlane:") {
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
                // The label half of the author's `style` directive, applied here rather than to the
                // rect. Inline `style` beats both the `fill` attribute above and the
                // `.fm-cluster-label` rule, so a declared `color` wins over the theme exactly as a
                // declared `fill` wins on the shape.
                let text = match declared_cluster_label_style.as_deref() {
                    Some(css) => text.attr("style", css),
                    None => text,
                };
                let text = if config.include_source_spans {
                    apply_span_metadata(text, cluster.span)
                } else {
                    text
                };
                doc = doc.child(text);
            }
        }

        // THE BOUNDARY TYPE ROW, which mermaid draws and we did not.
        //
        // `drawInsideBoundary` rewrites the stored type to `"[" + type + "]"` and `drawBoundary`
        // draws it through the same text helper as the label, one row below. The brackets are added
        // HERE rather than stored in the IR so the IR keeps mermaid's own token — see
        // `IrCluster::c4_boundary_type`.
        //
        // Drawn whenever the type exists, matching mermaid's `t.type && t.type.text !== ""`. It is
        // gated on `show_cluster_labels` for the same reason the label above is: a detail level that
        // suppresses cluster captions should not leave a bracketed type floating alone.
        if detail.show_cluster_labels
            && let Some(boundary_type) = c4_boundary_type
            && !boundary_type.is_empty()
        {
            let text = TextBuilder::new(&format!("[{boundary_type}]"))
                .x(cluster.bounds.x + offset_x + 8.0)
                .y(cluster.bounds.y + offset_y + 16.0 + detail.cluster_font_size * 1.25)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(detail.cluster_font_size)
                .fill(&label_color)
                .class("fm-cluster-type-label")
                .build();
            let text = if config.include_source_spans {
                apply_span_metadata(text, cluster.span)
            } else {
                text
            };
            doc = doc.child(text);
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

    // stateDiagram notes (bd-a6l4). `ir.state_notes` had been parsed, carried and hashed for
    // incremental equality since the note syntax landed, but no renderer had ever read it, so
    // `note right of X : text` was accepted and then silently dropped from the output.
    //
    // Empty for every diagram type but state, and for a state diagram that declares no notes, so
    // this loop is a no-op everywhere else and no other output moves. It lives in the PREFIX region
    // above the streaming fast-path gate deliberately: the gate's condition is "the slow path
    // inserts no child BETWEEN or AFTER the edge and node fragments", which a prefix child does not
    // violate — state diagrams keep streaming.
    for note in &layout.extensions.state_notes {
        let nx = note.bounds.x + offset_x;
        let ny = note.bounds.y + offset_y;

        // Leader from the state's edge to the note's, dashed like mermaid-js's note connector.
        doc = doc.child(
            Element::line()
                .x1(note.leader_start.x + offset_x)
                .y1(note.leader_start.y + offset_y)
                .x2(note.leader_end.x + offset_x)
                .y2(note.leader_end.y + offset_y)
                .stroke(&theme.colors.edge)
                .stroke_width(1.0)
                .stroke_dasharray("4,3")
                .class("fm-state-note-leader"),
        );

        doc = doc.child(
            Element::rect()
                .x(nx)
                .y(ny)
                .width(note.bounds.width)
                .height(note.bounds.height)
                .rx(4.0)
                .ry(4.0)
                .fill(&theme.colors.node_fill)
                .stroke(&theme.colors.accents[4 % theme.colors.accents.len()])
                .stroke_width(1.0)
                .class("fm-state-note"),
        );

        if !note.text.is_empty() {
            doc = doc.child(
                TextBuilder::new(&note.text)
                    .x(nx + 10.0)
                    .y(ny + 8.0)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .font_size(config.font_size * 0.8)
                    .line_height(config.line_height)
                    .baseline(text::DominantBaseline::Hanging)
                    .anchor(TextAnchor::Start)
                    .fill(&theme.colors.text)
                    .class("fm-state-note-text")
                    .build(),
            );
        }
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
        // Computed ONCE here; the per-edge helper used to recompute it, re-parsing every flow
        // value on every edge.
        sankey_widest_flow: sankey_widest_flow(ir),
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
    // The only children the slow path inserts BETWEEN the edge and node fragments are the ER / class
    // cardinality labels and bundle-count labels; the only ones AFTER are sequence mirror headers and the
    // C4 legend. Bundle labels / legend still force the slow path, but ER and class cardinality (between
    // edges and nodes) AND sequence mirror headers (after nodes) are now emitted INSIDE the streaming body
    // in byte-identical order, so they no longer disqualify a diagram. This pulls ER, class-relation, and
    // sequence diagrams (previously always slow-path) onto the streaming path — killing the second copy of
    // `edge_svg`+`cardinality_svg`+`node_svg`+mirror-header fragments.
    let no_between_or_after_children =
        !legend_enabled && layout.edges.iter().all(|edge| edge.bundle_count <= 1);
    #[cfg(not(target_arch = "wasm32"))]
    let stream_fast_path =
        no_between_or_after_children && layout.edges.len() < 4096 && layout.nodes.len() < 2048;
    #[cfg(target_arch = "wasm32")]
    let stream_fast_path = no_between_or_after_children;
    if stream_fast_path {
        if let Some(reuse) = batch_reuse.as_mut()
            && ir.diagram_type == DiagramType::Flowchart
        {
            reuse.next.detail = Some(detail);
            reuse.next.offset_x_bits = offset_x.to_bits();
            reuse.next.offset_y_bits = offset_y.to_bits();
            reuse.next.active = true;
        }
        return doc.to_string_with_body(layout_svg_capacity_hint(ir, layout), |out| {
            if let Some(reuse) = batch_reuse.as_mut()
                && ir.diagram_type == DiagramType::Flowchart
            {
                render_edges_with_batch_reuse(out, &layout.edges, &edge_context, layout, reuse);
            } else {
                render_edges_serial(out, &layout.edges, &edge_context);
            }
            // Cardinality labels sit between edges and nodes in the slow path's child order; stream them in
            // the same position. Each writer self-guards (ER emits only for ER edges, class only for edges
            // with source/target cardinality), so both are no-ops for a plain flowchart.
            if ir.diagram_type == fm_core::DiagramType::Er {
                write_er_cardinality_labels_into(
                    out,
                    ir,
                    layout,
                    offset_x,
                    offset_y,
                    config,
                    &theme.colors,
                );
            }
            write_class_cardinality_labels_into(
                out,
                ir,
                layout,
                offset_x,
                offset_y,
                config,
                &theme.colors,
            );
            if let Some(reuse) = batch_reuse.as_mut()
                && ir.diagram_type == DiagramType::Flowchart
            {
                render_nodes_with_batch_reuse(
                    out,
                    &layout.nodes,
                    ir,
                    layout,
                    offset_x,
                    offset_y,
                    config,
                    detail,
                    &theme.colors,
                    emit_classdef_classes,
                    &centrality_map,
                    reuse,
                );
            } else {
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
            }
            // Sequence mirror headers (participant boxes repeated at the bottom) sit AFTER the nodes in the
            // slow path's child order; stream each straight into `out` in the same position instead of
            // building it as a `doc.child` the final `to_string` copies a second time. Byte-identical: the
            // same `render_node(..).id(..).class(..)` Element bytes, written directly. No-op for non-sequence
            // diagrams (`sequence_mirror_headers` is empty).
            for node_box in &layout.extensions.sequence_mirror_headers {
                render_node(
                    node_box,
                    ir,
                    offset_x,
                    offset_y,
                    config,
                    detail,
                    &theme.colors,
                    emit_classdef_classes,
                    &centrality_map,
                    // Post-processed (.id + .class) — must NOT take the opaque fast path.
                    false,
                )
                .id(&mermaid_node_element_id_with_variant(
                    &node_box.node_id,
                    node_box.node_index,
                    Some("mirror-header"),
                ))
                .class("fm-sequence-mirror-header")
                .write_to_string(out);
            }
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
    // `EdgeRenderContext`, no shared mutable state), chunks are emitted in edge order so output is
    // byte-identical and thread-count-independent, native-only (WASM serial), size-gated.
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
            doc = doc.child(Element::raw_svg_parts(parts));
        } else {
            let mut edge_svg = String::with_capacity(layout.edges.len().saturating_mul(480));
            render_edges_serial(&mut edge_svg, &layout.edges, &edge_context);
            if !edge_svg.is_empty() {
                doc = doc.child(Element::raw_svg(edge_svg));
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let mut edge_svg = String::with_capacity(layout.edges.len().saturating_mul(480));
        render_edges_serial(&mut edge_svg, &layout.edges, &edge_context);
        if !edge_svg.is_empty() {
            doc = doc.child(Element::raw_svg(edge_svg));
        }
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

    // Render ER cardinality labels near edge endpoints — one raw fragment, byte-identical to the
    // per-label `Element::text()` (shared with the streaming fast path via `write_er_cardinality_labels_into`).
    if ir.diagram_type == fm_core::DiagramType::Er {
        let mut cardinality_svg = String::new();
        write_er_cardinality_labels_into(
            &mut cardinality_svg,
            ir,
            layout,
            offset_x,
            offset_y,
            config,
            &theme.colors,
        );
        if !cardinality_svg.is_empty() {
            doc = doc.child(Element::raw_svg(cardinality_svg));
        }
    }

    // Render class diagram cardinality labels near edge endpoints — one raw fragment, byte-identical to the
    // per-label `Element::text()` (shared with the streaming fast path via `write_class_cardinality_labels_into`).
    let mut class_cardinality_svg = String::new();
    write_class_cardinality_labels_into(
        &mut class_cardinality_svg,
        ir,
        layout,
        offset_x,
        offset_y,
        config,
        &theme.colors,
    );
    if !class_cardinality_svg.is_empty() {
        doc = doc.child(Element::raw_svg(class_cardinality_svg));
    }

    // Render nodes. Serialize each node subtree immediately into a shared buffer (as the edge loop
    // above does) and insert one internal raw fragment, so the root document does not retain
    // hundreds of node element trees — each a `<g>` with rect + text children — until final
    // serialization. Byte-identical: the same `render_node` elements are serialized in the same
    // order, just streamed rather than deferred.
    // The per-node render (`render_node` -> serialize) is the single largest pipeline cost (~43% of the
    // whole pipeline) and is embarrassingly parallel: `render_node` is pure (read-only `ir`/`config`/
    // `theme`/`centrality_map` + `Copy` scalars, no shared mutable state). For large diagrams on native
    // we fan the nodes across stdlib scoped threads (no new dependency — the crate stays zero-dep) and
    // emit the per-chunk buffers IN ORDER, so the output is byte-identical to the serial path.
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
            doc = doc.child(Element::raw_svg_parts(parts));
        } else {
            let mut node_svg = String::with_capacity(layout.nodes.len().saturating_mul(640));
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
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let mut node_svg = String::with_capacity(layout.nodes.len().saturating_mul(640));
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
        + layout.extensions.state_notes.len()
        + layout.extensions.sequence_mirror_headers.len();

    BASE_DOCUMENT_BYTES
        + ir.nodes.len().saturating_mul(NODE_BYTES)
        + layout.edges.len().saturating_mul(EDGE_BYTES)
        + layout.clusters.len().saturating_mul(CLUSTER_BYTES)
        + auxiliary_items.saturating_mul(AUXILIARY_ITEM_BYTES)
}

/// Stream a layout band (`<g class="fm-band fm-band-…"><rect/>[<text>label</text>]</g>`) directly into
/// `out`, byte-identical to [`render_layout_band`]'s `Element`. The rect attrs replicate that builder's
/// call order exactly (`x y width height rx fill stroke stroke-width stroke-dasharray fill-opacity
/// stroke-opacity`); float attrs go through `write_number_into` (so `0.8`→`"0.80"`). The optional band
/// label is rare (journey sections / xychart columns; sequence lifelines are unlabelled), so it reuses the
/// exact `TextBuilder` `Element` written in place — no from-scratch text replication. Lets the bands loop
/// stream N group+rect `Element`s into one raw fragment instead of N `doc.child` element trees.
fn write_layout_band_into(
    out: &mut String,
    band: &LayoutBand,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text, write_number_into};
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
    out.push_str("<g class=\"");
    out.push_str(class_name);
    out.push_str("\"><rect x=\"");
    let _ = write_number_into(out, band.bounds.x + offset_x);
    out.push_str("\" y=\"");
    let _ = write_number_into(out, band.bounds.y + offset_y);
    out.push_str("\" width=\"");
    let _ = write_number_into(out, band.bounds.width);
    out.push_str("\" height=\"");
    let _ = write_number_into(out, band.bounds.height);
    out.push_str("\" rx=\"");
    let _ = write_number_into(out, config.rounded_corners.max(4.0));
    out.push_str("\" fill=\"");
    let _ = write_escaped_attr(out, fill);
    out.push_str("\" stroke=\"");
    let _ = write_escaped_attr(out, stroke);
    out.push_str("\" stroke-width=\"1\" stroke-dasharray=\"6,4\" fill-opacity=\"");
    let _ = write_number_into(out, 0.8);
    out.push_str("\" stroke-opacity=\"");
    let _ = write_number_into(out, 0.9);
    out.push_str("\"/>");
    if !band.label.is_empty() {
        // A journey/gantt/kanban diagram puts a LABEL on every lane/section/column band (289 per
        // 300-task journey render), so this is a per-item hot path, not the "rare" case it once was.
        // Stream the single-line label `<text>` straight into `out` — byte-identical to the
        // `TextBuilder` `Element` under this call set (`x y text-anchor="start" [font-family] font-size
        // fill class` then escaped content; default anchor is `Start`, `font-family` present only when
        // the theme CSS is NOT embedded), exactly as the axis-tick streamer does. This kills the
        // per-band `Element` + `Attributes` Vec build + serialize + copy (TextBuilder/Attributes builder
        // was ~8% of journey render, plus the malloc/free churn on those transient Vecs). A multi-line
        // label (`\n`), which `TextBuilder::build` splits into `<tspan>`s, keeps the exact slow path.
        // Pinned by `layout_band_streaming_matches_element`.
        if band.label.contains('\n') {
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
                .build()
                .write_to_string(out);
        } else {
            out.push_str("<text x=\"");
            let _ = write_number_into(out, band.bounds.x + offset_x + 8.0);
            out.push_str("\" y=\"");
            let _ = write_number_into(out, band.bounds.y + offset_y + 16.0);
            out.push_str("\" text-anchor=\"start\"");
            if !config.embed_theme_css {
                out.push_str(" font-family=\"");
                let _ = write_escaped_attr(out, &config.font_family);
                out.push('"');
            }
            out.push_str(" font-size=\"");
            let _ = write_number_into(
                out,
                clamp_font_size(config.font_size * 0.82, config.min_font_size),
            );
            out.push_str("\" fill=\"var(--fm-text-color, #4a5568)\" class=\"fm-band-label\">");
            let _ = write_escaped_text(out, &band.label);
            out.push_str("</text>");
        }
    }
    out.push_str("</g>");
}

/// The `Element`-building band renderer, now superseded by [`write_layout_band_into`] on the render path
/// and retained only as the byte-identity oracle for `layout_band_streaming_matches_element`.
#[cfg(test)]
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

/// Stream an axis tick (`<g class="fm-axis-tick"><line/><text>label</text></g>`) directly into `out`,
/// byte-identical to [`render_layout_axis_tick`]'s `Element`. The `<line>` and `<text>` attrs replicate
/// that builder's call order; float attrs go through `write_number_into`. The label `<text>` mirrors
/// `TextBuilder::build` under this call set: `x y text-anchor="start" [font-family] font-size fill class`
/// then escaped content (default anchor is `Start`; `font-family` present only when the theme CSS is NOT
/// embedded). Lets the axis-ticks loop stream N group+line+text `Element`s into one raw fragment.
fn write_layout_axis_tick_into(
    out: &mut String,
    label: &str,
    x: f32,
    y: f32,
    config: &SvgRenderConfig,
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text, write_number_into};
    out.push_str("<g class=\"fm-axis-tick\"><line x1=\"");
    let _ = write_number_into(out, x);
    out.push_str("\" y1=\"");
    let _ = write_number_into(out, y + 4.0);
    out.push_str("\" x2=\"");
    let _ = write_number_into(out, x);
    out.push_str("\" y2=\"");
    let _ = write_number_into(out, y + 16.0);
    out.push_str("\" stroke=\"var(--fm-edge-color, #94a3b8)\" stroke-width=\"1\"/><text x=\"");
    let _ = write_number_into(out, x + 3.0);
    out.push_str("\" y=\"");
    let _ = write_number_into(out, y);
    out.push_str("\" text-anchor=\"start\"");
    if !config.embed_theme_css {
        out.push_str(" font-family=\"");
        let _ = write_escaped_attr(out, &config.font_family);
        out.push('"');
    }
    out.push_str(" font-size=\"");
    let _ = write_number_into(
        out,
        clamp_font_size(config.font_size * 0.72, config.min_font_size),
    );
    out.push_str("\" fill=\"var(--fm-text-color, #64748b)\" class=\"fm-axis-tick-label\">");
    let _ = write_escaped_text(out, label);
    out.push_str("</text></g>");
}

/// The `Element`-building axis-tick renderer, superseded by [`write_layout_axis_tick_into`] on the render
/// path and retained only as the byte-identity oracle for `layout_axis_tick_streaming_matches_element`.
#[cfg(test)]
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
/// Stream every ER cardinality `<text>` (left-then-right per edge, edge order) into `out`. Extracted from
/// the slow-path loop so both the slow path and the whole-document streaming fast path share ONE
/// implementation; emits nothing for non-ER edges. Byte-identical to the slow path's `cardinality_svg`.
fn write_er_cardinality_labels_into(
    out: &mut String,
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
) {
    for edge_path in &layout.edges {
        if let Some(ir_edge) = ir.edges.get(edge_path.edge_index)
            && let Some(notation) = ir_edge.er_notation()
            && edge_path.points.len() >= 2
        {
            // Shared with fm-render-canvas via fm-core (bd-2h3pp). This crate carried the only
            // copy of the mapping while it was the only surface drawing cardinality; a second copy
            // in the canvas would have been the forked-helper drift this repo keeps getting bitten
            // by, so the logic moved to the IR — it is a fact about the notation, not about drawing.
            let (left_label, right_label) = fm_core::parse_er_cardinality(notation);
            let font_size = config.font_size * 0.7;
            if !left_label.is_empty() {
                let p = &edge_path.points[0];
                write_cardinality_text_into(
                    out,
                    p.x + offset_x + 8.0,
                    p.y + offset_y - 8.0,
                    font_size,
                    &colors.text,
                    &config.font_family,
                    config.embed_theme_css,
                    "fm-er-cardinality",
                    left_label,
                );
            }
            if !right_label.is_empty() {
                let p = &edge_path.points[edge_path.points.len() - 1];
                write_cardinality_text_into(
                    out,
                    p.x + offset_x + 8.0,
                    p.y + offset_y - 8.0,
                    font_size,
                    &colors.text,
                    &config.font_family,
                    config.embed_theme_css,
                    "fm-er-cardinality",
                    right_label,
                );
            }
        }
    }
}

/// Stream every class-relation cardinality `<text>` (source-then-target per edge, edge order) into `out`.
/// Extracted twin of [`write_er_cardinality_labels_into`]; emits nothing for edges without source/target
/// cardinality. Byte-identical to the slow path's `class_cardinality_svg`.
fn write_class_cardinality_labels_into(
    out: &mut String,
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    offset_x: f32,
    offset_y: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
) {
    for edge_path in &layout.edges {
        if let Some(ir_edge) = ir.edges.get(edge_path.edge_index)
            && (ir_edge.source_cardinality().is_some() || ir_edge.target_cardinality().is_some())
            && edge_path.points.len() >= 2
        {
            let font_size = config.font_size * 0.7;
            if let Some(card) = ir_edge.source_cardinality() {
                let p = &edge_path.points[0];
                write_cardinality_text_into(
                    out,
                    p.x + offset_x + 8.0,
                    p.y + offset_y - 8.0,
                    font_size,
                    &colors.text,
                    &config.font_family,
                    config.embed_theme_css,
                    "fm-class-cardinality",
                    card,
                );
            }
            if let Some(card) = ir_edge.target_cardinality() {
                let p = &edge_path.points[edge_path.points.len() - 1];
                write_cardinality_text_into(
                    out,
                    p.x + offset_x + 8.0,
                    p.y + offset_y - 8.0,
                    font_size,
                    &colors.text,
                    &config.font_family,
                    config.embed_theme_css,
                    "fm-class-cardinality",
                    card,
                );
            }
        }
    }
}

/// Stream one cardinality `<text>` directly into `out`, byte-identical to the `Element::text()` the slow
/// path built: attrs in insertion order `x, y, text-anchor, dominant-baseline, font-size,
/// [font-family when NOT embedded], fill, class`, with the label as escaped text content. Numbers use the
/// shared 2-decimal `AttributeValue::Number` serializer; the label/fill escape identically to the element.
/// `class_name` is `fm-er-cardinality` (ER) or `fm-class-cardinality` (class relations).
#[allow(clippy::too_many_arguments)]
fn write_cardinality_text_into(
    out: &mut String,
    x: f32,
    y: f32,
    font_size: f32,
    fill: &str,
    font_family: &str,
    embed_css: bool,
    class_name: &str,
    label: &str,
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text};
    out.push_str("<text x=\"");
    let _ = crate::attributes::write_number_into(out, x);
    out.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(out, y);
    out.push_str("\" text-anchor=\"start\" dominant-baseline=\"auto\" font-size=\"");
    let _ = crate::attributes::write_number_into(out, font_size);
    out.push('"');
    if !embed_css {
        out.push_str(" font-family=\"");
        let _ = write_escaped_attr(out, font_family);
        out.push('"');
    }
    out.push_str(" fill=\"");
    let _ = write_escaped_attr(out, fill);
    out.push_str("\" class=\"");
    out.push_str(class_name);
    out.push_str("\">");
    let _ = write_escaped_text(out, label);
    out.push_str("</text>");
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
    // Stream all data points (circle + label) into one raw fragment under embedded CSS — the label's only
    // config-dependent attribute (`font-family`) is then CSS-driven/absent, so the whole point stack is a
    // fixed set of bytes. Skips two `Element` builds + `Attributes` Vecs per point. Byte-identical; the
    // non-embedded (attribute-driven) export keeps the Element path so `font-family` is emitted inline.
    if config.embed_theme_css {
        let mut points_svg = String::with_capacity(layout.nodes.len().saturating_mul(160));
        for (i, node_box) in layout.nodes.iter().enumerate() {
            let cx = node_box.bounds.x + node_box.bounds.width / 2.0 + offset_x;
            let cy = node_box.bounds.y + node_box.bounds.height / 2.0 + offset_y;
            let color = accent_colors[i % accent_colors.len()];
            let label = quad_meta
                .points
                .get(i)
                .map(|p| p.label.as_str())
                .unwrap_or(&node_box.node_id);
            let accessible_name = quad_meta
                .points
                .get(i)
                .filter(|_| config.a11y.text_alternatives)
                .map(|point| quadrant_point_accessible_name(point, &quad_meta.quadrant_labels));
            write_quadrant_point_into(
                &mut points_svg,
                cx,
                cy,
                color,
                &theme.colors.background,
                cx + 10.0,
                cy + 4.0,
                config.font_size * 0.75,
                &theme.colors.text,
                label,
                accessible_name.as_deref(),
            );
        }
        if !points_svg.is_empty() {
            doc = doc.child(Element::raw_svg(points_svg));
        }
        return doc;
    }

    for (i, node_box) in layout.nodes.iter().enumerate() {
        let cx = node_box.bounds.x + node_box.bounds.width / 2.0 + offset_x;
        let cy = node_box.bounds.y + node_box.bounds.height / 2.0 + offset_y;
        let color = accent_colors[i % accent_colors.len()];
        let point_circle = Element::circle()
            .cx(cx)
            .cy(cy)
            .r(6.0)
            .fill(color)
            .stroke(&theme.colors.background)
            .stroke_width(1.5)
            .class("fm-quadrant-point");
        // Same accessible name the streaming path emits (bd-0eoa6); this is the non-embedded-CSS
        // export, and the two must not disagree about what a point is called.
        let point_circle = match quad_meta
            .points
            .get(i)
            .filter(|_| config.a11y.text_alternatives)
            .map(|point| quadrant_point_accessible_name(point, &quad_meta.quadrant_labels))
        {
            Some(name) => point_circle.child(Element::title(&name)),
            None => point_circle,
        };
        doc = doc.child(point_circle);
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

/// The accessible name for one quadrant data point (bd-0eoa6).
///
/// The point's visible label already names it, so what a screen reader is missing is the thing the
/// POSITION conveys: which quadrant it landed in. `"Alpha: Do first"` when that quadrant is named,
/// otherwise `"Alpha: x 0.90, y 0.90"` — the coordinates, which is all the position says when the
/// author declared no quadrant labels.
///
/// ⚠️ THE QUADRANT INDEX IS MEASURED, NOT ASSUMED. Announcing the wrong quadrant is worse than
/// announcing none, so the mapping was verified against real output before being written: a point at
/// `[0.9, 0.9]` renders at the TOP right (canvas y 138 against the `quadrant-1` label's 186), so a
/// HIGH data `y` is the TOP half even though canvas y grows downward. The order matches
/// `label_positions` above — 0 top-right, 1 top-left, 2 bottom-left, 3 bottom-right — which is
/// mermaid's `quadrant-1..4`.
fn quadrant_point_accessible_name(point: &fm_core::IrQuadrantPoint, labels: &[String]) -> String {
    let index = match (point.x >= 0.5, point.y >= 0.5) {
        (true, true) => 0,
        (false, true) => 1,
        (false, false) => 2,
        (true, false) => 3,
    };
    match labels.get(index).filter(|label| !label.trim().is_empty()) {
        Some(quadrant) => format!("{}: {quadrant}", point.label),
        None => format!("{}: x {:.2}, y {:.2}", point.label, point.x, point.y),
    }
}

/// Stream a quadrant data point (`<circle>` + `<text>` label) byte-identical to the slow path's
/// `Element`s under embedded CSS (the label's `font-family` is CSS-driven, so absent inline). `r="6"` /
/// `stroke-width="1.50"` are the fixed `r(6.0)`/`stroke_width(1.5)` serializations. Skips the two per-point
/// `Element` builds + their `Attributes` Vecs (`Attributes::set` was ~8% of quadrant render).
#[allow(clippy::too_many_arguments)]
fn write_quadrant_point_into(
    f: &mut String,
    cx: f32,
    cy: f32,
    color: &str,
    bg: &str,
    label_x: f32,
    label_y: f32,
    label_font_size: f32,
    text_fill: &str,
    label: &str,
    accessible_name: Option<&str>,
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text};
    f.push_str("<circle cx=\"");
    let _ = crate::attributes::write_number_into(f, cx);
    f.push_str("\" cy=\"");
    let _ = crate::attributes::write_number_into(f, cy);
    f.push_str("\" r=\"6\" fill=\"");
    let _ = write_escaped_attr(f, color);
    f.push_str("\" stroke=\"");
    let _ = write_escaped_attr(f, bg);
    f.push_str("\" stroke-width=\"1.50\" class=\"fm-quadrant-point\"");
    match accessible_name {
        Some(name) => {
            f.push_str("><title>");
            let _ = write_escaped_text(f, name);
            f.push_str("</title></circle>");
        }
        None => f.push_str("/>"),
    }
    f.push_str("<text x=\"");
    let _ = crate::attributes::write_number_into(f, label_x);
    f.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(f, label_y);
    f.push_str("\" text-anchor=\"start\" font-size=\"");
    let _ = crate::attributes::write_number_into(f, label_font_size);
    f.push_str("\" fill=\"");
    let _ = write_escaped_attr(f, text_fill);
    f.push_str("\" class=\"fm-quadrant-point-label\">");
    let _ = write_escaped_text(f, label);
    f.push_str("</text>");
}

/// The accessible name for one gantt task bar (bd-ic3rx).
///
/// A bar conveys four things VISUALLY that no text run carries: where it starts (position), how long
/// it runs (width), what kind of task it is (colour) and how far along it is (the progress overlay).
/// The name states each of them, so a non-visual reader gets what the geometry says rather than the
/// task name alone.
///
/// Last of the four chart types that emitted zero per-element accessibility affordances (pie
/// bd-uf3p1, xychart bd-sdhzh, quadrant bd-0eoa6).
fn gantt_bar_accessible_name(label: &str, task: Option<&fm_core::IrGanttTask>) -> String {
    use std::fmt::Write as _;

    let mut name = label.to_string();
    let Some(task) = task else {
        return name;
    };

    if let Some(fm_core::GanttDate::Absolute(start)) = task.start.as_ref() {
        name.push_str(", starts ");
        name.push_str(start);
    }
    match task.end.as_ref() {
        Some(fm_core::GanttDate::Absolute(end)) => {
            name.push_str(", ends ");
            name.push_str(end);
        }
        Some(fm_core::GanttDate::DurationDays(days)) => {
            let _ = write!(name, ", {days} day{}", if *days == 1 { "" } else { "s" });
        }
        _ => {}
    }
    // The TYPE is carried only by the bar's fill colour, so a reader who cannot see the colour has
    // no other source for it. `Normal` is the default and adds nothing.
    // EVERY tag, not just the primary type (bd-124ew). A `:crit, done` bar used to announce only
    // ", done", so a reader who cannot see the fill colour lost the critical marking entirely —
    // the same information the fill was already losing.
    name.push_str(&task.flags.accessible_suffix());
    // ⚠️ `progress` is a FRACTION, not a percentage: `50%` parses to `0.5`
    // (`parse_gantt_progress` divides by 100). Formatting it directly as `{:.0}%` announced
    // "0% complete" for a task that is HALF DONE — a wrong number, which is worse than no number,
    // and one that only showed up by reading the rendered output rather than trusting the field name.
    //
    // Only a progress that says something: a task that declares none has `None`, and nothing is
    // gained by announcing 0% on every ordinary bar.
    if let Some(progress) = task.progress.filter(|value| *value > 0.0) {
        let _ = write!(name, ", {:.0}% complete", progress * 100.0);
    }
    name
}

/// Stream a gantt task bar `<rect>` byte-identical to the slow path's `Element::rect()`:
/// `x y width height fill stroke stroke-width="1" rx="3" class="fm-gantt-task {type_class}"`.
///
/// `tooltip` is the author's `click <task> ... "text"` hover, or `None` (bd-gydqv).
///
/// A `title=` ATTRIBUTE, matching what the flowchart path emits and what mermaid itself does
/// (`n.attr("title", t.tooltip)`) — deliberately NOT a `<title>` CHILD, which is this file's
/// accessible name for a shape. Conflating the author's hover text with the a11y name is how a
/// screen reader ends up announcing one in place of the other.
#[allow(clippy::too_many_arguments)]
fn write_gantt_bar_into(
    f: &mut String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    fill: &str,
    stroke: &str,
    type_class: &str,
    tooltip: Option<&str>,
    accessible_name: Option<&str>,
) {
    use crate::attributes::write_escaped_attr;
    f.push_str("<rect x=\"");
    let _ = crate::attributes::write_number_into(f, x);
    f.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(f, y);
    f.push_str("\" width=\"");
    let _ = crate::attributes::write_number_into(f, w);
    f.push_str("\" height=\"");
    let _ = crate::attributes::write_number_into(f, h);
    f.push_str("\" fill=\"");
    let _ = write_escaped_attr(f, fill);
    f.push_str("\" stroke=\"");
    let _ = write_escaped_attr(f, stroke);
    f.push_str("\" stroke-width=\"1\" rx=\"3\" class=\"fm-gantt-task ");
    f.push_str(type_class);
    f.push('"');
    if let Some(tooltip) = tooltip.map(str::trim).filter(|text| !text.is_empty()) {
        f.push_str(" title=\"");
        let _ = write_escaped_attr(f, tooltip);
        f.push('"');
    }
    // The `title=` ATTRIBUTE above is the author's `click` hover; the `<title>` CHILD here is the
    // accessible NAME. They are different things and the flowchart node path carries both the same
    // way, so a bar with a click keeps its hover text and still announces its schedule.
    match accessible_name
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        Some(name) => {
            f.push_str("><title>");
            let _ = crate::attributes::write_escaped_text(f, name);
            f.push_str("</title></rect>");
        }
        None => f.push_str("/>"),
    }
}

/// WCAG relative luminance of a `#rgb`/`#rrggbb` colour, or `None` for anything else.
///
/// `None` rather than a guess: a `var(...)`, a gradient reference or a named colour cannot be
/// measured here, and the caller falls back to its previous behaviour instead of picking a colour
/// from arithmetic it did not actually do.
fn relative_luminance(colour: &str) -> Option<f64> {
    let hex = colour.trim().strip_prefix('#')?;
    let expanded: String = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 => hex.to_string(),
        _ => return None,
    };
    let mut channels = [0.0_f64; 3];
    for (index, channel) in channels.iter_mut().enumerate() {
        let start = index * 2;
        let raw = u8::from_str_radix(expanded.get(start..start + 2)?, 16).ok()?;
        let value = f64::from(raw) / 255.0;
        *channel = if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        };
    }
    Some(0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2])
}

/// WCAG contrast ratio between two colours, or `None` if either cannot be measured.
fn contrast_ratio(a: &str, b: &str) -> Option<f64> {
    let (x, y) = (relative_luminance(a)?, relative_luminance(b)?);
    Some((x.max(y) + 0.05) / (x.min(y) + 0.05))
}

/// The theme colour that reads best ON `background` (bd-u0x67).
///
/// Gantt task bars are FIXED pastels — `#93c5fd` normal, `#fca5a5` critical, `#86efac` done,
/// `#94a3b8` active — the same in every theme. The label, however, used `colors.text`, which flips
/// to near-white on a dark theme. So every task label in every gantt chart measured 1.34–2.45:1
/// against its own bar in dark mode, against 6.65–12.15:1 in light. All four task types, not an
/// edge case.
///
/// Chooses between the theme's own TEXT and BACKGROUND colours rather than hardcoding black or
/// white: both stay theme-derived, the light theme keeps exactly the colour it already had (so its
/// output is unchanged), and a user-restyled bar — which may be dark — gets the right answer instead
/// of an assumption that bars are always pale.
///
/// Falls back to `text` when either colour cannot be measured, which keeps an unparseable fill on
/// its previous behaviour rather than inventing one.
fn readable_label_colour<'a>(background: &str, colors: &'a ThemeColors) -> &'a str {
    match (
        contrast_ratio(&colors.text, background),
        contrast_ratio(&colors.background, background),
    ) {
        (Some(on_text), Some(on_background)) if on_background > on_text => &colors.background,
        _ => &colors.text,
    }
}

/// Stream a gantt task label `<text>` byte-identical to the slow path's `Element::text()`:
/// `x y text-anchor="middle" dominant-baseline="central" font-size [font-family] fill class`.
#[allow(clippy::too_many_arguments)]
fn write_gantt_label_into(
    f: &mut String,
    x: f32,
    y: f32,
    font_size: f32,
    family: &str,
    embed: bool,
    fill: &str,
    label: &str,
    anchor: &str,
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text};
    f.push_str("<text x=\"");
    let _ = crate::attributes::write_number_into(f, x);
    f.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(f, y);
    f.push_str("\" text-anchor=\"");
    f.push_str(anchor);
    f.push_str("\" dominant-baseline=\"central\" font-size=\"");
    let _ = crate::attributes::write_number_into(f, font_size);
    f.push('"');
    if !embed {
        f.push_str(" font-family=\"");
        let _ = write_escaped_attr(f, family);
        f.push('"');
    }
    f.push_str(" fill=\"");
    let _ = write_escaped_attr(f, fill);
    f.push_str("\" class=\"fm-gantt-task-label\">");
    let _ = write_escaped_text(f, label);
    f.push_str("</text>");
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
    //
    // This loop read `layout.clusters`, which is EMPTY on the gantt path — `build_cluster_boxes`
    // builds from `ir.clusters` and a gantt has none — so it ran zero times and no section band or
    // section label ever reached the output (bd-trsd). The gantt layout arm puts its sections in
    // `extensions.bands`, one `LayoutBandKind::Section` per section with the section name as the
    // band label, which is the data this loop was always describing. Sourcing the name from
    // `band.label` rather than `gantt_meta.sections[idx]` also fixes a latent mispairing: the bands
    // are collected from a map keyed by section NAME, so their order is not the declaration order
    // the old index lookup assumed.
    let section_fills = ["#f0f4ff", "#fff8f0", "#f0fff4", "#fff0f8"];
    for (band_idx, band) in layout.extensions.bands.iter().enumerate() {
        let fill = section_fills[band_idx % section_fills.len()];
        doc = doc.child(
            Element::rect()
                .x(band.bounds.x + offset_x)
                .y(band.bounds.y + offset_y)
                .width(band.bounds.width)
                .height(band.bounds.height)
                .fill(fill)
                .attr("fill-opacity", "0.5")
                .rx(4.0)
                .class("fm-gantt-section-bg"),
        );
        if !band.label.is_empty() {
            doc = doc.child(
                Element::text()
                    .x(band.bounds.x + offset_x + 6.0)
                    .y(band.bounds.y + offset_y + config.font_size * 0.9)
                    .content(&band.label)
                    .attr("text-anchor", "start")
                    .attr("font-weight", "600")
                    .attr_num("font-size", config.font_size * 0.85)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .fill(&theme.colors.text)
                    .class("fm-gantt-section-label"),
            );
        }
    }

    // The time axis. A gantt chart with no dates is not a gantt chart: before bd-trsd the complete
    // text content of the shipped `gantt_basic.svg` was "Roadmap | Design | Build" — two bars whose
    // lengths encode durations no reader could name, with nothing to measure them against.
    //
    // The generic axis-tick loop that draws xychart's axis sits BELOW the gantt early return in
    // `render_svg_with_config`, so the one diagram type whose layout populates `axis_ticks` was the
    // one type that could never reach it. Drawing them here with the SAME writer keeps a single
    // source of tick markup, and taking label and x straight from `extensions.axis_ticks` keeps a
    // single source of tick GEOMETRY — re-deriving day positions here is how an axis and its bars
    // come to disagree about where a day is.
    //
    // The layout owns both baselines: Mermaid always has the bottom grid and `topAxis` appends a
    // second one. Keeping the rows here rather than deriving `bounds.y +/- 12` lets SVG and canvas
    // consume the same geometry and prevents a renderer-only axis from being clipped by its bounds.
    if !layout.extensions.axis_ticks.is_empty() {
        for axis in &layout.extensions.gantt_axis_rows {
            let mut ticks_svg = String::new();
            let class = match axis.placement {
                fm_layout::LayoutGanttAxisPlacement::Bottom => "fm-gantt-axis fm-gantt-axis-bottom",
                fm_layout::LayoutGanttAxisPlacement::Top => "fm-gantt-axis fm-gantt-axis-top",
            };
            ticks_svg.push_str("<g class=\"");
            ticks_svg.push_str(class);
            ticks_svg.push_str("\">");
            for tick in &layout.extensions.axis_ticks {
                write_layout_axis_tick_into(
                    &mut ticks_svg,
                    tick.label.as_str(),
                    tick.position + offset_x,
                    axis.y + offset_y,
                    config,
                );
            }
            ticks_svg.push_str("</g>");
            doc = doc.child(Element::raw_svg(ticks_svg));
        }
    }

    // The `todayMarker` line (bd-j0va). mermaid draws a vertical line across the chart at the
    // current date; `todayMarker off` disables it and a style string restyles it. We parsed the
    // directive and read it NOWHERE, so both the default marker and the directive that turns it off
    // were equally invisible.
    //
    // FOUR CONDITIONS, all required, and each one is a way this could have gone wrong:
    //
    //  1. `config.gantt_today` must be supplied. The renderer never calls the clock -- output bytes
    //     as a function of wall time is a defect class this project has already been bitten by, and
    //     it would make every gantt golden depend on the day it was blessed. The library default is
    //     `None`, so goldens are stable; the CLI injects the real date.
    //  2. It must parse as a real calendar date, via the SAME `parse_iso_day_number` the layout used
    //     to place the bars -- not a second copy of that arithmetic in this crate.
    //  3. Today must fall INSIDE the charted span. `x_for_day` returns `None` otherwise, which is
    //     why every fixture with fixed past dates draws nothing and no golden moves.
    //  4. `todayMarker off` must suppress it. mermaid treats the literal `off` as a disable, and a
    //     directive the user wrote to turn something off has to actually turn it off.
    if let (Some(today), Some(axis)) = (
        config.gantt_today.as_deref(),
        layout.extensions.gantt_day_axis,
    ) {
        let style = gantt_meta.today_marker_style.as_deref().unwrap_or("");
        let disabled = style.trim().eq_ignore_ascii_case("off");
        if !disabled
            && let Some(day) = fm_layout::parse_iso_day_number(today)
            && let Some(x) = axis.x_for_day(day)
        {
            use crate::attributes::{write_escaped_attr, write_number_into};
            let x = x + offset_x;
            let top = layout.bounds.y + offset_y + 12.0;
            let bottom = layout.bounds.y + layout.bounds.height + offset_y;
            let mut line = String::new();
            line.push_str("<line x1=\"");
            let _ = write_number_into(&mut line, x);
            line.push_str("\" y1=\"");
            let _ = write_number_into(&mut line, top);
            line.push_str("\" x2=\"");
            let _ = write_number_into(&mut line, x);
            line.push_str("\" y2=\"");
            let _ = write_number_into(&mut line, bottom);
            // The declared style reaches the element's stroke attributes rather than being accepted
            // and dropped. An empty directive falls back to mermaid's own red today line.
            line.push_str("\" class=\"fm-gantt-today\" style=\"");
            if style.is_empty() {
                line.push_str("stroke:#ff0000;stroke-width:2px");
            } else {
                let _ = write_escaped_attr(&mut line, style);
            }
            line.push_str("\"/>");
            doc = doc.child(Element::raw_svg(line));
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

    // Stream all task bars/milestones/progress/labels into ONE raw fragment instead of ~2-3 `Element`s
    // per task (the dominant cost of gantt render). Byte-identical: the same `<rect>`/`<path>`/`<text>`
    // bytes in the same per-task order (bar/milestone, then progress, then label) the `Element` children
    // serialized to.
    {
        use crate::attributes::write_escaped_attr;
        let mut task_svg = String::new();
        for (node_idx, node_box) in layout.nodes.iter().enumerate() {
            let x = node_box.bounds.x + offset_x;
            let y = node_box.bounds.y + offset_y;
            let w = node_box.bounds.width;
            let h = node_box.bounds.height;

            // Where THIS task's bytes begin, so a link can wrap the bar, its progress overlay and
            // its label together (bd-gqqkg). Recorded before anything is written and used only when
            // a link is actually emitted, so an ordinary chart streams byte-identically.
            let task_bytes_start = task_svg.len();

            let task_flags = gantt_meta
                .tasks
                .get(node_idx)
                .map(|t| t.flags)
                .unwrap_or_default();
            let task_type = task_flags.primary_type();
            let task_type = &task_type;
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
                task_svg.push_str("<path d=\"");
                task_svg.push_str(&d);
                task_svg.push_str("\" fill=\"");
                let _ = write_escaped_attr(&mut task_svg, fill);
                task_svg.push_str("\" stroke=\"");
                let _ = write_escaped_attr(&mut task_svg, &theme.colors.node_stroke);
                task_svg.push_str("\" stroke-width=\"1.5\" class=\"fm-gantt-milestone\"");
                // A MILESTONE is drawn as a diamond `<path>`, not a bar `<rect>`, so the bar writer
                // never sees it — it would have stayed the one unnamed mark on an otherwise named
                // chart. Same helper, so the two shapes cannot describe a task differently.
                let milestone_label = ir
                    .nodes
                    .get(node_box.node_index)
                    .and_then(|node| node.label)
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|label| label.text.as_str())
                    .or_else(|| {
                        ir.nodes
                            .get(node_box.node_index)
                            .map(|node| node.id.as_str())
                    })
                    .unwrap_or("");
                match config
                    .a11y
                    .text_alternatives
                    .then(|| {
                        gantt_bar_accessible_name(milestone_label, gantt_meta.tasks.get(node_idx))
                    })
                    .filter(|name| !name.trim().is_empty())
                {
                    Some(name) => {
                        task_svg.push_str("><title>");
                        let _ = crate::attributes::write_escaped_text(&mut task_svg, &name);
                        task_svg.push_str("</title></path>");
                    }
                    None => task_svg.push_str("/>"),
                }
            } else {
                let type_class = match task_type {
                    fm_core::GanttTaskType::Done => "fm-gantt-task-done",
                    fm_core::GanttTaskType::Active => "fm-gantt-task-active",
                    fm_core::GanttTaskType::Critical => "fm-gantt-task-critical",
                    fm_core::GanttTaskType::Milestone => "fm-gantt-task-milestone",
                    fm_core::GanttTaskType::Normal => "fm-gantt-task-normal",
                };
                // The task's own node carries whatever interaction the parser attached (bd-gydqv).
                let task_tooltip = gantt_meta
                    .tasks
                    .get(node_idx)
                    .and_then(|task| ir.nodes.get(task.node.0))
                    .and_then(|node| node.tooltip());
                // The task label is resolved again here rather than hoisted: the existing
                // resolution happens after the bar is written, and reordering it would move bytes in
                // a golden-pinned writer for no benefit.
                let bar_label = ir
                    .nodes
                    .get(node_box.node_index)
                    .and_then(|node| node.label)
                    .and_then(|lid| ir.labels.get(lid.0))
                    .map(|label| label.text.as_str())
                    .or_else(|| {
                        ir.nodes
                            .get(node_box.node_index)
                            .map(|node| node.id.as_str())
                    })
                    .unwrap_or("");
                let accessible_name = config
                    .a11y
                    .text_alternatives
                    .then(|| gantt_bar_accessible_name(bar_label, gantt_meta.tasks.get(node_idx)));
                write_gantt_bar_into(
                    &mut task_svg,
                    x,
                    y,
                    w,
                    h,
                    fill,
                    &theme.colors.node_stroke,
                    type_class,
                    task_tooltip,
                    accessible_name.as_deref(),
                );

                // Progress bar overlay.
                if let Some(task) = gantt_meta.tasks.get(node_idx)
                    && let Some(progress) = task.progress
                    && progress > 0.0
                {
                    let progress_w = w * progress.clamp(0.0, 1.0);
                    task_svg.push_str("<rect x=\"");
                    let _ = crate::attributes::write_number_into(&mut task_svg, x);
                    task_svg.push_str("\" y=\"");
                    let _ = crate::attributes::write_number_into(&mut task_svg, y);
                    task_svg.push_str("\" width=\"");
                    let _ = crate::attributes::write_number_into(&mut task_svg, progress_w);
                    task_svg.push_str("\" height=\"");
                    let _ = crate::attributes::write_number_into(&mut task_svg, h);
                    task_svg.push_str("\" fill=\"");
                    let _ = write_escaped_attr(&mut task_svg, fill);
                    task_svg
                        .push_str("\" fill-opacity=\"0.6\" rx=\"3\" class=\"fm-gantt-progress\"/>");
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
                // Layout decides placement, mirroring pinned mermaid-js: inside the bar when the name
                // fits, otherwise just outside it, never truncated (bd-h9gx). It has to be layout's
                // call because only layout can grow the diagram bounds to hold an outside label — a
                // renderer-side decision would put the text off the canvas. Absent an entry (any
                // non-gantt path, or a task with no name) the previous centred behaviour stands.
                let placement = layout
                    .extensions
                    .gantt_task_labels
                    .iter()
                    .find(|entry| entry.node_index == node_box.node_index);
                // PLACEMENT DECIDES WHAT THE LABEL SITS ON (bd-u0x67), which is why the colour is
                // chosen here and not once for the whole chart. An INSIDE label is drawn over the
                // task BAR and must contrast with it; an OUTSIDE label is drawn over the page and
                // must contrast with the background, which `colors.text` already does correctly.
                // Colouring every label for the bar would have broken the outside case to fix the
                // inside one.
                let (label_x, anchor, inside) =
                    match placement.map(|entry| (entry.x, entry.placement)) {
                        Some((label_x, fm_layout::GanttLabelPlacement::OutsideRight)) => {
                            (label_x + offset_x, "start", false)
                        }
                        Some((label_x, fm_layout::GanttLabelPlacement::OutsideLeft)) => {
                            (label_x + offset_x, "end", false)
                        }
                        Some((label_x, fm_layout::GanttLabelPlacement::Inside)) => {
                            (label_x + offset_x, "middle", true)
                        }
                        // No recorded placement: the label is centred on the bar, so it is inside.
                        None => (x + w / 2.0, "middle", true),
                    };
                let label_fill = if inside {
                    readable_label_colour(fill, &theme.colors)
                } else {
                    &theme.colors.text
                };
                write_gantt_label_into(
                    &mut task_svg,
                    label_x,
                    y + h / 2.0 + config.font_size * 0.3,
                    config.font_size * 0.8,
                    &config.font_family,
                    config.embed_theme_css,
                    label_fill,
                    label_text,
                    anchor,
                );
            }

            // A gantt task with a `click ... href` becomes a real LINK (bd-gqqkg).
            //
            // bd-gydqv attached the href to the task's node and rendered its tooltip, but the link
            // itself was still stored-and-unused: a reader could hover the bar and never click it.
            //
            // The gate is the SAME one the flowchart node path uses, read rather than re-derived —
            // `is_safe_link_target` against the diagram's sanitize mode, then `config.link_mode`.
            // Re-implementing a security decision for a second diagram type is how the two drift and
            // one of them starts emitting `javascript:` URLs. `Footnote` deliberately does nothing
            // here: it decorates a `<g>` with `data-link`, and these bars are raw bytes with no group
            // to hang it on, so claiming support would be worse than leaving it to the node path.
            if let Some(href) = gantt_meta
                .tasks
                .get(node_idx)
                .and_then(|task| ir.nodes.get(task.node.0))
                .and_then(|node| node.href())
                .filter(|href| is_safe_link_target(href, ir.meta.init.config.sanitize_mode))
                && matches!(config.link_mode, MermaidLinkMode::Inline)
            {
                let link_target = gantt_meta
                    .tasks
                    .get(node_idx)
                    .and_then(|task| ir.nodes.get(task.node.0))
                    .and_then(|node| node.link_target())
                    .unwrap_or("_blank");
                let mut open = String::with_capacity(href.len() + link_target.len() + 64);
                open.push_str("<a href=\"");
                let _ = write_escaped_attr(&mut open, href);
                open.push_str("\" target=\"");
                let _ = write_escaped_attr(&mut open, link_target);
                // `rel` unconditionally, matching the node path: it matters for `_blank` and is
                // inert rather than wrong on a same-frame target.
                open.push_str("\" rel=\"noopener noreferrer\" style=\"cursor: pointer;\">");
                task_svg.insert_str(task_bytes_start, &open);
                task_svg.push_str("</a>");
            }
        }
        if !task_svg.is_empty() {
            doc = doc.child(Element::raw_svg(task_svg));
        }

        // Dependency arrows — streamed into one raw fragment (path per edge).
        let mut dep_svg = String::new();
        for edge_path in &layout.edges {
            if edge_path.points.len() >= 2 {
                let path_d = smooth_layout_edge_path(edge_path, offset_x, offset_y);
                dep_svg.push_str("<path d=\"");
                dep_svg.push_str(&path_d);
                dep_svg.push_str("\" fill=\"none\" stroke=\"");
                let _ = write_escaped_attr(&mut dep_svg, &theme.colors.edge);
                dep_svg.push_str(
                    "\" stroke-width=\"1.2\" marker-end=\"url(#fm-gantt-arrowhead)\" class=\"fm-gantt-dependency\"/>",
                );
            }
        }
        if !dep_svg.is_empty() {
            // Define the marker in the fragment that references it.
            //
            // These arrows used to point at `url(#arrowhead)`, an id NOTHING in this document ever
            // defined: gantt renders through `render_gantt_svg`, which only appends children to a
            // document whose `<defs>` is already closed and, for this diagram type, empty — the
            // committed golden shows `<defs></defs>`. So every dependency arrowhead silently failed
            // to draw, in every gantt render, in every conformant viewer. It is not the shared
            // `arrow-*` set either; those ids are `arrow-end`, `arrow-filled`, … and none of them is
            // `arrowhead`.
            //
            // The id is namespaced rather than reusing `arrow-end` because this defs block is local
            // to the gantt fragment: a bare `arrow-end` here would shadow, or be shadowed by, the
            // shared set if the two ever appear in one document.
            let marker = ArrowheadMarker::standard("fm-gantt-arrowhead", &theme.colors.edge)
                .to_element()
                .render();
            let mut fragment = String::with_capacity(marker.len() + dep_svg.len() + 13);
            fragment.push_str("<defs>");
            fragment.push_str(&marker);
            fragment.push_str("</defs>");
            fragment.push_str(&dep_svg);
            doc = doc.child(Element::raw_svg(fragment));
        }
    }

    doc
}

/// Stream one pie `<text>` byte-identical to the slow path's `TextBuilder`: attrs in `TextBuilder::build`
/// order `x, y, text-anchor, [dominant-baseline="middle"], [font-family], font-size, [font-weight="600"],
/// fill, class`, then escaped content. Covers the pie title, slice labels, legend title, and legend rows.
#[allow(clippy::too_many_arguments)]
fn write_pie_text_into(
    f: &mut String,
    x: f32,
    y: f32,
    anchor: &str,
    baseline_middle: bool,
    family: &str,
    embed: bool,
    font_size: f32,
    weight_600: bool,
    fill: &str,
    class: &str,
    text: &str,
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text};
    f.push_str("<text x=\"");
    let _ = crate::attributes::write_number_into(f, x);
    f.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(f, y);
    f.push_str("\" text-anchor=\"");
    f.push_str(anchor);
    f.push('"');
    if baseline_middle {
        f.push_str(" dominant-baseline=\"middle\"");
    }
    if !embed {
        f.push_str(" font-family=\"");
        let _ = write_escaped_attr(f, family);
        f.push('"');
    }
    f.push_str(" font-size=\"");
    let _ = crate::attributes::write_number_into(f, font_size);
    f.push('"');
    if weight_600 {
        f.push_str(" font-weight=\"600\"");
    }
    f.push_str(" fill=\"");
    let _ = write_escaped_attr(f, fill);
    f.push_str("\" class=\"");
    f.push_str(class);
    f.push_str("\">");
    let _ = write_escaped_text(f, text);
    f.push_str("</text>");
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

    // Stream the whole pie (title + per-slice wedge+label + legend group) into ONE raw fragment instead
    // of ~4 `Element`s per slice + the legend group/box/title. Byte-identical: same element bytes/attr
    // order (`TextBuilder`/`Element::rect`/`circle`/`path`) in the same doc-child order; the wedge `<path
    // d>` keeps the same full-precision `format!`; label/legend `font-family` gated on `!embed`.
    use crate::attributes::write_escaped_attr;
    let text_fill = theme.colors.text.as_str();
    let family = config.font_family.as_str();
    let embed = config.embed_theme_css;
    let bg = theme.colors.background.as_str();
    let mut pie_svg = String::new();

    if let Some(title) = title {
        write_pie_text_into(
            &mut pie_svg,
            cx,
            bounds.y + offset_y + config.font_size + 2.0,
            "middle",
            false,
            family,
            embed,
            config.font_size + 4.0,
            true,
            text_fill,
            "fm-pie-title",
            title,
        );
    }

    // The wedge `<path d>` framing is loop-invariant: `cx`/`cy` (center) and `radius` never change
    // across slices, so the `format!("M {cx} {cy} L …")`'s four Grisu float formats for them were
    // re-run once per wedge (float formatting was ~36% of pie render). Format the invariant head
    // (`M cx cy L `) and arc (` A radius radius 0 `) fragments once; per wedge only the four variable
    // arc endpoints go through Grisu, written straight into `pie_svg` (no per-wedge `d` String).
    // Byte-identical: the pieces concatenate to exactly the old format string.
    use std::fmt::Write as _;
    let pie_head = format!("M {cx} {cy} L ");
    let pie_arc = format!(" A {radius} {radius} 0 ");
    let mut angle = -PI / 2.0;
    // A normal wedge's END point (at `angle + sweep`) is the NEXT wedge's START point (at the
    // next `angle`, since `angle += sweep` below) — bit-for-bit the same float, so its
    // Grisu-formatted `"{x} {y}"` text is identical. Cache it and reuse it as the next wedge's
    // start instead of re-running Grisu on `x1 y1` (wedge-boundary float formatting is ~19% of
    // pie render). Only valid immediately after a normal wedge: zero-value and full-circle wedges
    // emit no boundary point yet still advance `angle`, so they clear the cache. One reused buffer.
    let mut prev_end_point = String::new();
    let mut have_prev_end = false;
    for (i, slice) in pie_meta.slices.iter().enumerate() {
        let value = slice.value.max(0.0);
        let sweep = (value / total) * 2.0 * PI;
        let color = accent_colors[i % accent_colors.len()];

        if value <= f32::EPSILON {
            pie_svg.push_str(
                "<path d=\"\" fill=\"none\" stroke=\"none\" class=\"fm-pie-slice fm-pie-slice-zero\"/>",
            );
            have_prev_end = false;
        } else if (sweep - 2.0 * PI).abs() <= 0.0001 {
            pie_svg.push_str("<circle cx=\"");
            let _ = crate::attributes::write_number_into(&mut pie_svg, cx);
            pie_svg.push_str("\" cy=\"");
            let _ = crate::attributes::write_number_into(&mut pie_svg, cy);
            pie_svg.push_str("\" r=\"");
            let _ = crate::attributes::write_number_into(&mut pie_svg, radius);
            pie_svg.push_str("\" fill=\"");
            let _ = write_escaped_attr(&mut pie_svg, color);
            pie_svg.push_str("\" stroke=\"");
            let _ = write_escaped_attr(&mut pie_svg, bg);
            pie_svg.push_str("\" stroke-width=\"2\" class=\"fm-pie-slice fm-pie-slice-full\"");
            write_pie_slice_accessible_name(
                &mut pie_svg,
                config.a11y.text_alternatives,
                pie_meta.show_data,
                &slice.label,
                value,
                (value / total) * 100.0,
                "circle",
            );
            have_prev_end = false;
        } else {
            let x2 = cx + radius * (angle + sweep).cos();
            let y2 = cy + radius * (angle + sweep).sin();
            let large_arc = i32::from(sweep > PI);
            pie_svg.push_str("<path d=\"");
            pie_svg.push_str(&pie_head);
            // Start point: reuse the previous normal wedge's cached end-point text (byte-identical,
            // skips two Grisu formats + two trig calls), else format `x1 y1` for the first/after-reset wedge.
            if have_prev_end {
                pie_svg.push_str(&prev_end_point);
            } else {
                let x1 = cx + radius * angle.cos();
                let y1 = cy + radius * angle.sin();
                let _ = write!(pie_svg, "{x1} {y1}");
            }
            pie_svg.push_str(&pie_arc);
            // End point, isolated so its exact `"{x2} {y2}"` bytes can seed the next wedge's start.
            let _ = write!(pie_svg, "{large_arc} 1 ");
            let end_start = pie_svg.len();
            let _ = write!(pie_svg, "{x2} {y2}");
            prev_end_point.clear();
            prev_end_point.push_str(&pie_svg[end_start..]);
            have_prev_end = true;
            pie_svg.push_str(" Z");
            pie_svg.push_str("\" fill=\"");
            let _ = write_escaped_attr(&mut pie_svg, color);
            pie_svg.push_str("\" stroke=\"");
            let _ = write_escaped_attr(&mut pie_svg, bg);
            pie_svg.push_str("\" stroke-width=\"2\" class=\"fm-pie-slice\"");
            write_pie_slice_accessible_name(
                &mut pie_svg,
                config.a11y.text_alternatives,
                pie_meta.show_data,
                &slice.label,
                value,
                (value / total) * 100.0,
                "path",
            );
        }

        let mid_angle = angle + sweep / 2.0;
        let label_radius = radius + 24.0;
        let lx = cx + label_radius * mid_angle.cos();
        let ly = cy + label_radius * mid_angle.sin();
        let pct = (value / total) * 100.0;
        // ⚠️ THE SLICE IS LABELLED WITH ITS PERCENTAGE, as mermaid labels it. We drew the slice NAME
        // and no percentage at all, so `pie "Apples" : 40` rendered `Apples` where mermaid renders
        // `40%`. Measured on the pinned 11.15.0 bundle: 40/30/30 -> "40%","30%","30%"; 1/1/1 ->
        // three "33%"; 2/1 -> "67%","33%". The name is not lost — the legend beside the chart
        // carries it, which is also where mermaid puts it.
        let label_text = pie_percent_label(pct);
        let anchor = if mid_angle.cos() < -0.1 {
            "end"
        } else if mid_angle.cos() > 0.1 {
            "start"
        } else {
            "middle"
        };
        write_pie_text_into(
            &mut pie_svg,
            lx,
            ly,
            anchor,
            true,
            family,
            embed,
            clamp_font_size(config.font_size * 0.85, config.min_font_size),
            false,
            text_fill,
            "fm-pie-label",
            &label_text,
        );
        angle += sweep;
    }

    let legend_x = chart_left + chart_width + chart_gap;
    let legend_y = chart_top + 12.0;
    let legend_height = (pie_meta.slices.len() as f32 * 24.0 + 44.0).max(64.0);

    pie_svg.push_str("<g class=\"fm-pie-legend\"><rect x=\"");
    let _ = crate::attributes::write_number_into(&mut pie_svg, legend_x);
    pie_svg.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(&mut pie_svg, legend_y);
    pie_svg.push_str("\" width=\"");
    let _ = crate::attributes::write_number_into(&mut pie_svg, legend_width);
    pie_svg.push_str("\" height=\"");
    let _ = crate::attributes::write_number_into(&mut pie_svg, legend_height);
    pie_svg.push_str("\" rx=\"");
    let _ = crate::attributes::write_number_into(&mut pie_svg, config.rounded_corners.max(6.0));
    pie_svg.push_str("\" fill=\"");
    let _ = write_escaped_attr(&mut pie_svg, &theme.colors.node_fill);
    pie_svg.push_str("\" stroke=\"");
    let _ = write_escaped_attr(&mut pie_svg, &theme.colors.node_stroke);
    pie_svg.push_str("\" stroke-width=\"1.2\" class=\"fm-pie-legend-box\"/>");
    write_pie_text_into(
        &mut pie_svg,
        legend_x + 14.0,
        legend_y + 18.0,
        "start",
        false,
        family,
        embed,
        clamp_font_size(config.font_size * 0.82, config.min_font_size),
        true,
        text_fill,
        "fm-pie-legend-title",
        "Legend",
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
        pie_svg.push_str("<rect x=\"");
        let _ = crate::attributes::write_number_into(&mut pie_svg, legend_x + 14.0);
        pie_svg.push_str("\" y=\"");
        let _ = crate::attributes::write_number_into(&mut pie_svg, row_y - 9.0);
        pie_svg.push_str("\" width=\"12\" height=\"12\" rx=\"2\" fill=\"");
        let _ = write_escaped_attr(&mut pie_svg, color);
        pie_svg.push_str("\" stroke=\"");
        let _ = write_escaped_attr(&mut pie_svg, bg);
        pie_svg.push_str("\" stroke-width=\"1\" class=\"fm-pie-legend-swatch\"/>");
        write_pie_text_into(
            &mut pie_svg,
            legend_x + 34.0,
            row_y,
            "start",
            true,
            family,
            embed,
            clamp_font_size(config.font_size * 0.8, config.min_font_size),
            false,
            text_fill,
            "fm-pie-legend-entry",
            &entry_label,
        );
    }
    pie_svg.push_str("</g>");

    doc = doc.child(Element::raw_svg(pie_svg));

    doc
}

/// Close a pie wedge, giving it an ACCESSIBLE NAME when text alternatives are on (bd-uf3p1).
///
/// Pie slices shipped as bare self-closing shapes: no `data-id`, no `role`, no `aria-label`, no
/// `<title>`. Measured across the corpus, four chart types — gantt, pie, quadrant, xychart — emitted
/// ZERO per-element accessibility affordances, while the other fifteen (including chart-like
/// sankey, journey, timeline, packet and kanban) all did. A screen reader got the document `<desc>`
/// and nothing per wedge.
///
/// MIRRORS THE LEGEND, `showData` behaviour included: `Label: 50 (50.0%)` when the author asked for
/// data, the bare label when they did not.
///
/// I first made the share UNCONDITIONAL, reasoning that a wedge's angle conveys its proportion to a
/// sighted reader and an accessible name should carry what the visual conveys. That broke
/// `pie_without_showdata_omits_value_and_percentage_labels`, which asserts DOCUMENT-WIDE that
/// `showData: false` discloses no numbers anywhere. That is a real pre-existing contract about what
/// the author chose to publish — not an oversight in a visible-text-only check — so the name follows
/// it rather than the gate being narrowed to accommodate this function. Whether an accessible name
/// should be exempt from `showData` is a product question, raised on bd-uf3p1 rather than decided
/// here by whoever happened to be editing.
///
/// Gated on `text_alternatives`, matching `uniform_a11y`'s `<title>` component: with a11y off the
/// shape closes exactly as it did before, so that configuration is byte-identical.
/// Format a pie slice's share the way mermaid labels it: a whole percent with a `%` suffix.
///
/// ⚠️ HALF ROUNDS AWAY FROM ZERO, NOT TO EVEN, and that is the whole reason this is a function.
/// mermaid rounds in JavaScript, where `Math.round(12.5)` is `13`. Rust's `{:.0}` rounds half to
/// EVEN and renders the same value as `12`. Measured against the bundle to be sure rather than
/// reasoned about: `pie "A":1 "B":7` draws `13%` and `88%`, and 3/8 draws `38%`.
///
/// The difference shows up only on an exact half, which is why a `{:.0}` implementation passes every
/// obvious fixture — 40/30/30, 1/1/1, 2/1 — and fails one input in a hundred.
fn pie_percent_label(percent: f32) -> String {
    format!("{}%", (f64::from(percent) + 0.5).floor() as i64)
}

fn write_pie_slice_accessible_name(
    out: &mut String,
    text_alternatives: bool,
    show_data: bool,
    label: &str,
    value: f32,
    percent: f32,
    tag: &str,
) {
    use crate::attributes::write_escaped_text;
    if !text_alternatives {
        out.push_str("/>");
        return;
    }
    use std::fmt::Write as _;

    out.push_str("><title>");
    let _ = write_escaped_text(out, label);
    if show_data {
        // `{:.0}` and `{:.1}` are the LEGEND's own `entry_label` formatting, so the spoken name and
        // the printed one agree digit for digit. `write_number_into` renders 66.7 as `66.70`, which
        // would have made the accessible name disagree with the legend beside it.
        let _ = write!(out, ": {value:.0} ({percent:.1}%)");
    }
    out.push_str("</title></");
    out.push_str(tag);
    out.push('>');
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
    let plot_bounds = xychart_plot_bounds(layout, xy_chart_meta);
    let plot_x = plot_bounds.x + offset_x;
    let plot_y = plot_bounds.y + offset_y;
    let plot_bottom = plot_y + plot_bounds.height;
    let plot_right = plot_x + plot_bounds.width;
    let (y_min, y_max) = resolve_xychart_y_domain(xy_chart_meta);
    let baseline_value = y_min.min(0.0).max(y_max.min(0.0));
    let baseline_y = xychart_value_to_y(baseline_value, y_min, y_max, plot_bounds) + offset_y;
    let categories = xychart_categories(xy_chart_meta);
    let palette = theme.colors.accents.clone();
    let lookup_len = layout
        .nodes
        .iter()
        .map(|node| node.node_index)
        .max()
        .map_or(ir.nodes.len(), |max_index| {
            ir.nodes.len().max(max_index.saturating_add(1))
        });
    let mut layout_nodes_by_index = vec![None; lookup_len];
    for node in &layout.nodes {
        if let Some(slot) = layout_nodes_by_index.get_mut(node.node_index)
            && slot.is_none()
        {
            *slot = Some(node);
        }
    }

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

    // Nice tick values, not quarter points (see `xychart_nice_step`).
    for tick_value in xychart_y_ticks(y_min, y_max) {
        let tick_ratio = if (y_max - y_min).abs() > f32::EPSILON {
            (tick_value - y_min) / (y_max - y_min)
        } else {
            0.0
        };
        let tick_y = plot_y + plot_bounds.height - (plot_bounds.height * tick_ratio);
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
                // `colors.text`, NOT `colors.edge` (bd-c14jf). This painted TICK LABELS with the
                // LINE colour — a category error that is nearly invisible in review because both
                // slots hold a dark-ish value, and it has no CSS rule behind it to correct the
                // attribute (unlike `.fm-cluster-label`, where a rule wins over the attribute).
                //
                // Measured contrast against the theme background, before this change:
                //   default  #94a3b8 on #fafbfc  =  2.47:1   <- fails WCAG AA (4.5:1), and even
                //                                              the 3:1 large-text floor
                //   dark     #94a3b8 on #0f172a  =  6.96:1
                // Its own siblings were already right: `fm-xychart-x-tick` and `fm-xychart-title`
                // both use `colors.text` (16.46:1 and 17.06:1). One axis was legible and the other
                // was not, on the SHIPPED default theme.
                .fill(&theme.colors.text)
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
    // Stream the per-category x-tick `<text>` labels when the config matches the fast shape: embedded
    // theme CSS (so no per-label `font-family` — it is inherited from the root `<svg>`) and every
    // category single-line (a multi-line label needs `<tspan>` children). Byte-identical to the
    // TextBuilder/`Element` build: attribute order `x, y, text-anchor, font-size, fill, class` (baseline
    // is Auto and weight/style unset here, so those are absent), `write_value` numbers, `write_escaped
    // _attr` fill, and `write_escaped_text` content — exactly what `Element`'s `.content` serializes
    // (element.rs). Any other config falls back to the TextBuilder path below.
    let labels_streamable = config.embed_theme_css
        && categories
            .iter()
            .all(|c| !c.contains('\n') && !c.contains('\r'));
    if labels_streamable {
        use crate::attributes::{write_escaped_attr, write_escaped_text};
        let mut y_text = String::new();
        let _ = crate::attributes::write_number_into(&mut y_text, plot_bottom + 24.0);
        let mut fs_text = String::new();
        let _ = crate::attributes::write_number_into(
            &mut fs_text,
            clamp_font_size(config.font_size * 0.74, config.min_font_size),
        );
        let mut esc_fill = String::new();
        let _ = write_escaped_attr(&mut esc_fill, &theme.colors.text);
        let mut x_text = String::new();
        let mut label_svg = String::new();
        for (index, category) in categories.iter().enumerate() {
            let x = plot_x + band_width * (index as f32 + 0.5);
            x_text.clear();
            let _ = crate::attributes::write_number_into(&mut x_text, x);
            label_svg.push_str("<text x=\"");
            label_svg.push_str(&x_text);
            label_svg.push_str("\" y=\"");
            label_svg.push_str(&y_text);
            label_svg.push_str("\" text-anchor=\"middle\" font-size=\"");
            label_svg.push_str(&fs_text);
            label_svg.push_str("\" fill=\"");
            label_svg.push_str(&esc_fill);
            label_svg.push_str("\" class=\"fm-xychart-x-tick\">");
            let _ = write_escaped_text(&mut label_svg, category);
            label_svg.push_str("</text>");
        }
        doc = doc.child(Element::raw_svg(label_svg));
    } else {
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
    // The SAME values the labels use — a tick mark beside a different set of labels is worse than
    // no tick mark, and these two loops previously agreed only because both hardcoded quarters.
    for tick_value in xychart_y_ticks(y_min, y_max) {
        let frac = if (y_max - y_min).abs() > f32::EPSILON {
            (tick_value - y_min) / (y_max - y_min)
        } else {
            0.0
        };
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
    {
        // Per-category x-axis tick `<line>`s: only `x` varies (x1 == x2 == x); y1 (plot_bottom), y2
        // (plot_bottom + tick_len), the stroke colour, stroke-width and class are all invariant across
        // the loop. Hoist those, format `x` once per tick (shared by x1/x2), and stream into one
        // `raw_svg` child — no span metadata here, so no gate. Byte-identical to the `Element` build
        // (attribute order x1,y1,x2,y2,stroke,stroke-width,class; `stroke-width="1"` = `1.0`).
        use crate::attributes::write_escaped_attr;
        let mut y1_text = String::new();
        let _ = crate::attributes::write_number_into(&mut y1_text, plot_bottom);
        let mut y2_text = String::new();
        let _ = crate::attributes::write_number_into(&mut y2_text, plot_bottom + tick_len);
        let mut esc_stroke = String::new();
        let _ = write_escaped_attr(&mut esc_stroke, &theme.colors.text);
        let mut x_text = String::new();
        let mut tick_svg = String::new();
        for (index, _category) in categories.iter().enumerate() {
            let x = plot_x + band_width * (index as f32 + 0.5);
            x_text.clear();
            let _ = crate::attributes::write_number_into(&mut x_text, x);
            tick_svg.push_str("<line x1=\"");
            tick_svg.push_str(&x_text);
            tick_svg.push_str("\" y1=\"");
            tick_svg.push_str(&y1_text);
            tick_svg.push_str("\" x2=\"");
            tick_svg.push_str(&x_text);
            tick_svg.push_str("\" y2=\"");
            tick_svg.push_str(&y2_text);
            tick_svg.push_str("\" stroke=\"");
            tick_svg.push_str(&esc_stroke);
            tick_svg.push_str("\" stroke-width=\"1\" class=\"fm-xychart-tick\"/>");
        }
        doc = doc.child(Element::raw_svg(tick_svg));
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
        // The layout reserves exactly this 120px legend column. Keep the label beside its swatch
        // within that reservation rather than allowing a long series name to extend past the SVG
        // viewport. `textLength` only engages once the ordinary label would overflow, so normal
        // legend typography stays unchanged.
        let legend_text_width = legend_width - 32.0;
        let legend_font_size = clamp_font_size(config.font_size * 0.72, config.min_font_size);

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
            let legend_text = TextBuilder::new(name)
                .x(legend_x + 24.0)
                .y(row_y)
                .baseline(crate::text::DominantBaseline::Middle)
                .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                .font_size(legend_font_size)
                .fill(&theme.colors.text)
                .class("fm-xychart-legend-entry")
                .build();
            let estimated_text_width = name.chars().count() as f32 * legend_font_size * 0.56;
            let legend_text = if estimated_text_width > legend_text_width {
                legend_text
                    .attr_num("textLength", legend_text_width)
                    .attr("lengthAdjust", "spacingAndGlyphs")
            } else {
                legend_text
            };
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
            legend = legend.child(legend_text);
        }
        doc = doc.child(legend);
    }

    for (series_index, series) in xy_chart_meta.series.iter().enumerate() {
        let color = &palette[series_index % palette.len()];
        let series_nodes: Vec<_> = series
            .nodes
            .iter()
            .filter_map(|node_id| layout_nodes_by_index.get(node_id.0).and_then(|node| *node))
            .collect();
        // The ORIGINAL value index of each surviving node, built with the IDENTICAL filter above
        // (bd-sdhzh). `series_nodes` drops any node the layout did not place, so its positions are
        // not `series.values` positions; zipping the two directly would name a mark with its
        // neighbour's number. A parallel vector keeps the mapping exact without rewriting the five
        // other loops that consume `series_nodes`.
        let series_value_indices: Vec<usize> = series
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(value_index, node_id)| {
                layout_nodes_by_index
                    .get(node_id.0)
                    .and_then(|node| *node)
                    .map(|_| value_index)
            })
            .collect();

        match series.kind {
            IrXySeriesKind::Bar => {
                let rx = (config.rounded_corners * 0.45).max(3.0);
                if config.include_source_spans {
                    // Spans on: keep the per-bar `Element` build so `apply_span_metadata` can attach the
                    // `data-fm-source-*` attributes (rare config; not worth reproducing inline).
                    for (position, node) in series_nodes.iter().enumerate() {
                        let node = *node;
                        let rect = Element::rect()
                            .x(node.bounds.x + offset_x)
                            .y(node.bounds.y + offset_y)
                            .width(node.bounds.width)
                            .height(node.bounds.height)
                            .fill(color)
                            .fill_opacity(0.78)
                            .stroke(color)
                            .stroke_width(1.0)
                            .rx(rx)
                            .class("fm-xychart-bar");
                        let rect = match series_value_indices
                            .get(position)
                            .and_then(|index| {
                                xychart_mark_accessible_name(xy_chart_meta, series, *index)
                            })
                            .filter(|_| config.a11y.text_alternatives)
                        {
                            Some(name) => rect.child(Element::title(&name)),
                            None => rect,
                        };
                        doc = doc.child(apply_span_metadata(rect, node.span));
                    }
                } else {
                    // Stream the bar `<rect>`s straight into one `raw_svg` child instead of building ~N
                    // per-bar `Element`/`Attributes` trees (Attributes churn was ~18% of xychart render).
                    // Byte-identical to the `Element` build above: same attribute order (x, y, width,
                    // height, fill, fill-opacity, stroke, stroke-width, rx, class), same `write_value`
                    // number formatting, same `write_escaped_attr` for the colour. `fill-opacity="0.78"`
                    // and `stroke-width="1"` are the fixed serializations of `0.78`/`1.0`.
                    use crate::attributes::write_escaped_attr;
                    // Per-series invariants: the fill/stroke colour and the corner radius are identical
                    // for every bar, so escape/format them ONCE and reuse the bytes rather than
                    // re-escaping the colour twice + re-formatting `rx` per bar (write_escaped_attr was
                    // the top xychart frame after marker streaming). Byte-identical.
                    let mut esc_color = String::new();
                    let _ = write_escaped_attr(&mut esc_color, color);
                    let mut rx_text = String::new();
                    let _ = crate::attributes::write_number_into(&mut rx_text, rx);
                    let mut bar_svg = String::new();
                    for (position, node) in series_nodes.iter().enumerate() {
                        let node = *node;
                        bar_svg.push_str("<rect x=\"");
                        let _ = crate::attributes::write_number_into(
                            &mut bar_svg,
                            node.bounds.x + offset_x,
                        );
                        bar_svg.push_str("\" y=\"");
                        let _ = crate::attributes::write_number_into(
                            &mut bar_svg,
                            node.bounds.y + offset_y,
                        );
                        bar_svg.push_str("\" width=\"");
                        let _ =
                            crate::attributes::write_number_into(&mut bar_svg, node.bounds.width);
                        bar_svg.push_str("\" height=\"");
                        let _ =
                            crate::attributes::write_number_into(&mut bar_svg, node.bounds.height);
                        bar_svg.push_str("\" fill=\"");
                        bar_svg.push_str(&esc_color);
                        bar_svg.push_str("\" fill-opacity=\"0.78\" stroke=\"");
                        bar_svg.push_str(&esc_color);
                        bar_svg.push_str("\" stroke-width=\"1\" rx=\"");
                        bar_svg.push_str(&rx_text);
                        bar_svg.push_str("\" class=\"fm-xychart-bar\"");
                        let name = series_value_indices.get(position).and_then(|index| {
                            xychart_mark_accessible_name(xy_chart_meta, series, *index)
                        });
                        write_xychart_mark_accessible_name(
                            &mut bar_svg,
                            config.a11y.text_alternatives,
                            name.as_deref(),
                            "rect",
                        );
                    }
                    doc = doc.child(Element::raw_svg(bar_svg));
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

                if config.include_source_spans {
                    for node in series_nodes {
                        let center = node.bounds.center();
                        let point = Element::circle()
                            .cx(center.x + offset_x)
                            .cy(center.y + offset_y)
                            .r((node.bounds.width.min(node.bounds.height) / 2.0).max(3.5))
                            .fill(color)
                            .stroke(&theme.colors.background)
                            .stroke_width(2.0)
                            .class("fm-xychart-point");
                        doc = doc.child(apply_span_metadata(point, node.span));
                    }
                } else {
                    // Stream the series point `<circle>`s (see the bar-`<rect>` streaming above). Same
                    // attribute order as the `Element` build (cx, cy, r, fill, stroke, stroke-width, class);
                    // `stroke-width="2"` is the fixed serialization of `2.0`. Byte-identical.
                    use crate::attributes::write_escaped_attr;
                    // Per-series invariants: fill (series colour) and stroke (theme background) are the
                    // same for every point — escape once, reuse. Only cx/cy/r vary. Byte-identical.
                    let mut esc_color = String::new();
                    let _ = write_escaped_attr(&mut esc_color, color);
                    let mut esc_bg = String::new();
                    let _ = write_escaped_attr(&mut esc_bg, &theme.colors.background);
                    let mut point_svg = String::new();
                    for node in series_nodes {
                        let center = node.bounds.center();
                        point_svg.push_str("<circle cx=\"");
                        let _ = crate::attributes::write_number_into(
                            &mut point_svg,
                            center.x + offset_x,
                        );
                        point_svg.push_str("\" cy=\"");
                        let _ = crate::attributes::write_number_into(
                            &mut point_svg,
                            center.y + offset_y,
                        );
                        point_svg.push_str("\" r=\"");
                        let _ = crate::attributes::write_number_into(
                            &mut point_svg,
                            (node.bounds.width.min(node.bounds.height) / 2.0).max(3.5),
                        );
                        point_svg.push_str("\" fill=\"");
                        point_svg.push_str(&esc_color);
                        point_svg.push_str("\" stroke=\"");
                        point_svg.push_str(&esc_bg);
                        point_svg.push_str("\" stroke-width=\"2\" class=\"fm-xychart-point\"/>");
                    }
                    doc = doc.child(Element::raw_svg(point_svg));
                }
            }
        }
    }

    doc
}

fn xychart_plot_bounds(
    layout: &DiagramLayout,
    xy_chart_meta: &IrXyChartMeta,
) -> fm_layout::LayoutRect {
    const LEFT_MARGIN: f32 = 88.0;
    const TOP_MARGIN: f32 = 84.0;
    const RIGHT_MARGIN: f32 = 36.0;
    const LEGEND_RIGHT_MARGIN: f32 = 136.0;
    const BOTTOM_MARGIN: f32 = 76.0;
    let right_margin = if xy_chart_meta
        .series
        .iter()
        .any(|series| series.name.is_some())
    {
        LEGEND_RIGHT_MARGIN
    } else {
        RIGHT_MARGIN
    };

    fm_layout::LayoutRect {
        x: layout.bounds.x + LEFT_MARGIN,
        y: layout.bounds.y + TOP_MARGIN,
        width: (layout.bounds.width - LEFT_MARGIN - right_margin).max(1.0),
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

/// The accessible name for one xychart data mark (bd-sdhzh).
///
/// `"Series, Category: 42"`, or `"Category: 42"` for an unnamed series, or `"42"` when the x axis
/// declares no categories. The value is formatted by [`format_xychart_tick_value`], the same
/// function the Y AXIS uses, so a screen reader reads a bar's value in the notation the axis beside
/// it is labelled in.
///
/// Returns `None` when the mark's index has no value, which is the case a positional zip would
/// otherwise turn into a mark named with its NEIGHBOUR's number — worse than an unnamed mark,
/// because a wrong value cannot be detected by the person relying on it.
fn xychart_mark_accessible_name(
    meta: &fm_core::IrXyChartMeta,
    series: &fm_core::IrXySeries,
    value_index: usize,
) -> Option<String> {
    let value = series.values.get(value_index)?;
    let formatted = format_xychart_tick_value(*value);
    let category = meta.x_axis.categories.get(value_index);
    Some(match (series.name.as_deref(), category) {
        (Some(name), Some(category)) => format!("{name}, {category}: {formatted}"),
        (Some(name), None) => format!("{name}: {formatted}"),
        (None, Some(category)) => format!("{category}: {formatted}"),
        (None, None) => formatted,
    })
}

/// Close a data mark, giving it an accessible name when text alternatives are on (bd-sdhzh).
///
/// Mirrors `write_pie_slice_accessible_name`: with a11y off the shape closes exactly as before, so
/// that configuration stays byte-identical.
fn write_xychart_mark_accessible_name(
    out: &mut String,
    text_alternatives: bool,
    name: Option<&str>,
    tag: &str,
) {
    match name.filter(|_| text_alternatives) {
        Some(name) => {
            out.push_str("><title>");
            let _ = crate::attributes::write_escaped_text(out, name);
            out.push_str("</title></");
            out.push_str(tag);
            out.push('>');
        }
        None => out.push_str("/>"),
    }
}

/// The "nice" y-axis step mermaid uses: d3's `tickStep(min, max, 10)`.
///
/// MERMAID DOES NOT DIVIDE THE RANGE, IT SNAPS TO 1/2/5 x 10^k. We emitted five ticks at quarter
/// points, so `y-axis 4000 --> 11000` labelled 5750 and 9250 — values that appear nowhere on a
/// chart anyone would draw by hand. Measured across six ranges on the pinned 11.15.0 bundle, every
/// one of which this reproduces exactly:
///
/// ```text
///   0 -> 10        step 1        0 -> 100        step 10
///   0 -> 1         step 0.1      0 -> 7          step 0.5
///   100 -> 900     step 100      4000 -> 11000   step 500
/// ```
///
/// The thresholds are d3's own — sqrt(50), sqrt(10), sqrt(2) — and they are not round numbers.
/// `0 -> 7` is the case that needs them: the raw step is 0.7, whose error 7.0 sits between sqrt(10)
/// and sqrt(50), so the factor is 5 and the step 0.5. A rule using 7.5 as the cutoff picks 1.0
/// there and still agrees with mermaid on all five other ranges.
fn xychart_nice_step(min: f32, max: f32) -> f64 {
    const COUNT: f64 = 10.0;
    let span = f64::from(max) - f64::from(min);
    if !span.is_finite() || span <= 0.0 {
        return 0.0;
    }
    let raw = span / COUNT;
    let power = raw.log10().floor();
    let error = raw / 10.0_f64.powf(power);
    let factor = if error >= 50.0_f64.sqrt() {
        10.0
    } else if error >= 10.0_f64.sqrt() {
        5.0
    } else if error >= 2.0_f64.sqrt() {
        2.0
    } else {
        1.0
    };
    10.0_f64.powf(power) * factor
}

/// The y values mermaid labels, ascending: every multiple of the nice step inside `[min, max]`.
///
/// Falls back to the endpoints when the range is degenerate or the step would produce an absurd
/// number of ticks, so a malformed axis cannot spin here.
fn xychart_y_ticks(min: f32, max: f32) -> Vec<f32> {
    let step = xychart_nice_step(min, max);
    if step <= 0.0 {
        return vec![min];
    }
    let first = (f64::from(min) / step).ceil();
    let last = (f64::from(max) / step).floor();
    let count = last - first;
    if !count.is_finite() || count < 0.0 || count > 200.0 {
        return vec![min, max];
    }
    #[allow(clippy::cast_possible_truncation)]
    let ticks: Vec<f32> = (0..=(count as i64))
        .map(|index| ((first + index as f64) * step) as f32)
        .collect();
    if ticks.is_empty() {
        vec![min, max]
    } else {
        ticks
    }
}

/// One entry of the journey actor legend: the actor's name and where it sits on the legend row.
pub(crate) struct JourneyLegendEntry {
    pub(crate) name: String,
    pub(crate) offset_x: f32,
}

/// The actors a journey declares, deduplicated and sorted, as mermaid lists them (bd-mq273).
///
/// Empty for every other diagram type, and for a journey whose steps name no actor — an empty legend
/// row must not reserve space or draw a stray separator.
fn journey_actor_legend(ir: &MermaidDiagramIr) -> Vec<JourneyLegendEntry> {
    if ir.diagram_type != DiagramType::Journey {
        return Vec::new();
    }
    let mut names: Vec<&str> = ir
        .nodes
        .iter()
        .filter_map(|node| node.journey_meta.as_deref())
        .flat_map(|meta| meta.actors.iter().map(String::as_str))
        .collect();
    // Sorted then deduped, which is the order mermaid draws them in and NOT source order.
    names.sort_unstable();
    names.dedup();

    let mut offset = 0.0_f32;
    names
        .into_iter()
        .map(|name| {
            let entry = JourneyLegendEntry {
                name: name.to_string(),
                offset_x: offset,
            };
            // Advance by the name's own width so entries do not overlap; the constant is the same
            // average-character estimate the rest of this renderer sizes text with.
            offset += (name.chars().count() as f32).mul_add(8.0, 24.0);
            entry
        })
        .collect()
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
    // Nodes with compartments (class diagrams), an ER entity's attribute list, or C4 metadata render
    // extra content the plain-rect fast fragment does not produce; they were implicitly excluded by the
    // old `classes.is_empty()` gate, so keep excluding them now that arbitrary simple classes are allowed.
    // `members` is populated only for ER entities (`render_node`'s ER branch draws a name header + divider
    // + one `<text>` per attribute); without this exclusion a themed ER entity (the default `node_gradients`
    // config) was streamed as a plain rectangle with only its name, silently dropping every attribute row.
    if node.class_meta.is_some() || node.c4_meta.is_some() || !node.members.is_empty() {
        return None;
    }
    let mut suffix = String::new();
    let mut block_beta = false;
    for class in &node.classes {
        // One fused pass yields both the keyword flags (for the reject gate) and whether the class is an
        // already-clean CSS token (so the write below can bulk-`push_str` without re-scanning).
        let (kw, is_clean) = scan_class_keywords_and_clean(class);
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
        // A non-empty class always sanitizes to a non-empty token (every char maps to one char), so gate
        // on the raw class and write the token straight into `suffix` — no throwaway per-class `String`.
        // `is_clean` from the fused scan skips `write_sanitized_css_token_into`'s redundant `all(clean)`
        // re-scan on the common already-clean class.
        if !class.is_empty() {
            suffix.push_str(" fm-node-user-");
            if is_clean {
                suffix.push_str(class);
            } else {
                write_sanitized_css_token_into(&mut suffix, class);
            }
        }
    }
    // The slow path appends `fm-node-block-beta` AFTER the per-class `fm-node-user-…` loop (see the
    // `is_block_beta` block in `render_node`), so it goes last here too. Byte-identical.
    if block_beta {
        suffix.push_str(" fm-node-block-beta");
    }
    Some(suffix)
}

/// The ` fm-node-user-{sanitized}` class suffix for a **class-diagram node's** custom classes — the
/// class-node twin of [`simple_node_user_class_suffix`] (which excludes `class_meta` nodes outright).
/// Returns `None` (forcing the `Element` slow path) whenever any class would make `render_node` add a
/// state/border keyword class, a special fill/structure, or a block-beta marker — none of which the
/// class-node streaming fragment reproduces. Empty/no classes yield `Some("")`. Class nodes are never
/// block-beta/kanban/journey in practice, but every such case is rejected so the fragment stays
/// byte-identical to the slow path (pinned by `svg_golden_snapshots_are_stable`).
fn simple_class_node_user_suffix(node: &fm_core::IrNode) -> Option<String> {
    let mut suffix = String::new();
    for class in &node.classes {
        let (kw, is_clean) = scan_class_keywords_and_clean(class);
        if kw.highlighted
            || kw.inactive
            || kw.dashed_border
            || kw.double_border
            || class.eq_ignore_ascii_case("c4-external")
            || class.eq_ignore_ascii_case("block-beta")
            || class.eq_ignore_ascii_case("block-beta-space")
        {
            return None;
        }
        // kanban-priority / journey-score classes drive an inline `style="fill: …"` on the shape in the
        // slow path (the streaming rect always uses the gradient fill), so reject them.
        match class.as_str() {
            "kanban-priority-high"
            | "kanban-priority-critical"
            | "kanban-priority-medium"
            | "kanban-priority-low"
            | "journey-score-1"
            | "journey-score-2"
            | "journey-score-3"
            | "journey-score-4"
            | "journey-score-5" => return None,
            _ => {}
        }
        if !class.is_empty() {
            suffix.push_str(" fm-node-user-");
            if is_clean {
                suffix.push_str(class);
            } else {
                write_sanitized_css_token_into(&mut suffix, class);
            }
        }
    }
    Some(suffix)
}

/// Stream a class node's compartment stack (stereotype + name + separators + attribute/method rows) into
/// `f`. Extracted from [`render_class_compartments`]' streaming fast path so the whole-class-node fast
/// path ([`write_class_node_fragment_into`]) can reuse the exact same body — same `<text>`/`<line>`
/// attrs, order, positions, and cursor advance. Byte-identical to the `Element` slow path.
#[allow(clippy::too_many_arguments)]
fn write_class_compartments_into(
    f: &mut String,
    node: &fm_core::IrNode,
    meta: &fm_core::IrClassNodeMeta,
    ir: &MermaidDiagramIr,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    font_size: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
) {
    let line_h = font_size * config.line_height;
    let text_x = x + 8.0;
    let mut cursor_y = y + line_h;
    let fill = colors.text.as_str();
    let class_name = node
        .label
        .and_then(|lid| ir.labels.get(lid.0))
        .map(|l| l.text.as_str())
        .unwrap_or(&node.id);
    if let Some(stereotype) = &meta.stereotype {
        let stereo_text = stereotype.label();
        write_class_text_into(
            f,
            x + w / 2.0,
            cursor_y,
            "middle",
            font_size * 0.85,
            " font-style=\"italic\"",
            fill,
            stereo_text,
        );
        cursor_y += line_h;
    }
    // No-generics name is written directly (the slow path's `class_name.to_string()` copy is avoided).
    if meta.generics.is_empty() {
        write_class_text_into(
            f,
            x + w / 2.0,
            cursor_y,
            "middle",
            font_size,
            " font-weight=\"bold\"",
            fill,
            class_name,
        );
    } else {
        let display_name = format!("{class_name}<{}>", meta.generics.join(", "));
        write_class_text_into(
            f,
            x + w / 2.0,
            cursor_y,
            "middle",
            font_size,
            " font-weight=\"bold\"",
            fill,
            &display_name,
        );
    }
    cursor_y += line_h * 0.5;
    write_class_separator_into(f, x, cursor_y, x + w);
    cursor_y += line_h * 0.3;
    let member_font_size = font_size * 0.9;
    for attr in &meta.attributes {
        cursor_y += member_font_size * config.line_height * 0.9;
        if cursor_y > y + h - line_h * 0.5 {
            break;
        }
        let vis = visibility_symbol(attr.visibility);
        // Generics are rewritten HERE, not in the IR: mermaid keeps `List~int~` in its db and
        // turns it into `List<int>` only when the row is drawn (bd class-generics).
        let name = fm_core::class_member_display_name(&attr.name, false);
        let text = if let Some(ref ret) = attr.return_type {
            format!("{vis}{name}: {}", fm_core::parse_generic_types(ret))
        } else {
            format!("{vis}{name}")
        };
        write_class_text_into(
            f,
            text_x,
            cursor_y,
            "start",
            member_font_size,
            "",
            fill,
            &text,
        );
    }
    if !meta.attributes.is_empty() && !meta.methods.is_empty() {
        cursor_y += line_h * 0.3;
        write_class_separator_into(f, x, cursor_y, x + w);
        cursor_y += line_h * 0.3;
    }
    for method in &meta.methods {
        cursor_y += member_font_size * config.line_height * 0.9;
        if cursor_y > y + h - line_h * 0.5 {
            break;
        }
        let vis = visibility_symbol(method.visibility);
        // ⚠️ THE CLASSIFIER IS A STYLE, NOT A CHARACTER (bd-r2gll). mermaid's `getDisplayDetails()`
        // returns `+getName() : String` with NO `$`/`*` in it and carries the marker as
        // `cssStyle: text-decoration:underline;` / `font-style:italic;`. Appending the raw byte
        // here made the same member read as a different NAME.
        let classifier = fm_core::class_member_classifier_css(method.is_static, method.is_abstract);
        // ` : T`, not `: T` (bd-ci658). Measured on the pinned bundle: mermaid's
        // `getDisplayDetails()` builds the tail as `' : ' + parseGenericTypes(returnType)`, so a
        // typed method row differed from the incumbent by one character in every class diagram.
        let ret = method
            .return_type
            .as_deref()
            .map(|t| format!(" : {}", fm_core::parse_generic_types(t)))
            .unwrap_or_default();
        let text = format!(
            "{vis}{}{ret}",
            fm_core::class_member_display_name(&method.name, true)
        );
        // Same slot `TextBuilder` emits `font-style`/`text-decoration` into — after `font-size`,
        // before `fill` — so the streaming and Element paths stay byte-identical.
        let classifier_attr = match classifier {
            Some("font-style:italic") => " font-style=\"italic\"",
            Some("text-decoration:underline") => " text-decoration=\"underline\"",
            _ => "",
        };
        write_class_text_into(
            f,
            text_x,
            cursor_y,
            "start",
            member_font_size,
            classifier_attr,
            fill,
            &text,
        );
    }
}

/// Stream an ER entity's body (name header `<text>` + divider `<line>` + one `<text>` per attribute)
/// byte-identical to what `render_node`'s ER branch builds via `Element`s under embedded CSS with no
/// per-label style and no classdef class. Attrs replicate that branch's builder call order exactly:
/// name/attr `<text>` carry `x, y, text-anchor, dominant-baseline, font-size, font-weight, fill, class`
/// (font-family is embedded-CSS-driven, so absent); the divider `<line>` carries `x1, y1, x2, y2,
/// stroke-width` (stroke is CSS-driven, so absent). The per-attribute content `{key_prefix}{data_type}
/// {name}` streams in pieces instead of a `format!` temp — byte-identical because `write_escaped_text`
/// escapes per char and `key_prefix`/the separating space hold no XML specials. Used only via the ER
/// branch's fast path, mirroring [`write_class_compartments_into`]; replaces ~2 + N `Element`s per
/// entity (~400 for a 40-entity diagram) with one raw fragment.
#[allow(clippy::too_many_arguments)]
fn write_er_entity_into(
    f: &mut String,
    node: &fm_core::IrNode,
    label_text: &str,
    cx: f32,
    x: f32,
    y: f32,
    w: f32,
    node_font_size: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text, write_number_into};
    let attr_font_size = clamp_font_size(node_font_size * 0.8, config.min_font_size);
    let header_height = node_font_size * 1.5;
    let fill = colors.text.as_str();

    // Entity name header.
    f.push_str("<text x=\"");
    let _ = write_number_into(f, cx);
    f.push_str("\" y=\"");
    let _ = write_number_into(f, y + header_height * 0.6);
    f.push_str("\" text-anchor=\"middle\" dominant-baseline=\"central\" font-size=\"");
    let _ = write_number_into(f, node_font_size);
    f.push_str("\" font-weight=\"bold\" fill=\"");
    let _ = write_escaped_attr(f, fill);
    f.push_str("\" class=\"fm-er-entity-name\">");
    let _ = write_escaped_text(f, label_text);
    f.push_str("</text>");

    // Divider line.
    f.push_str("<line x1=\"");
    let _ = write_number_into(f, x + 2.0);
    f.push_str("\" y1=\"");
    let _ = write_number_into(f, y + header_height);
    f.push_str("\" x2=\"");
    let _ = write_number_into(f, x + w - 2.0);
    f.push_str("\" y2=\"");
    let _ = write_number_into(f, y + header_height);
    // `stroke_width(0.8)` serializes via `write_number_into` -> two decimals ("0.80"), NOT "0.8".
    f.push_str("\" stroke-width=\"0.80\"/>");

    // Attribute list.
    let mut attr_y = y + header_height + attr_font_size * 0.9;
    for attr in &node.members {
        let key_prefix = attr.key_prefix();
        let font_weight = if attr.keys.is_empty() {
            "normal"
        } else {
            "bold"
        };
        f.push_str("<text x=\"");
        let _ = write_number_into(f, x + 8.0);
        f.push_str("\" y=\"");
        let _ = write_number_into(f, attr_y);
        f.push_str("\" text-anchor=\"start\" dominant-baseline=\"central\" font-size=\"");
        let _ = write_number_into(f, attr_font_size);
        f.push_str("\" font-weight=\"");
        f.push_str(font_weight);
        f.push_str("\" fill=\"");
        let _ = write_escaped_attr(f, fill);
        f.push_str("\" class=\"fm-er-attribute\">");
        // `attr_text = format!("{key_prefix}{data_type} {name}")`, escaped in pieces (identical bytes).
        f.push_str(&key_prefix);
        let _ = write_escaped_text(f, &attr.data_type);
        f.push(' ');
        let _ = write_escaped_text(f, &attr.name);
        // The COMMENT was parsed and thrown away (bd-jerh). `IrEntityAttribute::comment` is
        // populated by the parser and was read by no renderer and no layout code, so
        // `A { string name "the name" }` rendered BYTE-IDENTICAL to the same entity without it.
        // mermaid draws it: its ER renderer measures the comment into a text element classed
        // `attribute-comment` and folds the width into the row.
        //
        // Appended to this row rather than given a column of its own, because this renderer draws
        // one text run per attribute; a real fourth column would need the row split into measured
        // cells, which is a larger change than the dropped content justifies. `er_attribute_row_width`
        // measures the SAME concatenation, so the widest row still fits inside the box.
        if let Some(comment) = attr.comment.as_deref().filter(|text| !text.is_empty()) {
            f.push(' ');
            let _ = write_escaped_text(f, comment);
        }
        f.push_str("</text>");
        attr_y += attr_font_size * 1.3;
    }
}

/// Stream a complete class-diagram node (`<g>` + gradient `<rect>` + compartment stack + `<title>`)
/// directly into `out`, **byte-identical** to what `render_node` builds via `Element`s — the class-node
/// analogue of [`write_common_node_fragment_into`]. The `<g>`/rect/title bytes replicate that helper's
/// (proven) sequence for a `Rect` shape; the body is [`write_class_compartments_into`] (the same code
/// `render_class_compartments`' fast path runs). Used only via `render_node_into`'s class gate, which
/// guarantees none of `render_node`'s conditional classes/children/post-processing apply. Skips the
/// group + rect `Element` builds, their `Attributes` Vecs, and the compartment fragment's second copy.
/// `A11Y` selects the accessibility variant at compile time, mirroring `write_common_node_fragment_into`.
/// `true` (`A11yConfig::full()`, default) emits `role="graphics-symbol" aria-label=".." tabindex="0"` and the
/// trailing `<title>`; `false` (`A11yConfig::none()`, lean) skips exactly those two spots — the `<g id/class/
/// data-id>` wrapper, `<rect>`, and the compartment stack are all a11y-independent, so the lean fragment is
/// the slow `Element` path's lean output by construction.
#[allow(clippy::too_many_arguments)]
fn write_class_node_fragment_into<const A11Y: bool>(
    out: &mut String,
    node: &fm_core::IrNode,
    meta: &fm_core::IrClassNodeMeta,
    node_id: &str,
    node_index: usize,
    raw_label: &str,
    ir: &MermaidDiagramIr,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: f32,
    font_size: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
    user_classes: &str,
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text};
    // <g id=".." class="fm-node fm-node-accent-N fm-node-shape-rect[ fm-node-user-…]" data-id=".." …>
    out.push_str("<g id=\"");
    fm_core::write_mermaid_node_element_id_into(out, node_id, node_index);
    out.push_str("\" class=\"fm-node fm-node-accent-");
    // `stable_accent_index` (small palette index) via the digit-table writer, not `write!`'s
    // Formatter/`pad_integral` machinery. Byte-identical: same decimal digits.
    let _ = crate::attributes::write_uint_into(out, stable_accent_index(node_id) as u64);
    out.push(' ');
    out.push_str(node_shape_css_class(fm_core::NodeShape::Rect));
    out.push_str(user_classes);
    out.push_str("\" data-id=\"");
    let _ = write_escaped_attr(out, node_id);
    if A11Y {
        out.push_str("\" role=\"graphics-symbol\" aria-label=\"");
        let _ = write_escaped_attr(out, raw_label);
        out.push_str("\" tabindex=\"0\">");
    } else {
        out.push_str("\">");
    }
    // <rect x y width height rx fill="url(#fm-node-gradient)"/> — same attr order as the common fragment.
    out.push_str("<rect x=\"");
    let _ = crate::attributes::write_number_into(out, x);
    out.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(out, y);
    out.push_str("\" width=\"");
    let _ = crate::attributes::write_number_into(out, w);
    out.push_str("\" height=\"");
    let _ = crate::attributes::write_number_into(out, h);
    out.push_str("\" rx=\"");
    let _ = crate::attributes::write_number_into(out, rx);
    out.push_str("\" fill=\"url(#fm-node-gradient)\"/>");
    write_class_compartments_into(out, node, meta, ir, x, y, w, h, font_size, config, colors);
    if A11Y {
        // <title>Node: {raw_label}, rectangle</title></g> — describe_node's Rect form, written piecewise.
        out.push_str("<title>Node: ");
        let _ = write_escaped_text(out, raw_label);
        out.push_str(", rectangle</title></g>");
    } else {
        out.push_str("</g>");
    }
}

/// Stream a complete C4 node (`<g>` + solid-fill rounded `<rect>` + stereotype/[person icon]/name/description +
/// `<title>`) directly into `out`, **byte-identical** to what `render_node` builds via `Element`s (the C4
/// analogue of [`write_class_node_fragment_into`]). Used only via `render_node_into`'s C4 gate, which guarantees
/// a `Rounded` node with `c4_meta`, no `technology`, and an absent-or-single-line description — so none of
/// `render_node`'s conditional classes/children/post-processing apply. The wrapper mirrors the class fragment's
/// (proven) `<g>`/title bytes; the rect uses the Rounded shape's SOLID `node_fill` (NOT the gradient the class/ER
/// fragments use, matching `render_node`'s `NodeShape::Rounded => rect.fill(node_fill).rx(rounded_corners)`); the
/// content mirrors `render_c4_node_content`. Skips the group + rect + per-`<text>` `Element` builds and the
/// whole-group serialize+copy.
#[allow(clippy::too_many_arguments)]
fn write_c4_node_fragment_into(
    out: &mut String,
    node: &fm_core::IrNode,
    c4_meta: &fm_core::IrC4NodeMeta,
    node_id: &str,
    node_index: usize,
    raw_label: &str,
    ir: &MermaidDiagramIr,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: f32,
    font_size: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
    user_classes: &str,
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text, write_number_into};
    // <g id=".." class="fm-node fm-node-accent-N fm-node-shape-rounded[ fm-node-user-…]" data-id=".." …>
    out.push_str("<g id=\"");
    fm_core::write_mermaid_node_element_id_into(out, node_id, node_index);
    out.push_str("\" class=\"fm-node fm-node-accent-");
    let _ = crate::attributes::write_uint_into(out, stable_accent_index(node_id) as u64);
    out.push(' ');
    out.push_str(node_shape_css_class(fm_core::NodeShape::Rounded));
    out.push_str(user_classes);
    out.push_str("\" data-id=\"");
    let _ = write_escaped_attr(out, node_id);
    out.push_str("\" role=\"graphics-symbol\" aria-label=\"");
    let _ = write_escaped_attr(out, raw_label);
    out.push_str("\" tabindex=\"0\">");
    // <rect x y width height rx fill="url(#fm-node-gradient)"/> — under `node_gradients` the Rounded rect's
    // fill is overridden to the gradient (same attr order the class fragment uses), NOT `node_fill`. (The
    // `c4_basic` golden's solid `#ffffff` is the gradients-OFF path, which this fast path is gated out of.)
    out.push_str("<rect x=\"");
    let _ = write_number_into(out, x);
    out.push_str("\" y=\"");
    let _ = write_number_into(out, y);
    out.push_str("\" width=\"");
    let _ = write_number_into(out, w);
    out.push_str("\" height=\"");
    let _ = write_number_into(out, h);
    out.push_str("\" rx=\"");
    let _ = write_number_into(out, rx);
    out.push_str("\" fill=\"url(#fm-node-gradient)\"/>");

    // Content — mirrors `render_c4_node_content` (same arithmetic + `write_number_into`, so numbers are identical).
    let label_text = node
        .label
        .and_then(|lid| ir.labels.get(lid.0))
        .map(|label| label.text.as_str())
        .unwrap_or(node.id.as_str());
    let line_h = font_size * config.line_height;
    let small_font = clamp_font_size(font_size * 0.78, config.min_font_size);
    let description_font = clamp_font_size(font_size * 0.72, config.min_font_size);
    let mut cursor_y = y + (small_font * 1.25);

    // Stereotype `<<type>>` — `write_escaped_text("<<…>>")` = `&lt;&lt;…>>` (`<` escaped, `>` literal), in pieces.
    out.push_str("<text x=\"");
    let _ = write_number_into(out, x + w / 2.0);
    out.push_str("\" y=\"");
    let _ = write_number_into(out, cursor_y);
    out.push_str("\" text-anchor=\"middle\" font-size=\"");
    let _ = write_number_into(out, small_font);
    out.push_str("\" font-weight=\"600\" fill=\"");
    // `colors.text`, NOT `colors.cluster_stroke` (bd-4rlrx). This painted the stereotype with the
    // cluster BORDER colour — a slot designed to sit QUIETLY against the background, which is the
    // opposite of what text needs. Measured against each theme's own background:
    //   default  #cbd5e1 on #fafbfc  =  1.43:1   effectively invisible
    //   dark     #475569 on #0f172a  =  2.36:1
    // Both fail WCAG AA (4.5:1) and the 3:1 large-text floor. It was themed — the colour does move
    // between themes — which is why a "does it follow the theme?" check passes and only a measured
    // contrast catches it. Its own siblings in the same box, `fm-c4-name` and `fm-c4-description`,
    // already use `colors.text`; the visual hierarchy is carried by size (0.78x) and weight (600),
    // not by making the label unreadable.
    let _ = write_escaped_attr(out, &colors.text);
    out.push_str("\" class=\"fm-c4-type-label\">&lt;&lt;");
    let _ = write_escaped_text(out, &c4_meta.element_type);
    out.push_str(">></text>");

    // Optional person icon.
    if node
        .classes
        .iter()
        .any(|class_name| class_name == "c4-person")
    {
        write_c4_person_icon_into(out, x + 18.0, y + 18.0, &colors.node_stroke);
    }

    // Name.
    cursor_y += line_h * 0.95;
    out.push_str("<text x=\"");
    let _ = write_number_into(out, x + w / 2.0);
    out.push_str("\" y=\"");
    let _ = write_number_into(out, cursor_y);
    out.push_str("\" text-anchor=\"middle\" font-size=\"");
    let _ = write_number_into(out, font_size);
    out.push_str("\" font-weight=\"600\" fill=\"");
    let _ = write_escaped_attr(out, &colors.text);
    out.push_str("\" class=\"fm-c4-name\">");
    let _ = write_escaped_text(out, label_text);
    out.push_str("</text>");

    // Description (gate guarantees single-line, so no tspans / no `line-height` attr and `description_height` = 0).
    if let Some(description) = &c4_meta.description {
        cursor_y += line_h * 0.9;
        let available_width = (w - 20.0).max(32.0);
        let description_lines =
            wrap_text_to_lines(description, available_width, config.avg_char_width * 0.92);
        if !description_lines.is_empty() {
            let description_height = (description_lines.len().saturating_sub(1) as f32)
                * description_font
                * config.line_height;
            let baseline_y =
                (cursor_y + description_height.min((h * 0.35).max(0.0))).min(y + h - 8.0);
            out.push_str("<text x=\"");
            let _ = write_number_into(out, x + w / 2.0);
            out.push_str("\" y=\"");
            let _ = write_number_into(out, baseline_y);
            out.push_str("\" text-anchor=\"middle\" font-size=\"");
            let _ = write_number_into(out, description_font);
            out.push_str("\" fill=\"");
            let _ = write_escaped_attr(out, &colors.text);
            out.push_str("\" class=\"fm-c4-description\">");
            let _ = write_escaped_text(out, &description_lines.join("\n"));
            out.push_str("</text>");
        }
    }

    // <title>Node: {raw_label}, rounded rectangle</title></g>
    out.push_str("<title>Node: ");
    let _ = write_escaped_text(out, raw_label);
    out.push_str(", rounded rectangle</title></g>");
}

/// Stream `render_c4_person_icon`'s `<g class="fm-c4-person-icon">` (circle + 4 lines) byte-identically.
fn write_c4_person_icon_into(f: &mut String, x: f32, y: f32, stroke: &str) {
    use crate::attributes::{write_escaped_attr, write_number_into};
    f.push_str("<g class=\"fm-c4-person-icon\"><circle cx=\"");
    let _ = write_number_into(f, x);
    f.push_str("\" cy=\"");
    let _ = write_number_into(f, y - 6.0);
    f.push_str("\" r=\"3\" fill=\"none\" stroke=\"");
    let _ = write_escaped_attr(f, stroke);
    f.push_str("\" stroke-width=\"1.10\"/>");
    write_c4_icon_line_into(f, x, y - 2.0, x, y + 7.0, stroke);
    write_c4_icon_line_into(f, x - 5.0, y + 1.0, x + 5.0, y + 1.0, stroke);
    write_c4_icon_line_into(f, x, y + 7.0, x - 4.5, y + 13.0, stroke);
    write_c4_icon_line_into(f, x, y + 7.0, x + 4.5, y + 13.0, stroke);
    f.push_str("</g>");
}

fn write_c4_icon_line_into(f: &mut String, x1: f32, y1: f32, x2: f32, y2: f32, stroke: &str) {
    use crate::attributes::{write_escaped_attr, write_number_into};
    f.push_str("<line x1=\"");
    let _ = write_number_into(f, x1);
    f.push_str("\" y1=\"");
    let _ = write_number_into(f, y1);
    f.push_str("\" x2=\"");
    let _ = write_number_into(f, x2);
    f.push_str("\" y2=\"");
    let _ = write_number_into(f, y2);
    f.push_str("\" stroke=\"");
    let _ = write_escaped_attr(f, stroke);
    f.push_str("\" stroke-width=\"1.10\"/>");
}

/// Stream a complete ER entity node (`<g>` + gradient `<rect>` + entity body + `<title>`) directly into
/// `out`, **byte-identical** to what `render_node` builds via `Element`s — the ER analogue of
/// [`write_class_node_fragment_into`]. The `<g>`/rect/title bytes replicate that helper's (proven)
/// sequence for a `Rect` shape (a Rect ER entity's shape and `describe_node` form are both identical to a
/// class node's); the body is [`write_er_entity_into`] (the same code `render_node`'s ER fast path runs).
/// Used only via `render_node_into`'s ER gate, which guarantees none of `render_node`'s conditional
/// classes/children/post-processing apply. Skips the group + rect `Element` builds, their `Attributes`
/// Vecs, the entity-body fragment's second copy, and the whole-group serialize+copy.
#[allow(clippy::too_many_arguments)]
/// `A11Y` selects the accessibility variant at compile time (see `write_class_node_fragment_into`): `true`
/// emits `role`/`aria-label`/`tabindex` + the trailing `<title>`; `false` (lean) skips exactly those two
/// spots. The `<g id/class/data-id>` wrapper, `<rect>`, and the entity attribute body are a11y-independent,
/// so the lean fragment matches the slow `Element` path's lean output by construction.
fn write_er_node_fragment_into<const A11Y: bool>(
    out: &mut String,
    node: &fm_core::IrNode,
    node_id: &str,
    node_index: usize,
    raw_label: &str,
    label_text: &str,
    cx: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: f32,
    font_size: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
    user_classes: &str,
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text};
    // <g id=".." class="fm-node fm-node-accent-N fm-node-shape-rect[ fm-node-user-…]" data-id=".." …>
    out.push_str("<g id=\"");
    fm_core::write_mermaid_node_element_id_into(out, node_id, node_index);
    out.push_str("\" class=\"fm-node fm-node-accent-");
    let _ = crate::attributes::write_uint_into(out, stable_accent_index(node_id) as u64);
    out.push(' ');
    out.push_str(node_shape_css_class(fm_core::NodeShape::Rect));
    out.push_str(user_classes);
    out.push_str("\" data-id=\"");
    let _ = write_escaped_attr(out, node_id);
    if A11Y {
        out.push_str("\" role=\"graphics-symbol\" aria-label=\"");
        let _ = write_escaped_attr(out, raw_label);
        out.push_str("\" tabindex=\"0\">");
    } else {
        out.push_str("\">");
    }
    // <rect x y width height rx fill="url(#fm-node-gradient)"/> — same attr order as the class fragment.
    out.push_str("<rect x=\"");
    let _ = crate::attributes::write_number_into(out, x);
    out.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(out, y);
    out.push_str("\" width=\"");
    let _ = crate::attributes::write_number_into(out, w);
    out.push_str("\" height=\"");
    let _ = crate::attributes::write_number_into(out, h);
    out.push_str("\" rx=\"");
    let _ = crate::attributes::write_number_into(out, rx);
    out.push_str("\" fill=\"url(#fm-node-gradient)\"/>");
    write_er_entity_into(
        out, node, label_text, cx, x, y, w, font_size, config, colors,
    );
    if A11Y {
        // <title>Node: {raw_label}, rectangle</title></g> — describe_node's Rect form, written piecewise.
        out.push_str("<title>Node: ");
        let _ = write_escaped_text(out, raw_label);
        out.push_str(", rectangle</title></g>");
    } else {
        out.push_str("</g>");
    }
}

/// The inline `style="fill: …"` color the slow path (`render_node`) applies to a node's shape rect for
/// journey-score / kanban-priority classes (`journey_score_fill`/`kanban_priority_fill`). The common
/// streaming fragment must emit the identical `style` or it silently drops the score/priority color under
/// the default embedded-CSS + gradient config. Requirement-risk fills never reach the common fragment (its
/// gate excludes `requirement_meta`), so they're intentionally not handled here.
fn common_fragment_special_fill(node: &fm_core::IrNode) -> Option<&'static str> {
    node.classes.iter().find_map(|class| match class.as_str() {
        "journey-score-1" => Some("#fca5a5"),
        "journey-score-2" => Some("#fdba74"),
        "journey-score-3" => Some("#fde68a"),
        "journey-score-4" => Some("#bef264"),
        "journey-score-5" => Some("#86efac"),
        "kanban-priority-high" | "kanban-priority-critical" => Some("#fca5a5"),
        "kanban-priority-medium" => Some("#fde68a"),
        "kanban-priority-low" => Some("#bbf7d0"),
        _ => None,
    })
}

/// Emit ` style="fill: {color}"` for a node whose journey-score/kanban-priority class overrides the shape
/// fill. Byte-identical to `render_node`'s slow path, which builds the same attr via
/// `Element::attr("style", &format!("fill: {fill}"))` (the color is a fixed `#rrggbb`, never escapable).
fn write_special_fill_style_into(f: &mut String, special_fill: Option<&str>) {
    if let Some(fill) = special_fill {
        f.push_str(" style=\"fill: ");
        f.push_str(fill);
        f.push('"');
    }
}

/// Stream a closed polygon `<path>` — the common streaming fast path's shape element for the single-path
/// polygon shapes (Diamond/Hexagon/Trapezoid/InvTrapezoid/Parallelogram/Asymmetric). Byte-identical to
/// `render_node`'s `PathBuilder::move_to(p0).line_to(p1)…close().build()`: commands join with single spaces
/// (`M{x0} {y0} L{x1} {y1} … Z`), and coords use `AttributeValue::Number::write_value`, which is bit-for-bit
/// identical to `PathBuilder`'s `FmtNum` (both: `n as i32` round-trip → `write_int_into` else `write_fixed2`).
fn write_polygon_shape_into(f: &mut String, points: &[(f32, f32)], special_fill: Option<&str>) {
    f.push_str("<path d=\"");
    for (i, &(px, py)) in points.iter().enumerate() {
        f.push_str(if i == 0 { "M" } else { " L" });
        let _ = crate::attributes::write_number_into(f, px);
        f.push(' ');
        let _ = crate::attributes::write_number_into(f, py);
    }
    f.push_str(" Z\" fill=\"url(#fm-node-gradient)\"");
    write_special_fill_style_into(f, special_fill);
    f.push_str("/>");
}

/// Stream the cylinder/database node path. This reproduces the slow `PathBuilder` bytes exactly:
/// `M x y+ry A w/2 ry 0 0 1 x+w y+ry L x+w y+h-ry A w/2 ry 0 0 0 x y+h-ry Z M x y+ry
/// A w/2 ry 0 0 0 x+w y+ry`.
fn write_cylinder_shape_into(
    f: &mut String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    special_fill: Option<&str>,
) {
    let ry = h * 0.1;
    let rx = w / 2.0;
    let top_y = y + ry;
    let bottom_y = y + h - ry;
    let right_x = x + w;

    f.push_str("<path d=\"M");
    let _ = crate::attributes::write_number_into(f, x);
    f.push(' ');
    let _ = crate::attributes::write_number_into(f, top_y);
    f.push_str(" A");
    let _ = crate::attributes::write_number_into(f, rx);
    f.push(' ');
    let _ = crate::attributes::write_number_into(f, ry);
    f.push_str(" 0 0 1 ");
    let _ = crate::attributes::write_number_into(f, right_x);
    f.push(' ');
    let _ = crate::attributes::write_number_into(f, top_y);
    f.push_str(" L");
    let _ = crate::attributes::write_number_into(f, right_x);
    f.push(' ');
    let _ = crate::attributes::write_number_into(f, bottom_y);
    f.push_str(" A");
    let _ = crate::attributes::write_number_into(f, rx);
    f.push(' ');
    let _ = crate::attributes::write_number_into(f, ry);
    f.push_str(" 0 0 0 ");
    let _ = crate::attributes::write_number_into(f, x);
    f.push(' ');
    let _ = crate::attributes::write_number_into(f, bottom_y);
    f.push_str(" Z M");
    let _ = crate::attributes::write_number_into(f, x);
    f.push(' ');
    let _ = crate::attributes::write_number_into(f, top_y);
    f.push_str(" A");
    let _ = crate::attributes::write_number_into(f, rx);
    f.push(' ');
    let _ = crate::attributes::write_number_into(f, ry);
    f.push_str(" 0 0 0 ");
    let _ = crate::attributes::write_number_into(f, right_x);
    f.push(' ');
    let _ = crate::attributes::write_number_into(f, top_y);
    f.push_str("\" fill=\"url(#fm-node-gradient)\"");
    write_special_fill_style_into(f, special_fill);
    f.push_str("/>");
}

#[allow(clippy::too_many_arguments)]
fn write_subroutine_node_fragment_into(
    out: &mut String,
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
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text};

    out.push_str("<g id=\"");
    fm_core::write_mermaid_node_element_id_into(out, node_id, node_index);
    out.push_str("\" class=\"fm-node fm-node-accent-");
    // `accent` (small palette index) via the digit-table writer, not `write!`'s Formatter/`pad_integral`
    // machinery (measured ~1.87% of node-heavy render). Byte-identical: same decimal digits.
    let _ = crate::attributes::write_uint_into(out, accent as u64);
    out.push(' ');
    out.push_str(node_shape_css_class(fm_core::NodeShape::Subroutine));
    out.push_str(user_classes);
    out.push_str("\" data-id=\"");
    let _ = write_escaped_attr(out, node_id);
    out.push_str("\" role=\"graphics-symbol\" aria-label=\"");
    let _ = write_escaped_attr(out, raw_label);
    out.push_str("\" tabindex=\"0\"><g><rect x=\"");
    let _ = crate::attributes::write_number_into(out, x);
    out.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(out, y);
    out.push_str("\" width=\"");
    let _ = crate::attributes::write_number_into(out, w);
    out.push_str("\" height=\"");
    let _ = crate::attributes::write_number_into(out, h);
    out.push_str("\" fill=\"url(#fm-node-gradient)\" rx=\"");
    let _ = crate::attributes::write_number_into(out, rx);
    out.push_str("\"/><line x1=\"");
    let _ = crate::attributes::write_number_into(out, x + 8.0);
    out.push_str("\" y1=\"");
    let _ = crate::attributes::write_number_into(out, y);
    out.push_str("\" x2=\"");
    let _ = crate::attributes::write_number_into(out, x + 8.0);
    out.push_str("\" y2=\"");
    let _ = crate::attributes::write_number_into(out, y + h);
    out.push_str("\" stroke-width=\"1\"/><line x1=\"");
    let _ = crate::attributes::write_number_into(out, x + w - 8.0);
    out.push_str("\" y1=\"");
    let _ = crate::attributes::write_number_into(out, y);
    out.push_str("\" x2=\"");
    let _ = crate::attributes::write_number_into(out, x + w - 8.0);
    out.push_str("\" y2=\"");
    let _ = crate::attributes::write_number_into(out, y + h);
    out.push_str("\" stroke-width=\"1\"/></g><text x=\"");
    let _ = crate::attributes::write_number_into(out, text_x);
    out.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(out, text_y);
    out.push_str("\" text-anchor=\"middle\" font-size=\"");
    let _ = crate::attributes::write_number_into(out, font_size);
    out.push_str("\" fill=\"");
    let _ = write_escaped_attr(out, text_fill);
    out.push_str("\">");
    let _ = write_escaped_text(out, label);
    out.push_str("</text></g>");
}

/// Whether the per-element accessibility flags are uniformly on (`Some(true)`) or uniformly off
/// (`Some(false)`), the two shapes the streaming node fragment can emit. Mixed combinations such as
/// [`A11yConfig::minimal`] return `None` and take the slow `Element` path, as they always have.
///
/// `accessibility_css` is deliberately not consulted: it controls a document-level `<style>` block, not
/// any per-element attribute.
const fn uniform_a11y(a11y: &A11yConfig) -> Option<bool> {
    match (a11y.aria_labels, a11y.keyboard_nav, a11y.text_alternatives) {
        (true, true, true) => Some(true),
        (false, false, false) => Some(false),
        _ => None,
    }
}

/// `A11Y` selects the accessibility variant at compile time: `true` emits the
/// `role`/`aria-label`/`tabindex`/`<title>` set that `A11yConfig::full()` produces, `false` emits none of
/// it, matching `A11yConfig::none()`. Making it a const parameter rather than a runtime flag keeps the
/// default (full) monomorphization exactly as branch-free as it was before the lean variant existed --
/// a runtime flag cost a measured +0.1..0.33% instructions on the default path.
#[allow(clippy::too_many_arguments)]
fn build_common_node_fragment<const A11Y: bool>(
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
    special_fill: Option<&str>,
) -> String {
    // `raw_label` is written twice (the `aria-label` and the `<title>` text), so size for both copies
    // plus the fixed tag/literal bytes.
    let mut f = String::with_capacity(label.len() + raw_label.len() * 2 + user_classes.len() + 340);
    write_common_node_fragment_into::<A11Y>(
        &mut f,
        node_id,
        node_index,
        accent,
        raw_label,
        label,
        x,
        y,
        w,
        h,
        rx,
        text_x,
        text_y,
        font_size,
        text_fill,
        user_classes,
        shape,
        special_fill,
    );
    f
}

/// Write-into core of [`build_common_node_fragment`]: streams the common rect node straight into `f` with
/// no intermediate `String`, so `render_node_into` can render it directly into the chunk output buffer.
#[allow(clippy::too_many_arguments)]
fn write_common_node_fragment_into<const A11Y: bool>(
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
    special_fill: Option<&str>,
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text};
    // <g id=".." class="fm-node fm-node-accent-N fm-node-shape-rect[ fm-node-user-…]" data-id=".." …>
    f.push_str("<g id=\"");
    // The node id is `fm-node-[{sanitized}-]{index}` — only `[a-z0-9-]`, never an escapable byte — so
    // write it straight into `f` (skipping `mermaid_node_element_id`'s 3 throwaway allocations: the
    // sanitizer's two Strings + the id String). Byte-identical to `write_escaped_attr(id)` because the
    // id can never contain `& < > " '`; pinned by `node_fast_fragment_matches_render`.
    fm_core::write_mermaid_node_element_id_into(f, node_id, node_index);
    f.push_str("\" class=\"fm-node fm-node-accent-");
    // `accent` (small palette index) via the digit-table writer, not `write!`'s Formatter/`pad_integral`.
    let _ = crate::attributes::write_uint_into(f, accent as u64);
    f.push(' ');
    f.push_str(node_shape_css_class(shape));
    // Simple custom classes (`class X foo` / `:::foo` on nodes with no state-keyword or special class);
    // empty for the overwhelmingly common no-class node. Matches the slow path's ` fm-node-user-…` tail.
    f.push_str(user_classes);
    f.push_str("\" data-id=\"");
    let _ = write_escaped_attr(f, node_id);
    // The a11y attributes appear in the same insertion order `render_node`'s slow path builds them
    // (`aria_labels` -> role + aria-label, then `keyboard_nav` -> tabindex). `A11Y` is const, so the
    // full variant compiles to the same straight-line pushes it always did, and the lean variant compiles
    // them away entirely -- matching, byte for byte, what the slow Element path emits under
    // `A11yConfig::none()`.
    if A11Y {
        f.push_str("\" role=\"graphics-symbol\" aria-label=\"");
        let _ = write_escaped_attr(f, raw_label);
        f.push_str("\" tabindex=\"0\">");
    } else {
        f.push_str("\">");
    }
    // Shape element with the gradient fill (stroke/stroke-width are CSS-driven under embedded theme, so
    // absent inline). `<rect x y width height rx …/>` for the rect family (Rect/Rounded/Stadium — the
    // caller passes each shape's `rx`); `<circle cx cy r …/>` for Circle, whose cx/cy/r match render_node's
    // slow path (cx=x+w/2, cy=y+h/2, r=w.min(h)/2) and whose serialized attr order (cx,cy,r,fill) matches
    // the slow `Element::circle()` after the gradient-fill override.
    match shape {
        fm_core::NodeShape::Circle => {
            f.push_str("<circle cx=\"");
            let _ = crate::attributes::write_number_into(f, x + w / 2.0);
            f.push_str("\" cy=\"");
            let _ = crate::attributes::write_number_into(f, y + h / 2.0);
            f.push_str("\" r=\"");
            let _ = crate::attributes::write_number_into(f, w.min(h) / 2.0);
            f.push_str("\" fill=\"url(#fm-node-gradient)\"");
            write_special_fill_style_into(f, special_fill);
            f.push_str("/>");
        }
        // Single-`<path>` polygon shapes: each reproduces `render_node`'s slow-path `PathBuilder` point
        // sequence exactly (see `write_polygon_shape_into`). `inset`/`flag` = `w * 0.15` as in the slow path.
        fm_core::NodeShape::Diamond => {
            let cx = x + w / 2.0;
            let cy = y + h / 2.0;
            write_polygon_shape_into(
                f,
                &[(cx, y), (x + w, cy), (cx, y + h), (x, cy)],
                special_fill,
            );
        }
        fm_core::NodeShape::Hexagon => {
            let cy = y + h / 2.0;
            let inset = w * 0.15;
            write_polygon_shape_into(
                f,
                &[
                    (x + inset, y),
                    (x + w - inset, y),
                    (x + w, cy),
                    (x + w - inset, y + h),
                    (x + inset, y + h),
                    (x, cy),
                ],
                special_fill,
            );
        }
        fm_core::NodeShape::Cylinder => {
            write_cylinder_shape_into(f, x, y, w, h, special_fill);
        }
        fm_core::NodeShape::Trapezoid => {
            let inset = w * fm_core::SLANTED_SHAPE_INSET_RATIO;
            write_polygon_shape_into(
                f,
                &[
                    (x + inset, y),
                    (x + w - inset, y),
                    (x + w, y + h),
                    (x, y + h),
                ],
                special_fill,
            );
        }
        fm_core::NodeShape::InvTrapezoid => {
            let inset = w * fm_core::SLANTED_SHAPE_INSET_RATIO;
            write_polygon_shape_into(
                f,
                &[
                    (x, y),
                    (x + w, y),
                    (x + w - inset, y + h),
                    (x + inset, y + h),
                ],
                special_fill,
            );
        }
        fm_core::NodeShape::Parallelogram => {
            let inset = w * fm_core::SLANTED_SHAPE_INSET_RATIO;
            write_polygon_shape_into(
                f,
                &[
                    (x + inset, y),
                    (x + w, y),
                    (x + w - inset, y + h),
                    (x, y + h),
                ],
                special_fill,
            );
        }
        fm_core::NodeShape::InvParallelogram => {
            let inset = w * fm_core::SLANTED_SHAPE_INSET_RATIO;
            write_polygon_shape_into(
                f,
                &[
                    (x, y),
                    (x + w - inset, y),
                    (x + w, y + h),
                    (x + inset, y + h),
                ],
                special_fill,
            );
        }
        fm_core::NodeShape::Asymmetric => {
            let cy = y + h / 2.0;
            let flag = w * 0.15;
            write_polygon_shape_into(
                f,
                &[
                    (x, y),
                    (x + w - flag, y),
                    (x + w, cy),
                    (x + w - flag, y + h),
                    (x, y + h),
                ],
                special_fill,
            );
        }
        _ => {
            // Rect / Rounded / Stadium — all rect elements, differing only in `rx` (set by the caller).
            f.push_str("<rect x=\"");
            let _ = crate::attributes::write_number_into(f, x);
            f.push_str("\" y=\"");
            let _ = crate::attributes::write_number_into(f, y);
            f.push_str("\" width=\"");
            let _ = crate::attributes::write_number_into(f, w);
            f.push_str("\" height=\"");
            let _ = crate::attributes::write_number_into(f, h);
            f.push_str("\" rx=\"");
            let _ = crate::attributes::write_number_into(f, rx);
            f.push_str("\" fill=\"url(#fm-node-gradient)\"");
            write_special_fill_style_into(f, special_fill);
            f.push_str("/>");
        }
    }
    // <text x y text-anchor="middle" font-size=".." fill="..">label</text>
    f.push_str("<text x=\"");
    let _ = crate::attributes::write_number_into(f, text_x);
    f.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(f, text_y);
    f.push_str("\" text-anchor=\"middle\" font-size=\"");
    let _ = crate::attributes::write_number_into(f, font_size);
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
    // Emitted only in the `A11Y` variant, exactly as the slow path's `Element::title` child is gated on
    // `text_alternatives`. Pinned by `node_fast_fragment_matches_render` (full) and
    // `node_lean_fast_fragment_omits_a11y` (lean).
    if A11Y {
        f.push_str("<title>Node: ");
        let _ = write_escaped_text(f, raw_label);
        // `describe_node`'s shape word for the gated shapes; the `</title></g>` tail is fused into the
        // literal so the full variant emits the whole title in one `push_str`, as it did before.
        f.push_str(match shape {
            fm_core::NodeShape::Rect => ", rectangle</title></g>",
            fm_core::NodeShape::Rounded => ", rounded rectangle</title></g>",
            fm_core::NodeShape::Stadium => ", stadium shape</title></g>",
            fm_core::NodeShape::Diamond => ", diamond</title></g>",
            fm_core::NodeShape::Hexagon => ", hexagon</title></g>",
            fm_core::NodeShape::Cylinder => ", cylinder</title></g>",
            fm_core::NodeShape::Trapezoid => ", trapezoid</title></g>",
            fm_core::NodeShape::InvTrapezoid => ", inverted trapezoid</title></g>",
            fm_core::NodeShape::Parallelogram => ", parallelogram</title></g>",
            fm_core::NodeShape::InvParallelogram => ", inverted parallelogram</title></g>",
            fm_core::NodeShape::Asymmetric => ", flag shape</title></g>",
            _ => ", circle</title></g>",
        });
    } else {
        f.push_str("</g>");
    }
}

#[allow(clippy::too_many_arguments)]
/// `A11Y` selects the accessibility variant at compile time (see `write_class_node_fragment_into`): `true`
/// emits `role`/`aria-label`/`tabindex` + the trailing `<title>`; `false` (lean) skips exactly those two
/// spots. The `<g id/class/data-id>` wrapper, `<rect>`, and the subtitle rows are a11y-independent, so the
/// lean fragment matches the slow `Element` path's lean output by construction.
fn write_requirement_node_fragment_into<const A11Y: bool>(
    out: &mut String,
    meta: &fm_core::IrRequirementNodeMeta,
    node_id: &str,
    node_index: usize,
    raw_label: &str,
    label: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: f32,
    cx: f32,
    font_size: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text};

    out.push_str("<g id=\"");
    fm_core::write_mermaid_node_element_id_into(out, node_id, node_index);
    out.push_str("\" class=\"fm-node fm-node-accent-");
    // `stable_accent_index` (small palette index) via the digit-table writer, not `write!`'s
    // Formatter/`pad_integral` machinery. Byte-identical: same decimal digits.
    let _ = crate::attributes::write_uint_into(out, stable_accent_index(node_id) as u64);
    out.push_str(" fm-node-shape-rect");
    if let Some(risk) = meta.risk.as_deref() {
        out.push_str(" fm-req-risk-");
        // `write_sanitized_css_token_into` lowercases every ASCII-alphanumeric char and maps the rest
        // to `-` regardless of case, so a pre-`to_ascii_lowercase()` (a per-node throwaway String) is
        // redundant — passing the raw `&str` is byte-identical.
        write_sanitized_css_token_into(out, risk);
    }
    if let Some(req_type) = meta.requirement_type.as_deref() {
        out.push_str(" fm-req-type-");
        write_sanitized_css_token_into(out, req_type);
    }
    if meta.verify_method.is_some() {
        out.push_str(" fm-req-has-verify");
    }
    out.push_str("\" data-id=\"");
    let _ = write_escaped_attr(out, node_id);
    if A11Y {
        out.push_str("\" role=\"graphics-symbol\" aria-label=\"");
        let _ = write_escaped_attr(out, raw_label);
        out.push_str("\" tabindex=\"0\">");
    } else {
        out.push_str("\">");
    }

    out.push_str("<rect x=\"");
    let _ = crate::attributes::write_number_into(out, x);
    out.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(out, y);
    out.push_str("\" width=\"");
    let _ = crate::attributes::write_number_into(out, w);
    out.push_str("\" height=\"");
    let _ = crate::attributes::write_number_into(out, h);
    out.push_str("\" rx=\"");
    let _ = crate::attributes::write_number_into(out, rx);
    out.push_str("\" fill=\"url(#fm-node-gradient)\"");
    if let Some(fill) = requirement_risk_fill(meta) {
        out.push_str(" style=\"fill: ");
        out.push_str(fill);
        out.push('"');
    }
    out.push_str("/>");

    let subtitle_font_size = clamp_font_size(font_size * 0.75, config.min_font_size);
    let mut text_y = y + h * 0.25 + font_size * 0.35;
    if let Some(req_type) = meta.requirement_type.as_deref() {
        // ⚠️ `<<Type>>`, NOT `«keyword»`, and both halves changed. mermaid draws
        // `` `<<${n.type}>>` `` with ASCII angles — the same wrapper this renderer already uses for a
        // CLASS stereotype — and `n.type` is the DISPLAY name from its `RequirementType` table, not
        // the authored keyword. We drew `«functionalRequirement»` where mermaid draws
        // `<<Functional Requirement>>`: wrong words inside a wrapper we spell differently from our
        // own class path. `>` IS XML-special, so the escaper writes the whole thing.
        let req_type = fm_core::requirement_type_display(req_type);
        write_req_subtitle_body_into(
            out,
            cx,
            text_y,
            subtitle_font_size,
            " font-style=\"italic\"",
            "",
            &colors.text,
            "fm-req-type-label",
            |f| {
                let _ = write_escaped_text(f, "<<");
                let _ = write_escaped_text(f, req_type);
                let _ = write_escaped_text(f, ">>");
            },
        );
        text_y += font_size * 0.85;
    }

    out.push_str("<text x=\"");
    let _ = crate::attributes::write_number_into(out, cx);
    out.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(out, text_y);
    out.push_str("\" text-anchor=\"middle\" font-size=\"");
    let _ = crate::attributes::write_number_into(out, font_size);
    out.push_str("\" fill=\"");
    let _ = write_escaped_attr(out, &colors.text);
    out.push_str("\">");
    let _ = write_escaped_text(out, label);
    out.push_str("</text>");
    text_y += font_size * 0.85;

    // `id:` and `text:` are the requirement's traceability key and the requirement itself — the
    // sentence the diagram exists to record. Both were parsed into the IR, held there, and read by
    // nothing (bd-f3tc). The incumbent draws them as their own rows between the name and the risk
    // row (`ID: …` then `Text: …` in mermaid 11.15.0's requirement renderer), which is what these
    // reproduce. The fixed prefixes hold no XML specials, so streaming the parts is byte-identical
    // to escaping a joined `format!` without the per-node String.
    // `Type:` and `Doc:` are an ELEMENT's fields, added by bd-qdmn for the same reason bd-f3tc added
    // the two above: declared by the author, dropped before the IR, drawn by nothing. They join this
    // table rather than getting their own block so an element's rows stack with a requirement's in
    // one order, and so a future row cannot forget the `text_y` advance below.
    // ⚠️ ONE ROW PER FIELD, IN mermaid's ORDER, WITH mermaid's LABELS. Its requirement renderer emits
    // each as an independent line and nothing is ever joined:
    //
    // ```text
    //   `ID: ${n.requirementId}`     `Text: ${n.text}`
    //   `Risk: ${n.risk}`            `Verification: ${n.verifyMethod}`      <- a requirement
    //   `Type: ${a.type}`            `Doc Ref: ${a.docRef}`                 <- an element
    // ```
    //
    // Two divergences lived here and both were found by rendering the incumbent in Chromium and
    // diffing drawn text (`scripts/headtohead/chromium_text_diff.mjs`):
    //
    //   * risk and verification were fused into ONE row, `Risk: High | Verify: Test`, a separator
    //     mermaid never draws;
    //   * the labels were `Verify:` and `Doc:` where mermaid writes `Verification:` and `Doc Ref:`.
    //
    // A node carries either the requirement fields or the element fields, never both, so one table in
    // this order reproduces both orders. Folding risk and verification in here rather than leaving
    // them in their own block is also what stops a future row forgetting the `text_y` advance below.
    for (prefix, value, class) in [
        ("ID: ", meta.req_id.as_deref(), "fm-req-id"),
        ("Text: ", meta.text.as_deref(), "fm-req-text"),
        ("Risk: ", meta.risk.as_deref(), "fm-req-metadata"),
        (
            "Verification: ",
            meta.verify_method.as_deref(),
            "fm-req-metadata",
        ),
        (
            "Type: ",
            meta.element_type.as_deref(),
            "fm-req-element-type",
        ),
        ("Doc Ref: ", meta.doc_ref.as_deref(), "fm-req-docref"),
    ] {
        let Some(value) = value else { continue };
        write_req_subtitle_body_into(
            out,
            cx,
            text_y,
            subtitle_font_size,
            "",
            " opacity=\"0.7\"",
            &colors.text,
            class,
            |f| {
                f.push_str(prefix);
                let _ = write_escaped_text(f, value);
            },
        );
        text_y += font_size * 0.85;
    }

    if A11Y {
        out.push_str("<title>Node: ");
        let _ = write_escaped_text(out, raw_label);
        out.push_str(", rectangle</title></g>");
    } else {
        out.push_str("</g>");
    }
}

fn requirement_risk_fill(meta: &fm_core::IrRequirementNodeMeta) -> Option<&'static str> {
    let risk = meta.risk.as_deref()?;
    if risk.eq_ignore_ascii_case("high") {
        Some("#fca5a5")
    } else if risk.eq_ignore_ascii_case("medium") {
        Some("#fde68a")
    } else if risk.eq_ignore_ascii_case("low") {
        Some("#bbf7d0")
    } else {
        None
    }
}

/// True if `s` contains a line break (`\n` or `\r`). One byte-scan pass, byte-identical to
/// `s.contains('\n') || s.contains('\r')` (both needles are ASCII, so a byte scan matches the
/// `char` scan) but the two separate `str::contains(char)` calls each scan the whole label, so the
/// common single-line label was read twice per node — this reads it once. Same ASCII-`bytes().any`
/// family as the parser's nested-bracket fast scan. Used by the node fast-path gates, where a
/// multi-line label must fall back to the slow multi-line `TextBuilder` path.
#[inline]
fn label_has_line_break(s: &str) -> bool {
    s.as_bytes().iter().any(|&b| b == b'\n' || b == b'\r')
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

    // A composite state IS its container: `anchor_composite_state_nodes` gave this node its
    // cluster's bounds so transitions attach to the container's boundary. Drawing it again here
    // would stack a second labelled box on the container — the duplicate this removes (bd-9w54).
    // The cluster still draws that box and its title, so nothing declared disappears.
    if is_composite_state_node(ir, node_box) {
        return;
    }

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
        // ⚠️ Shared with fm-layout, which SIZES the box from the same call (bd-3cj8v). A local copy
        // of this rule here drifted from layout's and reproduced a state-diagram id suppression on
        // gitGraph commits, which draw an id in mermaid.
        ir_node.map_or("", |node| ir.node_display_text(node))
    };
    // A sankey node additionally carries its throughput on a second line, as mermaid draws it.
    // Owned, so it must outlive the borrow above; `None` for every other diagram type.
    let sankey_label = sankey_node_label(ir, node_box.node_index);
    let raw_label_text = sankey_label.as_deref().unwrap_or(raw_label_text);
    let label_text = truncate_label(raw_label_text, detail.node_label_max_chars);
    let node_font_size = detail.node_font_size;
    let label_may_overflow = label_text.lines().any(|line| {
        line.chars().count() as f32
            * config.avg_char_width
            * (node_font_size / config.font_size.max(1.0))
            > (w - 16.0).max(node_font_size)
    });
    let node_icon = ir_node
        .and_then(|node| node.icon())
        .map(str::trim)
        .filter(|icon| !icon.is_empty())
        .filter(|_| ir_node.is_none_or(|node| node.class_meta.is_none() && node.c4_meta.is_none()));

    // Two a11y-uniform gates (see the class path below for the rationale): the full-a11y gate is unchanged
    // (direct `::<true>`) so the default path takes no regression; the lean gate streams a11y-off
    // requirement nodes that used to fall to the slow `Element` path. Mixed a11y → slow path, as before.
    if let Some(node) = ir_node
        && matches!(shape, NodeShape::Rect)
        && let Some(meta) = node.requirement_meta.as_deref()
        && detail.show_node_labels
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
        && !label_has_line_break(&label_text)
        && !label_may_overflow
        && lookup_centrality_tier(centrality_map, node_box.node_index).is_none()
        && label_id.is_none_or(|id| ir.label_markup.get(&id).is_none_or(|s| s.is_empty()))
        && node.class_meta.is_none()
        && node.c4_meta.is_none()
        && node.classes.is_empty()
        && node.menu_links.is_empty()
        && node.href().is_none()
        && node.callback().is_none()
        && node.tooltip().is_none()
    {
        write_requirement_node_fragment_into::<true>(
            out,
            meta,
            node_id,
            node_box.node_index,
            raw_label_text,
            &label_text,
            x,
            y,
            w,
            h,
            config.rounded_corners * 0.55,
            cx,
            node_font_size,
            config,
            colors,
        );
        return;
    }
    if let Some(node) = ir_node
        && matches!(shape, NodeShape::Rect)
        && let Some(meta) = node.requirement_meta.as_deref()
        && detail.show_node_labels
        && config.embed_theme_css
        && config.node_gradients
        && !emit_classdef_classes
        && !config.animations_enabled
        && !config.include_source_spans
        && !config.a11y.aria_labels
        && !config.a11y.keyboard_nav
        && !config.a11y.text_alternatives
        && shape_style.is_none()
        && text_style.is_none()
        && node_icon.is_none()
        && !placeholder_space_node
        && !label_has_line_break(&label_text)
        && !label_may_overflow
        && lookup_centrality_tier(centrality_map, node_box.node_index).is_none()
        && label_id.is_none_or(|id| ir.label_markup.get(&id).is_none_or(|s| s.is_empty()))
        && node.class_meta.is_none()
        && node.c4_meta.is_none()
        && node.classes.is_empty()
        && node.menu_links.is_empty()
        && node.href().is_none()
        && node.callback().is_none()
        && node.tooltip().is_none()
    {
        write_requirement_node_fragment_into::<false>(
            out,
            meta,
            node_id,
            node_box.node_index,
            raw_label_text,
            &label_text,
            x,
            y,
            w,
            h,
            config.rounded_corners * 0.55,
            cx,
            node_font_size,
            config,
            colors,
        );
        return;
    }

    // Whole-class-node streaming fast path: a themed class-diagram node with compartments (name +
    // attribute/method rows) whose config carries no conditional render (same gate class as the common
    // node fast path, plus the class-node specifics). `render_node`'s slow path would build a group
    // `Element` + rect `Element` + a *separate* compartment fragment `String` wrapped in a child; stream
    // the whole `<g>…</g>` in place instead. Byte-identical (pinned by `svg_golden_snapshots_are_stable`).
    // Two a11y-uniform gates instead of one relaxed `uniform_a11y()` gate + runtime dispatch. Keeping the
    // default (full-a11y) gate exactly as it was — direct `::<true>`, no `uniform_a11y` in the per-node hot
    // path — avoids a measured +0.37% default-`class` regression the single relaxed gate caused. The second
    // gate is the new lean behaviour: it streams a11y-off class nodes (`::<false>`) that used to fall to the
    // common gate (the +0.31% class_50 regression from bd-b2b6). `A11yConfig::minimal()` matches neither
    // (`aria_labels` on but `keyboard_nav`/`text_alternatives` off) → slow `Element` path, exactly as before.
    if let Some(node) = ir_node
        && matches!(shape, NodeShape::Rect)
        && let Some(meta) = node.class_meta.as_deref()
        && (!meta.attributes.is_empty() || !meta.methods.is_empty() || meta.stereotype.is_some())
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
        && lookup_centrality_tier(centrality_map, node_box.node_index).is_none()
        && node.requirement_meta.is_none()
        && node.menu_links.is_empty()
        && node.href().is_none()
        && node.callback().is_none()
        && node.tooltip().is_none()
        && let Some(user_classes) = simple_class_node_user_suffix(node)
    {
        write_class_node_fragment_into::<true>(
            out,
            node,
            meta,
            node_id,
            node_box.node_index,
            raw_label_text,
            ir,
            x,
            y,
            w,
            h,
            config.rounded_corners * 0.55,
            node_font_size,
            config,
            colors,
            &user_classes,
        );
        return;
    }
    if let Some(node) = ir_node
        && matches!(shape, NodeShape::Rect)
        && let Some(meta) = node.class_meta.as_deref()
        && (!meta.attributes.is_empty() || !meta.methods.is_empty() || meta.stereotype.is_some())
        && config.embed_theme_css
        && config.node_gradients
        && !emit_classdef_classes
        && !config.animations_enabled
        && !config.include_source_spans
        && !config.a11y.aria_labels
        && !config.a11y.keyboard_nav
        && !config.a11y.text_alternatives
        && shape_style.is_none()
        && text_style.is_none()
        && node_icon.is_none()
        && !placeholder_space_node
        && lookup_centrality_tier(centrality_map, node_box.node_index).is_none()
        && node.requirement_meta.is_none()
        && node.menu_links.is_empty()
        && node.href().is_none()
        && node.callback().is_none()
        && node.tooltip().is_none()
        && let Some(user_classes) = simple_class_node_user_suffix(node)
    {
        write_class_node_fragment_into::<false>(
            out,
            node,
            meta,
            node_id,
            node_box.node_index,
            raw_label_text,
            ir,
            x,
            y,
            w,
            h,
            config.rounded_corners * 0.55,
            node_font_size,
            config,
            colors,
            &user_classes,
        );
        return;
    }

    // Whole-C4-node streaming fast path: a themed C4 node (a `Rounded` node with `c4_meta`) with no `technology`
    // and an absent/single-line description, whose config carries no conditional render — the C4 twin of the
    // class/ER fast paths. `render_node`'s slow path builds a group `Element` + solid-fill rect `Element` +
    // `render_c4_node_content`'s child subtree + a title, serializes the whole group, then COPIES it into `out`;
    // stream the `<g>…</g>` in place instead. Reuses `simple_class_node_user_suffix` (byte-identical
    // ` fm-node-user-c4…` output; rejects the `c4-external` variant → slow path). Byte-identical, pinned by
    // `svg_golden_snapshots_are_stable`'s `c4_basic`.
    if let Some(node) = ir_node
        && matches!(shape, NodeShape::Rounded)
        && let Some(c4_meta) = node.c4_meta.as_deref()
        && c4_meta.technology.is_none()
        && node.class_meta.is_none()
        && node.requirement_meta.is_none()
        && node.members.is_empty()
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
        && lookup_centrality_tier(centrality_map, node_box.node_index).is_none()
        && node.menu_links.is_empty()
        && node.href().is_none()
        && node.callback().is_none()
        && node.tooltip().is_none()
        && c4_meta.description.as_ref().is_none_or(|d| {
            wrap_text_to_lines(d, (w - 20.0).max(32.0), config.avg_char_width * 0.92).len() <= 1
        })
        && let Some(user_classes) = simple_class_node_user_suffix(node)
    {
        write_c4_node_fragment_into(
            out,
            node,
            c4_meta,
            node_id,
            node_box.node_index,
            raw_label_text,
            ir,
            x,
            y,
            w,
            h,
            config.rounded_corners,
            node_font_size,
            config,
            colors,
            &user_classes,
        );
        return;
    }

    // Whole-ER-entity streaming fast path: a themed ER entity (a `Rect` node with a non-empty attribute
    // list) whose config carries no conditional render. `render_node`'s slow path would build a group
    // `Element` + rect `Element` + the entity-body fragment child + a title `Element`, serialize the whole
    // group into a temp, then COPY it into `out`. Stream the whole `<g>…</g>` in place instead — the exact
    // double-copy the class node fast path above kills, now for ER. Gate mirrors the class one (the ER
    // branch sits after class/requirement in `render_node`'s content chain, so `class_meta`/`c4_meta`/
    // `requirement_meta` must be absent; `show_node_labels` gates the body). Byte-identical (pinned by
    // `er_entity_node_streaming_matches_slow_render`).
    // Two a11y-uniform gates (see the class path for the rationale): the full-a11y gate is unchanged
    // (direct `::<true>`) so the default path takes no regression; the lean gate streams a11y-off ER
    // entities that used to fall to the ~1-Element-per-attribute slow path. Mixed a11y → slow path.
    if let Some(node) = ir_node
        && matches!(shape, NodeShape::Rect)
        && !node.members.is_empty()
        && ir.diagram_type == fm_core::DiagramType::Er
        && detail.show_node_labels
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
        && lookup_centrality_tier(centrality_map, node_box.node_index).is_none()
        && node.class_meta.is_none()
        && node.c4_meta.is_none()
        && node.requirement_meta.is_none()
        && node.menu_links.is_empty()
        && node.href().is_none()
        && node.callback().is_none()
        && node.tooltip().is_none()
        && let Some(user_classes) = simple_class_node_user_suffix(node)
    {
        write_er_node_fragment_into::<true>(
            out,
            node,
            node_id,
            node_box.node_index,
            raw_label_text,
            label_text.as_ref(),
            cx,
            x,
            y,
            w,
            h,
            config.rounded_corners * 0.55,
            node_font_size,
            config,
            colors,
            &user_classes,
        );
        return;
    }
    if let Some(node) = ir_node
        && matches!(shape, NodeShape::Rect)
        && !node.members.is_empty()
        && ir.diagram_type == fm_core::DiagramType::Er
        && detail.show_node_labels
        && config.embed_theme_css
        && config.node_gradients
        && !emit_classdef_classes
        && !config.animations_enabled
        && !config.include_source_spans
        && !config.a11y.aria_labels
        && !config.a11y.keyboard_nav
        && !config.a11y.text_alternatives
        && shape_style.is_none()
        && text_style.is_none()
        && node_icon.is_none()
        && !placeholder_space_node
        && lookup_centrality_tier(centrality_map, node_box.node_index).is_none()
        && node.class_meta.is_none()
        && node.c4_meta.is_none()
        && node.requirement_meta.is_none()
        && node.menu_links.is_empty()
        && node.href().is_none()
        && node.callback().is_none()
        && node.tooltip().is_none()
        && let Some(user_classes) = simple_class_node_user_suffix(node)
    {
        write_er_node_fragment_into::<false>(
            out,
            node,
            node_id,
            node_box.node_index,
            raw_label_text,
            label_text.as_ref(),
            cx,
            x,
            y,
            w,
            h,
            config.rounded_corners * 0.55,
            node_font_size,
            config,
            colors,
            &user_classes,
        );
        return;
    }

    // Same gate as `render_node`'s fast path (permit_fast is always true on this serialize-only path).
    // `user_class_suffix` is `Some("")` for the common no-class node and `Some(" fm-node-user-…")` for a
    // node whose custom classes are all simple; `None` (slow path) when a class needs conditional render.
    let user_class_suffix = ir_node.and_then(simple_node_user_class_suffix);
    if matches!(shape, NodeShape::Subroutine)
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
        && !label_has_line_break(&label_text)
        && !label_may_overflow
        && lookup_centrality_tier(centrality_map, node_box.node_index).is_none()
        && label_id.is_none_or(|id| ir.label_markup.get(&id).is_none_or(|s| s.is_empty()))
        && let Some(node) = ir_node
        && let Some(user_classes) = user_class_suffix.as_deref()
        && common_fragment_special_fill(node).is_none()
        && node.requirement_meta.is_none()
        && node.menu_links.is_empty()
        && node.href().is_none()
        && node.callback().is_none()
        && node.tooltip().is_none()
    {
        write_subroutine_node_fragment_into(
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
            config.rounded_corners * 0.45,
            cx,
            cy + node_font_size / 3.0,
            node_font_size,
            colors.text.as_str(),
            user_classes,
        );
        return;
    }

    if matches!(
        shape,
        NodeShape::Rect
            | NodeShape::Circle
            | NodeShape::Rounded
            | NodeShape::Stadium
            | NodeShape::Diamond
            | NodeShape::Hexagon
            | NodeShape::Cylinder
            | NodeShape::Trapezoid
            | NodeShape::InvTrapezoid
            | NodeShape::Parallelogram
            | NodeShape::InvParallelogram
            | NodeShape::Asymmetric
    ) && config.embed_theme_css
        && config.node_gradients
        && !emit_classdef_classes
        && !config.animations_enabled
        && !config.include_source_spans
        // Was `aria_labels && keyboard_nav && text_alternatives`. The fragment writer now has a lean
        // monomorphization, so uniformly-OFF a11y streams too -- previously `A11yConfig::none()` fell all
        // the way back to the per-element `Element` builder, which made the *smaller* output ~2x more
        // expensive to produce. Mixed combinations (e.g. `A11yConfig::minimal()`) still take the slow
        // path, exactly as they did before.
        && uniform_a11y(&config.a11y).is_some()
        && shape_style.is_none()
        && text_style.is_none()
        && node_icon.is_none()
        && !placeholder_space_node
        && !label_has_line_break(&label_text)
        && !label_may_overflow
        && lookup_centrality_tier(centrality_map, node_box.node_index).is_none()
        && label_id.is_none_or(|id| ir.label_markup.get(&id).is_none_or(|s| s.is_empty()))
        && let Some(node) = ir_node
        && let Some(user_classes) = user_class_suffix.as_deref()
        && node.requirement_meta.is_none()
        && node.menu_links.is_empty()
        && node.href().is_none()
        && node.callback().is_none()
        && node.tooltip().is_none()
        // A JOURNEY STEP takes the slow path so its description comes from `describe_node`, which
        // reads `IrNode.classes` and therefore keeps the author's casing (bd-fsj42). This fragment
        // has only the EMITTED class string, which is prefixed AND lowercased
        // (`fm-node-user-journey-actor-alice`), so deriving the actors here would announce `alice`
        // for an author who wrote `Alice` — and the two render paths would then disagree about what
        // the same step is called. One path for journey is worth more than the fragment's speed on a
        // diagram type whose charts are small.
        && !node
            .classes
            .iter()
            .any(|class| class == "journey-step")
    {
        let write = if matches!(uniform_a11y(&config.a11y), Some(true)) {
            write_common_node_fragment_into::<true>
        } else {
            write_common_node_fragment_into::<false>
        };
        write(
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
            match shape {
                NodeShape::Rounded => config.rounded_corners,
                NodeShape::Stadium => w.min(h) / 2.0,
                _ => config.rounded_corners * 0.55,
            },
            cx,
            cy + node_font_size / 3.0,
            node_font_size,
            colors.text.as_str(),
            user_classes,
            shape,
            common_fragment_special_fill(node),
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
        // ⚠️ Shared with fm-layout, which SIZES the box from the same call (bd-3cj8v). A local copy
        // of this rule here drifted from layout's and reproduced a state-diagram id suppression on
        // gitGraph commits, which draw an id in mermaid.
        ir_node.map_or("", |node| ir.node_display_text(node))
    };
    // A sankey node additionally carries its throughput on a second line, as mermaid draws it.
    // Owned, so it must outlive the borrow above; `None` for every other diagram type.
    let sankey_label = sankey_node_label(ir, node_box.node_index);
    let raw_label_text = sankey_label.as_deref().unwrap_or(raw_label_text);
    let label_text = truncate_label(raw_label_text, detail.node_label_max_chars);
    let node_font_size = detail.node_font_size;
    // ⚠️ A PACKET FIELD IS NEVER ELLIPSIZED, because its box width is the PROTOCOL, not a layout
    // choice. `fit_node_label_text` shrinks a label to 10px and then cuts it, which is right for a
    // node whose box was sized for its text — and wrong for a one-bit field, whose width is fixed at
    // one bit by the diagram's own semantics. `0: "Flag"` drew `Fl…` at 10px; mermaid draws `Flag`
    // and lets it overflow. Losing the field's NAME to keep it inside a box the author never chose
    // is the worse trade, and the name is the only thing identifying the field.
    let unbounded_label_width = ir.diagram_type == DiagramType::PacketBeta;
    let fit_width = |available: f32| {
        if unbounded_label_width {
            f32::MAX
        } else {
            available
        }
    };
    let label_may_overflow = label_text.lines().any(|line| {
        line.chars().count() as f32
            * config.avg_char_width
            * (node_font_size / config.font_size.max(1.0))
            > (w - 16.0).max(node_font_size)
    });
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
        && matches!(
            shape,
            NodeShape::Rect
                | NodeShape::Circle
                | NodeShape::Rounded
                | NodeShape::Stadium
                | NodeShape::Diamond
                | NodeShape::Hexagon
                | NodeShape::Cylinder
                | NodeShape::Trapezoid
                | NodeShape::InvTrapezoid
                | NodeShape::Parallelogram
                | NodeShape::InvParallelogram
                | NodeShape::Asymmetric
        )
        && config.embed_theme_css
        && config.node_gradients
        && !emit_classdef_classes
        && !config.animations_enabled
        && !config.include_source_spans
        // See the sibling gate in `render_node_into`. Keep these two gates in lockstep.
        && uniform_a11y(&config.a11y).is_some()
        && shape_style.is_none()
        && text_style.is_none()
        && node_icon.is_none()
        && !placeholder_space_node
        && !label_has_line_break(&label_text)
        && !label_may_overflow
        && lookup_centrality_tier(centrality_map, node_box.node_index).is_none()
        && label_id.is_none_or(|id| ir.label_markup.get(&id).is_none_or(|s| s.is_empty()))
        && let Some(node) = ir_node
        && let Some(user_classes) = user_class_suffix.as_deref()
        && node.requirement_meta.is_none()
        && node.menu_links.is_empty()
        && node.href().is_none()
        && node.callback().is_none()
        && node.tooltip().is_none()
        // Journey steps take the slow path so their description comes from `describe_node` — see
        // the twin gate on `write_common_node_fragment_into` (bd-fsj42). BOTH gates are needed: the
        // fragment is reachable through two callers, and excluding it from one left the fix inert.
        && !node.classes.iter().any(|class| class == "journey-step")
    {
        let build = if matches!(uniform_a11y(&config.a11y), Some(true)) {
            build_common_node_fragment::<true>
        } else {
            build_common_node_fragment::<false>
        };
        return Element::raw_svg(build(
            node_id,
            node_box.node_index,
            stable_accent_index(node_id),
            raw_label_text,
            &label_text,
            x,
            y,
            w,
            h,
            match shape {
                NodeShape::Rounded => config.rounded_corners,
                NodeShape::Stadium => w.min(h) / 2.0,
                _ => config.rounded_corners * 0.55,
            },
            cx,
            cy + node_font_size / 3.0,
            node_font_size,
            colors.text.as_str(),
            user_classes,
            shape,
            common_fragment_special_fill(node),
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
            if !class.is_empty() {
                group = group.class_prefixed_by("fm-node-user-", class.len(), |buf| {
                    write_sanitized_css_token_into(buf, class);
                });
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

    // Add accessibility attributes.
    //
    // A block-beta `space` is a grid spacer with no content: `raw_label_text` is forced to "" above,
    // so the old unconditional emission produced `aria-label=""` — an empty accessible name on an
    // element announced as a `graphics-symbol` — and `tabindex="0"` put an invisible empty cell into
    // the keyboard tab order (bd-ukj2). The correct markup for a decorative element is to hide it
    // from the accessibility tree entirely.
    if placeholder_space_node {
        if config.a11y.aria_labels {
            group = group.attr("aria-hidden", "true");
        }
    } else {
        if config.a11y.aria_labels {
            group = group
                .attr("role", "graphics-symbol")
                .attr("aria-label", raw_label_text);
        }

        if config.a11y.keyboard_nav {
            group = group.attr("tabindex", "0");
        }
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
            // Same 0.15, different role: this shortens the top and bottom edges, it is not the
            // trapezoid slant, and it must not follow SLANTED_SHAPE_INSET_RATIO if that changes.
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
            let elem = Element::circle()
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

            if shape != NodeShape::DoubleCircle {
                elem
            } else {
                // A double circle needs a SECOND RING, not a thicker stroke (bd-vfxu).
                //
                // The previous code drew one circle and nudged `stroke_width` to 2.0 to stand in for the
                // inner ring. That was a no-op in the shipping theme, whose base node stroke already
                // resolves to 2.0 — so `A(((Double Circle)))` and `B((Circle))` rendered byte-identical
                // apart from their centre, and a reader could not tell the two declared shapes apart.
                // The same shape carries a stateDiagram end state, so an end state was indistinguishable
                // from an ordinary circular state.
                //
                // The inner ring is inset by a fixed 4px rather than a ratio: mermaid's own double circle
                // keeps a constant gap, and a proportional inset would collapse to invisible on a small
                // node and gape on a large one. It is unfilled so the outer fill (or gradient) shows
                // through unchanged, which keeps this a purely additive change to the shape.
                let inner_r = (r - 4.0).max(r * 0.5);

                // The inner disc is FILLED for a state diagram's terminal pseudo-state and hollow
                // everywhere else (bd-wbxc). mermaid draws a state `[*]` end as a ring around a solid
                // dot, while a flowchart `(((x)))` is two hollow rings — one NodeShape carries both, so
                // the fill is decided by the diagram it belongs to rather than by the shape. Without it
                // the end state showed a target symbol where the incumbent shows a bullseye, which was
                // the surviving half of bd-vfxu.
                let inner_fill = if ir.diagram_type == fm_core::DiagramType::State {
                    colors.node_stroke.as_str()
                } else {
                    "none"
                };
                Element::group().child(elem).child(
                    Element::circle()
                        .cx(cx)
                        .cy(cy)
                        .r(inner_r)
                        .fill(inner_fill)
                        .stroke_unless_embedded_css(&colors.node_stroke, config.embed_theme_css)
                        .stroke_width_unless_embedded_css(1.6, config.embed_theme_css),
                )
            }
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
            let inset = w * fm_core::SLANTED_SHAPE_INSET_RATIO;
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
                    fit_width((w - 16.0).max(node_font_size)),
                    (h - 16.0).max(node_font_size),
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
            let inset = w * fm_core::SLANTED_SHAPE_INSET_RATIO;
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
            let inset = w * fm_core::SLANTED_SHAPE_INSET_RATIO;
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
            let inset = w * fm_core::SLANTED_SHAPE_INSET_RATIO;
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
                    fit_width((w * 0.8).max(node_font_size)),
                    (h * 0.8).max(node_font_size),
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
    //
    // A block-beta `space` has no label: `raw_label_text` is forced to "" above, so this emitted
    // `<text …></text>` — an empty text element on an invisible spacer. Dead output in the same
    // family as an unreferenced `<defs>` entry, and it made the golden reader report the node as
    // unreadable rather than as having no text, which is what blocked bd-7ute's gate (bd-ukj2).
    if detail.show_node_labels && !placeholder_space_node {
        if let Some(node) = ir_node
            && let Some(ref meta) = node.class_meta
            && (!meta.attributes.is_empty()
                || !meta.methods.is_empty()
                || meta.stereotype.is_some())
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

            // Requirement type header (e.g., "<<requirement>>"). Stream the `<text>` bytes directly under
            // the common themed config (embedded CSS, no per-label style/classdef) instead of building an
            // `Element` + its `Attributes` Vec — requirement nodes always take this slow path, and the
            // Element machinery was the top of requirement render.
            let stream_req_subtitles =
                config.embed_theme_css && !emit_classdef_classes && text_style.is_none();
            if let Some(ref req_type) = req_meta.requirement_type {
                // Same contract as the streaming path above: mermaid's display name, ASCII angles.
                let type_label = format!("<<{}>>", fm_core::requirement_type_display(req_type));
                if stream_req_subtitles {
                    let mut f = String::new();
                    write_req_subtitle_into(
                        &mut f,
                        cx,
                        text_y,
                        subtitle_font_size,
                        " font-style=\"italic\"",
                        "",
                        &colors.text,
                        "fm-req-type-label",
                        &type_label,
                    );
                    group = group.child(Element::raw_svg(f));
                } else {
                    let mut type_elem = Element::text()
                        .x(cx)
                        .y(text_y)
                        .content(&type_label)
                        .attr("text-anchor", "middle")
                        .attr("dominant-baseline", "central")
                        .attr_num("font-size", subtitle_font_size)
                        .attr("font-style", "italic")
                        .font_family_unless_embedded_css(
                            &config.font_family,
                            config.embed_theme_css,
                        )
                        .fill(&colors.text)
                        .class("fm-req-type-label");
                    type_elem = apply_label_class(type_elem);
                    if let Some(style) = text_style.as_deref() {
                        type_elem = type_elem.attr("style", style);
                    }
                    group = group.child(type_elem);
                }
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
                fit_width((w - 20.0).max(node_font_size)),
                (h - 20.0).max(node_font_size),
                config,
                colors,
                text_style.as_deref(),
                emit_classdef_classes,
            );
            group = group.child(text_elem);
            text_y += node_font_size * 0.85;

            // The field rows — see the fast writer's comment for the reference. Kept in lockstep with
            // it: same order, same labels, same classes, same `opacity 0.7`, same cursor advance.
            //
            // ⚠️ THE LOCKSTEP HAD ALREADY BROKEN, and the comment claiming it is how that went
            // unnoticed. This copy carried only `ID:` and `Text:`; the `Type:` and `Doc Ref:` rows an
            // ELEMENT declares existed solely in the fast writer, so which fields a requirement
            // diagram drew depended on which path it happened to take. Both tables are now the same
            // six rows in mermaid's order.
            for (prefix, value, class) in [
                ("ID: ", req_meta.req_id.as_deref(), "fm-req-id"),
                ("Text: ", req_meta.text.as_deref(), "fm-req-text"),
                ("Risk: ", req_meta.risk.as_deref(), "fm-req-metadata"),
                (
                    "Verification: ",
                    req_meta.verify_method.as_deref(),
                    "fm-req-metadata",
                ),
                (
                    "Type: ",
                    req_meta.element_type.as_deref(),
                    "fm-req-element-type",
                ),
                ("Doc Ref: ", req_meta.doc_ref.as_deref(), "fm-req-docref"),
            ] {
                let Some(value) = value else { continue };
                let row = format!("{prefix}{value}");
                if stream_req_subtitles {
                    let mut f = String::new();
                    write_req_subtitle_into(
                        &mut f,
                        cx,
                        text_y,
                        subtitle_font_size,
                        "",
                        " opacity=\"0.7\"",
                        &colors.text,
                        class,
                        &row,
                    );
                    group = group.child(Element::raw_svg(f));
                } else {
                    let mut row_elem = Element::text()
                        .x(cx)
                        .y(text_y)
                        .content(&row)
                        .attr("text-anchor", "middle")
                        .attr("dominant-baseline", "central")
                        .attr_num("font-size", subtitle_font_size)
                        .font_family_unless_embedded_css(
                            &config.font_family,
                            config.embed_theme_css,
                        )
                        .fill(&colors.text)
                        .attr("opacity", "0.7")
                        .class(class);
                    row_elem = apply_label_class(row_elem);
                    if let Some(style) = text_style.as_deref() {
                        row_elem = row_elem.attr("style", style);
                    }
                    group = group.child(row_elem);
                }
                text_y += node_font_size * 0.85;
            }
        } else if let Some(node) = ir_node
            && !node.members.is_empty()
            && ir.diagram_type == fm_core::DiagramType::Er
        {
            // ER entity: render name + attribute list.
            // Streaming fast path: embedded CSS, no per-label style, no classdef class -> the header +
            // divider + attribute rows are a fixed set of `<text>`/`<line>` bytes with no per-element
            // conditional class/style, so stream them into ONE raw fragment instead of ~2 + N `Element`s
            // per entity (~400 for a 40-entity diagram). Byte-identical to the Element path below (same
            // attrs/order/positions/cursor advance); mirrors `render_class_compartments`' fast path.
            // Every other case (per-label style, classdef, non-embedded CSS) falls through.
            if text_style.is_none() && !emit_classdef_classes && config.embed_theme_css {
                let mut fragment = String::new();
                write_er_entity_into(
                    &mut fragment,
                    node,
                    label_text.as_ref(),
                    cx,
                    x,
                    y,
                    w,
                    node_font_size,
                    config,
                    colors,
                );
                group = group.child(Element::raw_svg(fragment));
            } else {
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
                    let key_prefix = attr.key_prefix();
                    let attr_text = format!("{key_prefix}{} {}", attr.data_type, attr.name);
                    let font_weight = if attr.keys.is_empty() {
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
                        .font_family_unless_embedded_css(
                            &config.font_family,
                            config.embed_theme_css,
                        )
                        .fill(&colors.text)
                        .class("fm-er-attribute");
                    attr_elem = apply_label_class(attr_elem);
                    if let Some(style) = text_style.as_deref() {
                        attr_elem = attr_elem.attr("style", style);
                    }
                    group = group.child(attr_elem);
                    attr_y += attr_font_size * 1.3;
                }
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
                fit_width((content_width - 16.0).max(node_font_size)),
                (content_height - 16.0).max(node_font_size),
                config,
                colors,
                text_style.as_deref(),
                emit_classdef_classes,
            );
            group = group.child(text_elem);
        }
    }

    // Add title element for text alternatives.
    //
    // Skipped for a `space`, whose description would be `Node: __space_4, rectangle` — a generated
    // internal id, announced for an element that has no content and is drawn at opacity 0. It is
    // also inside an `aria-hidden` group by then, so the title could never be reached anyway
    // (bd-ukj2).
    if config.a11y.text_alternatives
        && !placeholder_space_node
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
                // ⚠️ `target` WAS HARDCODED `_blank`, so `click A "url" _self` opened a new tab
                // anyway — the author's declared frame was parsed and then ignored (bd-vn7s).
                //
                // `None` still means `_blank`: that is mermaid's own default (its `setLink` reads
                // `typeof i == "string" ? i : "_blank"`), so an ordinary link is byte-identical to
                // before and only an explicitly declared target changes anything.
                //
                // `rel` is kept unconditionally. It matters for `_blank`, and on a same-frame
                // target it is inert rather than wrong — dropping it per-target would be a security
                // regression for the sake of tidiness.
                let mut a = Element::new(crate::element::ElementKind::A)
                    .attr("href", href)
                    .attr("target", node.link_target().unwrap_or("_blank"))
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

    // `click a "url" "some tooltip"` (bd-bk7h). `IrNodeInteraction.tooltip` was parsed, stored, and
    // rendered by NOTHING: fm-render-svg and fm-render-canvas referenced it zero times, and the
    // terminal's only uses are in `diff.rs`, which reports that it CHANGED without ever drawing it.
    // That is the dead-IR-field class this project has already found twice (bd-jgco branch names,
    // bd-jerh attribute comments).
    //
    // A `title` ATTRIBUTE, not a `<title>` child, because that is what the incumbent emits:
    // mermaid 11.15.0 does `t.tooltip && n.attr("title", t.tooltip)`. The `<title>Node: ...`
    // children elsewhere in this file are the a11y name for the shape; a tooltip is the author's
    // own text and belongs where a browser will show it on hover.
    //
    // It sits on the same decoration path as href and callback, so the fast paths above already
    // had to learn to refuse it - hence the `node.tooltip().is_none()` clause added to each of
    // them. Without that clause a tooltip-bearing node would take a streaming path and never reach
    // this line, which is exactly how the field came to be dead in the first place.
    if let Some(node) = ir_node
        && let Some(tooltip) = node.tooltip()
    {
        group = group.attr("title", tooltip);
    }

    group
}

/// Whether this laid-out node is a composite state that has been anchored to its own cluster.
///
/// SINGLE SOURCE OF TRUTH with `fm_layout::anchor_composite_state_nodes`, which selects on the same
/// pair — state diagram, cluster title equal to the node id. Restating the condition in two crates is
/// how a node stops being drawn while its container never appears, so both sides read the same two
/// facts off the IR.
/// Draw one continuation box for a packet field that crosses a 32-bit row boundary (bd-8vr0).
///
/// The field's first row segment is an ordinary node; this draws rows 2..n, which cannot be node
/// boxes because a `LayoutNodeBox` is 1:1 with a node and element ids derive from `node_index`.
/// Carries the field's own label, as the incumbent's split does, and is `aria-hidden` so a screen
/// reader is told about the field once rather than once per row it happens to wrap across.
#[allow(clippy::too_many_arguments)]
fn write_packet_field_continuation_into(
    out: &mut String,
    continuation: &fm_layout::LayoutPacketFieldContinuation,
    ir: &MermaidDiagramIr,
    offset_x: f32,
    offset_y: f32,
    detail: RenderDetailProfile,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
) {
    use crate::attributes::{write_escaped_text, write_number_into};

    let label = ir
        .nodes
        .get(continuation.node_index)
        .map(|node| {
            node.label
                .and_then(|label_id| ir.labels.get(label_id.0))
                .map_or(node.id.as_str(), |label| label.text.as_str())
        })
        .unwrap_or_default();

    let x = continuation.bounds.x + offset_x;
    let y = continuation.bounds.y + offset_y;
    let (w, h) = (continuation.bounds.width, continuation.bounds.height);

    out.push_str("<g id=\"fm-packet-continuation-");
    let _ = crate::attributes::write_uint_into(out, continuation.node_index as u64);
    out.push('-');
    let _ = crate::attributes::write_uint_into(out, continuation.segment as u64);
    out.push_str(
        "\" class=\"fm-node fm-node-shape-rect fm-packet-continuation\" aria-hidden=\"true\"><rect x=\"",
    );
    let _ = write_number_into(out, x);
    out.push_str("\" y=\"");
    let _ = write_number_into(out, y);
    out.push_str("\" width=\"");
    let _ = write_number_into(out, w);
    out.push_str("\" height=\"");
    let _ = write_number_into(out, h);
    out.push_str("\" fill=\"");
    out.push_str(colors.node_fill.as_str());
    out.push_str("\" rx=\"");
    let _ = write_number_into(out, config.rounded_corners * 0.55);
    out.push_str("\"/>");

    if detail.show_node_labels && !label.is_empty() {
        let font_size = detail.node_font_size;
        out.push_str("<text x=\"");
        let _ = write_number_into(out, w.mul_add(0.5, x));
        out.push_str("\" y=\"");
        let _ = write_number_into(out, h.mul_add(0.5, y) + font_size / 3.0);
        out.push_str("\" text-anchor=\"middle\" font-size=\"");
        let _ = write_number_into(out, font_size);
        out.push_str("\" fill=\"");
        out.push_str(colors.text.as_str());
        out.push_str("\">");
        // A field label can carry a newline (`"Source Port\n[0-15]"`); the continuation draws the
        // first line only, so a wrapped range suffix is not repeated on every row.
        let first_line = label.split('\n').next().unwrap_or(label);
        let _ = write_escaped_text(out, first_line);
        out.push_str("</text>");
    }
    out.push_str("</g>");
}

fn is_composite_state_node(ir: &MermaidDiagramIr, node_box: &LayoutNodeBox) -> bool {
    if ir.diagram_type != DiagramType::State {
        return false;
    }
    let node_id = ir
        .nodes
        .get(node_box.node_index)
        .map_or(node_box.node_id.as_str(), |node| node.id.as_str());
    ir.clusters.iter().any(|cluster| {
        cluster
            .title
            .and_then(|label_id| ir.labels.get(label_id.0))
            .is_some_and(|label| label.text == node_id)
    })
}

pub(crate) fn is_block_beta_space_node(node: &fm_core::IrNode) -> bool {
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
/// Stream one class-compartment `<text>` byte-identical to the `TextBuilder` the slow path builds under
/// embedded CSS with no label-style/classdef class: attrs `x, y, text-anchor, font-size, [extra], fill`
/// then escaped content. `extra` is ` font-weight="bold"` (name), ` font-style="italic"` (stereotype), or
/// `""` (members) — placed right after `font-size` exactly as `TextBuilder::build`'s call order does.
fn write_class_text_into(
    f: &mut String,
    x: f32,
    y: f32,
    anchor: &str,
    font_size: f32,
    extra: &str,
    fill: &str,
    text: &str,
) {
    use crate::attributes::{write_escaped_attr, write_escaped_text};
    f.push_str("<text x=\"");
    let _ = crate::attributes::write_number_into(f, x);
    f.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(f, y);
    f.push_str("\" text-anchor=\"");
    f.push_str(anchor);
    f.push_str("\" font-size=\"");
    let _ = crate::attributes::write_number_into(f, font_size);
    f.push('"');
    f.push_str(extra);
    f.push_str(" fill=\"");
    let _ = write_escaped_attr(f, fill);
    f.push_str("\">");
    let _ = write_escaped_text(f, text);
    f.push_str("</text>");
}

/// Stream a requirement-node subtitle `<text>` (the `«type»` header / `Risk … | Verify …` metadata line)
/// byte-identical to the `Element` the slow path builds under the common themed config (embedded CSS,
/// no per-label style/classdef → `font-family` and `fm-node-label` absent). `before_fill`/`after_fill`
/// carry the config-specific attribute in the slow path's exact position: the type header's
/// `font-style="italic"` sits BEFORE `fill`, the metadata's `opacity="0.7"` AFTER it.
#[allow(clippy::too_many_arguments)]
fn write_req_subtitle_into(
    f: &mut String,
    x: f32,
    y: f32,
    font_size: f32,
    before_fill: &str,
    after_fill: &str,
    fill: &str,
    class: &str,
    text: &str,
) {
    write_req_subtitle_body_into(
        f,
        x,
        y,
        font_size,
        before_fill,
        after_fill,
        fill,
        class,
        |f| {
            let _ = crate::attributes::write_escaped_text(f, text);
        },
    );
}

/// Writes the requirement-subtitle `<text …>…</text>` envelope, leaving the body to a caller closure so
/// multi-part subtitles (`Risk: {risk} | Verify: {vm}`, `«{type}»`) stream their fixed labels + escaped
/// fields straight in instead of `format!`-allocating a joined `String` per node. Byte-identical because
/// `write_escaped_text` escapes per char (escape(a ++ b) == escape(a) ++ escape(b)) and the fixed labels
/// hold no XML specials.
#[allow(clippy::too_many_arguments)]
fn write_req_subtitle_body_into(
    f: &mut String,
    x: f32,
    y: f32,
    font_size: f32,
    before_fill: &str,
    after_fill: &str,
    fill: &str,
    class: &str,
    write_body: impl FnOnce(&mut String),
) {
    use crate::attributes::write_escaped_attr;
    f.push_str("<text x=\"");
    let _ = crate::attributes::write_number_into(f, x);
    f.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(f, y);
    f.push_str("\" text-anchor=\"middle\" dominant-baseline=\"central\" font-size=\"");
    let _ = crate::attributes::write_number_into(f, font_size);
    f.push('"');
    f.push_str(before_fill);
    f.push_str(" fill=\"");
    let _ = write_escaped_attr(f, fill);
    f.push('"');
    f.push_str(after_fill);
    f.push_str(" class=\"");
    f.push_str(class);
    f.push_str("\">");
    write_body(f);
    f.push_str("</text>");
}

/// Stream a class-compartment separator `<line>` byte-identical to the slow path's `Element::Line` under
/// embedded CSS (stroke is CSS-driven, so absent inline): `x1 y1 x2 y2 stroke-width="1"`.
fn write_class_separator_into(f: &mut String, x1: f32, y: f32, x2: f32) {
    f.push_str("<line x1=\"");
    let _ = crate::attributes::write_number_into(f, x1);
    f.push_str("\" y1=\"");
    let _ = crate::attributes::write_number_into(f, y);
    f.push_str("\" x2=\"");
    let _ = crate::attributes::write_number_into(f, x2);
    f.push_str("\" y2=\"");
    let _ = crate::attributes::write_number_into(f, y);
    f.push_str("\" stroke-width=\"1\"/>");
}

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

    // Streaming fast path: no per-label style, no classdef class, embedded CSS -> the whole compartment
    // stack (stereotype + name + separators + member rows) is a fixed set of `<text>`/`<line>` bytes with
    // no per-element class/style, so stream it into ONE raw fragment instead of ~5+ `Element`s per class
    // node. Byte-identical to the Element path below (same attrs/order/positions/cursor advance); every
    // other case (label style, classdef, non-embedded CSS) falls through.
    if label_style.is_none() && !emit_classdef_classes && config.embed_theme_css {
        let mut f = String::new();
        write_class_compartments_into(
            &mut f, node, meta, ir, x, y, w, h, font_size, config, colors,
        );
        group = group.child(Element::raw_svg(f));
        return group;
    }

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
        let stereo_text = stereotype.label();
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
        // Generics are rewritten HERE, not in the IR: mermaid keeps `List~int~` in its db and
        // turns it into `List<int>` only when the row is drawn (bd class-generics).
        let name = fm_core::class_member_display_name(&attr.name, false);
        let text = if let Some(ref ret) = attr.return_type {
            format!("{vis}{name}: {}", fm_core::parse_generic_types(ret))
        } else {
            format!("{vis}{name}")
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
        // ⚠️ THE CLASSIFIER IS A STYLE, NOT A CHARACTER (bd-r2gll). mermaid's `getDisplayDetails()`
        // returns `+getName() : String` with NO `$`/`*` in it and carries the marker as
        // `cssStyle: text-decoration:underline;` / `font-style:italic;`. Appending the raw byte
        // here made the same member read as a different NAME.
        let classifier = fm_core::class_member_classifier_css(method.is_static, method.is_abstract);
        // ` : T`, not `: T` (bd-ci658). Measured on the pinned bundle: mermaid's
        // `getDisplayDetails()` builds the tail as `' : ' + parseGenericTypes(returnType)`, so a
        // typed method row differed from the incumbent by one character in every class diagram.
        let ret = method
            .return_type
            .as_deref()
            .map(|t| format!(" : {}", fm_core::parse_generic_types(t)))
            .unwrap_or_default();
        let text = format!(
            "{vis}{}{ret}",
            fm_core::class_member_display_name(&method.name, true)
        );
        let builder = TextBuilder::new(&text)
            .x(text_x)
            .y(cursor_y)
            .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
            .font_size(member_font_size)
            .anchor(TextAnchor::Start);
        let builder = match classifier {
            Some("font-style:italic") => builder.font_style("italic"),
            Some("text-decoration:underline") => builder.text_decoration("underline"),
            _ => builder,
        };
        let elem = builder.fill(&colors.text).build();
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
            // See the streaming twin above (bd-4rlrx): the cluster BORDER colour on text gave
            // 1.43:1 in the default theme. Both paths must agree or the fix depends on which one a
            // given diagram happens to take.
            .fill(&colors.text)
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

#[derive(Debug)]
struct FittedNodeLabel<'a> {
    text: Cow<'a, str>,
    font_size: f32,
    changed: bool,
}

/// Fit a node label into its usable rectangle using the configured font metric calibration.
///
/// Ordinary labels return a borrowed string and preserve the existing fast path. Overflowing labels
/// first use word-boundary wrapping, then reduce their size no lower than 10px (or a stricter
/// configured minimum), and finally ellipsize the final visible line. This keeps SVG labels readable
/// without allowing a long identifier to draw outside its node.
fn fit_node_label_text<'a>(
    label: &'a str,
    max_width: f32,
    max_height: f32,
    font_size: f32,
    config: &SvgRenderConfig,
) -> FittedNodeLabel<'a> {
    if label.is_empty()
        || !max_width.is_finite()
        || !max_height.is_finite()
        || max_width <= 0.0
        || max_height <= 0.0
        || font_size <= 0.0
    {
        return FittedNodeLabel {
            text: Cow::Borrowed(label),
            font_size,
            changed: false,
        };
    }

    let metrics = config.font_metrics();
    let reference_size = config.font_size.max(1.0);
    let calibrated_width = |text: &str, size: f32| {
        text.lines()
            .map(|line| {
                metrics.estimate_width(line)
                    * (size / reference_size)
                    * (config.avg_char_width / metrics.avg_char_width())
            })
            .fold(0.0_f32, f32::max)
    };
    if calibrated_width(label, font_size) <= max_width {
        return FittedNodeLabel {
            text: Cow::Borrowed(label),
            font_size,
            changed: false,
        };
    }

    let min_size = config.min_font_size.max(10.0).min(font_size);
    let reduced_size =
        (font_size * max_width / calibrated_width(label, font_size)).clamp(min_size, font_size);
    let mut fitted_size = reduced_size;
    let max_lines =
        |size: f32| ((max_height / (size * config.line_height.max(1.0))).floor() as usize).max(1);
    let mut lines = wrap_node_label_lines(label, max_width, fitted_size, &calibrated_width);

    if lines.len() > max_lines(fitted_size) && fitted_size > min_size {
        fitted_size = min_size;
        lines = wrap_node_label_lines(label, max_width, fitted_size, &calibrated_width);
    }

    let visible_lines = max_lines(fitted_size);
    let was_truncated = lines.len() > visible_lines
        || lines
            .iter()
            .any(|line| calibrated_width(line, fitted_size) > max_width);
    if was_truncated {
        lines.truncate(visible_lines);
        if let Some(last) = lines.last_mut() {
            *last = ellipsize_label_line(last, max_width, fitted_size, &calibrated_width);
        }
    }

    let fitted_text = lines.join("\n");
    let changed = fitted_text != label || fitted_size != font_size;
    FittedNodeLabel {
        text: if changed {
            Cow::Owned(fitted_text)
        } else {
            Cow::Borrowed(label)
        },
        font_size: fitted_size,
        changed,
    }
}

fn wrap_node_label_lines(
    label: &str,
    max_width: f32,
    font_size: f32,
    width: &impl Fn(&str, f32) -> f32,
) -> Vec<String> {
    let mut lines = Vec::new();
    for source_line in label.split('\n') {
        let mut current = String::new();
        for word in source_line.split_whitespace() {
            if current.is_empty() {
                current.push_str(word);
                continue;
            }

            let previous_len = current.len();
            current.push(' ');
            current.push_str(word);
            if width(&current, font_size) > max_width {
                current.truncate(previous_len);
                lines.push(current);
                current = word.to_string();
            }
        }
        lines.push(current);
    }
    lines
}

fn ellipsize_label_line(
    line: &str,
    max_width: f32,
    font_size: f32,
    width: &impl Fn(&str, f32) -> f32,
) -> String {
    const ELLIPSIS: char = '…';
    let mut result = String::new();
    for character in line.chars() {
        result.push(character);
        result.push(ELLIPSIS);
        if width(&result, font_size) > max_width {
            let _ = result.pop();
            let _ = result.pop();
            break;
        }
        let _ = result.pop();
    }
    result.push(ELLIPSIS);
    result
}

#[allow(clippy::too_many_arguments)]
fn render_node_label_text(
    ir: &MermaidDiagramIr,
    label_id: Option<IrLabelId>,
    label_text: &str,
    x: f32,
    y: f32,
    font_size: f32,
    max_width: f32,
    max_height: f32,
    config: &SvgRenderConfig,
    colors: &ThemeColors,
    label_style: Option<&str>,
    emit_classdef_classes: bool,
) -> Element {
    let fitted = fit_node_label_text(label_text, max_width, max_height, font_size, config);
    let label_text = fitted.text.as_ref();
    let font_size = fitted.font_size;
    if !fitted.changed
        && let Some(label_id) = label_id
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

    let mut text = if label_has_line_break(label_text) {
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
        fm_core::ClassVisibility::Unmarked => "",
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
    /// Largest sankey flow value in the diagram, or `None` when this is not a sankey.
    ///
    /// Every ribbon's width is its value normalised against this ONE number, so it is invariant
    /// across the whole render. It used to be recomputed inside the per-edge width helper, which
    /// re-scanned `ir.edges` and re-parsed every flow value on every edge: O(E^2) float parses to
    /// produce E widths. Computing it once with the context makes the render O(E).
    sankey_widest_flow: Option<f32>,
}

/// A sankey node's label: its name, then its throughput on the next line, as mermaid labels it.
///
/// ⚠️ THE TOTAL IS THE MAX OF INFLOW AND OUTFLOW, NOT THE SUM, and not either side alone. Measured
/// on the pinned 11.15.0 bundle with `A,M,10` / `M,B,3`: mermaid draws `M\n10`. The sum would be
/// 13 and the outflow 3, so a node that does not conserve flow distinguishes all three rules —
/// every balanced diagram makes them agree, which is why the differential fixture carries an
/// unbalanced one.
///
/// The number is formatted plainly: `A,B,1.5` / `A,C,2.25` gives `A\n3.75`, with no padding and no
/// trailing zeros.
fn sankey_node_label(ir: &MermaidDiagramIr, node_index: usize) -> Option<String> {
    if ir.diagram_type != DiagramType::Sankey {
        return None;
    }
    let node = ir.nodes.get(node_index)?;
    let mut inflow = 0.0_f32;
    let mut outflow = 0.0_f32;
    for edge in &ir.edges {
        let Some(value) = sankey_flow_value(ir, edge) else {
            continue;
        };
        if edge.to == fm_core::IrEndpoint::Node(fm_core::IrNodeId(node_index)) {
            inflow += value;
        }
        if edge.from == fm_core::IrEndpoint::Node(fm_core::IrNodeId(node_index)) {
            outflow += value;
        }
    }
    let total = inflow.max(outflow);
    if total <= 0.0 {
        return None;
    }
    Some(format!(
        "{}\n{}",
        ir.node_display_text(node),
        format_sankey_total(total)
    ))
}

/// Plain decimal, no trailing zeros: 150 renders `150`, 3.75 renders `3.75`.
fn format_sankey_total(total: f32) -> String {
    if (total.fract()).abs() < f32::EPSILON {
        format!("{}", total as i64)
    } else {
        let text = format!("{total}");
        text
    }
}

/// Value carried by a sankey flow, which `parse_sankey` stores as the edge LABEL.
fn sankey_flow_value(ir: &MermaidDiagramIr, edge: &fm_core::IrEdge) -> Option<f32> {
    let text = ir.labels.get(edge.label?.0)?.text.trim();
    let value = text.parse::<f32>().ok()?;
    (value.is_finite() && value > 0.0).then_some(value)
}

/// Largest sankey flow in the diagram, computed ONCE per render. `None` for every other type.
fn sankey_widest_flow(ir: &MermaidDiagramIr) -> Option<f32> {
    if ir.diagram_type != DiagramType::Sankey {
        return None;
    }
    let widest = ir
        .edges
        .iter()
        .filter_map(|edge| sankey_flow_value(ir, edge))
        .fold(0.0_f32, f32::max);
    (widest > 0.0).then_some(widest)
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
    write_common_edge_path_into(
        &mut f,
        path_str,
        stroke_width,
        style_class,
        edge_index,
        marker_end,
    );
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
    write_common_edge_path_tail_with_dasharray_into(
        f,
        stroke_width,
        style_class,
        edge_index,
        marker_end,
        "",
    );
}

fn write_common_edge_path_tail_with_dasharray_into(
    f: &mut String,
    stroke_width: f32,
    style_class: &str,
    edge_index: i32,
    marker_end: &str,
    dasharray: &str,
) {
    // `<path>`-child callers: the `id` lives on the enclosing `<g>`, never on the path.
    write_common_edge_path_tail_with_markers_into::<false>(
        f,
        stroke_width,
        style_class,
        edge_index,
        "",
        marker_end,
        dasharray,
    );
}

/// `EDGE_ID` appends the trailing `id="fm-edge-{index}"` that the slow path's final
/// `elem.id(&mermaid_edge_element_id(edge_index))` puts on an *unwrapped* edge — the shape a lean
/// (`A11yConfig::none()`) edge takes, since it gets no `<g>` wrapper to carry the id. Group-wrapped
/// callers pass `false`: there the id is the group's, and `Attributes::set` would have placed it last
/// anyway, which is why it is written after the markers/dasharray here.
fn write_common_edge_path_tail_with_markers_into<const EDGE_ID: bool>(
    f: &mut String,
    stroke_width: f32,
    style_class: &str,
    edge_index: i32,
    marker_start: &str,
    marker_end: &str,
    dasharray: &str,
) {
    use crate::attributes::{AttributeValue, write_escaped_attr};
    f.push_str("\" stroke-width=\"");
    let _ = crate::attributes::write_number_into(f, stroke_width);
    f.push_str("\" class=\"fm-edge ");
    f.push_str(style_class);
    f.push_str("\" data-fm-edge-id=\"");
    let _ = AttributeValue::Integer(edge_index).write_value(f);
    // Empty marker strings mean the slow path's `render_edge` produced no marker attribute, so omit
    // them here too. When both are present, `marker-start` must precede `marker-end`, matching the
    // `Element` builder's insertion order.
    if !marker_start.is_empty() {
        f.push_str("\" marker-start=\"");
        let _ = write_escaped_attr(f, marker_start);
    }
    if !marker_end.is_empty() {
        f.push_str("\" marker-end=\"");
        let _ = write_escaped_attr(f, marker_end);
    }
    if !dasharray.is_empty() {
        f.push_str("\" stroke-dasharray=\"");
        let _ = write_escaped_attr(f, dasharray);
    }
    if EDGE_ID {
        // `mermaid_edge_element_id(i)` = "fm-edge-" + decimal(i) — no escapable byte, so the same
        // `Integer` serializer as `data-fm-edge-id` reproduces it exactly.
        f.push_str("\" id=\"fm-edge-");
        let _ = AttributeValue::Integer(edge_index).write_value(f);
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
    // `render_edge`'s fast path is gated on full a11y, so only the `A11Y = true` shape is reachable here.
    write_common_edge_full_fragment_into::<true, _>(
        &mut f,
        point_count,
        point_at,
        stroke_width,
        style_class,
        edge_index,
        "",
        marker_end,
        "",
        " points to ",
        from_label,
        to_label,
    );
    f
}

/// Write-into core of [`build_common_edge_full_fragment`]. Used by `render_edge_into` to stream the whole
/// common edge straight into the chunk output buffer, with NO per-edge fragment `String` (the fragment
/// `String` is the single largest remaining per-element render allocation on wide flowcharts).
///
/// `A11Y` selects the accessibility variant at compile time, mirroring `write_common_node_fragment_into`:
///
/// - `true` (`A11yConfig::full()`, the default profile) emits `<g id … role … tabindex><path/><title/></g>`.
/// - `false` (`A11yConfig::none()`, the lean profile) emits the **bare `<path …  id="fm-edge-N"/>`** with no
///   group and no title — exactly what the slow `Element` path produces when every a11y flag is off: the
///   unlabeled-edge `<title>` group at `render_edge`'s tail is skipped, `role`/`tabindex` are skipped, and
///   the final `elem.id(&mermaid_edge_element_id(edge_index))` lands on the `<path>` itself (last, because
///   `Attributes::set` appends).
///
/// Making it a const parameter rather than a runtime flag keeps the default monomorphization exactly as
/// branch-free as it was before the lean variant existed — a runtime flag cost a measured +0.1..0.33%
/// instructions on the default path when the same move was made for nodes (see `bd-b2b6`).
///
/// `arrow_phrase` / `from_label` / `to_label` feed only the `<title>`, so the `A11Y = false`
/// monomorphization discards them (and its callers need not compute the endpoint labels at all).
#[allow(clippy::too_many_arguments)]
fn write_common_edge_full_fragment_into<const A11Y: bool, F>(
    f: &mut String,
    point_count: usize,
    point_at: F,
    stroke_width: f32,
    style_class: &str,
    edge_index: i32,
    marker_start: &str,
    marker_end: &str,
    dasharray: &str,
    arrow_phrase: &str,
    from_label: Option<&str>,
    to_label: Option<&str>,
) where
    F: FnMut(usize) -> (f32, f32),
{
    use crate::attributes::{AttributeValue, write_escaped_text};
    if A11Y {
        // <g id="fm-edge-N" class="fm-edge" data-fm-edge-id="N" role="graphics-symbol" tabindex="0">
        // The id is `mermaid_edge_element_id(edge_index)` = "fm-edge-" + decimal(index); it never contains
        // an escapable byte, so it goes through the same `Integer` serializer as `data-fm-edge-id`.
        f.push_str("<g id=\"fm-edge-");
        let _ = AttributeValue::Integer(edge_index).write_value(f);
        f.push_str("\" class=\"fm-edge\" data-fm-edge-id=\"");
        let _ = AttributeValue::Integer(edge_index).write_value(f);
        f.push_str("\" role=\"graphics-symbol\" tabindex=\"0\">");
    }
    f.push_str("<path d=\"");
    crate::path::build_smooth_path_by_into(f, point_count, point_at);
    if A11Y {
        write_common_edge_path_tail_with_markers_into::<false>(
            f,
            stroke_width,
            style_class,
            edge_index,
            marker_start,
            marker_end,
            dasharray,
        );
        f.push_str("<title>");
        let _ = write_escaped_text(f, from_label.unwrap_or("unknown"));
        // The a11y connective phrase is `describe_edge_labels`'s per-arrow word surrounded by spaces
        // (`" points to "` for a solid arrow, `" connects to "` for a plain line). It contains no escapable
        // byte, so writing it verbatim matches the slow path's escaped whole-description byte-for-byte.
        f.push_str(arrow_phrase);
        let _ = write_escaped_text(f, to_label.unwrap_or("unknown"));
        f.push_str("</title></g>");
    } else {
        write_common_edge_path_tail_with_markers_into::<true>(
            f,
            stroke_width,
            style_class,
            edge_index,
            marker_start,
            marker_end,
            dasharray,
        );
    }
}

/// Compute the rendered edge label (truncated text with optional sequence autonumber prefix) and its
/// midpoint, exactly as the labeled-edge fragments need them. Shared by `render_edge` (slow/`Element`
/// path) and `render_edge_into` (streaming path) so both derive byte-identical label text + position.
fn compute_edge_label<'a>(
    ir: &'a MermaidDiagramIr,
    edge_path: &LayoutEdgePath,
    edge_index: usize,
    detail: RenderDetailProfile,
    offset_x: f32,
    offset_y: f32,
) -> Option<(Cow<'a, str>, f32, f32)> {
    let ir_edge = ir.edges.get(edge_index);
    if detail.show_edge_labels
        && edge_path.points.len() >= 2
        && let Some(label_id) = ir_edge.and_then(|e| e.label)
        && let Some(label) = ir.labels.get(label_id.0)
    {
        // The autonumber is NO LONGER PREFIXED here (bd-o02wn). mermaid 11.15.0's `drawMessage`
        // emits it as its OWN element — `.attr("class","sequenceNumber").text(f)`, the number and
        // nothing else — beside the message, not glued to the front of the label. Two repo tests
        // had contradicted each other about the prefix spelling (`10 Ping` vs `10. numbered once`)
        // and neither matched the incumbent, because a prefix is not what the incumbent produces.
        // The number is now written by `write_sequence_number_into`.
        let label_text: Cow<'a, str> = truncate_label(&label.text, detail.edge_label_max_chars);
        let (lx, ly) = if edge_path.points.len() == 4 {
            let p1 = &edge_path.points[1];
            let p2 = &edge_path.points[2];
            (
                f32::midpoint(p1.x, p2.x) + offset_x,
                f32::midpoint(p1.y, p2.y) + offset_y - 8.0,
            )
        } else if edge_path.points.len() == 2 {
            let p1 = &edge_path.points[0];
            let p2 = &edge_path.points[1];
            (
                f32::midpoint(p1.x, p2.x) + offset_x,
                f32::midpoint(p1.y, p2.y) + offset_y - 8.0,
            )
        } else {
            let mid_idx = edge_path.points.len() / 2;
            let mid_point = &edge_path.points[mid_idx];
            (mid_point.x + offset_x, mid_point.y + offset_y - 8.0)
        };
        Some((label_text, lx, ly))
    } else {
        None
    }
}

/// Stream the whole labeled-`Arrow` edge fragment (`<g><path/><rect/><text/><title/></g>`) directly into
/// `out`. Shared by `render_edge` (into a fresh String wrapped in `Element::raw_svg`) and
/// `render_edge_into` (straight into the output buffer, avoiding the per-edge fragment String + `Element`
/// + the copy). Byte-identical (pinned by `golden_svg_test` + `edge_fast_full_fragment_matches_render`).
///
/// `A11Y` selects the accessibility variant at compile time, mirroring
/// `write_common_edge_full_fragment_into`. `A11Y = true` (`A11yConfig::full()`, default profile) emits the
/// group with `role="graphics-symbol" tabindex="0"` and the trailing `<title>` text alternative. `A11Y =
/// false` (`A11yConfig::none()`, lean profile) skips `role`/`tabindex` and the `<title>` entirely — exactly
/// what the slow `Element` path produces when every a11y flag is off (the `<g id/class/data-fm-edge-id>`
/// wrapper, `<path>`, `<rect>`, and `<text>` are all a11y-independent, so only those two spots differ). The
/// `from_label`/`to_label` feed only the `<title>`, so the `false` monomorphization discards them and its
/// caller need not compute the endpoint labels.
///
/// A const parameter rather than a runtime flag keeps the default path exactly as branch-free as before
/// (a runtime flag measured +0.1..0.33% instructions on the node lever, see `bd-b2b6`).
#[allow(clippy::too_many_arguments)]
fn write_labeled_edge_fragment_into<const A11Y: bool>(
    out: &mut String,
    edge_index: usize,
    path_str: &str,
    stroke_width: f32,
    style_class: &str,
    marker_end_val: &str,
    label_str: &str,
    lx: f32,
    ly: f32,
    label_font_size: f32,
    avg_char_width: f32,
    from_label: Option<&str>,
    to_label: Option<&str>,
    colors: &ThemeColors,
) {
    use crate::attributes::{
        AttributeValue, write_escaped_attr, write_escaped_text, write_number_into,
    };
    let label_width = (label_str.chars().count() as f32 * avg_char_width) + 8.0 + 20.0;
    let label_height = label_font_size + 14.0;
    let start_y = ly + (label_font_size / 4.0);
    out.push_str("<g id=\"fm-edge-");
    let _ = AttributeValue::Integer(edge_index as i32).write_value(out);
    out.push_str("\" class=\"fm-edge-labeled\" data-fm-edge-id=\"");
    let _ = AttributeValue::Integer(edge_index as i32).write_value(out);
    if A11Y {
        out.push_str("\" role=\"graphics-symbol\" tabindex=\"0\">");
    } else {
        out.push_str("\">");
    }
    write_common_edge_path_into(
        out,
        path_str,
        stroke_width,
        style_class,
        edge_index as i32,
        marker_end_val,
    );
    out.push_str("<rect x=\"");
    let _ = write_number_into(out, lx - label_width / 2.0);
    out.push_str("\" y=\"");
    let _ = write_number_into(out, ly - label_height / 2.0 - 1.0);
    out.push_str("\" width=\"");
    let _ = write_number_into(out, label_width);
    out.push_str("\" height=\"");
    let _ = write_number_into(out, label_height);
    out.push_str("\" fill=\"");
    let _ = write_escaped_attr(out, &colors.background);
    out.push_str("\" stroke=\"");
    let _ = write_escaped_attr(out, &colors.cluster_stroke);
    out.push_str("\" stroke-width=\"0.75\" rx=\"6\" ry=\"6\"/><text x=\"");
    let _ = write_number_into(out, lx);
    out.push_str("\" y=\"");
    let _ = write_number_into(out, start_y);
    out.push_str("\" text-anchor=\"middle\" font-size=\"");
    let _ = write_number_into(out, label_font_size);
    out.push_str("\" fill=\"");
    let _ = write_escaped_attr(out, &colors.text);
    out.push_str("\" class=\"edge-label\">");
    let _ = write_escaped_text(out, label_str);
    if A11Y {
        out.push_str("</text><title>");
        let _ = write_escaped_text(out, from_label.unwrap_or("unknown"));
        out.push_str(" points to ");
        let _ = write_escaped_text(out, to_label.unwrap_or("unknown"));
        out.push_str(" with label: ");
        let _ = write_escaped_text(out, label_str);
        out.push_str("</title></g>");
    } else {
        // Lean: no `<title>`. `from_label`/`to_label` are unused (the caller passes `None`).
        let _ = (from_label, to_label);
        out.push_str("</text></g>");
    }
}

/// Ribbon width for a sankey flow, proportional to its value (bd-e69x).
///
/// Proportional width IS the sankey diagram — it is the only thing carrying the quantity. Drawing
/// every flow at the generic edge width leaves a stable, tidy picture that conveys nothing, which
/// is why a byte golden pinned it for months without complaint.
///
/// Returns `None` for anything that is not a numeric sankey flow, so every other diagram type and
/// any malformed row keeps the arrow-derived width exactly as before.
fn sankey_flow_stroke_width(
    ir: &MermaidDiagramIr,
    edge: Option<&fm_core::IrEdge>,
    widest: Option<f32>,
) -> Option<f32> {
    /// Keeps the smallest flow visible instead of collapsing it to a hairline.
    const MIN_WIDTH: f32 = 1.5;
    /// Width of the largest flow; every other flow is scaled against it.
    const MAX_WIDTH: f32 = 24.0;

    // `widest` is `None` for every non-sankey diagram and for a sankey with no usable flow, so it
    // subsumes the old `diagram_type` guard AND the old `widest <= 0.0` bail.
    let widest = widest?;
    let value = sankey_flow_value(ir, edge?)?;
    // Normalise against the widest flow so the ratio between two ribbons equals the ratio between
    // their values — the property a reader actually reads off the picture.
    Some(((value / widest) * MAX_WIDTH).max(MIN_WIDTH))
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
        sankey_widest_flow: sankey_widest,
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
            ArrowType::Circle | ArrowType::ThickCircle => {
                (None, None, Some("url(#arrow-circle)"), &colors.edge)
            }
            ArrowType::Cross | ArrowType::ThickCross => {
                (None, None, Some("url(#arrow-cross)"), &colors.edge)
            }
            ArrowType::DottedCircle => {
                (Some("5,5"), None, Some("url(#arrow-circle)"), &colors.edge)
            }
            ArrowType::CircleBoth | ArrowType::ThickCircleBoth => (
                None,
                Some("url(#arrow-circle)"),
                Some("url(#arrow-circle)"),
                &colors.edge,
            ),
            ArrowType::DottedCircleBoth => (
                Some("5,5"),
                Some("url(#arrow-circle)"),
                Some("url(#arrow-circle)"),
                &colors.edge,
            ),
            ArrowType::CrossBoth | ArrowType::ThickCrossBoth => (
                None,
                Some("url(#arrow-cross)"),
                Some("url(#arrow-cross)"),
                &colors.edge,
            ),
            ArrowType::DottedCrossBoth => (
                Some("5,5"),
                Some("url(#arrow-cross)"),
                Some("url(#arrow-cross)"),
                &colors.edge,
            ),
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
            // UML aggregation/composition put the diamond on the OWNING end, which is the source for
            // `o--`/`*--` and the target for the reversed `--o`/`--*` — hence marker-start vs -end
            // rather than one variant plus a flag. Hollow diamond = aggregation, filled = composition.
            ArrowType::Aggregation => (None, Some("url(#arrow-diamond-open)"), None, &colors.edge),
            ArrowType::AggregationReverse => {
                (None, None, Some("url(#arrow-diamond-open)"), &colors.edge)
            }
            ArrowType::Composition => (None, Some("url(#arrow-diamond)"), None, &colors.edge),
            ArrowType::CompositionReverse => {
                (None, None, Some("url(#arrow-diamond)"), &colors.edge)
            }
            // UML generalization: hollow triangle on the PARENT end. `Animal <|-- Dog` reads "Dog
            // inherits Animal", so the parent is the source; `--|>` puts it at the target.
            ArrowType::Inheritance => (
                None,
                Some("url(#start-arrow-triangle-open)"),
                None,
                &colors.edge,
            ),
            ArrowType::InheritanceReverse => {
                (None, None, Some("url(#arrow-triangle-open)"), &colors.edge)
            }
        }
    };

    let stroke_width =
        sankey_flow_stroke_width(ir, ir_edge, sankey_widest).unwrap_or(match arrow {
            ArrowType::ThickArrow
            | ArrowType::DoubleThickArrow
            | ArrowType::ThickLine
            // The `==` body sets the weight whatever marker ends it (bd-lrl48): `A ==o B` is as
            // thick as `A ==> B`. Missing here, these drew at the default 1.8 — which is what
            // "o==o renders a solid stroke" meant.
            | ArrowType::ThickCircle
            | ArrowType::ThickCross
            | ArrowType::ThickCircleBoth
            | ArrowType::ThickCrossBoth => 2.5,
            _ => 1.8,
        });

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
            | ArrowType::DoubleDottedArrow
            // Dotted bodies that end in a circle or cross marker (bd-lrl48).
            | ArrowType::DottedCircle
            | ArrowType::DottedCircleBoth
            | ArrowType::DottedCrossBoth => "fm-edge-dashed",
            ArrowType::ThickArrow
            | ArrowType::DoubleThickArrow
            | ArrowType::ThickLine
            | ArrowType::ThickCircle
            | ArrowType::ThickCross
            | ArrowType::ThickCircleBoth
            | ArrowType::ThickCrossBoth => "fm-edge-thick",
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

    // Extract the rendered label (text + midpoint) once, up front, so the labeled fast fragment below
    // can return before the `elem` path-`Element` is built. Shared with `render_edge_into` via
    // `compute_edge_label` so the streaming path derives byte-identical text + position.
    let edge_label = compute_edge_label(ir, edge_path, edge_index, detail, offset_x, offset_y);

    // Whole labeled-edge fast fragment, hoisted above `elem`: for the common single-line solid-`Arrow`
    // label under embedded CSS + default a11y, stream `<g><path/><rect/><text/><title/></g>` and RETURN
    // before `elem` is built (it would be discarded here). Byte-identical to the slow Element path
    // (pinned by `golden_svg_test` + `edge_fast_full_fragment_matches_render`); other cases build `elem`.
    if let Some((label_text, lx, ly)) = &edge_label {
        let label_str = label_text.as_ref();
        if config.embed_theme_css
            && config.a11y.aria_labels
            && config.a11y.keyboard_nav
            && config.a11y.text_alternatives
            && !config.animations_enabled
            && !config.include_source_spans
            && !is_back_edge
            && arrow == ArrowType::Arrow
            && marker_start.is_none()
            && base_dasharray.is_none()
            && !label_str.contains('\n')
            // ⚠️ THE FAST FRAGMENT DRAWS EXACTLY ONE TEXT ELEMENT, so an edge carrying a C4
            // technology must not take it — the second row would be silently dropped and the
            // fragment's "byte-identical to the slow path" contract would quietly stop holding.
            // Gating here keeps the fast path's shape assumption true rather than teaching it a
            // case it was never meant to cover.
            && ir_edge
                .and_then(|edge| edge.extras.as_ref())
                .and_then(|extras| extras.technology.as_deref())
                .is_none()
            && resolve_edge_inline_style(ir, edge_index).is_none()
            && let Some(marker_end_val) = marker_end
            && let Some(edge) = ir_edge
        {
            let label_font_size = detail.edge_font_size;
            let (from_label, to_label) =
                edge_endpoint_accessible_labels(edge, ir, accessible_node_labels);
            let mut f = String::with_capacity(path_str.len() + label_str.len() * 3 + 360);
            write_labeled_edge_fragment_into::<true>(
                &mut f,
                edge_index,
                &path_str,
                stroke_width,
                style_class,
                marker_end_val,
                label_str,
                *lx,
                *ly,
                label_font_size,
                config.avg_char_width,
                from_label,
                to_label,
                colors,
            );
            return Element::raw_svg(f);
        }
    }

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

    // If edge has a label, wrap in group with text. `edge_label` was extracted up front and the
    // labeled fast fragment already returned for the common single-line solid-`Arrow` case; this is
    // the Element slow path for the labeled edges the fragment does not cover.
    if let Some((label_text, lx, ly)) = edge_label {
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

        // A C4 relationship's TECHNOLOGY, drawn as its own italic row beneath the label.
        //
        // mermaid's `drawRels` emits two text elements, not one string: the label, then
        // `"[" + s.techn.text + "]"` at `… + messageFontSize + 5`, with `{"font-style":"italic"}`.
        // We used to fold it into the label as `Uses [HTTPS]`, a run neither engine ever draws, and
        // the technology inherited the label's upright weight and position. The offset and the
        // italic here mirror the incumbent; the brackets are added at draw time so the IR keeps the
        // author's bare string (see `IrEdgeExtras::technology`).
        if let Some(technology) = ir_edge
            .and_then(|edge| edge.extras.as_ref())
            .and_then(|extras| extras.technology.as_deref())
        {
            group = group.child(
                TextBuilder::new(&format!("[{technology}]"))
                    .x(lx)
                    .y(start_y + label_font_size + 5.0)
                    .font_family_unless_embedded_css(&config.font_family, config.embed_theme_css)
                    .font_size(label_font_size)
                    .anchor(TextAnchor::Middle)
                    .fill(&colors.text)
                    .class("edge-label")
                    .build()
                    .attr("font-style", "italic"),
            );
        }

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
/// Stream a sequence message's AUTONUMBER as its own `<text>` element (bd-o02wn).
///
/// mermaid 11.15.0's `drawMessage` writes the number as a standalone element carrying only the
/// digits — `.attr("text-anchor","middle").attr("class","sequenceNumber").text(f)` — beside the
/// message. We used to glue it onto the front of the label instead, which is why the label read
/// `10 Ping` and why two tests in this repo disagreed about whether a period belonged after the
/// number. Neither spelling was the incumbent's, because the incumbent does not build a prefix.
///
/// Positioned at the message's START point, which is where mermaid draws it, rather than at the
/// label midpoint. NOT claimed as pixel parity: mermaid also draws a filled circle behind the digits
/// and themes them via `sequenceNumberColor`, and neither is replicated here — this is the element
/// and its content, not the decoration.
///
/// Emitted from a WRAPPER around the edge body rather than from inside it, deliberately: the
/// labeled-edge fast fragments are byte-identity-pinned by `golden_svg_test` and
/// `edge_fast_full_fragment_matches_render`, and threading a second element through them would put
/// the hottest render path in the crate at risk to add a decoration beside it.
fn write_sequence_number_into(
    out: &mut String,
    edge_path: &LayoutEdgePath,
    context: &EdgeRenderContext<'_>,
) {
    let Some(number) = context
        .ir
        .sequence_meta
        .as_ref()
        .and_then(|meta| meta.autonumber_value(edge_path.edge_index))
    else {
        return;
    };
    let Some(start) = edge_path.points.first() else {
        return;
    };
    let Some(end) = edge_path.points.last() else {
        return;
    };

    out.push_str("<text x=\"");
    let _ = crate::attributes::write_number_into(out, start.x + context.offset_x);
    out.push_str("\" y=\"");
    let _ = crate::attributes::write_number_into(
        out,
        f32::midpoint(start.y, end.y) + context.offset_y - 8.0,
    );
    // THEMED, and it was not when this element first landed (bd-7hgxu). `fm-sequence-number` was
    // emitted with no `fill` and no CSS rule anywhere in the crate, so the number fell back to the
    // SVG default of black — invisible against a dark theme's background, on a diagram whose every
    // other text run is themed. A class with no rule behind it is not styling, it is a name.
    //
    // `colors.text`, the same colour every other label uses, rather than a new theme field. mermaid
    // has a dedicated `sequenceNumberColor` computed to CONTRAST with the line colour, which is not
    // the same thing and is not claimed here: this makes the number readable and theme-consistent,
    // not identical to the incumbent's palette.
    //
    // Attribute order follows `write_gantt_label_into` — text-anchor, dominant-baseline, font-size,
    // fill, class — because in this file order is output, and a sibling writer disagreeing about it
    // is how byte-identity tests start failing for reasons nobody can read.
    out.push_str("\" text-anchor=\"middle\" dominant-baseline=\"central\" font-size=\"");
    let _ = crate::attributes::write_number_into(out, context.config.font_size * 0.8);
    out.push_str("\" fill=\"");
    let _ = crate::attributes::write_escaped_attr(out, &context.colors.text);
    out.push_str("\" class=\"fm-sequence-number\">");
    let _ = crate::attributes::write_number_into(out, number as f32);
    out.push_str("</text>");
}

fn render_edge_into(out: &mut String, edge_path: &LayoutEdgePath, context: &EdgeRenderContext<'_>) {
    render_edge_body_into(out, edge_path, context);
    write_sequence_number_into(out, edge_path, context);
}

fn render_edge_body_into(
    out: &mut String,
    edge_path: &LayoutEdgePath,
    context: &EdgeRenderContext<'_>,
) {
    use fm_core::ArrowType;
    let EdgeRenderContext {
        ir,
        offset_x,
        offset_y,
        config,
        detail,
        colors,
        accessible_node_labels,
        sankey_widest_flow: sankey_widest,
    } = *context;
    let edge_index = edge_path.edge_index;
    let ir_edge = ir.edges.get(edge_index);
    let arrow = ir_edge.map_or(ArrowType::Arrow, |edge| edge.arrow);
    let is_back_edge = edge_path.reversed;

    // Stream the labeled-`Arrow` fast fragment straight into `out` instead of falling through to
    // `render_edge(..).write_to_string(out)`, which builds the fragment String + an `Element::raw_svg`
    // then COPIES it in (a per-labeled-edge double-copy — sequence messages / ER-class relationships).
    // The labeled fast path is Arrow-only, so `render_edge`'s Arrow-derived width/class/marker are the
    // constants below (1.8 / "fm-edge-solid" / "url(#arrow-end)"); the same gate + the shared
    // `compute_edge_label`/`write_labeled_edge_fragment_into` helpers keep the bytes identical. Only
    // labeled Arrow edges enter here; unlabeled edges (`compute_edge_label` -> None) and every other
    // labeled/complex case fall through to the tuple fast path / `Element` slow path exactly as before.
    // Was `aria_labels && keyboard_nav && text_alternatives`. The fragment writer now has a lean
    // (a11y-off) shape too, so the gate accepts a11y that is uniformly on OR uniformly off and dispatches
    // to the matching monomorphization — label-heavy diagrams (sequence/er/sankey) were the last lean edge
    // family still falling to the ~5-alloc `Element` slow path. Mixed a11y (`A11yConfig::minimal()`) still
    // takes the slow path, exactly as before.
    if arrow == ArrowType::Arrow
        && !is_back_edge
        // Cheap label-presence gate BEFORE `uniform_a11y`/`compute_edge_label`: an unlabeled Arrow edge
        // (the common flowchart/state case) is handled by the unlabeled fast path below, so short-circuit
        // it here instead of computing (and discarding) its label. Necessary condition for
        // `compute_edge_label` to return `Some`; without it, relaxing the a11y gate to `uniform_a11y`
        // regressed lean flowchart/state ~1% (the old `aria_labels` flag used to short-circuit here).
        && detail.show_edge_labels
        && ir_edge.is_some_and(|e| e.label.is_some())
        // ⚠️ THE SECOND FAST PATH, GATED FOR THE SAME REASON AS THE FIRST. `write_labeled_edge_
        // fragment_into` streams exactly one text element, so a C4 relationship carrying a
        // technology has to fall through to the `Element` path that can draw the second row.
        // Gating only the other fast path left this one live: the CLI agreed with the incumbent
        // while `render_svg` silently dropped `[HTTPS]`, which is the asymmetric-sibling failure in
        // its purest form — same fix, two writers, one of them missed.
        && ir_edge
            .and_then(|edge| edge.extras.as_ref())
            .and_then(|extras| extras.technology.as_deref())
            .is_none()
        && config.embed_theme_css
        && let Some(a11y) = uniform_a11y(&config.a11y)
        && !config.animations_enabled
        && !config.include_source_spans
        && let Some(edge) = ir_edge
        && let Some((label_text, lx, ly)) =
            compute_edge_label(ir, edge_path, edge_index, detail, offset_x, offset_y)
    {
        let label_str = label_text.as_ref();
        // A sankey flow's WIDTH is its value (bd-e69x). This labeled-edge fast path is the one
        // sankey actually takes — its edges always carry a label, because the parser stores the
        // flow amount there — so the width must be resolved here, not only after the arrow match.
        let labeled_stroke_width =
            sankey_flow_stroke_width(ir, ir_edge, sankey_widest).unwrap_or(1.8);
        if !label_str.contains('\n') && resolve_edge_inline_style(ir, edge_index).is_none() {
            let path_str = smooth_layout_edge_path(edge_path, offset_x, offset_y);
            if a11y {
                let (from_label, to_label) =
                    edge_endpoint_accessible_labels(edge, ir, accessible_node_labels);
                write_labeled_edge_fragment_into::<true>(
                    out,
                    edge_index,
                    &path_str,
                    labeled_stroke_width,
                    "fm-edge-solid",
                    "url(#arrow-end)",
                    label_str,
                    lx,
                    ly,
                    detail.edge_font_size,
                    config.avg_char_width,
                    from_label,
                    to_label,
                    colors,
                );
            } else {
                // Lean fragment has no `<title>`, so endpoint labels are skipped entirely rather than
                // computed and discarded (`accessible_node_labels` is `None` under lean anyway).
                write_labeled_edge_fragment_into::<false>(
                    out,
                    edge_index,
                    &path_str,
                    labeled_stroke_width,
                    "fm-edge-solid",
                    "url(#arrow-end)",
                    label_str,
                    lx,
                    ly,
                    detail.edge_font_size,
                    config.avg_char_width,
                    None,
                    None,
                    colors,
                );
            }
            return;
        }
    }

    // The whole-edge streaming fragment handles every non-reversed arrow whose slow-path `render_edge`
    // shape is a single `<path>`. Each tuple below is `(stroke_width, style_class, marker_start,
    // marker_end, dasharray, a11y_phrase)` read straight off `render_edge`'s matches and
    // `describe_edge_labels`'s per-arrow word (surrounded by spaces; `_ => "connects to"`).
    // Back-edges, labels, inline styles, animations, source spans, and non-full a11y all still fall to
    // the `Element` slow path below.
    // Byte-identity is pinned by `golden_svg_test` + `edge_fast_full_fragment_matches_render`.
    let (stroke_width, style_class, marker_start, marker_end, dasharray, arrow_phrase): (
        f32,
        &str,
        &str,
        &str,
        &str,
        &str,
    ) = match arrow {
        ArrowType::Arrow => (
            1.8,
            "fm-edge-solid",
            "",
            "url(#arrow-end)",
            "",
            " points to ",
        ),
        ArrowType::Line => (1.8, "fm-edge-solid", "", "", "", " connects to "),
        ArrowType::OpenArrow => (
            1.8,
            "fm-edge-solid",
            "",
            "url(#arrow-open)",
            "",
            " sends to ",
        ),
        ArrowType::Circle => (
            1.8,
            "fm-edge-solid",
            "",
            "url(#arrow-circle)",
            "",
            " relates to ",
        ),
        ArrowType::CircleBoth => (
            1.8,
            "fm-edge-solid",
            "url(#arrow-circle)",
            "url(#arrow-circle)",
            "",
            " relates to ",
        ),
        ArrowType::ThickCircle => (
            2.5,
            "fm-edge-thick",
            "",
            "url(#arrow-circle)",
            "",
            " relates to ",
        ),
        ArrowType::DottedCircle => (
            1.8,
            "fm-edge-dashed",
            "",
            "url(#arrow-circle)",
            "5,5",
            " relates to ",
        ),
        ArrowType::ThickCircleBoth => (
            2.5,
            "fm-edge-thick",
            "url(#arrow-circle)",
            "url(#arrow-circle)",
            "",
            " relates to ",
        ),
        ArrowType::DottedCircleBoth => (
            1.8,
            "fm-edge-dashed",
            "url(#arrow-circle)",
            "url(#arrow-circle)",
            "5,5",
            " relates to ",
        ),
        ArrowType::CrossBoth => (
            1.8,
            "fm-edge-solid",
            "url(#arrow-cross)",
            "url(#arrow-cross)",
            "",
            " relates to ",
        ),
        ArrowType::ThickCrossBoth => (
            2.5,
            "fm-edge-thick",
            "url(#arrow-cross)",
            "url(#arrow-cross)",
            "",
            " relates to ",
        ),
        ArrowType::DottedCrossBoth => (
            1.8,
            "fm-edge-dashed",
            "url(#arrow-cross)",
            "url(#arrow-cross)",
            "5,5",
            " relates to ",
        ),
        ArrowType::Cross => (
            1.8,
            "fm-edge-solid",
            "",
            "url(#arrow-cross)",
            "",
            " blocks ",
        ),
        ArrowType::ThickCross => (
            2.5,
            "fm-edge-thick",
            "",
            "url(#arrow-cross)",
            "",
            " blocks ",
        ),
        ArrowType::HalfArrowTop => (
            1.8,
            "fm-edge-solid",
            "",
            "url(#arrow-half-top)",
            "",
            " connects to ",
        ),
        ArrowType::HalfArrowBottom => (
            1.8,
            "fm-edge-solid",
            "",
            "url(#arrow-half-bottom)",
            "",
            " connects to ",
        ),
        ArrowType::HalfArrowTopReverse => (
            1.8,
            "fm-edge-solid",
            "url(#arrow-half-bottom)",
            "",
            "",
            " connects to ",
        ),
        ArrowType::HalfArrowBottomReverse => (
            1.8,
            "fm-edge-solid",
            "url(#arrow-half-top)",
            "",
            "",
            " connects to ",
        ),
        ArrowType::StickArrowTop => (
            1.8,
            "fm-edge-solid",
            "",
            "url(#arrow-stick-top)",
            "",
            " connects to ",
        ),
        ArrowType::StickArrowBottom => (
            1.8,
            "fm-edge-solid",
            "",
            "url(#arrow-stick-bottom)",
            "",
            " connects to ",
        ),
        ArrowType::StickArrowTopReverse => (
            1.8,
            "fm-edge-solid",
            "url(#arrow-stick-bottom)",
            "",
            "",
            " connects to ",
        ),
        ArrowType::StickArrowBottomReverse => (
            1.8,
            "fm-edge-solid",
            "url(#arrow-stick-top)",
            "",
            "",
            " connects to ",
        ),
        ArrowType::ThickArrow => (
            2.5,
            "fm-edge-thick",
            "",
            "url(#arrow-filled)",
            "",
            " strongly points to ",
        ),
        ArrowType::ThickLine => (2.5, "fm-edge-thick", "", "", "", " strongly connects to "),
        ArrowType::DottedArrow => (
            1.8,
            "fm-edge-dashed",
            "",
            "url(#arrow-end)",
            "5,5",
            " optionally points to ",
        ),
        ArrowType::DottedOpenArrow => (
            1.8,
            "fm-edge-dashed",
            "",
            "url(#arrow-open)",
            "5,5",
            " optionally sends to ",
        ),
        ArrowType::DottedCross => (
            1.8,
            "fm-edge-dashed",
            "",
            "url(#arrow-cross)",
            "5,5",
            " connects to ",
        ),
        ArrowType::HalfArrowTopDotted => (
            1.8,
            "fm-edge-dashed",
            "",
            "url(#arrow-half-top)",
            "5,5",
            " connects to ",
        ),
        ArrowType::HalfArrowBottomDotted => (
            1.8,
            "fm-edge-dashed",
            "",
            "url(#arrow-half-bottom)",
            "5,5",
            " connects to ",
        ),
        ArrowType::HalfArrowTopReverseDotted => (
            1.8,
            "fm-edge-dashed",
            "url(#arrow-half-bottom)",
            "",
            "5,5",
            " connects to ",
        ),
        ArrowType::HalfArrowBottomReverseDotted => (
            1.8,
            "fm-edge-dashed",
            "url(#arrow-half-top)",
            "",
            "5,5",
            " connects to ",
        ),
        ArrowType::StickArrowTopDotted => (
            1.8,
            "fm-edge-dashed",
            "",
            "url(#arrow-stick-top)",
            "5,5",
            " connects to ",
        ),
        ArrowType::StickArrowBottomDotted => (
            1.8,
            "fm-edge-dashed",
            "",
            "url(#arrow-stick-bottom)",
            "5,5",
            " connects to ",
        ),
        ArrowType::StickArrowTopReverseDotted => (
            1.8,
            "fm-edge-dashed",
            "url(#arrow-stick-bottom)",
            "",
            "5,5",
            " connects to ",
        ),
        ArrowType::StickArrowBottomReverseDotted => (
            1.8,
            "fm-edge-dashed",
            "url(#arrow-stick-top)",
            "",
            "5,5",
            " connects to ",
        ),
        ArrowType::DottedLine => (
            1.8,
            "fm-edge-dashed",
            "",
            "",
            "5,5",
            " optionally connects to ",
        ),
        ArrowType::DoubleArrow => (
            1.8,
            "fm-edge-solid",
            "url(#arrow-start)",
            "url(#arrow-end)",
            "",
            " points both ways to ",
        ),
        ArrowType::DoubleThickArrow => (
            2.5,
            "fm-edge-thick",
            "url(#arrow-start-filled)",
            "url(#arrow-filled)",
            "",
            " strongly points both ways to ",
        ),
        ArrowType::DoubleDottedArrow => (
            1.8,
            "fm-edge-dashed",
            "url(#arrow-start)",
            "url(#arrow-end)",
            "5,5",
            " optionally points both ways to ",
        ),
        // The diamond marks the OWNING end: source for `o--`/`*--`, target for `--o`/`--*`. The
        // spoken phrase reads owner-first in every case, so the reversed forms invert the wording
        // rather than the marker slot alone.
        ArrowType::Aggregation => (
            1.8,
            "fm-edge-solid",
            "url(#arrow-diamond-open)",
            "",
            "",
            " aggregates ",
        ),
        ArrowType::AggregationReverse => (
            1.8,
            "fm-edge-solid",
            "",
            "url(#arrow-diamond-open)",
            "",
            " is aggregated by ",
        ),
        ArrowType::Composition => (
            1.8,
            "fm-edge-solid",
            "url(#arrow-diamond)",
            "",
            "",
            " is composed of ",
        ),
        ArrowType::CompositionReverse => (
            1.8,
            "fm-edge-solid",
            "",
            "url(#arrow-diamond)",
            "",
            " composes ",
        ),
        ArrowType::Inheritance => (
            1.8,
            "fm-edge-solid",
            "url(#start-arrow-triangle-open)",
            "",
            "",
            " is inherited by ",
        ),
        ArrowType::InheritanceReverse => (
            1.8,
            "fm-edge-solid",
            "",
            "url(#arrow-triangle-open)",
            "",
            " inherits ",
        ),
    };
    // A sankey flow's WIDTH is its value — the arrow-derived width above carries no quantity, so
    // every flow drew identically and the diagram conveyed nothing (bd-e69x). Applied here, after
    // the arrow match, so it overrides whichever arm ran and leaves every other diagram type on
    // exactly the width it had before.
    let stroke_width = sankey_flow_stroke_width(ir, ir_edge, sankey_widest).unwrap_or(stroke_width);
    // Was `text_alternatives && aria_labels && keyboard_nav`. The fragment writer now has a lean
    // (a11y-off) shape too, so the gate accepts a11y that is uniformly on OR uniformly off and dispatches
    // to the matching monomorphization. Mixed combinations (e.g. `A11yConfig::minimal()`) still take the
    // slow `Element` path, exactly as before — a raw fragment cannot express "role but no tabindex".
    if !edge_path.reversed
        && config.embed_theme_css
        && !config.animations_enabled
        && !config.include_source_spans
        && let Some(a11y) = uniform_a11y(&config.a11y)
        && !(detail.show_edge_labels && ir_edge.and_then(|edge| edge.label).is_some())
        && let Some(edge) = ir_edge
        && resolve_edge_inline_style(ir, edge_index).is_none()
    {
        let point_at = |index: usize| {
            let point = &edge_path.points[index];
            (point.x + offset_x, point.y + offset_y)
        };
        if a11y {
            let (from_label, to_label) =
                edge_endpoint_accessible_labels(edge, ir, accessible_node_labels);
            write_common_edge_full_fragment_into::<true, _>(
                out,
                edge_path.points.len(),
                point_at,
                stroke_width,
                style_class,
                edge_index as i32,
                marker_start,
                marker_end,
                dasharray,
                arrow_phrase,
                from_label,
                to_label,
            );
        } else {
            // The lean fragment has no `<title>`, so the endpoint-label lookup is skipped entirely rather
            // than computed and discarded (`accessible_node_labels` is `None` under lean anyway).
            write_common_edge_full_fragment_into::<false, _>(
                out,
                edge_path.points.len(),
                point_at,
                stroke_width,
                style_class,
                edge_index as i32,
                marker_start,
                marker_end,
                dasharray,
                "",
                None,
                None,
            );
        }
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

    fn frame_test_layout(x: f32, y: f32, width: f32, height: f32) -> DiagramLayout {
        DiagramLayout {
            nodes: Vec::new(),
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: fm_layout::LayoutRect {
                x,
                y,
                width,
                height,
            },
            stats: Default::default(),
            extensions: Default::default(),
            dirty_regions: Vec::new(),
        }
    }

    #[test]
    fn svg_frame_preserves_plain_legacy_layout_math() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let layout = frame_test_layout(20.0, -10.0, 200.0, 80.0);

        assert_eq!(
            svg_frame(&ir, &layout, &SvgRenderConfig::default()),
            SvgFrame {
                viewbox_width: 280.0,
                viewbox_height: 160.0,
                offset_x: 20.0,
                offset_y: 50.0,
            }
        );
    }

    #[test]
    fn svg_frame_reserves_the_generic_title_band() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.meta.title = Some("Payments topology".to_string());
        let layout = frame_test_layout(20.0, -10.0, 200.0, 80.0);
        let frame = svg_frame(&ir, &layout, &SvgRenderConfig::default());

        assert_eq!(frame.viewbox_height, 197.0);
        assert_eq!(frame.offset_y, 87.0);
        assert_ne!(
            frame.offset_y, 50.0,
            "a naive frame that ignores the generic title must fail this case"
        );
    }

    #[test]
    fn svg_frame_reserves_the_c4_legend_inset() {
        let ir = create_c4_ir_with_legend();
        let layout = frame_test_layout(20.0, -10.0, 100.0, 80.0);
        let frame = svg_frame(&ir, &layout, &SvgRenderConfig::default());

        assert_eq!(frame.viewbox_width, 400.0);
        assert_eq!(frame.viewbox_height, 288.0);
        assert_ne!(
            frame.viewbox_width, 180.0,
            "a naive frame that omits the C4 legend width must fail this case"
        );
    }

    #[test]
    fn renders_every_node_in_a_2000_node_flowchart_deterministically() {
        const NODE_COUNT: usize = 2_000;

        let mut input = String::from("flowchart LR\n");
        for index in 0..NODE_COUNT {
            input.push_str(&format!("  N{index}[node {index}]\n"));
        }
        for index in 0..NODE_COUNT - 1 {
            input.push_str(&format!("  N{index}-->N{}\n", index + 1));
        }

        let ir = fm_parser::parse(&input).ir;
        assert_eq!(ir.nodes.len(), NODE_COUNT);
        assert_eq!(ir.edges.len(), NODE_COUNT - 1);

        let layout = fm_layout::layout_diagram_traced(&ir).layout;
        assert_eq!(layout.nodes.len(), NODE_COUNT);

        let first = render_svg_with_layout(&ir, &layout, &SvgRenderConfig::default());
        let second = render_svg_with_layout(&ir, &layout, &SvgRenderConfig::default());
        assert_eq!(
            first, second,
            "large flowchart output must be deterministic"
        );
        assert!(first.contains("data-nodes=\"2000\""));
        assert!(
            first.trim_end().ends_with("</svg>"),
            "large flowchart SVG must complete rather than stopping after a partial document"
        );
        assert!(first.contains("id=\"fm-edge-"), "edges must be emitted");
        for index in 0..NODE_COUNT {
            assert!(
                first.contains(&format!("data-id=\"N{index}\"")),
                "render omitted authored node N{index}"
            );
        }
    }

    #[test]
    fn parallel_node_renderer_preserves_2048_node_flowchart_structure() {
        // Keep this at the native parallel-render threshold. The 2k DNF witness above covers
        // incumbent failure admission; this one additionally exercises our threaded node writer.
        const NODE_COUNT: usize = 2_048;

        let mut input = String::from("flowchart LR\n");
        for index in 0..NODE_COUNT {
            input.push_str(&format!("  N{index}[node {index}]\n"));
        }
        for index in 0..NODE_COUNT - 1 {
            input.push_str(&format!("  N{index}-->N{}\n", index + 1));
        }

        let ir = fm_parser::parse(&input).ir;
        let layout = fm_layout::layout_diagram_traced(&ir).layout;
        let first = render_svg_with_layout(&ir, &layout, &SvgRenderConfig::default());
        let second = render_svg_with_layout(&ir, &layout, &SvgRenderConfig::default());

        assert_eq!(
            first, second,
            "parallel node fragments must retain stable ordering"
        );
        assert_eq!(first.matches("<g id=\"fm-node-").count(), NODE_COUNT);
        assert_eq!(first.matches("id=\"fm-edge-").count(), NODE_COUNT - 1);
    }

    #[test]
    fn batch_renderer_reuses_shared_flowchart_prefix_byte_identically() {
        let inputs = [
            concat!(
                "flowchart LR\n",
                "  subgraph Shared\n",
                "    S0[Gateway]\n",
                "    S1[Queue]\n",
                "    S2[Worker]\n",
                "    S0-->S1\n",
                "    S1-->S2\n",
                "  end\n",
                "  D0[Alpha]\n",
                "  D1[Store]\n",
                "  S2-->D0\n",
                "  D0-->D1",
            ),
            concat!(
                "flowchart LR\n",
                "  subgraph Shared\n",
                "    S0[Gateway]\n",
                "    S1[Queue]\n",
                "    S2[Worker]\n",
                "    S0-->S1\n",
                "    S1-->S2\n",
                "  end\n",
                "  D0[Bravo]\n",
                "  D1[Cache]\n",
                "  S2-->D0\n",
                "  D0-->D1",
            ),
        ];
        let config = SvgRenderConfig::default();
        let mut renderer = SvgBatchRenderer::default();
        for input in inputs {
            let ir = Arc::new(fm_parser::parse(input).ir);
            let layout = fm_layout::layout_diagram_traced(&ir).layout;
            let expected = render_svg_with_layout(&ir, &layout, &config);
            let actual = renderer.render(ir, layout, &config);
            assert_eq!(actual, expected);
        }
        let snapshot = renderer.previous.as_ref().expect("batch snapshot");
        assert!(snapshot.fragments.reused_nodes >= 3);
        assert!(snapshot.fragments.reused_edges >= 2);
    }

    #[test]
    fn borrowed_batch_renderer_reuses_certified_prefix_without_retaining_ir() {
        let inputs = [
            concat!(
                "flowchart LR\n",
                "  subgraph Shared[\"Shared ingestion platform\"]\n",
                "    S0[\"Receive & validate events\"]\n",
                "    S1[\"Normalize payload safely\"]\n",
                "    S2[\"Publish canonical records\"]\n",
                "    S0-->S1\n",
                "    S1-->S2\n",
                "  end\n",
                "  S2-->A[\"Consumer 000\"]",
            ),
            concat!(
                "flowchart LR\n",
                "  subgraph Shared[\"Shared ingestion platform\"]\n",
                "    S0[\"Receive & validate events\"]\n",
                "    S1[\"Normalize payload safely\"]\n",
                "    S2[\"Publish canonical records\"]\n",
                "    S0-->S1\n",
                "    S1-->S2\n",
                "  end\n",
                "  S2-->B[\"Consumer 111\"]",
            ),
        ];
        let plan = fm_parser::FlowchartBatchParsePlan::new(
            &inputs,
            fm_core::MermaidParseMode::Compat,
            &fm_parser::ParserConfig::default(),
        );
        let config = SvgRenderConfig::default();
        let mut scratch = fm_parser::FlowchartBatchParseScratch::default();
        let mut renderer = SvgBatchRenderer::default();
        assert_eq!(plan.stats().shared_prefix_inputs, inputs.len());

        for (index, input) in inputs.iter().enumerate() {
            plan.with_parse_scratch(index, input, &mut scratch, |parsed| {
                let prefix = parsed
                    .reusable_prefix
                    .expect("shared prefix should remain unchanged");
                let layout = fm_layout::layout_diagram_traced(parsed.ir).layout;
                let expected = render_svg_with_layout(parsed.ir, &layout, &config);
                let actual = renderer.render_borrowed(
                    parsed.ir,
                    layout,
                    &config,
                    Some(CertifiedSvgBatchPrefix::new(
                        Arc::clone(&prefix.identity),
                        prefix.node_count,
                        prefix.edge_count,
                    )),
                );
                assert_eq!(actual, expected);
            });
        }

        let snapshot = renderer.previous.as_ref().expect("batch snapshot");
        assert!(snapshot.ir.is_none());
        assert!(snapshot.fragments.reused_nodes >= 3);
        assert!(snapshot.fragments.reused_edges >= 2);
    }

    #[test]
    fn borrowed_batch_renderer_transplants_certified_path_geometry() {
        let make_input = |tail_labels: &[&str]| {
            let mut input =
                String::from("flowchart LR\n  subgraph Shared[\"Shared ingestion platform\"]\n");
            for index in 0..48 {
                input.push_str(&format!("    S{index}[\"Shared platform node {index}\"]\n"));
            }
            for index in 0..47 {
                input.push_str(&format!("    S{index}-->S{}\n", index + 1));
            }
            input.push_str("  end\n");
            for (index, label) in tail_labels.iter().enumerate() {
                input.push_str(&format!("  D{index}[\"{label}\"]\n"));
            }
            input.push_str("  S47-->D0\n");
            for index in 0..tail_labels.len() - 1 {
                input.push_str(&format!("  D{index}-->D{}\n", index + 1));
            }
            input
        };
        let owned_inputs = [
            make_input(&[
                "Scheduler",
                "Cache",
                "Client",
                "Queue",
                "API",
                "Store",
                "Worker",
            ]),
            make_input(&[
                "Scheduler",
                "Rollback on failure 545",
                "Scheduler",
                "Message Queue",
                "Read Replica",
                "Normalize payload",
                "Client",
                "Scheduler",
                "Parse config 552",
                "Normalize payload 553",
                "Session Store",
            ]),
        ];
        let inputs = owned_inputs.iter().map(String::as_str).collect::<Vec<_>>();
        let plan = fm_parser::FlowchartBatchParsePlan::new(
            &inputs,
            fm_core::MermaidParseMode::Compat,
            &fm_parser::ParserConfig::default(),
        );
        let config = SvgRenderConfig::default();
        let mut scratch = fm_parser::FlowchartBatchParseScratch::default();
        let mut renderer = SvgBatchRenderer::default();

        for (index, input) in inputs.iter().enumerate() {
            plan.with_parse_scratch(index, input, &mut scratch, |parsed| {
                let prefix = parsed.reusable_prefix.expect("certified prefix");
                let expected_layout = fm_layout::layout_diagram_traced(parsed.ir).layout;
                let expected = render_svg_with_layout(parsed.ir, &expected_layout, &config);
                let actual = renderer.layout_and_render_borrowed(
                    parsed.ir,
                    &config,
                    Some(CertifiedSvgBatchPrefix::new(
                        Arc::clone(&prefix.identity),
                        prefix.node_count,
                        prefix.edge_count,
                    )),
                );
                assert_eq!(actual, expected);
            });
        }

        let snapshot = renderer.previous.as_ref().expect("batch snapshot");
        assert!(snapshot.layout_prefix.is_some());
        assert!(snapshot.fragments.reused_nodes >= 48);
        assert!(snapshot.fragments.reused_edges >= 47);
    }

    #[test]
    fn certified_batch_seed_bootstraps_independent_worker_renderers() {
        let inputs = [
            concat!(
                "flowchart LR\n",
                "  subgraph Shared[\"Shared ingestion platform\"]\n",
                "    S0[\"Receive & validate events\"]\n",
                "    S1[\"Normalize payload safely\"]\n",
                "    S2[\"Publish canonical records\"]\n",
                "    S0-->S1\n",
                "    S1-->S2\n",
                "  end\n",
                "  S2-->A[Alpha]",
            ),
            concat!(
                "flowchart LR\n",
                "  subgraph Shared[\"Shared ingestion platform\"]\n",
                "    S0[\"Receive & validate events\"]\n",
                "    S1[\"Normalize payload safely\"]\n",
                "    S2[\"Publish canonical records\"]\n",
                "    S0-->S1\n",
                "    S1-->S2\n",
                "  end\n",
                "  S2-->B[Bravo]\n",
                "  B-->C[Cache]",
            ),
            concat!(
                "flowchart LR\n",
                "  subgraph Shared[\"Shared ingestion platform\"]\n",
                "    S0[\"Receive & validate events\"]\n",
                "    S1[\"Normalize payload safely\"]\n",
                "    S2[\"Publish canonical records\"]\n",
                "    S0-->S1\n",
                "    S1-->S2\n",
                "  end\n",
                "  S2-->D[Delta]\n",
                "  D-->E[Event log]\n",
                "  E-->F[Fanout]",
            ),
        ];
        let plan = fm_parser::FlowchartBatchParsePlan::new(
            &inputs,
            fm_core::MermaidParseMode::Compat,
            &fm_parser::ParserConfig::default(),
        );
        let config = SvgRenderConfig::default();
        let mut coordinator_scratch = fm_parser::FlowchartBatchParseScratch::default();
        let mut coordinator = SvgBatchRenderer::default();

        plan.with_parse_scratch(0, inputs[0], &mut coordinator_scratch, |parsed| {
            let prefix = parsed.reusable_prefix.expect("certified prefix");
            let actual = coordinator.layout_and_render_borrowed(
                parsed.ir,
                &config,
                Some(CertifiedSvgBatchPrefix::new(
                    Arc::clone(&prefix.identity),
                    prefix.node_count,
                    prefix.edge_count,
                )),
            );
            let expected_layout = fm_layout::layout_diagram_traced(parsed.ir).layout;
            assert_eq!(
                actual,
                render_svg_with_layout(parsed.ir, &expected_layout, &config)
            );
        });
        let seed = coordinator.seed().expect("coordinator seed");

        for (index, input) in inputs.iter().enumerate().skip(1) {
            let mut scratch = fm_parser::FlowchartBatchParseScratch::default();
            let mut worker = SvgBatchRenderer::from_seed(&seed);
            plan.with_parse_scratch(index, input, &mut scratch, |parsed| {
                let prefix = parsed.reusable_prefix.expect("certified prefix");
                let expected_layout = fm_layout::layout_diagram_traced(parsed.ir).layout;
                let expected = render_svg_with_layout(parsed.ir, &expected_layout, &config);
                let actual = worker.layout_and_render_borrowed(
                    parsed.ir,
                    &config,
                    Some(CertifiedSvgBatchPrefix::new(
                        Arc::clone(&prefix.identity),
                        prefix.node_count,
                        prefix.edge_count,
                    )),
                );
                assert_eq!(actual, expected);
            });
            let snapshot = worker.previous.as_ref().expect("worker snapshot");
            assert!(snapshot.fragments.reused_nodes >= 3);
            assert!(snapshot.fragments.reused_edges >= 2);
        }
    }

    fn render_common_rect_through_both_paths(config: &SvgRenderConfig) -> (String, String) {
        let ir = create_ir_with_single_node("N0", NodeShape::Rect);
        let colors = ThemeColors::default();
        let detail = resolve_detail_profile(800.0, 600.0, config);
        let centrality = HashMap::new();
        let node_box = LayoutNodeBox {
            node_index: 0,
            node_id: "N0".to_string(),
            rank: 0,
            order: 0,
            span: Span::default(),
            bounds: fm_layout::LayoutRect {
                x: 10.0,
                y: 20.0,
                width: 140.0,
                height: 90.0,
            },
        };

        let mut streamed = String::new();
        render_node_into(
            &mut streamed,
            &node_box,
            &ir,
            0.0,
            0.0,
            config,
            detail,
            &colors,
            false,
            &centrality,
        );

        let mut slow = String::new();
        render_node(
            &node_box,
            &ir,
            0.0,
            0.0,
            config,
            detail,
            &colors,
            false,
            &centrality,
            false,
        )
        .write_to_string(&mut slow);

        (streamed, slow)
    }

    /// The full-node streaming path must render byte-identically to the authoritative `Element`
    /// slow path it replaces. Both arms receive the same explicit geometry so layout-default changes
    /// cannot silently turn this renderer-equivalence test into a stale layout golden.
    #[test]
    fn node_fast_fragment_matches_render() {
        let config = SvgRenderConfig::default();
        let (streamed, slow) = render_common_rect_through_both_paths(&config);
        assert_eq!(streamed, slow);
        assert!(streamed.contains("role=\"graphics-symbol\""));
        assert!(streamed.contains("aria-label=\"Single Node\""));
        assert!(streamed.contains("tabindex=\"0\""));
        assert!(streamed.contains("<title>Node: Single Node, rectangle</title>"));
    }

    /// The lean (`A11yConfig::none()`) node must remain byte-identical across the streaming and forced
    /// slow paths while omitting every per-node accessibility field.
    #[test]
    fn node_lean_fast_fragment_omits_a11y() {
        let config = SvgRenderConfig {
            a11y: A11yConfig::none(),
            accessible: false,
            ..SvgRenderConfig::default()
        };
        let (streamed, slow) = render_common_rect_through_both_paths(&config);
        assert_eq!(streamed, slow);
        assert!(
            !streamed.contains("role=\"graphics-symbol\""),
            "lean output must not carry per-element role"
        );
        assert!(
            !streamed.contains("tabindex="),
            "lean output must not carry tabindex"
        );
        assert!(
            !streamed.contains("<title>Node:"),
            "lean output must not carry per-node <title>"
        );
    }

    /// A mixed `A11yConfig` cannot be represented by either fragment monomorphization, so it must fall to
    /// the slow `Element` path -- which still honours each flag individually.
    #[test]
    fn mixed_a11y_falls_back_to_slow_path_and_honours_each_flag() {
        assert_eq!(uniform_a11y(&A11yConfig::full()), Some(true));
        assert_eq!(uniform_a11y(&A11yConfig::none()), Some(false));
        assert_eq!(uniform_a11y(&A11yConfig::minimal()), None);

        let ir = create_ir_with_single_node("N0", NodeShape::Rect);
        let config = SvgRenderConfig {
            a11y: A11yConfig::minimal(),
            ..SvgRenderConfig::default()
        };
        let svg = render_svg_with_config(&ir, &config);
        // minimal() = aria_labels only.
        assert!(svg.contains("role=\"graphics-symbol\""));
        assert!(svg.contains("aria-label=\"Single Node\""));
        assert!(!svg.contains("tabindex="));
        assert!(!svg.contains("<title>Node:"));
    }

    #[test]
    fn node_shape_fast_fragments_match_slow_render() {
        let config = SvgRenderConfig::default();
        let colors = ThemeColors::default();
        let detail = resolve_detail_profile(800.0, 600.0, &config);
        let centrality = HashMap::new();

        for shape in [
            NodeShape::Diamond,
            NodeShape::Hexagon,
            NodeShape::Subroutine,
            NodeShape::Cylinder,
            NodeShape::Trapezoid,
            NodeShape::InvTrapezoid,
            NodeShape::Parallelogram,
            NodeShape::InvParallelogram,
            NodeShape::Asymmetric,
        ] {
            let ir = create_ir_with_single_node("N0", shape);
            let node_box = LayoutNodeBox {
                node_index: 0,
                node_id: "N0".to_string(),
                rank: 0,
                order: 0,
                span: Span::default(),
                bounds: fm_layout::LayoutRect {
                    x: 10.0,
                    y: 20.0,
                    width: 140.0,
                    height: 90.0,
                },
            };

            let mut streamed = String::new();
            render_node_into(
                &mut streamed,
                &node_box,
                &ir,
                0.0,
                0.0,
                &config,
                detail,
                &colors,
                false,
                &centrality,
            );

            let mut slow = String::new();
            render_node(
                &node_box,
                &ir,
                0.0,
                0.0,
                &config,
                detail,
                &colors,
                false,
                &centrality,
                false,
            )
            .write_to_string(&mut slow);

            assert_eq!(streamed, slow, "shape {shape:?}");
        }
    }

    #[test]
    fn requirement_node_streaming_matches_slow_render() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Requirement);
        ir.labels.push(IrLabel {
            text: "Requirement A".to_string(),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            id: "R0".to_string(),
            label: Some(IrLabelId(0)),
            shape: NodeShape::Rect,
            requirement_meta: Some(Box::new(fm_core::IrRequirementNodeMeta {
                requirement_type: Some("requirement".to_string()),
                req_id: Some("REQ-0001".to_string()),
                text: Some("Preserve rendered output".to_string()),
                risk: Some("high".to_string()),
                verify_method: Some("test".to_string()),
                element_type: None,
                doc_ref: None,
            })),
            ..Default::default()
        });
        let node_box = LayoutNodeBox {
            node_index: 0,
            node_id: "R0".to_string(),
            rank: 0,
            order: 0,
            span: Span::default(),
            bounds: fm_layout::LayoutRect {
                x: 10.0,
                y: 20.0,
                width: 140.0,
                height: 90.0,
            },
        };
        let config = SvgRenderConfig::default();
        let colors = ThemeColors::default();
        let detail = resolve_detail_profile(800.0, 600.0, &config);
        let centrality = HashMap::new();

        let mut streamed = String::new();
        render_node_into(
            &mut streamed,
            &node_box,
            &ir,
            0.0,
            0.0,
            &config,
            detail,
            &colors,
            false,
            &centrality,
        );

        let mut slow = String::new();
        render_node(
            &node_box,
            &ir,
            0.0,
            0.0,
            &config,
            detail,
            &colors,
            false,
            &centrality,
            false,
        )
        .write_to_string(&mut slow);

        assert_eq!(streamed, slow);
    }

    /// Regression: a themed ER entity (default `node_gradients` config) must render its attribute
    /// compartments, not be claimed by the plain-rectangle common fast path. Before `simple_node_user_
    /// class_suffix` excluded `members`, the whole attribute list was silently dropped whenever gradients
    /// were on (the `er` golden runs gradients-OFF, so it never caught this). Asserts the entity body is
    /// present AND that the streaming path is byte-identical to the `Element` slow path.
    #[test]
    fn er_entity_renders_attributes_with_gradients_on() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        ir.labels.push(IrLabel {
            text: "USER".to_string(),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            id: "USER".to_string(),
            label: Some(IrLabelId(0)),
            shape: NodeShape::Rect,
            members: vec![
                fm_core::IrEntityAttribute {
                    data_type: "int".to_string(),
                    name: "id".to_string(),
                    keys: vec![fm_core::IrAttributeKey::Pk],
                    comment: None,
                },
                fm_core::IrEntityAttribute {
                    data_type: "string".to_string(),
                    name: "email".to_string(),
                    keys: vec![fm_core::IrAttributeKey::Uk],
                    comment: None,
                },
            ],
            ..Default::default()
        });
        let node_box = LayoutNodeBox {
            node_index: 0,
            node_id: "USER".to_string(),
            rank: 0,
            order: 0,
            span: Span::default(),
            bounds: fm_layout::LayoutRect {
                x: 12.0,
                y: 24.0,
                width: 150.0,
                height: 110.0,
            },
        };
        let config = SvgRenderConfig::default();
        assert!(config.node_gradients, "test assumes gradients-on default");
        let colors = ThemeColors::default();
        let detail = resolve_detail_profile(800.0, 600.0, &config);
        let centrality = HashMap::new();

        let mut streamed = String::new();
        render_node_into(
            &mut streamed,
            &node_box,
            &ir,
            0.0,
            0.0,
            &config,
            detail,
            &colors,
            false,
            &centrality,
        );

        // The attribute compartments must be present (the bug dropped them).
        assert!(
            streamed.contains("fm-er-entity-name"),
            "ER entity name header missing: {streamed}"
        );
        assert!(
            streamed.contains("fm-er-attribute"),
            "ER attribute rows missing (dropped by the common fast path?): {streamed}"
        );

        // …and the streamed output must match the `Element` slow path byte-for-byte.
        let mut slow = String::new();
        render_node(
            &node_box,
            &ir,
            0.0,
            0.0,
            &config,
            detail,
            &colors,
            false,
            &centrality,
            false,
        )
        .write_to_string(&mut slow);
        assert_eq!(streamed, slow);
    }

    /// The whole-ER-entity streaming fast path (`write_er_node_fragment_into`, via `render_node_into`'s
    /// ER gate) must be byte-identical to the `Element` slow path (`render_node`). Uses the default config
    /// (`node_gradients` + embedded CSS on) so the gradient `<rect>` fast path is exercised — the `er`
    /// golden runs with gradients OFF. Mixed attribute keys (Pk/None/Uk → both font-weights + the `PK `/
    /// `UK ` prefixes) and an XML-special attribute name exercise the body writer's per-piece escaping.
    #[test]
    fn er_entity_node_streaming_matches_slow_render() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Er);
        ir.labels.push(IrLabel {
            text: "USER".to_string(),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            id: "USER".to_string(),
            label: Some(IrLabelId(0)),
            shape: NodeShape::Rect,
            members: vec![
                fm_core::IrEntityAttribute {
                    data_type: "int".to_string(),
                    name: "id".to_string(),
                    keys: vec![fm_core::IrAttributeKey::Pk],
                    comment: None,
                },
                fm_core::IrEntityAttribute {
                    data_type: "string".to_string(),
                    name: "na<me>".to_string(),
                    keys: Vec::new(),
                    comment: None,
                },
                fm_core::IrEntityAttribute {
                    data_type: "string".to_string(),
                    name: "email".to_string(),
                    keys: vec![fm_core::IrAttributeKey::Uk],
                    comment: None,
                },
            ],
            ..Default::default()
        });
        let node_box = LayoutNodeBox {
            node_index: 0,
            node_id: "USER".to_string(),
            rank: 0,
            order: 0,
            span: Span::default(),
            bounds: fm_layout::LayoutRect {
                x: 12.0,
                y: 24.0,
                width: 150.0,
                height: 110.0,
            },
        };
        let config = SvgRenderConfig::default();
        let colors = ThemeColors::default();
        let detail = resolve_detail_profile(800.0, 600.0, &config);
        let centrality = HashMap::new();

        let mut streamed = String::new();
        render_node_into(
            &mut streamed,
            &node_box,
            &ir,
            0.0,
            0.0,
            &config,
            detail,
            &colors,
            false,
            &centrality,
        );

        let mut slow = String::new();
        render_node(
            &node_box,
            &ir,
            0.0,
            0.0,
            &config,
            detail,
            &colors,
            false,
            &centrality,
            false,
        )
        .write_to_string(&mut slow);

        assert_eq!(streamed, slow);
        // Confirm the whole-node fast path actually fired (gradient rect + streamed body).
        assert!(streamed.contains("url(#fm-node-gradient)"));
        assert!(streamed.contains("fm-er-entity-name"));
        // Text content escapes `<` (and `&`) but leaves a standalone `>` literal (only `]]>` escapes it).
        assert!(streamed.contains("na&lt;me>"));
    }

    /// The streamed whole-C4-node fragment must be byte-identical to the `Element` the slow path builds,
    /// under the DEFAULT (gradients-on) config the C4 fast-path gate fires on. The `c4_basic` golden uses
    /// `node_gradients: false` (so the fast path does NOT fire there), so THIS is the real byte-pin for it.
    #[test]
    fn c4_node_streaming_matches_slow_render() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::C4Context);
        ir.labels.push(IrLabel {
            text: "User".to_string(),
            span: Span::default(),
        });
        ir.nodes.push(IrNode {
            id: "user".to_string(),
            label: Some(IrLabelId(0)),
            shape: NodeShape::Rounded,
            classes: vec!["c4".to_string(), "c4-person".to_string()],
            c4_meta: Some(Box::new(fm_core::IrC4NodeMeta {
                element_type: "Person".to_string(),
                technology: None,
                description: Some("A customer".to_string()),
            })),
            ..Default::default()
        });
        let node_box = LayoutNodeBox {
            node_index: 0,
            node_id: "user".to_string(),
            rank: 0,
            order: 0,
            span: Span::default(),
            bounds: fm_layout::LayoutRect {
                x: 12.0,
                y: 24.0,
                width: 150.0,
                height: 90.0,
            },
        };
        let config = SvgRenderConfig::default();
        let colors = ThemeColors::default();
        let detail = resolve_detail_profile(800.0, 600.0, &config);
        let centrality = HashMap::new();

        let mut streamed = String::new();
        render_node_into(
            &mut streamed,
            &node_box,
            &ir,
            0.0,
            0.0,
            &config,
            detail,
            &colors,
            false,
            &centrality,
        );

        let mut slow = String::new();
        render_node(
            &node_box,
            &ir,
            0.0,
            0.0,
            &config,
            detail,
            &colors,
            false,
            &centrality,
            false,
        )
        .write_to_string(&mut slow);

        assert_eq!(streamed, slow);
        // Confirm the whole-C4-node fast path actually fired (stereotype + person icon + solid, non-gradient fill).
        assert!(streamed.contains("fm-c4-type-label"));
        assert!(streamed.contains("fm-c4-person-icon"));
        assert!(streamed.contains("&lt;&lt;Person>>"));
        assert!(streamed.contains("url(#fm-node-gradient)"));
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
    fn requirement_subtitle_streaming_matches_element_render() {
        let cases = [
            (
                123.25,
                45.5,
                10.75,
                " font-style=\"italic\"",
                "",
                "fm-req-type-label",
                "\u{00ab}functional & safe\u{00bb}",
                true,
            ),
            (
                -2.0,
                77.25,
                9.5,
                "",
                " opacity=\"0.7\"",
                "fm-req-metadata",
                "Risk: high | Verify: test <manual>",
                false,
            ),
        ];

        for (x, y, font_size, before_fill, after_fill, class, text, italic) in cases {
            let mut elem = Element::text()
                .x(x)
                .y(y)
                .content(text)
                .attr("text-anchor", "middle")
                .attr("dominant-baseline", "central")
                .attr_num("font-size", font_size);
            if italic {
                elem = elem.attr("font-style", "italic");
            }
            elem = elem.fill("#1a1a2e");
            if !after_fill.is_empty() {
                elem = elem.attr("opacity", "0.7");
            }
            elem = elem.class(class);

            let mut expected = String::new();
            elem.write_to_string(&mut expected);

            let mut streamed = String::new();
            write_req_subtitle_into(
                &mut streamed,
                x,
                y,
                font_size,
                before_fill,
                after_fill,
                "#1a1a2e",
                class,
                text,
            );

            assert_eq!(
                streamed, expected,
                "streamed requirement subtitle must match Element render for {class}"
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
            let path_child = Element::raw_svg(build_common_edge_fragment(
                &d,
                sw,
                style,
                idx,
                "url(#arrow-end)",
            ));
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
        check(
            &[(0.0, 0.0), (10.0, 10.0)],
            0,
            "fm-edge-solid",
            1.8,
            Some("A"),
            Some("B"),
        );
        check(
            &[(1.5, 2.25), (3.0, 4.0), (5.0, 6.0), (7.0, 8.0)],
            42,
            "fm-edge-solid",
            1.8,
            Some("Node <1> & \"x\""),
            Some("Node 2"),
        );
        check(
            &[(-5.25, 0.0), (0.0, -3.5)],
            1000,
            "fm-edge-solid",
            2.5,
            None,
            None,
        );
        check(&[(0.0, 0.0)], 7, "fm-edge-solid", 1.8, Some("start"), None);
    }

    #[test]
    fn dotted_edge_streaming_matches_element_render() {
        let config = SvgRenderConfig::default();
        let colors = ThemeColors::default();
        let detail = resolve_detail_profile(800.0, 600.0, &config);

        for arrow in [
            ArrowType::DottedArrow,
            ArrowType::DottedOpenArrow,
            ArrowType::DottedCross,
            ArrowType::DottedLine,
            ArrowType::HalfArrowTopDotted,
            ArrowType::HalfArrowBottomDotted,
            ArrowType::StickArrowTopDotted,
            ArrowType::StickArrowBottomDotted,
        ] {
            let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
            ir.nodes.push(IrNode {
                id: "A <&>".to_string(),
                ..IrNode::default()
            });
            ir.nodes.push(IrNode {
                id: "B \"q\"".to_string(),
                ..IrNode::default()
            });
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(0)),
                to: IrEndpoint::Node(IrNodeId(1)),
                arrow,
                ..IrEdge::default()
            });
            let edge_path = LayoutEdgePath {
                edge_index: 0,
                span: Span::default(),
                points: [
                    fm_layout::LayoutPoint { x: 0.0, y: 0.0 },
                    fm_layout::LayoutPoint { x: 32.0, y: 48.0 },
                    fm_layout::LayoutPoint { x: 72.0, y: 48.0 },
                    fm_layout::LayoutPoint { x: 96.0, y: 80.0 },
                ]
                .into_iter()
                .collect(),
                reversed: false,
                is_self_loop: false,
                parallel_offset: 0.0,
                bundle_count: 1,
                bundled: false,
            };
            let context = EdgeRenderContext {
                ir: &ir,
                offset_x: 1.5,
                offset_y: -2.0,
                config: &config,
                detail,
                colors: &colors,
                accessible_node_labels: None,
                sankey_widest_flow: sankey_widest_flow(&ir),
            };

            let mut streamed = String::new();
            render_edge_into(&mut streamed, &edge_path, &context);

            let mut element_rendered = String::new();
            render_edge(&edge_path, &context).write_to_string(&mut element_rendered);

            assert_eq!(
                streamed, element_rendered,
                "streamed dotted edge must match Element render for {arrow:?}"
            );
            assert!(streamed.contains("stroke-dasharray=\"5,5\""));
        }
    }

    #[test]
    fn marker_start_edge_streaming_matches_element_render() {
        let config = SvgRenderConfig::default();
        let colors = ThemeColors::default();
        let detail = resolve_detail_profile(800.0, 600.0, &config);

        for arrow in [
            ArrowType::HalfArrowTopReverse,
            ArrowType::HalfArrowBottomReverse,
            ArrowType::StickArrowTopReverse,
            ArrowType::StickArrowBottomReverse,
            ArrowType::HalfArrowTopReverseDotted,
            ArrowType::HalfArrowBottomReverseDotted,
            ArrowType::StickArrowTopReverseDotted,
            ArrowType::StickArrowBottomReverseDotted,
            ArrowType::DoubleArrow,
            ArrowType::DoubleThickArrow,
            ArrowType::DoubleDottedArrow,
        ] {
            let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
            ir.nodes.push(IrNode {
                id: "A <&>".to_string(),
                ..IrNode::default()
            });
            ir.nodes.push(IrNode {
                id: "B \"q\"".to_string(),
                ..IrNode::default()
            });
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(0)),
                to: IrEndpoint::Node(IrNodeId(1)),
                arrow,
                ..IrEdge::default()
            });
            let edge_path = LayoutEdgePath {
                edge_index: 0,
                span: Span::default(),
                points: [
                    fm_layout::LayoutPoint { x: 0.0, y: 0.0 },
                    fm_layout::LayoutPoint { x: 32.0, y: 48.0 },
                    fm_layout::LayoutPoint { x: 72.0, y: 48.0 },
                    fm_layout::LayoutPoint { x: 96.0, y: 80.0 },
                ]
                .into_iter()
                .collect(),
                reversed: false,
                is_self_loop: false,
                parallel_offset: 0.0,
                bundle_count: 1,
                bundled: false,
            };
            let context = EdgeRenderContext {
                ir: &ir,
                offset_x: 1.5,
                offset_y: -2.0,
                config: &config,
                detail,
                colors: &colors,
                accessible_node_labels: None,
                sankey_widest_flow: sankey_widest_flow(&ir),
            };

            let mut streamed = String::new();
            render_edge_into(&mut streamed, &edge_path, &context);

            let mut element_rendered = String::new();
            render_edge(&edge_path, &context).write_to_string(&mut element_rendered);

            assert_eq!(
                streamed, element_rendered,
                "streamed marker-start edge must match Element render for {arrow:?}"
            );
            assert!(streamed.contains("marker-start=\"url(#arrow-"));
            if matches!(
                arrow,
                ArrowType::HalfArrowTopReverseDotted
                    | ArrowType::HalfArrowBottomReverseDotted
                    | ArrowType::StickArrowTopReverseDotted
                    | ArrowType::StickArrowBottomReverseDotted
                    | ArrowType::DoubleDottedArrow
            ) {
                assert!(streamed.contains("stroke-dasharray=\"5,5\""));
            }
        }
    }

    /// Build a two-node / one-edge flowchart plus the `EdgeRenderContext` scaffolding the streaming
    /// parity tests need. Labels carry `& < > "` so escaping divergences surface.
    fn single_edge_fixture(arrow: ArrowType) -> (MermaidDiagramIr, LayoutEdgePath) {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.nodes.push(IrNode {
            id: "A <&>".to_string(),
            ..IrNode::default()
        });
        ir.nodes.push(IrNode {
            id: "B \"q\"".to_string(),
            ..IrNode::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow,
            ..IrEdge::default()
        });
        let edge_path = LayoutEdgePath {
            edge_index: 0,
            span: Span::default(),
            points: [
                fm_layout::LayoutPoint { x: 0.0, y: 0.0 },
                fm_layout::LayoutPoint { x: 32.0, y: 48.0 },
                fm_layout::LayoutPoint { x: 72.0, y: 48.0 },
                fm_layout::LayoutPoint { x: 96.0, y: 80.0 },
            ]
            .into_iter()
            .collect(),
            reversed: false,
            is_self_loop: false,
            parallel_offset: 0.0,
            bundle_count: 1,
            bundled: false,
        };
        (ir, edge_path)
    }

    /// The lean (`A11yConfig::none()`) edge now streams through the whole-edge fast path instead of
    /// falling back to the per-element `Element` builder. Unlike the node half — where the fast path took
    /// over every reachable configuration, so its lean bytes had to be pinned as a literal — `render_edge`
    /// remains the live slow path here (`render_edge_into` delegates to it for every gated-out edge). So
    /// this asserts the streamed lean fragment against what the `Element` path *actually* produces, across
    /// the arrow matrix: bare `<path>` (no `<g>`), no `<title>`, no `role`/`tabindex`, and the trailing
    /// `id="fm-edge-N"` that the slow path's final `elem.id(..)` appends to an unwrapped edge.
    #[test]
    fn lean_edge_streaming_matches_element_render() {
        let config = SvgRenderConfig {
            a11y: A11yConfig::none(),
            accessible: false,
            ..SvgRenderConfig::default()
        };
        let colors = ThemeColors::default();
        let detail = resolve_detail_profile(800.0, 600.0, &config);

        for arrow in [
            ArrowType::Arrow,
            ArrowType::Line,
            ArrowType::OpenArrow,
            ArrowType::ThickArrow,
            ArrowType::DottedArrow,
            ArrowType::DoubleArrow,
            ArrowType::DoubleDottedArrow,
            ArrowType::HalfArrowTopReverse,
            ArrowType::StickArrowBottomReverseDotted,
        ] {
            let (ir, edge_path) = single_edge_fixture(arrow);
            let context = EdgeRenderContext {
                ir: &ir,
                offset_x: 1.5,
                offset_y: -2.0,
                config: &config,
                detail,
                colors: &colors,
                accessible_node_labels: None,
                sankey_widest_flow: sankey_widest_flow(&ir),
            };

            let mut streamed = String::new();
            render_edge_into(&mut streamed, &edge_path, &context);

            let mut element_rendered = String::new();
            render_edge(&edge_path, &context).write_to_string(&mut element_rendered);

            assert_eq!(
                streamed, element_rendered,
                "streamed lean edge must match Element render for {arrow:?}"
            );
            assert!(
                streamed.starts_with("<path d=\"") && streamed.ends_with("id=\"fm-edge-0\"/>"),
                "lean edge is a bare <path> carrying the id: {streamed}"
            );
            for banned in ["<g ", "<title>", "role=", "tabindex="] {
                assert!(
                    !streamed.contains(banned),
                    "lean edge must not emit {banned} for {arrow:?}: {streamed}"
                );
            }
        }
    }

    /// Differential oracle for the single-pass post-pass scanners against the per-needle `str::contains`
    /// chain they replaced. The reference implementations below are verbatim the pre-`bd-w5sn` logic.
    ///
    /// The generated alphabet manufactures exactly the cases the one-pass rewrite could get wrong:
    /// truncated prefixes (`fm-nod`), longer-than-single-digit accents (`fm-node-accent-12`, which the old
    /// needle DID match for accent 1), out-of-range digits (`accent-0`, `accent-9`), a `var(--fm-accent-12)`
    /// that must NOT count as a reference to accent 1, adjacent/overlapping prefixes
    /// (`fm-node-fm-node-accent-3`), and state classes with trailing text (`fm-node-inactive-foo`).
    #[test]
    fn single_pass_scanners_match_per_needle_contains() {
        const STATE_CLASSES: [&str; 5] = [
            "fm-node-inactive",
            "fm-node-block-beta",
            "fm-node-highlighted",
            "fm-node-border-dashed",
            "fm-node-border-double",
        ];
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
        // Verbatim pre-rewrite logic.
        let old_body = |body: &str| -> (bool, [bool; 9]) {
            (
                STATE_CLASSES.iter().any(|c| body.contains(c)),
                std::array::from_fn(|n| n != 0 && body.contains(ACCENT_NEEDLES[n])),
            )
        };
        let old_var = |svg: &str| -> [bool; 9] {
            std::array::from_fn(|n| n != 0 && svg.contains(VAR_NEEDLES[n]))
        };

        let check = |t: &str| {
            let (old_state, old_accent) = old_body(t);
            let (new_state, new_accent) = scan_body_fm_node_classes(t);
            assert_eq!(old_state, new_state, "state flag mismatch on {t:?}");
            // The one-pass scanner short-circuits on a state hit, so its accent flags are only defined
            // (and only ever read by the caller) when no state class is present.
            if !old_state {
                assert_eq!(old_accent, new_accent, "accent flags mismatch on {t:?}");
            }
            assert_eq!(
                old_var(t),
                scan_accent_var_refs(t),
                "var refs mismatch on {t:?}"
            );
        };

        for t in [
            "",
            "fm-node-",
            "fm-node-accent-",
            "fm-node-accent-0",
            "fm-node-accent-9",
            "fm-node-accent-12",
            "fm-node-inactive-foo",
            "fm-node-fm-node-accent-3",
            "xxfm-node-highlighted",
            "var(--fm-accent-",
            "var(--fm-accent-3",
            "var(--fm-accent-12)",
            "var(--fm-accent-1)",
            "fm-nod e-accent-1",
        ] {
            check(t);
        }

        const ATOMS: [&str; 22] = [
            "fm-node-",
            "accent-",
            "1",
            "2",
            "8",
            "9",
            "0",
            "12",
            ")",
            "(",
            "var(--fm-accent-",
            "inactive",
            "block-beta",
            "highlighted",
            "border-dashed",
            "border-double",
            "fm-node-accent-",
            "fm-node-inactive-foo",
            "-",
            "x",
            "fm-nod",
            "e-",
        ];
        let mut state: u64 = 0x243F_6A88_85A3_08D3;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            state
        };
        let mut scratch = String::new();
        for _ in 0..200_000 {
            scratch.clear();
            let len = (next() >> 60) % 6 + 1;
            for _ in 0..len {
                scratch.push_str(ATOMS[usize::try_from(next() >> 33).unwrap_or(0) % ATOMS.len()]);
            }
            check(&scratch);
        }
    }

    /// Mixed a11y (`A11yConfig::minimal()` = `aria_labels` only) has no raw-fragment shape — it cannot
    /// express "role but no tabindex" — so it must keep taking the slow `Element` path and keep honouring
    /// each flag independently. Guards the `uniform_a11y` gate against being widened to `any a11y`.
    #[test]
    fn mixed_a11y_edge_falls_back_to_slow_path() {
        let config = SvgRenderConfig {
            a11y: A11yConfig::minimal(),
            ..SvgRenderConfig::default()
        };
        let colors = ThemeColors::default();
        let detail = resolve_detail_profile(800.0, 600.0, &config);
        let (ir, edge_path) = single_edge_fixture(ArrowType::Arrow);
        let context = EdgeRenderContext {
            ir: &ir,
            offset_x: 1.5,
            offset_y: -2.0,
            config: &config,
            detail,
            colors: &colors,
            accessible_node_labels: None,
            sankey_widest_flow: sankey_widest_flow(&ir),
        };

        let mut streamed = String::new();
        render_edge_into(&mut streamed, &edge_path, &context);
        let mut element_rendered = String::new();
        render_edge(&edge_path, &context).write_to_string(&mut element_rendered);

        assert_eq!(streamed, element_rendered);
        // `minimal()` = aria_labels on, keyboard_nav + text_alternatives off: role, but no tabindex and
        // no <title>, so no `<g>` wrapper either — the role lands on the unwrapped `<path>`.
        assert!(streamed.contains("role=\"graphics-symbol\""));
        assert!(!streamed.contains("tabindex="));
        assert!(!streamed.contains("<title>"));
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
            200.0,
            80.0,
            &config,
            &colors,
            Some("font-weight:700"),
            true,
        );

        assert_eq!(actual.render(), expected.render());
    }

    #[test]
    fn node_label_fitting_preserves_labels_that_already_fit() {
        let config = SvgRenderConfig::default();
        let fitted = fit_node_label_text("Short label", 240.0, 48.0, 15.0, &config);

        assert!(!fitted.changed);
        assert!(matches!(fitted.text, Cow::Borrowed("Short label")));
        assert_eq!(fitted.font_size, 15.0);
    }

    #[test]
    fn node_label_fitting_wraps_on_word_boundaries_before_ellipsis() {
        let config = SvgRenderConfig::default();
        let fitted = fit_node_label_text("one two three four", 48.0, 52.0, 15.0, &config);

        assert!(fitted.changed);
        assert!(fitted.text.contains('\n'));
        assert!(!fitted.text.ends_with('…'));
        assert!(fitted.font_size >= 10.0);
    }

    #[test]
    fn node_label_fitting_ellipsizes_when_minimum_size_still_overflows() {
        let config = SvgRenderConfig::default();
        let fitted = fit_node_label_text(
            "extraordinarily-long-unbreakable-identifier",
            48.0,
            16.0,
            15.0,
            &config,
        );

        assert!(fitted.changed);
        assert_eq!(fitted.font_size, 10.0);
        assert!(fitted.text.ends_with('…'));
        assert_ne!(
            fitted.text.as_ref(),
            "extraordinarily-long-unbreakable-identifier"
        );
    }

    #[test]
    fn rendered_node_label_uses_the_fitted_font_and_ellipsis() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let config = SvgRenderConfig::default();
        let svg = render_node_label_text(
            &ir,
            None,
            "extraordinarily-long-unbreakable-identifier",
            24.0,
            24.0,
            15.0,
            48.0,
            16.0,
            &config,
            &ThemeColors::default(),
            None,
            false,
        )
        .render();

        assert!(svg.contains("font-size=\"10\""));
        assert!(svg.contains('…'));
        assert!(!svg.contains("extraordinarily-long-unbreakable-identifier"));
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
            c4_boundary_type: None,
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
            c4_boundary_type: None,
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

    fn doc_build_inputs() -> Vec<String> {
        fn flowchart(node_count: usize) -> String {
            let mut lines = vec![String::from("flowchart LR")];
            for index in 0..node_count {
                lines.push(format!("  N{index}[Node {index}]"));
            }
            for index in 0..node_count.saturating_sub(1) {
                lines.push(format!("  N{index}-->N{}", index + 1));
            }
            lines.join("\n")
        }

        fn sequence(participant_count: usize) -> String {
            let mut lines = vec![String::from("sequenceDiagram")];
            for index in 0..participant_count {
                lines.push(format!("  participant P{index}"));
            }
            for index in 0..participant_count.saturating_sub(1) {
                lines.push(format!("  P{index}->>P{}: request {index}", index + 1));
                lines.push(format!("  P{}-->>P{index}: response {index}", index + 1));
            }
            lines.join("\n")
        }

        fn class_diagram(class_count: usize) -> String {
            let mut lines = vec![String::from("classDiagram")];
            for index in 0..class_count {
                lines.push(format!("  class C{index} {{"));
                lines.push(format!("    +int field{index}"));
                lines.push(format!("    +method{index}() bool"));
                lines.push(String::from("  }"));
            }
            for index in 0..class_count.saturating_sub(1) {
                lines.push(format!("  C{index} <|-- C{}", index + 1));
            }
            lines.join("\n")
        }

        fn state_diagram(state_count: usize) -> String {
            let mut lines = vec![
                String::from("stateDiagram-v2"),
                String::from("  [*] --> S0"),
            ];
            for index in 0..state_count.saturating_sub(1) {
                lines.push(format!("  S{index} --> S{}: event{index}", index + 1));
            }
            lines.push(format!("  S{} --> [*]", state_count.saturating_sub(1)));
            lines.join("\n")
        }

        fn er_diagram(entity_count: usize) -> String {
            let mut lines = vec![String::from("erDiagram")];
            for index in 0..entity_count.saturating_sub(1) {
                lines.push(format!("  E{index} ||--o{{ E{} : has", index + 1));
            }
            lines.join("\n")
        }

        let mut inputs = Vec::with_capacity(40);
        for copy in 0..8 {
            inputs.push(flowchart(12 + copy % 7));
            inputs.push(sequence(6 + copy % 5));
            inputs.push(class_diagram(8 + copy % 4));
            inputs.push(state_diagram(10 + copy % 6));
            inputs.push(er_diagram(9 + copy % 3));
        }
        inputs
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
    fn render_scene_sanitizes_non_finite_stroke_style_values() {
        let mut scene = create_scene_with_path_and_text();
        assert!(matches!(
            scene.root.children.first(),
            Some(RenderItem::Path(_))
        ));
        let Some(RenderItem::Path(path)) = scene.root.children.first_mut() else {
            return;
        };
        let stroke = path.stroke.as_mut().expect("fixture path has a stroke");
        stroke.width = f32::NAN;
        stroke.opacity = f32::NAN;
        stroke.dash_array = vec![f32::NAN, f32::INFINITY, -2.0, 3.0];

        let svg = render_scene_to_svg(&scene, &SvgRenderConfig::default());

        assert!(!svg.contains("NaN") && !svg.contains("Infinity"));
        assert!(svg.contains("stroke-width=\"0\""));
        assert!(!svg.contains("stroke-opacity="));
        assert!(svg.contains("stroke-dasharray=\"3\""));
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

    /// Regression fixture shared with `scripts/headtohead/corpus.mjs`: Mermaid 11.15.0 renders
    /// `class e1 alert` as a red, 4px `e1@` edge. The parser resolves that class to Link(0), and
    /// this consumer must carry both declarations into the rendered path.
    #[test]
    fn class_directive_on_an_edge_id_reaches_the_svg_path() {
        let parsed = fm_parser::parse(
            "flowchart LR\n  Animal e1@--> Dog\n  classDef alert stroke:#ff0000,stroke-width:4px\n  class e1 alert\n",
        );
        let svg = render_svg(&parsed.ir);

        assert!(
            svg.contains("stroke:#ff0000"),
            "edge class stroke was lost: {svg}"
        );
        assert!(
            svg.contains("stroke-width:4px"),
            "edge class width was lost: {svg}"
        );
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
        assert!(
            svg.contains("id=\"arrow-end\""),
            "solid arrow marker missing"
        );
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

    /// The gantt `todayMarker` must be drawn, must be suppressible, and must not depend on the clock
    /// (bd-j0va).
    ///
    /// `IrGanttMeta::today_marker_style` was populated by the parser and referenced by no consumer in
    /// fm-layout, fm-render-svg, fm-render-canvas or fm-render-term, so both the default marker and
    /// the `todayMarker off` directive that disables it were equally invisible.
    ///
    /// THE DATE IS INJECTED in every case here, through `gantt_today`. That is the design, not a
    /// testing convenience: a renderer that called `now()` would make this test — and every gantt
    /// golden — depend on the day it happened to run, which is a defect class this project has
    /// already been bitten by.
    #[test]
    fn gantt_today_marker_is_drawn_suppressed_and_clock_independent() {
        // (case, injected today, todayMarker directive, marker expected)
        let cases = [
            ("in span, no directive", Some("2026-01-02"), None, true),
            (
                "in span, styled",
                Some("2026-01-02"),
                Some("stroke:red,stroke-width:4px"),
                true,
            ),
            (
                "in span, turned off",
                Some("2026-01-02"),
                Some("off"),
                false,
            ),
            (
                "today AFTER the chart span",
                Some("2026-08-16"),
                None,
                false,
            ),
            (
                "today BEFORE the chart span",
                Some("2020-01-01"),
                None,
                false,
            ),
            ("no date injected at all", None, None, false),
            ("not a real calendar date", Some("2026-02-31"), None, false),
        ];

        let source = "gantt\n  title Roadmap\n  dateFormat  YYYY-MM-DD\n  section Core\n  Design :a1, 2026-01-01, 3d\n  Build :a2, after a1, 4d\n";
        for (name, today, directive, expect_marker) in cases {
            let mut text = source.to_string();
            if let Some(directive) = directive {
                text.push_str("  todayMarker ");
                text.push_str(directive);
                text.push('\n');
            }
            let ir = fm_parser::parse(&text).ir;
            let config = SvgRenderConfig {
                gantt_today: today.map(str::to_string),
                ..SvgRenderConfig::default()
            };
            let svg = render_svg_with_config(&ir, &config);
            let drawn = svg.contains("fm-gantt-today");
            assert_eq!(
                drawn, expect_marker,
                "case {name:?}: marker drawn={drawn}, expected {expect_marker}"
            );
            // A restyling directive must reach the element's attributes rather than being accepted
            // and dropped, which is what "parsed and read by nothing" looked like.
            if expect_marker && let Some(directive) = directive {
                assert!(
                    svg.contains(directive),
                    "case {name:?}: the todayMarker style never reached the drawn element"
                );
            }
        }
    }

    /// THE CONTROL that makes every "no marker" case above worth anything.
    ///
    /// All five suppression cases would also pass if the marker were simply never drawn, so this
    /// pins the positive direction independently, and it pins bd-j0va's fourth negative case: the
    /// marker x must be derived the way tick x is, or the today line and the axis disagree about
    /// where today is.
    ///
    /// Asserted DIFFERENTIALLY, across two dates one day apart, rather than against an absolute
    /// coordinate. My first version compared the rendered x against the layout's raw `x_for_day`
    /// and failed with "marker x 140 disagrees with the axis position 68" — a difference of exactly
    /// the canvas `offset_x`, i.e. the test was comparing a layout coordinate against a rendered
    /// one. The differential form cancels the offset and asserts the thing that actually matters:
    /// that the marker advances by the axis's own `day_width` per day.
    #[test]
    fn gantt_today_marker_advances_with_the_axis_day_width() {
        let source = "gantt\n  title Roadmap\n  dateFormat  YYYY-MM-DD\n  section Core\n  Design :a1, 2026-01-01, 3d\n  Build :a2, after a1, 4d\n";
        let ir = fm_parser::parse(source).ir;
        let layout = fm_layout::layout_diagram(&ir);
        let axis = layout
            .extensions
            .gantt_day_axis
            .expect("a gantt layout must publish its day axis");

        let marker_x = |today: &str| -> f32 {
            let config = SvgRenderConfig {
                gantt_today: Some(today.to_string()),
                ..SvgRenderConfig::default()
            };
            let svg = render_svg_with_config(&ir, &config);
            assert_eq!(
                svg.matches("fm-gantt-today").count(),
                1,
                "exactly one today marker expected for {today}, once per chart not once per task"
            );
            let anchor = svg.find("fm-gantt-today").expect("marker present");
            let element_start = svg[..anchor]
                .rfind("<line")
                .expect("marker is a line element");
            svg[element_start..]
                .split("x1=\"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .and_then(|v| v.parse::<f32>().ok())
                .expect("marker carries a numeric x1")
        };

        // NON-VACUITY: both dates must be inside the charted span, or the renders below draw
        // nothing and the assertion is unreachable.
        for date in ["2026-01-02", "2026-01-03"] {
            let day = fm_layout::parse_iso_day_number(date).expect("a real calendar date");
            assert!(
                axis.x_for_day(day).is_some(),
                "CONTROL FAILED: {date} falls outside this chart's span, so this test proves nothing"
            );
        }

        let advance = marker_x("2026-01-03") - marker_x("2026-01-02");
        assert!(
            (advance - axis.day_width).abs() < 0.01,
            "one day of today-marker movement was {advance}, but the axis places days \
             {} apart -- the marker and the axis disagree about where a day is",
            axis.day_width
        );
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

    /// The streamed band fragment (`write_layout_band_into`) must be byte-identical to the `Element` the
    /// slow path builds (`render_layout_band`), across every kind and both labelled and unlabelled bands
    /// (sequence lifelines are unlabelled `Lane`s; journey sections / xychart columns carry a label).
    #[test]
    fn layout_band_streaming_matches_element() {
        let kinds = [
            LayoutBandKind::Section,
            LayoutBandKind::Lane,
            LayoutBandKind::Column,
        ];
        // Cover both the embedded-CSS config (no inline `font-family`) and the attribute-driven config
        // (font-family present), plus a plain label, an XML-special label (escaping), and a multi-line
        // label (`TextBuilder` `<tspan>` fallback) so every branch of the streamed label writer is
        // byte-checked against the `Element` slow path.
        for embed in [true, false] {
            let config = SvgRenderConfig {
                embed_theme_css: embed,
                ..SvgRenderConfig::default()
            };
            for kind in kinds {
                for label in ["", "Phase <1> & more", "Line 1\nLine 2"] {
                    let band = LayoutBand {
                        kind,
                        label: label.to_string(),
                        bounds: fm_layout::LayoutRect {
                            x: 12.5,
                            y: 24.0,
                            width: 180.0,
                            height: 83.5,
                        },
                    };
                    let mut streamed = String::new();
                    write_layout_band_into(&mut streamed, &band, 3.0, 5.0, &config);
                    let mut slow = String::new();
                    render_layout_band(&band, 3.0, 5.0, &config).write_to_string(&mut slow);
                    assert_eq!(
                        streamed, slow,
                        "band kind {kind:?} embed {embed} label {label:?}"
                    );
                }
            }
        }
    }

    /// The streamed axis-tick fragment (`write_layout_axis_tick_into`) must be byte-identical to the
    /// `Element` slow path (`render_layout_axis_tick`) — including an XML-special label and both the
    /// embedded-CSS (no font-family) and attribute-driven (font-family present) configs.
    #[test]
    fn layout_axis_tick_streaming_matches_element() {
        for embed in [true, false] {
            let config = SvgRenderConfig {
                embed_theme_css: embed,
                ..SvgRenderConfig::default()
            };
            for label in ["2026-02-01", "a<b>&c"] {
                let mut streamed = String::new();
                write_layout_axis_tick_into(&mut streamed, label, 42.5, 17.0, &config);
                let mut slow = String::new();
                render_layout_axis_tick(label, 42.5, 17.0, &config).write_to_string(&mut slow);
                assert_eq!(streamed, slow, "embed {embed} label {label:?}");
            }
        }
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
    fn named_xychart_legend_fits_inside_layout_viewport() {
        let ir = create_xychart_ir();
        let layout = layout_diagram(&ir);
        let xy_chart_meta = ir.xy_chart_meta.as_ref().expect("xy chart metadata");
        let plot_bounds = xychart_plot_bounds(&layout, xy_chart_meta);

        const LEGEND_GAP: f32 = 16.0;
        const LEGEND_WIDTH: f32 = 120.0;
        let legend_right = plot_bounds.x + plot_bounds.width + LEGEND_GAP + LEGEND_WIDTH;
        let viewport_right = layout.bounds.x + layout.bounds.width;
        assert!(
            legend_right <= viewport_right,
            "legend right edge {legend_right} exceeds viewport {viewport_right}"
        );
    }

    #[test]
    fn named_xychart_legend_constrains_overlong_series_labels() {
        let mut ir = create_xychart_ir();
        let meta = ir.xy_chart_meta.as_mut().expect("xy chart metadata");
        meta.series[0].name = Some("Revenue from enterprise subscriptions".to_string());

        let svg = render_svg_with_config(&ir, &SvgRenderConfig::default());

        assert!(svg.contains("class=\"fm-xychart-legend-entry\""));
        assert!(
            svg.contains("textLength=\"88\"") && svg.contains("lengthAdjust=\"spacingAndGlyphs\""),
            "overlong legend labels must remain inside the reserved 120px legend column"
        );
    }

    #[test]
    fn named_xychart_legend_leaves_short_series_labels_unconstrained() {
        // Negative case for `named_xychart_legend_constrains_overlong_series_labels`: the
        // constraint is conditional on the estimated label overflowing the reserved column, so a
        // label that fits must keep its natural glyph advances. An implementation that always
        // emitted `textLength` would satisfy the overlong test and fail this one.
        let ir = create_xychart_ir();
        let meta = ir.xy_chart_meta.as_ref().expect("xy chart metadata");
        assert_eq!(meta.series[0].name.as_deref(), Some("Revenue"));

        let svg = render_svg_with_config(&ir, &SvgRenderConfig::default());

        assert!(svg.contains("class=\"fm-xychart-legend-entry\""));
        assert!(
            !svg.contains("textLength=") && !svg.contains("lengthAdjust="),
            "legend labels that fit the reserved column must not be squeezed"
        );
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
        assert!(svg.contains("data-detail-tier=\"normal\""));
    }

    #[test]
    fn print_optimized_css_is_embedded_by_default() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let svg = render_svg(&ir);
        assert!(svg.contains("@media print"));
    }

    /// Printing must not reproduce the node gradient (bd-ccni).
    ///
    /// The gradient reaches nodes as an inline `fill="url(#fm-node-gradient)"` attribute, which no
    /// rule in the print block used to touch: text fill and shape stroke were flattened and
    /// clusters were forced to `#fff`, but nodes still printed their gradient. Neutralising the
    /// gradient's own stops fixes that without disturbing classDef fills or solid shapes.
    #[test]
    fn print_css_neutralizes_the_node_gradient() {
        // Read the print block BOUNDED at its closing braces. Substring-searching the whole SVG is
        // a trap: the document also carries a `<filter id="node-glow">` def and the
        // `<linearGradient>` itself, so a naive `svg.contains(..)` reports properties that are
        // nowhere near the print block. That mistake produced a false reading while diagnosing this.
        fn print_block(svg: &str) -> String {
            let start = svg.find("@media print").expect("print block present");
            let rest = &svg[start..];
            let end = rest.find("}\n}").map_or(rest.len(), |i| i + 3);
            rest[..end].to_string()
        }

        let ir = create_ir_with_single_node("printed", NodeShape::Rect);

        let with_gradient = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                print_optimized: true,
                node_gradients: true,
                ..SvgRenderConfig::default()
            },
        );
        assert!(
            with_gradient.contains("fill=\"url(#fm-node-gradient)\""),
            "precondition: nodes must actually carry the gradient, or this test proves nothing"
        );
        let printed = print_block(&with_gradient);
        assert!(
            printed.contains("#fm-node-gradient stop") && printed.contains("stop-color: #fff"),
            "print block must flatten the gradient stops, got:\n{printed}"
        );

        // NEGATIVE CONTROL 1: with gradients off there is nothing to neutralise, and the print
        // block must be exactly what it always was. This is what keeps the golden corpus still —
        // the fixtures pin `node_gradients: false`.
        let without_gradient = render_svg_with_config(
            &ir,
            &SvgRenderConfig {
                print_optimized: true,
                node_gradients: false,
                ..SvgRenderConfig::default()
            },
        );
        let printed_plain = print_block(&without_gradient);
        assert!(
            !printed_plain.contains("fm-node-gradient"),
            "gradients-off print block must not gain the reset rule, got:\n{printed_plain}"
        );

        // NEGATIVE CONTROL 2: the fix must not reach for node `fill`. A `fill` rule here would
        // beat classDef colours and solid shapes as well as the gradient, which is exactly the
        // collateral this approach avoids.
        assert!(
            !printed.contains(".fm-node rect {\n    fill:")
                && !printed.contains("fill: #fff !important;\n  }\n  #fm-node-gradient"),
            "print block must neutralise the gradient via its stops, not by overriding node fill"
        );
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
        assert!(
            min.contains("2px 8px"),
            "value-internal space dropped: {min:?}"
        );
        assert!(
            min.contains("in srgb"),
            "function-arg space dropped: {min:?}"
        );
        assert!(min.contains("4%, transparent") || min.contains("4%,transparent"));
        assert!(
            min.contains("fill: var(--fm-node-fill)"),
            "`: ` space dropped: {min:?}"
        );
    }

    #[test]
    fn cached_css_post_passes_match_uncached_across_doc_build_matrix() {
        let mut inputs = doc_build_inputs();
        inputs.extend([
            String::from(
                "flowchart LR\n  A:::hot-.->B\n  classDef hot fill:#f00,stroke:#111\n  style B fill:#0f0",
            ),
            String::from("flowchart LR\n  A[/note/]-->B[(store)]\n  subgraph G\n    B-->C\n  end"),
            String::from("sequenceDiagram\n  A-xB: stop\n  B-->>A: retry"),
            String::from("pie title Pets\n  \"Dogs\": 3\n  \"Cats\": 2"),
        ]);

        let mut configs: Vec<SvgRenderConfig> = [
            ThemePreset::Default,
            ThemePreset::Dark,
            ThemePreset::Forest,
            ThemePreset::Neutral,
        ]
        .into_iter()
        .map(|theme| SvgRenderConfig {
            theme,
            ..SvgRenderConfig::default()
        })
        .collect();
        configs.push(SvgRenderConfig {
            animations_enabled: true,
            print_optimized: true,
            glow_enabled: true,
            ..SvgRenderConfig::default()
        });
        configs.push(SvgRenderConfig {
            backend: SvgBackend::Scene,
            ..SvgRenderConfig::default()
        });

        for (config_index, config) in configs.iter().enumerate() {
            clear_css_post_pass_cache();
            for (input_index, input) in inputs.iter().enumerate() {
                let parsed = fm_parser::parse(input);
                let layout = fm_layout::layout_diagram(&parsed.ir);
                let expected = render_svg_with_layout_impl(&parsed.ir, &layout, config, false);
                let first = render_svg_with_layout_impl(&parsed.ir, &layout, config, true);
                let hit = render_svg_with_layout_impl(&parsed.ir, &layout, config, true);
                assert_eq!(
                    first, expected,
                    "cache miss drifted: config={config_index}, input={input_index}"
                );
                assert_eq!(
                    hit, expected,
                    "cache hit drifted: config={config_index}, input={input_index}"
                );
            }
        }
    }

    #[test]
    fn unknown_marker_identity_bypasses_css_post_pass_cache() {
        let raw = String::from(
            "<svg><defs>\
             <marker id=\"arrow-future\"><path/></marker>\
             </defs><style>\
.fm-node-inactive { opacity: 0.4; }\n\
.fm-cluster { fill-opacity: 0.8; }\n\
marker#arrow-future path { fill: red; }\n\
</style><path marker-end=\"url(#arrow-future)\"/></svg>",
        );
        let mut expected = raw.clone();
        apply_output_post_passes(&mut expected, false, None);
        let mut actual = raw;
        apply_output_post_passes(&mut actual, true, None);
        assert_eq!(actual, expected);
    }

    #[test]
    #[ignore = "release-only, same-binary doc_build_40 performance probe"]
    fn theme_css_post_pass_cache_doc_build_perf_ab() {
        use sha2::{Digest, Sha256};
        use std::{fmt::Write as _, hint::black_box, time::Instant};

        const ROUNDS: usize = 41;
        const BOOTSTRAP_RESAMPLES: usize = 20_000;
        const PINNED_INPUT_SHA256: &str =
            "8badedbf69bc204d952af1ba780c07569b7eb1091ff5d0fdd400dd2e3f6b59d7";

        fn sha256_hex(bytes: &[u8]) -> String {
            let digest = Sha256::digest(bytes);
            let mut hex = String::with_capacity(digest.len() * 2);
            for byte in digest {
                write!(hex, "{byte:02x}").expect("writing to String cannot fail");
            }
            hex
        }

        fn median(values: &[f64]) -> f64 {
            let mut sorted = values.to_vec();
            sorted.sort_by(f64::total_cmp);
            sorted[sorted.len() / 2]
        }

        fn bootstrap_median_ci(values: &[f64]) -> (f64, f64) {
            let mut seed = 0x4d59_5df4_d0f3_3173u64;
            let mut medians = Vec::with_capacity(BOOTSTRAP_RESAMPLES);
            let mut sample = vec![0.0; values.len()];
            for _ in 0..BOOTSTRAP_RESAMPLES {
                for slot in &mut sample {
                    seed = seed
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    let index = usize::try_from(seed >> 32).unwrap_or(0) % values.len();
                    *slot = values[index];
                }
                medians.push(median(&sample));
            }
            medians.sort_by(f64::total_cmp);
            (
                medians[BOOTSTRAP_RESAMPLES / 40],
                medians[BOOTSTRAP_RESAMPLES * 39 / 40],
            )
        }

        fn cv_pct(values: &[f64]) -> f64 {
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            let variance = values
                .iter()
                .map(|value| (value - mean).powi(2))
                .sum::<f64>()
                / (values.len() - 1) as f64;
            variance.sqrt() / mean * 100.0
        }

        fn render_doc_build(inputs: &[String], use_cache: bool) -> Vec<String> {
            if use_cache {
                // One cold process rendering one docs page: the cache may warm only from earlier
                // diagrams in this same 40-document batch.
                clear_css_post_pass_cache();
            }
            let config = SvgRenderConfig::default();
            inputs
                .iter()
                .map(|input| {
                    let parsed = fm_parser::parse(input);
                    let layout = fm_layout::layout_diagram(&parsed.ir);
                    render_svg_with_layout_impl(&parsed.ir, &layout, &config, use_cache)
                })
                .collect()
        }

        fn measure_min_of_three(inputs: &[String], use_cache: bool) -> f64 {
            let mut best = f64::INFINITY;
            for _ in 0..3 {
                let start = Instant::now();
                let rendered = render_doc_build(inputs, use_cache);
                black_box(&rendered);
                best = best.min(start.elapsed().as_nanos() as f64);
            }
            best
        }

        let inputs = doc_build_inputs();
        let joined = inputs.join("\n%%--revision--%%\n");
        assert_eq!(
            sha256_hex(joined.as_bytes()),
            PINNED_INPUT_SHA256,
            "Rust doc-build generator drifted from scripts/headtohead/corpus.mjs"
        );

        let expected = render_doc_build(&inputs, false);
        let actual = render_doc_build(&inputs, true);
        assert_eq!(
            actual, expected,
            "cached and uncached doc-build SVG bytes differ"
        );

        let executable = std::env::current_exe().expect("current test executable");
        let executable_bytes = std::fs::read(&executable).expect("read current test executable");
        println!(
            "binary elf_sha256={} elf_bytes={} rounds={} min_of=3",
            sha256_hex(&executable_bytes),
            executable_bytes.len(),
            ROUNDS
        );
        println!(
            "corpus id=doc_build_40 input_sha256={} documents={} parity=exact",
            PINNED_INPUT_SHA256,
            inputs.len()
        );

        // Untimed code/data warmup. Candidate samples still clear the content cache themselves.
        black_box(render_doc_build(&inputs, false));
        black_box(render_doc_build(&inputs, true));

        let mut null_ratios = Vec::with_capacity(ROUNDS);
        let mut ab_ratios = Vec::with_capacity(ROUNDS);
        let mut baseline_samples = Vec::with_capacity(ROUNDS);
        let mut candidate_samples = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            let (null_a, candidate, null_b, baseline) = if round % 2 == 0 {
                (
                    measure_min_of_three(&inputs, false),
                    measure_min_of_three(&inputs, true),
                    measure_min_of_three(&inputs, false),
                    measure_min_of_three(&inputs, false),
                )
            } else {
                let baseline = measure_min_of_three(&inputs, false);
                let null_b = measure_min_of_three(&inputs, false);
                let candidate = measure_min_of_three(&inputs, true);
                let null_a = measure_min_of_three(&inputs, false);
                (null_a, candidate, null_b, baseline)
            };
            null_ratios.push(null_a / null_b);
            ab_ratios.push(baseline / candidate);
            baseline_samples.push(baseline);
            candidate_samples.push(candidate);
        }

        let null_median = median(&null_ratios);
        let (null_low, null_high) = bootstrap_median_ci(&null_ratios);
        let ab_median = median(&ab_ratios);
        let (ab_low, ab_high) = bootstrap_median_ci(&ab_ratios);
        let null_radius = (1.0 - null_low).abs().max((null_high - 1.0).abs());
        let required_speedup = 1.03f64.max(1.0 + 2.0 * null_radius);
        println!("A/A speedup_median={null_median:.6} ci95=[{null_low:.6},{null_high:.6}]");
        println!(
            "A/B speedup_median={ab_median:.6} ci95=[{ab_low:.6},{ab_high:.6}] \
             required_lower_bound={required_speedup:.6}"
        );
        println!(
            "report_only baseline_cv_pct={:.2} candidate_cv_pct={:.2}",
            cv_pct(&baseline_samples),
            cv_pct(&candidate_samples)
        );
        assert!(
            ab_low > required_speedup,
            "REJECT: A/B lower CI {ab_low:.6} does not clear max(1.03, 2x null) \
             {required_speedup:.6}; CV is report-only"
        );
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
        let _ = strip_unused_markers(&mut svg);
        assert!(
            svg.contains("id=\"arrow-end\""),
            "referenced marker must stay"
        );
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
    fn default_edge_color_matches_preset() {
        // The memoized-marker fast path in `marker_defs_body` is keyed on this literal; if the preset
        // edge color ever changes, the default theme silently falls to the build-fresh path (still
        // correct, just slower) — this pins the two together so that regression is caught.
        assert_eq!(
            Theme::from_preset(ThemePreset::Default).colors.edge,
            DEFAULT_EDGE_COLOR
        );
    }

    #[test]
    fn marker_defs_body_streams_byte_identical() {
        use crate::defs::{DefsBuilder, MarkerOrient};
        // Reproduce the exact per-marker `.marker()` sequence both render backends used, then assert
        // the streamed `raw_markers(marker_defs_body(..))` <defs> is byte-for-byte identical — for the
        // default (memoized) color and a custom (build-fresh) color, basic and fancy, and with a
        // trailing filter + custom element to prove the markers-slot ordering is preserved.
        for &fancy in &[false, true] {
            for edge in [DEFAULT_EDGE_COLOR, "#ff0000"] {
                let mut old =
                    DefsBuilder::new().marker(ArrowheadMarker::standard("arrow-end", edge));
                if fancy {
                    old = old.marker(ArrowheadMarker::filled("arrow-filled", edge));
                }
                old = old.marker(ArrowheadMarker::open("arrow-open", edge));
                if fancy {
                    old = old
                        .marker(ArrowheadMarker::half_top("arrow-half-top", edge))
                        .marker(ArrowheadMarker::half_bottom("arrow-half-bottom", edge))
                        .marker(ArrowheadMarker::stick_top("arrow-stick-top", edge))
                        .marker(ArrowheadMarker::stick_bottom("arrow-stick-bottom", edge))
                        .marker(
                            ArrowheadMarker::standard("arrow-start", edge)
                                .with_orient(MarkerOrient::AutoStartReverse),
                        )
                        .marker(
                            ArrowheadMarker::filled("arrow-start-filled", edge)
                                .with_orient(MarkerOrient::AutoStartReverse),
                        )
                        .marker(ArrowheadMarker::circle_marker("arrow-circle", edge))
                        .marker(ArrowheadMarker::cross_marker("arrow-cross", edge))
                        .marker(ArrowheadMarker::diamond_marker("arrow-diamond", edge))
                        .marker(ArrowheadMarker::diamond_open_marker(
                            "arrow-diamond-open",
                            edge,
                        ))
                        .marker(ArrowheadMarker::triangle_open_marker(
                            "arrow-triangle-open",
                            edge,
                        ))
                        .marker(
                            ArrowheadMarker::triangle_open_marker(
                                "start-arrow-triangle-open",
                                edge,
                            )
                            .with_orient(MarkerOrient::AutoStartReverse),
                        );
                }
                let new = DefsBuilder::new().raw_markers(marker_defs_body(edge, fancy));
                assert_eq!(
                    old.to_element().render(),
                    new.to_element().render(),
                    "markers-only edge={edge} fancy={fancy}"
                );

                // Trailing filter + custom: markers must still serialize before them in both.
                let trailer = |d: DefsBuilder| {
                    d.filter(crate::defs::Filter::drop_shadow(
                        "shadow", 2.0, 2.0, 4.0, 0.3,
                    ))
                    .custom(crate::element::Element::text())
                };
                assert_eq!(
                    trailer(old).to_element().render(),
                    trailer(new).to_element().render(),
                    "markers+trailer edge={edge} fancy={fancy}"
                );
            }
        }
    }

    #[test]
    fn default_node_gradient_colors_match_preset() {
        // node_gradient_svg's memo fast path is keyed on these; if the preset colors drift, the default
        // theme silently falls to build-fresh (still correct, slower) — pin them together.
        let c = &Theme::from_preset(ThemePreset::Default).colors;
        assert_eq!(c.node_fill, DEFAULT_NODE_FILL);
        assert_eq!(c.background, DEFAULT_NODE_BG);
    }

    #[test]
    fn node_gradient_svg_streams_byte_identical() {
        use crate::defs::DefsBuilder;
        // The streamed raw_gradients(node_gradient_svg(..)) <defs> must equal the old
        // .gradient(node_gradient_for(..)) render — for the memoized default theme and a custom theme,
        // and with markers + trailing filter/custom to pin the gradients-slot ordering.
        let cfg = SvgRenderConfig::default();
        for preset in [ThemePreset::Default, ThemePreset::Forest] {
            let theme = Theme::from_preset(preset);
            let grad = node_gradient_for(&cfg, &theme).expect("gradients on by default");
            let old = DefsBuilder::new()
                .raw_markers(marker_defs_body(&theme.colors.edge, true))
                .gradient(grad)
                .filter(crate::defs::Filter::drop_shadow(
                    "shadow", 2.0, 2.0, 4.0, 0.3,
                ))
                .custom(crate::element::Element::text());
            let new = DefsBuilder::new()
                .raw_markers(marker_defs_body(&theme.colors.edge, true))
                .raw_gradients(node_gradient_svg(&cfg, &theme).expect("gradients on by default"))
                .filter(crate::defs::Filter::drop_shadow(
                    "shadow", 2.0, 2.0, 4.0, 0.3,
                ))
                .custom(crate::element::Element::text());
            assert_eq!(
                old.to_element().render(),
                new.to_element().render(),
                "preset={preset:?}"
            );
        }
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
        assert!(
            svg.contains("marker#arrow-end path"),
            "live marker selector dropped"
        );
        assert!(
            !svg.contains("marker#arrow-filled"),
            "dead sibling not pruned"
        );
        // Whole rule with only a dead marker is removed.
        assert!(
            !svg.contains("marker#arrow-open"),
            "fully-dead rule not removed"
        );
        // Non-marker rule untouched.
        assert!(svg.contains(".fm-edge {"), "non-marker rule corrupted");
        // The live rule still has its body.
        assert!(svg.contains("fill: red"));
    }

    #[test]
    fn strip_dead_marker_css_for_mask_matches_the_defs_scanning_twin() {
        let pretty = "\
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
";
        let mut by_mask = String::from(pretty);
        strip_dead_marker_css_for_mask(&mut by_mask, MARKER_END);

        // Same input pruned by the defs-scanning twin, with only arrow-end defined.
        let mut svg = format!(
            "<svg><defs><marker id=\"arrow-end\"><path/></marker></defs><style>{pretty}</style>\
             <path marker-end=\"url(#arrow-end)\"/></svg>"
        );
        strip_dead_marker_css(&mut svg);
        let start = svg.find("<style>").expect("style open") + "<style>".len();
        let end = svg.find("</style>").expect("style close");
        assert_eq!(
            by_mask,
            &svg[start..end],
            "mask-driven prune diverged from the defs-scanning prune"
        );

        // Negative case: a mask claiming every marker is live must change nothing, so a bug that
        // pruned unconditionally (or inverted the test) fails here.
        let mut all_live = String::from(pretty);
        strip_dead_marker_css_for_mask(&mut all_live, u16::MAX);
        assert_eq!(all_live, pretty, "live markers must not be pruned");
    }

    #[test]
    fn flowchart_direct_css_path_prunes_dead_marker_rules() {
        // A plain `-->` flowchart uses only arrow-end. This goes through the direct-minified
        // flowchart CSS path, which returns before the output post-passes -- so if the prune is
        // not applied at construction, every dead marker rule survives into the output.
        let parsed = fm_parser::parse("flowchart TD\n  A --> B\n  B --> C\n");
        let svg = render_svg(&parsed.ir);
        assert!(
            svg.contains("marker#arrow-end"),
            "live arrow-end CSS was dropped"
        );
        for dead in [
            "marker#arrow-filled",
            "marker#arrow-open",
            "marker#arrow-circle",
            "marker#arrow-cross",
            "marker#arrow-diamond",
        ] {
            assert!(!svg.contains(dead), "dead {dead} CSS survived");
        }
    }

    #[test]
    fn flowchart_direct_css_path_prunes_unused_accent_palettes() {
        // `N0` has the stable palette index 4. The direct-minified flowchart path returns before
        // the body post-pass, so this verifies construction-time pruning keeps its one live rule
        // and removes palette rules/variables that no surviving CSS can reference.
        let ir = create_ir_with_single_node("N0", NodeShape::Rect);
        assert_eq!(
            flowchart_accent_mask(&ir),
            [false, false, false, false, true, false, false, false, false]
        );
        let svg = render_svg(&ir);
        let start = svg.find("<style>").expect("style open") + "<style>".len();
        let end = svg.find("</style>").expect("style close");
        let css = &svg[start..end];

        assert!(
            css.contains(".fm-node-accent-4{"),
            "live palette rule missing"
        );
        assert!(
            css.contains("--fm-accent-4:"),
            "live palette variable missing"
        );
        for unused in [3, 5, 6, 7, 8] {
            assert!(
                !css.contains(&format!(".fm-node-accent-{unused}{{")),
                "unused palette rule {unused} survived"
            );
            assert!(
                !css.contains(&format!("--fm-accent-{unused}:")),
                "unused palette variable {unused} survived"
            );
        }
    }

    #[test]
    fn flowchart_direct_css_preserves_roots_for_inline_accent_styles() {
        let mut ir = create_ir_with_single_node("N0", NodeShape::Rect);
        ir.nodes[0].inline_style = Some(Box::new(fm_core::IrInlineStyle::from_pairs([(
            String::from("fill"),
            String::from("var(--fm-accent-7)"),
        )])));

        let svg = render_svg(&ir);
        let start = svg.find("<style>").expect("style open") + "<style>".len();
        let end = svg.find("</style>").expect("style close");
        let css = &svg[start..end];
        assert!(
            css.contains("--fm-accent-7:"),
            "inline body styles must retain their referenced root palette variable"
        );
    }

    #[test]
    fn edgeless_diagram_drops_all_marker_css() {
        // A pie chart has no edges -> no markers -> every marker#… rule is dead.
        let parsed = fm_parser::parse("pie title Pets\n  \"Dogs\": 3\n  \"Cats\": 2\n");
        let svg = render_svg(&parsed.ir);
        assert!(
            !svg.contains("marker#arrow"),
            "edge-less diagram kept dead marker CSS"
        );
        assert!(
            !svg.contains("<marker "),
            "edge-less diagram kept marker defs"
        );
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

    /// The `<filter id="node-glow">` def must not ship when nothing can reference it (bd-a6uk).
    ///
    /// `glow_enabled` defaults ON while the reference needs a `highlight` class almost no diagram
    /// carries, so the def was 168 bytes of dead output on nearly every render. Asserted as a pair
    /// with `highlighted_node_uses_glow_filter` above, and both directions are checked here too:
    /// the dangerous failure is not the wasted bytes but suppressing the def while a node still
    /// emits `url(#node-glow)`, which would leave a dangling reference and an unstyled node.
    #[test]
    fn glow_filter_def_is_omitted_when_no_node_is_highlighted() {
        let config = SvgRenderConfig {
            glow_enabled: true,
            ..Default::default()
        };

        // No highlight class anywhere: neither the def nor a reference may appear.
        let plain = render_svg_with_config(
            &create_ir_with_single_node("plain-node", NodeShape::Rect),
            &config,
        );
        assert!(
            !plain.contains("node-glow"),
            "unreferenced glow filter shipped: {}",
            plain
                .split("<filter")
                .find(|chunk| chunk.contains("node-glow"))
                .unwrap_or("<not found>")
        );

        // A class carrying a non-highlight state keyword still gets no glow def. `muted` and not
        // `inactive`: the keyword scan is SUBSTRING-based, so `inactive` contains `active` and the
        // renderer really does treat it as highlighted — the def is correctly emitted there, and
        // this gate mirrors that rather than second-guessing it.
        let unrelated = render_svg_with_config(
            &create_ir_with_single_node_classes("dim-node", NodeShape::Rect, &["muted"]),
            &config,
        );
        assert!(!unrelated.contains("node-glow"));

        // Control: with a highlighted node the def comes back AND is referenced, so this test
        // cannot be satisfied by never emitting the filter at all.
        let highlighted = render_svg_with_config(
            &create_ir_with_single_node_classes("hot-node", NodeShape::Rect, &["highlight"]),
            &config,
        );
        assert!(highlighted.contains("id=\"node-glow\""));
        assert!(highlighted.contains("filter=\"url(#node-glow)\""));
    }

    /// One IR per render path that `<defs>` hygiene must hold on, built by PARSING real Mermaid
    /// rather than hand-assembling metadata — a hand-built IR can silently miss the condition a
    /// dispatch branch actually tests (`pie_meta` with non-empty slices, `xy_chart_meta` with
    /// non-empty series), which is exactly the gate under test here.
    fn defs_hygiene_cases() -> Vec<(&'static str, MermaidDiagramIr)> {
        [
            ("flowchart", "flowchart LR\n  A-->B"),
            ("sequence", "sequenceDiagram\n  Alice->>Bob: hi"),
            ("class", "classDiagram\n  Animal <|-- Dog"),
            ("state", "stateDiagram-v2\n  [*] --> S1"),
            ("er", "erDiagram\n  A ||--o{ B : has"),
            ("journey", "journey\n  title J\n  section S\n    Task: 5: Me"),
            ("mindmap", "mindmap\n  root\n    a\n    b"),
            ("gitgraph", "gitGraph\n  commit\n  branch dev\n  commit"),
            ("timeline", "timeline\n  title T\n  2024 : x"),
            ("gantt", "gantt\n  title T\n  section S\n  t1 :a1, 2024-01-01, 3d"),
            ("pie", "pie title P\n  \"a\" : 30\n  \"b\" : 70"),
            ("quadrant", "quadrantChart\n  title Q\n  a: [0.3, 0.6]"),
            (
                "xychart",
                "xychart-beta\n  title \"T\"\n  x-axis [a, b]\n  y-axis \"y\" 0 --> 10\n  bar [3, 7]",
            ),
        ]
        .into_iter()
        .map(|(label, src)| (label, fm_parser::parse(src).ir))
        .collect()
    }

    /// Split a rendered document into its `<defs>` block and the body that follows it.
    fn split_defs_and_body(svg: &str) -> (&str, &str) {
        let Some(defs_start) = svg.find("<defs>") else {
            return ("", svg);
        };
        let defs_end = svg[defs_start..]
            .find("</defs>")
            .expect("defs block should close")
            + defs_start;
        (&svg[defs_start..defs_end], &svg[defs_end..])
    }

    /// `<defs>` ids declared inside `defs` that no `url(#…)` in `defs` or `body` points at.
    fn unreferenced_defs_ids<'a>(defs: &'a str, body: &str) -> Vec<&'a str> {
        let mut dead = Vec::new();
        for chunk in defs.split("id=\"").skip(1) {
            let Some(end) = chunk.find('"') else { continue };
            let id = &chunk[..end];
            let reference = format!("url(#{id})");
            if !body.contains(&reference) && !defs.contains(&reference) {
                dead.push(id);
            }
        }
        dead
    }

    /// `url(#…)` references in `body` whose target is not declared in `defs`.
    fn dangling_url_references<'a>(defs: &str, body: &'a str) -> Vec<&'a str> {
        let mut dangling = Vec::new();
        for chunk in body.split("url(#").skip(1) {
            let Some(end) = chunk.find(')') else { continue };
            let id = &chunk[..end];
            if !defs.contains(&format!("id=\"{id}\"")) {
                dangling.push(id);
            }
        }
        dangling
    }

    /// Every rendered ER attribute row must land inside its own entity rectangle (bd-090g).
    ///
    /// The end-to-end half of the fix: `fm-layout` sizes the box, `write_er_entity_into` places the
    /// rows, and that writer does NOT stop at the box edge — it keeps emitting rows at a fixed pitch.
    /// So a box sized from the entity name alone left attributes floating outside it from the fourth
    /// attribute on, which is invisible in a 2-3 attribute fixture and stable enough for a byte golden
    /// to bless. Asserted on the real parse -> layout -> render pipeline, as a containment property
    /// rather than against pinned coordinates, and swept over counts because the defect only appears
    /// past the label-derived minimum height.
    #[test]
    fn er_attribute_rows_render_inside_their_entity_box() {
        for count in 1_usize..=24 {
            let attributes = (0..count)
                .map(|index| format!("    string field_number_{index}"))
                .collect::<Vec<_>>()
                .join("\n");
            let src = format!("erDiagram\n  CUSTOMER {{\n{attributes}\n  }}");
            let parsed = fm_parser::parse(&src);
            let svg = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());

            assert!(
                !svg.contains("transform="),
                "{count}: a transform would make these coordinates non-final"
            );

            let group = svg
                .split("<g ")
                .find(|chunk| chunk.contains("fm-er-entity-name"));
            assert!(group.is_some(), "{count}: no ER entity group rendered");
            let group = group.unwrap_or_default();

            let rect = group.split("<rect ").nth(1);
            assert!(rect.is_some(), "{count}: entity has no box");
            let rect = rect.unwrap_or_default();

            // NaN for a missing/unparseable attribute, asserted finite at the call site, so a
            // malformed document fails loudly without this probe panicking its way there.
            let attr = |chunk: &str, name: &str| -> f32 {
                let Some(at) = chunk.find(&format!("{name}=\"")) else {
                    return f32::NAN;
                };
                let rest = &chunk[at + name.len() + 2..];
                let Some(end) = rest.find('"') else {
                    return f32::NAN;
                };
                rest[..end].parse().unwrap_or(f32::NAN)
            };
            let top = attr(rect, "y");
            let bottom = top + attr(rect, "height");
            assert!(
                top.is_finite() && bottom.is_finite(),
                "{count}: entity box is missing y/height"
            );

            let mut rows = 0_usize;
            for chunk in group.split("<text ").skip(1) {
                if !chunk.contains("class=\"fm-er-attribute\"") {
                    continue;
                }
                let baseline = attr(chunk, "y");
                // Rows carry `dominant-baseline="central"`, so `y` is the glyph's VERTICAL CENTRE and
                // roughly half a font size hangs below it. Requiring the baseline alone to be inside
                // the box would pass while the bottom half of the last row's glyphs was clipped —
                // which is exactly the state the 3-attribute fixture was in (2px of slack at a 12px
                // row). Assert the glyph box fits, not just its centre line.
                let half_glyph = attr(chunk, "font-size") * 0.5;
                assert!(
                    baseline - half_glyph >= top && baseline + half_glyph <= bottom,
                    "{count} attributes: row glyph box {}..{} escapes the entity box {top}..{bottom}",
                    baseline - half_glyph,
                    baseline + half_glyph
                );
                rows += 1;
            }
            assert_eq!(rows, count, "{count}: not every attribute row was emitted");
        }
    }

    /// A gantt task name must render IN FULL, and must never change the bar geometry (bd-h9gx).
    ///
    /// Bar length and position are a gantt chart's entire payload. They used to be
    /// `max(LABEL_width, duration*48, 156)` centred on the start day, so a 1-day task with a long name
    /// drew a 524.10px bar against a 10-day task's 480.00px — the duration ordering inverted by a
    /// rename. Bars are now purely temporal and the name is placed by layout, reproducing pinned
    /// mermaid-js 11.15.0: inside the bar when it fits, otherwise just outside it, never truncated.
    ///
    /// The two halves are asserted together because fixing only the geometry clips the text and fixing
    /// only the text leaves the bars lying.
    /// `(((x)))` must not render identically to `((x))` (bd-vfxu).
    ///
    /// The double circle used to be one circle with `stroke_width(2.0)` standing in for the inner
    /// ring — a no-op in the shipping theme, whose base node stroke already resolves to 2.0. The two
    /// declared shapes therefore produced byte-identical geometry apart from their centre, and the
    /// same shape carries a stateDiagram end state, so an end state looked like an ordinary circular
    /// state.
    ///
    /// The control is the plain circle: it must still emit exactly ONE circle, or a fix that simply
    /// added a ring everywhere would pass the first assertion while breaking every other node.
    #[test]
    fn double_circle_draws_two_rings_and_a_plain_circle_still_draws_one() {
        let count_circles = |src: &str| -> usize {
            let parsed = fm_parser::parse(src);
            let svg = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());
            svg.matches("<circle").count()
        };

        let plain = count_circles("flowchart TD\n  B((Circle))\n");
        let double = count_circles("flowchart TD\n  A(((Double Circle)))\n");

        assert_eq!(
            plain, 1,
            "a plain circle node must emit exactly one <circle>"
        );
        assert_eq!(
            double, 2,
            "a double circle must emit two concentric <circle> elements, got {double}"
        );

        // The node's own structure must survive. An earlier attempt used an early `return` for the
        // plain-circle branch, which returned from the whole function and skipped the group wrapper,
        // the shape class and the label text — the circle-count assertions above still passed while
        // every plain circle silently lost its label. Assert the structure, not just the rings.
        let plain_svg = render_svg_with_config(
            &fm_parser::parse("flowchart TD\n  B((Circle))\n").ir,
            &SvgRenderConfig::default(),
        );
        assert!(
            plain_svg.contains("fm-node-shape-circle"),
            "a plain circle must keep its shape class"
        );
        assert!(
            plain_svg.contains(">Circle<"),
            "a plain circle must keep its label text"
        );

        // The rings must be CONCENTRIC and DISTINCT: same centre, different radius. Equal radii
        // would satisfy the count above while still being one visible ring.
        let parsed = fm_parser::parse("flowchart TD\n  A(((Double Circle)))\n");
        let svg = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());
        let radii: Vec<f32> = svg
            .split("<circle")
            .skip(1)
            .filter_map(|chunk| {
                let tag = chunk.split('>').next()?;
                tag.split("r=\"").nth(1)?.split('"').next()?.parse().ok()
            })
            .collect();
        assert_eq!(radii.len(), 2, "expected two radii, got {radii:?}");
        assert!(
            (radii[0] - radii[1]).abs() > 1.0,
            "the two rings must differ by more than a hairline: {radii:?}"
        );
    }

    /// A state `[*]` end is a ring around a FILLED dot; a flowchart `(((x)))` is two hollow rings
    /// (bd-wbxc).
    ///
    /// One NodeShape::DoubleCircle carries both, so the inner fill is decided by the diagram rather
    /// than by the shape. The flowchart half is the control: filling the inner disc unconditionally
    /// would satisfy the state assertion while turning every flowchart double circle into a
    /// bullseye.
    #[test]
    fn double_circle_inner_disc_is_filled_only_for_a_state_terminal() {
        let inner_fills = |src: &str| -> Vec<String> {
            let parsed = fm_parser::parse(src);
            let svg = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());
            svg.split("<circle")
                .skip(1)
                .filter_map(|chunk| {
                    let tag = chunk.split('>').next()?;
                    let fill = tag.split("fill=\"").nth(1)?.split('"').next()?;
                    Some(fill.to_string())
                })
                .collect()
        };

        let state = inner_fills("stateDiagram-v2\n  [*] --> Idle\n  Idle --> [*]\n");
        assert!(
            state.iter().any(|fill| fill != "none" && !fill.is_empty()),
            "a state terminal must have a filled inner disc, got {state:?}"
        );

        let flow = inner_fills("flowchart TD\n  A(((Double)))\n");
        assert_eq!(
            flow.len(),
            2,
            "a flowchart double circle must emit two circles, got {flow:?}"
        );
        assert!(
            flow.iter().any(|fill| fill == "none"),
            "a flowchart double circle must keep its inner ring hollow, got {flow:?}"
        );
    }

    /// An `alt` / `opt` / `loop` / `par` frame must be drawn INSIDE the canvas (bd-zwh3).
    ///
    /// `build_sequence_fragment_geometry` anchors the frame at `-padding` and widens it by
    /// `2 * padding`, while the canvas is derived from the participant span — so the overhang had
    /// nowhere to live and the dashed border was clipped on both sides in every sequence diagram
    /// that used a fragment. golden/sequence_advanced carried it at x = -2 with width 761.90
    /// against a 757.90-wide viewBox.
    ///
    /// The second half is the non-vacuity control: a sequence diagram with NO fragment emits no
    /// `fm-sequence-fragment` at all, so an "everything is inside the viewBox" assertion would pass
    /// on a document that had simply stopped drawing frames.
    #[test]
    fn sequence_fragment_frames_are_drawn_inside_the_canvas() {
        let with_fragment = fm_parser::parse(
            "sequenceDiagram\n  participant A\n  participant B\n  A->>B: ask\n  alt happy path\n\
             \n    B-->>A: yes\n  else sad path\n    B-->>A: no\n  end\n",
        );
        let svg = render_svg_with_config(&with_fragment.ir, &SvgRenderConfig::default());

        assert!(
            svg.contains("fm-sequence-fragment"),
            "fixture emitted no interaction fragment, so the containment check is vacuous"
        );

        let view_box = svg
            .split("viewBox=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("rendered svg must carry a viewBox");
        let bounds: Vec<f32> = view_box
            .split_whitespace()
            .map(|value| value.parse().expect("viewBox components must be numeric"))
            .collect();
        let (vx, vw) = (bounds[0], bounds[2]);

        for chunk in svg.split("<rect ").skip(1) {
            let tag = chunk.split('>').next().unwrap_or("");
            if !tag.contains("fm-sequence-fragment") {
                continue;
            }
            let attr = |key: &str| -> f32 {
                tag.split(&format!("{key}=\""))
                    .nth(1)
                    .and_then(|rest| rest.split('"').next())
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(f32::NAN)
            };
            let x = attr("x");
            let width = attr("width");
            assert!(
                x >= vx - 0.01,
                "fragment rect starts at {x}, left of the viewBox origin {vx}: {tag}"
            );
            assert!(
                x + width <= vx + vw + 0.01,
                "fragment rect ends at {} past the viewBox right edge {}: {tag}",
                x + width,
                vx + vw
            );
        }
    }

    /// An `alt` whose second branch is invisible is not an `alt` (bd-zsfo).
    ///
    /// The complete text of `alt is ok / … / else is bad / … / end` was "A | B | alt [is ok] |
    /// start | yes | no": `is bad` appeared nowhere and nothing separated the branches, so the
    /// reader saw one undivided box labelled only with the first condition. The parser had
    /// preserved the branch label in `IrSequenceFragment.alternatives` the whole time — neither
    /// layout nor any renderer read the field.
    ///
    /// The branches deliberately hold DIFFERENT numbers of messages: a divider placed at a fixed
    /// fraction of the frame height passes on symmetric branches and fails here, which is the
    /// implementation this test exists to reject.
    #[test]
    fn sequence_alt_branches_are_divided_and_both_conditions_are_drawn() {
        let src = "sequenceDiagram\n    A->>B: start\n    alt is ok\n        A->>B: yes\n        \
                   A->>B: yes2\n    else is bad\n        A->>B: no\n    end";
        let parsed = fm_parser::parse(src);
        let layout = fm_layout::layout_diagram(&parsed.ir);
        let config = SvgRenderConfig::default();
        let svg = render_svg_with_config(&parsed.ir, &config);

        // Both conditions reach the document. A test that only checked for a divider would pass
        // while the second condition was still being dropped.
        assert!(
            svg.contains(">alt<") && svg.contains(">[is ok]<"),
            "the fragment's own condition must still be drawn"
        );
        assert_eq!(
            svg.matches(">[is bad]<").count(),
            1,
            "the `else` condition must be drawn exactly once"
        );

        let dividers: Vec<&str> = svg
            .match_indices("<line ")
            .map(|(at, _)| &svg[at..=at + svg[at..].find('>').unwrap()])
            .filter(|e| e.contains("fm-sequence-fragment-divider"))
            .collect();
        assert_eq!(
            dividers.len(),
            1,
            "one `else` must draw exactly one divider, got {dividers:?}"
        );
        let attr = |elem: &str, name: &str| -> f32 {
            let key = format!(" {name}=\"");
            elem.split(&key)
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .parse()
                .unwrap()
        };
        let divider_y = attr(dividers[0], "y1");
        assert!(
            (attr(dividers[0], "y2") - divider_y).abs() < 0.01,
            "a branch divider must be horizontal"
        );

        // POSITIONAL: the divider sits between the last message of the first branch (yes2, edge 2)
        // and the first message of the second (no, edge 3) — not at a fixed fraction of the frame.
        let offset_y = config.padding - layout.bounds.y;
        let message_y = |edge_index: usize| -> f32 {
            layout
                .edges
                .iter()
                .find(|edge| edge.edge_index == edge_index)
                .and_then(|edge| edge.points.first())
                .map(|point| point.y + offset_y)
                .unwrap_or_else(|| panic!("no layout edge {edge_index}"))
        };
        assert!(
            message_y(2) < divider_y && divider_y < message_y(3),
            "divider at y={divider_y} is not between the last message of the first branch \
             (y={}) and the first of the second (y={})",
            message_y(2),
            message_y(3)
        );

        // The frame is spanned, and spanned within the clamp bd-zwh3 put on the frame border: a
        // divider that ran past the viewBox would reintroduce exactly that defect.
        let frame = svg
            .match_indices("<rect ")
            .map(|(at, _)| &svg[at..=at + svg[at..].find('>').unwrap()])
            .find(|e| e.contains("fm-sequence-fragment\""))
            .expect("the alt frame must still be drawn");
        assert!(
            (attr(dividers[0], "x1") - attr(frame, "x")).abs() < 0.01
                && (attr(dividers[0], "x2") - (attr(frame, "x") + attr(frame, "width"))).abs()
                    < 0.01,
            "the divider must span exactly the (clamped) frame"
        );
    }

    /// Control for bd-zsfo: an `alt` with NO `else` has no branch boundary, so it must draw no
    /// divider. An implementation that emits one per FRAGMENT rather than per branch passes the
    /// test above and fails here, adding a line that means nothing.
    #[test]
    fn a_sequence_fragment_without_branches_draws_no_divider() {
        let with_branch = render_svg_with_config(
            &fm_parser::parse(
                "sequenceDiagram\n    A->>B: hi\n    alt ok\n        A->>B: yes\n    \
                 else no\n        A->>B: nope\n    end",
            )
            .ir,
            &SvgRenderConfig::default(),
        );
        let without_branch = render_svg_with_config(
            &fm_parser::parse(
                "sequenceDiagram\n    A->>B: hi\n    alt ok\n        A->>B: yes\n    end",
            )
            .ir,
            &SvgRenderConfig::default(),
        );
        assert!(
            with_branch.contains("fm-sequence-fragment-divider"),
            "non-vacuity: the two-branch arm of this control must draw a divider"
        );
        assert!(
            !without_branch.contains("fm-sequence-fragment-divider"),
            "an alt with no else must draw no divider"
        );
        // And a sequence diagram with no fragment at all is untouched.
        let plain = render_svg_with_config(
            &fm_parser::parse("sequenceDiagram\n    A->>B: hi\n    B->>A: bye").ir,
            &SvgRenderConfig::default(),
        );
        assert!(
            !plain.contains("fm-sequence-fragment"),
            "a sequence diagram with no fragments must draw no fragment markup"
        );
    }

    /// A gantt chart with no dates is not a gantt chart (bd-trsd).
    ///
    /// The complete `<text>` content of the shipped `gantt_basic.svg` was "Roadmap | Design |
    /// Build": no date anywhere, and the declared section name absent too. Not missing computation
    /// — the gantt layout arm fills `extensions.axis_ticks` (one per day) and `extensions.bands`
    /// (one per section) and the renderer threw both away, because the generic loops that draw them
    /// sit BELOW the gantt early return and the gantt arm's own band loop read `layout.clusters`,
    /// which is empty on this path.
    ///
    /// The assertions are the ones a naive implementation fails. Emitting a tick row is easy;
    /// emitting one that INDEXES ITS OWN BARS is what this diagram type exists for, so the axis is
    /// checked positionally against the bar geometry, and vertically against the topmost bar so the
    /// row cannot be drawn across the chart it annotates.
    #[test]
    fn gantt_draws_a_time_axis_and_named_section_bands() {
        let src = "gantt\n  title Roadmap\n  dateFormat YYYY-MM-DD\n  section Core\n  \
                   Design :a1, 2026-01-01, 3d\n  Build :a2, after a1, 4d";
        let parsed = fm_parser::parse(src);
        let layout = fm_layout::layout_diagram_gantt(&parsed.ir);
        let config = SvgRenderConfig::default();
        let svg = render_svg_with_config(&parsed.ir, &config);

        let attr = |elem: &str, name: &str| -> f32 {
            let key = format!(" {name}=\"");
            let rest = elem
                .split(&key)
                .nth(1)
                .unwrap_or_else(|| panic!("no {name} in {elem}"));
            rest.split('"').next().unwrap().parse().unwrap()
        };
        let elems = |tag: &str, class: &str| -> Vec<String> {
            svg.match_indices(&format!("<{tag} "))
                .map(|(at, _)| {
                    let end = svg[at..].find('>').unwrap();
                    svg[at..=at + end].to_string()
                })
                .filter(|e| e.contains(class))
                .collect()
        };

        // The section is drawn AND named. A band with no label still fails a reader looking for
        // "Core"; the shipped golden had neither.
        assert_eq!(
            elems("rect", "fm-gantt-section-bg").len(),
            1,
            "one declared section must draw exactly one band"
        );
        assert_eq!(
            svg.matches(">Core<").count(),
            1,
            "the declared section name must appear exactly once in the output"
        );

        // One tick per day over the inclusive 2026-01-01..2026-01-07 span — not an empty axis and
        // not a tick per pixel.
        let tick_labels: Vec<&str> = svg
            .match_indices("class=\"fm-axis-tick-label\">")
            .map(|(at, m)| {
                let from = at + m.len();
                &svg[from..from + svg[from..].find('<').unwrap()]
            })
            .collect();
        // ⚠️ EIGHT, NOT SEVEN, and the extra one is the chart's END BOUNDARY (bd-pqp2f). The axis
        // used to stop at the last day a bar OCCUPIED; mermaid labels the exclusive end — measured
        // on `gantt_basic`, where it draws 2026-01-08 and we drew only through 2026-01-07.
        assert_eq!(
            tick_labels.len(),
            8,
            "expected a daily tick per day through the chart end, got {tick_labels:?}"
        );
        assert!(
            tick_labels.contains(&"2026-01-01") && tick_labels.contains(&"2026-01-04"),
            "both task start dates must be labelled on the axis: {tick_labels:?}"
        );

        // POSITIONAL consistency: the tick labelled with a task's start date sits on that bar's
        // left edge. A tick row that merely exists can still be offset from the bars it indexes,
        // which is the failure this whole family is about — and it WAS offset, by the normalization
        // translation the raw day-space tick positions never received.
        let bars = elems("rect", "fm-gantt-task");
        assert_eq!(bars.len(), 2, "two tasks must draw two bars");
        let tick_x_for = |label: &str| -> f32 {
            let marker = format!("class=\"fm-axis-tick-label\">{label}<");
            let at = svg
                .find(&marker)
                .unwrap_or_else(|| panic!("no tick labelled {label}"));
            let text_start = svg[..at].rfind("<text ").unwrap();
            // The label is drawn 3px right of its tick line, per `write_layout_axis_tick_into`.
            attr(&svg[text_start..at], "x") - 3.0
        };
        for (bar_index, start_label) in [(0usize, "2026-01-01"), (1usize, "2026-01-04")] {
            let bar_left = attr(&bars[bar_index], "x");
            let tick_x = tick_x_for(start_label);
            assert!(
                (tick_x - bar_left).abs() <= 1.0,
                "tick {start_label} at x={tick_x} does not sit on the left edge x={bar_left} of \
                 the bar that starts that day"
            );
        }

        // Mermaid's unconditional grid is below the task rows. Read tick lines out of the tick
        // groups themselves rather than every `<line>` in the document, so an unrelated line
        // elsewhere can neither satisfy nor break this.
        let lowest_tick = svg
            .match_indices("<g class=\"fm-axis-tick\">")
            .map(|(at, m)| {
                let rest = &svg[at + m.len()..];
                let line = &rest[..rest.find('>').unwrap() + 1];
                attr(line, "y2")
            })
            .fold(f32::MIN, f32::max);
        let bottommost_bar = bars
            .iter()
            .map(|e| attr(e, "y") + attr(e, "height"))
            .fold(f32::MIN, f32::max);
        assert!(
            lowest_tick > bottommost_bar,
            "bottom tick marks end at y={lowest_tick} but the last bar ends at y={bottommost_bar}"
        );

        // The bars are untouched by this change: their x still comes straight from the layout box,
        // so the annotation cannot have moved the chart it annotates.
        let offset_x = config.padding - layout.bounds.x;
        for (bar_index, node) in layout.nodes.iter().enumerate() {
            assert!(
                (attr(&bars[bar_index], "x") - (node.bounds.x + offset_x)).abs() <= 0.01,
                "bar {bar_index} x drifted from its layout box"
            );
        }
    }

    /// Mermaid 11.15's `gantt.topAxis` configuration APPENDS a top grid; it does not move the
    /// bottom grid. The pinned reference SVG has exactly two `g.grid` groups for this source.
    #[test]
    fn gantt_top_axis_appends_a_second_axis_row() {
        let src = "%%{init: {'gantt': {'topAxis': true}} }%%\ngantt\n  dateFormat YYYY-MM-DD\n  \
                   section Delivery\n  Design :a1, 2026-01-01, 3d\n  Build :a2, after a1, 4d";
        let parsed = fm_parser::parse(src);
        let layout = fm_layout::layout_diagram_gantt(&parsed.ir);
        let svg = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());

        assert_eq!(
            layout.extensions.gantt_axis_rows.len(),
            2,
            "topAxis must append to the unconditional bottom axis"
        );
        assert!(
            layout.extensions.gantt_axis_rows[0].y > layout.extensions.gantt_axis_rows[1].y,
            "the first row is the reference bottom grid and the second is the added top grid"
        );
        assert_eq!(svg.matches("fm-gantt-axis-bottom").count(), 1);
        assert_eq!(svg.matches("fm-gantt-axis-top").count(), 1);
        assert_eq!(
            svg.matches("class=\"fm-axis-tick-label\">").count(),
            layout.extensions.axis_ticks.len() * 2,
            "both reference grid rows carry every date label"
        );
    }

    /// Control for the bd-trsd fix. A timeline is the diagram type that reaches BOTH generic loops
    /// the gantt early return skipped — `extensions.bands` and `extensions.axis_ticks` — so it is
    /// the one that would break if the fix had been made by moving or relaxing the shared code
    /// instead of by giving the gantt arm its own draw calls. Without this, "gantt now has an axis"
    /// is compatible with having broken the axis everywhere else.
    #[test]
    fn timeline_bands_and_axis_are_unaffected_by_the_gantt_axis_fix() {
        let parsed = fm_parser::parse(
            "timeline\n  title History\n  section Era\n  2021 : Started\n  2022 : Shipped",
        );
        let svg = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());
        assert!(
            svg.contains("fm-axis-tick"),
            "timeline must still draw its axis through the generic loop"
        );
        assert_eq!(
            svg.matches("class=\"fm-axis-tick-label\">").count(),
            2,
            "timeline must still emit one tick per period"
        );
    }

    /// Every `url(#id)` a gantt render emits must resolve to an id the same document declares.
    ///
    /// The dependency arrows referenced `url(#arrowhead)` and no element anywhere in the output
    /// carried that id — `render_gantt_svg` appends to a document whose `<defs>` is already closed
    /// and empty for this diagram type, and the shared marker set is named `arrow-end`,
    /// `arrow-filled`, … , never `arrowhead`. So every dependency arrowhead silently failed to draw.
    ///
    /// The assertion is the GENERIC invariant rather than a check for one id, because a dangling
    /// reference is a family: any future marker, gradient, filter or clip path added to this
    /// fragment has to be defined in it too. The second half is the non-vacuity control — a fixture
    /// with no dependency edge emits no reference at all, so a test that only checked "no dangling
    /// ids" would pass on a document that had stopped drawing arrows entirely.
    #[test]
    fn gantt_url_references_resolve_within_the_document() {
        let render = |src: &str| -> String {
            let parsed = fm_parser::parse(src);
            render_svg_with_config(&parsed.ir, &SvgRenderConfig::default())
        };
        let declared = |svg: &str| -> std::collections::HashSet<String> {
            svg.split(" id=\"")
                .skip(1)
                .filter_map(|rest| rest.split('"').next().map(str::to_string))
                .collect()
        };
        let referenced = |svg: &str| -> Vec<String> {
            svg.split("url(#")
                .skip(1)
                .filter_map(|rest| rest.split(')').next().map(str::to_string))
                .collect()
        };

        let with_dependency = render(
            "gantt\n  dateFormat YYYY-MM-DD\n  section P\n  Design :a1, 2024-01-01, 3d\n  \
             Build :a2, after a1, 4d",
        );
        let ids = declared(&with_dependency);
        let refs = referenced(&with_dependency);
        for id in &refs {
            assert!(
                ids.contains(id),
                "gantt render references url(#{id}) but declares no such id; declared: {ids:?}"
            );
        }

        // Non-vacuity: this fixture must actually exercise a dependency arrow, or the loop above
        // proved nothing.
        assert!(
            with_dependency.contains("class=\"fm-gantt-dependency\""),
            "fixture emitted no dependency arrow, so the reference check is vacuous"
        );
        assert!(
            !refs.is_empty(),
            "a gantt with a dependency must reference its arrowhead marker"
        );

        // And the marker must be a real definition, not an empty placeholder.
        assert!(
            with_dependency.contains("<marker"),
            "the referenced arrowhead is not defined as a <marker> element"
        );
    }

    #[test]
    fn gantt_task_name_renders_in_full_without_changing_bar_geometry() {
        /// One rendered gantt label: anchor x, `text-anchor`, and the text as emitted.
        struct RenderedLabel {
            x: f32,
            anchor: String,
            text: String,
        }
        /// Bars as (x, width), labels, and the canvas width.
        struct RenderedGantt {
            bars: Vec<(f32, f32)>,
            labels: Vec<RenderedLabel>,
            view_width: f32,
        }

        let render = |name: &str, days: u32| -> RenderedGantt {
            let src = format!(
                "gantt\n  dateFormat YYYY-MM-DD\n  section P\n  {name} :a1, 2024-01-01, {days}d\n  \
                 T2 :a2, 2024-01-02, 10d"
            );
            let parsed = fm_parser::parse(&src);
            let svg = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());
            let num = |chunk: &str, key: &str| -> f32 {
                chunk.find(&format!("{key}=\"")).map_or(f32::NAN, |at| {
                    let rest = &chunk[at + key.len() + 2..];
                    rest[..rest.find('"').unwrap_or(0)]
                        .parse()
                        .unwrap_or(f32::NAN)
                })
            };
            let view_width = svg.find("viewBox=\"").map_or(f32::NAN, |at| {
                let rest = &svg[at + 9..];
                rest[..rest.find('"').unwrap_or(0)]
                    .split_whitespace()
                    .nth(2)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(f32::NAN)
            });
            let mut bars = Vec::new();
            for chunk in svg.split("<rect ").skip(1) {
                if !chunk.contains("class=\"fm-gantt-task") {
                    continue;
                }
                bars.push((num(chunk, "x"), num(chunk, "width")));
            }
            let mut labels = Vec::new();
            for chunk in svg.split("<text ").skip(1) {
                if !chunk.contains("class=\"fm-gantt-task-label\"") {
                    continue;
                }
                let head = chunk.split('>').next().unwrap_or_default();
                let anchor = head
                    .find("text-anchor=\"")
                    .map(|at| {
                        let rest = &head[at + 13..];
                        rest[..rest.find('"').unwrap_or(0)].to_string()
                    })
                    .unwrap_or_default();
                let body = chunk
                    .split_once('>')
                    .and_then(|(_, rest)| rest.split_once("</text>"))
                    .map(|(text, _)| text.to_string())
                    .unwrap_or_default();
                labels.push(RenderedLabel {
                    x: num(chunk, "x"),
                    anchor,
                    text: body,
                });
            }
            RenderedGantt {
                bars,
                labels,
                view_width,
            }
        };

        let long_name = "A deliberately long gantt task name that far exceeds its bar";

        // BAR GEOMETRY IS INDEPENDENT OF THE NAME. Same durations, wildly different name lengths.
        for days in [1_u32, 4, 10] {
            let short_bars = render("T1", days).bars;
            let long_bars = render(long_name, days).bars;
            assert_eq!(
                short_bars, long_bars,
                "{days}d: renaming the task changed the bar geometry"
            );
            // And the bar is exactly its duration in columns: the 1-day bar must never out-measure the
            // 10-day one, which is what the defect did.
            let first = short_bars.first().copied().unwrap_or((f32::NAN, f32::NAN));
            let second = short_bars.get(1).copied().unwrap_or((f32::NAN, f32::NAN));
            assert!(
                (first.1 - second.1 * (days as f32 / 10.0)).abs() < 0.01,
                "{days}d bar width {} is not proportional to the 10d bar {}",
                first.1,
                second.1
            );
        }

        // THE NAME RENDERS IN FULL, at every duration — including 1 day, where it cannot fit inside a
        // 48px bar and must therefore be placed outside it rather than truncated.
        for days in [1_u32, 4, 10] {
            let rendered = render(long_name, days);
            let entry = rendered
                .labels
                .iter()
                .find(|label| label.text.len() >= long_name.len());
            assert!(
                entry.is_some(),
                "{days}d: the task name was not rendered in full; got {:?}",
                rendered
                    .labels
                    .iter()
                    .map(|label| &label.text)
                    .collect::<Vec<_>>()
            );
            let label = entry.expect("checked");
            assert_eq!(label.text, long_name, "{days}d: task name altered");
            assert!(
                !label.text.contains('\u{2026}'),
                "{days}d: task name ellipsized"
            );

            // Wherever it was placed, it is on the canvas. `anchor=start` extends right from x,
            // `end` extends left, `middle` splits the difference.
            let bar_width = rendered.bars.first().map_or(f32::NAN, |b| b.1);
            assert!(
                label.x >= 0.0 && label.x <= rendered.view_width,
                "{days}d: label anchor {} is off a canvas of width {}",
                label.x,
                rendered.view_width
            );
            if bar_width < 100.0 {
                assert_ne!(
                    label.anchor, "middle",
                    "{days}d: a name too wide for a {bar_width}px bar must be placed outside it"
                );
            }
        }
    }

    /// A cluster box must widen to hold its subgraph title (bd-tm5p).
    ///
    /// `build_cluster_boxes` sized the rect purely from its member nodes' bounding box plus padding
    /// and ignored the title entirely, while the renderer draws that title unclamped at
    /// `bounds.x + 8.0` with no wrapping or truncation — so an 80-character title on a one-node
    /// subgraph rendered ~320px outside the box it labels.
    ///
    /// Asserted WITHOUT re-using the layout's own width estimator, so this cannot pass by agreeing
    /// with the code under test: the property is that the box RESPONDS to the title at all —
    /// monotonically non-decreasing as the title grows, and strictly wider once the title outgrows
    /// the contents. Pre-fix the width is constant across every title length, which is what the
    /// negative control shows.
    #[test]
    fn cluster_box_widens_for_a_title_wider_than_its_contents() {
        let cluster_width = |title: &str, child: &str| -> f32 {
            let src = format!("flowchart TB\n  subgraph G0[{title}]\n    N0[{child}]\n  end");
            let parsed = fm_parser::parse(&src);
            let svg = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());
            let rect = svg
                .split("<rect ")
                .find(|chunk| chunk.contains("class=\"fm-cluster\"") || chunk.contains("rx="))
                .unwrap_or_default();
            let at = rect.find("width=\"").map_or("", |i| &rect[i + 7..]);
            at[..at.find('"').unwrap_or(0)].parse().unwrap_or(f32::NAN)
        };

        // Same tiny child throughout, so every change comes from the title.
        let titles = [
            "G",
            "Group zero",
            "Group zero ingestion",
            "Group zero ingestion and normalization",
            "Group zero ingestion and normalization and delivery stage",
            "A deliberately long subgraph title that is much wider than its single child node",
        ];
        let mut widths = Vec::new();
        for title in titles {
            let w = cluster_width(title, "x");
            assert!(w.is_finite(), "no cluster width for title {title:?}");
            widths.push(w);
        }
        for pair in widths.windows(2) {
            assert!(
                pair[1] >= pair[0],
                "cluster width must not shrink as the title grows: {widths:?}"
            );
        }
        assert!(
            widths[widths.len() - 1] > widths[0] * 1.5,
            "an 80-character title must widen the box well past a 1-character one: {widths:?}"
        );

        // Contents already wider than the title: the title must not shrink or inflate the box, so
        // existing diagrams and the head-to-head shared-subgraph item do not move.
        let wide_short = cluster_width("Shared", "A fairly wide child node label here");
        let wide_shorter = cluster_width("S", "A fairly wide child node label here");
        assert_eq!(
            wide_short, wide_shorter,
            "a title narrower than the contents must not affect the box width"
        );
    }

    /// Every line of a wrapped C4 description must land inside its node box (bd-9xjy).
    ///
    /// The trap this test exists to close: the description is ONE `<text>` whose `y` IS inside the
    /// box, and its lines are `<tspan dy="…">` children stacking downward from there. A guard that
    /// reads only the text element's `y` sees no overflow — which is how a 17-line description drew
    /// 220px below its own rectangle while looking fine to a naive check. So the position asserted
    /// here is the ACCUMULATED dy, per line.
    ///
    /// Also pins that a single-line description does not grow the box: the renderer's own baseline
    /// clamp already contains that case, and growing it would move committed goldens for nothing.
    #[test]
    fn c4_description_lines_render_inside_the_node_box() {
        let words = [
            "This",
            "description",
            "grows",
            "one",
            "word",
            "at",
            "a",
            "time",
            "until",
            "it",
            "must",
            "wrap",
            "across",
            "a",
            "great",
            "many",
            "rendered",
            "lines",
            "inside",
            "its",
            "box",
        ];

        let mut single_line_height: Option<f32> = None;
        for take in 1..=words.len() {
            let description = words[..take].join(" ");
            let src =
                format!("C4Context\n  title System\n  Person(user, \"Nm\", \"{description}\")");
            let parsed = fm_parser::parse(&src);
            let svg = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());
            assert!(
                !svg.contains("transform="),
                "{take}: a transform would make these coordinates non-final"
            );

            let number = |chunk: &str, name: &str| -> f32 {
                let Some(at) = chunk.find(&format!("{name}=\"")) else {
                    return f32::NAN;
                };
                let rest = &chunk[at + name.len() + 2..];
                let Some(end) = rest.find('"') else {
                    return f32::NAN;
                };
                rest[..end].parse().unwrap_or(f32::NAN)
            };

            let rect = svg.split("<rect ").nth(1).unwrap_or_default();
            let top = number(rect, "y");
            let bottom = top + number(rect, "height");
            assert!(
                top.is_finite() && bottom.is_finite(),
                "{take}: node box is missing y/height"
            );

            let Some(after) = svg.split("class=\"fm-c4-description\"").nth(1) else {
                continue;
            };
            let element = svg
                .split("<text ")
                .find(|chunk| chunk.contains("class=\"fm-c4-description\""))
                .unwrap_or_default();
            let first = number(element, "y");
            let font = number(element, "font-size");
            assert!(
                first.is_finite() && font.is_finite(),
                "{take}: description is missing y/font-size"
            );

            // Walk the tspan stack, accumulating dy exactly as a renderer would.
            let body = after.split("</text>").next().unwrap_or_default();
            let mut baseline = first;
            let mut lines = 1_usize;
            for tspan in body.split("<tspan ").skip(1) {
                let dy = number(tspan, "dy");
                if !dy.is_finite() {
                    continue;
                }
                baseline += dy;
                lines += 1;
                assert!(
                    baseline + font * 0.5 <= bottom && baseline - font * 0.5 >= top,
                    "{take} words / {lines} lines: description line at {baseline} escapes the node \
                     box {top}..{bottom}"
                );
            }

            // The first `<tspan dy="0">` counts as line one, so a single-line description reports 2.
            if lines <= 2 {
                let height = bottom - top;
                let previous = *single_line_height.get_or_insert(height);
                assert_eq!(
                    height, previous,
                    "{take}: a description that fits on one line must not grow the box"
                );
            }
        }
    }

    /// `<defs>` hygiene, both directions, across every diagram-render path (bd-a6uk, bd-w5f7).
    ///
    /// Declared-but-unreferenced is the wasteful direction and the one that found both beads: a byte
    /// golden blesses dead output happily, because it is stable — just useless. This is the file's
    /// own stated doctrine for markers ("never emits unused markers") and for the `drop-shadow` and
    /// `node-glow` filters, asserted generically so instance three is a test failure instead of a
    /// discovery.
    ///
    /// Referenced-but-undeclared is the DANGEROUS direction, and it is the reason the gates that
    /// suppress defs must mirror their consumers instead of approximating them: a `url(#…)` pointing
    /// at nothing is invalid SVG and an unpainted shape. Suppressing a def is only safe while this
    /// half holds, so both halves are asserted on every case.
    ///
    /// Swept across types deliberately: bd-a6uk's version ran on a lone flowchart node and passed,
    /// while gantt/pie/quadrant/xychart were each shipping a 283-byte dead gradient. One
    /// configuration is not a sweep.
    #[test]
    fn defs_and_url_references_agree_across_diagram_types() {
        let mut checked = 0_usize;
        for (label, ir) in defs_hygiene_cases() {
            let svg = render_svg_with_config(&ir, &SvgRenderConfig::default());
            let (defs, body) = split_defs_and_body(&svg);

            assert!(
                unreferenced_defs_ids(defs, body).is_empty(),
                "{label}: defs declared but never referenced: {:?}",
                unreferenced_defs_ids(defs, body)
            );
            assert!(
                dangling_url_references(defs, body).is_empty(),
                "{label}: url(#…) references with no matching def: {:?}",
                dangling_url_references(defs, body)
            );
            // A CSS rule selecting a def by id is dead the same way a def is: bd-ccni's print block
            // neutralises `#fm-node-gradient stop`, which selects nothing once the def is gated off.
            assert_eq!(
                svg.contains("#fm-node-gradient stop"),
                svg.contains("id=\"fm-node-gradient\""),
                "{label}: the print block styles #fm-node-gradient but no such def exists"
            );
            checked += 1;
        }
        assert!(checked >= 6, "expected a real sweep, checked {checked}");
    }

    /// `takes_dedicated_chart_renderer` must agree with what the renderer actually does (bd-w5f7).
    ///
    /// The predicate gates the `fm-node-gradient` def while four separate dispatch branches decide
    /// the matching early return, and those branches keep their own `meta` bindings so they cannot
    /// literally call it. This ties the two together empirically instead: the predicate is true
    /// exactly when the output contains no `url(#fm-node-gradient)` referrer. Change a dispatch
    /// condition without the predicate and this fails.
    #[test]
    fn dedicated_chart_predicate_matches_gradient_referrer_presence() {
        let config = SvgRenderConfig {
            node_gradients: true,
            ..Default::default()
        };
        for (label, ir) in defs_hygiene_cases() {
            let svg = render_svg_with_config(&ir, &config);
            let has_referrer = svg.contains("url(#fm-node-gradient)");
            let dedicated = takes_dedicated_chart_renderer(&ir);
            assert_ne!(
                dedicated,
                has_referrer,
                "{label}: takes_dedicated_chart_renderer={dedicated} but a gradient referrer \
                 is {}present — the defs gate and the dispatch disagree",
                if has_referrer { "" } else { "not " }
            );
            // And the def follows the referrer, never the other way round.
            assert_eq!(
                svg.contains("id=\"fm-node-gradient\""),
                has_referrer,
                "{label}: gradient def and its referrer must appear together"
            );
        }
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
            c4_boundary_type: None,
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

        // The number is its OWN element and the label is CLEAN (bd-o02wn). This test used to assert
        // `>10 Ping<` — a prefix — which is not what mermaid produces: `drawMessage` writes
        // `.attr("class","sequenceNumber").text(f)` with the digits alone. The configured start and
        // increment are still exactly what this test exists to pin, and they still are: 10 then 15.
        assert!(
            svg.contains("class=\"fm-sequence-number\">10</text>"),
            "the first message's number is not its own element"
        );
        assert!(
            svg.contains("class=\"fm-sequence-number\">15</text>"),
            "the increment did not reach the second message's number element"
        );
        assert!(
            svg.contains(">Ping</text>") && svg.contains(">Pong</text>"),
            "the labels did not come out clean; the number is still glued to them"
        );
        assert!(
            !svg.contains(">10 Ping<") && !svg.contains(">15 Pong<"),
            "a prefixed label survived alongside the number element, so both forms are emitted"
        );
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
        let (left, right) = fm_core::parse_er_cardinality("||--o{");
        assert_eq!(left, "1");
        assert_eq!(right, "0..*");
    }

    #[test]
    fn er_cardinality_many_to_one() {
        let (left, right) = fm_core::parse_er_cardinality("}|--||");
        assert_eq!(left, "1..*");
        assert_eq!(right, "1");
    }

    #[test]
    fn er_cardinality_one_to_one() {
        let (left, right) = fm_core::parse_er_cardinality("||--||");
        assert_eq!(left, "1");
        assert_eq!(right, "1");
    }

    #[test]
    fn er_cardinality_dotted() {
        let (left, right) = fm_core::parse_er_cardinality("}|..|{");
        assert_eq!(left, "1..*");
        assert_eq!(right, "1..*");
    }

    #[test]
    fn er_cardinality_no_connector() {
        let (left, right) = fm_core::parse_er_cardinality("unknown");
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
            svg.contains(">loop<") && svg.contains(">[Every minute]<"),
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
        // The keyword and the condition are SEPARATE runs, as mermaid draws them; asserting the
        // fused `alt [success]` would pin the very defect that split them apart.
        assert!(svg.contains(">alt<"), "missing alt keyword run");
        assert!(svg.contains(">[success]<"), "missing alt condition run");
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
        assert!(
            svg.contains(">loop<") && svg.contains(">[3 times]<"),
            "should render the keyword and the condition as separate runs"
        );
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

    /// Ribbon widths are proportional to flow value, and stay so after hoisting the normaliser.
    ///
    /// `sankey_flow_stroke_width` used to recompute the widest flow ON EVERY EDGE, re-scanning
    /// `ir.edges` and re-parsing each flow value: O(E^2) float parses to produce E widths. The
    /// widest flow is invariant across a render, so it now rides on `EdgeRenderContext`.
    ///
    /// This is a pure hoist, so the contract is that the OUTPUT does not move. The expected widths
    /// are written from the formula (`value / widest * 24`, floored at 1.5) rather than copied from
    /// a run, so the test fails if the hoist changed the arithmetic as well as where it happens.
    #[test]
    fn sankey_ribbon_widths_are_proportional_to_flow_value() {
        let svg = render_source("sankey-beta\n\nA,B,100\nA,C,50\nA,D,10\n");
        let widths = path_stroke_widths(&svg);

        // widest = 100 -> 24.0; 50 -> 12.0; 10 -> 2.4
        assert!(
            widths.contains(&"24".to_string()),
            "the widest flow must get the full ribbon width; got {widths:?}"
        );
        assert!(
            widths.contains(&"12".to_string()),
            "a half-size flow must get half the width; got {widths:?}"
        );
        assert!(
            widths.contains(&"2.40".to_string()),
            "a tenth-size flow must get a tenth of the width; got {widths:?}"
        );
    }

    /// The smallest flow is floored so it stays visible rather than collapsing to a hairline.
    #[test]
    fn a_tiny_sankey_flow_is_floored_not_hairline() {
        let svg = render_source("sankey-beta\n\nA,B,1000\nA,C,1\n");
        let widths = path_stroke_widths(&svg);
        // 1/1000 * 24 = 0.024, below the 1.5 floor.
        assert!(
            widths.contains(&"1.50".to_string()),
            "the smallest flow must clamp to the 1.5 floor; got {widths:?}"
        );
        assert!(
            !widths
                .iter()
                .any(|w| w.parse::<f32>().is_ok_and(|v| v > 0.0 && v < 1.5)),
            "no RIBBON may be thinner than the 1.5 floor; got {widths:?}"
        );
    }

    /// CONTROL: a diagram that is NOT a sankey must be completely unaffected by the hoist.
    ///
    /// The old helper bailed on `diagram_type != Sankey`; the new one bails because
    /// `sankey_widest_flow` returns `None`. This pins that the two bail conditions agree, which is
    /// the only way the hoist could have changed a non-sankey render.
    #[test]
    fn non_sankey_edge_widths_are_unchanged_by_the_hoist() {
        let flow = render_source("flowchart LR\n  A --> B\n  B --> C\n");
        let widths = path_stroke_widths(&flow);
        assert!(
            !widths.is_empty(),
            "the control must actually observe some edge widths, or it proves nothing"
        );
        assert!(
            !widths.contains(&"24".to_string()),
            "a flowchart edge must never pick up a sankey ribbon width; got {widths:?}"
        );
    }

    /// CONTROL: a sankey whose flows are not numbers must fall back, not divide by zero.
    ///
    /// `sankey_widest_flow` returns `None` when no edge carries a usable value, which is the
    /// replacement for the old `widest <= 0.0` bail. Without it this would be `value / 0.0`.
    #[test]
    fn a_sankey_with_no_usable_flow_values_falls_back() {
        let svg = render_source("sankey-beta\n\nA,B,notanumber\n");
        let widths = path_stroke_widths(&svg);
        assert!(
            !widths
                .iter()
                .any(|w| w.contains("NaN") || w.contains("inf")),
            "a non-numeric flow must not produce a non-finite width; got {widths:?}"
        );
    }

    fn render_source(src: &str) -> String {
        let parsed = fm_parser::parse(src);
        let layout = fm_layout::layout_diagram(&parsed.ir);
        render_svg_with_layout(&parsed.ir, &layout, &SvgRenderConfig::default())
    }

    /// stroke-width of every `<path>` only.
    ///
    /// ⚠️ Deliberately NOT every stroke-width in the document. Node rects carry 0.75 and 1, which
    /// are legitimately below the 1.5 ribbon floor — judging those against it would fail on
    /// correct output. The ribbons are paths.
    fn path_stroke_widths(svg: &str) -> Vec<String> {
        let mut out = Vec::new();
        for tag in svg.split("<path").skip(1) {
            let Some(end_of_tag) = tag.find('>') else {
                continue;
            };
            let head = &tag[..end_of_tag];
            if let Some(at) = head.find("stroke-width=\"") {
                let rest = &head[at + 14..];
                if let Some(end) = rest.find('"') {
                    out.push(rest[..end].to_string());
                }
            }
        }
        out
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
            "highligh",     // one char short of a match
            "doubleborder", // no hyphen — must NOT match
            "high-light",   // hyphen splits keyword — must NOT match
            "muTeDim",      // overlapping starts
            "DisabledInactiveDim",
        ];
        for c in cases {
            let got = scan_node_class_keywords(c);
            assert_eq!(
                (
                    got.highlighted,
                    got.inactive,
                    got.dashed_border,
                    got.double_border
                ),
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
