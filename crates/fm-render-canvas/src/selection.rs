//! Stable multi-node selection for raster diagrams (bd-vmk).
//!
//! Selection is not activation: no URL is followed and no callback is invoked. Regions use
//! the same raw layout coordinates as `NodeSelection`; canvas queries use the viewport from
//! the actual draw. The host owns pointer capture, modifier keys, and gesture cancellation.

use std::collections::BTreeSet;

use fm_core::MermaidDiagramIr;
use fm_layout::{DiagramLayout, LayoutRect};

use crate::{
    Canvas2dContext, CanvasRenderConfig, CanvasRenderResult, CanvasSelectionStyle, Viewport,
};

/// Maximum lasso vertices, including an optional repeated closing point.
pub const MAX_LASSO_POINTS: usize = 4096;

// Keep orientation products finite, even when a host supplies unreasonable coordinates.
// This exceeds the entire f32 layout domain by many orders of magnitude.
const MAX_LASSO_COORDINATE: f64 = 1.0e100;

/// How a query changes the selected author IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    /// Replace the selection, clearing it when a valid query hits empty space.
    Replace,
    /// Add matches without removing the existing selection.
    Add,
    /// Remove matches from the selection.
    Remove,
    /// Toggle each matching ID exactly once, even if it has multiple layout boxes.
    Toggle,
}

/// Whether a region must cover a whole node box or just touch it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionContainment {
    /// Include boxes which overlap or touch the region boundary.
    Intersects,
    /// Include only boxes entirely inside the region, including its boundary.
    /// A lasso stroke cutting the box's interior disqualifies it, including retraced strokes.
    Contains,
}

/// A drag rectangle, with either drag direction supported.
///
/// Coordinates are layout units, or backing-store pixels for `select_box_canvas`. A zero-area
/// or non-finite box is ignored without changing selection; use `select_at` for a click.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionBox {
    /// Pointer-down position.
    pub start: (f64, f64),
    /// Current pointer position.
    pub end: (f64, f64),
}

impl SelectionBox {
    fn normalized(self) -> Option<Bounds> {
        let (x0, y0) = self.start;
        let (x1, y1) = self.end;
        if ![x0, y0, x1, y1].iter().all(|value| value.is_finite()) || x0 == x1 || y0 == y1 {
            return None;
        }
        Some(Bounds {
            left: x0.min(x1),
            top: y0.min(y1),
            right: x0.max(x1),
            bottom: y0.max(y1),
        })
    }
}

/// Shape of a selection query or a live drag preview.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SelectionRegion<'a> {
    /// Rectangular marquee, dragged in either direction.
    Box(SelectionBox),
    /// Automatically closed freehand polygon, with even-odd fill and inclusive boundaries.
    /// Repeated points and self-intersections are allowed. Fewer than three non-collinear
    /// points, more than `MAX_LASSO_POINTS`, non-finite coordinates, or coordinates with
    /// absolute value above 1e100 are invalid and leave selection unchanged.
    Lasso(&'a [(f64, f64)]),
}

impl SelectionRegion<'_> {
    fn query(
        self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        containment: SelectionContainment,
    ) -> Option<BTreeSet<String>> {
        let matches = match self {
            Self::Box(region) => {
                let region = region.normalized()?;
                nodes(ir, layout)
                    .filter(|(_, bounds)| region.matches(Bounds::from(*bounds), containment))
                    .map(|(id, _)| id.to_owned())
                    .collect()
            }
            Self::Lasso(points) => {
                let polygon = Polygon::new(points)?;
                nodes(ir, layout)
                    .filter(|(_, bounds)| polygon.matches(Bounds::from(*bounds), containment))
                    .map(|(id, _)| id.to_owned())
                    .collect()
            }
        };
        Some(matches)
    }

    fn query_canvas(
        self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        viewport: &Viewport,
        containment: SelectionContainment,
    ) -> Option<BTreeSet<String>> {
        if !valid_viewport(viewport) {
            return None;
        }
        match self {
            Self::Box(region) => Self::Box(SelectionBox {
                start: viewport.canvas_to_diagram(region.start.0, region.start.1),
                end: viewport.canvas_to_diagram(region.end.0, region.end.1),
            })
            .query(ir, layout, containment),
            Self::Lasso(points) => {
                // Bound allocation and reject malformed input BEFORE transforming it.
                Polygon::new(points)?;
                let mapped: Vec<_> = points
                    .iter()
                    .map(|&(x, y)| viewport.canvas_to_diagram(x, y))
                    .collect();
                SelectionRegion::Lasso(&mapped).query(ir, layout, containment)
            }
        }
    }
}

/// One drag's immutable starting selection and modifier policy.
///
/// Every preview is recomputed from this baseline, NOT the preceding preview: moving a toggle
/// lasso cannot repeatedly flip the same node, and an additive drag does not accumulate nodes
/// outside its current region. A successful pointer-up commits the current preview by simply
/// dropping this value. Call `cancel` on Escape/lost capture, before starting another gesture.
/// The host owns pointer IDs, drag thresholds, and capture; only one gesture may edit a set.
#[derive(Debug)]
pub struct SelectionGesture {
    baseline: NodeSelectionSet,
    mode: SelectionMode,
    containment: SelectionContainment,
}

impl SelectionGesture {
    /// Update a live drag in layout coordinates. Invalid regions leave the preview untouched.
    /// Returns whether the visible selection changed, not whether the region was valid.
    pub fn preview(
        &self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        selection: &mut NodeSelectionSet,
        region: SelectionRegion<'_>,
    ) -> bool {
        let Some(matches) = region.query(ir, layout, self.containment) else {
            return false;
        };
        self.publish(ir, layout, selection, matches)
    }

    /// Update a live drag in backing-store pixels using the viewport from the current draw.
    pub fn preview_canvas(
        &self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        viewport: &Viewport,
        selection: &mut NodeSelectionSet,
        region: SelectionRegion<'_>,
    ) -> bool {
        let Some(matches) = region.query_canvas(ir, layout, viewport, self.containment) else {
            return false;
        };
        self.publish(ir, layout, selection, matches)
    }

    /// Restore the starting selection, excluding nodes removed since pointer-down.
    /// Consumes the gesture so a cancelled drag cannot subsequently publish another preview.
    pub fn cancel(
        mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        selection: &mut NodeSelectionSet,
    ) -> bool {
        self.baseline.reconcile(ir, layout);
        selection.apply(self.baseline.ids, SelectionMode::Replace)
    }

    fn publish(
        &self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        selection: &mut NodeSelectionSet,
        matches: BTreeSet<String>,
    ) -> bool {
        let mut next = self.baseline.clone();
        next.reconcile(ir, layout);
        next.apply(matches, self.mode);
        selection.apply(next.ids, SelectionMode::Replace)
    }
}

/// A deterministic set of author IDs, independent of node indices and layout iteration order.
///
/// Keep the IR/layout pair used for drawing and picking together. Call `reconcile` after a
/// diagram change (the rendering entry point does this automatically). Non-interactive nodes
/// are selectable too. Invalid node indices and non-finite or empty node boxes are skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeSelectionSet {
    ids: BTreeSet<String>,
}

impl NodeSelectionSet {
    /// Capture the immutable baseline for a live box/lasso drag. See `SelectionGesture`.
    #[must_use]
    pub fn begin_gesture(
        &self,
        mode: SelectionMode,
        containment: SelectionContainment,
    ) -> SelectionGesture {
        SelectionGesture {
            baseline: self.clone(),
            mode,
            containment,
        }
    }

    /// Selected author IDs in lexical order, suitable for stable event payloads.
    pub fn selected_node_ids(&self) -> impl DoubleEndedIterator<Item = &str> + ExactSizeIterator {
        self.ids.iter().map(String::as_str)
    }

    /// Number of distinct selected author IDs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether no nodes are selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Whether an author ID belongs to the selection.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }

    /// Clear all selected IDs, returning whether anything changed.
    pub fn clear(&mut self) -> bool {
        let changed = !self.ids.is_empty();
        self.ids.clear();
        changed
    }

    /// Select one valid, placed author ID. An unknown ID never clears existing selection.
    pub fn select_node(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        id: &str,
        mode: SelectionMode,
    ) -> bool {
        if !nodes(ir, layout).any(|(node_id, _)| node_id == id) {
            return false;
        }
        self.apply(BTreeSet::from([id.to_owned()]), mode)
    }

    /// Pick the last drawn node box. Right/bottom edges are half-open, as in `NodeSelection`.
    /// Invalid points leave selection unchanged; a valid empty-space Replace clears it.
    pub fn select_at(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        point: (f64, f64),
        mode: SelectionMode,
    ) -> bool {
        if !point.0.is_finite() || !point.1.is_finite() {
            return false;
        }
        let matches = nodes(ir, layout)
            .rev()
            .find(|(_, bounds)| Bounds::from(*bounds).contains_point(point, false))
            .map(|(id, _)| id.to_owned())
            .into_iter()
            .collect();
        self.apply(matches, mode)
    }

    /// Pick in backing-store pixels using the viewport returned by the actual draw.
    pub fn select_at_canvas(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        viewport: &Viewport,
        point: (f64, f64),
        mode: SelectionMode,
    ) -> bool {
        if !valid_viewport(viewport) {
            return false;
        }
        self.select_at(
            ir,
            layout,
            viewport.canvas_to_diagram(point.0, point.1),
            mode,
        )
    }

    /// Select by a layout-space marquee; all matching boxes participate, not just the topmost.
    pub fn select_box(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        region: SelectionBox,
        mode: SelectionMode,
        containment: SelectionContainment,
    ) -> bool {
        let Some(matches) = SelectionRegion::Box(region).query(ir, layout, containment) else {
            return false;
        };
        self.apply(matches, mode)
    }

    /// Marquee in canvas backing-store pixels, using the last draw's origin/padding/zoom map.
    /// Convert CSS pixels once in the host; the device-pixel ratio is NOT applied again here.
    pub fn select_box_canvas(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        viewport: &Viewport,
        region: SelectionBox,
        mode: SelectionMode,
        containment: SelectionContainment,
    ) -> bool {
        let Some(matches) =
            SelectionRegion::Box(region).query_canvas(ir, layout, viewport, containment)
        else {
            return false;
        };
        self.apply(matches, mode)
    }

    /// Select by a freehand polygon in layout coordinates. See `SelectionRegion::Lasso` for
    /// input limits and fill semantics. A valid empty-space Replace clears selection.
    pub fn select_lasso(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        points: &[(f64, f64)],
        mode: SelectionMode,
        containment: SelectionContainment,
    ) -> bool {
        let Some(matches) = SelectionRegion::Lasso(points).query(ir, layout, containment) else {
            return false;
        };
        self.apply(matches, mode)
    }

    /// Freehand polygon in backing-store pixels, inverse-mapped with the current draw viewport.
    pub fn select_lasso_canvas(
        &mut self,
        ir: &MermaidDiagramIr,
        layout: &DiagramLayout,
        viewport: &Viewport,
        points: &[(f64, f64)],
        mode: SelectionMode,
        containment: SelectionContainment,
    ) -> bool {
        let Some(matches) =
            SelectionRegion::Lasso(points).query_canvas(ir, layout, viewport, containment)
        else {
            return false;
        };
        self.apply(matches, mode)
    }

    /// Remove IDs whose nodes disappeared or no longer have usable geometry.
    pub fn reconcile(&mut self, ir: &MermaidDiagramIr, layout: &DiagramLayout) -> bool {
        let valid: BTreeSet<_> = nodes(ir, layout).map(|(id, _)| id).collect();
        let before = self.ids.len();
        self.ids.retain(|id| valid.contains(id.as_str()));
        self.ids.len() != before
    }

    fn apply(&mut self, matches: BTreeSet<String>, mode: SelectionMode) -> bool {
        if mode == SelectionMode::Replace {
            if self.ids == matches {
                return false;
            }
            self.ids = matches;
            return true;
        }
        let mut changed = false;
        for id in matches {
            match mode {
                SelectionMode::Replace => unreachable!(),
                SelectionMode::Add => changed |= self.ids.insert(id),
                SelectionMode::Remove => changed |= self.ids.remove(&id),
                SelectionMode::Toggle => {
                    if !self.ids.remove(&id) {
                        self.ids.insert(id);
                    }
                    changed = true;
                }
            }
        }
        changed
    }
}

fn valid_viewport(viewport: &Viewport) -> bool {
    viewport.zoom.is_finite()
        && viewport.zoom > 0.0
        && viewport.offset_x.is_finite()
        && viewport.offset_y.is_finite()
}

fn nodes<'a>(
    ir: &'a MermaidDiagramIr,
    layout: &'a DiagramLayout,
) -> impl DoubleEndedIterator<Item = (&'a str, LayoutRect)> {
    layout.nodes.iter().filter_map(move |placed| {
        let node = ir.nodes.get(placed.node_index)?;
        let rect = placed.bounds;
        (rect.x.is_finite()
            && rect.y.is_finite()
            && rect.width.is_finite()
            && rect.height.is_finite()
            && rect.width > 0.0
            && rect.height > 0.0)
            .then_some((node.id.as_str(), rect))
    })
}

#[derive(Clone, Copy)]
struct Bounds {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl From<LayoutRect> for Bounds {
    fn from(rect: LayoutRect) -> Self {
        Self {
            left: f64::from(rect.x),
            top: f64::from(rect.y),
            right: f64::from(rect.x) + f64::from(rect.width),
            bottom: f64::from(rect.y) + f64::from(rect.height),
        }
    }
}

impl Bounds {
    fn corners(self) -> [(f64, f64); 4] {
        [
            (self.left, self.top),
            (self.right, self.top),
            (self.right, self.bottom),
            (self.left, self.bottom),
        ]
    }

    fn contains_point(self, (x, y): (f64, f64), closed: bool) -> bool {
        x >= self.left
            && y >= self.top
            && if closed {
                x <= self.right && y <= self.bottom
            } else {
                x < self.right && y < self.bottom
            }
    }

    fn matches(self, other: Self, containment: SelectionContainment) -> bool {
        match containment {
            SelectionContainment::Contains => {
                other.left >= self.left
                    && other.top >= self.top
                    && other.right <= self.right
                    && other.bottom <= self.bottom
            }
            SelectionContainment::Intersects => {
                other.right >= self.left
                    && other.bottom >= self.top
                    && other.left <= self.right
                    && other.top <= self.bottom
            }
        }
    }

    fn intersects_segment(self, a: (f64, f64), b: (f64, f64), open: bool) -> bool {
        // Separating axes: the rectangle's x/y axes and the segment's normal. No division
        // or clipping parameter is needed, avoiding loss of a narrow box on a long segment.
        let overlaps = if open {
            a.0.max(b.0) > self.left
                && a.0.min(b.0) < self.right
                && a.1.max(b.1) > self.top
                && a.1.min(b.1) < self.bottom
        } else {
            a.0.max(b.0) >= self.left
                && a.0.min(b.0) <= self.right
                && a.1.max(b.1) >= self.top
                && a.1.min(b.1) <= self.bottom
        };
        if !overlaps || a == b {
            return overlaps;
        }
        let signs = self.corners().map(|point| orientation(a, b, point));
        if open {
            signs.iter().any(|&value| value < 0.0) && signs.iter().any(|&value| value > 0.0)
        } else {
            signs.iter().any(|&value| value <= 0.0) && signs.iter().any(|&value| value >= 0.0)
        }
    }
}

fn orientation(a: (f64, f64), b: (f64, f64), point: (f64, f64)) -> f64 {
    (b.0 - a.0) * (point.1 - a.1) - (b.1 - a.1) * (point.0 - a.0)
}

struct Polygon<'a> {
    points: &'a [(f64, f64)],
    bounds: Bounds,
}

impl<'a> Polygon<'a> {
    fn new(points: &'a [(f64, f64)]) -> Option<Self> {
        if !(3..=MAX_LASSO_POINTS).contains(&points.len())
            || points.iter().any(|&(x, y)| {
                !x.is_finite()
                    || !y.is_finite()
                    || x.abs() > MAX_LASSO_COORDINATE
                    || y.abs() > MAX_LASSO_COORDINATE
            })
        {
            return None;
        }
        let first = points[0];
        let &second = points.iter().find(|&&point| point != first)?;
        // Signed polygon area would incorrectly reject a bow-tie with two valid lobes.
        if !points
            .iter()
            .any(|&point| orientation(first, second, point) != 0.0)
        {
            return None;
        }
        let mut bounds = Bounds {
            left: first.0,
            top: first.1,
            right: first.0,
            bottom: first.1,
        };
        for &(x, y) in points {
            bounds.left = bounds.left.min(x);
            bounds.top = bounds.top.min(y);
            bounds.right = bounds.right.max(x);
            bounds.bottom = bounds.bottom.max(y);
        }
        Some(Self { points, bounds })
    }

    fn edges(&self) -> impl Iterator<Item = ((f64, f64), (f64, f64))> + '_ {
        self.points
            .iter()
            .copied()
            .zip(self.points.iter().copied().cycle().skip(1))
    }

    fn contains_point(&self, point: (f64, f64)) -> bool {
        if !self.bounds.contains_point(point, true) {
            return false;
        }
        let mut inside = false;
        for (a, b) in self.edges() {
            let side = orientation(a, b, point);
            if side == 0.0
                && point.0 >= a.0.min(b.0)
                && point.0 <= a.0.max(b.0)
                && point.1 >= a.1.min(b.1)
                && point.1 <= a.1.max(b.1)
            {
                return true;
            }
            if (a.1 > point.1) != (b.1 > point.1) && (side > 0.0) == (b.1 > a.1) {
                inside = !inside;
            }
        }
        inside
    }

    fn matches(&self, bounds: Bounds, containment: SelectionContainment) -> bool {
        if !self.bounds.matches(bounds, containment) {
            return false;
        }
        let corners = bounds.corners();
        match containment {
            SelectionContainment::Intersects => {
                corners.iter().any(|&point| self.contains_point(point))
                    || self
                        .edges()
                        .any(|(a, b)| bounds.intersects_segment(a, b, false))
            }
            SelectionContainment::Contains => {
                corners.iter().all(|&point| self.contains_point(point))
                    // Coincident boundary strokes can enclose no even-odd area (for example
                    // tracing the same box twice). Corners on that stroke are not sufficient.
                    && self.contains_point((
                        bounds.left + (bounds.right - bounds.left) / 2.0,
                        bounds.top + (bounds.bottom - bounds.top) / 2.0,
                    ))
                    // Corners alone miss a concave notch or hole entering the box interior.
                    && !self
                        .edges()
                        .any(|(a, b)| bounds.intersects_segment(a, b, true))
            }
        }
    }
}

/// Draw all selected node rings in layout draw order, without clearing the base diagram.
/// Widths and gaps stay in canvas pixels. Selection emits no interaction callbacks.
/// This scans the layout once; it does not perform a new lookup or layout for each selected ID.
pub fn draw_multi_selection_overlay<C: Canvas2dContext>(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    context: &mut C,
    viewport: &Viewport,
    selection: &NodeSelectionSet,
    style: &CanvasSelectionStyle,
) -> usize {
    if selection.is_empty() {
        return 0;
    }
    nodes(ir, layout)
        .filter(|(id, _)| selection.contains(id))
        .map(|(_, bounds)| crate::draw_selection_bounds(context, viewport, bounds, style))
        .sum()
}

/// Render and reconcile multi-selection from ONE matched IR/layout pair.
/// An empty selection preserves the ordinary renderer's complete drawing stream and metadata.
pub fn render_to_canvas_with_multi_selection<C: Canvas2dContext>(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    context: &mut C,
    config: &CanvasRenderConfig,
    selection: &mut NodeSelectionSet,
    style: &CanvasSelectionStyle,
) -> CanvasRenderResult {
    selection.reconcile(ir, layout);
    let mut result = crate::render_to_canvas_with_layout(ir, layout, context, config);
    result.draw_calls +=
        draw_multi_selection_overlay(ir, layout, context, &result.viewport, selection, style);
    result
}
