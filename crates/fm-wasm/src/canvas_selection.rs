//! Browser selection over the exact IR, layout and viewport retained by `Diagram` (bd-vmk).
//! Geometry and gesture policy remain in fm-render-canvas; this module owns only wire formats,
//! input limits and the HTML/OffscreenCanvas lifetime boundary. Selection never activates links.

use fm_render_canvas::interaction::CanvasSession;
use fm_render_canvas::selection::MAX_LASSO_POINTS;
use fm_render_canvas::{NavigationDirection, SelectionContainment, SelectionMode};
use serde::Serialize;

#[cfg(target_arch = "wasm32")]
use fm_render_canvas::selection::SelectionRegion;
#[cfg(target_arch = "wasm32")]
use fm_render_canvas::SelectionBox;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue, prelude::wasm_bindgen};
#[cfg(target_arch = "wasm32")]
use super::{Diagram, WebCanvas2dContext, WasmHitRegion, js_error, to_js_value};

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectionState {
    selected_node_ids: Vec<String>,
    primary_node_id: Option<String>,
    // u64 must not become a JS number: distinct gestures above 2^53 must stay distinct.
    active_gesture: Option<String>,
    frame_available: bool,
}

impl SelectionState {
    fn from_session(session: &CanvasSession, size: [f64; 2]) -> Self {
        let frame_available = session.last_result(size).is_some_and(|result| {
            let viewport = result.viewport;
            viewport.zoom.is_finite()
                && viewport.zoom > 0.0
                && viewport.offset_x.is_finite()
                && viewport.offset_y.is_finite()
        });
        Self {
            selected_node_ids: session.selected_node_ids().map(str::to_owned).collect(),
            primary_node_id: session.selected_node_id().map(str::to_owned),
            active_gesture: session.active_selection_gesture().map(|token| token.to_string()),
            frame_available,
        }
    }
}

fn selection_mode(value: Option<&str>) -> Result<SelectionMode, &'static str> {
    match value {
        None | Some("replace") => Ok(SelectionMode::Replace),
        Some("add") => Ok(SelectionMode::Add),
        Some("remove") => Ok(SelectionMode::Remove),
        Some("toggle") => Ok(SelectionMode::Toggle),
        _ => Err("selection mode must be replace, add, remove or toggle"),
    }
}

fn containment(value: Option<&str>) -> Result<SelectionContainment, &'static str> {
    match value {
        None | Some("intersects") => Ok(SelectionContainment::Intersects),
        Some("contains") => Ok(SelectionContainment::Contains),
        _ => Err("selection containment must be intersects or contains"),
    }
}

fn navigation(value: &str) -> Result<NavigationDirection, &'static str> {
    match value {
        "left" => Ok(NavigationDirection::Left),
        "right" => Ok(NavigationDirection::Right),
        "up" => Ok(NavigationDirection::Up),
        "down" => Ok(NavigationDirection::Down),
        _ => Err("selection direction must be left, right, up or down"),
    }
}

fn gesture_token(value: &str) -> Result<u64, &'static str> {
    if value.is_empty()
        || value.len() > 20
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("selection token must be a canonical positive u64 decimal string");
    }
    value.parse().map_err(|_| "selection token exceeds u64")
}

fn valid_lasso_length(length: usize) -> bool {
    (6..=MAX_LASSO_POINTS * 2).contains(&length) && length.is_multiple_of(2)
}

fn lasso_points(values: &[f64]) -> Result<Vec<(f64, f64)>, &'static str> {
    if !valid_lasso_length(values.len()) {
        return Err("lasso needs 3 to 4096 x/y pairs in a Float64Array");
    }
    if values.iter().any(|value| !value.is_finite() || value.abs() > 1.0e100) {
        return Err("lasso coordinates must be finite with magnitude at most 1e100");
    }
    // No polygon interpretation here: repeated points, closure and even-odd fill are native.
    let (points, _) = values.as_chunks::<2>();
    Ok(points.iter().map(|&[x, y]| (x, y)).collect())
}

#[cfg(target_arch = "wasm32")]
impl Diagram {
    /// Read the live backing store, not the dimensions captured by the constructor.
    pub(super) fn sync_canvas_size(&mut self) -> [f64; 2] {
        let (width, height) = if let Some(canvas) = &self.canvas {
            (canvas.width(), canvas.height())
        } else if let Some(canvas) = &self.offscreen_canvas {
            (canvas.width(), canvas.height())
        } else {
            (self.canvas_width, self.canvas_height)
        };
        if width != self.canvas_width || height != self.canvas_height {
            self.session.invalidate();
            self.canvas_width = width;
            self.canvas_height = height;
        }
        [f64::from(width), f64::from(height)]
    }

    pub(super) fn redraw_canvas(&mut self) -> Option<fm_render_canvas::CanvasRenderResult> {
        self.sync_canvas_size();
        if self.canvas_width == 0 || self.canvas_height == 0 {
            self.session.invalidate();
            return None;
        }
        let mut context = WebCanvas2dContext::new(
            self.canvas_width,
            self.canvas_height,
            self.context.clone(),
        );
        self.session.redraw(&mut context).cloned()
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl Diagram {
    /// Redraw retained geometry and current selection. No parse or layout is performed.
    /// Returns false before the first render or while the backing store has zero size.
    pub fn redraw(&mut self) -> Result<bool, JsValue> {
        self.ensure_alive()?;
        Ok(self.redraw_canvas().is_some())
    }

    /// Revoke picking and roll back an in-progress drag after an external canvas clear/reset.
    /// Call even when assigning the SAME width/height: that clears pixels without a size change.
    /// Geometry and committed selection remain available to redraw().
    #[wasm_bindgen(js_name = invalidateFrame)]
    pub fn invalidate_frame(&mut self) -> Result<(), JsValue> {
        self.ensure_alive()?;
        self.session.invalidate();
        Ok(())
    }

    /// Stable author IDs, primary keyboard anchor, active token and usable-frame availability.
    #[wasm_bindgen(js_name = selectionState)]
    pub fn selection_state(&mut self) -> Result<JsValue, JsValue> {
        self.ensure_alive()?;
        let size = self.sync_canvas_size();
        to_js_value(&SelectionState::from_session(&self.session, size))
    }

    /// Select a placed author ID. An unknown ID leaves selection and active gestures unchanged.
    #[wasm_bindgen(js_name = selectNode)]
    pub fn select_node(&mut self, id: &str, mode: Option<String>) -> Result<bool, JsValue> {
        self.ensure_alive()?;
        let mode = selection_mode(mode.as_deref()).map_err(js_error)?;
        self.sync_canvas_size();
        Ok(self.session.select_node_with_mode(id, mode))
    }

    /// Modifier click in backing-store pixels. Convert CSS coordinates once in the host.
    #[wasm_bindgen(js_name = selectAt)]
    pub fn select_at(&mut self, x: f64, y: f64, mode: Option<String>) -> Result<bool, JsValue> {
        self.ensure_alive()?;
        let mode = selection_mode(mode.as_deref()).map_err(js_error)?;
        let size = self.sync_canvas_size();
        Ok(self.session.select_at_with_mode(x, y, size, mode))
    }

    /// Move the primary keyboard selection, optionally extending the existing selected set.
    #[wasm_bindgen(js_name = navigateSelection)]
    pub fn navigate_selection(
        &mut self,
        direction: &str,
        extend: Option<bool>,
    ) -> Result<bool, JsValue> {
        self.ensure_alive()?;
        let direction = navigation(direction).map_err(js_error)?;
        self.sync_canvas_size();
        Ok(self.session.navigate_selection_with_extension(direction, extend.unwrap_or(false)))
    }

    /// Clear selection and revoke the current drag. A delayed cancellation cannot restore it.
    #[wasm_bindgen(js_name = clearSelection)]
    pub fn clear_selection(&mut self) -> Result<bool, JsValue> {
        self.ensure_alive()?;
        Ok(self.session.clear_selection())
    }

    /// Begin a transactional box/lasso preview. Returns a decimal token or undefined if no
    /// usable draw is available, another gesture owns the session, or tokens are exhausted.
    #[wasm_bindgen(js_name = beginSelectionGesture)]
    pub fn begin_selection_gesture(
        &mut self,
        mode: Option<String>,
        policy: Option<String>,
    ) -> Result<Option<String>, JsValue> {
        self.ensure_alive()?;
        let mode = selection_mode(mode.as_deref()).map_err(js_error)?;
        let policy = containment(policy.as_deref()).map_err(js_error)?;
        let size = self.sync_canvas_size();
        Ok(self.session.begin_selection_gesture(size, mode, policy).map(|token| token.to_string()))
    }

    /// Preview a marquee from its pointer-down selection, not the previous preview.
    /// Captured pointers may be outside the canvas. Wrong/retired tokens have no side effects.
    #[wasm_bindgen(js_name = previewSelectionBox)]
    pub fn preview_selection_box(
        &mut self,
        token: &str,
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
    ) -> Result<bool, JsValue> {
        self.ensure_alive()?;
        let token = gesture_token(token).map_err(js_error)?;
        if self.session.active_selection_gesture() != Some(token) {
            return Ok(false);
        }
        let size = self.sync_canvas_size();
        Ok(self.session.preview_selection_gesture(
            token,
            SelectionRegion::Box(SelectionBox {
                start: (start_x, start_y),
                end: (end_x, end_y),
            }),
            size,
        ))
    }

    /// Preview an even-odd lasso from interleaved backing-store x/y coordinates.
    /// Length is bounded BEFORE copying out of JS memory. Invalid input throws without changing
    /// selection; polygon degeneracy is handled by the native selector as an unchanged preview.
    #[wasm_bindgen(js_name = previewSelectionLasso)]
    pub fn preview_selection_lasso(
        &mut self,
        token: &str,
        values: &js_sys::Float64Array,
    ) -> Result<bool, JsValue> {
        self.ensure_alive()?;
        let token = gesture_token(token).map_err(js_error)?;
        if self.session.active_selection_gesture() != Some(token) {
            return Ok(false);
        }
        let raw: &JsValue = values.as_ref();
        if !raw.is_instance_of::<js_sys::Float64Array>() {
            return Err(js_error("lasso coordinates must be a Float64Array"));
        }
        let count = values.length();
        let length = usize::try_from(count).map_err(|_| js_error("lasso is too large"))?;
        if !valid_lasso_length(length) {
            return Err(js_error("lasso needs 3 to 4096 x/y pairs in a Float64Array"));
        }
        // Do not re-read a caller-owned length inside to_vec(): a typed-array subclass may
        // override its length getter. Only the admitted number of scalar reads is performed.
        let coordinates: Vec<f64> = (0..count).map(|index| values.get_index(index)).collect();
        let points = lasso_points(&coordinates).map_err(js_error)?;
        let size = self.sync_canvas_size();
        Ok(self.session.preview_selection_gesture(token, SelectionRegion::Lasso(&points), size))
    }

    /// Commit the latest accepted preview. True means committed, not selection-changed.
    #[wasm_bindgen(js_name = commitSelectionGesture)]
    pub fn commit_selection_gesture(&mut self, token: &str) -> Result<bool, JsValue> {
        self.ensure_alive()?;
        let token = gesture_token(token).map_err(js_error)?;
        if self.session.active_selection_gesture() != Some(token) {
            return Ok(false);
        }
        let size = self.sync_canvas_size();
        Ok(self.session.commit_selection_gesture(token, size))
    }

    /// Roll back the matching preview. True means selection changed and should be redrawn.
    #[wasm_bindgen(js_name = cancelSelectionGesture)]
    pub fn cancel_selection_gesture(&mut self, token: &str) -> Result<bool, JsValue> {
        self.ensure_alive()?;
        let token = gesture_token(token).map_err(js_error)?;
        Ok(self.session.cancel_selection_gesture(token))
    }

    /// Inspect the topmost node's interaction metadata without following a URL or calling JS.
    #[wasm_bindgen(js_name = hitTestInteraction)]
    pub fn hit_test_interaction(&mut self, x: f64, y: f64) -> Result<JsValue, JsValue> {
        self.ensure_alive()?;
        let size = self.sync_canvas_size();
        let region = self.session.hit_test_interaction(x, y, size).map(WasmHitRegion::from);
        to_js_value(&region)
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(typescript_custom_section)]
const SELECTION_TS: &str = r#"
/** selectionState(): IDs are author IDs, not layout indices. Absent optionals are nullish. */
export interface DiagramSelectionState {
  selectedNodeIds: string[];
  primaryNodeId?: string | null;
  activeGesture?: string | null;
  frameAvailable: boolean;
}
export type DiagramSelectionMode = "replace" | "add" | "remove" | "toggle";
export type DiagramSelectionContainment = "intersects" | "contains";
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use fm_render_canvas::{CanvasRenderConfig, DrawOperation, MockCanvas2dContext};
    use fm_render_canvas::selection::{SelectionBox, SelectionRegion};
    use std::sync::Arc;

    fn drawn_session() -> (CanvasSession, MockCanvas2dContext) {
        let ir = fm_parser::parse("flowchart LR\nA[Alpha] --> B[Beta]\n").ir;
        assert_eq!(ir.nodes.len(), 2);
        let layout = Arc::new(fm_layout::layout_diagram(&ir));
        let mut session = CanvasSession::default();
        session.set_diagram(ir, layout, CanvasRenderConfig::default());
        let mut context = MockCanvas2dContext::new(800.0, 400.0);
        session.redraw(&mut context).expect("rendered frame");
        (session, context)
    }

    #[test]
    fn selection_wire_policies_are_exact_and_have_explicit_defaults() {
        assert_eq!(selection_mode(None), Ok(SelectionMode::Replace));
        for (name, expected) in [
            ("replace", SelectionMode::Replace), ("add", SelectionMode::Add),
            ("remove", SelectionMode::Remove), ("toggle", SelectionMode::Toggle),
        ] {
            assert_eq!(selection_mode(Some(name)), Ok(expected));
        }
        assert_eq!(containment(None), Ok(SelectionContainment::Intersects));
        assert_eq!(containment(Some("contains")), Ok(SelectionContainment::Contains));
        for bad in ["", "ADD", " replace", "unknown"] {
            assert!(selection_mode(Some(bad)).is_err());
            assert!(containment(Some(bad)).is_err());
        }
        for (name, expected) in [
            ("left", NavigationDirection::Left), ("right", NavigationDirection::Right),
            ("up", NavigationDirection::Up), ("down", NavigationDirection::Down),
        ] {
            assert_eq!(navigation(name), Ok(expected));
        }
        assert!(navigation("ArrowRight").is_err());
    }

    #[test]
    fn tokens_preserve_the_full_u64_range_without_number_rounding() {
        for token in [1_u64, 9_007_199_254_740_992, 9_007_199_254_740_993, u64::MAX] {
            assert_eq!(gesture_token(&token.to_string()), Ok(token));
        }
        for bad in ["", "0", "01", "-1", "+1", " 1", "1\n", "1.0", "1e3", "１",
            "18446744073709551616", "999999999999999999999"] {
            assert!(gesture_token(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn lasso_decoder_bounds_allocation_and_preserves_pair_order() {
        let coordinates = [-20.5, 1.0, 0.0, 30.0, 40.0, 1.0, -20.5, 1.0];
        assert_eq!(lasso_points(&coordinates).unwrap(),
            [(-20.5, 1.0), (0.0, 30.0), (40.0, 1.0), (-20.5, 1.0)]);
        for length in [0, 4, 5, 7, MAX_LASSO_POINTS * 2 + 1, MAX_LASSO_POINTS * 2 + 2] {
            assert!(!valid_lasso_length(length));
            assert!(lasso_points(&vec![0.0; length]).is_err());
        }
        assert!(valid_lasso_length(MAX_LASSO_POINTS * 2));
        assert_eq!(lasso_points(&vec![0.0; MAX_LASSO_POINTS * 2]).unwrap().len(), MAX_LASSO_POINTS);
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1.1e100, -1.1e100] {
            assert!(lasso_points(&[0.0, 0.0, 1.0, 1.0, bad, 2.0]).is_err());
        }
        assert!(lasso_points(&[0.0, 0.0, 1.0e100, 1.0, 2.0, -1.0e100]).is_ok());
    }

    #[test]
    fn state_is_camel_case_with_sorted_author_ids_and_a_string_token() {
        let (mut session, _) = drawn_session();
        session.select_node("B");
        session.select_node_with_mode("A", selection_mode(Some("add")).unwrap());
        let token = session.begin_selection_gesture([800.0, 400.0],
            selection_mode(None).unwrap(), containment(None).unwrap()).unwrap();
        let state = SelectionState::from_session(&session, [800.0, 400.0]);
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["selectedNodeIds"], serde_json::json!(["A", "B"]));
        assert_eq!(json["primaryNodeId"], "A");
        assert_eq!(json["activeGesture"], token.to_string());
        assert_eq!(json["frameAvailable"], true);
        assert!(json.get("selected_node_ids").is_none());
        assert!(!SelectionState::from_session(&session, [801.0, 400.0]).frame_available);
    }

    #[test]
    fn decoded_lasso_preview_rolls_back_and_cannot_reuse_a_retired_token() {
        let (mut session, _) = drawn_session();
        session.select_node("B");
        let token = session.begin_selection_gesture([800.0, 400.0],
            selection_mode(Some("replace")).unwrap(), containment(None).unwrap()).unwrap();
        let wire = token.to_string();
        let points = lasso_points(&[-1.0, -1.0, 801.0, -1.0, 801.0, 401.0, -1.0, 401.0]).unwrap();
        assert!(session.preview_selection_gesture(gesture_token(&wire).unwrap(),
            SelectionRegion::Lasso(&points), [800.0, 400.0]));
        assert_eq!(session.selected_node_ids().collect::<Vec<_>>(), ["A", "B"]);
        assert!(session.cancel_selection_gesture(gesture_token(&wire).unwrap()));
        assert_eq!(session.selected_node_ids().collect::<Vec<_>>(), ["B"]);
        assert_eq!(session.selected_node_id(), Some("B"));
        let next = session.begin_selection_gesture([800.0, 400.0],
            SelectionMode::Replace, SelectionContainment::Intersects).unwrap();
        assert_ne!(next, token);
        assert!(!session.commit_selection_gesture(gesture_token(&wire).unwrap(), [800.0, 400.0]));
        assert_eq!(session.active_selection_gesture(), Some(next));
    }

    #[test]
    fn selection_redraw_keeps_content_and_clear_restores_the_original_draw() {
        let (mut session, mut context) = drawn_session();
        let original = context.operations().to_vec();
        session.select_node("A");
        session.navigate_selection_with_extension(navigation("right").unwrap(), true);
        assert_eq!(session.selected_node_ids().collect::<Vec<_>>(), ["A", "B"]);
        context.clear();
        session.redraw(&mut context).unwrap();
        assert_eq!(&context.operations()[..original.len()], original.as_slice());
        assert_eq!(context.operations().iter().filter(|op| matches!(op, DrawOperation::StrokeRect(..))).count(),
            original.iter().filter(|op| matches!(op, DrawOperation::StrokeRect(..))).count() + 4);
        session.clear_selection();
        context.clear();
        session.redraw(&mut context).unwrap();
        assert_eq!(context.operations(), original.as_slice());
    }

    #[test]
    fn invalidation_reports_no_frame_and_keeps_committed_not_preview_selection() {
        let (mut session, _) = drawn_session();
        session.select_node("B");
        let token = session.begin_selection_gesture([800.0, 400.0],
            SelectionMode::Replace, SelectionContainment::Intersects).unwrap();
        session.preview_selection_gesture(token, SelectionRegion::Box(SelectionBox {
            start: (-1.0, -1.0), end: (801.0, 401.0),
        }), [800.0, 400.0]);
        assert_eq!(session.selected_node_ids().count(), 2);
        session.invalidate();
        let state = SelectionState::from_session(&session, [800.0, 400.0]);
        assert_eq!(state.selected_node_ids, ["B"]);
        assert_eq!(state.primary_node_id.as_deref(), Some("B"));
        assert_eq!(state.active_gesture, None);
        assert!(!state.frame_available);
        assert!(!session.commit_selection_gesture(token, [800.0, 400.0]));
        let empty = SelectionState::from_session(&CanvasSession::default(), [800.0, 400.0]);
        assert!(empty.selected_node_ids.is_empty());
        assert_eq!(empty.primary_node_id, None);
        assert!(!empty.frame_available);
    }
}
