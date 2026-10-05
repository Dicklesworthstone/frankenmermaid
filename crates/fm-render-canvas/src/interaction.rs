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
        let target = selectable_nodes(ir, layout).rev().find(|node| {
            let bounds = node.bounds;
            x >= f64::from(bounds.x)
                && y >= f64::from(bounds.y)
                && x < f64::from(bounds.x) + f64::from(bounds.width)
                && y < f64::from(bounds.y) + f64::from(bounds.height)
        });
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
            .filter_map(|node| navigation_score(current, node, direction).map(|score| (node, score)))
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
        layout
            .nodes
            .sort_by(|left, right| ir.nodes[left.node_index].id.cmp(&ir.nodes[right.node_index].id));
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
