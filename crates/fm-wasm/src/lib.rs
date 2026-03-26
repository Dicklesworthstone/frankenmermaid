#![forbid(unsafe_code)]

use std::sync::{LazyLock, RwLock};
use std::time::Instant;

use fm_core::{
    MermaidBudgetLedger, MermaidDiagramIr, MermaidGuardReport, MermaidSourceMapKind,
    MermaidWasmPressureSignals, Span, capability_matrix, mermaid_layout_guard_observability,
};
use fm_layout::{
    DiagramLayout, LayoutConfig, LayoutGuardrails, build_layout_guard_report_with_pressure,
    layout_diagram_traced, layout_diagram_traced_with_config_and_guardrails,
};
#[cfg(target_arch = "wasm32")]
use fm_parser::ParseResult;
use fm_parser::{detect_type_with_confidence, parse};
use fm_render_canvas::CanvasRenderConfig;
#[cfg(target_arch = "wasm32")]
use fm_render_canvas::render_to_canvas_with_layout;
#[cfg(target_arch = "wasm32")]
use fm_render_canvas::{
    Canvas2dContext, CanvasRenderResult, LineCap, LineJoin, TextAlign, TextBaseline, TextMetrics,
    render_to_canvas,
};
use fm_render_svg::{
    SvgRenderConfig, ThemeColors, ThemePreset, describe_diagram_with_layout, render_svg_with_layout,
};
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    pub source_spans: Vec<SourceSpanRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSpanRecord {
    kind: &'static str,
    index: usize,
    id: Option<String>,
    element_id: String,
    span: Span,
}

#[derive(Debug, Clone)]
struct RuntimeConfig {
    svg: SvgRenderConfig,
    canvas: CanvasRenderConfig,
    pressure: MermaidWasmPressureSignals,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let svg = SvgRenderConfig::default();
        let canvas = align_canvas_typography_with_svg(CanvasRenderConfig::default(), &svg);
        Self {
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
    svg: SvgConfigOverrides,
    canvas: CanvasConfigOverrides,
    pressure: PressureConfigOverrides,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SvgConfigOverrides {
    responsive: Option<bool>,
    accessible: Option<bool>,
    font_family: Option<String>,
    font_size: Option<f32>,
    avg_char_width: Option<f32>,
    line_height: Option<f32>,
    padding: Option<f32>,
    shadows: Option<bool>,
    rounded_corners: Option<f32>,
    embed_theme_css: Option<bool>,
    theme: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CanvasConfigOverrides {
    font_family: Option<String>,
    font_size: Option<f64>,
    padding: Option<f64>,
    node_fill: Option<String>,
    node_stroke: Option<String>,
    node_stroke_width: Option<f64>,
    edge_stroke: Option<String>,
    edge_stroke_width: Option<f64>,
    cluster_fill: Option<String>,
    cluster_stroke: Option<String>,
    label_color: Option<String>,
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

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagramRenderOutput {
    svg: String,
    detected_type: String,
    confidence: f32,
    warnings: Vec<String>,
    trace_id: String,
    decision_id: String,
    policy_id: String,
    schema_version: String,
    guard: MermaidGuardReport,
    source_spans: Vec<SourceSpanRecord>,
    canvas: CanvasRenderSummary,
}

#[cfg(target_arch = "wasm32")]
impl DiagramRenderOutput {
    fn new(
        svg: String,
        parsed: &ParseResult,
        layout: &DiagramLayout,
        guard: MermaidGuardReport,
        canvas: &CanvasRenderResult,
    ) -> Self {
        Self {
            svg,
            detected_type: parsed.ir.diagram_type.as_str().to_string(),
            confidence: parsed.confidence,
            warnings: parsed.warnings.clone(),
            trace_id: guard.observability.trace_id.to_string(),
            decision_id: guard.observability.decision_id.to_string(),
            policy_id: guard.observability.policy_id.to_string(),
            schema_version: guard.observability.schema_version.to_string(),
            guard,
            source_spans: collect_source_spans(&parsed.ir, layout),
            canvas: CanvasRenderSummary::from(canvas),
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CanvasRenderSummary {
    draw_calls: usize,
    nodes_drawn: usize,
    edges_drawn: usize,
    clusters_drawn: usize,
    labels_drawn: usize,
    viewport: ViewportSummary,
}

#[cfg(target_arch = "wasm32")]
impl From<&CanvasRenderResult> for CanvasRenderSummary {
    fn from(value: &CanvasRenderResult) -> Self {
        Self {
            draw_calls: value.draw_calls,
            nodes_drawn: value.nodes_drawn,
            edges_drawn: value.edges_drawn,
            clusters_drawn: value.clusters_drawn,
            labels_drawn: value.labels_drawn,
            viewport: ViewportSummary {
                offset_x: value.viewport.offset_x,
                offset_y: value.viewport.offset_y,
                zoom: value.viewport.zoom,
                canvas_width: value.viewport.canvas_width,
                canvas_height: value.viewport.canvas_height,
                device_pixel_ratio: value.viewport.device_pixel_ratio,
            },
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewportSummary {
    offset_x: f64,
    offset_y: f64,
    zoom: f64,
    canvas_width: f64,
    canvas_height: f64,
    device_pixel_ratio: f64,
}

static RUNTIME_CONFIG: LazyLock<RwLock<RuntimeConfig>> =
    LazyLock::new(|| RwLock::new(RuntimeConfig::default()));

fn collect_source_spans(ir: &MermaidDiagramIr, layout: &DiagramLayout) -> Vec<SourceSpanRecord> {
    let _ = layout;
    ir.source_map()
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

fn read_runtime_config() -> RuntimeConfig {
    match RUNTIME_CONFIG.read() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
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

fn parse_js_value_or_default<T>(value: Option<JsValue>) -> Result<T, JsValue>
where
    T: for<'de> Deserialize<'de> + Default,
{
    match value {
        None => Ok(T::default()),
        Some(raw) if raw.is_undefined() || raw.is_null() => Ok(T::default()),
        Some(raw) => {
            #[cfg(target_arch = "wasm32")]
            {
                serde_wasm_bindgen::from_value(raw)
                    .map_err(|err| js_error(format!("invalid config: {err}")))
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let _ = raw;
                Ok(T::default())
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

fn merge_svg_config(
    base: &SvgRenderConfig,
    overrides: &SvgConfigOverrides,
    theme_override: Option<&str>,
) -> Result<SvgRenderConfig, JsValue> {
    let mut merged = base.clone();

    if let Some(value) = overrides.responsive {
        merged.responsive = value;
    }
    if let Some(value) = overrides.accessible {
        merged.accessible = value;
    }
    if let Some(value) = overrides.font_family.as_ref() {
        merged.font_family = value.clone();
    }
    if let Some(value) = overrides.font_size
        && value.is_finite()
        && value > 0.0
    {
        merged.font_size = value;
    }
    if let Some(value) = overrides.avg_char_width
        && value.is_finite()
        && value > 0.0
    {
        merged.avg_char_width = value;
    }
    if let Some(value) = overrides.line_height
        && value.is_finite()
        && value > 0.0
    {
        merged.line_height = value;
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

    let theme_name = overrides.theme.as_deref().or(theme_override);
    if let Some(name) = theme_name {
        merged.theme = name.parse::<ThemePreset>().map_err(|err| {
            js_error(format!(
                "invalid theme '{name}': {err}; expected one of default,dark,forest,neutral,corporate,neon,pastel,high-contrast,monochrome,blueprint"
            ))
        })?;
    }

    Ok(merged)
}

fn merge_canvas_config(
    base: &CanvasRenderConfig,
    overrides: &CanvasConfigOverrides,
) -> CanvasRenderConfig {
    let mut merged = base.clone();

    if let Some(value) = overrides.font_family.as_ref() {
        merged.font_family = value.clone();
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
    if let Some(value) = overrides.node_fill.as_ref() {
        merged.node_fill = value.clone();
    }
    if let Some(value) = overrides.node_stroke.as_ref() {
        merged.node_stroke = value.clone();
    }
    if let Some(value) = overrides.node_stroke_width
        && value.is_finite()
        && value >= 0.0
    {
        merged.node_stroke_width = value;
    }
    if let Some(value) = overrides.edge_stroke.as_ref() {
        merged.edge_stroke = value.clone();
    }
    if let Some(value) = overrides.edge_stroke_width
        && value.is_finite()
        && value >= 0.0
    {
        merged.edge_stroke_width = value;
    }
    if let Some(value) = overrides.cluster_fill.as_ref() {
        merged.cluster_fill = value.clone();
    }
    if let Some(value) = overrides.cluster_stroke.as_ref() {
        merged.cluster_stroke = value.clone();
    }
    if let Some(value) = overrides.label_color.as_ref() {
        merged.label_color = value.clone();
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
    canvas.font_family = svg.font_family.clone();
    canvas.font_size = f64::from(svg.font_size);
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

fn requested_theme_preset(overrides: &RuntimeInitConfig) -> Result<Option<ThemePreset>, JsValue> {
    let theme_name = overrides
        .svg
        .theme
        .as_deref()
        .or(overrides.theme.as_deref());
    theme_name
        .map(|name| {
            name.parse::<ThemePreset>().map_err(|err| {
                js_error(format!(
                    "invalid theme '{name}': {err}; expected one of default,dark,forest,neutral,corporate,neon,pastel,high-contrast,monochrome,blueprint"
                ))
            })
        })
        .transpose()
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

#[must_use]
pub fn render(input: &str) -> WasmRenderOutput {
    let runtime = read_runtime_config();
    let pressure = runtime.pressure.into_report();
    let mut budget_broker = MermaidBudgetLedger::new(&pressure);
    let parse_start = Instant::now();
    let parsed = parse(input);
    budget_broker.record_parse(parse_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
    let layout_guardrails = LayoutGuardrails {
        max_layout_time_ms: budget_broker.layout_time_budget_ms(),
        max_layout_iterations: budget_broker
            .layout_iteration_budget(LayoutGuardrails::default().max_layout_iterations),
        max_route_ops: budget_broker.route_budget(LayoutGuardrails::default().max_route_ops),
    };
    let layout_start = Instant::now();
    let layout_config = LayoutConfig {
        font_metrics: Some(runtime.svg.font_metrics()),
        ..Default::default()
    };
    let traced_layout = layout_diagram_traced_with_config_and_guardrails(
        &parsed.ir,
        fm_layout::LayoutAlgorithm::Auto,
        layout_config,
        layout_guardrails,
    );
    budget_broker
        .record_layout(layout_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
    let mut guard = build_layout_guard_report_with_pressure(&parsed.ir, &traced_layout, pressure);
    let (_cx, observability) = mermaid_layout_guard_observability(
        "wasm.render",
        input,
        traced_layout.trace.dispatch.selected.as_str(),
        traced_layout.trace.guard.estimated_layout_time_ms.max(1) as u64,
    );
    guard.observability = observability;
    guard.budget_broker = budget_broker.clone();
    let source_spans = collect_source_spans(&parsed.ir, &traced_layout.layout);
    let mut svg_config = runtime.svg.clone();
    svg_config.include_source_spans = true;
    apply_budget_svg_simplifications(&mut svg_config, &budget_broker);
    let render_start = Instant::now();
    let svg = render_svg_with_layout(&parsed.ir, &traced_layout.layout, &svg_config);
    budget_broker
        .record_render(render_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
    guard.budget_broker = budget_broker;

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
        source_spans,
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn init(config: Option<JsValue>) -> Result<(), JsValue> {
    let overrides: RuntimeInitConfig = parse_js_value_or_default(config)?;
    let current = read_runtime_config();
    let requested_theme = requested_theme_preset(&overrides)?;
    let svg = merge_svg_config(&current.svg, &overrides.svg, overrides.theme.as_deref())?;
    let canvas_base = requested_theme
        .map(|preset| apply_canvas_theme_preset(current.canvas.clone(), preset))
        .unwrap_or_else(|| current.canvas.clone());

    let next = RuntimeConfig {
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
    let overrides: RuntimeInitConfig = parse_js_value_or_default(config)?;
    let runtime = read_runtime_config();
    let mut svg_config =
        merge_svg_config(&runtime.svg, &overrides.svg, overrides.theme.as_deref())?;
    let pressure = merge_pressure_config(&runtime.pressure, &overrides.pressure).into_report();
    let mut budget_broker = MermaidBudgetLedger::new(&pressure);
    let parse_start = Instant::now();
    let parsed = parse(input);
    budget_broker.record_parse(parse_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
    let layout_guardrails = LayoutGuardrails {
        max_layout_time_ms: budget_broker.layout_time_budget_ms(),
        max_layout_iterations: budget_broker
            .layout_iteration_budget(LayoutGuardrails::default().max_layout_iterations),
        max_route_ops: budget_broker.route_budget(LayoutGuardrails::default().max_route_ops),
    };
    let layout_start = Instant::now();
    let layout_config = LayoutConfig {
        font_metrics: Some(svg_config.font_metrics()),
        ..Default::default()
    };
    let traced_layout = layout_diagram_traced_with_config_and_guardrails(
        &parsed.ir,
        fm_layout::LayoutAlgorithm::Auto,
        layout_config,
        layout_guardrails,
    );
    budget_broker
        .record_layout(layout_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
    let mut _guard = build_layout_guard_report_with_pressure(&parsed.ir, &traced_layout, pressure);
    let (_cx, observability) = mermaid_layout_guard_observability(
        "wasm.renderSvg",
        input,
        traced_layout.trace.dispatch.selected.as_str(),
        traced_layout.trace.guard.estimated_layout_time_ms.max(1) as u64,
    );
    _guard.observability = observability;
    svg_config.include_source_spans = true;
    apply_budget_svg_simplifications(&mut svg_config, &budget_broker);
    let render_start = Instant::now();
    let svg = render_svg_with_layout(&parsed.ir, &traced_layout.layout, &svg_config);
    budget_broker
        .record_render(render_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
    _guard.budget_broker = budget_broker;
    Ok(svg)
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = sourceSpans))]
pub fn source_spans_js(input: &str) -> Result<JsValue, JsValue> {
    let parsed = parse(input);
    let traced_layout = layout_diagram_traced(&parsed.ir);
    to_js_value(&collect_source_spans(&parsed.ir, &traced_layout.layout))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = describeDiagram))]
pub fn describe_diagram_js(input: &str) -> Result<String, JsValue> {
    let parsed = parse(input);
    let traced_layout = layout_diagram_traced(&parsed.ir);
    Ok(describe_diagram_with_layout(
        &parsed.ir,
        Some(&traced_layout.layout),
    ))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = capabilityMatrix))]
pub fn capability_matrix_js() -> Result<JsValue, JsValue> {
    to_js_value(&capability_matrix())
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
struct WebCanvas2dContext {
    canvas: web_sys::HtmlCanvasElement,
    context: web_sys::CanvasRenderingContext2d,
    current_font: String,
}

#[cfg(target_arch = "wasm32")]
impl WebCanvas2dContext {
    fn new(canvas: web_sys::HtmlCanvasElement, context: web_sys::CanvasRenderingContext2d) -> Self {
        Self {
            canvas,
            context,
            current_font: "14px sans-serif".to_string(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Canvas2dContext for WebCanvas2dContext {
    fn width(&self) -> f64 {
        f64::from(self.canvas.width())
    }

    fn height(&self) -> f64 {
        f64::from(self.canvas.height())
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
#[wasm_bindgen]
pub struct Diagram {
    canvas: web_sys::HtmlCanvasElement,
    context: web_sys::CanvasRenderingContext2d,
    svg_config: SvgRenderConfig,
    canvas_config: CanvasRenderConfig,
    pressure_config: MermaidWasmPressureSignals,
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

        let overrides: RuntimeInitConfig = parse_js_value_or_default(config)?;
        let runtime = read_runtime_config();
        let requested_theme = requested_theme_preset(&overrides)?;
        let svg_config =
            merge_svg_config(&runtime.svg, &overrides.svg, overrides.theme.as_deref())?;
        let canvas_base = requested_theme
            .map(|preset| apply_canvas_theme_preset(runtime.canvas.clone(), preset))
            .unwrap_or_else(|| runtime.canvas.clone());
        let canvas_config = align_canvas_typography_with_svg(
            merge_canvas_config(&canvas_base, &overrides.canvas),
            &svg_config,
        );
        let pressure_config = merge_pressure_config(&runtime.pressure, &overrides.pressure);

        Ok(Self {
            canvas,
            context,
            svg_config,
            canvas_config,
            pressure_config,
            destroyed: false,
        })
    }

    pub fn render(&mut self, input: &str, config: Option<JsValue>) -> Result<JsValue, JsValue> {
        self.ensure_alive()?;

        let overrides: RuntimeInitConfig = parse_js_value_or_default(config)?;
        let requested_theme = requested_theme_preset(&overrides)?;
        let next_svg =
            merge_svg_config(&self.svg_config, &overrides.svg, overrides.theme.as_deref())?;
        let next_pressure = merge_pressure_config(&self.pressure_config, &overrides.pressure);
        let pressure_report = next_pressure.into_report();
        let mut budget_broker = MermaidBudgetLedger::new(&pressure_report);
        let canvas_base = requested_theme
            .map(|preset| apply_canvas_theme_preset(self.canvas_config.clone(), preset))
            .unwrap_or_else(|| self.canvas_config.clone());
        let next_canvas = align_canvas_typography_with_svg(
            merge_canvas_config(&canvas_base, &overrides.canvas),
            &next_svg,
        );
        let parse_start = Instant::now();
        let parsed = parse(input);
        budget_broker
            .record_parse(parse_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        let layout_guardrails = LayoutGuardrails {
            max_layout_time_ms: budget_broker.layout_time_budget_ms(),
            max_layout_iterations: budget_broker
                .layout_iteration_budget(LayoutGuardrails::default().max_layout_iterations),
            max_route_ops: budget_broker.route_budget(LayoutGuardrails::default().max_route_ops),
        };
        let layout_start = Instant::now();
        let layout_config = LayoutConfig {
            font_metrics: Some(next_svg.font_metrics()),
            ..Default::default()
        };
        let traced_layout = fm_layout::layout_diagram_traced_with_config_and_guardrails(
            &parsed.ir,
            fm_layout::LayoutAlgorithm::Auto,
            layout_config,
            layout_guardrails,
        );
        budget_broker
            .record_layout(layout_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        let mut guard =
            build_layout_guard_report_with_pressure(&parsed.ir, &traced_layout, pressure_report);
        let (_cx, observability) = mermaid_layout_guard_observability(
            "wasm.diagram.render",
            input,
            traced_layout.trace.dispatch.selected.as_str(),
            traced_layout.trace.guard.estimated_layout_time_ms.max(1) as u64,
        );
        guard.observability = observability;
        let mut render_svg_config = next_svg.clone();
        render_svg_config.include_source_spans = true;
        apply_budget_svg_simplifications(&mut render_svg_config, &budget_broker);
        let render_start = Instant::now();
        let svg = render_svg_with_layout(&parsed.ir, &traced_layout.layout, &render_svg_config);

        let mut web_canvas = WebCanvas2dContext::new(self.canvas.clone(), self.context.clone());
        let canvas_result = render_to_canvas_with_layout(
            &parsed.ir,
            &traced_layout.layout,
            &mut web_canvas,
            &next_canvas,
        );
        budget_broker
            .record_render(render_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        guard.budget_broker = budget_broker;

        self.svg_config = next_svg;
        self.canvas_config = next_canvas;
        self.pressure_config = next_pressure;

        let output =
            DiagramRenderOutput::new(svg, &parsed, &traced_layout.layout, guard, &canvas_result);
        to_js_value(&output)
    }

    #[wasm_bindgen(js_name = setTheme)]
    pub fn set_theme(&mut self, theme: &str) -> Result<(), JsValue> {
        self.ensure_alive()?;
        let preset = theme.parse::<ThemePreset>().map_err(|err| {
            js_error(format!(
                "invalid theme '{theme}': {err}; expected one of default,dark,forest,neutral,corporate,neon,pastel,high-contrast,monochrome,blueprint"
            ))
        })?;
        let overrides = SvgConfigOverrides {
            theme: Some(theme.to_string()),
            ..SvgConfigOverrides::default()
        };
        self.svg_config = merge_svg_config(&self.svg_config, &overrides, None)?;
        self.canvas_config = apply_canvas_theme_preset(self.canvas_config.clone(), preset);
        Ok(())
    }

    pub fn on(&self, event: &str, callback: &js_sys::Function) -> Result<(), JsValue> {
        self.ensure_alive()?;
        self.canvas
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
            f64::from(self.canvas.width()),
            f64::from(self.canvas.height()),
        );
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
        CanvasConfigOverrides, PressureConfigOverrides, RuntimeConfig, RuntimeInitConfig,
        SvgConfigOverrides, ThemePreset, align_canvas_typography_with_svg,
        apply_budget_svg_simplifications, apply_canvas_theme_preset, canvas_font_size_px,
        collect_source_spans, describe_diagram_js, merge_canvas_config, merge_pressure_config,
        merge_svg_config, read_runtime_config, render, render_svg_js, requested_theme_preset,
        write_runtime_config,
    };
    use fm_core::{MermaidPressureTier, MermaidWasmPressureSignals};
    use fm_layout::layout_diagram_traced;
    use fm_parser::parse;
    use fm_render_canvas::CanvasRenderConfig;
    use fm_render_svg::SvgRenderConfig;

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
        assert!(output.source_spans.iter().any(|span| span.kind == "node"));
        assert!(output.source_spans.iter().any(|span| span.kind == "edge"));
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
    fn describe_diagram_js_returns_layout_aware_summary() {
        let summary =
            describe_diagram_js("flowchart LR\nA[Start]-->B[End]").expect("summary should succeed");
        assert!(summary.contains("Key nodes"));
        assert!(summary.contains("Layout spans"));
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
            avg_char_width: Some(-2.0),
            line_height: Some(0.0),
            padding: Some(-1.0),
            rounded_corners: Some(f32::INFINITY),
            ..SvgConfigOverrides::default()
        };

        let merged = merge_svg_config(&base, &overrides, None).expect("merge should succeed");

        assert_eq!(merged.font_size, base.font_size);
        assert_eq!(merged.avg_char_width, base.avg_char_width);
        assert_eq!(merged.line_height, base.line_height);
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
    fn theme_override_rethemes_canvas_before_explicit_canvas_overrides() {
        let base_canvas = CanvasRenderConfig::default();
        let overrides = RuntimeInitConfig {
            theme: Some("dark".to_string()),
            canvas: CanvasConfigOverrides {
                edge_stroke: Some("#ff00aa".to_string()),
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
        assert_eq!(merged.edge_stroke, "#ff00aa");
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

    #[test]
    fn render_svg_js_uses_same_font_metrics_layout_path_as_render() {
        struct RuntimeConfigGuard(RuntimeConfig);

        impl Drop for RuntimeConfigGuard {
            fn drop(&mut self) {
                write_runtime_config(self.0.clone());
            }
        }

        let original = read_runtime_config();
        let _guard = RuntimeConfigGuard(original.clone());
        let mut updated = original.clone();
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
}
