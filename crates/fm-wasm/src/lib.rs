#![forbid(unsafe_code)]

use std::sync::{LazyLock, RwLock};

use fm_layout::{LayoutAlgorithm, layout};
use fm_parser::{detect_type_with_confidence, parse};
use fm_render_canvas::CanvasRenderConfig;
#[cfg(target_arch = "wasm32")]
use fm_parser::ParseResult;
#[cfg(target_arch = "wasm32")]
use fm_render_canvas::{
    Canvas2dContext, CanvasRenderResult, LineCap, LineJoin, TextAlign, TextBaseline, TextMetrics,
    render_to_canvas,
};
use fm_render_svg::{SvgRenderConfig, ThemePreset, render_svg_with_config};
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmRenderOutput {
    pub svg: String,
    pub detected_type: String,
}

#[derive(Debug, Clone)]
struct RuntimeConfig {
    svg: SvgRenderConfig,
    canvas: CanvasRenderConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            svg: SvgRenderConfig::default(),
            canvas: CanvasRenderConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RuntimeInitConfig {
    theme: Option<String>,
    svg: SvgConfigOverrides,
    canvas: CanvasConfigOverrides,
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

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagramRenderOutput {
    svg: String,
    detected_type: String,
    confidence: f32,
    warnings: Vec<String>,
    canvas: CanvasRenderSummary,
}

#[cfg(target_arch = "wasm32")]
impl DiagramRenderOutput {
    fn new(svg: String, parsed: &ParseResult, canvas: &CanvasRenderResult) -> Self {
        Self {
            svg,
            detected_type: parsed.ir.diagram_type.as_str().to_string(),
            confidence: parsed.confidence,
            warnings: parsed.warnings.clone(),
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
    if let Some(value) = overrides.font_size {
        merged.font_size = value;
    }
    if let Some(value) = overrides.avg_char_width {
        merged.avg_char_width = value;
    }
    if let Some(value) = overrides.line_height {
        merged.line_height = value;
    }
    if let Some(value) = overrides.padding {
        merged.padding = value;
    }
    if let Some(value) = overrides.shadows {
        merged.shadows = value;
    }
    if let Some(value) = overrides.rounded_corners {
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
    if let Some(value) = overrides.font_size {
        merged.font_size = value;
    }
    if let Some(value) = overrides.padding {
        merged.padding = value;
    }
    if let Some(value) = overrides.node_fill.as_ref() {
        merged.node_fill = value.clone();
    }
    if let Some(value) = overrides.node_stroke.as_ref() {
        merged.node_stroke = value.clone();
    }
    if let Some(value) = overrides.node_stroke_width {
        merged.node_stroke_width = value;
    }
    if let Some(value) = overrides.edge_stroke.as_ref() {
        merged.edge_stroke = value.clone();
    }
    if let Some(value) = overrides.edge_stroke_width {
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

#[must_use]
pub fn render(input: &str) -> WasmRenderOutput {
    let parsed = parse(input);
    let runtime = read_runtime_config();
    let _stats = layout(&parsed.ir, LayoutAlgorithm::Auto);

    WasmRenderOutput {
        svg: render_svg_with_config(&parsed.ir, &runtime.svg),
        detected_type: format!("{:?}", parsed.ir.diagram_type),
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn init(config: Option<JsValue>) -> Result<(), JsValue> {
    let overrides: RuntimeInitConfig = parse_js_value_or_default(config)?;
    let current = read_runtime_config();

    let next = RuntimeConfig {
        svg: merge_svg_config(&current.svg, &overrides.svg, overrides.theme.as_deref())?,
        canvas: merge_canvas_config(&current.canvas, &overrides.canvas),
    };

    write_runtime_config(next);
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = renderSvg))]
pub fn render_svg_js(input: &str, config: Option<JsValue>) -> Result<String, JsValue> {
    let overrides: RuntimeInitConfig = parse_js_value_or_default(config)?;
    let runtime = read_runtime_config();
    let svg_config = merge_svg_config(&runtime.svg, &overrides.svg, overrides.theme.as_deref())?;
    let parsed = parse(input);
    Ok(render_svg_with_config(&parsed.ir, &svg_config))
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

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
struct WebCanvas2dContext {
    canvas: web_sys::HtmlCanvasElement,
    context: web_sys::CanvasRenderingContext2d,
}

#[cfg(target_arch = "wasm32")]
impl WebCanvas2dContext {
    fn new(canvas: web_sys::HtmlCanvasElement, context: web_sys::CanvasRenderingContext2d) -> Self {
        Self { canvas, context }
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
        self.context.set_fill_style(&JsValue::from_str(color));
    }

    fn set_stroke_style(&mut self, color: &str) {
        self.context.set_stroke_style(&JsValue::from_str(color));
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
        let width = match self.context.measure_text(text) {
            Ok(metrics) => metrics.width(),
            Err(_) => text.len() as f64 * 8.0,
        };
        TextMetrics {
            width,
            height: 14.0,
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
        let svg_config =
            merge_svg_config(&runtime.svg, &overrides.svg, overrides.theme.as_deref())?;
        let canvas_config = merge_canvas_config(&runtime.canvas, &overrides.canvas);

        Ok(Self {
            canvas,
            context,
            svg_config,
            canvas_config,
            destroyed: false,
        })
    }

    pub fn render(&mut self, input: &str, config: Option<JsValue>) -> Result<JsValue, JsValue> {
        self.ensure_alive()?;

        let overrides: RuntimeInitConfig = parse_js_value_or_default(config)?;
        let next_svg =
            merge_svg_config(&self.svg_config, &overrides.svg, overrides.theme.as_deref())?;
        let next_canvas = merge_canvas_config(&self.canvas_config, &overrides.canvas);

        let parsed = parse(input);
        let svg = render_svg_with_config(&parsed.ir, &next_svg);

        let mut web_canvas = WebCanvas2dContext::new(self.canvas.clone(), self.context.clone());
        let canvas_result = render_to_canvas(&parsed.ir, &mut web_canvas, &next_canvas);

        self.svg_config = next_svg;
        self.canvas_config = next_canvas;

        let output = DiagramRenderOutput::new(svg, &parsed, &canvas_result);
        to_js_value(&output)
    }

    #[wasm_bindgen(js_name = setTheme)]
    pub fn set_theme(&mut self, theme: &str) -> Result<(), JsValue> {
        self.ensure_alive()?;
        let overrides = SvgConfigOverrides {
            theme: Some(theme.to_string()),
            ..SvgConfigOverrides::default()
        };
        self.svg_config = merge_svg_config(&self.svg_config, &overrides, None)?;
        Ok(())
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

    pub fn destroy(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::{SvgConfigOverrides, ThemePreset, merge_svg_config, render};
    use fm_render_svg::SvgRenderConfig;

    #[test]
    fn render_returns_svg_and_type() {
        let output = render("flowchart LR\nA-->B");
        assert!(output.svg.starts_with("<svg"));
        assert_eq!(output.detected_type, "Flowchart");
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
}
