#![forbid(unsafe_code)]

use std::sync::{LazyLock, RwLock};
// `Diagram::last_layout` is the only `Arc` user and lives behind `#[cfg(target_arch = "wasm32")]`,
// so an ungated import is an `unused_imports` warning — and a `-D warnings` failure — on the host.
#[cfg(target_arch = "wasm32")]
use std::sync::Arc;
// NOT `std::time::Instant`: wasm32-unknown-unknown std has no clock, so `Instant::now()`
// panics and `panic = "abort"` turns that into an `unreachable` trap — which is exactly what
// made every browser `renderSvg`/`Diagram::render` call fail with `RuntimeError: unreachable`
// while `parse`/`detectType` (untimed) kept working (GH#3). `web_time::Instant` is a drop-in
// re-export of `std::time::Instant` off-wasm and `performance.now()` in the browser.
use web_time::Instant;

#[cfg(any(not(target_arch = "wasm32"), test))]
use fm_core::MermaidGuardReport;
#[cfg(any(not(target_arch = "wasm32"), test))]
use fm_core::capability_matrix;
// The CGA hit-test helpers below are reachable only from the wasm32 `Diagram` bindings and from
// tests, so both they and this import are dead on a host non-test build.
#[cfg(any(target_arch = "wasm32", test))]
use fm_core::cga::{CgaLineSegment, CgaPoint, CgaRect};
#[cfg(any(not(target_arch = "wasm32"), test))]
use fm_core::mermaid_layout_guard_observability;
use fm_core::{
    Diagnostic, MermaidBudgetLedger, MermaidLayoutDecisionExplanation, MermaidLinkMode,
    MermaidWasmPressureSignals,
};
#[cfg(any(not(target_arch = "wasm32"), test))]
use fm_core::{MermaidSourceMap, MermaidSourceMapKind, Span};
#[cfg(any(not(target_arch = "wasm32"), test))]
use fm_layout::build_layout_guard_report_with_pressure;
use fm_layout::{
    LayoutConfig, LayoutGuardrails, TracedLayout, build_layout_decision_explanation,
    layout_diagram_traced, layout_diagram_traced_with_config,
    layout_diagram_traced_with_config_and_guardrails,
};
use fm_parser::{
    apply_parse_lens_delete, apply_parse_lens_edit, apply_parse_lens_insert_line_after,
    build_parse_lens, detect_type_with_confidence, parse,
};
#[cfg(target_arch = "wasm32")]
use fm_render_canvas::render_to_canvas_with_layout;
#[cfg(target_arch = "wasm32")]
use fm_render_canvas::{
    Canvas2dContext, CanvasRenderResult, LineCap, LineJoin, TextAlign, TextBaseline, TextMetrics,
};
use fm_render_canvas::{CanvasRenderConfig, GpuRenderPlan};
use fm_render_svg::{
    SvgRenderConfig, ThemeColors, ThemePreset, describe_diagram_with_layout, render_svg_with_layout,
};
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(any(not(target_arch = "wasm32"), test))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmRenderOutput {
    pub svg: String,
    pub detected_type: String,
    pub accessibility_summary: String,
    pub trace_id: String,
    pub decision_id: String,
    pub policy_id: String,
    pub schema_version: String,
    pub guard: MermaidGuardReport,
    pub layout_decision_explanation: MermaidLayoutDecisionExplanation,
    pub layout: LayoutRuntimeSummary,
    pub source_spans: Vec<SourceSpanRecord>,
    /// FNX analysis witness metadata for telemetry (optional, additive field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fnx_witness: Option<WasmFnxWitness>,
}

/// FNX analysis witness for WASM API consumers.
#[cfg(any(not(target_arch = "wasm32"), test))]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmFnxWitness {
    /// Whether FNX integration was available.
    pub enabled: bool,
    /// Whether FNX analysis was used for this render.
    pub used: bool,
    /// Projection mode used for analysis.
    pub projection_mode: String,
    /// List of algorithms invoked.
    pub algorithms_invoked: Vec<String>,
    /// Analysis time in microseconds.
    pub analysis_time_us: u64,
    /// Whether budget was exceeded.
    pub budget_exceeded: bool,
    /// Fallback level if degradation occurred.
    pub fallback_level: String,
    /// Fallback reason code.
    pub fallback_reason: String,
}

#[cfg(any(not(target_arch = "wasm32"), test))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSpanRecord {
    kind: &'static str,
    index: usize,
    id: Option<String>,
    element_id: String,
    span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
enum WebRendererKind {
    #[default]
    #[serde(rename = "canvas2d")]
    Canvas2d,
    #[serde(rename = "webgpu")]
    WebGpu,
}

impl WebRendererKind {
    #[cfg(target_arch = "wasm32")]
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Canvas2d => "canvas2d",
            Self::WebGpu => "webgpu",
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererRuntimeSummary {
    requested: String,
    actual: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback_reason: Option<String>,
}

#[cfg(any(target_arch = "wasm32", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolvedRenderer {
    requested: WebRendererKind,
    actual: WebRendererKind,
    fallback_reason: Option<&'static str>,
}

#[cfg(target_arch = "wasm32")]
impl ResolvedRenderer {
    #[must_use]
    fn summary(self) -> RendererRuntimeSummary {
        RendererRuntimeSummary {
            requested: self.requested.as_str().to_string(),
            actual: self.actual.as_str().to_string(),
            fallback_reason: self.fallback_reason.map(str::to_string),
        }
    }
}

#[cfg(target_arch = "wasm32")]
const WEBGPU_RENDERER_IMPLEMENTED: bool = false;

#[derive(Debug, Clone)]
struct RuntimeConfig {
    renderer: WebRendererKind,
    svg: SvgRenderConfig,
    canvas: CanvasRenderConfig,
    pressure: MermaidWasmPressureSignals,
}

/// A browser-consumable description of the primitive sets prepared for WebGPU.
///
/// The device submission remains in `fm-render-canvas`: this boundary owns parsing, layout and
/// canvas-context acquisition, while the device-pass owner owns buffer uploads and draw encoding.
/// Returning counts and bounds lets a host reject an empty or mismatched plan before it creates GPU
/// resources, without duplicating the renderer's primitive extraction in JavaScript.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebGpuPlanSummary {
    bounds: [f32; 4],
    cluster_instances: usize,
    edge_segments: usize,
    arrowheads: usize,
    node_instances: usize,
    text_quads: usize,
    text_runs: usize,
}

impl From<&GpuRenderPlan> for WebGpuPlanSummary {
    fn from(plan: &GpuRenderPlan) -> Self {
        Self {
            bounds: [
                plan.bounds.x,
                plan.bounds.y,
                plan.bounds.width,
                plan.bounds.height,
            ],
            cluster_instances: plan.cluster_instances.len(),
            edge_segments: plan.edge_segments.len(),
            arrowheads: plan.arrowheads.len(),
            node_instances: plan.node_instances.len(),
            text_quads: plan.text_quads.len(),
            text_runs: plan.text_runs.len(),
        }
    }
}

struct PlannedWebGpuDiagram {
    #[cfg(test)]
    ir: fm_core::MermaidDiagramIr,
    plan: GpuRenderPlan,
}

/// A parsed diagram and the geometry every raster surface reads from it.
///
/// Extracted so the WebGPU plan and the hit regions cannot be computed from two different layouts.
/// They must agree about where a node landed — that is the whole content of a hit region — and the
/// layout depends on the resolved font metrics, so recomputing it from a separately-merged config
/// would make the regions drift from the picture whenever a theme or a font override moved.
struct DiagramGeometry {
    ir: fm_core::MermaidDiagramIr,
    layout: fm_layout::DiagramLayout,
    canvas: CanvasRenderConfig,
}

/// Resolve the runtime config, parse, and lay the diagram out — the step before any raster backend.
fn build_diagram_geometry(input: &str, config: Option<JsValue>) -> Result<DiagramGeometry, String> {
    let overrides: RuntimeInitConfig = parse_js_value_or_default(config);
    let runtime = read_runtime_config();
    let requested_theme = requested_theme_preset(&overrides)?;
    let effective_svg = overrides.effective_svg_overrides();
    let theme_name = overrides
        .theme
        .as_deref()
        .or(effective_svg.theme.as_deref());
    let svg = merge_svg_config(&runtime.svg, &effective_svg, theme_name)?;
    let canvas_base = requested_theme.map_or_else(
        || runtime.canvas.clone(),
        |preset| apply_canvas_theme_preset(runtime.canvas.clone(), preset),
    );
    let canvas = align_canvas_typography_with_svg(
        merge_canvas_config(&canvas_base, &overrides.canvas),
        &svg,
    );
    let parsed = parse(input);
    let layout = fm_layout::layout_diagram_with_config(
        &parsed.ir,
        LayoutConfig {
            font_metrics: Some(svg.font_metrics()),
            ..Default::default()
        },
    );

    Ok(DiagramGeometry {
        ir: parsed.ir,
        layout,
        canvas,
    })
}

fn build_webgpu_plan(input: &str, config: Option<JsValue>) -> Result<PlannedWebGpuDiagram, String> {
    let geometry = build_diagram_geometry(input, config)?;
    let scene = fm_layout::build_render_scene(&geometry.ir, &geometry.layout);
    let plan = GpuRenderPlan::from_layout_and_scene(
        &geometry.ir,
        &geometry.layout,
        &scene,
        geometry.canvas.edge_stroke_width as f32,
    );

    Ok(PlannedWebGpuDiagram {
        #[cfg(test)]
        ir: geometry.ir,
        plan,
    })
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let svg = SvgRenderConfig::default();
        let canvas = align_canvas_typography_with_svg(CanvasRenderConfig::default(), &svg);
        Self {
            renderer: WebRendererKind::Canvas2d,
            svg,
            canvas,
            pressure: MermaidWasmPressureSignals::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RuntimeInitConfig {
    theme: Option<String>,
    renderer: Option<WebRendererKind>,
    #[serde(default)]
    svg: SvgConfigOverrides,
    #[serde(default)]
    canvas: CanvasConfigOverrides,
    #[serde(default)]
    pressure: PressureConfigOverrides,
    // Top-level SVG override fields for direct JS options: { theme, responsive, fontSize, ... }
    responsive: Option<bool>,
    accessible: Option<bool>,
    font_size: Option<f32>,
    padding: Option<f32>,
    shadows: Option<bool>,
    rounded_corners: Option<f32>,
    embed_theme_css: Option<bool>,
    enable_links: Option<bool>,
    link_mode: Option<String>,
    node_gradients: Option<bool>,
    cluster_fill_opacity: Option<f32>,
}

impl RuntimeInitConfig {
    fn effective_svg_overrides(&self) -> SvgConfigOverrides {
        let mut merged = SvgConfigOverrides {
            responsive: self.responsive,
            accessible: self.accessible,
            font_size: self.font_size,
            padding: self.padding,
            shadows: self.shadows,
            rounded_corners: self.rounded_corners,
            embed_theme_css: self.embed_theme_css,
            theme: self.theme.clone(),
            enable_links: self.enable_links,
            link_mode: self.link_mode.clone(),
            node_gradients: self.node_gradients,
            cluster_fill_opacity: self.cluster_fill_opacity,
        };
        if self.svg.responsive.is_some() {
            merged.responsive = self.svg.responsive;
        }
        if self.svg.accessible.is_some() {
            merged.accessible = self.svg.accessible;
        }
        if self.svg.font_size.is_some() {
            merged.font_size = self.svg.font_size;
        }
        if self.svg.padding.is_some() {
            merged.padding = self.svg.padding;
        }
        if self.svg.shadows.is_some() {
            merged.shadows = self.svg.shadows;
        }
        if self.svg.rounded_corners.is_some() {
            merged.rounded_corners = self.svg.rounded_corners;
        }
        if self.svg.embed_theme_css.is_some() {
            merged.embed_theme_css = self.svg.embed_theme_css;
        }
        if self.svg.theme.is_some() {
            merged.theme = self.svg.theme.clone();
        }
        if self.svg.enable_links.is_some() {
            merged.enable_links = self.svg.enable_links;
        }
        if self.svg.link_mode.is_some() {
            merged.link_mode = self.svg.link_mode.clone();
        }
        if self.svg.node_gradients.is_some() {
            merged.node_gradients = self.svg.node_gradients;
        }
        if self.svg.cluster_fill_opacity.is_some() {
            merged.cluster_fill_opacity = self.svg.cluster_fill_opacity;
        }
        merged
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SvgConfigOverrides {
    responsive: Option<bool>,
    accessible: Option<bool>,
    font_size: Option<f32>,
    padding: Option<f32>,
    shadows: Option<bool>,
    rounded_corners: Option<f32>,
    embed_theme_css: Option<bool>,
    theme: Option<String>,
    enable_links: Option<bool>,
    link_mode: Option<String>,
    node_gradients: Option<bool>,
    cluster_fill_opacity: Option<f32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CanvasConfigOverrides {
    font_size: Option<f64>,
    padding: Option<f64>,
    auto_fit: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PressureConfigOverrides {
    frame_budget_ms: Option<u16>,
    frame_time_ms: Option<u16>,
    event_loop_lag_ms: Option<u16>,
    worker_saturation_permille: Option<u16>,
}

/// Where a browser host should send a render (bd-2u0.6 scope item 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "camelCase")]
pub enum CanvasTarget {
    /// Render in the worker against a transferred `OffscreenCanvas`.
    OffscreenInWorker,
    /// Render in the worker to SVG text and hand the string back for the main thread to insert.
    ///
    /// The useful middle case, and the reason this is not a boolean: a browser without
    /// `OffscreenCanvas` can still keep parse and layout off the main thread, which is where the
    /// time goes on a large diagram. Falling straight back to a main-thread canvas would give up
    /// the entire benefit over one missing API.
    SvgInWorker,
    /// Render on the main thread. The last resort.
    MainThreadCanvas,
}

/// What the host observed about the environment it is running in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostCapabilities {
    /// `typeof OffscreenCanvas !== "undefined"`.
    pub offscreen_canvas: bool,
    /// `typeof Worker !== "undefined"`.
    pub worker: bool,
    /// Whether the canvas has already been transferred to the worker.
    ///
    /// Separate from `offscreen_canvas` on purpose: `transferControlToOffscreen` may be present and
    /// still fail, and a canvas can be transferred only ONCE per element. A host that re-transfers
    /// gets a DOM exception, so the decision has to see whether the transfer actually happened
    /// rather than whether the API exists.
    pub canvas_transferred: bool,
}

/// Choose a render target from what the host can actually do.
///
/// Ordered by how much work leaves the main thread, most first. Every step down is a real
/// degradation rather than an equivalent alternative, so the decision is explicit and testable off
/// the browser — the wasm paths themselves cannot be unit-tested here, which makes this the only
/// part of the routing that can carry a proof.
#[must_use]
pub fn choose_canvas_target(capabilities: HostCapabilities) -> CanvasTarget {
    if !capabilities.worker {
        // No worker: nothing can leave the main thread, and an OffscreenCanvas on the main thread
        // buys nothing because the work still blocks the same event loop.
        return CanvasTarget::MainThreadCanvas;
    }
    if capabilities.offscreen_canvas && capabilities.canvas_transferred {
        return CanvasTarget::OffscreenInWorker;
    }
    CanvasTarget::SvgInWorker
}

/// A structured-clone-safe request for the browser worker render path.
///
/// Configuration stays JSON text so callers can forward the same payload to a
/// worker without depending on a browser-only `JsValue` representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRenderRequest {
    pub request_id: u64,
    pub input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_json: Option<String>,
}

/// Messages accepted by the dedicated diagram render worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorkerRenderMessage {
    Render(WorkerRenderRequest),
    Cancel { request_id: u64 },
}

/// The action a worker host must perform after receiving a protocol message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerRenderAction {
    Start(WorkerRenderRequest),
    Supersede {
        cancelled_request_id: u64,
        next: WorkerRenderRequest,
    },
    Cancelled {
        request_id: u64,
    },
    Ignored {
        request_id: u64,
    },
}

/// Per-stage wall time for one worker render, in milliseconds.
///
/// Reported so a UI can attribute a slow keystroke to parse, layout, or render instead of guessing
/// from one total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRenderTimings {
    pub parse_ms: u64,
    pub layout_ms: u64,
    pub render_ms: u64,
    pub total_ms: u64,
}

/// What a worker sends back to the UI thread.
///
/// Structured-clone-safe and `serde`-round-trippable, like [`WorkerRenderMessage`], so the host can
/// forward it across `postMessage` without a browser-only representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorkerRenderResponse {
    /// The render finished and its output is safe to publish.
    Completed {
        request_id: u64,
        svg: String,
        detected_type: String,
        accessibility_summary: String,
        node_count: usize,
        edge_count: usize,
        svg_bytes: usize,
        timings: WorkerRenderTimings,
        /// Parse diagnostics verbatim, so the UI can surface the same severities, messages, spans
        /// and remediation the CLI shows rather than a lossy summary.
        diagnostics: Vec<Diagnostic>,
    },
    /// The request could not be rendered. Carries an actionable reason, and any diagnostics that
    /// were produced before the failure.
    Failed {
        request_id: u64,
        error: String,
        diagnostics: Vec<Diagnostic>,
    },
    /// A newer request replaced this one, so its output must be discarded rather than published.
    Superseded { request_id: u64 },
}

impl WorkerRenderResponse {
    /// The request this response belongs to.
    #[must_use]
    pub const fn request_id(&self) -> u64 {
        match self {
            Self::Completed { request_id, .. }
            | Self::Failed { request_id, .. }
            | Self::Superseded { request_id } => *request_id,
        }
    }
}

/// Run one worker render request, honoring its per-request configuration.
///
/// `config_json` is applied here through the same `merge_svg_config` / `merge_pressure_config` path
/// the `renderSvg` entry point uses, so a worker and a main-thread render with the same config
/// produce the same bytes. Before this existed the field was carried across the protocol and
/// silently dropped, which meant a themed worker render came back with default styling.
#[must_use]
pub fn render_worker_request(request: &WorkerRenderRequest) -> WorkerRenderResponse {
    let overrides: RuntimeInitConfig = match request.config_json.as_deref() {
        None => RuntimeInitConfig::default(),
        Some(json) => match serde_json::from_str(json) {
            Ok(parsed) => parsed,
            Err(error) => {
                return WorkerRenderResponse::Failed {
                    request_id: request.request_id,
                    error: format!(
                        "invalid configJson: {error}; expected a JSON object of render overrides"
                    ),
                    diagnostics: Vec::new(),
                };
            }
        },
    };

    let runtime = read_runtime_config();
    let effective_svg = overrides.effective_svg_overrides();
    let theme_name = overrides
        .theme
        .as_deref()
        .or(effective_svg.theme.as_deref());
    let svg_config = match merge_svg_config(&runtime.svg, &effective_svg, theme_name) {
        Ok(config) => config,
        Err(error) => {
            return WorkerRenderResponse::Failed {
                request_id: request.request_id,
                error,
                diagnostics: Vec::new(),
            };
        }
    };
    let pressure = merge_pressure_config(&runtime.pressure, &overrides.pressure).into_report();
    let mut budget_broker = MermaidBudgetLedger::new(&pressure);

    let parse_start = Instant::now();
    let parsed = parse(&request.input);
    let parse_ms = elapsed_ms(parse_start);
    budget_broker.record_parse(parse_ms);

    let layout_guardrails = LayoutGuardrails::from(&budget_broker);
    let layout_config = LayoutConfig {
        font_metrics: Some(svg_config.font_metrics()),
        ..Default::default()
    };
    let layout_start = Instant::now();
    let traced_layout = layout_diagram_traced_with_config_and_guardrails(
        &parsed.ir,
        fm_layout::LayoutAlgorithm::Auto,
        layout_config,
        layout_guardrails,
    );
    let layout_ms = elapsed_ms(layout_start);
    budget_broker.record_layout(layout_ms);

    let mut svg_config = svg_config;
    apply_budget_svg_simplifications(&mut svg_config, &budget_broker);
    let render_start = Instant::now();
    let svg = render_svg_with_layout(&parsed.ir, &traced_layout.layout, &svg_config);
    let render_ms = elapsed_ms(render_start);

    WorkerRenderResponse::Completed {
        request_id: request.request_id,
        detected_type: parsed.ir.diagram_type.as_str().to_string(),
        accessibility_summary: describe_diagram_with_layout(
            &parsed.ir,
            Some(&traced_layout.layout),
        ),
        node_count: traced_layout.layout.nodes.len(),
        edge_count: traced_layout.layout.edges.len(),
        svg_bytes: svg.len(),
        timings: WorkerRenderTimings {
            parse_ms,
            layout_ms,
            render_ms,
            total_ms: parse_ms.saturating_add(layout_ms).saturating_add(render_ms),
        },
        diagnostics: parsed.ir.diagnostics.clone(),
        svg,
    }
}

fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

/// Tracks the one live worker render so typing updates never publish stale output.
#[derive(Debug, Default)]
pub struct WorkerRenderCoordinator {
    active_request_id: Option<u64>,
}

impl WorkerRenderCoordinator {
    /// Applies a message in arrival order and returns the host action.
    #[must_use]
    pub fn handle(&mut self, message: WorkerRenderMessage) -> WorkerRenderAction {
        match message {
            WorkerRenderMessage::Render(next) => {
                if let Some(cancelled_request_id) = self.active_request_id.replace(next.request_id)
                {
                    WorkerRenderAction::Supersede {
                        cancelled_request_id,
                        next,
                    }
                } else {
                    WorkerRenderAction::Start(next)
                }
            }
            WorkerRenderMessage::Cancel { request_id }
                if self.active_request_id == Some(request_id) =>
            {
                self.active_request_id = None;
                WorkerRenderAction::Cancelled { request_id }
            }
            WorkerRenderMessage::Cancel { request_id } => {
                WorkerRenderAction::Ignored { request_id }
            }
        }
    }

    /// Reports completion only when it belongs to the current request.
    ///
    /// A host must discard a `false` result because a newer typing update has
    /// already replaced that request.
    pub fn complete(&mut self, request_id: u64) -> bool {
        if self.active_request_id == Some(request_id) {
            self.active_request_id = None;
            true
        } else {
            false
        }
    }

    /// Gate a finished render before it reaches the UI.
    ///
    /// Returns the response when it belongs to the live request, and
    /// [`WorkerRenderResponse::Superseded`] when a newer typing update already replaced it. Hosts
    /// should publish whatever comes back: the substitution is what stops stale output from
    /// overwriting a newer diagram, and it stays observable instead of vanishing silently.
    #[must_use]
    pub fn publish(&mut self, response: WorkerRenderResponse) -> WorkerRenderResponse {
        let request_id = response.request_id();
        if self.complete(request_id) {
            response
        } else {
            WorkerRenderResponse::Superseded { request_id }
        }
    }
}

/// Handle one protocol message end to end: decide the action, render when the action says to, and
/// gate the result against a newer request.
///
/// This is the whole worker loop minus the `postMessage` plumbing, which keeps the decision logic
/// testable without a browser.
#[must_use]
pub fn handle_worker_message(
    coordinator: &mut WorkerRenderCoordinator,
    message: WorkerRenderMessage,
) -> Option<WorkerRenderResponse> {
    match coordinator.handle(message) {
        WorkerRenderAction::Start(request)
        | WorkerRenderAction::Supersede { next: request, .. } => {
            let response = render_worker_request(&request);
            Some(coordinator.publish(response))
        }
        // A cancel produces no render, and an unknown request id is not ours to answer.
        WorkerRenderAction::Cancelled { .. } | WorkerRenderAction::Ignored { .. } => None,
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagramRenderOutput {
    renderer: RendererRuntimeSummary,
    guard: WasmGuardSummary,
    layout_decision_explanation: MermaidLayoutDecisionExplanation,
    layout: LayoutRuntimeSummary,
    canvas: CanvasRenderSummary,
}

#[cfg(target_arch = "wasm32")]
impl DiagramRenderOutput {
    fn new(
        traced_layout: &TracedLayout,
        layout_config: &LayoutConfig,
        renderer: RendererRuntimeSummary,
        guard: WasmGuardSummary,
        layout_decision_explanation: MermaidLayoutDecisionExplanation,
        canvas: &CanvasRenderResult,
    ) -> Self {
        Self {
            renderer,
            guard,
            layout_decision_explanation,
            layout: LayoutRuntimeSummary::new(traced_layout, layout_config),
            canvas: CanvasRenderSummary::from(canvas),
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Serialize)]
struct WasmGuardSummary {
    budget_exceeded: bool,
    route_budget_exceeded: bool,
    layout_budget_exceeded: bool,
    route_ops_estimate: usize,
    layout_iterations_estimate: usize,
    layout_time_estimate_ms: usize,
    layout_requested_algorithm: Option<String>,
    layout_selected_algorithm: Option<String>,
    guard_reason: Option<String>,
    pressure: WasmPressureSummary,
}

#[cfg(target_arch = "wasm32")]
impl WasmGuardSummary {
    fn from_layout(
        traced_layout: &TracedLayout,
        pressure: &fm_core::MermaidPressureReport,
    ) -> Self {
        let guard = traced_layout.trace.guard;
        let layout_budget_exceeded = guard.time_budget_exceeded || guard.iteration_budget_exceeded;
        Self {
            budget_exceeded: guard.time_budget_exceeded
                || guard.iteration_budget_exceeded
                || guard.route_budget_exceeded,
            route_budget_exceeded: guard.route_budget_exceeded,
            layout_budget_exceeded,
            route_ops_estimate: guard.estimated_route_ops,
            layout_iterations_estimate: guard.estimated_layout_iterations,
            layout_time_estimate_ms: guard.estimated_layout_time_ms,
            layout_requested_algorithm: Some(
                traced_layout.trace.dispatch.requested.as_str().to_string(),
            ),
            layout_selected_algorithm: Some(
                traced_layout.trace.dispatch.selected.as_str().to_string(),
            ),
            guard_reason: Some(guard.reason.to_string()),
            pressure: WasmPressureSummary::from(pressure),
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Serialize)]
struct WasmPressureSummary {
    tier: String,
}

#[cfg(target_arch = "wasm32")]
impl From<&fm_core::MermaidPressureReport> for WasmPressureSummary {
    fn from(pressure: &fm_core::MermaidPressureReport) -> Self {
        Self {
            tier: pressure.tier.as_str().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutRuntimeSummary {
    cycle_strategy: String,
    cycle_clusters_collapsed: bool,
    node_count: usize,
    edge_count: usize,
    reversed_edges: usize,
    cycle_count: usize,
    cycle_node_count: usize,
    max_cycle_size: usize,
    reversed_edge_total_length: f32,
    total_edge_length: f32,
    phase_iterations: usize,
    /// Whether this layout used the incremental fast path.
    incremental: bool,
    /// Number of nodes that were recomputed (0 = full layout or cache hit).
    recomputed_nodes: usize,
    /// Incremental layout duration in microseconds.
    recompute_duration_us: u64,
}

impl LayoutRuntimeSummary {
    fn new(traced_layout: &TracedLayout, layout_config: &LayoutConfig) -> Self {
        let layout = &traced_layout.layout;
        Self {
            cycle_strategy: layout_config.cycle_strategy.as_str().to_string(),
            cycle_clusters_collapsed: layout_config.collapse_cycle_clusters,
            node_count: layout.stats.node_count,
            edge_count: layout.stats.edge_count,
            reversed_edges: layout.stats.reversed_edges,
            cycle_count: layout.stats.cycle_count,
            cycle_node_count: layout.stats.cycle_node_count,
            max_cycle_size: layout.stats.max_cycle_size,
            reversed_edge_total_length: layout.stats.reversed_edge_total_length,
            total_edge_length: layout.stats.total_edge_length,
            phase_iterations: layout.stats.phase_iterations,
            incremental: traced_layout.trace.incremental.cache_hit
                || traced_layout
                    .trace
                    .incremental
                    .query_type
                    .contains("incremental"),
            recomputed_nodes: traced_layout.trace.incremental.recomputed_nodes,
            recompute_duration_us: traced_layout.trace.incremental.recompute_duration_us,
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CanvasRenderSummary {
    draw_calls: usize,
    edges_drawn: usize,
    labels_drawn: usize,
}

#[cfg(target_arch = "wasm32")]
impl From<&CanvasRenderResult> for CanvasRenderSummary {
    fn from(value: &CanvasRenderResult) -> Self {
        Self {
            draw_calls: value.draw_calls,
            edges_drawn: value.edges_drawn,
            labels_drawn: value.labels_drawn,
        }
    }
}

static RUNTIME_CONFIG: LazyLock<RwLock<RuntimeConfig>> =
    LazyLock::new(|| RwLock::new(RuntimeConfig::default()));

#[cfg(any(not(target_arch = "wasm32"), test))]
fn source_map_records(source_map: MermaidSourceMap) -> Vec<SourceSpanRecord> {
    source_map
        .entries
        .into_iter()
        .map(|entry| SourceSpanRecord {
            kind: match entry.kind {
                MermaidSourceMapKind::Node => "node",
                MermaidSourceMapKind::Edge => "edge",
                MermaidSourceMapKind::Cluster => "cluster",
            },
            index: entry.index,
            id: entry.source_id,
            element_id: entry.element_id,
            span: entry.span,
        })
        .collect()
}

#[cfg(any(not(target_arch = "wasm32"), test))]
fn collect_source_spans(
    ir: &fm_core::MermaidDiagramIr,
    _layout: &fm_layout::DiagramLayout,
) -> Vec<SourceSpanRecord> {
    source_map_records(ir.source_map())
}

fn read_runtime_config() -> RuntimeConfig {
    match RUNTIME_CONFIG.read() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

fn with_runtime_config<R>(f: impl FnOnce(&RuntimeConfig) -> R) -> R {
    match RUNTIME_CONFIG.read() {
        Ok(guard) => f(&guard),
        Err(poisoned) => f(&poisoned.into_inner()),
    }
}

fn write_runtime_config(config: RuntimeConfig) {
    match RUNTIME_CONFIG.write() {
        Ok(mut guard) => *guard = config,
        Err(poisoned) => {
            let mut guard = poisoned.into_inner();
            *guard = config;
        }
    }
}

fn js_error(message: impl Into<String>) -> JsValue {
    JsValue::from_str(&message.into())
}

#[cfg(target_arch = "wasm32")]
fn js_error_with_value(prefix: &str, value: JsValue) -> JsValue {
    let detail = value
        .as_string()
        .unwrap_or_else(|| format!("non-string JS error: {value:?}"));
    js_error(format!("{prefix}: {detail}"))
}

fn parse_js_value_or_default<T>(value: Option<JsValue>) -> T
where
    T: for<'de> Deserialize<'de> + Default,
{
    match value {
        None => T::default(),
        Some(raw) if raw.is_undefined() || raw.is_null() => T::default(),
        Some(raw) => {
            #[cfg(target_arch = "wasm32")]
            {
                serde_wasm_bindgen::from_value(raw).unwrap_or_else(|_| T::default())
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let _ = raw;
                T::default()
            }
        }
    }
}

fn to_js_value<T>(value: &T) -> Result<JsValue, JsValue>
where
    T: Serialize,
{
    #[cfg(target_arch = "wasm32")]
    {
        serde_wasm_bindgen::to_value(value)
            .map_err(|err| js_error(format!("failed to serialize response: {err}")))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        serde_json::to_string(value)
            .map(|json| JsValue::from_str(&json))
            .map_err(|err| js_error(format!("failed to serialize response: {err}")))
    }
}

/// Merge SVG overrides onto `base`.
///
/// Errors are `String`, not `JsValue`, so the merge is callable from contexts with no JS at all —
/// the worker render path builds its config from JSON text, and a `JsValue` error there could not
/// be read back into a structured response. Callers at the JS boundary map with [`js_error`].
fn merge_svg_config(
    base: &SvgRenderConfig,
    overrides: &SvgConfigOverrides,
    theme_override: Option<&str>,
) -> Result<SvgRenderConfig, String> {
    let mut merged = base.clone();
    let parse_link_mode = |value: &str| -> Result<MermaidLinkMode, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "disabled" => Ok(MermaidLinkMode::Off),
            "inline" | "on" | "enabled" => Ok(MermaidLinkMode::Inline),
            "footnote" | "notes" => Ok(MermaidLinkMode::Footnote),
            other => Err(format!(
                "invalid link mode '{other}': expected off, inline, or footnote"
            )),
        }
    };

    if let Some(value) = overrides.responsive {
        merged.responsive = value;
    }
    if let Some(value) = overrides.accessible {
        merged.accessible = value;
    }
    if let Some(value) = overrides.font_size
        && value.is_finite()
        && value > 0.0
    {
        merged.font_size = value;
    }
    if let Some(value) = overrides.padding
        && value.is_finite()
        && value >= 0.0
    {
        merged.padding = value;
    }
    if let Some(value) = overrides.shadows {
        merged.shadows = value;
    }
    if let Some(value) = overrides.rounded_corners
        && value.is_finite()
        && value >= 0.0
    {
        merged.rounded_corners = value;
    }
    if let Some(value) = overrides.embed_theme_css {
        merged.embed_theme_css = value;
    }
    if let Some(value) = overrides.link_mode.as_deref() {
        merged.link_mode = parse_link_mode(value)?;
    }
    if let Some(value) = overrides.enable_links {
        if !value {
            merged.link_mode = MermaidLinkMode::Off;
        } else if overrides.link_mode.is_none() {
            merged.link_mode = MermaidLinkMode::Inline;
        }
    }

    let theme_name = overrides.theme.as_deref().or(theme_override);
    if let Some(name) = theme_name {
        let preset = name.parse::<ThemePreset>().map_err(|err| {
            format!(
                "invalid theme '{name}': {err}; expected one of default,mermaid,dark,forest,neutral,corporate,neon,pastel,high-contrast,monochrome,blueprint"
            )
        })?;
        merged.theme = preset;
        if preset == ThemePreset::Mermaid {
            merged.cluster_fill_opacity = 1.0;
            merged.node_gradients = false;
            merged.rounded_corners = 5.0;
        }
    }

    if let Some(value) = overrides.node_gradients {
        merged.node_gradients = value;
    }
    if let Some(value) = overrides.cluster_fill_opacity {
        merged.cluster_fill_opacity = value;
    }

    Ok(merged)
}

fn merge_canvas_config(
    base: &CanvasRenderConfig,
    overrides: &CanvasConfigOverrides,
) -> CanvasRenderConfig {
    let mut merged = base.clone();

    if let Some(value) = overrides.font_size
        && value.is_finite()
        && value > 0.0
    {
        merged.font_size = value;
    }
    if let Some(value) = overrides.padding
        && value.is_finite()
        && value >= 0.0
    {
        merged.padding = value;
    }
    if let Some(value) = overrides.auto_fit {
        merged.auto_fit = value;
    }

    merged
}

fn apply_canvas_theme_preset(
    mut canvas: CanvasRenderConfig,
    preset: ThemePreset,
) -> CanvasRenderConfig {
    let colors = ThemeColors::from_preset(preset);
    canvas.node_fill = colors.node_fill;
    canvas.node_stroke = colors.node_stroke;
    canvas.edge_stroke = colors.edge;
    canvas.cluster_fill = colors.cluster_fill;
    canvas.cluster_stroke = colors.cluster_stroke;
    canvas.label_color = colors.text;
    canvas
}

fn align_canvas_typography_with_svg(
    mut canvas: CanvasRenderConfig,
    svg: &SvgRenderConfig,
) -> CanvasRenderConfig {
    canvas.font_family.clone_from(&svg.font_family);
    canvas.font_size = f64::from(svg.font_size);
    // The gantt today date rides along with the typography rather than being plumbed separately,
    // because the requirement is the same one: the two backends must not disagree. A canvas preview
    // that drew a today line on a different day from the SVG export -- or drew none while the export
    // did -- would be a divergence between two views of the same diagram (bd-t1jj). Neither renderer
    // ever reads the clock; the date is supplied, and `None` here means no marker in either.
    canvas.gantt_today.clone_from(&svg.gantt_today);
    canvas
}

fn merge_pressure_config(
    base: &MermaidWasmPressureSignals,
    overrides: &PressureConfigOverrides,
) -> MermaidWasmPressureSignals {
    let mut merged = *base;
    if let Some(value) = overrides.frame_budget_ms {
        merged.frame_budget_ms = Some(value);
    }
    if let Some(value) = overrides.frame_time_ms {
        merged.frame_time_ms = Some(value);
    }
    if let Some(value) = overrides.event_loop_lag_ms {
        merged.event_loop_lag_ms = Some(value);
    }
    if let Some(value) = overrides.worker_saturation_permille {
        merged.worker_saturation_permille = Some(value.min(1_000));
    }
    merged
}

/// Resolve the requested theme preset. `String` error for the same reason as [`merge_svg_config`].
fn requested_theme_preset(overrides: &RuntimeInitConfig) -> Result<Option<ThemePreset>, String> {
    let effective_svg = overrides.effective_svg_overrides();
    let theme_name = effective_svg
        .theme
        .as_deref()
        .or(overrides.theme.as_deref());
    theme_name
        .map(|name| {
            name.parse::<ThemePreset>().map_err(|err| {
                format!(
                    "invalid theme '{name}': {err}; expected one of default,mermaid,dark,forest,neutral,corporate,neon,pastel,high-contrast,monochrome,blueprint"
                )
            })
        })
        .transpose()
}

fn merge_renderer_kind(
    base: WebRendererKind,
    override_renderer: Option<WebRendererKind>,
) -> WebRendererKind {
    override_renderer.unwrap_or(base)
}

#[cfg(any(target_arch = "wasm32", test))]
#[must_use]
fn resolve_renderer(
    requested: WebRendererKind,
    webgpu_supported: bool,
    webgpu_implemented: bool,
) -> ResolvedRenderer {
    match requested {
        WebRendererKind::Canvas2d => ResolvedRenderer {
            requested,
            actual: WebRendererKind::Canvas2d,
            fallback_reason: None,
        },
        WebRendererKind::WebGpu if !webgpu_supported => ResolvedRenderer {
            requested,
            actual: WebRendererKind::Canvas2d,
            fallback_reason: Some("webgpu_unavailable"),
        },
        WebRendererKind::WebGpu if !webgpu_implemented => ResolvedRenderer {
            requested,
            actual: WebRendererKind::Canvas2d,
            fallback_reason: Some("webgpu_not_implemented"),
        },
        WebRendererKind::WebGpu => ResolvedRenderer {
            requested,
            actual: WebRendererKind::WebGpu,
            fallback_reason: None,
        },
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn canvas_font_size_px(font: &str) -> f64 {
    font.split_whitespace()
        .next()
        .and_then(|token| token.strip_suffix("px"))
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(14.0)
}

fn apply_budget_svg_simplifications(
    config: &mut SvgRenderConfig,
    budget_broker: &MermaidBudgetLedger,
) {
    if budget_broker.should_simplify_render() {
        config.shadows = false;
    }
}

fn apply_degradation_to_svg(
    config: &mut SvgRenderConfig,
    degradation: &fm_core::MermaidDegradationPlan,
) {
    config.apply_degradation(degradation);
}

fn compute_wasm_degradation_plan(
    ir: &fm_core::MermaidDiagramIr,
    traced_layout: &TracedLayout,
    pressure: &fm_core::MermaidPressureReport,
) -> fm_core::MermaidDegradationPlan {
    let limits = fm_core::MermaidConfig::default();
    fm_core::compute_degradation_plan(&fm_core::DegradationContext {
        pressure_tier: pressure.tier,
        route_budget_exceeded: traced_layout.trace.guard.route_budget_exceeded,
        layout_budget_exceeded: traced_layout.trace.guard.iteration_budget_exceeded,
        time_budget_exceeded: traced_layout.trace.guard.time_budget_exceeded,
        node_limit_exceeded: ir.nodes.len() > limits.max_nodes,
        edge_limit_exceeded: ir.edges.len() > limits.max_edges,
    })
}

#[cfg(any(target_arch = "wasm32", test))]
fn hit_test_layout_node(layout: &fm_layout::DiagramLayout, x: f64, y: f64) -> Option<&str> {
    if !(x.is_finite() && y.is_finite()) {
        return None;
    }

    let point = CgaPoint::new(x, y);
    layout.nodes.iter().find_map(|node| {
        let bounds = node.bounds;
        CgaRect::new(
            f64::from(bounds.x),
            f64::from(bounds.y),
            f64::from(bounds.width),
            f64::from(bounds.height),
        )
        .contains(&point)
        .then_some(node.node_id.as_str())
    })
}

#[cfg(any(target_arch = "wasm32", test))]
fn hit_test_layout_edge(
    layout: &fm_layout::DiagramLayout,
    x: f64,
    y: f64,
    max_distance: f64,
) -> Option<usize> {
    if !(x.is_finite() && y.is_finite() && max_distance.is_finite() && max_distance >= 0.0) {
        return None;
    }

    let point = CgaPoint::new(x, y);
    let mut closest = None;
    for edge in &layout.edges {
        if edge.bundled {
            continue;
        }
        for points in edge.points.windows(2) {
            let [start, end] = points else {
                continue;
            };
            let segment = CgaLineSegment::new(
                CgaPoint::new(f64::from(start.x), f64::from(start.y)),
                CgaPoint::new(f64::from(end.x), f64::from(end.y)),
            );
            let distance = segment.distance_to_point(&point);
            let is_closer = match closest {
                None => true,
                Some((best_edge_index, best_distance)) => {
                    distance < best_distance
                        || (distance == best_distance && edge.edge_index < best_edge_index)
                }
            };
            if distance.is_finite() && distance <= max_distance && is_closer {
                closest = Some((edge.edge_index, distance));
            }
        }
    }
    closest.map(|(edge_index, _)| edge_index)
}

#[must_use]
#[cfg(any(not(target_arch = "wasm32"), test))]
pub fn render(input: &str) -> WasmRenderOutput {
    let runtime = read_runtime_config();
    let pressure = runtime.pressure.into_report();
    let mut budget_broker = MermaidBudgetLedger::new(&pressure);
    let parse_start = Instant::now();
    let parsed = parse(input);
    budget_broker.record_parse(
        parse_start
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
    );
    let layout_guardrails = LayoutGuardrails::from(&budget_broker);
    let layout_start = Instant::now();
    let layout_config = LayoutConfig {
        font_metrics: Some(runtime.svg.font_metrics()),
        ..Default::default()
    };
    let traced_layout = layout_diagram_traced_with_config_and_guardrails(
        &parsed.ir,
        fm_layout::LayoutAlgorithm::Auto,
        layout_config.clone(),
        layout_guardrails,
    );
    budget_broker.record_layout(
        layout_start
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
    );
    let mut guard = build_layout_guard_report_with_pressure(&parsed.ir, &traced_layout, pressure);
    #[cfg(any(not(target_arch = "wasm32"), test))]
    {
        let (_cx, observability) = mermaid_layout_guard_observability(
            "wasm.render",
            input,
            traced_layout.trace.dispatch.selected.as_str(),
            traced_layout.trace.guard.estimated_layout_time_ms.max(1) as u64,
        );
        guard.observability = observability;
    }
    let source_spans = collect_source_spans(&parsed.ir, &traced_layout.layout);
    let mut svg_config = runtime.svg;
    apply_budget_svg_simplifications(&mut svg_config, &budget_broker);
    apply_degradation_to_svg(&mut svg_config, &guard.degradation);
    let render_start = Instant::now();
    let svg = render_svg_with_layout(&parsed.ir, &traced_layout.layout, &svg_config);
    budget_broker.record_render(
        render_start
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
    );
    guard.budget_broker = budget_broker;
    let layout_decision_explanation = build_layout_decision_explanation(
        &parsed.ir,
        &traced_layout,
        guard.pressure.clone(),
        guard.budget_broker.total_budget_ms,
        guard.budget_broker.exhausted,
    );

    WasmRenderOutput {
        svg,
        detected_type: parsed.ir.diagram_type.as_str().to_string(),
        accessibility_summary: describe_diagram_with_layout(
            &parsed.ir,
            Some(&traced_layout.layout),
        ),
        trace_id: guard.observability.trace_id.to_string(),
        decision_id: guard.observability.decision_id.to_string(),
        policy_id: guard.observability.policy_id.to_string(),
        schema_version: guard.observability.schema_version.to_string(),
        guard,
        layout_decision_explanation,
        layout: LayoutRuntimeSummary::new(&traced_layout, &layout_config),
        source_spans,
        fnx_witness: build_wasm_fnx_witness(),
    }
}

/// Build FNX witness for WASM output.
#[cfg(any(not(target_arch = "wasm32"), test))]
fn build_wasm_fnx_witness() -> Option<WasmFnxWitness> {
    // FNX is not available in WASM builds (no fnx-integration feature)
    // This placeholder returns None; when FNX is available, it will be populated
    None
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn init(config: Option<JsValue>) -> Result<(), JsValue> {
    let overrides: RuntimeInitConfig = parse_js_value_or_default(config);
    let current = read_runtime_config();
    let requested_theme = requested_theme_preset(&overrides).map_err(js_error)?;
    let effective_svg = overrides.effective_svg_overrides();
    let theme_name = overrides
        .theme
        .as_deref()
        .or(effective_svg.theme.as_deref());
    let svg = merge_svg_config(&current.svg, &effective_svg, theme_name).map_err(js_error)?;
    let canvas_base = requested_theme.map_or_else(
        || current.canvas.clone(),
        |preset| apply_canvas_theme_preset(current.canvas.clone(), preset),
    );

    let next = RuntimeConfig {
        renderer: merge_renderer_kind(current.renderer, overrides.renderer),
        canvas: align_canvas_typography_with_svg(
            merge_canvas_config(&canvas_base, &overrides.canvas),
            &svg,
        ),
        svg,
        pressure: merge_pressure_config(&current.pressure, &overrides.pressure),
    };

    write_runtime_config(next);
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = renderSvg))]
pub fn render_svg_js(input: &str, config: Option<JsValue>) -> Result<String, JsValue> {
    let prepared = prepare_svg_render(input, config)?;
    Ok(render_svg_with_layout(
        &prepared.parsed.ir,
        &prepared.traced_layout.layout,
        &prepared.svg_config,
    ))
}

/// The shared parse → configure → layout pipeline behind [`render_svg_js`] and
/// [`render_deck_js`] (bd-cb9oi). One extraction instead of two copies: the pressure-tier
/// branching, budget broker, and degradation plumbing must stay identical between the two
/// exports or their SVG outputs drift — the deck/renderSvg byte-parity test pins that.
struct PreparedSvgRender {
    parsed: fm_parser::ParseResult,
    svg_config: SvgRenderConfig,
    traced_layout: fm_layout::TracedLayout,
}

fn prepare_svg_render(input: &str, config: Option<JsValue>) -> Result<PreparedSvgRender, JsValue> {
    let has_config = config
        .as_ref()
        .is_some_and(|raw| !raw.is_undefined() && !raw.is_null());

    let (mut svg_config, pressure_report) = if has_config {
        let overrides: RuntimeInitConfig = parse_js_value_or_default(config);
        with_runtime_config(|runtime| {
            let effective_svg = overrides.effective_svg_overrides();
            let theme_name = overrides
                .theme
                .as_deref()
                .or(effective_svg.theme.as_deref());
            let svg =
                merge_svg_config(&runtime.svg, &effective_svg, theme_name).map_err(js_error)?;
            let pressure =
                merge_pressure_config(&runtime.pressure, &overrides.pressure).into_report();
            Ok::<_, JsValue>((svg, pressure))
        })?
    } else {
        with_runtime_config(|runtime| (runtime.svg.clone(), runtime.pressure.into_report()))
    };

    let parsed = parse(input);
    let layout_config = LayoutConfig {
        font_metrics: Some(svg_config.font_metrics()),
        edge_routing: if svg_config.theme == ThemePreset::Mermaid {
            fm_layout::EdgeRouting::Spline
        } else {
            fm_layout::EdgeRouting::Orthogonal
        },
        ..Default::default()
    };

    let traced_layout = if pressure_report.tier == fm_core::MermaidPressureTier::Nominal {
        layout_diagram_traced_with_config(
            &parsed.ir,
            fm_layout::LayoutAlgorithm::Auto,
            layout_config,
        )
    } else {
        let budget_broker = MermaidBudgetLedger::new(&pressure_report);
        let layout_guardrails = LayoutGuardrails::from(&budget_broker);
        let traced = layout_diagram_traced_with_config_and_guardrails(
            &parsed.ir,
            fm_layout::LayoutAlgorithm::Auto,
            layout_config,
            layout_guardrails,
        );
        let degradation = compute_wasm_degradation_plan(&parsed.ir, &traced, &pressure_report);
        apply_budget_svg_simplifications(&mut svg_config, &budget_broker);
        apply_degradation_to_svg(&mut svg_config, &degradation);
        traced
    };

    Ok(PreparedSvgRender {
        parsed,
        svg_config,
        traced_layout,
    })
}

/// `renderDeck`'s structured result: the SVG, the deck manifest (or `null`), and every
/// deck-related diagnostic in the same serialized `Diagnostic` shape the `parse` export
/// emits — never bare strings, so hosts can underline a bad selector like any other finding.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmDeckOutput {
    pub svg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<fm_core::DeckManifest>,
    pub warnings: Vec<fm_core::Diagnostic>,
}

/// Native-callable core of the `renderDeck` export — the same pattern as [`render`], so the
/// full behavior is unit-testable without a browser.
fn render_deck(input: &str, config: Option<JsValue>) -> Result<WasmDeckOutput, JsValue> {
    let prepared = prepare_svg_render(input, config)?;
    let (svg, manifest) = fm_render_svg::render_svg_with_deck(
        &prepared.parsed.ir,
        &prepared.traced_layout.layout,
        &prepared.svg_config,
    );
    let mut warnings: Vec<fm_core::Diagnostic> = prepared
        .parsed
        .ir
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.message.starts_with("deck:"))
        .cloned()
        .collect();
    for error in &prepared.parsed.ir.meta.init.errors {
        let message = error.to_string();
        if message.contains("deck") {
            warnings.push(
                fm_core::Diagnostic::warning(message)
                    .with_category(fm_core::DiagnosticCategory::Parser),
            );
        }
    }
    if manifest.is_none() && prepared.parsed.ir.deck.is_none() {
        // renderDeck is a strict superset of renderSvg: deckless input still renders, with
        // one structured pointer at why the manifest is null.
        warnings.push(
            fm_core::Diagnostic::warning(
                "deck: no %%{deck: …}%% directive in the input; manifest is null",
            )
            .with_category(fm_core::DiagnosticCategory::Semantic),
        );
    }
    Ok(WasmDeckOutput {
        svg,
        manifest,
        warnings,
    })
}

/// Render a diagram AND its deck manifest from one parse + one layout (epic bd-z7g6k).
///
/// A strict superset of `renderSvg`: identical SVG bytes for identical input/config at
/// nominal pressure tier, plus `manifest` (or `null` with a structured warning when the
/// source has no deck, the family is unsupported, or no slide resolves).
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = renderDeck))]
pub fn render_deck_js(input: &str, config: Option<JsValue>) -> Result<JsValue, JsValue> {
    to_js_value(&render_deck(input, config)?)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(typescript_custom_section)]
const DECK_MANIFEST_TS: &'static str = r#"
/** A rectangle in SVG viewBox space (deck manifest schema 1.0.0). */
export interface DeckRect { x: number; y: number; width: number; height: number; }

export interface DeckManifestOptions {
  fitMargin: number; zoomMax: number; dimOpacity: number; autoAdvanceMs: number;
}

export interface DeckManifestNode {
  index: number; sourceId: string; elementId: string; step: number; tooltip?: string;
}

export interface DeckManifestEdge {
  index: number; elementId: string; step: number; touching: boolean;
}

export interface DeckManifestCluster {
  index: number; elementId: string; step: number; cameraContained: boolean;
}

export interface DeckManifestStep { step: number; elementIds: string[]; }

export interface DeckManifestSlide {
  id: string; title: string; caption?: string; bounds: DeckRect;
  fitMargin: number; zoomMax: number;
  nodes: DeckManifestNode[]; edges?: DeckManifestEdge[]; clusters?: DeckManifestCluster[];
  maxStep: number; steps?: DeckManifestStep[];
}

export interface DeckManifestOverview {
  enabled: boolean; title: string; caption?: string; tour: boolean;
}

/** The renderer-agnostic presentation contract; additive-only within 1.x. */
export interface DeckManifest {
  schemaVersion: string; generator: string; diagramType: string; title?: string;
  viewBox: DeckRect; options: DeckManifestOptions; slides: DeckManifestSlide[];
  overview: DeckManifestOverview; nodeSlideIndex?: Record<string, string[]>;
}

/** renderDeck() result: svg + manifest (null when no deck) + structured deck diagnostics. */
export interface WasmDeckOutput {
  svg: string; manifest?: DeckManifest; warnings: unknown[];
}
"#;

/// Prepare the WebGPU primitive plan for a diagram.
///
/// This is the WASM-side half of the WebGPU renderer: it runs the same parse and typography-aware
/// layout path as the SVG backend, then delegates primitive extraction to `fm-render-canvas`.
/// Device creation, buffer uploads and draw submission stay with the WebGPU device pass so this API
/// cannot grow a second renderer in JavaScript.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = planWebGpu))]
pub fn plan_webgpu_js(input: &str, config: Option<JsValue>) -> Result<JsValue, JsValue> {
    let planned = build_webgpu_plan(input, config).map_err(js_error)?;
    to_js_value(&WebGpuPlanSummary::from(&planned.plan))
}

/// One interactive node's clickable area, as a browser host receives it.
///
/// Mirrors `fm_render_canvas::HitRegion` rather than re-exporting it: the wasm boundary needs a
/// `Serialize` shape with camelCase keys, and the renderer type carries a `LayoutRect` that has no
/// business being part of this crate's published JSON contract.
///
/// `bounds` is `[x, y, width, height]` in LAYOUT coordinates — the same space `planWebGpu` reports
/// its bounds in, and the space the renderer draws in. The host applies the SAME viewport transform
/// it already uses for drawing. Baking one in here would assume a viewport this crate cannot see,
/// and the regions would drift from the picture the moment the user panned or zoomed.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmHitRegion {
    /// The author's node id — the same string the SVG backend puts in `data-id`.
    node_id: String,
    /// `[x, y, width, height]`, layout coordinates.
    bounds: [f32; 4],
    /// `click <id> href "url"`, when the author wrote one.
    href: Option<String>,
    /// `_self` / `_blank` / …, exactly as written. `null` means the author declared none, NOT that
    /// there is no target: a link defaults to `_blank`, and the host applies that default.
    link_target: Option<String>,
    /// `click <id> call fn()`.
    callback: Option<String>,
    /// The hover text.
    tooltip: Option<String>,
}

impl From<&fm_render_canvas::HitRegion> for WasmHitRegion {
    fn from(region: &fm_render_canvas::HitRegion) -> Self {
        Self {
            node_id: region.node_id.clone(),
            bounds: [
                region.bounds.x,
                region.bounds.y,
                region.bounds.width,
                region.bounds.height,
            ],
            href: region.href.clone(),
            link_target: region.link_target.clone(),
            callback: region.callback.clone(),
            tooltip: region.tooltip.clone(),
        }
    }
}

/// The clickable areas of a diagram, for a host driving a canvas or WebGPU surface.
///
/// SVG carries `click` in the document itself — a `title=` and, in link mode, an `<a href>` — so a
/// browser resolves a pointer against it with no help from us. A raster surface has no element to
/// hang an attribute on, so before this the whole `click` family was unreachable from every
/// non-SVG browser path: `renderWebGpuToRgba` returns pixels, and pixels cannot tell a host that
/// the box at (x, y) carries a URL.
///
/// The host owns the pointer. This returns WHERE each interactive node landed and WHAT the author
/// attached to it, once per render; hit-testing a point against those rectangles is a few lines of
/// JS and does not need to cross the wasm boundary on every `mousemove`.
///
/// Only nodes that actually carry an interaction are returned — a region per node would report the
/// whole diagram as clickable and push the filtering back onto the caller.
///
/// # Errors
/// Returns a JS error when the runtime config cannot be resolved.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = hitRegions))]
pub fn hit_regions_js(input: &str, config: Option<JsValue>) -> Result<JsValue, JsValue> {
    let geometry = build_diagram_geometry(input, config).map_err(js_error)?;
    let regions: Vec<WasmHitRegion> = fm_render_canvas::hit_regions(&geometry.ir, &geometry.layout)
        .iter()
        .map(WasmHitRegion::from)
        .collect();
    to_js_value(&regions)
}

/// Rasterise a diagram through the WebGPU device pass and return the RGBA pixels.
///
/// The half `planWebGpu` deliberately stopped short of. That call ends with primitive buffers and a
/// summary; this one acquires a device, builds every pipeline, draws all four families in ONE
/// submission, and reads the image back. It reuses `build_webgpu_plan`, so the parse, the
/// typography-aware layout and the primitive extraction are the same code the plan call runs —
/// there is no second pipeline here to drift from that one.
///
/// ASYNC, and it has to be: `requestAdapter`, `requestDevice` and `mapAsync` are all resolved by the
/// browser's event loop, so the blocking native helpers would hang a tab rather than fail. The
/// device layer shares one async implementation between this and the native path for exactly that
/// reason.
///
/// Returns raw `Rgba8Unorm` bytes, `width * height * 4`, which a caller hands to `ImageData` or
/// uploads as a texture. Returning pixels rather than drawing onto a passed-in canvas keeps this
/// callable from a worker, where there is no DOM canvas to draw onto.
///
/// # Errors
/// Returns the device or readback failure as a JS error.
#[cfg(all(target_arch = "wasm32", feature = "webgpu"))]
#[wasm_bindgen(js_name = renderWebGpuToRgba)]
pub async fn render_webgpu_to_rgba_js(
    input: String,
    width: u32,
    height: u32,
    config: Option<JsValue>,
) -> Result<js_sys::Uint8Array, JsValue> {
    let planned = build_webgpu_plan(&input, config).map_err(js_error)?;
    let pixels = render_webgpu_pixels(&planned.plan, width, height)
        .await
        .map_err(js_error)?;
    Ok(js_sys::Uint8Array::from(pixels.as_slice()))
}

/// Draw a planned diagram and return its pixels.
///
/// Split from the `wasm_bindgen` wrapper with a PLAIN STRING error for the same reason
/// `choose_canvas_target_result` was: `JsValue` is not constructible off wasm32, so a wrapper that
/// built one would abort the whole `--lib` test binary the moment a native test touched its error
/// path. This half is compiled and exercised on both targets.
///
/// # Errors
/// Returns the device or readback failure.
#[cfg(feature = "webgpu")]
// Unused in a NATIVE non-test build, and deliberately so: its only production caller is the
// wasm32-gated `renderWebGpuToRgba`, while the native test suite is what proves it works. Making it
// wasm-only would put the browser render path beyond the reach of every test that can run here,
// which is the trade this whole split exists to avoid.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn render_webgpu_pixels(
    plan: &GpuRenderPlan,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    use fm_render_canvas::gpu_device::{
        DiagramPipelines, GlyphAtlasTexture, GpuDevice, render_diagram_async,
    };

    let gpu = GpuDevice::request().await.map_err(|e| e.to_string())?;
    let pipelines = DiagramPipelines::new(&gpu);

    // The atlas is required exactly when there is text to draw. `solid_coverage` is the placeholder
    // the device tests use; a browser caller that wants readable labels supplies a rasterised atlas
    // through `glyph_raster`, which needs a font this crate deliberately does not ship.
    let atlas = (!plan.text_quads.is_empty()).then(|| {
        GlyphAtlasTexture::new(
            &gpu,
            &plan.glyph_atlas,
            &fm_render_canvas::gpu_device::solid_coverage(&plan.glyph_atlas),
        )
    });

    let image = render_diagram_async(&gpu, &pipelines, plan, atlas.as_ref(), width, height)
        .await
        .map_err(|e| e.to_string())?;
    Ok(image.rgba)
}

/// Acquire the browser's `GPUCanvasContext` for a canvas.
///
/// `web-sys` keeps the WebGPU DOM types behind unstable API flags, so this intentionally returns
/// the context as `JsValue`. The owner of the device passes receives the browser-native context
/// without a duplicate, unstable Rust binding layer.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = acquireWebGpuCanvasContext)]
pub fn acquire_webgpu_canvas_context(
    canvas: web_sys::HtmlCanvasElement,
) -> Result<JsValue, JsValue> {
    if !browser_supports_webgpu() {
        return Err(js_error("WebGPU is unavailable in this browser"));
    }
    canvas
        .get_context("webgpu")
        .map_err(|error| js_error_with_value("failed to get WebGPU canvas context", error))?
        .map(JsValue::from)
        .ok_or_else(|| js_error("canvas WebGPU context is unavailable"))
}

/// Choose a render target from probed host capabilities (bd-2u0.6 item 3).
///
/// JSON text in and out, exactly like [`worker_handle_message_js`], so the same call works from the
/// main thread, from inside the worker, and from a native test — and so the DECISION HAS ONE
/// IMPLEMENTATION. A host that re-derived the ladder in JavaScript would drift from this one, and
/// the drift would show up only in degraded environments, which are precisely the ones nobody
/// tests in.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = chooseCanvasTarget))]
pub fn choose_canvas_target_js(capabilities_json: &str) -> Result<String, JsValue> {
    choose_canvas_target_result(capabilities_json).map_err(js_error)
}

/// The decision and the rejection, with a PLAIN STRING error so the host can run it.
///
/// ⚠️ SPLIT OUT BECAUSE THE TEST COULD NOT RUN. `JsValue` is not constructible off wasm32 —
/// `JsValue::from_str` panics with "function not implemented on non-wasm32 targets" — so the
/// wrapper's ERROR path aborted the whole `fm-wasm --lib` binary the moment a test fed it junk.
/// An aborted binary emits no `test result:` line and no `FAILED` line either, so the workspace
/// summary read "0 failed" while a target was dying; only the exit status and a lone
/// `error: test failed` line said otherwise.
///
/// The wrapper above is now a single `map_err`, which is a stronger version of what the test was
/// reaching for: pinning it against a re-derived expectation is unnecessary when the JS boundary
/// has nothing left to re-derive.
pub(crate) fn choose_canvas_target_result(capabilities_json: &str) -> Result<String, String> {
    let capabilities: HostCapabilities =
        serde_json::from_str(capabilities_json).map_err(|error| {
            format!(
                "invalid host capabilities: {error}; expected {{\"offscreenCanvas\":bool,\"worker\":bool,\"canvasTransferred\":bool}}"
            )
        })?;

    serde_json::to_string(&choose_canvas_target(capabilities))
        .map_err(|error| format!("failed to encode canvas target: {error}"))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = detectType))]
pub fn detect_type_js(input: &str) -> Result<JsValue, JsValue> {
    let detected = detect_type_with_confidence(input);
    to_js_value(&detected)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = parse))]
pub fn parse_js(input: &str) -> Result<JsValue, JsValue> {
    let parsed = parse(input);
    to_js_value(&parsed)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = diagramLens))]
pub fn diagram_lens_js(input: &str) -> Result<JsValue, JsValue> {
    to_js_value(&build_parse_lens(input).bindings)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = applyLensEdit))]
pub fn apply_lens_edit_js(
    input: &str,
    element_id: &str,
    replacement: &str,
) -> Result<JsValue, JsValue> {
    let edit = fm_core::MermaidLensEdit {
        element_id: element_id.to_string(),
        replacement: replacement.to_string(),
    };
    let result = apply_parse_lens_edit(input, &edit).map_err(|e| js_error(e.to_string()))?;
    to_js_value(&result.result)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = parseLens))]
pub fn parse_lens_js(input: &str) -> Result<JsValue, JsValue> {
    to_js_value(&build_parse_lens(input))
}

/// One live render per worker, so a superseded typing update cannot publish over a newer one.
static WORKER_COORDINATOR: LazyLock<RwLock<WorkerRenderCoordinator>> =
    LazyLock::new(|| RwLock::new(WorkerRenderCoordinator::default()));

/// The worker entry point: hand it a [`WorkerRenderMessage`] as JSON, get a
/// [`WorkerRenderResponse`] as JSON, or `null` when the message needs no reply (a cancel, or an id
/// that is not the live request).
///
/// JSON text on both sides on purpose — a worker script can forward these straight through
/// `postMessage` with no `JsValue` dependency, which is what makes the same payload usable from the
/// main thread, a dedicated worker, and a native test.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = workerHandleMessage))]
pub fn worker_handle_message_js(message_json: &str) -> Result<Option<String>, JsValue> {
    let message: WorkerRenderMessage = serde_json::from_str(message_json).map_err(|error| {
        js_error(format!(
            "invalid worker message: {error}; expected {{\"kind\":\"render\",\"requestId\":N,\"input\":\"…\"}} or {{\"kind\":\"cancel\",\"requestId\":N}}"
        ))
    })?;

    // A poisoned lock must not wedge the worker for the rest of the session: the coordinator holds
    // only the live request id, so recovering the inner value keeps rendering possible.
    let mut coordinator = match WORKER_COORDINATOR.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let Some(response) = handle_worker_message(&mut coordinator, message) else {
        return Ok(None);
    };
    drop(coordinator);

    serde_json::to_string(&response)
        .map(Some)
        .map_err(|error| js_error(format!("failed to encode worker response: {error}")))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = applyParseLensEdit))]
pub fn apply_parse_lens_edit_js(
    input: &str,
    element_id: &str,
    replacement: &str,
) -> Result<JsValue, JsValue> {
    let edit = fm_core::MermaidLensEdit {
        element_id: element_id.to_string(),
        replacement: replacement.to_string(),
    };
    let result = apply_parse_lens_edit(input, &edit).map_err(|e| js_error(e.to_string()))?;
    to_js_value(&result)
}

/// Delete an element addressed by the lens, and return the post-delete snapshot with it.
///
/// The companion to `applyParseLensEdit` for the case a replacement cannot express: an empty
/// replacement leaves the element's indentation and line terminator behind, stranding a blank line
/// per removed node. The returned snapshot is re-derived from the shortened source, because every
/// element id and span after the deletion has moved.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = applyParseLensDelete))]
pub fn apply_parse_lens_delete_js(input: &str, element_id: &str) -> Result<JsValue, JsValue> {
    let result = apply_parse_lens_delete(input, element_id).map_err(|e| js_error(e.to_string()))?;
    to_js_value(&result)
}

/// Insert a line after the line holding an element, matching that line's indentation and the
/// document's line ending, and return the post-insert snapshot with it.
#[cfg_attr(
    target_arch = "wasm32",
    wasm_bindgen(js_name = applyParseLensInsertLineAfter)
)]
pub fn apply_parse_lens_insert_line_after_js(
    input: &str,
    element_id: &str,
    text: &str,
) -> Result<JsValue, JsValue> {
    let result = apply_parse_lens_insert_line_after(input, element_id, text)
        .map_err(|e| js_error(e.to_string()))?;
    to_js_value(&result)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = describeDiagram))]
pub fn describe_diagram_js(input: &str) -> Result<String, JsValue> {
    let parsed = parse(input);
    let traced = layout_diagram_traced(&parsed.ir);
    Ok(fm_render_svg::describe_diagram_with_layout(
        &parsed.ir,
        Some(&traced.layout),
    ))
}

#[cfg(any(not(target_arch = "wasm32"), test))]
pub fn source_spans_js(input: &str) -> Result<JsValue, JsValue> {
    let parsed = parse(input);
    to_js_value(&source_map_records(parsed.ir.source_map()))
}

#[cfg(any(not(target_arch = "wasm32"), test))]
pub fn capability_matrix_js() -> Result<JsValue, JsValue> {
    to_js_value(&capability_matrix())
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
struct WebCanvas2dContext {
    width: u32,
    height: u32,
    context: web_sys::CanvasRenderingContext2d,
    current_font: String,
}

#[cfg(target_arch = "wasm32")]
impl WebCanvas2dContext {
    fn new(width: u32, height: u32, context: web_sys::CanvasRenderingContext2d) -> Self {
        Self {
            width,
            height,
            context,
            current_font: "14px sans-serif".to_string(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Canvas2dContext for WebCanvas2dContext {
    fn width(&self) -> f64 {
        f64::from(self.width)
    }

    fn height(&self) -> f64 {
        f64::from(self.height)
    }

    fn save(&mut self) {
        self.context.save();
    }

    fn restore(&mut self) {
        self.context.restore();
    }

    fn set_fill_style(&mut self, color: &str) {
        self.context.set_fill_style_str(color);
    }

    fn set_stroke_style(&mut self, color: &str) {
        self.context.set_stroke_style_str(color);
    }

    fn set_line_width(&mut self, width: f64) {
        self.context.set_line_width(width);
    }

    fn set_line_cap(&mut self, cap: LineCap) {
        self.context.set_line_cap(cap.as_str());
    }

    fn set_line_join(&mut self, join: LineJoin) {
        self.context.set_line_join(join.as_str());
    }

    fn set_line_dash(&mut self, pattern: &[f64]) {
        let array = js_sys::Array::new();
        for value in pattern {
            array.push(&JsValue::from_f64(*value));
        }
        let _ = self.context.set_line_dash(&array);
    }

    fn set_global_alpha(&mut self, alpha: f64) {
        self.context.set_global_alpha(alpha);
    }

    fn set_font(&mut self, font: &str) {
        self.current_font = font.to_string();
        self.context.set_font(font);
    }

    fn set_text_align(&mut self, align: TextAlign) {
        self.context.set_text_align(align.as_str());
    }

    fn set_text_baseline(&mut self, baseline: TextBaseline) {
        self.context.set_text_baseline(baseline.as_str());
    }

    fn begin_path(&mut self) {
        self.context.begin_path();
    }

    fn close_path(&mut self) {
        self.context.close_path();
    }

    fn move_to(&mut self, x: f64, y: f64) {
        self.context.move_to(x, y);
    }

    fn line_to(&mut self, x: f64, y: f64) {
        self.context.line_to(x, y);
    }

    fn quadratic_curve_to(&mut self, cpx: f64, cpy: f64, x: f64, y: f64) {
        self.context.quadratic_curve_to(cpx, cpy, x, y);
    }

    fn bezier_curve_to(&mut self, cp1x: f64, cp1y: f64, cp2x: f64, cp2y: f64, x: f64, y: f64) {
        self.context.bezier_curve_to(cp1x, cp1y, cp2x, cp2y, x, y);
    }

    fn arc(&mut self, x: f64, y: f64, radius: f64, start_angle: f64, end_angle: f64) {
        let _ = self.context.arc(x, y, radius, start_angle, end_angle);
    }

    fn arc_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, radius: f64) {
        let _ = self.context.arc_to(x1, y1, x2, y2, radius);
    }

    fn rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.context.rect(x, y, width, height);
    }

    fn fill(&mut self) {
        self.context.fill();
    }

    fn stroke(&mut self) {
        self.context.stroke();
    }

    fn fill_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.context.fill_rect(x, y, width, height);
    }

    fn stroke_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.context.stroke_rect(x, y, width, height);
    }

    fn clear_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.context.clear_rect(x, y, width, height);
    }

    fn fill_text(&mut self, text: &str, x: f64, y: f64) {
        let _ = self.context.fill_text(text, x, y);
    }

    fn stroke_text(&mut self, text: &str, x: f64, y: f64) {
        let _ = self.context.stroke_text(text, x, y);
    }

    fn measure_text(&self, text: &str) -> TextMetrics {
        if let Ok(metrics) = self.context.measure_text(text) {
            TextMetrics {
                width: metrics.width(),
                height: canvas_font_size_px(&self.current_font),
            }
        } else {
            let font_size = canvas_font_size_px(&self.current_font);
            TextMetrics {
                width: text.chars().count() as f64 * (font_size * 0.57),
                height: font_size,
            }
        }
    }

    fn set_transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        let _ = self.context.set_transform(a, b, c, d, e, f);
    }

    fn reset_transform(&mut self) {
        let _ = self.context.reset_transform();
    }

    fn translate(&mut self, x: f64, y: f64) {
        let _ = self.context.translate(x, y);
    }

    fn scale(&mut self, x: f64, y: f64) {
        let _ = self.context.scale(x, y);
    }

    fn rotate(&mut self, angle: f64) {
        let _ = self.context.rotate(angle);
    }

    fn clip(&mut self) {
        self.context.clip();
    }

    fn set_shadow_blur(&mut self, blur: f64) {
        self.context.set_shadow_blur(blur);
    }

    fn set_shadow_color(&mut self, color: &str) {
        self.context.set_shadow_color(color);
    }

    fn set_shadow_offset(&mut self, x: f64, y: f64) {
        self.context.set_shadow_offset_x(x);
        self.context.set_shadow_offset_y(y);
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_supports_webgpu() -> bool {
    let global = js_sys::global();
    let Ok(navigator) = js_sys::Reflect::get(&global, &JsValue::from_str("navigator")) else {
        return false;
    };
    let Ok(gpu) = js_sys::Reflect::get(&navigator, &JsValue::from_str("gpu")) else {
        return false;
    };
    !(gpu.is_null() || gpu.is_undefined())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct Diagram {
    canvas: Option<web_sys::HtmlCanvasElement>,
    canvas_width: u32,
    canvas_height: u32,
    context: web_sys::CanvasRenderingContext2d,
    renderer: WebRendererKind,
    svg_config: SvgRenderConfig,
    canvas_config: CanvasRenderConfig,
    pressure_config: MermaidWasmPressureSignals,
    layout_engine: fm_layout::IncrementalLayoutEngine,
    last_layout: Option<Arc<fm_layout::DiagramLayout>>,
    destroyed: bool,
}

#[cfg(target_arch = "wasm32")]
impl Diagram {
    fn ensure_alive(&self) -> Result<(), JsValue> {
        if self.destroyed {
            return Err(js_error("diagram has been destroyed"));
        }
        Ok(())
    }

    fn from_canvas_context(
        canvas: Option<web_sys::HtmlCanvasElement>,
        canvas_width: u32,
        canvas_height: u32,
        context: web_sys::CanvasRenderingContext2d,
        config: Option<JsValue>,
    ) -> Result<Self, JsValue> {
        let overrides: RuntimeInitConfig = parse_js_value_or_default(config);
        let runtime = read_runtime_config();
        let requested_theme = requested_theme_preset(&overrides).map_err(js_error)?;
        let effective_svg = overrides.effective_svg_overrides();
        let theme_name = overrides
            .theme
            .as_deref()
            .or(effective_svg.theme.as_deref());
        let svg_config =
            merge_svg_config(&runtime.svg, &effective_svg, theme_name).map_err(js_error)?;
        let canvas_base = requested_theme
            .map(|preset| apply_canvas_theme_preset(runtime.canvas.clone(), preset))
            .unwrap_or_else(|| runtime.canvas.clone());
        let canvas_config = align_canvas_typography_with_svg(
            merge_canvas_config(&canvas_base, &overrides.canvas),
            &svg_config,
        );
        let pressure_config = merge_pressure_config(&runtime.pressure, &overrides.pressure);
        let renderer = merge_renderer_kind(runtime.renderer, overrides.renderer);

        Ok(Self {
            canvas,
            canvas_width,
            canvas_height,
            context,
            renderer,
            svg_config,
            canvas_config,
            pressure_config,
            layout_engine: fm_layout::IncrementalLayoutEngine::default(),
            last_layout: None,
            destroyed: false,
        })
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl Diagram {
    #[wasm_bindgen(constructor)]
    pub fn new(
        canvas: web_sys::HtmlCanvasElement,
        config: Option<JsValue>,
    ) -> Result<Self, JsValue> {
        let context_value = canvas
            .get_context("2d")
            .map_err(|err| js_error_with_value("failed to get 2d context", err))?;
        let context = context_value
            .ok_or_else(|| js_error("canvas 2d context is unavailable"))?
            .dyn_into::<web_sys::CanvasRenderingContext2d>()
            .map_err(|_| js_error("failed to cast context to CanvasRenderingContext2d"))?;
        let canvas_width = canvas.width();
        let canvas_height = canvas.height();
        Self::from_canvas_context(Some(canvas), canvas_width, canvas_height, context, config)
    }

    /// Creates a renderer for an `OffscreenCanvas` transferred to a worker.
    ///
    /// The offscreen 2D context implements the same CanvasRenderingContext2D
    /// method surface used by `Canvas2dContext`; it is stored structurally so
    /// the renderer can share the normal Canvas2D path without main-thread DOM
    /// access. Event registration remains unavailable because an offscreen
    /// canvas is not an `EventTarget`.
    #[wasm_bindgen(js_name = fromOffscreenCanvas)]
    pub fn from_offscreen_canvas(
        canvas: web_sys::OffscreenCanvas,
        config: Option<JsValue>,
    ) -> Result<Self, JsValue> {
        let canvas_width = canvas.width();
        let canvas_height = canvas.height();
        let context_value = canvas
            .get_context("2d")
            .map_err(|err| js_error_with_value("failed to get offscreen 2d context", err))?;
        let context = context_value
            .ok_or_else(|| js_error("offscreen canvas 2d context is unavailable"))?
            .unchecked_into::<web_sys::CanvasRenderingContext2d>();
        Self::from_canvas_context(None, canvas_width, canvas_height, context, config)
    }

    pub fn render(&mut self, input: &str, config: Option<JsValue>) -> Result<JsValue, JsValue> {
        self.ensure_alive()?;

        let overrides: RuntimeInitConfig = parse_js_value_or_default(config);
        let requested_theme = requested_theme_preset(&overrides).map_err(js_error)?;
        let effective_svg = overrides.effective_svg_overrides();
        let theme_name = overrides
            .theme
            .as_deref()
            .or(effective_svg.theme.as_deref());
        let next_svg =
            merge_svg_config(&self.svg_config, &effective_svg, theme_name).map_err(js_error)?;
        let next_pressure = merge_pressure_config(&self.pressure_config, &overrides.pressure);
        let pressure_report = next_pressure.into_report();
        let mut budget_broker = MermaidBudgetLedger::new(&pressure_report);
        let canvas_base = requested_theme.map_or_else(
            || self.canvas_config.clone(),
            |preset| apply_canvas_theme_preset(self.canvas_config.clone(), preset),
        );
        let next_canvas = align_canvas_typography_with_svg(
            merge_canvas_config(&canvas_base, &overrides.canvas),
            &next_svg,
        );
        let next_renderer = merge_renderer_kind(self.renderer, overrides.renderer);
        let parse_start = Instant::now();
        let parsed = parse(input);
        budget_broker.record_parse(
            parse_start
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
        );
        let layout_guardrails = LayoutGuardrails::from(&budget_broker);
        let layout_start = Instant::now();
        let layout_config = LayoutConfig {
            font_metrics: Some(next_svg.font_metrics()),
            edge_routing: if next_svg.theme == ThemePreset::Mermaid {
                fm_layout::EdgeRouting::Spline
            } else {
                fm_layout::EdgeRouting::Orthogonal
            },
            ..Default::default()
        };
        let traced_layout = self
            .layout_engine
            .layout_diagram_traced_with_config_and_guardrails(
                &parsed.ir,
                fm_layout::LayoutAlgorithm::Auto,
                layout_config.clone(),
                layout_guardrails,
            );
        budget_broker.record_layout(
            layout_start
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
        );
        let renderer = resolve_renderer(
            next_renderer,
            browser_supports_webgpu(),
            WEBGPU_RENDERER_IMPLEMENTED,
        );
        let guard = WasmGuardSummary::from_layout(&traced_layout, &pressure_report);
        let layout_decision_explanation = build_layout_decision_explanation(
            &parsed.ir,
            &traced_layout,
            pressure_report.clone(),
            budget_broker.total_budget_ms,
            budget_broker.exhausted,
        );
        let render_start = Instant::now();
        let canvas_result = match renderer.actual {
            WebRendererKind::Canvas2d => {
                let mut web_canvas = WebCanvas2dContext::new(
                    self.canvas_width,
                    self.canvas_height,
                    self.context.clone(),
                );
                render_to_canvas_with_layout(
                    &parsed.ir,
                    &traced_layout.layout,
                    &mut web_canvas,
                    &next_canvas,
                )
            }
            WebRendererKind::WebGpu => {
                return Err(js_error(
                    "internal error: WebGPU renderer was selected without an implementation",
                ));
            }
        };
        budget_broker.record_render(
            render_start
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
        );

        self.renderer = next_renderer;
        self.svg_config = next_svg;
        self.canvas_config = next_canvas;
        self.pressure_config = next_pressure;
        self.last_layout = Some(Arc::clone(&traced_layout.layout));

        let output = DiagramRenderOutput::new(
            &traced_layout,
            &layout_config,
            renderer.summary(),
            guard,
            layout_decision_explanation,
            &canvas_result,
        );
        to_js_value(&output)
    }

    /// Return the laid-out node below a canvas-space pointer, if any.
    ///
    /// The query uses CGA rectangle containment against the latest render's layout, so it never
    /// reparses or relayouts the diagram. Non-finite coordinates and calls before the first render
    /// return `None`.
    #[wasm_bindgen(js_name = hitTestNode)]
    pub fn hit_test_node(&self, x: f64, y: f64) -> Result<Option<String>, JsValue> {
        self.ensure_alive()?;
        Ok(self
            .last_layout
            .as_deref()
            .and_then(|layout| hit_test_layout_node(layout, x, y))
            .map(str::to_owned))
    }

    /// Return the nearest rendered edge index within a canvas-space tolerance.
    ///
    /// The query uses CGA point-to-segment distance over the latest render's edge paths, excludes
    /// bundled non-rendered paths, and returns `None` for invalid coordinates or tolerance.
    #[wasm_bindgen(js_name = hitTestEdge)]
    pub fn hit_test_edge(
        &self,
        x: f64,
        y: f64,
        max_distance: f64,
    ) -> Result<Option<usize>, JsValue> {
        self.ensure_alive()?;
        Ok(self
            .last_layout
            .as_deref()
            .and_then(|layout| hit_test_layout_edge(layout, x, y, max_distance)))
    }

    #[wasm_bindgen(js_name = setTheme)]
    pub fn set_theme(&mut self, theme: &str) -> Result<(), JsValue> {
        self.ensure_alive()?;
        let preset = theme.parse::<ThemePreset>().map_err(|err| {
            js_error(format!(
                "invalid theme '{theme}': {err}; expected one of default,mermaid,dark,forest,neutral,corporate,neon,pastel,high-contrast,monochrome,blueprint"
            ))
        })?;
        let overrides = SvgConfigOverrides {
            theme: Some(theme.to_string()),
            ..SvgConfigOverrides::default()
        };
        self.svg_config = merge_svg_config(&self.svg_config, &overrides, None).map_err(js_error)?;
        self.canvas_config = apply_canvas_theme_preset(self.canvas_config.clone(), preset);
        Ok(())
    }

    pub fn on(&self, event: &str, callback: &js_sys::Function) -> Result<(), JsValue> {
        self.ensure_alive()?;
        self.canvas
            .as_ref()
            .ok_or_else(|| js_error("offscreen canvas does not support DOM event listeners"))?
            .add_event_listener_with_callback(event, callback)
            .map_err(|err| js_error_with_value("failed to register canvas event listener", err))
    }

    pub fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.context.clear_rect(
            0.0,
            0.0,
            f64::from(self.canvas_width),
            f64::from(self.canvas_height),
        );
        self.last_layout = None;
        self.destroyed = true;
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default)]
pub struct Diagram;

#[cfg(not(target_arch = "wasm32"))]
impl Diagram {
    pub fn new(_canvas: JsValue, _config: Option<JsValue>) -> Result<Self, JsValue> {
        Err(js_error("Diagram is only available on wasm32 targets"))
    }

    pub fn render(&mut self, _input: &str, _config: Option<JsValue>) -> Result<JsValue, JsValue> {
        Err(js_error("Diagram is only available on wasm32 targets"))
    }

    pub fn hit_test_node(&self, _x: f64, _y: f64) -> Result<Option<String>, JsValue> {
        Err(js_error("Diagram is only available on wasm32 targets"))
    }

    pub fn hit_test_edge(
        &self,
        _x: f64,
        _y: f64,
        _max_distance: f64,
    ) -> Result<Option<usize>, JsValue> {
        Err(js_error("Diagram is only available on wasm32 targets"))
    }

    pub fn set_theme(&mut self, _theme: &str) -> Result<(), JsValue> {
        Err(js_error("Diagram is only available on wasm32 targets"))
    }

    pub fn on(&self, _event: &str, _callback: JsValue) -> Result<(), JsValue> {
        Err(js_error("Diagram is only available on wasm32 targets"))
    }

    pub fn destroy(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::{
        CanvasConfigOverrides, LayoutRuntimeSummary, PressureConfigOverrides, RuntimeConfig,
        RuntimeInitConfig, SvgConfigOverrides, ThemePreset, WasmHitRegion, WebGpuPlanSummary,
        WebRendererKind, WorkerRenderAction, WorkerRenderCoordinator, WorkerRenderMessage,
        WorkerRenderRequest, WorkerRenderResponse, align_canvas_typography_with_svg,
        apply_budget_svg_simplifications, apply_canvas_theme_preset, build_diagram_geometry,
        build_webgpu_plan, canvas_font_size_px, collect_source_spans, handle_worker_message,
        hit_test_layout_edge, hit_test_layout_node, merge_canvas_config, merge_pressure_config,
        merge_renderer_kind, merge_svg_config, read_runtime_config, render, render_deck,
        render_svg_js, render_worker_request, requested_theme_preset, resolve_renderer,
        write_runtime_config,
    };
    use fm_core::{
        MermaidBudgetLedger, MermaidGuardReport, MermaidLensBinding, MermaidLensEdit,
        MermaidLensEditResult, MermaidLensError, MermaidPressureTier, MermaidWasmPressureSignals,
    };
    use fm_layout::{
        IncrementalLayoutEngine, LayoutAlgorithm, LayoutConfig, LayoutGuardrails,
        layout_diagram_traced,
    };
    use fm_parser::{
        apply_parse_lens_delete, apply_parse_lens_edit, apply_parse_lens_insert_line_after,
        build_parse_lens, parse,
    };
    use fm_render_canvas::CanvasRenderConfig;
    use fm_render_svg::{SvgRenderConfig, describe_diagram_with_layout};
    use sha2::{Digest, Sha256};
    use std::{
        fmt::Write as _,
        fs,
        path::{Path, PathBuf},
    };

    #[cfg(feature = "webgpu")]
    use fm_render_canvas::gpu_device::{
        GpuDevice, InstanceDraw, InstancePass, node_instance_bytes, render_instances,
    };
    #[cfg(feature = "webgpu")]
    use fm_render_canvas::gpu_pipeline::node_pipeline;

    fn collect_source_files(directory: &Path, inputs: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()));
        for entry in entries {
            let path = entry
                .unwrap_or_else(|error| {
                    panic!(
                        "failed to read an entry in {}: {error}",
                        directory.display()
                    )
                })
                .path();
            if path.is_dir() {
                collect_source_files(&path, inputs);
            } else if path.is_file() {
                inputs.push(path);
            }
        }
    }

    fn source_input_sha256() -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("fm-wasm must live under <workspace>/crates/fm-wasm");
        let mut inputs: Vec<PathBuf> = [
            ".cargo/config.toml",
            "Cargo.lock",
            "Cargo.toml",
            "build-wasm.sh",
            "evidence/capability_matrix.json",
            "rust-toolchain.toml",
        ]
        .into_iter()
        .map(|relative| root.join(relative))
        .collect();

        for crate_name in [
            "fm-core",
            "fm-layout",
            "fm-parser",
            "fm-render-canvas",
            "fm-render-svg",
            "fm-wasm",
        ] {
            let crate_dir = root.join("crates").join(crate_name);
            inputs.push(crate_dir.join("Cargo.toml"));
            collect_source_files(&crate_dir.join("src"), &mut inputs);
        }
        inputs.sort();

        let mut hasher = Sha256::new();
        for path in inputs {
            let relative = path
                .strip_prefix(root)
                .expect("source input must be inside the workspace");
            let mut data = fs::read(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            if relative == Path::new("crates/fm-wasm/src/lib.rs") {
                const TEST_MODULE_MARKER: &[u8] = b"\n#[cfg(test)]\nmod tests {";
                let marker_position = data
                    .windows(TEST_MODULE_MARKER.len())
                    .position(|window| window == TEST_MODULE_MARKER)
                    .expect("fm-wasm test-module marker must remain present");
                data.truncate(marker_position);
            }

            let relative = relative.to_string_lossy().replace('\\', "/");
            hasher.update(relative.as_bytes());
            hasher.update([0]);
            hasher.update(
                u64::try_from(data.len())
                    .expect("source input length must fit in u64")
                    .to_le_bytes(),
            );
            hasher.update(&data);
        }

        let digest = hasher.finalize();
        let mut encoded = String::with_capacity(64);
        for byte in digest {
            write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
        }
        encoded
    }

    fn assert_package_source_matches_current_inputs() {
        let package_metadata: serde_json::Value =
            serde_json::from_str(include_str!("../../../pkg/package.json"))
                .expect("pkg/package.json must contain valid JSON");
        let packaged_source_sha256 = package_metadata
            .get("frankenmermaidSourceSha256")
            .and_then(serde_json::Value::as_str)
            .expect("pkg/package.json must record frankenmermaidSourceSha256");
        let current_source_sha256 = source_input_sha256();
        assert_eq!(
            packaged_source_sha256, current_source_sha256,
            "the committed WASM package was built from different source inputs; run the canonical \
             build-wasm.sh before updating any artifact or render digest"
        );
    }

    #[test]
    fn committed_wasm_package_records_current_source_inputs() {
        assert_package_source_matches_current_inputs();
    }

    #[allow(dead_code)]
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct WasmDiagramLens {
        bindings: Vec<MermaidLensBinding>,
    }

    #[allow(dead_code)]
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct WasmLensEditResponse {
        result: MermaidLensEditResult,
        bindings: Vec<MermaidLensBinding>,
    }

    #[allow(dead_code)]
    fn build_diagram_lens(input: &str) -> WasmDiagramLens {
        WasmDiagramLens {
            bindings: build_parse_lens(input).bindings,
        }
    }

    #[allow(dead_code)]
    fn build_lens_edit_response(
        input: &str,
        element_id: &str,
        replacement: &str,
    ) -> Result<WasmLensEditResponse, MermaidLensError> {
        let result = apply_parse_lens_edit(
            input,
            &MermaidLensEdit {
                element_id: element_id.to_string(),
                replacement: replacement.to_string(),
            },
        )?;
        Ok(WasmLensEditResponse {
            bindings: result.snapshot.bindings,
            result: result.result,
        })
    }

    #[allow(dead_code)]
    fn describe_diagram_js(input: &str) -> String {
        let parsed = parse(input);
        let traced = layout_diagram_traced(&parsed.ir);
        describe_diagram_with_layout(&parsed.ir, Some(&traced.layout))
    }

    #[test]
    fn render_returns_svg_and_type() {
        let output = render("flowchart LR\nA-->B");
        assert!(output.svg.starts_with("<svg"));
        assert_eq!(output.detected_type, "flowchart");
        assert!(output.accessibility_summary.contains("Key relationships"));
        assert!(!output.trace_id.is_empty());
        assert!(!output.decision_id.is_empty());
        assert_eq!(output.policy_id, "fm.layout.guard@v1");
        assert_eq!(output.schema_version, "1.0.0");
        assert!(output.guard.budget_broker.total_budget_ms > 0);
        assert_eq!(
            output.guard.layout_selected_algorithm.as_deref(),
            Some("sugiyama")
        );
        assert_eq!(output.guard.guard_reason.as_deref(), Some("within_budget"));
        assert_eq!(output.guard.pressure.tier, MermaidPressureTier::Unknown);
        assert_eq!(
            output
                .layout_decision_explanation
                .level_0_traffic_light
                .status,
            fm_core::MermaidDecisionTrafficLight::Green
        );
        assert!(
            output
                .layout_decision_explanation
                .level_1_plain_english
                .summary
                .contains("sugiyama")
        );
        assert_eq!(output.layout.cycle_strategy, "greedy");
        assert_eq!(output.layout.node_count, 2);
        assert_eq!(output.layout.edge_count, 1);
        assert!(output.source_spans.iter().any(|span| span.kind == "node"));
        assert!(output.source_spans.iter().any(|span| span.kind == "edge"));
    }

    #[test]
    fn cga_hit_testing_uses_rendered_node_bounds_and_rejects_invalid_coordinates() {
        let parsed = parse("flowchart LR\nA-->B");
        let traced = layout_diagram_traced(&parsed.ir);
        let hit = traced
            .layout
            .nodes
            .iter()
            .find(|node| node.node_id == "A")
            .and_then(|node| {
                let center = node.bounds.center();
                hit_test_layout_node(&traced.layout, f64::from(center.x), f64::from(center.y))
            });

        assert_eq!(hit, Some("A"));
        assert_eq!(hit_test_layout_node(&traced.layout, f64::NAN, 0.0), None);
        assert_eq!(
            hit_test_layout_node(&traced.layout, -10_000.0, -10_000.0),
            None
        );
    }

    #[test]
    fn cga_edge_hit_testing_selects_rendered_segments_and_rejects_invalid_tolerance() {
        let parsed = parse("flowchart LR\nA-->B");
        let traced = layout_diagram_traced(&parsed.ir);
        let expected_hit = traced.layout.edges.iter().find_map(|edge| {
            (!edge.bundled)
                .then(|| edge.points.first().zip(edge.points.get(1)))
                .flatten()
                .map(|(start, end)| {
                    (
                        edge.edge_index,
                        (
                            (f64::from(start.x) + f64::from(end.x)) / 2.0,
                            (f64::from(start.y) + f64::from(end.y)) / 2.0,
                        ),
                    )
                })
        });

        assert!(expected_hit.is_some());
        assert_eq!(
            expected_hit.and_then(|(edge_index, (x, y))| {
                hit_test_layout_edge(&traced.layout, x, y, 0.01).map(|hit| (edge_index, hit))
            }),
            expected_hit.map(|(edge_index, _)| (edge_index, edge_index))
        );
        assert_eq!(
            hit_test_layout_edge(&traced.layout, f64::NAN, 0.0, 1.0),
            None
        );
        assert_eq!(hit_test_layout_edge(&traced.layout, 0.0, 0.0, -1.0), None);
    }

    #[test]
    fn cga_edge_hit_testing_breaks_equal_distance_ties_by_edge_index() -> Result<(), &'static str> {
        let parsed = parse("flowchart LR\nA-->B");
        let traced = layout_diagram_traced(&parsed.ir);
        // `TracedLayout::layout` is an `Arc<DiagramLayout>`, so take an owned copy to mutate.
        let mut layout = (*traced.layout).clone();
        let first_edge = layout
            .edges
            .first()
            .cloned()
            .ok_or("single-edge flowchart must produce a rendered path")?;
        // Read the segment before `first_edge` is moved into `duplicate`.
        let midpoint = {
            let (start, end) = first_edge
                .points
                .first()
                .zip(first_edge.points.get(1))
                .ok_or("rendered path must contain a segment")?;
            (
                (f64::from(start.x) + f64::from(end.x)) / 2.0,
                (f64::from(start.y) + f64::from(end.y)) / 2.0,
            )
        };
        let expected_edge_index = first_edge.edge_index;

        let mut duplicate = first_edge;
        duplicate.edge_index = expected_edge_index + 1;
        layout.edges.push(duplicate);
        layout.edges.reverse();

        assert_eq!(
            hit_test_layout_edge(&layout, midpoint.0, midpoint.1, 0.01),
            Some(expected_edge_index)
        );
        Ok(())
    }

    #[test]
    #[ignore = "manual short release A/B"]
    fn wasm_dead_budget_ledger_clone_perf_ab() {
        use std::hint::black_box;
        use std::time::Instant;

        const ITERATIONS: usize = 16_384;
        const ROUNDS: usize = 9;

        #[inline(never)]
        fn finish_render_guard(
            mut guard: MermaidGuardReport,
            mut budget_broker: MermaidBudgetLedger,
        ) -> MermaidGuardReport {
            black_box((&guard.degradation, budget_broker.should_simplify_render()));
            budget_broker.record_render(7);
            guard.budget_broker = budget_broker;
            guard
        }

        #[inline(never)]
        fn baseline(
            mut guard: MermaidGuardReport,
            budget_broker: MermaidBudgetLedger,
        ) -> MermaidGuardReport {
            guard.budget_broker = budget_broker.clone();
            finish_render_guard(guard, budget_broker)
        }

        #[inline(never)]
        fn candidate(
            guard: MermaidGuardReport,
            budget_broker: MermaidBudgetLedger,
        ) -> MermaidGuardReport {
            finish_render_guard(guard, budget_broker)
        }

        fn measure(
            guard: &MermaidGuardReport,
            budget_broker: &MermaidBudgetLedger,
            run: fn(MermaidGuardReport, MermaidBudgetLedger) -> MermaidGuardReport,
        ) -> (u128, u64) {
            let mut checksum = 0_u64;
            let started = Instant::now();
            for iteration in 0..ITERATIONS {
                let final_guard = run(guard.clone(), budget_broker.clone());
                checksum = checksum
                    .wrapping_mul(0x100_0000_01b3)
                    .wrapping_add(final_guard.budget_broker.events.len() as u64)
                    .wrapping_add(final_guard.budget_broker.remaining_total_ms)
                    .wrapping_add(iteration as u64);
                black_box(final_guard);
            }
            (started.elapsed().as_nanos(), checksum)
        }

        let pressure = MermaidWasmPressureSignals::default().into_report();
        let mut budget_broker = MermaidBudgetLedger::new(&pressure);
        budget_broker.record_parse(5);
        budget_broker.record_layout(30);
        let mut guard = MermaidGuardReport {
            pressure,
            ..MermaidGuardReport::default()
        };
        guard.degradation.hide_labels = true;

        let baseline_output = baseline(guard.clone(), budget_broker.clone());
        let candidate_output = candidate(guard.clone(), budget_broker.clone());
        assert_eq!(candidate_output, baseline_output);
        assert_eq!(candidate_output.budget_broker.events.len(), 12);

        let mut baseline_ns = Vec::with_capacity(ROUNDS);
        let mut candidate_ns = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            let (baseline_result, candidate_result) = if round % 2 == 0 {
                (
                    measure(&guard, &budget_broker, baseline),
                    measure(&guard, &budget_broker, candidate),
                )
            } else {
                let candidate_result = measure(&guard, &budget_broker, candidate);
                let baseline_result = measure(&guard, &budget_broker, baseline);
                (baseline_result, candidate_result)
            };
            assert_eq!(baseline_result.1, candidate_result.1);
            baseline_ns.push(baseline_result.0);
            candidate_ns.push(candidate_result.0);
        }

        baseline_ns.sort_unstable();
        candidate_ns.sort_unstable();
        let baseline_median_ns = baseline_ns[ROUNDS / 2];
        let candidate_median_ns = candidate_ns[ROUNDS / 2];
        let improvement_pct = (baseline_median_ns as f64 - candidate_median_ns as f64) * 100.0
            / baseline_median_ns as f64;

        println!(
            "PERF wasm_dead_budget_ledger_clone baseline_median_ns={baseline_median_ns} candidate_median_ns={candidate_median_ns} improvement_pct={improvement_pct:.3} parity=exact rounds={ROUNDS} iterations={ITERATIONS} checksum={}",
            baseline_output.budget_broker.events.len()
        );
    }

    #[test]
    fn layout_runtime_summary_marks_memoized_reuse_as_incremental() {
        let parsed = parse("flowchart LR\nA-->B");
        let mut engine = IncrementalLayoutEngine::default();
        let config = LayoutConfig::default();
        let guardrails = LayoutGuardrails::default();

        let _warm = engine.layout_diagram_traced_with_config_and_guardrails(
            &parsed.ir,
            LayoutAlgorithm::Auto,
            config.clone(),
            guardrails,
        );
        let traced = engine.layout_diagram_traced_with_config_and_guardrails(
            &parsed.ir,
            LayoutAlgorithm::Auto,
            config.clone(),
            guardrails,
        );

        assert!(traced.trace.incremental.cache_hit);
        assert_eq!(traced.trace.incremental.query_type, "layout_memoized_reuse");

        let summary = LayoutRuntimeSummary::new(&traced, &config);
        assert!(
            summary.incremental,
            "memoized reuse should count as incremental fast path"
        );
    }

    #[test]
    fn build_parse_lens_reports_format_complement_and_bindings() {
        let lens = build_parse_lens("%% comment\nflowchart LR\nA[Alpha] --> B[Beta]\n");

        assert_eq!(lens.parsed.format_complement.comments.len(), 1);
        assert!(!lens.bindings.is_empty());
    }

    #[test]
    fn collect_source_spans_reports_node_edge_and_cluster_records() {
        let parsed = parse("flowchart TD\nsubgraph Cluster\nA-->B\nend\n");
        let traced = layout_diagram_traced(&parsed.ir);
        let spans = collect_source_spans(&parsed.ir, &traced.layout);
        assert!(spans.iter().any(|span| span.kind == "node"));
        assert!(spans.iter().any(|span| span.kind == "edge"));
        assert!(spans.iter().any(|span| span.kind == "cluster"));
    }

    #[test]
    fn merge_svg_config_applies_theme_override() {
        let base = SvgRenderConfig::default();
        let overrides = SvgConfigOverrides {
            theme: Some("dark".to_string()),
            ..SvgConfigOverrides::default()
        };
        let merged = merge_svg_config(&base, &overrides, None).expect("theme should parse");
        assert_eq!(merged.theme, ThemePreset::Dark);
    }

    #[test]
    fn merge_svg_config_rejects_invalid_numeric_overrides_by_ignoring_them() {
        let base = SvgRenderConfig::default();
        let overrides = SvgConfigOverrides {
            font_size: Some(f32::NAN),
            padding: Some(-1.0),
            rounded_corners: Some(f32::INFINITY),
            ..SvgConfigOverrides::default()
        };

        let merged = merge_svg_config(&base, &overrides, None).expect("merge should succeed");

        assert_eq!(merged.font_size, base.font_size);
        assert_eq!(merged.padding, base.padding);
        assert_eq!(merged.rounded_corners, base.rounded_corners);
    }

    #[test]
    fn apply_canvas_theme_preset_updates_canvas_colors() {
        let base = CanvasRenderConfig::default();
        let themed = apply_canvas_theme_preset(base, ThemePreset::Dark);

        assert_eq!(themed.node_fill, "#1e293b");
        assert_eq!(themed.node_stroke, "#334155");
        assert_eq!(themed.edge_stroke, "#94a3b8");
        assert_eq!(themed.label_color, "#f8fafc");
    }

    #[test]
    fn requested_theme_preset_prefers_svg_theme_override() {
        let overrides = RuntimeInitConfig {
            theme: Some("forest".to_string()),
            svg: SvgConfigOverrides {
                theme: Some("dark".to_string()),
                ..SvgConfigOverrides::default()
            },
            ..RuntimeInitConfig::default()
        };

        let preset = requested_theme_preset(&overrides).expect("theme should parse");
        assert_eq!(preset, Some(ThemePreset::Dark));
    }

    #[test]
    fn runtime_init_accepts_top_level_svg_options_and_nested_options_win() {
        let overrides: RuntimeInitConfig = serde_json::from_str(
            r#"{
                "theme": "dark",
                "responsive": false,
                "fontSize": 18.0,
                "shadows": true,
                "svg": {
                    "theme": "forest",
                    "fontSize": 21.0,
                    "shadows": false
                }
            }"#,
        )
        .expect("runtime options should deserialize from the JavaScript wire shape");

        let effective = overrides.effective_svg_overrides();
        assert_eq!(effective.theme.as_deref(), Some("forest"));
        assert_eq!(effective.responsive, Some(false));
        assert_eq!(effective.font_size, Some(21.0));
        assert_eq!(effective.shadows, Some(false));
    }

    #[test]
    fn merge_renderer_kind_prefers_explicit_override() {
        assert_eq!(
            merge_renderer_kind(WebRendererKind::Canvas2d, Some(WebRendererKind::WebGpu)),
            WebRendererKind::WebGpu
        );
        assert_eq!(
            merge_renderer_kind(WebRendererKind::WebGpu, None),
            WebRendererKind::WebGpu
        );
    }

    #[test]
    fn worker_protocol_supersedes_stale_typing_render_and_rejects_its_completion() {
        let mut coordinator = WorkerRenderCoordinator::default();
        let first = WorkerRenderRequest {
            request_id: 41,
            input: "flowchart LR\nA-->B".to_string(),
            config_json: None,
        };
        let second = WorkerRenderRequest {
            request_id: 42,
            input: "flowchart LR\nA-->C".to_string(),
            config_json: Some("{\"theme\":\"dark\"}".to_string()),
        };

        assert_eq!(
            coordinator.handle(WorkerRenderMessage::Render(first.clone())),
            WorkerRenderAction::Start(first),
        );
        assert_eq!(
            coordinator.handle(WorkerRenderMessage::Render(second.clone())),
            WorkerRenderAction::Supersede {
                cancelled_request_id: 41,
                next: second,
            },
        );
        assert!(!coordinator.complete(41));
        assert!(coordinator.complete(42));
    }

    #[test]
    fn worker_protocol_cancels_only_the_active_request() {
        let mut coordinator = WorkerRenderCoordinator::default();
        let request = WorkerRenderRequest {
            request_id: 7,
            input: "flowchart LR\nA-->B".to_string(),
            config_json: None,
        };
        let _ = coordinator.handle(WorkerRenderMessage::Render(request));

        assert_eq!(
            coordinator.handle(WorkerRenderMessage::Cancel { request_id: 6 }),
            WorkerRenderAction::Ignored { request_id: 6 },
        );
        assert_eq!(
            coordinator.handle(WorkerRenderMessage::Cancel { request_id: 7 }),
            WorkerRenderAction::Cancelled { request_id: 7 },
        );
        assert!(!coordinator.complete(7));
    }

    /// The element id of the first lens binding whose span covers `needle`.
    fn binding_covering(input: &str, needle: &str) -> String {
        let snapshot = build_parse_lens(input);
        snapshot
            .source_map
            .entries
            .iter()
            .find(|entry| {
                fm_core::resolve_span_text_range(input, entry.span)
                    .and_then(|range| input.get(range.start_byte..range.end_byte))
                    .is_some_and(|text| text.contains(needle))
            })
            .map(|entry| entry.element_id.clone())
            .unwrap_or_else(|| panic!("no binding covers {needle:?} in {input:?}"))
    }

    #[test]
    fn lens_delete_removes_the_line_and_re_snapshots_the_shortened_source() {
        // Exercises exactly what `applyParseLensDelete` calls. The re-snapshot is the part a caller
        // cannot skip: every id and span after the deletion has moved.
        let input = "flowchart LR\n  A-->B\n  C-->D\n";
        let element_id = binding_covering(input, "C-->D");

        let response = apply_parse_lens_delete(input, &element_id).expect("delete applies");

        assert_eq!(response.result.updated_source, "flowchart LR\n  A-->B\n");
        assert_eq!(response.result.replacement, "");
        assert_eq!(
            response.snapshot.original_source(),
            response.result.updated_source,
            "the returned snapshot must describe the source AFTER the delete"
        );
        // Restoring the reported snippet at the reported offset must rebuild the original exactly.
        let mut restored = response.result.updated_source.clone();
        restored.insert_str(
            response.result.replaced_range.start_byte,
            &response.result.previous_snippet,
        );
        assert_eq!(restored, input);
    }

    #[test]
    fn lens_insert_places_a_line_after_the_target_with_its_indentation() {
        let input = "flowchart LR\n    A-->B\n    C-->D\n";
        let element_id = binding_covering(input, "A-->B");

        let response = apply_parse_lens_insert_line_after(input, &element_id, "E-->F")
            .expect("insert applies");

        assert_eq!(
            response.result.updated_source,
            "flowchart LR\n    A-->B\n    E-->F\n    C-->D\n"
        );
        assert!(response.result.previous_snippet.is_empty());
        assert_eq!(
            response.snapshot.original_source(),
            response.result.updated_source
        );
        // The inserted line must be real syntax the parser sees, not just text.
        assert!(
            response
                .snapshot
                .parsed
                .ir
                .nodes
                .iter()
                .any(|node| node.id == "E"),
            "the inserted edge must parse into nodes: {:?}",
            response
                .snapshot
                .parsed
                .ir
                .nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn lens_delete_and_insert_refuse_an_unknown_element_by_name() {
        let input = "flowchart LR\n  A-->B\n";
        let delete_error = apply_parse_lens_delete(input, "fm-node-nope-9")
            .expect_err("an unknown id must not resolve");
        assert!(delete_error.to_string().contains("fm-node-nope-9"));

        let insert_error = apply_parse_lens_insert_line_after(input, "fm-node-nope-9", "C-->D")
            .expect_err("an unknown id must not resolve");
        assert!(insert_error.to_string().contains("fm-node-nope-9"));
    }

    #[test]
    fn worker_render_reports_timings_diagnostics_and_counts() {
        let response = render_worker_request(&WorkerRenderRequest {
            request_id: 9,
            input: "flowchart LR\n  A-->B\n  B-->C".to_string(),
            config_json: None,
        });

        let WorkerRenderResponse::Completed {
            request_id,
            svg,
            detected_type,
            accessibility_summary,
            node_count,
            edge_count,
            svg_bytes,
            timings,
            diagnostics,
        } = response
        else {
            panic!("a valid flowchart must render: {response:?}");
        };

        assert_eq!(request_id, 9);
        assert_eq!(detected_type, "flowchart");
        assert_eq!(node_count, 3);
        assert_eq!(edge_count, 2);
        assert!(svg.contains("</svg>"));
        assert_eq!(svg_bytes, svg.len());
        assert!(!accessibility_summary.is_empty());
        // Timings are wall clock and may each round to zero on a fast host; what must hold is that
        // the total is the sum, so a UI cannot be handed inconsistent attribution.
        assert_eq!(
            timings.total_ms,
            timings.parse_ms + timings.layout_ms + timings.render_ms
        );
        assert!(
            diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
            "a valid flowchart must not report errors: {diagnostics:?}"
        );
    }

    #[test]
    fn worker_render_applies_per_request_config_json() {
        // The regression this pins: `configJson` was carried across the protocol and never read, so
        // a themed worker render came back with default styling.
        let request = |config: Option<&str>| WorkerRenderRequest {
            request_id: 1,
            input: "flowchart LR\n  A-->B".to_string(),
            config_json: config.map(str::to_string),
        };

        let default_svg = match render_worker_request(&request(None)) {
            WorkerRenderResponse::Completed { svg, .. } => svg,
            other => panic!("default render failed: {other:?}"),
        };
        let dark_svg = match render_worker_request(&request(Some("{\"theme\":\"dark\"}"))) {
            WorkerRenderResponse::Completed { svg, .. } => svg,
            other => panic!("themed render failed: {other:?}"),
        };

        assert_ne!(
            default_svg, dark_svg,
            "the dark theme must change the output, else configJson is still being dropped"
        );
    }

    #[test]
    fn worker_render_rejects_malformed_config_with_an_actionable_error() {
        let response = render_worker_request(&WorkerRenderRequest {
            request_id: 3,
            input: "flowchart LR\n  A-->B".to_string(),
            config_json: Some("{\"theme\":".to_string()),
        });

        let WorkerRenderResponse::Failed {
            request_id, error, ..
        } = response
        else {
            panic!("malformed configJson must fail: {response:?}");
        };
        assert_eq!(request_id, 3);
        assert!(
            error.contains("configJson"),
            "the error must name the offending field: {error}"
        );
    }

    #[test]
    fn worker_render_rejects_an_unknown_theme_by_name() {
        let response = render_worker_request(&WorkerRenderRequest {
            request_id: 4,
            input: "flowchart LR\n  A-->B".to_string(),
            config_json: Some("{\"theme\":\"chartreuse\"}".to_string()),
        });

        let WorkerRenderResponse::Failed { error, .. } = response else {
            panic!("an unknown theme must fail: {response:?}");
        };
        assert!(
            error.contains("chartreuse") && error.contains("expected one of"),
            "the error must name the bad theme and the valid set: {error}"
        );
    }

    #[test]
    fn worker_publish_substitutes_superseded_for_stale_output() {
        let mut coordinator = WorkerRenderCoordinator::default();
        let stale = WorkerRenderRequest {
            request_id: 10,
            input: "flowchart LR\n  A-->B".to_string(),
            config_json: None,
        };
        let fresh = WorkerRenderRequest {
            request_id: 11,
            input: "flowchart LR\n  A-->C".to_string(),
            config_json: None,
        };
        let _ = coordinator.handle(WorkerRenderMessage::Render(stale.clone()));
        let _ = coordinator.handle(WorkerRenderMessage::Render(fresh.clone()));

        // The stale render finishes late. Publishing must NOT hand back its SVG.
        let stale_published = coordinator.publish(render_worker_request(&stale));
        assert_eq!(
            stale_published,
            WorkerRenderResponse::Superseded { request_id: 10 }
        );

        let fresh_published = coordinator.publish(render_worker_request(&fresh));
        assert!(
            matches!(
                fresh_published,
                WorkerRenderResponse::Completed { request_id: 11, .. }
            ),
            "the live request must publish normally: {fresh_published:?}"
        );
    }

    #[test]
    fn worker_message_loop_renders_a_render_and_answers_nothing_for_a_cancel() {
        let mut coordinator = WorkerRenderCoordinator::default();
        let request = WorkerRenderRequest {
            request_id: 20,
            input: "flowchart LR\n  A-->B".to_string(),
            config_json: None,
        };

        let response = handle_worker_message(
            &mut coordinator,
            WorkerRenderMessage::Render(request.clone()),
        );
        assert!(
            matches!(
                response,
                Some(WorkerRenderResponse::Completed { request_id: 20, .. })
            ),
            "a render message must produce a completed response: {response:?}"
        );

        // The render already completed, so this cancel refers to nothing live and needs no reply.
        assert_eq!(
            handle_worker_message(
                &mut coordinator,
                WorkerRenderMessage::Cancel { request_id: 20 }
            ),
            None
        );
    }

    #[test]
    fn worker_response_round_trips_through_json() {
        // The response crosses `postMessage` as text, so a field that does not survive serde is a
        // field the UI never sees.
        let original = render_worker_request(&WorkerRenderRequest {
            request_id: 77,
            input: "flowchart LR\n  A-->B".to_string(),
            config_json: None,
        });
        let json = serde_json::to_string(&original).expect("serialize response");
        let restored: WorkerRenderResponse =
            serde_json::from_str(&json).expect("deserialize response");
        assert_eq!(original, restored);
        assert!(
            json.contains("\"timings\"") && json.contains("\"totalMs\""),
            "timings must be on the wire in camelCase: {json}"
        );
    }

    #[test]
    fn resolve_renderer_keeps_canvas2d_when_requested() {
        let resolved = resolve_renderer(WebRendererKind::Canvas2d, false, false);
        assert_eq!(resolved.requested, WebRendererKind::Canvas2d);
        assert_eq!(resolved.actual, WebRendererKind::Canvas2d);
        assert_eq!(resolved.fallback_reason, None);
    }

    #[test]
    fn resolve_renderer_falls_back_when_webgpu_is_unavailable() {
        let resolved = resolve_renderer(WebRendererKind::WebGpu, false, true);
        assert_eq!(resolved.actual, WebRendererKind::Canvas2d);
        assert_eq!(resolved.fallback_reason, Some("webgpu_unavailable"));
    }

    #[test]
    fn resolve_renderer_falls_back_when_webgpu_is_not_implemented() {
        let resolved = resolve_renderer(WebRendererKind::WebGpu, true, false);
        assert_eq!(resolved.actual, WebRendererKind::Canvas2d);
        assert_eq!(resolved.fallback_reason, Some("webgpu_not_implemented"));
    }

    #[test]
    fn theme_override_rethemes_canvas_before_explicit_canvas_overrides() {
        let base_canvas = CanvasRenderConfig::default();
        let overrides = RuntimeInitConfig {
            theme: Some("dark".to_string()),
            canvas: CanvasConfigOverrides {
                font_size: Some(21.0),
                ..CanvasConfigOverrides::default()
            },
            ..RuntimeInitConfig::default()
        };

        let preset = requested_theme_preset(&overrides).expect("theme should parse");
        let themed_base = preset
            .map(|value| apply_canvas_theme_preset(base_canvas, value))
            .expect("theme override should be present");
        let merged = merge_canvas_config(&themed_base, &overrides.canvas);

        assert_eq!(merged.node_fill, "#1e293b");
        assert_eq!(merged.label_color, "#f8fafc");
        assert_eq!(merged.font_size, 21.0);
    }

    #[test]
    fn canvas_font_size_px_parses_css_font_prefix() {
        assert_eq!(canvas_font_size_px("18px Inter, sans-serif"), 18.0);
        assert_eq!(canvas_font_size_px("12.5px serif"), 12.5);
        assert_eq!(canvas_font_size_px("bad-font-value"), 14.0);
    }

    #[test]
    fn merge_pressure_config_applies_runtime_overrides() {
        let base = MermaidWasmPressureSignals {
            frame_budget_ms: Some(16),
            ..MermaidWasmPressureSignals::default()
        };
        let overrides = PressureConfigOverrides {
            frame_time_ms: Some(24),
            worker_saturation_permille: Some(910),
            ..PressureConfigOverrides::default()
        };
        let merged = merge_pressure_config(&base, &overrides);
        assert_eq!(merged.frame_budget_ms, Some(16));
        assert_eq!(merged.frame_time_ms, Some(24));
        assert_eq!(merged.worker_saturation_permille, Some(910));
        let report = merged.into_report();
        assert_eq!(report.tier, MermaidPressureTier::Critical);
    }

    #[test]
    fn runtime_default_keeps_canvas_typography_aligned_with_svg_layout() {
        let runtime = RuntimeConfig::default();
        assert_eq!(runtime.canvas.font_family, runtime.svg.font_family);
        assert_eq!(runtime.canvas.font_size, f64::from(runtime.svg.font_size));
    }

    #[test]
    fn align_canvas_typography_with_svg_preserves_non_typography_canvas_settings() {
        let canvas = CanvasRenderConfig {
            padding: 12.0,
            node_fill: "#123456".to_string(),
            edge_stroke_width: 3.0,
            ..CanvasRenderConfig::default()
        };
        let svg = SvgRenderConfig {
            font_family: "Test Font".to_string(),
            font_size: 22.0,
            ..SvgRenderConfig::default()
        };

        let aligned = align_canvas_typography_with_svg(canvas, &svg);

        assert_eq!(aligned.font_family, "Test Font");
        assert_eq!(aligned.font_size, 22.0);
        assert_eq!(aligned.padding, 12.0);
        assert_eq!(aligned.node_fill, "#123456");
        assert_eq!(aligned.edge_stroke_width, 3.0);
    }

    #[test]
    fn budget_simplification_respects_explicit_shadow_disable() {
        let mut config = SvgRenderConfig {
            shadows: false,
            ..SvgRenderConfig::default()
        };
        let budget_broker =
            fm_core::MermaidBudgetLedger::new(&MermaidWasmPressureSignals::default().into_report());
        apply_budget_svg_simplifications(&mut config, &budget_broker);
        assert!(!config.shadows);
    }

    #[test]
    fn budget_simplification_disables_shadows_under_pressure() {
        let mut config = SvgRenderConfig {
            shadows: true,
            ..SvgRenderConfig::default()
        };
        let pressure = MermaidWasmPressureSignals {
            frame_budget_ms: Some(16),
            frame_time_ms: Some(24),
            worker_saturation_permille: Some(900),
            ..MermaidWasmPressureSignals::default()
        };
        let budget_broker = fm_core::MermaidBudgetLedger::new(&pressure.into_report());
        apply_budget_svg_simplifications(&mut config, &budget_broker);
        assert!(!config.shadows);
    }

    /// Serialises every test that reads or writes `RUNTIME_CONFIG` (bd-vca5l).
    ///
    /// The config is a process-wide `RwLock` and cargo runs tests in PARALLEL, so a test that
    /// installs a non-default config is visible to every concurrent test that reads one. The
    /// mutator below already restores the original through an RAII guard, which is careful and
    /// still insufficient: the exposure is the INTERVAL it holds the lock for, not what it leaves
    /// behind afterwards.
    ///
    /// Poison-tolerant on purpose. A `Mutex` poisoned by one failing test would otherwise make
    /// every other test in this set report a poisoned lock instead of its own result, turning one
    /// real failure into a screenful of noise that hides it.
    static CONFIG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn config_guard() -> std::sync::MutexGuard<'static, ()> {
        CONFIG_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // ── renderDeck battery (bd-cb9oi) ─────────────────────────────

    const DECKED_INPUT: &str = "flowchart LR\n%%{deck: {tips: {a: 'start here'}, slides: [{id: 's1', nodes: ['a', 'b'], reveal: [['b']]}, {id: 's2', nodes: ['c']}]}}%%\n  a --> b\n  b --> c\n";

    #[test]
    fn render_deck_returns_svg_and_manifest_for_decked_input() {
        let _serial = config_guard();
        let output = render_deck(DECKED_INPUT, None).expect("render_deck");
        assert!(output.svg.starts_with("<svg"));
        let manifest = output.manifest.expect("manifest present");
        assert_eq!(manifest.schema_version, "1.0.0");
        assert_eq!(manifest.slides.len(), 2);
        assert_eq!(manifest.slides[0].id, "s1");
        assert_eq!(manifest.slides[0].max_step, 1);
        assert_eq!(
            manifest.slides[0].nodes[0].tooltip.as_deref(),
            Some("start here")
        );
        // Every manifest element id must exist verbatim in the paired SVG (the join-key
        // contract, checked at the export boundary too).
        for slide in &manifest.slides {
            for node in &slide.nodes {
                assert!(
                    output.svg.contains(&format!("id=\"{}\"", node.element_id)),
                    "missing {} in svg",
                    node.element_id
                );
            }
        }
        assert!(
            output.warnings.is_empty(),
            "clean deck must produce no warnings: {:?}",
            output.warnings
        );
    }

    #[test]
    fn render_deck_is_a_strict_superset_of_render_svg() {
        let _serial = config_guard();
        // Byte-parity at nominal pressure tier: the shared prepare_svg_render pipeline is
        // what makes this hold — a second copy of the config/pressure plumbing would drift.
        let deck_output = render_deck(DECKED_INPUT, None).expect("render_deck");
        let svg_only = render_svg_js(DECKED_INPUT, None).expect("render_svg_js");
        assert_eq!(deck_output.svg, svg_only);
    }

    #[test]
    fn render_deck_on_deckless_input_yields_null_manifest_and_one_warning() {
        let _serial = config_guard();
        let output = render_deck("flowchart LR\n  a --> b\n", None).expect("render_deck");
        assert!(output.svg.starts_with("<svg"));
        assert!(output.manifest.is_none());
        assert_eq!(output.warnings.len(), 1);
        assert!(output.warnings[0].message.contains("manifest is null"));
    }

    #[test]
    fn render_deck_surfaces_structured_deck_diagnostics() {
        let _serial = config_guard();
        // Unknown selector: the parser's semantic warning must flow through, structured.
        let output = render_deck(
            "flowchart LR\n%%{deck: {slides: [{id: 's', nodes: ['a', 'ghost']}]}}%%\n  a --> b\n",
            None,
        )
        .expect("render_deck");
        assert!(output.manifest.is_some(), "slide still resolves via 'a'");
        assert!(
            output
                .warnings
                .iter()
                .any(|warning| warning.message.contains("unknown selector 'ghost'")),
            "{:?}",
            output.warnings
        );

        // Unsupported family: SVG renders, manifest null, family warning present.
        let output = render_deck(
            "pie\n%%{deck: {slides: [{id: 's', nodes: ['*']}]}}%%\n  \"A\": 10\n",
            None,
        )
        .expect("render_deck");
        assert!(output.svg.starts_with("<svg"));
        assert!(output.manifest.is_none());
        assert!(
            output
                .warnings
                .iter()
                .any(|warning| warning.message.contains("no addressable elements")),
            "{:?}",
            output.warnings
        );
    }

    #[test]
    fn render_svg_js_uses_same_font_metrics_layout_path_as_render() {
        let _serial = config_guard();
        struct RuntimeConfigGuard(RuntimeConfig);

        impl Drop for RuntimeConfigGuard {
            fn drop(&mut self) {
                write_runtime_config(self.0.clone());
            }
        }

        let original = read_runtime_config();
        let _guard = RuntimeConfigGuard(original.clone());
        let mut updated = original;
        updated.svg = SvgRenderConfig {
            font_size: 28.0,
            avg_char_width: 18.0,
            line_height: 1.4,
            ..updated.svg
        };
        write_runtime_config(updated);

        let input = "flowchart LR\nA[This is a long label that should widen layout]-->B";
        let render_output = render(input);
        let svg_only = render_svg_js(input, None).expect("renderSvg should succeed");

        assert_eq!(svg_only, render_output.svg);
    }

    #[cfg(target_arch = "wasm32")]
    #[test]
    fn capability_matrix_js_returns_matrix_payload() {
        let value = capability_matrix_js().expect("capability matrix should serialize");
        let json = value
            .as_string()
            .expect("wasm tests should receive stringifiable payload");
        let payload: serde_json::Value =
            serde_json::from_str(&json).expect("payload should parse as JSON");
        assert_eq!(payload["project"], "frankenmermaid");
        assert_eq!(payload["schema_version"], "1.0.0");
        assert!(
            payload["claims"]
                .as_array()
                .is_some_and(|claims| !claims.is_empty())
        );
    }

    /// Every capability combination must route somewhere sensible (bd-2u0.6 item 3).
    ///
    /// Table-driven because the interesting content is the FALLBACK ladder, and a ladder is only
    /// correct as a whole: asserting the happy path alone would let every degraded environment
    /// collapse to the same answer without a test noticing.
    #[test]
    fn canvas_routing_degrades_one_step_at_a_time() {
        use super::{CanvasTarget, HostCapabilities, choose_canvas_target};

        let cases = [
            (
                HostCapabilities {
                    offscreen_canvas: true,
                    worker: true,
                    canvas_transferred: true,
                },
                CanvasTarget::OffscreenInWorker,
                "everything available",
            ),
            (
                HostCapabilities {
                    offscreen_canvas: true,
                    worker: true,
                    canvas_transferred: false,
                },
                CanvasTarget::SvgInWorker,
                "OffscreenCanvas exists but the transfer did not happen",
            ),
            (
                HostCapabilities {
                    offscreen_canvas: false,
                    worker: true,
                    canvas_transferred: false,
                },
                CanvasTarget::SvgInWorker,
                "no OffscreenCanvas, but parse and layout can still leave the main thread",
            ),
            (
                HostCapabilities {
                    offscreen_canvas: true,
                    worker: false,
                    canvas_transferred: true,
                },
                CanvasTarget::MainThreadCanvas,
                "no worker: an offscreen canvas on the main thread blocks the same event loop",
            ),
            (
                HostCapabilities::default(),
                CanvasTarget::MainThreadCanvas,
                "nothing available",
            ),
        ];

        for (capabilities, expected, why) in cases {
            assert_eq!(
                choose_canvas_target(capabilities),
                expected,
                "{why}: {capabilities:?}"
            );
        }
    }

    /// CONTROL: a missing worker beats a present OffscreenCanvas.
    ///
    /// Stated separately because it is the one ordering a reasonable implementation gets wrong —
    /// `offscreen_canvas` is the more specific-sounding capability, so checking it first is the
    /// natural mistake, and it would route work to an offscreen canvas that still blocks the UI.
    #[test]
    fn a_missing_worker_outranks_a_present_offscreen_canvas() {
        use super::{CanvasTarget, HostCapabilities, choose_canvas_target};

        assert_eq!(
            choose_canvas_target(HostCapabilities {
                offscreen_canvas: true,
                worker: false,
                canvas_transferred: true,
            }),
            CanvasTarget::MainThreadCanvas
        );
    }

    /// The routing decision must survive the wire, like every other worker type.
    #[test]
    fn capabilities_and_target_round_trip_as_json() {
        use super::{CanvasTarget, HostCapabilities, choose_canvas_target};

        let capabilities = HostCapabilities {
            offscreen_canvas: true,
            worker: true,
            canvas_transferred: false,
        };
        let encoded = serde_json::to_string(&capabilities).expect("capabilities serialize");
        assert!(
            encoded.contains("offscreenCanvas") && encoded.contains("canvasTransferred"),
            "capabilities are not camelCase on the wire: {encoded}"
        );
        let decoded: HostCapabilities =
            serde_json::from_str(&encoded).expect("capabilities deserialize");
        assert_eq!(decoded, capabilities);

        let target = choose_canvas_target(capabilities);
        let encoded = serde_json::to_string(&target).expect("target serialize");
        assert!(
            encoded.contains("svgInWorker"),
            "target is not camelCase on the wire: {encoded}"
        );
        assert_eq!(
            serde_json::from_str::<CanvasTarget>(&encoded).expect("target deserialize"),
            target
        );
    }

    /// The JS-facing wrapper must agree with the Rust decision and reject junk.
    ///
    /// The wrapper is where a second implementation would creep in, so it is pinned to the same
    /// function rather than to a restated expectation.
    #[test]
    fn the_exported_target_chooser_matches_the_rust_decision() {
        use super::{HostCapabilities, choose_canvas_target, choose_canvas_target_result};

        for capabilities in [
            HostCapabilities {
                offscreen_canvas: true,
                worker: true,
                canvas_transferred: true,
            },
            HostCapabilities {
                offscreen_canvas: true,
                worker: true,
                canvas_transferred: false,
            },
            HostCapabilities {
                offscreen_canvas: false,
                worker: true,
                canvas_transferred: false,
            },
            HostCapabilities::default(),
        ] {
            let json = serde_json::to_string(&capabilities).expect("serialize");
            let exported =
                choose_canvas_target_result(&json).expect("the wrapper should accept it");
            let expected =
                serde_json::to_string(&choose_canvas_target(capabilities)).expect("expected");
            assert_eq!(exported, expected, "wrapper disagreed for {capabilities:?}");
        }

        assert!(
            choose_canvas_target_result("not json").is_err(),
            "the wrapper accepted malformed capabilities instead of reporting them"
        );
    }

    /// The WASM WebGPU plan must keep the corpus node identity that SVG emits.
    #[test]
    fn webgpu_plan_for_corpus_flowchart_matches_svg_node_identity() {
        let _serial = config_guard();
        const FLOWCHART: &str =
            include_str!("../../fm-cli/tests/fixtures/frankentui_conformance/flowchart_basic.mmd");
        let planned = build_webgpu_plan(FLOWCHART, None).expect("plan corpus flowchart");
        let summary = WebGpuPlanSummary::from(&planned.plan);

        assert!(
            summary.node_instances > 0 && summary.edge_segments > 0 && summary.text_quads > 0,
            "CONTROL FAILED: the corpus flowchart yielded an incomplete WebGPU plan"
        );
        let svg = fm_render_svg::render_svg(&planned.ir);
        for node in &planned.ir.nodes {
            assert!(
                svg.contains(node.id.as_str()),
                "SVG omitted node {:?}; it cannot serve as the reference arm",
                node.id
            );
        }
        assert_eq!(
            summary.node_instances,
            planned.ir.nodes.len(),
            "WebGPU plan and SVG source disagree about the corpus node set"
        );
    }

    /// SAME-IR COMPARISON: the regions the wasm surface publishes must describe the same
    /// interactive nodes the SVG backend decorates, joined on the id both arms use.
    ///
    /// Before this export, `href`, `callback` and `tooltip` appeared NOWHERE in this crate: a
    /// browser host on the canvas or WebGPU path could not reach `click` at all, while the SVG path
    /// got it for free from the document. That is the same parsed-stored-reachable-by-nobody shape
    /// bd-bk7h found one layer down, and it is invisible unless the two arms are compared.
    #[test]
    fn the_wasm_hit_regions_describe_the_same_interactive_nodes_as_the_svg() {
        let _serial = config_guard();
        const SOURCE: &str = "flowchart LR\n  A[Alpha] --> B[Beta]\n  B --> C[Gamma]\n  \
                              click A \"https://example.com\" \"Alpha tip\"\n  \
                              click B call doThing() \"Beta tip\"\n";

        // Through the same path `hitRegions` takes, stopping just short of the `JsValue` wrapper:
        // wasm-bindgen's `JsValue` traps outside wasm, so — as with every other export in this
        // crate — a native test drives the inner call. The `Serialize` impl asserted here is
        // literally the one `to_js_value` hands to serde-wasm-bindgen in the browser, so the JSON
        // contract below is the contract a JS host sees.
        let geometry = build_diagram_geometry(SOURCE, None).expect("lay the diagram out");
        let exported: Vec<WasmHitRegion> =
            fm_render_canvas::hit_regions(&geometry.ir, &geometry.layout)
                .iter()
                .map(WasmHitRegion::from)
                .collect();
        let json = serde_json::to_value(&exported).expect("regions serialize");
        let regions = json.as_array().expect("an array of regions").clone();

        let svg = fm_render_svg::render_svg(&geometry.ir);

        // CONTROL ON THE REFERENCE ARM: if the SVG emitted no tooltips then `click` never reached
        // the IR, and every assertion below would hold vacuously for the wrong reason.
        for tip in ["Alpha tip", "Beta tip"] {
            assert!(
                svg.contains(&format!("title=\"{tip}\"")),
                "CONTROL FAILED: SVG emitted no title={tip:?}, so this fixture cannot compare arms"
            );
        }

        assert_eq!(
            regions.len(),
            2,
            "expected exactly the two interactive nodes, got {regions:?}"
        );

        let alpha = regions
            .iter()
            .find(|r| r["nodeId"] == "A")
            .expect("A is interactive in both arms");
        assert_eq!(alpha["href"], "https://example.com");
        assert_eq!(alpha["tooltip"], "Alpha tip");
        assert!(
            alpha["callback"].is_null(),
            "A declared an href, not a callback"
        );
        // camelCase, not snake_case: this is the published JSON contract a browser host reads.
        assert!(
            alpha.get("linkTarget").is_some() && alpha.get("link_target").is_none(),
            "the JSON contract must be camelCase for a JS host, got {alpha:?}"
        );

        let beta = regions
            .iter()
            .find(|r| r["nodeId"] == "B")
            .expect("B is interactive in both arms");
        assert_eq!(beta["tooltip"], "Beta tip");
        assert!(
            !beta["callback"].is_null(),
            "B declared `call doThing()` and the callback was dropped crossing the boundary"
        );

        // C has no `click`, so it must not be reported — otherwise a host sees the whole diagram as
        // clickable and the filtering this export exists to do lands back on the caller.
        assert!(
            !regions.iter().any(|r| r["nodeId"] == "C"),
            "a node with no `click` was given a region"
        );
        assert!(
            !svg.contains("title=\"Gamma"),
            "CONTROL FAILED: the SVG gave C a tooltip, so C is not the negative case assumed here"
        );

        // The bounds must be the LAYOUT's, not a re-derivation: a region in the wrong place is a
        // defect with no visible symptom until somebody clicks.
        let placed = geometry
            .layout
            .nodes
            .iter()
            .find(|n| geometry.ir.nodes[n.node_index].id.as_str() == "A")
            .expect("A was placed");
        let bounds = alpha["bounds"].as_array().expect("bounds is an array");
        assert_eq!(bounds.len(), 4, "bounds is [x, y, width, height]");
        for (index, expected) in [
            placed.bounds.x,
            placed.bounds.y,
            placed.bounds.width,
            placed.bounds.height,
        ]
        .into_iter()
        .enumerate()
        {
            let actual = bounds[index].as_f64().expect("a number") as f32;
            assert!(
                (actual - expected).abs() < f32::EPSILON,
                "bounds[{index}] was {actual}, the layout placed A at {expected}"
            );
        }
        assert!(
            placed.bounds.width > 0.0 && placed.bounds.height > 0.0,
            "CONTROL FAILED: A has a degenerate box, so matching it proves nothing"
        );
    }

    /// The WASM WebGPU entrypoint must prepare primitives from the same corpus source that SVG
    /// renders, then a real headless device must paint those node primitives.
    ///
    /// This is intentionally a geometry comparison rather than a pixel diff: SVG text shaping and
    /// raster antialiasing are backend-specific, but a node missing from either plan is an outright
    /// cross-backend correctness failure. The feature gate keeps default WASM consumers from
    /// pulling in the optional device stack.
    #[cfg(feature = "webgpu")]
    #[test]
    fn webgpu_plan_rasterises_the_corpus_flowchart_and_agrees_with_svg_nodes() {
        const FLOWCHART: &str =
            include_str!("../../fm-cli/tests/fixtures/frankentui_conformance/flowchart_basic.mmd");
        let planned = build_webgpu_plan(FLOWCHART, None).expect("plan corpus flowchart");

        assert!(
            !planned.plan.node_instances.is_empty(),
            "CONTROL FAILED: corpus flowchart produced no WebGPU node instances"
        );
        assert!(
            !planned.plan.text_quads.is_empty(),
            "CONTROL FAILED: corpus flowchart produced no WebGPU text primitives"
        );

        let svg = fm_render_svg::render_svg(&planned.ir);
        for node in &planned.ir.nodes {
            assert!(
                svg.contains(node.id.as_str()),
                "SVG omitted node {:?}; it cannot serve as the reference arm",
                node.id
            );
        }
        assert_eq!(
            planned.plan.node_instances.len(),
            planned.ir.nodes.len(),
            "WebGPU plan and SVG source disagree about the corpus node set"
        );

        let gpu = GpuDevice::headless()
            .unwrap_or_else(|error| panic!("headless WebGPU test requires an adapter: {error}"));
        eprintln!(
            "[gpu] adapter={:?} backend={:?}",
            gpu.adapter_name(),
            gpu.backend()
        );

        let pass = InstancePass::new(&gpu, &node_pipeline());
        let bytes = node_instance_bytes(&planned.plan.node_instances);
        let draw = InstanceDraw {
            bounds: &planned.plan.bounds,
            instance_bytes: &bytes,
            instance_count: u32::try_from(planned.plan.node_instances.len())
                .expect("corpus node count fits u32"),
            atlas: None,
        };
        let image = render_instances(&gpu, &pass, &draw, 512, 512).expect("rasterise nodes");
        assert!(
            image
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] > 0),
            "WebGPU rendered a fully transparent image"
        );
        for instance in &planned.plan.node_instances {
            let [node_x, node_y] = instance.center;
            let (x, y) = image.pixel_for(&planned.plan.bounds, node_x, node_y);
            let pixel = image.pixel(x, y).expect("node centre maps into the target");
            assert!(
                pixel[3] > 0,
                "WebGPU omitted node {} at pixel ({x}, {y})",
                instance.node_index
            );
        }
    }

    /// THE BROWSER ENTRY POINT'S OWN PATH, exercised natively (bd-2u0.2).
    ///
    /// `renderWebGpuToRgba` is `async` and wasm-only, but the half that does the work —
    /// `render_webgpu_pixels` — is compiled on both targets precisely so it can be tested here. What
    /// this proves is that the browser export is not a separate renderer: it plans through
    /// `build_webgpu_plan`, the same call `planWebGpu` uses, and draws every family in one
    /// submission through the shared device pass.
    ///
    /// The test above renders NODES ONLY, through the per-family path. This renders the WHOLE
    /// diagram, which is what a browser caller receives, and asserts the difference is real: the
    /// composed image must contain strictly more ink than the node pass alone, or the families are
    /// not composing and the browser would get a picture missing its edges and labels.
    #[cfg(feature = "webgpu")]
    #[test]
    fn the_browser_render_path_composes_every_family_for_the_corpus_flowchart() {
        const FLOWCHART: &str =
            include_str!("../../fm-cli/tests/fixtures/frankentui_conformance/flowchart_basic.mmd");
        let planned = build_webgpu_plan(FLOWCHART, None).expect("plan corpus flowchart");

        // CONTROLS: a "whole diagram" claim is empty unless the fixture has more than one family.
        assert!(
            !planned.plan.node_instances.is_empty(),
            "CONTROL FAILED: no nodes"
        );
        assert!(
            !planned.plan.edge_segments.is_empty(),
            "CONTROL FAILED: no edges"
        );
        assert!(
            !planned.plan.text_quads.is_empty(),
            "CONTROL FAILED: no text"
        );

        let rgba = pollster::block_on(super::render_webgpu_pixels(&planned.plan, 512, 512))
            .unwrap_or_else(|error| panic!("browser render path failed: {error}"));
        assert_eq!(
            rgba.len(),
            512 * 512 * 4,
            "the browser path must return width*height*4 RGBA bytes"
        );

        let composed_ink = rgba.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count();
        assert!(composed_ink > 0, "the browser path returned a blank image");

        // The node family alone, for comparison, through the per-family path in the same run.
        let gpu = GpuDevice::headless().expect("adapter");
        let pass = InstancePass::new(&gpu, &node_pipeline());
        let bytes = node_instance_bytes(&planned.plan.node_instances);
        let draw = InstanceDraw {
            bounds: &planned.plan.bounds,
            instance_bytes: &bytes,
            instance_count: u32::try_from(planned.plan.node_instances.len()).expect("fits u32"),
            atlas: None,
        };
        let nodes_only = render_instances(&gpu, &pass, &draw, 512, 512).expect("rasterise nodes");
        let nodes_ink = nodes_only
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0)
            .count();

        eprintln!("[wasm-gpu] composed={composed_ink}px nodes_only={nodes_ink}px");
        assert!(
            composed_ink > nodes_ink,
            "the browser path painted {composed_ink} pixels and the node pass alone painted \
             {nodes_ink}; a browser caller would be getting nodes without their edges or labels"
        );
    }

    /// CROSS-TARGET DETERMINISM: x86_64 must render byte-identically to wasm32 (bd-1s1g.6).
    ///
    /// The bead asks for layout output compared ACROSS targets. Every determinism test in this
    /// project until now ran on ONE machine and said so, because there was no runner for a second
    /// target. There is one: `pkg/` is a real wasm32-unknown-unknown build of THIS crate, and node
    /// can drive it.
    ///
    /// ⚠️ BOTH SIDES MUST CALL `render_svg_js`, AND THAT IS NOT INTERCHANGEABLE WITH `render`.
    /// `render` reads the runtime config directly; `render_svg_js` merges an optional override and
    /// a theme. Digesting one against the other would compare two FUNCTIONS and report the
    /// difference as a cross-target divergence — a false positive of exactly the kind this bead
    /// must not manufacture. The JS side calls `renderSvg(source)` with no config argument, which
    /// reaches the same default path as `None` here.
    ///
    /// The constants come from `scripts/wasm_cross_target_digest.mjs` run against the wasm bundle.
    /// FNV-1a over the UTF-8 bytes, not `DefaultHasher`: std's hasher is not stable across versions
    /// or targets, so a cross-target golden built on it would differ for reasons unrelated to
    /// floating point.
    ///
    /// WHAT A FAILURE MEANS, in order of likelihood: the wasm bundle in `pkg/` is stale relative to
    /// HEAD (rebuild it with `build-wasm.sh`), a config default diverged between the two entry
    /// points, or — the case this bead is actually about — the two targets genuinely disagree about
    /// floating point. Check them in that order; only the third is a determinism defect.
    /// ⚠️ TAKES `config_guard()`, AND WITHOUT IT THIS TEST IS A FALSE-POSITIVE GENERATOR.
    ///
    /// `render_svg_js` reads `RUNTIME_CONFIG`. While the font-metrics test holds a non-default
    /// config, this one renders under it: run alone it passes, run in the suite it produced
    /// `0xf9c38140c62e9ffa` against the bundle's `0x8835c1df81b8f371`. A global-state leak wearing
    /// the costume of a cross-target divergence — the exact false positive this bead must not
    /// manufacture. Resetting the config from here instead was tried and broke the other test, which
    /// renders twice and compares; the lock is what fixes both directions (bd-vca5l).
    ///
    /// THE BUNDLE IS THE OTHER HALF OF THIS TEST AND IT IS NOT REBUILT AUTOMATICALLY. `pkg/` is a
    /// committed artifact; if it drifts from source these digests describe an older revision:
    ///
    /// ```text
    /// env -u CARGO_TARGET_DIR bash build-wasm.sh
    /// node scripts/wasm_cross_target_digest.mjs
    /// ```
    #[test]
    fn x86_64_and_wasm32_render_the_same_bytes() {
        let _serial = config_guard();
        struct RuntimeConfigGuard(RuntimeConfig);

        impl Drop for RuntimeConfigGuard {
            fn drop(&mut self) {
                write_runtime_config(self.0.clone());
            }
        }

        // The Node driver starts a fresh WASM instance and never calls `init`, so its runtime is
        // exactly `RuntimeConfig::default()`. Reset the native arm to the same state before any
        // digest comparison. With source and package provenance checked below, a later byte
        // mismatch is then a real target disagreement rather than leaked process-global config.
        let _runtime_config_guard = RuntimeConfigGuard(read_runtime_config());
        write_runtime_config(RuntimeConfig::default());

        // These five files are the versioned code, interface, and metadata package surface. The
        // copied README is deliberately excluded so a documentation-only edit does not require a
        // bundle rebuild. Pinning the combined digest separately from rendered-output digests makes
        // a regenerated or partially rebuilt package fail with an artifact-specific instruction
        // before it can masquerade as a cross-target floating-point difference. Rebuild from the
        // same source revision, then update this digest and the per-fixture digests together after
        // reviewing every generated artifact.
        const EXPECTED_PACKAGE_ARTIFACT_DIGEST: u64 = 0x096d_2792_8e2a_0002;
        let package_artifacts: [&[u8]; 5] = [
            include_bytes!("../../../pkg/frankenmermaid_bg.wasm"),
            include_bytes!("../../../pkg/frankenmermaid.js"),
            include_bytes!("../../../pkg/frankenmermaid.d.ts"),
            include_bytes!("../../../pkg/frankenmermaid_bg.wasm.d.ts"),
            include_bytes!("../../../pkg/package.json"),
        ];
        let mut package_hash = 0xcbf2_9ce4_8422_2325_u64;
        for artifact in package_artifacts {
            for byte in artifact {
                package_hash ^= u64::from(*byte);
                package_hash = package_hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        assert_eq!(
            package_hash, EXPECTED_PACKAGE_ARTIFACT_DIGEST,
            "the committed pkg artifact set changed or is only partially rebuilt; run the canonical \
             build-wasm.sh, review all five generated files, then update this package digest and \
             the four render digests together"
        );

        assert_package_source_matches_current_inputs();

        const EXPECTED: &[(&str, &str, u64)] = &[
            (
                "flowchart",
                "flowchart TD\n  a[Alpha] --> b[Beta]\n  b --> c[Gamma]\n  c -.--> a\n  b --> d[Delta]\n",
                0x1282_95d3_c573_12a8,
            ),
            (
                "sequence",
                "sequenceDiagram\n  participant A\n  participant B\n  A->>B: hello\n  B-->>A: reply\n",
                0x0ee7_7963_7b5e_4ec8,
            ),
            (
                "class",
                "classDiagram\n  class Alpha {\n    +String name\n    +run()\n  }\n  Alpha <|-- Beta\n",
                0xbc1c_5e78_1065_7b1e,
            ),
            (
                "state",
                "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy: start\n  Busy --> Idle: done\n",
                0x2848_9ff7_e724_f4f8,
            ),
        ];

        for (name, source, wasm_digest) in EXPECTED {
            let svg = render_svg_js(source, None).expect("renderSvg should succeed natively");
            let mut hash = 0xcbf2_9ce4_8422_2325_u64;
            for byte in svg.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            assert!(
                svg.len() > 1000,
                "{name}: rendered only {} bytes, so this comparison is vacuous",
                svg.len()
            );
            assert_eq!(
                hash, *wasm_digest,
                "{name}: x86_64 rendered {hash:#018x}, wasm32 rendered {wasm_digest:#018x} \
                 (stale pkg/? config default? or a real cross-target divergence)"
            );
        }
    }
}
