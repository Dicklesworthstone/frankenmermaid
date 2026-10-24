//! Collision-aware graph label placement shared by vector and raster renderers.
//!
//! Renderers supply the dimensions of the text they actually paint. Placement is resolved in
//! paint order so an opaque label plate cannot hide an earlier label, node, or foreign route.

use crate::{
    DiagramLayout, ErMarkerGlyph, LayoutEdgePath, LayoutPoint, LayoutRect, MarkerKind,
    edge_label_position,
};
use fm_core::{ArrowType, MermaidDiagramIr};

/// Stateful placement for one rendered graph, with shared route and previous-label obstacles.
pub struct EdgeLabelPlacer<'a> {
    ir: &'a MermaidDiagramIr,
    layout: &'a DiagramLayout,
    padding: f32,
    routes: Vec<(usize, EdgeLabelObstacle)>,
    plates: Vec<LayoutRect>,
    containers: Vec<Vec<usize>>,
}

impl<'a> EdgeLabelPlacer<'a> {
    /// Prepare obstacles once. Call [`Self::place`] in the renderer's label paint order.
    #[must_use]
    pub fn new(ir: &'a MermaidDiagramIr, layout: &'a DiagramLayout, padding: f32) -> Self {
        Self {
            ir,
            layout,
            padding,
            routes: graph_edge_label_route_obstacles(ir, layout),
            plates: Vec::new(),
            containers: crate::composite_containers(ir, ir.nodes.len()),
        }
    }

    /// Find a clear center for a measured backing plate and reserve it for subsequent labels.
    /// The returned plate can extend outside the original layout; include it in render bounds.
    pub fn place(&mut self, path: &LayoutEdgePath, width: f32, height: f32) -> LayoutPoint {
        let foreign_routes: Vec<_> = self
            .routes
            .iter()
            .filter(|(edge_index, obstacle)| {
                *edge_index != path.edge_index
                    && !matches!(obstacle, EdgeLabelObstacle::Segment(from, to)
                        if path.points.windows(2).any(|pair|
                            edge_label_routes_share_line(pair[0], pair[1], *from, *to)))
            })
            .map(|(_, obstacle)| *obstacle)
            .collect();
        // A composite state is also a routing node, but its interior holds its own transitions.
        // Keep unrelated containers as obstacles while allowing a label inside its parent state.
        let mut ignored_nodes = Vec::new();
        if !self.containers.is_empty()
            && let Some(edge) = self.ir.edges.get(path.edge_index)
        {
            for endpoint in [edge.from, edge.to] {
                if let Some(node) = crate::endpoint_node_index(self.ir, endpoint)
                    && let Some(parents) = self.containers.get(node)
                {
                    ignored_nodes.extend_from_slice(parents);
                }
            }
            ignored_nodes.sort_unstable();
            ignored_nodes.dedup();
        }
        let center = clear_edge_label_position_ignoring_nodes(
            edge_label_position(path),
            width,
            height,
            self.layout,
            self.padding,
            &self.plates,
            &foreign_routes,
            &ignored_nodes,
        );
        self.plates.push(LayoutRect {
            x: center.x - width / 2.0,
            y: center.y - height / 2.0,
            width,
            height,
        });
        center
    }
}

/// Geometry that must remain visible around a graph edge label.
#[derive(Clone, Copy, Debug)]
pub enum EdgeLabelObstacle {
    /// A node, note, marker envelope, or already placed label.
    Bounds(LayoutRect),
    /// A visible connector segment, including its stroke clearance.
    Segment(LayoutPoint, LayoutPoint),
}

type EdgeLabelInterval = Option<(f32, f32)>;

/// Whether straight route segments overlap along the same line.
/// Branches sharing part of a trunk should not displace a label from its own centerline.
#[must_use]
pub fn edge_label_routes_share_line(
    a: LayoutPoint,
    b: LayoutPoint,
    from: LayoutPoint,
    to: LayoutPoint,
) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length_squared = dx * dx + dy * dy;
    let cross_from = (from.x - a.x) * dy - (from.y - a.y) * dx;
    let cross_to = (to.x - a.x) * dy - (to.y - a.y) * dx;
    let along_from = (from.x - a.x) * dx + (from.y - a.y) * dy;
    let along_to = (to.x - a.x) * dx + (to.y - a.y) * dy;
    length_squared > f32::EPSILON
        && cross_from == 0.0
        && cross_to == 0.0
        && along_from.min(along_to) < length_squared
        && along_from.max(along_to) > 0.0
}

/// Clip to the cross-axis slab before projecting the forbidden interval. A diagonal route's
/// whole bounding box would displace labels from empty space far away from the actual line.
fn edge_label_segment_interval(
    from: (f32, f32),
    to: (f32, f32),
    cross_position: f32,
    cross_clearance: f32,
    along_clearance: f32,
) -> EdgeLabelInterval {
    let delta = to.1 - from.1;
    let (lo, hi) = if delta.abs() <= f32::EPSILON {
        if from.1 <= cross_position - cross_clearance || from.1 >= cross_position + cross_clearance
        {
            return None;
        }
        (0.0, 1.0)
    } else {
        let a = (cross_position - cross_clearance - from.1) / delta;
        let b = (cross_position + cross_clearance - from.1) / delta;
        let (lo, hi) = (a.min(b).max(0.0), a.max(b).min(1.0));
        if lo > hi {
            return None;
        }
        (lo, hi)
    };
    let a = from.0 + (to.0 - from.0) * lo;
    let b = from.0 + (to.0 - from.0) * hi;
    Some((a.min(b) - along_clearance, a.max(b) + along_clearance))
}

impl EdgeLabelObstacle {
    fn exclusion_intervals(
        self,
        anchor: LayoutPoint,
        clearance_x: f32,
        clearance_y: f32,
    ) -> (EdgeLabelInterval, EdgeLabelInterval) {
        match self {
            Self::Bounds(bounds) => {
                let x = (
                    bounds.x - clearance_x,
                    bounds.x + bounds.width + clearance_x,
                );
                let y = (
                    bounds.y - clearance_y,
                    bounds.y + bounds.height + clearance_y,
                );
                (
                    (y.0 < anchor.y && anchor.y < y.1).then_some(x),
                    (x.0 < anchor.x && anchor.x < x.1).then_some(y),
                )
            }
            Self::Segment(from, to) => {
                // Includes the stroke and the rounded elbow's <=1.5px route departure.
                let x = clearance_x + 3.0;
                let y = clearance_y + 3.0;
                (
                    edge_label_segment_interval((from.x, from.y), (to.x, to.y), anchor.y, y, x),
                    edge_label_segment_interval((from.y, from.x), (to.y, to.x), anchor.x, x, y),
                )
            }
        }
    }
}

fn graph_edge_label_route_obstacles(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
) -> Vec<(usize, EdgeLabelObstacle)> {
    let mut obstacles = Vec::new();
    for path in &layout.edges {
        let Some(edge) = ir.edges.get(path.edge_index) else {
            continue;
        };
        if path.bundled || edge.arrow == ArrowType::Invisible || path.points.len() < 2 {
            continue;
        }
        for pair in path.points.windows(2) {
            if pair[0] != pair[1] {
                obstacles.push((
                    path.edge_index,
                    EdgeLabelObstacle::Segment(pair[0], pair[1]),
                ));
            }
        }
        let Some(notation) = edge.er_notation() else {
            continue;
        };
        let (left, right) = fm_core::parse_er_cardinality_forms(notation);
        let first = path.points[0];
        let last = path.points[path.points.len() - 1];
        for (endpoint, neighbor, kind, start) in [
            (
                first,
                path.points.iter().copied().find(|point| *point != first),
                left.map(|form| MarkerKind::er_pair(form).0),
                true,
            ),
            (
                last,
                path.points
                    .iter()
                    .copied()
                    .rev()
                    .find(|point| *point != last),
                right.map(|form| MarkerKind::er_pair(form).1),
                false,
            ),
        ] {
            let (Some(neighbor), Some(glyph)) = (neighbor, kind.and_then(|kind| kind.er_glyph()))
            else {
                continue;
            };
            let direction = if start {
                (neighbor.x - endpoint.x, neighbor.y - endpoint.y)
            } else {
                (endpoint.x - neighbor.x, endpoint.y - neighbor.y)
            };
            let length = direction.0.hypot(direction.1);
            if !length.is_finite() || length <= f32::EPSILON {
                continue;
            }
            let (dx, dy) = (direction.0 / length, direction.1 / length);
            let mut lo = f32::INFINITY;
            let mut hi = f32::NEG_INFINITY;
            for x in &glyph.bars {
                lo = lo.min(*x);
                hi = hi.max(*x);
            }
            if glyph.foot {
                lo = lo.min(-ErMarkerGlyph::FOOT_HALF_LENGTH);
                hi = hi.max(ErMarkerGlyph::FOOT_HALF_LENGTH);
            }
            if let Some(x) = glyph.bubble {
                lo = lo.min(x - ErMarkerGlyph::BUBBLE_RADIUS);
                hi = hi.max(x + ErMarkerGlyph::BUBBLE_RADIUS);
            }
            let half_height = if glyph.foot || !glyph.bars.is_empty() {
                ErMarkerGlyph::BAR_HALF_HEIGHT
            } else {
                ErMarkerGlyph::BUBBLE_RADIUS
            };
            let mut bounds = LayoutRect {
                x: f32::INFINITY,
                y: f32::INFINITY,
                width: 0.0,
                height: 0.0,
            };
            let (mut right, mut bottom) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
            for (x, y) in [
                (lo, -half_height),
                (lo, half_height),
                (hi, -half_height),
                (hi, half_height),
            ] {
                let (x, y) = (endpoint.x + dx * x - dy * y, endpoint.y + dy * x + dx * y);
                bounds.x = bounds.x.min(x);
                bounds.y = bounds.y.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
            }
            bounds.width = right - bounds.x;
            bounds.height = bottom - bounds.y;
            obstacles.push((path.edge_index, EdgeLabelObstacle::Bounds(bounds)));
        }
    }
    obstacles
}

/// Merge connected projections so a label cannot oscillate between adjacent obstacles.
fn edge_label_exclusion_interval(mut intervals: Vec<(f32, f32)>, position: f32) -> (f32, f32) {
    intervals.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut merged: Option<(f32, f32)> = None;
    for (start, end) in intervals {
        if let Some((left, right)) = merged {
            if start < right {
                merged = Some((left, right.max(end)));
                continue;
            }
            if left < position && position < right {
                return (left, right);
            }
        }
        merged = Some((start, end));
    }
    merged
        .filter(|(left, right)| *left < position && position < *right)
        .unwrap_or((position, position))
}

/// Keep a measured plate clear of nodes, state notes, earlier plates, and foreign routes.
/// Prefer the nearest clear axis-aligned candidate within the frame, or a clear outside
/// candidate when necessary. The renderer must include the resulting plate in its bounds.
#[must_use]
pub fn clear_edge_label_position(
    anchor: LayoutPoint,
    width: f32,
    height: f32,
    layout: &DiagramLayout,
    padding: f32,
    placed_labels: &[LayoutRect],
    foreign_routes: &[EdgeLabelObstacle],
) -> LayoutPoint {
    clear_edge_label_position_ignoring_nodes(
        anchor,
        width,
        height,
        layout,
        padding,
        placed_labels,
        foreign_routes,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
fn clear_edge_label_position_ignoring_nodes(
    anchor: LayoutPoint,
    width: f32,
    height: f32,
    layout: &DiagramLayout,
    padding: f32,
    placed_labels: &[LayoutRect],
    foreign_routes: &[EdgeLabelObstacle],
    ignored_nodes: &[usize],
) -> LayoutPoint {
    let half_width = width / 2.0;
    let half_height = height / 2.0;
    let min_x = layout.bounds.x - padding + half_width + 2.0;
    let max_x = layout.bounds.x + layout.bounds.width + padding - half_width - 2.0;
    let min_y = layout.bounds.y - padding + half_height + 2.0;
    let max_y = layout.bounds.y + layout.bounds.height + padding - half_height - 2.0;
    let routed_anchor = anchor;
    let project = |value: f32, low: f32, high: f32| {
        if low <= high {
            value.clamp(low, high)
        } else {
            (low + high) / 2.0
        }
    };
    let anchor = LayoutPoint {
        x: project(anchor.x, min_x, max_x),
        y: project(anchor.y, min_y, max_y),
    };
    let clearance_x = half_width + 2.0;
    let clearance_y = half_height + 2.0;
    let obstacles = || {
        layout
            .nodes
            .iter()
            .filter(|node| !ignored_nodes.contains(&node.node_index))
            .map(|node| EdgeLabelObstacle::Bounds(node.bounds))
            .chain(
                layout
                    .extensions
                    .state_notes
                    .iter()
                    .map(|note| EdgeLabelObstacle::Bounds(note.bounds)),
            )
            .chain(placed_labels.iter().copied().map(EdgeLabelObstacle::Bounds))
            .chain(foreign_routes.iter().copied())
    };
    if !obstacles().any(|obstacle| {
        let (x, y) = obstacle.exclusion_intervals(anchor, clearance_x, clearance_y);
        x.is_some_and(|(lo, hi)| lo < anchor.x && anchor.x < hi)
            && y.is_some_and(|(lo, hi)| lo < anchor.y && anchor.y < hi)
    }) {
        return anchor;
    }
    let mut x_intervals = Vec::new();
    let mut y_intervals = Vec::new();
    for obstacle in obstacles() {
        let (x, y) = obstacle.exclusion_intervals(anchor, clearance_x, clearance_y);
        x_intervals.extend(x);
        y_intervals.extend(y);
    }
    let (left, right) = edge_label_exclusion_interval(x_intervals, anchor.x);
    let (top, bottom) = edge_label_exclusion_interval(y_intervals, anchor.y);
    let mut closest_inside = None;
    let mut inside_distance = f32::INFINITY;
    let mut closest = anchor;
    let mut distance = f32::INFINITY;
    for (x, y) in [
        (anchor.x, bottom),
        (anchor.x, top),
        (right, anchor.y),
        (left, anchor.y),
    ] {
        let candidate_distance = (x - routed_anchor.x).abs() + (y - routed_anchor.y).abs();
        if candidate_distance < distance {
            closest = LayoutPoint { x, y };
            distance = candidate_distance;
        }
        if x >= min_x
            && x <= max_x
            && y >= min_y
            && y <= max_y
            && candidate_distance < inside_distance
        {
            closest_inside = Some(LayoutPoint { x, y });
            inside_distance = candidate_distance;
        }
    }
    closest_inside.unwrap_or(closest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LayoutNodeBox;
    use fm_core::{DiagramType, Span};

    fn frame() -> DiagramLayout {
        DiagramLayout {
            nodes: Vec::new(),
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
            stats: Default::default(),
            extensions: Default::default(),
            dirty_regions: Vec::new(),
        }
    }

    #[test]
    fn connected_obstacle_projections_are_merged_before_placement() {
        assert_eq!(
            edge_label_exclusion_interval(vec![(40.0, 60.0), (10.0, 30.0), (20.0, 50.0)], 25.0),
            (10.0, 60.0)
        );
    }

    #[test]
    fn sequential_labels_reserve_their_measured_plates() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let layout = frame();
        let path = LayoutEdgePath {
            edge_index: 0,
            span: Span::default(),
            points: [
                LayoutPoint { x: 0.0, y: 50.0 },
                LayoutPoint { x: 100.0, y: 50.0 },
            ]
            .into_iter()
            .collect(),
            reversed: false,
            is_self_loop: false,
            parallel_offset: 0.0,
            bundle_count: 1,
            bundled: false,
        };
        let mut placer = EdgeLabelPlacer::new(&ir, &layout, 0.0);
        let first = placer.place(&path, 40.0, 20.0);
        let second = placer.place(&path, 40.0, 20.0);
        assert_eq!(first, LayoutPoint { x: 50.0, y: 50.0 });
        assert!(
            (first.x - second.x).abs() >= 42.0 || (first.y - second.y).abs() >= 22.0,
            "measured backing plates must retain a visible gap"
        );
        let mut repeat = EdgeLabelPlacer::new(&ir, &layout, 0.0);
        assert_eq!(repeat.place(&path, 40.0, 20.0), first);
        assert_eq!(repeat.place(&path, 40.0, 20.0), second);
    }

    #[test]
    fn diagonal_routes_only_exclude_their_actual_corridor() {
        let layout = frame();
        let routes = [EdgeLabelObstacle::Segment(
            LayoutPoint { x: 0.0, y: 0.0 },
            LayoutPoint { x: 100.0, y: 100.0 },
        )];
        let clear = LayoutPoint { x: 10.0, y: 90.0 };
        assert_eq!(
            clear_edge_label_position(clear, 20.0, 20.0, &layout, 24.0, &[], &routes),
            clear
        );
        let crossing = LayoutPoint { x: 50.0, y: 50.0 };
        let moved = clear_edge_label_position(crossing, 20.0, 20.0, &layout, 24.0, &[], &routes);
        assert!((moved.x - moved.y).abs() >= 29.99);
    }

    #[test]
    fn occupied_frame_returns_a_clear_plate_outside_it() {
        let mut layout = frame();
        layout.nodes.push(LayoutNodeBox {
            node_index: 0,
            node_id: String::from("obstacle"),
            rank: 0,
            order: 0,
            span: Span::default(),
            bounds: layout.bounds,
        });
        let center = clear_edge_label_position(
            LayoutPoint { x: 50.0, y: 65.0 },
            20.0,
            20.0,
            &layout,
            0.0,
            &[],
            &[],
        );
        assert_eq!(center, LayoutPoint { x: 50.0, y: 112.0 });
        assert_eq!(center.y - 10.0 - layout.bounds.height, 2.0);
    }

    #[test]
    fn partial_shared_trunks_are_distinct_from_crossings_and_disjoint_lines() {
        let a = LayoutPoint { x: 0.0, y: 50.0 };
        let b = LayoutPoint { x: 100.0, y: 50.0 };
        assert!(edge_label_routes_share_line(
            a,
            b,
            LayoutPoint { x: 50.0, y: 50.0 },
            LayoutPoint { x: 150.0, y: 50.0 }
        ));
        assert!(!edge_label_routes_share_line(
            a,
            b,
            LayoutPoint { x: 50.0, y: 0.0 },
            LayoutPoint { x: 50.0, y: 100.0 }
        ));
        assert!(!edge_label_routes_share_line(
            a,
            b,
            LayoutPoint { x: 100.0, y: 50.0 },
            LayoutPoint { x: 150.0, y: 50.0 }
        ));
    }
}
