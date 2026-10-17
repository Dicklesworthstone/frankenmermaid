//! Hit regions: the channel through which a raster surface can express `click` (bd-2u0.2).
//!
//! bd-bk7h measured that `fm-render-canvas/src` contained ZERO references to `href`, `callback` or
//! `tooltip`, and drew the right conclusion: that is not a missing tooltip, it is a design boundary.
//! An immediate-mode raster surface has no element to hang an attribute on, so it cannot carry an
//! interaction the way SVG carries `title=` and `<a href>`. What it CAN do is tell the embedding
//! application where each interactive node landed and what the author attached to it, and let the
//! host own the pointer.
//!
//! That is this module. It is the missing half of `click` for every raster backend — Canvas2D today
//! and the WebGPU path tomorrow, since both draw the same `DiagramLayout` and neither can be asked
//! to grow a DOM.
//!
//! **Layout coordinates, not screen coordinates.** A region is reported in the same space the
//! renderer draws in, so the host applies the SAME viewport transform it already uses for drawing.
//! Baking a transform in here would silently assume a viewport this module cannot see, and the
//! regions would drift from the picture the moment the user panned or zoomed.

use fm_core::MermaidDiagramIr;
use fm_layout::{DiagramLayout, LayoutRect};
use std::sync::Arc;

/// One interactive node's clickable area and what the author attached to it.
///
/// Every field the parser records for `click` is carried, because dropping one here would recreate
/// exactly the parsed-stored-drawn-by-nothing defect this module exists to close — bd-bk7h found
/// `tooltip` dead in three renderers, and bd-jgco and bd-jerh are the same shape.
// `PartialEq` but NOT `Eq`: `bounds` carries `f32`, so equality is partial by construction and
// claiming otherwise would be a lie the compiler happens not to catch on the other fields.
#[derive(Debug, Clone, PartialEq)]
pub struct HitRegion {
    /// Index into [`MermaidDiagramIr::nodes`].
    pub node_index: usize,
    /// The author's node id — the same string the SVG backend puts in `data-id`, so a host (or a
    /// test) can join the two backends on it.
    pub node_id: String,
    /// Clickable area in LAYOUT coordinates.
    pub bounds: LayoutRect,
    /// `click <id> href "url"`.
    pub href: Option<String>,
    /// `_self` / `_blank` / `_parent` / `_top`.
    ///
    /// `None` means the author declared none, NOT that there is no target: mermaid defaults a link
    /// to `_blank` and so does fm-render-svg. The host applies that default; this reports what was
    /// written, because a module that substituted the default here would make "author asked for
    /// `_self`" and "author asked for nothing" indistinguishable downstream.
    pub link_target: Option<String>,
    /// `click <id> call fn()`.
    pub callback: Option<String>,
    /// `click <id> "url" "tooltip"` — what a browser shows on hover.
    pub tooltip: Option<String>,
}

impl HitRegion {
    /// Does this region contain a point in layout coordinates?
    ///
    /// Half-open on the far edges: a point exactly on the right or bottom edge belongs to the next
    /// region, not this one. Closed-closed would make two abutting nodes both claim their shared
    /// boundary, and which one won would depend on iteration order.
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.bounds.x
            && y >= self.bounds.y
            && x < self.bounds.x + self.bounds.width
            && y < self.bounds.y + self.bounds.height
    }
}

/// Every interactive node's region, in draw order.
///
/// ⚠️ ONLY NODES THAT CARRY AN INTERACTION. A region per node would be easier and wrong: the host
/// uses this to decide whether the pointer is over something clickable, so returning a region for
/// every box makes the whole diagram report a hit and pushes the filtering back onto the caller —
/// which is the work this function exists to do.
///
/// Order follows `layout.nodes`, which is draw order, so [`hit_test`] can resolve an overlap the
/// same way the renderer resolves it visually.
#[must_use]
pub fn hit_regions(ir: &MermaidDiagramIr, layout: &DiagramLayout) -> Vec<HitRegion> {
    layout
        .nodes
        .iter()
        .filter_map(|placed| {
            let node = ir.nodes.get(placed.node_index)?;
            let interaction = node.interaction.as_ref()?;
            // An `icon` is decoration, not an interaction: a node carrying only an icon is not
            // clickable, and reporting it would make every icon-bearing diagram look interactive.
            if interaction.href.is_none()
                && interaction.callback.is_none()
                && interaction.tooltip.is_none()
            {
                return None;
            }
            Some(HitRegion {
                node_index: placed.node_index,
                node_id: node.id.to_string(),
                bounds: placed.bounds,
                href: interaction.href.clone(),
                link_target: interaction.link_target.clone(),
                callback: interaction.callback.clone(),
                tooltip: interaction.tooltip.clone(),
            })
        })
        .collect()
}

/// The region under a point, or `None`.
///
/// Returns the LAST match in draw order, which is the one drawn on top and therefore the one a user
/// believes they clicked. Returning the first would hand the pointer to whatever happens to be
/// underneath — correct only when nothing overlaps, which is not a property this module can assume
/// of a layout it did not compute.
#[must_use]
pub fn hit_test(regions: &[HitRegion], x: f32, y: f32) -> Option<&HitRegion> {
    regions.iter().rev().find(|region| region.contains(x, y))
}

/// A spatial navigation direction, independent of the diagram's authored flow direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationDirection {
    /// Move toward smaller layout x coordinates.
    Left,
    /// Move toward larger layout x coordinates.
    Right,
    /// Move toward smaller layout y coordinates.
    Up,
    /// Move toward larger layout y coordinates.
    Down,
}

/// Selection for a raster diagram, including nodes without a `click` directive.
///
/// Selection is NOT activation: moving focus never follows a URL or invokes a callback. Keep using
/// [`hit_regions`] for those actions. The author's id, rather than an array index or cached bounds,
/// is retained so inserting nodes and recomputing layout do not silently select a different node.
/// Pass the IR and layout that were actually rendered to every operation. After replacing either,
/// call [`Self::reconcile`] to clear a selection whose node has disappeared.
///
/// All mutating methods return whether the selected id changed, so hosts can avoid redundant
/// redraws. No DOM, GPU, parser, or timer is required by the selection implementation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeSelection {
    selected_id: Option<String>,
}

impl NodeSelection {
    /// The selected author's node id, or `None`.
    #[must_use]
    pub fn selected_node_id(&self) -> Option<&str> {
        self.selected_id.as_deref()
    }

    /// Clear selection. Returns false when it was already empty.
    pub fn clear(&mut self) -> bool {
        self.selected_id.take().is_some()
    }

    /// Select an existing, placed node. An unknown id leaves the current selection unchanged.
    pub fn select_node(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        node_id: &str,
    ) -> bool {
        if selectable_nodes(ir, layout).any(|node| node.id == node_id) {
            self.replace(Some(node_id))
        } else {
            false
        }
    }

    /// The current selection's bounds in the supplied layout, never stale cached geometry.
    #[must_use]
    pub fn selected_bounds(
        &self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
    ) -> Option<LayoutRect> {
        let id = self.selected_node_id()?;
        selectable_nodes(ir, layout)
            .rev()
            .find(|node| node.id == id)
            .map(|node| node.bounds)
    }

    /// Clear focus if its node no longer has usable geometry in the current diagram.
    pub fn reconcile(&mut self, ir: &MermaidDiagramIr, layout: &DiagramLayout) -> bool {
        if self.selected_id.is_some() && self.selected_bounds(ir, layout).is_none() {
            self.clear()
        } else {
            false
        }
    }

    /// Select the topmost node box under a layout-space point; empty space clears selection.
    ///
    /// Bounds are half-open on the right/bottom, just like [`HitRegion::contains`]. Non-finite
    /// points are ignored, not treated as clicks on empty space. Picking intentionally uses node
    /// boxes (including their label area), not shape contours. Invalid/empty geometry is skipped.
    pub fn select_at(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        x: f64,
        y: f64,
    ) -> bool {
        if !x.is_finite() || !y.is_finite() {
            return false;
        }
        let target = selectable_nodes(ir, layout)
            .rev()
            .find(|node| node.contains(x, y));
        self.replace(target.map(|node| node.id))
    }

    /// Pick using the exact viewport returned by the renderer, after pan/zoom.
    ///
    /// `x`/`y` use the same canvas-pixel units as `viewport.canvas_width`/`canvas_height`.
    /// A browser host must convert CSS pointer coordinates to those units when its backing-store
    /// size differs. Do not apply the device-pixel ratio a second time here.
    pub fn select_at_canvas(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        viewport: &crate::Viewport,
        x: f64,
        y: f64,
    ) -> bool {
        if !viewport.zoom.is_finite()
            || viewport.zoom <= 0.0
            || !viewport.offset_x.is_finite()
            || !viewport.offset_y.is_finite()
        {
            return false;
        }
        let (diagram_x, diagram_y) = viewport.canvas_to_diagram(x, y);
        self.select_at(ir, layout, diagram_x, diagram_y)
    }

    /// Move to a node strictly in the requested half-plane, without wrapping at the boundary.
    ///
    /// Nodes whose boxes overlap the current row/column are preferred, then nearest center,
    /// then smallest forward distance, then author's id. This keeps arrow keys in a lane when a
    /// nearer diagonal node belongs to a different branch. Ties never depend on layout iteration
    /// order. With no valid selection, focus starts at the topmost, then leftmost node center.
    /// Arithmetic uses f64 even though layout stores f32, avoiding overflow for finite layouts.
    pub fn navigate(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        direction: NavigationDirection,
    ) -> bool {
        let current = selectable_nodes(ir, layout)
            .rev()
            .find(|node| Some(node.id) == self.selected_node_id());
        let Some(current) = current else {
            let first = selectable_nodes(ir, layout).min_by(|left, right| {
                let (lx, ly) = left.center();
                let (rx, ry) = right.center();
                ly.total_cmp(&ry)
                    .then_with(|| lx.total_cmp(&rx))
                    .then_with(|| left.id.cmp(right.id))
                    .then_with(|| left.node_index.cmp(&right.node_index))
            });
            return self.replace(first.map(|node| node.id));
        };
        let target = selectable_nodes(ir, layout)
            .filter(|node| node.id != current.id)
            .filter_map(|node| {
                navigation_score(current, node, direction).map(|score| (node, score))
            })
            .min_by(|(left, ls), (right, rs)| {
                ls.0.cmp(&rs.0)
                    .then_with(|| ls.1.total_cmp(&rs.1))
                    .then_with(|| ls.2.total_cmp(&rs.2))
                    .then_with(|| left.id.cmp(right.id))
                    .then_with(|| left.node_index.cmp(&right.node_index))
            });
        target.is_some_and(|(node, _)| self.replace(Some(node.id)))
    }

    fn replace(&mut self, id: Option<&str>) -> bool {
        if self.selected_node_id() == id {
            return false;
        }
        self.selected_id = id.map(str::to_owned);
        true
    }
}

#[derive(Clone, Copy)]
struct SelectableNode<'a> {
    id: &'a str,
    node_index: usize,
    bounds: LayoutRect,
}

impl SelectableNode<'_> {
    fn contains(self, x: f64, y: f64) -> bool {
        x >= f64::from(self.bounds.x)
            && y >= f64::from(self.bounds.y)
            && x < f64::from(self.bounds.x) + f64::from(self.bounds.width)
            && y < f64::from(self.bounds.y) + f64::from(self.bounds.height)
    }

    fn center(self) -> (f64, f64) {
        (
            f64::from(self.bounds.x) + f64::from(self.bounds.width) / 2.0,
            f64::from(self.bounds.y) + f64::from(self.bounds.height) / 2.0,
        )
    }
}

fn selectable_nodes<'a>(
    ir: &'a MermaidDiagramIr,
    layout: &'a DiagramLayout,
) -> impl DoubleEndedIterator<Item = SelectableNode<'a>> {
    layout.nodes.iter().filter_map(move |placed| {
        let node = ir.nodes.get(placed.node_index)?;
        let bounds = placed.bounds;
        (bounds.x.is_finite()
            && bounds.y.is_finite()
            && bounds.width.is_finite()
            && bounds.height.is_finite()
            && bounds.width > 0.0
            && bounds.height > 0.0)
            .then_some(SelectableNode {
                id: node.id.as_str(),
                node_index: placed.node_index,
                bounds,
            })
    })
}

fn navigation_score(
    current: SelectableNode<'_>,
    candidate: SelectableNode<'_>,
    direction: NavigationDirection,
) -> Option<(u8, f64, f64)> {
    let (cx, cy) = current.center();
    let (nx, ny) = candidate.center();
    let (forward, cross, low, high, next_low, next_high) = match direction {
        NavigationDirection::Left | NavigationDirection::Right => (
            if direction == NavigationDirection::Right {
                nx - cx
            } else {
                cx - nx
            },
            ny - cy,
            f64::from(current.bounds.y),
            f64::from(current.bounds.y) + f64::from(current.bounds.height),
            f64::from(candidate.bounds.y),
            f64::from(candidate.bounds.y) + f64::from(candidate.bounds.height),
        ),
        NavigationDirection::Up | NavigationDirection::Down => (
            if direction == NavigationDirection::Down {
                ny - cy
            } else {
                cy - ny
            },
            nx - cx,
            f64::from(current.bounds.x),
            f64::from(current.bounds.x) + f64::from(current.bounds.width),
            f64::from(candidate.bounds.x),
            f64::from(candidate.bounds.x) + f64::from(candidate.bounds.width),
        ),
    };
    if forward <= 0.0 {
        return None;
    }
    let outside_beam = u8::from(low.max(next_low) >= high.min(next_high));
    Some((outside_beam, forward * forward + cross * cross, forward))
}

/// A retained Canvas2D diagram: geometry, selection and the last actual draw stay together.
///
/// Supply a matched IR/layout pair to [`Self::set_diagram`], then [`Self::redraw`]. Redrawing after
/// a resize or selection change never parses or lays out again. Picking uses the viewport from
/// that draw, including fit, padding and origin normalization, NOT raw layout coordinates.
///
/// Pointer coordinates and `canvas_size` are backing-store pixels. Browser hosts convert CSS
/// coordinates once and pass the canvas's current width/height. A resized canvas cannot be picked
/// until redrawn. Hosts must also call [`Self::invalidate`] when clearing/replacing its pixels
/// without changing dimensions (including assigning the same width). The session owns no DOM and
/// cannot observe external canvas writes. Selection methods change state; redraw to show it.
#[derive(Debug, Default)]
pub struct CanvasSession {
    frame: Option<CanvasFrame>,
    selection: NodeSelection,
    selection_style: crate::CanvasSelectionStyle,
    last_result: Option<crate::CanvasRenderResult>,
}

#[derive(Debug)]
struct CanvasFrame {
    ir: MermaidDiagramIr,
    layout: Arc<DiagramLayout>,
    config: crate::CanvasRenderConfig,
}

impl CanvasSession {
    /// Replace the diagram without copying its layout. Retain selection by author ID when valid.
    /// The old pointer map becomes unusable immediately, before any new drawing occurs.
    pub fn set_diagram(
        &mut self,
        ir: MermaidDiagramIr,
        layout: Arc<DiagramLayout>,
        config: crate::CanvasRenderConfig,
    ) {
        self.selection.reconcile(&ir, &layout);
        self.frame = Some(CanvasFrame { ir, layout, config });
        self.invalidate();
    }

    /// Draw the retained diagram at the context's CURRENT size and refresh the pointer map.
    /// Returns `None` before a diagram is supplied; in that case the context is untouched.
    pub fn redraw<C: crate::Canvas2dContext>(
        &mut self,
        context: &mut C,
    ) -> Option<&crate::CanvasRenderResult> {
        let frame = self.frame.as_ref()?;
        let result = crate::render_to_canvas_with_selection(
            &frame.ir,
            &frame.layout,
            context,
            &frame.config,
            &mut self.selection,
            &self.selection_style,
        );
        Some(self.last_result.insert(result))
    }

    /// Forget pixels and their pointer map, but keep geometry and selection for a later redraw.
    pub fn invalidate(&mut self) {
        self.last_result = None;
    }

    /// Release retained IR, layout, click metadata and selection. Does not clear a host's canvas.
    pub fn clear(&mut self) {
        self.frame = None;
        self.last_result = None;
        self.selection.clear();
    }

    /// The last completed draw, provided its dimensions still match the host's canvas.
    #[must_use]
    pub fn last_result(&self, canvas_size: [f64; 2]) -> Option<&crate::CanvasRenderResult> {
        let result = self.last_result.as_ref()?;
        let viewport = result.viewport;
        (canvas_size[0].is_finite()
            && canvas_size[1].is_finite()
            && canvas_size[0] > 0.0
            && canvas_size[1] > 0.0
            && canvas_size[0] == viewport.canvas_width
            && canvas_size[1] == viewport.canvas_height)
            .then_some(result)
    }

    /// Topmost node box at a canvas-space point. Far boundaries are half-open, like selection.
    #[must_use]
    pub fn hit_test_node(&self, x: f64, y: f64, canvas_size: [f64; 2]) -> Option<&str> {
        let (x, y) = self.layout_point(x, y, canvas_size)?;
        self.node_at(x, y).map(|node| node.id)
    }

    /// Interaction attached to the topmost node under the pointer; never activates it.
    /// An ordinary node obscuring a linked node blocks the link rather than clicking through it.
    #[must_use]
    pub fn hit_test_interaction(
        &self,
        x: f64,
        y: f64,
        canvas_size: [f64; 2],
    ) -> Option<&HitRegion> {
        let (x, y) = self.layout_point(x, y, canvas_size)?;
        let node = self.node_at(x, y)?;
        self.last_result(canvas_size)?
            .hit_regions
            .iter()
            .rev()
            .find(|region| region.node_index == node.node_index && region.bounds == node.bounds)
    }

    /// Nearest rendered edge polyline within a tolerance measured in CANVAS pixels.
    /// Bundled-away paths and non-finite segments are excluded. Equal-distance ties use edge ID.
    #[must_use]
    pub fn hit_test_edge(
        &self,
        x: f64,
        y: f64,
        max_distance: f64,
        canvas_size: [f64; 2],
    ) -> Option<usize> {
        use fm_core::cga::{CgaLineSegment, CgaPoint};

        if !max_distance.is_finite() || max_distance < 0.0 {
            return None;
        }
        let (x, y) = self.layout_point(x, y, canvas_size)?;
        let tolerance = max_distance / self.last_result(canvas_size)?.viewport.zoom;
        if !tolerance.is_finite() {
            return None;
        }
        let point = CgaPoint::new(x, y);
        let frame = self.frame.as_ref()?;
        let mut closest: Option<(usize, f64)> = None;
        for edge in &frame.layout.edges {
            if edge.bundled {
                continue;
            }
            for points in edge.points.windows(2) {
                let start = points[0];
                let end = points[1];
                if ![start.x, start.y, end.x, end.y]
                    .iter()
                    .all(|value| value.is_finite())
                {
                    continue;
                }
                let distance = CgaLineSegment::new(
                    CgaPoint::new(f64::from(start.x), f64::from(start.y)),
                    CgaPoint::new(f64::from(end.x), f64::from(end.y)),
                )
                .distance_to_point(&point);
                if distance.is_finite()
                    && distance <= tolerance
                    && closest.is_none_or(|(index, best)| {
                        distance < best || (distance == best && edge.edge_index < index)
                    })
                {
                    closest = Some((edge.edge_index, distance));
                }
            }
        }
        closest.map(|(index, _)| index)
    }

    /// The selected author's ID. Selection does not itself follow a link or call user code.
    #[must_use]
    pub fn selected_node_id(&self) -> Option<&str> {
        self.selection.selected_node_id()
    }

    /// Select a placed node. Unknown IDs and repeated selection leave state unchanged.
    pub fn select_node(&mut self, id: &str) -> bool {
        self.frame
            .as_ref()
            .is_some_and(|frame| self.selection.select_node(&frame.ir, &frame.layout, id))
    }

    /// Select at a rendered canvas point. Empty space clears focus; invalid/stale points do not.
    pub fn select_at(&mut self, x: f64, y: f64, canvas_size: [f64; 2]) -> bool {
        let Some((x, y)) = self.layout_point(x, y, canvas_size) else {
            return false;
        };
        self.frame
            .as_ref()
            .is_some_and(|frame| self.selection.select_at(&frame.ir, &frame.layout, x, y))
    }

    /// Navigate spatially without reparsing or relayout. Returns whether focus moved.
    pub fn navigate_selection(&mut self, direction: NavigationDirection) -> bool {
        self.frame
            .as_ref()
            .is_some_and(|frame| self.selection.navigate(&frame.ir, &frame.layout, direction))
    }

    /// Clear focus, retaining the diagram.
    pub fn clear_selection(&mut self) -> bool {
        self.selection.clear()
    }

    /// Set the focus-ring appearance for the next redraw.
    pub fn set_selection_style(&mut self, style: crate::CanvasSelectionStyle) {
        self.selection_style = style;
    }

    fn layout_point(&self, x: f64, y: f64, canvas_size: [f64; 2]) -> Option<(f64, f64)> {
        let viewport = self.last_result(canvas_size)?.viewport;
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= canvas_size[0]
            || y >= canvas_size[1]
            || !viewport.zoom.is_finite()
            || viewport.zoom <= 0.0
            || !viewport.offset_x.is_finite()
            || !viewport.offset_y.is_finite()
        {
            return None;
        }
        let point = viewport.canvas_to_diagram(x, y);
        (point.0.is_finite() && point.1.is_finite()).then_some(point)
    }

    fn node_at(&self, x: f64, y: f64) -> Option<SelectableNode<'_>> {
        let frame = self.frame.as_ref()?;
        selectable_nodes(&frame.ir, &frame.layout)
            .rev()
            .find(|node| node.contains(x, y))
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    fn fixture() -> (MermaidDiagramIr, DiagramLayout) {
        let ir = fm_parser::parse(
            "flowchart LR\n A[Alpha]\n B[Beta]\n C[Gamma]\n D[Delta]\n E[Epsilon]\n",
        )
        .ir;
        assert_eq!(ir.nodes.len(), 5);
        let mut layout = fm_layout::layout_diagram(&ir);
        place(&ir, &mut layout, "A", 0.0, 0.0);
        place(&ir, &mut layout, "B", 200.0, 0.0);
        place(&ir, &mut layout, "C", 50.0, 50.0);
        place(&ir, &mut layout, "D", 0.0, 200.0);
        place(&ir, &mut layout, "E", 0.0, -200.0);
        (ir, layout)
    }

    fn place(ir: &MermaidDiagramIr, layout: &mut DiagramLayout, id: &str, x: f32, y: f32) {
        let node = layout
            .nodes
            .iter_mut()
            .find(|node| ir.nodes[node.node_index].id == id)
            .unwrap();
        node.bounds = LayoutRect {
            x,
            y,
            width: 40.0,
            height: 40.0,
        };
    }

    #[test]
    fn parsed_nodes_without_click_are_selectable() {
        let (ir, layout) = fixture();
        assert!(hit_regions(&ir, &layout).is_empty());
        let mut selection = NodeSelection::default();
        assert!(selection.select_node(&ir, &layout, "B"));
        assert_eq!(selection.selected_node_id(), Some("B"));
        assert!(!selection.select_node(&ir, &layout, "B"));
        assert!(!selection.select_node(&ir, &layout, "missing"));
        assert!(selection.select_at(&ir, &layout, 20.0, 20.0));
        assert_eq!(selection.selected_node_id(), Some("A"));
    }

    #[test]
    fn directional_navigation_stays_in_lane_and_stops_at_boundary() {
        let (ir, layout) = fixture();
        let mut selection = NodeSelection::default();
        selection.select_node(&ir, &layout, "A");
        for (direction, expected) in [
            (NavigationDirection::Right, "B"),
            (NavigationDirection::Left, "A"),
            (NavigationDirection::Down, "D"),
            (NavigationDirection::Up, "A"),
            (NavigationDirection::Up, "E"),
        ] {
            assert!(selection.navigate(&ir, &layout, direction));
            assert_eq!(selection.selected_node_id(), Some(expected));
        }
        assert!(!selection.navigate(&ir, &layout, NavigationDirection::Up));
        assert_eq!(selection.selected_node_id(), Some("E"));
    }

    #[test]
    fn navigation_ties_are_independent_of_draw_order() {
        let (ir, mut layout) = fixture();
        place(&ir, &mut layout, "B", 100.0, -20.0);
        place(&ir, &mut layout, "C", 100.0, 20.0);
        let mut selection = NodeSelection::default();
        for _ in 0..2 {
            selection.select_node(&ir, &layout, "A");
            assert!(selection.navigate(&ir, &layout, NavigationDirection::Right));
            assert_eq!(selection.selected_node_id(), Some("B"));
            layout.nodes.reverse();
        }
    }

    #[test]
    fn initial_focus_uses_visual_order_and_empty_diagram_clears_it() {
        let (ir, layout) = fixture();
        let mut selection = NodeSelection::default();
        assert!(selection.navigate(&ir, &layout, NavigationDirection::Right));
        assert_eq!(selection.selected_node_id(), Some("E"));
        let empty = MermaidDiagramIr::empty(fm_core::DiagramType::Flowchart);
        let empty_layout = fm_layout::layout_diagram(&empty);
        assert!(selection.navigate(&empty, &empty_layout, NavigationDirection::Down));
        assert_eq!(selection.selected_node_id(), None);
        assert!(!selection.navigate(&empty, &empty_layout, NavigationDirection::Down));
    }

    #[test]
    fn pointer_picks_last_drawn_box_and_excludes_far_boundary() {
        let (ir, mut layout) = fixture();
        layout
            .nodes
            .retain(|node| matches!(ir.nodes[node.node_index].id.as_str(), "A" | "B"));
        layout.nodes.sort_by(|left, right| {
            ir.nodes[left.node_index]
                .id
                .cmp(&ir.nodes[right.node_index].id)
        });
        place(&ir, &mut layout, "B", 0.0, 0.0);
        let mut selection = NodeSelection::default();
        assert!(selection.select_at(&ir, &layout, 0.0, 0.0));
        assert_eq!(selection.selected_node_id(), Some("B"));
        assert!(selection.select_at(&ir, &layout, 40.0, 20.0));
        assert_eq!(selection.selected_node_id(), None);
        assert!(!selection.select_at(&ir, &layout, 20.0, 40.0));
        layout.nodes.reverse();
        assert!(selection.select_at(&ir, &layout, 20.0, 20.0));
        assert_eq!(selection.selected_node_id(), Some("A"));
    }

    #[test]
    fn viewport_picking_tracks_zoom_pan_and_uses_canvas_units() {
        let (ir, layout) = fixture();
        let mut selection = NodeSelection::default();
        let viewport = crate::Viewport {
            offset_x: 100.0,
            offset_y: -40.0,
            zoom: 2.0,
            device_pixel_ratio: 2.0,
            ..crate::Viewport::default()
        };
        let (x, y) = viewport.diagram_to_canvas(220.0, 20.0);
        assert!(selection.select_at_canvas(&ir, &layout, &viewport, x, y));
        assert_eq!(selection.selected_node_id(), Some("B"));
        for zoom in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let invalid = crate::Viewport { zoom, ..viewport };
            assert!(!selection.select_at_canvas(&ir, &layout, &invalid, x, y));
            assert_eq!(selection.selected_node_id(), Some("B"));
        }
    }

    #[test]
    fn selection_survives_index_changes_and_reconciles_removed_nodes() {
        let (ir, layout) = fixture();
        let mut selection = NodeSelection::default();
        selection.select_node(&ir, &layout, "B");
        let reordered = fm_parser::parse("flowchart LR\n B[Beta]\n A[Alpha]\n").ir;
        let reordered_layout = fm_layout::layout_diagram(&reordered);
        assert_ne!(
            ir.nodes.iter().position(|node| node.id == "B"),
            reordered.nodes.iter().position(|node| node.id == "B")
        );
        assert!(!selection.reconcile(&reordered, &reordered_layout));
        assert_eq!(selection.selected_node_id(), Some("B"));
        let expected = reordered_layout
            .nodes
            .iter()
            .find(|node| reordered.nodes[node.node_index].id == "B")
            .unwrap()
            .bounds;
        assert_eq!(
            selection.selected_bounds(&reordered, &reordered_layout),
            Some(expected)
        );
        let removed = fm_parser::parse("flowchart LR\n A[Alpha]\n").ir;
        let removed_layout = fm_layout::layout_diagram(&removed);
        assert!(selection.reconcile(&removed, &removed_layout));
        assert_eq!(selection.selected_node_id(), None);
    }

    #[test]
    fn invalid_points_and_geometry_do_not_steal_focus() {
        let (ir, mut layout) = fixture();
        let mut selection = NodeSelection::default();
        selection.select_node(&ir, &layout, "A");
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(!selection.select_at(&ir, &layout, value, 20.0));
            assert!(!selection.select_at(&ir, &layout, 20.0, value));
        }
        for node in &mut layout.nodes {
            if ir.nodes[node.node_index].id != "A" {
                node.bounds.width = f32::NAN;
            }
        }
        assert!(!selection.select_node(&ir, &layout, "B"));
        assert!(!selection.navigate(&ir, &layout, NavigationDirection::Right));
        assert_eq!(selection.selected_node_id(), Some("A"));
        layout
            .nodes
            .iter_mut()
            .find(|node| ir.nodes[node.node_index].id == "A")
            .unwrap()
            .node_index = usize::MAX;
        selection.reconcile(&ir, &layout);
        assert!(!selection.navigate(&ir, &layout, NavigationDirection::Right));
        assert_eq!(selection.selected_node_id(), None);
    }

    #[test]
    fn finite_f32_extremes_do_not_overflow_navigation_score() {
        let (ir, mut layout) = fixture();
        layout
            .nodes
            .retain(|node| matches!(ir.nodes[node.node_index].id.as_str(), "A" | "B"));
        place(&ir, &mut layout, "A", -f32::MAX, 0.0);
        place(&ir, &mut layout, "B", f32::MAX, 0.0);
        let mut selection = NodeSelection::default();
        selection.select_node(&ir, &layout, "A");
        assert!(selection.navigate(&ir, &layout, NavigationDirection::Right));
        assert_eq!(selection.selected_node_id(), Some("B"));
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;
    use crate::{CanvasRenderConfig, DrawOperation, MockCanvas2dContext, ViewportTransform};
    use fm_layout::LayoutPoint;

    fn diagram() -> (MermaidDiagramIr, Arc<DiagramLayout>) {
        let ir = fm_parser::parse(
            "flowchart LR\n A[Alpha] --> B[Beta]\n click A \"https://example.com\" \"Alpha tip\"\n",
        )
        .ir;
        assert_eq!(ir.nodes.len(), 2);
        let mut layout = fm_layout::layout_diagram(&ir);
        assert_eq!(layout.nodes.len(), 2);
        assert_eq!(layout.edges.len(), 1);
        layout.bounds = LayoutRect {
            x: -100.0,
            y: -50.0,
            width: 400.0,
            height: 200.0,
        };
        for node in &mut layout.nodes {
            node.bounds = LayoutRect {
                x: if ir.nodes[node.node_index].id == "A" {
                    -80.0
                } else {
                    200.0
                },
                y: -20.0,
                width: 40.0,
                height: 30.0,
            };
        }
        layout.edges[0].points = vec![
            LayoutPoint { x: -40.0, y: -5.0 },
            LayoutPoint { x: 200.0, y: -5.0 },
        ]
        .into();
        (ir, Arc::new(layout))
    }

    fn session(auto_fit: bool) -> CanvasSession {
        let (ir, layout) = diagram();
        let mut session = CanvasSession::default();
        session.set_diagram(
            ir,
            layout,
            CanvasRenderConfig {
                auto_fit,
                padding: if auto_fit { 0.0 } else { 7.0 },
                ..CanvasRenderConfig::default()
            },
        );
        session
    }

    // Independent observation of what the renderer issued, not a round trip through the reported
    // viewport (a mutually wrong forward/inverse pair would pass such a test).
    fn drawn_label(context: &MockCanvas2dContext, label: &str) -> (f64, f64) {
        let transform = context
            .operations()
            .iter()
            .find_map(|operation| match *operation {
                DrawOperation::SetTransform(a, b, c, d, e, f) => {
                    Some(ViewportTransform { a, b, c, d, e, f })
                }
                _ => None,
            })
            .expect("the real renderer sets a viewport transform");
        let (x, y) = context
            .operations()
            .iter()
            .find_map(|operation| match operation {
                DrawOperation::FillText(text, x, y) if text == label => Some((*x, *y)),
                _ => None,
            })
            .expect("the real renderer draws the parsed label");
        transform.apply(x, y)
    }

    #[test]
    fn session_picks_the_actual_drawn_label_under_fit_and_manual_padding() {
        for (auto_fit, expected) in [(true, (20.0, 32.5)), (false, (47.0, 52.0))] {
            let mut session = session(auto_fit);
            let mut context = MockCanvas2dContext::new(200.0, 120.0);
            session.redraw(&mut context).expect("draw");
            let (x, y) = drawn_label(&context, "Alpha");
            assert!((x - expected.0).abs() < 1e-6 && (y - expected.1).abs() < 1e-6);
            assert_eq!(session.hit_test_node(x, y, [200.0, 120.0]), Some("A"));
            let hit = session
                .hit_test_interaction(x, y, [200.0, 120.0])
                .expect("link");
            assert_eq!(hit.href.as_deref(), Some("https://example.com"));
            assert_eq!(hit.tooltip.as_deref(), Some("Alpha tip"));
            assert!(session.select_at(x, y, [200.0, 120.0]));
            assert_eq!(session.selected_node_id(), Some("A"));
        }
    }

    #[test]
    fn session_redraw_reuses_geometry_and_replaces_resize_sensitive_picking() {
        let mut session = session(true);
        let original = Arc::clone(&session.frame.as_ref().unwrap().layout);
        let mut small = MockCanvas2dContext::new(200.0, 120.0);
        session.redraw(&mut small).unwrap();
        let small_point = drawn_label(&small, "Alpha");
        assert_eq!(
            session.hit_test_node(small_point.0, small_point.1, [400.0, 240.0]),
            None
        );
        assert!(!session.select_at(small_point.0, small_point.1, [400.0, 240.0]));
        let mut large = MockCanvas2dContext::new(400.0, 240.0);
        session.redraw(&mut large).unwrap();
        let large_point = drawn_label(&large, "Alpha");
        assert_ne!(small_point, large_point);
        assert_eq!(
            session.hit_test_node(large_point.0, large_point.1, [400.0, 240.0]),
            Some("A")
        );
        assert!(session.last_result([200.0, 120.0]).is_none());
        assert!(Arc::ptr_eq(
            &original,
            &session.frame.as_ref().unwrap().layout
        ));
    }

    #[test]
    fn session_edge_tolerance_is_in_pixels_not_layout_units() {
        let mut session = session(true);
        let mut context = MockCanvas2dContext::new(200.0, 120.0);
        session.redraw(&mut context).unwrap();
        // The horizontal edge is at y=32.5 after 0.5x fit: the query is 3 screen pixels away,
        // but 6 layout units away. Passing the unscaled tolerance to layout-space distance fails.
        assert_eq!(
            session.hit_test_edge(90.0, 35.5, 3.01, [200.0, 120.0]),
            Some(0)
        );
        assert_eq!(
            session.hit_test_edge(90.0, 35.5, 2.99, [200.0, 120.0]),
            None
        );
        for tolerance in [-1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                session.hit_test_edge(90.0, 32.5, tolerance, [200.0, 120.0]),
                None
            );
        }
    }

    #[test]
    fn session_excludes_bundled_edges_and_breaks_ties_deterministically() {
        let (ir, original) = diagram();
        let mut layout = (*original).clone();
        let mut duplicate = layout.edges[0].clone();
        duplicate.edge_index = 7;
        layout.edges.insert(0, duplicate);
        let mut session = CanvasSession::default();
        let mut context = MockCanvas2dContext::new(200.0, 120.0);
        let config = CanvasRenderConfig {
            padding: 0.0,
            ..CanvasRenderConfig::default()
        };
        session.set_diagram(ir.clone(), Arc::new(layout.clone()), config.clone());
        session.redraw(&mut context).unwrap();
        assert_eq!(
            session.hit_test_edge(90.0, 32.5, 0.01, [200.0, 120.0]),
            Some(0)
        );
        layout.edges[1].bundled = true;
        session.set_diagram(ir.clone(), Arc::new(layout.clone()), config.clone());
        session.redraw(&mut context).unwrap();
        assert_eq!(
            session.hit_test_edge(90.0, 32.5, 0.01, [200.0, 120.0]),
            Some(7)
        );
        layout.edges[0].points[0].x = f32::NAN;
        session.set_diagram(ir, Arc::new(layout), config);
        session.redraw(&mut context).unwrap();
        assert_eq!(
            session.hit_test_edge(90.0, 32.5, 0.01, [200.0, 120.0]),
            None
        );
    }

    #[test]
    fn session_never_clicks_through_an_unlinked_node_and_uses_half_open_bounds() {
        let (ir, original) = diagram();
        let mut layout = (*original).clone();
        layout
            .nodes
            .sort_by(|a, b| ir.nodes[a.node_index].id.cmp(&ir.nodes[b.node_index].id));
        layout.nodes[1].bounds = layout.nodes[0].bounds;
        let config = CanvasRenderConfig {
            padding: 0.0,
            ..CanvasRenderConfig::default()
        };
        let mut session = CanvasSession::default();
        let mut context = MockCanvas2dContext::new(200.0, 120.0);
        session.set_diagram(ir.clone(), Arc::new(layout.clone()), config.clone());
        session.redraw(&mut context).unwrap();
        assert_eq!(session.hit_test_node(20.0, 32.5, [200.0, 120.0]), Some("B"));
        assert!(
            session
                .hit_test_interaction(20.0, 32.5, [200.0, 120.0])
                .is_none()
        );
        assert_eq!(session.hit_test_node(30.0, 32.5, [200.0, 120.0]), None);
        assert_eq!(session.hit_test_node(20.0, 40.0, [200.0, 120.0]), None);
        layout.nodes.reverse();
        session.set_diagram(ir, Arc::new(layout), config);
        session.redraw(&mut context).unwrap();
        assert_eq!(session.hit_test_node(20.0, 32.5, [200.0, 120.0]), Some("A"));
        assert!(
            session
                .hit_test_interaction(20.0, 32.5, [200.0, 120.0])
                .is_some()
        );
    }

    #[test]
    fn session_selection_redraw_preserves_content_and_erases_the_old_ring() {
        let mut session = session(true);
        let mut context = MockCanvas2dContext::new(200.0, 120.0);
        let plain = session.redraw(&mut context).unwrap().clone();
        let baseline = context.operations().to_vec();
        assert!(session.select_node("A"));
        context.clear();
        let selected = session.redraw(&mut context).unwrap();
        assert_eq!(selected.draw_calls, plain.draw_calls + 2);
        assert_eq!(selected.hit_regions, plain.hit_regions);
        assert_eq!(selected.nodes_drawn, plain.nodes_drawn);
        assert_eq!(selected.edges_drawn, plain.edges_drawn);
        assert_eq!(selected.labels_drawn, plain.labels_drawn);
        assert_eq!(&context.operations()[..baseline.len()], baseline.as_slice());
        assert!(session.navigate_selection(NavigationDirection::Right));
        assert_eq!(session.selected_node_id(), Some("B"));
        assert!(!session.navigate_selection(NavigationDirection::Right));
        assert!(session.clear_selection());
        context.clear();
        session.redraw(&mut context).unwrap();
        assert_eq!(context.operations(), baseline.as_slice());
    }

    #[test]
    fn session_source_replacement_reconciles_selection_and_invalidates_old_hits() {
        let mut session = session(true);
        let mut context = MockCanvas2dContext::new(200.0, 120.0);
        session.redraw(&mut context).unwrap();
        assert!(session.select_node("B"));
        let ir = fm_parser::parse("flowchart LR\n B[Beta]\n A[Alpha]\n").ir;
        let layout = Arc::new(fm_layout::layout_diagram(&ir));
        session.set_diagram(ir, layout, CanvasRenderConfig::default());
        assert_eq!(session.selected_node_id(), Some("B"));
        assert!(session.last_result([200.0, 120.0]).is_none());
        assert_eq!(session.hit_test_node(20.0, 32.5, [200.0, 120.0]), None);
        let ir = fm_parser::parse("flowchart LR\n A[Alpha]\n").ir;
        let layout = Arc::new(fm_layout::layout_diagram(&ir));
        session.set_diagram(ir, layout, CanvasRenderConfig::default());
        assert_eq!(session.selected_node_id(), None);
        assert!(session.redraw(&mut context).is_some());
    }

    #[test]
    fn session_invalid_and_outside_queries_do_not_steal_selection() {
        let mut session = session(true);
        let mut context = MockCanvas2dContext::new(200.0, 120.0);
        session.redraw(&mut context).unwrap();
        assert!(session.select_node("A"));
        for (x, y) in [
            (f64::NAN, 32.5),
            (20.0, f64::INFINITY),
            (-1.0, 32.5),
            (200.0, 32.5),
        ] {
            assert_eq!(session.hit_test_node(x, y, [200.0, 120.0]), None);
            assert!(!session.select_at(x, y, [200.0, 120.0]));
            assert_eq!(session.selected_node_id(), Some("A"));
        }
        assert!(session.select_at(100.0, 100.0, [200.0, 120.0]));
        assert_eq!(session.selected_node_id(), None);
        for zoom in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            session.last_result.as_mut().unwrap().viewport.zoom = zoom;
            assert_eq!(session.hit_test_node(20.0, 32.5, [200.0, 120.0]), None);
        }
    }

    #[test]
    fn session_explicit_invalidation_and_clear_cannot_revive_stale_metadata() {
        let mut session = session(true);
        let mut context = MockCanvas2dContext::new(200.0, 120.0);
        session.redraw(&mut context).unwrap();
        session.select_node("A");
        session.invalidate();
        assert!(
            session
                .hit_test_interaction(20.0, 32.5, [200.0, 120.0])
                .is_none()
        );
        assert_eq!(session.selected_node_id(), Some("A"));
        assert!(session.redraw(&mut context).is_some());
        session.clear();
        context.clear();
        assert_eq!(session.selected_node_id(), None);
        assert!(session.redraw(&mut context).is_none());
        assert!(context.operations().is_empty());
        assert!(session.last_result([200.0, 120.0]).is_none());
        assert!(!session.navigate_selection(NavigationDirection::Right));
    }
}
