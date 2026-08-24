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
