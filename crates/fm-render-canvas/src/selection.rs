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

/// Whether a marquee must cover a whole node box or just touch it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionContainment {
    /// Include boxes which overlap or touch the region boundary.
    Intersects,
    /// Include only boxes entirely inside the region, including its boundary.
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
        let Some(region) = region.normalized() else {
            return false;
        };
        let matches = nodes(ir, layout)
            .filter(|(_, bounds)| region.matches(Bounds::from(*bounds), containment))
            .map(|(id, _)| id.to_owned())
            .collect();
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
        if !valid_viewport(viewport) {
            return false;
        }
        self.select_box(
            ir,
            layout,
            SelectionBox {
                start: viewport.canvas_to_diagram(region.start.0, region.start.1),
                end: viewport.canvas_to_diagram(region.end.0, region.end.1),
            },
            mode,
            containment,
        )
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
