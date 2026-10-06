//! `eventmodeling` layout: timeframes in type swimlanes, placed the way the pinned mermaid 11.15.0
//! `eventmodeling` db places them.
//!
//! - Every frame's box is its text plus 10px padding, clamped to 80–450 × 80–750, plus 10px
//!   padding again (upstream's `boxPadding` twice).
//! - Boxes advance left to right in SOURCE order, from x = 250 (room for the lane captions): a frame
//!   in the same lane as the previous one sits 10px right of that lane's last box; a frame that
//!   changes lane starts 80px left of the previous box's right edge, so the model reads as a
//!   staircase.
//! - Lanes stack top to bottom in upstream's key order (UI/Automation, Command/Read Model, Events,
//!   each followed by its namespaced lanes), 10px apart, each as tall as its tallest box + 30px
//!   (at least 100px), and as wide as the whole model.
//! - A flow arrow leaves its source two thirds along and enters its target one third along, from
//!   the edges that face each other.
//!
//! Boxes, lanes and arrows are ordinary node boxes, clusters and edge routes, so every backend draws
//! the model without a family-specific branch.

use std::sync::Arc;

use fm_core::{FontMetrics, IrEndpoint, MermaidDiagramIr};

use crate::{
    DiagramLayout, LayoutClusterBox, LayoutEdgePath, LayoutExtensions, LayoutNodeBox, LayoutPoint,
    LayoutRect, LayoutStats, LayoutTrace, TracedLayout, compute_edge_length_metrics,
    display_node_label_ref, push_snapshot,
};

/// Upstream `diagramProps`.
const CONTENT_START_X: f32 = 250.0;
const BOX_PADDING: f32 = 10.0;
const BOX_OVERLAP: f32 = 90.0;
const SWIMLANE_MIN_HEIGHT: f32 = 70.0;
const SWIMLANE_PADDING: f32 = 15.0;
const SWIMLANE_GAP: f32 = 10.0;

/// Lay out an `eventmodeling` diagram.
#[must_use]
pub(crate) fn layout_diagram_event_model_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let mut trace = LayoutTrace::default();
    let metrics = FontMetrics::default_metrics();
    let empty = fm_core::IrEventModelMeta::default();
    let meta = ir.event_model_meta.as_ref().unwrap_or(&empty);
    let lane_count = meta.lanes.len();

    // Boxes in source order, as upstream's `evolveFramePositioned` places them.
    let mut lane_right = vec![0.0_f32; lane_count];
    let mut lane_tallest = vec![0.0_f32; lane_count];
    let mut placed: Vec<(usize, usize, f32, f32, f32)> = Vec::with_capacity(meta.frames.len());
    let mut previous: Option<(usize, f32)> = None;
    let mut max_right = 0.0_f32;
    for &(node, lane) in &meta.frames {
        if node >= ir.nodes.len() || lane >= lane_count {
            continue;
        }
        // A bold name, then data lines drawn MONOSPACE: measured as such, or they overflow the box.
        let label = display_node_label_ref(ir, &ir.nodes[node]);
        let mut lines = label.lines();
        let name_width = lines
            .next()
            .map_or(0.0, |name| metrics.estimate_width(name) * 1.1);
        #[allow(clippy::cast_precision_loss)]
        let code_width = lines
            .map(|line| line.chars().count() as f32 * metrics.font_size() * 0.62)
            .fold(0.0_f32, f32::max);
        #[allow(clippy::cast_precision_loss)]
        let text_h = label.lines().count().max(1) as f32 * metrics.line_height_px();
        let text_w = name_width.max(code_width);
        let width = 2.0f32.mul_add(BOX_PADDING, text_w).clamp(80.0, 450.0) + 2.0 * BOX_PADDING;
        let height = 2.0f32.mul_add(BOX_PADDING, text_h).clamp(80.0, 750.0) + 2.0 * BOX_PADDING;
        let x = match previous {
            None => CONTENT_START_X,
            Some((previous_lane, _)) if previous_lane == lane && lane_right[lane] > 0.0 => {
                lane_right[lane] + BOX_PADDING
            }
            // ⚠️ Upstream steps off the previous box only, so RETURNING to a lane after a narrow
            // detour lands on that lane's own last box. The staircase is kept; the overlap is not.
            Some((_, previous_right)) => {
                (previous_right - BOX_OVERLAP + BOX_PADDING).max(if lane_right[lane] > 0.0 {
                    lane_right[lane] + BOX_PADDING
                } else {
                    0.0
                })
            }
        };
        let right = x + width + BOX_PADDING;
        lane_right[lane] = x + width;
        lane_tallest[lane] = lane_tallest[lane].max(height);
        max_right = max_right.max(right);
        previous = Some((lane, right));
        placed.push((node, lane, x, width, height));
    }

    // Lanes stacked top to bottom.
    let mut lane_top = vec![0.0_f32; lane_count];
    let mut lane_height = vec![0.0_f32; lane_count];
    let mut cursor = 0.0_f32;
    for lane in 0..lane_count {
        lane_top[lane] = cursor;
        lane_height[lane] = 2.0f32.mul_add(
            SWIMLANE_PADDING,
            SWIMLANE_MIN_HEIGHT.max(lane_tallest[lane]),
        );
        cursor += lane_height[lane] + SWIMLANE_GAP;
    }
    let total_height = (cursor - SWIMLANE_GAP).max(0.0);
    let total_width = max_right + SWIMLANE_PADDING;

    let mut boxes: Vec<Option<LayoutRect>> = vec![None; ir.nodes.len()];
    for &(node, lane, x, width, height) in &placed {
        boxes[node] = Some(LayoutRect {
            x,
            y: lane_top[lane] + SWIMLANE_PADDING,
            width,
            height,
        });
    }
    push_snapshot(
        &mut trace,
        "event_model_frames",
        ir.nodes.len(),
        ir.edges.len(),
        0,
        0,
    );

    let clusters = meta
        .lanes
        .iter()
        .enumerate()
        .map(|(lane, &cluster_index)| {
            let cluster = ir.clusters.get(cluster_index);
            LayoutClusterBox {
                cluster_index,
                span: cluster.map_or_else(Default::default, |c| c.span),
                title: cluster
                    .and_then(|c| c.title)
                    .and_then(|title| ir.labels.get(title.0))
                    .map(|label| label.text.clone()),
                color: None,
                bounds: LayoutRect {
                    x: 0.0,
                    y: lane_top[lane],
                    width: total_width,
                    height: lane_height[lane],
                },
            }
        })
        .collect();

    // A node no frame placed (a hand-built IR) goes under the lanes so it still has geometry.
    let nodes: Vec<LayoutNodeBox> = ir
        .nodes
        .iter()
        .enumerate()
        .map(|(node_index, node)| LayoutNodeBox {
            node_index,
            node_id: node.id.clone(),
            rank: 0,
            order: node_index,
            span: node.span_primary,
            bounds: boxes[node_index].unwrap_or(LayoutRect {
                x: CONTENT_START_X,
                y: total_height + SWIMLANE_GAP,
                width: 80.0,
                height: 80.0,
            }),
        })
        .collect();

    let box_of = |endpoint: IrEndpoint| match endpoint {
        IrEndpoint::Node(node) => nodes.get(node.0).map(|n| n.bounds),
        _ => None,
    };
    let edges: Vec<LayoutEdgePath> = ir
        .edges
        .iter()
        .enumerate()
        .map(|(edge_index, edge)| {
            let points = match (box_of(edge.from), box_of(edge.to)) {
                (Some(source), Some(target)) => {
                    let upwards = source.y > target.y;
                    let (source_y, target_y) = if upwards {
                        (source.y, target.y + target.height)
                    } else {
                        (source.y + source.height, target.y)
                    };
                    smallvec::smallvec![
                        LayoutPoint {
                            x: source.x + source.width * 2.0 / 3.0,
                            y: source_y,
                        },
                        LayoutPoint {
                            x: target.x + target.width / 3.0,
                            y: target_y,
                        },
                    ]
                }
                _ => smallvec::SmallVec::new(),
            };
            LayoutEdgePath {
                edge_index,
                span: edge.span,
                points,
                reversed: false,
                is_self_loop: false,
                parallel_offset: 0.0,
                bundle_count: 1,
                bundled: false,
            }
        })
        .collect();

    let mut bounds = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: total_width,
        height: total_height,
    };
    for node in &nodes {
        let b = node.bounds;
        bounds.width = bounds.width.max(b.x + b.width);
        bounds.height = bounds.height.max(b.y + b.height);
    }
    let (total_edge_length, reversed_edge_total_length) = compute_edge_length_metrics(&edges);
    let stats = LayoutStats {
        node_count: ir.nodes.len(),
        edge_count: ir.edges.len(),
        total_edge_length,
        reversed_edge_total_length,
        phase_iterations: trace.snapshots.len(),
        ..LayoutStats::default()
    };
    TracedLayout {
        layout: Arc::new(DiagramLayout {
            nodes,
            clusters,
            cycle_clusters: Vec::new(),
            edges,
            bounds,
            stats,
            extensions: LayoutExtensions::default(),
            dirty_regions: Vec::new(),
        }),
        trace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(source: &str) -> (MermaidDiagramIr, DiagramLayout) {
        let ir = fm_parser::parse(source).ir;
        let traced = layout_diagram_event_model_traced(&ir);
        (ir, Arc::unwrap_or_clone(traced.layout))
    }

    /// The canonical cart model: UI, command, event, read model, UI. Lanes come out in upstream's
    /// order, every lane change overlaps the previous box by 80px, and the flow runs frame to frame.
    #[test]
    fn frames_staircase_through_the_type_lanes() {
        let (ir, layout) = layout(
            "eventmodeling\ntf 01 ui CartScreen\ntf 02 cmd AddItem\ntf 03 evt ItemAdded\ntf 04 rmo CartItems\ntf 05 ui CartScreen2\n",
        );
        let titles: Vec<&str> = layout
            .clusters
            .iter()
            .map(|c| c.title.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(titles, ["UI/Automation", "Command/Read Model", "Events"]);
        let b = |i: usize| layout.nodes[i].bounds;
        assert_eq!(b(0).x, CONTENT_START_X);
        // Each frame entering a lane for the first time steps 80px back under the previous box.
        for i in 1..3 {
            let previous_right = b(i - 1).x + b(i - 1).width + BOX_PADDING;
            assert!(
                (b(i).x - (previous_right - BOX_OVERLAP + BOX_PADDING)).abs() < 0.01,
                "frame {i} is not stepped off frame {}",
                i - 1
            );
        }
        // Lanes stack with their gap, and each box sits inside its lane.
        assert!(
            layout.clusters[1].bounds.y
                > layout.clusters[0].bounds.y + layout.clusters[0].bounds.height
        );
        assert!(b(2).y > b(1).y && b(1).y > b(0).y);
        // UI -> command -> event -> read model -> UI: four arrows; the last climbs back up.
        assert_eq!(ir.edges.len(), 4);
        let last = &layout.edges[3].points;
        assert!(last[0].y > last[1].y, "the read model -> UI arrow climbs");
        assert!(
            (last[0].y - b(3).y).abs() < 0.01,
            "an upward arrow leaves the top edge"
        );
        assert!((last[1].y - (b(4).y + b(4).height)).abs() < 0.01);
    }

    /// Returning to a lane after a narrow detour never lands on that lane's own last box, which
    /// upstream's step-off-the-previous-box rule alone would do.
    #[test]
    fn returning_to_a_lane_never_overlaps_its_last_box() {
        let (_, layout) = layout(
            "eventmodeling\ntf 01 cmd PlaceAnOrderWithAVeryLongName\ntf 02 evt E\ntf 03 rmo Orders\n",
        );
        let (first, back) = (layout.nodes[0].bounds, layout.nodes[2].bounds);
        assert_eq!(first.y, back.y, "both sit in the command/read-model lane");
        assert!(
            back.x >= first.x + first.width + BOX_PADDING - 0.01,
            "{first:?} {back:?}"
        );
    }

    /// Consecutive frames in ONE lane sit side by side, 10px apart.
    #[test]
    fn frames_in_one_lane_sit_side_by_side() {
        let (_, layout) = layout("eventmodeling\ntf 01 evt A\ntf 02 evt B\n");
        let (a, b) = (layout.nodes[0].bounds, layout.nodes[1].bounds);
        assert!((b.x - (a.x + a.width + BOX_PADDING)).abs() < 0.01);
        assert_eq!(a.y, b.y);
        assert_eq!(layout.clusters.len(), 1);
    }
}
